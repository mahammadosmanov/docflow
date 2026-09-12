//! Pure RFI Follow Up matching, layout, and classification primitives.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::OnceLock,
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
        OrderedWorkbookDiscovery, WorkbookData, WorksheetData, WorksheetDescriptor,
        WorksheetVisibility, ordered_workbook_discovery, package_has_signature,
        patch_workbook_sheets_guarded, read_workbook_from_reader, verify_guarded_workbook_delta,
    },
    rfi_log::status_columns,
};

#[allow(dead_code)]
pub(crate) const OPERATION_ID: &str = "rfi.follow_up_sync";
const SOURCE_FIRST_DATA_ROW: u32 = 10;
const MAX_SOURCE_DATA_ROWS_PER_SHEET: u32 = 20_000;
const MAX_HEADER_ROWS: u32 = 32;
const MAX_HEADER_COLUMNS: u32 = 64;
const MAX_FOLLOW_UP_DATA_ROWS: u32 = 20_000;
const MAX_PREVIEW_ROWS: usize = 5_000;
const MAX_SOURCE_EVIDENCE_PER_RFI: usize = 500;
const MAX_PREVIEW_AUTHORITY_BYTES: u64 = 32 * 1024 * 1024;
const MAX_FOLLOW_UP_MERGE_COMPARISONS: u64 = 20_000_000;
const MAX_CELL_TEXT_CHARS: usize = 20_000;
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
#[allow(dead_code)]
pub(crate) struct WorkbookFingerprint {
    identity: FilesystemObjectIdentity,
    size: u64,
    mtime_ns: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
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
#[allow(dead_code)]
struct FollowUpApprovedUpdate {
    row_id: String,
    canonical_rfi: String,
    proposed_status: String,
    target: FollowUpCellTarget,
    source_evidence: Vec<SourceStatusEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct FollowUpStepMeta {
    rfi_log_workbook: String,
    rfi_log_fingerprint: WorkbookFingerprint,
    follow_up_fingerprint: WorkbookFingerprint,
    layout: FollowUpLayout,
    #[serde(default)]
    layouts: Vec<FollowUpLayout>,
    updates: Vec<FollowUpApprovedUpdate>,
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
    disposition: FollowUpDisposition,
    reason_key: String,
    selectable: bool,
    enabled: bool,
    target: FollowUpReviewTarget,
    source_evidence: Vec<SourceStatusEvidence>,
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
            .ok_or_else(limit_exceeded)?;
        Ok(worksheet.merged_ranges.iter().any(|range| {
            range.first_row <= row
                && row <= range.last_row
                && range.first_column <= column
                && column <= range.last_column
        }))
    }
}

pub fn canonical_rfi(value: &str) -> Option<String> {
    static RFI: OnceLock<Regex> = OnceLock::new();
    let normalized = normalize_status(value).to_uppercase();
    RFI.get_or_init(|| {
        Regex::new(r"\AAZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4}\z")
            .expect("canonical RFI regex is valid")
    })
    .is_match(&normalized)
    .then_some(normalized)
}

pub fn canonical_follow_up_rfi(value: &str) -> Option<(String, String)> {
    static SHORT_RFI: OnceLock<Regex> = OnceLock::new();
    let normalized = normalize_status(value).to_uppercase();
    let short = if let Some(short) = normalized.strip_prefix(FULL_RFI_PREFIX) {
        short.to_string()
    } else {
        normalized
    };
    if !SHORT_RFI
        .get_or_init(|| {
            Regex::new(r"\A(?:CIV|MEC|ELE|PRC)-[0-9]{4}\z")
                .expect("short Follow Up RFI regex is valid")
        })
        .is_match(&short)
    {
        return None;
    }
    Some((format!("{FULL_RFI_PREFIX}{short}"), short))
}

pub fn mapped_follow_up_status(remark: &str) -> &'static str {
    if remark
        .lines()
        .any(|line| normalize_status(line) == WORLEY_SIGNED_MARKER)
    {
        "Signed"
    } else {
        "Not Signed"
    }
}

pub fn classify_workbooks(
    source_discovery: &OrderedWorkbookDiscovery,
    source_workbook: &WorkbookData,
    target_discovery: &OrderedWorkbookDiscovery,
    target_workbook: &WorkbookData,
    target_has_signature: bool,
) -> Result<Value, BackendError> {
    let source = scan_source(source_discovery, source_workbook)?;
    let mut merge_budget = MergeQueryBudget {
        remaining: MAX_FOLLOW_UP_MERGE_COMPARISONS,
    };
    let mut analyses = BTreeMap::new();
    for (rfi, evidence) in &source {
        analyses.insert(
            rfi.clone(),
            analyze_source(evidence, source_workbook, &mut merge_budget)?,
        );
    }
    let empty_analysis = SourceAnalysis {
        statuses: Vec::new(),
        proposed_status: None,
        reason: None,
    };
    let layouts = discover_layouts(
        target_discovery,
        target_workbook,
        target_has_signature,
        &mut merge_budget,
    )?;
    let mut all_pending = Vec::new();
    for layout in &layouts {
        let worksheet = target_workbook
            .sheet(&layout.worksheet.name)
            .ok_or_else(workbook_invalid)?;
        let pending = scan_target_rows(worksheet, layout)?;
        for row in pending {
            all_pending.push((layout, worksheet, row));
        }
    }
    let duplicate_counts = all_pending
        .iter()
        .filter_map(|(_, _, row)| row.canonical_rfi.as_ref())
        .fold(BTreeMap::<String, usize>::new(), |mut counts, rfi| {
            *counts.entry(rfi.clone()).or_default() += 1;
            counts
        });
    let borrowed_rows = all_pending
        .iter()
        .map(|(layout, worksheet, pending)| {
            let evidence = pending
                .canonical_rfi
                .as_ref()
                .and_then(|rfi| source.get(rfi))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let analysis = pending
                .canonical_rfi
                .as_ref()
                .and_then(|rfi| analyses.get(rfi))
                .unwrap_or(&empty_analysis);
            classify_borrowed_row(
                pending,
                layout,
                worksheet,
                evidence,
                analysis,
                &duplicate_counts,
                &mut merge_budget,
            )
        })
        .collect::<Result<Vec<_>, BackendError>>()?;

    let manual_count = borrowed_rows
        .iter()
        .filter(|row| row.disposition == FollowUpDisposition::ManualReview)
        .count();
    let enabled_count = borrowed_rows.iter().filter(|row| row.enabled).count();
    let mut warnings = Vec::new();
    if manual_count > 0 {
        warnings.push(Warning {
            key: "warn.rfi.followUp.manualRows".to_string(),
            subject: manual_count.to_string(),
        });
    }
    if enabled_count == 0 {
        warnings.push(Warning {
            key: "warn.rfi.followUp.noRowsSelected".to_string(),
            subject: String::new(),
        });
    }
    let columns = editor_columns();
    let primary_layout = layouts.first().ok_or_else(workbook_invalid)?;
    {
        let projection = BorrowedAuthorityProjection {
            layout: primary_layout,
            layouts: &layouts,
            rows: &borrowed_rows,
            warnings: &warnings,
            editor: borrowed_editor(&columns, &borrowed_rows),
            approved_rows: borrowed_rows.iter().filter(|row| row.enabled).collect(),
        };
        validate_projection_budget(&projection)?;
    }
    let rows = borrowed_rows
        .iter()
        .map(BorrowedFollowUpRow::materialize)
        .collect::<Vec<_>>();
    let editor = editor_metadata(&rows, columns);
    Ok(json!({
        "layout": primary_layout,
        "layouts": layouts,
        "rows": rows,
        "warnings": warnings,
        "editor": editor,
    }))
}

