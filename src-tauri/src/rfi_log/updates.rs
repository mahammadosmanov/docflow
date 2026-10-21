use super::*;

pub(crate) fn workbook_step_updates(step: &Step) -> Result<WorkbookUpdates, BackendError> {
    let batches = decode_batches(step)?;
    let mut sheets = BTreeMap::new();
    for batch in batches {
        let layout = layout(&batch.discipline)
            .ok_or_else(|| BackendError::validation("gecersiz sheet plani"))?;
        let mut updates = BTreeMap::new();
        for (index, record) in batch.records.iter().enumerate() {
            let row = batch.start_row + u32::try_from(index).unwrap_or(u32::MAX);
            let values = [
                (1, CellUpdate::Text(record.rfi_no.clone())),
                (2, CellUpdate::Text(record.itp_no.clone())),
                (3, CellUpdate::Text(record.item_no.clone())),
                (6, CellUpdate::Text(record.tag_no.clone())),
                (7, CellUpdate::Text(record.area.clone())),
                (
                    layout.issue_col,
                    CellUpdate::Number(excel_date(parse_date(&record.date_of_issue)?).to_string()),
                ),
                (
                    layout.inspection_col,
                    CellUpdate::Number(
                        excel_date(parse_date(&record.inspection_date)?).to_string(),
                    ),
                ),
                (
                    layout.time_col,
                    CellUpdate::Number(excel_time(parse_time(&record.start_time)?)),
                ),
                (
                    layout.location_col,
                    CellUpdate::Text(record.location.clone()),
                ),
                (
                    layout.subcontractor_col,
                    CellUpdate::Text(record.subcontractor.clone()),
                ),
            ];
            updates.extend(
                values
                    .into_iter()
                    .map(|(column, update)| ((row, column), update)),
            );
            if let Some(column) = layout.row_col {
                updates.insert((row, column), CellUpdate::Text(record.row_no.clone()));
            }
            let preset = batch.presets.get(&record.item_no).ok_or_else(|| {
                BackendError::validation(format!("bilinmeyen Item No.: {}", record.item_no))
            })?;
            let lookup = &batch.lookup_columns;
            updates.insert(
                (row, 4),
                formula_or_value(
                    &record.description,
                    &preset.description,
                    format!(
                        "IFERROR(VLOOKUP(C{row},Sheet1!{}:{},2,0),\"\")",
                        column_name(lookup["item"]),
                        column_name(lookup["description"])
                    ),
                ),
            );
            updates.insert(
                (row, 5),
                formula_or_value(
                    &record.forms,
                    &preset.forms,
                    format!(
                        "IFERROR(VLOOKUP(D{row},Sheet1!{}:{},2,0),\"\")",
                        column_name(lookup["description"]),
                        column_name(lookup["forms"])
                    ),
                ),
            );
            updates.insert(
                (row, layout.azfen_col),
                formula_or_value(
                    &record.azfen_type,
                    &preset.azfen_type,
                    format!(
                        "IFERROR(VLOOKUP(C{row},Sheet1!{}:{},2,0),\"\")",
                        column_name(lookup["azfenItem"]),
                        column_name(lookup["azfenType"])
                    ),
                ),
            );
            updates.insert(
                (row, layout.intec_col),
                formula_or_value(
                    &record.intec_type,
                    &preset.intec_type,
                    format!(
                        "IFERROR(VLOOKUP(C{row},Sheet1!{}:{},2,0),\"\")",
                        column_name(lookup["intecItem"]),
                        column_name(lookup["intecType"])
                    ),
                ),
            );
        }
        sheets.insert(layout.sheet.to_string(), updates);
    }
    Ok(sheets)
}

pub(crate) fn workbook_record_count(step: &Step) -> Result<u64, BackendError> {
    let count = decode_batches(step)?
        .iter()
        .map(|batch| batch.records.len())
        .sum::<usize>();
    Ok(u64::try_from(count).unwrap_or(u64::MAX))
}

