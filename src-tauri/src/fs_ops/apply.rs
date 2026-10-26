use super::*;

impl FilesystemAuthority {
    pub(crate) fn apply_generated_binary_write<F, P>(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        expected_operation: &str,
        mut progress: F,
        producer: P,
    ) -> Result<OperationResult, BackendError>
    where
        F: FnMut(ProgressEvent),
        P: FnOnce() -> Result<Vec<u8>, GeneratedWriteFailure>,
    {
        let inspected = self.plans.resolve_bound(plan_id, caller)?;
        let step = inspected.plan.steps.first();
        let is_outlook = expected_operation == "rfi.outlook_archive_selected";
        let valid_generated_step = step.is_some_and(|step| {
            step.kind == StepKind::Write
                && !step.meta.contains_key("content")
                && !step.meta.contains_key("contentBase64")
                && step.meta.get("generatedBy").and_then(Value::as_str) == Some("outlook_msg")
        });
        if !is_outlook
            || inspected.plan.op_id != expected_operation
            || inspected.plan.steps.len() != 1
            || !valid_generated_step
        {
            return Err(BackendError::validation("generated write plan is invalid"));
        }
        let claim = self
            .plans
            .claim_for_apply_bound(plan_id, caller, &self.mutation_lock)?;
        let binding = claim
            .claimed
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        let roots =
            self.grants
                .roots_for_plan(&binding.grant_ids, caller, &binding.operation_id)?;
        let root_identities = self.grants.root_identities_for_plan(
            &binding.grant_ids,
            caller,
            &binding.operation_id,
        )?;
        let token = self.cancellations.register(plan_id);
        let _registration = CancellationRegistration {
            registry: &self.cancellations,
            request_id: plan_id,
        };
        let approved = claim.claimed.plan.steps[0].clone();
        let mut record = JournalRecord::fresh(
            expected_operation,
            &binding.owner_id,
            &binding.session_id,
            &claim.claimed.digest,
        );
        record.grant_roots = roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        record.grant_root_identities = root_identities;
        record.state = Some(JournalState::Applying);
        self.journals.write(&record)?;
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: 0,
            total: 1,
            subject: approved.src.clone().unwrap_or_default(),
        });
        if token.checkpoint().is_err() {
            record.state = Some(JournalState::CancelledClean);
            self.journals.write(&record)?;
            return Ok(operation_result_for_generated_cancelled(&record));
        }
        let bytes = match producer() {
            Ok(bytes) => bytes,
            Err(failure) => {
                let (key, outcome_unknown) = match failure {
                    GeneratedWriteFailure::NoEffect(key) => (key, false),
                    GeneratedWriteFailure::OutcomeUnknown(key) => (key, true),
                };
                record.state = Some(if outcome_unknown {
                    JournalState::RecoveryRequired
                } else {
                    JournalState::NoEffectRetryable
                });
                self.journals.write(&record)?;
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
                return Ok(result);
            }
        };
        if bytes.len() > MAX_OUTLOOK_MSG_BYTES || !bytes.starts_with(OUTLOOK_MSG_HEADER) {
            record.state = Some(JournalState::NoEffectRetryable);
            self.journals.write(&record)?;
            return Ok(operation_result_for_generated_failure(
                &record,
                "error.rfi.outlookArchive.invalidMsg",
                false,
            ));
        }
        if token.checkpoint().is_err() {
            record.state = Some(JournalState::CancelledClean);
            self.journals.write(&record)?;
            return Ok(operation_result_for_generated_cancelled(&record));
        }
        let destination = approved
            .dst
            .as_deref()
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| BackendError::validation("generated destination is missing"))?;
        let destination = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )?
            .anchor()
            .map_err(|_| BackendError::path_invalid("archive destination cannot be anchored"))?;
        let mut actual = approved;
        actual.src = None;
        actual.meta.remove("generatedBy");
        actual.meta.remove("contentBase64");
        self.journals.write(&record)?;
        if let Err(error) = destination.write_no_clobber(&bytes) {
            if error.kind() != io::ErrorKind::AlreadyExists && destination.open_read().is_ok() {
                record.state = Some(JournalState::RecoveryRequired);
                self.journals.write(&record)?;
                return Ok(operation_result_for_generated_failure(
                    &record,
                    "error.rfi.outlookArchive.writeUnknown",
                    true,
                ));
            }
            record.pending_step = None;
            record.state = Some(JournalState::NoEffectRetryable);
            self.journals.write(&record)?;
            return Ok(operation_result_for_generated_failure(
                &record,
                if error.kind() == io::ErrorKind::AlreadyExists {
                    "error.rfi.outlookArchive.destinationExists"
                } else {
                    "error.rfi.outlookArchive.writeFailed"
                },
                false,
            ));
        }
        let hash = if let Ok(hash) = destination.sha256() {
            hash
        } else {
            record.state = Some(JournalState::RecoveryRequired);
            self.journals.write(&record)?;
            return Ok(operation_result_for_generated_failure(
                &record,
                "error.rfi.outlookArchive.writeUnknown",
                true,
            ));
        };
        actual
            .meta
            .insert("undoSha256".to_string(), Value::String(hash));
        record.pending_step = None;
        record.steps.push(actual);
        record.state = Some(JournalState::Committed);
        self.journals.write(&record)?;
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        let mut result = OperationResult {
            applied: 1,
            failed: 0,
            journal_id: record.journal_id.clone(),
            errors: Vec::new(),
            meta: JsonObject::new(),
            cloud_synced: None,
        };
        insert_effect_state(&mut result, "committed");
        Ok(result)
    }

    pub fn apply_with_progress<F>(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        mut progress: F,
    ) -> Result<OperationResult, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let inspected = self.plans.resolve_bound(plan_id, caller)?;
        if inspected
            .plan
            .steps
            .iter()
            .any(|step| step.meta.contains_key("contentBase64"))
            && (inspected
                .binding
                .as_ref()
                .map(|binding| binding.operation_id.as_str())
                != Some(CONCRETE_DOCUMENT_OPERATION)
                || inspected.plan.op_id != CONCRETE_DOCUMENT_OPERATION
                || inspected.plan.steps.iter().any(|step| {
                    step.meta.contains_key("contentBase64")
                        && (step.kind != StepKind::Write
                            || step.note_key.as_deref() != Some(CONCRETE_DOCUMENT_NOTE)
                            || step.meta.contains_key("content")
                            || !step.meta.get("contentBase64").is_some_and(Value::is_string))
                }))
        {
            return Err(BackendError::validation(
                "encoded generated payload is not authorized for this operation",
            ));
        }
        if inspected.plan.op_id == rfi_follow_up::OPERATION_ID {
            if inspected.plan.steps.is_empty() {
                return Err(BackendError::validation(
                    "error.rfi.followUp.noRowsSelected",
                ));
            }
            return self.apply_follow_up(plan_id, caller, progress);
        }
        if matches!(
            inspected.plan.op_id.as_str(),
            rfi_system_sync::OPERATION_ID
                | rfi_system_sync::COMPLETE_STATUS_OPERATION_ID
                | rfi_system_sync::PUBLISH_STATUS_OPERATION_ID
        ) {
            if inspected.plan.steps.is_empty() {
                return Err(BackendError::validation(
                    if inspected.plan.op_id == rfi_system_sync::COMPLETE_STATUS_OPERATION_ID {
                        "warn.rfi.completeStatus.noChangesNeeded"
                    } else if inspected.plan.op_id == rfi_system_sync::PUBLISH_STATUS_OPERATION_ID {
                        "warn.rfi.publishStatus.noChangesNeeded"
                    } else {
                        "warn.rfi.systemSync.noChangesNeeded"
                    },
                ));
            }
            return self.apply_rfi_system_sync(plan_id, caller, progress);
        }
        let claim = self
            .plans
            .claim_for_apply_bound(plan_id, caller, &self.mutation_lock)?;
        if claim.claimed.origin != BackendOrigin::Rust {
            return Err(BackendError::validation(
                "plan is not owned by the Rust filesystem backend",
            ));
        }
        let binding = claim
            .claimed
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        let roots =
            self.grants
                .roots_for_plan(&binding.grant_ids, caller, &binding.operation_id)?;
        let root_identities = self.grants.root_identities_for_plan(
            &binding.grant_ids,
            caller,
            &binding.operation_id,
        )?;
        let token = self.cancellations.register(plan_id);
        let _registration = CancellationRegistration {
            registry: &self.cancellations,
            request_id: plan_id,
        };
        let mut record = JournalRecord::fresh(
            &claim.claimed.plan.op_id,
            &binding.owner_id,
            &binding.session_id,
            &claim.claimed.digest,
        );
        record.grant_roots = roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        record.grant_root_identities = root_identities;
        record.state = Some(JournalState::Applying);
        let mut result = OperationResult {
            applied: 0,
            failed: 0,
            journal_id: record.journal_id.clone(),
            errors: Vec::new(),
            meta: JsonObject::new(),
            cloud_synced: None,
        };
        let fail_fast = claim.claimed.plan.op_id == replica_publish::OPERATION_ID
            || (matches!(
                claim.claimed.plan.op_id.as_str(),
                "rfi.process_scans" | "rfi.process_scans_fast_covers"
            ) && claim
                .claimed
                .plan
                .steps
                .iter()
                .any(|step| step.kind == StepKind::PdfPagesExtract));
        if fail_fast {
            self.preflight_replica_steps(&claim.claimed.plan.steps, binding, caller)?;
        }
        let total = claim.claimed.plan.steps.len();
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: 0,
            total: total as u64,
            subject: String::new(),
        });
        for (index, step) in claim.claimed.plan.steps.iter().enumerate() {
            if token.checkpoint().is_err() {
                mark_cancelled(&mut record, &mut result, total - index);
                self.cleanup_signed_page_directories(binding, caller, &mut record, &mut result)?;
                self.journals.write(&record)?;
                return Ok(result);
            }
            match self.execute_step(step, binding, caller, &token, &mut record) {
                Ok(applied) => result.applied += applied,
                Err(StepFailure::NoEffect(subject)) => {
                    result.failed += 1;
                    result.errors.push(Warning {
                        key: "error.step_failed".to_string(),
                        subject,
                    });
                }
                Err(StepFailure::WarningNoEffect(warning)) => {
                    result.failed += 1;
                    result.errors.push(warning);
                }
                Err(StepFailure::OutcomeUnknown(subject)) => {
                    result.failed += 1;
                    result.errors.push(Warning {
                        key: "error.step_failed".to_string(),
                        subject,
                    });
                    record.state = Some(JournalState::RecoveryRequired);
                    insert_effect_state(&mut result, "outcome_unknown");
                    self.journals.write(&record)?;
                    return Ok(result);
                }
                Err(StepFailure::Cancelled) => {
                    mark_cancelled(&mut record, &mut result, total - index);
                    self.cleanup_signed_page_directories(
                        binding,
                        caller,
                        &mut record,
                        &mut result,
                    )?;
                    self.journals.write(&record)?;
                    return Ok(result);
                }
            }
            if fail_fast && result.failed > 0 {
                break;
            }
            progress(ProgressEvent {
                phase: "apply".to_string(),
                done: (index + 1) as u64,
                total: total as u64,
                subject: step
                    .src
                    .as_deref()
                    .or(step.dst.as_deref())
                    .unwrap_or_default()
                    .to_string(),
            });
        }
        if result.failed > 0 {
            self.cleanup_signed_page_directories(binding, caller, &mut record, &mut result)?;
        }
        record.state = Some(if result.failed == 0 {
            JournalState::Committed
        } else if result.applied > 0 || !record.steps.is_empty() {
            JournalState::PartialCommitted
        } else {
            JournalState::NoEffectRetryable
        });
        let effect_state = if result.failed == 0 {
            "committed"
        } else if result.applied > 0 || !record.steps.is_empty() {
            "partial_committed"
        } else {
            "no_effect_retryable"
        };
        insert_effect_state(&mut result, effect_state);
        self.journals.write(&record)?;
        Ok(result)
    }

    pub(super) fn apply_follow_up<F>(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        mut progress: F,
    ) -> Result<OperationResult, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let inspected = self.plans.resolve_bound(plan_id, caller)?;
        let binding = inspected
            .binding
            .clone()
            .ok_or_else(BackendError::plan_unavailable)?;
        let process = self.mutation_lock.try_acquire()?;
        let interprocess = self.journals.try_acquire_mutation_lock()?;
        let roots = self.grants.root_accesses_for_plan(
            &binding.grant_ids,
            caller,
            &binding.operation_id,
        )?;
        let permits = self.journals.bind_s5_permits(process, interprocess);
        let claim = self
            .plans
            .claim_s5_for_apply_bound_under_permit(plan_id, caller, permits, roots)?;
        let mut record = self.journals.begin_s5(claim, caller)?;
        let token = self.cancellations.register(plan_id);
        let _registration = CancellationRegistration {
            registry: &self.cancellations,
            request_id: plan_id,
        };
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: 0,
            total: 1,
            subject: String::new(),
        });
        let result = match self.execute_follow_up_workbook_update(caller, &token, &mut record) {
            Ok(changed) => {
                let mut result = OperationResult {
                    applied: changed,
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
            Err(FollowUpApplyFailure::NoEffect(warning)) => {
                let expected = record.revision();
                record.prepare_no_effect()?;
                self.journals.write_s5_cas(&mut record, expected)?;
                let mut result = OperationResult {
                    applied: 0,
                    failed: 1,
                    journal_id: record.journal_id().to_string(),
                    errors: vec![warning],
                    meta: JsonObject::new(),
                    cloud_synced: None,
                };
                insert_effect_state(&mut result, "no_effect_retryable");
                result
            }
            Err(FollowUpApplyFailure::Partial(warning)) => {
                let locators = record.retained_locators()?;
                let notice = RecoveryNotice {
                    warning_key: warning.key.clone(),
                    locator: locators.first().to_string(),
                };
                let expected = record.revision();
                record.prepare_failed_partial(notice)?;
                self.journals.write_s5_cas(&mut record, expected)?;
                let mut result = OperationResult {
                    applied: 0,
                    failed: 1,
                    journal_id: record.journal_id().to_string(),
                    errors: vec![warning],
                    meta: JsonObject::new(),
                    cloud_synced: None,
                };
                insert_effect_state(&mut result, "partial");
                insert_retained_locators(&mut result, locators.as_slice());
                insert_retained_evidence(&mut result, record.retained_evidence());
                result
            }
            Err(FollowUpApplyFailure::OutcomeUnknown(warning)) => {
                let notice = RecoveryNotice::from_warning(&warning);
                let expected = record.revision();
                record.prepare_outcome_unknown(notice)?;
                self.journals.write_s5_cas(&mut record, expected)?;
                let mut result = OperationResult {
                    applied: 0,
                    failed: 1,
                    journal_id: record.journal_id().to_string(),
                    errors: vec![warning],
                    meta: JsonObject::new(),
                    cloud_synced: None,
                };
                insert_effect_state(&mut result, "outcome_unknown");
                insert_retained_evidence(&mut result, record.retained_evidence());
                result
            }
            Err(FollowUpApplyFailure::Cancelled) => {
                let expected = record.revision();
                record.prepare_cancelled()?;
                self.journals.write_s5_cas(&mut record, expected)?;
                let partial = !record.retained_evidence().is_empty();
                let mut result = OperationResult {
                    applied: 0,
                    failed: 0,
                    journal_id: record.journal_id().to_string(),
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
                insert_effect_state(
                    &mut result,
                    if partial {
                        "cancelled_partial"
                    } else {
                        "cancelled_clean"
                    },
                );
                insert_retained_evidence(&mut result, record.retained_evidence());
                result
            }
        };
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        Ok(result)
    }

    pub(super) fn apply_rfi_system_sync<F>(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        mut progress: F,
    ) -> Result<OperationResult, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let claim = self
            .plans
            .claim_for_apply_bound(plan_id, caller, &self.mutation_lock)?;
        if claim.claimed.origin != BackendOrigin::Rust {
            return Err(BackendError::validation(
                "plan is not owned by the Rust filesystem backend",
            ));
        }
        let binding = claim
            .claimed
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        let roots =
            self.grants
                .roots_for_plan(&binding.grant_ids, caller, &binding.operation_id)?;
        let root_identities = self.grants.root_identities_for_plan(
            &binding.grant_ids,
            caller,
            &binding.operation_id,
        )?;
        let token = self.cancellations.register(plan_id);
        let _registration = CancellationRegistration {
            registry: &self.cancellations,
            request_id: plan_id,
        };
        let mut record = JournalRecord::fresh(
            &claim.claimed.plan.op_id,
            &binding.owner_id,
            &binding.session_id,
            &claim.claimed.digest,
        );
        record.grant_roots = roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        record.grant_root_identities = root_identities;
        record.state = Some(JournalState::Applying);
        self.journals.write(&record)?;

        let mut result = OperationResult {
            applied: 0,
            failed: 0,
            journal_id: record.journal_id.clone(),
            errors: Vec::new(),
            meta: JsonObject::new(),
            cloud_synced: None,
        };

        // Validate every approved workbook before creating a backup or changing
        // either canonical file. This prevents a stale two-workbook plan from
        // partially applying its first step.
        for step in &claim.claimed.plan.steps {
            let step_meta = rfi_system_sync::decode_step_meta(step)?;
            let workbook_path = PathBuf::from(
                step.src
                    .as_deref()
                    .ok_or_else(|| BackendError::validation("Step is missing source path"))?,
            );
            let backup_path = PathBuf::from(
                step.dst
                    .as_deref()
                    .ok_or_else(|| BackendError::validation("Step is missing backup path"))?,
            );
            if workbook_path.as_path() != Path::new(&step_meta.target_path)
                || backup_path.as_path() != Path::new(&step_meta.backup_path)
            {
                return Err(BackendError::validation(
                    "error.rfi.systemSync.approvedUpdateInvalid",
                ));
            }
            let capability = self.grants.authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &workbook_path,
                PathIntent::Write,
            )?;
            let anchor = capability
                .anchor()
                .map_err(|error| BackendError::validation(error.to_string()))?;
            let mut workbook = anchor
                .open_mutable()
                .map_err(|_| BackendError::validation("error.rfi.systemSync.workbookLocked"))?;
            if rfi_follow_up::fingerprint_open_file(&mut workbook)? != step_meta.source_fingerprint
            {
                return Err(BackendError::validation(
                    "error.rfi.systemSync.workbookChanged",
                ));
            }
        }

        let total_steps = claim.claimed.plan.steps.len();
        for (step_idx, step) in claim.claimed.plan.steps.iter().enumerate() {
            if token.checkpoint().is_err() {
                record.state = Some(JournalState::CancelledPartial);
                let _ = self.journals.write(&record);
                return Err(BackendError::stopped());
            }

            let src_path_str = step
                .src
                .as_deref()
                .ok_or_else(|| BackendError::validation("Step is missing source path"))?;
            let dst_path_str = step
                .dst
                .as_deref()
                .ok_or_else(|| BackendError::validation("Step is missing backup path"))?;
            let workbook_path = PathBuf::from(src_path_str);
            let backup_path = PathBuf::from(dst_path_str);

            let workbook_cap = self.grants.authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &workbook_path,
                PathIntent::Write,
            )?;
            let workbook_anchor = workbook_cap
                .anchor()
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let step_meta = rfi_system_sync::decode_step_meta(step)?;
            let mut current_workbook = workbook_anchor
                .open_mutable()
                .map_err(|_| BackendError::validation("error.rfi.systemSync.workbookLocked"))?;
            if rfi_follow_up::fingerprint_open_file(&mut current_workbook)?
                != step_meta.source_fingerprint
            {
                return Err(BackendError::validation(
                    "error.rfi.systemSync.workbookChanged",
                ));
            }
            drop(current_workbook);

            let backup_cap = self.grants.authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &backup_path,
                PathIntent::Write,
            )?;
            let backup_anchor = backup_cap
                .anchor()
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let actual_hash = workbook_anchor
                .sha256()
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let mut backup_step = Step {
                kind: StepKind::Copy,
                src: Some(workbook_path.to_string_lossy().into_owned()),
                dst: Some(backup_path.to_string_lossy().into_owned()),
                note_key: Some("note.rfi.systemSync.backup".to_string()),
                meta: JsonObject::new(),
            };
            backup_step.meta.insert(
                "sourceSha256".to_string(),
                Value::String(actual_hash.clone()),
            );
            record.pending_step = Some(backup_step.clone());
            self.journals.write(&record)?;

            if let Err(error) = backup_anchor.copy_no_clobber_from(&workbook_anchor) {
                record.pending_step = None;
                let _ = self.journals.write(&record);
                return Err(BackendError::validation(format!(
                    "Workbook backup failed for {}: {error}",
                    backup_path.display()
                )));
            }

            let backup_hash = backup_anchor
                .sha256()
                .map_err(|e| BackendError::validation(e.to_string()))?;
            if backup_hash != actual_hash {
                record.pending_step = None;
                let _ = self.journals.write(&record);
                return Err(BackendError::validation(
                    "Backup verification hash mismatch",
                ));
            }
            backup_step
                .meta
                .insert("undoSha256".to_string(), Value::String(backup_hash.clone()));
            record.steps.push(backup_step);
            record.pending_step = None;
            self.journals.write(&record)?;

            let workspace =
                tempfile::tempdir().map_err(|e| BackendError::validation(e.to_string()))?;
            let staged_input = workspace.path().join("staged-input.xlsx");
            let staged_output = workspace.path().join("staged-output.xlsx");

            fs::copy(&workbook_path, &staged_input)
                .map_err(|e| BackendError::validation(e.to_string()))?;

            rfi_system_sync::execute_sync_step(step, &staged_input, &staged_output)?;

            fs::copy(&staged_output, &workbook_path)
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let applied_hash = workbook_anchor
                .sha256()
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let mut restore_step = Step {
                kind: StepKind::RestoreBackup,
                src: Some(backup_path.to_string_lossy().into_owned()),
                dst: Some(workbook_path.to_string_lossy().into_owned()),
                note_key: Some("note.rfi.systemSync.restore".to_string()),
                meta: JsonObject::new(),
            };
            restore_step
                .meta
                .insert("originalSha256".to_string(), Value::String(backup_hash));
            restore_step
                .meta
                .insert("appliedSha256".to_string(), Value::String(applied_hash));
            record.steps.push(restore_step);
            self.journals.write(&record)?;

            result.applied += 1;
            progress(ProgressEvent {
                phase: "apply".to_string(),
                done: (step_idx + 1) as u64,
                total: total_steps as u64,
                subject: workbook_path.to_string_lossy().into_owned(),
            });
        }

        record.state = Some(JournalState::Committed);
        self.journals.write(&record)?;

        result.journal_id = record.journal_id;
        Ok(result)
    }
}