pub fn preview_from_open_files(
    rfi_log_path: &Path,
    follow_up_path: &Path,
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<Plan, BackendError> {
    let rfi_log_display = rfi_log_path.to_str().ok_or_else(|| {
        BackendError::validation(
            "error.rfi.followUp.workbookInvalid: RFI Log path contains invalid UTF-8",
        )
    })?;
    let follow_up_display = follow_up_path.to_str().ok_or_else(|| {
        BackendError::validation(
            "error.rfi.followUp.workbookInvalid: Follow Up path contains invalid UTF-8",
        )
    })?;
    let source_fingerprint = fingerprint_open_file(rfi_log)?;
    let target_fingerprint = fingerprint_open_file(follow_up)?;
    if source_fingerprint.identity == target_fingerprint.identity {
        return Err(BackendError::validation(
            "RFI Log and Follow Up workbooks must be different files",
        ));
    }

    rfi_log.seek(SeekFrom::Start(0)).map_err(|e| {
        BackendError::validation(format!(
            "error.rfi.followUp.workbookInvalid: RFI Log seek failed: {e}"
        ))
    })?;
    let source_discovery =
        ordered_workbook_discovery(rfi_log).map_err(map_follow_up_ooxml_error)?;
    rfi_log.seek(SeekFrom::Start(0)).map_err(|e| {
        BackendError::validation(format!(
            "error.rfi.followUp.workbookInvalid: RFI Log seek failed: {e}"
        ))
    })?;
    let source_workbook = read_workbook_from_reader(rfi_log).map_err(map_follow_up_ooxml_error)?;

    follow_up.seek(SeekFrom::Start(0)).map_err(|e| {
        BackendError::validation(format!(
            "error.rfi.followUp.workbookInvalid: Follow Up seek failed: {e}"
        ))
    })?;
    let target_has_signature =
        package_has_signature(follow_up).map_err(map_follow_up_ooxml_error)?;
    follow_up.seek(SeekFrom::Start(0)).map_err(|e| {
        BackendError::validation(format!(
            "error.rfi.followUp.workbookInvalid: Follow Up seek failed: {e}"
        ))
    })?;
    let target_discovery =
        ordered_workbook_discovery(follow_up).map_err(map_follow_up_ooxml_error)?;
    follow_up.seek(SeekFrom::Start(0)).map_err(|e| {
        BackendError::validation(format!(
            "error.rfi.followUp.workbookInvalid: Follow Up seek failed: {e}"
        ))
    })?;
    let target_workbook =
        read_workbook_from_reader(follow_up).map_err(map_follow_up_ooxml_error)?;
    let classification = classify_workbooks(
        &source_discovery,
        &source_workbook,
        &target_discovery,
        &target_workbook,
        target_has_signature,
    )?;
    let extension = follow_up_path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| value.eq_ignore_ascii_case("xlsx") || value.eq_ignore_ascii_case("xlsm"))
        .ok_or_else(|| {
            BackendError::validation(
                "error.rfi.followUp.workbookInvalid: Follow Up workbook must have .xlsx or .xlsm extension",
            )
        })?;
    let backup = follow_up_path
        .parent()
        .ok_or_else(|| {
            BackendError::validation(
                "error.rfi.followUp.workbookInvalid: Follow Up workbook parent directory missing",
            )
        })?
        .join(format!(
            ".docflow-s5-backup-{}.{}",
            Uuid::new_v4().simple(),
            extension.to_ascii_lowercase()
        ));
    let backup_display = backup.to_str().ok_or_else(|| {
        BackendError::validation(
            "error.rfi.followUp.workbookInvalid: backup path contains invalid UTF-8",
        )
    })?;
    plan_from_classification(
        rfi_log_display,
        follow_up_display,
        backup_display,
        source_fingerprint,
        target_fingerprint,
        classification,
    )
}

pub fn revise_from_open_files(
    plan: &Plan,
    submitted_rows: &[Value],
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<Plan, BackendError> {
    if plan.op_id != OPERATION_ID {
        return Err(BackendError::validation(
            "plan is not an RFI Follow Up plan",
        ));
    }
    let authority: FollowUpPlanMeta = serde_json::from_value(
        plan.meta
            .get("rfiFollowUp")
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    let original_rows: Vec<FollowUpRow> = serde_json::from_value(
        plan.meta
            .get("editor")
            .and_then(|editor| editor.get("rows"))
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    let mut revised_rows = submitted_rows
        .iter()
        .cloned()
        .map(|row| serde_json::from_value(row).map_err(|_| workbook_invalid()))
        .collect::<Result<Vec<FollowUpRow>, BackendError>>()?;
    if original_rows.len() != revised_rows.len() {
        return Err(workbook_invalid());
    }
    for (original, revised) in original_rows.iter().zip(&revised_rows) {
        if original.row_id != revised.row_id || !rows_equal_except_enabled(original, revised) {
            return Err(workbook_invalid());
        }
        if revised.enabled
            && (!original.selectable || original.disposition != FollowUpDisposition::Ready)
        {
            return Err(workbook_invalid());
        }
    }

    let fresh = preview_from_open_files(
        Path::new(&authority.rfi_log_workbook),
        Path::new(&authority.follow_up_workbook),
        rfi_log,
        follow_up,
    )?;
    let fresh_authority: FollowUpPlanMeta = serde_json::from_value(
        fresh
            .meta
            .get("rfiFollowUp")
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    if authority.rfi_log_workbook != fresh_authority.rfi_log_workbook
        || authority.follow_up_workbook != fresh_authority.follow_up_workbook
        || authority.rfi_log_fingerprint != fresh_authority.rfi_log_fingerprint
        || authority.follow_up_fingerprint != fresh_authority.follow_up_fingerprint
        || authority.layout != fresh_authority.layout
        || authority.layouts != fresh_authority.layouts
    {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookChanged",
        ));
    }
    let fresh_rows: Vec<FollowUpRow> = serde_json::from_value(
        fresh
            .meta
            .get("editor")
            .and_then(|editor| editor.get("rows"))
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    if fresh_rows.len() != original_rows.len()
        || original_rows
            .iter()
            .zip(&fresh_rows)
            .any(|(original, fresh)| !rows_equal_except_enabled(original, fresh))
    {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookChanged",
        ));
    }
    for (revised, fresh) in revised_rows.iter_mut().zip(fresh_rows) {
        revised.enabled = revised.enabled && fresh.selectable;
    }
    let mut warnings = fresh.warnings;
    warnings.retain(|warning| warning.key != "warn.rfi.followUp.noRowsSelected");
    if revised_rows.iter().all(|row| !row.enabled) {
        warnings.push(Warning {
            key: "warn.rfi.followUp.noRowsSelected".to_string(),
            subject: String::new(),
        });
    }
    let columns = editor_columns();
    let classification = json!({
        "layout": authority.layout,
        "rows": revised_rows,
        "warnings": warnings,
        "editor": editor_metadata(&revised_rows, columns),
    });
    let mut revised = plan_from_classification(
        &authority.rfi_log_workbook,
        &authority.follow_up_workbook,
        &authority.backup_workbook,
        fresh_authority.rfi_log_fingerprint,
        fresh_authority.follow_up_fingerprint,
        classification,
    )?;
    revised.plan_id.clone_from(&plan.plan_id);
    validate_owned_plan_budget(&revised)?;
    Ok(revised)
}

fn rows_equal_except_enabled(left: &FollowUpRow, right: &FollowUpRow) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.enabled = false;
    right.enabled = false;
    left == right
}

