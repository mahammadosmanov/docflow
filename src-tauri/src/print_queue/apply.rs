use super::*;

impl PrintService {
    pub fn apply<F>(
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
        if claim.claimed.origin != BackendOrigin::Rust
            || claim.claimed.plan.op_id != "files.batch_print"
        {
            return Err(BackendError::validation(
                "plan is not owned by the Rust print backend",
            ));
        }
        let binding = claim
            .claimed
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        self.revalidate_worksheet_plan(binding, caller, &claim.claimed.plan)?;
        let roots =
            self.grants
                .roots_for_plan(&binding.grant_ids, caller, &binding.operation_id)?;
        let root_identities = self.grants.root_identities_for_plan(
            &binding.grant_ids,
            caller,
            &binding.operation_id,
        )?;
        let token = self.cancellations.register(plan_id);
        let _registration = PrintCancellationRegistration {
            registry: &self.cancellations,
            plan_id,
        };
        let raw_jobs = claim
            .claimed
            .plan
            .meta
            .get("jobs")
            .cloned()
            .ok_or_else(|| BackendError::validation("print plan has no jobs"))?;
        let mut jobs: Vec<PrintJob> = serde_json::from_value(raw_jobs)
            .map_err(|_| BackendError::validation("print plan contains invalid jobs"))?;
        for (index, job) in jobs.iter_mut().enumerate() {
            job.normalize_legacy(index);
            batch_print::validate_job(job)?;
        }
        let (queue_root_identities, source_identities) =
            self.source_identities_for_plan(binding, caller, &jobs, &root_identities)?;
        let mut state = self.state_for_plan(
            &claim.claimed.plan,
            &jobs,
            caller,
            &queue_root_identities,
            &source_identities,
        )?;
        self.queues.begin(&state.queue_id)?;
        let _active_queue = PrintQueueActiveRegistration {
            store: &self.queues,
            queue_id: state.queue_id.clone(),
        };
        state.status = "running".to_string();
        self.queues.save(&mut state)?;
        let mut record = JournalRecord::fresh(
            "files.batch_print",
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
        for saved in &state.jobs {
            if let Some(evidence) = saved.cleanup_review.as_ref() {
                record
                    .cleanup_reviews
                    .extend(evidence.iter().map(|review| JournalCleanupReview {
                        job_id: saved.job_id.clone(),
                        warning_key: review.warning_key.clone(),
                        locator: review.locator.clone(),
                    }));
            }
        }
        self.write_journal(&record)?;

        let mut result = OperationResult {
            applied: 0,
            failed: 0,
            journal_id: String::new(),
            errors: Vec::new(),
            meta: JsonObject::new(),
            cloud_synced: None,
        };
        let mut paused = false;
        let mut cancelled_state = None;
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: 0,
            total: jobs.len() as u64,
            subject: String::new(),
        });
        for (index, job) in jobs.iter().enumerate() {
            if token.checkpoint().is_err() {
                paused = true;
                cancelled_state = Some(if record.steps.is_empty() {
                    JournalState::CancelledClean
                } else {
                    JournalState::CancelledPartial
                });
                result.errors.push(warning("warn.print.cancelled", ""));
                break;
            }
            progress(ProgressEvent {
                phase: "print_waiting".to_string(),
                done: index as u64,
                total: jobs.len() as u64,
                subject: display_name(&job.path),
            });
            let saved_index = state
                .jobs
                .iter()
                .position(|saved| saved.job_id == job.job_id)
                .ok_or_else(|| BackendError::validation("saved print job no longer exists"))?;
            let attempts_allowed = 1 + job.retries;
            let mut completed = false;
            for _ in 0..attempts_allowed {
                {
                    let saved = &mut state.jobs[saved_index];
                    saved.status = Some("printing".to_string());
                    saved.attempts = Some(saved.attempts.unwrap_or_default() + 1);
                    saved.error = Some(String::new());
                    state.current_job_id = Some(saved.job_id.clone());
                }
                self.queues.save(&mut state)?;
                let pending = journal_step(job);
                record.pending_step = Some(pending.clone());
                record.state = Some(JournalState::Applying);
                self.write_journal(&record)?;

                let submitted = self.submit_approved_job(job, binding, caller, token.clone());
                self.record_attempt_cleanup_review(
                    &mut state,
                    &mut record,
                    &mut result,
                    saved_index,
                    job,
                    &submitted,
                )?;
                match submitted {
                    Ok(_outcome) => {
                        let saved = &mut state.jobs[saved_index];
                        saved.status = Some("completed".to_string());
                        saved.completed_at = Some(utc_now());
                        saved.error = Some(String::new());
                        state.current_job_id = None;
                        record.steps.push(pending);
                        record.pending_step = None;
                        self.write_journal(&record)?;
                        self.queues.save(&mut state)?;
                        result.applied += 1;
                        completed = true;
                        break;
                    }
                    Err(error) if error.kind == PrintFailureKind::Cancelled => {
                        let saved = &mut state.jobs[saved_index];
                        saved.status = Some(
                            if error.uncertain {
                                "uncertain"
                            } else {
                                "cancelled"
                            }
                            .to_string(),
                        );
                        saved.error = Some(failure_evidence(&error));
                        state.current_job_id = None;
                        state.status = "paused".to_string();
                        record.pending_step = if error.uncertain { Some(pending) } else { None };
                        record.state = Some(if error.uncertain {
                            JournalState::RecoveryRequired
                        } else if record.steps.is_empty() {
                            JournalState::CancelledClean
                        } else {
                            JournalState::CancelledPartial
                        });
                        if !error.uncertain {
                            cancelled_state = record.state;
                        }
                        self.write_journal(&record)?;
                        self.queues.save(&mut state)?;
                        result.errors.push(warning(
                            if error.uncertain {
                                "warn.print.cancelledUncertain"
                            } else {
                                "warn.print.cancelled"
                            },
                            display_name(&job.path),
                        ));
                        paused = true;
                        break;
                    }
                    Err(error) if error.kind == PrintFailureKind::Uncertain => {
                        let saved = &mut state.jobs[saved_index];
                        saved.status = Some("uncertain".to_string());
                        saved.error = Some(failure_evidence(&error));
                        state.current_job_id = None;
                        state.status = "paused".to_string();
                        record.pending_step = Some(pending);
                        record.state = Some(JournalState::RecoveryRequired);
                        self.write_journal(&record)?;
                        self.queues.save(&mut state)?;
                        result.errors.push(warning(
                            "warn.print.stateUncertain",
                            display_name(&job.path),
                        ));
                        paused = true;
                        break;
                    }
                    Err(error) if error.kind == PrintFailureKind::SubmittedUnconfirmed => {
                        let saved = &mut state.jobs[saved_index];
                        saved.status = Some("completed_unconfirmed".to_string());
                        saved.completed_at = Some(utc_now());
                        saved.error = Some(failure_evidence(&error));
                        state.current_job_id = None;
                        let mut submitted_step = pending;
                        submitted_step.meta.insert(
                            "outcome".to_string(),
                            Value::String("submitted_unconfirmed".to_string()),
                        );
                        record.steps.push(submitted_step);
                        record.pending_step = None;
                        self.write_journal(&record)?;
                        self.queues.save(&mut state)?;
                        result.applied += 1;
                        result.errors.push(warning(
                            "warn.print.submittedUnconfirmed",
                            display_name(&job.path),
                        ));
                        completed = true;
                        break;
                    }
                    Err(error) => {
                        let saved = &mut state.jobs[saved_index];
                        saved.error = Some(failure_evidence(&error));
                        state.current_job_id = None;
                        record.pending_step = None;
                        self.write_journal(&record)?;
                        if saved.attempts.unwrap_or_default() < attempts_allowed
                            && retry_disposition_allows_retry(&error)
                            && token.checkpoint().is_ok()
                        {
                            saved.status = Some("pending".to_string());
                            self.queues.save(&mut state)?;
                            continue;
                        }
                        saved.status = Some("failed".to_string());
                        self.queues.save(&mut state)?;
                        result.failed += 1;
                        let redacted = redact_error(&error.message);
                        result.errors.push(if is_localized_print_error(&redacted) {
                            warning(&redacted, display_name(&job.path))
                        } else {
                            warning(
                                "error.print.failed",
                                format!("{}: {redacted}", display_name(&job.path)),
                            )
                        });
                        if job.on_error == "pause" {
                            result.errors.push(warning(
                                "error.print.queuePaused",
                                (jobs.len() - index - 1).to_string(),
                            ));
                            paused = true;
                        }
                        break;
                    }
                }
            }
            if paused {
                break;
            }
            if !completed && job.on_error != "skip" {
                break;
            }
        }

