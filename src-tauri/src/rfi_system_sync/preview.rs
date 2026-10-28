use super::*;

mod complete_status;
pub use complete_status::*;

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
    let (rfi_log_fingerprint, rfi_log_workbook) =
        read_sync_workbook(rfi_log_file, "error.rfi.systemSync.workbookInvalid")?;
    let rfi_log_rows = read_rfi_log_rows(&rfi_log_workbook)?;
    progress(ProgressEvent {
        phase: "system_workbook_read".to_string(),
        done: 1,
        total: 2,
        subject: follow_up_path.to_string_lossy().into_owned(),
    });

    let (follow_up_fingerprint, follow_up_workbook) =
        read_sync_workbook(follow_up_file, "error.rfi.systemSync.workbookInvalid")?;
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
            let (action, updates, flagged) = follow_up_status_updates(
                &row_id,
                matching_follow_up,
                scanned.signed_by_worley,
                unsafe_target,
            );
            follow_up_action = action;
            unsafe_target = flagged;
            ready |= !updates.is_empty();
            pending_follow_up_updates = updates;
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

        let (disposition, reason_key, selectable) = sync_disposition(
            unsafe_target,
            matching_log.is_empty(),
            ready,
            scanned.signed_by_worley,
        );
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
        steps.extend(sync_workbook_step(
            kind,
            path,
            fingerprint,
            updates,
            (note, label),
            &review_rows,
        )?);
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

fn read_sync_workbook(
    file: &mut File,
    invalid_key: &str,
) -> Result<(WorkbookFingerprint, crate::ooxml::WorkbookData), BackendError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let fingerprint = fingerprint_open_file(file)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let workbook = read_workbook_from_reader(file)
        .map_err(|e| BackendError::validation(format!("{invalid_key}: {e}")))?;
    Ok((fingerprint, workbook))
}

fn sync_workbook_step(
    kind: SyncWorkbookKind,
    path: &Path,
    fingerprint: WorkbookFingerprint,
    updates: BTreeMap<String, BTreeMap<String, GuardedCellUpdateDto>>,
    (note, label): (&str, &str),
    review_rows: &[SystemSyncRow],
) -> Result<Option<Step>, BackendError> {
    let count = updates.values().map(BTreeMap::len).sum::<usize>();
    if count == 0 {
        return Ok(None);
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
    Ok(Some(Step {
        kind: StepKind::WorkbookUpdate,
        src: Some(path.to_string_lossy().into_owned()),
        dst: Some(backup_path.to_string_lossy().into_owned()),
        note_key: Some(note.to_string()),
        meta,
    }))
}

fn follow_up_status_updates(
    row_id: &str,
    targets: &[FollowUpTarget],
    signed_by_worley: bool,
    unsafe_target: bool,
) -> (String, Vec<(String, String, GuardedCellUpdateDto)>, bool) {
    if !targets
        .iter()
        .all(|target| safe_cell(&target.status_cell, target.status_merged))
    {
        let action = "Manual review: Follow Up status cell missing, merged, or formula-bearing";
        return (action.to_string(), Vec::new(), true);
    }
    let mut unsafe_target = unsafe_target;
    let mut updates = Vec::new();
    for target in targets {
        let cell = target.status_cell.as_ref().expect("safe cell is present");
        let current = cell.value.nfc().collect::<String>().trim().to_string();
        if !matches!(current.as_str(), "" | "Signed" | "Not Signed") {
            unsafe_target = true;
            continue;
        }
        // A weak/unsigned scan can never revoke a prior signature.
        let desired = if signed_by_worley || current == "Signed" {
            "Signed"
        } else {
            "Not Signed"
        };
        if current != desired {
            updates.push((
                target.sheet.clone(),
                format!("{}:{}", target.row, target.status_col),
                dto(
                    row_id,
                    cell,
                    desired.to_string(),
                    target.row,
                    target.status_col,
                ),
            ));
        }
    }
    let changed = updates.len();
    let action = if unsafe_target {
        "Manual review: unexpected Follow Up status".to_string()
    } else if changed == 0 {
        format!(
            "Already {} in Follow Up",
            if signed_by_worley {
                "Signed"
            } else {
                "Not Signed or preserved Signed"
            }
        )
    } else {
        format!(
            "Set {changed} Follow Up control status cell(s) to {}",
            if signed_by_worley {
                "Signed"
            } else {
                "Not Signed"
            }
        )
    };
    (action, updates, unsafe_target)
}

fn sync_disposition(
    blocked: bool,
    unmatched: bool,
    ready: bool,
    signed_by_worley: bool,
) -> (&'static str, String, bool) {
    if blocked || unmatched {
        (
            "rfi.systemSync.disposition.manualReview",
            "Unsafe or incomplete workbook mapping; no unsafe cell was authorized".to_string(),
            false,
        )
    } else if ready {
        (
            "rfi.systemSync.disposition.ready",
            if signed_by_worley {
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
    }
}
