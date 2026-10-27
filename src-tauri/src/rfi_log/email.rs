use super::*;

pub(super) fn rows_from_email(
    base_rows: &[Record],
    extraction: &EmailExtraction,
    source: &Source,
) -> (Vec<Record>, Vec<String>) {
    if extraction.rows.is_empty() {
        return (base_rows.to_vec(), Vec::new());
    }
    let mut grouped = BTreeMap::<usize, Vec<&EmailRow>>::new();
    for row in extraction.rows.iter().take(MAX_RFI_COUNT) {
        if (1..=base_rows.len()).contains(&row.rfi_index) {
            grouped.entry(row.rfi_index - 1).or_default().push(row);
        }
    }
    let mut rows = Vec::new();
    let mut notes = Vec::new();
    for (index, base) in base_rows.iter().enumerate() {
        let candidates = grouped.remove(&index).unwrap_or_default();
        if candidates.is_empty() {
            rows.push(base.clone());
            continue;
        }
        for (duplicate_index, extracted) in candidates.into_iter().enumerate() {
            let mut row = base.clone();
            if duplicate_index > 0 {
                row.row_id = format!("{}-g{}-r{}", row.sheet, index + 1, duplicate_index + 1);
            }
            apply_email_values(&mut row, extracted, source, &mut notes);
            rows.push(row);
        }
    }
    (rows, notes)
}

pub(super) fn apply_email_values(
    row: &mut Record,
    extracted: &EmailRow,
    source: &Source,
    notes: &mut Vec<String>,
) {
    let item = extracted.item_no.trim();
    let itp = extracted.itp_no.trim();
    if !item.is_empty() {
        if let Some(preset) = source.data.presets.get(item) {
            row.item_no = item.to_string();
            if let Some(resolved) = resolve_itp(itp, item, &source.data.itp_to_items)
                .or_else(|| unique_itp_for_item(item, &source.data.itp_to_items))
            {
                row.itp_no = resolved;
            }
            row.description = preset.description.clone();
            row.forms = preset.forms.clone();
            row.azfen_type = preset.azfen_type.clone();
            row.intec_type = preset.intec_type.clone();
        } else {
            notes.push(format!("Unknown Item No. ignored: {item}"));
        }
    } else if !itp.is_empty() {
        if let Some(resolved) = resolve_itp(itp, &row.item_no, &source.data.itp_to_items) {
            row.itp_no = resolved.clone();
            let allowed = &source.data.itp_to_items[&resolved];
            if !allowed.contains(&row.item_no) && allowed.len() == 1 {
                row.item_no = allowed[0].clone();
                let preset = &source.data.presets[&row.item_no];
                row.description = preset.description.clone();
                row.forms = preset.forms.clone();
                row.azfen_type = preset.azfen_type.clone();
                row.intec_type = preset.intec_type.clone();
            }
        } else {
            notes.push(format!("Unknown ITP No. ignored: {itp}"));
        }
    }
    for (target, value) in [
        (&mut row.tag_no, extracted.tag_no.trim()),
        (&mut row.area, extracted.area.trim()),
        (&mut row.location, extracted.location.trim()),
        (&mut row.subcontractor, extracted.subcontractor.trim()),
    ] {
        if !value.is_empty() {
            *target = value.to_string();
        }
    }
    if row.area.is_empty() {
        row.area = infer_area(&row.tag_no);
    }
}

