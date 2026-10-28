use super::*;

pub fn revise(plan: &Plan, payload: &Value) -> Result<Plan, BackendError> {
    let step = only_workbook_step(plan)?;
    let batches = decode_batches(step)?;
    let raw_rows = payload
        .as_object()
        .and_then(|object| object.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("gecersiz RFI satiri"))?;
    if raw_rows.len() > MAX_PHYSICAL_ROWS {
        return Err(BackendError::validation(
            "tek planda en fazla 500 fiziksel satir olabilir",
        ));
    }
    let allowed = batches
        .iter()
        .map(|batch| batch.discipline.clone())
        .collect::<BTreeSet<_>>();
    let supplied = raw_rows
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|row| row.get("_sheet"))
        .filter_map(Value::as_str)
        .map(str::to_ascii_uppercase)
        .collect::<BTreeSet<_>>();
    if supplied != allowed {
        return Err(BackendError::validation(
            "RFI sheet gruplari eklenemez veya silinemez",
        ));
    }
    let mut revised_batches = Vec::new();
    let mut revised_rows = Vec::new();
    let mut sources = BTreeMap::new();
    for batch in batches {
        let sheet_values = raw_rows
            .iter()
            .filter(|row| {
                row.get("_sheet")
                    .and_then(Value::as_str)
                    .is_some_and(|sheet| sheet.eq_ignore_ascii_case(&batch.discipline))
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut records = Vec::new();
        for (index, value) in sheet_values.into_iter().enumerate() {
            let mut record: Record = serde_json::from_value(value)
                .map_err(|_| BackendError::validation("gecersiz RFI satiri"))?;
            clean_record(&mut record);
            validate_record(&mut record, index, &batch)?;
            records.push(record);
        }
        if records.is_empty() {
            return Err(BackendError::validation("en az bir RFI satiri gerekli"));
        }
        let present = records
            .iter()
            .map(|record| record.rfi_no.clone())
            .collect::<BTreeSet<_>>();
        let expected = batch.base_rfis.iter().cloned().collect::<BTreeSet<_>>();
        if present != expected {
            let missing = expected.difference(&present).cloned().collect::<Vec<_>>();
            return Err(BackendError::validation(format!(
                "RFI grubu silinemez: {}",
                missing.join(", ")
            )));
        }
        let end =
            batch.start_row + u32::try_from(records.len().saturating_sub(1)).unwrap_or(u32::MAX);
        if end > batch.max_row {
            return Err(BackendError::validation(format!(
                "{} sheet'inde kopyalar icin kapasite kalmadi",
                batch.discipline
            )));
        }
        let mut revised = batch.clone();
        revised.records = records.clone();
        sources.insert(batch.discipline.clone(), source_from_data(&batch));
        revised_rows.extend(records);
        revised_batches.push(revised);
    }
    let mut revised_step = step.clone();
    insert_json(&mut revised_step.meta, "batches", &revised_batches)?;
    if revised_batches.len() == 1 {
        insert_json(
            &mut revised_step.meta,
            "records",
            &revised_batches[0].records,
        )?;
    }
    let mut meta = plan_meta(&sources, &revised_rows)?;
    let workbook = PathBuf::from(step.src.as_deref().unwrap_or_default());
    let copy_sources = revised_batches
        .iter()
        .map(|batch| {
            Ok((
                batch.discipline.clone(),
                serde_json::to_value(copy_source(&workbook, batch)?)
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
    if let Some(extraction) = plan
        .meta
        .get("editor")
        .and_then(|editor| editor.get("emailExtraction"))
    {
        editor.insert("emailExtraction".to_string(), extraction.clone());
    }
    Ok(Plan {
        plan_id: String::new(),
        op_id: "rfi.log_add".to_string(),
        steps: vec![revised_step],
        warnings: warnings_for_rows(&revised_rows, 0, true),
        meta,
    })
}