pub(crate) fn plan_workbook_paths(plan: &Plan) -> Result<(String, String), BackendError> {
    let authority: FollowUpPlanMeta = serde_json::from_value(
        plan.meta
            .get("rfiFollowUp")
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    Ok((authority.rfi_log_workbook, authority.follow_up_workbook))
}

pub(crate) fn validate_owned_plan_budget(plan: &Plan) -> Result<(), BackendError> {
    let mut writer = CountingLimitWriter {
        count: 0,
        limit: MAX_PREVIEW_AUTHORITY_BYTES,
    };
    serde_json::to_writer(&mut writer, plan).map_err(|_| limit_exceeded())
}

fn fingerprint_open_file(file: &mut File) -> Result<WorkbookFingerprint, BackendError> {
    let metadata = file.metadata().map_err(|_| workbook_invalid())?;
    let identity = file_identity(file).map_err(|_| workbook_invalid())?;
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0);
    file.seek(SeekFrom::Start(0))
        .map_err(|_| workbook_invalid())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| workbook_invalid())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| workbook_invalid())?;
    Ok(WorkbookFingerprint {
        identity,
        size: metadata.len(),
        mtime_ns,
        sha256: format!("{:x}", digest.finalize()),
    })
}

fn plan_from_classification(
    rfi_log_workbook: &str,
    follow_up_workbook: &str,
    backup_workbook: &str,
    rfi_log_fingerprint: WorkbookFingerprint,
    follow_up_fingerprint: WorkbookFingerprint,
    classification: Value,
) -> Result<Plan, BackendError> {
    let object = classification.as_object().ok_or_else(workbook_invalid)?;
    let decode_field = |name: &str| object.get(name).cloned().ok_or_else(workbook_invalid);
    let layout: FollowUpLayout =
        serde_json::from_value(decode_field("layout")?).map_err(|_| workbook_invalid())?;
    let layouts: Vec<FollowUpLayout> = object
        .get("layouts")
        .and_then(|val| serde_json::from_value(val.clone()).ok())
        .unwrap_or_else(|| vec![layout.clone()]);
    let rows: Vec<FollowUpRow> =
        serde_json::from_value(decode_field("rows")?).map_err(|_| workbook_invalid())?;
    let warnings: Vec<Warning> =
        serde_json::from_value(decode_field("warnings")?).map_err(|_| workbook_invalid())?;
    let editor = decode_field("editor")?;
    let updates = rows
        .iter()
        .filter(|row| row.enabled)
        .map(|row| {
            if !row.selectable || row.disposition != FollowUpDisposition::Ready {
                return Err(workbook_invalid());
            }
            Ok(FollowUpApprovedUpdate {
                row_id: row.row_id.clone(),
                canonical_rfi: canonical_follow_up_rfi(&row.rfi_no)
                    .map(|(canonical, _)| canonical)
                    .ok_or_else(workbook_invalid)?,
                proposed_status: row.proposed_status.clone().ok_or_else(workbook_invalid)?,
                target: FollowUpCellTarget {
                    worksheet: row.target.worksheet.clone(),
                    row: row.target.row,
                    rfi_column: row.target.rfi_column,
                    status_column: row.target.status_column,
                    rfi_cell: row.target.rfi_cell.clone().ok_or_else(workbook_invalid)?,
                    status_cell: row
                        .target
                        .status_cell
                        .clone()
                        .ok_or_else(workbook_invalid)?,
                },
                source_evidence: row.source_evidence.clone(),
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    let plan_meta = FollowUpPlanMeta {
        rfi_log_workbook: rfi_log_workbook.to_string(),
        follow_up_workbook: follow_up_workbook.to_string(),
        backup_workbook: backup_workbook.to_string(),
        rfi_log_fingerprint: rfi_log_fingerprint.clone(),
        follow_up_fingerprint: follow_up_fingerprint.clone(),
        layout: layout.clone(),
        layouts: layouts.clone(),
    };
    let mut meta = serde_json::Map::new();
    meta.insert("editor".to_string(), editor);
    meta.insert(
        "rfiFollowUp".to_string(),
        serde_json::to_value(plan_meta).map_err(|_| workbook_invalid())?,
    );
    let steps = if updates.is_empty() {
        Vec::new()
    } else {
        let step_meta = FollowUpStepMeta {
            rfi_log_workbook: rfi_log_workbook.to_string(),
            rfi_log_fingerprint,
            follow_up_fingerprint,
            layout,
            layouts,
            updates,
        };
        let step_meta = serde_json::to_value(step_meta)
            .map_err(|_| workbook_invalid())?
            .as_object()
            .cloned()
            .ok_or_else(workbook_invalid)?;
        vec![Step {
            kind: StepKind::WorkbookUpdate,
            src: Some(follow_up_workbook.to_string()),
            dst: Some(backup_workbook.to_string()),
            note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
            meta: step_meta,
        }]
    };
    Ok(Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps,
        warnings,
        meta,
    })
}

fn matches_discipline_sheet(sheet_name: &str, discipline: &str) -> bool {
    let name = sheet_name.trim().to_uppercase();
    if name == discipline {
        return true;
    }
    let tokens: Vec<&str> = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '.' || c == '/')
        .filter(|t| !t.is_empty())
        .collect();
    let has_token = |patterns: &[&str]| {
        tokens
            .iter()
            .any(|t| patterns.iter().any(|p| *t == *p || t.starts_with(p)))
    };
    match discipline {
        "CIV" => {
            name.starts_with("CIV")
                || has_token(&[
                    "CIV",
                    "CIVIL",
                    "INSAAT",
                    "İNŞAAT",
                    "STRUCT",
                    "STRUCTURAL",
                    "CONCRETE",
                ])
        }
        "MEC" => {
            name.starts_with("MEC")
                || has_token(&[
                    "MEC",
                    "MECH",
                    "MECHANICAL",
                    "PIPE",
                    "PIPING",
                    "MEKANIK",
                    "MEKANİK",
                    "HVAC",
                ])
        }
        "ELE" => {
            name.starts_with("ELE")
                || has_token(&[
                    "ELE",
                    "ELEC",
                    "ELECTRICAL",
                    "INST",
                    "INSTRUMENTATION",
                    "ELEKTRIK",
                    "ELEKTRİK",
                ])
        }
        "PRC" => {
            name.starts_with("PRC")
                || name.starts_with("PROC")
                || has_token(&["PRC", "PROC", "PROCESS", "PROSES", "COMMISSIONING"])
        }
        _ => false,
    }
}

fn detect_rfi_and_status_columns(
    worksheet: &WorksheetData,
    default_rfi_col: u32,
    default_status_col: u32,
) -> (u32, u32) {
    let scan_rows = worksheet.max_row.min(100);
    let has_rfi_in_default = (1..=scan_rows).any(|row| {
        worksheet.cell(row, default_rfi_col).is_some_and(|cell| {
            canonical_rfi(&cell.value).is_some() || canonical_follow_up_rfi(&cell.value).is_some()
        })
    });
    let rfi_column = if has_rfi_in_default {
        default_rfi_col
    } else {
        (1..=10.min(worksheet.max_row))
            .find_map(|row| {
                (1..=64.min(worksheet.max_column)).find(|&col| {
                    worksheet
                        .cell(row, col)
                        .is_some_and(|cell| is_follow_up_rfi_header(&normalize_header(&cell.value)))
                })
            })
            .or_else(|| {
                (1..=64.min(worksheet.max_column)).find(|&col| {
                    (1..=scan_rows).any(|row| {
                        worksheet.cell(row, col).is_some_and(|cell| {
                            canonical_rfi(&cell.value).is_some()
                                || canonical_follow_up_rfi(&cell.value).is_some()
                        })
                    })
                })
            })
            .unwrap_or(default_rfi_col)
    };

    let mut detected_status = None;
    for row in 1..=worksheet.max_row.min(10) {
        for col in 1..=worksheet.max_column.min(64) {
            if col == rfi_column {
                continue;
            }
            if let Some(cell) = worksheet.cell(row, col) {
                let token = normalize_header(&cell.value);
                if is_follow_up_worley_status_header(&token)
                    || is_follow_up_generic_status_header(&token)
                    || token == "NOTLAR"
                    || token == "AÇIKLAMA"
                    || token == "ACIKLAMA"
                {
                    detected_status = Some(col);
                    break;
                }
            }
        }
        if detected_status.is_some() {
            break;
        }
    }
    let status_column = detected_status.unwrap_or(default_status_col);
    (rfi_column, status_column)
}

fn scan_source(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
) -> Result<BTreeMap<String, Vec<SourceStatusEvidence>>, BackendError> {
    if discovery.safety.has_workbook_protection {
        return Err(protected());
    }
    let mut grouped = BTreeMap::<String, Vec<SourceStatusEvidence>>::new();
    let mut scanned_sheets = BTreeSet::new();
    for projection in status_columns() {
        let descriptor = match discovery
            .worksheets
            .iter()
            .find(|worksheet| worksheet.name.trim().eq_ignore_ascii_case(projection.sheet))
            .or_else(|| {
                discovery
                    .worksheets
                    .iter()
                    .find(|worksheet| matches_discipline_sheet(&worksheet.name, projection.sheet))
            }) {
            Some(d) => d,
            None => continue,
        };
        let worksheet = match workbook.sheet(&descriptor.name) {
            Some(w) => w,
            None => continue,
        };
        if !scanned_sheets.insert(descriptor.name.clone()) {
            continue;
        }
        if worksheet.has_sheet_protection {
            return Err(protected());
        }
        validate_cell_text(worksheet)?;
        let last_allowed = SOURCE_FIRST_DATA_ROW + MAX_SOURCE_DATA_ROWS_PER_SHEET - 1;
        if worksheet.max_row > last_allowed {
            return Err(limit_exceeded());
        }
        let (rfi_col, status_col) = detect_rfi_and_status_columns(
            worksheet,
            projection.rfi_column,
            projection.status_column,
        );
        for row in 1..=worksheet.max_row {
            let Some(rfi_cell) = worksheet.cell(row, rfi_col) else {
                continue;
            };
            let Some(rfi) = canonical_rfi(&rfi_cell.value).or_else(|| {
                canonical_follow_up_rfi(&rfi_cell.value).map(|(canonical, _)| canonical)
            }) else {
                continue;
            };
            let status_cell = worksheet.cell(row, status_col);
            let evidence = grouped.entry(rfi).or_default();
            if evidence.len() >= MAX_SOURCE_EVIDENCE_PER_RFI {
                return Err(limit_exceeded());
            }
            evidence.push(SourceStatusEvidence {
                sheet: descriptor.name.clone(),
                row,
                rfi_column: rfi_col,
                status_column: status_col,
                rfi_cell: rfi_cell.into(),
                status_cell: status_cell.map(Into::into),
            });
        }
    }

    if scanned_sheets.is_empty() {
        // Fallback for consolidated or custom RFI Log workbooks (e.g. single-sheet "RFI Log", "RFIs", "Sheet1")
        for descriptor in &discovery.worksheets {
            if descriptor.visibility != WorksheetVisibility::Visible {
                continue;
            }
            let Some(worksheet) = workbook.sheet(&descriptor.name) else {
                continue;
            };
            if worksheet.has_sheet_protection {
                continue;
            }
            let (rfi_col, status_col) = detect_rfi_and_status_columns(worksheet, 1, 20);
            let last_allowed = SOURCE_FIRST_DATA_ROW + MAX_SOURCE_DATA_ROWS_PER_SHEET - 1;
            if worksheet.max_row > last_allowed {
                return Err(limit_exceeded());
            }
            let mut sheet_had_rfis = false;
            for row in 1..=worksheet.max_row {
                let Some(rfi_cell) = worksheet.cell(row, rfi_col) else {
                    continue;
                };
                let Some(rfi) = canonical_rfi(&rfi_cell.value).or_else(|| {
                    canonical_follow_up_rfi(&rfi_cell.value).map(|(canonical, _)| canonical)
                }) else {
                    continue;
                };
                let status_cell = worksheet.cell(row, status_col);
                let evidence = grouped.entry(rfi).or_default();
                if evidence.len() >= MAX_SOURCE_EVIDENCE_PER_RFI {
                    return Err(limit_exceeded());
                }
                evidence.push(SourceStatusEvidence {
                    sheet: descriptor.name.clone(),
                    row,
                    rfi_column: rfi_col,
                    status_column: status_col,
                    rfi_cell: rfi_cell.into(),
                    status_cell: status_cell.map(Into::into),
                });
                sheet_had_rfis = true;
            }
            if sheet_had_rfis {
                validate_cell_text(worksheet)?;
                scanned_sheets.insert(descriptor.name.clone());
            }
        }
    }

    if scanned_sheets.is_empty() {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookInvalid: no sheets containing valid RFI entries found in RFI Log",
        ));
    }
    Ok(grouped)
}

