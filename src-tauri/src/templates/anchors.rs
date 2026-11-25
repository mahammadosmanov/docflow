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
pub const MAX_REPEAT_COLUMNS: u32 = 20;
pub const MAX_REPEAT_ROWS: usize = 200;

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
static FIRST_PARAGRAPH_ALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<w:p[ >].*?</w:p>").expect("static paragraphs"));
static TEXT_NODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<w:t(?:\s[^>]*)?>(.*?)</w:t>").expect("static text node"));
static GRID_SPAN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<w:gridSpan\s+w:val="(\d+)""#).expect("static gridSpan"));
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
    /// Word only: this many cells from `cell` repeat once per line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_columns: Option<u32>,
}

impl CellAnchor {
    /// The file kind this anchor belongs to, when its fields are complete and
    /// do not mix the Word and Excel forms.
    pub fn kind(&self) -> Result<FileKind, String> {
        let invalid = || "The cell anchor is invalid".to_string();
        match (&self.sheet, &self.address, self.table, self.row, self.cell) {
            (Some(sheet), Some(address), None, None, None) if self.repeat_columns.is_none() => {
                let name_ok = !sheet.trim().is_empty()
                    && sheet.chars().count() <= 31
                    && !sheet.contains(['[', ']', ':', '*', '?', '/', '\\']);
                (name_ok && cell_coordinate(address).is_some())
                    .then_some(FileKind::Xlsx)
                    .ok_or_else(invalid)
            }
            (None, None, Some(table), Some(row), Some(cell)) => {
                let columns = self.repeat_columns.unwrap_or(1);
                ((1..=MAX_REPEAT_COLUMNS).contains(&columns)
                    && table.max(row).max(cell.saturating_add(columns)) <= MAX_INDEX)
                    .then_some(FileKind::Docx)
                    .ok_or_else(invalid)
            }
            _ => Err(invalid()),
        }
    }

    pub fn check_shape(&self) -> Result<(), String> {
        self.kind().map(|_| ())
    }
}

