use super::*;

pub(super) fn classify_workbooks_full(
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
                        record.excluded_civ_pcs
                            || is_confirmed_follow_up_exclusion(record)
                            || record.state == Some(LogState::Remove)
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
            let next = main_status_for_codes(&slot.codes, &records);
            if next
                .as_deref()
                .is_some_and(|status| status != slot.current_status)
            {
                slot.changed_rfis.extend(slot.codes.iter().cloned());
            }
        }
    }

    let mut potential_slots_by_sheet_col =
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
            potential_slots_by_sheet_col
                .entry((key.0.clone(), key.2))
                .or_default()
                .push(key.clone());
        }
    }

    let mut unmapped_count = 0usize;
    for record in records.values().filter(|record| {
        !record.excluded_civ_pcs
            && !is_confirmed_follow_up_exclusion(record)
            && record.state != Some(LogState::Remove)
    }) {
        let Some((target_sheet, target_clause)) = route_clause(record) else {
            if slots
                .values()
                .any(|slot| slot.codes.contains(&record.canonical_rfi))
            {
                continue;
            }
            unmapped_count += 1;
            if let Some(layout) =
                layouts.iter().find(|layout| {
                    layout.worksheet.name.trim().eq_ignore_ascii_case(
                        record.short_rfi.split('-').next().unwrap_or_default(),
                    )
                })
            {
                manual_rows.push(FollowUpRow {
                    row_id: format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
                    sheet: layout.worksheet.name.clone(),
                    row: 0,
                    rfi_no: record.short_rfi.clone(),
                    source_statuses: vec![record.inspection_status_for_ui()],
                    current_status: String::new(),
                    proposed_status: None,
                    proposed_rfi_no: None,
                    disposition: FollowUpDisposition::ManualReview,
                    reason_key: if record.state.is_none() {
                        "warn.rfi.followUp.reason.sourceStatusUnknown".to_string()
                    } else {
                        "warn.rfi.followUp.reason.mappingUnsupported".to_string()
                    },
                    selectable: false,
                    enabled: false,
                    target: FollowUpReviewTarget {
                        worksheet: layout.worksheet.clone(),
                        row: 0,
                        rfi_column: layout.rfi_column,
                        status_column: layout.status_column,
                        rfi_cell: None,
                        status_cell: None,
                    },
                    source_evidence: record.evidence.clone(),
                    mapping_details: Some(mapping_details(record, None, target_workbook, &[])),
                });
            }
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
            // FINAL routes are reconciled separately against Signed By rows.
            if signed_route_clause(record).is_some() {
                continue;
            }
            unmapped_count += 1;
            if let Some(layout) = layouts.iter().find(|layout| {
                layout
                    .worksheet
                    .name
                    .trim()
                    .eq_ignore_ascii_case(target_sheet)
            }) {
                manual_rows.push(FollowUpRow {
                    row_id: format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
                    sheet: target_sheet.to_string(),
                    row: 0,
                    rfi_no: record.short_rfi.clone(),
                    source_statuses: vec![record.inspection_status_for_ui()],
                    current_status: String::new(),
                    proposed_status: None,
                    proposed_rfi_no: None,
                    disposition: FollowUpDisposition::ManualReview,
                    reason_key: if record.state.is_none() {
                        "warn.rfi.followUp.reason.sourceStatusUnknown".to_string()
                    } else {
                        "warn.rfi.followUp.reason.mappingMissing".to_string()
                    },
                    selectable: false,
                    enabled: false,
                    target: FollowUpReviewTarget {
                        worksheet: layout.worksheet.clone(),
                        row: 0,
                        rfi_column: default_route_col(target_sheet, target_clause),
                        status_column: default_route_col(target_sheet, target_clause) + 1,
                        rfi_cell: None,
                        status_cell: None,
                    },
                    source_evidence: record.evidence.clone(),
                    mapping_details: Some(mapping_details(
                        record,
                        Some((target_sheet, target_clause)),
                        target_workbook,
                        &[],
                    )),
                });
            }
            continue;
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
            .flatten();
        // Signed By rows repeat RFIs as Worley summaries, not lifecycle placements.
        let existing_slots = existing_slot_keys_by_code
            .get(record.canonical_rfi.as_str())
            .cloned()
            .unwrap_or_default();
        if scope_candidates.is_empty() {
            continue;
        }
        if candidate_count == 1
            && !existing_slots.is_empty()
            && mec_context_coverage != Some(false)
            && civ_tag_coverage != Some(false)
        {
            continue;
        }
        let exact_multi_area = candidate_count > 1
            && ((sheet_name == "CIV"
                && (distinct_civ_area_rows(target_workbook, &candidates, record)
                    || distinct_civ_zone_area_rows(target_workbook, &candidates, record)
                    || civ_tag_coverage == Some(true)))
                || (sheet_name == "MEC" && mec_context_coverage.is_some()));
        if (candidate_count == 0 || candidate_count > 1 && !exact_multi_area)
            && (candidate_count == 0 || existing_slots.is_empty())
        {
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
                reason_key: if record.state.is_none() {
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
                mapping_details: Some(mapping_details(
                    record,
                    Some((sheet_name, target_clause)),
                    target_workbook,
                    &scope_candidates,
                )),
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
                mapping_details: Some(mapping_details(
                    record,
                    Some((sheet_name, target_clause)),
                    target_workbook,
                    &scope_candidates,
                )),
            });
            continue;
        }
        if mec_context_coverage == Some(false) || civ_tag_coverage == Some(false) {
            unmapped_count += 1;
            let layout = slots
                .get(&candidates[0])
                .map(|slot| slot.layout.clone())
                .ok_or_else(workbook_invalid)?;
            manual_rows.push(FollowUpRow {
                row_id: format!(
                    "rfi-follow-up-unmapped:{}:missing-context",
                    record.canonical_rfi
                ),
                sheet: sheet_name.to_string(),
                row: 0,
                rfi_no: record.short_rfi.clone(),
                source_statuses: vec![record.inspection_status_for_ui()],
                current_status: String::new(),
                proposed_status: None,
                proposed_rfi_no: None,
                disposition: FollowUpDisposition::ManualReview,
                reason_key: if record.state.is_none() {
                    "warn.rfi.followUp.reason.sourceStatusUnknown".to_string()
                } else {
                    "warn.rfi.followUp.reason.partialContext".to_string()
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
                mapping_details: Some(mapping_details(
                    record,
                    Some((sheet_name, target_clause)),
                    target_workbook,
                    &scope_candidates,
                )),
            });
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
                mapping_details: None,
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

    let mut routed_signed_targets = BTreeMap::<&str, BTreeSet<(u32, u32, u32, u32)>>::new();
    for record in records.values().filter(|record| {
        !record.excluded_civ_pcs
            && !is_confirmed_follow_up_exclusion(record)
            && record.state != Some(LogState::Remove)
            && signed_route_clause(record).is_some()
    }) {
        let (target_sheet, target_clause, match_layout) =
            signed_route_clause(record).expect("filtered final record has a signed route");
        let matching_layout = layouts.iter().find(|l| {
            l.worksheet.name.trim().eq_ignore_ascii_case(target_sheet)
                && (l.clause.as_deref() == Some(target_clause)
                    || (target_sheet.trim().to_uppercase().contains("PCU")
                        && l.status_column == 18)
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
                unmapped_count += 1;
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
            continue;
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
                unmapped_count += 1;
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
            continue;
        };
        let sheet_name = layout.worksheet.name.as_str();
        let rfi_column = layout.rfi_column;
        let status_column = layout.status_column;
        let Some(matching_sheet) = target_workbook
            .sheet(sheet_name)
            .or_else(|| find_sheet_flexible(target_workbook, sheet_name))
        else {
            continue;
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
            continue;
        }
        let coverage =
            signed_destination_coverage(sheet_name, matching_sheet, &candidate_keys, record);
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
            if coverage.missing_contexts
                && !manual_rows.iter().any(|row| row.row_id == final_review_id)
            {
                unmapped_count += 1;
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
            mapping_details: Some(mapping_details(
                record,
                Some((target_sheet, target_clause)),
                target_workbook,
                &candidate_keys,
            )),
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

pub(super) fn find_sheet_flexible<'a>(
    workbook: &'a WorkbookData,
    target: &str,
) -> Option<&'a WorksheetData> {
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

pub(super) fn supports_full_sync_schema(workbook: &WorkbookData) -> bool {
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
pub(super) fn classify_workbooks_legacy(
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
