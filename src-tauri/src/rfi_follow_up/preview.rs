use super::*;

pub fn preview_from_open_files(
    rfi_log_path: &Path,
    follow_up_path: &Path,
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<Plan, BackendError> {
    let rfi_log_display = rfi_log_path.to_str().ok_or_else(|| {
        BackendError::validation(
            "error.rfi.followUp.workbookInvalid: RFI Log path contains invalid UTF-8",
        )
    })?;
    let follow_up_display = follow_up_path.to_str().ok_or_else(|| {
        BackendError::validation(
            "error.rfi.followUp.workbookInvalid: Follow Up path contains invalid UTF-8",
        )
    })?;
    let source_fingerprint = fingerprint_open_file(rfi_log)?;
    let target_fingerprint = fingerprint_open_file(follow_up)?;
    if source_fingerprint.identity == target_fingerprint.identity {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookInvalid: RFI Log and Follow Up workbooks must be different files (the filesystem may report the same file identity for both)",
        ));
    }

    rfi_log.seek(SeekFrom::Start(0)).map_err(|e| {
        BackendError::validation(format!(
            "error.rfi.followUp.workbookInvalid: RFI Log seek failed: {e}"
        ))
    })?;
    let (source_discovery, source_workbook) =
        read_workbook_and_discovery(rfi_log).map_err(map_follow_up_ooxml_error)?;

    follow_up.seek(SeekFrom::Start(0)).map_err(|e| {
        BackendError::validation(format!(
            "error.rfi.followUp.workbookInvalid: Follow Up seek failed: {e}"
        ))
    })?;
    let target_has_signature =
        package_has_signature(follow_up).map_err(map_follow_up_ooxml_error)?;
    follow_up.seek(SeekFrom::Start(0)).map_err(|e| {
        BackendError::validation(format!(
            "error.rfi.followUp.workbookInvalid: Follow Up seek failed: {e}"
        ))
    })?;
    let (target_discovery, target_workbook) =
        read_workbook_and_discovery(follow_up).map_err(map_follow_up_ooxml_error)?;
    let classification = classify_workbooks(
        &source_discovery,
        &source_workbook,
        &target_discovery,
        &target_workbook,
        target_has_signature,
    )?;
    let extension = follow_up_path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| value.eq_ignore_ascii_case("xlsx") || value.eq_ignore_ascii_case("xlsm"))
        .ok_or_else(|| {
            BackendError::validation(
                "error.rfi.followUp.workbookInvalid: Follow Up workbook must have .xlsx or .xlsm extension",
            )
        })?;
    let backup = follow_up_path
        .parent()
        .ok_or_else(|| {
            BackendError::validation(
                "error.rfi.followUp.workbookInvalid: Follow Up workbook parent directory missing",
            )
        })?
        .join(format!(
            ".docflow-s5-backup-{}.{}",
            Uuid::new_v4().simple(),
            extension.to_ascii_lowercase()
        ));
    let backup_display = backup.to_str().ok_or_else(|| {
        BackendError::validation(
            "error.rfi.followUp.workbookInvalid: backup path contains invalid UTF-8",
        )
    })?;
    plan_from_classification(
        rfi_log_display,
        follow_up_display,
        backup_display,
        source_fingerprint,
        target_fingerprint,
        classification,
    )
}

