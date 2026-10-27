use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    naming::display_name,
    ooxml::{
        WorkbookSafetyFacts, WorksheetDescriptor, WorksheetVisibility, ordered_workbook_discovery,
    },
    page_range::{PageRange, PageRangeCapability, canonical_for},
    print_queue::{
        BatchPrintParams, FileSignature, PrintAdapter, PrintJob, PrintJobSupport, PrintSettings,
        PrinterInventory, file_signature_from_open_file,
    },
};

mod rows;
use rows::*;

pub const MAX_FILES: usize = 250;
pub const MAX_COPIES: u32 = 99;
pub const MAX_RETRIES: u32 = 3;
pub const MIN_TIMEOUT_MINUTES: u32 = 1;
pub const MAX_TIMEOUT_MINUTES: u32 = 240;

const PAPER_CHOICES: &[&str] = &["auto", "A4", "A3", "A5", "Letter", "Legal"];
const ORIENTATION_CHOICES: &[&str] = &["auto", "portrait", "landscape"];
const SCALING_CHOICES: &[&str] = &["fit", "actual", "shrink"];
const COLOR_CHOICES: &[&str] = &["printer", "color", "grayscale"];
const DUPLEX_CHOICES: &[&str] = &["printer", "simplex", "long", "short"];
const ERROR_ACTION_CHOICES: &[&str] = &["pause", "skip"];

pub fn preview<A, P>(
    params: &BatchPrintParams,
    inventory: &PrinterInventory,
    adapter: &A,
    progress: P,
) -> Result<Plan, BackendError>
where
    A: PrintAdapter + ?Sized,
    P: FnMut(ProgressEvent),
{
    preview_with_workbook_opener(params, inventory, adapter, progress, |path| {
        File::open(path).map_err(|_| "warn.print.workbookDiscoveryFailed")
    })
}

pub(crate) fn preview_with_workbook_opener<A, P, O>(
    params: &BatchPrintParams,
    inventory: &PrinterInventory,
    adapter: &A,
    mut progress: P,
    mut open_workbook: O,
) -> Result<Plan, BackendError>
where
    A: PrintAdapter + ?Sized,
    P: FnMut(ProgressEvent),
    O: FnMut(&Path) -> Result<File, &'static str>,
{
    if inventory.printers.is_empty() {
        return Ok(warning_plan("warn.print.noPrinters", ""));
    }
    if !inventory.contains(&params.printer) {
        return Ok(warning_plan(
            "warn.print.printerUnavailable",
            &params.printer,
        ));
    }
    let files = ordered_unique_files(&params.files);
    if files.len() > MAX_FILES {
        return Ok(warning_plan(
            "warn.print.tooManyFiles",
            &files.len().to_string(),
        ));
    }
    if !(1..=MAX_COPIES).contains(&params.copies) {
        return Ok(warning_plan(
            "warn.print.invalidCopies",
            &params.copies.to_string(),
        ));
    }
    if params.retries > MAX_RETRIES {
        return Ok(warning_plan(
            "warn.print.invalidRetries",
            &params.retries.to_string(),
        ));
    }
    if !(MIN_TIMEOUT_MINUTES..=MAX_TIMEOUT_MINUTES).contains(&params.timeout_minutes) {
        return Ok(warning_plan(
            "warn.print.invalidTimeout",
            &params.timeout_minutes.to_string(),
        ));
    }

    let normalized = NormalizedSettings::from_params(params);
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    for (index, path) in files.iter().enumerate() {
        progress(ProgressEvent {
            phase: "print_file_inspection".to_string(),
            done: index as u64,
            total: files.len() as u64,
            subject: String::new(),
        });
        if is_excel_workbook(path) {
            match open_workbook(path).and_then(|file| rows_from_workbook(path, &normalized, file)) {
                Ok(workbook_rows) => {
                    if rows.len() + workbook_rows.len() > MAX_FILES {
                        return Ok(warning_plan(
                            "warn.print.expandedOrderTooLarge",
                            &MAX_FILES.to_string(),
                        ));
                    }
                    rows.extend(workbook_rows);
                }
                Err(warning_key) => {
                    warnings.push(warning(warning_key, display_name(path)));
                }
            }
            continue;
        }
        if !regular_file(path) {
            warnings.push(warning("warn.print.fileMissing", display_name(path)));
            continue;
        }
        match adapter.inspect(path, &params.printer, &normalized.print_settings()) {
            Ok(support) => {
                if support.settings_limited {
                    warnings.push(warning("warn.print.settingsLimited", display_name(path)));
                }
                if rows.len() == MAX_FILES {
                    return Ok(warning_plan(
                        "warn.print.expandedOrderTooLarge",
                        &MAX_FILES.to_string(),
                    ));
                }
                rows.push(row_from_file(path, index, &normalized, &support)?);
            }
            Err(error) if error.is_unsupported() => {
                warnings.push(warning("warn.print.noHandler", display_name(path)));
            }
            Err(error) if error.is_unsupported_options() => {
                warnings.push(warning("warn.print.unsupportedOptions", display_name(path)));
            }
            Err(error) => return Err(error.into_backend_error()),
        }
    }
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: files.len() as u64,
        total: files.len() as u64,
        subject: String::new(),
    });
    if rows.len() > MAX_FILES {
        return Ok(warning_plan(
            "warn.print.expandedOrderTooLarge",
            &MAX_FILES.to_string(),
        ));
    }
    Ok(plan_from_rows(
        rows,
        inventory,
        params.collate,
        Uuid::new_v4().to_string(),
        String::new(),
        warnings,
    ))
}

