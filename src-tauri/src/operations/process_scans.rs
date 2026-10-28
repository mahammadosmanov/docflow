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
mod preview;
mod scan_pdf;
pub use editor::*;
pub use ocr::*;
use output::*;
use preview::*;
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

fn plan_ocr_tasks(
    pdfs: &[PathBuf],
    inspections: &PdfInspections,
    scan_mode: ScanMode,
    fast_cover_mode: bool,
) -> Result<(OcrTasks, BTreeSet<PathBuf>), BackendError> {
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
            if fast_cover_mode && scan_mode == ScanMode::MultiRfi && page_count > 1 {
                fast_cover_pdfs.insert(pdf.clone());
            }
            let ocr_page_count = if scan_mode == ScanMode::SingleRfi {
                1
            } else {
                page_count
            };
            tasks.extend((0..ocr_page_count).map(|page| (pdf.clone(), page)));
        }
    }
    Ok((tasks, fast_cover_pdfs))
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
