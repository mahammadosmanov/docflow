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
pub use editor::*;
pub use ocr::*;

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
    let mut pdfs = fs::read_dir(&source)
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

    for (code, mut files) in groups {
        files.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
        let winner = files.remove(0);
        let (folder, target) =
            process_scan_targets(&destination, &code, params.organize_into_rfi_folders);
        if same_file(&winner.path, &target) {
            continue;
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
                |pages| review_target(&destination, &winner.path, pages),
            );
            push_candidate_output(
                &mut plan,
                &winner,
                &review_target,
                "note.needs_review",
                meta,
                params.keep_original,
            );
        } else if requires_existing_rfi_folder
            && folder.as_ref().is_some_and(|folder| !folder.is_dir())
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
                |pages| review_target(&destination, &winner.path, pages),
            );
            push_candidate_output(
                &mut plan,
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
                &mut plan,
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
                |pages| review_target(&destination, &older.path, pages),
            );
            push_candidate_output(
                &mut plan,
                &older,
                &review_target,
                "note.needs_review",
                meta,
                params.keep_original,
            );
        }
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

struct ScanContext<'a, A> {
    params: &'a ProcessScansParams,
    ocr: &'a A,
    operation_id: &'a str,
    source_sha256: Option<&'a BTreeMap<PathBuf, String>>,
    destination: &'a Path,
    transfer_kind: StepKind,
    total_pdfs: usize,
    fast_cover_mode: bool,
}

struct ScanState {
    plan: Plan,
    groups: Vec<(String, Vec<Candidate>)>,
    failures: Vec<(PathBuf, String)>,
    review_pages: Vec<ReviewPages>,
    split_sources: BTreeMap<PathBuf, String>,
    scanned_pages: usize,
}

