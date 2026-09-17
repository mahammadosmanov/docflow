use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    cap_fs::{
        AnchoredPath, ArtifactParentCreateOutcome, AuthorizedPath, CreationSecurityDescriptor,
        FileSecuritySnapshot, RetainedStorageReservation, SecuredCreateOutcome,
    },
    contracts::{JsonObject, OperationResult, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    journal::{
        FollowUpJournalPhase, JournalRecord, JournalState, JournalStore, MAX_S5_RETAINED_BYTES,
        MAX_S5_RETAINED_FILES, PendingArtifactCreationState, PendingArtifactParent,
        PendingRetainedArtifact, RecoveryNotice, RetainedArtifactEvidence, RetainedArtifactRole,
        RetainedPublication, RetainedPublicationPhase, RetainedPublicationPurpose, S5Transition,
        ValidatedS5Record,
    },
    leaf_transaction::{
        PublicationCancellation, RetainedPublicationOutcome, execute_retained_publication,
    },
    ooxml::patch_workbook_sheets,
    operations::replica_publish,
    paths::{
        CallerIdentity, GrantAccess, GrantScope, PathGrant, PathGrantStore, PathIntent,
        ProvisionalPathGrants,
    },
    pdf::{replace_first_page_incremental_from_page, rotate_pages_incremental},
    pdf_publish::temporary_pdf_name_matches,
    plan_store::{BackendOrigin, ClaimedPlan, PlanBinding, PlanStore, ReservedPlanId},
    progress::{CancellationRegistry, CancellationToken, GlobalMutationLock},
    rfi_follow_up,
    rfi_log::{verify_workbook_step, workbook_record_count, workbook_step_updates},
    rfi_system_sync,
};

#[cfg(test)]
use crate::pdf_publish::publish_pdf_no_clobber_with;

