use std::{collections::BTreeMap, sync::OnceLock};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::errors::BackendError;

pub const MAX_EMAIL_CHARS: usize = 30_000;
pub const MAX_RFI_COUNT: usize = 100;
const MAX_EXTRACTED_TEXT_CHARS: usize = 4_096;
const MAX_NOTES: usize = 100;
const MAX_NOTE_CHARS: usize = 500;
const MAX_EXTRACTION_BYTES: usize = 512 * 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmailContext {
    pub discipline: String,
    pub itp_to_items: BTreeMap<String, Vec<String>>,
    pub known_tags: Vec<String>,
    pub known_subcontractors: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailRow {
    pub rfi_index: usize,
    pub itp_no: String,
    pub item_no: String,
    pub tag_no: String,
    pub area: String,
    pub location: String,
    pub subcontractor: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailExtraction {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub discipline: String,
    pub count: usize,
    pub rows: Vec<EmailRow>,
    pub confidence: f64,
    pub notes: Vec<String>,
}

pub fn detect_discipline(text: &str) -> Option<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^A-Z])(CIV|MEC|ELE|PRC)(?:$|[^A-Z])").expect("discipline regex")
    });
    pattern
        .captures(text)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_ascii_uppercase())
}

pub fn offline_extract(text: &str, context: &EmailContext) -> EmailExtraction {
    let clean = text
        .chars()
        .take(MAX_EMAIL_CHARS)
        .collect::<String>()
        .replace(r"\_", "_");
    let clean = markdown_pattern().replace_all(&clean, "").into_owned();
    let all_items = context
        .itp_to_items
        .values()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    let mut items = mentioned_known(&clean, &all_items);
    let mut itps = mentioned_identifiers(
        &clean,
        &context.itp_to_items.keys().cloned().collect::<Vec<_>>(),
    );

    if itps.is_empty()
        && let Some(captures) = itp_number_pattern().captures(&clean)
        && let Some(suffix) = captures
            .get(1)
            .and_then(|value| value.as_str().parse::<usize>().ok())
    {
        let suffix = format!("{suffix:04}");
        itps = context
            .itp_to_items
            .keys()
            .filter(|label| label.ends_with(&suffix))
            .cloned()
            .collect();
    }
    if items.is_empty()
        && let Some(item) = abbreviated_item(&clean, &itps, &context.itp_to_items)
    {
        items.push(item);
    }

    let mut tag_values = tag_line_pattern()
        .captures_iter(&clean)
        .filter_map(|captures| captures.get(1))
        .flat_map(|value| split_values(value.as_str()))
        .collect::<Vec<_>>();
    if tag_values.is_empty() {
        let compact = expand_compact_tags(&clean);
        let tokens = extract_tag_tokens(&clean);
        let known = mentioned_known(&clean, &context.known_tags);
        tag_values = if !compact.is_empty() {
            compact
        } else if !tokens.is_empty() {
            tokens
        } else {
            known
        };
    }
    deduplicate(&mut tag_values);
    tag_values.truncate(MAX_RFI_COUNT);

    let count = explicit_count(&clean);
    let inferred_count = count.or_else(|| (!tag_values.is_empty()).then_some(tag_values.len()));
    let area = first_line(area_line_pattern(), &clean);
    let location = first_line(location_line_pattern(), &clean);
    let mut subcontractor = first_line(subcontractor_line_pattern(), &clean);
    if subcontractor.is_empty() {
        subcontractor = mentioned_known(&clean, &context.known_subcontractors)
            .into_iter()
            .next()
            .unwrap_or_default();
    }

    let shared_itp = if itps.len() == 1 {
        itps[0].clone()
    } else {
        String::new()
    };
    let shared_item = if items.len() == 1 {
        items[0].clone()
    } else {
        String::new()
    };
    let row_count = inferred_count
        .unwrap_or_default()
        .max(tag_values.len())
        .clamp(1, MAX_RFI_COUNT);
    let index_limit = inferred_count.unwrap_or(row_count).max(1);
    let rows = (0..row_count)
        .map(|index| EmailRow {
            rfi_index: (index + 1).min(index_limit),
            itp_no: shared_itp.clone(),
            item_no: shared_item.clone(),
            tag_no: tag_values.get(index).cloned().unwrap_or_default(),
            area: area.clone(),
            location: location.clone(),
            subcontractor: subcontractor.clone(),
        })
        .collect();
    let mut notes = Vec::new();
    if items.len() > 1 {
        notes.push("Multiple Item No. values were mentioned; review each row.".to_string());
    }
    if itps.len() > 1 {
        notes.push("Multiple ITP No. values were mentioned; review each row.".to_string());
    }
    if count.is_none() {
        notes.push("RFI count was not explicit; the form count was kept.".to_string());
    }

    let recognized = [
        detect_discipline(&clean).is_some(),
        count.is_some(),
        !items.is_empty(),
        !itps.is_empty(),
        !tag_values.is_empty(),
        !area.is_empty(),
        !location.is_empty(),
        !subcontractor.is_empty(),
    ]
    .into_iter()
    .filter(|recognized| *recognized)
    .count();
    EmailExtraction {
        provider: "offline".to_string(),
        model: None,
        discipline: detect_discipline(&clean).unwrap_or_else(|| context.discipline.clone()),
        count: count.unwrap_or_default(),
        rows,
        confidence: (0.35 + recognized as f64 * 0.08).min(0.95),
        notes,
    }
}

