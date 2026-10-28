use super::*;

mod follow_up_apply;
mod rfi_system_apply;

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
}
