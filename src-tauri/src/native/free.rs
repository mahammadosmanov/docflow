use super::*;

pub(super) fn resolve_scoped_rfi_print_file(
    selections: &PathSelectionStore,
    selection: &PathSelection,
    caller: &CallerIdentity,
    canonical_root: &Path,
) -> Result<PathBuf, BackendError> {
    let issued_path = selections.resolve(
        selection,
        caller,
        "files.batch_print",
        "files",
        SelectionKind::File,
    )?;
    let canonical_path = if let Ok(path) = fs::canonicalize(&issued_path) {
        path
    } else {
        selections.revoke_selection(&selection.selection_id, caller);
        return Err(BackendError::validation("error.print.rfi.changed"));
    };
    if canonical_path != issued_path || !canonical_path.starts_with(canonical_root) {
        selections.revoke_selection(&selection.selection_id, caller);
        return Err(BackendError::validation("error.print.rfi.rootEscape"));
    }
    Ok(canonical_path)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathSelectionMode {
    Folder,
    File,
    Files,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecoveryPayload {
    pub(super) journals: Vec<crate::journal::RecoveryEntry>,
    pub(super) print_queue: Option<PrintQueueSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FrontendReviewItem {
    pub(super) selection_id: String,
    pub(super) name: String,
    pub(super) path: String,
    pub(super) code: String,
    pub(super) confidence: f64,
    pub(super) target_exists: bool,
}

pub(super) fn path_parameter_kind(operation_id: &str, parameter_name: &str) -> Option<ParamKind> {
    if operation_id == "rfi.review" && parameter_name == "destination"
        || operation_id == "history.undo" && parameter_name == "destination"
    {
        return Some(ParamKind::Folder);
    }
    operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == operation_id)
        .and_then(|operation| {
            operation
                .schema
                .params
                .into_iter()
                .find(|parameter| parameter.name == parameter_name)
        })
        .map(|parameter| parameter.kind)
}

pub(super) fn operation_path_parameters(
    operation_id: &str,
) -> Result<Vec<(String, ParamKind)>, BackendError> {
    let operation = operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == operation_id)
        .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
    Ok(operation
        .schema
        .params
        .into_iter()
        .filter(|parameter| {
            matches!(
                parameter.kind,
                ParamKind::Folder | ParamKind::File | ParamKind::Files
            )
        })
        .map(|parameter| (parameter.name, parameter.kind))
        .collect())
}

#[cfg(test)]
pub(super) fn filter_saved_operation_params(
    operation_id: &str,
    mut values: Map<String, Value>,
) -> Result<Map<String, Value>, BackendError> {
    let operation = operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == operation_id)
        .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
    let allowed = operation
        .schema
        .params
        .into_iter()
        .filter(|parameter| {
            parameter.remember && !matches!(parameter.kind, ParamKind::Secret | ParamKind::LongText)
        })
        .map(|parameter| parameter.name)
        .collect::<HashSet<_>>();
    values.retain(|name, _| allowed.contains(name));
    Ok(values)
}

