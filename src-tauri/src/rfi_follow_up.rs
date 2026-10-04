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
        OrderedWorkbookDiscovery, WorkbookData, WorksheetData, WorksheetDescriptor,
        WorksheetVisibility, package_has_signature, patch_workbook_sheets_guarded_text,
        read_workbook_and_discovery, verify_guarded_workbook_delta_text,
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
    #[serde(default)]
    proposed_rfi: String,
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
    #[serde(default)]
    proposed_rfi_no: Option<String>,
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
    static SHORT_RFI: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A(CIV|MEC|ELE|PRC)[-_]?([0-9]{4})\z")
            .expect("short Follow Up RFI regex is valid")
    });
    let normalized = normalize_status(value).to_uppercase();
    let short = if let Some(short) = normalized.strip_prefix(FULL_RFI_PREFIX) {
        short.to_string()
    } else {
        normalized
    };
    let captures = SHORT_RFI.captures(&short)?;
    let disc = &captures[1];
    let num = captures[2].parse::<u32>().ok()?;
    let canonical_short = format!("{disc}-{num:04}");
    Some((
        format!("{FULL_RFI_PREFIX}{canonical_short}"),
        canonical_short,
    ))
}

pub fn is_placeholder_rfi(code: &str) -> bool {
    let digits = code.rsplit_once('-').map(|(_, num)| num).unwrap_or(code);
    digits.parse::<u32>().ok().is_some_and(|n| n >= 2000)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogState {
    Pending,
    InProgress,
    Completed,
    Remove,
}

#[derive(Clone, Debug)]
struct SourceRecord {
    canonical_rfi: String,
    short_rfi: String,
    itp: String,
    item: String,
    state: Option<LogState>,
    excluded_civ_pcs: bool,
    worley_signed: bool,
    contexts: Vec<SourceRouteContext>,
    evidence: Vec<SourceStatusEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceRouteContext {
    tag: String,
    area: String,
    location: String,
}

fn log_state(value: &str) -> Option<LogState> {
    match normalize_header(value).as_str() {
        "" => Some(LogState::Pending),
        "ONGOING" | "IN PROGRESS" => Some(LogState::InProgress),
        "COMPLETED" => Some(LogState::Completed),
        "CANCELLED" | "CANCELED" | "REJECTED" => Some(LogState::Remove),
        _ => None,
    }
}

fn aggregate_status(states: impl IntoIterator<Item = LogState>) -> Option<String> {
    let states = states
        .into_iter()
        .filter(|state| *state != LogState::Remove)
        .collect::<Vec<_>>();
    if states.is_empty() {
        return Some(String::new());
    }
    if states.iter().all(|state| *state == LogState::Pending) {
        Some("Pending".to_string())
    } else if states.iter().all(|state| *state == LogState::Completed) {
        Some("Completed".to_string())
    } else {
        Some("In Progress".to_string())
    }
}

fn normalize_code_text(value: &str) -> String {
    value
        .nfc()
        .collect::<String>()
        .replace(['˗', '‐', '‑', '‒', '–', '—', '−'], "-")
        .trim()
        .to_uppercase()
}

fn itp_identity(value: &str) -> Option<(String, String)> {
    static ITP: OnceLock<Regex> = OnceLock::new();
    let normalized = normalize_code_text(value);
    let captures = ITP
        .get_or_init(|| Regex::new(r"ITP-(CIV|MEC)-([0-9]{4})(?:_[A-Z])?\z").unwrap())
        .captures(&normalized)?;
    Some((captures[1].to_string(), captures[2].to_string()))
}

fn item_clause(value: &str) -> Option<String> {
    static ITEM: OnceLock<Regex> = OnceLock::new();
    let normalized = normalize_code_text(value);
    let captures = ITEM
        .get_or_init(|| Regex::new(r"(?:CIV|MEC)-[0-9]+\.([0-9]+\.[0-9]+)\z").unwrap())
        .captures(&normalized)?;
    Some(captures[1].to_string())
}

fn split_values(value: &str) -> Vec<String> {
    value
        .split([',', '/', ';', '\n'])
        .map(normalize_status)
        .filter(|part| !part.is_empty())
        .collect()
}

fn parse_rfi_group(value: &str) -> Option<Vec<String>> {
    static PART: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A(?:(CIV|MEC|ELE|PRC)[-_]?)?([0-9]{1,4})(?:-([0-9]{1,4}))?\z").unwrap()
    });
    let mut normalized = normalize_status(value).to_uppercase().replace(',', "/");
    if let Some(short) = normalized.strip_prefix(FULL_RFI_PREFIX) {
        normalized = short.to_string();
    }
    let mut discipline: Option<String> = None;
    let mut numbers = Vec::new();
    for raw_part in normalized.split('/') {
        let raw_part = raw_part.trim();
        if raw_part.is_empty() {
            return None;
        }
        let captures = PART.captures(raw_part)?;
        if let Some(found) = captures.get(1) {
            match discipline.as_deref() {
                Some(existing) if existing != found.as_str() => return None,
                None => discipline = Some(found.as_str().to_string()),
                _ => {}
            }
        }
        let start = captures[2].parse::<u32>().ok()?;
        let end = captures
            .get(3)
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .unwrap_or(start);
        if end < start || end - start > 500 {
            return None;
        }
        numbers.extend(start..=end);
    }
    let discipline = discipline?;
    let mut seen = BTreeSet::new();
    let result = numbers
        .into_iter()
        .filter(|number| seen.insert(*number))
        .map(|number| format!("{FULL_RFI_PREFIX}{discipline}-{number:04}"))
        .collect::<Vec<_>>();
    (!result.is_empty()).then_some(result)
}

fn format_rfi_group(codes: &[String]) -> Option<String> {
    let mut discipline: Option<String> = None;
    let mut numbers = Vec::new();
    for code in codes {
        let (_, short) = canonical_follow_up_rfi(code)?;
        let (found_discipline, number) = short.rsplit_once('-')?;
        match discipline {
            Some(ref existing) if existing != found_discipline => return None,
            None => discipline = Some(found_discipline.to_string()),
            _ => {}
        }
        numbers.push(number.to_string());
    }
    let discipline = discipline?;
    Some(format!("{discipline}-{}", numbers.join("/")))
}

fn find_header_column(sheet: &WorksheetData, names: &[&str]) -> Option<u32> {
    for row in 1..=sheet.max_row.min(16) {
        for column in 1..=sheet.max_column.min(MAX_HEADER_COLUMNS) {
            let token = sheet
                .cell(row, column)
                .map(|cell| normalize_header(&cell.value));
            if token.as_deref().is_some_and(|token| names.contains(&token)) {
                return Some(column);
            }
        }
    }
    None
}

