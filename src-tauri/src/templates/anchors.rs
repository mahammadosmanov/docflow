//! Cell anchors (ADR 0058): an editable part that sits at a table cell of a
//! Word form or a worksheet cell of an Excel form, instead of at a
//! `{{placeholder}}`. Everything else in the file is copied unchanged.

use std::{collections::BTreeMap, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::{
    FileKind,
    field_map::{escape_text, rewrite_package},
};

const MAX_INDEX: u32 = 10_000;

static ADDRESS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Z]{1,3})([1-9][0-9]{0,6})$").expect("static address regex"));
static OPEN_OR_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(/?)w:(tbl|tr|tc)[ >]").expect("static table tag regex"));
static TC_PR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<w:tcPr[ >].*?</w:tcPr>|<w:tcPr/>").expect("static tcPr"));
static P_PR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<w:pPr[ >].*?</w:pPr>|<w:pPr/>").expect("static pPr"));
static R_PR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<w:rPr[ >].*?</w:rPr>|<w:rPr/>").expect("static rPr"));
static FIRST_PARAGRAPH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<w:p[ >].*?</w:p>|<w:p/>").expect("static paragraph"));
static FIRST_RUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<w:r[ >].*?</w:r>").expect("static run"));
static VERTICAL_CONTINUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<w:vMerge\s*/>|<w:vMerge\s+w:val="continue""#).expect("static vMerge")
});

/// Cell content this writer cannot rewrite without losing it.
const UNSAFE_CONTENT: [&str; 9] = [
    "<w:drawing",
    "<w:pict",
    "<w:object",
    "<w:sdt",
    "<w:ffData",
    "<w:sym ",
    "<w:fldChar",
    "<w:fldSimple",
    "<w:tbl",
];

/// Where a slot sits. Excel: `sheet` and `address`. Word: `table`, `row`,
/// and `cell` (all zero-based, body tables only).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellAnchor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<u32>,
}

impl CellAnchor {
    /// The file kind this anchor belongs to, when its fields are complete and
    /// do not mix the Word and Excel forms.
    pub fn kind(&self) -> Result<FileKind, String> {
        let invalid = || "The cell anchor is invalid".to_string();
        match (&self.sheet, &self.address, self.table, self.row, self.cell) {
            (Some(sheet), Some(address), None, None, None) => {
                let name_ok = !sheet.trim().is_empty()
                    && sheet.chars().count() <= 31
                    && !sheet.contains(['[', ']', ':', '*', '?', '/', '\\']);
                (name_ok && cell_coordinate(address).is_some())
                    .then_some(FileKind::Xlsx)
                    .ok_or_else(invalid)
            }
            (None, None, Some(table), Some(row), Some(cell)) => (table.max(row).max(cell)
                <= MAX_INDEX)
                .then_some(FileKind::Docx)
                .ok_or_else(invalid),
            _ => Err(invalid()),
        }
    }

    pub fn check_shape(&self) -> Result<(), String> {
        self.kind().map(|_| ())
    }
}

/// `C14` becomes `(14, 3)` (row, column).
fn cell_coordinate(address: &str) -> Option<(u32, u32)> {
    let captures = ADDRESS.captures(address)?;
    let column = captures[1].bytes().fold(0_u32, |total, letter| {
        total * 26 + u32::from(letter - b'A' + 1)
    });
    let row = captures[2].parse::<u32>().ok()?;
    (column <= 16_384 && row <= 1_048_576).then_some((row, column))
}

/// Writes each value into its anchored cell and returns the new package.
pub fn apply(
    kind: FileKind,
    package: &[u8],
    anchored: &[(&CellAnchor, &str)],
) -> Result<Vec<u8>, String> {
    if anchored.is_empty() {
        return Ok(package.to_vec());
    }
    for (anchor, _) in anchored {
        if anchor.kind()? != kind {
            return Err("A cell anchor does not match the template file type".to_string());
        }
    }
    match kind {
        FileKind::Xlsx => apply_excel(package, anchored),
        FileKind::Docx => apply_word(package, anchored),
    }
}

fn apply_excel(package: &[u8], anchored: &[(&CellAnchor, &str)]) -> Result<Vec<u8>, String> {
    let mut updates = BTreeMap::<String, BTreeMap<(u32, u32), String>>::new();
    for (anchor, value) in anchored {
        let (Some(sheet), Some(address)) = (&anchor.sheet, &anchor.address) else {
            return Err("The cell anchor is invalid".to_string());
        };
        let coordinate = cell_coordinate(address).ok_or("The cell anchor is invalid")?;
        updates
            .entry(sheet.clone())
            .or_default()
            .insert(coordinate, (*value).to_string());
    }
    crate::ooxml::patch_template_cells(package, &updates)
        .map_err(|error| format!("Cannot write the template cells: {error}"))
}