fn is_preferred_follow_up_sheet_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let trimmed = upper.trim();
    trimmed.contains("SIGNED BY")
        || trimmed.contains("FOLLOW UP")
        || trimmed.contains("FOLLOW-UP")
        || trimmed.contains("FOLLOWUP")
        || trimmed == "CIV"
        || trimmed == "MEC"
        || trimmed == "ELE"
        || trimmed == "PRC"
        || trimmed.starts_with("CIV ")
        || trimmed.starts_with("MEC ")
        || trimmed.starts_with("ELE ")
        || trimmed.starts_with("PRC ")
        || trimmed.ends_with(" CIV")
        || trimmed.ends_with(" MEC")
        || trimmed.ends_with(" ELE")
        || trimmed.ends_with(" PRC")
        || trimmed.ends_with("-CIV")
        || trimmed.ends_with("-MEC")
        || trimmed.ends_with("-ELE")
        || trimmed.ends_with("-PRC")
}

fn discover_layouts(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
    has_signature: bool,
    merge_budget: &mut MergeQueryBudget,
) -> Result<Vec<FollowUpLayout>, BackendError> {
    if has_signature {
        return Err(signed_package());
    }
    if discovery.safety.has_workbook_protection {
        return Err(protected());
    }
    let mut candidates = Vec::new();
    let mut duplicate_header_sheets = BTreeSet::new();
    for descriptor in &discovery.worksheets {
        let worksheet = workbook
            .sheet(&descriptor.name)
            .ok_or_else(workbook_invalid)?;
        validate_cell_text(worksheet)?;
        for row in 1..=worksheet.max_row.min(MAX_HEADER_ROWS) {
            let mut rfi_columns = Vec::new();
            let mut status_columns = Vec::new();
            for column in 1..=MAX_HEADER_COLUMNS {
                let Some(cell) = worksheet.cell(row, column) else {
                    continue;
                };
                let token = normalize_header(&cell.value);
                if token != "RFI NO." && token != "STATUS" {
                    continue;
                }
                let safe =
                    cell.formula.is_none() && !merge_budget.contains(worksheet, row, column)?;
                if token == "RFI NO." {
                    rfi_columns.push((column, safe));
                } else {
                    status_columns.push((column, safe));
                }
            }
            if rfi_columns.len() > 1 || status_columns.len() > 1 {
                duplicate_header_sheets.insert(descriptor.name.clone());
            }
            if rfi_columns.len() == 1
                && status_columns.len() == 1
                && rfi_columns[0].1
                && status_columns[0].1
            {
                if descriptor.visibility != WorksheetVisibility::Visible
                    || worksheet.has_sheet_protection
                {
                    return Err(protected());
                }
                candidates.push(FollowUpLayout {
                    worksheet: descriptor.clone(),
                    header_row: row,
                    rfi_column: rfi_columns[0].0,
                    status_column: status_columns[0].0,
                });
            }
        }
    }
    let mut sheet_candidate_counts = BTreeMap::<String, usize>::new();
    for candidate in &candidates {
        *sheet_candidate_counts
            .entry(candidate.worksheet.name.clone())
            .or_default() += 1;
    }
    for (sheet_name, count) in sheet_candidate_counts {
        if count > 1 {
            duplicate_header_sheets.insert(sheet_name);
        }
    }
    candidates.retain(|candidate| !duplicate_header_sheets.contains(&candidate.worksheet.name));
    if candidates.is_empty() {
        if !duplicate_header_sheets.is_empty() {
            return Err(BackendError::validation(
                "error.rfi.followUp.layoutAmbiguous",
            ));
        }
        return Err(BackendError::validation("error.rfi.followUp.layoutMissing"));
    }
    if candidates.len() == 1 {
        return Ok(candidates);
    }
    let preferred: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| is_preferred_follow_up_sheet_name(&candidate.worksheet.name))
        .collect();
    if !preferred.is_empty() {
        return Ok(preferred);
    }
    Err(BackendError::validation(
        "error.rfi.followUp.layoutAmbiguous",
    ))
}

