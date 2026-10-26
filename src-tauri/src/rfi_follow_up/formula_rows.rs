//! Maintenance of formula columns on the Follow Up sheets (CIV, PCU, Road): a data row
//! whose formula cell is empty while the column is otherwise formula-driven gets the
//! column's formula, translated from the nearest formula above. Cells that hold any
//! value or formula are never touched.

use super::*;
use crate::ooxml::translate_formula;

const FORMULA_SHEETS: [&str; 3] = ["CIV", "PCU", "ROAD"];
/// A column counts as formula-driven when this share (percent) of its span has formulas.
const MIN_FORMULA_SHARE_PERCENT: usize = 60;
const MIN_FORMULA_ROWS: usize = 3;

fn is_blank(cell: Option<&CellData>) -> bool {
    cell.is_none_or(|cell| cell.value.trim().is_empty() && cell.formula.is_none())
}

pub(super) fn formula_repair_rows(
    layouts: &[FollowUpLayout],
    target_workbook: &WorkbookData,
) -> Vec<FollowUpRow> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for layout in layouts {
        let name = layout.worksheet.name.trim().to_uppercase();
        if !FORMULA_SHEETS.contains(&name.as_str()) || !seen.insert(layout.worksheet.name.clone()) {
            continue;
        }
        let Some(sheet) = target_workbook.sheet(&layout.worksheet.name) else {
            continue;
        };
        let last_lifecycle_column = layouts
            .iter()
            .filter(|other| other.worksheet == layout.worksheet)
            .map(|other| other.status_column)
            .max()
            .unwrap_or(layout.status_column);
        let remarks_column = find_header_column(sheet, &["REMARKS", "REMARK"]);
        let first_row = layout.header_row + 1;
        let last_row = sheet
            .max_row
            .min(layout.header_row + MAX_FOLLOW_UP_DATA_ROWS);

        // Formula-driven columns to the right of the lifecycle pairs.
        let mut fixes = BTreeMap::<u32, Vec<(u32, String, Option<String>)>>::new();
        for column in (last_lifecycle_column + 1)..=sheet.max_column {
            if Some(column) == remarks_column {
                continue;
            }
            let formula_rows = (first_row..=last_row)
                .filter(|row| {
                    sheet
                        .cell(*row, column)
                        .is_some_and(|cell| cell.formula.is_some())
                })
                .collect::<Vec<_>>();
            let (Some(first), Some(last)) = (formula_rows.first(), formula_rows.last()) else {
                continue;
            };
            let span = (last - first + 1) as usize;
            if formula_rows.len() < MIN_FORMULA_ROWS
                || formula_rows.len() * 100 < span * MIN_FORMULA_SHARE_PERCENT
            {
                continue;
            }
            for row in *first..=*last {
                let cell = sheet.cell(row, column);
                // Only an existing, styled, truly empty cell inside a populated data row.
                if !is_blank(cell) || cell.is_none() {
                    continue;
                }
                let populated =
                    (1..=last_lifecycle_column).any(|left| !is_blank(sheet.cell(row, left)));
                if !populated {
                    continue;
                }
                let master = (first_row..row).rev().find_map(|above| {
                    let text = sheet.cell(above, column)?.formula.as_deref()?;
                    (!text.trim().is_empty()).then(|| (above, text.to_string()))
                });
                let Some((master_row, text)) = master else {
                    continue;
                };
                let Some(translated) =
                    translate_formula(&text, i64::from(row) - i64::from(master_row), 0)
                else {
                    continue;
                };
                fixes.entry(row).or_default().push((
                    column,
                    translated,
                    cell.and_then(|cell| cell.style.clone()),
                ));
            }
        }
        for (row, cells) in fixes {
            let label = (1..=last_lifecycle_column)
                .filter_map(|column| sheet.cell(row, column))
                .map(|cell| cell.value.trim())
                .find(|value| !value.is_empty() && value.chars().any(char::is_alphabetic))
                .unwrap_or("-")
                .to_string();
            rows.push(FollowUpRow {
                row_id: format!("rfi-follow-up-formula:{}:{row}", layout.worksheet.sheet_id),
                sheet: layout.worksheet.name.clone(),
                row,
                rfi_no: label,
                source_statuses: Vec::new(),
                current_status: String::new(),
                proposed_status: None,
                proposed_rfi_no: None,
                disposition: FollowUpDisposition::Ready,
                reason_key: "warn.rfi.followUp.reason.readyFormulaRepair".to_string(),
                selectable: true,
                enabled: true,
                remark: None,
                formula_fixes: cells
                    .iter()
                    .map(|(column, formula, style)| FollowUpFormulaFix {
                        column: *column,
                        formula: formula.clone(),
                        style: style.clone(),
                    })
                    .collect(),
                target: FollowUpReviewTarget {
                    worksheet: layout.worksheet.clone(),
                    row,
                    rfi_column: cells[0].0,
                    status_column: cells[0].0,
                    rfi_cell: None,
                    status_cell: None,
                },
                source_evidence: Vec::new(),
                mapping_details: Some(
                    cells
                        .iter()
                        .map(|(column, formula, _)| {
                            format!("{}{row} = {formula}", column_letters(*column))
                        })
                        .collect::<Vec<_>>()
                        .join(" | "),
                ),
            });
        }
    }
    rows
}

fn column_letters(mut column: u32) -> String {
    let mut out = Vec::new();
    while column > 0 {
        column -= 1;
        out.push(b'A' + u8::try_from(column % 26).unwrap_or(0));
        column /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}
