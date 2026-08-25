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
    paths::CallerIdentity,
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
    use std::time::{Duration, SystemTime};

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
}
