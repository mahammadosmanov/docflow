use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    print_queue::{
        BatchPrintParams, PrintAdapter, PrintJob, PrintJobSupport, PrintSettings, PrinterInventory,
        file_signature,
    },
};

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
    mut progress: P,
) -> Result<Plan, BackendError>
where
    A: PrintAdapter + ?Sized,
    P: FnMut(ProgressEvent),
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
            phase: "preview".to_string(),
            done: index as u64,
            total: files.len() as u64,
            subject: String::new(),
        });
        if !regular_file(path) {
            warnings.push(warning("warn.print.fileMissing", display_name(path)));
            continue;
        }
        match adapter.inspect(path, &params.printer, &normalized.print_settings()) {
            Ok(support) => {
                if support.settings_limited {
                    warnings.push(warning("warn.print.settingsLimited", display_name(path)));
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
        phase: "preview".to_string(),
        done: files.len() as u64,
        total: files.len() as u64,
        subject: String::new(),
    });
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
        let printer = string_field(raw, "printer");
        if !inventory.contains(&printer) {
            return Err(BackendError::validation("printer is unavailable"));
        }
        let copies = unsigned_field(raw, "copies").unwrap_or_default();
        if !(1..=u64::from(MAX_COPIES)).contains(&copies) {
            return Err(BackendError::validation("copies must be between 1 and 99"));
        }
        let path = PathBuf::from(string_field(original, "path"));
        let settings = PrintSettings {
            paper: choice(raw, "paper", PAPER_CHOICES, "A4"),
            orientation: choice(raw, "orientation", ORIENTATION_CHOICES, "auto"),
            scaling: choice(raw, "scaling", SCALING_CHOICES, "fit"),
            color: choice(raw, "color", COLOR_CHOICES, "printer"),
            duplex: choice(raw, "duplex", DUPLEX_CHOICES, "printer"),
            timeout_minutes: unsigned_field(original, "timeoutMinutes").unwrap_or(30) as u32,
        };
        let support = adapter
            .inspect(&path, &printer, &settings)
            .map_err(|error| error.into_backend_error())?;
        let mut row = original.clone();
        row.insert(
            "enabled".to_string(),
            Value::Bool(raw.get("enabled").and_then(Value::as_bool).unwrap_or(true)),
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
        rows.push(row);
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
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("job-{}", index + 1))
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
                    .map(str::to_string)
                    .unwrap_or_else(|| file_format(&path))
            ),
        );
        copy_or_default(&mut row, saved, "printer", json!(""));
        copy_or_default(&mut row, saved, "paper", json!("A4"));
        copy_or_default(&mut row, saved, "orientation", json!("auto"));
        copy_or_default(&mut row, saved, "scaling", json!("fit"));
        copy_or_default(&mut row, saved, "color", json!("printer"));
        copy_or_default(&mut row, saved, "duplex", json!("printer"));
        row.insert("copies".to_string(), json!(1));
        copy_or_default(&mut row, saved, "retries", json!(0));
        copy_or_default(&mut row, saved, "onError", json!("pause"));
        copy_or_default(&mut row, saved, "timeoutMinutes", json!(30));
        copy_or_default(&mut row, saved, "verb", json!(""));
        copy_or_default(&mut row, saved, "signature", json!({}));
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
    Ok(())
}

