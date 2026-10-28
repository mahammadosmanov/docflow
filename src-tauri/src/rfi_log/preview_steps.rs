use super::*;

/// Outcome of applying one pasted RFI email to the requested sheet counts.
pub(super) struct EmailOutcome {
    pub(super) extraction: Option<EmailExtraction>,
    pub(super) discipline: Option<String>,
    pub(super) notes: Vec<String>,
    pub(super) warnings: Vec<Warning>,
}

/// Runs offline or Gemini extraction for a pasted email and folds its RFI count into `counts`.
pub(super) fn extract_email_request(
    values: &Map<String, Value>,
    email_text: &str,
    detected: Option<String>,
    sources: &BTreeMap<String, Source>,
    counts: &mut BTreeMap<String, usize>,
    gemini: Option<&GeminiClient>,
    cancelled: &impl Fn() -> bool,
) -> Result<EmailOutcome, BackendError> {
    let mut outcome = EmailOutcome {
        extraction: None,
        discipline: None,
        notes: Vec::new(),
        warnings: Vec::new(),
    };
    if email_text.is_empty() {
        return Ok(outcome);
    }
    let requested = DISCIPLINES
        .iter()
        .filter(|discipline| counts.contains_key(**discipline))
        .copied()
        .collect::<Vec<_>>();
    let discipline = detected
        .filter(|discipline| sources.contains_key(discipline))
        .unwrap_or_else(|| requested[0].to_string());
    let source = &sources[&discipline];
    let context = EmailContext {
        discipline: discipline.clone(),
        itp_to_items: source.data.itp_to_items.clone(),
        known_tags: source.data.tags.clone(),
        known_subcontractors: source.data.subcontractors.clone(),
    };
    let offline = offline_extract(email_text, &context);
    let provider = value_string(values, "email_provider").to_ascii_lowercase();
    let provider = if provider == "gemini" {
        "gemini"
    } else {
        "offline"
    };
    let (selected, fallback_error) = if provider == "gemini" {
        checkpoint(cancelled)?;
        extract_with_gemini(values, email_text, &discipline, &source.data, gemini).map_or_else(
            |error| (offline, Some(safe_error(&error))),
            |result| (result, None),
        )
    } else {
        (offline, None)
    };
    if (1..=MAX_RFI_COUNT).contains(&selected.count) {
        counts.insert(discipline.clone(), selected.count);
    }
    outcome.notes.extend(
        selected
            .notes
            .iter()
            .filter(|note| !note.trim().is_empty())
            .cloned(),
    );
    outcome.warnings.push(Warning {
        key: "warn.rfi.emailExtracted".to_string(),
        subject: format!(
            "{} | {} {} RFI",
            selected.provider, discipline, counts[&discipline]
        ),
    });
    if provider == "gemini" {
        outcome.warnings.push(Warning {
            key: "warn.rfi.geminiPrivacy".to_string(),
            subject: String::new(),
        });
    }
    if let Some(error) = fallback_error {
        outcome.warnings.push(Warning {
            key: "warn.rfi.aiFallback".to_string(),
            subject: truncate_chars(&error, 300),
        });
    }
    outcome.discipline = Some(discipline);
    outcome.extraction = Some(selected);
    Ok(outcome)
}

/// Deduplicates extraction review notes into one `warn.rfi.emailNeedsReview` warning.
pub(super) fn email_review_warning(notes: &mut Vec<String>) -> Option<Warning> {
    if notes.is_empty() {
        return None;
    }
    deduplicate(notes);
    Some(Warning {
        key: "warn.rfi.emailNeedsReview".to_string(),
        subject: truncate_chars(&notes.join(" | "), MAX_WARNING_CHARS),
    })
}