fn scan_source_records(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
) -> Result<BTreeMap<String, SourceRecord>, BackendError> {
    if discovery.safety.has_workbook_protection {
        return Err(protected());
    }
    let mut records = BTreeMap::<String, SourceRecord>::new();
    for discipline in ["CIV", "MEC"] {
        let descriptor = discovery
            .worksheets
            .iter()
            .find(|sheet| sheet.name.trim().eq_ignore_ascii_case(discipline))
            .ok_or_else(workbook_invalid)?;
        let sheet = workbook
            .sheet(&descriptor.name)
            .ok_or_else(workbook_invalid)?;
        if sheet.has_sheet_protection
            || sheet.max_row > SOURCE_FIRST_DATA_ROW + MAX_SOURCE_DATA_ROWS_PER_SHEET
        {
            return Err(protected());
        }
        validate_cell_text(sheet)?;
        let rfi_col =
            find_header_column(sheet, &["RFI NO.", "RFI NO"]).ok_or_else(workbook_invalid)?;
        let itp_col = find_header_column(sheet, &["ITP NO:", "ITP NO.", "ITP NO"])
            .ok_or_else(workbook_invalid)?;
        let item_col =
            find_header_column(sheet, &["ITEM NO.", "ITEM NO"]).ok_or_else(workbook_invalid)?;
        let tag_col = find_header_column(sheet, &["TAG NO", "TAG NO."]).unwrap_or(6);
        let area_col = find_header_column(sheet, &["AREA"]).unwrap_or(7);
        let location_col =
            find_header_column(sheet, &["INSPECTION LOCATION", "INSP. LOC./AREA"]).unwrap_or(14);
        let status_col =
            find_header_column(sheet, &["INSPECTION STATUS"]).ok_or_else(workbook_invalid)?;
        let remarks_col = find_header_column(sheet, &["REMARKS"]).ok_or_else(workbook_invalid)?;
        for row in SOURCE_FIRST_DATA_ROW..=sheet.max_row {
            let Some(rfi_cell) = sheet.cell(row, rfi_col) else {
                continue;
            };
            let Some(canonical_rfi) = canonical_rfi(&rfi_cell.value) else {
                continue;
            };
            if !canonical_rfi.contains(&format!("-RFI-{discipline}-")) {
                continue;
            }
            let short_rfi = canonical_rfi
                .strip_prefix(FULL_RFI_PREFIX)
                .unwrap()
                .to_string();
            let get = |column| {
                sheet
                    .cell(row, column)
                    .map(|cell| normalize_status(&cell.value))
                    .unwrap_or_default()
            };
            let status = get(status_col);
            let remarks = get(remarks_col);
            let itp = get(itp_col);
            let item = get(item_col);
            let tag = get(tag_col);
            let area = get(area_col);
            let location = get(location_col);
            let context = SourceRouteContext {
                tag: tag.clone(),
                area: area.clone(),
                location: location.clone(),
            };
            let evidence = SourceStatusEvidence {
                sheet: descriptor.name.clone(),
                row,
                rfi_column: rfi_col,
                status_column: status_col,
                rfi_cell: rfi_cell.into(),
                status_cell: sheet.cell(row, status_col).map(Into::into),
            };
            let excluded_civ_pcs = discipline == "CIV"
                && split_values(&context.tag)
                    .iter()
                    .any(|tag| normalize_header(tag).starts_with("PCS-"));
            let incoming = SourceRecord {
                canonical_rfi: canonical_rfi.clone(),
                short_rfi,
                itp,
                item,
                state: log_state(&status),
                excluded_civ_pcs,
                worley_signed: mapped_follow_up_status(&remarks) == "Signed",
                contexts: vec![context],
                evidence: vec![evidence],
            };
            if let Some(existing) = records.get_mut(&canonical_rfi) {
                if existing.excluded_civ_pcs || incoming.excluded_civ_pcs {
                    existing.excluded_civ_pcs = true;
                } else if existing.itp != incoming.itp
                    || existing.item != incoming.item
                    || existing.state != incoming.state
                    || existing.worley_signed != incoming.worley_signed
                    || route_key(existing) != route_key(&incoming)
                {
                    existing.state = None;
                } else {
                    for context in incoming.contexts.iter() {
                        if !existing.contexts.contains(context) {
                            existing.contexts.push(context.clone());
                        }
                    }
                }
                existing.evidence.extend(incoming.evidence);
            } else {
                records.insert(canonical_rfi, incoming);
            }
        }
    }
    Ok(records)
}

fn approved_sheet(name: &str) -> bool {
    let norm = name.trim().to_uppercase();
    matches!(
        norm.as_str(),
        "CIV" | "MEC" | "PCU" | "ROAD" | "SIGNED BY WORLEY CIV" | "SIGNED BY WORLEY MEC"
    )
}

fn extract_header_clause(sheet: &WorksheetData, max_row: u32, column: u32) -> Option<String> {
    static CLAUSE_PARENS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\((\d+\.\d+(?:\s*,\s*\d+\.\d+)*)\)").unwrap());
    static CLAUSE_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d+\.\d+)\b").unwrap());
    for r in (1..max_row).rev() {
        if let Some(cell) = sheet.cell(r, column) {
            let text = &cell.value;
            if let Some(caps) = CLAUSE_PARENS.captures(text)
                && let Some(m) = caps.get(1)
            {
                let last = m
                    .as_str()
                    .split(',')
                    .next_back()
                    .unwrap_or(m.as_str())
                    .trim();
                return Some(last.to_string());
            }
            if let Some(caps) = CLAUSE_WORD.captures(text)
                && let Some(m) = caps.get(1)
            {
                return Some(m.as_str().trim().to_string());
            }
        }
    }
    None
}

fn discover_approved_layouts(
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
    let mut layouts = Vec::new();
    for descriptor in &discovery.worksheets {
        if !approved_sheet(&descriptor.name) {
            continue;
        }
        if descriptor.visibility != WorksheetVisibility::Visible {
            return Err(protected());
        }
        let sheet = workbook
            .sheet(&descriptor.name)
            .ok_or_else(workbook_invalid)?;
        if sheet.has_sheet_protection {
            return Err(protected());
        }
        validate_cell_text(sheet)?;
        for row in 1..=sheet.max_row.min(10) {
            for rfi_column in 1..sheet.max_column.min(MAX_HEADER_COLUMNS) {
                if descriptor
                    .name
                    .trim()
                    .to_uppercase()
                    .contains("SIGNED BY WORLEY MEC")
                    && rfi_column > 14
                {
                    continue;
                }
                let rfi_header = sheet
                    .cell(row, rfi_column)
                    .map(|cell| normalize_header(&cell.value));
                let status_header = sheet
                    .cell(row, rfi_column + 1)
                    .map(|cell| normalize_header(&cell.value));
                if rfi_header.as_deref().is_some_and(is_follow_up_rfi_header)
                    && status_header.as_deref().is_some_and(|token| {
                        is_follow_up_generic_status_header(token)
                            || is_follow_up_worley_status_header(token)
                    })
                    && !merge_budget.contains(sheet, row, rfi_column)?
                    && !merge_budget.contains(sheet, row, rfi_column + 1)?
                {
                    let clause = extract_header_clause(sheet, row, rfi_column);
                    layouts.push(FollowUpLayout {
                        worksheet: descriptor.clone(),
                        header_row: row,
                        rfi_column,
                        status_column: rfi_column + 1,
                        clause,
                    });
                }
            }
        }
        if descriptor.name == "PCU"
            && sheet
                .cell(3, 18)
                .is_some_and(|cell| normalize_header(&cell.value) == "SIGNED BY WORLEY")
            && sheet
                .cell(4, 16)
                .is_some_and(|cell| is_follow_up_rfi_header(&normalize_header(&cell.value)))
            && sheet.cell(4, 17).is_some_and(|cell| {
                is_follow_up_generic_status_header(&normalize_header(&cell.value))
            })
            && !merge_budget.contains(sheet, 4, 16)?
            && !merge_budget.contains(sheet, 3, 18)?
        {
            layouts.push(FollowUpLayout {
                worksheet: descriptor.clone(),
                header_row: 4,
                rfi_column: 16,
                status_column: 18,
                clause: Some("3.6".to_string()),
            });
        }
    }
    if layouts.is_empty() {
        return Err(BackendError::validation("error.rfi.followUp.layoutMissing"));
    }
    Ok(layouts)
}

