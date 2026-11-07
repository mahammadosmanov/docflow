//! Template field maps (ADR 0055, schema 1) and `{{placeholder}}` handling.
//!
//! A template file marks each value with `{{key}}`. The field map says what
//! each key starts as: a text that may contain RFI Log tokens such as
//! `{rfiNo}` or `({area}) {tagNo}`. Presets are named sets of field values
//! (for example one ITP stage group) that fill several fields at once.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
    sync::LazyLock,
};

use quick_xml::escape::unescape;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::{FileKind, MAX_FIELDS};

pub const MAX_PRESETS: usize = 500;
const MAX_TEXT_CHARS: usize = 4_000;
const MAX_OPTIONS: usize = 100;
const MAX_PACKAGE_ENTRIES: usize = 4_096;
const MAX_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;

/// RFI Log values a field text can refer to as `{token}`.
pub const LOG_TOKENS: [&str; 17] = [
    "rfiNo",
    "itpNo",
    "itemNo",
    "description",
    "forms",
    "tagNo",
    "area",
    "dateOfIssue",
    "inspectionDate",
    "expiryDate",
    "startTime",
    "azfenType",
    "intecType",
    "location",
    "subcontractor",
    "remarks",
    "today",
];

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*([A-Za-z][A-Za-z0-9_]{0,63})\s*\}\}").expect("static placeholder regex")
});
static TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{([A-Za-z][A-Za-z0-9]{0,31})\}").expect("static token regex"));
static KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]{0,63}$").expect("static key regex"));
static TEXT_NODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<((?:w:)?t)(\s[^>]*)?>(.*?)</(?:w:)?t\s*>").expect("static text-node regex")
});

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub key: String,
    #[serde(default)]
    pub label: String,
    /// Starting value; `{token}` parts come from the RFI Log row.
    #[serde(default)]
    pub value: String,
    /// Suggestions offered while editing (for example people's names).
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub multiline: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetDef {
    pub name: String,
    /// Field key -> value. Fields not listed keep their own value.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldMap {
    pub schema: u32,
    /// Output file name without extension; may use `{{key}}`.
    #[serde(default = "default_file_name")]
    pub file_name: String,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    #[serde(default)]
    pub presets: Vec<PresetDef>,
}

fn default_file_name() -> String {
    "{{rfiNo}}".to_string()
}

impl Default for FieldMap {
    fn default() -> Self {
        Self {
            schema: 1,
            file_name: default_file_name(),
            fields: Vec::new(),
            presets: Vec::new(),
        }
    }
}

impl FieldMap {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let map: Self = serde_json::from_value(value.clone())
            .map_err(|_| "The template field map is invalid".to_string())?;
        map.validate()?;
        Ok(map)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err("The template field map version is not supported".to_string());
        }
        if self.fields.len() > MAX_FIELDS || self.presets.len() > MAX_PRESETS {
            return Err("The template has too many fields or presets".to_string());
        }
        let long = |text: &str| text.chars().count() > MAX_TEXT_CHARS;
        if self.file_name.trim().is_empty() || long(&self.file_name) {
            return Err("The output file name pattern is invalid".to_string());
        }
        let mut keys = BTreeSet::new();
        for field in &self.fields {
            if !KEY.is_match(&field.key) || !keys.insert(field.key.as_str()) {
                return Err(format!(
                    "Field key \"{}\" is invalid or repeated",
                    field.key
                ));
            }
            if long(&field.label)
                || long(&field.value)
                || field.options.len() > MAX_OPTIONS
                || field.options.iter().any(|option| long(option))
            {
                return Err(format!("Field \"{}\" is too long", field.key));
            }
        }
        let mut names = BTreeSet::new();
        for preset in &self.presets {
            let name = preset.name.trim();
            if name.is_empty() || long(name) || !names.insert(name.to_lowercase()) {
                return Err(format!("Preset \"{}\" is empty or repeated", preset.name));
            }
            for (key, value) in &preset.values {
                if !keys.contains(key.as_str()) || long(value) {
                    return Err(format!(
                        "Preset \"{}\" sets an unknown field \"{key}\"",
                        preset.name
                    ));
                }
            }
        }
        Ok(())
    }

    /// Adds a field for every placeholder that has none. Existing fields keep
    /// their settings; a placeholder named like an RFI Log token starts as it.
    pub fn merge_placeholders(&mut self, placeholders: &[String]) {
        let known = self
            .fields
            .iter()
            .map(|field| field.key.clone())
            .collect::<BTreeSet<_>>();
        for key in placeholders {
            if known.contains(key) || self.fields.len() >= MAX_FIELDS {
                continue;
            }
            let value = LOG_TOKENS
                .iter()
                .find(|token| token.eq_ignore_ascii_case(key))
                .map(|token| format!("{{{token}}}"))
                .unwrap_or_default();
            self.fields.push(FieldDef {
                key: key.clone(),
                label: key.clone(),
                value,
                options: Vec::new(),
                multiline: false,
            });
        }
    }
}

