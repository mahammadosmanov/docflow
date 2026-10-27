use super::*;

pub(super) fn journal_effect_state(state: Option<JournalState>) -> &'static str {
    match state {
        Some(JournalState::Committed | JournalState::Undone) => "committed",
        Some(JournalState::PartialCommitted | JournalState::UndoPartial) => "partial_committed",
        Some(JournalState::NoEffectRetryable) => "no_effect_retryable",
        Some(JournalState::CancelledClean) => "cancelled_clean",
        Some(JournalState::CancelledPartial) => "cancelled_partial",
        Some(JournalState::RecoveryRequired | JournalState::Applying) | None => "outcome_unknown",
    }
}

pub(super) fn persisted_source_reselection_required() -> BackendError {
    BackendError::validation("error.print.queueSourceReselectionRequired")
}

pub(super) fn validate_verified_resume_plan_sources(
    plan: &Plan,
    verified_sources: &[VerifiedPersistedSource],
) -> Result<(), BackendError> {
    let verified_paths = verified_sources
        .iter()
        .map(|source| source.path.as_path())
        .collect::<HashSet<_>>();
    if verified_paths.is_empty() {
        return Err(persisted_source_reselection_required());
    }
    let jobs = plan
        .meta
        .get("jobs")
        .and_then(Value::as_array)
        .ok_or_else(persisted_source_reselection_required)?;
    if jobs.iter().any(|job| {
        job.get("path")
            .and_then(Value::as_str)
            .is_none_or(|path| !verified_paths.contains(Path::new(path)))
    }) {
        return Err(persisted_source_reselection_required());
    }
    let rows = plan
        .meta
        .get("editor")
        .and_then(Value::as_object)
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(persisted_source_reselection_required)?;
    let row_paths = rows
        .iter()
        .map(|row| {
            row.get("path")
                .and_then(Value::as_str)
                .map(Path::new)
                .filter(|path| verified_paths.contains(path))
                .ok_or_else(persisted_source_reselection_required)
        })
        .collect::<Result<HashSet<_>, _>>()?;
    if row_paths != verified_paths {
        return Err(persisted_source_reselection_required());
    }
    Ok(())
}

pub(super) struct PrintCancellationRegistration<'a> {
    pub(super) registry: &'a CancellationRegistry,
    pub(super) plan_id: &'a str,
}

impl Drop for PrintCancellationRegistration<'_> {
    fn drop(&mut self) {
        self.registry.finish(self.plan_id);
    }
}

pub(super) struct PrintQueueActiveRegistration<'a> {
    pub(super) store: &'a PrintQueueStore,
    pub(super) queue_id: String,
}

impl Drop for PrintQueueActiveRegistration<'_> {
    fn drop(&mut self) {
        self.store.finish(&self.queue_id);
    }
}

pub fn file_signature(path: &Path) -> Result<FileSignature, BackendError> {
    let mut file =
        File::open(path).map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
    file_signature_from_open_file(&mut file, false)
}

pub fn file_signature_from_open_file(
    file: &mut File,
    include_sha256: bool,
) -> Result<FileSignature, BackendError> {
    let metadata = file
        .metadata()
        .map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
    let modified = metadata
        .modified()
        .map_err(|_| BackendError::path_invalid("print file timestamp is unavailable"))?;
    let mtime_ns = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| BackendError::path_invalid("print file timestamp is unavailable"))?
        .as_nanos();
    let mtime_ns = u64::try_from(mtime_ns)
        .map_err(|_| BackendError::path_invalid("print file timestamp is unavailable"))?;
    let sha256 = if include_sha256 {
        file.seek(SeekFrom::Start(0))
            .map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
        let mut digest = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        Some(format!("{:x}", digest.finalize()))
    } else {
        None
    };
    file.seek(SeekFrom::Start(0))
        .map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
    Ok(FileSignature {
        size: metadata.len(),
        mtime_ns,
        sha256,
    })
}

pub(super) fn workbook_safety_is_unsafe(safety: &WorkbookSafetyFacts) -> bool {
    safety.has_xlm_macro_sheet
        || safety.has_workbook_protection
        || safety.external_link_count > 0
        || safety.connection_count > 0
        || safety.query_table_count > 0
}