fn plan_from_rows(
    rows: Vec<Map<String, Value>>,
    inventory: &PrinterInventory,
    collate: bool,
    queue_id: String,
    resume_queue_id: String,
    mut warnings: Vec<Warning>,
) -> Plan {
    let jobs = jobs_from_rows(&rows, collate);
    let steps = rows
        .iter()
        .filter(|row| row.get("enabled").and_then(Value::as_bool).unwrap_or(true))
        .map(|row| {
            let path = string_field(row, "path");
            let mut meta = JsonObject::new();
            for key in [
                "printer",
                "verb",
                "format",
                "copies",
                "paper",
                "orientation",
                "scaling",
                "color",
                "duplex",
            ] {
                if let Some(value) = row.get(key) {
                    meta.insert(key.to_string(), value.clone());
                }
            }
            Step {
                kind: StepKind::Print,
                src: Some(path.clone()),
                dst: Some(path),
                note_key: Some("note.print.queued".to_string()),
                meta,
            }
        })
        .collect::<Vec<_>>();
    if !steps.is_empty()
        && !warnings
            .iter()
            .any(|warning| warning.key == "warn.print.nonUndoable")
    {
        warnings.push(warning("warn.print.nonUndoable", ""));
    }
    if steps.is_empty() {
        warnings.push(warning("warn.print.noJobsSelected", ""));
    }
    let mut meta = JsonObject::new();
    meta.insert("queueId".to_string(), json!(queue_id));
    meta.insert("resumeQueueId".to_string(), json!(resume_queue_id));
    meta.insert("collate".to_string(), json!(collate));
    meta.insert("jobs".to_string(), json!(jobs));
    meta.insert("fileCount".to_string(), json!(steps.len()));
    meta.insert("jobCount".to_string(), json!(jobs.len()));
    meta.insert("nonUndoable".to_string(), json!(true));
    meta.insert(
        "editor".to_string(),
        editor(rows, inventory.printers.clone()),
    );
    Plan {
        plan_id: String::new(),
        op_id: "files.batch_print".to_string(),
        steps,
        warnings,
        meta,
    }
}

fn jobs_from_rows(rows: &[Map<String, Value>], collate: bool) -> Vec<PrintJob> {
    let selected = rows
        .iter()
        .filter(|row| row.get("enabled").and_then(Value::as_bool).unwrap_or(true))
        .collect::<Vec<_>>();
    let physical = |row: &Map<String, Value>, copy: u32| job_from_row(row, copy);
    let mut jobs = Vec::new();
    if collate {
        let max_copies = selected
            .iter()
            .filter_map(|row| unsigned_field(row, "copies"))
            .max()
            .unwrap_or_default() as u32;
        for copy in 1..=max_copies {
            for row in &selected {
                if unsigned_field(row, "copies").unwrap_or_default() >= u64::from(copy) {
                    jobs.push(physical(row, copy));
                }
            }
        }
    } else {
        for row in selected {
            let copies = unsigned_field(row, "copies").unwrap_or_default() as u32;
            for copy in 1..=copies {
                jobs.push(physical(row, copy));
            }
        }
    }
    jobs
}

fn job_from_row(row: &Map<String, Value>, copy: u32) -> PrintJob {
    let row_id = row
        .get("_rowId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
    let job_id = row
        .get("_resumeJobId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{row_id}-c{copy}"));
    PrintJob {
        job_id,
        path: string_field(row, "path"),
        printer: string_field(row, "printer"),
        verb: string_field(row, "verb"),
        format: string_field(row, "format"),
        paper: string_field(row, "paper"),
        orientation: string_field(row, "orientation"),
        scaling: string_field(row, "scaling"),
        color: string_field(row, "color"),
        duplex: string_field(row, "duplex"),
        timeout_minutes: unsigned_field(row, "timeoutMinutes").unwrap_or(30) as u32,
        retries: unsigned_field(row, "retries").unwrap_or_default() as u32,
        on_error: string_field(row, "onError"),
        copy,
        signature: row
            .get("signature")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        status: None,
        attempts: None,
        error: None,
        completed_at: None,
        extra: Map::new(),
    }
}

