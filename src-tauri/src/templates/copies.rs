//! Suggest editable cells by comparing filled copies of one form (ADR 0058).
//!
//! The draft file is one filled copy. Other copies of the same form, picked by
//! the operator, show which cells change from document to document. Copies are
//! read here only; they are never uploaded or stored.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

use serde::Serialize;

use super::{
    FileKind,
    anchors::{CellAnchor, address_of, word_tables},
    field_map::read_text_part,
    verify_package,
};

pub const MAX_COPIES: usize = 10;
const MAX_SUGGESTIONS: usize = 60;
const MAX_SAMPLES: usize = 3;
const MAX_SAMPLE_CHARS: usize = 60;
const MAX_LABEL_CHARS: usize = 40;
const MAX_START_CHARS: usize = 119;

#[derive(Debug, Serialize)]
pub struct CopySuggestion {
    pub anchor: CellAnchor,
    pub label: String,
    pub value: String,
    pub samples: Vec<String>,
    /// Every filled value holds an RFI code, so the operator can map it to the
    /// RFI Log. The start value then has the code written as `{rfiNo}`.
    pub rfi: bool,
}

/// Cells of one sheet or table: `(row, column)` to text. Excel is one-based,
/// Word zero-based.
#[derive(Default)]
pub(super) struct Group {
    pub(super) name: String,
    pub(super) texts: BTreeMap<(u32, u32), String>,
    /// Cells that cannot be written (formula, merge tail, protection, unsafe).
    pub(super) locked: BTreeSet<(u32, u32)>,
    pub(super) protected: bool,
    pub(super) merges: Vec<(u32, u32, u32, u32)>,
}

impl Group {
    pub(super) fn writable(&self, place: (u32, u32)) -> bool {
        !self.protected
            && !self.locked.contains(&place)
            && !self
                .merges
                .iter()
                .any(|&(row, column, last_row, last_column)| {
                    (row..=last_row).contains(&place.0)
                        && (column..=last_column).contains(&place.1)
                        && (row, column) != place
                })
    }
}

pub(super) fn read(kind: FileKind, bytes: &[u8]) -> Result<Vec<Group>, String> {
    verify_package(kind, bytes)
        .map_err(|_| "A copy is not a valid Word or Excel file".to_string())?;
    match kind {
        FileKind::Docx => read_word(bytes),
        FileKind::Xlsx => read_excel(bytes),
    }
}

fn read_word(bytes: &[u8]) -> Result<Vec<Group>, String> {
    let xml = read_text_part(bytes, "word/document.xml")?;
    Ok(word_tables(&xml)?
        .into_iter()
        .enumerate()
        .map(|(index, table)| {
            let mut group = Group {
                name: index.to_string(),
                ..Group::default()
            };
            for (row, cells) in table.into_iter().enumerate() {
                for (column, cell) in cells.into_iter().enumerate() {
                    let place = (row as u32, column as u32);
                    if cell.problem.is_some() {
                        group.locked.insert(place);
                    }
                    group.texts.insert(place, cell.text);
                }
            }
            group
        })
        .collect())
}

fn read_excel(bytes: &[u8]) -> Result<Vec<Group>, String> {
    let invalid = |error: crate::ooxml::OoxmlError| format!("Cannot read the workbook: {error}");
    let order = crate::ooxml::ordered_workbook_discovery(&mut Cursor::new(bytes))
        .map_err(invalid)?
        .worksheets;
    let workbook =
        crate::ooxml::read_workbook_from_reader(&mut Cursor::new(bytes)).map_err(invalid)?;
    let mut groups = Vec::new();
    for descriptor in order {
        let Some(sheet) = workbook.sheet(&descriptor.name) else {
            continue;
        };
        let mut group = Group {
            name: descriptor.name,
            protected: sheet.has_sheet_protection,
            merges: sheet
                .merged_ranges
                .iter()
                .map(|range| {
                    (
                        range.first_row,
                        range.first_column,
                        range.last_row,
                        range.last_column,
                    )
                })
                .collect(),
            ..Group::default()
        };
        for (&place, cell) in &sheet.cells {
            if cell.formula.is_some() {
                group.locked.insert(place);
            }
            group.texts.insert(place, cell.value.clone());
        }
        groups.push(group);
    }
    Ok(groups)
}

