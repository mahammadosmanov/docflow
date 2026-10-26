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
    rfi_log_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| BackendError::protocol(error.to_string()))?;
    let fingerprint = fingerprint_open_file(rfi_log_file)?;
    rfi_log_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| BackendError::protocol(error.to_string()))?;
    let workbook = read_workbook_from_reader(rfi_log_file).map_err(|error| {
        BackendError::validation(format!("error.rfi.completeStatus.workbookInvalid: {error}"))
    })?;
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
        let (disposition, action, reason, selectable) = if matching.is_empty() {
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
        } else if pending.is_empty() {
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
        };
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

    let mut steps = Vec::new();
    let count = updates.values().map(BTreeMap::len).sum::<usize>();
    if count > 0 {
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
        steps.push(Step {
            kind: StepKind::WorkbookUpdate,
            src: Some(rfi_log_path.to_string_lossy().into_owned()),
            dst: Some(backup_path.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.completeStatus.rfiLog".to_string()),
            meta,
        });
    }

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

pub fn preview_system_sync(
    system_root: &Path,
    rfi_log_path: &Path,
    follow_up_path: &Path,
    rfi_log_file: &mut File,
    follow_up_file: &mut File,
) -> Result<Plan, BackendError> {
    preview_system_sync_indexed(
        system_root,
        rfi_log_path,
        follow_up_path,
        rfi_log_file,
        follow_up_file,
        SystemSyncScan {
            index: None,
            owner_id: "local",
            force_rescan: false,
        },
        |_| {},
    )
}
/// Scan-cache options for system-sync preview; `index: None` disables reuse.
pub struct SystemSyncScan<'a> {
    pub index: Option<&'a ScanIndex>,
    pub owner_id: &'a str,
    pub force_rescan: bool,
}

