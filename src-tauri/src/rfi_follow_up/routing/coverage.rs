use super::*;

pub(in crate::rfi_follow_up) struct SignedDestinationCoverage {
    pub(in crate::rfi_follow_up) valid: bool,
    pub(in crate::rfi_follow_up) missing_contexts: bool,
}

pub(in crate::rfi_follow_up) fn signed_destination_coverage(
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
pub(in crate::rfi_follow_up) fn civ_tag_coverage(
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

pub(in crate::rfi_follow_up) fn distinct_civ_area_rows(
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

pub(in crate::rfi_follow_up) fn distinct_civ_zone_area_rows(
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

pub(in crate::rfi_follow_up) fn mec_context_coverage(
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
