use super::*;

pub(in crate::rfi_follow_up) fn normalize_code_text(value: &str) -> String {
    value
        .nfc()
        .collect::<String>()
        .replace(['˗', '‐', '‑', '‒', '–', '—', '−'], "-")
        .trim()
        .to_uppercase()
}

pub(in crate::rfi_follow_up) fn itp_identity(value: &str) -> Option<(String, String)> {
    static ITP: OnceLock<Regex> = OnceLock::new();
    let normalized = normalize_code_text(value);
    let captures = ITP
        .get_or_init(|| Regex::new(r"ITP-(CIV|MEC)-([0-9]{4})(?:_[A-Z])?\z").unwrap())
        .captures(&normalized)?;
    Some((captures[1].to_string(), captures[2].to_string()))
}

pub(in crate::rfi_follow_up) fn item_clause(value: &str) -> Option<String> {
    static ITEM: OnceLock<Regex> = OnceLock::new();
    let normalized = normalize_code_text(value);
    let captures = ITEM
        .get_or_init(|| Regex::new(r"(?:CIV|MEC)-[0-9]+\.([0-9]+\.[0-9]+)\z").unwrap())
        .captures(&normalized)?;
    Some(captures[1].to_string())
}
pub(in crate::rfi_follow_up) fn full_item_identity(value: &str) -> Option<(String, String)> {
    static ITEM: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A(CIV|MEC)-([0-9]+(?:\.[0-9]+)+)\z").unwrap());
    let normalized = normalize_code_text(value);
    let captures = ITEM.captures(&normalized)?;
    Some((captures[1].to_string(), captures[2].to_string()))
}
pub(in crate::rfi_follow_up) fn split_values(value: &str) -> Vec<String> {
    value
        .split([',', '/', ';', '\n'])
        .map(normalize_status)
        .filter(|part| !part.is_empty())
        .collect()
}

pub(in crate::rfi_follow_up) fn parse_rfi_group(value: &str) -> Option<Vec<String>> {
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

pub(in crate::rfi_follow_up) fn format_rfi_group(codes: &[String]) -> Option<String> {
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