pub fn copy_no_clobber(source: &Path, destination: &Path) -> io::Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "destination has no parent"))?;
    let (temporary, mut output) = create_temporary(parent, destination)?;
    let result = (|| -> io::Result<()> {
        let mut input = open_source_no_follow(source)?;
        io::copy(&mut input, &mut output)?;
        output.set_permissions(input.metadata()?.permissions())?;
        output.sync_all()?;
        drop(output);
        publish_no_clobber(&temporary, destination)?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn write_no_clobber(destination: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "destination has no parent"))?;
    let (temporary, mut output) = create_temporary(parent, destination)?;
    let result = (|| -> io::Result<()> {
        output.write_all(contents)?;
        output.sync_all()?;
        drop(output);
        publish_no_clobber(&temporary, destination)?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn move_no_clobber(source: &Path, destination: &Path) -> io::Result<()> {
    match rename_no_replace(source, destination) {
        Ok(()) => {
            if let Some(parent) = destination.parent() {
                sync_directory(parent)?;
            }
            if let Some(parent) = source.parent()
                && destination.parent() != Some(parent)
            {
                sync_directory(parent)?;
            }
            Ok(())
        }
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
            copy_no_clobber(source, destination)?;
            fs::remove_file(source)?;
            if let Some(parent) = source.parent() {
                sync_directory(parent)?;
            }
            Ok(())
        }
        Err(error) => Err(error),
    }
}

pub fn file_sha256(path: &Path) -> io::Result<String> {
    let mut file = open_source_no_follow(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub struct FilesystemAuthority {
    plans: PlanStore,
    grants: PathGrantStore,
    journals: JournalStore,
    mutation_lock: GlobalMutationLock,
    cancellations: CancellationRegistry,
    #[cfg(test)]
    fail_after_effect: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    before_capability_effect: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

pub(crate) enum GeneratedWriteFailure {
    NoEffect(&'static str),
    OutcomeUnknown(&'static str),
}

const OUTLOOK_MSG_HEADER: &[u8; 8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";
const MAX_OUTLOOK_MSG_BYTES: usize = 100 * 1024 * 1024;

impl FilesystemAuthority {
    pub fn new(journal_dir: impl AsRef<Path>, ttl: Duration) -> Self {
        Self::with_mutation_lock(journal_dir, ttl, GlobalMutationLock::default())
    }

    pub fn with_mutation_lock(
        journal_dir: impl AsRef<Path>,
        ttl: Duration,
        mutation_lock: GlobalMutationLock,
    ) -> Self {
        Self {
            plans: PlanStore::new(ttl),
            grants: PathGrantStore::new(ttl),
            journals: JournalStore::new(journal_dir),
            mutation_lock,
            cancellations: CancellationRegistry::default(),
            #[cfg(test)]
            fail_after_effect: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            before_capability_effect: std::sync::Mutex::new(None),
        }
    }

    #[cfg(test)]
    fn inject_post_effect_failure(&self) {
        self.fail_after_effect
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(all(test, unix))]
    fn inject_before_capability_effect<F>(&self, hook: F)
    where
        F: FnOnce() + Send + 'static,
    {
        *self
            .before_capability_effect
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
    }

    #[cfg(test)]
    fn run_before_capability_effect(&self) {
        let hook = self
            .before_capability_effect
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(hook) = hook {
            hook();
        }
    }

    pub fn issue_grant(
        &self,
        scope: &GrantScope,
        selected_root: &Path,
        access: GrantAccess,
    ) -> Result<PathGrant, BackendError> {
        self.grants.issue(scope, selected_root, access)
    }

    pub(crate) fn issue_provisional_grants<'a>(
        &'a self,
        scope: &GrantScope,
        requests: &[(PathBuf, GrantAccess)],
    ) -> Result<ProvisionalPathGrants<'a>, BackendError> {
        self.grants.issue_provisional(scope, requests)
    }

    pub(crate) fn anchor_granted_path(
        &self,
        grant: &PathGrant,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<AnchoredPath, BackendError> {
        self.grants
            .authorize_capability_for_grant(grant, candidate, intent)?
            .anchor()
            .map_err(|_| BackendError::path_invalid("selected workbook cannot be anchored"))
    }

    pub(crate) fn reserve_plan_id(&self) -> ReservedPlanId {
        self.plans.reserve_plan_id()
    }

    pub(crate) fn cache_reserved_plan(
        &self,
        origin: BackendOrigin,
        binding: PlanBinding,
        reservation: ReservedPlanId,
        plan: Plan,
    ) -> Result<Plan, BackendError> {
        let caller = CallerIdentity::new(&binding.owner_id, &binding.session_id);
        self.grants
            .roots_for_plan(&binding.grant_ids, &caller, &binding.operation_id)?;
        self.plans
            .insert_bound_reserved(origin, binding, reservation, plan)
    }

    pub fn cache_plan(
        &self,
        origin: BackendOrigin,
        binding: PlanBinding,
        plan: Plan,
    ) -> Result<Plan, BackendError> {
        let caller = CallerIdentity::new(&binding.owner_id, &binding.session_id);
        self.grants
            .roots_for_plan(&binding.grant_ids, &caller, &binding.operation_id)?;
        self.plans.insert_bound(origin, binding, plan)
    }

    pub fn journals(&self) -> &JournalStore {
        &self.journals
    }

    pub fn plan_origin(&self, plan_id: &str) -> Option<BackendOrigin> {
        self.plans.snapshot(plan_id).map(|snapshot| snapshot.origin)
    }

    pub fn validate_plan_operation(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        operation_id: Option<&str>,
    ) -> Result<String, BackendError> {
        let plan = self.plans.resolve_bound(plan_id, caller)?;
        if plan.origin != BackendOrigin::Rust {
            return Err(BackendError::validation(
                "plan is not owned by the Rust filesystem backend",
            ));
        }
        if operation_id.is_some_and(|operation_id| operation_id != plan.plan.op_id) {
            return Err(BackendError::validation(
                "apply operation does not match the approved plan",
            ));
        }
        Ok(plan.plan.op_id)
    }

    pub fn resolve_native_plan(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
    ) -> Result<ClaimedPlan, BackendError> {
        let plan = self.plans.resolve_bound(plan_id, caller)?;
        if plan.origin != BackendOrigin::Rust {
            return Err(BackendError::validation(
                "plan is not owned by the Rust filesystem backend",
            ));
        }
        Ok(plan)
    }

    pub(crate) fn anchor_bound_path(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<AnchoredPath, BackendError> {
        self.grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                candidate,
                intent,
            )?
            .anchor()
            .map_err(|_| BackendError::path_invalid("bound workbook cannot be anchored"))
    }

    pub(crate) fn replace_cached_plan(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        plan_id: &str,
        expected_digest: &str,
        replacement: Plan,
    ) -> Result<Plan, BackendError> {
        self.grants
            .roots_for_plan(&binding.grant_ids, caller, &binding.operation_id)?;
        self.plans
            .replace_bound_available(plan_id, caller, expected_digest, replacement)
    }

    pub fn cancel(&self, plan_id: &str) -> bool {
        self.cancellations.cancel(plan_id)
    }

    pub fn revoke(&self, caller: &CallerIdentity) {
        self.plans.revoke(caller);
        self.grants.revoke(caller);
        self.cancellations.revoke_all();
    }

    pub fn apply(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
    ) -> Result<OperationResult, BackendError> {
        self.apply_with_progress(plan_id, caller, |_| {})
    }

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
        if inspected.plan.op_id != expected_operation
            || inspected.plan.steps.len() != 1
            || inspected.plan.steps[0].kind != StepKind::Write
            || inspected.plan.steps[0]
                .meta
                .get("generatedBy")
                .and_then(Value::as_str)
                != Some("outlook_msg")
            || inspected.plan.steps[0].meta.contains_key("content")
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
        record.pending_step = Some(actual.clone());
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
        let hash = match destination.sha256() {
            Ok(hash) => hash,
            Err(_) => {
                record.state = Some(JournalState::RecoveryRequired);
                self.journals.write(&record)?;
                return Ok(operation_result_for_generated_failure(
                    &record,
                    "error.rfi.outlookArchive.writeUnknown",
                    true,
                ));
            }
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
            rfi_system_sync::OPERATION_ID | rfi_system_sync::COMPLETE_STATUS_OPERATION_ID
        ) {
            if inspected.plan.steps.is_empty() {
                return Err(BackendError::validation(
                    if inspected.plan.op_id == rfi_system_sync::COMPLETE_STATUS_OPERATION_ID {
                        "warn.rfi.completeStatus.noChangesNeeded"
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
        let fail_fast = claim.claimed.plan.op_id == replica_publish::OPERATION_ID;
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

    fn apply_follow_up<F>(
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

    fn apply_rfi_system_sync<F>(
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

    fn execute_follow_up_workbook_update(
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

    fn cleanup_signed_page_directories(
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

    fn undo_follow_up(
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
        let roots = self.grants.reopen_recorded_roots(
            pending.recorded_root_accesses(),
            caller,
            rfi_follow_up::OPERATION_ID,
        )?;
        let mut record = self.journals.admit_s5_undo(pending, roots)?;
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

    fn execute_step(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        match step.kind {
            StepKind::Mkdir => self
                .execute_mkdir(step, binding, caller, token, record)
                .map(|()| 1),
            StepKind::Copy | StepKind::Move | StepKind::Rename | StepKind::Write => self
                .execute_file_effect(step, binding, caller, token, record)
                .map(|()| 1),
            StepKind::WorkbookUpdate => {
                self.execute_workbook_update(step, binding, caller, token, record)
            }
            StepKind::PdfFirstPageReplace => {
                self.execute_pdf_first_page_replace(step, binding, caller, token, record)
            }
            StepKind::PdfPageRotate => {
                self.execute_pdf_page_rotate(step, binding, caller, token, record)
            }
            StepKind::ReplicaPublish => {
                self.execute_replica_publish(step, binding, caller, token, record)
            }
            StepKind::CleanupTemp => {
                self.execute_cleanup_temp(step, binding, caller, token, record)
            }
            _ => Err(StepFailure::NoEffect(format!(
                "unsupported Rust filesystem action: {:?}",
                step.kind
            ))),
        }
    }

    fn preflight_replica_steps(
        &self,
        steps: &[Step],
        binding: &PlanBinding,
        caller: &CallerIdentity,
    ) -> Result<(), BackendError> {
        for step in steps {
            if step.kind != StepKind::ReplicaPublish {
                continue;
            }
            let meta = replica_publish::decode_step_meta(step)?;
            let source =
                PathBuf::from(step.src.as_deref().ok_or_else(|| {
                    BackendError::validation("error.replica.approvedStepInvalid")
                })?);
            let target =
                PathBuf::from(step.dst.as_deref().ok_or_else(|| {
                    BackendError::validation("error.replica.approvedStepInvalid")
                })?);
            let source = self
                .grants
                .authorize_capability_for_plan(
                    &binding.grant_ids,
                    caller,
                    &binding.operation_id,
                    &source,
                    PathIntent::Read,
                )?
                .anchor()
                .map_err(|_| BackendError::validation("error.replica.sourceChanged"))?;
            let target = self
                .grants
                .authorize_capability_for_plan(
                    &binding.grant_ids,
                    caller,
                    &binding.operation_id,
                    &target,
                    PathIntent::Write,
                )?
                .anchor()
                .map_err(|_| BackendError::validation("error.replica.targetChanged"))?;
            if source
                .sha256()
                .map_err(|_| BackendError::validation("error.replica.sourceChanged"))?
                != meta.source_sha256
            {
                return Err(BackendError::validation("error.replica.sourceChanged"));
            }
            if target
                .sha256()
                .map_err(|_| BackendError::validation("error.replica.targetChanged"))?
                != meta.expected_target_sha256
            {
                return Err(BackendError::validation("error.replica.targetChanged"));
            }
        }
        Ok(())
    }

    fn execute_replica_publish(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        let meta = replica_publish::decode_step_meta(step).map_err(step_no_effect)?;
        let source_path = required_path(step.src.as_deref(), "replica source")?;
        let target_path = required_path(step.dst.as_deref(), "replica target")?;
        let backup_path = PathBuf::from(&meta.backup_path);
        let source = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &source_path,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let target = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &target_path,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let backup = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &backup_path,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        if source.sha256().map_err(io_no_effect)? != meta.source_sha256 {
            return Err(StepFailure::NoEffect(
                "error.replica.sourceChanged".to_string(),
            ));
        }
        if target.sha256().map_err(io_no_effect)? != meta.expected_target_sha256 {
            return Err(StepFailure::NoEffect(
                "error.replica.targetChanged".to_string(),
            ));
        }
        match backup.open_read() {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(StepFailure::NoEffect(
                    "error.replica.backupExists".to_string(),
                ));
            }
            Err(error) => return Err(io_no_effect(error)),
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        backup.copy_no_clobber_from(&target).map_err(io_no_effect)?;
        if backup.sha256().map_err(io_no_effect)? != meta.expected_target_sha256 {
            return Err(StepFailure::OutcomeUnknown(
                "error.replica.backupVerificationFailed".to_string(),
            ));
        }

        let temporary = target.temporary_sibling("replica").map_err(io_no_effect)?;
        temporary
            .copy_no_clobber_from(&source)
            .map_err(io_no_effect)?;
        if temporary.sha256().map_err(io_no_effect)? != meta.source_sha256 {
            let _ = temporary.remove_file();
            return Err(StepFailure::NoEffect(
                "error.replica.copyVerificationFailed".to_string(),
            ));
        }
        if token.checkpoint().is_err() {
            let _ = temporary.remove_file();
            return Err(StepFailure::Cancelled);
        }
        let mut restore = Step {
            kind: StepKind::RestoreBackup,
            src: Some(meta.backup_path.clone()),
            dst: Some(target_path.to_string_lossy().into_owned()),
            note_key: Some("note.replica.restore".to_string()),
            meta: JsonObject::new(),
        };
        restore.meta.insert(
            "originalSha256".to_string(),
            Value::String(meta.expected_target_sha256.clone()),
        );
        restore.meta.insert(
            "appliedSha256".to_string(),
            Value::String(meta.source_sha256.clone()),
        );
        record.pending_step = Some(restore.clone());
        self.journals.write(record).map_err(step_unknown)?;
        if let Err(error) = atomic_replace_capability_with_retry(&temporary, &target) {
            return Err(replacement_failure(&target, error));
        }
        match target.sha256() {
            Ok(hash) if hash == meta.source_sha256 => {}
            Ok(_) | Err(_) => {
                if rollback_from_backup_capability(&backup, &target, &meta.source_sha256).is_ok() {
                    record.pending_step = None;
                    self.journals.write(record).map_err(step_unknown)?;
                    return Err(StepFailure::NoEffect(
                        "error.replica.publishVerificationFailed".to_string(),
                    ));
                }
                return Err(StepFailure::OutcomeUnknown(
                    "error.replica.publishVerificationFailed".to_string(),
                ));
            }
        }
        record.steps.push(restore);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }

    fn execute_mkdir(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<(), StepFailure> {
        let destination = required_path(step.dst.as_deref(), "mkdir destination")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        self.create_missing_directories(&destination, step, binding, caller, token, record)
    }

    fn create_missing_directories(
        &self,
        destination: &Path,
        template: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<(), StepFailure> {
        let authorized = self
            .grants
            .authorize_directory_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let mut missing = authorized.missing_directories().map_err(io_no_effect)?;
        while let Some(directory) = missing.next_path().map(Path::to_path_buf) {
            token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
            let actual = Step {
                kind: StepKind::Mkdir,
                src: None,
                dst: Some(directory.to_string_lossy().into_owned()),
                note_key: template.note_key.clone(),
                meta: template.meta.clone(),
            };
            record.pending_step = Some(actual.clone());
            record.state = Some(JournalState::Applying);
            self.journals.write(record).map_err(step_unknown)?;
            #[cfg(test)]
            self.run_before_capability_effect();
            if let Err(error) = missing.create_next() {
                if error.is_outcome_unknown() {
                    return Err(StepFailure::OutcomeUnknown(format!(
                        "{}: {}",
                        directory.display(),
                        error.into_inner()
                    )));
                }
                record.pending_step = None;
                self.journals.write(record).map_err(step_unknown)?;
                return Err(StepFailure::NoEffect(format!(
                    "could not create {}: {}",
                    directory.display(),
                    error.into_inner()
                )));
            }
            record.steps.push(actual);
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
        }
        Ok(())
    }

    fn execute_file_effect(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<(), StepFailure> {
        let destination = required_path(step.dst.as_deref(), "step destination")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let source = if step.kind == StepKind::Write {
            None
        } else {
            let source = required_path(step.src.as_deref(), "step source")?;
            let intent = if step.kind == StepKind::Copy {
                PathIntent::Read
            } else {
                PathIntent::Write
            };
            Some(
                self.grants
                    .authorize_for_plan(
                        &binding.grant_ids,
                        caller,
                        &binding.operation_id,
                        &source,
                        intent,
                    )
                    .map_err(step_no_effect)?,
            )
        };
        if source.as_deref() == Some(destination.as_path()) {
            return Err(StepFailure::NoEffect(
                "source and destination are the same path".to_string(),
            ));
        }
        let parent = destination.parent().ok_or_else(|| {
            StepFailure::NoEffect("destination has no parent directory".to_string())
        })?;
        self.create_missing_directories(parent, step, binding, caller, token, record)?;
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        let mut actual = step.clone();
        actual.src = source
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        actual.dst = Some(destination.to_string_lossy().into_owned());
        actual.meta.remove("content");
        let source_anchor = source
            .as_deref()
            .map(|source| {
                self.grants
                    .authorize_capability_for_plan(
                        &binding.grant_ids,
                        caller,
                        &binding.operation_id,
                        source,
                        if step.kind == StepKind::Copy {
                            PathIntent::Read
                        } else {
                            PathIntent::Write
                        },
                    )
                    .map_err(step_no_effect)?
                    .anchor()
                    .map_err(io_no_effect)
            })
            .transpose()?;
        let destination_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        if let Some(source) = source_anchor.as_ref() {
            if let Some(expected) = step
                .meta
                .get("expectedSourceSha256")
                .and_then(Value::as_str)
            {
                let hash = source.sha256().map_err(io_no_effect)?;
                if hash != expected {
                    return Err(StepFailure::NoEffect(
                        "copy source changed after preview; preview again".to_string(),
                    ));
                }
                actual
                    .meta
                    .insert("sourceSha256".to_string(), Value::String(hash));
            } else if let Ok(hash) = source.sha256() {
                actual
                    .meta
                    .insert("sourceSha256".to_string(), Value::String(hash));
            }
        }
        record.pending_step = Some(actual.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;
        #[cfg(test)]
        self.run_before_capability_effect();
        let effect = match step.kind {
            StepKind::Copy => destination_anchor
                .copy_no_clobber_from(source_anchor.as_ref().expect("copy source")),
            StepKind::Move | StepKind::Rename => source_anchor
                .as_ref()
                .expect("move source")
                .move_no_clobber_to(&destination_anchor),
            StepKind::Write => {
                let Some(contents) = step.meta.get("content").and_then(Value::as_str) else {
                    record.pending_step = None;
                    self.journals.write(record).map_err(step_unknown)?;
                    return Err(StepFailure::NoEffect(
                        "write action has no string content".to_string(),
                    ));
                };
                destination_anchor.write_no_clobber(contents.as_bytes())
            }
            _ => unreachable!("file effect kind checked by caller"),
        };
        if let Err(error) = effect {
            let outcome_unknown = error.kind() != io::ErrorKind::AlreadyExists
                && destination_anchor.open_read().is_ok();
            if outcome_unknown {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    destination.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(io_no_effect(error));
        }
        #[cfg(test)]
        if self
            .fail_after_effect
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(StepFailure::OutcomeUnknown(
                "injected failure after filesystem effect".to_string(),
            ));
        }
        let hash = destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })?;
        actual
            .meta
            .insert("undoSha256".to_string(), Value::String(hash));
        record.steps.push(actual);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(())
    }

    fn execute_workbook_update(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        let workbook = required_path(step.src.as_deref(), "workbook source")?;
        let workbook = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &workbook,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let backup = required_path(step.dst.as_deref(), "workbook backup")?;
        let backup = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &backup,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        if workbook == backup {
            return Err(StepFailure::NoEffect(
                "workbook and backup are the same path".to_string(),
            ));
        }
        let signature = step
            .meta
            .get("sourceSignature")
            .and_then(Value::as_object)
            .ok_or_else(|| StepFailure::NoEffect("workbook signature is missing".to_string()))?;
        let expected_size = signature
            .get("size")
            .and_then(Value::as_u64)
            .ok_or_else(|| StepFailure::NoEffect("workbook size is missing".to_string()))?;
        let expected_hash = signature
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| StepFailure::NoEffect("workbook hash is missing".to_string()))?;
        let workbook_capability = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &workbook,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let workbook_anchor = workbook_capability.anchor().map_err(io_no_effect)?;
        let actual_size = workbook_anchor.len().map_err(io_no_effect)?;
        let actual_hash = workbook_anchor.sha256().map_err(io_no_effect)?;
        if actual_size != expected_size || actual_hash != expected_hash {
            return Err(StepFailure::NoEffect(
                "workbook changed after preview".to_string(),
            ));
        }
        workbook_anchor.open_mutable().map(drop).map_err(|_| {
            StepFailure::WarningNoEffect(Warning {
                key: "error.rfi.workbookLocked".to_string(),
                subject: workbook
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string(),
            })
        })?;

        let backup_parent = backup
            .parent()
            .ok_or_else(|| StepFailure::NoEffect("workbook backup has no parent".to_string()))?;
        self.create_missing_directories(backup_parent, step, binding, caller, token, record)?;
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;

        let mut backup_step = Step {
            kind: StepKind::Copy,
            src: Some(workbook.to_string_lossy().into_owned()),
            dst: Some(backup.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.workbookUpdate".to_string()),
            meta: JsonObject::new(),
        };
        backup_step.meta.insert(
            "sourceSha256".to_string(),
            Value::String(actual_hash.clone()),
        );
        record.pending_step = Some(backup_step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;
        let backup_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &backup,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        if let Err(error) = backup_anchor.copy_no_clobber_from(&workbook_anchor) {
            if error.kind() != io::ErrorKind::AlreadyExists && backup_anchor.open_read().is_ok() {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    backup.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::NoEffect(format!(
                "workbook backup {}: {error}",
                backup.display()
            )));
        }
        let backup_hash = backup_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", backup.display()))
        })?;
        if backup_hash != actual_hash {
            return Err(StepFailure::OutcomeUnknown(
                "workbook backup verification failed".to_string(),
            ));
        }
        backup_step
            .meta
            .insert("undoSha256".to_string(), Value::String(backup_hash));
        record.steps.push(backup_step);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;

        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        let workspace = tempfile::tempdir().map_err(io_no_effect)?;
        let workspace_capability =
            crate::cap_fs::CapabilityRoot::open(workspace.path()).map_err(io_no_effect)?;
        let input = workspace.path().join("approved-input.xlsm");
        let output = workspace.path().join("completed-output.xlsm");
        let input_anchor = workspace_capability
            .authorize(&input)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        input_anchor
            .copy_no_clobber_from(&workbook_anchor)
            .map_err(|error| {
                StepFailure::NoEffect(format!("workbook staging {}: {error}", input.display()))
            })?;
        let updates = workbook_step_updates(step).map_err(step_no_effect)?;
        if let Err(error) = patch_workbook_sheets(&input, &output, &updates, Some(1)) {
            return Err(StepFailure::NoEffect(format!(
                "workbook patch {}: {error}",
                output.display()
            )));
        }
        if let Err(error) = verify_workbook_step(step, &output) {
            return Err(StepFailure::NoEffect(format!(
                "workbook verification {}: {}",
                output.display(),
                error.message
            )));
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&output)
            .and_then(|file| file.sync_all())
            .map_err(|error| {
                StepFailure::NoEffect(format!("workbook sync {}: {error}", output.display()))
            })?;
        let output_anchor = workspace_capability
            .authorize(&output)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let output_hash = output_anchor.sha256().map_err(|error| {
            StepFailure::NoEffect(format!("workbook hash {}: {error}", output.display()))
        })?;
        if token.checkpoint().is_err() {
            return Err(StepFailure::Cancelled);
        }

        let mut restore_step = Step {
            kind: StepKind::RestoreBackup,
            src: Some(backup.to_string_lossy().into_owned()),
            dst: Some(workbook.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.restoreBackup".to_string()),
            meta: JsonObject::new(),
        };
        restore_step
            .meta
            .insert("originalSha256".to_string(), Value::String(actual_hash));
        record.pending_step = Some(restore_step.clone());
        self.journals.write(record).map_err(step_unknown)?;
        if let Err(error) = atomic_replace_capability_with_retry(&output_anchor, &workbook_anchor) {
            return Err(replacement_failure(&workbook_anchor, error));
        }
        let applied_hash = match workbook_anchor.sha256() {
            Ok(hash) => hash,
            Err(error) => {
                if rollback_from_backup_capability(&backup_anchor, &workbook_anchor, expected_hash)
                    .is_ok()
                {
                    record.pending_step = None;
                    self.journals.write(record).map_err(step_unknown)?;
                    return Err(io_no_effect(error));
                }
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    workbook.display()
                )));
            }
        };
        if applied_hash != output_hash {
            if rollback_from_backup_capability(&backup_anchor, &workbook_anchor, expected_hash)
                .is_ok()
            {
                record.pending_step = None;
                self.journals.write(record).map_err(step_unknown)?;
                return Err(StepFailure::NoEffect(
                    "workbook replacement verification failed".to_string(),
                ));
            }
            return Err(StepFailure::OutcomeUnknown(
                "workbook replacement verification failed".to_string(),
            ));
        }
        restore_step
            .meta
            .insert("appliedSha256".to_string(), Value::String(applied_hash));
        record.steps.push(restore_step);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        workbook_record_count(step).map_err(step_no_effect)
    }

    fn execute_pdf_first_page_replace(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        let archive = required_path(step.src.as_deref(), "archive PDF")?;
        let archive = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &archive,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?;
        let signed = step
            .meta
            .get("signedPdf")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| StepFailure::NoEffect("signed PDF path is missing".to_string()))?;
        let signed = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &signed,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?;
        let destination = required_path(step.dst.as_deref(), "review PDF destination")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let expected_archive_hash = step
            .meta
            .get("archiveSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("archive PDF hash is missing".to_string()))?;
        let expected_signed_hash = step
            .meta
            .get("signedSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("signed PDF hash is missing".to_string()))?;
        let signed_page_index = step
            .meta
            .get("signedPageIndex")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        StepFailure::NoEffect("signed PDF page index is invalid".to_string())
                    })
            })
            .transpose()?
            .unwrap_or(0);
        let archive_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &archive,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let signed_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &signed,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let destination_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        if archive_anchor.sha256().map_err(io_no_effect)? != expected_archive_hash {
            return Err(StepFailure::NoEffect(
                "archive PDF changed after preview".to_string(),
            ));
        }
        if signed_anchor.sha256().map_err(io_no_effect)? != expected_signed_hash {
            return Err(StepFailure::NoEffect(
                "signed PDF changed after preview".to_string(),
            ));
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        record.pending_step = Some(step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;

        let workspace = tempfile::tempdir().map_err(io_no_effect)?;
        let workspace_capability =
            crate::cap_fs::CapabilityRoot::open(workspace.path()).map_err(io_no_effect)?;
        let archive_copy = workspace.path().join("archive.pdf");
        let signed_copy = workspace.path().join("signed.pdf");
        let completed = workspace.path().join("completed.pdf");
        let archive_copy_anchor = workspace_capability
            .authorize(&archive_copy)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let signed_copy_anchor = workspace_capability
            .authorize(&signed_copy)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        archive_copy_anchor
            .copy_no_clobber_from(&archive_anchor)
            .map_err(io_no_effect)?;
        signed_copy_anchor
            .copy_no_clobber_from(&signed_anchor)
            .map_err(io_no_effect)?;
        let generated = replace_first_page_incremental_from_page(
            &archive_copy,
            &signed_copy,
            signed_page_index,
            &completed,
        )
        .map_err(|error| StepFailure::NoEffect(error.to_string()));
        if let Err(error) = generated {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(error);
        }
        let completed_anchor = workspace_capability
            .authorize(&completed)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let output_hash = completed_anchor.sha256().map_err(io_no_effect)?;
        if token.checkpoint().is_err() {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::Cancelled);
        }
        if archive_anchor.sha256().map_err(io_no_effect)? != expected_archive_hash
            || signed_anchor.sha256().map_err(io_no_effect)? != expected_signed_hash
        {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::NoEffect(
                "approved PDF input changed during apply".to_string(),
            ));
        }
        if let Err(error) = destination_anchor.copy_no_clobber_from(&completed_anchor) {
            if error.kind() != io::ErrorKind::AlreadyExists
                && destination_anchor.open_read().is_ok()
            {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    destination.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(io_no_effect(error));
        }
        if destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })? != output_hash
        {
            return Err(StepFailure::OutcomeUnknown(
                "published review PDF failed hash validation".to_string(),
            ));
        }
        let mut actual = Step {
            kind: StepKind::Copy,
            src: Some(archive.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: step.note_key.clone(),
            meta: step.meta.clone(),
        };
        actual
            .meta
            .insert("undoSha256".to_string(), Value::String(output_hash));
        record.steps.push(actual);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }

    fn execute_pdf_page_rotate(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        if binding.operation_id != "rfi.normalize_pdf_orientation" {
            return Err(StepFailure::NoEffect(
                "PDF rotation is not authorized for this operation".to_string(),
            ));
        }
        let source = required_path(step.src.as_deref(), "source PDF")?;
        let source = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &source,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?;
        let destination = required_path(step.dst.as_deref(), "orientation review PDF")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let expected_hash = step
            .meta
            .get("sourceSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("source PDF hash is missing".to_string()))?;
        let rotations = step
            .meta
            .get("rotations")
            .and_then(Value::as_array)
            .ok_or_else(|| StepFailure::NoEffect("approved PDF rotations are missing".to_string()))?
            .iter()
            .map(|rotation| {
                let page_index = rotation
                    .get("pageIndex")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        StepFailure::NoEffect("approved page index is invalid".to_string())
                    })?;
                let target = rotation
                    .get("targetRotation")
                    .and_then(Value::as_i64)
                    .filter(|value| matches!(value, 0 | 90 | 180 | 270))
                    .ok_or_else(|| {
                        StepFailure::NoEffect("approved page rotation is invalid".to_string())
                    })?;
                Ok((page_index, target))
            })
            .collect::<Result<Vec<_>, StepFailure>>()?;
        if rotations.is_empty() {
            return Err(StepFailure::NoEffect(
                "approved PDF rotations are empty".to_string(),
            ));
        }
        let source_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &source,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let destination_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        if source_anchor.sha256().map_err(io_no_effect)? != expected_hash {
            return Err(StepFailure::NoEffect(
                "source PDF changed after preview".to_string(),
            ));
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        record.pending_step = Some(step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;

        let workspace = tempfile::tempdir().map_err(io_no_effect)?;
        let workspace_capability =
            crate::cap_fs::CapabilityRoot::open(workspace.path()).map_err(io_no_effect)?;
        let source_copy = workspace.path().join("source.pdf");
        let completed = workspace.path().join("completed.pdf");
        let source_copy_anchor = workspace_capability
            .authorize(&source_copy)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        source_copy_anchor
            .copy_no_clobber_from(&source_anchor)
            .map_err(io_no_effect)?;
        if let Err(error) = rotate_pages_incremental(&source_copy, &completed, &rotations) {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::NoEffect(error.to_string()));
        }
        let completed_anchor = workspace_capability
            .authorize(&completed)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let output_hash = completed_anchor.sha256().map_err(io_no_effect)?;
        if token.checkpoint().is_err() {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::Cancelled);
        }
        if source_anchor.sha256().map_err(io_no_effect)? != expected_hash {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::NoEffect(
                "approved PDF changed during apply".to_string(),
            ));
        }
        if let Err(error) = destination_anchor.copy_no_clobber_from(&completed_anchor) {
            if error.kind() != io::ErrorKind::AlreadyExists
                && destination_anchor.open_read().is_ok()
            {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    destination.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(io_no_effect(error));
        }
        if destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })? != output_hash
        {
            return Err(StepFailure::OutcomeUnknown(
                "published orientation PDF failed hash validation".to_string(),
            ));
        }
        let mut actual = Step {
            kind: StepKind::Copy,
            src: Some(source.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: step.note_key.clone(),
            meta: step.meta.clone(),
        };
        actual
            .meta
            .insert("undoSha256".to_string(), Value::String(output_hash));
        record.steps.push(actual);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }

    fn execute_cleanup_temp(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        if binding.operation_id != "rfi.stage_signed_first_pages" {
            return Err(StepFailure::NoEffect(
                "temporary PDF cleanup is not authorized for this operation".to_string(),
            ));
        }
        let temporary = required_path(step.src.as_deref(), "stale PDF temporary path")?;
        let temporary = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &temporary,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let destination = required_path(step.dst.as_deref(), "completed review PDF")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let review_root = step
            .meta
            .get("reviewRoot")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| StepFailure::NoEffect("review root is missing".to_string()))?;
        let review_root = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &review_root,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        if !temporary.starts_with(&review_root)
            || !destination.starts_with(&review_root)
            || temporary.parent() != destination.parent()
        {
            return Err(StepFailure::NoEffect(
                "temporary PDF cleanup escaped the approved review path".to_string(),
            ));
        }
        let destination_name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StepFailure::NoEffect("review PDF name is invalid".to_string()))?;
        let temporary_name = temporary
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StepFailure::NoEffect("temporary PDF name is invalid".to_string()))?;
        if !temporary_pdf_name_matches(temporary_name, destination_name) {
            return Err(StepFailure::NoEffect(
                "temporary PDF name is outside the cleanup contract".to_string(),
            ));
        }
        let temporary_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &temporary,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let destination_anchor = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        if !temporary_anchor
            .same_file_as(&destination_anchor)
            .map_err(io_no_effect)?
        {
            return Err(StepFailure::NoEffect(
                "temporary PDF is not the completed review PDF hard link".to_string(),
            ));
        }
        let expected_hash = step
            .meta
            .get("destinationSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("review PDF hash is missing".to_string()))?;
        if destination_anchor.sha256().map_err(io_no_effect)? != expected_hash {
            return Err(StepFailure::NoEffect(
                "completed review PDF changed after preview".to_string(),
            ));
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        record.pending_step = Some(step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;
        if let Err(error) = remove_anchored_file_with_access_retry(&temporary_anchor, 40) {
            if temporary_anchor.open_read().is_ok()
                && destination_anchor
                    .sha256()
                    .is_ok_and(|hash| hash == expected_hash)
            {
                record.pending_step = None;
                self.journals.write(record).map_err(step_unknown)?;
                return Err(io_no_effect(error));
            }
            return Err(StepFailure::OutcomeUnknown(format!(
                "{}: {error}",
                temporary.display()
            )));
        }
        if destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })? != expected_hash
        {
            return Err(StepFailure::OutcomeUnknown(
                "completed review PDF changed during temporary cleanup".to_string(),
            ));
        }
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }
}

enum FollowUpApplyFailure {
    NoEffect(Warning),
    Partial(Warning),
    OutcomeUnknown(Warning),
    Cancelled,
}

fn follow_up_warning(key: &str, subject: impl Into<String>) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.into(),
    }
}

fn follow_up_no_effect(key: &str) -> FollowUpApplyFailure {
    FollowUpApplyFailure::NoEffect(follow_up_warning(key, String::new()))
}

fn follow_up_partial(key: &str) -> FollowUpApplyFailure {
    FollowUpApplyFailure::Partial(follow_up_warning(key, String::new()))
}

fn follow_up_backend_no_effect(error: BackendError) -> FollowUpApplyFailure {
    follow_up_no_effect(&error.message)
}

fn follow_up_backend_partial(error: BackendError) -> FollowUpApplyFailure {
    follow_up_partial(&error.message)
}

fn follow_up_outcome(record: &ValidatedS5Record) -> FollowUpApplyFailure {
    let locator = record
        .retained_locators()
        .ok()
        .map(|locators| locators.first().to_string())
        .unwrap_or_else(|| format!("journal:{};root:0;leaf:record", record.journal_id()));
    FollowUpApplyFailure::OutcomeUnknown(follow_up_warning(
        "error.rfi.followUp.recoveryRequired",
        locator,
    ))
}

fn s5_transition(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
    transition: S5Transition,
) -> Result<(), FollowUpApplyFailure> {
    let expected = record.revision();
    record
        .apply_transition(transition)
        .map_err(|_| follow_up_outcome(record))?;
    journals
        .write_s5_cas(record, expected)
        .map_err(|_| follow_up_outcome(record))
}

fn s5_locator(record: &ValidatedS5Record, path: &Path) -> Result<String, FollowUpApplyFailure> {
    let roots = record
        .roots()
        .ok_or_else(|| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
    let index = roots
        .roots()
        .iter()
        .position(|root| path.starts_with(root.canonical()))
        .ok_or_else(|| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
    let leaf = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| follow_up_no_effect("error.rfi.followUp.workbookInvalid"))?;
    Ok(format!(
        "journal:{};root:{index};leaf:{leaf}",
        record.journal_id()
    ))
}

fn retained_evidence(
    role: RetainedArtifactRole,
    path: &Path,
    identity: crate::journal::FilesystemObjectIdentity,
    sha256: String,
    security: &FileSecuritySnapshot,
    default_stream_bytes: u64,
    locator: String,
) -> Result<RetainedArtifactEvidence, FollowUpApplyFailure> {
    let auxiliary_charge_bytes = security.auxiliary_charge_bytes();
    let logical_charge_bytes = default_stream_bytes
        .checked_add(auxiliary_charge_bytes)
        .ok_or_else(|| follow_up_partial("error.rfi.followUp.retainedStorageFull"))?;
    Ok(RetainedArtifactEvidence {
        role,
        path: path.to_string_lossy().into_owned(),
        identity,
        sha256,
        security_sha256: security.sha256(),
        default_stream_bytes,
        auxiliary_charge_bytes,
        logical_charge_bytes,
        locator,
    })
}

fn insert_retained_locators(result: &mut OperationResult, locators: &[String]) {
    result.meta.insert(
        "retainedLocators".to_string(),
        Value::Array(locators.iter().cloned().map(Value::String).collect()),
    );
}

fn insert_retained_evidence(result: &mut OperationResult, evidence: &[RetainedArtifactEvidence]) {
    let order = [
        RetainedArtifactRole::Backup,
        RetainedArtifactRole::PrivateOutput,
        RetainedArtifactRole::DisplacedOriginal,
        RetainedArtifactRole::DisplacedApplied,
    ];
    let projection = order
        .into_iter()
        .filter_map(|role| evidence.iter().find(|item| item.role == role))
        .map(|item| {
            serde_json::json!({
                "role": item.role.projection_key(),
                "locator": item.locator,
            })
        })
        .collect();
    result
        .meta
        .insert("retainedEvidence".to_string(), Value::Array(projection));
}

fn authorize_s5_recorded(
    record: &ValidatedS5Record,
    path: &Path,
) -> Result<AuthorizedPath, BackendError> {
    record
        .roots()
        .ok_or_else(|| BackendError::journal_invalid("S5 root authority is unavailable"))?
        .roots()
        .iter()
        .find_map(|root| root.authorize(path).ok())
        .ok_or_else(BackendError::path_escape)
}

fn authorize_s5_recorded_directory(
    record: &ValidatedS5Record,
    path: &Path,
) -> Result<AuthorizedPath, BackendError> {
    record
        .roots()
        .ok_or_else(|| BackendError::journal_invalid("S5 root authority is unavailable"))?
        .roots()
        .iter()
        .find_map(|root| root.authorize_directory(path).ok())
        .ok_or_else(BackendError::path_escape)
}

fn s5_recorded_retained_usage(
    record: &ValidatedS5Record,
    canonical_parent: &Path,
) -> Result<(usize, u64), BackendError> {
    let canonical = authorize_s5_recorded_directory(record, canonical_parent)?;
    let canonical_usage = canonical
        .s5_retained_storage_usage(MAX_S5_RETAINED_FILES)
        .map_err(|_| BackendError::validation("error.rfi.followUp.retainedStorageFull"))?;
    let private_parent = canonical_parent.join(".docflow-backups");
    let private_usage = match authorize_s5_recorded_directory(record, &private_parent)?
        .s5_retained_storage_usage(MAX_S5_RETAINED_FILES)
    {
        Ok(usage) => usage,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            crate::cap_fs::RetainedStorageUsage {
                file_count: 0,
                logical_charge_bytes: 0,
            }
        }
        Err(_) => {
            return Err(BackendError::validation(
                "error.rfi.followUp.retainedStorageFull",
            ));
        }
    };
    let count = canonical_usage
        .file_count
        .checked_add(private_usage.file_count)
        .ok_or_else(|| BackendError::validation("error.rfi.followUp.retainedStorageFull"))?;
    let bytes = canonical_usage
        .logical_charge_bytes
        .checked_add(private_usage.logical_charge_bytes)
        .ok_or_else(|| BackendError::validation("error.rfi.followUp.retainedStorageFull"))?;
    Ok((count, bytes))
}

fn transition_s5_backend(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
    transition: S5Transition,
) -> Result<(), BackendError> {
    let expected = record.revision();
    record.apply_transition(transition)?;
    journals.write_s5_cas(record, expected)
}

fn s5_recovery_result(record: &ValidatedS5Record, notice: RecoveryNotice) -> OperationResult {
    let mut result = OperationResult {
        applied: 0,
        failed: 1,
        journal_id: record.journal_id().to_string(),
        errors: vec![Warning {
            key: notice.warning_key,
            subject: notice.locator,
        }],
        meta: JsonObject::new(),
        cloud_synced: None,
    };
    insert_effect_state(&mut result, "outcome_unknown");
    insert_retained_evidence(&mut result, record.retained_evidence());
    result
}

fn sha256_open_file(file: &mut File) -> io::Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(format!("{:x}", digest.finalize()))
}

enum StepFailure {
    NoEffect(String),
    WarningNoEffect(Warning),
    OutcomeUnknown(String),
    Cancelled,
}

fn operation_result_for_generated_failure(
    record: &JournalRecord,
    key: &str,
    outcome_unknown: bool,
) -> OperationResult {
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
    result
}

fn operation_result_for_generated_cancelled(record: &JournalRecord) -> OperationResult {
    let mut result = OperationResult {
        applied: 0,
        failed: 0,
        journal_id: record.journal_id.clone(),
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
    insert_effect_state(&mut result, "cancelled_clean");
    result
}

struct CancellationRegistration<'a> {
    registry: &'a CancellationRegistry,
    request_id: &'a str,
}

impl Drop for CancellationRegistration<'_> {
    fn drop(&mut self) {
        self.registry.finish(self.request_id);
    }
}

fn mark_cancelled(record: &mut JournalRecord, result: &mut OperationResult, pending: usize) {
    let partial = !record.steps.is_empty();
    record.state = Some(if partial {
        JournalState::CancelledPartial
    } else {
        JournalState::CancelledClean
    });
    result.errors.push(Warning {
        key: "warn.operation.cancelled".to_string(),
        subject: String::new(),
    });
    result
        .meta
        .insert("cancelled".to_string(), Value::Bool(true));
    result.meta.insert(
        "pending".to_string(),
        Value::from(u64::try_from(pending).unwrap_or(u64::MAX)),
    );
    insert_effect_state(
        result,
        if partial {
            "cancelled_partial"
        } else {
            "cancelled_clean"
        },
    );
}

fn insert_effect_state(result: &mut OperationResult, state: &str) {
    result
        .meta
        .insert("effectState".to_string(), Value::String(state.to_string()));
}

fn required_path(raw: Option<&str>, label: &str) -> Result<PathBuf, StepFailure> {
    raw.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| StepFailure::NoEffect(format!("{label} is missing")))
}

fn step_no_effect(error: BackendError) -> StepFailure {
    StepFailure::NoEffect(error.message)
}

fn step_unknown(error: BackendError) -> StepFailure {
    StepFailure::OutcomeUnknown(error.message)
}

fn io_no_effect(error: io::Error) -> StepFailure {
    StepFailure::NoEffect(error.to_string())
}

fn replacement_failure(destination: &AnchoredPath, error: io::Error) -> StepFailure {
    StepFailure::OutcomeUnknown(format!("{}: {error}", destination.absolute().display()))
}

fn create_temporary(parent: &Path, destination: &Path) -> io::Result<(PathBuf, File)> {
    if !parent.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "destination parent does not exist",
        ));
    }
    for _ in 0..16 {
        let name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("file");
        let temporary = parent.join(format!(".{name}.docflow-{}.tmp", Uuid::new_v4()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary file",
    ))
}

fn rollback_from_backup_capability(
    backup: &AnchoredPath,
    workbook: &AnchoredPath,
    expected_hash: &str,
) -> io::Result<()> {
    let temporary = workbook.temporary_sibling("restore")?;
    temporary.copy_no_clobber_from(backup)?;
    let result = (|| {
        if temporary.sha256()? != expected_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "workbook rollback copy did not match the approved backup",
            ));
        }
        atomic_replace_capability_with_retry(&temporary, workbook)?;
        if workbook.sha256()? != expected_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "workbook rollback verification failed",
            ));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = temporary.remove_file();
    }
    result
}