fn route_clause(record: &SourceRecord) -> Option<(&'static str, &'static str)> {
    let (discipline, itp) = itp_identity(&record.itp)?;
    let clause = item_clause(&record.item)?;
    let pcu_tag = record.contexts.iter().any(|context| {
        let tags = normalized_parts(&context.tag);
        tags.iter().any(|tag| tag.starts_with("PCS-"))
            || normalize_header(&context.tag).starts_with("PCU-")
    });
    match (discipline.as_str(), itp.as_str(), clause.as_str()) {
        ("CIV", "0002", "1.5") => Some(("CIV", "1.5")),
        ("CIV", "0002", "1.8") => Some((if pcu_tag { "PCU" } else { "CIV" }, "1.8")),
        ("CIV", "0002", "2.1") | ("CIV", "0002", "2.2") => Some(("CIV", "2.1")),
        ("CIV", "0002", "2.4") => Some((if pcu_tag { "PCU" } else { "CIV" }, "2.4")),
        ("CIV", "0002", "2.5") => Some((if pcu_tag { "PCU" } else { "CIV" }, "2.5")),
        ("CIV", "0002", "3.1") => Some(("CIV", "3.1")),
        ("CIV", "0004", "1.7") => Some(("Road", "1.7")),
        ("CIV", "0004", "2.1") => Some(("Road", "2.1")),
        ("CIV", "0004", "2.2") => Some(("Road", "2.2")),
        ("CIV", "0004", "2.3") => Some(("Road", "2.3")),
        ("CIV", "0004", "2.4") => Some(("Road", "2.4")),
        ("CIV", "0004", "2.5") => Some(("Road", "2.5")),
        ("CIV", "0004", "2.6") => Some(("Road", "2.6")),
        ("CIV", "0004", "2.7") => Some(("Road", "2.7")),
        ("CIV", "0004", "2.9") => Some(("Road", "2.9")),
        ("CIV", "0004", "3.2") => Some(("Road", "3.2")),
        ("CIV", "0005", "2.1") => Some(("PCU", "2.1")),
        ("CIV", "0005", "2.2") => Some(("PCU", "2.2")),
        ("CIV", "0005", "2.3") => Some(("PCU", "2.3")),
        ("CIV", "0005", "3.6") => Some(("PCU", "3.6")),
        ("MEC", "0001", "1.6") => Some(("MEC", "1.6")),
        ("MEC", "0001", "2.1") | ("MEC", "0001", "2.2") | ("MEC", "0001", "2.3") => {
            Some(("MEC", "2.3"))
        }
        ("MEC", "0001", "3.3") | ("MEC", "0001", "3.4") => Some(("MEC", "3.4")),
        ("MEC", "0001", "3.5") => Some(("MEC", "3.5")),
        ("MEC", "0003", "3.1") => Some(("MEC", "3.1")),
        ("MEC", "0004", "3.2") => Some(("MEC", "3.2")),
        _ => None,
    }
}

fn signed_route_clause(
    record: &SourceRecord,
) -> Option<(&'static str, &'static str, &'static str)> {
    match route_clause(record)? {
        ("CIV", "3.1") => Some(("Signed By Worley CIV", "3.1", "CIV")),
        ("PCU", "3.6") => Some(("PCU", "3.6", "PCU")),
        ("MEC", "3.5") => Some(("Signed By Worley MEC", "3.5", "MEC")),
        ("MEC", "3.1") => Some(("Signed By Worley MEC", "3.1", "MEC")),
        ("MEC", "3.2") => Some(("Signed By Worley MEC", "3.2", "MEC")),
        _ => None,
    }
}

fn default_route_col(sheet: &str, clause: &str) -> u32 {
    match (sheet, clause) {
        ("CIV", "1.5") => 3,
        ("CIV", "1.8") => 5,
        ("CIV", "2.1") => 7,
        ("CIV", "2.4") => 9,
        ("CIV", "2.5") => 11,
        ("CIV", "3.1") => 13,
        ("PCU", "1.8") => 4,
        ("PCU", "2.4") => 6,
        ("PCU", "2.5") => 8,
        ("PCU", "2.1") => 10,
        ("PCU", "2.2") => 12,
        ("PCU", "2.3") => 14,
        ("PCU", "3.6") => 16,
        ("Road", "1.7") => 3,
        ("Road", "2.1") => 5,
        ("Road", "2.2") => 7,
        ("Road", "2.3") => 9,
        ("Road", "2.4") => 11,
        ("Road", "2.5") => 13,
        ("Road", "2.6") => 15,
        ("Road", "2.7") => 17,
        ("Road", "2.9") => 19,
        ("Road", "3.2") => 23,
        ("MEC", "1.6") => 8,
        ("MEC", "2.3") => 10,
        ("MEC", "3.4") => 12,
        ("MEC", "3.5") => 14,
        ("MEC", "3.1") => 16,
        ("MEC", "3.2") => 18,
        _ => 3,
    }
}

fn default_signed_col(sheet: &str, clause: &str) -> (u32, u32) {
    match (sheet, clause) {
        ("Signed By Worley CIV", "3.1") => (3, 4),
        ("PCU", "3.6") => (16, 18),
        ("Signed By Worley MEC", "3.5") => (8, 9),
        ("Signed By Worley MEC", "3.1") => (10, 11),
        ("Signed By Worley MEC", "3.2") => (12, 13),
        _ => (3, 4),
    }
}

#[allow(dead_code)]
fn route_key(record: &SourceRecord) -> Option<(&'static str, u32)> {
    let (sheet, clause) = route_clause(record)?;
    Some((sheet, default_route_col(sheet, clause)))
}

#[allow(dead_code)]
fn signed_route_key(record: &SourceRecord) -> Option<(&'static str, u32, u32, &'static str)> {
    let (sheet, clause, match_layout) = signed_route_clause(record)?;
    let (rfi, st) = default_signed_col(sheet, clause);
    Some((sheet, rfi, st, match_layout))
}

fn is_signed_layout(layout: &FollowUpLayout) -> bool {
    layout
        .worksheet
        .name
        .trim()
        .to_uppercase()
        .contains("SIGNED BY WORLEY")
        || (layout.worksheet.name.trim().to_uppercase().contains("PCU")
            && layout.status_column == 18)
}

fn normalized_location(value: &str) -> String {
    normalize_header(value)
        .replace(" SITE", "")
        .replace("PCU-", "PCS-")
        .replace("ROAD 0", "ROAD ")
}

fn normalized_parts(value: &str) -> Vec<String> {
    split_values(value)
        .into_iter()
        .map(|part| normalized_location(&part))
        .filter(|part| !part.is_empty())
        .collect()
}

fn pcu_row_matches(sheet: &WorksheetData, row: u32, context: &SourceRouteContext) -> bool {
    let tags = normalized_parts(&context.tag);
    let blocks = tags
        .iter()
        .filter(|tag| tag.starts_with("PCS-"))
        .collect::<Vec<_>>();
    let block = if blocks.len() == 1 {
        blocks[0].clone()
    } else if blocks.is_empty() && normalize_header(&context.tag).starts_with("PCU-") {
        normalized_location(&context.tag)
    } else {
        return false;
    };
    if sheet
        .cell(row, 3)
        .map(|cell| normalized_location(&cell.value))
        .as_deref()
        != Some(block.as_str())
    {
        return false;
    }
    let areas = tags
        .iter()
        .filter(|tag| !tag.starts_with("PCS-"))
        .collect::<Vec<_>>();
    areas.is_empty()
        || sheet.cell(row, 2).is_some_and(|cell| {
            normalized_parts(&cell.value)
                .iter()
                .any(|part| areas.iter().any(|area| **area == *part))
        })
}

fn unique_zone_for_pcs(target_workbook: &WorkbookData, pcs: &str) -> Option<String> {
    let sheet = target_workbook.sheet("MEC")?;
    let zones = (1..=sheet.max_row)
        .filter_map(|row| {
            let block = sheet.cell(row, 3)?;
            (normalized_location(&block.value) == pcs)
                .then(|| {
                    sheet
                        .cell(row, 1)
                        .map(|cell| normalized_location(&cell.value))
                })
                .flatten()
        })
        .filter(|zone| !zone.is_empty())
        .collect::<BTreeSet<_>>();
    (zones.len() == 1)
        .then(|| zones.into_iter().next())
        .flatten()
}

fn row_matches(
    sheet_name: &str,
    sheet: &WorksheetData,
    row: u32,
    record: &SourceRecord,
    target_workbook: &WorkbookData,
) -> bool {
    let cell = |column| {
        sheet
            .cell(row, column)
            .map(|cell| normalized_location(&cell.value))
            .unwrap_or_default()
    };
    match sheet_name {
        "CIV" => record.contexts.iter().any(|context| {
            let tags = normalized_parts(&context.tag);
            let pcs = tags
                .iter()
                .filter(|tag| tag.starts_with("PCS-"))
                .collect::<Vec<_>>();
            if pcs.len() == 1 && tags.len() > 1 {
                let areas = tags
                    .iter()
                    .filter(|tag| !tag.starts_with("PCS-"))
                    .cloned()
                    .chain(normalized_parts(&context.area))
                    .collect::<BTreeSet<_>>();
                unique_zone_for_pcs(target_workbook, pcs[0]).as_deref() == Some(cell(1).as_str())
                    && areas.contains(&cell(2))
            } else if pcs.is_empty() {
                tags.iter().any(|value| value == &cell(2))
            } else {
                false
            }
        }),
        "PCU" => record
            .contexts
            .iter()
            .any(|context| pcu_row_matches(sheet, row, context)),
        "Road" => record
            .contexts
            .iter()
            .any(|context| normalized_location(&context.tag) == cell(2)),
        "MEC" => record.contexts.iter().any(|context| {
            let sections = split_values(&context.area);
            let tags = normalized_parts(&context.tag);
            !sections.is_empty()
                && !tags.is_empty()
                && tags
                    .iter()
                    .any(|tag| normalized_parts(&cell(3)).contains(tag))
                && sections
                    .iter()
                    .any(|value| normalized_parts(&cell(4)).contains(&normalized_location(value)))
        }),
        _ => false,
    }
}

