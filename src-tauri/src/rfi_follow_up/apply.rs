use super::*;

pub(crate) fn decode_step_meta(step: &Step) -> Result<FollowUpStepMeta, BackendError> {
    if step.kind != StepKind::WorkbookUpdate {
        return Err(workbook_invalid());
    }
    let meta: FollowUpStepMeta =
        serde_json::from_value(Value::Object(step.meta.clone())).map_err(|_| workbook_invalid())?;
    if meta.updates.is_empty() || meta.updates.len() > MAX_PREVIEW_ROWS {
        return Err(workbook_invalid());
    }
    let mut row_ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for update in &meta.updates {
        let valid_layout = meta.all_layouts().any(|layout| {
            layout.worksheet == update.target.worksheet
                && layout.rfi_column == update.target.rfi_column
                && layout.status_column == update.target.status_column
        });
        if !valid_layout
            || !row_ids.insert(update.row_id.as_str())
            || !targets.insert((
                update.target.worksheet.part_path.as_str(),
                update.target.row,
                update.target.status_column,
            ))
            || canonical_rfi(&update.canonical_rfi).as_deref()
                != Some(update.canonical_rfi.as_str())
            || !matches!(
                update.proposed_status.as_str(),
                "" | "Pending" | "In Progress" | "Completed" | "Signed" | "Not Signed"
            )
            || (!update.proposed_rfi.is_empty() && parse_rfi_group(&update.proposed_rfi).is_none())
            || (!update.target.rfi_cell.value.trim().is_empty()
                && parse_rfi_group(&update.target.rfi_cell.value).is_none())
            || update.target.rfi_cell.formula.is_some()
            || update.target.status_cell.formula.is_some()
        {
            return Err(workbook_invalid());
        }
    }
    Ok(meta)
}

pub type FollowUpGuardedUpdates = BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>;

pub fn guarded_updates(step: &Step) -> Result<FollowUpGuardedUpdates, BackendError> {
    let meta = decode_step_meta(step)?;
    let mut updates_by_sheet = BTreeMap::<String, BTreeMap<(u32, u32), GuardedCellUpdate>>::new();
    for update in meta.updates {
        let sheet_updates = updates_by_sheet
            .entry(update.target.worksheet.name)
            .or_default();
        if update.proposed_rfi != update.target.rfi_cell.value {
            let rfi_guarded = GuardedCellUpdate {
                expected: CellData {
                    value: update.target.rfi_cell.value,
                    formula: update.target.rfi_cell.formula,
                    style: update.target.rfi_cell.style,
                },
                replacement: CellUpdate::Text(update.proposed_rfi),
                create_if_absent: false,
            };
            let key = (update.target.row, update.target.rfi_column);
            if let Some(existing) = sheet_updates.get(&key) {
                if existing != &rfi_guarded {
                    return Err(workbook_invalid());
                }
            } else {
                sheet_updates.insert(key, rfi_guarded);
            }
        }
        let status_guarded = GuardedCellUpdate {
            expected: CellData {
                value: update.target.status_cell.value,
                formula: update.target.status_cell.formula,
                style: update.target.status_cell.style,
            },
            replacement: CellUpdate::Text(update.proposed_status),
            create_if_absent: false,
        };
        if sheet_updates
            .insert(
                (update.target.row, update.target.status_column),
                status_guarded,
            )
            .is_some()
        {
            return Err(workbook_invalid());
        }
    }
    Ok(updates_by_sheet)
}

#[allow(dead_code)]
pub(crate) fn verify_patched_output<R1, R2>(
    step: &Step,
    source: &mut R1,
    private_output: &mut R2,
) -> Result<(), BackendError>
where
    R1: Read + Seek,
    R2: Read + Seek,
{
    let updates = guarded_updates(step)?;
    verify_guarded_workbook_delta_text_recalc(source, private_output, &updates).map_err(|error| {
        match error.kind() {
            OoxmlErrorKind::LimitExceeded => {
                limit_exceeded_for(format!("patch verification: {error}"))
            }
            OoxmlErrorKind::Invalid => {
                BackendError::validation("error.rfi.followUp.patchVerification")
            }
        }
    })
}

#[allow(dead_code)]
pub(crate) fn changed_cell_count(step: &Step) -> Result<u64, BackendError> {
    u64::try_from(
        guarded_updates(step)?
            .values()
            .map(BTreeMap::len)
            .sum::<usize>(),
    )
    .map_err(|_| limit_exceeded_for("changed cell count"))
}