        state.current_job_id = None;
        let counts = queue_counts(&state.jobs);
        let resumable = ["pending", "failed", "cancelled", "uncertain"]
            .into_iter()
            .map(|status| counts.get(status).copied().unwrap_or_default())
            .sum::<u64>();
        state.status = queue_final_status(resumable, paused, &counts).to_string();
        self.queues.save(&mut state)?;
        if record.state != Some(JournalState::RecoveryRequired) {
            record.pending_step = None;
            record.state = Some(cancelled_state.unwrap_or({
                if result.failed == 0 && resumable == 0 {
                    JournalState::Committed
                } else if !record.steps.is_empty() {
                    JournalState::PartialCommitted
                } else {
                    JournalState::NoEffectRetryable
                }
            }));
            self.write_journal(&record)?;
        }
        let summary = state.summary();
        result.meta = serde_json::to_value(&summary)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        result
            .meta
            .insert("resumeAvailable".to_string(), json!(resumable > 0));
        result.meta.insert("nonUndoable".to_string(), json!(true));
        result
            .meta
            .insert("auditJournalId".to_string(), json!(record.journal_id));
        result.meta.insert(
            "effectState".to_string(),
            json!(journal_effect_state(record.state)),
        );
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: jobs.len() as u64 - counts.get("pending").copied().unwrap_or_default(),
            total: jobs.len() as u64,
            subject: String::new(),
        });
        Ok(result)
    }
}