pub fn validate_gemini_extraction(value: Value) -> Result<EmailExtraction, BackendError> {
    let encoded = serde_json::to_vec(&value)
        .map_err(|_| BackendError::validation("Gemini extraction is invalid"))?;
    if encoded.len() > MAX_EXTRACTION_BYTES {
        return Err(BackendError::validation(
            "Gemini extraction exceeded the size limit",
        ));
    }
    let extraction: EmailExtraction = serde_json::from_value(value)
        .map_err(|_| BackendError::validation("Gemini extraction DTO is invalid"))?;
    if extraction.provider != "gemini"
        || !is_discipline(&extraction.discipline)
        || extraction.count > MAX_RFI_COUNT
        || extraction.rows.len() > MAX_RFI_COUNT
        || !extraction.confidence.is_finite()
        || !(0.0..=1.0).contains(&extraction.confidence)
        || extraction.notes.len() > MAX_NOTES
        || extraction
            .model
            .as_deref()
            .is_none_or(|model| model.is_empty() || model.chars().count() > 128)
    {
        return Err(BackendError::validation(
            "Gemini extraction values are out of range",
        ));
    }
    if extraction.rows.iter().any(|row| {
        row.rfi_index == 0
            || row.rfi_index > MAX_RFI_COUNT
            || row_fields(row)
                .into_iter()
                .any(|value| value.chars().count() > MAX_EXTRACTED_TEXT_CHARS)
    }) || extraction
        .notes
        .iter()
        .any(|note| note.chars().count() > MAX_NOTE_CHARS)
    {
        return Err(BackendError::validation(
            "Gemini extraction values are out of range",
        ));
    }
    Ok(extraction)
}

fn is_discipline(value: &str) -> bool {
    matches!(value, "CIV" | "MEC" | "ELE" | "PRC")
}

fn row_fields(row: &EmailRow) -> [&str; 6] {
    [
        &row.itp_no,
        &row.item_no,
        &row.tag_no,
        &row.area,
        &row.location,
        &row.subcontractor,
    ]
}

fn explicit_count(text: &str) -> Option<usize> {
    count_patterns().iter().find_map(|pattern| {
        pattern
            .captures(text)
            .and_then(|captures| captures.get(1))
            .and_then(|value| value.as_str().parse::<usize>().ok())
            .filter(|value| (1..=MAX_RFI_COUNT).contains(value))
    })
}

fn mentioned_known(text: &str, values: &[String]) -> Vec<String> {
    let upper = text.to_uppercase();
    values
        .iter()
        .filter(|value| !value.is_empty() && upper.contains(&value.to_uppercase()))
        .cloned()
        .collect()
}

fn identifier_text(value: &str) -> String {
    static TOKENS: OnceLock<Regex> = OnceLock::new();
    let tokens = TOKENS.get_or_init(|| Regex::new(r"[A-Z]+|\d+").expect("identifier regex"));
    let normalized = value.nfkc().collect::<String>().to_uppercase();
    tokens
        .find_iter(&normalized)
        .map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join("-")
}

fn mentioned_identifiers(text: &str, values: &[String]) -> Vec<String> {
    let normalized_text = identifier_text(text);
    values
        .iter()
        .filter(|value| {
            let candidate = identifier_text(value);
            !candidate.is_empty() && normalized_text.contains(&candidate)
        })
        .cloned()
        .collect()
}

