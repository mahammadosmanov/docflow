use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use chrono::{Days, Local, NaiveDate, NaiveTime, Timelike};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    email_extract::{
        EmailContext, EmailExtraction, EmailRow, MAX_RFI_COUNT, detect_discipline, offline_extract,
        validate_gemini_extraction,
    },
    errors::BackendError,
    fs_ops::file_sha256,
    gemini::{GeminiClient, GeminiRequest},
    ooxml::{CellUpdate, WorkbookData, WorksheetData, read_workbook},
};

const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
const DAY_START_MINUTES: u32 = 9 * 60;
const DAY_END_MINUTES: u32 = 17 * 60 + 30;
const SLOT_MINUTES: u32 = 30;
const MAX_PHYSICAL_ROWS: usize = 500;
const MAX_ROW_TEXT_CHARS: usize = 20_000;
const MAX_WARNING_CHARS: usize = 500;
const MAX_TABLE_BYTES: usize = 8 * 1024 * 1024;
const WORKBOOK_SELECTION_ERROR: &str = "error.rfi.workbookInvalid";

type WorkbookUpdates = BTreeMap<String, BTreeMap<(u32, u32), CellUpdate>>;

const COPY_FIELDS: [&str; 14] = [
    "rfiNo",
    "itpNo",
    "itemNo",
    "description",
    "forms",
    "tagNo",
    "area",
    "dateOfIssue",
    "inspectionDate",
    "startTime",
    "azfenType",
    "intecType",
    "location",
    "subcontractor",
];

const COPY_HEADERS: [&str; 14] = [
    "RFI No.",
    "ITP No.",
    "Item No.",
    "INSPECTION DESCRIPTION",
    "FORMS",
    "TAG NO",
    "AREA",
    "DATE OF ISSUE",
    "INSPECTION DATE",
    "INSPECTION START TIME",
    "INSP. TYPE AZFEN",
    "INSP. TYPE INTEC",
    "Inspection Location",
    "SUBCONTRACTOR",
];

#[derive(Clone, Copy)]
struct Layout {
    sheet: &'static str,
    issue_col: u32,
    inspection_col: u32,
    expiry_col: u32,
    time_col: u32,
    azfen_col: u32,
    intec_col: u32,
    location_col: u32,
    subcontractor_col: u32,
    status_col: u32,
    validity_col: u32,
    row_col: Option<u32>,
}