pub fn revise<A: PrintAdapter + ?Sized>(
    plan: &Plan,
    raw_rows: &[Value],
    inventory: &PrinterInventory,
    adapter: &A,
) -> Result<Plan, BackendError> {
    let editor = plan
        .meta
        .get("editor")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("print plan has no editor"))?;
    let originals = editor
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("print plan has no rows"))?;
    let original_by_id = originals
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|row| {
            row.get("_rowId")
                .and_then(Value::as_str)
                .map(|id| (id.to_string(), row.clone()))
        })
        .collect::<HashMap<_, _>>();
    if raw_rows.len() != original_by_id.len() {
        return Err(BackendError::validation(
            "print queue rows cannot be added or removed",
        ));
    }
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for raw in raw_rows {
        let raw = raw
            .as_object()
            .ok_or_else(|| BackendError::validation("invalid print row"))?;
        let row_id = raw
            .get("_rowId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !seen.insert(row_id.to_string()) {
            return Err(BackendError::validation("invalid print row identity"));
        }
        let original = original_by_id
            .get(row_id)
            .ok_or_else(|| BackendError::validation("invalid print row identity"))?;
        for field in ["signature", "worksheet", "workbookSafety"] {
            if raw.get(field) != original.get(field) {
                return Err(BackendError::validation(
                    "immutable print row metadata changed",
                ));
            }
        }
        let printer = string_field(raw, "printer");
        if !inventory.contains(&printer) {
            return Err(BackendError::validation("printer is unavailable"));
        }
        let copies = unsigned_field(raw, "copies").unwrap_or_default();
        if !(1..=u64::from(MAX_COPIES)).contains(&copies) {
            return Err(BackendError::validation("copies must be between 1 and 99"));
        }
        let path = PathBuf::from(string_field(original, "path"));
        let parsed_page_range = PageRange::parse(&string_field(raw, "pageRange"))?.canonical();
        let settings = PrintSettings {
            paper: choice(raw, "paper", PAPER_CHOICES, "A4"),
            orientation: choice(raw, "orientation", ORIENTATION_CHOICES, "auto"),
            scaling: choice(raw, "scaling", SCALING_CHOICES, "fit"),
            color: choice(raw, "color", COLOR_CHOICES, "printer"),
            duplex: choice(raw, "duplex", DUPLEX_CHOICES, "printer"),
            page_range: parsed_page_range.clone(),
            timeout_minutes: unsigned_field(original, "timeoutMinutes").unwrap_or(30) as u32,
        };
        let worksheet_row = original.get("worksheet").is_some();
        let support = if worksheet_row {
            PrintJobSupport::controlled()
        } else {
            adapter
                .inspect(&path, &printer, &settings)
                .map_err(super::super::print_queue::PrintFailure::into_backend_error)?
        };
        let page_range = if worksheet_row {
            if string_field(raw, "pageRange") != "all" {
                return Err(BackendError::validation(
                    "error.print.worksheetPageRangeAllOnly",
                ));
            }
            "all".to_string()
        } else {
            canonical_for(&parsed_page_range, &support.page_range)?
        };
        let default_enabled = original.get("worksheet").is_none()
            || original.get("worksheetVisibility").and_then(Value::as_str) == Some("visible");
        let mut row = original.clone();
        row.insert(
            "enabled".to_string(),
            Value::Bool(
                raw.get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(default_enabled),
            ),
        );
        row.insert("printer".to_string(), Value::String(printer));
        row.insert(
            "paper".to_string(),
            Value::String(choice(raw, "paper", PAPER_CHOICES, "A4")),
        );
        row.insert(
            "orientation".to_string(),
            Value::String(choice(raw, "orientation", ORIENTATION_CHOICES, "auto")),
        );
        row.insert(
            "scaling".to_string(),
            Value::String(choice(raw, "scaling", SCALING_CHOICES, "fit")),
        );
        row.insert(
            "color".to_string(),
            Value::String(choice(raw, "color", COLOR_CHOICES, "printer")),
        );
        row.insert(
            "duplex".to_string(),
            Value::String(choice(raw, "duplex", DUPLEX_CHOICES, "printer")),
        );
        row.insert("copies".to_string(), json!(copies));
        row.insert("verb".to_string(), Value::String(support.verb));
        row.insert("pageRange".to_string(), Value::String(page_range));
        rows.push(row);
    }
    if originals.iter().any(|row| row.get("worksheet").is_some())
        && !rows
            .iter()
            .any(|row| row.get("enabled").and_then(Value::as_bool).unwrap_or(true))
    {
        return Err(BackendError::validation(
            "print queue requires an enabled worksheet row",
        ));
    }
    let warnings = plan
        .warnings
        .iter()
        .filter(|warning| {
            !matches!(
                warning.key.as_str(),
                "warn.print.nonUndoable" | "warn.print.noJobsSelected"
            )
        })
        .cloned()
        .collect();
    Ok(plan_from_rows(
        rows,
        inventory,
        plan.meta
            .get("collate")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        string_meta(plan, "queueId"),
        string_meta(plan, "resumeQueueId"),
        warnings,
    ))
}