pub fn revise_from_open_files(
    plan: &Plan,
    submitted_rows: &[Value],
    rfi_log: &mut File,
    follow_up: &mut File,
) -> Result<Plan, BackendError> {
    if plan.op_id != OPERATION_ID {
        return Err(BackendError::validation(
            "plan is not an RFI Follow Up plan",
        ));
    }
    let authority: FollowUpPlanMeta = serde_json::from_value(
        plan.meta
            .get("rfiFollowUp")
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    let original_rows: Vec<FollowUpRow> = serde_json::from_value(
        plan.meta
            .get("editor")
            .and_then(|editor| editor.get("rows"))
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    let mut revised_rows = submitted_rows
        .iter()
        .cloned()
        .map(|row| serde_json::from_value(row).map_err(|_| workbook_invalid()))
        .collect::<Result<Vec<FollowUpRow>, BackendError>>()?;
    if original_rows.len() != revised_rows.len() {
        return Err(workbook_invalid());
    }
    for (original, revised) in original_rows.iter().zip(&revised_rows) {
        if original.row_id != revised.row_id || !rows_equal_except_enabled(original, revised) {
            return Err(workbook_invalid());
        }
        if revised.enabled
            && (!original.selectable || original.disposition != FollowUpDisposition::Ready)
        {
            return Err(workbook_invalid());
        }
    }

    let fresh = preview_from_open_files(
        Path::new(&authority.rfi_log_workbook),
        Path::new(&authority.follow_up_workbook),
        rfi_log,
        follow_up,
    )?;
    let fresh_authority: FollowUpPlanMeta = serde_json::from_value(
        fresh
            .meta
            .get("rfiFollowUp")
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    if authority.rfi_log_workbook != fresh_authority.rfi_log_workbook
        || authority.follow_up_workbook != fresh_authority.follow_up_workbook
        || authority.rfi_log_fingerprint != fresh_authority.rfi_log_fingerprint
        || authority.follow_up_fingerprint != fresh_authority.follow_up_fingerprint
        || authority.layout != fresh_authority.layout
        || authority.layouts != fresh_authority.layouts
    {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookChanged",
        ));
    }
    let fresh_rows: Vec<FollowUpRow> = serde_json::from_value(
        fresh
            .meta
            .get("editor")
            .and_then(|editor| editor.get("rows"))
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    if fresh_rows.len() != original_rows.len()
        || original_rows
            .iter()
            .zip(&fresh_rows)
            .any(|(original, fresh)| !rows_equal_except_enabled(original, fresh))
    {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookChanged",
        ));
    }
    for (revised, fresh) in revised_rows.iter_mut().zip(fresh_rows) {
        revised.enabled = revised.enabled && fresh.selectable;
    }
    let mut warnings = fresh.warnings;
    warnings.retain(|warning| warning.key != "warn.rfi.followUp.noRowsSelected");
    if revised_rows.iter().all(|row| !row.enabled) {
        warnings.push(Warning {
            key: "warn.rfi.followUp.noRowsSelected".to_string(),
            subject: String::new(),
        });
    }
    let columns = editor_columns();
    let classification = json!({
        "layout": authority.layout,
        "layouts": authority.layouts,
        "rows": revised_rows,
        "warnings": warnings,
        "editor": editor_metadata(&revised_rows, columns),
    });
    let mut revised = plan_from_classification(
        &authority.rfi_log_workbook,
        &authority.follow_up_workbook,
        &authority.backup_workbook,
        fresh_authority.rfi_log_fingerprint,
        fresh_authority.follow_up_fingerprint,
        classification,
    )?;
    revised.plan_id.clone_from(&plan.plan_id);
    validate_owned_plan_budget(&revised)?;
    Ok(revised)
}

pub(super) fn rows_equal_except_enabled(left: &FollowUpRow, right: &FollowUpRow) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.enabled = false;
    right.enabled = false;
    left == right
}

pub(crate) fn plan_workbook_paths(plan: &Plan) -> Result<(String, String), BackendError> {
    let authority: FollowUpPlanMeta = serde_json::from_value(
        plan.meta
            .get("rfiFollowUp")
            .cloned()
            .ok_or_else(workbook_invalid)?,
    )
    .map_err(|_| workbook_invalid())?;
    Ok((authority.rfi_log_workbook, authority.follow_up_workbook))
}

pub(crate) fn validate_owned_plan_budget(plan: &Plan) -> Result<(), BackendError> {
    let mut writer = CountingLimitWriter {
        count: 0,
        limit: MAX_PREVIEW_AUTHORITY_BYTES,
    };
    serde_json::to_writer(&mut writer, plan)
        .map_err(|_| limit_exceeded_for("the preview is larger than 32 MiB"))
}

pub(crate) fn fingerprint_open_file(file: &mut File) -> Result<WorkbookFingerprint, BackendError> {
    let metadata = file.metadata().map_err(|_| workbook_invalid())?;
    let identity = file_identity(file).map_err(|_| workbook_invalid())?;
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0);
    file.seek(SeekFrom::Start(0))
        .map_err(|_| workbook_invalid())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| workbook_invalid())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| workbook_invalid())?;
    Ok(WorkbookFingerprint {
        identity,
        size: metadata.len(),
        mtime_ns,
        sha256: format!("{:x}", digest.finalize()),
    })
}

