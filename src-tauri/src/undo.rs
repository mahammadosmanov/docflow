use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;

use crate::{
    cap_fs::{AnchoredPath, CapabilityRoot},
    contracts::{JsonObject, OperationResult, Step, StepKind, Warning},
    errors::BackendError,
    journal::{
        GrantRootIdentity, JournalRecord, JournalState, JournalStore, UndoRecovery,
        UndoRecoveryPhase,
    },
    paths::CallerIdentity,
};

#[cfg(test)]
thread_local! {
    static BEFORE_QUARANTINE_RESTORE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
    static FAIL_QUARANTINE_DELETE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn inject_before_quarantine_restore<F>(hook: F)
where
    F: FnOnce() + 'static,
{
    BEFORE_QUARANTINE_RESTORE.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_before_quarantine_restore() {
    BEFORE_QUARANTINE_RESTORE.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

#[cfg(not(test))]
fn run_before_quarantine_restore() {}

#[cfg(test)]
fn inject_quarantine_delete_failure() {
    FAIL_QUARANTINE_DELETE.with(|fail| fail.set(true));
}

#[cfg(test)]
fn injected_quarantine_delete_failure() -> io::Result<()> {
    FAIL_QUARANTINE_DELETE.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other("injected quarantine delete failure"))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn injected_quarantine_delete_failure() -> io::Result<()> {
    Ok(())
}

pub fn undo_journal(
    journals: &JournalStore,
    journal_id: &str,
    caller: &CallerIdentity,
    destination: Option<&Path>,
) -> Result<OperationResult, BackendError> {
    let mut record = journals.read(journal_id)?;
    if record
        .owner_id
        .as_deref()
        .is_some_and(|owner| owner != caller.owner_id)
    {
        return Err(BackendError::journal_owner_mismatch());
    }
    if record.pending_step.is_some()
        || record.pending_undo.is_some()
        || record.undo_recovery.is_some()
    {
        return Err(BackendError::journal_invalid(
            "journal has an unresolved pending effect",
        ));
    }
    let roots = prepare_roots(journals, &mut record)?;
    if let Some(destination) = destination {
        validate_path(destination, &roots, false)?;
    }

    let mut result = OperationResult {
        applied: 0,
        failed: 0,
        journal_id: record.journal_id.clone(),
        errors: Vec::new(),
        meta: JsonObject::new(),
        cloud_synced: None,
    };
    for index in (0..record.steps.len()).rev() {
        if record.undone_steps.contains(&index) {
            continue;
        }
        let step = record.steps[index].clone();
        validate_step(&step, &roots)?;
        let recovery = plan_undo_recovery(&step, destination, &roots, index)
            .map_err(|error| BackendError::journal_invalid(error.to_string()))?;
        record.pending_undo = Some(index);
        record.undo_recovery = recovery;
        journals.write(&record)?;
        validate_step(&step, &roots)?;
        let restore_backup = step.kind == StepKind::RestoreBackup;
        match undo_step(&step, destination, &roots, journals, &mut record) {
            Ok(()) => {
                record.undone_steps.push(index);
                result.applied += 1;
                record.pending_undo = None;
                record.undo_recovery = None;
                journals.write(&record)?;
            }
            Err(UndoStepFailure::NoEffect(error)) => {
                result.failed += 1;
                result.errors.push(Warning {
                    key: "error.undo_failed".to_string(),
                    subject: format!("{}: {error}", step.dst.as_deref().unwrap_or_default()),
                });
                record.pending_undo = None;
                record.undo_recovery = None;
                journals.write(&record)?;
            }
            Err(UndoStepFailure::OutcomeUnknown(error)) => {
                result.failed += 1;
                result.errors.push(Warning {
                    key: "error.undo_failed".to_string(),
                    subject: format!("{}: {error}", step.dst.as_deref().unwrap_or_default()),
                });
                record.state = Some(JournalState::RecoveryRequired);
                result.meta.insert(
                    "effectState".to_string(),
                    Value::String("outcome_unknown".to_string()),
                );
                journals.write(&record)?;
                return Ok(result);
            }
        }
        if restore_backup && result.failed > 0 {
            break;
        }
    }
    record.state = Some(if result.failed == 0 {
        JournalState::Undone
    } else {
        JournalState::UndoPartial
    });
    result.meta.insert(
        "effectState".to_string(),
        Value::String(if result.failed == 0 {
            "committed".to_string()
        } else {
            "partial_committed".to_string()
        }),
    );
    journals.write(&record)?;
    Ok(result)
}

pub(crate) fn journal_is_safely_undoable(record: &JournalRecord) -> bool {
    if record.op_id == crate::outlook_reply_draft::OPERATION_ID {
        return crate::outlook_reply_draft::journal_is_safely_undoable(record);
    }
    if record.steps.is_empty()
        || record.pending_step.is_some()
        || record.pending_undo.is_some()
        || record.undo_recovery.is_some()
        || matches!(
            record.state,
            Some(
                JournalState::Applying
                    | JournalState::RecoveryRequired
                    | JournalState::NoEffectRetryable
                    | JournalState::CancelledClean
                    | JournalState::Undone
            )
        )
    {
        return false;
    }
    let raw_roots = if record.grant_roots.is_empty() {
        match derive_legacy_roots(record) {
            Ok(roots) => roots
                .into_iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            Err(_) => return false,
        }
    } else {
        record.grant_roots.clone()
    };
    let Ok(roots) = open_roots(&raw_roots) else {
        return false;
    };
    if !record.grant_root_identities.is_empty()
        && validate_root_identities(&roots, &record.grant_root_identities).is_err()
    {
        return false;
    }
    let mut remaining = 0;
    for (index, step) in record.steps.iter().enumerate() {
        if record.undone_steps.contains(&index) {
            continue;
        }
        remaining += 1;
        if validate_step(step, &roots).is_err() || !step_is_ready_for_undo(step, &roots) {
            return false;
        }
    }
    remaining > 0
}

fn step_is_ready_for_undo(step: &Step, roots: &[CapabilityRoot]) -> bool {
    match step.kind {
        StepKind::Mkdir => true,
        StepKind::Copy | StepKind::Write => step
            .dst
            .as_deref()
            .and_then(|path| anchor_path_io(Path::new(path), roots).ok())
            .is_some_and(|current| verify_expected_hash(&current, step, roots).is_ok()),
        StepKind::Move | StepKind::Rename => {
            let Some(current) = step
                .dst
                .as_deref()
                .and_then(|path| anchor_path_io(Path::new(path), roots).ok())
            else {
                return false;
            };
            let Some(target) = step
                .src
                .as_deref()
                .and_then(|path| anchor_path_io(Path::new(path), roots).ok())
            else {
                return false;
            };
            verify_expected_hash(&current, step, roots).is_ok() && target.open_read().is_err()
        }
        StepKind::RestoreBackup => {
            let Some(backup) = step
                .src
                .as_deref()
                .and_then(|path| anchor_path_io(Path::new(path), roots).ok())
            else {
                return false;
            };
            let Some(workbook) = step
                .dst
                .as_deref()
                .and_then(|path| anchor_path_io(Path::new(path), roots).ok())
            else {
                return false;
            };
            verify_hash_from_meta(&backup, step, "originalSha256", "workbook backup").is_ok()
                && verify_hash_from_meta(&workbook, step, "appliedSha256", "applied workbook")
                    .is_ok()
        }
        _ => false,
    }
}

fn plan_undo_recovery(
    step: &Step,
    destination: Option<&Path>,
    roots: &[CapabilityRoot],
    index: usize,
) -> io::Result<Option<UndoRecovery>> {
    if !matches!(
        step.kind,
        StepKind::Move | StepKind::Rename | StepKind::Copy | StepKind::Write
    ) {
        return Ok(None);
    }
    let current = required_path(step.dst.as_deref(), "undo current path")?;
    let current = anchor_path_io(&current, roots)?;
    let quarantine = current.temporary_sibling("undo")?;
    let target = if let Some(destination_root) = destination {
        let source_name = required_path(step.src.as_deref(), "step source")?
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "source has no name"))?
            .to_os_string();
        Some(destination_root.join(source_name))
    } else if matches!(step.kind, StepKind::Move | StepKind::Rename) {
        Some(required_path(step.src.as_deref(), "move source")?)
    } else {
        None
    };
    Ok(Some(UndoRecovery {
        index,
        original: current.absolute().to_string_lossy().into_owned(),
        quarantine: quarantine.absolute().to_string_lossy().into_owned(),
        target: target.map(|path| path.to_string_lossy().into_owned()),
        phase: UndoRecoveryPhase::Prepared,
    }))
}

fn undo_step(
    step: &Step,
    destination: Option<&Path>,
    roots: &[CapabilityRoot],
    journals: &JournalStore,
    record: &mut JournalRecord,
) -> Result<(), UndoStepFailure> {
    if step.kind == StepKind::Mkdir {
        if destination.is_some() {
            return Ok(());
        }
        let directory = required_path(step.dst.as_deref(), "mkdir destination")
            .map_err(UndoStepFailure::NoEffect)?;
        let directory = anchor_path_io(&directory, roots).map_err(UndoStepFailure::NoEffect)?;
        match directory.remove_dir_durable() {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::PermissionDenied
                ) => {}
            Err(error) => return Err(UndoStepFailure::OutcomeUnknown(error)),
        }
        return Ok(());
    }

    if step.kind == StepKind::RestoreBackup {
        if destination.is_some() {
            return Ok(());
        }
        return restore_backup(step, roots);
    }
    match step.kind {
        StepKind::Move | StepKind::Rename | StepKind::Copy | StepKind::Write => {
            undo_quarantined_file(step, roots, journals, record)
        }
        _ => Err(UndoStepFailure::NoEffect(io::Error::new(
            io::ErrorKind::Unsupported,
            "journal action does not have a Rust inverse",
        ))),
    }
}

