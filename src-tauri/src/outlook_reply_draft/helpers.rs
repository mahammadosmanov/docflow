use super::*;

pub(crate) fn journal_is_safely_undoable(record: &JournalRecord) -> bool {
    let caller = CallerIdentity::new(
        record.owner_id.clone().unwrap_or_default(),
        record.session_id.clone().unwrap_or_default(),
    );
    validate_undo_record(record, &caller).is_ok()
        && durable_identity_from_step(&record.steps[0]).is_ok()
}

pub(super) fn validate_selected_mail_item(selected: &SelectedMailItem) -> Result<(), BackendError> {
    if !bounded_identifier(&selected.identity.entry_id)
        || !bounded_identifier(&selected.identity.store_id)
        || selected.fingerprint.len() != 64
        || !selected
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(BackendError::validation(
            "error.rfi.outlookDraft.selectionInvalid",
        ));
    }
    if selected.subject.chars().count() > MAX_SUBJECT_CHARS
        || selected.subject.chars().any(char::is_control)
    {
        return Err(BackendError::validation(
            "error.rfi.outlookDraft.subjectTooLong",
        ));
    }
    Ok(())
}

pub(super) fn validate_undo_record(
    record: &JournalRecord,
    caller: &CallerIdentity,
) -> Result<(), BackendError> {
    if record.op_id != OPERATION_ID
        || record.state != Some(JournalState::Committed)
        || record.pending_step.is_some()
        || record.pending_undo.is_some()
        || record.undo_recovery.is_some()
        || record.steps.len() != 1
        || record.steps[0].kind != StepKind::OutlookReplyDraft
        || !record.undone_steps.is_empty()
    {
        return Err(BackendError::journal_invalid(
            "Outlook draft journal is not eligible for exact undo",
        ));
    }
    if record.owner_id.as_deref() != Some(caller.owner_id.as_str()) {
        return Err(BackendError::journal_owner_mismatch());
    }
    Ok(())
}

pub(super) fn durable_identity_from_step(step: &Step) -> Result<DraftIdentity, BackendError> {
    if step.meta.get("draftDurable") != Some(&Value::Bool(true)) {
        return Err(BackendError::validation(
            "error.rfi.outlookDraft.undoUnavailable",
        ));
    }
    let field = |name: &str| {
        step.meta
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| bounded_identifier(value))
            .map(str::to_string)
            .ok_or_else(|| BackendError::journal_invalid("Outlook draft identity is invalid"))
    };
    Ok(DraftIdentity {
        entry_id: field("draftEntryId")?,
        store_id: field("draftStoreId")?,
        fingerprint: field("draftFingerprint")?,
    })
}

pub(super) fn bounded_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_OUTLOOK_ID_CHARS
        && value.chars().all(|character| !character.is_control())
}

pub(super) fn operation_result(journal_id: &str, applied: u64, failed: u64) -> OperationResult {
    let mut meta = JsonObject::new();
    meta.insert(
        "effectState".to_string(),
        Value::String(if failed == 0 {
            "committed".to_string()
        } else {
            "no_effect_retryable".to_string()
        }),
    );
    OperationResult {
        applied,
        failed,
        journal_id: journal_id.to_string(),
        errors: Vec::new(),
        meta,
        cloud_synced: None,
    }
}

pub(super) fn outlook_validation(failure: OutlookFailure) -> BackendError {
    BackendError::validation(failure.key)
}

pub(super) fn json_object(value: Value) -> Result<JsonObject, BackendError> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| BackendError::protocol("Outlook plan metadata is invalid"))
}

pub(super) fn lock_authorities(
    authorities: &Mutex<HashMap<String, OutlookPlanAuthority>>,
) -> MutexGuard<'_, HashMap<String, OutlookPlanAuthority>> {
    authorities
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn lock_archive_authorities(
    authorities: &Mutex<HashMap<String, OutlookArchiveAuthority>>,
) -> MutexGuard<'_, HashMap<String, OutlookArchiveAuthority>> {
    authorities
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