fn scan_one_pdf<A, P, C>(
    ctx: &ScanContext<'_, A>,
    state: &mut ScanState,
    index: usize,
    pdf: &PathBuf,
    progress: &mut P,
    cancelled: &mut C,
) -> Result<Option<Plan>, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let ScanContext {
        params,
        ocr,
        operation_id,
        source_sha256,
        destination,
        transfer_kind,
        total_pdfs,
        fast_cover_mode,
    } = *ctx;
    let ScanState {
        plan,
        groups,
        failures,
        review_pages,
        split_sources,
        scanned_pages,
    } = state;
    if cancelled() {
        return Err(BackendError::cancelled());
    }
    progress(ProgressEvent {
        phase: "scan_planning".to_string(),
        done: index as u64,
        total: total_pdfs as u64,
        subject: String::new(),
    });
    let inspection = if ocr.supports_page_scan() {
        Some(
            ocr.inspect_pdf(pdf)
                .unwrap_or_else(|| crate::pdf::inspect(pdf).map_err(|error| error.to_string())),
        )
    } else {
        None
    };
    let facts = match inspection {
        Some(Ok(facts)) => Some(facts),
        Some(Err(error)) => {
            failures.push((pdf.clone(), error));
            return Ok(None);
        }
        None => None,
    };
    let page_count = facts.as_ref().map_or(1, |facts| facts.page_count);
    *scanned_pages = scanned_pages.saturating_add(page_count);
    if *scanned_pages > MAX_SCAN_PAGES {
        return Err(BackendError::validation(format!(
            "scan source exceeds the {MAX_SCAN_PAGES}-page limit"
        )));
    }
    if page_count == 0 {
        failures.push((pdf.clone(), "PDF has no pages".to_string()));
        return Ok(None);
    }
    let mut rotations = Vec::new();
    let should_correct_orientation =
        params.scan_mode == ScanMode::MultiRfi && params.correct_orientation;
    let orientation_source_hash = if should_correct_orientation {
        Some(match source_sha256.and_then(|hashes| hashes.get(pdf)) {
            Some(expected) => expected.clone(),
            None => hash_pdf(pdf).map_err(BackendError::path_invalid)?,
        })
    } else {
        None
    };
    if should_correct_orientation {
        let orientation_results = (0..page_count)
            .map(|page_index| {
                let detected = ocr.detect_orientation(pdf, page_index)?;
                let current = facts
                    .as_ref()
                    .and_then(|facts| facts.pages.get(page_index))
                    .map_or(0, |page| page.rotation);
                let target = crate::operations::pdf_orientation::orientation_target_rotation(
                    current, &detected,
                )?;
                Ok((page_index, target, detected.confidence))
            })
            .collect::<Result<Vec<_>, String>>()
            .and_then(|results| {
                let rotations = results
                    .into_iter()
                    .filter_map(|(page, target, confidence)| {
                        target.map(|target| (page, target, confidence))
                    })
                    .collect::<Vec<_>>();
                if !rotations.is_empty()
                    && facts.as_ref().is_some_and(|facts| {
                        facts.signed || facts.encrypted || !crate::pdf::extraction_supported(facts)
                    })
                {
                    Err("PDF features prevent safe orientation correction".to_string())
                } else {
                    Ok(rotations)
                }
            });
        match orientation_results {
            Ok(detected_rotations) => {
                rotations = detected_rotations;
                if let Some(expected) = orientation_source_hash.as_deref()
                    && hash_pdf(pdf).map_err(BackendError::path_invalid)? != expected
                {
                    return Err(BackendError::path_invalid(
                        "scan source changed during orientation analysis",
                    ));
                }
            }
            Err(reason) => {
                plan.warnings.push(Warning {
                    key: "warn.rfi.processScansOrientationReview".to_string(),
                    subject: format!("{} — {reason}", display_name(pdf)),
                });
                let mut meta = JsonObject::new();
                meta.insert("reason".to_string(), Value::String(reason));
                plan.steps.push(step(
                    transfer_kind,
                    Some(pdf),
                    Some(&destination.join(REVIEW_DIR).join(display_name(pdf))),
                    "note.needs_review",
                    meta,
                ));
                return Ok(None);
            }
        }
    }
    if page_count > 1 && params.scan_mode == ScanMode::MultiRfi {
        if !facts.as_ref().is_some_and(crate::pdf::extraction_supported) {
            failures.push((
                pdf.clone(),
                "PDF features prevent safe page separation".to_string(),
            ));
            return Ok(None);
        }
        let source_hash = hash_pdf(pdf).map_err(BackendError::path_invalid)?;
        if orientation_source_hash
            .as_deref()
            .is_some_and(|expected| expected != source_hash.as_str())
        {
            return Err(BackendError::path_invalid(
                "scan source changed during Preview",
            ));
        }
        let mut local_groups = Vec::<PageGroup>::new();
        let mut active = None::<PageGroup>;
        let mut seen_codes = std::collections::BTreeSet::new();
        for page_index in 0..page_count {
            if cancelled() {
                return Err(BackendError::cancelled());
            }
            let page_read = if fast_cover_mode {
                ocr.read_cover_page(pdf, page_index)
            } else {
                ocr.read_page(pdf, page_index)
            };
            let read = match page_read {
                Ok(read) => read,
                Err(OcrFailure::EngineMissing) => {
                    return Ok(Some(Plan {
                        warnings: vec![Warning {
                            key: "warn.no_ocr".to_string(),
                            subject: ocr.diagnostics(),
                        }],
                        ..empty_plan(operation_id)
                    }));
                }
                Err(OcrFailure::Document(error)) => {
                    if let Some(group) = active.as_mut() {
                        group.pages.push(page_index);
                    } else {
                        active = Some(PageGroup {
                            code: None,
                            pages: vec![page_index],
                            confidence: 0.0,
                            source: String::new(),
                            reason: format!("unreadable page: {error}"),
                        });
                    }
                    continue;
                }
            };
            // A strict, high-confidence RFI identity is the only group boundary.
            // Once a cover starts a group, every page stays with it until another
            // confirmed cover appears, even when continuation OCR is uncertain.
            if fast_cover_mode && read.source.starts_with("ocr:fast-cover:uncertain") {
                flush_page_group(&mut active, &mut local_groups);
                active = Some(PageGroup {
                    code: None,
                    pages: vec![page_index],
                    confidence: read.matched.confidence,
                    source: read.source,
                    reason: "possible RFI cover could not be confirmed".to_string(),
                });
            } else if is_confirmed_cover(&read) {
                let code = read.matched.code.clone();
                let continues = active
                    .as_ref()
                    .is_some_and(|group| group.code.as_ref() == Some(&code));
                if continues {
                    active
                        .as_mut()
                        .expect("current group exists")
                        .pages
                        .push(page_index);
                    continue;
                }
                flush_page_group(&mut active, &mut local_groups);
                if !seen_codes.insert(code.clone()) {
                    active = Some(PageGroup {
                        code: None,
                        pages: vec![page_index],
                        confidence: read.matched.confidence,
                        source: read.source,
                        reason: format!("duplicate RFI page for {code}"),
                    });
                } else {
                    active = Some(PageGroup {
                        code: Some(code),
                        pages: vec![page_index],
                        confidence: read.matched.confidence,
                        source: read.source,
                        reason: String::new(),
                    });
                }
            } else if active.as_ref().is_some_and(|group| group.code.is_some()) {
                active
                    .as_mut()
                    .expect("current RFI group exists")
                    .pages
                    .push(page_index);
            } else if read.matched.method == RfiMethod::Miss
                && !read.source.starts_with("ocr:ambiguous-or-unconfirmed:")
            {
                if let Some(group) = active.as_mut() {
                    group.pages.push(page_index);
                } else {
                    active = Some(PageGroup {
                        code: None,
                        pages: vec![page_index],
                        confidence: 0.0,
                        source: read.source,
                        reason: "page precedes the first exact RFI identity".to_string(),
                    });
                }
            } else {
                flush_page_group(&mut active, &mut local_groups);
                active = Some(PageGroup {
                    code: None,
                    pages: vec![page_index],
                    confidence: read.matched.confidence,
                    source: read.source,
                    reason: format!(
                        "uncertain RFI evidence: {}",
                        method_name(read.matched.method)
                    ),
                });
            }
        }
        flush_page_group(&mut active, &mut local_groups);
        if hash_pdf(pdf).map_err(BackendError::path_invalid)? != source_hash {
            return Err(BackendError::path_invalid(
                "scan source changed during Preview",
            ));
        }
        split_sources.insert(pdf.clone(), source_hash.clone());
        for group in local_groups {
            if let Some(code) = group.code {
                let pages = group.pages;
                let group_rotations = rotations
                    .iter()
                    .filter(|(page, _, _)| pages.contains(page))
                    .copied()
                    .collect::<Vec<_>>();
                if let Some((_, candidates)) = groups.iter_mut().find(|(value, _)| value == &code) {
                    candidates.push(Candidate {
                        path: pdf.clone(),
                        pages: Some(pages.clone()),
                        source_hash: Some(source_hash.clone()),
                        modified: fs::metadata(pdf)
                            .and_then(|metadata| metadata.modified())
                            .unwrap_or(UNIX_EPOCH),
                        confidence: group.confidence,
                        source: group.source,
                        rotations: group_rotations.clone(),
                    });
                } else {
                    groups.push((
                        code,
                        vec![Candidate {
                            path: pdf.clone(),
                            pages: Some(pages),
                            source_hash: Some(source_hash.clone()),
                            modified: fs::metadata(pdf)
                                .and_then(|metadata| metadata.modified())
                                .unwrap_or(UNIX_EPOCH),
                            confidence: group.confidence,
                            source: group.source,
                            rotations: group_rotations,
                        }],
                    ));
                }
            } else {
                let group_rotations = rotations
                    .iter()
                    .filter(|(page, _, _)| group.pages.contains(page))
                    .copied()
                    .collect();
                plan.warnings.push(Warning {
                    key: "warn.no_code".to_string(),
                    subject: format!("{} — {}", display_name(pdf), page_range(&group.pages)),
                });
                review_pages.push(ReviewPages {
                    path: pdf.clone(),
                    pages: group.pages,
                    reason: group.reason,
                    rotations: group_rotations,
                });
            }
        }
        return Ok(None);
    }

    let read = match ocr.read_page(pdf, 0) {
        Ok(read) => read,
        Err(OcrFailure::EngineMissing) => {
            return Ok(Some(Plan {
                warnings: vec![Warning {
                    key: "warn.no_ocr".to_string(),
                    subject: ocr.diagnostics(),
                }],
                ..empty_plan(operation_id)
            }));
        }
        Err(OcrFailure::Document(error)) => {
            failures.push((pdf.clone(), error));
            return Ok(None);
        }
    };
    if !matches!(read.matched.method, RfiMethod::Strict | RfiMethod::OcrAlias)
        || read.matched.confidence < MIN_CONFIDENCE
    {
        plan.warnings.push(Warning {
            key: if read.matched.ok() {
                "warn.low_confidence".to_string()
            } else {
                "warn.no_code".to_string()
            },
            subject: display_name(pdf),
        });
        let mut meta = JsonObject::new();
        meta.insert(
            "confidence".to_string(),
            Value::from(read.matched.confidence),
        );
        meta.insert(
            "method".to_string(),
            Value::String(method_name(read.matched.method).to_string()),
        );
        meta.insert("source".to_string(), Value::String(read.source));
        plan.steps.push(step(
            transfer_kind,
            Some(pdf),
            Some(&destination.join(REVIEW_DIR).join(display_name(pdf))),
            "note.needs_review",
            meta,
        ));
        return Ok(None);
    }

    let source_hash = (!rotations.is_empty())
        .then(|| orientation_source_hash.clone())
        .flatten();
    if let Some(expected) = source_hash.as_deref()
        && hash_pdf(pdf).map_err(BackendError::path_invalid)? != expected
    {
        return Err(BackendError::path_invalid(
            "scan source changed during Preview",
        ));
    }
    if let Some(source_hash) = source_hash.as_ref() {
        split_sources.insert(pdf.clone(), source_hash.clone());
    }
    let candidate = Candidate {
        path: pdf.clone(),
        pages: source_hash.as_ref().map(|_| vec![0]),
        source_hash,
        modified: fs::metadata(pdf)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH),
        confidence: read.matched.confidence,
        source: read.source,
        rotations,
    };
    if let Some((_, files)) = groups
        .iter_mut()
        .find(|(code, _)| code == &read.matched.code)
    {
        files.push(candidate);
    } else {
        groups.push((read.matched.code, vec![candidate]));
    }
    Ok(None)
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
        (
            workers,
            pdfs.iter()
                .cloned()
                .zip(
                    results
                        .into_iter()
                        .map(|result| result.map_err(|error| error.to_string())),
                )
                .collect::<BTreeMap<_, _>>(),
        )
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
        let mut completed = 0_u64;
        let (workers, results) = bounded_map_stream(
            &tasks,
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
        (workers, tasks.iter().cloned().zip(results).collect())
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

fn destination_contains_exact_rfi_folder(destination: &Path) -> bool {
    exact_rfi_folder_in(destination)
        || DISCIPLINE_FOLDERS
            .iter()
            .any(|discipline| exact_rfi_folder_in(&destination.join(discipline)))
}

fn exact_rfi_folder_in(directory: &Path) -> bool {
    fs::read_dir(directory).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            let Ok(file_type) = entry.file_type() else {
                return false;
            };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                return false;
            };
            let matched = crate::rfi_code::find(&name);
            file_type.is_dir() && matched.method == RfiMethod::Strict && matched.code == name
        })
    })
}

