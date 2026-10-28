use super::*;

impl FilesystemAuthority {
    pub fn undo(
        &self,
        journal_id: &str,
        caller: &CallerIdentity,
        destination: Option<&Path>,
    ) -> Result<OperationResult, BackendError> {
        if self.journals.inspect_operation(journal_id)? == rfi_follow_up::OPERATION_ID {
            if destination.is_some() {
                return Err(BackendError::validation(
                    "RFI Follow Up undo does not accept a destination",
                ));
            }
            return self.undo_follow_up(journal_id, caller);
        }
        let _permit = self.mutation_lock.try_acquire()?;
        crate::undo::undo_journal(&self.journals, journal_id, caller, destination)
    }

    pub fn recover_pending(
        &self,
        caller: &CallerIdentity,
    ) -> Result<Vec<crate::journal::RecoveryEntry>, BackendError> {
        let process = self.mutation_lock.try_acquire()?;
        let interprocess = self.journals.try_acquire_mutation_lock()?;
        let permits = self.journals.bind_s5_permits(process, interprocess);
        let mut session = self.journals.begin_s5_recovery(permits)?;
        let mut recovered = Vec::new();
        while session.has_next() {
            let roots = session
                .next_root_accesses()
                .map(|records| {
                    self.grants
                        .reopen_recorded_roots(records, caller, rfi_follow_up::OPERATION_ID)
                })
                .transpose()?;
            if let Some(entry) = session.recover_next(roots, caller)? {
                recovered.push(entry);
            }
        }
        recovered.sort_by(|left, right| left.journal_id.cmp(&right.journal_id));
        Ok(recovered)
    }

