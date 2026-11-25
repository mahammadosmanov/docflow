//! Documents generated from shared templates (ADR 0055).
//!
//! Preview reads each requested RFI from the RFI Log, resolves every field's
//! starting value, and shows one editable row per RFI. Revise renders the
//! edited rows again from the same cached template version. Apply writes new
//! files only, through the journaled generated-write path.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek},
};

use serde_json::{Map, Value, json};

use crate::{
    concrete_documents::GeneratedFile,
    ooxml::{WorksheetData, read_workbook_selective_from_reader},
    templates::{
        FileKind,
        field_map::{FieldMap, fill_text, render_package, resolve_tokens},
        sheets,
        values::{FieldKind, convert, truthy},
    },
};

pub const OPERATION_ID: &str = "templates.generate_document";
pub const STEP_NOTE: &str = "step.templates.generatedDocument";
pub const MAX_RFIS: usize = 100;
const MAX_VALUE_CHARS: usize = 4_000;
const PRESET_COLUMN: &str = "_preset";
const ROW_ID: &str = "_rowId";
const SHEET_PREFIX: &str = "_sheet";
const FREE_PREFIX: &str = "document-";

/// The row key of the checkbox for the `index`-th optional sheet.
fn sheet_key(index: usize) -> String {
    format!("{SHEET_PREFIX}{index}")
}

/// RFI Log headers (normalized) for each token, in preference order.
const LOG_COLUMNS: [(&str, &[&str]); 16] = [
    ("rfiNo", &["rfino"]),
    ("itpNo", &["itpno"]),
    ("itemNo", &["itemno"]),
    ("description", &["inspectiondescription", "description"]),
    ("forms", &["forms", "form"]),
    ("tagNo", &["tagno", "tag"]),
    ("area", &["area"]),
    ("dateOfIssue", &["dateofissue"]),
    ("inspectionDate", &["inspectiondate"]),
    ("expiryDate", &["expirydate"]),
    ("startTime", &["inspectionstarttime", "starttime"]),
    ("azfenType", &["insptypeazfen"]),
    ("intecType", &["insptypeintec"]),
    (
        "location",
        &["insplocarea", "inspectionlocation", "location"],
    ),
    ("subcontractor", &["subcontractor"]),
    ("remarks", &["remarks"]),
];
const DATE_TOKENS: [&str; 3] = ["dateOfIssue", "inspectionDate", "expiryDate"];

pub type LogRow = BTreeMap<String, String>;

/// The four RFI disciplines; a template of one of them lets users type
/// only the number (`49` means `AZE-SUN-INT-QMS-RFI-ELE-0049`).
pub const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];

/// One RFI entry: a full code, `ELE-49` / `ELE 49`, or just `49` when the
/// template has an RFI discipline.
fn parse_rfi(item: &str, discipline: Option<&str>) -> Option<String> {
    let upper = item.trim().to_ascii_uppercase();
    let (prefix, digits) = match upper.rsplit_once(['-', ' ']) {
        Some((prefix, digits)) if DISCIPLINES.contains(&prefix.trim()) => {
            (Some(prefix.trim().to_string()), digits.trim())
        }
        _ => (None, upper.as_str()),
    };
    if let Some(number) = crate::rfi_code::normalize_candidate_number(digits) {
        let discipline = prefix.or_else(|| discipline.map(str::to_string))?;
        return Some(format!("{}-{discipline}-{number}", crate::rfi_code::PREFIX));
    }
    let codes = crate::rfi_code::exact_codes(item);
    (codes.len() == 1)
        .then(|| codes.into_iter().next())
        .flatten()
}

/// Canonical RFI numbers from free text (commas, semicolons, or lines).
pub fn requested_rfis(raw: &str, discipline: Option<&str>) -> Result<Vec<String>, String> {
    let discipline = discipline.filter(|value| DISCIPLINES.contains(value));
    let mut seen = BTreeSet::new();
    let mut values = Vec::new();
    for item in raw.split([',', ';', '\n', '\r', '\t']) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let code =
            parse_rfi(item, discipline).ok_or_else(|| format!("Invalid RFI number: {item}"))?;
        if seen.insert(code.clone()) {
            values.push(code);
        }
    }
    if values.is_empty() {
        return Err("Enter at least one RFI number".to_string());
    }
    if values.len() > MAX_RFIS {
        return Err(format!("Enter at most {MAX_RFIS} RFI numbers at once"));
    }
    Ok(values)
}

