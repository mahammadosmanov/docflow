use super::*;

pub(in crate::rfi_follow_up) fn route_clause(
    record: &SourceRecord,
) -> Option<(&'static str, &'static str)> {
    let (discipline, itp) = itp_identity(&record.itp)?;
    if discipline == "ELE" {
        return match (itp.as_str(), normalize_code_text(&record.item).as_str()) {
            ("0005", "ELE-5.2.6") => Some(("ELE", "2.6")),
            ("0006", "ELE-6.2.5.3") => Some(("ELE", "2.5.3")),
            _ => None,
        };
    }
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
pub(in crate::rfi_follow_up) fn intended_lifecycle_route(
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

pub(in crate::rfi_follow_up) fn is_confirmed_follow_up_exclusion(record: &SourceRecord) -> bool {
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