fn distinct_civ_area_rows(
    target_workbook: &WorkbookData,
    candidates: &[(String, u32, u32, u32)],
    record: &SourceRecord,
) -> bool {
    if candidates.len() < 2 {
        return false;
    }
    let Some(sheet) = target_workbook.sheet("CIV") else {
        return false;
    };
    let source_tags = record
        .contexts
        .iter()
        .flat_map(|context| split_values(&context.tag))
        .map(|value| normalized_location(&value))
        .collect::<BTreeSet<_>>();
    let mut matched_tags = BTreeSet::new();
    candidates.iter().all(|(_, row, _, _)| {
        let tag = sheet
            .cell(*row, 2)
            .map(|cell| normalized_location(&cell.value))
            .unwrap_or_default();
        !tag.is_empty() && source_tags.contains(&tag) && matched_tags.insert(tag)
    })
}

fn distinct_civ_zone_area_rows(
    target_workbook: &WorkbookData,
    candidates: &[(String, u32, u32, u32)],
    record: &SourceRecord,
) -> bool {
    if candidates.len() < 2 {
        return false;
    }
    let Some(sheet) = target_workbook.sheet("CIV") else {
        return false;
    };
    let contexts = record
        .contexts
        .iter()
        .filter_map(|context| {
            let tags = normalized_parts(&context.tag);
            let pcs = tags
                .iter()
                .filter(|tag| tag.starts_with("PCS-"))
                .collect::<Vec<_>>();
            let areas = tags
                .iter()
                .filter(|tag| !tag.starts_with("PCS-"))
                .cloned()
                .chain(normalized_parts(&context.area))
                .collect::<BTreeSet<_>>();
            let zone = if pcs.len() == 1 {
                unique_zone_for_pcs(target_workbook, pcs[0])
            } else {
                None
            };
            (!areas.is_empty())
                .then(|| zone.map(|zone| (zone, areas)))
                .flatten()
        })
        .collect::<Vec<_>>();
    if contexts.is_empty() {
        return false;
    }
    let mut matched = BTreeSet::new();
    candidates.iter().all(|(_, row, _, _)| {
        let zone = sheet
            .cell(*row, 1)
            .map(|cell| normalized_location(&cell.value))
            .unwrap_or_default();
        let area = sheet
            .cell(*row, 2)
            .map(|cell| normalized_location(&cell.value))
            .unwrap_or_default();
        contexts
            .iter()
            .any(|(expected_zone, areas)| expected_zone == &zone && areas.contains(&area))
            && matched.insert((*row, zone, area))
    })
}

fn distinct_mec_context_rows(
    target_workbook: &WorkbookData,
    sheet_name: &str,
    candidates: &[(String, u32, u32, u32)],
    record: &SourceRecord,
) -> bool {
    if candidates.len() < 2 {
        return false;
    }
    let Some(sheet) = target_workbook.sheet(sheet_name) else {
        return false;
    };
    let mut source_sections = BTreeSet::new();
    let mut source_blocks = BTreeSet::new();
    for context in &record.contexts {
        let tags = normalized_parts(&context.tag);
        let areas = normalized_parts(&context.area);
        for tag in tags {
            if tag.starts_with("PCS-") {
                source_blocks.insert(tag);
            }
        }
        for area in areas {
            if area.starts_with('S') {
                source_sections.insert(area);
            }
        }
    }
    if source_blocks.len() != 1 || source_sections.is_empty() {
        return false;
    }
    let mut matched_sections = BTreeSet::new();
    for (_, row, _, _) in candidates {
        let row_pcs = sheet
            .cell(*row, 3)
            .map(|cell| normalized_location(&cell.value))
            .unwrap_or_default();
        let row_sec = sheet
            .cell(*row, 4)
            .map(|cell| normalized_location(&cell.value))
            .unwrap_or_default();
        if !source_blocks.contains(&row_pcs) || !source_sections.contains(&row_sec) {
            return false;
        }
        if !matched_sections.insert(row_sec) {
            return false;
        }
    }
    matched_sections.len() == candidates.len()
}

#[derive(Clone)]
struct SlotWork {
    layout: FollowUpLayout,
    row: u32,
    rfi_cell: CellSnapshot,
    status_cell: CellSnapshot,
    codes: Vec<String>,
    current_status: String,
    changed_rfis: BTreeSet<String>,
    unsafe_reason: Option<&'static str>,
}

fn safe_slot(
    layout: &FollowUpLayout,
    sheet: &WorksheetData,
    row: u32,
    merge_budget: &mut MergeQueryBudget,
) -> Result<Option<SlotWork>, BackendError> {
    let Some(rfi_cell) = sheet.cell(row, layout.rfi_column) else {
        return Ok(None);
    };
    let Some(status_cell) = sheet.cell(row, layout.status_column) else {
        return Ok(None);
    };
    let unsafe_reason = if rfi_cell.formula.is_some() {
        Some("targetRfiFormula")
    } else if status_cell.formula.is_some() {
        Some("targetFormula")
    } else if merge_budget.contains(sheet, row, layout.rfi_column)? {
        Some("targetRfiMerged")
    } else if merge_budget.contains(sheet, row, layout.status_column)? {
        Some("targetMerged")
    } else {
        None
    };
    let raw = normalize_status(&rfi_cell.value);
    let codes = if raw.is_empty() {
        Vec::new()
    } else if let Some(codes) = parse_rfi_group(&raw) {
        codes
    } else {
        return Ok(Some(SlotWork {
            layout: layout.clone(),
            row,
            rfi_cell: rfi_cell.into(),
            status_cell: status_cell.into(),
            codes: Vec::new(),
            current_status: normalize_status(&status_cell.value),
            changed_rfis: BTreeSet::new(),
            unsafe_reason: Some("invalidRfi"),
        }));
    };
    Ok(Some(SlotWork {
        layout: layout.clone(),
        row,
        rfi_cell: rfi_cell.into(),
        status_cell: status_cell.into(),
        codes,
        current_status: normalize_status(&status_cell.value),
        changed_rfis: BTreeSet::new(),
        unsafe_reason,
    }))
}

fn main_status_for_codes(
    codes: &[String],
    records: &BTreeMap<String, SourceRecord>,
) -> Option<String> {
    let real_codes = codes
        .iter()
        .filter(|code| !is_placeholder_rfi(code))
        .collect::<Vec<_>>();
    if real_codes.is_empty() {
        return None;
    }
    let states = real_codes
        .iter()
        .map(|code| records.get(*code)?.state)
        .collect::<Option<Vec<_>>>()?;
    aggregate_status(states)
}

