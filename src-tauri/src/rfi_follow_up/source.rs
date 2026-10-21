use super::*;

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

pub(super) fn normalize_code_text(value: &str) -> String {
    value
        .nfc()
        .collect::<String>()
        .replace(['˗', '‐', '‑', '‒', '–', '—', '−'], "-")
        .trim()
        .to_uppercase()
}

pub(super) fn itp_identity(value: &str) -> Option<(String, String)> {
    static ITP: OnceLock<Regex> = OnceLock::new();
    let normalized = normalize_code_text(value);
    let captures = ITP
        .get_or_init(|| Regex::new(r"ITP-(CIV|MEC)-([0-9]{4})(?:_[A-Z])?\z").unwrap())
        .captures(&normalized)?;
    Some((captures[1].to_string(), captures[2].to_string()))
}

pub(super) fn item_clause(value: &str) -> Option<String> {
    static ITEM: OnceLock<Regex> = OnceLock::new();
    let normalized = normalize_code_text(value);
    let captures = ITEM
        .get_or_init(|| Regex::new(r"(?:CIV|MEC)-[0-9]+\.([0-9]+\.[0-9]+)\z").unwrap())
        .captures(&normalized)?;
    Some(captures[1].to_string())
}
pub(super) fn full_item_identity(value: &str) -> Option<(String, String)> {
    static ITEM: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A(CIV|MEC)-([0-9]+(?:\.[0-9]+)+)\z").unwrap());
    let normalized = normalize_code_text(value);
    let captures = ITEM.captures(&normalized)?;
    Some((captures[1].to_string(), captures[2].to_string()))
}
pub(super) fn split_values(value: &str) -> Vec<String> {
    value
        .split([',', '/', ';', '\n'])
        .map(normalize_status)
        .filter(|part| !part.is_empty())
        .collect()
}

pub(super) fn parse_rfi_group(value: &str) -> Option<Vec<String>> {
    static PART: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A(?:(CIV|MEC|ELE|PRC)[-_]?)?([0-9]{1,4})(?:-([0-9]{1,4}))?\z").unwrap()
    });
    let mut normalized = normalize_status(value).to_uppercase().replace(',', "/");
    if let Some(short) = normalized.strip_prefix(FULL_RFI_PREFIX) {
        normalized = short.to_string();
    } else if let Some(short) = normalized.strip_prefix("RFI-") {
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
        let start = normalize_candidate_number(&captures[2])?
            .parse::<u32>()
            .ok()?;
        let end = captures
            .get(3)
            .and_then(|value| normalize_candidate_number(value.as_str()))
            .and_then(|value| value.parse::<u32>().ok())
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

pub(super) fn format_rfi_group(codes: &[String]) -> Option<String> {
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

pub(super) fn route_clause(record: &SourceRecord) -> Option<(&'static str, &'static str)> {
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
pub(super) fn intended_lifecycle_route(
    record: &SourceRecord,
) -> Option<(&'static str, &'static str)> {
    let (sheet, clause) = route_clause(record)?;
    let composite_pcs_route = record.contexts.iter().any(|context| {
        let tags = normalized_parts(&context.tag);
        tags.iter().filter(|tag| tag.starts_with("PCS-")).count() == 1 && tags.len() > 1
    });
    if composite_pcs_route {
        Some((
            "CIV",
            match clause {
                "1.8" => "1.8",
                "2.4" => "2.4",
                _ => "2.5",
            },
        ))
    } else {
        Some((sheet, clause))
    }
}

pub(super) fn is_confirmed_follow_up_exclusion(record: &SourceRecord) -> bool {
    let Some((discipline, itp)) = itp_identity(&record.itp) else {
        return false;
    };
    let Some((item_discipline, item)) = full_item_identity(&record.item) else {
        return false;
    };
    if discipline != item_discipline {
        return false;
    }
    matches!(
        (discipline.as_str(), itp.as_str(), item.as_str()),
        ("CIV", "0002", "2.1.4" | "2.1.6")
            | (
                "CIV",
                "0005",
                "5.1.4" | "5.1.5" | "5.1.9" | "5.3.4" | "5.3.5"
            )
            | (
                "CIV",
                "0006",
                "6.1.8" | "6.2.1" | "6.2.2" | "6.2.3" | "6.2.4" | "6.2.5" | "6.2.9"
            )
            | ("CIV", "0007", "7.1.5" | "7.2.3" | "7.2.4" | "7.3.9")
            | ("MEC", "0001", "1.1.4" | "1.2.5")
            | ("MEC", "0004", "4.3.1")
    )
}
