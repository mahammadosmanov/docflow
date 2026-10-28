use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    naming::{display_name, flat_target_for, folder_for, target_for},
    parallel::bounded_map_stream,
    rfi_code::{RfiMatch, RfiMethod},
};

mod editor;
mod ocr;
mod output;
mod scan_pdf;
pub use editor::*;
pub use ocr::*;
use output::*;
use scan_pdf::{ScanContext, ScanState, scan_one_pdf};

const MIN_CONFIDENCE: f64 = 0.8;
pub const FAST_COVER_OPERATION_ID: &str = "rfi.process_scans_fast_covers";
const REVIEW_DIR: &str = "_review";
const MAX_SCAN_PDFS: usize = 10_000;
const MAX_SCAN_PAGES: usize = 20_000;
const DISCIPLINE_FOLDERS: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
type ApprovedRotation = (usize, i64, f64);

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    #[default]
    MultiRfi,
    SingleRfi,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessScansParams {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub scan_mode: ScanMode,
    pub organize_into_rfi_folders: bool,
    pub keep_original: bool,
    pub correct_orientation: bool,
    pub trash_root: Option<PathBuf>,
}

struct Candidate {
    path: PathBuf,
    pages: Option<Vec<usize>>,
    source_hash: Option<String>,
    modified: SystemTime,
    confidence: f64,
    source: String,
    rotations: Vec<ApprovedRotation>,
}

struct ReviewPages {
    path: PathBuf,
    pages: Vec<usize>,
    reason: String,
    rotations: Vec<ApprovedRotation>,
}

struct PageGroup {
    code: Option<String>,
    pages: Vec<usize>,
    confidence: f64,
    source: String,
    reason: String,
}

struct CachedPageOcr {
    reads: BTreeMap<(PathBuf, usize), Result<OcrRead, OcrFailure>>,
    inspections: BTreeMap<PathBuf, Result<crate::pdf::PdfInspection, String>>,
    page_scan: bool,
    diagnostics: String,
    orientations: BTreeMap<
        (PathBuf, usize),
        Result<crate::operations::pdf_orientation::OrientationDetection, String>,
    >,
}

impl OcrAdapter for CachedPageOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        self.read_page(path, 0)
    }

    fn supports_page_scan(&self) -> bool {
        self.page_scan
    }

    fn inspect_pdf(&self, path: &Path) -> Option<Result<crate::pdf::PdfInspection, String>> {
        self.inspections.get(path).cloned()
    }

    fn read_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.reads
            .get(&(path.to_path_buf(), page_index))
            .cloned()
            .unwrap_or_else(|| {
                Err(OcrFailure::Document(
                    "parallel page OCR cache entry is missing".to_string(),
                ))
            })
    }

    fn diagnostics(&self) -> String {
        self.diagnostics.clone()
    }

    fn supports_orientation(&self) -> bool {
        !self.orientations.is_empty()
    }

    fn detect_orientation(
        &self,
        path: &Path,
        page_index: usize,
    ) -> Result<crate::operations::pdf_orientation::OrientationDetection, String> {
        self.orientations
            .get(&(path.to_path_buf(), page_index))
            .cloned()
            .unwrap_or_else(|| Err("page orientation result is missing".to_string()))
    }
}

pub fn preview_fast_covers<A, P, C>(
    params: &ProcessScansParams,
    ocr: &A,
    progress: P,
    cancelled: C,
) -> Result<Plan, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    preview_with_cover_mode(
        params,
        ocr,
        FAST_COVER_OPERATION_ID,
        true,
        None,
        progress,
        cancelled,
    )
}

