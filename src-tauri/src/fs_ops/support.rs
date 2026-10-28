use super::*;

mod atomic;
pub(crate) use atomic::*;

pub(super) enum FollowUpApplyFailure {
    NoEffect(Warning),
    Partial(Warning),
    OutcomeUnknown(Warning),
    Cancelled,
}

pub(super) fn follow_up_warning(key: &str, subject: impl Into<String>) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.into(),
    }
}

pub(super) fn follow_up_no_effect(key: &str) -> FollowUpApplyFailure {
    FollowUpApplyFailure::NoEffect(follow_up_warning(key, String::new()))
}

pub(super) fn follow_up_partial(key: &str) -> FollowUpApplyFailure {
    FollowUpApplyFailure::Partial(follow_up_warning(key, String::new()))
}

pub(super) fn follow_up_backend_no_effect(error: BackendError) -> FollowUpApplyFailure {
    follow_up_no_effect(&error.message)
}

pub(super) fn follow_up_backend_partial(error: BackendError) -> FollowUpApplyFailure {
    follow_up_partial(&error.message)
}

pub(super) fn follow_up_outcome(record: &ValidatedS5Record) -> FollowUpApplyFailure {
    let locator = record.retained_locators().ok().map_or_else(
        || format!("journal:{};root:0;leaf:record", record.journal_id()),
        |locators| locators.first().to_string(),
    );
    FollowUpApplyFailure::OutcomeUnknown(follow_up_warning(
        "error.rfi.followUp.recoveryRequired",
        locator,
    ))
}

pub(super) fn s5_transition(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
    transition: S5Transition,
) -> Result<(), FollowUpApplyFailure> {
    let expected = record.revision();
    record
        .apply_transition(transition)
        .map_err(|_| follow_up_outcome(record))?;
    journals
        .write_s5_cas(record, expected)
        .map_err(|_| follow_up_outcome(record))
}