pub fn resume_plan(state: &Value, inventory: &PrinterInventory) -> Result<Plan, BackendError> {
    let queue_id = state
        .get("queueId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BackendError::validation("saved print queue is invalid"))?;
    let saved_jobs = state
        .get("jobs")
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("saved print queue has no jobs"))?;
    let mut rows = Vec::with_capacity(saved_jobs.len());
    for (index, saved) in saved_jobs.iter().enumerate() {
        let saved = saved
            .as_object()
            .ok_or_else(|| BackendError::validation("saved print job is invalid"))?;
        let status = string_field(saved, "status");
        let path = PathBuf::from(string_field(saved, "path"));
        let mut row = Map::new();
        row.insert("_rowId".to_string(), json!(format!("resume-{}", index + 1)));
        row.insert(
            "_resumeJobId".to_string(),
            json!(
                saved
                    .get("jobId")
                    .and_then(Value::as_str)
                    .map_or_else(|| format!("job-{}", index + 1), str::to_string)
            ),
        );
        row.insert(
            "enabled".to_string(),
            json!(matches!(
                status.as_str(),
                "pending" | "failed" | "cancelled"
            )),
        );
        row.insert("status".to_string(), json!(status));
        row.insert("file".to_string(), json!(display_name(&path)));
        row.insert("path".to_string(), json!(path));
        row.insert(
            "format".to_string(),
            json!(
                saved
                    .get("format")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map_or_else(|| file_format(&path), str::to_string)
            ),
        );
        copy_or_default(&mut row, saved, "printer", json!(""));
        copy_or_default(&mut row, saved, "paper", json!("A4"));
        copy_or_default(&mut row, saved, "orientation", json!("auto"));
        copy_or_default(&mut row, saved, "scaling", json!("fit"));
        copy_or_default(&mut row, saved, "color", json!("printer"));
        copy_or_default(&mut row, saved, "duplex", json!("printer"));
        copy_or_default(&mut row, saved, "pageRange", json!("all"));
        row.insert("copies".to_string(), json!(1));
        copy_or_default(&mut row, saved, "retries", json!(0));
        copy_or_default(&mut row, saved, "onError", json!("pause"));
        copy_or_default(&mut row, saved, "timeoutMinutes", json!(30));
        copy_or_default(&mut row, saved, "verb", json!(""));
        copy_or_default(&mut row, saved, "signature", json!({}));
        if let Some(worksheet) = saved.get("worksheet") {
            row.insert("worksheet".to_string(), worksheet.clone());
            row.insert(
                "worksheetName".to_string(),
                worksheet.get("name").cloned().unwrap_or(Value::Null),
            );
            row.insert(
                "worksheetVisibility".to_string(),
                worksheet.get("visibility").cloned().unwrap_or(Value::Null),
            );
        }
        if let Some(safety) = saved.get("workbookSafety") {
            row.insert("workbookSafety".to_string(), safety.clone());
        }
        rows.push(row);
    }
    Ok(plan_from_rows(
        rows,
        inventory,
        false,
        queue_id.to_string(),
        queue_id.to_string(),
        vec![warning(
            "warn.print.resumingQueue",
            queue_id.chars().take(8).collect::<String>(),
        )],
    ))
}

