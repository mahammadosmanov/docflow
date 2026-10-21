use super::*;

pub(super) fn read_rfi_log_rows(
    workbook: &WorkbookData,
) -> Result<Vec<RfiLogRowInfo>, BackendError> {
    let mut rows = Vec::new();
    for discipline in DISCIPLINES {
        let sheet = workbook.sheet(discipline).ok_or_else(|| {
            BackendError::validation("error.rfi.systemSync.workbookLayoutInvalid")
        })?;
        let layout = discover_header(sheet, &["REMARKS"])?;
        for row in (layout.row + 1)..=sheet.max_row {
            let Some(rfi_cell) = sheet.cell(row, layout.rfi_col) else {
                continue;
            };
            let Some(canonical_rfi) = exact_canonical_rfi(&rfi_cell.value) else {
                continue;
            };
            if !canonical_rfi.contains(&format!("-RFI-{discipline}-")) {
                continue;
            }
            rows.push(RfiLogRowInfo {
                sheet: discipline.to_string(),
                row,
                canonical_rfi,
                remarks_col: layout.value_col,
                remarks_cell: sheet.cell(row, layout.value_col).cloned(),
                remarks_merged: cell_is_merged(sheet, row, layout.value_col),
            });
        }
    }
    Ok(rows)
}

pub(super) fn read_rfi_log_status_rows(
    workbook: &WorkbookData,
) -> Result<Vec<RfiLogStatusRowInfo>, BackendError> {
    let mut rows = Vec::new();
    for discipline in DISCIPLINES {
        let sheet = workbook.sheet(discipline).ok_or_else(|| {
            BackendError::validation("error.rfi.completeStatus.workbookLayoutInvalid")
        })?;
        let layout = discover_header(sheet, &["INSPECTION STATUS"]).map_err(|_| {
            BackendError::validation("error.rfi.completeStatus.workbookLayoutInvalid")
        })?;
        for row in (layout.row + 1)..=sheet.max_row {
            let Some(rfi_cell) = sheet.cell(row, layout.rfi_col) else {
                continue;
            };
            let Some(canonical_rfi) = exact_canonical_rfi(&rfi_cell.value) else {
                continue;
            };
            if !canonical_rfi.contains(&format!("-RFI-{discipline}-")) {
                continue;
            }
            rows.push(RfiLogStatusRowInfo {
                sheet: discipline.to_string(),
                row,
                canonical_rfi,
                status_col: layout.value_col,
                status_cell: sheet.cell(row, layout.value_col).cloned(),
                status_merged: cell_is_merged(sheet, row, layout.value_col),
            });
        }
    }
    Ok(rows)
}

pub(super) fn read_follow_up_targets(
    workbook: &WorkbookData,
) -> Result<BTreeMap<String, Vec<FollowUpTarget>>, BackendError> {
    let mut targets = BTreeMap::<String, Vec<FollowUpTarget>>::new();
    for discipline in ["CIV", "MEC"] {
        let candidates = workbook
            .sheets
            .iter()
            .filter(|(name, _)| {
                let name = clean_header(name);
                name.contains("SIGNED BY WORLEY") && name.ends_with(discipline)
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(BackendError::validation(
                "error.rfi.systemSync.followUpLayoutInvalid",
            ));
        }
        let (sheet_name, sheet) = candidates[0];
        let value_headers: &[&str] = if discipline == "MEC" {
            &["WORLEY STATUS"]
        } else {
            &["STATUS"]
        };
        let layout = discover_header(sheet, value_headers)?;
        for row in (layout.row + 1)..=sheet.max_row {
            let Some(rfi_cell) = sheet.cell(row, layout.rfi_col) else {
                continue;
            };
            let short = clean_header(&rfi_cell.value);
            let Some((parsed_discipline, number)) = parse_rfi_discipline_and_number(&short) else {
                continue;
            };
            if parsed_discipline != discipline || short != format!("{discipline}-{number:04}") {
                continue;
            }
            let canonical = format!("{}{short}", canonical_rfi_prefix());
            targets.entry(canonical).or_default().push(FollowUpTarget {
                sheet: sheet_name.to_string(),
                row,
                status_col: layout.value_col,
                status_cell: sheet.cell(row, layout.value_col).cloned(),
                status_merged: cell_is_merged(sheet, row, layout.value_col),
            });
        }
    }
    Ok(targets)
}

pub(super) fn safe_cell(cell: &Option<CellData>, merged: bool) -> bool {
    cell.as_ref().is_some_and(|cell| cell.formula.is_none()) && !merged
}

pub(super) fn dto(
    row_id: &str,
    cell: &CellData,
    replacement: String,
    row: u32,
    col: u32,
) -> GuardedCellUpdateDto {
    GuardedCellUpdateDto {
        row_id: row_id.to_string(),
        expected_value: cell.value.clone(),
        expected_formula: cell.formula.clone(),
        expected_style: cell.style.clone(),
        replacement_value: replacement,
        row,
        col,
    }
}

pub(super) fn append_signed_marker(value: &str) -> String {
    if value
        .lines()
        .any(|line| line.trim() == WORLEY_SIGNED_MARKER)
    {
        value.to_string()
    } else if value.is_empty() {
        WORLEY_SIGNED_MARKER.to_string()
    } else {
        format!("{value}\n{WORLEY_SIGNED_MARKER}")
    }
}
