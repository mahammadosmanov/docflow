use std::{collections::BTreeSet, sync::OnceLock};

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

pub const PREFIX: &str = "AZE-SUN-INT-QMS-RFI";
const PREFIX_HEAD: &str = "AZE-SUN-INT-QMS";
const DISCIPLINES: [&str; 4] = ["CIV", "ELE", "MEC", "PRC"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RfiMethod {
    Strict,
    Repaired,
    OcrAlias,
    Fuzzy,
    Miss,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RfiMatch {
    pub code: String,
    pub discipline: String,
    pub number: String,
    pub method: RfiMethod,
    pub confidence: f64,
}

impl RfiMatch {
    pub fn ok(&self) -> bool {
        self.method != RfiMethod::Miss
    }
}

pub fn canonical(value: &str) -> String {
    value.nfc().collect::<String>().to_uppercase()
}

pub fn find(value: &str) -> RfiMatch {
    canonical_and_find(value).1
}

pub fn canonical_and_find(value: &str) -> (String, RfiMatch) {
    let text = canonical(value);
    let matched = find_canonical(&text);
    (text, matched)
}

pub fn normalize_candidate_number(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 4 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(format!("{value:0>4}"))
}

pub fn normalize_full_code(value: &str) -> Option<String> {
    static FULL_CODE: OnceLock<Regex> = OnceLock::new();
    let normalized = canonical(value);
    let captures = FULL_CODE
        .get_or_init(|| {
            Regex::new(r"\AAZE-SUN-INT-QMS-RFI-(CIV|ELE|MEC|PRC)-([0-9]{1,4})\z")
                .expect("anchored full RFI code pattern is valid")
        })
        .captures(&normalized)?;
    Some(format!(
        "{PREFIX}-{}-{}",
        &captures[1],
        normalize_candidate_number(&captures[2])?
    ))
}

pub fn exact_codes(value: &str) -> BTreeSet<String> {
    let text = canonical(value);
    exact_wrapped()
        .captures_iter(&text)
        .filter(|captures| {
            let matched = captures.get(0).expect("exact RFI match exists");
            DISCIPLINES.contains(&captures.get(1).expect("discipline capture exists").as_str())
                && line_break_count(matched.as_str()) <= 2
                && !text[..matched.start()]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric)
                && !text[matched.end()..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric)
                && !followed_by_alphanumeric(&text, matched.end())
        })
        .map(|captures| {
            format!(
                "{PREFIX}-{}-{}",
                &captures[1],
                normalize_candidate_number(&captures[2]).expect("RFI suffix is numeric")
            )
        })
        .collect()
}

fn find_canonical(text: &str) -> RfiMatch {
    for captures in strict().captures_iter(text) {
        let matched = captures.get(0).expect("strict match exists");
        if DISCIPLINES.contains(&captures.get(1).expect("discipline capture exists").as_str())
            && valid_rfi_suffix_boundary(text, matched.end(), &captures[2])
        {
            return build(&captures[1], &captures[2], RfiMethod::Strict, 1.0);
        }
    }
    for captures in exact_wrapped().captures_iter(text) {
        let matched = captures.get(0).expect("wrapped match exists");
        if DISCIPLINES.contains(&captures.get(1).expect("discipline capture exists").as_str())
            && line_break_count(matched.as_str()) <= 2
            && valid_rfi_suffix_boundary(text, matched.end(), &captures[2])
        {
            return build(&captures[1], &captures[2], RfiMethod::Strict, 1.0);
        }
    }

    for candidate in loose().find_iter(text) {
        if followed_by_alphanumeric(text, candidate.end()) {
            continue;
        }
        let token = candidate.as_str();
        let fixed = repair(token);
        if let Some(captures) = strict().captures(&fixed)
            && DISCIPLINES.contains(&captures.get(1).expect("discipline capture exists").as_str())
            && !followed_by_alphanumeric(
                &fixed,
                captures.get(0).expect("strict match exists").end(),
            )
        {
            return build(&captures[1], &captures[2], RfiMethod::Repaired, 0.8);
        }
        let segments = fixed.split('-').collect::<Vec<_>>();
        if segments.len() != 7 || segments[4] != "RFI" {
            continue;
        }
        let number = segments[6];
        if number.len() != 4 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let discipline = segments[5];
        if !DISCIPLINES.contains(&discipline) {
            continue;
        }
        let distance = levenshtein(&segments[..4].join("-"), PREFIX_HEAD);
        if distance > 2 {
            continue;
        }
        return build(
            discipline,
            number,
            RfiMethod::Fuzzy,
            1.0 - distance as f64 / 10.0,
        );
    }
    miss()
}

fn strict() -> &'static Regex {
    static STRICT: OnceLock<Regex> = OnceLock::new();
    STRICT.get_or_init(|| {
        Regex::new(r"AZE-SUN-INT-QMS-RFI-([A-Z]{3})-([0-9]{1,4})")
            .expect("the strict RFI pattern is valid")
    })
}

