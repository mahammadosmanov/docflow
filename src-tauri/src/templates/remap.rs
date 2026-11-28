//! After a template file is replaced, checks every marked cell against the
//! new file (ADR 0058): found where it was, moved with its label, or missing.
//!
//! A marked cell is usually a value cell, and its value changes from copy to
//! copy, so it is matched by its label: the nearest filled cell to its left in
//! the same row. Same label beside the same place is found. The label
//! somewhere else, exactly once, is moved. Anything else is missing and needs
//! the operator.

use serde::{Deserialize, Serialize};

use super::{
    FileKind,
    anchors::{CellAnchor, address_of, cell_coordinate},
    copies::{Group, read},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Found,
    Moved,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorCheck {
    pub key: String,
    pub status: CheckStatus,
    /// Where the cell is now, when it moved.
    pub moved_to: Option<CellAnchor>,
}

/// The group index and `(row, column)` an anchor points at in `groups`.
fn place_of(kind: FileKind, anchor: &CellAnchor, groups: &[Group]) -> Option<(usize, (u32, u32))> {
    match kind {
        FileKind::Xlsx => {
            let name = anchor.sheet.as_deref()?;
            let index = groups.iter().position(|group| group.name == name)?;
            Some((index, cell_coordinate(anchor.address.as_deref()?)?))
        }
        FileKind::Docx => {
            let index = usize::try_from(anchor.table?).ok()?;
            (index < groups.len()).then_some((index, (anchor.row?, anchor.cell?)))
        }
    }
}

fn anchor_at(kind: FileKind, group_index: usize, group: &Group, place: (u32, u32)) -> CellAnchor {
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

/// The nearest filled cell to the left: its text and column.
fn context(group: &Group, place: (u32, u32), first_column: u32) -> Option<(String, u32)> {
    (first_column..place.1).rev().find_map(|column| {
        let text = group.texts.get(&(place.0, column))?.trim();
        (!text.is_empty()).then(|| (text.to_string(), column))
    })
}

pub fn check(
    kind: FileKind,
    old: &[u8],
    new: &[u8],
    fields: &[(String, CellAnchor)],
) -> Result<Vec<AnchorCheck>, String> {
    let old_groups = read(kind, old)?;
    let new_groups = read(kind, new)?;
    let first_column = u32::from(kind == FileKind::Xlsx);
    let mut results = Vec::with_capacity(fields.len());
    for (key, anchor) in fields {
        let old_context = place_of(kind, anchor, &old_groups).and_then(|(index, place)| {
            context(&old_groups[index], place, first_column).map(|found| (place, found))
        });
        let here = place_of(kind, anchor, &new_groups);
        let same_label = |place: (u32, u32), group: &Group| match &old_context {
            Some((_, (label, _))) => {
                context(group, place, first_column).is_some_and(|(text, _)| &text == label)
            }
            None => true,
        };
        let status = if let Some((index, place)) = here
            && new_groups[index].writable(place)
            && same_label(place, &new_groups[index])
        {
            (CheckStatus::Found, None)
        } else if let Some((place, (label, label_column))) = &old_context {
            let offset = place.1 - label_column;
            let mut candidates = Vec::new();
            for (index, group) in new_groups.iter().enumerate() {
                for (&(row, column), text) in &group.texts {
                    let target = (row, column + offset);
                    if text.trim() == label
                        && group.writable(target)
                        && (kind == FileKind::Docx || target.1 <= 16_384)
                    {
                        candidates.push(CellAnchor {
                            repeat_columns: anchor.repeat_columns,
                            ..anchor_at(kind, index, group, target)
                        });
                    }
                }
            }
            match candidates.as_slice() {
                [only] => (CheckStatus::Moved, Some(only.clone())),
                _ => (CheckStatus::Missing, None),
            }
        } else {
            (CheckStatus::Missing, None)
        };
        results.push(AnchorCheck {
            key: key.clone(),
            status: status.0,
            moved_to: status.1,
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests;
