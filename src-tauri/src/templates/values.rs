//! Typed slot values (ADR 0058): number, date, and checkbox.
//!
//! A slot is still written as text. The type checks what the operator typed
//! and picks how it is written, so a wrong value stops in Preview, not in the
//! finished file.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::field_map::FieldDef;

const DEFAULT_DATE_FORMAT: &str = "dd.MM.yyyy";
const DEFAULT_CHECKBOX: &str = "\u{2612}|\u{2610}";
const MAX_FORMAT_CHARS: usize = 40;

static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-?[0-9]+([.,][0-9]+)?$").expect("static number regex"));
static DAY_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]{1,2})[./-]([0-9]{1,2})[./-]([0-9]{4})$").expect("static date regex")
});
static YEAR_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})$").expect("static date regex")
});

/// How a slot's value is checked and written.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    #[default]
    Text,
    Number,
    Date,
    Checkbox,
    /// Made from other fields; see `templates::compute`.
    Computed,
}

impl FieldKind {
    pub fn is_text(&self) -> bool {
        *self == Self::Text
    }

    /// The schema 2 `type` name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Date => "date",
            Self::Checkbox => "checkbox",
            Self::Computed => "computed",
        }
    }
}

/// True for the starting-value texts that tick a checkbox.
pub fn truthy(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "1" | "x"
    )
}

/// Checks a slot's `format` when the map is saved.
pub fn check_format(kind: FieldKind, format: &str) -> Result<(), String> {
    let invalid = || "A field format is invalid".to_string();
    match kind {
        FieldKind::Text | FieldKind::Number | FieldKind::Computed if format.is_empty() => Ok(()),
        FieldKind::Text | FieldKind::Number | FieldKind::Computed => Err(invalid()),
        FieldKind::Date => {
            let clean = format.chars().count() <= MAX_FORMAT_CHARS
                && !format
                    .chars()
                    .any(|ch| ch.is_control() || "{}".contains(ch));
            let has_part = ["yyyy", "yy", "MM", "dd"]
                .iter()
                .any(|token| format.contains(token));
            if format.is_empty() || (clean && has_part) {
                Ok(())
            } else {
                Err(invalid())
            }
        }
        FieldKind::Checkbox => {
            if format.is_empty() {
                return Ok(());
            }
            let parts = format.split('|').collect::<Vec<_>>();
            let part_ok = |part: &&str| {
                (1..=20).contains(&part.chars().count())
                    && !part.chars().any(|ch| ch.is_control() || "{}".contains(ch))
            };
            if parts.len() == 2 && parts.iter().all(part_ok) {
                Ok(())
            } else {
                Err(invalid())
            }
        }
    }
}

fn name_of(field: &FieldDef) -> &str {
    if field.label.trim().is_empty() {
        &field.key
    } else {
        &field.label
    }
}

fn valid_date(year: u32, month: u32, day: u32) -> bool {
    let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year >= 1 && (1..=days).contains(&day)
}

fn read_date(text: &str) -> Option<(u32, u32, u32)> {
    if let Some(found) = YEAR_FIRST.captures(text) {
        return finish_date(&found[1], &found[2], &found[3]);
    }
    let found = DAY_FIRST.captures(text)?;
    finish_date(&found[3], &found[2], &found[1])
}

fn finish_date(year: &str, month: &str, day: &str) -> Option<(u32, u32, u32)> {
    let (year, month, day) = (year.parse().ok()?, month.parse().ok()?, day.parse().ok()?);
    valid_date(year, month, day).then_some((year, month, day))
}

fn write_date(format: &str, (year, month, day): (u32, u32, u32)) -> String {
    let mut out = String::new();
    let mut rest = format;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("yyyy") {
            out.push_str(&format!("{year:04}"));
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("yy") {
            out.push_str(&format!("{:02}", year % 100));
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("MM") {
            out.push_str(&format!("{month:02}"));
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("dd") {
            out.push_str(&format!("{day:02}"));
            rest = tail;
        } else {
            let ch = rest.chars().next().expect("not empty");
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

/// The text to write for `raw`, or why the operator's value is refused. A
/// checkbox's `raw` is `"true"` or `"false"`.
pub fn convert(field: &FieldDef, raw: &str) -> Result<String, String> {
    match field.kind {
        FieldKind::Text | FieldKind::Computed => Ok(raw.to_string()),
        FieldKind::Number => {
            let text = raw.trim();
            if text.is_empty() || NUMBER.is_match(text) {
                Ok(text.to_string())
            } else {
                Err(format!(
                    "{} must be a number such as 12 or 3,5",
                    name_of(field)
                ))
            }
        }
        FieldKind::Date => {
            let text = raw.trim();
            if text.is_empty() {
                return Ok(String::new());
            }
            let parts = read_date(text)
                .ok_or_else(|| format!("{} must be a date such as 07.11.2026", name_of(field)))?;
            let format = if field.format.is_empty() {
                DEFAULT_DATE_FORMAT
            } else {
                &field.format
            };
            Ok(write_date(format, parts))
        }
        FieldKind::Checkbox => {
            let format = if field.format.is_empty() {
                DEFAULT_CHECKBOX
            } else {
                &field.format
            };
            let (checked, unchecked) = format.split_once('|').unwrap_or((format, ""));
            Ok(if truthy(raw) { checked } else { unchecked }.to_string())
        }
    }
}

#[cfg(test)]
mod tests;
