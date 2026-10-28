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
mod inspect;
mod ocr;
mod output;
mod preview;
mod scan_pdf;
pub use editor::*;
use inspect::*;
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
