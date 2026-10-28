use super::*;

/// Queues one manual-review row for a signed record that has no placement.
/// Falls back from `preferred` to a sheet-name match, then to the first layout.
fn queue_unplaced_final_review(
    record: &SourceRecord,
    index: &RecordIndex,
    (target_sheet, target_clause): (&str, &str),
    preferred: Option<&FollowUpLayout>,
    manual_rows: &mut Vec<FollowUpRow>,
    unmapped_count: &mut usize,
    reason_key: Option<&str>,
) {
    let final_review_id = format!("rfi-follow-up-unmapped:{}", record.canonical_rfi);
    if manual_rows.iter().any(|row| row.row_id == final_review_id) {
        return;
    }
    let Some(layout) = preferred
        .or_else(|| {
            index.layouts.iter().find(|layout| {
                layout
                    .worksheet
                    .name
                    .trim()
                    .eq_ignore_ascii_case(target_sheet)
            })
        })
        .or_else(|| index.layouts.first())
    else {
        return;
    };
    *unmapped_count += 1;
    let (rfi_column, status_column) = default_signed_col(target_sheet, target_clause);
    let mut review = missing_final_review(
        record,
        layout,
        target_sheet,
        target_clause,
        (rfi_column, status_column),
        &[],
        index.target_workbook,
    );
    if let Some(key) = reason_key {
        review.reason_key = key.to_string();
    }
    manual_rows.push(review);
}

