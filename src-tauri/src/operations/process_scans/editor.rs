use std::path::Path;

use crate::operations::process_scans::*;

pub(super) fn attach_scan_editor(plan: &mut Plan, destination: &Path, organize: bool) {
    let mut rows = Vec::new();
    for (index, step) in plan.steps.iter_mut().enumerate() {
        if step.note_key.as_deref() != Some("note.renamed") {
            continue;
        }
        let Some(code) = step
            .meta
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let Some(target) = step.dst.as_deref().map(str::to_owned) else {
            continue;
        };
        let row_id = format!("scan-output-{index}");
        step.meta
            .insert("scanEditRowId".to_string(), Value::String(row_id.clone()));
        let source_file = step
            .src
            .as_deref()
            .and_then(|source| Path::new(source).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let evidence_key = if step
            .meta
            .get("source")
            .and_then(Value::as_str)
            .is_some_and(|source| source.contains("CLV->CIV"))
        {
            "rfi.fastScan.evidence.clvToCiv"
        } else {
            "rfi.fastScan.evidence.confirmed"
        };
        rows.push(serde_json::json!({
            "_rowId": row_id,
            "sourceFile": source_file,
            "evidenceKey": evidence_key,
            "detectedCode": code,
            "code": code,
            "target": target,
        }));
    }
    if rows.is_empty() {
        return;
    }
    plan.meta.insert(
        "scanEdit".to_string(),
        serde_json::json!({
            "destinationRoot": destination.to_string_lossy(),
            "organizeIntoRfiFolders": organize,
        }),
    );
    plan.meta.insert(
        "editor".to_string(),
        serde_json::json!({
            "kind": "rfiScan",
            "titleKey": "rfi.fastScan.editor.title",
            "rowCountKey": "rfi.fastScan.editor.rowCount",
            "groupField": "_rowId",
            "requiredFields": ["code"],
            "columns": [
                {"key": "sourceFile", "labelKey": "rfi.fastScan.column.sourceFile", "kind": "readonly", "width": 170, "bulk": false},
                {"key": "evidenceKey", "labelKey": "rfi.fastScan.column.evidence", "kind": "readonly", "display": "localizedKey", "width": 170, "bulk": false},
                {"key": "detectedCode", "labelKey": "rfi.fastScan.column.detectedCode", "kind": "readonly", "width": 270, "bulk": false},
                {"key": "code", "labelKey": "rfi.fastScan.column.code", "kind": "text", "width": 310, "bulk": false},
                {"key": "target", "labelKey": "rfi.fastScan.column.target", "kind": "readonly", "width": 360, "bulk": false}
            ],
            "rows": rows,
        }),
    );
}

pub fn revise(plan: &Plan, payload: &Value) -> Result<Plan, BackendError> {
    if plan.op_id != FAST_COVER_OPERATION_ID {
        return Err(BackendError::validation(
            "error.rfi.fastScan.revisionInvalid",
        ));
    }
    let original_rows = plan
        .meta
        .get("editor")
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
    let submitted_rows = payload
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
    if original_rows.is_empty() || original_rows.len() != submitted_rows.len() {
        return Err(BackendError::validation(
            "error.rfi.fastScan.revisionInvalid",
        ));
    }
    let edit = plan
        .meta
        .get("scanEdit")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
    let destination = edit
        .get("destinationRoot")
        .and_then(Value::as_str)
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
    let organize = edit
        .get("organizeIntoRfiFolders")
        .and_then(Value::as_bool)
        .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;

    let mut originals = BTreeMap::new();
    for row in original_rows {
        let row_id = row
            .get("_rowId")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
        if originals.insert(row_id.to_string(), row).is_some() {
            return Err(BackendError::validation(
                "error.rfi.fastScan.revisionInvalid",
            ));
        }
    }

    let mut seen = BTreeSet::new();
    let mut edits = BTreeMap::new();
    let mut targets = BTreeSet::new();
    for submitted in submitted_rows {
        let row_id = submitted
            .get("_rowId")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
        if !seen.insert(row_id.to_string()) {
            return Err(BackendError::validation(
                "error.rfi.fastScan.revisionInvalid",
            ));
        }
        let original = originals
            .get(row_id)
            .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
        let code = submitted
            .get("code")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation("error.rfi.fastScan.codeInvalid"))?;
        let code = crate::rfi_code::normalize_full_code(code)
            .ok_or_else(|| BackendError::validation("error.rfi.fastScan.codeInvalid"))?;
        let mut expected = (*original).clone();
        expected["code"] = Value::String(code.clone());
        let unchanged_except_code =
            submitted
                .as_object()
                .zip(expected.as_object())
                .is_some_and(|(submitted, expected)| {
                    submitted.len() == expected.len()
                        && expected
                            .iter()
                            .all(|(key, value)| key == "code" || submitted.get(key) == Some(value))
                });
        if !unchanged_except_code {
            return Err(BackendError::validation(
                "error.rfi.fastScan.revisionInvalid",
            ));
        }
        let (folder, target) = process_scan_targets(destination, &code, organize);
        if !target.starts_with(destination) || !targets.insert(target.clone()) {
            return Err(BackendError::validation("error.rfi.fastScan.targetExists"));
        }
        let folder =
            folder.ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
        scan_edit_folder_exists(destination, &folder)?;
        match fs::symlink_metadata(&target) {
            Ok(_) => {
                return Err(BackendError::validation("error.rfi.fastScan.targetExists"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(BackendError::path_invalid(
                    "scan output target changed during Preview revision",
                ));
            }
        }
        expected["target"] = Value::String(target.to_string_lossy().into_owned());
        edits.insert(
            row_id.to_string(),
            (code.to_string(), folder, target, expected),
        );
    }
    if seen.len() != originals.len() {
        return Err(BackendError::validation(
            "error.rfi.fastScan.revisionInvalid",
        ));
    }

    let mut steps = Vec::with_capacity(plan.steps.len() + edits.len());
    let mut created_folders = BTreeSet::new();
    let mut revised_ids = BTreeSet::new();
    for original_step in &plan.steps {
        if original_step.kind == StepKind::Mkdir
            && original_step.note_key.as_deref() == Some("note.folder_created")
        {
            continue;
        }
        let Some(row_id) = original_step
            .meta
            .get("scanEditRowId")
            .and_then(Value::as_str)
        else {
            steps.push(original_step.clone());
            continue;
        };
        let (code, folder, target, _) = edits
            .get(row_id)
            .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
        if !revised_ids.insert(row_id.to_string()) {
            return Err(BackendError::validation(
                "error.rfi.fastScan.revisionInvalid",
            ));
        }
        let folder_created = !directory_exists(folder)?;
        if folder_created && created_folders.insert(folder.clone()) {
            let mut meta = JsonObject::new();
            meta.insert("code".to_string(), Value::String(code.clone()));
            steps.push(step(
                StepKind::Mkdir,
                None,
                Some(folder),
                "note.folder_created",
                meta,
            ));
        }
        let mut revised = original_step.clone();
        revised.dst = Some(target.to_string_lossy().into_owned());
        revised
            .meta
            .insert("code".to_string(), Value::String(code.clone()));
        revised
            .meta
            .insert("folderCreated".to_string(), Value::Bool(folder_created));
        steps.push(revised);
    }
    if revised_ids.len() != edits.len() {
        return Err(BackendError::validation(
            "error.rfi.fastScan.revisionInvalid",
        ));
    }

    let mut rows = original_rows.clone();
    for (index, original) in original_rows.iter().enumerate() {
        let row_id = original
            .get("_rowId")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
        rows[index] = edits
            .get(row_id)
            .map(|(_, _, _, row)| row.clone())
            .ok_or_else(|| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
    }
    let mut meta = plan.meta.clone();
    meta["editor"]["rows"] = Value::Array(rows);
    Ok(Plan {
        plan_id: String::new(),
        op_id: FAST_COVER_OPERATION_ID.to_string(),
        steps,
        warnings: plan.warnings.clone(),
        meta,
    })
}

fn scan_edit_folder_exists(destination: &Path, folder: &Path) -> Result<bool, BackendError> {
    if !folder.starts_with(destination) || folder == destination {
        return Err(BackendError::validation(
            "error.rfi.fastScan.revisionInvalid",
        ));
    }
    let root = fs::symlink_metadata(destination).map_err(|_| {
        BackendError::path_invalid("scan destination changed during Preview revision")
    })?;
    if root.file_type().is_symlink() || !root.is_dir() {
        return Err(BackendError::path_invalid(
            "scan destination changed during Preview revision",
        ));
    }
    let relative = folder
        .strip_prefix(destination)
        .map_err(|_| BackendError::validation("error.rfi.fastScan.revisionInvalid"))?;
    let mut current = destination.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(BackendError::validation("error.rfi.fastScan.targetExists"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => {
                return Err(BackendError::path_invalid(
                    "scan output folder changed during Preview revision",
                ));
            }
        }
    }
    Ok(true)
}

fn directory_exists(folder: &Path) -> Result<bool, BackendError> {
    match fs::symlink_metadata(folder) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(BackendError::validation("error.rfi.fastScan.targetExists"))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(BackendError::path_invalid(
            "scan output folder changed during Preview revision",
        )),
    }
}
