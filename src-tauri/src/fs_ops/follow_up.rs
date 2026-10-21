use super::*;

impl FilesystemAuthority {
    pub(super) fn execute_follow_up_workbook_update(
        &self,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut ValidatedS5Record,
    ) -> Result<u64, FollowUpApplyFailure> {
        if record
            .plan()
            .is_none_or(|plan| plan.op_id != rfi_follow_up::OPERATION_ID)
        {
            return Err(follow_up_no_effect("error.rfi.followUp.workbookInvalid"));
        }
        let step = record.approved_step().clone();
        let binding = record
            .binding()
            .cloned()
            .ok_or_else(|| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
        let (source_path, target_path, backup_path) =
            rfi_follow_up::step_workbook_paths(&step).map_err(follow_up_backend_no_effect)?;
        let source_path = PathBuf::from(source_path);
        let target_path = PathBuf::from(target_path);
        let backup_path = PathBuf::from(backup_path);
        let source = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &source_path,
                PathIntent::Read,
            )
            .and_then(|path| {
                path.anchor()
                    .map_err(|_| BackendError::path_invalid("RFI Log workbook is unavailable"))
            })
            .map_err(follow_up_backend_no_effect)?;
        let canonical = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &target_path,
                PathIntent::Write,
            )
            .and_then(|path| {
                path.anchor()
                    .map_err(|_| BackendError::path_invalid("Follow Up workbook is unavailable"))
            })
            .map_err(follow_up_backend_no_effect)?;
        let backup = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &backup_path,
                PathIntent::Write,
            )
            .and_then(|path| {
                path.anchor()
                    .map_err(|_| BackendError::path_invalid("Follow Up backup cannot be anchored"))
            })
            .map_err(follow_up_backend_no_effect)?;
        let mut source_file = source
            .open_read()
            .map_err(|_| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
        let mut original_file = canonical
            .open_mutable()
            .map_err(|_| follow_up_no_effect("error.rfi.followUp.workbookLocked"))?;
        rfi_follow_up::revalidate_open_workbooks(&step, &mut source_file, &mut original_file)
            .map_err(follow_up_backend_no_effect)?;
        if token.checkpoint().is_err() {
            return Err(FollowUpApplyFailure::Cancelled);
        }
        let original_identity = crate::cap_fs::file_identity(&original_file)
            .map_err(|_| follow_up_no_effect("error.rfi.followUp.workbookChanged"))?;
        let original_len = original_file
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|_| follow_up_no_effect("error.rfi.followUp.workbookChanged"))?;
        let original_snapshot = FileSecuritySnapshot::capture(&original_file, None)
            .map_err(|_| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
        let parent_snapshot = canonical
            .capture_parent_security()
            .map_err(|_| follow_up_no_effect("error.rfi.followUp.workbookChanged"))?;
        let creation = original_snapshot
            .creation_descriptor(&parent_snapshot)
            .map_err(|_| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
        let original_hash = canonical
            .sha256()
            .map_err(|_| follow_up_no_effect("error.rfi.followUp.workbookChanged"))?;
        let canonical_parent_path = target_path
            .parent()
            .ok_or_else(|| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?
            .to_path_buf();
        let private_parent_path = canonical_parent_path.join(".docflow-backups");
        let usage = s5_recorded_retained_usage(record, &canonical_parent_path)
            .map_err(follow_up_backend_no_effect)?;
        let reservation = RetainedStorageReservation::for_apply(
            original_len,
            original_snapshot.auxiliary_charge_bytes(),
        )
        .map_err(|_| follow_up_no_effect("error.rfi.followUp.retainedStorageFull"))?;
        if usage
            .0
            .checked_add(reservation.file_count)
            .is_none_or(|count| count > MAX_S5_RETAINED_FILES)
            || usage
                .1
                .checked_add(reservation.logical_charge_bytes)
                .is_none_or(|bytes| bytes > MAX_S5_RETAINED_BYTES)
        {
            return Err(follow_up_no_effect(
                "error.rfi.followUp.retainedStorageFull",
            ));
        }

        let parent_locator = s5_locator(record, &private_parent_path)?;
        s5_transition(
            &self.journals,
            record,
            S5Transition::SetPendingArtifactParent(PendingArtifactParent {
                path: private_parent_path.to_string_lossy().into_owned(),
                locator: parent_locator,
            }),
        )?;
        let private_parent = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &private_parent_path,
                PathIntent::Write,
            )
            .map_err(follow_up_backend_partial)?;
        let parent_outcome = private_parent
            .create_artifact_parent_no_clobber()
            .map_err(|_| follow_up_outcome(record))?;
        if matches!(parent_outcome, ArtifactParentCreateOutcome::Created { .. }) {
            private_parent
                .anchor()
                .and_then(|path| path.sync_parent())
                .map_err(|_| follow_up_partial("error.rfi.followUp.recoveryRequired"))?;
        }
        s5_transition(
            &self.journals,
            record,
            S5Transition::ResolveArtifactParent(parent_outcome),
        )?;
        if record
            .pending_artifact_parent()
            .is_some_and(|_| record.phase() == Some(FollowUpJournalPhase::ArtifactParentReady))
        {
            return Err(follow_up_outcome(record));
        }

        let extension = target_path
            .extension()
            .and_then(|value| value.to_str())
            .ok_or_else(|| follow_up_partial("error.rfi.followUp.workbookInvalid"))?;
        let private_path = private_parent_path.join(format!(
            ".docflow-s5-private-output-{}-{}.{}",
            record.journal_id(),
            Uuid::new_v4().simple(),
            extension
        ));
        let private_locator = s5_locator(record, &private_path)?;
        s5_transition(
            &self.journals,
            record,
            S5Transition::SetPendingRetainedArtifact(PendingRetainedArtifact {
                role: RetainedArtifactRole::PrivateOutput,
                path: private_path.to_string_lossy().into_owned(),
                locator: private_locator.clone(),
                creation_state: PendingArtifactCreationState::Intent,
            }),
        )?;
        let private = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &private_path,
                PathIntent::Write,
            )
            .and_then(|path| {
                path.anchor().map_err(|_| {
                    BackendError::path_invalid("Follow Up private output cannot be anchored")
                })
            })
            .map_err(follow_up_backend_partial)?;
        let private_creation = CreationSecurityDescriptor::current_owner_only()
            .map_err(|_| follow_up_partial("error.rfi.followUp.workbookInvalid"))?;
        let (mut private_file, private_identity) = match private
            .create_new_secured_identified(&private_creation)
            .map_err(|_| follow_up_outcome(record))?
        {
            SecuredCreateOutcome::Created { file, identity } => (file, identity),
            SecuredCreateOutcome::CreatedIdentityUnknown { file } => {
                drop(file);
                s5_transition(
                    &self.journals,
                    record,
                    S5Transition::MarkPendingRetainedIdentityUnknown,
                )?;
                return Err(follow_up_outcome(record));
            }
        };
        s5_transition(
            &self.journals,
            record,
            S5Transition::MarkPendingRetainedCreated(private_identity.clone()),
        )?;
        rfi_follow_up::write_and_verify_private_output(
            &step,
            &mut original_file,
            &mut private_file,
        )
        .map_err(follow_up_backend_partial)?;
        private_file
            .sync_all()
            .map_err(|_| follow_up_partial("error.rfi.followUp.recoveryRequired"))?;
        let private_snapshot = private_file
            .security_snapshot()
            .map_err(|_| follow_up_partial("error.rfi.followUp.recoveryRequired"))?;
        let private_hash = private_file
            .sha256()
            .map_err(|_| follow_up_partial("error.rfi.followUp.recoveryRequired"))?;
        let private_len = private_file
            .len()
            .map_err(|_| follow_up_partial("error.rfi.followUp.recoveryRequired"))?;
        let private_evidence = retained_evidence(
            RetainedArtifactRole::PrivateOutput,
            &private_path,
            private_identity.clone(),
            private_hash.clone(),
            &private_snapshot,
            private_len,
            private_locator,
        )?;
        s5_transition(
            &self.journals,
            record,
            S5Transition::FinalizeRetainedArtifact {
                evidence: private_evidence,
                phase: FollowUpJournalPhase::PrivateOutputReady,
            },
        )?;
        if token.checkpoint().is_err() {
            return Err(FollowUpApplyFailure::Cancelled);
        }

        rfi_follow_up::revalidate_open_workbooks(&step, &mut source_file, &mut original_file)
            .map_err(follow_up_backend_partial)?;
        canonical
            .verify_parent_security(&parent_snapshot)
            .map_err(|_| follow_up_partial("error.rfi.followUp.workbookChanged"))?;
        let backup_locator = s5_locator(record, &backup_path)?;
        s5_transition(
            &self.journals,
            record,
            S5Transition::SetPendingRetainedArtifact(PendingRetainedArtifact {
                role: RetainedArtifactRole::Backup,
                path: backup_path.to_string_lossy().into_owned(),
                locator: backup_locator.clone(),
                creation_state: PendingArtifactCreationState::Intent,
            }),
        )?;
        let (mut backup_file, backup_identity) = match backup
            .create_new_secured_identified(&creation)
            .map_err(|_| follow_up_partial("error.rfi.followUp.backupExists"))?
        {
            SecuredCreateOutcome::Created { file, identity } => (file, identity),
            SecuredCreateOutcome::CreatedIdentityUnknown { file } => {
                drop(file);
                s5_transition(
                    &self.journals,
                    record,
                    S5Transition::MarkPendingRetainedIdentityUnknown,
                )?;
                return Err(follow_up_outcome(record));
            }
        };
        s5_transition(
            &self.journals,
            record,
            S5Transition::MarkPendingRetainedCreated(backup_identity.clone()),
        )?;
        original_snapshot
            .verify_created_access(&backup_file)
            .and_then(|_| original_snapshot.replay_and_verify_auxiliary(&mut backup_file))
            .map_err(|_| follow_up_partial("error.rfi.followUp.backupVerify"))?;
        original_file
            .seek(SeekFrom::Start(0))
            .and_then(|_| std::io::copy(&mut original_file, &mut backup_file).map(|_| ()))
            .and_then(|_| backup_file.sync_all())
            .map_err(|_| follow_up_partial("error.rfi.followUp.backupVerify"))?;
        original_snapshot
            .verify_complete(&backup_file)
            .map_err(|_| follow_up_partial("error.rfi.followUp.backupVerify"))?;
        let backup_hash = backup_file
            .sha256()
            .map_err(|_| follow_up_partial("error.rfi.followUp.backupVerify"))?;
        if backup_hash != original_hash {
            return Err(follow_up_partial("error.rfi.followUp.backupVerify"));
        }
        let backup_evidence = retained_evidence(
            RetainedArtifactRole::Backup,
            &backup_path,
            backup_identity.clone(),
            backup_hash.clone(),
            &original_snapshot,
            original_len,
            backup_locator,
        )?;
        s5_transition(
            &self.journals,
            record,
            S5Transition::FinalizeRetainedArtifact {
                evidence: backup_evidence,
                phase: FollowUpJournalPhase::BackupReady,
            },
        )?;
        if token.checkpoint().is_err() {
            return Err(FollowUpApplyFailure::Cancelled);
        }

        rfi_follow_up::revalidate_open_workbooks(&step, &mut source_file, &mut original_file)
            .map_err(follow_up_backend_partial)?;
        canonical
            .verify_parent_security(&parent_snapshot)
            .map_err(|_| follow_up_partial("error.rfi.followUp.workbookChanged"))?;
        let retained_path = canonical_parent_path.join(format!(
            ".docflow-s5-retained-original-{}-{}.{}",
            record.journal_id(),
            Uuid::new_v4().simple(),
            extension
        ));
        let retained_locator = s5_locator(record, &retained_path)?;
        let retained = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &retained_path,
                PathIntent::Write,
            )
            .and_then(|path| {
                path.anchor().map_err(|_| {
                    BackendError::path_invalid("Follow Up retained original cannot be anchored")
                })
            })
            .map_err(follow_up_backend_partial)?;
        s5_transition(
            &self.journals,
            record,
            S5Transition::SetPendingRetainedArtifact(PendingRetainedArtifact {
                role: RetainedArtifactRole::DisplacedOriginal,
                path: retained_path.to_string_lossy().into_owned(),
                locator: retained_locator,
                creation_state: PendingArtifactCreationState::Intent,
            }),
        )?;
        s5_transition(
            &self.journals,
            record,
            S5Transition::InitializePublication(Box::new(RetainedPublication {
                purpose: RetainedPublicationPurpose::Apply,
                canonical: target_path.to_string_lossy().into_owned(),
                retained: retained_path.to_string_lossy().into_owned(),
                backup: backup_path.to_string_lossy().into_owned(),
                publication_source: private_path.to_string_lossy().into_owned(),
                expected_canonical_identity: original_identity,
                expected_canonical_sha256: original_hash.clone(),
                expected_backup_identity: backup_identity,
                expected_backup_sha256: backup_hash,
                publication_source_identity: private_identity,
                publication_sha256: private_hash.clone(),
                expected_canonical_security_sha256: original_snapshot.sha256(),
                expected_backup_security_sha256: original_snapshot.sha256(),
                publication_security_sha256: private_snapshot.sha256(),
                expected_canonical_default_stream_bytes: original_len,
                expected_canonical_auxiliary_charge_bytes: original_snapshot
                    .auxiliary_charge_bytes(),
                expected_canonical_logical_charge_bytes: original_len
                    .checked_add(original_snapshot.auxiliary_charge_bytes())
                    .ok_or_else(|| follow_up_partial("error.rfi.followUp.retainedStorageFull"))?,
                destination_access_sha256: original_snapshot.access_sha256(),
                destination_auxiliary_sha256: original_snapshot.auxiliary_sha256(),
                destination_auxiliary_charge_bytes: original_snapshot.auxiliary_charge_bytes(),
                retained_identity: None,
                retained_sha256: None,
                retained_security_sha256: None,
                created_canonical_identity: None,
                phase: RetainedPublicationPhase::Prepared,
            })),
        )?;
        let source_for_publication = std::cell::RefCell::new(&mut source_file);
        let original_for_publication = std::cell::RefCell::new(&mut original_file);
        let outcome = execute_retained_publication(
            &self.journals,
            record,
            &canonical,
            &retained,
            &mut private_file,
            &creation,
            &original_snapshot,
            PublicationCancellation::Apply(token),
            || {
                let mut source = source_for_publication.borrow_mut();
                let mut original = original_for_publication.borrow_mut();
                rfi_follow_up::revalidate_open_workbooks(&step, &mut source, &mut original)
            },
            |created| {
                let mut original = original_for_publication.borrow_mut();
                rfi_follow_up::verify_patched_output(&step, &mut **original, created)
            },
        )
        .map_err(|_| follow_up_outcome(record))?;
        match outcome {
            RetainedPublicationOutcome::CancelledBeforeDisplacement => {
                Err(FollowUpApplyFailure::Cancelled)
            }
            RetainedPublicationOutcome::RecoveryRequired(notice) => {
                Err(FollowUpApplyFailure::OutcomeUnknown(Warning {
                    key: notice.warning_key,
                    subject: notice.locator,
                }))
            }
            RetainedPublicationOutcome::Published { created_identity } => {
                let mut source = source_for_publication.borrow_mut();
                let mut original = original_for_publication.borrow_mut();
                rfi_follow_up::revalidate_open_workbooks(&step, &mut source, &mut original)
                    .map_err(|_| follow_up_outcome(record))?;
                let completed = Step {
                    kind: StepKind::RestoreBackup,
                    src: Some(backup_path.to_string_lossy().into_owned()),
                    dst: Some(target_path.to_string_lossy().into_owned()),
                    note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
                    meta: serde_json::Map::from_iter([
                        ("originalSha256".to_string(), Value::String(original_hash)),
                        ("appliedSha256".to_string(), Value::String(private_hash)),
                        (
                            "createdCanonicalIdentity".to_string(),
                            serde_json::to_value(created_identity)
                                .map_err(|_| follow_up_outcome(record))?,
                        ),
                    ]),
                };
                s5_transition(
                    &self.journals,
                    record,
                    S5Transition::FinalizeApplySuccess {
                        completed_step: completed,
                        evidence: Vec::new(),
                    },
                )?;
                rfi_follow_up::changed_cell_count(&step).map_err(follow_up_backend_partial)
            }
        }
    }

    pub(super) fn cleanup_signed_page_directories(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        record: &mut JournalRecord,
        result: &mut OperationResult,
    ) -> Result<(), BackendError> {
        if !matches!(
            record.op_id.as_str(),
            "rfi.stage_signed_first_pages" | "rfi.normalize_pdf_orientation"
        ) {
            return Ok(());
        }
        let directories = record
            .steps
            .iter()
            .enumerate()
            .filter(|(_, step)| step.kind == StepKind::Mkdir)
            .map(|(index, step)| (index, step.clone()))
            .collect::<Vec<_>>();
        for (index, step) in directories.into_iter().rev() {
            let Some(directory) = step.dst.as_deref().map(PathBuf::from) else {
                continue;
            };
            record.pending_step = Some(step);
            self.journals.write(record)?;
            let removed = self
                .grants
                .authorize_capability_for_plan(
                    &binding.grant_ids,
                    caller,
                    &binding.operation_id,
                    &directory,
                    PathIntent::Write,
                )
                .and_then(|path| {
                    path.anchor()
                        .map_err(|_| BackendError::path_invalid("created directory is unavailable"))
                })
                .and_then(|directory| {
                    directory
                        .remove_dir()
                        .map_err(|_| BackendError::path_invalid("created directory is not empty"))
                })
                .is_ok();
            if removed {
                record.steps.remove(index);
                result.applied = result.applied.saturating_sub(1);
            }
            record.pending_step = None;
            self.journals.write(record)?;
        }
        Ok(())
    }

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

    pub(super) fn undo_follow_up(
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
            .cloned()
            .ok_or_else(|| BackendError::journal_invalid("S5 backup evidence is missing"))?;
        if backup_evidence.path != backup_path.to_string_lossy() {
            return Err(BackendError::journal_invalid(
                "S5 backup evidence path is invalid",
            ));
        }
        let expected_applied_hash = step
            .meta
            .get("appliedSha256")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::journal_invalid("S5 applied hash is missing"))?
            .to_string();
        let expected_applied_identity: crate::journal::FilesystemObjectIdentity =
            serde_json::from_value(
                step.meta
                    .get("createdCanonicalIdentity")
                    .cloned()
                    .ok_or_else(|| {
                        BackendError::journal_invalid("S5 applied identity is missing")
                    })?,
            )
            .map_err(|_| BackendError::journal_invalid("S5 applied identity is invalid"))?;
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
        if usage
            .0
            .checked_add(reservation.file_count)
            .is_none_or(|count| count > MAX_S5_RETAINED_FILES)
            || usage
                .1
                .checked_add(reservation.logical_charge_bytes)
                .is_none_or(|bytes| bytes > MAX_S5_RETAINED_BYTES)
        {
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
                locator: retained_locator.clone(),
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
        match outcome {
            Ok(RetainedPublicationOutcome::Published { .. }) => {
                transition_s5_backend(
                    &self.journals,
                    &mut record,
                    S5Transition::FinalizeUndoSuccess {
                        completed_step: step,
                        evidence: Vec::new(),
                    },
                )?;
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
                Ok(result)
            }
            Ok(RetainedPublicationOutcome::CancelledBeforeDisplacement) => {
                Err(BackendError::journal_invalid("S5 undo cannot be cancelled"))
            }
            Ok(RetainedPublicationOutcome::RecoveryRequired(notice)) => {
                let expected = record.revision();
                record.prepare_outcome_unknown(notice.clone())?;
                self.journals.write_s5_cas(&mut record, expected)?;
                Ok(s5_recovery_result(&record, notice))
            }
            Err(_) => {
                let locator = record.retained_locators()?.first().to_string();
                let notice = RecoveryNotice {
                    warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
                    locator,
                };
                let expected = record.revision();
                record.prepare_outcome_unknown(notice.clone())?;
                self.journals.write_s5_cas(&mut record, expected)?;
                Ok(s5_recovery_result(&record, notice))
            }
        }
    }

    pub(super) fn restore_failed_follow_up_apply(
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
                let exact_current = crate::cap_fs::file_identity(&current).ok().as_ref()
                    == Some(&publication.expected_canonical_identity)
                    && sha256_open_file(&mut current).ok().as_deref()
                        == Some(publication.expected_canonical_sha256.as_str())
                    && FileSecuritySnapshot::capture(&current, None)
                        .ok()
                        .is_some_and(|snapshot| {
                            snapshot.sha256() == publication.expected_canonical_security_sha256
                        });
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
        let retained_exact = crate::cap_fs::file_identity(&retained_file).ok().as_ref()
            == Some(&publication.expected_canonical_identity)
            && sha256_open_file(&mut retained_file).ok().as_deref()
                == Some(publication.expected_canonical_sha256.as_str())
            && FileSecuritySnapshot::capture(&retained_file, None)
                .ok()
                .is_some_and(|snapshot| {
                    snapshot.sha256() == publication.expected_canonical_security_sha256
                });
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
        let restored_exact = crate::cap_fs::file_identity(&restored).ok().as_ref()
            == Some(&publication.expected_canonical_identity)
            && sha256_open_file(&mut restored).ok().as_deref()
                == Some(publication.expected_canonical_sha256.as_str())
            && FileSecuritySnapshot::capture(&restored, None)
                .ok()
                .is_some_and(|snapshot| {
                    snapshot.sha256() == publication.expected_canonical_security_sha256
                })
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

    pub(super) fn finalize_failed_follow_up_restore(
        &self,
        record: &mut ValidatedS5Record,
    ) -> Result<OperationResult, BackendError> {
        transition_s5_backend(
            &self.journals,
            record,
            S5Transition::FinalizeFailedApplyRestore,
        )?;
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
        Ok(result)
    }
}
