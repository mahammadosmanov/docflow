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

    let EmailOutcome {
        extraction,
        discipline: extraction_discipline,
        notes: mut extraction_notes,
        mut warnings,
    } = extract_email_request(
        values,
        &email_text,
        detected,
        &sources,
        &mut counts,
        gemini,
        &cancelled,
    )?;
    checkpoint(&cancelled)?;
    progress_event(&mut progress, "rfi_request_analysis", 1, 1);
    progress_event(
        &mut progress,
        "rfi_plan_preparation",
        0,
        requested.len() as u64,
    );

    let start_day = today
        .checked_add_days(Days::new(1))
        .ok_or_else(|| BackendError::validation("RFI schedule date is out of range"))?;
    let mut rows = Vec::new();
    let mut batches = Vec::new();
    let mut overflowed = 0_usize;
    for (discipline_index, discipline) in requested.iter().enumerate() {
        let source = &sources[*discipline];
        let email = if extraction_discipline.as_deref() == Some(*discipline) {
            extraction.as_ref()
        } else {
            None
        };
        let (batch, sheet_overflowed, notes) = sheet_batch(
            discipline,
            source,
            counts[*discipline],
            wrap,
            today,
            start_day,
            email,
        )?;
        overflowed += sheet_overflowed;
        extraction_notes.extend(notes);
        rows.extend(batch.records.iter().cloned());
        batches.push(batch);
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
    let step = workbook_update_step(&workbook_path, &signature, &batches)?;
    let mut meta = plan_meta(&sources, &rows)?;
    attach_editor_context(
        &mut meta,
        &workbook_path,
        &batches,
        extraction.as_ref(),
        extraction_discipline.as_deref(),
        &counts,
    )?;
    progress_event(&mut progress, "preview_ready", 1, 1);
    Ok(Plan {
        plan_id: String::new(),
        op_id: "rfi.log_add".to_string(),
        steps: vec![step],
        warnings,
        meta,
    })
}

mod column_layout;
mod payloads;
mod revise;
pub use payloads::*;
pub use revise::*;
mod copy;
mod email;
mod preview_steps;
mod source;
mod support;
mod updates;
pub(crate) use column_layout::*;
use copy::*;
use email::*;
use preview_steps::*;
use source::*;
use support::*;
pub(crate) use updates::*;

#[cfg(test)]
mod rfi_identity_tests;
