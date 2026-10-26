//! Pure RFI Follow Up matching, layout, and classification primitives.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{LazyLock, OnceLock},
    time::UNIX_EPOCH,
};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{
    cap_fs::file_identity,
    contracts::{Plan, Step, StepKind, Warning},
    errors::BackendError,
    journal::FilesystemObjectIdentity,
    ooxml::{
        CellData, CellUpdate, GuardedCellUpdate, OoxmlError, OoxmlErrorKind,
        OrderedWorkbookDiscovery, RichRun, RowOp, WorkbookData, WorksheetData, WorksheetDescriptor,
        WorksheetVisibility, apply_row_ops, package_has_signature,
        patch_workbook_sheets_guarded_text_recalc, read_workbook_and_discovery,
        verify_guarded_workbook_delta_text_recalc,
    },
    rfi_code::normalize_candidate_number,
    rfi_log::status_columns,
};

pub(crate) const OPERATION_ID: &str = "rfi.follow_up_sync";
const SOURCE_FIRST_DATA_ROW: u32 = 10;
const MAX_SOURCE_DATA_ROWS_PER_SHEET: u32 = 20_000;
const MAX_HEADER_ROWS: u32 = 32;
const MAX_HEADER_COLUMNS: u32 = 64;
const MAX_FOLLOW_UP_DATA_ROWS: u32 = 20_000;
const MAX_PREVIEW_ROWS: usize = 20_000;
const MAX_SOURCE_EVIDENCE_PER_RFI: usize = 500;
const MAX_PREVIEW_AUTHORITY_BYTES: u64 = 32 * 1024 * 1024;
const MAX_FOLLOW_UP_MERGE_COMPARISONS: u64 = 20_000_000;
const MAX_CELL_TEXT_CHARS: usize = 20_000;
/// More than this many absent RFIs, and more than 1/5 of Follow Up's RFIs, pauses removal.
const MASS_DELETION_FLOOR: usize = 50;
const MASS_DELETION_DENOMINATOR: usize = 5;
const FULL_RFI_PREFIX: &str = "AZE-SUN-INT-QMS-RFI-";
const WORLEY_SIGNED_MARKER: &str = "Signed by Worley";

#[cfg(test)]
static SOURCE_ANALYSIS_CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CellSnapshot {
    value: String,
    formula: Option<String>,
    style: Option<String>,
}

