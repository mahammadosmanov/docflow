use super::*;

mod dispatch;
pub use dispatch::*;
mod placement;
use placement::*;
mod signed;
use signed::*;

/// Builds a manual-review follow-up row that cannot be applied automatically.
fn manual_review_row(
    row_id: String,
    sheet_name: &str,
    layout: &FollowUpLayout,
    record: &SourceRecord,
    reason_key: String,
    mapping: Option<String>,
) -> FollowUpRow {
    FollowUpRow {
        row_id,
        sheet: sheet_name.to_string(),
        row: 0,
        rfi_no: record.short_rfi.clone(),
        source_statuses: vec![record.inspection_status_for_ui()],
        current_status: String::new(),
        proposed_status: None,
        proposed_rfi_no: None,
        disposition: FollowUpDisposition::ManualReview,
        reason_key,
        selectable: false,
        enabled: false,
        remark: None,
        formula_fixes: Vec::new(),
        target: FollowUpReviewTarget {
            worksheet: layout.worksheet.clone(),
            row: 0,
            rfi_column: layout.rfi_column,
            status_column: layout.status_column,
            rfi_cell: None,
            status_cell: None,
        },
        source_evidence: record.evidence.clone(),
        mapping_details: mapping,
    }
}

type SlotKey = (String, u32, u32, u32);
type SlotMap = BTreeMap<SlotKey, SlotWork>;
type SlotKeysBySheetCol = BTreeMap<(String, u32), Vec<SlotKey>>;
type SlotKeysByCode = BTreeMap<String, Vec<SlotKey>>;

/// Indexes non-signed slots by code and by (sheet, column) so the record pass can
/// find candidate slots without rescanning the whole map.
fn index_slots(slots: &SlotMap) -> (SlotKeysBySheetCol, SlotKeysByCode) {
    let mut potential_slots_by_sheet_col = SlotKeysBySheetCol::new();
    let mut existing_slot_keys_by_code = SlotKeysByCode::new();
    for (key, slot) in slots {
        if !is_signed_layout(&slot.layout) {
            for code in &slot.codes {
                existing_slot_keys_by_code
                    .entry(code.clone())
                    .or_default()
                    .push(key.clone());
            }
            potential_slots_by_sheet_col
                .entry((key.0.clone(), key.2))
                .or_default()
                .push(key.clone());
        }
    }
    (potential_slots_by_sheet_col, existing_slot_keys_by_code)
}

