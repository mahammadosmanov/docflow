use super::*;

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

#[cfg(any(windows, test))]
pub(super) fn overwrite_existing_capability(
    source: &AnchoredPath,
    destination: &AnchoredPath,
    expected_destination_sha256: &str,
) -> io::Result<()> {
    destination.overwrite_verified_from(source, expected_destination_sha256)?;
    source.remove_file_durable()
}

pub(super) fn create_temporary(parent: &Path, destination: &Path) -> io::Result<(PathBuf, File)> {
    if !parent.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "destination parent does not exist",
        ));
    }
    for _ in 0..16 {
        let name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("file");
        let temporary = parent.join(format!(".{name}.docflow-{}.tmp", Uuid::new_v4()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary file",
    ))
}

pub(super) fn rollback_from_backup_capability(
    backup: &AnchoredPath,
    workbook: &AnchoredPath,
    expected_hash: &str,
) -> io::Result<()> {
    let temporary = workbook.temporary_sibling("restore")?;
    temporary.copy_no_clobber_from(backup)?;
    let result = (|| {
        if temporary.sha256()? != expected_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "workbook rollback copy did not match the approved backup",
            ));
        }
        atomic_replace_capability_with_retry(&temporary, workbook)?;
        if workbook.sha256()? != expected_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "workbook rollback verification failed",
            ));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = temporary.remove_file();
    }
    result
}

pub(super) fn atomic_replace_capability_with_retry(
    source: &AnchoredPath,
    destination: &AnchoredPath,
) -> io::Result<()> {
    for attempt in 0..20 {
        match destination.replace_with(source) {
            Ok(()) => return Ok(()),
            Err(error) if is_workbook_access_error(&error) && attempt < 19 => {
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("replacement retry loop always returns")
}

pub(crate) fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    atomic_replace_platform(source, destination)?;
    if let Some(parent) = destination.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

pub(super) fn is_workbook_access_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        || matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

pub(super) fn remove_anchored_file_with_access_retry(
    path: &AnchoredPath,
    attempts: usize,
) -> io::Result<()> {
    for attempt in 0..attempts.max(1) {
        match path.remove_file() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) if is_workbook_access_error(&error) && attempt + 1 < attempts.max(1) => {
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(
        "capability-relative file could not be removed",
    ))
}

#[cfg(unix)]
pub(super) fn atomic_replace_platform(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
pub(super) fn atomic_replace_platform(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: source and destination are NUL-terminated UTF-16 paths that live across the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(super) fn publish_no_clobber(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::hard_link(temporary, destination)?;
    fs::remove_file(temporary)
}

#[cfg(unix)]
pub(super) fn open_source_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
pub(super) fn open_source_no_follow(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(target_os = "macos")]
pub(super) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source contains NUL"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination contains NUL"))?;
    // SAFETY: source and destination are NUL-terminated paths that live across the call.
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
pub(super) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source contains NUL"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination contains NUL"))?;
    // SAFETY: source and destination are NUL-terminated paths that live across the call.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
pub(super) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: source and destination are NUL-terminated UTF-16 paths that live across the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
pub(super) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    if source.is_file() {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "atomic no-replace directory rename is unavailable",
        ))
    }
}

#[cfg(unix)]
pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
pub(super) fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
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