/// Replaces `{token}` with RFI Log values. Unknown tokens stay as written.
pub fn resolve_tokens(text: &str, row: &BTreeMap<String, String>) -> String {
    TOKEN
        .replace_all(text, |captures: &regex::Captures<'_>| {
            row.get(&captures[1])
                .cloned()
                .unwrap_or_else(|| captures[0].to_string())
        })
        .into_owned()
}

/// Replaces `{{key}}` in a short text such as the output file name.
pub fn fill_text(text: &str, values: &BTreeMap<String, String>) -> String {
    PLACEHOLDER
        .replace_all(text, |captures: &regex::Captures<'_>| {
            values.get(&captures[1]).cloned().unwrap_or_default()
        })
        .into_owned()
}

fn text_parts(kind: FileKind, name: &str) -> bool {
    match kind {
        FileKind::Docx => {
            name == "word/document.xml"
                || (name.starts_with("word/header") || name.starts_with("word/footer"))
                    && name.ends_with(".xml")
        }
        FileKind::Xlsx => {
            name == "xl/sharedStrings.xml"
                || name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml")
        }
    }
}

struct Package {
    comment: Vec<u8>,
    entries: Vec<(String, Vec<u8>)>,
}

fn read_package(bytes: &[u8]) -> Result<Package, String> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Invalid Office file".to_string())?;
    if archive.is_empty() || archive.len() > MAX_PACKAGE_ENTRIES {
        return Err("Invalid Office file".to_string());
    }
    let comment = archive.comment().to_vec();
    let mut total = 0_u64;
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| "Invalid Office file".to_string())?;
        total = total.saturating_add(entry.size());
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err("Office file expands beyond the supported limit".to_string());
        }
        let name = entry
            .name()
            .map_err(|_| "Invalid Office file".to_string())?
            .into_owned();
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|_| "Invalid Office file".to_string())?;
        entries.push((name, data));
    }
    Ok(Package { comment, entries })
}

/// Placeholder keys in document order, without repeats.
pub fn placeholders(kind: FileKind, bytes: &[u8]) -> Result<Vec<String>, String> {
    let package = read_package(bytes)?;
    let mut seen = BTreeSet::new();
    let mut keys = Vec::new();
    for (name, data) in &package.entries {
        if !text_parts(kind, name) {
            continue;
        }
        let xml = String::from_utf8_lossy(data);
        for group in text_groups(&xml) {
            let joined = group
                .iter()
                .map(|node| node.text.as_str())
                .collect::<String>();
            for captures in PLACEHOLDER.captures_iter(&joined) {
                let key = captures[1].to_string();
                if seen.insert(key.clone()) {
                    keys.push(key);
                }
            }
        }
    }
    Ok(keys)
}

/// Writes `values` into every `{{key}}` and returns the new package plus the
/// placeholder keys that had no value (they become empty).
pub fn fill_package(
    kind: FileKind,
    bytes: &[u8],
    values: &BTreeMap<String, String>,
) -> Result<(Vec<u8>, BTreeSet<String>), String> {
    let package = read_package(bytes)?;
    let mut missing = BTreeSet::new();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in &package.entries {
        let filled;
        let output = if text_parts(kind, name) {
            let xml = std::str::from_utf8(data)
                .map_err(|_| format!("Template part {name} is not UTF-8"))?;
            filled = fill_xml(xml, values, &mut missing);
            filled.as_bytes()
        } else {
            data.as_slice()
        };
        if name.ends_with('/') {
            writer
                .add_directory(name.as_str(), options)
                .map_err(|_| "Could not build the document".to_string())?;
            continue;
        }
        writer
            .start_file(name.as_str(), options)
            .and_then(|()| writer.write_all(output).map_err(Into::into))
            .map_err(|_| "Could not build the document".to_string())?;
    }
    writer
        .set_raw_comment(package.comment.into())
        .map_err(|_| "Could not build the document".to_string())?;
    let bytes = writer
        .finish()
        .map_err(|_| "Could not build the document".to_string())?
        .into_inner();
    Ok((bytes, missing))
}