/// `C14` becomes `(14, 3)` (row, column).
pub(super) fn cell_coordinate(address: &str) -> Option<(u32, u32)> {
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
    let (repeats, singles): (Vec<_>, Vec<_>) = anchored
        .iter()
        .partition(|(anchor, _)| anchor.repeat_columns.is_some());
    // Rows lower in a table are expanded first, so a row above is not moved by
    // the rows added below it. Single cells are written before any expansion.
    let mut repeats = repeats;
    repeats.sort_by_key(|(anchor, _)| std::cmp::Reverse((anchor.table, anchor.row)));
    let mut failure: Option<String> = None;
    let output = rewrite_package(package, |name, xml| {
        if name != "word/document.xml" {
            return None;
        }
        let mut current = xml.to_string();
        for (anchor, value) in &singles {
            match set_word_cell(&current, anchor, value) {
                Ok(next) => current = next,
                Err(error) => {
                    failure = Some(error);
                    return None;
                }
            }
        }
        for (anchor, value) in &repeats {
            match repeat_word_row(&current, anchor, value) {
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

/// Lines become rows and tabs separate the columns. An empty value is one
/// empty row; trailing empty lines are dropped.
fn rows_of(value: &str, columns: u32) -> Result<Vec<Vec<String>>, String> {
    let mut lines = value
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .collect::<Vec<_>>();
    while lines.len() > 1 && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.len() > MAX_REPEAT_ROWS {
        return Err(format!(
            "At most {MAX_REPEAT_ROWS} repeated rows are allowed"
        ));
    }
    let width = columns as usize;
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let mut cells = line.split('\t').map(str::to_string).collect::<Vec<_>>();
            if cells.len() > width {
                return Err(format!(
                    "Line {} has more than {columns} columns",
                    index + 1
                ));
            }
            cells.resize(width, String::new());
            Ok(cells)
        })
        .collect()
}

/// Replaces the template row with one copy per line, each cell rewritten.
fn repeat_word_row(xml: &str, anchor: &CellAnchor, value: &str) -> Result<String, String> {
    let missing = || "The Word table cell was not found in the template".to_string();
    let (Some(table), Some(row), Some(first), Some(columns)) =
        (anchor.table, anchor.row, anchor.cell, anchor.repeat_columns)
    else {
        return Err("The cell anchor is invalid".to_string());
    };
    let tables = element_ranges(xml, "tbl")?;
    let table_range = tables.get(table as usize).ok_or_else(missing)?.clone();
    let rows = element_ranges(&xml[table_range.clone()], "tr")?;
    let row_range = rows.get(row as usize).ok_or_else(missing)?.clone();
    let start = table_range.start + row_range.start;
    let end = table_range.start + row_range.end;
    let row_xml = &xml[start..end];
    if row_xml.contains("<w:vMerge") {
        return Err("The Word row has vertically merged cells and cannot repeat".to_string());
    }
    let cells = element_ranges(row_xml, "tc")?;
    if (first + columns) as usize > cells.len() {
        return Err(missing());
    }
    let data = rows_of(value, columns)?;
    let mut copies = String::with_capacity(row_xml.len() * data.len());
    for line in &data {
        let mut copy = row_xml.to_string();
        for (offset, text) in line.iter().enumerate().rev() {
            let range = cells[first as usize + offset].clone();
            let rewritten = rewrite_cell(&row_xml[range.clone()], text)?;
            copy.replace_range(range, &rewritten);
        }
        copies.push_str(&copy);
    }
    let mut output = String::with_capacity(xml.len() + copies.len());
    output.push_str(&xml[..start]);
    output.push_str(&copies);
    output.push_str(&xml[end..]);
    Ok(output)
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

/// A cell split into its parts: markup before the content, the cell
/// properties, the content, and the closing markup.
struct CellParts<'a> {
    open: &'a str,
    properties: &'a str,
    inner: &'a str,
    close: &'a str,
}

fn split_cell(cell: &str) -> Result<CellParts<'_>, String> {
    let open_end = cell
        .find('>')
        .ok_or("The Word table markup is not supported")?
        + 1;
    let close_start = cell
        .rfind("</w:tc")
        .ok_or("The Word table markup is not supported")?;
    let inner = &cell[open_end..close_start];
    let properties = TC_PR.find(inner).map_or("", |found| found.as_str());
    Ok(CellParts {
        open: &cell[..open_end],
        properties,
        inner,
        close: &cell[close_start..],
    })
}

/// Why a cell cannot be written, as a short code the editor translates.
fn cell_problem(parts: &CellParts<'_>) -> Option<&'static str> {
    if VERTICAL_CONTINUE.is_match(parts.properties) {
        Some("verticalMerge")
    } else if UNSAFE_CONTENT
        .iter()
        .any(|marker| parts.inner.contains(marker))
    {
        Some("content")
    } else {
        None
    }
}

fn rewrite_cell(cell: &str, value: &str) -> Result<String, String> {
    let parts = split_cell(cell)?;
    match cell_problem(&parts) {
        Some("verticalMerge") => {
            return Err("The Word cell continues a vertically merged cell".to_string());
        }
        Some(_) => {
            return Err("The Word cell holds content that cannot be replaced safely".to_string());
        }
        None => {}
    }
    let body = parts.inner.replacen(parts.properties, "", 1);
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
        "{}{}{paragraph}{}",
        parts.open, parts.properties, parts.close
    ))
}

/// One Word table cell as the editor shows it.
pub struct WordCell {
    pub text: String,
    pub col_span: u32,
    /// `None` when the cell can be written, else a reason code.
    pub problem: Option<&'static str>,
}

/// The body tables of `document.xml`: tables, rows, cells.
pub fn word_tables(xml: &str) -> Result<Vec<Vec<Vec<WordCell>>>, String> {
    let mut tables = Vec::new();
    for table in element_ranges(xml, "tbl")? {
        let table_xml = &xml[table];
        let mut rows = Vec::new();
        for row in element_ranges(table_xml, "tr")? {
            let row_xml = &table_xml[row];
            let mut cells = Vec::new();
            for cell in element_ranges(row_xml, "tc")? {
                let parts = split_cell(&row_xml[cell])?;
                let text = FIRST_PARAGRAPH_ALL
                    .find_iter(parts.inner)
                    .map(|paragraph| run_text(paragraph.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                let col_span = GRID_SPAN
                    .captures(parts.properties)
                    .and_then(|found| found[1].parse().ok())
                    .unwrap_or(1);
                cells.push(WordCell {
                    text,
                    col_span,
                    problem: cell_problem(&parts),
                });
            }
            rows.push(cells);
        }
        tables.push(rows);
    }
    Ok(tables)
}

fn run_text(paragraph: &str) -> String {
    TEXT_NODE
        .captures_iter(paragraph)
        .map(|found| {
            quick_xml::escape::unescape(&found[1])
                .map_or_else(|_| found[1].to_string(), |text| text.into_owned())
        })
        .collect()
}

/// `(14, 3)` (row, column) becomes `C14`.
pub fn address_of(row: u32, column: u32) -> String {
    let mut letters = String::new();
    let mut rest = column;
    while rest > 0 {
        let digit = u8::try_from((rest - 1) % 26).unwrap_or(0);
        letters.insert(0, char::from(b'A' + digit));
        rest = (rest - 1) / 26;
    }
    format!("{letters}{row}")
}

#[cfg(test)]
pub(crate) mod tests;