fn restore_backup(step: &Step, roots: &[CapabilityRoot]) -> Result<(), UndoStepFailure> {
    let backup =
        required_path(step.src.as_deref(), "workbook backup").map_err(UndoStepFailure::NoEffect)?;
    let workbook = required_path(step.dst.as_deref(), "workbook destination")
        .map_err(UndoStepFailure::NoEffect)?;
    let backup = anchor_path_io(&backup, roots).map_err(UndoStepFailure::NoEffect)?;
    let workbook = anchor_path_io(&workbook, roots).map_err(UndoStepFailure::NoEffect)?;
    verify_hash_from_meta(&backup, step, "originalSha256", "workbook backup")
        .map_err(UndoStepFailure::NoEffect)?;
    verify_hash_from_meta(&workbook, step, "appliedSha256", "applied workbook")
        .map_err(UndoStepFailure::NoEffect)?;
    #[cfg(windows)]
    {
        let applied_hash = step
            .meta
            .get("appliedSha256")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                UndoStepFailure::NoEffect(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "applied workbook hash is missing",
                ))
            })?;
        return workbook
            .overwrite_verified_from(&backup, applied_hash)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::InvalidData {
                    UndoStepFailure::NoEffect(error)
                } else {
                    UndoStepFailure::OutcomeUnknown(error)
                }
            });
    }
    #[cfg(not(windows))]
    {
        let temporary = workbook
            .temporary_sibling("restore")
            .map_err(UndoStepFailure::NoEffect)?;
        temporary
            .copy_no_clobber_from(&backup)
            .map_err(UndoStepFailure::NoEffect)?;
        if let Err(error) =
            verify_hash_from_meta(&temporary, step, "originalSha256", "restore copy")
        {
            let _ = temporary.remove_file();
            return Err(UndoStepFailure::NoEffect(error));
        }
        if let Err(error) = workbook.replace_with(&temporary) {
            return Err(UndoStepFailure::OutcomeUnknown(error));
        }
        Ok(())
    }
}