fn apply_word(package: &[u8], anchored: &[(&CellAnchor, &str)]) -> Result<Vec<u8>, String> {
    let mut failure: Option<String> = None;
    let output = rewrite_package(package, |name, xml| {
        if name != "word/document.xml" {
            return None;
        }
        let mut current = xml.to_string();
        for (anchor, value) in anchored {
            match set_word_cell(&current, anchor, value) {
                Ok(next) => current = next,
                Err(error) => {
                    failure = Some(error);
                    return None;
                }
            }
        }
        Some(current)
    })?;
    failure.map_or(Ok(output), Err)
}

/// Byte ranges of the `index`-th element of `tag` in `xml`, where the
/// elements do not nest (a nested table is refused by the caller).
fn element_ranges(xml: &str, tag: &str) -> Result<Vec<std::ops::Range<usize>>, String> {
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    for found in OPEN_OR_CLOSE.captures_iter(xml) {
        if &found[2] != tag {
            continue;
        }
        let at = found.get(0).map_or(0, |whole| whole.start());
        if &found[1] == "/" {
            let Some(begin) = start.take() else {
                return Err("The Word table markup is not supported".to_string());
            };
            let end = xml[at..]
                .find('>')
                .map_or(xml.len(), |offset| at + offset + 1);
            ranges.push(begin..end);
        } else if start.replace(at).is_some() {
            return Err("Nested Word tables are not supported".to_string());
        }
    }
    if start.is_some() {
        return Err("The Word table markup is not supported".to_string());
    }
    Ok(ranges)
}

fn set_word_cell(xml: &str, anchor: &CellAnchor, value: &str) -> Result<String, String> {
    let missing = || "The Word table cell was not found in the template".to_string();
    let (Some(table), Some(row), Some(cell)) = (anchor.table, anchor.row, anchor.cell) else {
        return Err("The cell anchor is invalid".to_string());
    };
    let tables = element_ranges(xml, "tbl")?;
    let table_range = tables.get(table as usize).ok_or_else(missing)?.clone();
    let rows = element_ranges(&xml[table_range.clone()], "tr")?;
    let row_range = rows.get(row as usize).ok_or_else(missing)?.clone();
    let row_base = table_range.start + row_range.start;
    let cells = element_ranges(&xml[row_base..table_range.start + row_range.end], "tc")?;
    let cell_range = cells.get(cell as usize).ok_or_else(missing)?;
    let range = row_base + cell_range.start..row_base + cell_range.end;
    let rewritten = rewrite_cell(&xml[range.clone()], value)?;
    let mut output = String::with_capacity(xml.len() + rewritten.len());
    output.push_str(&xml[..range.start]);
    output.push_str(&rewritten);
    output.push_str(&xml[range.end..]);
    Ok(output)
}

fn rewrite_cell(cell: &str, value: &str) -> Result<String, String> {
    let open_end = cell
        .find('>')
        .ok_or("The Word table markup is not supported")?
        + 1;
    let close_start = cell
        .rfind("</w:tc")
        .ok_or("The Word table markup is not supported")?;
    let inner = &cell[open_end..close_start];
    let properties = TC_PR.find(inner).map_or("", |found| found.as_str());
    if VERTICAL_CONTINUE.is_match(properties) {
        return Err("The Word cell continues a vertically merged cell".to_string());
    }
    if UNSAFE_CONTENT.iter().any(|marker| inner.contains(marker)) {
        return Err("The Word cell holds content that cannot be replaced safely".to_string());
    }
    let body = inner.replacen(properties, "", 1);
    let first = FIRST_PARAGRAPH
        .find(&body)
        .map_or("", |found| found.as_str());
    let paragraph_properties = P_PR.find(first).map_or("", |found| found.as_str());
    let without = first.replacen(paragraph_properties, "", 1);
    let run_properties = FIRST_RUN
        .find(&without)
        .and_then(|run| {
            R_PR.find(run.as_str())
                .map(|found| found.as_str().to_string())
        })
        .unwrap_or_default();
    let paragraph = if value.is_empty() {
        format!("<w:p>{paragraph_properties}</w:p>")
    } else {
        format!(
            "<w:p>{paragraph_properties}<w:r>{run_properties}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            escape_text(value, true)
        )
    };
    Ok(format!(
        "{}{properties}{paragraph}{}",
        &cell[..open_end],
        &cell[close_start..]
    ))
}

#[cfg(test)]
mod tests;