pub fn validate_job(job: &PrintJob) -> Result<(), BackendError> {
    if job.job_id.is_empty()
        || job.path.is_empty()
        || job.printer.is_empty()
        || !matches!(job.verb.as_str(), "docflow" | "printto" | "print")
        || !PAPER_CHOICES.contains(&job.paper.as_str())
        || !ORIENTATION_CHOICES.contains(&job.orientation.as_str())
        || !SCALING_CHOICES.contains(&job.scaling.as_str())
        || !COLOR_CHOICES.contains(&job.color.as_str())
        || !DUPLEX_CHOICES.contains(&job.duplex.as_str())
        || !ERROR_ACTION_CHOICES.contains(&job.on_error.as_str())
        || job.retries > MAX_RETRIES
        || !(MIN_TIMEOUT_MINUTES..=MAX_TIMEOUT_MINUTES).contains(&job.timeout_minutes)
        || job.copy == 0
    {
        return Err(BackendError::validation(
            "print plan contains an invalid job",
        ));
    }
    if let Some(worksheet) = job.worksheet.as_ref() {
        if !matches!(job.format.as_str(), "XLSX" | "XLSM")
            || job.verb != "docflow"
            || job.page_range != "all"
            || !job.signature.has_valid_sha256()
            || job
                .workbook_safety
                .as_ref()
                .is_none_or(workbook_safety_is_unsafe)
            || worksheet.workbook_index == 0
            || worksheet.sheet_id == 0
            || worksheet.relationship_id.is_empty()
            || worksheet.part_path.is_empty()
            || worksheet.name.is_empty()
        {
            return Err(BackendError::validation(if job.page_range != "all" {
                "error.print.worksheetPageRangeAllOnly"
            } else {
                "print plan contains invalid worksheet metadata"
            }));
        }
    } else if job.workbook_safety.is_some() {
        return Err(BackendError::validation(
            "print plan contains invalid worksheet metadata",
        ));
    }
    let canonical = canonical_for(&job.page_range, &job_page_range_capability(job))?;
    if canonical != job.page_range {
        return Err(BackendError::validation("error.print.pageRangeInvalid"));
    }
    Ok(())
}

fn job_page_range_capability(job: &PrintJob) -> PageRangeCapability {
    if job.worksheet.is_some() {
        return PageRangeCapability::AllOnly;
    }
    if matches!(job.verb.as_str(), "printto" | "print") {
        return PageRangeCapability::AllOnly;
    }
    match job.format.as_str() {
        "PDF" => PageRangeCapability::Pdf { page_count: None },
        "PNG" | "JPG" | "JPEG" | "BMP" | "TIF" | "TIFF" => PageRangeCapability::SinglePage,
        _ => PageRangeCapability::AllOnly,
    }
}

