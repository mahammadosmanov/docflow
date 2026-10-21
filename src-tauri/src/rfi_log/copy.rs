use super::*;

pub(super) fn select_copy_source(
    editor: &Map<String, Value>,
    selected: Option<&str>,
) -> Result<Value, BackendError> {
    if let Some(sources) = editor.get("copySources").and_then(Value::as_object) {
        let selected = selected.unwrap_or_default().to_ascii_uppercase();
        let selected = if selected.is_empty() {
            if sources.len() != 1 {
                return Err(BackendError::validation("kopyalanacak sheet secilmelidir"));
            }
            sources.keys().next().cloned().unwrap_or_default()
        } else {
            selected
        };
        return sources
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(&selected))
            .map(|(_, value)| value.clone())
            .ok_or_else(|| {
                BackendError::validation(format!("bu planda {selected} kopyalama kaynagi yok"))
            });
    }
    editor
        .get("copySource")
        .cloned()
        .ok_or_else(|| BackendError::validation("mail tablosu kaynagi gecersiz"))
}

pub(super) fn validate_record(
    record: &mut Record,
    index: usize,
    batch: &Batch,
) -> Result<(), BackendError> {
    if record.sheet != batch.discipline {
        return Err(BackendError::validation(format!(
            "RFI satiri yanlis sheet'e ait: {}",
            batch.discipline
        )));
    }
    if !batch.base_rfis.contains(&record.rfi_no) {
        return Err(BackendError::validation(format!(
            "plan disi RFI numarasi: {}",
            record.rfi_no
        )));
    }
    parse_date(&record.date_of_issue)?;
    parse_date(&record.inspection_date)?;
    parse_time(&record.start_time)?;
    if !batch.source_data.presets.contains_key(&record.item_no) {
        return Err(BackendError::validation(format!(
            "bilinmeyen Item No.: {}",
            record.item_no
        )));
    }
    let resolved = resolve_itp(
        &record.itp_no,
        &record.item_no,
        &batch.source_data.itp_to_items,
    )
    .ok_or_else(|| BackendError::validation(format!("bilinmeyen ITP No.: {}", record.itp_no)))?;
    record.itp_no = resolved.clone();
    if !batch.source_data.itp_to_items[&resolved].contains(&record.item_no) {
        return Err(BackendError::validation(format!(
            "Item No. secilen ITP'ye ait degil: {} / {}",
            resolved, record.item_no
        )));
    }
    for (name, value) in [
        ("description", &record.description),
        ("forms", &record.forms),
        ("azfenType", &record.azfen_type),
        ("intecType", &record.intec_type),
        ("subcontractor", &record.subcontractor),
    ] {
        if value.is_empty() {
            return Err(BackendError::validation(format!(
                "zorunlu alan bos: {name} (satir {})",
                index + 1
            )));
        }
    }
    Ok(())
}

pub(super) fn clean_record(record: &mut Record) {
    for value in [
        &mut record.row_id,
        &mut record.sheet,
        &mut record.rfi_no,
        &mut record.itp_no,
        &mut record.item_no,
        &mut record.description,
        &mut record.forms,
        &mut record.tag_no,
        &mut record.area,
        &mut record.row_no,
        &mut record.date_of_issue,
        &mut record.inspection_date,
        &mut record.start_time,
        &mut record.azfen_type,
        &mut record.intec_type,
        &mut record.location,
        &mut record.subcontractor,
    ] {
        *value = truncate_chars(&clean_text(value), MAX_ROW_TEXT_CHARS);
    }
    if record.row_id.is_empty() {
        record.row_id = format!("row-{}", &Uuid::new_v4().simple().to_string()[..10]);
    }
    record.sheet = record.sheet.to_ascii_uppercase();
    if layout(&record.sheet).is_some_and(|layout| layout.row_col.is_none()) {
        record.row_no.clear();
    }
    if record.area.is_empty() {
        record.area = infer_area(&record.tag_no);
    }
}

pub(super) fn warnings_for_rows(rows: &[Record], overflowed: usize, wrap: bool) -> Vec<Warning> {
    let mut warnings = Vec::new();
    if overflowed > 0 {
        warnings.push(Warning {
            key: if wrap {
                "warn.rfi.overflowMoved"
            } else {
                "warn.rfi.overflowKept"
            }
            .to_string(),
            subject: overflowed.to_string(),
        });
    }
    let mut grouped = BTreeMap::<String, BTreeSet<(String, String)>>::new();
    for row in rows {
        grouped
            .entry(row.rfi_no.clone())
            .or_default()
            .insert((row.inspection_date.clone(), row.start_time.clone()));
    }
    for (rfi, schedules) in grouped {
        if schedules.len() > 1 {
            warnings.push(Warning {
                key: "warn.rfi.groupScheduleMismatch".to_string(),
                subject: rfi,
            });
        }
    }
    warnings
}

pub(super) fn extract_with_gemini(
    values: &Map<String, Value>,
    email: &str,
    discipline: &str,
    source: &SourceData,
    gemini: Option<&GeminiClient>,
) -> Result<EmailExtraction, BackendError> {
    let key = value_string(values, "gemini_api_key");
    let key = if key.trim().is_empty() {
        env::var("GEMINI_API_KEY").unwrap_or_default()
    } else {
        key
    };
    let client = gemini.ok_or_else(|| BackendError::request_failed("Gemini is unavailable"))?;
    let value = client.extract(&GeminiRequest::new(
        email,
        key,
        discipline,
        source.itp_to_items.clone(),
    ))?;
    validate_gemini_extraction(value)
}

pub(super) fn safe_error(error: &BackendError) -> String {
    let safe = match error.code {
        "validation" => format!("ValueError: {}", error.message),
        _ => format!("RuntimeError: {}", error.message),
    };
    truncate_chars(&safe.replace(['\r', '\n'], " "), 300)
}