pub fn default_queue_root(app_data_dir: &Path) -> PathBuf {
    std::env::var_os("DOCFLOW_PRINT_QUEUE_DIR")
        .filter(|value| !value.is_empty())
        .map_or_else(|| app_data_dir.join("print-queues"), PathBuf::from)
}

pub(super) fn valid_printer_name(printer: &str) -> bool {
    !printer.is_empty()
        && printer.chars().count() <= 256
        && !printer
            .chars()
            .any(|character| character.is_control() || character == '\0' || character == '"')
}

pub(super) fn validate_queue_id(queue_id: &str) -> Result<(), BackendError> {
    if queue_id.is_empty()
        || queue_id.len() > 128
        || !queue_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(BackendError::validation("invalid print queue id"));
    }
    Ok(())
}

pub(super) fn queue_counts(jobs: &[PrintJob]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::from([
        ("pending".to_string(), 0),
        ("printing".to_string(), 0),
        ("completed".to_string(), 0),
        ("completed_unconfirmed".to_string(), 0),
        ("failed".to_string(), 0),
        ("cancelled".to_string(), 0),
        ("uncertain".to_string(), 0),
    ]);
    for job in jobs {
        if let Some(count) = counts.get_mut(job.status.as_deref().unwrap_or("pending")) {
            *count += 1;
        }
    }
    counts
}

pub(super) fn journal_step(job: &PrintJob) -> Step {
    let mut meta = serde_json::to_value(job)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    meta.remove("path");
    Step {
        kind: StepKind::Print,
        src: Some(job.path.clone()),
        dst: Some(job.path.clone()),
        note_key: Some("note.print.queued".to_string()),
        meta,
    }
}

pub(super) fn queue_diagnostic(state: QueueState) -> PrintQueueDiagnostic {
    const MAX_DIAGNOSTIC_JOBS: usize = 64;
    let counts = queue_counts(&state.jobs);
    let file_count = state
        .jobs
        .iter()
        .filter(|job| !job.path.is_empty())
        .map(|job| &job.path)
        .collect::<HashSet<_>>()
        .len() as u64;
    let printer = state
        .jobs
        .iter()
        .map(|job| job.printer.trim())
        .find(|printer| !printer.is_empty())
        .unwrap_or("unknown")
        .chars()
        .take(512)
        .collect();
    let first = state.jobs.len().saturating_sub(MAX_DIAGNOSTIC_JOBS);
    let jobs = state.jobs[first..]
        .iter()
        .map(|job| PrintJobDiagnostic {
            format: job.format.clone(),
            route: job.verb.clone(),
            status: job.status.clone().unwrap_or_else(|| "pending".to_string()),
            attempts: job.attempts.unwrap_or_default(),
            paper: job.paper.clone(),
            orientation: job.orientation.clone(),
            scaling: job.scaling.clone(),
            color: job.color.clone(),
            duplex: job.duplex.clone(),
            page_range: job.page_range.clone(),
            error_detail: job.error.as_deref().map(redact_error).unwrap_or_default(),
        })
        .collect();
    PrintQueueDiagnostic {
        queue_status: state.status,
        printer,
        file_count,
        counts,
        jobs,
    }
}

pub(super) fn utc_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

pub(super) fn display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

pub(super) fn warning(key: &str, subject: impl Into<String>) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.into(),
    }
}

pub(super) fn is_localized_print_error(message: &str) -> bool {
    message.strip_prefix("error.print.").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.len() <= 64
            && suffix
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '.')
    })
}

pub(super) fn redact_error(message: &str) -> String {
    let single_line = message
        .chars()
        .filter(|character| !matches!(character, '\r' | '\n'))
        .take(512)
        .collect::<String>();
    if single_line.is_empty() {
        "print adapter failed".to_string()
    } else {
        single_line
    }
}

pub(super) fn failure_evidence(error: &PrintFailure) -> String {
    error
        .diagnostic_detail
        .as_deref()
        .map_or_else(|| redact_error(&error.message), redact_error)
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
