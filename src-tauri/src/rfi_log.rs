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
    ooxml::{CellUpdate, WorkbookData, WorksheetData, read_workbook_selective},
    rfi_code::normalize_candidate_number,
};

const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
const REQUIRED_SHEETS: [&str; 5] = ["CIV", "MEC", "ELE", "PRC", "Sheet1"];
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
pub(crate) struct Layout {
    pub(crate) sheet: &'static str,
    pub(crate) issue_col: u32,
    pub(crate) inspection_col: u32,
    pub(crate) expiry_col: u32,
    pub(crate) time_col: u32,
    pub(crate) azfen_col: u32,
    pub(crate) intec_col: u32,
    pub(crate) location_col: u32,
    pub(crate) subcontractor_col: u32,
    pub(crate) status_col: u32,
    pub(crate) remarks_col: u32,
    pub(crate) validity_col: u32,
    pub(crate) row_col: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RfiStatusColumns {
    pub sheet: &'static str,
    pub rfi_column: u32,
    pub status_column: u32,
}

pub(crate) fn status_columns() -> [RfiStatusColumns; 4] {
    DISCIPLINES.map(|discipline| {
        let projection = layout(discipline).expect("canonical RFI Log discipline has a layout");
        RfiStatusColumns {
            sheet: projection.sheet,
            rfi_column: 1,
            status_column: projection.remarks_col,
        }
    })
}

pub(crate) fn layout(discipline: &str) -> Option<Layout> {
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
            remarks_col: 20,
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
            remarks_col: 21,
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
            remarks_col: 20,
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
            remarks_col: 20,
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
    progress_event(&mut progress, "workbook_reading", 0, 1);
    let workbook =
        read_workbook_selective(&workbook_path, &REQUIRED_SHEETS).map_err(document_error)?;
    checkpoint(&cancelled)?;
    let mut sources = BTreeMap::new();
    for discipline in &requested {
        sources.insert(
            (*discipline).to_string(),
            read_source(&workbook, discipline)?,
        );
        checkpoint(&cancelled)?;
    }
    progress_event(&mut progress, "workbook_reading", 1, 1);
    progress_event(&mut progress, "rfi_request_analysis", 0, 1);

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
                "{} | {} {} RFI",
                selected.provider, discipline, counts[&discipline]
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
    progress_event(&mut progress, "rfi_request_analysis", 1, 1);
    progress_event(
        &mut progress,
        "rfi_plan_preparation",
        0,
        requested.len() as u64,
    );

    let issue_day = today;
    let start_day = today
        .checked_add_days(Days::new(1))
        .ok_or_else(|| BackendError::validation("RFI schedule date is out of range"))?;
    let mut rows = Vec::new();
    let mut batches = Vec::new();
    let mut overflowed = 0_usize;
    for (discipline_index, discipline) in requested.iter().enumerate() {
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
        progress_event(
            &mut progress,
            "rfi_plan_preparation",
            (discipline_index + 1) as u64,
            requested.len() as u64,
        );
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
    progress_event(&mut progress, "preview_ready", 1, 1);
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
                    let value = html_escape(row.get(field).map_or("", String::as_str)).replace('\n', "<br>");
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

pub fn reply_payload(plan: &Plan) -> Result<ClipboardPayload, BackendError> {
    let editor = plan
        .meta
        .get("editor")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("bu planda kopyalanabilir tablo yok"))?;
    let sources = editor
        .get("copySources")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("bu planda kopyalanabilir tablo yok"))?;
    let sheets = DISCIPLINES
        .into_iter()
        .filter(|sheet| sources.contains_key(*sheet))
        .collect::<Vec<_>>();
    if sheets.is_empty() {
        return Err(BackendError::validation(
            "bu planda kopyalanabilir tablo yok",
        ));
    }

    let multiple = sheets.len() > 1;
    let mut plain_parts = Vec::with_capacity(sheets.len());
    let mut html_parts = Vec::with_capacity(sheets.len());
    let mut count = 0usize;
    for sheet in sheets {
        let payload = copy_payload(plan, Some(sheet))?;
        count += payload.count;
        plain_parts.push(if multiple {
            format!("{sheet}\r\n{}", payload.plain)
        } else {
            payload.plain
        });
        html_parts.push(if multiple {
            format!("<div style=\"font-family:Arial,sans-serif;font-size:10pt;font-weight:700;margin:0 0 6px 0;\">{sheet}</div>{}", payload.html)
        } else {
            payload.html
        });
    }
    let plain = plain_parts.join("\r\n\r\n");
    let html = format!("{}<br><br>", html_parts.join("<br>"));
    if plain.len() > MAX_TABLE_BYTES || html.len() > MAX_TABLE_BYTES {
        return Err(BackendError::validation(
            "clipboard table exceeded the size limit",
        ));
    }
    Ok(ClipboardPayload { plain, html, count })
}

mod copy;
mod email;
mod source;
mod support;
mod updates;
use copy::*;
use email::*;
use source::*;
use support::*;
pub(crate) use updates::*;

#[cfg(test)]
mod rfi_identity_tests;