fn discipline_of(rfi: &str) -> &str {
    rfi.rsplit('-').nth(1).unwrap_or_default()
}

fn normalize_header(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// Dashes that look like `-` in the RFI Log are written as `-`.
fn plain_dashes(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\u{02D7}' | '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            other => other,
        })
        .collect()
}

fn header_columns(sheet: &WorksheetData) -> Option<(u32, BTreeMap<&'static str, u32>)> {
    for row in 1..=sheet.max_row.min(32) {
        let headers = (1..=sheet.max_column)
            .map(|column| (normalize_header(sheet.value(row, column)), column))
            .filter(|(name, _)| !name.is_empty())
            .collect::<BTreeMap<_, _>>();
        if !headers.contains_key("rfino") {
            continue;
        }
        let columns = LOG_COLUMNS
            .iter()
            .filter_map(|(token, names)| {
                names
                    .iter()
                    .find_map(|name| headers.get(*name))
                    .map(|column| (*token, *column))
            })
            .collect();
        return Some((row, columns));
    }
    None
}

/// Reads the requested RFIs from their discipline sheets of the RFI Log.
/// The first row of each RFI wins; missing RFIs are simply absent.
pub fn read_log_rows<R: Read + Seek>(
    reader: &mut R,
    requested: &[String],
) -> Result<BTreeMap<String, LogRow>, String> {
    let disciplines = requested
        .iter()
        .map(|rfi| discipline_of(rfi).to_string())
        .collect::<BTreeSet<_>>();
    let wanted = disciplines.iter().map(String::as_str).collect::<Vec<_>>();
    let workbook = read_workbook_selective_from_reader(reader, &wanted)
        .map_err(|error| format!("Cannot read the RFI Log workbook: {error}"))?;
    let today = jiff::Zoned::now().strftime("%d.%m.%Y").to_string();
    let mut rows = BTreeMap::new();
    for discipline in &disciplines {
        let Some(sheet) = workbook
            .sheets
            .iter()
            .find(|(name, _)| name.trim().eq_ignore_ascii_case(discipline))
            .map(|(_, sheet)| sheet)
        else {
            return Err(format!("The RFI Log has no {discipline} sheet"));
        };
        let (header, columns) = header_columns(sheet)
            .ok_or_else(|| format!("The {discipline} sheet has no RFI No. header"))?;
        let rfi_column = columns["rfiNo"];
        for row in header + 1..=sheet.max_row {
            let codes = crate::rfi_code::exact_codes(sheet.value(row, rfi_column));
            let Some(rfi) = requested
                .iter()
                .find(|rfi| codes.contains(*rfi) && !rows.contains_key(*rfi))
            else {
                continue;
            };
            let mut values = LogRow::new();
            for (token, column) in &columns {
                let raw = sheet.value(row, *column).trim();
                let value = if DATE_TOKENS.contains(token) {
                    crate::native::excel_date_text(raw)
                } else if *token == "startTime" {
                    crate::native::excel_time_text(raw)
                } else {
                    raw.to_string()
                };
                values.insert((*token).to_string(), plain_dashes(&value));
            }
            values.insert("rfiNo".to_string(), rfi.clone());
            values.insert("today".to_string(), today.clone());
            rows.insert(rfi.clone(), values);
        }
    }
    Ok(rows)
}

/// One editable row with every field's starting value.
fn start_row(map: &FieldMap, id: &str, source: &LogRow) -> Value {
    let mut row = Map::new();
    row.insert(ROW_ID.to_string(), json!(id));
    if !map.presets.is_empty() {
        row.insert(PRESET_COLUMN.to_string(), json!(""));
    }
    for field in &map.fields {
        let start = resolve_tokens(&field.value, source);
        let value = if field.kind == FieldKind::Checkbox {
            json!(truthy(&start))
        } else {
            json!(start)
        };
        row.insert(field.key.clone(), value);
    }
    for index in 0..map.optional_sheets.len() {
        row.insert(sheet_key(index), json!(true));
    }
    Value::Object(row)
}