fn flush_page_group(active: &mut Option<PageGroup>, groups: &mut Vec<PageGroup>) {
    if let Some(group) = active.take() {
        groups.push(group);
    }
}

fn is_confirmed_cover(read: &OcrRead) -> bool {
    let canonical = crate::rfi_code::find(&read.matched.code);
    matches!(read.matched.method, RfiMethod::Strict | RfiMethod::OcrAlias)
        && read.matched.confidence >= MIN_CONFIDENCE
        && canonical.code == read.matched.code
        && canonical.method == RfiMethod::Strict
}

fn process_scan_targets(
    destination: &Path,
    code: &str,
    organize: bool,
) -> (Option<PathBuf>, PathBuf) {
    let discipline = crate::rfi_code::find(code).discipline;
    let discipline_root = DISCIPLINE_FOLDERS
        .contains(&discipline.as_str())
        .then(|| destination.join(&discipline));
    let root = discipline_root.as_deref().unwrap_or(destination);
    if organize {
        let folder = folder_for(root, code);
        (Some(folder.clone()), target_for(root, code))
    } else {
        let target = flat_target_for(root, code);
        (discipline_root, target)
    }
}

fn review_target(destination: &Path, source: &Path, pages: &[usize]) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("scan");
    let first = pages.first().copied().unwrap_or(0) + 1;
    let last = pages.last().copied().unwrap_or(first - 1) + 1;
    destination
        .join(REVIEW_DIR)
        .join(format!("{stem} - pages {first:04}-{last:04}.pdf"))
}

