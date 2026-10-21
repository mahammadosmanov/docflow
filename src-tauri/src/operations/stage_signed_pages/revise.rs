use super::*;

pub fn revise(plan: &Plan, payload: &Value) -> Result<Plan, BackendError> {
    if plan.op_id != OPERATION_ID {
        return Err(BackendError::validation(
            "signed-page plan identity is invalid",
        ));
    }
    let existing_rows = plan
        .meta
        .get("editor")
        .and_then(Value::as_object)
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("signed-page review rows are missing"))?;
    let supplied_rows = payload
        .as_object()
        .and_then(|object| object.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("signed-page review rows are invalid"))?;
    if existing_rows.len() != supplied_rows.len() {
        return Err(BackendError::validation(
            "signed-page review rows cannot be added or removed",
        ));
    }
    let existing_ids = existing_rows
        .iter()
        .filter_map(|row| row.get("_rowId").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let supplied_ids = supplied_rows
        .iter()
        .filter_map(|row| row.get("_rowId").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    if existing_ids.len() != existing_rows.len() || supplied_ids != existing_ids {
        return Err(BackendError::validation(
            "signed-page review row identity is invalid",
        ));
    }
    let cached_candidates = plan
        .meta
        .get("archiveCandidateSteps")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("signed-page archive candidates are missing"))?;
    let mut steps = plan
        .steps
        .iter()
        .filter(|step| {
            step.kind == StepKind::CleanupTemp
                || step.meta.get("warningReview").and_then(Value::as_bool) == Some(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut meta = plan.meta.clone();
    let mut revised_rows = existing_rows.clone();
    for row in &mut revised_rows {
        let row_id = row
            .get("_rowId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let supplied = supplied_rows
            .iter()
            .find(|candidate| {
                candidate.get("_rowId").and_then(Value::as_str) == Some(row_id.as_str())
            })
            .expect("validated row identities");
        let enabled = supplied
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| BackendError::validation("signed-page row state is invalid"))?;
        let selected = supplied
            .get("archiveFile")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation("signed-page archive selection is invalid"))?;
        let choices = cached_candidates
            .get(&row_id)
            .and_then(Value::as_object)
            .ok_or_else(|| BackendError::validation("signed-page archive choices are invalid"))?;
        if !selected.is_empty() && !choices.contains_key(selected) {
            return Err(BackendError::validation(
                "signed-page archive selection was not offered by preview",
            ));
        }
        if enabled {
            if selected.is_empty() {
                return Err(BackendError::validation(
                    "signed-page archive selection is required",
                ));
            }
            let selected_steps: Vec<Step> = serde_json::from_value(
                choices
                    .get(selected)
                    .expect("selected candidate is allowlisted")
                    .clone(),
            )
            .map_err(|_| BackendError::validation("signed-page candidate plan is invalid"))?;
            if selected_steps.iter().any(|step| {
                !matches!(step.kind, StepKind::Mkdir | StepKind::PdfFirstPageReplace)
                    || (step.kind == StepKind::PdfFirstPageReplace
                        && step.meta.get("rowId").and_then(Value::as_str) != Some(row_id.as_str()))
            }) {
                return Err(BackendError::validation(
                    "signed-page candidate plan contains an invalid step",
                ));
            }
            extend_candidate_steps(&mut steps, &selected_steps);
        }
        let revised = row.as_object_mut().expect("validated editor row");
        revised.insert("enabled".to_string(), Value::Bool(enabled));
        revised.insert(
            "archiveFile".to_string(),
            Value::String(selected.to_string()),
        );
    }
    meta.get_mut("editor")
        .and_then(Value::as_object_mut)
        .expect("validated editor")
        .insert("rows".to_string(), Value::Array(revised_rows));
    Ok(Plan {
        plan_id: String::new(),
        op_id: plan.op_id.clone(),
        steps,
        warnings: plan.warnings.clone(),
        meta,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_archive_candidate(
    code: &str,
    row_id: &str,
    candidate: &Candidate,
    archive_pdf: &Path,
    signed_facts: &PdfInspection,
    signed_hash: &str,
    signed_root: &Path,
    archive_root: &Path,
    review_root: &Path,
    plan: &mut Plan,
) -> Result<Option<PreparedArchive>, BackendError> {
    let relative = archive_pdf
        .strip_prefix(archive_root)
        .map_err(|_| BackendError::path_invalid("archive match escaped the selected root"))?
        .to_path_buf();
    let destination = review_root.join(&relative);
    if destination.exists() {
        let stale_temps = stale_hardlink_temps(&destination, review_root);
        let destination_hash = if stale_temps.is_empty() {
            None
        } else {
            file_sha256(&destination).ok()
        };
        if let Some(destination_hash) = destination_hash {
            for stale_temp in stale_temps {
                let mut meta = JsonObject::new();
                meta.insert(
                    "reviewRoot".to_string(),
                    Value::String(review_root.to_string_lossy().into_owned()),
                );
                meta.insert(
                    "destinationSha256".to_string(),
                    Value::String(destination_hash.clone()),
                );
                plan.steps.push(Step {
                    kind: StepKind::CleanupTemp,
                    src: Some(stale_temp.to_string_lossy().into_owned()),
                    dst: Some(destination.to_string_lossy().into_owned()),
                    note_key: Some("note.rfi.cleanupStaleTemp".to_string()),
                    meta,
                });
            }
            plan.warnings.push(warning(
                "warn.rfi.staleTempFound",
                &relative.to_string_lossy(),
            ));
        } else {
            plan.warnings.push(warning(
                "warn.rfi.reviewExists",
                &relative.to_string_lossy(),
            ));
        }
        return Ok(None);
    }
    if destination
        .ancestors()
        .take_while(|path| *path != review_root)
        .any(|path| {
            fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        })
    {
        plan.warnings.push(warning(
            "warn.rfi.reviewPathBlocked",
            &relative.to_string_lossy(),
        ));
        return Ok(None);
    }
    let archive_facts = match inspect(archive_pdf) {
        Ok(facts) if fidelity_supported(&facts, PdfRole::Archive) => facts,
        Ok(facts) => {
            plan.warnings.push(warning(
                "warn.rfi.pdfFidelityUnsupported",
                &truncate(
                    &format!("{code} — {}", fidelity_reason(&facts, PdfRole::Archive)),
                    300,
                ),
            ));
            return Ok(None);
        }
        Err(error) => {
            plan.warnings.push(warning(
                "warn.rfi.pdfFidelityUnsupported",
                &truncate(&format!("{code} — {error}"), 300),
            ));
            return Ok(None);
        }
    };
    if archive_facts.page_count == 0 {
        plan.warnings.push(warning(
            "warn.rfi.pdfFidelityUnsupported",
            &format!("{code} — PDF has no pages"),
        ));
        return Ok(None);
    }
    if archive_facts.pages[0].widgets > 0 {
        plan.warnings.push(warning(
            "warn.rfi.pdfFidelityUnsupported",
            &format!("{code} — page-one form widgets require manual review"),
        ));
        return Ok(None);
    }
    let archive_hash = file_sha256(archive_pdf)
        .map_err(|_| BackendError::path_invalid("archive PDF could not be hashed"))?;
    let mut steps = Vec::new();
    add_directory_steps(
        destination
            .parent()
            .ok_or_else(|| BackendError::path_invalid("review destination has no parent"))?,
        review_root,
        &mut BTreeSet::new(),
        &mut steps,
    )?;
    let mut meta = JsonObject::new();
    meta.insert("code".to_string(), Value::String(code.to_string()));
    meta.insert(
        "signedPdf".to_string(),
        Value::String(candidate.path.to_string_lossy().into_owned()),
    );
    meta.insert(
        "signedRoot".to_string(),
        Value::String(signed_root.to_string_lossy().into_owned()),
    );
    meta.insert(
        "archiveRoot".to_string(),
        Value::String(archive_root.to_string_lossy().into_owned()),
    );
    meta.insert(
        "reviewRoot".to_string(),
        Value::String(review_root.to_string_lossy().into_owned()),
    );
    meta.insert(
        "relativePath".to_string(),
        Value::String(relative.to_string_lossy().into_owned()),
    );
    meta.insert("confidence".to_string(), Value::from(candidate.confidence));
    meta.insert(
        "method".to_string(),
        Value::String(candidate.method.clone()),
    );
    meta.insert("archiveSha256".to_string(), Value::String(archive_hash));
    meta.insert(
        "signedSha256".to_string(),
        Value::String(signed_hash.to_string()),
    );
    meta.insert(
        "archivePageCount".to_string(),
        Value::from(archive_facts.page_count as u64),
    );
    meta.insert(
        "signedPageCount".to_string(),
        Value::from(signed_facts.page_count as u64),
    );
    meta.insert(
        "signedPageIndex".to_string(),
        Value::from(candidate.page_index as u64),
    );
    meta.insert("rowId".to_string(), Value::String(row_id.to_string()));
    steps.push(Step {
        kind: StepKind::PdfFirstPageReplace,
        src: Some(archive_pdf.to_string_lossy().into_owned()),
        dst: Some(destination.to_string_lossy().into_owned()),
        note_key: Some("note.rfi.signedFirstPageStaged".to_string()),
        meta,
    });
    Ok(Some(PreparedArchive {
        archive: archive_pdf.to_path_buf(),
        relative,
        steps,
    }))
}

pub(super) fn accept_page_preview(value: String, used: &mut usize) -> Option<String> {
    let next = used.checked_add(value.len())?;
    if next > MAX_PAGE_PREVIEW_BYTES {
        return None;
    }
    *used = next;
    Some(value)
}

pub(super) fn extend_candidate_steps(target: &mut Vec<Step>, candidate: &[Step]) {
    for step in candidate {
        if step.kind == StepKind::Mkdir
            && target.iter().any(|existing| {
                existing.kind == StepKind::Mkdir && existing.dst.as_deref() == step.dst.as_deref()
            })
        {
            continue;
        }
        target.push(step.clone());
    }
}