fn row_from_file(
    path: &Path,
    index: usize,
    settings: &NormalizedSettings,
    support: &PrintJobSupport,
) -> Result<Map<String, Value>, BackendError> {
    let mut row = Map::new();
    row.insert("_rowId".to_string(), json!(format!("file-{}", index + 1)));
    row.insert("enabled".to_string(), json!(true));
    row.insert("status".to_string(), json!("ready"));
    row.insert("file".to_string(), json!(display_name(path)));
    row.insert("path".to_string(), json!(path));
    row.insert("format".to_string(), json!(file_format(path)));
    row.insert("printer".to_string(), json!(settings.printer));
    row.insert("paper".to_string(), json!(settings.paper));
    row.insert("orientation".to_string(), json!(settings.orientation));
    row.insert("scaling".to_string(), json!(settings.scaling));
    row.insert("color".to_string(), json!(settings.color));
    row.insert("duplex".to_string(), json!(settings.duplex));
    row.insert("copies".to_string(), json!(settings.copies));
    row.insert("retries".to_string(), json!(settings.retries));
    row.insert("onError".to_string(), json!(settings.on_error));
    row.insert(
        "timeoutMinutes".to_string(),
        json!(settings.timeout_minutes),
    );
    row.insert("verb".to_string(), json!(support.verb));
    row.insert(
        "signature".to_string(),
        Value::Object(file_signature(path)?),
    );
    Ok(row)
}

struct NormalizedSettings {
    printer: String,
    paper: String,
    orientation: String,
    scaling: String,
    color: String,
    duplex: String,
    copies: u32,
    retries: u32,
    on_error: String,
    timeout_minutes: u32,
}

impl NormalizedSettings {
    fn from_params(params: &BatchPrintParams) -> Self {
        Self {
            printer: params.printer.clone(),
            paper: normalized_choice(&params.paper, PAPER_CHOICES, "A4"),
            orientation: normalized_choice(&params.orientation, ORIENTATION_CHOICES, "auto"),
            scaling: normalized_choice(&params.scaling, SCALING_CHOICES, "fit"),
            color: normalized_choice(&params.color, COLOR_CHOICES, "printer"),
            duplex: normalized_choice(&params.duplex, DUPLEX_CHOICES, "printer"),
            copies: params.copies,
            retries: params.retries,
            on_error: normalized_choice(&params.on_error, ERROR_ACTION_CHOICES, "pause"),
            timeout_minutes: params.timeout_minutes,
        }
    }

    fn print_settings(&self) -> PrintSettings {
        PrintSettings {
            paper: self.paper.clone(),
            orientation: self.orientation.clone(),
            scaling: self.scaling.clone(),
            color: self.color.clone(),
            duplex: self.duplex.clone(),
            timeout_minutes: self.timeout_minutes,
        }
    }
}

fn editor(rows: Vec<Map<String, Value>>, printers: Vec<String>) -> Value {
    json!({
        "kind": "recordTable",
        "layout": "compact",
        "titleKey": "print.editor.title",
        "rowCountKey": "print.editor.rowCount",
        "groupField": "format",
        "selectionGroupFields": ["format"],
        "columns": [
            {"key":"enabled","labelKey":"print.col.include","kind":"bool","width":64},
            {"key":"status","labelKey":"print.col.status","kind":"readonly","width":90,"bulk":false},
            {"key":"file","labelKey":"print.col.file","kind":"readonly","width":260,"bulk":false},
            {"key":"format","labelKey":"print.col.format","kind":"readonly","width":76,"bulk":false},
            {"key":"printer","labelKey":"print.col.printer","kind":"choice","width":190},
            {"key":"paper","labelKey":"print.col.paper","kind":"choice","width":88},
            {"key":"orientation","labelKey":"print.col.orientation","kind":"choice","width":112},
            {"key":"scaling","labelKey":"print.col.scaling","kind":"choice","width":104},
            {"key":"color","labelKey":"print.col.color","kind":"choice","width":110},
            {"key":"duplex","labelKey":"print.col.duplex","kind":"choice","width":116},
            {"key":"copies","labelKey":"print.col.copies","kind":"int","width":76}
        ],
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
        "duplicateEnabled": false
    })
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

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
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
