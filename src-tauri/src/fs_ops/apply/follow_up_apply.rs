use super::*;

impl FilesystemAuthority {
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
}
