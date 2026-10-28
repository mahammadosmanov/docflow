use super::*;

#[doc(hidden)]
pub fn revise(plan: &Plan, rows: &[Value]) -> Result<Plan, BackendError> {
    let completion = plan.op_id == COMPLETE_STATUS_OPERATION_ID;
    let publish = plan.op_id == PUBLISH_STATUS_OPERATION_ID;
    let invalid_key = if completion {
        "error.rfi.completeStatus.approvedUpdateInvalid"
    } else if publish {
        "error.rfi.publishStatus.approvedUpdateInvalid"
    } else {
        "error.rfi.systemSync.approvedUpdateInvalid"
    };
    let no_rows_key = if completion {
        "warn.rfi.completeStatus.noRowsSelected"
    } else if publish {
        "warn.rfi.publishStatus.noRowsSelected"
    } else {
        "warn.rfi.systemSync.noRowsSelected"
    };
    let original_rows = plan
        .meta
        .get("editor")
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation(invalid_key))?;
    if rows.len() != original_rows.len() {
        return Err(BackendError::validation(invalid_key));
    }
    let originals = original_rows
        .iter()
        .map(|row| {
            row.get("_rowId")
                .and_then(Value::as_str)
                .map(|row_id| (row_id.to_string(), row.clone()))
                .ok_or_else(|| BackendError::validation(invalid_key))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut enabled = BTreeSet::new();
    let mut revised_rows = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for submitted in rows {
        let row_id = submitted
            .get("_rowId")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation(invalid_key))?;
        if !seen.insert(row_id.to_string()) {
            return Err(BackendError::validation(invalid_key));
        }
        let original = originals
            .get(row_id)
            .ok_or_else(|| BackendError::validation(invalid_key))?;
        let requested = submitted
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| BackendError::validation(invalid_key))?;
        let selectable = original
            .get("selectable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if requested && !selectable {
            return Err(BackendError::validation(invalid_key));
        }
        let mut expected = original.clone();
        expected["enabled"] = Value::Bool(requested);
        if submitted != &expected {
            return Err(BackendError::validation(invalid_key));
        }
        if requested {
            enabled.insert(row_id.to_string());
        }
        revised_rows.push(expected);
    }

    let mut revised = plan.clone();
    revised.meta["editor"]["rows"] = Value::Array(revised_rows);
    revised.steps.clear();
    for original_step in &plan.steps {
        let mut step = original_step.clone();
        let mut meta = decode_step_meta(&step)?;
        for updates in meta.updates.values_mut() {
            updates.retain(|_, update| enabled.contains(&update.row_id));
        }
        meta.updates.retain(|_, updates| !updates.is_empty());
        if meta.updates.is_empty() {
            continue;
        }
        let update_row_ids = meta
            .updates
            .values()
            .flat_map(BTreeMap::values)
            .map(|update| update.row_id.as_str())
            .collect::<BTreeSet<_>>();
        meta.canonical_rfi_codes = originals
            .values()
            .filter(|row| {
                row.get("_rowId")
                    .and_then(Value::as_str)
                    .is_some_and(|row_id| update_row_ids.contains(row_id))
            })
            .filter_map(|row| row.get("canonicalRfi").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        step.meta.insert(
            "syncStep".to_string(),
            serde_json::to_value(meta).map_err(|_| BackendError::validation(invalid_key))?,
        );
        revised.steps.push(step);
    }
    revised.warnings.retain(|warning| {
        warning.key != "warn.rfi.systemSync.noRowsSelected"
            && warning.key != "warn.rfi.completeStatus.noRowsSelected"
            && warning.key != "warn.rfi.publishStatus.noRowsSelected"
    });
    if revised.steps.is_empty() {
        revised.warnings.push(Warning {
            key: no_rows_key.to_string(),
            subject: if publish {
                "No RFI rows are selected for publishing".to_string()
            } else if completion {
                "No completed RFI rows are selected for synchronization".to_string()
            } else {
                "No signed RFI rows are selected for synchronization".to_string()
            },
        });
    }
    Ok(revised)
}

pub(super) fn validate_replacement(kind: SyncWorkbookKind, update: &GuardedCellUpdateDto) -> bool {
    match kind {
        SyncWorkbookKind::FollowUp => match update.replacement_value.as_str() {
            "Signed" => {
                let expected = update.expected_value.trim();
                expected.is_empty() || crate::ooxml::SIGNED_STATUS_VALUES.contains(&expected)
            }
            "Not Signed" => matches!(update.expected_value.trim(), "" | "Not Signed"),
            _ => false,
        },
        SyncWorkbookKind::RfiLogStatus => {
            update.replacement_value == "Completed"
                && matches!(
                    update.expected_value.nfc().collect::<String>().trim(),
                    "" | "Ongoing" | "Completed"
                )
        }
        SyncWorkbookKind::RfiLogPublish => {
            update.replacement_value == "Ongoing"
                && update
                    .expected_value
                    .nfc()
                    .collect::<String>()
                    .trim()
                    .is_empty()
        }
        SyncWorkbookKind::RfiLog => {
            update.replacement_value == WORLEY_SIGNED_MARKER
                || update
                    .replacement_value
                    .ends_with(&format!("\n{WORLEY_SIGNED_MARKER}"))
                    && !update.expected_value.is_empty()
                    && update.replacement_value
                        == format!("{}\n{WORLEY_SIGNED_MARKER}", update.expected_value)
        }
    }
}

#[doc(hidden)]
pub fn execute_sync_step(
    step: &Step,
    source_path: &Path,
    dest_path: &Path,
) -> Result<(), BackendError> {
    let meta = decode_step_meta(step)?;
    let mut updates = BTreeMap::<String, BTreeMap<(u32, u32), GuardedCellUpdate>>::new();
    for (sheet, sheet_updates) in &meta.updates {
        for update in sheet_updates.values() {
            if update.row == 0
                || update.col == 0
                || update.expected_formula.is_some()
                || !validate_replacement(meta.workbook_kind, update)
            {
                let key = if meta.workbook_kind == SyncWorkbookKind::RfiLogStatus {
                    "error.rfi.completeStatus.approvedUpdateInvalid"
                } else if meta.workbook_kind == SyncWorkbookKind::RfiLogPublish {
                    "error.rfi.publishStatus.approvedUpdateInvalid"
                } else {
                    "error.rfi.systemSync.approvedUpdateInvalid"
                };
                return Err(BackendError::validation(key));
            }
            let guarded = GuardedCellUpdate {
                expected: CellData {
                    value: update.expected_value.clone(),
                    formula: update.expected_formula.clone(),
                    style: update.expected_style.clone(),
                },
                replacement: CellUpdate::Text(update.replacement_value.clone()),
                create_if_absent: false,
            };
            if updates
                .entry(sheet.clone())
                .or_default()
                .insert((update.row, update.col), guarded)
                .is_some()
            {
                return Err(BackendError::validation(
                    "error.rfi.systemSync.approvedUpdateInvalid",
                ));
            }
        }
    }
    let mut source = File::open(source_path)
        .map_err(|e| BackendError::validation(format!("Failed to open staged workbook: {e}")))?;
    let mut destination = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dest_path)
        .map_err(|e| BackendError::validation(format!("Failed to create staged workbook: {e}")))?;
    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates)
        .map_err(|e| BackendError::validation(format!("Failed to patch workbook: {e}")))?;
    source
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    destination
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).map_err(|e| {
        BackendError::validation(format!("Workbook delta verification failed: {e}"))
    })?;
    Ok(())
}