fn layout(discipline: &str) -> Option<Layout> {
    match discipline {
        "CIV" => Some(Layout {
            sheet: "CIV",
            issue_col: 8,
            inspection_col: 9,
            expiry_col: 10,
            time_col: 11,
            azfen_col: 12,
            intec_col: 13,
            location_col: 14,
            subcontractor_col: 15,
            status_col: 16,
            validity_col: 17,
            row_col: None,
        }),
        "MEC" => Some(Layout {
            sheet: "MEC",
            issue_col: 9,
            inspection_col: 10,
            expiry_col: 11,
            time_col: 12,
            azfen_col: 13,
            intec_col: 14,
            location_col: 15,
            subcontractor_col: 16,
            status_col: 17,
            validity_col: 18,
            row_col: Some(8),
        }),
        "ELE" => Some(Layout {
            sheet: "ELE",
            issue_col: 8,
            inspection_col: 9,
            expiry_col: 10,
            time_col: 11,
            azfen_col: 12,
            intec_col: 13,
            location_col: 14,
            subcontractor_col: 15,
            status_col: 16,
            validity_col: 17,
            row_col: None,
        }),
        "PRC" => Some(Layout {
            sheet: "PRC",
            issue_col: 8,
            inspection_col: 9,
            expiry_col: 10,
            time_col: 11,
            azfen_col: 12,
            intec_col: 13,
            location_col: 14,
            subcontractor_col: 15,
            status_col: 16,
            validity_col: 17,
            row_col: None,
        }),
        _ => None,
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Preset {
    description: String,
    forms: String,
    azfen_type: String,
    intec_type: String,
}

#[derive(Clone, Debug)]
struct Source {
    data: SourceData,
    default_item: String,
    default_itp: String,
    max_number: u32,
    last_row: u32,
    occupied: BTreeMap<NaiveDate, BTreeMap<String, NaiveTime>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceData {
    presets: BTreeMap<String, Preset>,
    itp_to_items: BTreeMap<String, Vec<String>>,
    lookup_columns: BTreeMap<String, u32>,
    tags: Vec<String>,
    areas: Vec<String>,
    locations: Vec<String>,
    subcontractors: Vec<String>,
    max_row: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
struct Record {
    #[serde(rename = "_rowId")]
    row_id: String,
    #[serde(rename = "_sheet")]
    sheet: String,
    rfi_no: String,
    itp_no: String,
    item_no: String,
    description: String,
    forms: String,
    tag_no: String,
    area: String,
    row_no: String,
    date_of_issue: String,
    inspection_date: String,
    start_time: String,
    azfen_type: String,
    intec_type: String,
    location: String,
    subcontractor: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Batch {
    discipline: String,
    start_row: u32,
    records: Vec<Record>,
    base_rfis: Vec<String>,
    presets: BTreeMap<String, Preset>,
    lookup_columns: BTreeMap<String, u32>,
    max_row: u32,
    source_data: SourceData,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceSignature {
    size: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CopySource {
    workbook: String,
    sheet: String,
    header_row: u32,
    start_row: u32,
    rfi_numbers: Vec<String>,
    fields: Vec<String>,
    headers: Vec<String>,
    columns: Vec<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardPayload {
    pub html: String,
    pub plain: String,
    pub count: usize,
}

pub fn preview<F, C>(
    values: &Map<String, Value>,
    today: NaiveDate,
    gemini: Option<&GeminiClient>,
    mut progress: F,
    cancelled: C,
) -> Result<Plan, BackendError>
where
    F: FnMut(ProgressEvent),
    C: Fn() -> bool,
{
    checkpoint(&cancelled)?;
    let workbook_path = workbook_path(values)?;
    let email_text = value_string(values, "email_text").trim().to_string();
    let wrap = values
        .get("overflow_next_day")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut counts = requested_sheet_counts(values)?;
    let detected = (!email_text.is_empty())
        .then(|| detect_discipline(&email_text))
        .flatten();
    let has_new_selection = values.get("sheet_counts").is_some_and(Value::is_object);
    if let Some(discipline) = detected.as_deref()
        && is_discipline(discipline)
    {
        if !has_new_selection || counts == BTreeMap::from([("CIV".to_string(), 3)]) {
            let count = counts.get("CIV").copied().unwrap_or(3);
            counts = BTreeMap::from([(discipline.to_string(), count)]);
        } else if !counts.contains_key(discipline) {
            counts.insert(discipline.to_string(), 1);
        }
    }
    let requested = DISCIPLINES
        .iter()
        .filter(|discipline| counts.contains_key(**discipline))
        .copied()
        .collect::<Vec<_>>();
    progress_event(&mut progress, "preview", 0, 4);
    let workbook = read_workbook(&workbook_path).map_err(document_error)?;
    checkpoint(&cancelled)?;
    let mut sources = BTreeMap::new();
    for discipline in &requested {
        sources.insert(
            (*discipline).to_string(),
            read_source(&workbook, discipline)?,
        );
        checkpoint(&cancelled)?;
    }
    progress_event(&mut progress, "preview", 1, 4);

    let mut extraction = None;
    let mut extraction_discipline = None;
    let mut extraction_notes = Vec::new();
    let mut warnings = Vec::new();
    if !email_text.is_empty() {
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
        let offline = offline_extract(&email_text, &context);
        let provider = value_string(values, "email_provider").to_ascii_lowercase();
        let provider = if provider == "gemini" {
            "gemini"
        } else {
            "offline"
        };
        let (selected, fallback_error) = if provider == "gemini" {
            checkpoint(&cancelled)?;
            extract_with_gemini(values, &email_text, &discipline, &source.data, gemini).map_or_else(
                |error| (offline, Some(safe_error(&error))),
                |result| (result, None),
            )
        } else {
            (offline, None)
        };
        if (1..=MAX_RFI_COUNT).contains(&selected.count) {
            counts.insert(discipline.clone(), selected.count);
        }
        extraction_notes.extend(
            selected
                .notes
                .iter()
                .filter(|note| !note.trim().is_empty())
                .cloned(),
        );
        warnings.push(Warning {
            key: "warn.rfi.emailExtracted".to_string(),
            subject: format!(
                "{} | {} {} RFI | confidence {:.0}%",
                selected.provider,
                discipline,
                counts[&discipline],
                selected.confidence * 100.0
            ),
        });
        if provider == "gemini" {
            warnings.push(Warning {
                key: "warn.rfi.geminiPrivacy".to_string(),
                subject: String::new(),
            });
        }
        if let Some(error) = fallback_error {
            warnings.push(Warning {
                key: "warn.rfi.aiFallback".to_string(),
                subject: truncate_chars(&error, 300),
            });
        }
        extraction_discipline = Some(discipline);
        extraction = Some(selected);
    }
    checkpoint(&cancelled)?;

    let issue_day = today;
    let start_day = today
        .checked_add_days(Days::new(1))
        .ok_or_else(|| BackendError::validation("RFI schedule date is out of range"))?;
    let mut rows = Vec::new();
    let mut batches = Vec::new();
    let mut overflowed = 0_usize;
    for discipline in &requested {
        let source = &sources[*discipline];
        let (slots, sheet_overflowed) =
            allocate(&source.occupied, start_day, counts[*discipline], wrap)?;
        overflowed += sheet_overflowed;
        let mut sheet_rows = Vec::new();
        let mut base_rfis = Vec::new();
        for (index, slot) in slots.iter().enumerate() {
            let number = source.max_number + u32::try_from(index).unwrap_or(u32::MAX) + 1;
            let rfi = format!("AZE-SUN-INT-QMS-RFI-{discipline}-{number:04}");
            base_rfis.push(rfi.clone());
            sheet_rows.push(record(index, discipline, &rfi, source, issue_day, *slot));
        }
        if extraction_discipline.as_deref() == Some(*discipline)
            && let Some(extraction) = extraction.as_ref()
        {
            let (extracted_rows, notes) = rows_from_email(&sheet_rows, extraction, source);
            sheet_rows = extracted_rows;
            extraction_notes.extend(notes);
        }
        for row in &mut sheet_rows {
            clean_record(row);
        }
        let start_row = source.last_row + 1;
        let end_row =
            start_row + u32::try_from(sheet_rows.len().saturating_sub(1)).unwrap_or(u32::MAX);
        if end_row > source.data.max_row {
            return Err(BackendError::validation(format!(
                "{discipline} sheet'inde yeni satirlar icin ayrilmis kapasite kalmadi"
            )));
        }
        batches.push(Batch {
            discipline: (*discipline).to_string(),
            start_row,
            records: sheet_rows.clone(),
            base_rfis,
            presets: source.data.presets.clone(),
            lookup_columns: source.data.lookup_columns.clone(),
            max_row: source.data.max_row,
            source_data: source.data.clone(),
        });
        rows.extend(sheet_rows);
        checkpoint(&cancelled)?;
    }
    if !extraction_notes.is_empty() {
        deduplicate(&mut extraction_notes);
        warnings.push(Warning {
            key: "warn.rfi.emailNeedsReview".to_string(),
            subject: truncate_chars(&extraction_notes.join(" | "), MAX_WARNING_CHARS),
        });
    }
    warnings.extend(warnings_for_rows(&rows, overflowed, wrap));
    let signature = signature(&workbook_path)?;
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
    insert_json(&mut step_meta, "sourceSignature", &signature)?;
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
    let step = Step {
        kind: StepKind::WorkbookUpdate,
        src: Some(workbook_path.to_string_lossy().into_owned()),
        dst: Some(backup_path.to_string_lossy().into_owned()),
        note_key: Some("note.rfi.workbookUpdate".to_string()),
        meta: step_meta,
    };
    let mut meta = plan_meta(&sources, &rows)?;
    let copy_sources = batches
        .iter()
        .map(|batch| {
            Ok((
                batch.discipline.clone(),
                serde_json::to_value(copy_source(&workbook_path, batch)?)
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
    if let (Some(extraction), Some(discipline)) = (&extraction, &extraction_discipline) {
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
    progress_event(&mut progress, "preview", 4, 4);
    Ok(Plan {
        plan_id: String::new(),
        op_id: "rfi.log_add".to_string(),
        steps: vec![step],
        warnings,
        meta,
    })
}

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

pub fn preview_table(plan: &Plan, sheet: Option<&str>) -> Result<Value, BackendError> {
    let (fields, headers, rows, from_workbook, widths) = copy_data(plan, sheet)?;
    let table = json!({
        "headers": headers,
        "fields": fields,
        "rows": rows.iter().map(|row| fields.iter().map(|field| row.get(field).cloned().unwrap_or_default()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "columnWidths": widths.into_iter().map(|width| width.clamp(7.0, 42.0)).collect::<Vec<_>>(),
        "fromWorkbook": from_workbook,
        "count": rows.len(),
    });
    if serde_json::to_vec(&table).is_ok_and(|encoded| encoded.len() <= MAX_TABLE_BYTES) {
        Ok(table)
    } else {
        Err(BackendError::validation(
            "table preview exceeded the size limit",
        ))
    }
}

pub fn copy_payload(plan: &Plan, sheet: Option<&str>) -> Result<ClipboardPayload, BackendError> {
    let (fields, headers, rows, _, _) = copy_data(plan, sheet)?;
    let plain_rows = rows
        .iter()
        .map(|row| {
            fields
                .iter()
                .map(|field| row.get(field).cloned().unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>();
    let plain = std::iter::once(headers.join("\t"))
        .chain(plain_rows)
        .collect::<Vec<_>>()
        .join("\r\n");
    let headings = headers
        .iter()
        .map(|header| format!("<th style=\"border:1px solid #8b8b8b;padding:6px 8px;font-weight:700;text-align:center;vertical-align:middle;\">{}</th>", html_escape(header)))
        .collect::<String>();
    let body = rows
        .iter()
        .map(|row| {
            let cells = fields
                .iter()
                .map(|field| {
                    let align = if matches!(field.as_str(), "description" | "forms" | "location") { "left" } else { "center" };
                    let value = html_escape(row.get(field).map(String::as_str).unwrap_or("")).replace('\n', "<br>");
                    format!("<td style=\"border:1px solid #9a9a9a;padding:6px 8px;text-align:{align};vertical-align:middle;white-space:normal;\">{value}</td>")
                })
                .collect::<String>();
            format!("<tr>{cells}</tr>")
        })
        .collect::<String>();
    let html = format!(
        "<table cellpadding=\"0\" cellspacing=\"0\" style=\"border-collapse:collapse;font-family:Arial,sans-serif;font-size:10pt;color:#202020;\"><thead><tr style=\"background:#dedede;\">{headings}</tr></thead><tbody>{body}</tbody></table>"
    );
    if plain.len() > MAX_TABLE_BYTES || html.len() > MAX_TABLE_BYTES {
        return Err(BackendError::validation(
            "clipboard table exceeded the size limit",
        ));
    }
    Ok(ClipboardPayload {
        html,
        plain,
        count: rows.len(),
    })
}

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
    let workbook = read_workbook(path).map_err(document_error)?;
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

fn workbook_path(values: &Map<String, Value>) -> Result<PathBuf, BackendError> {
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

fn requested_sheet_counts(
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

fn protocol_integer(value: &Value) -> Option<i64> {
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

fn read_source(workbook: &WorkbookData, discipline: &str) -> Result<Source, BackendError> {
    let layout =
        layout(discipline).ok_or_else(|| BackendError::validation("gecersiz RFI sheet'i"))?;
    let sheet = workbook.sheet(layout.sheet).ok_or_else(|| {
        BackendError::validation(format!(
            "RFI Log icin {}/Sheet1 sayfalari bulunamadi",
            layout.sheet
        ))
    })?;
    let lookup = workbook.sheet("Sheet1").ok_or_else(|| {
        BackendError::validation(format!(
            "RFI Log icin {}/Sheet1 sayfalari bulunamadi",
            layout.sheet
        ))
    })?;
    let lookup_columns = lookup_columns(lookup)?;
    let mut presets = BTreeMap::new();
    for row in 1..=lookup.max_row {
        let item = clean_text(lookup.value(row, lookup_columns["item"]));
        let Some((item_discipline, _)) = item_parts(&item) else {
            continue;
        };
        if item_discipline != discipline {
            continue;
        }
        let azfen_item = clean_text(lookup.value(row, lookup_columns["azfenItem"]));
        let intec_item = clean_text(lookup.value(row, lookup_columns["intecItem"]));
        if azfen_item != item || intec_item != item {
            return Err(BackendError::validation(format!(
                "{discipline} Sheet1 Item eslemesi tutarsiz: {item}"
            )));
        }
        let preset = Preset {
            description: clean_text(lookup.value(row, lookup_columns["description"])),
            forms: clean_text(lookup.value(row, lookup_columns["forms"])),
            azfen_type: clean_text(lookup.value(row, lookup_columns["azfenType"])),
            intec_type: clean_text(lookup.value(row, lookup_columns["intecType"])),
        };
        let missing = [
            ("description", &preset.description),
            ("forms", &preset.forms),
            ("azfenType", &preset.azfen_type),
            ("intecType", &preset.intec_type),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.is_empty().then_some(name))
        .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(BackendError::validation(format!(
                "{discipline} Sheet1 esleme verisi eksik: {item} / {}",
                missing.join(", ")
            )));
        }
        if presets
            .get(&item)
            .is_some_and(|existing| existing != &preset)
        {
            return Err(BackendError::validation(format!(
                "{discipline} Sheet1 cakisan Item eslemesi: {item}"
            )));
        }
        presets.insert(item, preset);
    }
    if presets.is_empty() {
        return Err(BackendError::validation(format!(
            "{discipline} icin Sheet1 esleme verisi bulunamadi"
        )));
    }
    let mut max_number = 0_u32;
    let mut last_row = 9_u32;
    let mut latest_item = String::new();
    let mut tags = Vec::new();
    let mut areas = Vec::new();
    let mut locations = Vec::new();
    let mut subcontractors = Vec::new();
    let mut occupied = BTreeMap::<NaiveDate, BTreeMap<String, NaiveTime>>::new();
    for row in 10..=sheet.max_row {
        let rfi = sheet.value(row, 1).trim();
        let Some((rfi_discipline, number)) = rfi_parts(rfi) else {
            continue;
        };
        if rfi_discipline != discipline {
            continue;
        }
        max_number = max_number.max(number);
        last_row = row;
        let item = clean_text(sheet.value(row, 3));
        if !item.is_empty() {
            latest_item = item;
        }
        tags.push(clean_text(sheet.value(row, 6)));
        areas.push(clean_text(sheet.value(row, 7)));
        locations.push(clean_text(sheet.value(row, layout.location_col)));
        subcontractors.push(clean_text(sheet.value(row, layout.subcontractor_col)));
        if let (Some(day), Some(time)) = (
            cell_date(sheet.value(row, layout.inspection_col)),
            cell_time(sheet.value(row, layout.time_col)),
        ) {
            occupied
                .entry(day)
                .or_default()
                .insert(rfi.to_string(), time);
        }
    }
    let mut workbook_itps = BTreeMap::new();
    for column in 1..=lookup.max_column {
        let label = clean_text(lookup.value(1, column));
        if label.to_ascii_uppercase().contains("ITP")
            && label.to_ascii_uppercase().contains(discipline)
            && let Some(number) = trailing_four_digits(&label)
        {
            workbook_itps.insert(number, label);
        }
    }
    let mut itp_to_items = BTreeMap::<String, Vec<String>>::new();
    for item in presets.keys() {
        let (_, number) = item_parts(item).expect("validated item");
        let itp = workbook_itps
            .get(&number)
            .cloned()
            .unwrap_or_else(|| canonical_itp(discipline, number));
        itp_to_items.entry(itp).or_default().push(item.clone());
    }
    for items in itp_to_items.values_mut() {
        items.sort_by_key(|item| natural_key(item));
    }
    let default_item = if presets.contains_key(&latest_item) {
        latest_item
    } else {
        presets.keys().next().cloned().expect("nonempty presets")
    };
    let (_, default_number) = item_parts(&default_item).expect("validated default item");
    let default_itp = workbook_itps
        .get(&default_number)
        .cloned()
        .unwrap_or_else(|| canonical_itp(discipline, default_number));
    Ok(Source {
        data: SourceData {
            presets,
            itp_to_items,
            lookup_columns,
            tags: recent_unique(tags),
            areas: recent_unique(areas),
            locations: recent_unique(locations),
            subcontractors: recent_unique(subcontractors),
            max_row: sheet.max_row,
        },
        default_item,
        default_itp,
        max_number,
        last_row,
        occupied,
    })
}

fn source_from_data(batch: &Batch) -> Source {
    Source {
        data: batch.source_data.clone(),
        default_item: batch
            .records
            .first()
            .map(|record| record.item_no.clone())
            .unwrap_or_default(),
        default_itp: batch
            .records
            .first()
            .map(|record| record.itp_no.clone())
            .unwrap_or_default(),
        max_number: 0,
        last_row: batch.start_row.saturating_sub(1),
        occupied: BTreeMap::new(),
    }
}

fn lookup_columns(sheet: &WorksheetData) -> Result<BTreeMap<String, u32>, BackendError> {
    for start in 1..=sheet.max_column.saturating_sub(6) {
        if !matches!(
            header_key(sheet.value(1, start)).as_str(),
            "item" | "itemno" | "itemnumber"
        ) {
            continue;
        }
        if !matches!(
            header_key(sheet.value(1, start + 3)).as_str(),
            "item" | "itemno" | "itemnumber"
        ) || !matches!(
            header_key(sheet.value(1, start + 5)).as_str(),
            "item" | "itemno" | "itemnumber"
        ) {
            continue;
        }
        return Ok(BTreeMap::from([
            ("item".to_string(), start),
            ("description".to_string(), start + 1),
            ("forms".to_string(), start + 2),
            ("azfenItem".to_string(), start + 3),
            ("azfenType".to_string(), start + 4),
            ("intecItem".to_string(), start + 5),
            ("intecType".to_string(), start + 6),
        ]));
    }
    Err(BackendError::validation(
        "Sheet1 Item/description/forms/type esleme kolonlari bulunamadi",
    ))
}

fn allocate(
    occupied: &BTreeMap<NaiveDate, BTreeMap<String, NaiveTime>>,
    start_day: NaiveDate,
    count: usize,
    wrap: bool,
) -> Result<(Vec<(NaiveDate, NaiveTime)>, usize), BackendError> {
    let mut working = occupied.clone();
    let mut slots = Vec::new();
    let mut overflowed = 0;
    let mut day = start_day;
    for index in 0..count {
        let mut minutes = next_slot_minutes(&working, day);
        if minutes > DAY_END_MINUTES {
            overflowed += 1;
            if wrap {
                while minutes > DAY_END_MINUTES {
                    day = day.checked_add_days(Days::new(1)).ok_or_else(|| {
                        BackendError::validation("RFI schedule date is out of range")
                    })?;
                    minutes = next_slot_minutes(&working, day);
                }
            }
        }
        let time = NaiveTime::from_num_seconds_from_midnight_opt((minutes % (24 * 60)) * 60, 0)
            .ok_or_else(|| BackendError::validation("RFI schedule time is invalid"))?;
        working
            .entry(day)
            .or_default()
            .insert(format!("__planned_{index}"), time);
        slots.push((day, time));
    }
    Ok((slots, overflowed))
}

fn next_slot_minutes(
    occupied: &BTreeMap<NaiveDate, BTreeMap<String, NaiveTime>>,
    day: NaiveDate,
) -> u32 {
    occupied
        .get(&day)
        .and_then(|values| values.values().max())
        .map(|time| time.hour() * 60 + time.minute() + SLOT_MINUTES)
        .unwrap_or(DAY_START_MINUTES)
}

fn record(
    index: usize,
    discipline: &str,
    rfi: &str,
    source: &Source,
    issue_day: NaiveDate,
    slot: (NaiveDate, NaiveTime),
) -> Record {
    let preset = &source.data.presets[&source.default_item];
    Record {
        row_id: format!("{discipline}-g{}-r1", index + 1),
        sheet: discipline.to_string(),
        rfi_no: rfi.to_string(),
        itp_no: source.default_itp.clone(),
        item_no: source.default_item.clone(),
        description: preset.description.clone(),
        forms: preset.forms.clone(),
        tag_no: String::new(),
        area: String::new(),
        row_no: String::new(),
        date_of_issue: date_text(issue_day),
        inspection_date: date_text(slot.0),
        start_time: time_text(slot.1),
        azfen_type: preset.azfen_type.clone(),
        intec_type: preset.intec_type.clone(),
        location: String::new(),
        subcontractor: "AZFEN".to_string(),
    }
}

fn rows_from_email(
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

fn apply_email_values(
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

fn plan_meta(
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

fn columns() -> Vec<Value> {
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

fn copy_source(path: &Path, batch: &Batch) -> Result<CopySource, BackendError> {
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

type CopyData = (
    Vec<String>,
    Vec<String>,
    Vec<BTreeMap<String, String>>,
    bool,
    Vec<f64>,
);

fn copy_data(plan: &Plan, selected_sheet: Option<&str>) -> Result<CopyData, BackendError> {
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
    let source: CopySource = serde_json::from_value(source_value.clone())
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
    let Ok(workbook) = read_workbook(Path::new(&source.workbook)) else {
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

fn select_copy_source(
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

fn validate_record(record: &mut Record, index: usize, batch: &Batch) -> Result<(), BackendError> {
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

fn clean_record(record: &mut Record) {
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

fn warnings_for_rows(rows: &[Record], overflowed: usize, wrap: bool) -> Vec<Warning> {
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

fn extract_with_gemini(
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

fn safe_error(error: &BackendError) -> String {
    let safe = match error.code {
        "validation" => format!("ValueError: {}", error.message),
        _ => format!("RuntimeError: {}", error.message),
    };
    truncate_chars(&safe.replace(['\r', '\n'], " "), 300)
}

fn signature(path: &Path) -> Result<SourceSignature, BackendError> {
    let size = fs::metadata(path)
        .map_err(|_| BackendError::path_invalid("workbook metadata is unavailable"))?
        .len();
    let sha256 = file_sha256(path)
        .map_err(|_| BackendError::path_invalid("workbook hash is unavailable"))?;
    Ok(SourceSignature { size, sha256 })
}

fn only_workbook_step(plan: &Plan) -> Result<&Step, BackendError> {
    if plan.op_id != "rfi.log_add"
        || plan.steps.len() != 1
        || plan.steps[0].kind != StepKind::WorkbookUpdate
    {
        return Err(BackendError::validation("gecersiz RFI Log plani"));
    }
    Ok(&plan.steps[0])
}

fn decode_batches(step: &Step) -> Result<Vec<Batch>, BackendError> {
    serde_json::from_value(
        step.meta
            .get("batches")
            .cloned()
            .ok_or_else(|| BackendError::validation("gecersiz coklu-sheet RFI plani"))?,
    )
    .map_err(|_| BackendError::validation("gecersiz coklu-sheet RFI plani"))
}

fn formula_or_value(value: &str, default: &str, formula: String) -> CellUpdate {
    if normalize_lines(value) == normalize_lines(default) {
        CellUpdate::Formula {
            formula,
            cached: Some(default.to_string()),
        }
    } else {
        CellUpdate::Text(value.to_string())
    }
}

fn normalize_lines(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .nfc()
        .collect()
}

fn resolve_itp(value: &str, item: &str, mapping: &BTreeMap<String, Vec<String>>) -> Option<String> {
    if mapping.contains_key(value) {
        return Some(value.to_string());
    }
    let signature = itp_signature(value);
    let matches = mapping
        .keys()
        .filter(|label| !signature.is_empty() && itp_signature(label) == signature)
        .cloned()
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        return matches.into_iter().next();
    }
    unique_itp_for_item(item, mapping)
}

fn unique_itp_for_item(item: &str, mapping: &BTreeMap<String, Vec<String>>) -> Option<String> {
    let matches = mapping
        .iter()
        .filter(|(_, items)| items.iter().any(|candidate| candidate == item))
        .map(|(label, _)| label.clone())
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0].clone())
}

fn itp_signature(value: &str) -> Vec<String> {
    token_pattern()
        .find_iter(&value.nfkc().collect::<String>().to_uppercase())
        .map(|token| token.as_str().to_string())
        .collect()
}

fn infer_area(tag: &str) -> String {
    let upper = tag.to_ascii_uppercase();
    if let Some(site) = leading_site_pattern()
        .captures(&upper)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str())
    {
        return if site == "N" {
            "North Site"
        } else {
            "South Site"
        }
        .to_string();
    }
    let north = north_pattern().is_match(&upper);
    let south = south_pattern().is_match(&upper);
    match (north, south) {
        (true, false) => "North Site".to_string(),
        (false, true) => "South Site".to_string(),
        _ => String::new(),
    }
}

fn clean_text(value: &str) -> String {
    let mut text = value.trim().nfc().collect::<String>();
    for _ in 0..2 {
        if mojibake_score(&text) == 0 {
            break;
        }
        let Some(bytes) = text.chars().map(cp1252_byte).collect::<Option<Vec<_>>>() else {
            break;
        };
        let Ok(repaired) = String::from_utf8(bytes) else {
            break;
        };
        if mojibake_score(&repaired) >= mojibake_score(&text) {
            break;
        }
        text = repaired.nfc().collect();
    }
    text
}

fn cp1252_byte(character: char) -> Option<u8> {
    if (character as u32) <= 0x7f || (0xa0..=0xff).contains(&(character as u32)) {
        return Some(character as u8);
    }
    Some(match character {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8a,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        'Ž' => 0x8e,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9a,
        '›' => 0x9b,
        'œ' => 0x9c,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        _ => return None,
    })
}

fn mojibake_score(value: &str) -> usize {
    ["Ã", "Â", "â", "ð", "Ë"]
        .iter()
        .map(|marker| value.matches(marker).count())
        .sum()
}

fn header_key(value: &str) -> String {
    value
        .nfkd()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn item_parts(value: &str) -> Option<(String, u32)> {
    let captures = item_pattern().captures(value)?;
    Some((
        captures.get(1)?.as_str().to_ascii_uppercase(),
        captures.get(2)?.as_str().parse().ok()?,
    ))
}

fn rfi_parts(value: &str) -> Option<(String, u32)> {
    let captures = rfi_pattern().captures(value)?;
    Some((
        captures.get(1)?.as_str().to_ascii_uppercase(),
        captures.get(2)?.as_str().parse().ok()?,
    ))
}

fn trailing_four_digits(value: &str) -> Option<u32> {
    trailing_digits_pattern()
        .captures(value)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse().ok())
}

fn canonical_itp(discipline: &str, number: u32) -> String {
    format!("AZE-SUN-INT-QMS-ITP-{discipline}-{number:04}")
}

fn natural_key(value: &str) -> Vec<String> {
    natural_pattern()
        .find_iter(value)
        .map(|part| {
            part.as_str().parse::<u64>().map_or_else(
                |_| format!("1{}", part.as_str()),
                |number| format!("0{number:020}"),
            )
        })
        .collect()
}

fn recent_unique(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .rev()
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .take(500)
        .collect()
}

fn cell_date(value: &str) -> Option<NaiveDate> {
    if let Ok(serial) = value.parse::<f64>() {
        return NaiveDate::from_ymd_opt(1899, 12, 30)?
            .checked_add_days(Days::new(serial.floor().max(0.0) as u64));
    }
    parse_date(value).ok()
}

fn cell_time(value: &str) -> Option<NaiveTime> {
    if let Ok(serial) = value.parse::<f64>() {
        let seconds = (serial * 24.0 * 60.0 * 60.0).round() as u32 % (24 * 60 * 60);
        return NaiveTime::from_num_seconds_from_midnight_opt(seconds, 0)
            .map(|time| time.with_second(0).unwrap_or(time));
    }
    parse_time(value).ok()
}

fn parse_date(value: &str) -> Result<NaiveDate, BackendError> {
    for format in ["%d-%b-%y", "%Y-%m-%d", "%d.%m.%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(value.trim(), format) {
            return Ok(date);
        }
    }
    Err(BackendError::validation(format!("gecersiz tarih: {value}")))
}

fn parse_time(value: &str) -> Result<NaiveTime, BackendError> {
    NaiveTime::parse_from_str(value.trim(), "%H:%M")
        .map_err(|_| BackendError::validation(format!("gecersiz saat: {value}")))
}

fn date_text(value: NaiveDate) -> String {
    value.format("%d-%b-%y").to_string()
}

fn time_text(value: NaiveTime) -> String {
    value.format("%H:%M").to_string()
}

fn excel_date(value: NaiveDate) -> i64 {
    value
        .signed_duration_since(NaiveDate::from_ymd_opt(1899, 12, 30).expect("Excel epoch"))
        .num_days()
}

fn excel_time(value: NaiveTime) -> String {
    let seconds = value.num_seconds_from_midnight() as f64;
    format!("{:.15}", seconds / (24.0 * 60.0 * 60.0))
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn record_map(record: Record) -> BTreeMap<String, String> {
    serde_json::to_value(record)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| (key, value.as_str().unwrap_or_default().to_string()))
        .collect()
}

fn copy_cell_text(sheet: &WorksheetData, row: u32, column: u32, field: &str) -> String {
    let raw = sheet.value(row, column);
    match field {
        "dateOfIssue" | "inspectionDate" => cell_date(raw)
            .map(date_text)
            .unwrap_or_else(|| raw.to_string()),
        "startTime" => cell_time(raw)
            .map(time_text)
            .unwrap_or_else(|| raw.to_string()),
        _ => raw.to_string(),
    }
}

fn field_width(field: &str) -> f64 {
    match field {
        "rfiNo" => 22.0,
        "itpNo" => 24.0,
        "itemNo" | "rowNo" | "azfenType" | "intecType" => 10.0,
        "description" => 28.0,
        "forms" => 20.0,
        "tagNo" => 15.0,
        "area" | "dateOfIssue" | "inspectionDate" | "startTime" => 12.0,
        "location" => 18.0,
        "subcontractor" => 14.0,
        _ => 14.0,
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn column_name(mut number: u32) -> String {
    let mut output = String::new();
    while number > 0 {
        let remainder = (number - 1) % 26;
        output.insert(0, char::from(b'A' + remainder as u8));
        number = (number - 1) / 26;
    }
    output
}

fn value_string(values: &Map<String, Value>, key: &str) -> String {
    values
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn is_discipline(value: &str) -> bool {
    DISCIPLINES.contains(&value)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn deduplicate(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn insert_json<T: Serialize>(
    object: &mut JsonObject,
    key: &str,
    value: &T,
) -> Result<(), BackendError> {
    object.insert(
        key.to_string(),
        serde_json::to_value(value)
            .map_err(|_| BackendError::protocol("RFI plan could not be encoded"))?,
    );
    Ok(())
}

fn insert_value(object: &mut JsonObject, key: &str, value: impl Into<Value>) {
    object.insert(key.to_string(), value.into());
}

fn checkpoint(cancelled: &impl Fn() -> bool) -> Result<(), BackendError> {
    if cancelled() {
        Err(BackendError::cancelled())
    } else {
        Ok(())
    }
}

fn progress_event(progress: &mut impl FnMut(ProgressEvent), phase: &str, done: u64, total: u64) {
    progress(ProgressEvent {
        phase: phase.to_string(),
        done,
        total,
        subject: String::new(),
    });
}

fn document_error(error: impl std::fmt::Display) -> BackendError {
    BackendError::validation(error.to_string())
}

fn rfi_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)-RFI-(CIV|MEC|ELE|PRC)-(\d+)$").unwrap())
}

fn item_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)^(CIV|MEC|ELE|PRC)-(\d+)\.").unwrap())
}

fn trailing_digits_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(\d{4})$").unwrap())
}

fn token_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"[A-Z]+|\d+").unwrap())
}

fn natural_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"\d+|\D+").unwrap())
}

fn leading_site_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)^\s*([NS])\d{1,3}(?:[^A-Z0-9]|$)").unwrap())
}

fn north_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Z0-9])N\d{1,3}(?:[^A-Z0-9]|$)").unwrap())
}

fn south_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Z0-9])S\d{1,3}(?:[^A-Z0-9]|$)").unwrap())
}