#[allow(dead_code)]
fn discover_layout(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
    has_signature: bool,
    merge_budget: &mut MergeQueryBudget,
) -> Result<FollowUpLayout, BackendError> {
    let layouts = discover_layouts(discovery, workbook, has_signature, merge_budget)?;
    layouts.into_iter().next().ok_or_else(workbook_invalid)
}

fn scan_target_rows(
    worksheet: &WorksheetData,
    layout: &FollowUpLayout,
) -> Result<Vec<PendingTargetRow>, BackendError> {
    let last_allowed = layout.header_row + MAX_FOLLOW_UP_DATA_ROWS;
    if worksheet.max_row > last_allowed {
        return Err(limit_exceeded());
    }
    let mut rows = Vec::new();
    for row in (layout.header_row + 1)..=worksheet.max_row {
        let rfi_cell = worksheet.cell(row, layout.rfi_column);
        let status_cell = worksheet.cell(row, layout.status_column);
        let raw_rfi = rfi_cell
            .map(|cell| normalize_status(&cell.value))
            .unwrap_or_default();
        let (canonical_rfi, visible_rfi) = match canonical_follow_up_rfi(&raw_rfi) {
            Some((canonical, short)) => (Some(canonical), short),
            None => (None, raw_rfi),
        };
        let current_status = status_cell
            .map(|cell| normalize_status(&cell.value))
            .unwrap_or_default();
        if visible_rfi.is_empty() && current_status.is_empty() {
            continue;
        }
        if rows.len() >= MAX_PREVIEW_ROWS {
            return Err(limit_exceeded());
        }
        rows.push(PendingTargetRow {
            row,
            canonical_rfi,
            visible_rfi,
            current_status,
            rfi_cell: rfi_cell.map(Into::into),
            status_cell: status_cell.map(Into::into),
        });
    }
    Ok(rows)
}

fn classify_borrowed_row<'a>(
    pending: &'a PendingTargetRow,
    layout: &'a FollowUpLayout,
    target_worksheet: &WorksheetData,
    evidence: &'a [SourceStatusEvidence],
    analysis: &'a SourceAnalysis,
    duplicate_counts: &BTreeMap<String, usize>,
    merge_budget: &mut MergeQueryBudget,
) -> Result<BorrowedFollowUpRow<'a>, BackendError> {
    let duplicate = pending
        .canonical_rfi
        .as_ref()
        .and_then(|rfi| duplicate_counts.get(rfi))
        .is_some_and(|count| *count > 1);
    let rfi_formula = pending
        .rfi_cell
        .as_ref()
        .is_some_and(|cell| cell.formula.is_some());
    let rfi_merged = merge_budget.contains(target_worksheet, pending.row, layout.rfi_column)?;
    let status_formula = pending
        .status_cell
        .as_ref()
        .is_some_and(|cell| cell.formula.is_some());
    let status_merged =
        merge_budget.contains(target_worksheet, pending.row, layout.status_column)?;

    let (disposition, reason) = if rfi_formula {
        manual("targetRfiFormula")
    } else if rfi_merged {
        manual("targetRfiMerged")
    } else if pending.visible_rfi.is_empty() {
        manual("blankRfi")
    } else if pending.canonical_rfi.is_none() {
        manual("invalidRfi")
    } else if duplicate {
        manual("targetDuplicate")
    } else if evidence.is_empty() {
        manual("sourceMissing")
    } else if let Some(reason) = analysis.reason {
        manual(reason)
    } else if pending.status_cell.is_none() {
        manual("targetCellMissing")
    } else if status_formula {
        manual("targetFormula")
    } else if status_merged {
        manual("targetMerged")
    } else if !pending.current_status.is_empty()
        && pending.current_status != "Signed"
        && pending.current_status != "Not Signed"
    {
        manual("targetStatusUnknown")
    } else if analysis.proposed_status.as_deref() == Some(pending.current_status.as_str()) {
        (
            FollowUpDisposition::AlreadySynced,
            "warn.rfi.followUp.reason.alreadySynced".to_string(),
        )
    } else {
        let reason = if analysis.proposed_status.as_deref() == Some("Signed") {
            "warn.rfi.followUp.reason.readySigned"
        } else {
            "warn.rfi.followUp.reason.readyNotSigned"
        };
        (FollowUpDisposition::Ready, reason.to_string())
    };
    let selectable = disposition == FollowUpDisposition::Ready;
    let rfi_no = pending.visible_rfi.as_str();
    Ok(BorrowedFollowUpRow {
        row_id: format!(
            "rfi-follow-up:{}:{}:{}",
            layout.worksheet.sheet_id, pending.row, layout.status_column
        ),
        sheet: &layout.worksheet.name,
        row: pending.row,
        rfi_no,
        source_statuses: &analysis.statuses,
        current_status: &pending.current_status,
        proposed_status: analysis.proposed_status.as_deref(),
        disposition,
        reason_key: reason,
        selectable,
        enabled: selectable,
        target: BorrowedReviewTarget {
            worksheet: &layout.worksheet,
            row: pending.row,
            rfi_column: layout.rfi_column,
            status_column: layout.status_column,
            rfi_cell: pending.rfi_cell.as_ref(),
            status_cell: pending.status_cell.as_ref(),
        },
        source_evidence: evidence,
    })
}

