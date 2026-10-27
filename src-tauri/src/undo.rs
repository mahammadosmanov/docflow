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

#[cfg(all(test, not(windows)))]
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

#[cfg(all(test, not(windows)))]
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

mod quarantine;
mod roots;
mod steps;
use quarantine::*;
use roots::*;
use steps::*;

#[cfg(test)]
mod tests;