fn exact_wrapped() -> &'static Regex {
    static EXACT_WRAPPED: OnceLock<Regex> = OnceLock::new();
    EXACT_WRAPPED.get_or_init(|| {
        let separator = r"[\t ]*(?:\r?\n[\t ]*){0,2}-[\t ]*(?:\r?\n[\t ]*){0,2}";
        Regex::new(&format!(
            "AZE{separator}SUN{separator}INT{separator}QMS{separator}RFI{separator}([A-Z]{{3}}){separator}([0-9]{{1,4}})"
        ))
        .expect("the wrapped exact RFI pattern is valid")
    })
}

fn line_break_count(value: &str) -> usize {
    value.bytes().filter(|byte| *byte == b'\n').count()
}

fn loose() -> &'static Regex {
    static LOOSE: OnceLock<Regex> = OnceLock::new();
    LOOSE.get_or_init(|| {
        Regex::new(r"[A-Z0-9]{2,4}(?:-[A-Z0-9]{2,4}){5,6}")
            .expect("the loose RFI candidate pattern is valid")
    })
}

fn followed_by_alphanumeric(text: &str, end: usize) -> bool {
    text[end..]
        .chars()
        .next()
        .is_some_and(char::is_alphanumeric)
}

fn valid_rfi_suffix_boundary(text: &str, end: usize, number: &str) -> bool {
    if text[end..]
        .chars()
        .find(|character| !character.is_whitespace())
        .is_some_and(|character| character.is_ascii_digit())
    {
        return false;
    }
    if !followed_by_alphanumeric(text, end) {
        return true;
    }
    if number.len() != 4 {
        return false;
    }
    let trailing_ascii_letters = text[end..]
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    trailing_ascii_letters > 0 && !followed_by_alphanumeric(text, end + trailing_ascii_letters)
}

fn repair(token: &str) -> String {
    let mut segments = token.split('-').map(str::to_string).collect::<Vec<_>>();
    let Some((number, head)) = segments.split_last_mut() else {
        return token.to_string();
    };
    for segment in head {
        *segment = segment.chars().map(to_letter).collect();
    }
    *number = number.chars().map(to_digit).collect();
    segments.join("-")
}

fn to_letter(character: char) -> char {
    match character {
        '0' => 'O',
        '1' => 'I',
        '2' => 'Z',
        '4' => 'A',
        '5' => 'S',
        '8' => 'B',
        value => value,
    }
}

fn to_digit(character: char) -> char {
    match character {
        'O' => '0',
        'I' | 'L' => '1',
        'S' => '5',
        'B' => '8',
        'Z' => '2',
        value => value,
    }
}