impl BorrowedFollowUpRow<'_> {
    fn materialize(&self) -> FollowUpRow {
        FollowUpRow {
            row_id: self.row_id.clone(),
            sheet: self.sheet.to_string(),
            row: self.row,
            rfi_no: self.rfi_no.to_string(),
            source_statuses: self.source_statuses.to_vec(),
            current_status: self.current_status.to_string(),
            proposed_status: self.proposed_status.map(str::to_string),
            disposition: self.disposition.clone(),
            reason_key: self.reason_key.clone(),
            selectable: self.selectable,
            enabled: self.enabled,
            target: FollowUpReviewTarget {
                worksheet: self.target.worksheet.clone(),
                row: self.target.row,
                rfi_column: self.target.rfi_column,
                status_column: self.target.status_column,
                rfi_cell: self.target.rfi_cell.cloned(),
                status_cell: self.target.status_cell.cloned(),
            },
            source_evidence: self.source_evidence.to_vec(),
        }
    }
}

fn analyze_source(
    evidence: &[SourceStatusEvidence],
    workbook: &WorkbookData,
    merge_budget: &mut MergeQueryBudget,
) -> Result<SourceAnalysis, BackendError> {
    #[cfg(test)]
    SOURCE_ANALYSIS_CALLS.fetch_add(1, Ordering::SeqCst);

    let statuses = evidence
        .iter()
        .map(|evidence| {
            mapped_follow_up_status(
                evidence
                    .status_cell
                    .as_ref()
                    .map(|cell| cell.value.as_str())
                    .unwrap_or_default(),
            )
            .to_string()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if evidence.is_empty() {
        return Ok(SourceAnalysis {
            statuses,
            proposed_status: None,
            reason: None,
        });
    }
    if evidence.iter().any(|evidence| {
        evidence.rfi_cell.formula.is_some()
            || evidence
                .status_cell
                .as_ref()
                .is_some_and(|cell| cell.formula.is_some())
    }) {
        return Ok(SourceAnalysis {
            statuses,
            proposed_status: None,
            reason: Some("sourceFormula"),
        });
    }
    let mut source_merged = false;
    for evidence in evidence {
        let Some(worksheet) = workbook.sheet(&evidence.sheet) else {
            continue;
        };
        if merge_budget.contains(worksheet, evidence.row, evidence.rfi_column)?
            || merge_budget.contains(worksheet, evidence.row, evidence.status_column)?
        {
            source_merged = true;
            break;
        }
    }
    if source_merged {
        return Ok(SourceAnalysis {
            statuses,
            proposed_status: None,
            reason: Some("sourceMerged"),
        });
    }
    if statuses.len() == 1 {
        return Ok(SourceAnalysis {
            proposed_status: statuses.first().cloned(),
            statuses,
            reason: None,
        });
    }
    Ok(SourceAnalysis {
        statuses,
        proposed_status: None,
        reason: Some("sourceConflict"),
    })
}

fn editor_columns() -> Vec<Value> {
    vec![
        json!({
            "key": "enabled",
            "labelKey": "rfi.followUp.column.enabled",
            "kind": "bool",
            "bulk": false,
            "disabledWhen": {"field": "selectable", "equals": false},
        }),
        json!({"key": "rfiNo", "labelKey": "rfi.followUp.column.rfiNo", "kind": "readonly"}),
        json!({"key": "sourceStatuses", "labelKey": "rfi.followUp.column.sourceStatuses", "kind": "readonly"}),
        json!({"key": "currentStatus", "labelKey": "rfi.followUp.column.currentStatus", "kind": "readonly"}),
        json!({"key": "proposedStatus", "labelKey": "rfi.followUp.column.proposedStatus", "kind": "readonly"}),
        json!({
            "key": "disposition",
            "labelKey": "rfi.followUp.column.disposition",
            "kind": "readonly",
            "display": "localizedKey",
        }),
        json!({
            "key": "reasonKey",
            "labelKey": "rfi.followUp.column.reason",
            "kind": "readonly",
            "display": "localizedKey",
            "detail": true,
        }),
        json!({"key": "_sheet", "labelKey": "rfi.followUp.column.sheet", "kind": "readonly", "detail": true}),
        json!({"key": "_row", "labelKey": "rfi.followUp.column.row", "kind": "readonly", "detail": true}),
    ]
}

fn editor_metadata(rows: &[FollowUpRow], columns: Vec<Value>) -> Value {
    json!({
        "kind": "recordTable",
        "layout": "compact",
        "titleKey": "rfi.followUp.editor.title",
        "rowCountKey": "rfi.followUp.editor.rowCount",
        "requiredFields": ["enabled"],
        "groupField": "rfiNo",
        "selectionGroupFields": [],
        "columns": columns,
        "rows": rows,
        "duplicateEnabled": false,
        "selectionEnabled": false,
        "bulkEnabled": false,
        "reorderable": false,
        "detailsEnabled": true,
        "detailsDefaultVisible": false,
        "copyEnabled": false,
    })
}

fn borrowed_editor<'a>(
    columns: &'a [Value],
    rows: &'a [BorrowedFollowUpRow<'a>],
) -> BorrowedEditor<'a> {
    BorrowedEditor {
        kind: "recordTable",
        layout: "compact",
        title_key: "rfi.followUp.editor.title",
        row_count_key: "rfi.followUp.editor.rowCount",
        required_fields: ["enabled"],
        group_field: "rfiNo",
        selection_group_fields: [],
        columns,
        rows,
        duplicate_enabled: false,
        selection_enabled: false,
        bulk_enabled: false,
        reorderable: false,
        details_enabled: true,
        details_default_visible: false,
        copy_enabled: false,
    }
}

fn validate_projection_budget<T: Serialize>(projection: &T) -> Result<(), BackendError> {
    let mut writer = CountingLimitWriter {
        count: 0,
        limit: MAX_PREVIEW_AUTHORITY_BYTES,
    };
    serde_json::to_writer(&mut writer, projection).map_err(|_| limit_exceeded())
}

pub(crate) fn decode_step_meta(step: &Step) -> Result<FollowUpStepMeta, BackendError> {
    if step.kind != StepKind::WorkbookUpdate {
        return Err(workbook_invalid());
    }
    let meta: FollowUpStepMeta =
        serde_json::from_value(Value::Object(step.meta.clone())).map_err(|_| workbook_invalid())?;
    if meta.updates.is_empty() || meta.updates.len() > MAX_PREVIEW_ROWS {
        return Err(workbook_invalid());
    }
    let mut row_ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for update in &meta.updates {
        let valid_layout = meta.all_layouts().any(|layout| {
            layout.worksheet == update.target.worksheet
                && layout.rfi_column == update.target.rfi_column
                && layout.status_column == update.target.status_column
        });
        if !valid_layout
            || !row_ids.insert(update.row_id.as_str())
            || !targets.insert((
                update.target.worksheet.part_path.as_str(),
                update.target.row,
                update.target.status_column,
            ))
            || canonical_rfi(&update.canonical_rfi).as_deref()
                != Some(update.canonical_rfi.as_str())
            || !matches!(update.proposed_status.as_str(), "Signed" | "Not Signed")
            || canonical_follow_up_rfi(&update.target.rfi_cell.value)
                .map(|(canonical, _)| canonical)
                .as_deref()
                != Some(update.canonical_rfi.as_str())
            || update.target.rfi_cell.formula.is_some()
            || update.target.status_cell.formula.is_some()
        {
            return Err(workbook_invalid());
        }
    }
    Ok(meta)
}

pub type FollowUpGuardedUpdates = BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>;

