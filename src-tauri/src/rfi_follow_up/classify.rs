use super::*;

mod dispatch;
pub use dispatch::*;

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

/// Places one source record onto the slots it belongs to, or queues a manual-review row.
struct RecordIndex<'a> {
    layouts: &'a [FollowUpLayout],
    records: &'a BTreeMap<String, SourceRecord>,
    target_workbook: &'a WorkbookData,
    potential_slots_by_sheet_col: &'a SlotKeysBySheetCol,
    existing_slot_keys_by_code: &'a SlotKeysByCode,
}

fn place_record(
    record: &SourceRecord,
    index: &RecordIndex,
    slots: &mut SlotMap,
    manual_rows: &mut Vec<FollowUpRow>,
    unmapped_count: &mut usize,
) -> Result<(), BackendError> {
    let RecordIndex {
        layouts,
        records,
        target_workbook,
        potential_slots_by_sheet_col,
        existing_slot_keys_by_code,
    } = index;
    let Some((target_sheet, target_clause)) = route_clause(record) else {
        if slots
            .values()
            .any(|slot| slot.codes.contains(&record.canonical_rfi))
        {
            return Ok(());
        }
        *unmapped_count += 1;
        if let Some(layout) = layouts.iter().find(|layout| {
            layout
                .worksheet
                .name
                .trim()
                .eq_ignore_ascii_case(record.short_rfi.split('-').next().unwrap_or_default())
        }) {
            manual_rows.push(manual_review_row(
                format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
                &layout.worksheet.name,
                layout,
                record,
                if record.state.is_none() {
                    "warn.rfi.followUp.reason.sourceStatusUnknown".to_string()
                } else {
                    "warn.rfi.followUp.reason.mappingUnsupported".to_string()
                },
                Some(mapping_details(record, None, target_workbook, &[])),
            ));
        }
        return Ok(());
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
        // FINAL routes are reconciled separately against Signed By rows.
        if signed_route_clause(record).is_some() {
            return Ok(());
        }
        *unmapped_count += 1;
        if let Some(layout) = layouts.iter().find(|layout| {
            layout
                .worksheet
                .name
                .trim()
                .eq_ignore_ascii_case(target_sheet)
        }) {
            let reason_key = if record.state.is_none() {
                "warn.rfi.followUp.reason.sourceStatusUnknown".to_string()
            } else {
                "warn.rfi.followUp.reason.mappingMissing".to_string()
            };
            let mut row = manual_review_row(
                format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
                target_sheet,
                layout,
                record,
                reason_key,
                Some(mapping_details(
                    record,
                    Some((target_sheet, target_clause)),
                    target_workbook,
                    &[],
                )),
            );
            row.target.rfi_column = default_route_col(target_sheet, target_clause);
            row.target.status_column = default_route_col(target_sheet, target_clause) + 1;
            manual_rows.push(row);
        }
        return Ok(());
    };
    let sheet_name = layout.worksheet.name.as_str();
    let rfi_column = layout.rfi_column;
    let empty_potential = Vec::new();
    let potential = potential_slots_by_sheet_col
        .get(&(sheet_name.to_string(), rfi_column))
        .unwrap_or(&empty_potential);
    let target_sheet_data = target_workbook.sheet(sheet_name).unwrap();
    let mut scope_candidates = Vec::new();
    let mut candidates = Vec::new();
    for key in potential {
        if row_matches(
            sheet_name,
            target_sheet_data,
            key.1,
            record,
            target_workbook,
        ) {
            scope_candidates.push(key.clone());
            if slots
                .get(key)
                .is_some_and(|slot| slot.unsafe_reason.is_none())
            {
                candidates.push(key.clone());
            }
        }
    }
    let candidate_count = candidates.len();
    let civ_tag_coverage = (sheet_name == "CIV")
        .then(|| civ_tag_coverage(target_workbook, &candidates, record))
        .flatten();
    let mec_context_coverage = (sheet_name == "MEC")
        .then(|| mec_context_coverage(target_workbook, sheet_name, &candidates, record))
        .flatten()
        .or_else(|| {
            // Road tags are checked the same way: every named Road/part needs a row.
            (sheet_name == "Road")
                .then(|| road_context_coverage(target_workbook, &candidates, record))
                .flatten()
        });
    // Signed By rows repeat RFIs as Worley summaries, not lifecycle placements.
    let existing_slots = existing_slot_keys_by_code
        .get(record.canonical_rfi.as_str())
        .cloned()
        .unwrap_or_default();
    if scope_candidates.is_empty() {
        return Ok(());
    }
    if candidate_count == 1
        && !existing_slots.is_empty()
        && mec_context_coverage != Some(false)
        && civ_tag_coverage != Some(false)
    {
        return Ok(());
    }
    let exact_multi_area = candidate_count > 1
        && ((sheet_name == "CIV"
            && (distinct_civ_area_rows(target_workbook, &candidates, record)
                || distinct_civ_zone_area_rows(target_workbook, &candidates, record)
                || civ_tag_coverage == Some(true)))
            || (sheet_name == "MEC" && mec_context_coverage.is_some())
            // A Road tag without a part intentionally fans out to every part row.
            || sheet_name == "Road");
    if (candidate_count == 0 || candidate_count > 1 && !exact_multi_area)
        && (candidate_count == 0 || existing_slots.is_empty())
    {
        *unmapped_count += 1;
        let Some(layout) = layouts
            .iter()
            .find(|layout| layout.worksheet.name == sheet_name && layout.rfi_column == rfi_column)
            .cloned()
        else {
            *unmapped_count -= 1;
            return Ok(());
        };
        manual_rows.push(manual_review_row(
            format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
            sheet_name,
            &layout,
            record,
            if record.state.is_none() {
                "warn.rfi.followUp.reason.sourceStatusUnknown".to_string()
            } else if candidate_count == 0 {
                scope_candidates
                    .iter()
                    .find_map(|key| slots.get(key).and_then(|slot| slot.unsafe_reason))
                    .map_or_else(
                        || "warn.rfi.followUp.reason.mappingMissing".to_string(),
                        |reason| format!("warn.rfi.followUp.reason.{reason}"),
                    )
            } else {
                "warn.rfi.followUp.reason.mappingAmbiguous".to_string()
            },
            Some(mapping_details(
                record,
                Some((sheet_name, target_clause)),
                target_workbook,
                &scope_candidates,
            )),
        ));
        return Ok(());
    }
    let already_mapped_elsewhere = existing_slots.iter().any(|key| !candidates.contains(key));
    if already_mapped_elsewhere {
        *unmapped_count += 1;
        let layout = slots
            .get(&candidates[0])
            .map(|slot| slot.layout.clone())
            .ok_or_else(workbook_invalid)?;
        manual_rows.push(manual_review_row(
            format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
            sheet_name,
            &layout,
            record,
            "warn.rfi.followUp.reason.mappingAmbiguous".to_string(),
            Some(mapping_details(
                record,
                Some((sheet_name, target_clause)),
                target_workbook,
                &scope_candidates,
            )),
        ));
        return Ok(());
    }
    if mec_context_coverage == Some(false) || civ_tag_coverage == Some(false) {
        *unmapped_count += 1;
        let layout = slots
            .get(&candidates[0])
            .map(|slot| slot.layout.clone())
            .ok_or_else(workbook_invalid)?;
        manual_rows.push(manual_review_row(
            format!(
                "rfi-follow-up-unmapped:{}:missing-context",
                record.canonical_rfi
            ),
            sheet_name,
            &layout,
            record,
            if record.state.is_none() {
                "warn.rfi.followUp.reason.sourceStatusUnknown".to_string()
            } else {
                "warn.rfi.followUp.reason.partialContext".to_string()
            },
            Some(mapping_details(
                record,
                Some((sheet_name, target_clause)),
                target_workbook,
                &scope_candidates,
            )),
        ));
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
    Ok(())
}

/// Routes one signed-by record onto its target slots or queues a manual-review row.
fn place_signed_record<'a>(
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
        if !manual_rows.iter().any(|row| row.row_id == final_review_id)
            && let Some(layout) = matching_layout
                .or_else(|| {
                    layouts.iter().find(|layout| {
                        layout
                            .worksheet
                            .name
                            .trim()
                            .eq_ignore_ascii_case(target_sheet)
                    })
                })
                .or_else(|| layouts.first())
        {
            *unmapped_count += 1;
            let (rfi_column, status_column) = default_signed_col(target_sheet, target_clause);
            let mut review = missing_final_review(
                record,
                layout,
                target_sheet,
                target_clause,
                (rfi_column, status_column),
                &[],
                target_workbook,
            );
            review.reason_key = "warn.rfi.followUp.reason.sourceStatusUnknown".to_string();
            manual_rows.push(review);
        }
        return Ok(());
    }
    let Some(layout) = matching_layout else {
        if !manual_rows.iter().any(|row| row.row_id == final_review_id)
            && let Some(layout) = layouts
                .iter()
                .find(|layout| {
                    layout
                        .worksheet
                        .name
                        .trim()
                        .eq_ignore_ascii_case(target_sheet)
                })
                .or_else(|| layouts.first())
        {
            *unmapped_count += 1;
            let (rfi_column, status_column) = default_signed_col(target_sheet, target_clause);
            manual_rows.push(missing_final_review(
                record,
                layout,
                target_sheet,
                target_clause,
                (rfi_column, status_column),
                &[],
                target_workbook,
            ));
        }
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
                && slot.current_status
                    == if record.worley_signed {
                        "Signed"
                    } else {
                        "Not Signed"
                    }
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
    *unmapped_count += 1;
    let Some(layout) = layouts
        .iter()
        .find(|layout| {
            layout.worksheet.name == sheet_name
                && layout.rfi_column == rfi_column
                && layout.status_column == status_column
        })
        .cloned()
    else {
        *unmapped_count -= 1;
        return Ok(());
    };
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
    row.source_statuses = vec![if record.worley_signed {
        "Signed".to_string()
    } else {
        "Not Signed".to_string()
    }];
    manual_rows.push(row);
    Ok(())
}

fn refresh_signed_slots(
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
            {
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
}

/// Rows for signed-by slots whose sources were all placed on their routed targets.
fn signed_slot_rows(
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