fn editor(rows: Vec<Map<String, Value>>, printers: Vec<String>) -> Value {
    let reorderable = rows.iter().any(|row| row.get("worksheet").is_some());
    let mut columns = vec![
        json!({"key":"enabled","labelKey":"print.col.include","kind":"bool","width":64}),
        json!({"key":"status","labelKey":"print.col.status","kind":"readonly","width":90,"bulk":false}),
        json!({"key":"file","labelKey":"print.col.file","kind":"readonly","width":260,"bulk":false}),
        json!({"key":"format","labelKey":"print.col.format","kind":"readonly","width":76,"bulk":false}),
    ];
    if reorderable {
        columns.push(json!({"key":"worksheetName","labelKey":"print.col.worksheet","kind":"readonly","width":180,"bulk":false}));
        columns.push(json!({"key":"worksheetVisibility","labelKey":"print.col.worksheetVisibility","kind":"readonly","width":112,"bulk":false}));
    }
    columns.extend([
        json!({"key":"pageRange","labelKey":"print.col.pageRange","helpKey":"print.col.pageRange.help","kind":"text","width":112,"bulk":false}),
        json!({"key":"printer","labelKey":"print.col.printer","kind":"choice","width":190}),
        json!({"key":"paper","labelKey":"print.col.paper","kind":"choice","width":88}),
        json!({"key":"orientation","labelKey":"print.col.orientation","kind":"choice","width":112}),
        json!({"key":"scaling","labelKey":"print.col.scaling","kind":"choice","width":104}),
        json!({"key":"color","labelKey":"print.col.color","kind":"choice","width":110}),
        json!({"key":"duplex","labelKey":"print.col.duplex","kind":"choice","width":116}),
        json!({"key":"copies","labelKey":"print.col.copies","kind":"int","width":76}),
    ]);
    json!({
        "kind": "recordTable",
        "layout": "compact",
        "titleKey": "print.editor.title",
        "rowCountKey": "print.editor.rowCount",
        "groupField": "format",
        "selectionGroupFields": ["format"],
        "columns": columns,
        "rows": rows,
        "choices": {
            "printer": printers,
            "paper": PAPER_CHOICES,
            "orientation": ORIENTATION_CHOICES,
            "scaling": SCALING_CHOICES,
            "color": COLOR_CHOICES,
            "duplex": DUPLEX_CHOICES
        },
        "choiceLabels": {
            "paper": choice_labels("choice.print.paper", PAPER_CHOICES),
            "orientation": choice_labels("choice.print.orientation", ORIENTATION_CHOICES),
            "scaling": choice_labels("choice.print.scaling", SCALING_CHOICES),
            "color": choice_labels("choice.print.color", COLOR_CHOICES),
            "duplex": choice_labels("choice.print.duplex", DUPLEX_CHOICES)
        },
        "bulkEnabled": true,
        "selectionEnabled": true,
        "duplicateEnabled": false,
        "reorderable": reorderable
    })
}

fn workbook_safety_warning(safety: &WorkbookSafetyFacts) -> Option<&'static str> {
    if safety.has_xlm_macro_sheet {
        Some("warn.print.workbookUnsafeXlm")
    } else if safety.has_workbook_protection {
        Some("warn.print.workbookUnsafeProtection")
    } else if safety.external_link_count > 0 {
        Some("warn.print.workbookUnsafeExternalLinks")
    } else if safety.connection_count > 0 {
        Some("warn.print.workbookUnsafeConnections")
    } else if safety.query_table_count > 0 {
        Some("warn.print.workbookUnsafeQueryTables")
    } else {
        None
    }
}

fn workbook_safety_is_unsafe(safety: &WorkbookSafetyFacts) -> bool {
    workbook_safety_warning(safety).is_some()
}

fn choice_labels(prefix: &str, choices: &[&str]) -> Map<String, Value> {
    choices
        .iter()
        .map(|choice| {
            (
                (*choice).to_string(),
                Value::String(format!("{prefix}.{choice}")),
            )
        })
        .collect()
}

fn ordered_unique_files(raw: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    raw.iter()
        .filter_map(|path| {
            let absolute = absolute_path(path);
            let key = if cfg!(target_os = "windows") {
                absolute.to_string_lossy().to_lowercase()
            } else {
                absolute.to_string_lossy().into_owned()
            };
            seen.insert(key).then_some(absolute)
        })
        .collect()
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn file_format(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("file")
        .to_uppercase()
}

fn is_excel_workbook(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "xlsx" | "xlsm"))
}

fn warning_plan(key: &str, subject: &str) -> Plan {
    Plan {
        plan_id: String::new(),
        op_id: "files.batch_print".to_string(),
        steps: Vec::new(),
        warnings: vec![warning(key, subject)],
        meta: JsonObject::new(),
    }
}

fn warning(key: &str, subject: impl Into<String>) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.into(),
    }
}

fn normalized_choice(value: &str, choices: &[&str], default: &str) -> String {
    if choices.contains(&value) {
        value.to_string()
    } else {
        default.to_string()
    }
}

fn choice(row: &Map<String, Value>, key: &str, choices: &[&str], default: &str) -> String {
    normalized_choice(&string_field(row, key), choices, default)
}

fn string_field(row: &Map<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn unsigned_field(row: &Map<String, Value>, key: &str) -> Option<u64> {
    row.get(key).and_then(Value::as_u64)
}

fn string_meta(plan: &Plan, key: &str) -> String {
    plan.meta
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn copy_or_default(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    key: &str,
    default: Value,
) {
    target.insert(key.to_string(), source.get(key).cloned().unwrap_or(default));
}