fn build_slots(
    layouts: &[FollowUpLayout],
    target_workbook: &WorkbookData,
    merge_budget: &mut MergeQueryBudget,
) -> Result<SlotMap, BackendError> {
    let mut slots = SlotMap::new();
    for layout in layouts {
        let sheet = target_workbook
            .sheet(&layout.worksheet.name)
            .ok_or_else(workbook_invalid)?;
        let last = sheet
            .max_row
            .min(layout.header_row + MAX_FOLLOW_UP_DATA_ROWS);
        for row in (layout.header_row + 1)..=last {
            if let Some(slot) = safe_slot(layout, sheet, row, merge_budget)? {
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
    Ok(slots)
}

fn deletion_evidence(
    slots: &SlotMap,
    records: &BTreeMap<String, SourceRecord>,
    logged_numbers: &BTreeSet<String>,
) -> (usize, bool) {
    let deleted_count = slots
        .values()
        .flat_map(|slot| slot.codes.iter())
        .filter(|code| {
            !is_placeholder_rfi(code)
                && !records.contains_key(*code)
                && !logged_numbers.contains(*code)
        })
        .count();
    let total_codes = slots
        .values()
        .flat_map(|slot| slot.codes.iter())
        .filter(|code| !is_placeholder_rfi(code))
        .count();
    // Removing a large share of Follow Up's RFIs means the wrong or an outdated RFI Log
    // was selected; refuse to remove anything on that evidence.
    let mass_deletion = deleted_count > MASS_DELETION_FLOOR
        && deleted_count * MASS_DELETION_DENOMINATOR > total_codes;
    (deleted_count, mass_deletion)
}

/// Drops RFIs whose log state removes them from the slot and marks slots whose
/// lifecycle status changed.
fn prune_slot_codes(
    slot: &mut SlotWork,
    records: &BTreeMap<String, SourceRecord>,
    logged_numbers: &BTreeSet<String>,
    mass_deletion: bool,
    target_workbook: &WorkbookData,
) {
    let removed = slot
        .codes
        .iter()
        .filter(|code| {
            !is_placeholder_rfi(code)
                && match records.get(*code) {
                    Some(record) => {
                        record.excluded_civ_pcs
                            || is_confirmed_follow_up_exclusion(record)
                            || record.state == Some(LogState::Remove)
                    }
                    // Deleted from the Log: absent even as a bare number.
                    None => !mass_deletion && !logged_numbers.contains(*code),
                }
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
            .all(|code| {
                records.get(code).is_some_and(|record| {
                    record_matches_lifecycle_slot(slot, record, target_workbook)
                })
            })
        && slot
            .codes
            .iter()
            .filter(|code| !is_placeholder_rfi(code))
            .any(|code| records.get(code).and_then(|record| record.state).is_some())
    {
        let next = main_status_for_codes(&slot.codes, records);
        if next
            .as_deref()
            .is_some_and(|status| status != slot.current_status)
        {
            slot.changed_rfis.extend(slot.codes.iter().cloned());
        }
    }
}

/// Manual-review row for a slot whose source record is missing or has an unknown status.
fn source_issue_row(slot: &SlotWork, record: &SourceRecord, reason: &str) -> FollowUpRow {
    FollowUpRow {
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
        remark: None,
        formula_fixes: Vec::new(),
        target: FollowUpReviewTarget {
            worksheet: slot.layout.worksheet.clone(),
            row: slot.row,
            rfi_column: slot.layout.rfi_column,
            status_column: slot.layout.status_column,
            rfi_cell: Some(slot.rfi_cell.clone()),
            status_cell: Some(slot.status_cell.clone()),
        },
        source_evidence: record.evidence.clone(),
        mapping_details: None,
    }
}

fn projection_warnings(
    unmapped_count: usize,
    mass_deletion: bool,
    deleted_count: usize,
    no_rows: bool,
) -> Vec<Warning> {
    let mut warnings = Vec::new();
    if unmapped_count > 0 {
        warnings.push(Warning {
            key: "warn.rfi.followUp.unmappedRecords".to_string(),
            subject: unmapped_count.to_string(),
        });
    }
    if mass_deletion {
        warnings.push(Warning {
            key: "warn.rfi.followUp.massDeletionBrake".to_string(),
            subject: deleted_count.to_string(),
        });
    }
    if no_rows {
        warnings.push(Warning {
            key: "warn.rfi.followUp.noRowsSelected".to_string(),
            subject: String::new(),
        });
    }
    warnings
}

pub(super) fn classify_workbooks_full(
    source_discovery: &OrderedWorkbookDiscovery,
    source_workbook: &WorkbookData,
    target_discovery: &OrderedWorkbookDiscovery,
    target_workbook: &WorkbookData,
    target_has_signature: bool,
) -> Result<Value, BackendError> {
    let records = scan_source_records(source_discovery, source_workbook)?;
    let logged_numbers = scan_logged_rfi_numbers(source_discovery, source_workbook);
    let mut merge_budget = MergeQueryBudget {
        remaining: MAX_FOLLOW_UP_MERGE_COMPARISONS,
    };
    let layouts = discover_approved_layouts(
        target_discovery,
        target_workbook,
        target_has_signature,
        &mut merge_budget,
    )?;
    let mut slots = build_slots(&layouts, target_workbook, &mut merge_budget)?;
    let (deleted_count, mass_deletion) = deletion_evidence(&slots, &records, &logged_numbers);
    let mut manual_rows = Vec::new();
    for slot in slots.values_mut() {
        prune_slot_codes(
            slot,
            &records,
            &logged_numbers,
            mass_deletion,
            target_workbook,
        );
    }

    let (potential_slots_by_sheet_col, existing_slot_keys_by_code) = index_slots(&slots);

    let mut unmapped_count = 0usize;
    let index = RecordIndex {
        layouts: &layouts,
        records: &records,
        target_workbook,
        potential_slots_by_sheet_col: &potential_slots_by_sheet_col,
        existing_slot_keys_by_code: &existing_slot_keys_by_code,
    };
    for record in records.values().filter(|record| {
        !record.excluded_civ_pcs
            && !is_confirmed_follow_up_exclusion(record)
            && record.state != Some(LogState::Remove)
    }) {
        place_record(
            record,
            &index,
            &mut slots,
            &mut manual_rows,
            &mut unmapped_count,
        )?;
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
            manual_rows.push(source_issue_row(&slot, record, reason));
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

    let mut routed_signed_targets = BTreeMap::<&str, BTreeSet<(u32, u32, u32, u32)>>::new();
    for record in records.values().filter(|record| {
        !record.excluded_civ_pcs
            && !is_confirmed_follow_up_exclusion(record)
            && record.state != Some(LogState::Remove)
            && signed_route_clause(record).is_some()
    }) {
        place_signed_record(
            record,
            &index,
            &mut slots,
            &mut manual_rows,
            &mut unmapped_count,
            &mut routed_signed_targets,
        )?;
    }

    refresh_signed_slots(&mut slots, &records, &routed_signed_targets);
    rows.extend(signed_slot_rows(
        &slots,
        &records,
        &routed_signed_targets,
        &mut manual_rows,
        &mut unmapped_count,
    ));
    rows.extend(manual_rows);
    rows.extend(remark_review_rows(
        &slots,
        &records,
        target_workbook,
        &mut merge_budget,
    )?);
    rows.extend(formula_repair_rows(&layouts, target_workbook));

    if rows.len() > MAX_PREVIEW_ROWS {
        return Err(limit_exceeded_for(format!(
            "{} preview rows; at most {MAX_PREVIEW_ROWS}",
            rows.len()
        )));
    }
    rows.sort_by(|left, right| {
        left.sheet
            .cmp(&right.sheet)
            .then(left.row.cmp(&right.row))
            .then(left.target.rfi_column.cmp(&right.target.rfi_column))
    });
    let warnings = projection_warnings(
        unmapped_count,
        mass_deletion,
        deleted_count,
        rows.is_empty(),
    );
    let columns = editor_columns();
    let primary_layout = layouts.first().ok_or_else(workbook_invalid)?.clone();
    let editor = editor_metadata(&rows, columns);
    let projection = json!({"layout": primary_layout, "layouts": layouts, "rows": rows, "warnings": warnings, "editor": editor});
    validate_projection_budget(&projection)?;
    Ok(projection)
}