impl From<&CellData> for CellSnapshot {
    fn from(cell: &CellData) -> Self {
        Self {
            value: cell.value.clone(),
            formula: cell.formula.clone(),
            style: cell.style.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceStatusEvidence {
    sheet: String,
    row: u32,
    rfi_column: u32,
    status_column: u32,
    rfi_cell: CellSnapshot,
    status_cell: Option<CellSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FollowUpLayout {
    worksheet: WorksheetDescriptor,
    header_row: u32,
    rfi_column: u32,
    status_column: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) clause: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum FollowUpDisposition {
    #[serde(rename = "rfi.followUp.disposition.ready")]
    Ready,
    #[serde(rename = "rfi.followUp.disposition.alreadySynced")]
    AlreadySynced,
    #[serde(rename = "rfi.followUp.disposition.manualReview")]
    ManualReview,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpReviewTarget {
    worksheet: WorksheetDescriptor,
    row: u32,
    rfi_column: u32,
    status_column: u32,
    rfi_cell: Option<CellSnapshot>,
    status_cell: Option<CellSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkbookFingerprint {
    identity: FilesystemObjectIdentity,
    size: u64,
    mtime_ns: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpCellTarget {
    worksheet: WorksheetDescriptor,
    row: u32,
    rfi_column: u32,
    status_column: u32,
    rfi_cell: CellSnapshot,
    status_cell: CellSnapshot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpApprovedUpdate {
    row_id: String,
    canonical_rfi: String,
    proposed_status: String,
    #[serde(default)]
    proposed_rfi: String,
    target: FollowUpCellTarget,
    source_evidence: Vec<SourceStatusEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FollowUpStepMeta {
    rfi_log_workbook: String,
    rfi_log_fingerprint: WorkbookFingerprint,
    follow_up_fingerprint: WorkbookFingerprint,
    layout: FollowUpLayout,
    #[serde(default)]
    layouts: Vec<FollowUpLayout>,
    updates: Vec<FollowUpApprovedUpdate>,
    #[serde(default)]
    remarks: Vec<FollowUpApprovedRemark>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    formulas: Vec<FollowUpApprovedFormula>,
    /// Road part rows to add or relabel before the cell updates; applied in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    structure: Vec<RowOp>,
}

impl FollowUpStepMeta {
    pub(crate) fn all_layouts(&self) -> impl Iterator<Item = &FollowUpLayout> {
        if self.layouts.is_empty() {
            std::slice::from_ref(&self.layout).iter()
        } else {
            self.layouts.iter()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpPlanMeta {
    rfi_log_workbook: String,
    follow_up_workbook: String,
    backup_workbook: String,
    rfi_log_fingerprint: WorkbookFingerprint,
    follow_up_fingerprint: WorkbookFingerprint,
    layout: FollowUpLayout,
    #[serde(default)]
    layouts: Vec<FollowUpLayout>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    structure: Vec<RowOp>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpRow {
    #[serde(rename = "_rowId")]
    row_id: String,
    #[serde(rename = "_sheet")]
    sheet: String,
    #[serde(rename = "_row")]
    row: u32,
    rfi_no: String,
    source_statuses: Vec<String>,
    current_status: String,
    proposed_status: Option<String>,
    #[serde(default)]
    proposed_rfi_no: Option<String>,
    disposition: FollowUpDisposition,
    reason_key: String,
    selectable: bool,
    enabled: bool,
    target: FollowUpReviewTarget,
    source_evidence: Vec<SourceStatusEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mapping_details: Option<String>,
    /// Present only on remark review rows; they write no RFI or status cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remark: Option<FollowUpRemarkChange>,
    /// Present only on formula repair rows: empty formula cells to fill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    formula_fixes: Vec<FollowUpFormulaFix>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpFormulaFix {
    column: u32,
    formula: String,
    style: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpApprovedFormula {
    row_id: String,
    worksheet: WorksheetDescriptor,
    row: u32,
    column: u32,
    formula: String,
    style: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpRemarkChange {
    column: u32,
    /// `None` when the remark cell is absent from the worksheet XML.
    expected: Option<CellSnapshot>,
    /// Empty clears a generated remark.
    runs: Vec<RemarkRun>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FollowUpApprovedRemark {
    row_id: String,
    worksheet: WorksheetDescriptor,
    row: u32,
    column: u32,
    expected: Option<CellSnapshot>,
    runs: Vec<RemarkRun>,
}

#[derive(Clone, Debug)]
struct SourceAnalysis {
    statuses: Vec<String>,
    proposed_status: Option<String>,
    reason: Option<&'static str>,
}

#[derive(Clone, Debug)]
struct PendingTargetRow {
    row: u32,
    rfi_cell: Option<CellSnapshot>,
    status_cell: Option<CellSnapshot>,
    visible_rfi: String,
    canonical_rfi: Option<String>,
    current_status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BorrowedReviewTarget<'a> {
    worksheet: &'a WorksheetDescriptor,
    row: u32,
    rfi_column: u32,
    status_column: u32,
    rfi_cell: Option<&'a CellSnapshot>,
    status_cell: Option<&'a CellSnapshot>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BorrowedFollowUpRow<'a> {
    #[serde(rename = "_rowId")]
    row_id: String,
    #[serde(rename = "_sheet")]
    sheet: &'a str,
    #[serde(rename = "_row")]
    row: u32,
    rfi_no: &'a str,
    source_statuses: &'a [String],
    current_status: &'a str,
    proposed_status: Option<&'a str>,
    disposition: FollowUpDisposition,
    reason_key: String,
    selectable: bool,
    enabled: bool,
    target: BorrowedReviewTarget<'a>,
    source_evidence: &'a [SourceStatusEvidence],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mapping_details: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BorrowedEditor<'a> {
    kind: &'static str,
    layout: &'static str,
    title_key: &'static str,
    row_count_key: &'static str,
    required_fields: [&'static str; 1],
    group_field: &'static str,
    selection_group_fields: [&'static str; 0],
    columns: &'a [Value],
    rows: &'a [BorrowedFollowUpRow<'a>],
    duplicate_enabled: bool,
    selection_enabled: bool,
    bulk_enabled: bool,
    reorderable: bool,
    details_enabled: bool,
    details_default_visible: bool,
    copy_enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BorrowedAuthorityProjection<'a> {
    layout: &'a FollowUpLayout,
    #[serde(skip_serializing_if = "slice_is_empty")]
    layouts: &'a [FollowUpLayout],
    rows: &'a [BorrowedFollowUpRow<'a>],
    warnings: &'a [Warning],
    editor: BorrowedEditor<'a>,
    approved_rows: Vec<&'a BorrowedFollowUpRow<'a>>,
}

fn slice_is_empty<T>(slice: &[T]) -> bool {
    slice.is_empty()
}

struct CountingLimitWriter {
    count: u64,
    limit: u64,
}

impl Write for CountingLimitWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .count
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| io::Error::other("projection byte count overflow"))?;
        if next > self.limit {
            return Err(io::Error::other("projection byte limit exceeded"));
        }
        self.count = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct MergeQueryBudget {
    remaining: u64,
}

impl MergeQueryBudget {
    fn contains(
        &mut self,
        worksheet: &WorksheetData,
        row: u32,
        column: u32,
    ) -> Result<bool, BackendError> {
        self.remaining = self
            .remaining
            .checked_sub(worksheet.merged_ranges.len() as u64)
            .ok_or_else(|| limit_exceeded_for("merged-cell comparison budget"))?;
        Ok(worksheet.merged_ranges.iter().any(|range| {
            range.first_row <= row
                && row <= range.last_row
                && range.first_column <= column
                && column <= range.last_column
        }))
    }
}

mod apply;
mod classify;
mod codes;
mod formula_rows;
mod preview;
mod remark;
mod remark_rows;
mod road;
mod road_structure;
mod routing;
mod scan;
mod source;
mod structure_rows;
pub use apply::*;
pub use classify::*;
pub use codes::*;
use formula_rows::*;
pub use preview::*;
use remark::*;
use remark_rows::*;
use road::*;
use road_structure::*;
use routing::*;
use scan::*;
use source::*;
use structure_rows::*;

#[cfg(test)]
mod tests;
