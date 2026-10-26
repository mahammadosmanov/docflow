use super::*;
use crate::publish_intake::{self, Disposition, LogRow};

const MAX_STAGING_ENTRIES: usize = 5_000;

/// Read-only Preview for the publish flow: PDF+DOCX pairs found at the top level of one
/// staging folder are mapped to existing RFI Log rows; only literal blank statuses become
/// `Ongoing`. Nothing is recursed, created, or sent.
pub fn preview_publish_status<P, C>(
    staging_dir: &Path,
    rfi_log_path: &Path,
    rfi_log_file: &mut File,
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let names = list_staging_names(staging_dir, &mut cancelled)?;
    let analysis = publish_intake::analyze(&names.iter().map(String::as_str).collect::<Vec<_>>());

    progress(ProgressEvent {
        phase: "publish_workbook_read".to_string(),
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
        BackendError::validation(format!("error.rfi.publishStatus.workbookInvalid: {error}"))
    })?;
    let log_rows = read_rfi_log_status_rows(&workbook)?;
    progress(ProgressEvent {
        phase: "publish_workbook_read".to_string(),
        done: 1,
        total: 1,
        subject: rfi_log_path.to_string_lossy().into_owned(),
    });

    let intake_rows = log_rows
        .iter()
        .map(|row| LogRow {
            code: row.canonical_rfi.clone(),
            status: row
                .status_cell
                .as_ref()
                .map(|cell| cell.value.nfc().collect::<String>())
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let dispositions = publish_intake::match_log(&analysis, &intake_rows);

    let mut updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut review_rows = Vec::new();
    let mut ready_codes = Vec::new();
    for (index, (code, files)) in analysis.rfis.iter().enumerate() {
        let row_id = format!("publish-row-{}", index + 1);
        let matching = log_rows
            .iter()
            .filter(|row| row.canonical_rfi == *code)
            .collect::<Vec<_>>();
        let mut disposition = dispositions[code];
        let mut pending = Vec::new();
        if disposition == Disposition::Ready {
            if matching
                .iter()
                .all(|row| safe_cell(&row.status_cell, row.status_merged))
            {
                for row in &matching {
                    let cell = row.status_cell.as_ref().expect("safe status cell exists");
                    pending.push((
                        row.sheet.clone(),
                        format!("{}:{}", row.row, row.status_col),
                        dto(
                            &row_id,
                            cell,
                            "Ongoing".to_string(),
                            row.row,
                            row.status_col,
                        ),
                    ));
                }
            } else {
                disposition = Disposition::NotInLog;
            }
        }
        let (key, action, selectable) = match (disposition, pending.is_empty()) {
            (Disposition::Ready, false) => (
                "ready",
                "Set matching Inspection Status cell(s) to Ongoing",
                true,
            ),
            (Disposition::Ready, true) | (Disposition::NotInLog, _) if !matching.is_empty() => (
                "unsafeCell",
                "Inspection Status cell is missing, merged, or formula-bearing",
                false,
            ),
            (Disposition::IncompleteFiles, _) => (
                "incompleteFiles",
                "Exactly one PDF and one DOCX are required",
                false,
            ),
            (Disposition::StatusNotBlank, _) => (
                "statusNotBlank",
                "Inspection Status is already filled",
                false,
            ),
            _ => ("notInLog", "RFI was not found in the RFI Log", false),
        };
        if selectable {
            ready_codes.push(code.clone());
            for (sheet, cell_key, update) in pending {
                updates.entry(sheet).or_default().insert(cell_key, update);
            }
        }
        let current_status = matching
            .iter()
            .filter_map(|row| row.status_cell.as_ref())
            .map(|cell| cell.value.nfc().collect::<String>().trim().to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(" / ");
        let (discipline, number) =
            parse_rfi_discipline_and_number(code).expect("exact canonical RFI code is parseable");
        review_rows.push(json!({
            "_rowId": row_id,
            "discipline": discipline,
            "rfiNo": format!("{discipline}-{number:04}"),
            "canonicalRfi": code,
            "pdfName": files.pdf.join("\n"),
            "docxName": files.docx.join("\n"),
            "currentStatus": current_status,
            "proposedStatus": "Ongoing",
            "action": action,
            "disposition": format!("rfi.publishStatus.disposition.{key}"),
            "enabled": selectable,
            "selectable": selectable,
        }));
    }

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
                ".docflow-publish-status-backup-{}.{}",
                Uuid::new_v4().simple(),
                extension
            ));
        let step_meta = SystemSyncStepMeta {
            target_path: rfi_log_path.to_string_lossy().into_owned(),
            backup_path: backup_path.to_string_lossy().into_owned(),
            source_fingerprint: fingerprint,
            workbook_kind: SyncWorkbookKind::RfiLogPublish,
            updates,
            canonical_rfi_codes: ready_codes,
            summary: format!("Set {count} RFI Log Inspection Status cell(s) to Ongoing"),
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
            note_key: Some("note.rfi.publishStatus.rfiLog".to_string()),
            meta,
        });
    }

    let mut warnings = analysis
        .ambiguous
        .iter()
        .map(|name| Warning {
            key: "warn.rfi.publishStatus.ambiguousName".to_string(),
            subject: name.clone(),
        })
        .collect::<Vec<_>>();
    if steps.is_empty() {
        warnings.push(Warning {
            key: "warn.rfi.publishStatus.noChangesNeeded".to_string(),
            subject: "No safe RFI Log status changes are ready".to_string(),
        });
    }
    let columns = vec![
        json!({"key":"enabled","labelKey":"rfi.publishStatus.column.enabled","kind":"bool","bulk":true,"disabledWhen":{"field":"selectable","equals":false}}),
        json!({"key":"discipline","labelKey":"rfi.publishStatus.column.discipline","kind":"readonly"}),
        json!({"key":"rfiNo","labelKey":"rfi.publishStatus.column.rfiNo","kind":"readonly"}),
        json!({"key":"pdfName","labelKey":"rfi.publishStatus.column.pdfName","kind":"readonly"}),
        json!({"key":"docxName","labelKey":"rfi.publishStatus.column.docxName","kind":"readonly"}),
        json!({"key":"currentStatus","labelKey":"rfi.publishStatus.column.currentStatus","kind":"readonly"}),
        json!({"key":"proposedStatus","labelKey":"rfi.publishStatus.column.proposedStatus","kind":"readonly"}),
        json!({"key":"disposition","labelKey":"rfi.publishStatus.column.disposition","kind":"readonly","display":"localizedKey"}),
        json!({"key":"action","labelKey":"rfi.publishStatus.column.action","kind":"readonly","detail":true}),
    ];
    let editor = json!({
        "kind":"recordTable","layout":"compact","titleKey":"rfi.publishStatus.editor.title",
        "rowCountKey":"rfi.publishStatus.editor.rowCount","requiredFields":["enabled"],
        "groupField":"discipline","selectionGroupFields":[],"columns":columns,"rows":review_rows,
        "duplicateEnabled":false,"selectionEnabled":true,"bulkEnabled":true,"reorderable":false,
        "detailsEnabled":true,"detailsDefaultVisible":false,"copyEnabled":false
    });
    let mut meta = JsonObject::new();
    meta.insert("editor".to_string(), editor);
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(Plan {
        plan_id: String::new(),
        op_id: PUBLISH_STATUS_OPERATION_ID.to_string(),
        steps,
        warnings,
        meta,
    })
}

fn list_staging_names<C>(dir: &Path, cancelled: &mut C) -> Result<Vec<String>, BackendError>
where
    C: FnMut() -> bool,
{
    let invalid = || BackendError::validation("error.rfi.publishStatus.stagingInvalid");
    let mut names = Vec::new();
    for (count, entry) in fs::read_dir(dir).map_err(|_| invalid())?.enumerate() {
        if count >= MAX_STAGING_ENTRIES {
            return Err(BackendError::validation(
                "error.rfi.publishStatus.stagingTooLarge",
            ));
        }
        if cancelled() {
            return Err(BackendError::validation("error.operation.cancelled"));
        }
        let entry = entry.map_err(|_| invalid())?;
        // file_type() does not follow symlinks, so links and directories are skipped.
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && let Some(name) = entry.file_name().to_str()
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}