pub(super) fn s5_locator(
    record: &ValidatedS5Record,
    path: &Path,
) -> Result<String, FollowUpApplyFailure> {
    let roots = record
        .roots()
        .ok_or_else(|| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
    let index = roots
        .roots()
        .iter()
        .position(|root| path.starts_with(root.canonical()))
        .ok_or_else(|| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
    let leaf = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
    Ok(format!(
        "journal:{};root:{index};leaf:{leaf}",
        record.journal_id()
    ))
}

pub(super) fn retained_evidence(
    role: RetainedArtifactRole,
    path: &Path,
    identity: crate::journal::FilesystemObjectIdentity,
    sha256: String,
    security: &FileSecuritySnapshot,
    default_stream_bytes: u64,
    locator: String,
) -> Result<RetainedArtifactEvidence, FollowUpApplyFailure> {
    let auxiliary_charge_bytes = security.auxiliary_charge_bytes();
    let logical_charge_bytes = default_stream_bytes
        .checked_add(auxiliary_charge_bytes)
        .ok_or_else(|| follow_up_partial("error.rfi.followUp.retainedStorageFull"))?;
    Ok(RetainedArtifactEvidence {
        role,
        path: path.to_string_lossy().into_owned(),
        identity,
        sha256,
        security_sha256: security.sha256(),
        default_stream_bytes,
        auxiliary_charge_bytes,
        logical_charge_bytes,
        locator,
    })
}

pub(super) fn insert_retained_locators(result: &mut OperationResult, locators: &[String]) {
    result.meta.insert(
        "retainedLocators".to_string(),
        Value::Array(locators.iter().cloned().map(Value::String).collect()),
    );
}

pub(super) fn insert_retained_evidence(
    result: &mut OperationResult,
    evidence: &[RetainedArtifactEvidence],
) {
    let order = [
        RetainedArtifactRole::Backup,
        RetainedArtifactRole::PrivateOutput,
        RetainedArtifactRole::DisplacedOriginal,
        RetainedArtifactRole::DisplacedApplied,
    ];
    let projection = order
        .into_iter()
        .filter_map(|role| evidence.iter().find(|item| item.role == role))
        .map(|item| {
            serde_json::json!({
                "role": item.role.projection_key(),
                "locator": item.locator,
            })
        })
        .collect();
    result
        .meta
        .insert("retainedEvidence".to_string(), Value::Array(projection));
}

pub(super) fn authorize_s5_recorded(
    record: &ValidatedS5Record,
    path: &Path,
) -> Result<AuthorizedPath, BackendError> {
    record
        .roots()
        .ok_or_else(|| BackendError::journal_invalid("S5 root authority is unavailable"))?
        .roots()
        .iter()
        .find_map(|root| root.authorize(path).ok())
        .ok_or_else(BackendError::path_escape)
}

pub(super) fn authorize_s5_recorded_directory(
    record: &ValidatedS5Record,
    path: &Path,
) -> Result<AuthorizedPath, BackendError> {
    record
        .roots()
        .ok_or_else(|| BackendError::journal_invalid("S5 root authority is unavailable"))?
        .roots()
        .iter()
        .find_map(|root| root.authorize_directory(path).ok())
        .ok_or_else(BackendError::path_escape)
}

pub(super) fn s5_retained_budget_exceeded(
    usage: (usize, u64),
    reservation: &RetainedStorageReservation,
) -> bool {
    usage
        .0
        .checked_add(reservation.file_count)
        .is_none_or(|count| count > MAX_S5_RETAINED_FILES)
        || usage
            .1
            .checked_add(reservation.logical_charge_bytes)
            .is_none_or(|bytes| bytes > MAX_S5_RETAINED_BYTES)
}

pub(super) fn s5_recorded_retained_usage(
    record: &ValidatedS5Record,
    canonical_parent: &Path,
) -> Result<(usize, u64), BackendError> {
    let canonical = authorize_s5_recorded_directory(record, canonical_parent)?;
    let canonical_usage = canonical
        .s5_retained_storage_usage(MAX_S5_RETAINED_FILES)
        .map_err(|_| BackendError::validation("error.rfi.followUp.retainedStorageFull"))?;
    let private_parent = canonical_parent.join(".docflow-backups");
    let private_usage = match authorize_s5_recorded_directory(record, &private_parent)?
        .s5_retained_storage_usage(MAX_S5_RETAINED_FILES)
    {
        Ok(usage) => usage,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            crate::cap_fs::RetainedStorageUsage {
                file_count: 0,
                logical_charge_bytes: 0,
            }
        }
        Err(_) => {
            return Err(BackendError::validation(
                "error.rfi.followUp.retainedStorageFull",
            ));
        }
    };
    let count = canonical_usage
        .file_count
        .checked_add(private_usage.file_count)
        .ok_or_else(|| BackendError::validation("error.rfi.followUp.retainedStorageFull"))?;
    let bytes = canonical_usage
        .logical_charge_bytes
        .checked_add(private_usage.logical_charge_bytes)
        .ok_or_else(|| BackendError::validation("error.rfi.followUp.retainedStorageFull"))?;
    Ok((count, bytes))
}

pub(super) fn transition_s5_backend(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
    transition: S5Transition,
) -> Result<(), BackendError> {
    let expected = record.revision();
    record.apply_transition(transition)?;
    journals.write_s5_cas(record, expected)
}

pub(super) fn s5_recovery_result(
    record: &ValidatedS5Record,
    notice: RecoveryNotice,
) -> OperationResult {
    let mut result = OperationResult {
        applied: 0,
        failed: 1,
        journal_id: record.journal_id().to_string(),
        errors: vec![Warning {
            key: notice.warning_key,
            subject: notice.locator,
        }],
        meta: JsonObject::new(),
        cloud_synced: None,
    };
    insert_effect_state(&mut result, "outcome_unknown");
    insert_retained_evidence(&mut result, record.retained_evidence());
    result
}

pub(super) fn sha256_open_file(file: &mut File) -> io::Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) enum StepFailure {
    NoEffect(String),
    WarningNoEffect(Warning),
    OutcomeUnknown(String),
    Cancelled,
}

pub(super) fn operation_result_for_generated_failure(
    record: &JournalRecord,
    key: &str,
    outcome_unknown: bool,
) -> OperationResult {
    let mut result = OperationResult {
        applied: 0,
        failed: 1,
        journal_id: record.journal_id.clone(),
        errors: vec![Warning {
            key: key.to_string(),
            subject: String::new(),
        }],
        meta: JsonObject::new(),
        cloud_synced: None,
    };
    insert_effect_state(
        &mut result,
        if outcome_unknown {
            "outcome_unknown"
        } else {
            "no_effect_retryable"
        },
    );
    result
}

pub(super) fn operation_result_for_generated_cancelled(record: &JournalRecord) -> OperationResult {
    let mut result = OperationResult {
        applied: 0,
        failed: 0,
        journal_id: record.journal_id.clone(),
        errors: vec![Warning {
            key: "warn.operation.cancelled".to_string(),
            subject: String::new(),
        }],
        meta: JsonObject::new(),
        cloud_synced: None,
    };
    result
        .meta
        .insert("cancelled".to_string(), Value::Bool(true));
    insert_effect_state(&mut result, "cancelled_clean");
    result
}

pub(crate) struct CancellationRegistration<'a> {
    pub(super) registry: &'a CancellationRegistry,
    pub(super) request_id: &'a str,
}

impl Drop for CancellationRegistration<'_> {
    fn drop(&mut self) {
        self.registry.finish(self.request_id);
    }
}