impl PrintService {
    fn record_attempt_cleanup_review(
        &self,
        state: &mut QueueState,
        record: &mut JournalRecord,
        result: &mut OperationResult,
        saved_index: usize,
        job: &PrintJob,
        submitted: &Result<PrintSubmissionOutcome, PrintFailure>,
    ) -> Result<(), BackendError> {
        let cleanup_review = match submitted {
            Ok(outcome) => outcome.cleanup_review.as_ref(),
            Err(failure) => failure.cleanup_review.as_ref(),
        };
        let Some(review) = cleanup_review else {
            return Ok(());
        };
        let primary_classification = submission_primary_classification(submitted);
        result
            .errors
            .push(warning(review.warning_key, review.locator.clone()));
        let appended = if let Some(evidence) = state.jobs[saved_index].cleanup_review.as_mut() {
            evidence
                .append(review.warning_key, review.locator.clone())
                .unwrap_or(true)
        } else {
            state.jobs[saved_index].cleanup_review = Some(CleanupReviewEvidence::single(
                review.warning_key,
                review.locator.clone(),
            ));
            true
        };
        if appended {
            record.cleanup_reviews.push(JournalCleanupReview {
                job_id: job.job_id.clone(),
                warning_key: review.warning_key.to_string(),
                locator: review.locator.clone(),
            });
        }
        self.persist_cleanup_review(state, record, primary_classification, review)
    }
}

fn queue_final_status(
    resumable: u64,
    paused: bool,
    counts: &BTreeMap<String, u64>,
) -> &'static str {
    if resumable == 0 {
        "completed"
    } else if paused || counts.get("pending").copied().unwrap_or_default() > 0 {
        "paused"
    } else {
        "completed_with_errors"
    }
}

#[cfg(test)]
mod queue_final_status_tests {
    use super::queue_final_status;
    use std::collections::BTreeMap;

    #[test]
    fn empty_resumable_queue_is_completed() {
        assert_eq!(queue_final_status(0, true, &BTreeMap::new()), "completed");
    }

    #[test]
    fn paused_or_pending_queue_is_paused() {
        assert_eq!(queue_final_status(2, true, &BTreeMap::new()), "paused");
        let pending = BTreeMap::from([("pending".to_string(), 1)]);
        assert_eq!(queue_final_status(2, false, &pending), "paused");
    }

    #[test]
    fn otherwise_queue_completes_with_errors() {
        assert_eq!(
            queue_final_status(2, false, &BTreeMap::new()),
            "completed_with_errors"
        );
    }
}
