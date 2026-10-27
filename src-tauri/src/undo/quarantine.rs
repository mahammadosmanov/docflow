//! Quarantine-based undo of file moves, copies and writes.

use super::*;

pub(super) fn verify_hash_from_meta(
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

pub(super) fn undo_quarantined_file(
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
    #[cfg(windows)]
    if recovery.target.is_none() {
        let expected = expected_undo_hash(step, roots).map_err(UndoStepFailure::NoEffect)?;
        persist_recovery_phase(journals, record, UndoRecoveryPhase::Deleting)?;
        return original.remove_verified(&expected).map_err(|error| {
            if error.kind() == io::ErrorKind::InvalidData {
                UndoStepFailure::NoEffect(error)
            } else {
                UndoStepFailure::OutcomeUnknown(error)
            }
        });
    }
    #[cfg(windows)]
    if let Some(target) = recovery.target.as_deref() {
        verify_expected_hash(&original, step, roots).map_err(UndoStepFailure::NoEffect)?;
        let target = anchor_path_io(Path::new(target), roots).map_err(UndoStepFailure::NoEffect)?;
        persist_recovery_phase(journals, record, UndoRecoveryPhase::Publishing)?;
        return original
            .move_no_clobber_to(&target)
            .map_err(UndoStepFailure::OutcomeUnknown);
    }
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

pub(super) fn verify_expected_hash(
    path: &AnchoredPath,
    step: &Step,
    roots: &[CapabilityRoot],
) -> io::Result<()> {
    let expected = expected_undo_hash(step, roots)?;
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

pub(super) fn expected_undo_hash(step: &Step, roots: &[CapabilityRoot]) -> io::Result<String> {
    step.meta
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
        })
}

pub(super) fn restore_after_failure(
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

pub(super) fn persist_recovery_phase(
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