fn verify_hash_from_meta(
    path: &AnchoredPath,
    step: &Step,
    key: &str,
    label: &str,
) -> io::Result<()> {
    let expected = step.meta.get(key).and_then(Value::as_str).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{label} hash is missing"),
        )
    })?;
    let actual = path.sha256()?;
    if actual == expected {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} changed since apply", path.absolute().display()),
        ))
    }
}

fn undo_quarantined_file(
    step: &Step,
    roots: &[CapabilityRoot],
    journals: &JournalStore,
    record: &mut JournalRecord,
) -> Result<(), UndoStepFailure> {
    let recovery = record.undo_recovery.clone().ok_or_else(|| {
        UndoStepFailure::NoEffect(io::Error::new(
            io::ErrorKind::InvalidData,
            "undo quarantine recovery metadata is missing",
        ))
    })?;
    let original =
        anchor_path_io(Path::new(&recovery.original), roots).map_err(UndoStepFailure::NoEffect)?;
    let quarantine = anchor_path_io(Path::new(&recovery.quarantine), roots)
        .map_err(UndoStepFailure::NoEffect)?;
    if let Err(error) = original.move_no_clobber_to(&quarantine) {
        return Err(
            if matches!(
                error.kind(),
                io::ErrorKind::AlreadyExists | io::ErrorKind::NotFound
            ) {
                UndoStepFailure::NoEffect(error)
            } else {
                UndoStepFailure::OutcomeUnknown(error)
            },
        );
    }
    persist_recovery_phase(journals, record, UndoRecoveryPhase::Quarantined)?;
    if let Err(error) = verify_expected_hash(&quarantine, step, roots) {
        return restore_after_failure(error, &quarantine, &original, journals, record);
    }
    if let Some(target) = recovery.target.as_deref() {
        let target = anchor_path_io(Path::new(target), roots).map_err(|error| {
            UndoStepFailure::OutcomeUnknown(io::Error::new(
                error.kind(),
                format!("undo target is unavailable after quarantine: {error}"),
            ))
        })?;
        persist_recovery_phase(journals, record, UndoRecoveryPhase::Publishing)?;
        if let Err(error) = quarantine.move_no_clobber_to(&target) {
            return Err(UndoStepFailure::OutcomeUnknown(io::Error::new(
                error.kind(),
                format!("undo target publication could not be confirmed: {error}"),
            )));
        }
    } else {
        persist_recovery_phase(journals, record, UndoRecoveryPhase::Deleting)?;
        injected_quarantine_delete_failure().map_err(UndoStepFailure::OutcomeUnknown)?;
        quarantine
            .remove_file_durable()
            .map_err(UndoStepFailure::OutcomeUnknown)?;
    }
    Ok(())
}