pub(crate) fn step_workbook_paths(step: &Step) -> Result<(String, String, String), BackendError> {
    let meta = decode_step_meta(step)?;
    let target = step.src.clone().ok_or_else(workbook_invalid)?;
    let backup = step.dst.clone().ok_or_else(workbook_invalid)?;
    Ok((meta.rfi_log_workbook, target, backup))
}

pub(crate) fn revalidate_open_workbooks(
    step: &Step,
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<(), BackendError> {
    let meta = decode_step_meta(step)?;
    let source = fingerprint_open_file(rfi_log)?;
    let target = fingerprint_open_file(follow_up)?;
    if source != meta.rfi_log_fingerprint || target != meta.follow_up_fingerprint {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookChanged",
        ));
    }
    Ok(())
}

pub(crate) fn write_and_verify_private_output<R>(
    step: &Step,
    source: &mut File,
    destination: &mut R,
) -> Result<(), BackendError>
where
    R: Read + Write + Seek,
{
    let updates = guarded_updates(step)?;
    patch_workbook_sheets_guarded_text_recalc(source, destination, &updates)
        .map_err(map_apply_ooxml_error)?;
    verify_patched_output(step, source, destination)
}

pub(super) fn map_apply_ooxml_error(error: OoxmlError) -> BackendError {
    match error.kind() {
        OoxmlErrorKind::LimitExceeded => {
            BackendError::validation(format!("error.rfi.followUp.limitExceeded: {error}"))
        }
        OoxmlErrorKind::Invalid => {
            BackendError::validation(format!("error.rfi.followUp.workbookInvalid: {error}"))
        }
    }
}

pub(super) fn validate_cell_text(worksheet: &WorksheetData) -> Result<(), BackendError> {
    if worksheet.cells.values().any(|cell| {
        cell.value.chars().count() > MAX_CELL_TEXT_CHARS
            || cell
                .formula
                .as_ref()
                .is_some_and(|formula| formula.chars().count() > MAX_CELL_TEXT_CHARS)
    }) {
        return Err(limit_exceeded_for(format!(
            "a cell has more than {MAX_CELL_TEXT_CHARS} characters"
        )));
    }
    Ok(())
}

pub(super) fn normalize_status(value: &str) -> String {
    value.nfc().collect::<String>().trim().to_string()
}

pub(super) fn normalize_header(value: &str) -> String {
    normalize_status(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

pub(super) fn is_follow_up_rfi_header(token: &str) -> bool {
    token == "RFI NO."
        || token == "RFI NO"
        || token == "RFI #"
        || token == "RFI NUMBER"
        || token == "RFI NO:"
        || token == "RFI REF"
        || token == "RFI REFERENCE"
}

pub(super) fn is_follow_up_worley_status_header(token: &str) -> bool {
    token == "WORLEY STATUS"
        || token == "WORLEY SIGNATURE"
        || token == "WORLEY REMARKS"
        || token == "WORLEY REMARK"
        || token == "STATUS (WORLEY)"
        || token == "SIGNATURE (WORLEY)"
}

pub(super) fn is_follow_up_generic_status_header(token: &str) -> bool {
    token == "STATUS"
        || token == "RFI STATUS"
        || token == "CURRENT STATUS"
        || token == "SIGNATURE"
        || token == "SIGNATURE STATUS"
        || token == "DURUM"
        || token == "REMARKS"
        || token == "REMARK"
}

pub(super) fn manual(reason: &str) -> (FollowUpDisposition, String) {
    (
        FollowUpDisposition::ManualReview,
        format!("warn.rfi.followUp.reason.{reason}"),
    )
}

pub(super) fn workbook_invalid() -> BackendError {
    BackendError::validation("error.rfi.followUp.workbookInvalid")
}

pub(super) fn protected() -> BackendError {
    BackendError::validation("error.rfi.followUp.protected")
}

pub(super) fn signed_package() -> BackendError {
    BackendError::validation("error.rfi.followUp.signedPackage")
}

#[allow(dead_code)]
pub(crate) fn map_follow_up_ooxml_error(error: OoxmlError) -> BackendError {
    match error.kind() {
        OoxmlErrorKind::LimitExceeded => {
            BackendError::validation(format!("error.rfi.followUp.limitExceeded: {error}"))
        }
        OoxmlErrorKind::Invalid => {
            BackendError::validation(format!("error.rfi.followUp.workbookInvalid: {error}"))
        }
    }
}

/// Same stable key plus the exact limit that tripped, so the operator can tell
/// a huge used range from an oversized cell or preview.
pub(super) fn limit_exceeded_for(reason: impl std::fmt::Display) -> BackendError {
    BackendError::validation(format!("error.rfi.followUp.limitExceeded: {reason}"))
}
