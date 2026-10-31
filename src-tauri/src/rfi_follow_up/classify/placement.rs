use super::*;

/// Places one source record onto the slots it belongs to, or queues a manual-review row.
pub(super) struct RecordIndex<'a> {
    pub(super) layouts: &'a [FollowUpLayout],
    pub(super) records: &'a BTreeMap<String, SourceRecord>,
    pub(super) target_workbook: &'a WorkbookData,
    pub(super) potential_slots_by_sheet_col: &'a SlotKeysBySheetCol,
    pub(super) existing_slot_keys_by_code: &'a SlotKeysByCode,
}

pub(super) fn place_record(
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
    let existing_slots = existing_slot_keys_by_code
        .get(record.canonical_rfi.as_str())
        .cloned()
        .unwrap_or_default();
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
    let composite_pcs_route = target_sheet != "ELE"
        && record.contexts.iter().any(|context| {
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
    if target_sheet == "ELE" && existing_slots.is_empty() {
        return Ok(());
    }
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
        let Some(layout) = layouts
            .iter()
            .find(|layout| layout.worksheet.name == sheet_name && layout.rfi_column == rfi_column)
            .cloned()
        else {
            return Ok(());
        };
        *unmapped_count += 1;
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
