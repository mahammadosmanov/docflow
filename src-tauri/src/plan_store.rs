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
mod tests;
