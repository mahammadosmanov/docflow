//! Per-step undo dispatch and undo planning.

use super::*;

pub(super) fn step_is_ready_for_undo(step: &Step, roots: &[CapabilityRoot]) -> bool {
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

pub(super) fn plan_undo_recovery(
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

pub(super) fn undo_step(
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

pub(super) fn restore_backup(step: &Step, roots: &[CapabilityRoot]) -> Result<(), UndoStepFailure> {
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
        workbook
            .overwrite_verified_from(&backup, applied_hash)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::InvalidData {
                    UndoStepFailure::NoEffect(error)
                } else {
                    UndoStepFailure::OutcomeUnknown(error)
                }
            })
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

pub(super) enum UndoStepFailure {
    NoEffect(io::Error),
    OutcomeUnknown(io::Error),
}

pub(super) fn required_path(raw: Option<&str>, label: &str) -> io::Result<PathBuf> {
    raw.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{label} is missing")))
}