    pub(in crate::fs_ops) fn undo_follow_up(
        &self,
        journal_id: &str,
        caller: &CallerIdentity,
    ) -> Result<OperationResult, BackendError> {
        let process = self.mutation_lock.try_acquire()?;
        let interprocess = self.journals.try_acquire_mutation_lock()?;
        let permits = self.journals.bind_s5_permits(process, interprocess);
        let pending = self
            .journals
            .inspect_s5_for_undo(journal_id, caller, permits)?;
        let undo_mode = pending.mode();
        let roots = self.grants.reopen_recorded_roots(
            pending.recorded_root_accesses(),
            caller,
            rfi_follow_up::OPERATION_ID,
        )?;
        let mut record = self.journals.admit_s5_undo(pending, roots)?;
        if undo_mode == S5UndoMode::RestoreFailedApply {
            return self.restore_failed_follow_up_apply(&mut record);
        }
        let step = record
            .completed_steps()
            .first()
            .cloned()
            .ok_or_else(|| BackendError::journal_invalid("S5 undo step is missing"))?;
        let (backup_path, canonical_path) = s5_undo_paths(&step, &record)?;
        let backup_evidence = record
            .retained_evidence_for(RetainedArtifactRole::Backup)
            .cloned()
            .ok_or_else(|| BackendError::journal_invalid("S5 backup evidence is missing"))?;
        let (expected_applied_hash, expected_applied_identity) = s5_applied_expectations(&step)?;
        let canonical = authorize_s5_recorded(&record, &canonical_path)?
            .anchor()
            .map_err(|_| BackendError::path_invalid("S5 canonical workbook cannot be anchored"))?;
        let backup = authorize_s5_recorded(&record, &backup_path)?
            .anchor()
            .map_err(|_| BackendError::path_invalid("S5 backup cannot be anchored"))?;
        let mut current_file = canonical
            .open_mutable()
            .map_err(|_| BackendError::validation("error.rfi.followUp.workbookLocked"))?;
        let mut backup_file = backup
            .open_read()
            .map_err(|_| BackendError::validation("error.rfi.followUp.backupVerify"))?;
        let current_identity = crate::cap_fs::file_identity(&current_file)
            .map_err(|_| BackendError::validation("error.rfi.followUp.workbookChanged"))?;
        let backup_identity = crate::cap_fs::file_identity(&backup_file)
            .map_err(|_| BackendError::validation("error.rfi.followUp.backupVerify"))?;
        let current_hash = sha256_open_file(&mut current_file)
            .map_err(|_| BackendError::validation("error.rfi.followUp.workbookChanged"))?;
        let backup_hash = sha256_open_file(&mut backup_file)
            .map_err(|_| BackendError::validation("error.rfi.followUp.backupVerify"))?;
        let current_snapshot = FileSecuritySnapshot::capture(&current_file, None)
            .map_err(|_| BackendError::validation("error.rfi.followUp.workbookChanged"))?;
        let backup_snapshot = FileSecuritySnapshot::capture(&backup_file, None)
            .map_err(|_| BackendError::validation("error.rfi.followUp.backupVerify"))?;
        if current_identity != expected_applied_identity
            || current_hash != expected_applied_hash
            || current_snapshot.sha256() != backup_evidence.security_sha256
            || backup_identity != backup_evidence.identity
            || backup_hash != backup_evidence.sha256
            || backup_snapshot.sha256() != backup_evidence.security_sha256
        {
            return Err(BackendError::validation(
                "error.rfi.followUp.workbookChanged",
            ));
        }
        let current_len = current_file
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|_| BackendError::validation("error.rfi.followUp.workbookChanged"))?;
        let reservation = RetainedStorageReservation::for_undo(
            current_len,
            current_snapshot.auxiliary_charge_bytes(),
        )
        .map_err(|_| BackendError::validation("error.rfi.followUp.retainedStorageFull"))?;
        let canonical_parent_path = canonical_path
            .parent()
            .ok_or_else(|| BackendError::journal_invalid("S5 canonical parent is unavailable"))?;
        let usage = s5_recorded_retained_usage(&record, canonical_parent_path)?;
        if s5_retained_budget_exceeded(usage, &reservation) {
            return Err(BackendError::validation(
                "error.rfi.followUp.retainedStorageFull",
            ));
        }
        let parent_snapshot = canonical
            .capture_parent_security()
            .map_err(|_| BackendError::validation("error.rfi.followUp.workbookChanged"))?;
        let creation = backup_snapshot
            .creation_descriptor(&parent_snapshot)
            .map_err(|_| BackendError::validation("error.rfi.followUp.backupVerify"))?;
        let extension = canonical_path
            .extension()
            .and_then(|value| value.to_str())
            .ok_or_else(|| BackendError::journal_invalid("S5 canonical extension is invalid"))?;
        let retained_path = canonical_parent_path.join(format!(
            ".docflow-s5-retained-applied-{}-{}.{}",
            record.journal_id(),
            Uuid::new_v4().simple(),
            extension
        ));
        let retained = authorize_s5_recorded(&record, &retained_path)?
            .anchor()
            .map_err(|_| {
                BackendError::path_invalid("S5 retained applied leaf cannot be anchored")
            })?;
        let retained_locator = s5_locator(&record, &retained_path)
            .map_err(|_| BackendError::journal_invalid("S5 retained locator is invalid"))?;
        transition_s5_backend(
            &self.journals,
            &mut record,
            S5Transition::SetPendingRetainedArtifact(PendingRetainedArtifact {
                role: RetainedArtifactRole::DisplacedApplied,
                path: retained_path.to_string_lossy().into_owned(),
                locator: retained_locator,
                creation_state: PendingArtifactCreationState::Intent,
            }),
        )?;
        transition_s5_backend(
            &self.journals,
            &mut record,
            S5Transition::InitializePublication(Box::new(RetainedPublication {
                purpose: RetainedPublicationPurpose::Undo,
                canonical: canonical_path.to_string_lossy().into_owned(),
                retained: retained_path.to_string_lossy().into_owned(),
                backup: backup_path.to_string_lossy().into_owned(),
                publication_source: backup_path.to_string_lossy().into_owned(),
                expected_canonical_identity: current_identity,
                expected_canonical_sha256: current_hash,
                expected_backup_identity: backup_identity.clone(),
                expected_backup_sha256: backup_hash.clone(),
                publication_source_identity: backup_identity,
                publication_sha256: backup_hash,
                expected_canonical_security_sha256: current_snapshot.sha256(),
                expected_backup_security_sha256: backup_snapshot.sha256(),
                publication_security_sha256: backup_snapshot.sha256(),
                expected_canonical_default_stream_bytes: current_len,
                expected_canonical_auxiliary_charge_bytes: current_snapshot
                    .auxiliary_charge_bytes(),
                expected_canonical_logical_charge_bytes: current_len
                    .checked_add(current_snapshot.auxiliary_charge_bytes())
                    .ok_or_else(|| {
                        BackendError::validation("error.rfi.followUp.retainedStorageFull")
                    })?,
                destination_access_sha256: backup_snapshot.access_sha256(),
                destination_auxiliary_sha256: backup_snapshot.auxiliary_sha256(),
                destination_auxiliary_charge_bytes: backup_snapshot.auxiliary_charge_bytes(),
                retained_identity: None,
                retained_sha256: None,
                retained_security_sha256: None,
                created_canonical_identity: None,
                phase: RetainedPublicationPhase::Prepared,
            })),
        )?;
        let outcome = execute_retained_publication(
            &self.journals,
            &mut record,
            &canonical,
            &retained,
            &mut backup_file,
            &creation,
            &backup_snapshot,
            PublicationCancellation::NotCancellable,
            || Ok(()),
            |_| Ok(()),
        );
        self.finish_follow_up_undo(&mut record, step, outcome)
    }

    pub(in crate::fs_ops) fn restore_failed_follow_up_apply(
        &self,
        record: &mut ValidatedS5Record,
    ) -> Result<OperationResult, BackendError> {
        if record.undo_mode() != Some(S5UndoMode::RestoreFailedApply) {
            return Err(BackendError::journal_invalid(
                "S5 failed apply restore lease is missing",
            ));
        }
        let publication = record
            .pending_publication()
            .cloned()
            .filter(|publication| publication.purpose == RetainedPublicationPurpose::Apply)
            .ok_or_else(|| BackendError::journal_invalid("S5 failed apply is not pending"))?;
        let pending_retained = record
            .pending_retained_artifact()
            .cloned()
            .filter(|pending| {
                pending.role == RetainedArtifactRole::DisplacedOriginal
                    && pending.path == publication.retained
            })
            .ok_or_else(|| BackendError::journal_invalid("S5 retained original is not pending"))?;
        if record
            .retained_evidence_for(RetainedArtifactRole::DisplacedOriginal)
            .is_some_and(|evidence| {
                evidence.path != pending_retained.path
                    || evidence.identity != publication.expected_canonical_identity
                    || evidence.sha256 != publication.expected_canonical_sha256
                    || evidence.security_sha256 != publication.expected_canonical_security_sha256
            })
        {
            return Err(BackendError::journal_invalid(
                "S5 retained original evidence does not match publication authority",
            ));
        }
        let canonical_path = PathBuf::from(&publication.canonical);
        let retained_path = PathBuf::from(&publication.retained);
        let canonical = authorize_s5_recorded(record, &canonical_path)?
            .anchor()
            .map_err(|_| BackendError::path_invalid("S5 Follow Up path cannot be anchored"))?;
        let retained = authorize_s5_recorded(record, &retained_path)?
            .anchor()
            .map_err(|_| BackendError::path_invalid("S5 retained original cannot be anchored"))?;

        transition_s5_backend(
            &self.journals,
            record,
            S5Transition::BeginFailedApplyRestore,
        )?;

        match canonical.open_read() {
            Ok(mut current) => {
                let exact_current = is_expected_canonical_file(&mut current, &publication);
                let retained_absent = matches!(
                    retained.open_read(),
                    Err(ref error) if error.kind() == io::ErrorKind::NotFound
                );
                if exact_current && retained_absent && canonical.sync_parent().is_ok() {
                    return self.finalize_failed_follow_up_restore(record);
                }
                return Err(BackendError::validation(
                    "error.rfi.followUp.workbookChanged",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(BackendError::validation(
                    "error.rfi.followUp.workbookChanged",
                ));
            }
        }

        let mut retained_file = retained
            .open_read()
            .map_err(|_| BackendError::validation("error.rfi.followUp.recoveryRequired"))?;
        let retained_exact = is_expected_canonical_file(&mut retained_file, &publication);
        if !retained_exact {
            return Err(BackendError::validation(
                "error.rfi.followUp.workbookChanged",
            ));
        }
        drop(retained_file);

        retained
            .restore_original_name_no_clobber(&canonical)
            .map_err(|_| BackendError::validation("error.rfi.followUp.recoveryRequired"))?;
        let mut restored = canonical
            .open_read()
            .map_err(|_| BackendError::validation("error.rfi.followUp.recoveryRequired"))?;
        let restored_exact = is_expected_canonical_file(&mut restored, &publication)
            && matches!(
                retained.open_read(),
                Err(ref error) if error.kind() == io::ErrorKind::NotFound
            );
        if !restored_exact {
            return Err(BackendError::validation(
                "error.rfi.followUp.recoveryRequired",
            ));
        }
        self.finalize_failed_follow_up_restore(record)
    }

    pub(in crate::fs_ops) fn finalize_failed_follow_up_restore(
        &self,
        record: &mut ValidatedS5Record,
    ) -> Result<OperationResult, BackendError> {
        transition_s5_backend(
            &self.journals,
            record,
            S5Transition::FinalizeFailedApplyRestore,
        )?;
        Ok(committed_s5_result(record))
    }
}

/// True when `file` matches the canonical workbook captured before the failed apply.
fn is_expected_canonical_file(file: &mut File, publication: &RetainedPublication) -> bool {
    crate::cap_fs::file_identity(file).ok().as_ref()
        == Some(&publication.expected_canonical_identity)
        && sha256_open_file(file).ok().as_deref()
            == Some(publication.expected_canonical_sha256.as_str())
        && FileSecuritySnapshot::capture(file, None)
            .ok()
            .is_some_and(|snapshot| {
                snapshot.sha256() == publication.expected_canonical_security_sha256
            })
}

fn committed_s5_result(record: &ValidatedS5Record) -> OperationResult {
    let mut result = OperationResult {
        applied: 1,
        failed: 0,
        journal_id: record.journal_id().to_string(),
        errors: Vec::new(),
        meta: JsonObject::new(),
        cloud_synced: None,
    };
    insert_effect_state(&mut result, "committed");
    insert_retained_evidence(&mut result, record.retained_evidence());
    result
}

/// Reads the backup and canonical paths from the S5 step and checks them against the backup evidence.
fn s5_undo_paths(
    step: &Step,
    record: &ValidatedS5Record,
) -> Result<(PathBuf, PathBuf), BackendError> {
    let backup_path = PathBuf::from(
        step.src
            .as_deref()
            .ok_or_else(|| BackendError::journal_invalid("S5 backup path is missing"))?,
    );
    let canonical_path = PathBuf::from(
        step.dst
            .as_deref()
            .ok_or_else(|| BackendError::journal_invalid("S5 canonical path is missing"))?,
    );
    let backup_evidence = record
        .retained_evidence_for(RetainedArtifactRole::Backup)
        .ok_or_else(|| BackendError::journal_invalid("S5 backup evidence is missing"))?;
    if backup_evidence.path != backup_path.to_string_lossy() {
        return Err(BackendError::journal_invalid(
            "S5 backup evidence path is invalid",
        ));
    }
    Ok((backup_path, canonical_path))
}

fn s5_applied_expectations(
    step: &Step,
) -> Result<(String, crate::journal::FilesystemObjectIdentity), BackendError> {
    let hash = step
        .meta
        .get("appliedSha256")
        .and_then(Value::as_str)
        .ok_or_else(|| BackendError::journal_invalid("S5 applied hash is missing"))?
        .to_string();
    let identity = serde_json::from_value(
        step.meta
            .get("createdCanonicalIdentity")
            .cloned()
            .ok_or_else(|| BackendError::journal_invalid("S5 applied identity is missing"))?,
    )
    .map_err(|_| BackendError::journal_invalid("S5 applied identity is invalid"))?;
    Ok((hash, identity))
}

impl FilesystemAuthority {
    fn finish_follow_up_undo(
        &self,
        record: &mut ValidatedS5Record,
        step: Step,
        outcome: Result<RetainedPublicationOutcome, BackendError>,
    ) -> Result<OperationResult, BackendError> {
        match outcome {
            Ok(RetainedPublicationOutcome::Published { .. }) => {
                transition_s5_backend(
                    &self.journals,
                    record,
                    S5Transition::FinalizeUndoSuccess {
                        completed_step: step,
                        evidence: Vec::new(),
                    },
                )?;
                Ok(committed_s5_result(record))
            }
            Ok(RetainedPublicationOutcome::CancelledBeforeDisplacement) => {
                Err(BackendError::journal_invalid("S5 undo cannot be cancelled"))
            }
            Ok(RetainedPublicationOutcome::RecoveryRequired(notice)) => {
                let expected = record.revision();
                record.prepare_outcome_unknown(notice.clone())?;
                self.journals.write_s5_cas(record, expected)?;
                Ok(s5_recovery_result(record, notice))
            }
            Err(_) => {
                let locator = record.retained_locators()?.first().to_string();
                let notice = RecoveryNotice {
                    warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
                    locator,
                };
                let expected = record.revision();
                record.prepare_outcome_unknown(notice.clone())?;
                self.journals.write_s5_cas(record, expected)?;
                Ok(s5_recovery_result(record, notice))
            }
        }
    }
}
