use super::*;

pub fn preview_complete_status_sync<P, C>(
    system_root: &Path,
    rfi_log_path: &Path,
    rfi_log_file: &mut File,
    mut progress: P,
    cancelled: C,
) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let discovery = discover_completion_pdfs(system_root, &mut progress, cancelled)?;
    progress(ProgressEvent {
        phase: "completion_workbook_read".to_string(),
        done: 0,
        total: 1,
        subject: rfi_log_path.to_string_lossy().into_owned(),
    });
    let (fingerprint, workbook) =
        read_sync_workbook(rfi_log_file, "error.rfi.completeStatus.workbookInvalid")?;
    let log_rows = read_rfi_log_status_rows(&workbook)?;
    progress(ProgressEvent {
        phase: "completion_workbook_read".to_string(),
        done: 1,
        total: 1,
        subject: rfi_log_path.to_string_lossy().into_owned(),
    });

    let mut updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut review_rows = Vec::new();
    let total = discovery.matches.len() as u64;
    for (index, (canonical_rfi, pdf_paths)) in discovery.matches.iter().enumerate() {
        progress(ProgressEvent {
            phase: "completion_mapping".to_string(),
            done: index as u64,
            total,
            subject: canonical_rfi.clone(),
        });
        let row_id = format!("complete-row-{}", index + 1);
        let matching = log_rows
            .iter()
            .filter(|row| row.canonical_rfi == *canonical_rfi)
            .collect::<Vec<_>>();
        let current_values = matching
            .iter()
            .filter_map(|row| row.status_cell.as_ref())
            .map(|cell| cell.value.nfc().collect::<String>().trim().to_string())
            .collect::<BTreeSet<_>>();
        let current_status = if current_values.is_empty() {
            "Missing cell".to_string()
        } else {
            current_values
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ")
        };
        let safe_cells = !matching.is_empty()
            && matching
                .iter()
                .all(|row| safe_cell(&row.status_cell, row.status_merged));
        let known_values = current_values
            .iter()
            .all(|value| matches!(value.as_str(), "" | "Ongoing" | "Completed"));
        let mut pending = Vec::new();
        if safe_cells && known_values {
            for row in &matching {
                let cell = row.status_cell.as_ref().expect("safe status cell exists");
                if cell.value.nfc().collect::<String>().trim() != "Completed" {
                    pending.push((
                        row.sheet.clone(),
                        format!("{}:{}", row.row, row.status_col),
                        dto(
                            &row_id,
                            cell,
                            "Completed".to_string(),
                            row.row,
                            row.status_col,
                        ),
                    ));
                }
            }
        }
        let (disposition, action, reason, selectable) = completion_disposition(
            matching.is_empty(),
            safe_cells,
            known_values,
            pending.is_empty(),
        );
        if selectable {
            for (sheet, key, update) in pending {
                updates.entry(sheet).or_default().insert(key, update);
            }
        }
        let (discipline, number) = parse_rfi_discipline_and_number(canonical_rfi)
            .expect("exact canonical completion code is parseable");
        let relative_paths = pdf_paths
            .iter()
            .filter_map(|path| path.strip_prefix(system_root).ok())
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        review_rows.push(json!({
            "_rowId": row_id,
            "discipline": discipline,
            "rfiNo": format!("{discipline}-{number:04}"),
            "canonicalRfi": canonical_rfi,
            "pdfPath": relative_paths.first().cloned().unwrap_or_else(|| pdf_paths[0].to_string_lossy().into_owned()),
            "pdfCount": pdf_paths.len(),
            "pdfPaths": relative_paths.join("\n"),
            "currentStatus": current_status,
            "proposedStatus": "Completed",
            "action": action,
            "disposition": disposition,
            "reasonKey": reason,
            "enabled": selectable,
            "selectable": selectable,
        }));
    }
    progress(ProgressEvent {
        phase: "completion_mapping".to_string(),
        done: total,
        total,
        subject: String::new(),
    });

    let steps = completion_status_step(rfi_log_path, fingerprint, updates, &review_rows)?;

    let mut warnings = discovery
        .invalid_names
        .iter()
        .take(MAX_COMPLETION_WARNINGS)
        .map(|path| Warning {
            key: "warn.rfi.completeStatus.invalidPdfName".to_string(),
            subject: path.to_string_lossy().into_owned(),
        })
        .collect::<Vec<_>>();
    if discovery.invalid_names.len() > MAX_COMPLETION_WARNINGS {
        warnings.push(Warning {
            key: "warn.rfi.completeStatus.invalidPdfNameOverflow".to_string(),
            subject: format!(
                "{} additional invalid RFI PDF name(s)",
                discovery.invalid_names.len() - MAX_COMPLETION_WARNINGS
            ),
        });
    }
    if steps.is_empty() {
        warnings.push(Warning {
            key: "warn.rfi.completeStatus.noChangesNeeded".to_string(),
            subject: "No safe RFI Log status changes are ready".to_string(),
        });
    }
    let columns = vec![
        json!({"key":"enabled","labelKey":"rfi.completeStatus.column.enabled","kind":"bool","bulk":true,"disabledWhen":{"field":"selectable","equals":false}}),
        json!({"key":"discipline","labelKey":"rfi.completeStatus.column.discipline","kind":"readonly"}),
        json!({"key":"rfiNo","labelKey":"rfi.completeStatus.column.rfiNo","kind":"readonly"}),
        json!({"key":"pdfPath","labelKey":"rfi.completeStatus.column.pdfPath","kind":"readonly"}),
        json!({"key":"pdfCount","labelKey":"rfi.completeStatus.column.pdfCount","kind":"readonly"}),
        json!({"key":"currentStatus","labelKey":"rfi.completeStatus.column.currentStatus","kind":"readonly"}),
        json!({"key":"proposedStatus","labelKey":"rfi.completeStatus.column.proposedStatus","kind":"readonly"}),
        json!({"key":"disposition","labelKey":"rfi.completeStatus.column.disposition","kind":"readonly","display":"localizedKey"}),
        json!({"key":"action","labelKey":"rfi.completeStatus.column.action","kind":"readonly","detail":true}),
        json!({"key":"pdfPaths","labelKey":"rfi.completeStatus.column.pdfPaths","kind":"readonly","detail":true}),
        json!({"key":"reasonKey","labelKey":"rfi.completeStatus.column.reason","kind":"readonly","detail":true}),
    ];
    let editor = json!({
        "kind":"recordTable","layout":"compact","titleKey":"rfi.completeStatus.editor.title",
        "rowCountKey":"rfi.completeStatus.editor.rowCount","requiredFields":["enabled"],
        "groupField":"discipline","selectionGroupFields":[],"columns":columns,"rows":review_rows,
        "duplicateEnabled":false,"selectionEnabled":true,"bulkEnabled":true,"reorderable":false,
        "detailsEnabled":true,"detailsDefaultVisible":false,"copyEnabled":false
    });
    let mut meta = JsonObject::new();
    meta.insert("editor".to_string(), editor);
    meta.insert(
        "workerCount".to_string(),
        Value::from(discovery.worker_count as u64),
    );
    meta.insert(
        "directoriesScanned".to_string(),
        Value::from(discovery.directories_scanned as u64),
    );
    meta.insert(
        "entriesScanned".to_string(),
        Value::from(discovery.entries_scanned as u64),
    );
    meta.insert(
        "exactPdfCount".to_string(),
        Value::from(discovery.matches.values().map(Vec::len).sum::<usize>() as u64),
    );
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(Plan {
        plan_id: String::new(),
        op_id: COMPLETE_STATUS_OPERATION_ID.to_string(),
        steps,
        warnings,
        meta,
    })
}