fn verify_expected_hash(
    path: &AnchoredPath,
    step: &Step,
    roots: &[CapabilityRoot],
) -> io::Result<()> {
    let expected = step
        .meta
        .get("undoSha256")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let source = step.src.as_deref()?;
            let source = Path::new(source);
            anchor_path_io(source, roots)
                .ok()
                .and_then(|source| source.sha256().ok())
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "legacy journal has no safe hash fallback",
            )
        })?;
    let actual = path.sha256()?;
    if actual == expected {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} changed since apply", path.absolute().display()),
        ))
    }
}

fn restore_after_failure(
    cause: io::Error,
    quarantine: &AnchoredPath,
    original: &AnchoredPath,
    journals: &JournalStore,
    record: &mut JournalRecord,
) -> Result<(), UndoStepFailure> {
    persist_recovery_phase(journals, record, UndoRecoveryPhase::Restoring)?;
    run_before_quarantine_restore();
    match quarantine.move_no_clobber_to(original) {
        Ok(()) => Err(UndoStepFailure::NoEffect(cause)),
        Err(restore) => Err(UndoStepFailure::OutcomeUnknown(io::Error::new(
            restore.kind(),
            format!("undo failed ({cause}) and quarantine restoration failed ({restore})"),
        ))),
    }
}

fn persist_recovery_phase(
    journals: &JournalStore,
    record: &mut JournalRecord,
    phase: UndoRecoveryPhase,
) -> Result<(), UndoStepFailure> {
    let recovery = record.undo_recovery.as_mut().ok_or_else(|| {
        UndoStepFailure::OutcomeUnknown(io::Error::new(
            io::ErrorKind::InvalidData,
            "undo recovery metadata disappeared",
        ))
    })?;
    recovery.phase = phase;
    journals.write(record).map_err(|error| {
        UndoStepFailure::OutcomeUnknown(io::Error::other(format!(
            "undo recovery phase could not be persisted: {}",
            error.message
        )))
    })
}

enum UndoStepFailure {
    NoEffect(io::Error),
    OutcomeUnknown(io::Error),
}

fn required_path(raw: Option<&str>, label: &str) -> io::Result<PathBuf> {
    raw.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{label} is missing")))
}

fn prepare_roots(
    journals: &JournalStore,
    record: &mut JournalRecord,
) -> Result<Vec<CapabilityRoot>, BackendError> {
    if record.grant_roots.is_empty() && !record.grant_root_identities.is_empty() {
        return Err(BackendError::journal_invalid(
            "journal root identity metadata has no grant roots",
        ));
    }
    if record.grant_roots.is_empty() {
        record.grant_roots = derive_legacy_roots(record)?
            .into_iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
    }
    let roots = open_roots(&record.grant_roots)?;
    if record.grant_root_identities.is_empty() {
        record.grant_root_identities = root_identity_records(&roots)?;
        journals.write(record)?;
        return Ok(roots);
    }
    validate_root_identities(&roots, &record.grant_root_identities)?;
    Ok(roots)
}

