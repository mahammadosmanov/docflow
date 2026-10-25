//! Review rows for generated Follow Up remarks (CIV, PCU, Road).
//!
//! The remark is derived from the final state of every lifecycle RFI/status pair in a
//! physical row, left to right. DocFlow only replaces or clears text it wrote itself
//! (see [`is_app_remark`]); operator notes are never touched.

use super::*;

const REMARK_SHEETS: [&str; 3] = ["CIV", "PCU", "ROAD"];

fn remark_column(sheet: &WorksheetData) -> Option<u32> {
    find_header_column(sheet, &["REMARKS", "REMARK"])
}

fn state_of(records: &BTreeMap<String, SourceRecord>, code: &str) -> Option<RemarkState> {
    match records.get(code)?.state? {
        LogState::Completed => Some(RemarkState::Completed),
        LogState::InProgress => Some(RemarkState::InProgress),
        LogState::Pending => Some(RemarkState::Pending),
        LogState::Remove => None,
    }
}

pub(super) fn remark_review_rows(
    slots: &BTreeMap<(String, u32, u32, u32), SlotWork>,
    records: &BTreeMap<String, SourceRecord>,
    target_workbook: &WorkbookData,
    merge_budget: &mut MergeQueryBudget,
) -> Result<Vec<FollowUpRow>, BackendError> {
    let mut physical = BTreeMap::<(String, u32), Vec<&SlotWork>>::new();
    for ((sheet, row, _, _), slot) in slots {
        if is_signed_layout(&slot.layout)
            || !REMARK_SHEETS.contains(&sheet.trim().to_uppercase().as_str())
        {
            continue;
        }
        physical
            .entry((sheet.clone(), *row))
            .or_default()
            .push(slot);
    }

    let mut rows = Vec::new();
    for ((sheet_name, row), mut row_slots) in physical {
        let Some(sheet) = target_workbook.sheet(&sheet_name) else {
            continue;
        };
        let Some(column) = remark_column(sheet) else {
            continue;
        };
        row_slots.sort_by_key(|slot| slot.layout.rfi_column);
        // An RFI whose Log status is unknown, or a cell that could not be proven safe,
        // makes the whole row ineligible: no guess is written.
        if row_slots.iter().any(|slot| slot.unsafe_reason.is_some()) {
            continue;
        }
        let mut cells = Vec::<Vec<RemarkState>>::new();
        let mut entries = Vec::<(String, RemarkState)>::new();
        let mut row_is_provable = true;
        for slot in &row_slots {
            let mut states = Vec::new();
            for code in slot.codes.iter().filter(|code| !is_placeholder_rfi(code)) {
                let Some(state) = state_of(records, code) else {
                    row_is_provable = false;
                    continue;
                };
                states.push(state);
                let short = code
                    .strip_prefix(FULL_RFI_PREFIX)
                    .unwrap_or(code)
                    .to_string();
                if !entries.iter().any(|(existing, _)| *existing == short) {
                    entries.push((short, state));
                }
            }
            cells.push(states);
        }
        if !row_is_provable {
            continue;
        }

        let desired = if needs_remark(&cells) {
            build_remark(&entries)
        } else {
            Vec::new()
        };
        let current = sheet.cell(row, column);
        let current_text = current
            .map(|cell| cell.value.trim().to_string())
            .unwrap_or_default();
        let desired_text = runs_text(&desired);
        if current_text == desired_text {
            continue;
        }
        let managed = current_text.is_empty() || is_app_remark(&current_text);
        if desired.is_empty() && !managed {
            continue;
        }
        let unsafe_cell = current.is_some_and(|cell| cell.formula.is_some())
            || merge_budget.contains(sheet, row, column)?;
        let layout = row_slots[0].layout.clone();
        let (disposition, reason_key, selectable) = if unsafe_cell {
            (
                FollowUpDisposition::ManualReview,
                "warn.rfi.followUp.reason.remarkUnsafe",
                false,
            )
        } else if !managed {
            (
                FollowUpDisposition::ManualReview,
                "warn.rfi.followUp.reason.remarkManual",
                false,
            )
        } else if desired.is_empty() {
            (
                FollowUpDisposition::Ready,
                "warn.rfi.followUp.reason.readyRemarkClear",
                true,
            )
        } else {
            (
                FollowUpDisposition::Ready,
                "warn.rfi.followUp.reason.readyRemark",
                true,
            )
        };
        rows.push(FollowUpRow {
            row_id: format!("rfi-follow-up-remark:{}:{row}", layout.worksheet.sheet_id),
            sheet: sheet_name.clone(),
            row,
            rfi_no: entries
                .first()
                .map_or_else(|| "-".to_string(), |(code, _)| code.clone()),
            source_statuses: Vec::new(),
            current_status: current_text.clone(),
            proposed_status: None,
            proposed_rfi_no: None,
            disposition,
            reason_key: reason_key.to_string(),
            selectable,
            enabled: selectable,
            remark: selectable.then(|| FollowUpRemarkChange {
                column,
                expected: current.map(Into::into),
                runs: desired.clone(),
            }),
            target: FollowUpReviewTarget {
                worksheet: layout.worksheet,
                row,
                rfi_column: column,
                status_column: column,
                rfi_cell: None,
                status_cell: None,
            },
            source_evidence: Vec::new(),
            mapping_details: Some(if desired_text.is_empty() {
                String::new()
            } else {
                desired_text
            }),
        });
    }
    Ok(rows)
}