pub(super) fn preset_store_error(error: PresetStoreError) -> BackendError {
    match error {
        PresetStoreError::InvalidName => BackendError::validation("error.preset.nameInvalid"),
        PresetStoreError::InvalidValue => BackendError::validation("preset value is invalid"),
        PresetStoreError::DuplicateName => BackendError::validation("error.preset.duplicate"),
        PresetStoreError::LimitReached => BackendError::validation("error.preset.limit"),
        PresetStoreError::Missing => BackendError::validation("error.preset.missing"),
        PresetStoreError::Write(_) => BackendError::settings_write(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyParams {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OperationIdParams {
    pub(super) op_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SaveParams {
    pub(super) op_id: String,
    pub(super) values: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SavePresetParams {
    pub(super) op_id: String,
    pub(super) name: String,
    pub(super) values: JsonObject,
    #[serde(default)]
    pub(super) replace: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeletePresetParams {
    pub(super) op_id: String,
    pub(super) preset_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SignInParams {
    pub(super) identifier: String,
    pub(super) password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SignUpParams {
    pub(super) email: String,
    pub(super) password: String,
    pub(super) username: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UsernameParams {
    pub(super) username: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SetFlagsParams {
    pub(super) user_id: String,
    #[serde(default)]
    pub(super) allowed: Option<bool>,
    #[serde(default)]
    pub(super) is_admin: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryParams {
    #[serde(default)]
    pub(super) limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct QueueIdParams {
    pub(super) queue_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PreviewEnvelope<T> {
    pub(super) op_id: String,
    pub(super) values: T,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessScansValues {
    pub(super) source: std::path::PathBuf,
    pub(super) dest: std::path::PathBuf,
    #[serde(default)]
    pub(super) scan_mode: ScanMode,
    #[serde(default = "default_true")]
    pub(super) organize_into_rfi_folders: bool,
    #[serde(default)]
    pub(super) keep_original: bool,
    #[serde(default = "default_true")]
    pub(super) correct_orientation: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RfiFileGroupValues {
    pub(super) source: PathBuf,
    pub(super) destination: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RfiFollowUpPreviewValues {
    #[serde(rename = "rfi_log_workbook")]
    pub(super) rfi_log_workbook: PathBuf,
    #[serde(rename = "follow_up_workbook")]
    pub(super) follow_up_workbook: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RfiSystemSyncPreviewValues {
    #[serde(rename = "rfi_system_root")]
    pub(super) rfi_system_root: PathBuf,
    #[serde(rename = "rfi_log_workbook")]
    pub(super) rfi_log_workbook: PathBuf,
    #[serde(rename = "follow_up_workbook")]
    pub(super) follow_up_workbook: PathBuf,
    #[serde(default)]
    pub(super) force_rescan: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RfiPublishStatusPreviewValues {
    #[serde(rename = "staging_folder")]
    pub(super) staging_folder: PathBuf,
    #[serde(rename = "rfi_log_workbook")]
    pub(super) rfi_log_workbook: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RfiCompleteStatusPreviewValues {
    #[serde(rename = "rfi_system_root")]
    pub(super) rfi_system_root: PathBuf,
    #[serde(rename = "rfi_log_workbook")]
    pub(super) rfi_log_workbook: PathBuf,
}

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(deny_unknown_fields)]
pub(super) struct OutlookReplyDraftValues {
    pub(super) reply_text: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(deny_unknown_fields)]
pub(super) struct OutlookArchiveValues {
    pub(super) destination: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StageSignedPagesValues {
    pub(super) signed_scans: PathBuf,
    pub(super) archive_root: PathBuf,
    pub(super) review_root: PathBuf,
    #[serde(default)]
    pub(super) scan_subfolders: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PdfOrientationValues {
    pub(super) rfi_system_root: PathBuf,
    pub(super) review_root: PathBuf,
    #[serde(default)]
    pub(super) force_rescan: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReviseParams {
    pub(super) plan_id: String,
    pub(super) rows: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TableParams {
    pub(super) plan_id: String,
    #[serde(default)]
    pub(super) sheet: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RfiReplyPreviewParams {
    pub(super) plan_id: String,
    pub(super) journal_id: String,
}

pub(super) fn committed_rfi_reply_matches(
    journal: &crate::journal::JournalRecord,
    caller: &CallerIdentity,
    plan_digest: &str,
) -> bool {
    journal.op_id == "rfi.log_add"
        && journal.state == Some(crate::journal::JournalState::Committed)
        && journal.owner_id.as_deref() == Some(caller.owner_id.as_str())
        && journal.session_id.as_deref() == Some(caller.session_id.as_str())
        && journal.plan_digest.as_deref() == Some(plan_digest)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewScanParams {
    pub(super) destination: PathSelection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewPreviewParams {
    pub(super) destination: PathSelection,
    pub(super) decisions: Vec<FrontendReviewDecision>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FrontendReviewDecision {
    pub(super) item_id: String,
    pub(super) action: crate::review::ReviewAction,
    pub(super) code: String,
}

pub(super) const fn default_true() -> bool {
    true
}

pub(super) const fn process_scans_source_access() -> GrantAccess {
    // Move mode removes every classified or review-bound source; copy mode
    // preserves every source, including unreadable and low-confidence files.
    GrantAccess::ReadWrite
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SettingsUpdate {
    #[serde(default)]
    pub(super) locale: PatchField<Option<String>>,
    #[serde(default)]
    pub(super) theme: Option<String>,
    #[serde(default)]
    pub(super) font: Option<String>,
    #[serde(default)]
    pub(super) font_scale: Option<f64>,
    #[serde(default)]
    pub(super) compact: Option<bool>,
    #[serde(default)]
    pub(super) start_with_windows: Option<bool>,
    #[serde(default)]
    pub(super) replica_sync_enabled: Option<bool>,
}

#[derive(Debug, Default, PartialEq)]
pub(super) enum PatchField<T> {
    #[default]
    Missing,
    Value(T),
}

impl<'de, T> Deserialize<'de> for PatchField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Self::Value)
    }
}

pub(super) fn decode<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, BackendError> {
    serde_json::from_value(params).map_err(|_| BackendError::invalid_params())
}

pub(super) fn decode_empty(params: Value) -> Result<(), BackendError> {
    decode::<EmptyParams>(params).map(|_| ())
}

pub(super) fn encode(value: impl serde::Serialize) -> Result<Value, BackendError> {
    serde_json::to_value(value).map_err(|_| BackendError::protocol("response encoding failed"))
}

pub(super) fn validate_operation_id(op_id: &str) -> Result<(), BackendError> {
    if op_id.is_empty() || op_id.chars().count() > MAX_OPERATION_ID_CHARS {
        return Err(BackendError::validation("operation ID is invalid"));
    }
    Ok(())
}

pub(super) fn configured_gemini_models() -> Vec<String> {
    let configured = env::var("DOCFLOW_GEMINI_MODEL").unwrap_or_default();
    let mut models = [configured.as_str(), DEFAULT_MODEL]
        .into_iter()
        .chain(FALLBACK_MODELS.iter().copied())
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    models.retain(|model| seen.insert(model.clone()));
    models
}

pub(super) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

pub(super) fn authenticated_caller_identity(
    state: &crate::contracts::AuthState,
    session_id: &str,
) -> Result<CallerIdentity, BackendError> {
    let caller = signed_in_caller_identity(state, session_id)?;
    if !state.allowed {
        return Err(BackendError::permission_denied());
    }
    Ok(caller)
}

pub(super) fn signed_in_caller_identity(
    state: &crate::contracts::AuthState,
    session_id: &str,
) -> Result<CallerIdentity, BackendError> {
    if !state.signed_in {
        return Err(BackendError::auth_required());
    }
    let owner_id = state
        .session
        .as_ref()
        .map(|session| session.user_id.as_str())
        .filter(|owner_id| !owner_id.is_empty() && *owner_id != "anonymous")
        .ok_or_else(BackendError::auth_required)?;
    if session_id.is_empty() {
        return Err(BackendError::auth_required());
    }
    Ok(CallerIdentity::new(owner_id, session_id))
}

pub(super) fn cloud_result_summary(applied: u64, journal_step_count: usize) -> String {
    format!("{applied}/{journal_step_count}")
}

pub(super) fn sync_result_metadata<L, P>(result: &mut Value, mut load_steps: L, mut push: P)
where
    L: FnMut(&str) -> Option<usize>,
    P: FnMut(u64, u64, &str, &str) -> bool,
{
    let Some(payload) = result.as_object_mut() else {
        return;
    };
    let applied = payload
        .get("applied")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let failed = payload
        .get("failed")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let journal_id = payload
        .get("journalId")
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("meta")
                .and_then(Value::as_object)
                .and_then(|meta| meta.get("auditJournalId"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string();
    let Some(step_count) = load_steps(&journal_id) else {
        payload.insert("cloudSynced".to_string(), Value::Bool(false));
        return;
    };
    let summary = cloud_result_summary(applied, step_count);
    let synced = push(applied, failed, &journal_id, &summary);
    payload.insert("cloudSynced".to_string(), Value::Bool(synced));
}

/// Keeps the stable localized key and appends the OS cause (no path), so a
/// remote-share failure is distinguishable from a plain sharing violation.
pub(super) fn open_failure(key: &str, error: &std::io::Error) -> BackendError {
    BackendError::validation(format!("{key}: {error}"))
}

#[cfg(test)]
mod open_failure_tests {
    use super::*;

    #[test]
    fn open_failure_keeps_key_and_os_cause() {
        let error = std::io::Error::from_raw_os_error(5);
        let failure = open_failure("error.rfi.followUp.workbookLocked", &error);
        assert!(
            failure
                .message
                .starts_with("error.rfi.followUp.workbookLocked: ")
        );
        assert!(failure.message.contains("os error 5"));
    }
}