fn abbreviated_item(
    text: &str,
    itps: &[String],
    mapping: &BTreeMap<String, Vec<String>>,
) -> Option<String> {
    let candidates = if itps.len() == 1 {
        mapping.get(&itps[0]).cloned().unwrap_or_default()
    } else {
        mapping.values().flatten().cloned().collect()
    };
    let mut matches = Vec::new();
    for item in candidates {
        let numbers = number_pattern()
            .find_iter(&item)
            .map(|value| value.as_str())
            .collect::<Vec<_>>();
        if numbers.len() < 2 {
            continue;
        }
        let short = format!(
            "{}.{}",
            numbers[numbers.len() - 2],
            numbers[numbers.len() - 1]
        );
        if contains_bounded_number(text, &short) && !matches.contains(&item) {
            matches.push(item);
        }
    }
    (matches.len() == 1).then(|| matches.remove(0))
}

fn contains_bounded_number(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(start, value)| {
        let before = text[..start].chars().next_back();
        let after = text[start + value.len()..].chars().next();
        !before.is_some_and(|character| character.is_ascii_digit() || character == '.')
            && !after.is_some_and(|character| character.is_ascii_digit() || character == '.')
    })
}

fn split_values(value: &str) -> Vec<String> {
    split_pattern()
        .split(value)
        .map(|item| {
            item.trim_matches(|character: char| matches!(character, ' ' | '\t' | '.' | ';'))
        })
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn expand_compact_tags(text: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for captures in compact_tag_pattern().captures_iter(&text.to_uppercase()) {
        let Some(prefix) = captures.get(1).map(|value| value.as_str()) else {
            continue;
        };
        let Some(numbers) = captures.get(2).map(|value| value.as_str()) else {
            continue;
        };
        tags.extend(
            slash_pattern()
                .split(numbers)
                .map(|number| format!("{prefix}{number}")),
        );
    }
    tags
}

fn extract_tag_tokens(text: &str) -> Vec<String> {
    let upper = text.to_uppercase();
    let mut tokens = tag_token_pattern()
        .captures_iter(&upper)
        .filter_map(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
        .filter(|token| {
            !token.contains("QMS-ITP")
                && !["AZE-SUN-", "CIV-", "MEC-", "ELE-", "PRC-"]
                    .iter()
                    .any(|prefix| token.starts_with(prefix))
        })
        .collect::<Vec<_>>();
    deduplicate(&mut tokens);
    tokens
}

fn first_line(pattern: &Regex, text: &str) -> String {
    pattern
        .captures(text)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_string())
        .unwrap_or_default()
}

fn deduplicate(values: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn markdown_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"[*`]+").expect("markdown regex"))
}

fn number_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\d+").expect("number regex"))
}

fn itp_number_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\bITP(?:\s+NO\.?)?\s*[:#-]?\s*0*(\d{1,4})\b").expect("ITP number regex")
    })
}

fn count_patterns() -> &'static [Regex; 3] {
    static PATTERNS: OnceLock<[Regex; 3]> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            Regex::new(r"(?i)\b(\d{1,3})\s*(?:CIV|MEC|ELE|PRC)?\s*RFI(?:\s+numbers?)?\b")
                .expect("RFI count regex"),
            Regex::new(
                r"(?i)\b(\d{1,3})\s*(?:adet|ədəd|pcs?\.?|rfi(?:\s+numbers?)?|numara(?:s[ıi])?|n[oö]mr[əe])\b",
            )
            .expect("localized count regex"),
            Regex::new(
                r"(?i)\b(?:give|create|open|ver|olu[sş]tur|ay[ıi]r)[^\r\n]{0,18}(\d{1,3})\s*(?:new\s+)?(?:rfi|number|numara)",
            )
            .expect("imperative count regex"),
        ]
    })
}

fn split_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\s*[,;]\s*|\s+/\s+").expect("value split regex"))
}

fn slash_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\s*/\s*").expect("slash regex"))
}

fn compact_tag_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b([A-Z][A-Z0-9]*-)(\d+(?:\s*/\s*\d+)+)\b").expect("compact tag regex")
    })
}

fn tag_token_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)(?:^|[^A-Z0-9])([A-Z0-9]+(?:[_-][A-Z0-9]+)+(?:/[A-Z0-9]+)*)(?:$|[^A-Z0-9])",
        )
        .expect("tag token regex")
    })
}

fn tag_line_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?im)^\s*(?:tag(?:\s+no\.?)?|etiket)\s*[:=-]\s*([^\r\n]+)")
            .expect("tag line regex")
    })
}

fn area_line_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"(?im)^\s*area\s*[:=-]\s*([^\r\n]+)").expect("area line regex"))
}

fn location_line_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?im)^\s*(?:inspection\s+)?location\s*[:=-]\s*([^\r\n]+)")
            .expect("location line regex")
    })
}

fn subcontractor_line_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?im)^\s*subcontractor\s*[:=-]\s*([^\r\n]+)")
            .expect("subcontractor line regex")
    })
}
