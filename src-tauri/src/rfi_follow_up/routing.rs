use super::*;

pub(super) fn signed_route_clause(
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

pub(super) fn default_route_col(sheet: &str, clause: &str) -> u32 {
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

pub(super) fn default_signed_col(sheet: &str, clause: &str) -> (u32, u32) {
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
pub(super) fn route_key(record: &SourceRecord) -> Option<(&'static str, u32)> {
    let (sheet, clause) = route_clause(record)?;
    Some((sheet, default_route_col(sheet, clause)))
}

#[allow(dead_code)]
pub(super) fn signed_route_key(
    record: &SourceRecord,
) -> Option<(&'static str, u32, u32, &'static str)> {
    let (sheet, clause, match_layout) = signed_route_clause(record)?;
    let (rfi, st) = default_signed_col(sheet, clause);
    Some((sheet, rfi, st, match_layout))
}

pub(super) fn is_signed_layout(layout: &FollowUpLayout) -> bool {
    layout
        .worksheet
        .name
        .trim()
        .to_uppercase()
        .contains("SIGNED BY WORLEY")
        || (layout.worksheet.name.trim().to_uppercase().contains("PCU")
            && layout.status_column == 18)
}

pub(super) fn normalized_location(value: &str) -> String {
    normalize_header(value)
        .replace(" SITE", "")
        .replace("PCU-", "PCS-")
        .replace("ROAD 0", "ROAD ")
}

pub(super) fn normalized_parts(value: &str) -> Vec<String> {
    split_values(value)
        .into_iter()
        .map(|part| normalized_location(&part))
        .filter(|part| !part.is_empty())
        .collect()
}

pub(super) fn pcu_row_matches(
    sheet: &WorksheetData,
    row: u32,
    context: &SourceRouteContext,
) -> bool {
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

pub(super) fn unique_zone_for_pcs(target_workbook: &WorkbookData, pcs: &str) -> Option<String> {
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

pub(super) fn row_matches(
    sheet_name: &str,
    sheet: &WorksheetData,
    row: u32,
    record: &SourceRecord,
    target_workbook: &WorkbookData,
) -> bool {
    record
        .contexts
        .iter()
        .any(|context| row_matches_context(sheet_name, sheet, row, context, target_workbook))
}

pub(super) fn row_matches_context(
    sheet_name: &str,
    sheet: &WorksheetData,
    row: u32,
    context: &SourceRouteContext,
    target_workbook: &WorkbookData,
) -> bool {
    let cell = |column| {
        sheet
            .cell(row, column)
            .map(|cell| normalized_location(&cell.value))
            .unwrap_or_default()
    };
    match sheet_name {
        "CIV" => {
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
        }
        "PCU" => pcu_row_matches(sheet, row, context),
        "Road" => normalized_location(&context.tag) == cell(2),
        "MEC" => {
            let target_sections = normalized_parts(&cell(4));
            let target_tags = normalized_parts(&cell(3));
            let sections = normalized_parts(&context.area);
            let tags = normalized_parts(&context.tag);
            !sections.is_empty()
                && !tags.is_empty()
                && tags.iter().any(|tag| target_tags.contains(tag))
                && sections
                    .iter()
                    .any(|value| target_sections.contains(&normalized_location(value)))
        }
        _ => false,
    }
}

pub(super) fn record_matches_lifecycle_slot(
    slot: &SlotWork,
    record: &SourceRecord,
    target_workbook: &WorkbookData,
) -> bool {
    let Some((route_sheet, route_clause)) = intended_lifecycle_route(record) else {
        return true;
    };
    if !slot
        .layout
        .worksheet
        .name
        .trim()
        .eq_ignore_ascii_case(route_sheet)
        || slot
            .layout
            .clause
            .as_deref()
            .is_some_and(|clause| clause != route_clause)
        || slot.layout.clause.is_none()
            && slot.layout.rfi_column != default_route_col(route_sheet, route_clause)
    {
        return false;
    }
    let Some(sheet) = target_workbook
        .sheet(&slot.layout.worksheet.name)
        .or_else(|| find_sheet_flexible(target_workbook, &slot.layout.worksheet.name))
    else {
        return false;
    };
    row_matches(route_sheet, sheet, slot.row, record, target_workbook)
}

pub(super) struct SignedDestinationCoverage {
    pub(super) valid: bool,
    pub(super) missing_contexts: bool,
}

pub(super) fn signed_destination_coverage(
    sheet_name: &str,
    sheet: &WorksheetData,
    candidates: &[(String, u32, u32, u32)],
    record: &SourceRecord,
) -> SignedDestinationCoverage {
    if sheet_name == "Signed By Worley MEC" && record.evidence.len() > record.contexts.len() {
        return SignedDestinationCoverage {
            valid: false,
            missing_contexts: false,
        };
    }
    if sheet_name == "Signed By Worley CIV" {
        let source_tags = record
            .contexts
            .iter()
            .flat_map(|context| normalized_parts(&context.tag))
            .filter(|tag| !tag.starts_with("PCS-"))
            .collect::<BTreeSet<_>>();
        if source_tags.is_empty() {
            return SignedDestinationCoverage {
                valid: false,
                missing_contexts: false,
            };
        }
        let mut matched_tags = BTreeSet::new();
        for (_, row, _, _) in candidates {
            let tag = sheet
                .cell(*row, 2)
                .map(|cell| normalized_location(&cell.value))
                .unwrap_or_default();
            if !source_tags.contains(&tag) || !matched_tags.insert(tag) {
                return SignedDestinationCoverage {
                    valid: false,
                    missing_contexts: false,
                };
            }
        }
        return SignedDestinationCoverage {
            valid: true,
            missing_contexts: matched_tags.len() < source_tags.len(),
        };
    }
    if sheet_name == "Signed By Worley MEC" {
        let mut source_pairs = BTreeSet::new();
        for context in &record.contexts {
            let tags = normalized_parts(&context.tag);
            let blocks = tags
                .iter()
                .filter(|tag| tag.starts_with("PCS-"))
                .cloned()
                .collect::<Vec<_>>();
            let areas = normalized_parts(&context.area);
            if tags.len() != 1 || blocks.len() != 1 || areas.is_empty() {
                return SignedDestinationCoverage {
                    valid: false,
                    missing_contexts: false,
                };
            }
            for area in areas {
                source_pairs.insert((blocks[0].clone(), area));
            }
        }
        if source_pairs.is_empty() {
            return SignedDestinationCoverage {
                valid: false,
                missing_contexts: false,
            };
        }
        let mut matched_pairs = BTreeSet::new();
        for (_, row, _, _) in candidates {
            let blocks = sheet
                .cell(*row, 3)
                .map(|cell| normalized_parts(&cell.value))
                .unwrap_or_default();
            let areas = sheet
                .cell(*row, 4)
                .map(|cell| normalized_parts(&cell.value))
                .unwrap_or_default();
            if blocks.len() != 1 || areas.len() != 1 {
                return SignedDestinationCoverage {
                    valid: false,
                    missing_contexts: false,
                };
            }
            let mut matching_pairs = source_pairs
                .iter()
                .filter(|(block, area)| blocks.contains(block) && areas.contains(area));
            let Some(matched_pair) = matching_pairs.next() else {
                return SignedDestinationCoverage {
                    valid: false,
                    missing_contexts: false,
                };
            };
            if matching_pairs.next().is_some() || !matched_pairs.insert(matched_pair) {
                return SignedDestinationCoverage {
                    valid: false,
                    missing_contexts: false,
                };
            }
        }
        return SignedDestinationCoverage {
            valid: true,
            missing_contexts: matched_pairs.len() < source_pairs.len(),
        };
    }
    SignedDestinationCoverage {
        valid: candidates.len() == 1,
        missing_contexts: false,
    }
}
pub(super) fn civ_tag_coverage(
    target_workbook: &WorkbookData,
    candidates: &[(String, u32, u32, u32)],
    record: &SourceRecord,
) -> Option<bool> {
    let source_tags = record
        .contexts
        .iter()
        .flat_map(|context| normalized_parts(&context.tag))
        .filter(|tag| !tag.starts_with("PCS-"))
        .collect::<BTreeSet<_>>();
    if candidates.is_empty() || source_tags.len() < 2 {
        return None;
    }
    let sheet = target_workbook.sheet("CIV")?;
    let mut matched_tags = BTreeSet::new();
    for (_, row, _, _) in candidates {
        let tags = sheet
            .cell(*row, 2)
            .map(|cell| normalized_parts(&cell.value))
            .unwrap_or_default();
        if tags.len() != 1
            || !source_tags.contains(&tags[0])
            || !matched_tags.insert(tags[0].clone())
        {
            return None;
        }
    }
    Some(matched_tags.len() == source_tags.len())
}

pub(super) fn distinct_civ_area_rows(
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

pub(super) fn distinct_civ_zone_area_rows(
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

pub(super) fn mec_context_coverage(
    target_workbook: &WorkbookData,
    sheet_name: &str,
    candidates: &[(String, u32, u32, u32)],
    record: &SourceRecord,
) -> Option<bool> {
    if candidates.is_empty() {
        return None;
    }
    let sheet = target_workbook.sheet(sheet_name)?;
    let mut source_blocks = BTreeSet::new();
    let mut source_sections = BTreeSet::new();
    let mut source_contexts = BTreeSet::new();
    for context in &record.contexts {
        let blocks = normalized_parts(&context.tag)
            .into_iter()
            .filter(|tag| tag.starts_with("PCS-"))
            .collect::<Vec<_>>();
        let areas = normalized_parts(&context.area)
            .into_iter()
            .filter(|area| area.starts_with('S'))
            .collect::<Vec<_>>();
        if blocks.len() != 1 || areas.is_empty() {
            return None;
        }
        source_blocks.insert(blocks[0].clone());
        for area in areas {
            source_sections.insert(area.clone());
            source_contexts.insert((blocks[0].clone(), area));
        }
    }
    if source_blocks.len() != 1 || source_sections.is_empty() || source_contexts.len() < 2 {
        return None;
    }
    let mut matched_contexts = BTreeSet::new();
    for (_, row, _, _) in candidates {
        let blocks = sheet
            .cell(*row, 3)
            .map(|cell| normalized_parts(&cell.value))
            .unwrap_or_default();
        let areas = sheet
            .cell(*row, 4)
            .map(|cell| normalized_parts(&cell.value))
            .unwrap_or_default();
        if blocks.len() != 1 || areas.len() != 1 {
            return None;
        }
        let context = (blocks[0].clone(), areas[0].clone());
        if !source_blocks.contains(&context.0)
            || !source_sections.contains(&context.1)
            || !source_contexts.contains(&context)
            || !matched_contexts.insert(context)
        {
            return None;
        }
    }
    Some(matched_contexts.len() == source_contexts.len())
}

#[derive(Clone)]
pub(super) struct SlotWork {
    pub(super) layout: FollowUpLayout,
    pub(super) row: u32,
    pub(super) rfi_cell: CellSnapshot,
    pub(super) status_cell: CellSnapshot,
    pub(super) codes: Vec<String>,
    pub(super) current_status: String,
    pub(super) changed_rfis: BTreeSet<String>,
    pub(super) unsafe_reason: Option<&'static str>,
}

pub(super) fn safe_slot(
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

pub(super) fn main_status_for_codes(
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

pub(super) fn row_from_slot(
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
        mapping_details: None,
    })
}

pub(super) fn unsafe_slot_review(
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
        mapping_details: None,
    })
}
pub(super) fn missing_final_review(
    record: &SourceRecord,
    layout: &FollowUpLayout,
    sheet: &str,
    target_clause: &str,
    columns: (u32, u32),
    candidates: &[(String, u32, u32, u32)],
    target_workbook: &WorkbookData,
) -> FollowUpRow {
    let (rfi_column, status_column) = columns;
    FollowUpRow {
        row_id: format!("rfi-follow-up-unmapped:{}", record.canonical_rfi),
        sheet: sheet.to_string(),
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
        reason_key: "warn.rfi.followUp.reason.mappingMissing".to_string(),
        selectable: false,
        enabled: false,
        target: FollowUpReviewTarget {
            worksheet: layout.worksheet.clone(),
            row: 0,
            rfi_column,
            status_column,
            rfi_cell: None,
            status_cell: None,
        },
        source_evidence: record.evidence.clone(),
        mapping_details: Some(mapping_details(
            record,
            Some((sheet, target_clause)),
            target_workbook,
            candidates,
        )),
    }
}

impl SourceRecord {
    pub(super) fn inspection_status_for_ui(&self) -> String {
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