fn levenshtein(left: &str, right: &str) -> usize {
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (row, left_character) in left.chars().enumerate() {
        let mut current = Vec::with_capacity(right.len() + 1);
        current.push(row + 1);
        for (column, right_character) in right.iter().enumerate() {
            current.push(
                (previous[column + 1] + 1)
                    .min(current[column] + 1)
                    .min(previous[column] + usize::from(left_character != *right_character)),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

fn build(discipline: &str, number: &str, method: RfiMethod, confidence: f64) -> RfiMatch {
    let number =
        normalize_candidate_number(number).expect("recognized RFI suffix is one to four digits");
    RfiMatch {
        code: format!("{PREFIX}-{discipline}-{number}"),
        discipline: discipline.to_string(),
        number,
        method,
        confidence,
    }
}
fn miss() -> RfiMatch {
    RfiMatch {
        code: String::new(),
        discipline: String::new(),
        number: String::new(),
        method: RfiMethod::Miss,
        confidence: 0.0,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_number_normalizes_only_one_to_four_ascii_digits() {
        assert_eq!(normalize_candidate_number("9").as_deref(), Some("0009"));
        assert_eq!(normalize_candidate_number("91").as_deref(), Some("0091"));
        assert_eq!(normalize_candidate_number("901").as_deref(), Some("0901"));
        assert_eq!(normalize_candidate_number("0901").as_deref(), Some("0901"));
        assert_eq!(normalize_candidate_number("09010"), None);
        assert_eq!(normalize_candidate_number("9O1"), None);
        assert_eq!(normalize_candidate_number(""), None);
    }

    #[test]
    fn short_suffixes_are_normalized_only_inside_supported_anchored_rfi_codes() {
        let matched = find("AZE-SUN-INT-QMS-RFI-CIV-901");
        assert_eq!(matched.code, "AZE-SUN-INT-QMS-RFI-CIV-0901");
        assert_eq!(
            normalize_full_code("AZE-SUN-INT-QMS-RFI-CIV-901").as_deref(),
            Some("AZE-SUN-INT-QMS-RFI-CIV-0901")
        );
        assert_eq!(normalize_full_code("ITP-CIV-901"), None);
        assert_eq!(normalize_full_code("AZE-SUN-INT-QMS-RFI-CIV-09010"), None);
        assert_eq!(
            exact_codes("AZE-SUN-INT-QMS-RFI-CIV-9"),
            BTreeSet::from(["AZE-SUN-INT-QMS-RFI-CIV-0009".to_string()])
        );
        assert!(exact_codes("ITP-CIV-901").is_empty());
        assert!(exact_codes("2026-09-01").is_empty());
        assert!(exact_codes("AZE-SUN-INT-QMS-RFI-XYZ-901").is_empty());
        assert!(!find("AZE-SUN-INT-QMS-RFI-CIV-09010").ok());
    }
    #[test]
    fn recognizer_rejects_alphanumeric_suffixes_without_truncating_the_identity() {
        for suffix in ["901A", "901А", "09010"] {
            let value = format!("AZE-SUN-INT-QMS-RFI-CIV-{suffix}");
            let matched = find(&value);
            assert!(!matched.ok(), "{suffix} was accepted as {}", matched.code);
            assert!(exact_codes(&value).is_empty(), "{suffix} was extracted");
        }
        assert_eq!(
            find("AZE-SUN-INT-QMS-RFI-CIV-0901_EXTRA").code,
            "AZE-SUN-INT-QMS-RFI-CIV-0901"
        );
    }

    #[test]
    fn wrapped_short_suffix_requires_a_complete_noncontinued_serial() {
        let matched = find("AZE-SUN-INT-QMS-RFI-CIV-\n9");
        assert_eq!(matched.method, RfiMethod::Strict);
        assert_eq!(matched.code, "AZE-SUN-INT-QMS-RFI-CIV-0009");
        assert!(!find("AZE-SUN-INT-QMS-RFI-CIV-\n9\n01").ok());
    }
    #[test]
    fn four_digit_identity_survives_ascii_ocr_text_concatenation() {
        let matched = find("AZE-SUN-INT-QMS-RFI-CIV-0064REQUEST");
        assert_eq!(matched.method, RfiMethod::Strict);
        assert_eq!(matched.code, "AZE-SUN-INT-QMS-RFI-CIV-0064");
        assert!(!find("AZE-SUN-INT-QMS-RFI-CIV-901A").ok());
        assert!(!find("AZE-SUN-INT-QMS-RFI-CIV-0064А").ok());
        assert!(!find("AZE-SUN-INT-QMS-RFI-CIV-00645").ok());
    }
}
