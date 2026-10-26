//! Road structure for the Follow Up preview: derives the part rows the RFI Log requires,
//! builds the workbook as it will look after those rows exist (classification runs on
//! that), and describes each change as an informational review row.

use std::io::Cursor;

use super::*;

pub(super) const ROAD_SHEET: &str = "Road";
const MAX_STRUCTURE_OPS: usize = 500;

pub(super) struct RoadStructure {
    pub(super) ops: Vec<RowOp>,
}

fn road_sheet(workbook: &WorkbookData) -> Option<&WorksheetData> {
    find_sheet_flexible(workbook, ROAD_SHEET)
}

fn road_rows(sheet: &WorksheetData) -> Vec<RoadRow> {
    // Data rows start under the header that holds "ROAD No." in column B.
    let header = (1..=sheet.max_row.min(16)).find(|row| {
        sheet
            .cell(*row, 2)
            .is_some_and(|cell| normalize_header(&cell.value).starts_with("ROAD NO"))
    });
    let Some(header) = header else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for row in (header + 1)..=sheet.max_row {
        let Some(cell) = sheet.cell(row, 2) else {
            continue;
        };
        let refs = road_refs(&cell.value);
        if let [reference] = refs.as_slice() {
            rows.push(RoadRow {
                row,
                road: reference.road,
                part: reference.part,
            });
        }
    }
    rows
}

/// Needs derived from Log records that route to the Road sheet.
fn road_needs(records: &BTreeMap<String, SourceRecord>) -> BTreeMap<u32, RoadNeed> {
    let tags = records
        .values()
        .filter(|record| {
            !record.excluded_civ_pcs
                && !is_confirmed_follow_up_exclusion(record)
                && record.state != Some(LogState::Remove)
                && route_clause(record).is_some_and(|(sheet, _)| sheet == ROAD_SHEET)
        })
        .flat_map(|record| record.contexts.iter().map(|context| context.tag.as_str()))
        .collect::<Vec<_>>();
    needs_from_tags(tags)
}

/// Returns the structural operations for the Road sheet, or none when the sheet or the
/// full-schema Log is not available, or the package is signed.
pub(super) fn plan_structure(
    source_discovery: &OrderedWorkbookDiscovery,
    source_workbook: &WorkbookData,
    target_workbook: &WorkbookData,
    target_has_signature: bool,
) -> Result<RoadStructure, BackendError> {
    let none = RoadStructure { ops: Vec::new() };
    if target_has_signature || !supports_full_sync_schema(source_workbook) {
        return Ok(none);
    }
    let Some(sheet) = road_sheet(target_workbook) else {
        return Ok(none);
    };
    let rows = road_rows(sheet);
    if rows.is_empty() {
        return Ok(none);
    }
    let records = scan_source_records(source_discovery, source_workbook)?;
    let ops = plan_road_structure(
        &sheet_name_of(target_workbook, sheet),
        &rows,
        &road_needs(&records),
    );
    if ops.len() > MAX_STRUCTURE_OPS {
        return Err(limit_exceeded_for(format!(
            "{} Road structure changes; at most {MAX_STRUCTURE_OPS}",
            ops.len()
        )));
    }
    Ok(RoadStructure { ops })
}

fn sheet_name_of(workbook: &WorkbookData, sheet: &WorksheetData) -> String {
    workbook
        .sheets
        .iter()
        .find_map(|(name, candidate)| std::ptr::eq(candidate, sheet).then(|| name.clone()))
        .unwrap_or_else(|| ROAD_SHEET.to_string())
}

/// Builds the structured baseline package: the original with every operation applied.
pub(super) fn structured_baseline<R: Read + Seek>(
    original: &mut R,
    ops: &[RowOp],
) -> Result<Vec<u8>, BackendError> {
    apply_row_ops(original, ops).map_err(map_follow_up_ooxml_error)
}

/// Parses the structured baseline so classification sees the new rows.
pub(super) fn parse_baseline(
    bytes: Vec<u8>,
) -> Result<(OrderedWorkbookDiscovery, WorkbookData), BackendError> {
    let mut cursor = Cursor::new(bytes);
    read_workbook_and_discovery(&mut cursor).map_err(map_follow_up_ooxml_error)
}

/// Structural operations may only relabel or add Road part rows.
pub(super) fn validate_structure(ops: &[RowOp]) -> Result<(), BackendError> {
    if ops.len() > MAX_STRUCTURE_OPS {
        return Err(workbook_invalid());
    }
    let label_ok = |text: &str| {
        let refs = road_refs(text);
        matches!(refs.as_slice(), [reference] if road_label(reference.road, reference.part) == text)
    };
    for op in ops {
        let valid = match op {
            RowOp::SetText {
                sheet,
                row,
                column,
                text,
            } => {
                sheet.eq_ignore_ascii_case(ROAD_SHEET) && *row > 0 && *column == 2 && label_ok(text)
            }
            RowOp::InsertRow {
                sheet,
                after_row,
                template_row,
                copy_columns,
                texts,
            } => {
                sheet.eq_ignore_ascii_case(ROAD_SHEET)
                    && *template_row > 0
                    && *template_row <= *after_row + 1
                    && copy_columns.iter().all(|column| *column == 1)
                    && texts.len() == 1
                    && texts
                        .first()
                        .is_some_and(|entry| entry.column == 2 && label_ok(&entry.text))
            }
        };
        if !valid {
            return Err(workbook_invalid());
        }
    }
    Ok(())
}

/// Informational (unselectable) review rows, one per structural change.
pub(super) fn structure_review_rows(ops: &[RowOp], layouts: &[FollowUpLayout]) -> Vec<FollowUpRow> {
    let Some(layout) = layouts.iter().find(|layout| {
        layout
            .worksheet
            .name
            .trim()
            .eq_ignore_ascii_case(ROAD_SHEET)
    }) else {
        return Vec::new();
    };
    ops.iter()
        .enumerate()
        .map(|(index, op)| {
            let (row, label, reason) = match op {
                RowOp::SetText { row, text, .. } => (
                    *row,
                    text.clone(),
                    "warn.rfi.followUp.reason.structureRename",
                ),
                RowOp::InsertRow {
                    after_row, texts, ..
                } => (
                    *after_row + 1,
                    texts
                        .first()
                        .map(|entry| entry.text.clone())
                        .unwrap_or_default(),
                    "warn.rfi.followUp.reason.structureInsert",
                ),
            };
            FollowUpRow {
                row_id: format!("rfi-follow-up-structure:{index}"),
                sheet: ROAD_SHEET.to_string(),
                row,
                rfi_no: label.clone(),
                source_statuses: Vec::new(),
                current_status: String::new(),
                proposed_status: None,
                proposed_rfi_no: None,
                disposition: FollowUpDisposition::Ready,
                reason_key: reason.to_string(),
                selectable: false,
                enabled: false,
                remark: None,
                formula_fixes: Vec::new(),
                target: FollowUpReviewTarget {
                    worksheet: layout.worksheet.clone(),
                    row,
                    rfi_column: 2,
                    status_column: 2,
                    rfi_cell: None,
                    status_cell: None,
                },
                source_evidence: Vec::new(),
                mapping_details: Some(label),
            }
        })
        .collect()
}
