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
        S5UndoMode, ValidatedS5Record,
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
    let mut buffer = vec![0_u8; 1024 * 1024];
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
const CONCRETE_DOCUMENT_OPERATION: &str = "rfi.generate_concrete_documents";
const CONCRETE_DOCUMENT_NOTE: &str = "step.rfi.generatedConcreteDocument";

fn decode_concrete_document_base64(value: &str) -> Result<Vec<u8>, GeneratedWriteFailure> {
    let max_bytes = usize::try_from(crate::ooxml::MAX_PACKAGE_BYTES).unwrap_or(usize::MAX);
    let max_encoded = max_bytes.div_ceil(3) * 4;
    if value.len() > max_encoded {
        return Err(GeneratedWriteFailure::NoEffect(
            "error.rfi.documents.invalidOutput",
        ));
    }
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, value)
        .map_err(|_| GeneratedWriteFailure::NoEffect("error.rfi.documents.invalidOutput"))?;
    if bytes.len() > max_bytes {
        return Err(GeneratedWriteFailure::NoEffect(
            "error.rfi.documents.invalidOutput",
        ));
    }
    Ok(bytes)
}

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
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new(hook));
    }

    #[cfg(test)]
    fn run_before_capability_effect(&self) {
        let hook = self
            .before_capability_effect
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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

    pub fn cancel(&self, request_id: &str) -> bool {
        self.cancellations.cancel(request_id)
    }

    pub(crate) fn register_cancellation<'a>(
        &'a self,
        request_id: &'a str,
    ) -> (
        crate::progress::CancellationToken,
        CancellationRegistration<'a>,
    ) {
        let token = self.cancellations.register(request_id);
        let registration = CancellationRegistration {
            registry: &self.cancellations,
            request_id,
        };
        (token, registration)
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
}

mod apply;
mod effects;
mod effects_pdf;
mod follow_up;
mod steps;
mod support;
pub(crate) use support::*;

#[cfg(test)]
mod tests;