fn page_range(pages: &[usize]) -> String {
    match (pages.first(), pages.last()) {
        (Some(first), Some(last)) => format!("pages {}-{}", first + 1, last + 1),
        _ => "pages unavailable".to_string(),
    }
}

fn push_candidate_output(
    plan: &mut Plan,
    candidate: &Candidate,
    destination: &Path,
    note_key: &str,
    mut meta: JsonObject,
    keep_original: bool,
) {
    if let Some(pages) = candidate.pages.as_deref() {
        meta.insert(
            "pageIndexes".to_string(),
            Value::Array(pages.iter().map(|page| Value::from(*page as u64)).collect()),
        );
        meta.insert(
            "sourceSha256".to_string(),
            Value::String(
                candidate
                    .source_hash
                    .as_deref()
                    .expect("split page candidate has a Preview source hash")
                    .to_string(),
            ),
        );
        if !candidate.rotations.is_empty() {
            let rotations = pages
                .iter()
                .enumerate()
                .filter_map(|(output_page, source_page)| {
                    candidate
                        .rotations
                        .iter()
                        .find(|(page, _, _)| page == source_page)
                        .map(|(_, target, confidence)| {
                            serde_json::json!({
                                "pageIndex": output_page,
                                "targetRotation": target,
                                "confidence": confidence,
                            })
                        })
                })
                .collect::<Vec<_>>();
            meta.insert("rotations".to_string(), Value::Array(rotations));
        }
        plan.steps.push(step(
            StepKind::PdfPagesExtract,
            Some(&candidate.path),
            Some(destination),
            note_key,
            meta,
        ));
    } else {
        plan.steps.push(step(
            if keep_original {
                StepKind::Copy
            } else {
                StepKind::Move
            },
            Some(&candidate.path),
            Some(destination),
            note_key,
            meta,
        ));
    }
}