fn short(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

fn anchor_of(kind: FileKind, group_index: usize, group: &Group, place: (u32, u32)) -> CellAnchor {
    match kind {
        FileKind::Xlsx => CellAnchor {
            sheet: Some(group.name.clone()),
            address: Some(address_of(place.0, place.1)),
            ..CellAnchor::default()
        },
        FileKind::Docx => CellAnchor {
            table: u32::try_from(group_index).ok(),
            row: Some(place.0),
            cell: Some(place.1),
            ..CellAnchor::default()
        },
    }
}

/// The nearest filled cell to the left that reads the same in every file.
fn label_for(group: &Group, others: &[&Group], place: (u32, u32), first_column: u32) -> String {
    for column in (first_column..place.1).rev() {
        let key = (place.0, column);
        let text = group.texts.get(&key).map_or("", |text| text.trim());
        let constant = others
            .iter()
            .all(|other| other.texts.get(&key).map_or("", |text| text.trim()) == text);
        if !text.is_empty() && constant {
            return short(text.trim_end_matches(':').trim(), MAX_LABEL_CHARS);
        }
    }
    String::new()
}

pub fn compare(
    kind: FileKind,
    base: &[u8],
    copies: &[Vec<u8>],
) -> Result<Vec<CopySuggestion>, String> {
    if copies.len() > MAX_COPIES {
        return Err(format!("Pick at most {MAX_COPIES} copies"));
    }
    let base_groups = read(kind, base)?;
    let copy_groups = copies
        .iter()
        .map(|bytes| read(kind, bytes))
        .collect::<Result<Vec<_>, _>>()?;
    let first_column = u32::from(kind == FileKind::Xlsx);
    let mut found = Vec::new();
    for (index, group) in base_groups.iter().enumerate() {
        let matching = copy_groups
            .iter()
            .filter_map(|groups| groups.iter().find(|other| other.name == group.name))
            .collect::<Vec<_>>();
        let all = std::iter::once(group)
            .chain(matching.iter().copied())
            .collect::<Vec<_>>();
        let places = all
            .iter()
            .flat_map(|item| item.texts.keys().copied())
            .collect::<BTreeSet<_>>();
        for place in places {
            let values = all
                .iter()
                .map(|item| item.texts.get(&place).map_or("", |text| text.trim()))
                .collect::<Vec<_>>();
            if values.iter().all(|value| *value == values[0]) || !group.writable(place) {
                continue;
            }
            if found.len() >= MAX_SUGGESTIONS {
                return Ok(found);
            }
            let mut filled = Vec::<&str>::new();
            for value in values.iter().filter(|value| !value.is_empty()) {
                if !filled.contains(value) {
                    filled.push(value);
                }
            }
            let rfi = filled.len() >= 2
                && filled
                    .iter()
                    .all(|value| !crate::rfi_code::exact_codes(value).is_empty());
            let own = group.texts.get(&place).map_or("", |text| text.trim());
            let value = if rfi {
                let source = if own.is_empty() { filled[0] } else { own };
                crate::rfi_code::exact_codes(source)
                    .iter()
                    .fold(source.to_string(), |text, code| {
                        text.replace(code, "{rfiNo}")
                    })
            } else if own.chars().count() <= MAX_START_CHARS {
                own.to_string()
            } else {
                String::new()
            };
            found.push(CopySuggestion {
                anchor: anchor_of(kind, index, group, place),
                label: label_for(group, &all, place, first_column),
                value,
                samples: filled
                    .iter()
                    .take(MAX_SAMPLES)
                    .map(|value| short(value, MAX_SAMPLE_CHARS))
                    .collect(),
                rfi,
            });
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