pub(super) fn plan_meta(
    sources: &BTreeMap<String, Source>,
    rows: &[Record],
) -> Result<JsonObject, BackendError> {
    let mut itp_to_items = BTreeMap::<String, Vec<String>>::new();
    let mut presets = BTreeMap::<String, Preset>::new();
    let mut autocomplete = BTreeMap::from([
        ("tagNo".to_string(), Vec::new()),
        ("area".to_string(), Vec::new()),
        ("location".to_string(), Vec::new()),
        (
            "subcontractor".to_string(),
            vec!["AZFEN".to_string(), "PDL".to_string()],
        ),
    ]);
    for discipline in DISCIPLINES {
        let Some(source) = sources.get(discipline) else {
            continue;
        };
        itp_to_items.extend(source.data.itp_to_items.clone());
        presets.extend(source.data.presets.clone());
        for (field, values) in [
            ("tagNo", &source.data.tags),
            ("area", &source.data.areas),
            ("location", &source.data.locations),
            ("subcontractor", &source.data.subcontractors),
        ] {
            let target = autocomplete.get_mut(field).expect("autocomplete field");
            target.extend(values.clone());
            deduplicate(target);
        }
    }
    let rows_value = serde_json::to_value(rows)
        .map_err(|_| BackendError::protocol("RFI rows could not be encoded"))?;
    let columns = columns();
    let sheets = DISCIPLINES
        .iter()
        .filter(|discipline| sources.contains_key(**discipline))
        .copied()
        .collect::<Vec<_>>();
    let sheet_counts = sheets
        .iter()
        .map(|sheet| {
            (
                (*sheet).to_string(),
                rows.iter().filter(|row| row.sheet == *sheet).count(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(json!({
        "editor": {
            "kind": "recordTable",
            "layout": "compact",
            "titleKey": "rfi.editor.title",
            "groupField": "rfiNo",
            "selectionGroupFields": ["_sheet", "rfiNo"],
            "columns": columns,
            "rows": rows_value,
            "choices": {"itpNo": itp_to_items.keys().collect::<Vec<_>>()},
            "choicesByValue": {"itemNo": {"dependsOn": "itpNo", "values": itp_to_items}},
            "autoFill": {"field": "itemNo", "values": presets},
            "autocomplete": autocomplete,
            "regexFill": [
                {"source": "tagNo", "target": "area", "pattern": "(?:^|[^A-Z0-9])N\\d{1,3}(?:[^A-Z0-9]|$)", "value": "North Site"},
                {"source": "tagNo", "target": "area", "pattern": "(?:^|[^A-Z0-9])S\\d{1,3}(?:[^A-Z0-9]|$)", "value": "South Site"}
            ],
            "duplicateEnabled": true,
            "selectionEnabled": true,
            "bulkEnabled": true,
            "detailsEnabled": true,
            "detailsDefaultVisible": false,
            "copyEnabled": true,
            "copyFields": COPY_FIELDS,
            "copyHeaders": COPY_HEADERS,
            "sheets": sheets,
            "sheetCounts": sheet_counts,
        }
    })
    .as_object()
    .cloned()
    .expect("plan meta object"))
}

pub(super) fn columns() -> Vec<Value> {
    vec![
        json!({"key":"_sheet","labelKey":"rfi.col.sheet","kind":"readonly","width":42,"compact":true}),
        json!({"key":"rfiNo","labelKey":"rfi.col.rfiNo","kind":"readonly","width":54,"compact":true,"display":"suffix"}),
        json!({"key":"itpNo","labelKey":"rfi.col.itpNo","kind":"choice","width":50,"compact":true,"display":"shortNumber"}),
        json!({"key":"itemNo","labelKey":"rfi.col.itemNo","kind":"choice","width":62,"compact":true,"display":"itemSuffix"}),
        json!({"key":"description","labelKey":"rfi.col.description","kind":"multiline","width":360,"detail":true}),
        json!({"key":"forms","labelKey":"rfi.col.forms","kind":"multiline","width":280,"detail":true}),
        json!({"key":"tagNo","labelKey":"rfi.col.tagNo","kind":"autocomplete","width":92,"compact":true}),
        json!({"key":"area","labelKey":"rfi.col.area","kind":"autocomplete","width":94,"compact":true}),
        json!({"key":"rowNo","labelKey":"rfi.col.rowNo","kind":"text","width":70,"compact":true,"sheets":["MEC"]}),
        json!({"key":"dateOfIssue","labelKey":"rfi.col.issueDate","kind":"compactDate","width":116,"compact":true}),
        json!({"key":"inspectionDate","labelKey":"rfi.col.inspectionDate","kind":"compactDate","width":116,"compact":true}),
        json!({"key":"startTime","labelKey":"rfi.col.startTime","kind":"text","width":58,"compact":true}),
        json!({"key":"azfenType","labelKey":"rfi.col.azfenType","kind":"text","width":130,"detail":true}),
        json!({"key":"intecType","labelKey":"rfi.col.intecType","kind":"text","width":130,"detail":true}),
        json!({"key":"location","labelKey":"rfi.col.location","kind":"autocomplete","width":104,"compact":true}),
        json!({"key":"subcontractor","labelKey":"rfi.col.subcontractor","kind":"autocomplete","width":88,"compact":true}),
    ]
}

pub(super) fn copy_source(path: &Path, batch: &Batch) -> Result<CopySource, BackendError> {
    let layout = layout(&batch.discipline)
        .ok_or_else(|| BackendError::validation("gecersiz sheet plani"))?;
    let mut fields = COPY_FIELDS
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut headers = COPY_HEADERS
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut columns = vec![1, 2, 3, 4, 5, 6, 7];
    if let Some(row_col) = layout.row_col {
        fields.insert(7, "rowNo".to_string());
        headers.insert(7, "ROW No.".to_string());
        columns.push(row_col);
    }
    columns.extend([
        layout.issue_col,
        layout.inspection_col,
        layout.time_col,
        layout.azfen_col,
        layout.intec_col,
        layout.location_col,
        layout.subcontractor_col,
    ]);
    Ok(CopySource {
        workbook: path.to_string_lossy().into_owned(),
        sheet: layout.sheet.to_string(),
        header_row: 9,
        start_row: batch.start_row,
        rfi_numbers: batch
            .records
            .iter()
            .map(|record| record.rfi_no.clone())
            .collect(),
        fields,
        headers,
        columns,
    })
}

pub(super) type CopyData = (
    Vec<String>,
    Vec<String>,
    Vec<BTreeMap<String, String>>,
    bool,
    Vec<f64>,
);

pub(super) fn copy_data(
    plan: &Plan,
    selected_sheet: Option<&str>,
) -> Result<CopyData, BackendError> {
    let editor = plan
        .meta
        .get("editor")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("bu planda kopyalanabilir tablo yok"))?;
    if editor.get("kind").and_then(Value::as_str) != Some("recordTable")
        || editor.get("copyEnabled").and_then(Value::as_bool) != Some(true)
    {
        return Err(BackendError::validation(
            "bu planda kopyalanabilir tablo yok",
        ));
    }
    let source_value = select_copy_source(editor, selected_sheet)?;
    let source: CopySource = serde_json::from_value(source_value)
        .map_err(|_| BackendError::validation("mail tablosu kaynagi gecersiz"))?;
    if source.fields.len() != source.headers.len() || source.fields.len() != source.columns.len() {
        return Err(BackendError::validation("mail tablosu kolonlari gecersiz"));
    }
    let planned = editor
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|row| row.get("_sheet").and_then(Value::as_str) == Some(source.sheet.as_str()))
        .filter_map(|row| serde_json::from_value::<Record>(row.clone()).ok())
        .map(record_map)
        .collect::<Vec<_>>();
    let fallback_widths = source
        .fields
        .iter()
        .map(|field| field_width(field))
        .collect::<Vec<_>>();
    let Ok(workbook) = read_workbook_selective(Path::new(&source.workbook), &REQUIRED_SHEETS)
    else {
        return Ok((
            source.fields,
            source.headers,
            planned,
            false,
            fallback_widths,
        ));
    };
    let Some(sheet) = workbook.sheet(&source.sheet) else {
        return Ok((
            source.fields,
            source.headers,
            planned,
            false,
            fallback_widths,
        ));
    };
    let workbook_headers = source
        .columns
        .iter()
        .map(|column| sheet.value(source.header_row, *column).to_string())
        .collect::<Vec<_>>();
    let headers = if workbook_headers.iter().all(|header| !header.is_empty()) {
        workbook_headers
    } else {
        source.headers.clone()
    };
    let mut remaining =
        source
            .rfi_numbers
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, rfi| {
                *counts.entry(rfi.clone()).or_default() += 1;
                counts
            });
    let mut planned_by_rfi = planned.iter().cloned().fold(
        BTreeMap::<String, VecDeque<BTreeMap<String, String>>>::new(),
        |mut grouped, row| {
            grouped
                .entry(row.get("rfiNo").cloned().unwrap_or_default())
                .or_default()
                .push_back(row);
            grouped
        },
    );
    let mut rows = Vec::new();
    for row_number in source.header_row + 1..=sheet.max_row {
        let rfi = sheet.value(row_number, source.columns[0]).to_string();
        let Some(count) = remaining.get_mut(&rfi) else {
            continue;
        };
        if *count == 0 {
            continue;
        }
        let planned_row = planned_by_rfi
            .get_mut(&rfi)
            .and_then(VecDeque::pop_front)
            .unwrap_or_default();
        let mut actual = BTreeMap::new();
        for (field, column) in source.fields.iter().zip(&source.columns) {
            let value = copy_cell_text(sheet, row_number, *column, field);
            actual.insert(
                field.clone(),
                if value.is_empty() {
                    planned_row.get(field).cloned().unwrap_or_default()
                } else {
                    value
                },
            );
        }
        rows.push(actual);
        *count -= 1;
    }
    if remaining.values().any(|count| *count > 0) || rows.len() != source.rfi_numbers.len() {
        return Ok((
            source.fields,
            source.headers,
            planned,
            false,
            fallback_widths,
        ));
    }
    let widths = source
        .columns
        .iter()
        .zip(&fallback_widths)
        .map(|(column, fallback)| {
            sheet
                .column_widths
                .get(column)
                .copied()
                .unwrap_or(*fallback)
        })
        .collect();
    Ok((source.fields, headers, rows, true, widths))
}