pub(crate) fn verify_workbook_step(step: &Step, path: &Path) -> Result<(), BackendError> {
    let workbook = read_workbook_selective(path, &REQUIRED_SHEETS).map_err(document_error)?;
    for batch in decode_batches(step)? {
        let layout = layout(&batch.discipline)
            .ok_or_else(|| BackendError::validation("gecersiz sheet plani"))?;
        let sheet = workbook
            .sheet(layout.sheet)
            .ok_or_else(|| BackendError::validation("yazma dogrulamasi basarisiz"))?;
        for (index, record) in batch.records.iter().enumerate() {
            let row = batch.start_row + u32::try_from(index).unwrap_or(u32::MAX);
            let checks = [
                (1, &record.rfi_no),
                (2, &record.itp_no),
                (3, &record.item_no),
                (6, &record.tag_no),
                (7, &record.area),
                (layout.location_col, &record.location),
                (layout.subcontractor_col, &record.subcontractor),
            ];
            for (column, expected) in checks {
                if sheet.value(row, column) != expected {
                    return Err(BackendError::request_failed(format!(
                        "yazma dogrulamasi basarisiz: {}!{}{}",
                        layout.sheet,
                        column_name(column),
                        row
                    )));
                }
            }
            if layout.expiry_col == layout.issue_col
                || layout.status_col == layout.inspection_col
                || layout.validity_col == layout.time_col
            {
                return Err(BackendError::protocol("gecersiz kolon eslemesi"));
            }
        }
    }
    Ok(())
}

pub(super) fn workbook_path(values: &Map<String, Value>) -> Result<PathBuf, BackendError> {
    let raw = values
        .get("workbook")
        .and_then(Value::as_str)
        .ok_or_else(|| BackendError::validation(WORKBOOK_SELECTION_ERROR))?;
    let path = PathBuf::from(raw);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "xlsx" | "xlsm") || !path.is_file() {
        return Err(BackendError::validation(WORKBOOK_SELECTION_ERROR));
    }
    fs::canonicalize(path)
        .map_err(|_| BackendError::path_invalid("workbook path cannot be canonicalized"))
}

pub(super) fn requested_sheet_counts(
    values: &Map<String, Value>,
) -> Result<BTreeMap<String, usize>, BackendError> {
    let mut counts = BTreeMap::new();
    if let Some(raw) = values.get("sheet_counts").and_then(Value::as_object) {
        for discipline in DISCIPLINES {
            let count = match raw.get(discipline) {
                None | Some(Value::Null) => 0,
                Some(value) => protocol_integer(value).ok_or_else(|| {
                    BackendError::validation(format!("{discipline} RFI adedi gecersiz"))
                })?,
            };
            if !(0..=MAX_RFI_COUNT as i64).contains(&count) {
                return Err(BackendError::validation(format!(
                    "{discipline} RFI adedi 0-{MAX_RFI_COUNT} arasinda olmali"
                )));
            }
            if count > 0 {
                counts.insert(discipline.to_string(), count as usize);
            }
        }
    } else {
        let discipline = value_string(values, "discipline").to_ascii_uppercase();
        let discipline = if discipline.is_empty() {
            "CIV"
        } else {
            &discipline
        };
        let count = match values.get("count") {
            None | Some(Value::Null) => 0,
            Some(value) => protocol_integer(value).ok_or_else(|| {
                BackendError::validation(format!("{discipline} RFI adedi gecersiz"))
            })?,
        };
        if !(0..=MAX_RFI_COUNT as i64).contains(&count) {
            return Err(BackendError::validation(format!(
                "{discipline} RFI adedi 0-{MAX_RFI_COUNT} arasinda olmali"
            )));
        }
        if is_discipline(discipline) && count > 0 {
            counts.insert(discipline.to_string(), count as usize);
        }
    }
    if counts.is_empty() {
        return Err(BackendError::validation(
            "en az bir sheet icin RFI adedi secin",
        ));
    }
    Ok(counts)
}

pub(super) fn protocol_integer(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(value) => Some(i64::from(*value)),
        Value::Number(value) => value.as_i64().or_else(|| {
            let value = value.as_f64()?;
            (value.is_finite() && value >= i64::MIN as f64 && value <= i64::MAX as f64)
                .then_some(value.trunc() as i64)
        }),
        Value::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}