fn open_roots(raw_roots: &[String]) -> Result<Vec<CapabilityRoot>, BackendError> {
    if raw_roots.is_empty() {
        return Err(BackendError::journal_invalid(
            "journal has no capability roots for safe undo",
        ));
    }
    let mut roots = Vec::new();
    for raw in raw_roots {
        let root = PathBuf::from(raw);
        reject_lexical_path(&root)?;
        let canonical = fs::canonicalize(&root)
            .map_err(|_| BackendError::journal_invalid("journal grant root is unavailable"))?;
        if canonical != root {
            return Err(BackendError::journal_invalid(
                "journal grant root path is not canonical",
            ));
        }
        if roots
            .iter()
            .any(|known: &CapabilityRoot| known.canonical() == root)
        {
            return Err(BackendError::journal_invalid(
                "journal contains duplicate grant roots",
            ));
        }
        roots.push(
            CapabilityRoot::open(&root)
                .map_err(|_| BackendError::journal_invalid("journal grant root is unavailable"))?,
        );
    }
    Ok(roots)
}

fn root_identity_records(roots: &[CapabilityRoot]) -> Result<Vec<GrantRootIdentity>, BackendError> {
    roots
        .iter()
        .map(|root| {
            Ok(GrantRootIdentity {
                root: root.canonical().to_string_lossy().into_owned(),
                identity: root.identity().map_err(|_| {
                    BackendError::journal_invalid("journal grant root identity is unavailable")
                })?,
            })
        })
        .collect()
}

fn validate_root_identities(
    roots: &[CapabilityRoot],
    identities: &[GrantRootIdentity],
) -> Result<(), BackendError> {
    if roots.len() != identities.len() {
        return Err(BackendError::journal_invalid(
            "journal grant root identity metadata is incomplete",
        ));
    }
    for root in roots {
        let path = root.canonical().to_string_lossy();
        let mut matching = identities.iter().filter(|entry| entry.root == path);
        let expected = matching.next().ok_or_else(|| {
            BackendError::journal_invalid("journal grant root identity metadata is incomplete")
        })?;
        if matching.next().is_some() {
            return Err(BackendError::journal_invalid(
                "journal contains duplicate grant root identities",
            ));
        }
        let actual = root.identity().map_err(|_| {
            BackendError::journal_invalid("journal grant root identity is unavailable")
        })?;
        if actual != expected.identity {
            return Err(BackendError::journal_invalid(
                "journal grant root object identity changed",
            ));
        }
    }
    Ok(())
}

fn derive_legacy_roots(record: &JournalRecord) -> Result<Vec<PathBuf>, BackendError> {
    if record.steps.is_empty() {
        return Err(BackendError::journal_invalid(
            "legacy journal has no reversible steps",
        ));
    }
    let mut roots = Vec::new();
    for step in &record.steps {
        if !matches!(
            step.kind,
            StepKind::Mkdir
                | StepKind::Move
                | StepKind::Rename
                | StepKind::Copy
                | StepKind::Write
                | StepKind::RestoreBackup
        ) {
            return Err(BackendError::journal_invalid(
                "legacy journal contains an unmigrated executable action",
            ));
        }
        for raw in [step.src.as_deref(), step.dst.as_deref()]
            .into_iter()
            .flatten()
        {
            let path = PathBuf::from(raw);
            reject_lexical_path(&path)?;
            let parent = path.parent().ok_or_else(|| {
                BackendError::journal_invalid("legacy journal path has no safe parent root")
            })?;
            if parent.parent().is_none() {
                return Err(BackendError::journal_invalid(
                    "legacy journal would require a filesystem root grant",
                ));
            }
            let parent = parent.to_path_buf();
            if !roots.contains(&parent) {
                roots.push(parent);
            }
        }
    }
    if roots.is_empty() {
        return Err(BackendError::journal_invalid(
            "legacy journal has no safe path roots",
        ));
    }
    roots.sort_by_key(|root| root.components().count());
    let mut non_redundant = Vec::<PathBuf>::new();
    for root in roots {
        if !non_redundant.iter().any(|known| root.starts_with(known)) {
            non_redundant.push(root);
        }
    }
    Ok(non_redundant)
}

