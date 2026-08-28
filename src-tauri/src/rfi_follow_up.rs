//! Pure RFI Follow Up matching, layout, and classification primitives.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    sync::OnceLock,
};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

use crate::{
    contracts::Warning,
    errors::BackendError,
    journal::FilesystemObjectIdentity,
    ooxml::{
        CellData, OoxmlError, OoxmlErrorKind, OrderedWorkbookDiscovery, WorkbookData,
        WorksheetData, WorksheetDescriptor, WorksheetVisibility,
    },
    rfi_log::status_columns,
};

#[allow(dead_code)]
pub(crate) const OPERATION_ID: &str = "rfi.follow_up_sync";
const SOURCE_FIRST_DATA_ROW: u32 = 10;
const MAX_SOURCE_DATA_ROWS_PER_SHEET: u32 = 10_000;
const MAX_HEADER_ROWS: u32 = 32;
const MAX_HEADER_COLUMNS: u32 = 64;
const MAX_FOLLOW_UP_DATA_ROWS: u32 = 10_000;
const MAX_PREVIEW_ROWS: usize = 500;
const MAX_SOURCE_EVIDENCE_PER_RFI: usize = 500;
const MAX_PREVIEW_AUTHORITY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FOLLOW_UP_MERGE_COMPARISONS: u64 = 2_000_000;
const MAX_CELL_TEXT_CHARS: usize = 20_000;

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
struct FollowUpLayout {
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
struct WorkbookFingerprint {
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
    updates: Vec<FollowUpApprovedUpdate>,
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
    rows: &'a [BorrowedFollowUpRow<'a>],
    warnings: &'a [Warning],
    editor: BorrowedEditor<'a>,
    approved_rows: Vec<&'a BorrowedFollowUpRow<'a>>,
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

pub fn mapped_follow_up_status(value: &str) -> Option<&'static str> {
    match value {
        "Completed" => Some("Completed"),
        "Ongoing" => Some("In Progress"),
        _ => None,
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
    let layout = discover_layout(
        target_discovery,
        target_workbook,
        target_has_signature,
        &mut merge_budget,
    )?;
    let worksheet = target_workbook
        .sheet(&layout.worksheet.name)
        .ok_or_else(workbook_invalid)?;
    let pending = scan_target_rows(worksheet, &layout)?;
    let duplicate_counts = pending
        .iter()
        .filter_map(|row| row.canonical_rfi.as_ref())
        .fold(BTreeMap::<String, usize>::new(), |mut counts, rfi| {
            *counts.entry(rfi.clone()).or_default() += 1;
            counts
        });
    let borrowed_rows = pending
        .iter()
        .map(|pending| {
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
                &layout,
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
    {
        let projection = BorrowedAuthorityProjection {
            layout: &layout,
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
        "layout": layout,
        "rows": rows,
        "warnings": warnings,
        "editor": editor,
    }))
}

fn scan_source(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
) -> Result<BTreeMap<String, Vec<SourceStatusEvidence>>, BackendError> {
    if discovery.safety.has_workbook_protection {
        return Err(protected());
    }
    let mut grouped = BTreeMap::<String, Vec<SourceStatusEvidence>>::new();
    for projection in status_columns() {
        let descriptor = discovery
            .worksheets
            .iter()
            .find(|worksheet| worksheet.name == projection.sheet)
            .ok_or_else(workbook_invalid)?;
        let worksheet = workbook
            .sheet(projection.sheet)
            .ok_or_else(workbook_invalid)?;
        if worksheet.has_sheet_protection {
            return Err(protected());
        }
        validate_cell_text(worksheet)?;
        let last_allowed = SOURCE_FIRST_DATA_ROW + MAX_SOURCE_DATA_ROWS_PER_SHEET - 1;
        if worksheet.max_row > last_allowed {
            return Err(limit_exceeded());
        }
        for row in SOURCE_FIRST_DATA_ROW..=worksheet.max_row {
            let Some(rfi_cell) = worksheet.cell(row, projection.rfi_column) else {
                continue;
            };
            let Some(rfi) = canonical_rfi(&rfi_cell.value) else {
                continue;
            };
            let status_cell = worksheet.cell(row, projection.status_column);
            let evidence = grouped.entry(rfi).or_default();
            if evidence.len() >= MAX_SOURCE_EVIDENCE_PER_RFI {
                return Err(limit_exceeded());
            }
            evidence.push(SourceStatusEvidence {
                sheet: descriptor.name.clone(),
                row,
                rfi_column: projection.rfi_column,
                status_column: projection.status_column,
                rfi_cell: rfi_cell.into(),
                status_cell: status_cell.map(Into::into),
            });
        }
    }
    Ok(grouped)
}

fn discover_layout(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
    has_signature: bool,
    merge_budget: &mut MergeQueryBudget,
) -> Result<FollowUpLayout, BackendError> {
    if has_signature {
        return Err(signed_package());
    }
    if discovery.safety.has_workbook_protection {
        return Err(protected());
    }
    let mut candidates = Vec::new();
    let mut duplicate_header = false;
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
                duplicate_header = true;
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
    if duplicate_header || candidates.len() > 1 {
        return Err(BackendError::validation(
            "error.rfi.followUp.layoutAmbiguous",
        ));
    }
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| BackendError::validation("error.rfi.followUp.layoutMissing"))
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
        let visible_rfi = rfi_cell
            .map(|cell| normalize_status(&cell.value))
            .unwrap_or_default();
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
            canonical_rfi: canonical_rfi(&visible_rfi),
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
        && pending.current_status != "Completed"
        && pending.current_status != "In Progress"
    {
        manual("targetStatusUnknown")
    } else if analysis.proposed_status.as_deref() == Some(pending.current_status.as_str()) {
        (
            FollowUpDisposition::AlreadySynced,
            "warn.rfi.followUp.reason.alreadySynced".to_string(),
        )
    } else {
        let reason = if analysis.proposed_status.as_deref() == Some("Completed") {
            "warn.rfi.followUp.reason.readyCompleted"
        } else {
            "warn.rfi.followUp.reason.readyOngoing"
        };
        (FollowUpDisposition::Ready, reason.to_string())
    };
    let selectable = disposition == FollowUpDisposition::Ready;
    let rfi_no = pending
        .canonical_rfi
        .as_deref()
        .unwrap_or(&pending.visible_rfi);
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
            evidence
                .status_cell
                .as_ref()
                .map(|cell| normalize_status(&cell.value))
                .unwrap_or_default()
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
    if statuses.iter().all(String::is_empty) {
        return Ok(SourceAnalysis {
            statuses,
            proposed_status: None,
            reason: Some("sourceStatusBlank"),
        });
    }
    if statuses.len() == 1 {
        if let Some(mapped) = mapped_follow_up_status(&statuses[0]) {
            return Ok(SourceAnalysis {
                statuses,
                proposed_status: Some(mapped.to_string()),
                reason: None,
            });
        }
        return Ok(SourceAnalysis {
            statuses,
            proposed_status: None,
            reason: Some("sourceStatusUnknown"),
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

#[allow(dead_code)]
pub(crate) fn decode_step_meta(value: Value) -> Result<FollowUpStepMeta, BackendError> {
    let meta: FollowUpStepMeta = serde_json::from_value(value).map_err(|_| workbook_invalid())?;
    let mut row_ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for update in &meta.updates {
        if !row_ids.insert(update.row_id.as_str())
            || !targets.insert((
                update.target.worksheet.sheet_id,
                update.target.row,
                update.target.status_column,
            ))
            || canonical_rfi(&update.canonical_rfi).as_deref()
                != Some(update.canonical_rfi.as_str())
            || !matches!(update.proposed_status.as_str(), "Completed" | "In Progress")
        {
            return Err(workbook_invalid());
        }
    }
    Ok(meta)
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
        OoxmlErrorKind::LimitExceeded => limit_exceeded(),
        OoxmlErrorKind::Invalid => workbook_invalid(),
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
        FollowUpApprovedUpdate {
            row_id: "rfi-follow-up:7:2:2".to_string(),
            canonical_rfi: "AZE-SUN-INT-QMS-RFI-CIV-0001".to_string(),
            proposed_status: "Completed".to_string(),
            target: FollowUpCellTarget {
                worksheet: descriptor(),
                row: 2,
                rfi_column: 1,
                status_column: 2,
                rfi_cell: CellSnapshot::default(),
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
        let valid = serde_json::to_value(step_meta(vec![approved_update()])).unwrap();
        assert!(decode_step_meta(valid.clone()).is_ok());

        let mut unknown = valid;
        unknown
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_string(), Value::Bool(true));
        assert!(decode_step_meta(unknown).is_err());

        let update = approved_update();
        let duplicate = serde_json::to_value(step_meta(vec![update.clone(), update])).unwrap();
        assert!(decode_step_meta(duplicate).is_err());
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
                value: "Completed".to_string(),
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
        let target_workbook = WorkbookData {
            sheets: BTreeMap::from([("Follow Up".to_string(), target)]),
        };

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