struct TextNode {
    /// Byte range of the node's inner text in the XML.
    inner: std::ops::Range<usize>,
    /// Byte range of the opening tag.
    open: std::ops::Range<usize>,
    text: String,
}

/// Text nodes grouped by paragraph (Word) or string item (Excel), so a
/// placeholder split across runs by the editor is still found.
fn text_groups(xml: &str) -> Vec<Vec<TextNode>> {
    let mut groups: Vec<Vec<TextNode>> = Vec::new();
    let mut last_end = 0;
    for captures in TEXT_NODE.captures_iter(xml) {
        let whole = captures.get(0).expect("whole match");
        let inner = captures.get(3).expect("inner text");
        let gap = &xml[last_end..whole.start()];
        let breaks = groups.is_empty()
            || ["</w:p>", "</si>", "</c>", "</is>", "</w:tc>"]
                .iter()
                .any(|close| gap.contains(close));
        if breaks {
            groups.push(Vec::new());
        }
        let text = unescape(inner.as_str())
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_else(|_| inner.as_str().to_string());
        let open_end = whole.start() + xml[whole.start()..].find('>').unwrap_or(0) + 1;
        groups.last_mut().expect("group exists").push(TextNode {
            inner: inner.range(),
            open: whole.start()..open_end,
            text,
        });
        last_end = whole.end();
    }
    groups
}

fn fill_xml(
    xml: &str,
    values: &BTreeMap<String, String>,
    missing: &mut BTreeSet<String>,
) -> String {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for group in text_groups(xml) {
        let joined = group
            .iter()
            .map(|node| node.text.as_str())
            .collect::<String>();
        if !joined.contains("{{") {
            continue;
        }
        let matches = PLACEHOLDER.captures_iter(&joined).collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        let mut texts = group
            .iter()
            .map(|node| node.text.clone())
            .collect::<Vec<_>>();
        // Replace from the end so earlier offsets stay valid.
        for captures in matches.iter().rev() {
            let span = captures.get(0).expect("whole placeholder");
            let key = &captures[1];
            let value = values.get(key).cloned().unwrap_or_else(|| {
                missing.insert(key.to_string());
                String::new()
            });
            let mut offset = 0;
            let mut owner = None;
            let mut cuts = Vec::new();
            for (index, node) in group.iter().enumerate() {
                let end = offset + node.text.len();
                if span.start() < end && span.end() > offset {
                    owner.get_or_insert(index);
                    cuts.push((
                        index,
                        span.start().max(offset) - offset,
                        span.end().min(end) - offset,
                    ));
                }
                offset = end;
            }
            // Matches run last to first, so these offsets (taken from the
            // original text) still point at unchanged bytes.
            for (index, from, to) in cuts {
                let current = &texts[index];
                let insert = if Some(index) == owner {
                    value.as_str()
                } else {
                    ""
                };
                texts[index] = format!("{}{}{}", &current[..from], insert, &current[to..]);
            }
        }
        for (node, text) in group.iter().zip(texts) {
            if text == node.text {
                continue;
            }
            let open = &xml[node.open.clone()];
            if !open.contains("xml:space") {
                let tag_end = node.open.end - 1;
                edits.push((tag_end..tag_end, " xml:space=\"preserve\"".to_string()));
            }
            edits.push((
                node.inner.clone(),
                escape_text(&text, open.starts_with("<w:")),
            ));
        }
    }
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    let mut output = xml.to_string();
    for (range, text) in edits.into_iter().rev() {
        output.replace_range(range, &text);
    }
    output
}

fn escape_text(value: &str, word: bool) -> String {
    let escaped = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "");
    if word {
        escaped.replace('\n', "</w:t><w:br/><w:t xml:space=\"preserve\">")
    } else {
        escaped
    }
}

#[cfg(test)]
mod tests;