fn validate_step(step: &Step, roots: &[CapabilityRoot]) -> Result<(), BackendError> {
    match step.kind {
        StepKind::Mkdir => {
            validate_path(&required_journal_path(step.dst.as_deref())?, roots, true)?;
        }
        StepKind::Move
        | StepKind::Rename
        | StepKind::Copy
        | StepKind::Write
        | StepKind::RestoreBackup => {
            if let Some(source) = step.src.as_deref() {
                validate_path(&PathBuf::from(source), roots, true)?;
            }
            validate_path(&required_journal_path(step.dst.as_deref())?, roots, true)?;
        }
        _ => {
            return Err(BackendError::journal_invalid(
                "journal contains an unmigrated executable action",
            ));
        }
    }
    Ok(())
}

fn required_journal_path(raw: Option<&str>) -> Result<PathBuf, BackendError> {
    raw.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| BackendError::journal_invalid("journal step path is missing"))
}

fn validate_path(
    path: &Path,
    roots: &[CapabilityRoot],
    _allow_missing: bool,
) -> Result<(), BackendError> {
    reject_lexical_path(path)?;
    for root in roots {
        if path == root.canonical() || root.authorize(path).is_ok() {
            return Ok(());
        }
    }
    Err(BackendError::journal_invalid(
        "journal step escapes its original granted roots",
    ))
}

fn anchor_path_io(path: &Path, roots: &[CapabilityRoot]) -> io::Result<AnchoredPath> {
    for root in roots {
        if let Ok(path) = root.authorize(path)
            && let Ok(anchor) = path.anchor()
        {
            return Ok(anchor);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "journal path is unavailable through its pinned capability root",
    ))
}

