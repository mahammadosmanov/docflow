use super::*;

mod undo;

impl FilesystemAuthority {
    /// Authorizes one workbook path under the approved plan and anchors it for I/O.
    fn authorize_workbook(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        path: &Path,
        intent: PathIntent,
        unavailable: &str,
    ) -> Result<AnchoredPath, FollowUpApplyFailure> {
        self.grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                path,
                intent,
            )
            .and_then(|path| {
                path.anchor()
                    .map_err(|_| BackendError::path_invalid(unavailable))
            })
            .map_err(follow_up_backend_no_effect)
    }

    /// Authorizes a private S5 artifact path for writing and anchors it for I/O.
    fn authorize_private_artifact(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        path: &Path,
        unavailable: &str,
    ) -> Result<AnchoredPath, FollowUpApplyFailure> {
        self.grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                path,
                PathIntent::Write,
            )
            .and_then(|path| {
                path.anchor()
                    .map_err(|_| BackendError::path_invalid(unavailable))
            })
            .map_err(follow_up_backend_partial)
    }

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
        let source = self.authorize_workbook(
            &binding,
            caller,
            &source_path,
            PathIntent::Read,
            "RFI Log workbook is unavailable",
        )?;
        let canonical = self.authorize_workbook(
            &binding,
            caller,
            &target_path,
            PathIntent::Write,
            "Follow Up workbook is unavailable",
        )?;
        let backup = self.authorize_workbook(
            &binding,
            caller,
            &backup_path,
            PathIntent::Write,
            "Follow Up backup cannot be anchored",
        )?;
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
        if s5_retained_budget_exceeded(usage, &reservation) {
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
        let private = self.authorize_private_artifact(
            &binding,
            caller,
            &private_path,
            "Follow Up private output cannot be anchored",
        )?;
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
        let retained = self.authorize_private_artifact(
            &binding,
            caller,
            &retained_path,
            "Follow Up retained original cannot be anchored",
        )?;
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
        .map_err(|error| {
            crate::leaf_transaction::note_failure("publication_error", &format!("{error:?}"));
            follow_up_outcome(record)
        })?;
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
}