fn preview_with_cover_mode<A, P, C>(
    params: &ProcessScansParams,
    ocr: &A,
    operation_id: &str,
    fast_cover_mode: bool,
    source_sha256: Option<&BTreeMap<PathBuf, String>>,
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let source = canonical_directory(&params.source, "scan source")?;
    let destination = canonical_directory(&params.destination, "scan destination")?;
    progress(ProgressEvent {
        phase: "scan_discovery".to_string(),
        done: 0,
        total: 0,
        subject: source.to_string_lossy().into_owned(),
    });
    let pdfs = discover_pdfs(&source)?;
    progress(ProgressEvent {
        phase: "scan_discovery".to_string(),
        done: pdfs.len() as u64,
        total: pdfs.len() as u64,
        subject: String::new(),
    });

    if ocr.supports_parallel_analysis() && !pdfs.is_empty() {
        return preview_parallel(
            params,
            ocr,
            operation_id,
            fast_cover_mode,
            &pdfs,
            progress,
            cancelled,
        );
    }

    let transfer_kind = if params.keep_original {
        StepKind::Copy
    } else {
        StepKind::Move
    };
    let requires_existing_rfi_folder = !fast_cover_mode
        && params.organize_into_rfi_folders
        && destination_contains_exact_rfi_folder(&destination);
    let plan = empty_plan(operation_id);
    let groups: Vec<(String, Vec<Candidate>)> = Vec::new();
    let failures = Vec::<(PathBuf, String)>::new();
    let review_pages = Vec::<ReviewPages>::new();
    let split_sources = BTreeMap::<PathBuf, String>::new();
    let scanned_pages = 0_usize;
    let ctx = ScanContext {
        params,
        ocr,
        operation_id,
        source_sha256,
        destination: &destination,
        transfer_kind,
        total_pdfs: pdfs.len(),
        fast_cover_mode,
    };
    let mut state = ScanState {
        plan,
        groups,
        failures,
        review_pages,
        split_sources,
        scanned_pages,
    };
    for (index, pdf) in pdfs.iter().enumerate() {
        if let Some(early) =
            scan_one_pdf(&ctx, &mut state, index, pdf, &mut progress, &mut cancelled)?
        {
            return Ok(early);
        }
    }
    let ScanState {
        mut plan,
        groups,
        failures,
        review_pages,
        split_sources,
        ..
    } = state;
    if pdfs.len() > 1 && failures.len() == pdfs.len() {
        return Ok(Plan {
            warnings: vec![Warning {
                key: "warn.ocr_broken".to_string(),
                subject: format!("{}  ||  {}", failures[0].1, ocr.diagnostics()),
            }],
            ..empty_plan(operation_id)
        });
    }

    push_unreadable_outputs(&mut plan, failures, &destination, transfer_kind);

    for review in review_pages {
        let target = review_target(&destination, &review.path, &review.pages);
        let candidate = Candidate {
            path: review.path.clone(),
            pages: Some(review.pages),
            source_hash: split_sources.get(&review.path).cloned(),
            modified: UNIX_EPOCH,
            confidence: 0.0,
            source: String::new(),
            rotations: review.rotations,
        };
        let mut meta = JsonObject::new();
        meta.insert("reason".to_string(), Value::String(review.reason));
        push_candidate_output(
            &mut plan,
            &candidate,
            &target,
            "note.needs_review",
            meta,
            params.keep_original,
        );
    }

    for (code, files) in groups {
        push_group_outputs(
            &mut plan,
            &destination,
            params,
            requires_existing_rfi_folder,
            code,
            files,
        );
    }

    if !params.keep_original && !split_sources.is_empty() {
        let trash_root = params.trash_root.as_deref().ok_or_else(|| {
            BackendError::path_invalid("recoverable trash is unavailable for batch scans")
        })?;
        let trash_root = canonical_directory(trash_root, "recoverable trash")?;
        for (pdf, source_hash) in split_sources {
            let trash_name = format!(
                "process-scans-{}_{}",
                uuid::Uuid::new_v4(),
                display_name(&pdf)
            );
            let mut meta = JsonObject::new();
            meta.insert(
                "expectedSourceSha256".to_string(),
                Value::String(source_hash),
            );
            plan.steps.push(step(
                StepKind::Move,
                Some(&pdf),
                Some(&trash_root.join(trash_name)),
                "note.deleted",
                meta,
            ));
        }
    }

    if pdfs.is_empty() {
        plan.warnings.push(Warning {
            key: "warn.no_pdfs".to_string(),
            subject: source.to_string_lossy().into_owned(),
        });
    }
    attach_scan_editor(&mut plan, &destination, params.organize_into_rfi_folders);
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(plan)
}