fn reject_lexical_path(path: &Path) -> Result<(), BackendError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(BackendError::journal_invalid(
            "journal contains an invalid path",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contracts::JsonObject,
        journal::{GrantRootIdentity, JournalRecord},
    };

    fn committed_copy_record(root: &Path, destination: &Path) -> JournalRecord {
        let capability = CapabilityRoot::open(root).unwrap();
        let mut meta = JsonObject::new();
        meta.insert(
            "undoSha256".to_string(),
            Value::String(
                capability
                    .authorize(destination)
                    .unwrap()
                    .anchor()
                    .unwrap()
                    .sha256()
                    .unwrap(),
            ),
        );
        let mut record = JournalRecord::new(
            "undo-recovery-test",
            "op.test",
            "2026-08-25T00:00:00+00:00",
            "owner",
            "session",
            "digest",
        );
        record.steps.push(Step {
            kind: StepKind::Copy,
            src: Some(root.join("source.txt").to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: None,
            meta,
        });
        record.grant_roots = vec![root.to_string_lossy().into_owned()];
        record.grant_root_identities = vec![GrantRootIdentity {
            root: root.to_string_lossy().into_owned(),
            identity: capability.identity().unwrap(),
        }];
        record.state = Some(JournalState::Committed);
        record
    }

    fn assert_durable_outcome_unknown(journals: &JournalStore) -> serde_json::Value {
        let record = journals.read("undo-recovery-test").unwrap();
        assert_eq!(record.state, Some(JournalState::RecoveryRequired));
        assert_eq!(record.pending_undo, Some(0));
        let encoded = serde_json::to_value(record).unwrap();
        let recovery = encoded
            .get("undoRecovery")
            .expect("quarantine recovery locator must remain durable")
            .clone();
        assert!(Path::new(recovery["quarantine"].as_str().unwrap()).exists());
        assert_eq!(
            journals.recover_pending().unwrap()[0].disposition,
            crate::journal::RecoveryDisposition::OutcomeUnknown
        );
        recovery
    }

    #[test]
    fn rename_success_followed_by_directory_sync_failure_keeps_durable_undo_recovery() {
        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let journal_dir = fixture.path().join("journal");
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("source.txt"), b"APPROVED").unwrap();
        let destination = root.join("copy.txt");
        std::fs::write(&destination, b"APPROVED").unwrap();
        let journals = JournalStore::new(&journal_dir);
        journals
            .write(&committed_copy_record(&root, &destination))
            .unwrap();
        crate::cap_fs::inject_directory_sync_failure_after(0);

        let result = undo_journal(
            &journals,
            "undo-recovery-test",
            &CallerIdentity::new("owner", "restart"),
            None,
        )
        .unwrap();

        assert_eq!(result.meta["effectState"], "outcome_unknown");
        let recovery = assert_durable_outcome_unknown(&journals);
        let quarantine = recovery["quarantine"].as_str().unwrap();
        assert!(Path::new(quarantine).exists());
    }

    #[test]
    fn hash_failure_and_quarantine_restore_collision_remain_recoverable() {
        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let journal_dir = fixture.path().join("journal");
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("source.txt"), b"APPROVED").unwrap();
        let destination = root.join("copy.txt");
        std::fs::write(&destination, b"APPROVED").unwrap();
        let journals = JournalStore::new(&journal_dir);
        let mut record = committed_copy_record(&root, &destination);
        record.steps[0].meta.insert(
            "undoSha256".to_string(),
            Value::String("not-the-approved-hash".to_string()),
        );
        journals.write(&record).unwrap();
        let collision = destination.clone();
        inject_before_quarantine_restore(move || {
            std::fs::write(collision, b"USER COLLISION").unwrap();
        });

        let result = undo_journal(
            &journals,
            "undo-recovery-test",
            &CallerIdentity::new("owner", "restart"),
            None,
        )
        .unwrap();

        assert_eq!(result.meta["effectState"], "outcome_unknown");
        let recovery = assert_durable_outcome_unknown(&journals);
        assert_eq!(recovery["original"], destination.to_string_lossy().as_ref());
        assert_eq!(std::fs::read(destination).unwrap(), b"USER COLLISION");
    }

    #[test]
    fn move_target_failure_retains_quarantine_and_recovery_without_restoring() {
        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let journal_dir = fixture.path().join("journal");
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let source = root.join("source.txt");
        let destination = root.join("moved.txt");
        std::fs::write(&source, b"USER TARGET COLLISION").unwrap();
        std::fs::write(&destination, b"APPROVED MOVE").unwrap();
        let capability = CapabilityRoot::open(&root).unwrap();
        let mut meta = JsonObject::new();
        meta.insert(
            "undoSha256".to_string(),
            Value::String(
                capability
                    .authorize(&destination)
                    .unwrap()
                    .anchor()
                    .unwrap()
                    .sha256()
                    .unwrap(),
            ),
        );
        let mut record = JournalRecord::new(
            "undo-recovery-test",
            "op.test",
            "2026-08-25T00:00:00+00:00",
            "owner",
            "session",
            "digest",
        );
        record.steps.push(Step {
            kind: StepKind::Move,
            src: Some(source.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: None,
            meta,
        });
        record.grant_roots = vec![root.to_string_lossy().into_owned()];
        record.grant_root_identities = vec![GrantRootIdentity {
            root: root.to_string_lossy().into_owned(),
            identity: capability.identity().unwrap(),
        }];
        record.state = Some(JournalState::Committed);
        let journals = JournalStore::new(&journal_dir);
        journals.write(&record).unwrap();

        let result = undo_journal(
            &journals,
            "undo-recovery-test",
            &CallerIdentity::new("owner", "restart"),
            None,
        )
        .unwrap();

        assert_eq!(result.meta["effectState"], "outcome_unknown");
        let recovery = assert_durable_outcome_unknown(&journals);
        assert_eq!(recovery["target"], source.to_string_lossy().as_ref());
        assert_eq!(std::fs::read(source).unwrap(), b"USER TARGET COLLISION");
        assert!(!destination.exists());
        assert!(Path::new(recovery["quarantine"].as_str().unwrap()).exists());
    }

    #[test]
    fn quarantine_delete_failure_retains_the_durable_locator() {
        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let journal_dir = fixture.path().join("journal");
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("source.txt"), b"APPROVED").unwrap();
        let destination = root.join("copy.txt");
        std::fs::write(&destination, b"APPROVED").unwrap();
        let journals = JournalStore::new(&journal_dir);
        journals
            .write(&committed_copy_record(&root, &destination))
            .unwrap();
        inject_quarantine_delete_failure();

        let result = undo_journal(
            &journals,
            "undo-recovery-test",
            &CallerIdentity::new("owner", "restart"),
            None,
        )
        .unwrap();

        assert_eq!(result.meta["effectState"], "outcome_unknown");
        assert_durable_outcome_unknown(&journals);
    }
}