fn completion_status_step(
    rfi_log_path: &Path,
    fingerprint: WorkbookFingerprint,
    updates: BTreeMap<String, BTreeMap<String, GuardedCellUpdateDto>>,
    review_rows: &[Value],
) -> Result<Vec<Step>, BackendError> {
    let count = updates.values().map(BTreeMap::len).sum::<usize>();
    if count == 0 {
        return Ok(Vec::new());
    }
    let extension = rfi_log_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("xlsm");
    let backup_path = rfi_log_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(
            ".docflow-completion-sync-backup-{}.{}",
            Uuid::new_v4().simple(),
            extension
        ));
    let update_row_ids = updates
        .values()
        .flat_map(BTreeMap::values)
        .map(|update| update.row_id.as_str())
        .collect::<BTreeSet<_>>();
    let canonical_rfi_codes = review_rows
        .iter()
        .filter(|row| {
            row.get("_rowId")
                .and_then(Value::as_str)
                .is_some_and(|row_id| update_row_ids.contains(row_id))
        })
        .filter_map(|row| row.get("canonicalRfi").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let step_meta = SystemSyncStepMeta {
        target_path: rfi_log_path.to_string_lossy().into_owned(),
        backup_path: backup_path.to_string_lossy().into_owned(),
        source_fingerprint: fingerprint,
        workbook_kind: SyncWorkbookKind::RfiLogStatus,
        updates,
        canonical_rfi_codes,
        summary: format!("Update {count} RFI Log Inspection Status cell(s)"),
    };
    let mut meta = JsonObject::new();
    meta.insert(
        "syncStep".to_string(),
        serde_json::to_value(step_meta)
            .map_err(|error| BackendError::protocol(error.to_string()))?,
    );
    Ok(vec![Step {
        kind: StepKind::WorkbookUpdate,
        src: Some(rfi_log_path.to_string_lossy().into_owned()),
        dst: Some(backup_path.to_string_lossy().into_owned()),
        note_key: Some("note.rfi.completeStatus.rfiLog".to_string()),
        meta,
    }])
}

fn completion_disposition(
    unmatched: bool,
    safe_cells: bool,
    known_values: bool,
    nothing_pending: bool,
) -> (&'static str, &'static str, &'static str, bool) {
    if unmatched {
        (
            "rfi.completeStatus.disposition.manualReview",
            "RFI was not found in the RFI Log",
            "Exact completed PDF exists, but no exact RFI Log row was found",
            false,
        )
    } else if !safe_cells {
        (
            "rfi.completeStatus.disposition.manualReview",
            "Inspection Status cell is missing, merged, or formula-bearing",
            "No unsafe workbook cell was authorized",
            false,
        )
    } else if !known_values {
        (
            "rfi.completeStatus.disposition.manualReview",
            "Current status conflicts with automatic completion",
            "Only blank, Ongoing, or Completed status cells may be synchronized automatically",
            false,
        )
    } else if nothing_pending {
        (
            "rfi.completeStatus.disposition.alreadyCompleted",
            "Already Completed",
            "Every matching RFI Log row is already Completed",
            false,
        )
    } else {
        (
            "rfi.completeStatus.disposition.ready",
            "Set matching Inspection Status cell(s) to Completed",
            "Exact completed RFI PDF and safe RFI Log mapping found",
            true,
        )
    }
}
