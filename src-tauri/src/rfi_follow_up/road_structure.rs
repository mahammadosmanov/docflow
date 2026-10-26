//! Planning of Road part rows in the Follow Up workbook.
//!
//! The RFI Log decides which Roads and parts exist. Follow Up keeps one row per part.
//! Missing parts are inserted, a partless row that now needs parts becomes
//! `Road NN (Part 1)`, and rows are never removed (an empty part is still a part).
//! Operations are returned bottom-up in the workbook's current row numbers so each one
//! can be applied in order without renumbering the ones before it.

use std::collections::{BTreeMap, BTreeSet};

use crate::ooxml::{CellText, RowOp};

use super::road::{RoadRef, road_refs};

/// What the RFI Log requires for one Road number.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct RoadNeed {
    /// Parts named explicitly by some RFI (`Road 06 (Part 2)`).
    pub(super) parts: BTreeSet<u32>,
    /// Some RFI names the Road with no part.
    pub(super) partless: bool,
}

/// A data row of the Follow Up Road sheet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RoadRow {
    pub(super) row: u32,
    pub(super) road: u32,
    pub(super) part: Option<u32>,
    /// The ZONE cell (column A) exists but holds nothing.
    pub(super) zone_blank: bool,
}

/// The project's Roads, all of which must have a Follow Up row even before any RFI exists.
pub(super) const PLANNED_ROADS: std::ops::RangeInclusive<u32> = 1..=21;

/// Fixed zone of each planned Road (owner decision, 2026-10-26).
pub(super) fn zone_for_road(road: u32) -> Option<&'static str> {
    match road {
        1..=5 | 21 => Some("North"),
        6..=13 => Some("Center"),
        14..=20 => Some("South"),
        _ => None,
    }
}

pub(super) const ZONES: [&str; 3] = ["North", "Center", "South"];

pub(super) fn road_label(road: u32, part: Option<u32>) -> String {
    match part {
        Some(part) => format!("Road {road:02} (Part {part})"),
        None => format!("Road {road:02}"),
    }
}

fn label_text(road: u32, part: Option<u32>) -> Vec<CellText> {
    let mut texts = Vec::new();
    if let Some(zone) = zone_for_road(road) {
        texts.push(CellText {
            column: 1,
            text: zone.to_string(),
        });
    }
    texts.push(CellText {
        column: 2,
        text: road_label(road, part),
    });
    texts
}

/// Fills ZONE cells that exist but are empty.
pub(super) fn plan_zone_ops(sheet: &str, rows: &[RoadRow]) -> Vec<RowOp> {
    rows.iter()
        .filter(|row| row.zone_blank)
        .filter_map(|row| {
            Some(RowOp::SetText {
                sheet: sheet.to_string(),
                row: row.row,
                column: 1,
                text: zone_for_road(row.road)?.to_string(),
            })
        })
        .collect()
}

/// Merges the Road references of Log tags into per-Road needs.
pub(super) fn needs_from_tags<'a>(
    tags: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<u32, RoadNeed> {
    let mut needs = BTreeMap::<u32, RoadNeed>::new();
    for tag in tags {
        for RoadRef { road, part } in road_refs(tag) {
            let need = needs.entry(road).or_default();
            match part {
                Some(part) => {
                    need.parts.insert(part);
                }
                None => need.partless = true,
            }
        }
    }
    needs
}

