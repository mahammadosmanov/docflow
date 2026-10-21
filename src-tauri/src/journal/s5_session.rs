use super::*;

impl S5RecoverySession {
    pub(crate) fn has_next(&self) -> bool {
        !self.entries.is_empty()
    }

    pub(crate) fn next_root_accesses(&self) -> Option<&[GrantRootAccessRecord]> {
        match self.entries.front() {
            Some(PendingS5Entry::Valid(record))
                if record.recovery_notice.is_none()
                    || record
                        .pending_publication
                        .as_ref()
                        .is_some_and(|publication| {
                            publication.phase == RetainedPublicationPhase::RestoringOriginal
                        }) =>
            {
                Some(&record.grant_root_accesses)
            }
            Some(PendingS5Entry::Legacy(_)) | None => None,
            Some(PendingS5Entry::Valid(_)) => None,
        }
    }

    pub(crate) fn recover_next(
        &mut self,
        roots: Option<ValidatedGrantRootRoster>,
        caller: &crate::paths::CallerIdentity,
    ) -> Result<Option<RecoveryEntry>, BackendError> {
        let Some(entry) = self.entries.pop_front() else {
            return Ok(None);
        };
        let record = match entry {
            PendingS5Entry::Legacy(entry) => {
                if roots.is_some() {
                    return Err(BackendError::journal_invalid(
                        "legacy recovery cannot consume S5 root authority",
                    ));
                }
                return Ok(Some(entry));
            }
            PendingS5Entry::Valid(record) => *record,
        };
        if record.owner_id.as_deref() != Some(caller.owner_id.as_str()) {
            return Err(BackendError::journal_owner_mismatch());
        }
        let restoring_original = record
            .pending_publication
            .as_ref()
            .is_some_and(|publication| {
                publication.phase == RetainedPublicationPhase::RestoringOriginal
            });
        if let Some(notice) = record.recovery_notice.clone()
            && !restoring_original
        {
            return Ok(Some(recovery_entry(&record, notice)));
        }
        let roots = roots.ok_or_else(|| {
            BackendError::journal_invalid("S5 recovery root authority is missing")
        })?;
        if roots.records() != record.grant_root_accesses {
            return Err(BackendError::journal_invalid(
                "S5 recovery root authority changed",
            ));
        }
        let mut token = ValidatedS5Record {
            record,
            lease: S5MutationLease::Recovery { roots },
        };
        if token.pending_publication().is_some() {
            return crate::leaf_transaction::reconcile_retained_publication(
                &self.store,
                &mut token,
            );
        }
        let known_effect = !token.record.retained_evidence.is_empty()
            || token.record.created_artifact_parent.is_some()
            || token
                .record
                .pending_retained_artifact
                .as_ref()
                .is_some_and(|pending| {
                    matches!(
                        pending.creation_state,
                        PendingArtifactCreationState::Created(_)
                    )
                });
        let locator = token.retained_locators().map_or_else(
            |_| format!("journal:{};root:0;leaf:recovery", token.record.journal_id),
            |locators| locators.first().to_string(),
        );
        let notice = RecoveryNotice {
            warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
            locator,
        };
        let terminal = if known_effect {
            S5RecoveryTerminal::FailedPartial
        } else {
            S5RecoveryTerminal::OutcomeUnknown
        };
        let expected = token.revision();
        token.apply_transition(S5Transition::FinalizeRecovery {
            terminal,
            notice: Some(notice.clone()),
            evidence: Vec::new(),
        })?;
        self.store.write_s5_cas(&mut token, expected)?;
        Ok(Some(recovery_entry(&token.record, notice)))
    }
}

pub(super) fn same_s5_provenance(left: &JournalRecord, right: &JournalRecord) -> bool {
    left.journal_id == right.journal_id
        && left.op_id == right.op_id
        && left.ts == right.ts
        && left.owner_id == right.owner_id
        && left.session_id == right.session_id
        && left.plan_digest == right.plan_digest
        && left.grant_roots == right.grant_roots
        && left.grant_root_identities == right.grant_root_identities
        && left.grant_root_accesses == right.grant_root_accesses
        && left.s5_authority == right.s5_authority
}

#[cfg(unix)]
pub(super) fn open_mutation_lock_file(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
pub(super) fn open_mutation_lock_file(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .attributes(FILE_ATTRIBUTE_NORMAL)
        .open(path)
}