fn row_from_slot(
    slot: SlotWork,
    records: &BTreeMap<String, SourceRecord>,
    signed: bool,
) -> Option<FollowUpRow> {
    if slot.changed_rfis.is_empty() || slot.unsafe_reason.is_some() {
        return None;
    }
    let proposed_rfi = format_rfi_group(&slot.codes).unwrap_or_default();
    let proposed_status = if signed {
        if slot.codes.is_empty() {
            String::new()
        } else {
            let real_codes = slot
                .codes
                .iter()
                .filter(|code| !is_placeholder_rfi(code))
                .collect::<Vec<_>>();
            if real_codes.is_empty() {
                slot.current_status.clone()
            } else {
                let states = real_codes
                    .iter()
                    .map(|code| records.get(*code))
                    .collect::<Option<Vec<_>>>()?;
                if states.is_empty() {
                    return None;
                }
                if states.iter().all(|record| record.worley_signed) {
                    "Signed".to_string()
                } else {
                    "Not Signed".to_string()
                }
            }
        }
    } else if slot.codes.is_empty() {
        String::new()
    } else {
        main_status_for_codes(&slot.codes, records).unwrap_or_else(|| slot.current_status.clone())
    };
    if proposed_rfi == slot.rfi_cell.value && proposed_status == slot.current_status {
        return None;
    }
    let canonical_rfi = slot.changed_rfis.iter().next()?.clone();
    let record = records.get(&canonical_rfi)?;
    Some(FollowUpRow {
        row_id: if slot.layout.status_column == slot.layout.rfi_column + 1 {
            format!(
                "rfi-follow-up:{}:{}:{}",
                slot.layout.worksheet.sheet_id, slot.row, slot.layout.rfi_column
            )
        } else {
            format!(
                "rfi-follow-up:{}:{}:{}:{}",
                slot.layout.worksheet.sheet_id,
                slot.row,
                slot.layout.rfi_column,
                slot.layout.status_column
            )
        },
        sheet: slot.layout.worksheet.name.clone(),
        row: slot.row,
        rfi_no: record.short_rfi.clone(),
        source_statuses: vec![if signed {
            if record.worley_signed {
                "Signed".to_string()
            } else {
                "Not Signed".to_string()
            }
        } else {
            record.inspection_status_for_ui()
        }],
        current_status: slot.current_status,
        proposed_status: Some(proposed_status),
        proposed_rfi_no: Some(proposed_rfi),
        disposition: FollowUpDisposition::Ready,
        reason_key: if record.excluded_civ_pcs || record.state == Some(LogState::Remove) {
            "warn.rfi.followUp.reason.readyRemove".to_string()
        } else if signed {
            "warn.rfi.followUp.reason.readyWorleySync".to_string()
        } else {
            "warn.rfi.followUp.reason.readyLogSync".to_string()
        },
        selectable: true,
        enabled: true,
        target: FollowUpReviewTarget {
            worksheet: slot.layout.worksheet,
            row: slot.row,
            rfi_column: slot.layout.rfi_column,
            status_column: slot.layout.status_column,
            rfi_cell: Some(slot.rfi_cell),
            status_cell: Some(slot.status_cell),
        },
        source_evidence: record.evidence.clone(),
    })
}

fn unsafe_slot_review(
    slot: &SlotWork,
    records: &BTreeMap<String, SourceRecord>,
) -> Option<FollowUpRow> {
    let reason = slot.unsafe_reason?;
    let record = slot
        .changed_rfis
        .iter()
        .find_map(|code| records.get(code))?;
    Some(FollowUpRow {
        row_id: format!(
            "rfi-follow-up-removal-unsafe:{}:{}:{}",
            slot.layout.worksheet.sheet_id, slot.row, slot.layout.rfi_column
        ),
        sheet: slot.layout.worksheet.name.clone(),
        row: slot.row,
        rfi_no: record.short_rfi.clone(),
        source_statuses: vec![record.inspection_status_for_ui()],
        current_status: slot.current_status.clone(),
        proposed_status: None,
        proposed_rfi_no: None,
        disposition: FollowUpDisposition::ManualReview,
        reason_key: format!("warn.rfi.followUp.reason.{reason}"),
        selectable: false,
        enabled: false,
        target: FollowUpReviewTarget {
            worksheet: slot.layout.worksheet.clone(),
            row: slot.row,
            rfi_column: slot.layout.rfi_column,
            status_column: slot.layout.status_column,
            rfi_cell: Some(slot.rfi_cell.clone()),
            status_cell: Some(slot.status_cell.clone()),
        },
        source_evidence: record.evidence.clone(),
    })
}

impl SourceRecord {
    fn inspection_status_for_ui(&self) -> String {
        match self.state {
            Some(LogState::Pending) => "Pending",
            Some(LogState::InProgress) => "In Progress",
            Some(LogState::Completed) => "Completed",
            Some(LogState::Remove) => "Cancelled / Rejected",
            None => "Manual review",
        }
        .to_string()
    }
}