/// One editable row per RFI with every field's starting value.
pub fn initial_rows(map: &FieldMap, log: &BTreeMap<String, LogRow>, rfis: &[String]) -> Vec<Value> {
    let rows = rfis
        .iter()
        .filter_map(|rfi| Some(start_row(map, rfi, log.get(rfi)?)))
        .collect();
    with_computed(map, rows)
}

/// Free mode: `count` numbered documents with no RFI. Every RFI Log value is
/// empty (only `{today}` is known), so a field starts from its fixed text.
pub fn free_rows(map: &FieldMap, count: usize) -> Vec<Value> {
    let mut source = LogRow::new();
    for token in crate::templates::field_map::LOG_TOKENS {
        source.insert(token.to_string(), String::new());
    }
    source.insert(
        "today".to_string(),
        jiff::Zoned::now().strftime("%d.%m.%Y").to_string(),
    );
    let rows = (1..=count.min(MAX_RFIS))
        .map(|number| start_row(map, &format!("{FREE_PREFIX}{number}"), &source))
        .collect();
    with_computed(map, rows)
}

/// The editable table shown in Preview. Choosing a preset fills its fields.
pub fn editor(map: &FieldMap, rows: Vec<Value>) -> Value {
    let mut columns = Vec::new();
    if !map.presets.is_empty() {
        columns.push(json!({"key": PRESET_COLUMN, "labelKey": "templates.generate.preset", "kind": "choice"}));
    }
    for field in &map.fields {
        let label = if field.label.trim().is_empty() {
            &field.key
        } else {
            &field.label
        };
        let repeats = field
            .anchor
            .as_ref()
            .is_some_and(|anchor| anchor.repeat_columns.is_some());
        let (kind, width) = if field.kind == FieldKind::Computed {
            ("readonly", 120)
        } else if field.kind == FieldKind::Checkbox {
            ("bool", 90)
        } else if field.multiline || repeats {
            ("multiline", 260)
        } else {
            ("text", 180)
        };
        columns.push(json!({"key": field.key, "labelKey": label, "kind": kind, "width": width}));
    }
    for (index, name) in map.optional_sheets.iter().enumerate() {
        columns
            .push(json!({"key": sheet_key(index), "labelKey": name, "kind": "bool", "width": 90}));
    }
    let autocomplete = map
        .fields
        .iter()
        .filter(|field| !field.options.is_empty())
        .map(|field| (field.key.clone(), json!(field.options)))
        .collect::<Map<_, _>>();
    let preset_names = std::iter::once(String::new())
        .chain(map.presets.iter().map(|preset| preset.name.clone()))
        .collect::<Vec<_>>();
    let preset_values = map
        .presets
        .iter()
        .map(|preset| (preset.name.clone(), json!(preset.values)))
        .collect::<Map<_, _>>();
    json!({
        "kind": "recordTable",
        "layout": "compact",
        "titleKey": "templates.generate.editor.title",
        "rowCountKey": "templates.generate.editor.rowCount",
        "groupField": ROW_ID,
        "requiredFields": [],
        "columns": columns,
        "rows": rows,
        "choices": {PRESET_COLUMN: preset_names},
        "autoFill": {"field": PRESET_COLUMN, "values": preset_values},
        "autocomplete": autocomplete,
        "duplicateEnabled": false,
        "selectionEnabled": false,
        "bulkEnabled": false,
        "reorderable": false,
        "detailsEnabled": false,
        "copyEnabled": false,
    })
}