pub fn guarded_updates(step: &Step) -> Result<FollowUpGuardedUpdates, BackendError> {
    let meta = decode_step_meta(step)?;
    let mut updates_by_sheet = BTreeMap::<String, BTreeMap<(u32, u32), GuardedCellUpdate>>::new();
    for update in meta.updates {
        let coordinate = (update.target.row, update.target.status_column);
        let guarded = GuardedCellUpdate {
            expected: CellData {
                value: update.target.status_cell.value,
                formula: update.target.status_cell.formula,
                style: update.target.status_cell.style,
            },
            replacement: CellUpdate::Text(update.proposed_status),
        };
        if updates_by_sheet
            .entry(update.target.worksheet.name)
            .or_default()
            .insert(coordinate, guarded)
            .is_some()
        {
            return Err(workbook_invalid());
        }
    }
    Ok(updates_by_sheet)
}

#[allow(dead_code)]
pub(crate) fn verify_patched_output<R1, R2>(
    step: &Step,
    source: &mut R1,
    private_output: &mut R2,
) -> Result<(), BackendError>
where
    R1: Read + Seek,
    R2: Read + Seek,
{
    let updates = guarded_updates(step)?;
    verify_guarded_workbook_delta(source, private_output, &updates).map_err(|error| {
        match error.kind() {
            OoxmlErrorKind::LimitExceeded => limit_exceeded(),
            OoxmlErrorKind::Invalid => {
                BackendError::validation("error.rfi.followUp.patchVerification")
            }
        }
    })
}

#[allow(dead_code)]
pub(crate) fn changed_cell_count(step: &Step) -> Result<u64, BackendError> {
    u64::try_from(decode_step_meta(step)?.updates.len()).map_err(|_| limit_exceeded())
}

pub(crate) fn step_workbook_paths(step: &Step) -> Result<(String, String, String), BackendError> {
    let meta = decode_step_meta(step)?;
    let target = step.src.clone().ok_or_else(workbook_invalid)?;
    let backup = step.dst.clone().ok_or_else(workbook_invalid)?;
    Ok((meta.rfi_log_workbook, target, backup))
}

pub(crate) fn revalidate_open_workbooks(
    step: &Step,
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<(), BackendError> {
    let meta = decode_step_meta(step)?;
    let source = fingerprint_open_file(rfi_log)?;
    let target = fingerprint_open_file(follow_up)?;
    if source != meta.rfi_log_fingerprint || target != meta.follow_up_fingerprint {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookChanged",
        ));
    }
    Ok(())
}

pub(crate) fn write_and_verify_private_output<R>(
    step: &Step,
    source: &mut File,
    destination: &mut R,
) -> Result<(), BackendError>
where
    R: Read + Write + Seek,
{
    let updates = guarded_updates(step)?;
    patch_workbook_sheets_guarded(source, destination, &updates).map_err(map_apply_ooxml_error)?;
    verify_patched_output(step, source, destination)
}

fn map_apply_ooxml_error(error: OoxmlError) -> BackendError {
    match error.kind() {
        OoxmlErrorKind::LimitExceeded => {
            BackendError::validation(format!("error.rfi.followUp.limitExceeded: {error}"))
        }
        OoxmlErrorKind::Invalid => {
            BackendError::validation(format!("error.rfi.followUp.workbookInvalid: {error}"))
        }
    }
}

fn validate_cell_text(worksheet: &WorksheetData) -> Result<(), BackendError> {
    if worksheet.cells.values().any(|cell| {
        cell.value.chars().count() > MAX_CELL_TEXT_CHARS
            || cell
                .formula
                .as_ref()
                .is_some_and(|formula| formula.chars().count() > MAX_CELL_TEXT_CHARS)
    }) {
        return Err(limit_exceeded());
    }
    Ok(())
}

fn normalize_status(value: &str) -> String {
    value.nfc().collect::<String>().trim().to_string()
}

fn normalize_header(value: &str) -> String {
    normalize_status(value).to_ascii_uppercase()
}

fn is_follow_up_rfi_header(token: &str) -> bool {
    token == "RFI NO."
        || token == "RFI NO"
        || token == "RFI #"
        || token == "RFI NUMBER"
        || token == "RFI NO:"
        || token == "RFI REF"
        || token == "RFI REFERENCE"
}

fn is_follow_up_worley_status_header(token: &str) -> bool {
    token == "WORLEY STATUS"
        || token == "WORLEY SIGNATURE"
        || token == "WORLEY REMARKS"
        || token == "WORLEY REMARK"
        || token == "STATUS (WORLEY)"
        || token == "SIGNATURE (WORLEY)"
}

fn is_follow_up_generic_status_header(token: &str) -> bool {
    token == "STATUS"
        || token == "RFI STATUS"
        || token == "CURRENT STATUS"
        || token == "SIGNATURE"
        || token == "SIGNATURE STATUS"
        || token == "DURUM"
        || token == "REMARKS"
        || token == "REMARK"
}

fn manual(reason: &str) -> (FollowUpDisposition, String) {
    (
        FollowUpDisposition::ManualReview,
        format!("warn.rfi.followUp.reason.{reason}"),
    )
}

fn workbook_invalid() -> BackendError {
    BackendError::validation("error.rfi.followUp.workbookInvalid")
}

fn protected() -> BackendError {
    BackendError::validation("error.rfi.followUp.protected")
}

fn signed_package() -> BackendError {
    BackendError::validation("error.rfi.followUp.signedPackage")
}

#[allow(dead_code)]
pub(crate) fn map_follow_up_ooxml_error(error: OoxmlError) -> BackendError {
    match error.kind() {
        OoxmlErrorKind::LimitExceeded => {
            BackendError::validation(format!("error.rfi.followUp.limitExceeded: {error}"))
        }
        OoxmlErrorKind::Invalid => {
            BackendError::validation(format!("error.rfi.followUp.workbookInvalid: {error}"))
        }
    }
}