pub(super) fn mark_cancelled(
    record: &mut JournalRecord,
    result: &mut OperationResult,
    pending: usize,
) {
    let partial = !record.steps.is_empty();
    record.state = Some(if partial {
        JournalState::CancelledPartial
    } else {
        JournalState::CancelledClean
    });
    result.errors.push(Warning {
        key: "warn.operation.cancelled".to_string(),
        subject: String::new(),
    });
    result
        .meta
        .insert("cancelled".to_string(), Value::Bool(true));
    result.meta.insert(
        "pending".to_string(),
        Value::from(u64::try_from(pending).unwrap_or(u64::MAX)),
    );
    insert_effect_state(
        result,
        if partial {
            "cancelled_partial"
        } else {
            "cancelled_clean"
        },
    );
}

pub(super) fn insert_effect_state(result: &mut OperationResult, state: &str) {
    result
        .meta
        .insert("effectState".to_string(), Value::String(state.to_string()));
}

pub(super) fn required_path(raw: Option<&str>, label: &str) -> Result<PathBuf, StepFailure> {
    raw.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| StepFailure::NoEffect(format!("{label} is missing")))
}

pub(super) fn step_no_effect(error: BackendError) -> StepFailure {
    StepFailure::NoEffect(error.message)
}

pub(super) fn step_unknown(error: BackendError) -> StepFailure {
    StepFailure::OutcomeUnknown(error.message)
}

pub(super) fn io_no_effect(error: io::Error) -> StepFailure {
    StepFailure::NoEffect(error.to_string())
}

pub(super) fn replacement_failure(destination: &AnchoredPath, error: io::Error) -> StepFailure {
    StepFailure::OutcomeUnknown(format!("{}: {error}", destination.absolute().display()))
}

#[cfg(test)]
mod follow_up_failure_tests {
    use super::*;

    #[test]
    fn no_effect_and_partial_keep_the_key_with_an_empty_subject() {
        match follow_up_no_effect("error.key") {
            FollowUpApplyFailure::NoEffect(warning) => {
                assert_eq!(warning.key, "error.key");
                assert_eq!(warning.subject, "");
            }
            _ => panic!("expected NoEffect"),
        }
        assert!(matches!(
            follow_up_partial("error.key"),
            FollowUpApplyFailure::Partial(warning) if warning.key == "error.key"
        ));
    }

    #[test]
    fn backend_errors_use_their_message_as_the_key() {
        let error = BackendError::path_invalid("bad path");
        assert!(matches!(
            follow_up_backend_no_effect(error),
            FollowUpApplyFailure::NoEffect(warning) if warning.key == "bad path"
        ));
        let error = BackendError::path_invalid("bad path");
        assert!(matches!(
            follow_up_backend_partial(error),
            FollowUpApplyFailure::Partial(warning) if warning.key == "bad path"
        ));
    }

    #[test]
    fn warning_subject_is_kept_as_given() {
        let warning = follow_up_warning("error.key", "subject-1");
        assert_eq!(warning.key, "error.key");
        assert_eq!(warning.subject, "subject-1");
    }
}

#[cfg(test)]
mod s5_retained_budget_tests {
    use super::s5_retained_budget_exceeded;
    use crate::cap_fs::RetainedStorageReservation;
    use crate::journal::{MAX_S5_RETAINED_BYTES, MAX_S5_RETAINED_FILES};

    fn reservation(file_count: usize, logical_charge_bytes: u64) -> RetainedStorageReservation {
        RetainedStorageReservation {
            file_count,
            logical_charge_bytes,
        }
    }

    #[test]
    fn reservation_within_budget_is_allowed() {
        assert!(!s5_retained_budget_exceeded((0, 0), &reservation(1, 10)));
        assert!(!s5_retained_budget_exceeded(
            (MAX_S5_RETAINED_FILES - 1, MAX_S5_RETAINED_BYTES - 10),
            &reservation(1, 10),
        ));
    }

    #[test]
    fn file_count_over_limit_is_rejected() {
        assert!(s5_retained_budget_exceeded(
            (MAX_S5_RETAINED_FILES, 0),
            &reservation(1, 0),
        ));
    }

    #[test]
    fn byte_total_over_limit_is_rejected() {
        assert!(s5_retained_budget_exceeded(
            (0, MAX_S5_RETAINED_BYTES),
            &reservation(0, 1),
        ));
    }

    #[test]
    fn arithmetic_overflow_is_rejected() {
        assert!(s5_retained_budget_exceeded(
            (usize::MAX, 0),
            &reservation(1, 0)
        ));
        assert!(s5_retained_budget_exceeded(
            (0, u64::MAX),
            &reservation(0, 1)
        ));
    }
}
