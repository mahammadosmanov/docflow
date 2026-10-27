use super::*;

pub(super) fn plan_from_rows(
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
                "pageRange",
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

pub(super) fn jobs_from_rows(rows: &[Map<String, Value>], collate: bool) -> Vec<PrintJob> {
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

pub(super) fn job_from_row(row: &Map<String, Value>, copy: u32) -> PrintJob {
    let row_id = row
        .get("_rowId")
        .and_then(Value::as_str)
        .map_or_else(|| Uuid::new_v4().simple().to_string(), str::to_string);
    let job_id = row
        .get("_resumeJobId")
        .and_then(Value::as_str)
        .map_or_else(|| format!("{row_id}-c{copy}"), str::to_string);
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
        page_range: string_field(row, "pageRange"),
        timeout_minutes: unsigned_field(row, "timeoutMinutes").unwrap_or(30) as u32,
        retries: unsigned_field(row, "retries").unwrap_or_default() as u32,
        on_error: string_field(row, "onError"),
        copy,
        signature: row
            .get("signature")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default(),
        worksheet: row
            .get("worksheet")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
        workbook_safety: row
            .get("workbookSafety")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
        status: None,
        attempts: None,
        error: None,
        completed_at: None,
        cleanup_review: None,
        extra: Map::new(),
    }
}

pub(super) fn insert_print_settings(row: &mut Map<String, Value>, settings: &NormalizedSettings) {
    row.insert("printer".to_string(), json!(settings.printer));
    row.insert("paper".to_string(), json!(settings.paper));
    row.insert("orientation".to_string(), json!(settings.orientation));
    row.insert("scaling".to_string(), json!(settings.scaling));
    row.insert("color".to_string(), json!(settings.color));
    row.insert("duplex".to_string(), json!(settings.duplex));
    row.insert("pageRange".to_string(), json!("all"));
    row.insert("copies".to_string(), json!(settings.copies));
    row.insert("retries".to_string(), json!(settings.retries));
    row.insert("onError".to_string(), json!(settings.on_error));
    row.insert(
        "timeoutMinutes".to_string(),
        json!(settings.timeout_minutes),
    );
}

pub(super) fn row_from_file(
    path: &Path,
    index: usize,
    settings: &NormalizedSettings,
    support: &PrintJobSupport,
) -> Result<Map<String, Value>, BackendError> {
    let mut file =
        File::open(path).map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
    let mut row = Map::new();
    row.insert("_rowId".to_string(), json!(format!("file-{}", index + 1)));
    row.insert("enabled".to_string(), json!(true));
    row.insert("status".to_string(), json!("ready"));
    row.insert("file".to_string(), json!(display_name(path)));
    row.insert("path".to_string(), json!(path));
    row.insert("format".to_string(), json!(file_format(path)));
    insert_print_settings(&mut row, settings);
    row.insert("verb".to_string(), json!(support.verb));
    row.insert(
        "signature".to_string(),
        json!(file_signature_from_open_file(&mut file, false)?),
    );
    Ok(row)
}

pub(super) fn rows_from_workbook(
    path: &Path,
    settings: &NormalizedSettings,
    mut file: File,
) -> Result<Vec<Map<String, Value>>, &'static str> {
    let signature = file_signature_from_open_file(&mut file, true)
        .map_err(|_| "warn.print.workbookDiscoveryFailed")?;
    let discovery = ordered_workbook_discovery(&mut file).map_err(|error| {
        if error.to_string() == "workbook has too many worksheets" {
            "warn.print.workbookTooManyWorksheets"
        } else {
            "warn.print.workbookDiscoveryFailed"
        }
    })?;
    if let Some(warning_key) = workbook_safety_warning(&discovery.safety) {
        return Err(warning_key);
    }
    Ok(discovery
        .worksheets
        .into_iter()
        .map(|worksheet| {
            row_from_worksheet(path, settings, &signature, &discovery.safety, worksheet)
        })
        .collect())
}

pub(super) fn row_from_worksheet(
    path: &Path,
    settings: &NormalizedSettings,
    signature: &FileSignature,
    safety: &WorkbookSafetyFacts,
    worksheet: WorksheetDescriptor,
) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("_rowId".to_string(), json!(Uuid::new_v4().to_string()));
    row.insert(
        "enabled".to_string(),
        json!(worksheet.visibility == WorksheetVisibility::Visible),
    );
    row.insert("status".to_string(), json!("ready"));
    row.insert("file".to_string(), json!(display_name(path)));
    row.insert("path".to_string(), json!(path));
    row.insert("format".to_string(), json!(file_format(path)));
    row.insert("worksheetName".to_string(), json!(worksheet.name));
    row.insert(
        "worksheetVisibility".to_string(),
        json!(worksheet.visibility),
    );
    row.insert("worksheet".to_string(), json!(worksheet));
    row.insert("signature".to_string(), json!(signature));
    row.insert("workbookSafety".to_string(), json!(safety));
    insert_print_settings(&mut row, settings);
    row.insert("verb".to_string(), json!("docflow"));
    row
}

pub(super) struct NormalizedSettings {
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
    pub(super) fn from_params(params: &BatchPrintParams) -> Self {
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

    pub(super) fn print_settings(&self) -> PrintSettings {
        PrintSettings {
            paper: self.paper.clone(),
            orientation: self.orientation.clone(),
            scaling: self.scaling.clone(),
            color: self.color.clone(),
            duplex: self.duplex.clone(),
            page_range: "all".to_string(),
            timeout_minutes: self.timeout_minutes,
        }
    }
}
