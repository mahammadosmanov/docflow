use std::sync::OnceLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

pub const PREFIX: &str = "AZE-SUN-INT-QMS-RFI";
const PREFIX_HEAD: &str = "AZE-SUN-INT-QMS";
const DISCIPLINES: [&str; 8] = ["ARC", "CIV", "ELE", "INS", "MEC", "PIP", "STR", "TEL"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RfiMethod {
    Strict,
    Repaired,
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

fn find_canonical(text: &str) -> RfiMatch {
    if let Some(captures) = strict().captures(text) {
        return build(&captures[1], &captures[2], RfiMethod::Strict, 1.0);
    }

    for token in loose().find_iter(text).map(|candidate| candidate.as_str()) {
        let fixed = repair(token);
        if let Some(captures) = strict().captures(&fixed) {
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
        let raw_discipline = segments[5];
        let discipline = recover_discipline(raw_discipline);
        if discipline.len() != 3
            || !discipline.bytes().all(|byte| byte.is_ascii_uppercase())
            || !DISCIPLINES.contains(&discipline)
        {
            continue;
        }
        let distance = levenshtein(&segments[..4].join("-"), PREFIX_HEAD);
        if distance > 2 {
            continue;
        }
        if discipline != raw_discipline {
            return build(
                discipline,
                number,
                RfiMethod::Repaired,
                if distance == 0 {
                    0.9
                } else {
                    0.85 - distance as f64 / 10.0
                },
            );
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
        Regex::new(r"AZE-SUN-INT-QMS-RFI-([A-Z]{3})-(\d{4})")
            .expect("the strict RFI pattern is valid")
    })
}

fn loose() -> &'static Regex {
    static LOOSE: OnceLock<Regex> = OnceLock::new();
    LOOSE.get_or_init(|| {
        Regex::new(r"[A-Z0-9]{2,4}(?:-[A-Z0-9]{2,4}){5,6}")
            .expect("the loose RFI candidate pattern is valid")
    })
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

fn recover_discipline(raw: &str) -> &str {
    if DISCIPLINES.contains(&raw) {
        return raw;
    }
    let mut best = raw;
    let mut best_distance = usize::MAX;
    let mut ties = 0;
    for candidate in DISCIPLINES {
        let distance = levenshtein(raw, candidate);
        if distance < best_distance {
            best = candidate;
            best_distance = distance;
            ties = 1;
        } else if distance == best_distance {
            ties += 1;
        }
    }
    if best_distance <= 1 && ties == 1 {
        best
    } else {
        raw
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
    RfiMatch {
        code: format!("{PREFIX}-{discipline}-{number}"),
        discipline: discipline.to_string(),
        number: number.to_string(),
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
