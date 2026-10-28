use super::*;

mod clause_routing;
mod rfi_codes;
pub(super) use clause_routing::*;
pub(super) use rfi_codes::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LogState {
    Pending,
    InProgress,
    Completed,
    Remove,
}

#[derive(Clone, Debug)]
pub(super) struct SourceRecord {
    pub(super) canonical_rfi: String,
    pub(super) short_rfi: String,
    pub(super) itp: String,
    pub(super) item: String,
    pub(super) state: Option<LogState>,
    pub(super) excluded_civ_pcs: bool,
    pub(super) worley_signed: bool,
    pub(super) contexts: Vec<SourceRouteContext>,
    pub(super) evidence: Vec<SourceStatusEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SourceRouteContext {
    pub(super) tag: String,
    pub(super) area: String,
    pub(super) location: String,
}
pub(super) fn compact_detail(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}

pub(super) fn mapping_details(
    record: &SourceRecord,
    route: Option<(&str, &str)>,
    target_workbook: &WorkbookData,
    candidates: &[(String, u32, u32, u32)],
) -> String {
    let mut details = format!(
        "ITP: {}; item: {}",
        compact_detail(&record.itp),
        compact_detail(&record.item)
    );
    let ignore_inspection_location =
        route.is_some_and(|(sheet, _)| matches!(sheet, "MEC" | "Signed By Worley MEC"));
    let context_label = |context: &SourceRouteContext| {
        let tag = compact_detail(&context.tag);
        let area = compact_detail(&context.area);
        if ignore_inspection_location {
            format!("tag={tag}; area={area}")
        } else {
            format!(
                "tag={tag}; area={area}; location={}",
                compact_detail(&context.location)
            )
        }
    };
    let split_civ_tags =
        route.is_some_and(|(sheet, _)| matches!(sheet, "CIV" | "Signed By Worley CIV"));
    let source_contexts = if ignore_inspection_location || split_civ_tags {
        record
            .contexts
            .iter()
            .flat_map(|context| {
                let values = if ignore_inspection_location {
                    normalized_parts(&context.area)
                } else {
                    normalized_parts(&context.tag)
                        .into_iter()
                        .filter(|tag| !tag.starts_with("PCS-"))
                        .collect()
                };
                if values.is_empty() {
                    vec![context.clone()]
                } else {
                    values
                        .into_iter()
                        .map(|value| {
                            if ignore_inspection_location {
                                SourceRouteContext {
                                    area: value,
                                    ..context.clone()
                                }
                            } else {
                                SourceRouteContext {
                                    tag: value,
                                    ..context.clone()
                                }
                            }
                        })
                        .collect()
                }
            })
            .collect::<Vec<_>>()
    } else {
        record.contexts.clone()
    };
    let contexts = source_contexts
        .iter()
        .map(context_label)
        .collect::<BTreeSet<_>>();
    if contexts.is_empty() {
        details.push_str("; source context: none");
    } else {
        details.push_str("; source context: ");
        details.push_str(
            &contexts
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" | "),
        );
        if contexts.len() > 5 {
            details.push_str(&format!(" | +{} more", contexts.len() - 5));
        }
    }
    if let Some((sheet, clause)) = route {
        details.push_str(&format!("; expected target: {sheet}/{clause}"));
    } else {
        details.push_str("; expected target: no configured Follow Up route");
    }
    if candidates.is_empty() {
        details.push_str("; matching prepared target rows: none");
    } else {
        let target_columns: &[u32] = if ignore_inspection_location {
            &[3, 4]
        } else {
            &[1, 2, 3, 4]
        };
        let targets = candidates
            .iter()
            .take(5)
            .map(|(sheet_name, row, _, _)| {
                let context = target_workbook
                    .sheet(sheet_name)
                    .into_iter()
                    .flat_map(|sheet| {
                        target_columns
                            .iter()
                            .filter_map(move |column| sheet.cell(*row, *column))
                    })
                    .map(|cell| compact_detail(&cell.value))
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(" / ");
                format!("row {row}: {context}")
            })
            .collect::<Vec<_>>()
            .join(" | ");
        details.push_str(&format!("; candidate target rows: {targets}"));
        if candidates.len() > 5 {
            details.push_str(&format!(" | +{} more", candidates.len() - 5));
        }
    }
    if let Some((target_sheet, _)) = route {
        let matcher_sheet = match target_sheet {
            "Signed By Worley CIV" => "CIV",
            "Signed By Worley MEC" => "MEC",
            sheet => sheet,
        };
        let mut matched_contexts = BTreeMap::<String, BTreeSet<u32>>::new();
        let mut unmatched_contexts = BTreeSet::new();
        for context in &source_contexts {
            let label = context_label(context);
            let rows = candidates
                .iter()
                .filter_map(|(sheet_name, row, _, _)| {
                    let sheet = target_workbook.sheet(sheet_name)?;
                    row_matches_context(matcher_sheet, sheet, *row, context, target_workbook)
                        .then_some(*row)
                })
                .collect::<BTreeSet<_>>();
            if rows.is_empty() {
                unmatched_contexts.insert(label);
            } else {
                matched_contexts.insert(label, rows);
            }
        }
        if !matched_contexts.is_empty() {
            let matches = matched_contexts
                .iter()
                .take(5)
                .map(|(context, rows)| {
                    format!(
                        "{context} -> row(s) {}",
                        rows.iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            details.push_str(&format!("; matched source context: {matches}"));
        }
        if !unmatched_contexts.is_empty() {
            let label = if unmatched_contexts.len() == 1 {
                "unmatched source context"
            } else {
                "unmatched source contexts"
            };
            details.push_str(&format!(
                "; {label}: {}",
                unmatched_contexts
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
            if unmatched_contexts.len() > 5 {
                details.push_str(&format!(" | +{} more", unmatched_contexts.len() - 5));
            }
        }
    }
    details
}

pub(super) fn log_state(value: &str) -> Option<LogState> {
    match normalize_header(value).as_str() {
        "" => Some(LogState::Pending),
        "ONGOING" | "IN PROGRESS" => Some(LogState::InProgress),
        "COMPLETED" => Some(LogState::Completed),
        "CANCELLED" | "CANCELED" | "REJECTED" => Some(LogState::Remove),
        _ => None,
    }
}

pub(super) fn aggregate_status(states: impl IntoIterator<Item = LogState>) -> Option<String> {
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

pub(super) fn find_header_column(sheet: &WorksheetData, names: &[&str]) -> Option<u32> {
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

/// Every canonical RFI number written in the CIV and MEC RFI No. columns of the Log,
/// including rows that only carry a number. The Log is the source of truth: a
/// number found nowhere in it was deleted and does not belong in Follow Up.
pub(super) fn scan_logged_rfi_numbers(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
) -> BTreeSet<String> {
    let mut numbers = BTreeSet::new();
    for discipline in ["CIV", "MEC"] {
        let Some(sheet) = discovery
            .worksheets
            .iter()
            .find(|sheet| sheet.name.trim().eq_ignore_ascii_case(discipline))
            .and_then(|descriptor| workbook.sheet(&descriptor.name))
        else {
            continue;
        };
        let Some(rfi_col) = find_header_column(sheet, &["RFI NO.", "RFI NO"]) else {
            continue;
        };
        for row in SOURCE_FIRST_DATA_ROW..=sheet.max_row {
            if let Some(code) = sheet
                .cell(row, rfi_col)
                .and_then(|cell| canonical_rfi(&cell.value))
            {
                numbers.insert(code);
            }
        }
    }
    numbers
}

pub(super) fn scan_source_records(
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
            // A row that holds only a pre-numbered RFI No. is not a record yet.
            if [&status, &remarks, &itp, &item, &tag, &area, &location]
                .iter()
                .all(|value| value.is_empty())
            {
                continue;
            }
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
                // An unsigned duplicate is not evidence that an already
                // observed Worley signature was withdrawn.
                existing.worley_signed |= incoming.worley_signed;
                existing.evidence.extend(incoming.evidence);
            } else {
                records.insert(canonical_rfi, incoming);
            }
        }
    }
    Ok(records)
}

pub(super) fn approved_sheet(name: &str) -> bool {
    let norm = name.trim().to_uppercase();
    matches!(
        norm.as_str(),
        "CIV" | "MEC" | "PCU" | "ROAD" | "SIGNED BY WORLEY CIV" | "SIGNED BY WORLEY MEC"
    )
}

pub(super) fn extract_header_clause(
    sheet: &WorksheetData,
    max_row: u32,
    column: u32,
) -> Option<String> {
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

pub(super) fn discover_approved_layouts(
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