/// Builds the single guarded workbook-update step, with its backup destination.
pub(super) fn workbook_update_step(
    workbook_path: &Path,
    signature: &SourceSignature,
    batches: &[Batch],
) -> Result<Step, BackendError> {
    let extension = workbook_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("xlsx");
    let backup_name = format!(
        "{}_{:}_{}.{}",
        workbook_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("RFI Log"),
        Local::now().format("%Y%m%d_%H%M%S"),
        &Uuid::new_v4().simple().to_string()[..8],
        extension
    );
    let backup_path = workbook_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".docflow-backups")
        .join(backup_name);
    let mut step_meta = JsonObject::new();
    insert_json(&mut step_meta, "sourceSignature", signature)?;
    insert_json(&mut step_meta, "batches", &batches)?;
    if batches.len() == 1 {
        let batch = &batches[0];
        insert_value(&mut step_meta, "discipline", batch.discipline.clone());
        insert_value(&mut step_meta, "startRow", batch.start_row);
        insert_json(&mut step_meta, "records", &batch.records)?;
        insert_json(&mut step_meta, "baseRfis", &batch.base_rfis)?;
        insert_json(&mut step_meta, "presets", &batch.presets)?;
        insert_json(&mut step_meta, "lookupColumns", &batch.lookup_columns)?;
        insert_json(&mut step_meta, "sourceData", &batch.source_data)?;
    }
    Ok(Step {
        kind: StepKind::WorkbookUpdate,
        src: Some(workbook_path.to_string_lossy().into_owned()),
        dst: Some(backup_path.to_string_lossy().into_owned()),
        note_key: Some("note.rfi.workbookUpdate".to_string()),
        meta: step_meta,
    })
}

/// Adds copy sources and the email-extraction summary to the plan's editor metadata.
pub(super) fn attach_editor_context(
    meta: &mut JsonObject,
    workbook_path: &Path,
    batches: &[Batch],
    extraction: Option<&EmailExtraction>,
    discipline: Option<&str>,
    counts: &BTreeMap<String, usize>,
) -> Result<(), BackendError> {
    let copy_sources = batches
        .iter()
        .map(|batch| {
            Ok((
                batch.discipline.clone(),
                serde_json::to_value(copy_source(workbook_path, batch)?)
                    .map_err(|_| BackendError::protocol("copy source could not be encoded"))?,
            ))
        })
        .collect::<Result<Map<String, Value>, BackendError>>()?;
    let editor = meta
        .get_mut("editor")
        .and_then(Value::as_object_mut)
        .expect("editor meta is an object");
    editor.insert(
        "copySources".to_string(),
        Value::Object(copy_sources.clone()),
    );
    if copy_sources.len() == 1 {
        editor.insert(
            "copySource".to_string(),
            copy_sources.values().next().cloned().unwrap_or(Value::Null),
        );
    }
    if let (Some(extraction), Some(discipline)) = (extraction, discipline) {
        editor.insert(
            "emailExtraction".to_string(),
            json!({
                "provider": extraction.provider,
                "confidence": extraction.confidence,
                "count": counts.get(discipline).copied().unwrap_or_default(),
                "sheet": discipline,
            }),
        );
    }
    Ok(())
}

/// Allocates one sheet's new RFI rows, applies that sheet's email extraction when present,
/// and returns the batch, its overflow count, and any extraction review notes.
pub(super) fn sheet_batch(
    discipline: &str,
    source: &Source,
    count: usize,
    wrap: bool,
    issue_day: NaiveDate,
    start_day: NaiveDate,
    email: Option<&EmailExtraction>,
) -> Result<(Batch, usize, Vec<String>), BackendError> {
    let (slots, overflowed) = allocate(&source.occupied, start_day, count, wrap)?;
    let mut sheet_rows = Vec::new();
    let mut base_rfis = Vec::new();
    for (index, slot) in slots.iter().enumerate() {
        let number = source.max_number + u32::try_from(index).unwrap_or(u32::MAX) + 1;
        let rfi = format!("AZE-SUN-INT-QMS-RFI-{discipline}-{number:04}");
        base_rfis.push(rfi.clone());
        sheet_rows.push(record(index, discipline, &rfi, source, issue_day, *slot));
    }
    let mut notes = Vec::new();
    if let Some(extraction) = email {
        let (extracted_rows, extracted_notes) = rows_from_email(&sheet_rows, extraction, source);
        sheet_rows = extracted_rows;
        notes = extracted_notes;
    }
    for row in &mut sheet_rows {
        clean_record(row);
    }
    let start_row = source.last_row + 1;
    let end_row = start_row + u32::try_from(sheet_rows.len().saturating_sub(1)).unwrap_or(u32::MAX);
    if end_row > source.data.max_row {
        return Err(BackendError::validation(format!(
            "{discipline} sheet'inde yeni satirlar icin ayrilmis kapasite kalmadi"
        )));
    }
    let batch = Batch {
        discipline: discipline.to_string(),
        start_row,
        records: sheet_rows,
        base_rfis,
        presets: source.data.presets.clone(),
        lookup_columns: source.data.lookup_columns.clone(),
        max_row: source.data.max_row,
        source_data: source.data.clone(),
    };
    Ok((batch, overflowed, notes))
}