/// Structural operations needed to give every Road in `needs` the rows it requires.
/// `rows` must be in sheet order.
pub(super) fn plan_road_structure(
    sheet: &str,
    rows: &[RoadRow],
    needs: &BTreeMap<u32, RoadNeed>,
) -> Vec<RowOp> {
    // (anchor_after_row, tie-break) -> op; sorted descending at the end.
    let mut inserts = Vec::<(u32, (u32, u32), RowOp)>::new();
    let mut renames = Vec::<(u32, RowOp)>::new();
    let first_row = rows.first().map(|row| row.row);

    for (&road, need) in needs {
        let existing: Vec<RoadRow> = rows
            .iter()
            .copied()
            .filter(|row| row.road == road)
            .collect();
        let partless_row = existing.iter().find(|row| row.part.is_none()).copied();
        let mut present: BTreeMap<u32, u32> = existing
            .iter()
            .filter_map(|row| row.part.map(|part| (part, row.row)))
            .collect();

        if existing.is_empty() {
            let Some(template) =
                previous_block_end(rows, road).or_else(|| first_row.map(|row| (row, false)))
            else {
                continue;
            };
            let wanted: Vec<Option<u32>> = if need.parts.is_empty() {
                vec![None]
            } else {
                need.parts.iter().map(|part| Some(*part)).collect()
            };
            let (anchor, before) = template;
            let (after_row, template_row) = if before {
                (anchor - 1, anchor)
            } else {
                (anchor, anchor)
            };
            for (order, part) in wanted.into_iter().enumerate() {
                inserts.push((
                    after_row,
                    (road, u32::try_from(order).unwrap_or(0)),
                    RowOp::InsertRow {
                        sheet: sheet.to_string(),
                        after_row,
                        template_row,
                        copy_columns: Vec::new(),
                        texts: label_text(road, part),
                    },
                ));
            }
            continue;
        }

        if need.parts.is_empty() {
            continue;
        }
        if let Some(row) = partless_row.filter(|_| present.is_empty()) {
            renames.push((
                row.row,
                RowOp::SetText {
                    sheet: sheet.to_string(),
                    row: row.row,
                    column: 2,
                    text: road_label(road, Some(1)),
                },
            ));
            present.insert(1, row.row);
        }
        if present.is_empty() {
            continue;
        }
        for &part in need.parts.iter().filter(|part| !present.contains_key(part)) {
            // Insert after the nearest lower part, or before the first part.
            let lower = present.range(..part).next_back().map(|(_, row)| *row);
            let (after_row, template_row) = match lower {
                Some(row) => (row, row),
                None => {
                    let first = *present.values().min().expect("present is not empty");
                    (first - 1, first)
                }
            };
            inserts.push((
                after_row,
                (road, part),
                RowOp::InsertRow {
                    sheet: sheet.to_string(),
                    after_row,
                    template_row,
                    copy_columns: Vec::new(),
                    texts: label_text(road, Some(part)),
                },
            ));
            // Later (higher) parts anchor after this one; they share its row number for
            // ordering, which the descending sort below resolves correctly.
        }
    }

    let mut ops: Vec<(u32, (u32, u32), RowOp)> = renames
        .into_iter()
        .map(|(row, op)| (row, (0, 0), op))
        .collect();
    ops.extend(inserts);
    // Bottom-up by row; for the same anchor, later Roads/parts first so inserting each
    // at the anchor leaves them in ascending order.
    ops.sort_by(|left, right| right.0.cmp(&left.0).then(right.1.cmp(&left.1)));
    ops.into_iter().map(|(_, _, op)| op).collect()
}