fn classify_workbooks_full(
    source_discovery: &OrderedWorkbookDiscovery,
    source_workbook: &WorkbookData,
    target_discovery: &OrderedWorkbookDiscovery,
    target_workbook: &WorkbookData,
    target_has_signature: bool,
) -> Result<Value, BackendError> {
    let records = scan_source_records(source_discovery, source_workbook)?;
    let mut merge_budget = MergeQueryBudget {
        remaining: MAX_FOLLOW_UP_MERGE_COMPARISONS,
    };
    let layouts = discover_approved_layouts(
        target_discovery,
        target_workbook,
        target_has_signature,
        &mut merge_budget,
    )?;
    let mut slots = BTreeMap::<(String, u32, u32, u32), SlotWork>::new();
    for layout in &layouts {
        let sheet = target_workbook
            .sheet(&layout.worksheet.name)
            .ok_or_else(workbook_invalid)?;
        let last = sheet
            .max_row
            .min(layout.header_row + MAX_FOLLOW_UP_DATA_ROWS);
        for row in (layout.header_row + 1)..=last {
            if let Some(slot) = safe_slot(layout, sheet, row, &mut merge_budget)? {
                slots.insert(
                    (
                        layout.worksheet.name.clone(),
                        row,
                        layout.rfi_column,
                        layout.status_column,
                    ),
                    slot,
                );
            }
        }
    }

    let mut manual_rows = Vec::new();
    for slot in slots.values_mut() {
        let removed = slot
            .codes
            .iter()
            .filter(|code| {
                !is_placeholder_rfi(code)
                    && records.get(*code).is_some_and(|record| {
                        record.excluded_civ_pcs || record.state == Some(LogState::Remove)
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        if !removed.is_empty() {
            slot.codes.retain(|code| !removed.contains(code));
            slot.changed_rfis.extend(removed);
        }
        // Signed By layouts show Worley state, not RFI lifecycle state.
        if !is_signed_layout(&slot.layout)
            && slot
                .codes
                .iter()
                .filter(|code| !is_placeholder_rfi(code))
                .any(|code| records.get(code).and_then(|record| record.state).is_some())
        {
            let next = main_status_for_codes(&slot.codes, &records);
            if next
                .as_deref()
                .is_some_and(|status| status != slot.current_status)
            {
                slot.changed_rfis.extend(slot.codes.iter().cloned());
            }
        }
    }

    let mut eligible_slots_by_sheet_col =
        BTreeMap::<(String, u32), Vec<(String, u32, u32, u32)>>::new();
    let mut existing_slot_keys_by_code = BTreeMap::<String, Vec<(String, u32, u32, u32)>>::new();
    for (key, slot) in &slots {
        if !is_signed_layout(&slot.layout) {
            for code in &slot.codes {
                existing_slot_keys_by_code
                    .entry(code.clone())
                    .or_default()
                    .push(key.clone());
            }
            if slot.unsafe_reason.is_none() {
                eligible_slots_by_sheet_col
                    .entry((key.0.clone(), key.2))
                    .or_default()
                    .push(key.clone());
            }
        }
    }

    let mut unmapped_count = 0usize;
    for record in records.values().filter(|record| {
        !record.excluded_civ_pcs && record.state.is_some_and(|state| state != LogState::Remove)
    }) {
        let Some((target_sheet, target_clause)) = route_clause(record) else {
            continue;
        };
        let composite_pcs_route = record.contexts.iter().any(|context| {
            let tags = normalized_parts(&context.tag);
            tags.iter().filter(|tag| tag.starts_with("PCS-")).count() == 1 && tags.len() > 1
        });
        let (target_sheet, target_clause) = if composite_pcs_route {
            (
                "CIV",
                match target_clause {
                    "1.8" => "1.8",
                    "2.4" => "2.4",
                    _ => "2.5",
                },
            )
        } else {
            (target_sheet, target_clause)
        };
        let matching_layout = layouts.iter().find(|l| {
            l.worksheet.name.trim().eq_ignore_ascii_case(target_sheet)
                && (l.clause.as_deref() == Some(target_clause)
                    || (l.clause.is_none()
                        && l.rfi_column == default_route_col(target_sheet, target_clause)))
        });
        let Some(layout) = matching_layout else {
            continue;
        };
        let sheet_name = layout.worksheet.name.as_str();
        let rfi_column = layout.rfi_column;
        let empty_eligible = Vec::new();
        let eligible = eligible_slots_by_sheet_col
            .get(&(sheet_name.to_string(), rfi_column))
            .unwrap_or(&empty_eligible);
        let candidates = eligible
            .iter()
            .filter(|(_, row, _, _)| {
                row_matches(
                    sheet_name,
                    target_workbook.sheet(sheet_name).unwrap(),
                    *row,
                    record,
                    target_workbook,
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut candidates = candidates;
        if candidates.is_empty() && sheet_name == "CIV" {
            let source_tags = record
                .contexts
                .iter()
                .flat_map(|context| normalized_parts(&context.tag))
                .collect::<BTreeSet<_>>();
            if source_tags.iter().any(|tag| {
                tag.starts_with("PCS-") && unique_zone_for_pcs(target_workbook, tag).is_some()
            }) {
                candidates = eligible
                    .iter()
                    .filter(|(_, row, _, _)| {
                        row_matches(
                            "CIV",
                            target_workbook.sheet("CIV").unwrap(),
                            *row,
                            record,
                            target_workbook,
                        )
                    })
                    .cloned()
                    .collect();
            }
        }
        let candidate_count = candidates.len();
        // Signed By rows repeat RFIs as Worley summaries, not lifecycle placements.
        let existing_slots = existing_slot_keys_by_code
            .get(record.canonical_rfi.as_str())
            .cloned()
            .unwrap_or_default();
        if candidate_count == 0 {
            // A recognized source route with no corresponding Follow Up row is
            // outside this workbook's current scope. Keep it out of the review
            // queue; records matching multiple rows still require operator review.
            continue;
        }
        if candidate_count == 1 && !existing_slots.is_empty() {
            continue;
        }
        let exact_multi_area = candidate_count > 1
            && ((sheet_name == "CIV"
                && (distinct_civ_area_rows(target_workbook, &candidates, record)
                    || distinct_civ_zone_area_rows(target_workbook, &candidates, record)))
                || (sheet_name == "MEC"
                    && distinct_mec_context_rows(
                        target_workbook,
                        sheet_name,
                        &candidates,
                        record,
                    )));
        if candidate_count > 1 && !exact_multi_area {
            if !existing_slots.is_empty() {
                continue;
            }
            unmapped_count += 1;
            let Some(layout) = layouts
                .iter()
                .find(|layout| {
                    layout.worksheet.name == sheet_name && layout.rfi_column == rfi_column
                })
                .cloned()
            else {
                unmapped_count -= 1;
                continue;
            };
            manual_rows.push(FollowUpRow {
                row_id: format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
                sheet: sheet_name.to_string(),
                row: 0,
                rfi_no: record.short_rfi.clone(),
                source_statuses: vec![record.inspection_status_for_ui()],
                current_status: String::new(),
                proposed_status: None,
                proposed_rfi_no: None,
                disposition: FollowUpDisposition::ManualReview,
                reason_key: if candidate_count == 0 {
                    "warn.rfi.followUp.reason.mappingMissing".to_string()
                } else {
                    "warn.rfi.followUp.reason.mappingAmbiguous".to_string()
                },
                selectable: false,
                enabled: false,
                target: FollowUpReviewTarget {
                    worksheet: layout.worksheet,
                    row: 0,
                    rfi_column: layout.rfi_column,
                    status_column: layout.status_column,
                    rfi_cell: None,
                    status_cell: None,
                },
                source_evidence: record.evidence.clone(),
            });
            continue;
        }
        let already_mapped_elsewhere = existing_slots.iter().any(|key| !candidates.contains(key));
        if already_mapped_elsewhere {
            unmapped_count += 1;
            let layout = slots
                .get(&candidates[0])
                .map(|slot| slot.layout.clone())
                .ok_or_else(workbook_invalid)?;
            manual_rows.push(FollowUpRow {
                row_id: format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
                sheet: sheet_name.to_string(),
                row: 0,
                rfi_no: record.short_rfi.clone(),
                source_statuses: vec![record.inspection_status_for_ui()],
                current_status: String::new(),
                proposed_status: None,
                proposed_rfi_no: None,
                disposition: FollowUpDisposition::ManualReview,
                reason_key: "warn.rfi.followUp.reason.mappingAmbiguous".to_string(),
                selectable: false,
                enabled: false,
                target: FollowUpReviewTarget {
                    worksheet: layout.worksheet,
                    row: 0,
                    rfi_column: layout.rfi_column,
                    status_column: layout.status_column,
                    rfi_cell: None,
                    status_cell: None,
                },
                source_evidence: record.evidence.clone(),
            });
            continue;
        }
        for key in candidates {
            let slot = slots.get_mut(&key).ok_or_else(workbook_invalid)?;
            if !slot.codes.contains(&record.canonical_rfi) {
                let compatible = slot
                    .codes
                    .iter()
                    .filter(|code| !is_placeholder_rfi(code))
                    .all(|code| {
                        records.get(code).is_some_and(|existing| {
                            existing.itp == record.itp
                                && item_clause(&existing.item) == item_clause(&record.item)
                        })
                    });
                if !compatible {
                    continue;
                }
                slot.codes.push(record.canonical_rfi.clone());
                slot.changed_rfis.insert(record.canonical_rfi.clone());
            }
        }
    }

    let mut rows = Vec::new();
    for slot in slots
        .values()
        .filter(|slot| !is_signed_layout(&slot.layout))
        .cloned()
    {
        let source_issue = slot
            .codes
            .iter()
            .filter(|code| !is_placeholder_rfi(code))
            .find_map(|code| {
                if !records.contains_key(code) {
                    Some("sourceMissing")
                } else if records
                    .get(code)
                    .is_some_and(|record| record.state.is_none())
                {
                    Some("sourceStatusUnknown")
                } else {
                    None
                }
            });
        if slot.unsafe_reason.is_none()
            && let Some(reason) = source_issue
            && let Some(record) = slot
                .codes
                .iter()
                .filter(|code| !is_placeholder_rfi(code))
                .find_map(|code| records.get(code))
        {
            manual_rows.push(FollowUpRow {
                row_id: format!(
                    "rfi-follow-up-source-unsafe:{}:{}:{}",
                    slot.layout.worksheet.sheet_id, slot.row, slot.layout.rfi_column
                ),
                sheet: slot.layout.worksheet.name.clone(),
                row: slot.row,
                rfi_no: record.short_rfi.clone(),
                source_statuses: vec![record.inspection_status_for_ui()],
                current_status: slot.current_status.clone(),
                proposed_status: None,
                proposed_rfi_no: None,
                disposition: FollowUpDisposition::ManualReview,
                reason_key: format!("warn.rfi.followUp.reason.{reason}"),
                selectable: false,
                enabled: false,
                target: FollowUpReviewTarget {
                    worksheet: slot.layout.worksheet.clone(),
                    row: slot.row,
                    rfi_column: slot.layout.rfi_column,
                    status_column: slot.layout.status_column,
                    rfi_cell: Some(slot.rfi_cell.clone()),
                    status_cell: Some(slot.status_cell.clone()),
                },
                source_evidence: record.evidence.clone(),
            });
            continue;
        }
        if slot.unsafe_reason.is_some() && !slot.changed_rfis.is_empty() {
            if let Some(row) = unsafe_slot_review(&slot, &records) {
                unmapped_count += 1;
                manual_rows.push(row);
            }
            continue;
        }
        if let Some(row) = row_from_slot(slot, &records, false) {
            rows.push(row);
        }
    }

    let mut present_signed_targets = BTreeSet::new();
    for slot in slots.values().filter(|slot| is_signed_layout(&slot.layout)) {
        for code in &slot.codes {
            present_signed_targets.insert((
                slot.layout.worksheet.name.clone(),
                slot.layout.rfi_column,
                slot.layout.status_column,
                code.clone(),
            ));
        }
    }
    for record in records.values().filter(|record| {
        !record.excluded_civ_pcs
            && record.state.is_some_and(|state| state != LogState::Remove)
            && signed_route_clause(record).is_some()
    }) {
        let (target_sheet, target_clause, match_layout) =
            signed_route_clause(record).expect("filtered final record has a signed route");
        if target_sheet
            .trim()
            .to_uppercase()
            .contains("SIGNED BY WORLEY MEC")
            && !record.worley_signed
        {
            continue;
        }
        let matching_layout = layouts.iter().find(|l| {
            l.worksheet.name.trim().eq_ignore_ascii_case(target_sheet)
                && (l.clause.as_deref() == Some(target_clause)
                    || (target_sheet.trim().to_uppercase().contains("PCU")
                        && l.status_column == 18)
                    || (l.clause.is_none()
                        && l.rfi_column == default_signed_col(target_sheet, target_clause).0))
        });
        let Some(layout) = matching_layout else {
            continue;
        };
        let sheet_name = layout.worksheet.name.as_str();
        let rfi_column = layout.rfi_column;
        let status_column = layout.status_column;
        if present_signed_targets.contains(&(
            sheet_name.to_string(),
            rfi_column,
            status_column,
            record.canonical_rfi.clone(),
        )) {
            continue;
        }
        let Some(matching_sheet) = target_workbook
            .sheet(sheet_name)
            .or_else(|| find_sheet_flexible(target_workbook, sheet_name))
        else {
            continue;
        };
        let candidates = slots
            .iter_mut()
            .filter(|((sheet, _, column, status), _)| {
                sheet == sheet_name && *column == rfi_column && *status == status_column
            })
            .filter(|((_, row, _, _), _)| {
                row_matches(match_layout, matching_sheet, *row, record, target_workbook)
            })
            .collect::<Vec<_>>();
        let candidate_keys = candidates
            .iter()
            .map(|(key, _)| (*key).clone())
            .collect::<Vec<_>>();
        let candidate_count = candidate_keys.len();
        if candidate_count == 0 {
            // No prepared row means this FINAL item is absent from the current
            // Follow Up template. Do not treat absence as a failed sync.
            continue;
        }
        let exact_multi_signed = candidate_count > 1
            && ((sheet_name == "CIV"
                && (distinct_civ_area_rows(target_workbook, &candidate_keys, record)
                    || distinct_civ_zone_area_rows(target_workbook, &candidate_keys, record)))
                || (sheet_name == "Signed By Worley MEC"
                    && distinct_mec_context_rows(
                        target_workbook,
                        sheet_name,
                        &candidate_keys,
                        record,
                    )));
        if candidate_count == 1 || exact_multi_signed {
            for (_, slot) in candidates {
                if sheet_name == "PCU" && slot.codes.is_empty() {
                    continue;
                }
                if let Some(reason) = slot.unsafe_reason {
                    unmapped_count += 1;
                    manual_rows.push(FollowUpRow {
                        row_id: format!(
                            "rfi-follow-up-signed-unsafe:{}:{}:{}",
                            record.canonical_rfi, slot.row, slot.layout.rfi_column
                        ),
                        sheet: sheet_name.to_string(),
                        row: slot.row,
                        rfi_no: record.short_rfi.clone(),
                        source_statuses: vec![if record.worley_signed {
                            "Signed".to_string()
                        } else {
                            "Not Signed".to_string()
                        }],
                        current_status: slot.current_status.clone(),
                        proposed_status: None,
                        proposed_rfi_no: None,
                        disposition: FollowUpDisposition::ManualReview,
                        reason_key: format!("warn.rfi.followUp.reason.{reason}"),
                        selectable: false,
                        enabled: false,
                        target: FollowUpReviewTarget {
                            worksheet: slot.layout.worksheet.clone(),
                            row: slot.row,
                            rfi_column: slot.layout.rfi_column,
                            status_column: slot.layout.status_column,
                            rfi_cell: Some(slot.rfi_cell.clone()),
                            status_cell: Some(slot.status_cell.clone()),
                        },
                        source_evidence: record.evidence.clone(),
                    });
                    continue;
                }
                if !slot.codes.contains(&record.canonical_rfi) {
                    let compatible = slot
                        .codes
                        .iter()
                        .filter(|code| !is_placeholder_rfi(code))
                        .all(|code| {
                            records.get(code).is_some_and(|existing| {
                                existing.itp == record.itp
                                    && item_clause(&existing.item) == item_clause(&record.item)
                            })
                        });
                    if !compatible {
                        continue;
                    }
                    slot.codes.push(record.canonical_rfi.clone());
                    slot.changed_rfis.insert(record.canonical_rfi.clone());
                }
                present_signed_targets.insert((
                    sheet_name.to_string(),
                    rfi_column,
                    status_column,
                    record.canonical_rfi.clone(),
                ));
            }
            continue;
        }
        unmapped_count += 1;
        let Some(layout) = layouts
            .iter()
            .find(|layout| {
                layout.worksheet.name == sheet_name
                    && layout.rfi_column == rfi_column
                    && layout.status_column == status_column
            })
            .cloned()
        else {
            unmapped_count -= 1;
            continue;
        };
        manual_rows.push(FollowUpRow {
            row_id: format!("rfi-follow-up-signed-unmapped:{}", record.canonical_rfi),
            sheet: sheet_name.to_string(),
            row: 0,
            rfi_no: record.short_rfi.clone(),
            source_statuses: vec![if record.worley_signed {
                "Signed".to_string()
            } else {
                "Not Signed".to_string()
            }],
            current_status: String::new(),
            proposed_status: None,
            proposed_rfi_no: None,
            disposition: FollowUpDisposition::ManualReview,
            reason_key: if candidate_count == 0 {
                "warn.rfi.followUp.reason.mappingMissing".to_string()
            } else {
                "warn.rfi.followUp.reason.mappingAmbiguous".to_string()
            },
            selectable: false,
            enabled: false,
            target: FollowUpReviewTarget {
                worksheet: layout.worksheet,
                row: 0,
                rfi_column: layout.rfi_column,
                status_column: layout.status_column,
                rfi_cell: None,
                status_cell: None,
            },
            source_evidence: record.evidence.clone(),
        });
    }

    for slot in slots
        .values_mut()
        .filter(|slot| is_signed_layout(&slot.layout))
    {
        let original = slot.codes.clone();
        slot.codes.retain(|code| {
            if is_placeholder_rfi(code) {
                return true;
            }
            !records
                .get(code)
                .is_some_and(|record| record.state == Some(LogState::Remove))
        });
        if slot.codes != original {
            slot.changed_rfis.extend(
                original
                    .into_iter()
                    .filter(|code| !slot.codes.contains(code)),
            );
        }
        for code in &slot.codes {
            if let Some(record) = records.get(code) {
                let proposed = if record.worley_signed {
                    "Signed"
                } else {
                    "Not Signed"
                };
                if proposed != slot.current_status {
                    slot.changed_rfis.insert(code.clone());
                }
            }
        }
    }
    for slot in slots
        .values()
        .filter(|slot| is_signed_layout(&slot.layout))
        .cloned()
    {
        if slot.unsafe_reason.is_some() && !slot.changed_rfis.is_empty() {
            if let Some(row) = unsafe_slot_review(&slot, &records) {
                unmapped_count += 1;
                manual_rows.push(row);
            }
            continue;
        }
        if let Some(row) = row_from_slot(slot, &records, true) {
            rows.push(row);
        }
    }
    rows.extend(manual_rows);

    if rows.len() > MAX_PREVIEW_ROWS {
        return Err(limit_exceeded());
    }
    rows.sort_by(|left, right| {
        left.sheet
            .cmp(&right.sheet)
            .then(left.row.cmp(&right.row))
            .then(left.target.rfi_column.cmp(&right.target.rfi_column))
    });
    let mut warnings = Vec::new();
    if unmapped_count > 0 {
        warnings.push(Warning {
            key: "warn.rfi.followUp.unmappedRecords".to_string(),
            subject: unmapped_count.to_string(),
        });
    }
    if rows.is_empty() {
        warnings.push(Warning {
            key: "warn.rfi.followUp.noRowsSelected".to_string(),
            subject: String::new(),
        });
    }
    let columns = editor_columns();
    let primary_layout = layouts.first().ok_or_else(workbook_invalid)?.clone();
    let editor = editor_metadata(&rows, columns);
    let projection = json!({"layout": primary_layout, "layouts": layouts, "rows": rows, "warnings": warnings, "editor": editor});
    validate_projection_budget(&projection)?;
    Ok(projection)
}

fn find_sheet_flexible<'a>(workbook: &'a WorkbookData, target: &str) -> Option<&'a WorksheetData> {
    workbook.sheet(target).or_else(|| {
        let norm = target.trim().to_uppercase();
        workbook.sheets.iter().find_map(|(name, sheet)| {
            if name.trim().to_uppercase() == norm {
                Some(sheet)
            } else {
                None
            }
        })
    })
}

fn supports_full_sync_schema(workbook: &WorkbookData) -> bool {
    ["CIV", "MEC"].iter().all(|name| {
        find_sheet_flexible(workbook, name).is_some_and(|sheet| {
            find_header_column(sheet, &["RFI NO.", "RFI NO", "RFI #"]).is_some()
                && find_header_column(sheet, &["ITP NO:", "ITP NO.", "ITP NO"]).is_some()
                && find_header_column(sheet, &["ITEM NO.", "ITEM NO", "ITEM #"]).is_some()
                && find_header_column(sheet, &["INSPECTION STATUS", "STATUS"]).is_some()
                && find_header_column(sheet, &["REMARKS", "REMARK"]).is_some()
        })
    })
}

pub fn classify_workbooks(
    source_discovery: &OrderedWorkbookDiscovery,
    source_workbook: &WorkbookData,
    target_discovery: &OrderedWorkbookDiscovery,
    target_workbook: &WorkbookData,
    target_has_signature: bool,
) -> Result<Value, BackendError> {
    if supports_full_sync_schema(source_workbook)
        && target_workbook
            .sheets
            .keys()
            .any(|name| approved_sheet(name.trim()))
    {
        classify_workbooks_full(
            source_discovery,
            source_workbook,
            target_discovery,
            target_workbook,
            target_has_signature,
        )
    } else {
        classify_workbooks_legacy(
            source_discovery,
            source_workbook,
            target_discovery,
            target_workbook,
            target_has_signature,
        )
    }
}

#[allow(dead_code)]
fn classify_workbooks_legacy(
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
    let columns = editor_columns_legacy();
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
    let (source_discovery, source_workbook) =
        read_workbook_and_discovery(rfi_log).map_err(map_follow_up_ooxml_error)?;

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
    let (target_discovery, target_workbook) =
        read_workbook_and_discovery(follow_up).map_err(map_follow_up_ooxml_error)?;
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
        "layouts": authority.layouts,
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

pub(crate) fn fingerprint_open_file(file: &mut File) -> Result<WorkbookFingerprint, BackendError> {
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
                proposed_rfi: row.proposed_rfi_no.clone().unwrap_or_else(|| {
                    row.target
                        .rfi_cell
                        .as_ref()
                        .map(|cell| cell.value.clone())
                        .unwrap_or_default()
                }),
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
                    clause: None,
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
    } else if analysis.proposed_status.as_deref() == Some(pending.current_status.as_str()) {
        (
            FollowUpDisposition::AlreadySynced,
            "warn.rfi.followUp.reason.alreadySynced".to_string(),
        )
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
            proposed_rfi_no: None,
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
        json!({"key": "proposedRfiNo", "labelKey": "rfi.followUp.column.proposedRfiNo", "kind": "readonly"}),
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

fn editor_columns_legacy() -> Vec<Value> {
    editor_columns()
        .into_iter()
        .filter(|column| {
            !matches!(
                column.get("key").and_then(Value::as_str),
                Some("proposedRfiNo")
            )
        })
        .collect()
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
            || !matches!(
                update.proposed_status.as_str(),
                "" | "Pending" | "In Progress" | "Completed" | "Signed" | "Not Signed"
            )
            || (!update.proposed_rfi.is_empty() && parse_rfi_group(&update.proposed_rfi).is_none())
            || (!update.target.rfi_cell.value.trim().is_empty()
                && parse_rfi_group(&update.target.rfi_cell.value).is_none())
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
        let sheet_updates = updates_by_sheet
            .entry(update.target.worksheet.name)
            .or_default();
        if update.proposed_rfi != update.target.rfi_cell.value {
            let rfi_guarded = GuardedCellUpdate {
                expected: CellData {
                    value: update.target.rfi_cell.value,
                    formula: update.target.rfi_cell.formula,
                    style: update.target.rfi_cell.style,
                },
                replacement: CellUpdate::Text(update.proposed_rfi),
                create_if_absent: false,
            };
            let key = (update.target.row, update.target.rfi_column);
            if let Some(existing) = sheet_updates.get(&key) {
                if existing != &rfi_guarded {
                    return Err(workbook_invalid());
                }
            } else {
                sheet_updates.insert(key, rfi_guarded);
            }
        }
        let status_guarded = GuardedCellUpdate {
            expected: CellData {
                value: update.target.status_cell.value,
                formula: update.target.status_cell.formula,
                style: update.target.status_cell.style,
            },
            replacement: CellUpdate::Text(update.proposed_status),
            create_if_absent: false,
        };
        if sheet_updates
            .insert(
                (update.target.row, update.target.status_column),
                status_guarded,
            )
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
    verify_guarded_workbook_delta_text(source, private_output, &updates).map_err(
        |error| match error.kind() {
            OoxmlErrorKind::LimitExceeded => limit_exceeded(),
            OoxmlErrorKind::Invalid => {
                BackendError::validation("error.rfi.followUp.patchVerification")
            }
        },
    )
}

#[allow(dead_code)]
pub(crate) fn changed_cell_count(step: &Step) -> Result<u64, BackendError> {
    u64::try_from(
        guarded_updates(step)?
            .values()
            .map(BTreeMap::len)
            .sum::<usize>(),
    )
    .map_err(|_| limit_exceeded())
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
    patch_workbook_sheets_guarded_text(source, destination, &updates)
        .map_err(map_apply_ooxml_error)?;
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
    normalize_status(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
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

    #[test]
    fn compound_rfi_parser_expands_historical_forms_without_guessing() {
        let parsed = parse_rfi_group("CIV-71-77/ 100/101/162/163").unwrap();
        assert_eq!(parsed.len(), 11);
        assert_eq!(parsed.first().unwrap(), "AZE-SUN-INT-QMS-RFI-CIV-0071");
        assert_eq!(parsed.last().unwrap(), "AZE-SUN-INT-QMS-RFI-CIV-0163");
        assert_eq!(
            format_rfi_group(&parse_rfi_group("MEC-0370/0565").unwrap()).as_deref(),
            Some("MEC-0370/0565")
        );
        assert!(parse_rfi_group("CVI-0028").is_none());
        assert!(parse_rfi_group("MEC-02Â49").is_none());
        assert!(parse_rfi_group("MEC˗0249").is_none());
        assert!(parse_rfi_group("CIV-0100/MEC-0101").is_none());
        assert!(parse_rfi_group("CIV-0200-0100").is_none());
    }

    #[test]
    fn approved_status_policy_removes_terminal_failures_and_aggregates_groups() {
        assert_eq!(log_state("Cancelled"), Some(LogState::Remove));
        assert_eq!(log_state("Rejected"), Some(LogState::Remove));
        assert_eq!(
            aggregate_status([LogState::Pending]).as_deref(),
            Some("Pending")
        );
        assert_eq!(
            aggregate_status([LogState::Completed]).as_deref(),
            Some("Completed")
        );
        assert_eq!(
            aggregate_status([LogState::Completed, LogState::Pending]).as_deref(),
            Some("In Progress")
        );
        assert_eq!(aggregate_status([LogState::Remove]).as_deref(), Some(""));
    }

    #[test]
    fn worley_marker_requires_an_exact_line() {
        assert_eq!(mapped_follow_up_status("Signed by Worley"), "Signed");
        assert_eq!(
            mapped_follow_up_status("Note\nSigned by Worley\nOther"),
            "Signed"
        );
        assert_eq!(
            mapped_follow_up_status("Not Signed by Worley"),
            "Not Signed"
        );
        assert_eq!(
            mapped_follow_up_status("Signed by Worley team"),
            "Not Signed"
        );
    }

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
            proposed_rfi: "CIV-0001".to_string(),
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
                clause: None,
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
            clause: None,
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
            proposed_rfi_no: Some(update.proposed_rfi.clone()),
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

        classify_workbooks_legacy(
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