/// Checks submitted rows against the previewed ones: same RFIs, same keys,
/// string values only. Returns the accepted rows.
pub fn accept_rows(original: &[Value], submitted: &[Value]) -> Result<Vec<Value>, String> {
    let invalid = || "The edited document rows are invalid".to_string();
    if original.len() != submitted.len() {
        return Err(invalid());
    }
    let mut accepted = Vec::with_capacity(submitted.len());
    for (before, after) in original.iter().zip(submitted) {
        let (Some(before), Some(after)) = (before.as_object(), after.as_object()) else {
            return Err(invalid());
        };
        if before.get(ROW_ID) != after.get(ROW_ID)
            || before.keys().collect::<BTreeSet<_>>() != after.keys().collect::<BTreeSet<_>>()
            || after.iter().any(|(key, value)| {
                // A value keeps the kind it was previewed with: a checkbox
                // stays a boolean, everything else stays text.
                if before.get(key).is_some_and(Value::is_boolean) {
                    !value.is_boolean()
                } else {
                    value
                        .as_str()
                        .is_none_or(|text| text.chars().count() > MAX_VALUE_CHARS)
                }
            })
        {
            return Err(invalid());
        }
        accepted.push(Value::Object(after.clone()));
    }
    Ok(accepted)
}

/// The text of every field of a row: typed values converted, computed values
/// made from them. A submitted computed value is never used.
fn row_values(map: &FieldMap, row: &Value) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    for (key, value) in row
        .as_object()
        .ok_or_else(|| "The document row is invalid".to_string())?
        .iter()
        .filter(|(key, _)| !key.starts_with('_'))
    {
        let raw = match value {
            Value::Bool(flag) => flag.to_string(),
            other => other.as_str().unwrap_or_default().to_string(),
        };
        let text = match map.fields.iter().find(|field| field.key == *key) {
            Some(field) if field.kind == FieldKind::Computed => continue,
            Some(field) => convert(field, &raw)?,
            None => raw,
        };
        values.insert(key.clone(), text);
    }
    for field in &map.fields {
        if let Some(compute) = &field.compute {
            let label = if field.label.trim().is_empty() {
                &field.key
            } else {
                &field.label
            };
            let text = compute.evaluate(label, &values)?;
            values.insert(field.key.clone(), text);
        }
    }
    Ok(values)
}

/// Puts each computed field's current value into the rows shown in Preview.
/// A row whose values cannot be computed shows an empty value; the error is
/// reported when the document is rendered.
pub fn with_computed(map: &FieldMap, rows: Vec<Value>) -> Vec<Value> {
    if map.fields.iter().all(|field| field.compute.is_none()) {
        return rows;
    }
    rows.into_iter()
        .map(|mut row| {
            let values = row_values(map, &row).ok();
            for field in map.fields.iter().filter(|field| field.compute.is_some()) {
                let text = values
                    .as_ref()
                    .and_then(|values| values.get(&field.key))
                    .cloned()
                    .unwrap_or_default();
                row[field.key.as_str()] = json!(text);
            }
            row
        })
        .collect()
}

/// Renders one row into a new document.
pub(crate) fn render(
    kind: FileKind,
    template: &[u8],
    map: &FieldMap,
    row: &Value,
) -> Result<(GeneratedFile, BTreeSet<String>), String> {
    let values = row_values(map, row)?;
    let (bytes, missing) = render_package(kind, template, map, &values)?;
    let left_out = map
        .optional_sheets
        .iter()
        .enumerate()
        .filter(|(index, _)| row.get(sheet_key(*index)).and_then(Value::as_bool) == Some(false))
        .map(|(_, name)| name.clone())
        .collect::<Vec<_>>();
    let bytes = if left_out.is_empty() {
        bytes
    } else {
        sheets::remove_sheets(&bytes, &left_out)?
    };
    let row_id = row[ROW_ID].as_str().unwrap_or("document");
    let stem = safe_file_stem(&fill_text(&map.file_name, &values));
    // A free-mode document whose name does not yet depend on anything the
    // operator typed gets its number, so Preview never shows two equal names.
    let blank = safe_file_stem(&fill_text(&map.file_name, &BTreeMap::new()));
    let stem = if stem.is_empty() {
        safe_file_stem(row_id)
    } else if row_id.starts_with(FREE_PREFIX) && stem == blank {
        safe_file_stem(&format!("{blank} {row_id}"))
    } else {
        stem
    };
    Ok((
        GeneratedFile {
            filename: format!("{stem}.{}", kind.extension()),
            bytes,
        },
        missing,
    ))
}

/// A Windows-safe file name stem.
fn safe_file_stem(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    trimmed.chars().take(150).collect()
}

#[cfg(test)]
mod tests;