fn atomic_replace_capability_with_retry(
    source: &AnchoredPath,
    destination: &AnchoredPath,
) -> io::Result<()> {
    for attempt in 0..20 {
        match destination.replace_with(source) {
            Ok(()) => return Ok(()),
            Err(error) if is_workbook_access_error(&error) && attempt < 19 => {
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("replacement retry loop always returns")
}

pub(crate) fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    atomic_replace_platform(source, destination)?;
    if let Some(parent) = destination.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn is_workbook_access_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        || matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

fn remove_anchored_file_with_access_retry(path: &AnchoredPath, attempts: usize) -> io::Result<()> {
    for attempt in 0..attempts.max(1) {
        match path.remove_file() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) if is_workbook_access_error(&error) && attempt + 1 < attempts.max(1) => {
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(
        "capability-relative file could not be removed",
    ))
}

#[cfg(unix)]
fn atomic_replace_platform(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace_platform(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn publish_no_clobber(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::hard_link(temporary, destination)?;
    fs::remove_file(temporary)
}

#[cfg(unix)]
fn open_source_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
fn open_source_no_follow(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(target_os = "macos")]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source contains NUL"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination contains NUL"))?;
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source contains NUL"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination contains NUL"))?;
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    if source.is_file() {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "atomic no-replace directory rename is unavailable",
        ))
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_outlook_msg_write_is_no_clobber_and_undoable() {
        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("archive");
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let destination = root.join("RFI response.msg");
        let authority =
            FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
        let grant = authority
            .issue_grant(
                &GrantScope::new(
                    "owner",
                    "session",
                    "request",
                    "rfi.outlook_archive_selected",
                ),
                &root,
                GrantAccess::Write,
            )
            .unwrap();
        let cached = authority
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new(
                    "owner",
                    "session",
                    "rfi.outlook_archive_selected",
                    vec![grant.grant_id],
                ),
                Plan {
                    plan_id: String::new(),
                    op_id: "rfi.outlook_archive_selected".to_string(),
                    steps: vec![Step {
                        kind: StepKind::Write,
                        src: Some("RFI response".to_string()),
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: Some("step.rfi.outlookArchive".to_string()),
                        meta: serde_json::json!({
                            "generatedBy": "outlook_msg",
                            "format": "msg"
                        })
                        .as_object()
                        .unwrap()
                        .clone(),
                    }],
                    warnings: Vec::new(),
                    meta: JsonObject::new(),
                },
            )
            .unwrap();
        let caller = CallerIdentity::new("owner", "session");
        let bytes = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1fixture".to_vec();

        let result = authority
            .apply_generated_binary_write(
                &cached.plan_id,
                &caller,
                "rfi.outlook_archive_selected",
                |_| {},
                || Ok(bytes.clone()),
            )
            .unwrap();

        assert_eq!(result.applied, 1);
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        let journal = authority.journals().read(&result.journal_id).unwrap();
        assert_eq!(journal.steps[0].kind, StepKind::Write);
        assert!(!journal.steps[0].meta.contains_key("generatedBy"));
        let undone = authority.undo(&result.journal_id, &caller, None).unwrap();
        assert_eq!(undone.applied, 1);
        assert!(!destination.exists());

        fs::write(&destination, b"existing").unwrap();
        let collision_grant = authority
            .issue_grant(
                &GrantScope::new(
                    "owner",
                    "session",
                    "collision-request",
                    "rfi.outlook_archive_selected",
                ),
                &root,
                GrantAccess::Write,
            )
            .unwrap();
        let collision = authority
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new(
                    "owner",
                    "session",
                    "rfi.outlook_archive_selected",
                    vec![collision_grant.grant_id],
                ),
                Plan {
                    plan_id: String::new(),
                    op_id: "rfi.outlook_archive_selected".to_string(),
                    steps: vec![Step {
                        kind: StepKind::Write,
                        src: Some("RFI response".to_string()),
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: Some("step.rfi.outlookArchive".to_string()),
                        meta: serde_json::json!({"generatedBy": "outlook_msg"})
                            .as_object()
                            .unwrap()
                            .clone(),
                    }],
                    warnings: Vec::new(),
                    meta: JsonObject::new(),
                },
            )
            .unwrap();
        let collision_result = authority
            .apply_generated_binary_write(
                &collision.plan_id,
                &caller,
                "rfi.outlook_archive_selected",
                |_| {},
                || Ok(bytes.clone()),
            )
            .unwrap();
        assert_eq!(collision_result.applied, 0);
        assert_eq!(collision_result.failed, 1);
        assert_eq!(fs::read(&destination).unwrap(), b"existing");
    }

    #[test]
    fn workbook_output_sync_uses_a_writable_handle_for_windows() {
        let source = include_str!("fs_ops.rs");
        let workbook_update = source
            .split("fn execute_workbook_update(")
            .nth(1)
            .expect("workbook update implementation must remain inspectable")
            .split("fn execute_pdf_first_page_replace(")
            .next()
            .expect("workbook update implementation must remain bounded");

        assert!(workbook_update.contains("OpenOptions::new()"));
        assert!(workbook_update.contains(".read(true)"));
        assert!(workbook_update.contains(".write(true)"));
        assert!(!workbook_update.contains("File::open(&output)"));
    }

    #[test]
    fn pdf_publish_retries_transient_access_errors_without_overwriting() {
        let fixture = tempfile::TempDir::new().unwrap();
        let temporary = fixture.path().join(".review.pdf.docflow-test.tmp");
        let destination = fixture.path().join("review.pdf");
        fs::write(&temporary, b"completed PDF bytes").unwrap();
        let mut calls = 0;

        publish_pdf_no_clobber_with(
            &temporary,
            &destination,
            4,
            |source, target| {
                calls += 1;
                if calls < 3 {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "simulated OneDrive lock",
                    ))
                } else {
                    fs::hard_link(source, target)
                }
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(calls, 3);
        assert_eq!(fs::read(destination).unwrap(), b"completed PDF bytes");
        assert!(!temporary.exists());
    }

    #[test]
    fn pdf_publish_falls_back_to_exclusive_copy_after_persistent_access_errors() {
        let fixture = tempfile::TempDir::new().unwrap();
        let temporary = fixture.path().join(".review.pdf.docflow-test.tmp");
        let destination = fixture.path().join("review.pdf");
        fs::write(&temporary, b"completed PDF bytes").unwrap();
        let mut calls = 0;

        publish_pdf_no_clobber_with(
            &temporary,
            &destination,
            3,
            |_, _| {
                calls += 1;
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "simulated persistent OneDrive lock",
                ))
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(calls, 3);
        assert_eq!(fs::read(destination).unwrap(), b"completed PDF bytes");
        assert!(!temporary.exists());
    }

    #[test]
    fn pdf_publish_never_overwrites_an_existing_destination() {
        let fixture = tempfile::TempDir::new().unwrap();
        let temporary = fixture.path().join(".review.pdf.docflow-test.tmp");
        let destination = fixture.path().join("review.pdf");
        fs::write(&temporary, b"new PDF bytes").unwrap();
        fs::write(&destination, b"existing user bytes").unwrap();

        let error = publish_pdf_no_clobber_with(
            &temporary,
            &destination,
            3,
            |source, target| fs::hard_link(source, target),
            |_| {},
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(destination).unwrap(), b"existing user bytes");
        assert_eq!(fs::read(temporary).unwrap(), b"new PDF bytes");
    }

    #[test]
    fn a_fault_after_a_move_effect_keeps_the_pending_step_for_recovery() {
        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let source = root.join("source.pdf");
        let destination = root.join("destination.pdf");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"PDF").unwrap();

        let authority =
            FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
        let scope = GrantScope::new("owner", "session", "request", "op.test");
        let grant = authority
            .issue_grant(&scope, &root, GrantAccess::ReadWrite)
            .unwrap();
        let cached = authority
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new("owner", "session", "op.test", vec![grant.grant_id]),
                Plan {
                    plan_id: String::new(),
                    op_id: "op.test".to_string(),
                    steps: vec![Step {
                        kind: StepKind::Move,
                        src: Some(source.to_string_lossy().into_owned()),
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: None,
                        meta: JsonObject::new(),
                    }],
                    warnings: Vec::new(),
                    meta: JsonObject::new(),
                },
            )
            .unwrap();
        authority.inject_post_effect_failure();

        let result = authority
            .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
            .unwrap();

        assert_eq!(result.meta["effectState"], "outcome_unknown");
        assert!(!source.exists());
        assert_eq!(fs::read(destination).unwrap(), b"PDF");
        let journal = authority.journals().read(&result.journal_id).unwrap();
        assert_eq!(journal.state, Some(JournalState::RecoveryRequired));
        assert_eq!(journal.steps.len(), 0);
        assert_eq!(journal.pending_step.unwrap().kind, StepKind::Move);
    }

    #[cfg(not(windows))]
    #[test]
    fn post_rename_sync_failures_keep_copy_write_and_move_pending_for_recovery() {
        for kind in [StepKind::Copy, StepKind::Write, StepKind::Move] {
            let fixture = tempfile::TempDir::new().unwrap();
            let root = fixture.path().join("documents");
            let source = root.join("source.txt");
            let destination = root.join("destination.txt");
            fs::create_dir_all(&root).unwrap();
            fs::write(&source, b"APPROVED BYTES").unwrap();
            let authority =
                FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
            let grant = authority
                .issue_grant(
                    &GrantScope::new("owner", "session", "request", "op.test"),
                    &root,
                    GrantAccess::ReadWrite,
                )
                .unwrap();
            let mut meta = JsonObject::new();
            if kind == StepKind::Write {
                meta.insert(
                    "content".to_string(),
                    Value::String("APPROVED BYTES".to_string()),
                );
            }
            let cached = authority
                .cache_plan(
                    BackendOrigin::Rust,
                    PlanBinding::new("owner", "session", "op.test", vec![grant.grant_id]),
                    Plan {
                        plan_id: String::new(),
                        op_id: "op.test".to_string(),
                        steps: vec![Step {
                            kind,
                            src: (kind != StepKind::Write)
                                .then(|| source.to_string_lossy().into_owned()),
                            dst: Some(destination.to_string_lossy().into_owned()),
                            note_key: None,
                            meta,
                        }],
                        warnings: Vec::new(),
                        meta: JsonObject::new(),
                    },
                )
                .unwrap();
            crate::cap_fs::inject_directory_sync_failure_after(0);

            let result = authority
                .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
                .unwrap();

            assert_eq!(result.meta["effectState"], "outcome_unknown", "{kind:?}");
            assert_eq!(
                fs::read(&destination).unwrap(),
                b"APPROVED BYTES",
                "{kind:?}"
            );
            let journal = authority.journals().read(&result.journal_id).unwrap();
            assert_eq!(
                journal.state,
                Some(JournalState::RecoveryRequired),
                "{kind:?}"
            );
            assert_eq!(
                journal.pending_step.as_ref().map(|step| step.kind),
                Some(kind)
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn workbook_backup_post_rename_sync_failure_remains_outcome_unknown() {
        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let workbook = root.join("RFI Log.xlsm");
        let backup = root.join("RFI Log.backup.xlsm");
        fs::create_dir_all(&root).unwrap();
        fs::write(&workbook, b"APPROVED WORKBOOK PACKAGE").unwrap();
        let authority =
            FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
        let grant = authority
            .issue_grant(
                &GrantScope::new("owner", "session", "request", "rfi.log_add"),
                &root,
                GrantAccess::ReadWrite,
            )
            .unwrap();
        let mut meta = JsonObject::new();
        meta.insert(
            "sourceSignature".to_string(),
            serde_json::json!({
                "size": fs::metadata(&workbook).unwrap().len(),
                "sha256": file_sha256(&workbook).unwrap()
            }),
        );
        let cached = authority
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new("owner", "session", "rfi.log_add", vec![grant.grant_id]),
                Plan {
                    plan_id: String::new(),
                    op_id: "rfi.log_add".to_string(),
                    steps: vec![Step {
                        kind: StepKind::WorkbookUpdate,
                        src: Some(workbook.to_string_lossy().into_owned()),
                        dst: Some(backup.to_string_lossy().into_owned()),
                        note_key: None,
                        meta,
                    }],
                    warnings: Vec::new(),
                    meta: JsonObject::new(),
                },
            )
            .unwrap();
        crate::cap_fs::inject_directory_sync_failure_after(0);

        let result = authority
            .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
            .unwrap();

        assert_eq!(result.meta["effectState"], "outcome_unknown");
        assert_eq!(fs::read(&backup).unwrap(), b"APPROVED WORKBOOK PACKAGE");
        let journal = authority.journals().read(&result.journal_id).unwrap();
        assert_eq!(journal.state, Some(JournalState::RecoveryRequired));
        assert_eq!(
            journal.pending_step.as_ref().map(|step| step.kind),
            Some(StepKind::Copy)
        );
    }

    #[test]
    fn byte_identical_replace_with_post_rename_sync_failure_is_still_outcome_unknown() {
        let fixture = tempfile::TempDir::new().unwrap();
        let source = fixture.path().join("replacement.xlsm");
        let destination = fixture.path().join("workbook.xlsm");
        fs::write(&source, b"IDENTICAL PACKAGE").unwrap();
        fs::write(&destination, b"IDENTICAL PACKAGE").unwrap();
        let capability = crate::cap_fs::CapabilityRoot::open(fixture.path()).unwrap();
        let source = capability.authorize(&source).unwrap().anchor().unwrap();
        let destination = capability
            .authorize(&destination)
            .unwrap()
            .anchor()
            .unwrap();
        crate::cap_fs::inject_directory_sync_failure_after(0);

        let error = destination
            .replace_with(&source)
            .expect_err("rename succeeded but directory sync was injected to fail");
        let classified = replacement_failure(&destination, error);

        assert!(matches!(classified, StepFailure::OutcomeUnknown(_)));
        assert_eq!(destination.sha256().unwrap(), {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(b"IDENTICAL PACKAGE"))
        });
        assert!(source.open_read().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn nested_mkdirs_stay_on_the_pinned_parent_after_visible_parent_swap() {
        use std::os::unix::fs::symlink;

        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let visible_parent = root.join("approved-parent");
        let pinned_parent = root.join("pinned-parent");
        let outside = fixture.path().join("outside");
        fs::create_dir_all(&visible_parent).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let destination = visible_parent.join("nested/deeper");
        let authority =
            FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
        let grant = authority
            .issue_grant(
                &GrantScope::new("owner", "session", "request", "op.test"),
                &root,
                GrantAccess::ReadWrite,
            )
            .unwrap();
        let cached = authority
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new("owner", "session", "op.test", vec![grant.grant_id]),
                Plan {
                    plan_id: String::new(),
                    op_id: "op.test".to_string(),
                    steps: vec![Step {
                        kind: StepKind::Mkdir,
                        src: None,
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: None,
                        meta: JsonObject::new(),
                    }],
                    warnings: Vec::new(),
                    meta: JsonObject::new(),
                },
            )
            .unwrap();
        let outside_target = outside.clone();
        let swapped_parent = pinned_parent.clone();
        authority.inject_before_capability_effect(move || {
            fs::rename(&visible_parent, &swapped_parent).unwrap();
            symlink(&outside_target, &visible_parent).unwrap();
        });

        let result = authority
            .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
            .unwrap();

        assert_eq!((result.applied, result.failed), (1, 0));
        assert!(pinned_parent.join("nested/deeper").is_dir());
        assert!(!outside.join("nested").exists());
        let journal = authority.journals().read(&result.journal_id).unwrap();
        assert_eq!(
            journal
                .steps
                .iter()
                .map(|step| step.kind)
                .collect::<Vec<_>>(),
            vec![StepKind::Mkdir, StepKind::Mkdir]
        );
    }

    #[cfg(unix)]
    #[test]
    fn pinned_parent_capability_defeats_post_acquisition_path_swap() {
        use std::os::unix::fs::symlink;

        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let visible_parent = root.join("approved-parent");
        let pinned_parent = root.join("pinned-parent");
        let outside = fixture.path().join("outside");
        fs::create_dir_all(&visible_parent).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let destination = visible_parent.join("created.txt");
        let authority =
            FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
        let grant = authority
            .issue_grant(
                &GrantScope::new("owner", "session", "request", "op.test"),
                &root,
                GrantAccess::ReadWrite,
            )
            .unwrap();
        let mut meta = JsonObject::new();
        meta.insert("content".to_string(), Value::String("PINNED".to_string()));
        let cached = authority
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new("owner", "session", "op.test", vec![grant.grant_id]),
                Plan {
                    plan_id: String::new(),
                    op_id: "op.test".to_string(),
                    steps: vec![Step {
                        kind: StepKind::Write,
                        src: None,
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: None,
                        meta,
                    }],
                    warnings: Vec::new(),
                    meta: JsonObject::new(),
                },
            )
            .unwrap();
        let swapped_parent = pinned_parent.clone();
        let outside_target = outside.clone();
        authority.inject_before_capability_effect(move || {
            fs::rename(&visible_parent, &swapped_parent).unwrap();
            symlink(&outside_target, &visible_parent).unwrap();
        });

        let result = authority
            .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
            .unwrap();

        assert!(!outside.join("created.txt").exists());
        if result.applied == 1 {
            assert_eq!(
                fs::read(pinned_parent.join("created.txt")).unwrap(),
                b"PINNED"
            );
        } else {
            assert_eq!(result.meta["effectState"], "no_effect_retryable");
        }
    }

    #[test]
    fn file_effect_source_metadata_is_read_only_through_the_pinned_capability() {
        let source = include_str!("fs_ops.rs");
        let function = source
            .split("fn execute_file_effect(")
            .nth(1)
            .and_then(|tail| tail.split("fn execute_workbook_update(").next())
            .expect("file effect implementation");

        assert!(!function.contains("file_sha256(source)"));
        assert!(function.contains("source_anchor"));
        assert!(function.contains(".sha256()"));
    }

    #[test]
    fn verified_backup_rollback_atomically_restores_the_original_bytes() {
        let fixture = tempfile::TempDir::new().unwrap();
        let workbook = fixture.path().join("RFI Log.xlsm");
        let backup = fixture.path().join("backup.xlsm");
        fs::write(&workbook, b"changed package").unwrap();
        fs::write(&backup, b"original package").unwrap();
        let expected = file_sha256(&backup).unwrap();

        let capability = crate::cap_fs::CapabilityRoot::open(fixture.path()).unwrap();
        let backup = capability.authorize(&backup).unwrap().anchor().unwrap();
        let workbook = capability.authorize(&workbook).unwrap().anchor().unwrap();
        rollback_from_backup_capability(&backup, &workbook, &expected).unwrap();

        assert_eq!(workbook.sha256().unwrap(), expected);
        assert_eq!(backup.sha256().unwrap(), expected);
        assert!(fixture.path().read_dir().unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("restore-")
        }));
    }
}
