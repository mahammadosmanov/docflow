//! RFI-system scanning and exact, guarded workbook synchronization.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom},
    path::{Path, PathBuf},
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    fs_ops::file_sha256,
    naming::FILE_PREFIX,
    ooxml::{
        CellData, CellUpdate, GuardedCellUpdate, WorkbookData, WorksheetData,
        patch_workbook_sheets_guarded_text, read_workbook_from_reader,
        verify_guarded_workbook_delta_text,
    },
    parallel::{bounded_map, bounded_map_stream},
    rfi_code::normalize_candidate_number,
    rfi_follow_up::{WorkbookFingerprint, fingerprint_open_file},
    scan_index::{FileFingerprint, ScanIndex, ScanIndexStats},
};

pub const OPERATION_ID: &str = "rfi.system_sync";
pub const COMPLETE_STATUS_OPERATION_ID: &str = "rfi.complete_status_sync";
pub const WORLEY_SIGNED_MARKER: &str = "Signed by Worley";
pub const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
const MAX_HEADER_ROWS: u32 = 32;
const MAX_HEADER_COLUMNS: u32 = 64;
const SYSTEM_SCAN_PROFILE: &str = "worley-first-page-v2-others-only";
const MAX_COMPLETION_ENTRIES: usize = 250_000;
const MAX_COMPLETION_WARNINGS: usize = 100;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[doc(hidden)]
pub struct ScannedRfiRecord {
    pub discipline: String,
    pub number: u32,
    pub canonical_rfi: String,
    pub short_rfi: String,
    pub rfi_folder: String,
    pub pdf_path: Option<String>,
    pub signed_by_worley: bool,
    pub signature_reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedWorleyDetection {
    signed_by_worley: bool,
    signature_reason: String,
    source_sha256: Option<String>,
}

#[derive(Clone, Debug)]
struct CandidateAnalysis {
    record: ScannedRfiRecord,
    relative: Option<PathBuf>,
    fingerprint: Option<FileFingerprint>,
    detection: Option<CachedWorleyDetection>,
    reused: bool,
    transient_failure: bool,
    subject: String,
}

#[derive(Clone, Debug)]
struct CandidateAnalysisError {
    relative: Option<PathBuf>,
    message: String,
}

impl CandidateAnalysisError {
    fn new(relative: Option<&Path>, message: impl Into<String>) -> Self {
        Self {
            relative: relative.map(Path::to_path_buf),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemSyncRow {
    #[serde(rename = "_rowId")]
    pub row_id: String,
    pub discipline: String,
    pub rfi_no: String,
    pub canonical_rfi: String,
    pub worley_status: String,
    pub rfi_log_action: String,
    pub follow_up_action: String,
    pub disposition: String,
    pub reason_key: String,
    pub enabled: bool,
    pub selectable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SyncWorkbookKind {
    RfiLog,
    RfiLogStatus,
    FollowUp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SystemSyncStepMeta {
    pub target_path: String,
    pub backup_path: String,
    pub source_fingerprint: WorkbookFingerprint,
    pub workbook_kind: SyncWorkbookKind,
    pub updates: BTreeMap<String, BTreeMap<String, GuardedCellUpdateDto>>,
    #[serde(default)]
    pub canonical_rfi_codes: Vec<String>,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GuardedCellUpdateDto {
    pub row_id: String,
    pub expected_value: String,
    pub expected_formula: Option<String>,
    pub expected_style: Option<String>,
    pub replacement_value: String,
    pub row: u32,
    pub col: u32,
}

#[derive(Clone, Debug)]
struct RfiLogRowInfo {
    sheet: String,
    row: u32,
    canonical_rfi: String,
    remarks_col: u32,
    remarks_cell: Option<CellData>,
    remarks_merged: bool,
}

#[derive(Clone, Debug)]
struct RfiLogStatusRowInfo {
    sheet: String,
    row: u32,
    canonical_rfi: String,
    status_col: u32,
    status_cell: Option<CellData>,
    status_merged: bool,
}

struct CompletionPdfDiscovery {
    matches: BTreeMap<String, Vec<PathBuf>>,
    invalid_names: Vec<PathBuf>,
    worker_count: usize,
    directories_scanned: usize,
    entries_scanned: usize,
}

struct CompletionDirectoryBatch {
    directories: Vec<PathBuf>,
    matches: Vec<(String, PathBuf)>,
    invalid_names: Vec<PathBuf>,
    entries_scanned: usize,
}

#[derive(Clone, Debug)]
struct FollowUpTarget {
    sheet: String,
    row: u32,
    status_col: u32,
    status_cell: Option<CellData>,
    status_merged: bool,
}

#[derive(Clone, Copy)]
struct HeaderLayout {
    row: u32,
    rfi_col: u32,
    value_col: u32,
}

mod codes;
mod log;
mod preview;
mod revise;
mod scan;
pub(crate) use codes::*;
use log::*;
pub use preview::*;
pub use revise::*;
pub use scan::*;

#[cfg(test)]
mod candidate_normalization_tests;