fn limit_exceeded() -> BackendError {
    BackendError::validation("error.rfi.followUp.limitExceeded")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> WorksheetDescriptor {
        WorksheetDescriptor {
            workbook_index: 1,
            sheet_id: 7,
            relationship_id: "rId7".to_string(),
            relationship_type: "worksheet".to_string(),
            part_path: "xl/worksheets/sheet7.xml".to_string(),
            name: "Follow Up".to_string(),
            visibility: WorksheetVisibility::Visible,
        }
    }

    fn fingerprint() -> WorkbookFingerprint {
        WorkbookFingerprint {
            identity: FilesystemObjectIdentity::Unix {
                device: 1,
                inode: 2,
            },
            size: 3,
            mtime_ns: 4,
            sha256: "00".repeat(32),
        }
    }

    fn approved_update() -> FollowUpApprovedUpdate {
        let canonical_rfi = "AZE-SUN-INT-QMS-RFI-CIV-0001".to_string();
        FollowUpApprovedUpdate {
            row_id: "rfi-follow-up:7:2:2".to_string(),
            canonical_rfi: canonical_rfi.clone(),
            proposed_status: "Signed".to_string(),
            target: FollowUpCellTarget {
                worksheet: descriptor(),
                row: 2,
                rfi_column: 1,
                status_column: 2,
                rfi_cell: CellSnapshot {
                    value: "CIV-0001".to_string(),
                    ..CellSnapshot::default()
                },
                status_cell: CellSnapshot::default(),
            },
            source_evidence: Vec::new(),
        }
    }

    fn step_meta(updates: Vec<FollowUpApprovedUpdate>) -> FollowUpStepMeta {
        FollowUpStepMeta {
            rfi_log_workbook: "/source.xlsx".to_string(),
            rfi_log_fingerprint: fingerprint(),
            follow_up_fingerprint: fingerprint(),
            layout: FollowUpLayout {
                worksheet: descriptor(),
                header_row: 1,
                rfi_column: 1,
                status_column: 2,
            },
            layouts: Vec::new(),
            updates,
        }
    }

    #[test]
    fn projection_budget_exact_boundary_and_one_byte_over() {
        let mut writer = CountingLimitWriter {
            count: 0,
            limit: MAX_PREVIEW_AUTHORITY_BYTES,
        };
        let chunk = [0_u8; 4096];
        for _ in 0..(MAX_PREVIEW_AUTHORITY_BYTES / chunk.len() as u64) {
            writer.write_all(&chunk).unwrap();
        }
        assert_eq!(writer.count, MAX_PREVIEW_AUTHORITY_BYTES);
        assert!(writer.write_all(&[0]).is_err());
        assert_eq!(writer.count, MAX_PREVIEW_AUTHORITY_BYTES);
    }

    #[test]
    fn follow_up_authority_rejects_unknown_fields_and_duplicate_updates() {
        let valid_meta = serde_json::to_value(step_meta(vec![approved_update()]))
            .unwrap()
            .as_object()
            .cloned()
            .unwrap();
        let valid = Step {
            kind: StepKind::WorkbookUpdate,
            src: Some("/target.xlsx".to_string()),
            dst: Some("/backup.xlsx".to_string()),
            note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
            meta: valid_meta,
        };
        assert!(decode_step_meta(&valid).is_ok());

        let mut unknown = valid.clone();
        unknown
            .meta
            .insert("unknown".to_string(), Value::Bool(true));
        assert!(decode_step_meta(&unknown).is_err());

        let update = approved_update();
        let mut duplicate = valid;
        duplicate.meta = serde_json::to_value(step_meta(vec![update.clone(), update]))
            .unwrap()
            .as_object()
            .cloned()
            .unwrap();
        assert!(decode_step_meta(&duplicate).is_err());
    }

    #[test]
    fn plan_from_classification_emits_one_exact_workbook_step_for_enabled_rows() {
        let layout = FollowUpLayout {
            worksheet: descriptor(),
            header_row: 1,
            rfi_column: 1,
            status_column: 2,
        };
        let update = approved_update();
        let row = FollowUpRow {
            row_id: update.row_id.clone(),
            sheet: "Follow Up".to_string(),
            row: 2,
            rfi_no: "CIV-0001".to_string(),
            source_statuses: vec!["Signed".to_string()],
            current_status: String::new(),
            proposed_status: Some(update.proposed_status.clone()),
            disposition: FollowUpDisposition::Ready,
            reason_key: "warn.rfi.followUp.reason.readySigned".to_string(),
            selectable: true,
            enabled: true,
            target: FollowUpReviewTarget {
                worksheet: descriptor(),
                row: 2,
                rfi_column: 1,
                status_column: 2,
                rfi_cell: Some(update.target.rfi_cell.clone()),
                status_cell: Some(update.target.status_cell.clone()),
            },
            source_evidence: Vec::new(),
        };
        let columns = editor_columns();
        let classification = json!({
            "layout": layout,
            "rows": [row],
            "warnings": [],
            "editor": editor_metadata(&[row], columns),
        });

        let plan = plan_from_classification(
            "/source.xlsx",
            "/follow-up.xlsx",
            "/.docflow-s5-backup-test.xlsx",
            fingerprint(),
            fingerprint(),
            classification,
        )
        .unwrap();

        assert_eq!(plan.op_id, OPERATION_ID);
        assert!(plan.plan_id.is_empty());
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(
            plan.steps[0].kind,
            crate::contracts::StepKind::WorkbookUpdate
        );
        assert_eq!(plan.steps[0].src.as_deref(), Some("/follow-up.xlsx"));
        assert_eq!(
            plan.steps[0].dst.as_deref(),
            Some("/.docflow-s5-backup-test.xlsx")
        );
        assert_eq!(
            plan.steps[0].note_key.as_deref(),
            Some("note.rfi.followUp.workbookUpdate")
        );
        assert_eq!(
            decode_step_meta(&plan.steps[0]).unwrap().updates,
            vec![update]
        );
        assert!(plan.meta.contains_key("editor"));
        assert!(plan.meta.contains_key("rfiFollowUp"));
    }

    #[test]
    fn source_group_analysis_runs_once_for_500_duplicate_targets() {
        SOURCE_ANALYSIS_CALLS.store(0, Ordering::SeqCst);
        let make_descriptor = |name: &str, id: u32| WorksheetDescriptor {
            workbook_index: id,
            sheet_id: id,
            relationship_id: format!("rId{id}"),
            relationship_type: "worksheet".to_string(),
            part_path: format!("xl/worksheets/sheet{id}.xml"),
            name: name.to_string(),
            visibility: WorksheetVisibility::Visible,
        };
        let source_columns = status_columns();
        let source_names = source_columns.map(|columns| columns.sheet);
        let civ_status_column = source_columns[0].status_column;
        let source_discovery = OrderedWorkbookDiscovery {
            worksheets: source_names
                .iter()
                .enumerate()
                .map(|(index, name)| make_descriptor(name, index as u32 + 1))
                .collect(),
            safety: Default::default(),
        };
        let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
        let mut source_workbook = WorkbookData::default();
        for name in source_names {
            source_workbook
                .sheets
                .insert(name.to_string(), WorksheetData::default());
        }
        let civ = source_workbook.sheets.get_mut("CIV").unwrap();
        civ.max_row = 10;
        civ.max_column = civ_status_column;
        civ.cells.insert(
            (10, 1),
            CellData {
                value: rfi.to_string(),
                ..Default::default()
            },
        );
        civ.cells.insert(
            (10, civ_status_column),
            CellData {
                value: "Signed by Worley".to_string(),
                ..Default::default()
            },
        );

        let target_discovery = OrderedWorkbookDiscovery {
            worksheets: vec![make_descriptor("Follow Up", 1)],
            safety: Default::default(),
        };
        let mut target = WorksheetData {
            max_row: 501,
            max_column: 2,
            ..Default::default()
        };
        target.cells.insert(
            (1, 1),
            CellData {
                value: "RFI NO.".to_string(),
                ..Default::default()
            },
        );
        target.cells.insert(
            (1, 2),
            CellData {
                value: "STATUS".to_string(),
                ..Default::default()
            },
        );
        for row in 2..=501 {
            target.cells.insert(
                (row, 1),
                CellData {
                    value: rfi.to_string(),
                    ..Default::default()
                },
            );
            target.cells.insert((row, 2), CellData::default());
        }
        let mut target_workbook = WorkbookData::default();
        target_workbook
            .sheets
            .insert("Follow Up".to_string(), target);

        classify_workbooks(
            &source_discovery,
            &source_workbook,
            &target_discovery,
            &target_workbook,
            false,
        )
        .unwrap();
        assert_eq!(SOURCE_ANALYSIS_CALLS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn merge_query_budget_fails_at_the_first_comparison_over_limit() {
        let worksheet = WorksheetData {
            merged_ranges: vec![crate::ooxml::CellRange {
                first_row: 1,
                first_column: 1,
                last_row: 1,
                last_column: 1,
            }],
            ..Default::default()
        };
        let mut budget = MergeQueryBudget { remaining: 1 };
        assert!(budget.contains(&worksheet, 1, 1).unwrap());
        assert_eq!(
            budget.contains(&worksheet, 1, 1).unwrap_err().message,
            "error.rfi.followUp.limitExceeded"
        );
    }
}