/// The last row of the nearest lower-numbered Road, or the first Road row (insert before
/// it) when the new Road sorts first. `(row, insert_before)`.
fn previous_block_end(rows: &[RoadRow], road: u32) -> Option<(u32, bool)> {
    rows.iter()
        .filter(|row| row.road < road)
        .max_by_key(|row| (row.road, row.row))
        .map(|row| (row.row, false))
        .or_else(|| rows.first().map(|row| (row.row, true)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(row: u32, road: u32, part: Option<u32>) -> RoadRow {
        RoadRow {
            row,
            road,
            part,
            zone_blank: false,
        }
    }

    fn need(parts: &[u32], partless: bool) -> RoadNeed {
        RoadNeed {
            parts: parts.iter().copied().collect(),
            partless,
        }
    }

    fn sheet() -> Vec<RoadRow> {
        vec![
            row(5, 1, Some(1)),
            row(6, 1, Some(2)),
            row(7, 2, None),
            row(8, 3, Some(1)),
            row(9, 3, Some(2)),
            row(10, 5, None),
        ]
    }

    fn insert_text(op: &RowOp) -> (u32, u32, String) {
        match op {
            RowOp::InsertRow {
                after_row,
                template_row,
                texts,
                ..
            } => (
                *after_row,
                *template_row,
                texts.last().expect("label text").text.clone(),
            ),
            RowOp::SetText { row, text, .. } => (*row, 0, text.clone()),
        }
    }

    #[test]
    fn partless_needs_change_nothing() {
        let needs = BTreeMap::from([
            (1, need(&[], true)),
            (2, need(&[], true)),
            (5, need(&[], true)),
        ]);
        assert!(plan_road_structure("Road", &sheet(), &needs).is_empty());
    }

    #[test]
    fn existing_parts_change_nothing_and_unreferenced_parts_are_kept() {
        let needs = BTreeMap::from([(1, need(&[1], false)), (3, need(&[2], true))]);
        assert!(plan_road_structure("Road", &sheet(), &needs).is_empty());
    }

    #[test]
    fn a_new_part_goes_after_the_nearest_lower_part() {
        let needs = BTreeMap::from([(3, need(&[3], false))]);
        let ops = plan_road_structure("Road", &sheet(), &needs);
        assert_eq!(ops.len(), 1);
        assert_eq!(insert_text(&ops[0]), (9, 9, "Road 03 (Part 3)".to_string()));
    }

    #[test]
    fn several_new_parts_are_ordered_ascending_by_inserting_descending() {
        let needs = BTreeMap::from([(1, need(&[3, 4], false))]);
        let ops = plan_road_structure("Road", &sheet(), &needs);
        let texts: Vec<String> = ops.iter().map(|op| insert_text(op).2).collect();
        assert_eq!(texts, ["Road 01 (Part 4)", "Road 01 (Part 3)"]);
        assert!(ops.iter().all(|op| insert_text(op).0 == 6));
    }

    #[test]
    fn a_lower_part_is_inserted_before_the_first_existing_part() {
        let needs = BTreeMap::from([(3, need(&[0], false)), (1, need(&[], false))]);
        let ops = plan_road_structure("Road", &sheet(), &needs);
        assert_eq!(insert_text(&ops[0]), (7, 8, "Road 03 (Part 0)".to_string()));
    }

    #[test]
    fn a_partless_row_that_needs_parts_becomes_part_one_and_gains_the_rest() {
        let needs = BTreeMap::from([(2, need(&[2, 3], false))]);
        let ops = plan_road_structure("Road", &sheet(), &needs);
        let described: Vec<(u32, u32, String)> = ops.iter().map(insert_text).collect();
        assert_eq!(
            described,
            [
                (7, 7, "Road 02 (Part 3)".to_string()),
                (7, 7, "Road 02 (Part 2)".to_string()),
                (7, 0, "Road 02 (Part 1)".to_string()),
            ]
        );
        assert!(matches!(ops[2], RowOp::SetText { .. }));
    }

    #[test]
    fn a_new_road_is_placed_after_the_previous_road_in_number_order() {
        let needs = BTreeMap::from([(4, need(&[], true)), (6, need(&[1], false))]);
        let ops = plan_road_structure("Road", &sheet(), &needs);
        let described: Vec<(u32, u32, String)> = ops.iter().map(insert_text).collect();
        // Road 6 follows Road 5 (row 10); Road 4 follows Road 3's last part (row 9).
        assert_eq!(
            described,
            [
                (10, 10, "Road 06 (Part 1)".to_string()),
                (9, 9, "Road 04".to_string()),
            ]
        );
        for op in &ops {
            let RowOp::InsertRow { texts, .. } = op else {
                panic!("insert expected");
            };
            assert_eq!(texts.len(), 2, "zone and label");
            assert_eq!(texts[0].column, 1);
        }
    }

    #[test]
    fn a_road_that_sorts_first_is_inserted_before_the_first_row() {
        let rows = vec![row(5, 2, None), row(6, 3, None)];
        let needs = BTreeMap::from([(1, need(&[], true))]);
        let ops = plan_road_structure("Road", &rows, &needs);
        assert_eq!(insert_text(&ops[0]), (4, 5, "Road 01".to_string()));
    }

    #[test]
    fn two_new_roads_after_the_same_row_stay_in_number_order() {
        let needs = BTreeMap::from([(6, need(&[], true)), (7, need(&[], true))]);
        let ops = plan_road_structure("Road", &sheet(), &needs);
        let texts: Vec<String> = ops.iter().map(|op| insert_text(op).2).collect();
        assert_eq!(texts, ["Road 07", "Road 06"]);
    }

    #[test]
    fn zones_follow_the_fixed_road_table() {
        for (road, zone) in [
            (1, "North"),
            (5, "North"),
            (6, "Center"),
            (13, "Center"),
            (14, "South"),
            (20, "South"),
            (21, "North"),
        ] {
            assert_eq!(zone_for_road(road), Some(zone), "Road {road}");
        }
        assert_eq!(zone_for_road(22), None);
    }

    #[test]
    fn blank_zone_cells_are_filled_from_the_table() {
        let mut rows = sheet();
        rows[2].zone_blank = true; // Road 02
        rows[5].zone_blank = true; // Road 05
        let ops = plan_zone_ops("Road", &rows);
        let described: Vec<(u32, String)> = ops
            .iter()
            .map(|op| match op {
                RowOp::SetText {
                    row, text, column, ..
                } => {
                    assert_eq!(*column, 1);
                    (*row, text.clone())
                }
                RowOp::InsertRow { .. } => panic!("set text expected"),
            })
            .collect();
        assert_eq!(
            described,
            [(7, "North".to_string()), (10, "North".to_string())]
        );
    }

    #[test]
    fn needs_merge_tags() {
        let needs = needs_from_tags(["Road 06", "Road 06 (Part 2), (Part 3)", "PCS-01", "Road 08"]);
        assert_eq!(needs[&6], need(&[2, 3], true));
        assert_eq!(needs[&8], need(&[], true));
        assert_eq!(needs.len(), 2);
    }
}