pub fn preview_system_sync_indexed<P>(
    system_root: &Path,
    rfi_log_path: &Path,
    follow_up_path: &Path,
    rfi_log_file: &mut File,
    follow_up_file: &mut File,
    scan: SystemSyncScan<'_>,
    mut progress: P,
) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
{
    let SystemSyncScan {
        index: scan_index,
        owner_id,
        force_rescan,
    } = scan;
    let (scanned_rfis, scan_stats) = scan_system_root_indexed(
        system_root,
        scan_index,
        owner_id,
        force_rescan,
        &mut progress,
    )?;
    progress(ProgressEvent {
        phase: "system_workbook_read".to_string(),
        done: 0,
        total: 2,
        subject: rfi_log_path.to_string_lossy().into_owned(),
    });
    rfi_log_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let rfi_log_fingerprint = fingerprint_open_file(rfi_log_file)?;
    rfi_log_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let rfi_log_workbook = read_workbook_from_reader(rfi_log_file).map_err(|e| {
        BackendError::validation(format!("error.rfi.systemSync.workbookInvalid: {e}"))
    })?;
    let rfi_log_rows = read_rfi_log_rows(&rfi_log_workbook)?;
    progress(ProgressEvent {
        phase: "system_workbook_read".to_string(),
        done: 1,
        total: 2,
        subject: follow_up_path.to_string_lossy().into_owned(),
    });

    follow_up_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let follow_up_fingerprint = fingerprint_open_file(follow_up_file)?;
    follow_up_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let follow_up_workbook = read_workbook_from_reader(follow_up_file).map_err(|e| {
        BackendError::validation(format!("error.rfi.systemSync.workbookInvalid: {e}"))
    })?;
    let follow_up_targets = read_follow_up_targets(&follow_up_workbook)?;
    progress(ProgressEvent {
        phase: "system_workbook_read".to_string(),
        done: 2,
        total: 2,
        subject: String::new(),
    });

    let mut log_updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut follow_up_updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut review_rows = Vec::new();

    for (index, scanned) in scanned_rfis.iter().enumerate() {
        progress(ProgressEvent {
            phase: "system_mapping".to_string(),
            done: index as u64,
            total: scanned_rfis.len() as u64,
            subject: scanned.canonical_rfi.clone(),
        });
        let row_id = format!("sync-row-{}", index + 1);
        let matching_log = rfi_log_rows
            .iter()
            .filter(|row| row.canonical_rfi == scanned.canonical_rfi)
            .collect::<Vec<_>>();
        let matching_follow_up = follow_up_targets
            .get(&scanned.canonical_rfi)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut log_action = "Not found in RFI Log".to_string();
        let mut follow_up_action = if matches!(scanned.discipline.as_str(), "CIV" | "MEC") {
            "Not found in Follow Up control table".to_string()
        } else {
            "Not applicable for ELE/PRC".to_string()
        };
        let mut ready = false;
        let mut unsafe_target = false;
        let mut pending_log_updates = Vec::new();
        let mut pending_follow_up_updates = Vec::new();

        if scanned.signed_by_worley && !matching_log.is_empty() {
            if matching_log
                .iter()
                .all(|row| safe_cell(&row.remarks_cell, row.remarks_merged))
            {
                let mut changed = 0usize;
                for row in &matching_log {
                    let cell = row.remarks_cell.as_ref().expect("safe cell is present");
                    let replacement = append_signed_marker(&cell.value);
                    if replacement != cell.value {
                        pending_log_updates.push((
                            row.sheet.clone(),
                            format!("{}:{}", row.row, row.remarks_col),
                            dto(&row_id, cell, replacement, row.row, row.remarks_col),
                        ));
                        changed += 1;
                    }
                }
                log_action = if changed == 0 {
                    "Already signed in RFI Log".to_string()
                } else {
                    format!("Add exact Worley remark to {changed} RFI Log row(s)")
                };
                ready |= changed > 0;
            } else {
                log_action =
                    "Manual review: REMARKS cell missing, merged, or formula-bearing".to_string();
                unsafe_target = true;
            }
        }

        if !matching_follow_up.is_empty() {
            if matching_follow_up
                .iter()
                .all(|target| safe_cell(&target.status_cell, target.status_merged))
            {
                let mut changed = 0usize;
                for target in matching_follow_up {
                    let cell = target.status_cell.as_ref().expect("safe cell is present");
                    let current = cell.value.nfc().collect::<String>().trim().to_string();
                    if !matches!(current.as_str(), "" | "Signed" | "Not Signed") {
                        unsafe_target = true;
                        continue;
                    }
                    let desired = if scanned.signed_by_worley {
                        "Signed"
                    } else if current == "Signed" {
                        // A weak/unsigned scan can never revoke a prior signature.
                        "Signed"
                    } else {
                        "Not Signed"
                    };
                    if current != desired {
                        pending_follow_up_updates.push((
                            target.sheet.clone(),
                            format!("{}:{}", target.row, target.status_col),
                            dto(
                                &row_id,
                                cell,
                                desired.to_string(),
                                target.row,
                                target.status_col,
                            ),
                        ));
                        changed += 1;
                    }
                }
                follow_up_action = if unsafe_target {
                    "Manual review: unexpected Follow Up status".to_string()
                } else if changed == 0 {
                    format!(
                        "Already {} in Follow Up",
                        if scanned.signed_by_worley {
                            "Signed"
                        } else {
                            "Not Signed or preserved Signed"
                        }
                    )
                } else {
                    format!(
                        "Set {changed} Follow Up control status cell(s) to {}",
                        if scanned.signed_by_worley {
                            "Signed"
                        } else {
                            "Not Signed"
                        }
                    )
                };
                ready |= changed > 0;
            } else {
                follow_up_action =
                    "Manual review: Follow Up status cell missing, merged, or formula-bearing"
                        .to_string();
                unsafe_target = true;
            }
        } else if matches!(scanned.discipline.as_str(), "CIV" | "MEC") {
            follow_up_action = "Manual review: no existing Follow Up control row".to_string();
            unsafe_target = true;
        }

        if unsafe_target || matching_log.is_empty() {
            ready = false;
        } else {
            for (sheet, key, update) in pending_log_updates {
                log_updates.entry(sheet).or_default().insert(key, update);
            }
            for (sheet, key, update) in pending_follow_up_updates {
                follow_up_updates
                    .entry(sheet)
                    .or_default()
                    .insert(key, update);
            }
        }

        let (disposition, reason_key, selectable) = if unsafe_target || matching_log.is_empty() {
            (
                "rfi.systemSync.disposition.manualReview",
                "Unsafe or incomplete workbook mapping; no unsafe cell was authorized".to_string(),
                false,
            )
        } else if ready {
            (
                "rfi.systemSync.disposition.ready",
                if scanned.signed_by_worley {
                    "Worley signature detected; exact guarded updates are ready".to_string()
                } else {
                    "No Worley signature detected; exact Not Signed update is ready".to_string()
                },
                true,
            )
        } else {
            (
                "rfi.systemSync.disposition.alreadySynced",
                "Already synchronized".to_string(),
                false,
            )
        };
        review_rows.push(SystemSyncRow {
            row_id,
            discipline: scanned.discipline.clone(),
            rfi_no: scanned.short_rfi.clone(),
            canonical_rfi: scanned.canonical_rfi.clone(),
            worley_status: if scanned.signed_by_worley {
                WORLEY_SIGNED_MARKER.to_string()
            } else {
                "Unsigned".to_string()
            },
            rfi_log_action: log_action,
            follow_up_action,
            disposition: disposition.to_string(),
            reason_key,
            enabled: selectable,
            selectable,
        });
    }

    let mut steps = Vec::new();
    for (kind, path, fingerprint, updates, note, label) in [
        (
            SyncWorkbookKind::RfiLog,
            rfi_log_path,
            rfi_log_fingerprint,
            log_updates,
            "note.rfi.systemSync.rfiLog",
            "RFI Log REMARKS",
        ),
        (
            SyncWorkbookKind::FollowUp,
            follow_up_path,
            follow_up_fingerprint,
            follow_up_updates,
            "note.rfi.systemSync.followUp",
            "Follow Up status",
        ),
    ] {
        let count = updates.values().map(BTreeMap::len).sum::<usize>();
        if count == 0 {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("xlsm");
        let backup_path = path.parent().unwrap_or(Path::new(".")).join(format!(
            ".docflow-system-sync-backup-{}.{}",
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
            .filter(|row| update_row_ids.contains(row.row_id.as_str()))
            .map(|row| row.canonical_rfi.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let step_meta = SystemSyncStepMeta {
            target_path: path.to_string_lossy().into_owned(),
            backup_path: backup_path.to_string_lossy().into_owned(),
            source_fingerprint: fingerprint,
            workbook_kind: kind,
            updates,
            canonical_rfi_codes,
            summary: format!("Update {count} {label} cell(s)"),
        };
        let mut meta = JsonObject::new();
        meta.insert(
            "syncStep".to_string(),
            serde_json::to_value(step_meta).map_err(|e| BackendError::protocol(e.to_string()))?,
        );
        steps.push(Step {
            kind: StepKind::WorkbookUpdate,
            src: Some(path.to_string_lossy().into_owned()),
            dst: Some(backup_path.to_string_lossy().into_owned()),
            note_key: Some(note.to_string()),
            meta,
        });
    }

    let warnings = if steps.is_empty() {
        vec![Warning {
            key: "warn.rfi.systemSync.noChangesNeeded".to_string(),
            subject:
                "All detected RFIs are already synchronized, unsigned, or require manual review"
                    .to_string(),
        }]
    } else {
        Vec::new()
    };
    let columns = vec![
        json!({"key":"enabled","labelKey":"rfi.systemSync.column.enabled","kind":"bool","bulk":false,"disabledWhen":{"field":"selectable","equals":false}}),
        json!({"key":"discipline","labelKey":"rfi.systemSync.column.discipline","kind":"readonly"}),
        json!({"key":"rfiNo","labelKey":"rfi.systemSync.column.rfiNo","kind":"readonly"}),
        json!({"key":"worleyStatus","labelKey":"rfi.systemSync.column.worleyStatus","kind":"readonly"}),
        json!({"key":"rfiLogAction","labelKey":"rfi.systemSync.column.rfiLogAction","kind":"readonly"}),
        json!({"key":"followUpAction","labelKey":"rfi.systemSync.column.followUpAction","kind":"readonly"}),
        json!({"key":"disposition","labelKey":"rfi.systemSync.column.disposition","kind":"readonly","display":"localizedKey"}),
        json!({"key":"reasonKey","labelKey":"rfi.systemSync.column.reason","kind":"readonly","detail":true}),
    ];
    let editor = json!({
        "kind":"recordTable","layout":"compact","titleKey":"rfi.systemSync.editor.title",
        "rowCountKey":"rfi.systemSync.editor.rowCount","requiredFields":["enabled"],
        "groupField":"discipline","selectionGroupFields":[],"columns":columns,"rows":review_rows,
        "duplicateEnabled":false,"selectionEnabled":false,"bulkEnabled":false,"reorderable":false,
        "detailsEnabled":true,"detailsDefaultVisible":false,"copyEnabled":false
    });
    let mut meta = JsonObject::new();
    meta.insert("editor".to_string(), editor);
    meta.insert(
        "scanIndex".to_string(),
        serde_json::to_value(&scan_stats)
            .map_err(|error| BackendError::protocol(error.to_string()))?,
    );
    meta.insert(
        "parallelAnalysis".to_string(),
        Value::Bool(scan_stats.parallel_analysis),
    );
    meta.insert(
        "workerCount".to_string(),
        Value::from(scan_stats.analysis_workers),
    );
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps,
        warnings,
        meta,
    })
}

pub(crate) fn decode_step_meta(step: &Step) -> Result<SystemSyncStepMeta, BackendError> {
    let value = step
        .meta
        .get("syncStep")
        .ok_or_else(|| BackendError::validation("syncStep metadata missing in plan step"))?;
    serde_json::from_value(value.clone())
        .map_err(|error| BackendError::validation(format!("Invalid syncStep metadata: {error}")))
}
