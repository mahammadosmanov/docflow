//! A bounded, read-only picture of a template file for the mark-editable
//! screen (ADR 0058): Word tables and Excel sheets with their cells, and a
//! few suggestions. The webview only sees this summary, never the file.

use std::io::Cursor;

use serde::Serialize;

use super::{
    FileKind,
    anchors::{CellAnchor, address_of, word_tables},
    field_map::read_text_part,
};

const MAX_SHEETS: usize = 30;
const MAX_ROWS: u32 = 300;
const MAX_COLUMNS: u32 = 80;
const MAX_CELLS: usize = 6_000;
const MAX_TABLES: usize = 60;
const MAX_TABLE_ROWS: usize = 600;
const MAX_TEXT_CHARS: usize = 120;
const MAX_SUGGESTIONS: usize = 5;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Structure {
    Xlsx {
        sheets: Vec<SheetView>,
        suggestions: Vec<Suggestion>,
    },
    Docx {
        tables: Vec<TableView>,
        suggestions: Vec<Suggestion>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetView {
    pub name: String,
    pub max_row: u32,
    pub max_column: u32,
    pub truncated: bool,
    pub protected: bool,
    pub cells: Vec<SheetCell>,
    pub merges: Vec<Merge>,
}

#[derive(Debug, Serialize)]
pub struct SheetCell {
    pub row: u32,
    pub column: u32,
    pub text: String,
    pub formula: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Merge {
    pub row: u32,
    pub column: u32,
    pub row_span: u32,
    pub col_span: u32,
}

#[derive(Debug, Serialize)]
pub struct TableView {
    pub rows: Vec<RowView>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct RowView {
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub text: String,
    pub col_span: u32,
    pub problem: Option<&'static str>,
}

/// A cell the editor offers to make editable first: it holds an RFI code.
/// `value` is the cell text with the code replaced by the RFI Log token, so
/// the cell's own label stays.
#[derive(Debug, Serialize)]
pub struct Suggestion {
    pub key: &'static str,
    pub label: &'static str,
    pub value: String,
    pub anchor: CellAnchor,
}

fn short(text: &str) -> String {
    text.chars().take(MAX_TEXT_CHARS).collect()
}

/// The text with every RFI code in it written as `{rfiNo}`.
fn rfi_value(text: &str) -> Option<String> {
    let codes = crate::rfi_code::exact_codes(text);
    (!codes.is_empty()).then(|| {
        codes.iter().fold(text.to_string(), |value, code| {
            value.replace(code, "{rfiNo}")
        })
    })
}

fn suggestion(text: &str, anchor: CellAnchor) -> Option<Suggestion> {
    Some(Suggestion {
        key: "rfiNo",
        label: "RFI No.",
        value: short(&rfi_value(text)?),
        anchor,
    })
}

pub fn cells(kind: FileKind, bytes: &[u8]) -> Result<Structure, String> {
    match kind {
        FileKind::Docx => word(bytes),
        FileKind::Xlsx => excel(bytes),
    }
}

fn word(bytes: &[u8]) -> Result<Structure, String> {
    let xml = read_text_part(bytes, "word/document.xml")?;
    let mut tables = Vec::new();
    let mut suggestions = Vec::new();
    for (table_index, table) in word_tables(&xml)?.into_iter().take(MAX_TABLES).enumerate() {
        let truncated = table.len() > MAX_TABLE_ROWS;
        let mut rows = Vec::new();
        for (row_index, row) in table.into_iter().take(MAX_TABLE_ROWS).enumerate() {
            let mut cells = Vec::new();
            for (cell_index, cell) in row.into_iter().enumerate() {
                if cell.problem.is_none() && suggestions.len() < MAX_SUGGESTIONS {
                    let anchor = CellAnchor {
                        table: u32::try_from(table_index).ok(),
                        row: u32::try_from(row_index).ok(),
                        cell: u32::try_from(cell_index).ok(),
                        ..CellAnchor::default()
                    };
                    suggestions.extend(suggestion(&cell.text, anchor));
                }
                cells.push(TableCell {
                    text: short(&cell.text),
                    col_span: cell.col_span,
                    problem: cell.problem,
                });
            }
            rows.push(RowView { cells });
        }
        tables.push(TableView { rows, truncated });
    }
    Ok(Structure::Docx {
        tables,
        suggestions,
    })
}

fn excel(bytes: &[u8]) -> Result<Structure, String> {
    let invalid = |error: crate::ooxml::OoxmlError| format!("Cannot read the workbook: {error}");
    let order = crate::ooxml::ordered_workbook_discovery(&mut Cursor::new(bytes))
        .map_err(invalid)?
        .worksheets;
    let workbook =
        crate::ooxml::read_workbook_from_reader(&mut Cursor::new(bytes)).map_err(invalid)?;
    let mut sheets = Vec::new();
    let mut suggestions = Vec::new();
    for descriptor in order.iter().take(MAX_SHEETS) {
        let Some(sheet) = workbook.sheet(&descriptor.name) else {
            continue;
        };
        let max_row = sheet.max_row.min(MAX_ROWS);
        let max_column = sheet.max_column.min(MAX_COLUMNS);
        let mut cells = Vec::new();
        let mut cut = sheet.max_row > MAX_ROWS || sheet.max_column > MAX_COLUMNS;
        for (&(row, column), cell) in &sheet.cells {
            if row > max_row || column > max_column {
                continue;
            }
            let formula = cell.formula.is_some();
            if cell.value.is_empty() && !formula {
                continue;
            }
            if cells.len() >= MAX_CELLS {
                cut = true;
                break;
            }
            let writable = !formula
                && !sheet.has_sheet_protection
                && sheet
                    .merged_range_containing(row, column)
                    .is_none_or(|range| (range.first_row, range.first_column) == (row, column));
            if writable && suggestions.len() < MAX_SUGGESTIONS {
                let anchor = CellAnchor {
                    sheet: Some(descriptor.name.clone()),
                    address: Some(address_of(row, column)),
                    ..CellAnchor::default()
                };
                suggestions.extend(suggestion(&cell.value, anchor));
            }
            cells.push(SheetCell {
                row,
                column,
                text: short(&cell.value),
                formula,
            });
        }
        let merges = sheet
            .merged_ranges
            .iter()
            .filter(|range| range.first_row <= max_row && range.first_column <= max_column)
            .map(|range| Merge {
                row: range.first_row,
                column: range.first_column,
                row_span: range.last_row - range.first_row + 1,
                col_span: range.last_column - range.first_column + 1,
            })
            .collect();
        sheets.push(SheetView {
            name: descriptor.name.clone(),
            max_row,
            max_column,
            truncated: cut,
            protected: sheet.has_sheet_protection,
            cells,
            merges,
        });
    }
    Ok(Structure::Xlsx {
        sheets,
        suggestions,
    })
}

#[cfg(test)]
mod tests;