fn preview_parallel<A, P, C>(
    params: &ProcessScansParams,
    ocr: &A,
    operation_id: &str,
    fast_cover_mode: bool,
    pdfs: &[PathBuf],
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    if cancelled() {
        return Err(BackendError::cancelled());
    }
    let mut orientation_source_hashes = BTreeMap::new();
    if params.scan_mode == ScanMode::MultiRfi
        && params.correct_orientation
        && ocr.supports_orientation()
    {
        for pdf in pdfs {
            orientation_source_hashes.insert(
                pdf.clone(),
                hash_pdf(pdf).map_err(BackendError::path_invalid)?,
            );
        }
    }
    let (inspection_workers, inspections) = if ocr.supports_page_scan() {
        inspect_pdfs(pdfs, &mut progress)?
    } else {
        (0, BTreeMap::new())
    };
    let mut tasks = Vec::new();
    let mut fast_cover_pdfs = BTreeSet::new();
    let mut total_pages = 0_usize;
    for pdf in pdfs {
        let page_count = inspections
            .get(pdf)
            .and_then(|inspection| inspection.as_ref().ok())
            .map_or(1, |facts| facts.page_count);
        total_pages = total_pages.saturating_add(page_count);
        if total_pages > MAX_SCAN_PAGES {
            return Err(BackendError::validation(format!(
                "scan source exceeds the {MAX_SCAN_PAGES}-page limit"
            )));
        }
        if inspections.get(pdf).is_none_or(Result::is_ok) && page_count > 0 {
            if fast_cover_mode && params.scan_mode == ScanMode::MultiRfi && page_count > 1 {
                fast_cover_pdfs.insert(pdf.clone());
            }
            let ocr_page_count = if params.scan_mode == ScanMode::SingleRfi {
                1
            } else {
                page_count
            };
            tasks.extend((0..ocr_page_count).map(|page| (pdf.clone(), page)));
        }
    }
    let (orientation_workers, orientations) = if params.scan_mode == ScanMode::MultiRfi
        && params.correct_orientation
        && ocr.supports_orientation()
    {
        detect_orientations(ocr, &tasks, &mut progress)?
    } else {
        (0, BTreeMap::new())
    };
    let mut completed = 0_u64;
    let (ocr_workers, results) = if tasks.is_empty() {
        (0, Vec::new())
    } else {
        bounded_map_stream(
            &tasks,
            |(pdf, page)| {
                if fast_cover_pdfs.contains(pdf) {
                    ocr.read_cover_page(pdf, *page)
                } else {
                    ocr.read_page(pdf, *page)
                }
            },
            |index, _| {
                completed += 1;
                let (pdf, page) = &tasks[index];
                progress(ProgressEvent {
                    phase: "scan_ocr".to_string(),
                    done: completed,
                    total: tasks.len() as u64,
                    subject: format!("{} — #{}", display_name(pdf), page + 1),
                });
                Ok(())
            },
        )
        .map_err(BackendError::request_failed)?
    };
    if cancelled() {
        return Err(BackendError::cancelled());
    }
    for (pdf, expected) in &orientation_source_hashes {
        if hash_pdf(pdf).map_err(BackendError::path_invalid)? != *expected {
            return Err(BackendError::path_invalid(format!(
                "scan source changed during orientation or OCR analysis: {}",
                display_name(pdf)
            )));
        }
    }
    let cached = CachedPageOcr {
        reads: tasks.into_iter().zip(results).collect(),
        inspections,
        page_scan: ocr.supports_page_scan(),
        diagnostics: ocr.diagnostics(),
        orientations,
    };
    let mut plan = preview_with_cover_mode(
        params,
        &cached,
        operation_id,
        fast_cover_mode,
        Some(&orientation_source_hashes),
        progress,
        cancelled,
    )?;
    plan.meta.insert(
        "workerCount".to_string(),
        Value::from(ocr_workers.max(inspection_workers) as u64),
    );
    plan.meta
        .insert("parallelAnalysis".to_string(), Value::Bool(true));
    plan.meta.insert(
        "orientationWorkerCount".to_string(),
        Value::from(orientation_workers as u64),
    );
    Ok(plan)
}

