use super::*;

pub(in crate::rfi_follow_up) fn mapping_details(
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
    let split_civ_tags =
        route.is_some_and(|(sheet, _)| matches!(sheet, "CIV" | "Signed By Worley CIV"));
    let source_contexts = route_source_contexts(record, ignore_inspection_location, split_civ_tags);
    let contexts = source_contexts
        .iter()
        .map(|context| route_context_label(context, ignore_inspection_location))
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
        details.push_str(&candidate_rows_detail(
            candidates,
            target_workbook,
            ignore_inspection_location,
        ));
    }
    if let Some((target_sheet, _)) = route {
        details.push_str(&matched_context_detail(
            target_sheet,
            &source_contexts,
            candidates,
            target_workbook,
            ignore_inspection_location,
        ));
    }
    details
}

/// Expands comma-separated CIV tags or MEC areas into one source context per value.
fn route_source_contexts(
    record: &SourceRecord,
    ignore_inspection_location: bool,
    split_civ_tags: bool,
) -> Vec<SourceRouteContext> {
    if !(ignore_inspection_location || split_civ_tags) {
        return record.contexts.clone();
    }
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
        .collect()
}

fn route_context_label(context: &SourceRouteContext, ignore_inspection_location: bool) -> String {
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
}

/// Lists up to five candidate Follow Up rows with their identifying cells.
fn candidate_rows_detail(
    candidates: &[(String, u32, u32, u32)],
    target_workbook: &WorkbookData,
    ignore_inspection_location: bool,
) -> String {
    let targets = candidates
        .iter()
        .take(5)
        .map(|(sheet_name, row, _, _)| {
            let target_columns: &[u32] = match sheet_name.as_str() {
                "ELE" => &[1],
                _ if ignore_inspection_location => &[3, 4],
                _ => &[1, 2, 3, 4],
            };
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
    let mut detail = format!("; candidate target rows: {targets}");
    if candidates.len() > 5 {
        detail.push_str(&format!(" | +{} more", candidates.len() - 5));
    }
    detail
}

/// Reports which source contexts match a candidate row and which match none.
fn matched_context_detail(
    target_sheet: &str,
    source_contexts: &[SourceRouteContext],
    candidates: &[(String, u32, u32, u32)],
    target_workbook: &WorkbookData,
    ignore_inspection_location: bool,
) -> String {
    let matcher_sheet = match target_sheet {
        "Signed By Worley CIV" => "CIV",
        "Signed By Worley MEC" => "MEC",
        sheet => sheet,
    };
    let mut matched_contexts = BTreeMap::<String, BTreeSet<u32>>::new();
    let mut unmatched_contexts = BTreeSet::new();
    for context in source_contexts {
        let label = route_context_label(context, ignore_inspection_location);
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
    let mut detail = String::new();
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
        detail.push_str(&format!("; matched source context: {matches}"));
    }
    if !unmatched_contexts.is_empty() {
        let label = if unmatched_contexts.len() == 1 {
            "unmatched source context"
        } else {
            "unmatched source contexts"
        };
        detail.push_str(&format!(
            "; {label}: {}",
            unmatched_contexts
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        if unmatched_contexts.len() > 5 {
            detail.push_str(&format!(" | +{} more", unmatched_contexts.len() - 5));
        }
    }
    detail
}