/// Builds the manual-review row for a signed record whose target slot is unsafe to edit.
fn unsafe_signed_slot_row(
    record: &SourceRecord,
    slot: &SlotWork,
    sheet: &str,
    reason: &str,
) -> FollowUpRow {
    FollowUpRow {
        row_id: format!(
            "rfi-follow-up-signed-unsafe:{}:{}:{}",
            record.canonical_rfi, slot.row, slot.layout.rfi_column
        ),
        sheet: sheet.to_string(),
        row: slot.row,
        rfi_no: record.short_rfi.clone(),
        source_statuses: vec![signed_status_text(record).to_string()],
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

/// Routes one signed-by record onto its target slots or queues a manual-review row.
pub(super) fn place_signed_record<'a>(
    record: &'a SourceRecord,
    index: &RecordIndex,
    slots: &mut SlotMap,
    manual_rows: &mut Vec<FollowUpRow>,
    unmapped_count: &mut usize,
    routed_signed_targets: &mut BTreeMap<&'a str, BTreeSet<(u32, u32, u32, u32)>>,
) -> Result<(), BackendError> {
    let RecordIndex {
        layouts,
        records,
        target_workbook,
        ..
    } = index;
    let (target_sheet, target_clause, match_layout) =
        signed_route_clause(record).expect("filtered final record has a signed route");
    let matching_layout = layouts.iter().find(|l| {
        l.worksheet.name.trim().eq_ignore_ascii_case(target_sheet)
            && (l.clause.as_deref() == Some(target_clause)
                || (target_sheet.trim().to_uppercase().contains("PCU") && l.status_column == 18)
                || (l.clause.is_none()
                    && l.rfi_column == default_signed_col(target_sheet, target_clause).0))
    });
    let final_review_id = format!("rfi-follow-up-unmapped:{}", record.canonical_rfi);
    if record.state.is_none() {
        queue_unplaced_final_review(
            record,
            index,
            (target_sheet, target_clause),
            matching_layout,
            manual_rows,
            unmapped_count,
            Some("warn.rfi.followUp.reason.sourceStatusUnknown"),
        );
        return Ok(());
    }
    let Some(layout) = matching_layout else {
        queue_unplaced_final_review(
            record,
            index,
            (target_sheet, target_clause),
            None,
            manual_rows,
            unmapped_count,
            None,
        );
        return Ok(());
    };
    let sheet_name = layout.worksheet.name.as_str();
    let rfi_column = layout.rfi_column;
    let status_column = layout.status_column;
    let Some(matching_sheet) = target_workbook
        .sheet(sheet_name)
        .or_else(|| find_sheet_flexible(target_workbook, sheet_name))
    else {
        return Ok(());
    };
    let candidate_keys = slots
        .iter()
        .filter(|((sheet, _, column, status), _)| {
            sheet == sheet_name && *column == rfi_column && *status == status_column
        })
        .filter(|((_, row, _, _), _)| {
            row_matches(match_layout, matching_sheet, *row, record, target_workbook)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let candidate_count = candidate_keys.len();
    let has_unrelated_placement = slots.iter().any(|(key, slot)| {
        key.0 == sheet_name
            && key.2 == rfi_column
            && key.3 == status_column
            && slot.codes.contains(&record.canonical_rfi)
            && !candidate_keys.contains(key)
    });
    if candidate_count == 0 {
        return Ok(());
    }
    let coverage = signed_destination_coverage(sheet_name, matching_sheet, &candidate_keys, record);
    let can_process_candidates = !has_unrelated_placement && coverage.valid;
    if can_process_candidates {
        for candidate_key in &candidate_keys {
            let slot = slots
                .get_mut(candidate_key)
                .expect("signed target candidate remains available");
            if sheet_name == "PCU" && slot.codes.is_empty() {
                continue;
            }
            if slot.unsafe_reason == Some("targetFormula")
                && slot.codes.contains(&record.canonical_rfi)
                && slot.current_status == signed_status_text(record)
            {
                routed_signed_targets
                    .entry(record.canonical_rfi.as_str())
                    .or_default()
                    .insert((
                        slot.layout.worksheet.sheet_id,
                        slot.row,
                        slot.layout.rfi_column,
                        slot.layout.status_column,
                    ));
                continue;
            }
            if let Some(reason) = slot.unsafe_reason {
                *unmapped_count += 1;
                manual_rows.push(unsafe_signed_slot_row(record, slot, sheet_name, reason));
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
            routed_signed_targets
                .entry(record.canonical_rfi.as_str())
                .or_default()
                .insert((
                    slot.layout.worksheet.sheet_id,
                    slot.row,
                    slot.layout.rfi_column,
                    slot.layout.status_column,
                ));
        }
        if coverage.missing_contexts && !manual_rows.iter().any(|row| row.row_id == final_review_id)
        {
            *unmapped_count += 1;
            let mut review = missing_final_review(
                record,
                layout,
                target_sheet,
                target_clause,
                (rfi_column, status_column),
                &candidate_keys,
                target_workbook,
            );
            review.reason_key = "warn.rfi.followUp.reason.partialContext".to_string();
            manual_rows.push(review);
        }
        return Ok(());
    }
    let Some(layout) = layouts
        .iter()
        .find(|layout| {
            layout.worksheet.name == sheet_name
                && layout.rfi_column == rfi_column
                && layout.status_column == status_column
        })
        .cloned()
    else {
        return Ok(());
    };
    *unmapped_count += 1;
    let reason_key = if candidate_count == 0 {
        "warn.rfi.followUp.reason.mappingMissing"
    } else {
        "warn.rfi.followUp.reason.mappingAmbiguous"
    };
    let mut row = manual_review_row(
        format!("rfi-follow-up-signed-unmapped:{}", record.canonical_rfi),
        sheet_name,
        &layout,
        record,
        reason_key.to_string(),
        Some(mapping_details(
            record,
            Some((target_sheet, target_clause)),
            target_workbook,
            &candidate_keys,
        )),
    );
    row.source_statuses = vec![signed_status_text(record).to_string()];
    manual_rows.push(row);
    Ok(())
}

pub(super) fn refresh_signed_slots(
    slots: &mut SlotMap,
    records: &BTreeMap<String, SourceRecord>,
    routed_signed_targets: &BTreeMap<&str, BTreeSet<(u32, u32, u32, u32)>>,
) {
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
            if let Some(record) = records.get(code)
                && routed_signed_targets
                    .get(code.as_str())
                    .is_some_and(|targets| {
                        targets.contains(&(
                            slot.layout.worksheet.sheet_id,
                            slot.row,
                            slot.layout.rfi_column,
                            slot.layout.status_column,
                        ))
                    })
                && signed_status_text(record) != slot.current_status
            {
                slot.changed_rfis.insert(code.clone());
            }
        }
    }
}

/// Rows for signed-by slots whose sources were all placed on their routed targets.
pub(super) fn signed_slot_rows(
    slots: &SlotMap,
    records: &BTreeMap<String, SourceRecord>,
    routed_signed_targets: &BTreeMap<&str, BTreeSet<(u32, u32, u32, u32)>>,
    manual_rows: &mut Vec<FollowUpRow>,
    unmapped_count: &mut usize,
) -> Vec<FollowUpRow> {
    let mut rows = Vec::new();
    for slot in slots
        .values()
        .filter(|slot| is_signed_layout(&slot.layout))
        .cloned()
    {
        let has_unmatched_source = slot
            .codes
            .iter()
            .filter(|code| !is_placeholder_rfi(code))
            .any(|code| {
                records.get(code).is_some_and(|record| {
                    record.state.is_none()
                        || !routed_signed_targets
                            .get(code.as_str())
                            .is_some_and(|targets| {
                                targets.contains(&(
                                    slot.layout.worksheet.sheet_id,
                                    slot.row,
                                    slot.layout.rfi_column,
                                    slot.layout.status_column,
                                ))
                            })
                })
            });
        if has_unmatched_source {
            continue;
        }
        if slot.unsafe_reason.is_some() && !slot.changed_rfis.is_empty() {
            if let Some(row) = unsafe_slot_review(&slot, records) {
                *unmapped_count += 1;
                manual_rows.push(row);
            }
            continue;
        }
        if let Some(row) = row_from_slot(slot, records, true) {
            rows.push(row);
        }
    }
    rows
}
