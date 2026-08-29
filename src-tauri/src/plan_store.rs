use std::{
    collections::HashMap,
    fmt::Write as _,
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    contracts::Plan,
    errors::BackendError,
    journal::S5PermitBundle,
    paths::CallerIdentity,
    paths::ValidatedGrantRootRoster,
    progress::{GlobalMutationLock, GlobalMutationPermit},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendOrigin {
    Rust,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanState {
    Available,
    Claimed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanBinding {
    pub owner_id: String,
    pub session_id: String,
    pub operation_id: String,
    pub grant_ids: Vec<String>,
}

impl PlanBinding {
    pub fn new(
        owner_id: impl Into<String>,
        session_id: impl Into<String>,
        operation_id: impl Into<String>,
        grant_ids: Vec<String>,
    ) -> Self {
        Self {
            owner_id: owner_id.into(),
            session_id: session_id.into(),
            operation_id: operation_id.into(),
            grant_ids,
        }
    }
}

#[derive(Clone, Debug)]
struct StoredPlan {
    plan: Plan,
    backend_plan_id: String,
    origin: BackendOrigin,
    binding: Option<PlanBinding>,
    digest: String,
    created_at: SystemTime,
    state: PlanState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanRecordSnapshot {
    pub origin: BackendOrigin,
    pub digest: String,
    pub created_at: SystemTime,
    pub state: PlanState,
    pub binding: Option<PlanBinding>,
}

#[derive(Clone, Debug)]
pub struct ClaimedPlan {
    pub plan: Plan,
    pub backend_plan_id: String,
    pub origin: BackendOrigin,
    pub binding: Option<PlanBinding>,
    pub digest: String,
    pub created_at: SystemTime,
}

#[derive(Debug)]
pub struct ApplyClaim {
    pub claimed: ClaimedPlan,
    _mutation: GlobalMutationPermit,
}

pub(crate) struct ClaimedS5Apply {
    claimed: ClaimedPlan,
    caller: CallerIdentity,
    _permits: S5PermitBundle,
    roots: ValidatedGrantRootRoster,
}

impl ClaimedS5Apply {
    pub(crate) fn plan(&self) -> &Plan {
        &self.claimed.plan
    }

    pub(crate) fn binding(&self) -> &PlanBinding {
        self.claimed
            .binding
            .as_ref()
            .expect("S5 claims are always bound")
    }

    pub(crate) fn digest(&self) -> &str {
        &self.claimed.digest
    }

    pub(crate) fn created_at(&self) -> SystemTime {
        self.claimed.created_at
    }

    pub(crate) fn caller(&self) -> &CallerIdentity {
        &self.caller
    }

    pub(crate) fn roots(&self) -> &ValidatedGrantRootRoster {
        &self.roots
    }
}

#[derive(Debug)]
pub struct ReservedPlanId {
    value: String,
}

impl ReservedPlanId {
    pub fn assign_to_empty(&self, plan: &mut Plan) -> Result<(), BackendError> {
        if !plan.plan_id.is_empty() {
            return Err(BackendError::validation(
                "reserved plan ID requires an empty plan ID",
            ));
        }
        plan.plan_id.clone_from(&self.value);
        Ok(())
    }
}

pub struct PlanStore {
    ttl: Duration,
    plans: Mutex<HashMap<String, StoredPlan>>,
}

impl PlanStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            plans: Mutex::new(HashMap::new()),
        }
    }

    pub fn reserve_plan_id(&self) -> ReservedPlanId {
        ReservedPlanId {
            value: Uuid::new_v4().to_string(),
        }
    }

    pub fn insert_bound_reserved(
        &self,
        origin: BackendOrigin,
        binding: PlanBinding,
        reservation: ReservedPlanId,
        plan: Plan,
    ) -> Result<Plan, BackendError> {
        if plan.op_id != binding.operation_id {
            return Err(BackendError::validation(
                "plan operation does not match its binding",
            ));
        }
        if plan.plan_id != reservation.value
            || Uuid::parse_str(&plan.plan_id)
                .ok()
                .is_none_or(|parsed| parsed.to_string() != plan.plan_id)
        {
            return Err(BackendError::validation(
                "reserved plan ID does not match the plan",
            ));
        }
        let digest = plan_digest(&plan)?;
        let now = SystemTime::now();
        let mut plans = lock_plans(&self.plans);
        if plans.contains_key(&plan.plan_id) {
            return Err(BackendError::validation("reserved plan ID is occupied"));
        }
        let plan_id = plan.plan_id.clone();
        plans.insert(
            plan_id,
            StoredPlan {
                plan: plan.clone(),
                backend_plan_id: String::new(),
                origin,
                binding: Some(binding),
                digest,
                created_at: now,
                state: PlanState::Available,
            },
        );
        Ok(plan)
    }

    #[cfg(test)]
    fn insert(&self, origin: BackendOrigin, mut plan: Plan) -> Result<Plan, BackendError> {
        self.insert_inner(origin, None, &mut plan)
    }

    pub fn insert_bound(
        &self,
        origin: BackendOrigin,
        binding: PlanBinding,
        mut plan: Plan,
    ) -> Result<Plan, BackendError> {
        if plan.op_id != binding.operation_id {
            return Err(BackendError::validation(
                "plan operation does not match its binding",
            ));
        }
        self.insert_inner(origin, Some(binding), &mut plan)
    }

    fn insert_inner(
        &self,
        origin: BackendOrigin,
        binding: Option<PlanBinding>,
        plan: &mut Plan,
    ) -> Result<Plan, BackendError> {
        let now = SystemTime::now();
        let mut plans = lock_plans(&self.plans);
        plans.retain(|_, stored| !expired(stored.created_at, now, self.ttl));

        let plan_id = loop {
            let candidate = Uuid::new_v4().to_string();
            if !plans.contains_key(&candidate) {
                break candidate;
            }
        };
        let backend_plan_id = plan.plan_id.clone();
        plan.plan_id = plan_id.clone();
        let digest = plan_digest(plan)?;
        plans.insert(
            plan_id,
            StoredPlan {
                plan: plan.clone(),
                backend_plan_id,
                origin,
                binding,
                digest,
                created_at: now,
                state: PlanState::Available,
            },
        );
        Ok(plan.clone())
    }

    pub fn snapshot(&self, plan_id: &str) -> Option<PlanRecordSnapshot> {
        let now = SystemTime::now();
        let mut plans = lock_plans(&self.plans);
        if plans
            .get(plan_id)
            .is_some_and(|stored| expired(stored.created_at, now, self.ttl))
        {
            plans.remove(plan_id);
            return None;
        }
        plans.get(plan_id).map(|stored| PlanRecordSnapshot {
            origin: stored.origin,
            digest: stored.digest.clone(),
            created_at: stored.created_at,
            state: stored.state,
            binding: stored.binding.clone(),
        })
    }

    pub fn revoke(&self, caller: &CallerIdentity) {
        lock_plans(&self.plans).retain(|_, stored| {
            stored.binding.as_ref().is_none_or(|binding| {
                binding.owner_id != caller.owner_id || binding.session_id != caller.session_id
            })
        });
    }

    #[cfg(test)]
    fn claim_for_apply(
        &self,
        plan_id: &str,
        mutation_lock: &GlobalMutationLock,
    ) -> Result<ApplyClaim, BackendError> {
        let mutation = mutation_lock.try_acquire()?;
        let claimed = self.claim(plan_id)?;
        Ok(ApplyClaim {
            claimed,
            _mutation: mutation,
        })
    }

    pub fn claim_for_apply_bound(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        mutation_lock: &GlobalMutationLock,
    ) -> Result<ApplyClaim, BackendError> {
        let mutation = mutation_lock.try_acquire()?;
        let claimed = self.claim_bound(plan_id, caller)?;
        Ok(ApplyClaim {
            claimed,
            _mutation: mutation,
        })
    }

    pub fn claim_for_apply_bound_under_permit(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        mutation: GlobalMutationPermit,
    ) -> Result<ApplyClaim, BackendError> {
        let claimed = self.claim_bound(plan_id, caller)?;
        Ok(ApplyClaim {
            claimed,
            _mutation: mutation,
        })
    }

    pub fn inspect_available_bound_operation(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
    ) -> Result<String, BackendError> {
        let now = SystemTime::now();
        let plans = lock_plans(&self.plans);
        let stored = plans
            .get(plan_id)
            .ok_or_else(BackendError::plan_unavailable)?;
        if expired(stored.created_at, now, self.ttl) || stored.state != PlanState::Available {
            return Err(BackendError::plan_unavailable());
        }
        let binding = stored
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        if binding.owner_id != caller.owner_id {
            return Err(BackendError::plan_owner_mismatch());
        }
        if binding.session_id != caller.session_id {
            return Err(BackendError::plan_session_mismatch());
        }
        if plan_digest(&stored.plan)? != stored.digest {
            return Err(BackendError::plan_unavailable());
        }
        Ok(stored.plan.op_id.clone())
    }

    pub(crate) fn claim_s5_for_apply_bound_under_permit(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        permits: S5PermitBundle,
        roots: ValidatedGrantRootRoster,
    ) -> Result<ClaimedS5Apply, BackendError> {
        let now = SystemTime::now();
        let claimed = {
            let mut plans = lock_plans(&self.plans);
            let stored = plans
                .get_mut(plan_id)
                .ok_or_else(BackendError::plan_unavailable)?;
            if expired(stored.created_at, now, self.ttl)
                || stored.state != PlanState::Available
                || stored.plan.op_id != "rfi.follow_up_sync"
                || plan_digest(&stored.plan)? != stored.digest
            {
                return Err(BackendError::plan_unavailable());
            }
            let binding = stored
                .binding
                .as_ref()
                .ok_or_else(BackendError::plan_unavailable)?;
            if binding.owner_id != caller.owner_id {
                return Err(BackendError::plan_owner_mismatch());
            }
            if binding.session_id != caller.session_id {
                return Err(BackendError::plan_session_mismatch());
            }
            if binding.operation_id != stored.plan.op_id
                || !roots.matches_grants(&binding.grant_ids)
            {
                return Err(BackendError::plan_unavailable());
            }
            stored.state = PlanState::Claimed;
            ClaimedPlan {
                plan: stored.plan.clone(),
                backend_plan_id: stored.backend_plan_id.clone(),
                origin: stored.origin,
                binding: stored.binding.clone(),
                digest: stored.digest.clone(),
                created_at: stored.created_at,
            }
        };
        Ok(ClaimedS5Apply {
            claimed,
            caller: caller.clone(),
            _permits: permits,
            roots,
        })
    }

    pub fn resolve_bound(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
    ) -> Result<ClaimedPlan, BackendError> {
        let now = SystemTime::now();
        let mut plans = lock_plans(&self.plans);
        if plans
            .get(plan_id)
            .is_some_and(|stored| expired(stored.created_at, now, self.ttl))
        {
            plans.remove(plan_id);
            return Err(BackendError::plan_unavailable());
        }
        let stored = plans
            .get(plan_id)
            .ok_or_else(BackendError::plan_unavailable)?;
        let binding = stored
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        if binding.owner_id != caller.owner_id {
            return Err(BackendError::plan_owner_mismatch());
        }
        if binding.session_id != caller.session_id {
            return Err(BackendError::plan_session_mismatch());
        }
        if plan_digest(&stored.plan)? != stored.digest {
            return Err(BackendError::plan_unavailable());
        }
        Ok(ClaimedPlan {
            plan: stored.plan.clone(),
            backend_plan_id: stored.backend_plan_id.clone(),
            origin: stored.origin,
            binding: stored.binding.clone(),
            digest: stored.digest.clone(),
            created_at: stored.created_at,
        })
    }

    pub fn replace_bound_available(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        expected_digest: &str,
        mut replacement: Plan,
    ) -> Result<Plan, BackendError> {
        let now = SystemTime::now();
        let mut plans = lock_plans(&self.plans);
        let stored = plans
            .get_mut(plan_id)
            .ok_or_else(BackendError::plan_unavailable)?;
        if expired(stored.created_at, now, self.ttl) {
            return Err(BackendError::plan_unavailable());
        }
        let binding = stored
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        if binding.owner_id != caller.owner_id {
            return Err(BackendError::plan_owner_mismatch());
        }
        if binding.session_id != caller.session_id {
            return Err(BackendError::plan_session_mismatch());
        }
        if stored.state != PlanState::Available
            || stored.digest != expected_digest
            || plan_digest(&stored.plan)? != expected_digest
        {
            return Err(BackendError::plan_unavailable());
        }
        if replacement.op_id != stored.plan.op_id || replacement.op_id != binding.operation_id {
            return Err(BackendError::validation(
                "replacement plan operation does not match its binding",
            ));
        }
        replacement.plan_id = plan_id.to_string();
        let replacement_digest = plan_digest(&replacement)?;
        stored.plan = replacement.clone();
        stored.digest = replacement_digest;
        Ok(replacement)
    }

    #[cfg(debug_assertions)]
    pub(crate) fn active_plan_count_for_testing(&self) -> usize {
        lock_plans(&self.plans).len()
    }

    #[cfg(test)]
    fn claim(&self, plan_id: &str) -> Result<ClaimedPlan, BackendError> {
        self.claim_checked(plan_id, None)
    }

    fn claim_bound(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
    ) -> Result<ClaimedPlan, BackendError> {
        self.claim_checked(plan_id, Some(caller))
    }

    fn claim_checked(
        &self,
        plan_id: &str,
        caller: Option<&CallerIdentity>,
    ) -> Result<ClaimedPlan, BackendError> {
        let now = SystemTime::now();
        let mut plans = lock_plans(&self.plans);
        if plans
            .get(plan_id)
            .is_some_and(|stored| expired(stored.created_at, now, self.ttl))
        {
            plans.remove(plan_id);
            return Err(BackendError::plan_unavailable());
        }
        let stored = plans
            .get_mut(plan_id)
            .ok_or_else(BackendError::plan_unavailable)?;
        if let Some(caller) = caller {
            let binding = stored
                .binding
                .as_ref()
                .ok_or_else(BackendError::plan_unavailable)?;
            if binding.owner_id != caller.owner_id {
                return Err(BackendError::plan_owner_mismatch());
            }
            if binding.session_id != caller.session_id {
                return Err(BackendError::plan_session_mismatch());
            }
        }
        if stored.state != PlanState::Available || plan_digest(&stored.plan)? != stored.digest {
            return Err(BackendError::plan_unavailable());
        }
        stored.state = PlanState::Claimed;
        Ok(ClaimedPlan {
            plan: stored.plan.clone(),
            backend_plan_id: stored.backend_plan_id.clone(),
            origin: stored.origin,
            binding: stored.binding.clone(),
            digest: stored.digest.clone(),
            created_at: stored.created_at,
        })
    }
}

fn lock_plans(
    plans: &Mutex<HashMap<String, StoredPlan>>,
) -> MutexGuard<'_, HashMap<String, StoredPlan>> {
    plans
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn expired(created_at: SystemTime, now: SystemTime, ttl: Duration) -> bool {
    now.duration_since(created_at).is_ok_and(|age| age >= ttl)
}

fn plan_digest(plan: &Plan) -> Result<String, BackendError> {
    let encoded = serde_json::to_vec(plan)
        .map_err(|_| BackendError::protocol("Plan could not be encoded"))?;
    let hash = Sha256::digest(encoded);
    let mut encoded_hash = String::with_capacity(hash.len() * 2);
    for byte in hash {
        write!(&mut encoded_hash, "{byte:02x}").expect("writing to a string cannot fail");
    }
    Ok(encoded_hash)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
        time::{Duration, SystemTime},
    };

    use crate::contracts::{JsonObject, Plan, Step, StepKind};

    use super::*;

    fn plan() -> Plan {
        Plan {
            plan_id: "client-authored-id".to_string(),
            op_id: "rfi.process_scans".to_string(),
            steps: vec![Step {
                kind: StepKind::Move,
                src: Some("/in/scan.pdf".to_string()),
                dst: Some("/out/RFI.pdf".to_string()),
                note_key: Some("note.renamed".to_string()),
                meta: JsonObject::new(),
            }],
            warnings: Vec::new(),
            meta: JsonObject::new(),
        }
    }

    #[test]
    fn plans_are_opaque_immutable_one_shot_values_with_provenance_and_digest() {
        let store = PlanStore::new(Duration::from_secs(300));
        let before = SystemTime::now();
        let mut review = store
            .insert(BackendOrigin::Rust, plan())
            .expect("plan should be cached");
        let plan_id = review.plan_id.clone();

        assert_ne!(plan_id, "client-authored-id");
        assert!(uuid::Uuid::parse_str(&plan_id).is_ok());
        let snapshot = store.snapshot(&plan_id).expect("stored plan snapshot");
        assert_eq!(snapshot.origin, BackendOrigin::Rust);
        assert_eq!(snapshot.state, PlanState::Available);
        assert_eq!(snapshot.digest.len(), 64);
        assert!(snapshot.created_at >= before);

        review.steps[0].dst = Some("/forged/victim.pdf".to_string());
        let claimed = store.claim(&plan_id).expect("first claim should succeed");
        assert_eq!(claimed.origin, BackendOrigin::Rust);
        assert_eq!(claimed.plan.steps[0].dst.as_deref(), Some("/out/RFI.pdf"));
        assert_eq!(claimed.digest, snapshot.digest);

        let reused = store.claim(&plan_id).expect_err("plan reuse must fail");
        let forged = store
            .claim("00000000-0000-4000-8000-000000000000")
            .expect_err("forged plan id must fail");
        assert_eq!(reused.code, "plan_not_found_or_expired");
        assert_eq!(forged, reused);
    }

    #[test]
    fn expired_plans_are_pruned_and_rejected_before_claiming() {
        let store = PlanStore::new(Duration::ZERO);
        let review = store
            .insert(BackendOrigin::Rust, plan())
            .expect("plan should initially be cached");

        assert!(store.snapshot(&review.plan_id).is_none());
        let error = store
            .claim(&review.plan_id)
            .expect_err("expired plan must not be claimable");
        assert_eq!(error.code, "plan_not_found_or_expired");
    }

    #[test]
    fn concurrent_apply_rejection_does_not_consume_the_waiting_plan() {
        let store = PlanStore::new(Duration::from_secs(300));
        let mutation_lock = crate::progress::GlobalMutationLock::default();
        let first = store
            .insert(BackendOrigin::Rust, plan())
            .expect("first plan should be cached");
        let waiting = store
            .insert(BackendOrigin::Rust, plan())
            .expect("waiting plan should be cached");
        let active = store
            .claim_for_apply(&first.plan_id, &mutation_lock)
            .expect("first apply should acquire and claim");

        let error = store
            .claim_for_apply(&waiting.plan_id, &mutation_lock)
            .expect_err("concurrent apply should be rejected before claiming");
        assert_eq!(error.code, "operation_in_progress");
        assert_eq!(
            store
                .snapshot(&waiting.plan_id)
                .expect("waiting plan should remain cached")
                .state,
            PlanState::Available
        );

        drop(active);
        assert!(
            store
                .claim_for_apply(&waiting.plan_id, &mutation_lock)
                .is_ok()
        );
    }

    #[test]
    fn replace_bound_available_preserves_identity_binding_provenance_and_ttl() {
        let store = PlanStore::new(Duration::from_secs(300));
        let caller = CallerIdentity::new("owner", "session");
        let binding = PlanBinding::new(
            "owner",
            "session",
            "rfi.process_scans",
            vec!["grant-a".to_string(), "grant-b".to_string()],
        );
        let review = store
            .insert_bound(BackendOrigin::Rust, binding.clone(), plan())
            .unwrap();
        let before = store.snapshot(&review.plan_id).unwrap();
        let original = store.resolve_bound(&review.plan_id, &caller).unwrap();
        let mut replacement = original.plan.clone();
        replacement.plan_id = "forged-replacement-id".to_string();
        replacement.steps[0].dst = Some("/out/revised.pdf".to_string());

        let revised = store
            .replace_bound_available(&review.plan_id, &caller, &before.digest, replacement)
            .unwrap();
        let after = store.snapshot(&review.plan_id).unwrap();
        let resolved = store.resolve_bound(&review.plan_id, &caller).unwrap();

        assert_eq!(revised.plan_id, review.plan_id);
        assert_eq!(resolved.plan, revised);
        assert_eq!(resolved.backend_plan_id, original.backend_plan_id);
        assert_eq!(resolved.origin, original.origin);
        assert_eq!(after.binding, Some(binding));
        assert_eq!(after.created_at, before.created_at);
        assert_eq!(after.state, PlanState::Available);
        assert_ne!(after.digest, before.digest);
        assert_eq!(resolved.digest, after.digest);
        assert_eq!(
            store.claim(&review.plan_id).unwrap().plan.steps[0]
                .dst
                .as_deref(),
            Some("/out/revised.pdf")
        );
    }

    fn bound_store() -> (PlanStore, CallerIdentity, Plan, PlanRecordSnapshot) {
        let store = PlanStore::new(Duration::from_secs(300));
        let caller = CallerIdentity::new("owner", "session");
        let review = store
            .insert_bound(
                BackendOrigin::Rust,
                PlanBinding::new(
                    "owner",
                    "session",
                    "rfi.process_scans",
                    vec!["grant".to_string()],
                ),
                plan(),
            )
            .unwrap();
        let snapshot = store.snapshot(&review.plan_id).unwrap();
        (store, caller, review, snapshot)
    }

    #[test]
    fn replace_bound_available_rejects_wrong_authority_digest_operation_and_state_unchanged() {
        for case in ["owner", "session", "digest", "operation", "claimed"] {
            let (store, caller, review, before) = bound_store();
            let mut replacement = review.clone();
            replacement.steps[0].dst = Some("/out/revised.pdf".to_string());
            let mut expected_digest = before.digest.clone();
            let attempted_caller = match case {
                "owner" => CallerIdentity::new("other", "session"),
                "session" => CallerIdentity::new("owner", "other"),
                _ => caller.clone(),
            };
            if case == "digest" {
                expected_digest = "00".repeat(32);
            }
            if case == "operation" {
                replacement.op_id = "other.operation".to_string();
            }
            if case == "claimed" {
                store.claim(&review.plan_id).unwrap();
            }
            let snapshot_before_attempt = store.snapshot(&review.plan_id).unwrap();

            assert!(
                store
                    .replace_bound_available(
                        &review.plan_id,
                        &attempted_caller,
                        &expected_digest,
                        replacement,
                    )
                    .is_err(),
                "case {case} must fail"
            );
            assert_eq!(
                store.snapshot(&review.plan_id).unwrap(),
                snapshot_before_attempt,
                "case {case} must not mutate stored authority"
            );
        }
    }

    #[test]
    fn replace_bound_available_rejects_expired_entry_without_removing_it() {
        let store = PlanStore::new(Duration::from_secs(1));
        let caller = CallerIdentity::new("owner", "session");
        let review = store
            .insert_bound(
                BackendOrigin::Rust,
                PlanBinding::new("owner", "session", "rfi.process_scans", Vec::new()),
                plan(),
            )
            .unwrap();
        let mut plans = lock_plans(&store.plans);
        let stored = plans.get_mut(&review.plan_id).unwrap();
        stored.created_at = SystemTime::UNIX_EPOCH;
        let before = stored.clone();
        drop(plans);

        assert!(
            store
                .replace_bound_available(&review.plan_id, &caller, &before.digest, review.clone(),)
                .is_err()
        );
        let plans = lock_plans(&store.plans);
        let after = plans.get(&review.plan_id).unwrap();
        assert_eq!(after.digest, before.digest);
        assert_eq!(after.plan, before.plan);
        assert_eq!(after.created_at, before.created_at);
        assert_eq!(after.state, before.state);
        assert_eq!(after.binding, before.binding);
    }

    #[test]
    fn replace_bound_available_concurrent_revisions_allow_exactly_one_digest_transition() {
        let (store, caller, review, before) = bound_store();
        let store = Arc::new(store);
        let barrier = Arc::new(Barrier::new(3));
        let handles = ["one", "two"].map(|suffix| {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let caller = caller.clone();
            let plan_id = review.plan_id.clone();
            let digest = before.digest.clone();
            let mut replacement = review.clone();
            replacement.steps[0].dst = Some(format!("/out/{suffix}.pdf"));
            thread::spawn(move || {
                barrier.wait();
                store.replace_bound_available(&plan_id, &caller, &digest, replacement)
            })
        });
        barrier.wait();
        let results = handles.map(|handle| handle.join().unwrap());

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        assert_ne!(
            store.snapshot(&review.plan_id).unwrap().digest,
            before.digest
        );
    }

    #[test]
    fn insert_bound_reserved_assigns_exact_id_and_reservation_is_store_inert() {
        let store = PlanStore::new(Duration::from_secs(300));
        let binding = PlanBinding::new(
            "owner",
            "session",
            "rfi.process_scans",
            vec!["grant".to_string()],
        );
        let reservation = store.reserve_plan_id();
        assert!(lock_plans(&store.plans).is_empty());

        let mut candidate = plan();
        candidate.plan_id.clear();
        reservation.assign_to_empty(&mut candidate).unwrap();
        let assigned = candidate.plan_id.clone();
        assert_eq!(Uuid::parse_str(&assigned).unwrap().to_string(), assigned);
        let expected = candidate.clone();

        let cached = store
            .insert_bound_reserved(BackendOrigin::Rust, binding, reservation, candidate)
            .unwrap();
        assert_eq!(cached, expected);
        assert_eq!(
            store
                .resolve_bound(&assigned, &CallerIdentity::new("owner", "session"))
                .unwrap()
                .plan,
            expected
        );
    }

    #[test]
    fn s5_preclaim_returns_nonforgeable_exact_authority_and_rejects_roster_mismatch_inertly() {
        use crate::{
            journal::JournalStore,
            paths::{GrantAccess, GrantScope, PathGrantStore},
        };

        let fixture = tempfile::tempdir().unwrap();
        let source_root = fixture.path().join("source");
        let target_root = fixture.path().join("target");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::create_dir_all(&target_root).unwrap();
        let caller = CallerIdentity::new("owner", "session");
        let scope = GrantScope::new("owner", "session", "request", "rfi.follow_up_sync");
        let grants = PathGrantStore::new(Duration::from_secs(300));
        let source = grants
            .issue(&scope, &source_root, GrantAccess::Read)
            .unwrap();
        let target = grants
            .issue(&scope, &target_root, GrantAccess::ReadWrite)
            .unwrap();
        let store = PlanStore::new(Duration::from_secs(300));
        let mut candidate = plan();
        candidate.op_id = "rfi.follow_up_sync".to_string();
        let cached = store
            .insert_bound(
                BackendOrigin::Rust,
                PlanBinding::new(
                    "owner",
                    "session",
                    "rfi.follow_up_sync",
                    vec![source.grant_id.clone(), target.grant_id.clone()],
                ),
                candidate,
            )
            .unwrap();

        let wrong_roster = grants
            .root_accesses_for_plan(
                std::slice::from_ref(&source.grant_id),
                &caller,
                "rfi.follow_up_sync",
            )
            .unwrap();
        let journal = JournalStore::new(fixture.path().join("journals"));
        let process = crate::progress::GlobalMutationLock::default();
        let permits = journal.bind_s5_permits(
            process.try_acquire().unwrap(),
            journal.try_acquire_mutation_lock().unwrap(),
        );
        assert!(
            store
                .claim_s5_for_apply_bound_under_permit(
                    &cached.plan_id,
                    &caller,
                    permits,
                    wrong_roster,
                )
                .is_err()
        );
        assert_eq!(
            store.snapshot(&cached.plan_id).unwrap().state,
            PlanState::Available
        );

        let roots = grants
            .root_accesses_for_plan(
                &[source.grant_id, target.grant_id],
                &caller,
                "rfi.follow_up_sync",
            )
            .unwrap();
        let permits = journal.bind_s5_permits(
            process.try_acquire().unwrap(),
            journal.try_acquire_mutation_lock().unwrap(),
        );
        let claim = store
            .claim_s5_for_apply_bound_under_permit(&cached.plan_id, &caller, permits, roots)
            .unwrap();
        assert_eq!(claim.plan().plan_id, cached.plan_id);
        assert_eq!(claim.binding().grant_ids.len(), 2);
        assert_eq!(claim.digest().len(), 64);
        assert_eq!(claim.caller(), &caller);
        assert_eq!(claim.roots().records().len(), 2);
    }
}