pub(super) fn plan_from_classification(
    rfi_log_workbook: &str,
    follow_up_workbook: &str,
    backup_workbook: &str,
    rfi_log_fingerprint: WorkbookFingerprint,
    follow_up_fingerprint: WorkbookFingerprint,
    classification: Value,
) -> Result<Plan, BackendError> {
    let object = classification.as_object().ok_or_else(workbook_invalid)?;
    let decode_field = |name: &str| object.get(name).cloned().ok_or_else(workbook_invalid);
    let layout: FollowUpLayout =
        serde_json::from_value(decode_field("layout")?).map_err(|_| workbook_invalid())?;
    let layouts: Vec<FollowUpLayout> = object
        .get("layouts")
        .and_then(|val| serde_json::from_value(val.clone()).ok())
        .unwrap_or_else(|| vec![layout.clone()]);
    let rows: Vec<FollowUpRow> =
        serde_json::from_value(decode_field("rows")?).map_err(|_| workbook_invalid())?;
    let warnings: Vec<Warning> =
        serde_json::from_value(decode_field("warnings")?).map_err(|_| workbook_invalid())?;
    let editor = decode_field("editor")?;
    let remarks = rows
        .iter()
        .filter(|row| row.enabled)
        .filter_map(|row| {
            let change = row.remark.as_ref()?;
            Some(
                if !row.selectable || row.disposition != FollowUpDisposition::Ready {
                    Err(workbook_invalid())
                } else {
                    Ok(FollowUpApprovedRemark {
                        row_id: row.row_id.clone(),
                        worksheet: row.target.worksheet.clone(),
                        row: row.row,
                        column: change.column,
                        expected: change.expected.clone(),
                        runs: change.runs.clone(),
                    })
                },
            )
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    let updates = rows
        .iter()
        .filter(|row| row.enabled && row.remark.is_none())
        .map(|row| {
            if !row.selectable || row.disposition != FollowUpDisposition::Ready {
                return Err(workbook_invalid());
            }
            Ok(FollowUpApprovedUpdate {
                row_id: row.row_id.clone(),
                canonical_rfi: canonical_follow_up_rfi(&row.rfi_no)
                    .map(|(canonical, _)| canonical)
                    .ok_or_else(workbook_invalid)?,
                proposed_status: row.proposed_status.clone().ok_or_else(workbook_invalid)?,
                proposed_rfi: row.proposed_rfi_no.clone().unwrap_or_else(|| {
                    row.target
                        .rfi_cell
                        .as_ref()
                        .map(|cell| cell.value.clone())
                        .unwrap_or_default()
                }),
                target: FollowUpCellTarget {
                    worksheet: row.target.worksheet.clone(),
                    row: row.target.row,
                    rfi_column: row.target.rfi_column,
                    status_column: row.target.status_column,
                    rfi_cell: row.target.rfi_cell.clone().ok_or_else(workbook_invalid)?,
                    status_cell: row
                        .target
                        .status_cell
                        .clone()
                        .ok_or_else(workbook_invalid)?,
                },
                source_evidence: row.source_evidence.clone(),
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    let plan_meta = FollowUpPlanMeta {
        rfi_log_workbook: rfi_log_workbook.to_string(),
        follow_up_workbook: follow_up_workbook.to_string(),
        backup_workbook: backup_workbook.to_string(),
        rfi_log_fingerprint: rfi_log_fingerprint.clone(),
        follow_up_fingerprint: follow_up_fingerprint.clone(),
        layout: layout.clone(),
        layouts: layouts.clone(),
    };
    let mut meta = serde_json::Map::new();
    meta.insert("editor".to_string(), editor);
    meta.insert(
        "rfiFollowUp".to_string(),
        serde_json::to_value(plan_meta).map_err(|_| workbook_invalid())?,
    );
    let steps = if updates.is_empty() && remarks.is_empty() {
        Vec::new()
    } else {
        let step_meta = FollowUpStepMeta {
            rfi_log_workbook: rfi_log_workbook.to_string(),
            rfi_log_fingerprint,
            follow_up_fingerprint,
            layout,
            layouts,
            updates,
            remarks,
        };
        let step_meta = serde_json::to_value(step_meta)
            .map_err(|_| workbook_invalid())?
            .as_object()
            .cloned()
            .ok_or_else(workbook_invalid)?;
        vec![Step {
            kind: StepKind::WorkbookUpdate,
            src: Some(follow_up_workbook.to_string()),
            dst: Some(backup_workbook.to_string()),
            note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
            meta: step_meta,
        }]
    };
    Ok(Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps,
        warnings,
        meta,
    })
}