fn hash_pdf(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > crate::pdf::MAX_PDF_BYTES {
        return Err("source PDF is not a bounded regular file".to_string());
    }
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BackendError> {
    let path = fs::canonicalize(path)
        .map_err(|_| BackendError::path_invalid(format!("{label} does not exist")))?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err(BackendError::path_invalid(format!(
            "{label} is not a directory"
        )))
    }
}

fn empty_plan(operation_id: &str) -> Plan {
    Plan {
        plan_id: String::new(),
        op_id: operation_id.to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
        meta: JsonObject::new(),
    }
}

fn step(
    kind: StepKind,
    source: Option<&Path>,
    destination: Option<&Path>,
    note_key: &str,
    meta: JsonObject,
) -> Step {
    Step {
        kind,
        src: source.map(|path| path.to_string_lossy().into_owned()),
        dst: destination.map(|path| path.to_string_lossy().into_owned()),
        note_key: Some(note_key.to_string()),
        meta,
    }
}

fn method_name(method: RfiMethod) -> &'static str {
    match method {
        RfiMethod::Strict => "strict",
        RfiMethod::Repaired => "repaired",
        RfiMethod::OcrAlias => "ocr_alias",
        RfiMethod::Fuzzy => "fuzzy",
        RfiMethod::Miss => "miss",
    }
}

fn same_file(left: &Path, right: &Path) -> bool {
    if !right.exists() {
        return false;
    }
    fs::canonicalize(left)
        .and_then(|left| fs::canonicalize(right).map(|right| left == right))
        .unwrap_or(false)
}
