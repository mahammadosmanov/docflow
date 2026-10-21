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
            .claim_s5_for_apply_bound_under_permit(&cached.plan_id, &caller, permits, wrong_roster,)
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