fn push_unreadable_outputs(
    plan: &mut Plan,
    failures: Vec<(PathBuf, String)>,
    destination: &Path,
    transfer_kind: StepKind,
) {
    for (pdf, error) in failures {
        let truncated = error.chars().take(200).collect::<String>();
        plan.warnings.push(Warning {
            key: "warn.unreadable".to_string(),
            subject: format!("{} — {truncated}", display_name(&pdf)),
        });
        let mut meta = JsonObject::new();
        meta.insert("error".to_string(), Value::String(error));
        plan.steps.push(step(
            transfer_kind,
            Some(&pdf),
            Some(&destination.join(REVIEW_DIR).join(display_name(&pdf))),
            "note.unreadable",
            meta,
        ));
    }
}

fn discover_pdfs(source: &Path) -> Result<Vec<PathBuf>, BackendError> {
    let mut pdfs = fs::read_dir(source)
        .map_err(|_| BackendError::path_invalid("scan source could not be read"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let file_type = entry.file_type().ok()?;
            (file_type.is_file()
                && !name.starts_with('.')
                && entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf")))
            .then_some(entry.path())
        })
        .take(MAX_SCAN_PDFS + 1)
        .collect::<Vec<_>>();
    if pdfs.len() > MAX_SCAN_PDFS {
        return Err(BackendError::validation(format!(
            "scan source exceeds the {MAX_SCAN_PDFS}-PDF limit"
        )));
    }
    pdfs.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(pdfs)
}

fn push_group_outputs(
    plan: &mut Plan,
    destination: &Path,
    params: &ProcessScansParams,
    requires_existing_rfi_folder: bool,
    code: String,
    mut files: Vec<Candidate>,
) {
    files.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
    let winner = files.remove(0);
    let (folder, target) =
        process_scan_targets(destination, &code, params.organize_into_rfi_folders);
    if same_file(&winner.path, &target) {
        return;
    }
    if target.exists() {
        plan.warnings.push(Warning {
            key: "warn.exists".to_string(),
            subject: display_name(&winner.path),
        });
        let mut meta = JsonObject::new();
        meta.insert("code".to_string(), Value::String(code.clone()));
        meta.insert("confidence".to_string(), Value::from(winner.confidence));
        meta.insert("source".to_string(), Value::String(winner.source.clone()));
        meta.insert(
            "reason".to_string(),
            Value::String("target_exists".to_string()),
        );
        meta.insert(
            "target".to_string(),
            Value::String(target.to_string_lossy().into_owned()),
        );
        let review_target = winner.pages.as_ref().map_or_else(
            || {
                destination
                    .join(REVIEW_DIR)
                    .join(display_name(&winner.path))
            },
            |pages| review_target(destination, &winner.path, pages),
        );
        push_candidate_output(
            plan,
            &winner,
            &review_target,
            "note.needs_review",
            meta,
            params.keep_original,
        );
    } else if requires_existing_rfi_folder && folder.as_ref().is_some_and(|folder| !folder.is_dir())
    {
        plan.warnings.push(Warning {
            key: "warn.unknown_rfi_folder".to_string(),
            subject: format!("{} — {code}", display_name(&winner.path)),
        });
        let mut meta = JsonObject::new();
        meta.insert("code".to_string(), Value::String(code.clone()));
        meta.insert("confidence".to_string(), Value::from(winner.confidence));
        meta.insert("source".to_string(), Value::String(winner.source.clone()));
        meta.insert(
            "reason".to_string(),
            Value::String("unknown_rfi_folder".to_string()),
        );
        let review_target = winner.pages.as_ref().map_or_else(
            || {
                destination
                    .join(REVIEW_DIR)
                    .join(display_name(&winner.path))
            },
            |pages| review_target(destination, &winner.path, pages),
        );
        push_candidate_output(
            plan,
            &winner,
            &review_target,
            "note.needs_review",
            meta,
            params.keep_original,
        );
    } else {
        let folder_created = folder.as_ref().is_some_and(|folder| !folder.is_dir());
        if let Some(folder) = folder.as_ref().filter(|folder| !folder.is_dir()) {
            let mut meta = JsonObject::new();
            meta.insert("code".to_string(), Value::String(code.clone()));
            plan.steps.push(step(
                StepKind::Mkdir,
                None,
                Some(folder),
                "note.folder_created",
                meta,
            ));
        }
        let mut meta = JsonObject::new();
        meta.insert("code".to_string(), Value::String(code.clone()));
        meta.insert("confidence".to_string(), Value::from(winner.confidence));
        meta.insert("source".to_string(), Value::String(winner.source.clone()));
        meta.insert(
            "intoFolder".to_string(),
            Value::Bool(params.organize_into_rfi_folders),
        );
        meta.insert("folderCreated".to_string(), Value::Bool(folder_created));
        push_candidate_output(
            plan,
            &winner,
            &target,
            "note.renamed",
            meta,
            params.keep_original,
        );
    }

    for older in files {
        plan.warnings.push(Warning {
            key: "warn.duplicate".to_string(),
            subject: display_name(&older.path),
        });
        let mut meta = JsonObject::new();
        meta.insert("code".to_string(), Value::String(code.clone()));
        meta.insert("confidence".to_string(), Value::from(older.confidence));
        meta.insert("source".to_string(), Value::String(older.source.clone()));
        meta.insert(
            "reason".to_string(),
            Value::String("older_revision".to_string()),
        );
        meta.insert(
            "kept".to_string(),
            Value::String(display_name(&winner.path)),
        );
        let review_target = older.pages.as_ref().map_or_else(
            || destination.join(REVIEW_DIR).join(display_name(&older.path)),
            |pages| review_target(destination, &older.path, pages),
        );
        push_candidate_output(
            plan,
            &older,
            &review_target,
            "note.needs_review",
            meta,
            params.keep_original,
        );
    }
}

type PdfInspections = BTreeMap<PathBuf, Result<crate::pdf::PdfInspection, String>>;

fn inspect_pdfs<P>(
    pdfs: &[PathBuf],
    progress: &mut P,
) -> Result<(usize, PdfInspections), BackendError>
where
    P: FnMut(ProgressEvent),
{
    let mut completed = 0_u64;
    let (workers, results) = bounded_map_stream(
        pdfs,
        |pdf| crate::pdf::inspect(pdf),
        |index, _| {
            completed += 1;
            progress(ProgressEvent {
                phase: "scan_discovery".to_string(),
                done: completed,
                total: pdfs.len() as u64,
                subject: display_name(&pdfs[index]),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    let inspections = pdfs
        .iter()
        .cloned()
        .zip(
            results
                .into_iter()
                .map(|result| result.map_err(|error| error.to_string())),
        )
        .collect();
    Ok((workers, inspections))
}

type OrientationDetections = BTreeMap<
    (PathBuf, usize),
    Result<crate::operations::pdf_orientation::OrientationDetection, String>,
>;

fn detect_orientations<A, P>(
    ocr: &A,
    tasks: &[(PathBuf, usize)],
    progress: &mut P,
) -> Result<(usize, OrientationDetections), BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
{
    let mut completed = 0_u64;
    let (workers, results) = bounded_map_stream(
        tasks,
        |(pdf, page)| ocr.detect_orientation(pdf, *page),
        |index, _| {
            completed += 1;
            let (pdf, page) = &tasks[index];
            progress(ProgressEvent {
                phase: "scan_orientation".to_string(),
                done: completed,
                total: tasks.len() as u64,
                subject: format!("{} — #{}", display_name(pdf), page + 1),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    Ok((workers, tasks.iter().cloned().zip(results).collect()))
}
