use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

use docflow_lib::{
    contracts::{JsonObject, Plan, Step, StepKind},
    fs_ops::{FilesystemAuthority, copy_no_clobber, write_no_clobber},
    journal::{JournalRecord, JournalState, JournalStore, RecoveryDisposition},
    paths::{CallerIdentity, GrantAccess, GrantScope, PathGrantStore, PathIntent},
    plan_store::{BackendOrigin, PlanBinding, PlanStore},
    progress::GlobalMutationLock,
    review::recoverable_delete_plan,
};
use serde_json::json;
use tempfile::TempDir;

fn step(kind: StepKind, src: Option<&std::path::Path>, dst: Option<&std::path::Path>) -> Step {
    Step {
        kind,
        src: src.map(|path| path.to_string_lossy().into_owned()),
        dst: dst.map(|path| path.to_string_lossy().into_owned()),
        note_key: None,
        meta: JsonObject::new(),
    }
}

fn plan(op_id: &str, steps: Vec<Step>) -> Plan {
    Plan {
        plan_id: "client-authored-plan".to_string(),
        op_id: op_id.to_string(),
        steps,
        warnings: Vec::new(),
        meta: JsonObject::new(),
    }
}

fn scope(op_id: &str) -> GrantScope {
    GrantScope::new("owner-1", "session-1", "preview-request-1", op_id)
}

fn caller() -> CallerIdentity {
    CallerIdentity::new("owner-1", "session-1")
}

#[test]
fn plans_are_bound_to_owner_session_and_exact_cached_payload() {
    let store = PlanStore::new(Duration::from_secs(300));
    let binding = PlanBinding::new("owner-1", "session-1", "op.test", Vec::new());
    let original = plan(
        "op.test",
        vec![step(
            StepKind::Write,
            None,
            Some(std::path::Path::new("/approved/output.txt")),
        )],
    );
    let mut review = store
        .insert_bound(BackendOrigin::Rust, binding.clone(), original)
        .expect("preview plan should be cached");
    let plan_id = review.plan_id.clone();

    review.steps[0].dst = Some("/forged/victim.txt".to_string());
    let wrong_owner = store
        .claim_for_apply_bound(
            &plan_id,
            &CallerIdentity::new("owner-2", "session-1"),
            &GlobalMutationLock::default(),
        )
        .expect_err("another owner must not claim the plan");
    assert_eq!(wrong_owner.code, "plan_owner_mismatch");
    let wrong_session = store
        .claim_for_apply_bound(
            &plan_id,
            &CallerIdentity::new("owner-1", "session-2"),
            &GlobalMutationLock::default(),
        )
        .expect_err("another session must not claim the plan");
    assert_eq!(wrong_session.code, "plan_session_mismatch");

    let claim = store
        .claim_for_apply_bound(&plan_id, &caller(), &GlobalMutationLock::default())
        .expect("the bound caller should claim once");
    assert_eq!(
        claim.claimed.plan.steps[0].dst.as_deref(),
        Some("/approved/output.txt")
    );

    let expiring_store = PlanStore::new(Duration::ZERO);
    let expired = expiring_store
        .insert_bound(BackendOrigin::Rust, binding, plan("op.test", Vec::new()))
        .expect("preview can be returned before the zero TTL is checked");
    assert_eq!(
        expiring_store
            .claim_for_apply_bound(&expired.plan_id, &caller(), &GlobalMutationLock::default(),)
            .expect_err("an unknown/expired identity must fail")
            .code,
        "plan_not_found_or_expired"
    );
}

#[test]
fn path_grants_reject_wrong_scope_traversal_and_symlink_escape() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("root");
    let outside = fixture.path().join("outside");
    fs::create_dir_all(&root).expect("grant root");
    fs::create_dir_all(&outside).expect("outside root");
    fs::write(root.join("sentinel.txt"), b"KEEP").expect("sentinel");
    fs::write(outside.join("secret.txt"), b"SECRET").expect("outside fixture");

    let grants = PathGrantStore::new(Duration::from_secs(300));
    let grant_scope = scope("op.test");
    let grant = grants
        .issue(&grant_scope, &root, GrantAccess::ReadWrite)
        .expect("selected root should produce a grant");

    assert_eq!(
        grants
            .authorize(
                &grant.grant_id,
                &GrantScope::new("owner-2", "session-1", "preview-request-1", "op.test",),
                &root.join("sentinel.txt"),
                PathIntent::Read,
            )
            .expect_err("grant owner is part of the authority")
            .code,
        "path_grant_scope_mismatch"
    );
    assert_eq!(
        grants
            .authorize(
                &grant.grant_id,
                &grant_scope,
                &root.join("child/../../outside/secret.txt"),
                PathIntent::Read,
            )
            .expect_err("lexical traversal is rejected even if normalized later")
            .code,
        "path_traversal"
    );

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("escape")).expect("escape symlink");
        assert_eq!(
            grants
                .authorize(
                    &grant.grant_id,
                    &grant_scope,
                    &root.join("escape/secret.txt"),
                    PathIntent::Read,
                )
                .expect_err("a symlink may not escape the selected root")
                .code,
            "path_escape"
        );
    }

    assert_eq!(fs::read(root.join("sentinel.txt")).unwrap(), b"KEEP");
    assert_eq!(fs::read(outside.join("secret.txt")).unwrap(), b"SECRET");
}

#[cfg(unix)]
#[test]
fn authorized_parent_replacement_is_rejected_immediately_before_effect() {
    use std::os::unix::fs::symlink;

    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("root");
    let parent = root.join("approved-parent");
    let outside = fixture.path().join("outside");
    fs::create_dir_all(&parent).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let destination = parent.join("new.txt");
    let grants = PathGrantStore::new(Duration::from_secs(300));
    let grant_scope = scope("op.test");
    let grant = grants
        .issue(&grant_scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let caller = caller();

    grants
        .authorize_for_plan(
            std::slice::from_ref(&grant.grant_id),
            &caller,
            "op.test",
            &destination,
            PathIntent::Write,
        )
        .expect("initial authorization should succeed");
    fs::remove_dir(&parent).unwrap();
    symlink(&outside, &parent).unwrap();

    assert_eq!(
        grants
            .revalidate_for_plan(
                std::slice::from_ref(&grant.grant_id),
                &caller,
                "op.test",
                &destination,
                PathIntent::Write,
            )
            .unwrap_err()
            .code,
        "path_escape"
    );
    assert!(!outside.join("new.txt").exists());
}

#[test]
fn no_clobber_primitives_preserve_existing_destinations_and_resolve_races() {
    let fixture = TempDir::new().expect("temporary fixture");
    let source_a = fixture.path().join("a.bin");
    let source_b = fixture.path().join("b.bin");
    let destination = fixture.path().join("destination.bin");
    fs::write(&source_a, b"AAAA").unwrap();
    fs::write(&source_b, b"BBBB").unwrap();
    fs::write(&destination, b"KEEP").unwrap();

    let collision = copy_no_clobber(&source_a, &destination)
        .expect_err("an existing destination must never be overwritten");
    assert_eq!(collision.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&destination).unwrap(), b"KEEP");

    fs::remove_file(&destination).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for source in [source_a, source_b] {
        let barrier = Arc::clone(&barrier);
        let destination = destination.clone();
        workers.push(thread::spawn(move || {
            barrier.wait();
            copy_no_clobber(&source, &destination)
        }));
    }
    barrier.wait();
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().expect("copy worker"))
        .collect::<Vec<_>>();

    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| outcome
                .as_ref()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists))
            .count(),
        1
    );
    assert!(matches!(
        fs::read(&destination).unwrap().as_slice(),
        b"AAAA" | b"BBBB"
    ));

    let write_collision = write_no_clobber(&destination, b"REPLACE")
        .expect_err("atomic writes also use no-replace publication");
    assert_eq!(write_collision.kind(), std::io::ErrorKind::AlreadyExists);
}

#[test]
fn apply_destination_collision_is_durably_no_effect_and_retryable() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    let source = root.join("source.txt");
    let destination = root.join("destination.txt");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"SOURCE").unwrap();
    fs::write(&destination, b"KEEP").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant = authority
        .issue_grant(&scope("op.test"), &root, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner-1", "session-1", "op.test", vec![grant.grant_id]),
            plan(
                "op.test",
                vec![step(StepKind::Copy, Some(&source), Some(&destination))],
            ),
        )
        .unwrap();
    let result = authority.apply(&cached.plan_id, &caller()).unwrap();

    assert_eq!((result.applied, result.failed), (0, 1));
    assert_eq!(result.meta["effectState"], json!("no_effect_retryable"));
    assert_eq!(fs::read(&source).unwrap(), b"SOURCE");
    assert_eq!(fs::read(&destination).unwrap(), b"KEEP");
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.state, Some(JournalState::NoEffectRetryable));
    assert!(journal.steps.is_empty());
    assert!(journal.pending_step.is_none());
}

#[test]
fn approved_plan_applies_journals_and_undoes_losslessly_in_reverse() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    let input = root.join("in/scan.pdf");
    let output_dir = root.join("out/code");
    let output = output_dir.join("code_R0.pdf");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::write(&input, b"%PDF sentinel").unwrap();
    fs::write(root.join("unrelated.txt"), b"KEEP").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant_scope = scope("rfi.process_scans");
    let grant = authority
        .issue_grant(&grant_scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let mut approved = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                "rfi.process_scans",
                vec![grant.grant_id.clone()],
            ),
            plan(
                "rfi.process_scans",
                vec![
                    step(StepKind::Mkdir, None, Some(&output_dir)),
                    step(StepKind::Move, Some(&input), Some(&output)),
                ],
            ),
        )
        .unwrap();
    let plan_id = approved.plan_id.clone();
    approved.steps[1].dst = Some(root.join("unrelated.txt").to_string_lossy().into_owned());

    let result = authority.apply(&plan_id, &caller()).unwrap();
    assert_eq!((result.applied, result.failed), (2, 0));
    assert!(!input.exists());
    assert_eq!(fs::read(&output).unwrap(), b"%PDF sentinel");
    assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(), b"KEEP");

    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.state, Some(JournalState::Committed));
    assert_eq!(journal.steps.last().unwrap().kind, StepKind::Move);
    assert!(
        journal
            .steps
            .last()
            .unwrap()
            .meta
            .contains_key("undoSha256")
    );

    let undone = authority.undo(&result.journal_id, &caller(), None).unwrap();
    assert_eq!(
        (undone.applied, undone.failed),
        (3, 0),
        "{:?}",
        undone.errors
    );
    assert_eq!(fs::read(&input).unwrap(), b"%PDF sentinel");
    assert!(!output_dir.exists());
    assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(), b"KEEP");
}

#[test]
fn partial_failure_and_partial_cancellation_leave_recoverable_journals() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.txt");
    let first = root.join("first.txt");
    let second = root.join("second.txt");
    fs::write(&source, b"ORIGINAL").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant_scope = scope("op.test");
    let grant = authority
        .issue_grant(&grant_scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let cache = |steps| {
        authority
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new(
                    "owner-1",
                    "session-1",
                    "op.test",
                    vec![grant.grant_id.clone()],
                ),
                plan("op.test", steps),
            )
            .unwrap()
    };

    let partial = cache(vec![
        step(StepKind::Copy, Some(&source), Some(&first)),
        step(
            StepKind::Move,
            Some(&root.join("missing.txt")),
            Some(&second),
        ),
    ]);
    let result = authority.apply(&partial.plan_id, &caller()).unwrap();
    assert_eq!((result.applied, result.failed), (1, 1));
    assert_eq!(result.meta["effectState"], json!("partial_committed"));
    assert_eq!(
        authority.journals().read(&result.journal_id).unwrap().state,
        Some(JournalState::PartialCommitted)
    );

    fs::remove_file(&first).unwrap();
    let cancelled = cache(vec![
        step(StepKind::Copy, Some(&source), Some(&first)),
        step(StepKind::Copy, Some(&source), Some(&second)),
    ]);
    let cancelled_id = cancelled.plan_id.clone();
    let cancelled_result = authority
        .apply_with_progress(&cancelled_id, &caller(), |event| {
            if event.done == 1 {
                assert!(authority.cancel(&cancelled_id));
            }
        })
        .unwrap();
    assert_eq!((cancelled_result.applied, cancelled_result.failed), (1, 0));
    assert_eq!(
        cancelled_result.meta["effectState"],
        json!("cancelled_partial")
    );
    assert!(first.exists());
    assert!(!second.exists());
    assert_eq!(
        authority
            .journals()
            .read(&cancelled_result.journal_id)
            .unwrap()
            .state,
        Some(JournalState::CancelledPartial)
    );
}

#[test]
fn cancellation_before_the_first_effect_is_clean_and_durable() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.txt");
    let destination = root.join("destination.txt");
    fs::write(&source, b"ORIGINAL").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant = authority
        .issue_grant(&scope("op.test"), &root, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner-1", "session-1", "op.test", vec![grant.grant_id]),
            plan(
                "op.test",
                vec![step(StepKind::Move, Some(&source), Some(&destination))],
            ),
        )
        .unwrap();
    let plan_id = cached.plan_id.clone();
    let result = authority
        .apply_with_progress(&plan_id, &caller(), |event| {
            if event.done == 0 {
                assert!(authority.cancel(&plan_id));
            }
        })
        .unwrap();

    assert_eq!((result.applied, result.failed), (0, 0));
    assert_eq!(result.meta["effectState"], json!("cancelled_clean"));
    assert_eq!(fs::read(&source).unwrap(), b"ORIGINAL");
    assert!(!destination.exists());
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.state, Some(JournalState::CancelledClean));
    assert!(journal.steps.is_empty());
    assert!(journal.pending_step.is_none());
}

#[test]
fn restart_recovery_reports_pending_effect_without_trusting_it() {
    let fixture = TempDir::new().expect("temporary fixture");
    let journal_dir = fixture.path().join("journal");
    let root = fixture.path().join("documents");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.txt");
    let destination = root.join("destination.txt");
    fs::write(&source, b"DATA").unwrap();

    let store = JournalStore::new(&journal_dir);
    let mut record = JournalRecord::new(
        "journal-restart",
        "op.test",
        "2026-08-25T00:00:00+00:00",
        "owner-1",
        "session-1",
        "digest",
    );
    record.pending_step = Some(step(StepKind::Copy, Some(&source), Some(&destination)));
    record.state = Some(JournalState::Applying);
    store.write(&record).unwrap();
    copy_no_clobber(&source, &destination).unwrap();
    drop(store);

    let restarted = JournalStore::new(&journal_dir);
    let recovery = restarted.recover_pending().unwrap();
    assert_eq!(recovery.len(), 1);
    assert_eq!(recovery[0].journal_id, "journal-restart");
    assert_eq!(recovery[0].disposition, RecoveryDisposition::OutcomeUnknown);
    assert_eq!(fs::read(&source).unwrap(), b"DATA");
    assert_eq!(fs::read(&destination).unwrap(), b"DATA");
}

#[test]
fn legacy_protocol_journal_bytes_and_omissions_round_trip_exactly() {
    let fixture = TempDir::new().expect("temporary fixture");
    let journal_dir = fixture.path().join("journal");
    fs::create_dir_all(&journal_dir).unwrap();
    let expected = r#"{
  "journalId": "legacy-journal",
  "opId": "rfi.process_scans",
  "ts": "2026-08-25T00:00:00+00:00",
  "steps": [
    {
      "kind": "copy",
      "src": "/tmp/girdi-ş.pdf",
      "dst": "/tmp/çıktı.pdf",
      "noteKey": null,
      "meta": {}
    }
  ]
}"#;
    let path = journal_dir.join("2026-08-25T00-00-00+00-00_legacy-j.json");
    fs::write(&path, expected.as_bytes()).unwrap();

    let store = JournalStore::new(&journal_dir);
    let record = store.read("legacy-journal").unwrap();
    assert_eq!(record.state, None);
    assert_eq!(record.owner_id, None);
    assert_eq!(record.pending_step, None);
    store.write(&record).unwrap();

    assert_eq!(fs::read_to_string(path).unwrap(), expected);
}

#[test]
fn journal_timestamp_cannot_escape_the_journal_directory() {
    let fixture = TempDir::new().expect("temporary fixture");
    let journal_dir = fixture.path().join("journal");
    let store = JournalStore::new(&journal_dir);
    let record = JournalRecord::new(
        "journal-safe-id",
        "op.test",
        "../escaped",
        "owner-1",
        "session-1",
        "digest",
    );

    let error = store
        .write(&record)
        .expect_err("timestamp-derived filenames must remain contained");
    assert_eq!(error.code, "journal_invalid");
    assert!(!fixture.path().join("escaped_journal-.json").exists());
}

#[test]
fn undo_refuses_wrong_owner_and_later_edits_and_keeps_nonempty_created_dirs() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    let source = root.join("source.txt");
    let created = root.join("created");
    let destination = created.join("copy.txt");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"ORIGINAL").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant_scope = scope("op.test");
    let grant = authority
        .issue_grant(&grant_scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner-1", "session-1", "op.test", vec![grant.grant_id]),
            plan(
                "op.test",
                vec![
                    step(StepKind::Mkdir, None, Some(&created)),
                    step(StepKind::Copy, Some(&source), Some(&destination)),
                ],
            ),
        )
        .unwrap();
    let result = authority.apply(&cached.plan_id, &caller()).unwrap();

    assert_eq!(
        authority
            .undo(
                &result.journal_id,
                &CallerIdentity::new("owner-2", "session-1"),
                None,
            )
            .expect_err("only the journal owner/session may authorize undo")
            .code,
        "journal_owner_mismatch"
    );

    fs::write(&destination, b"USER EDIT").unwrap();
    fs::write(created.join("user.txt"), b"KEEP").unwrap();
    let undone = authority.undo(&result.journal_id, &caller(), None).unwrap();
    assert_eq!(undone.failed, 1);
    assert_eq!(fs::read(&destination).unwrap(), b"USER EDIT");
    assert_eq!(fs::read(created.join("user.txt")).unwrap(), b"KEEP");
    assert!(created.exists());
}

#[test]
fn durable_journal_undo_survives_a_new_backend_session_for_the_same_owner() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    let source = root.join("source.txt");
    let destination = root.join("copy.txt");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"ORIGINAL").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant = authority
        .issue_grant(&scope("op.test"), &root, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner-1", "session-1", "op.test", vec![grant.grant_id]),
            plan(
                "op.test",
                vec![step(StepKind::Copy, Some(&source), Some(&destination))],
            ),
        )
        .unwrap();
    let result = authority.apply(&cached.plan_id, &caller()).unwrap();

    let undone = authority
        .undo(
            &result.journal_id,
            &CallerIdentity::new("owner-1", "session-after-restart"),
            None,
        )
        .expect("a durable local journal is owner-bound, not process-session-bound");
    assert_eq!((undone.applied, undone.failed), (1, 0));
    assert!(!destination.exists());
    assert_eq!(fs::read(&source).unwrap(), b"ORIGINAL");
}

#[cfg(unix)]
#[test]
fn undo_rejects_a_replacement_directory_at_the_original_grant_root_path() {
    let fixture = TempDir::new().expect("temporary fixture");
    let visible_root = fixture.path().join("documents");
    let original_root = fixture.path().join("original-documents");
    let journal_dir = fixture.path().join("journal");
    let source = visible_root.join("source.txt");
    let destination = visible_root.join("copy.txt");
    fs::create_dir_all(&visible_root).unwrap();
    fs::write(&source, b"ORIGINAL").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant = authority
        .issue_grant(&scope("op.test"), &visible_root, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner-1", "session-1", "op.test", vec![grant.grant_id]),
            plan(
                "op.test",
                vec![step(StepKind::Copy, Some(&source), Some(&destination))],
            ),
        )
        .unwrap();
    let applied = authority.apply(&cached.plan_id, &caller()).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"ORIGINAL");

    fs::rename(&visible_root, &original_root).unwrap();
    fs::create_dir(&visible_root).unwrap();
    fs::write(visible_root.join("source.txt"), b"ORIGINAL").unwrap();
    fs::write(visible_root.join("copy.txt"), b"ORIGINAL").unwrap();

    let error = authority
        .undo(&applied.journal_id, &caller(), None)
        .expect_err("undo must bind the root pathname to the object granted at apply time");

    assert_eq!(error.code, "journal_invalid");
    assert_eq!(
        fs::read(visible_root.join("source.txt")).unwrap(),
        b"ORIGINAL"
    );
    assert_eq!(
        fs::read(visible_root.join("copy.txt")).unwrap(),
        b"ORIGINAL"
    );
    assert_eq!(
        fs::read(original_root.join("copy.txt")).unwrap(),
        b"ORIGINAL"
    );
}

#[test]
fn legacy_process_scans_journal_still_supports_guarded_undo() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&journal_dir).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let source = root.join("source.txt");
    let destination = root.join("copy.txt");
    fs::write(&source, b"LEGACY APPROVED BYTES").unwrap();
    fs::write(&destination, b"LEGACY APPROVED BYTES").unwrap();
    let hash = docflow_lib::fs_ops::file_sha256(&destination).unwrap();
    let journal_path = journal_dir.join("2026-08-25T00-00-00+00-00_legacy-r.json");
    fs::write(
        &journal_path,
        serde_json::to_vec_pretty(&json!({
            "journalId": "legacy-real-journal",
            "opId": "rfi.process_scans",
            "ts": "2026-08-25T00:00:00+00:00",
            "steps": [{
                "kind": "copy",
                "src": source,
                "dst": destination,
                "noteKey": null,
                "meta": {"undoSha256": hash}
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let before = fs::read_to_string(&journal_path).unwrap();
    assert!(!before.contains("grantRoots"));
    assert!(!before.contains("grantRootIdentities"));
    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));

    let undone = authority
        .undo("legacy-real-journal", &caller(), None)
        .expect("trusted local legacy journal should migrate and undo");

    assert_eq!((undone.applied, undone.failed), (1, 0));
    assert_eq!(fs::read(&source).unwrap(), b"LEGACY APPROVED BYTES");
    assert!(!destination.exists());
    let migrated = fs::read_to_string(&journal_path).unwrap();
    assert!(migrated.contains("grantRoots"));
    assert!(migrated.contains("grantRootIdentities"));
}

#[test]
fn migration_era_journal_pins_and_persists_missing_root_identities_before_undo() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    fs::create_dir_all(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let source = root.join("source.txt");
    let destination = root.join("copy.txt");
    fs::write(&source, b"MIGRATION ERA BYTES").unwrap();
    fs::write(&destination, b"MIGRATION ERA BYTES").unwrap();
    let mut copy = step(StepKind::Copy, Some(&source), Some(&destination));
    copy.meta.insert(
        "undoSha256".to_string(),
        json!(docflow_lib::fs_ops::file_sha256(&destination).unwrap()),
    );
    let mut record = JournalRecord::new(
        "migration-era-journal",
        "op.test",
        "2026-08-25T00:00:00+00:00",
        "owner-1",
        "session-old",
        "digest",
    );
    record.steps.push(copy);
    record.grant_roots = vec![root.to_string_lossy().into_owned()];
    record.state = Some(JournalState::Committed);
    let store = JournalStore::new(&journal_dir);
    store.write(&record).unwrap();
    let before = serde_json::to_value(store.read("migration-era-journal").unwrap()).unwrap();
    assert!(before.get("grantRootIdentities").is_none());

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let undone = authority
        .undo("migration-era-journal", &caller(), None)
        .expect("current roots should be pinned and identified before the inverse");

    assert_eq!((undone.applied, undone.failed), (1, 0));
    assert!(!destination.exists());
    let migrated = serde_json::to_value(store.read("migration-era-journal").unwrap()).unwrap();
    assert_eq!(migrated["grantRootIdentities"].as_array().unwrap().len(), 1);
}

#[test]
fn review_delete_is_an_approved_recoverable_move_with_trusted_undo() {
    let fixture = TempDir::new().expect("temporary fixture");
    let documents = fixture.path().join("documents");
    let review = documents.join("_review");
    let app_data = fixture.path().join("app-data");
    let trash = app_data.join("trash");
    let journal_dir = app_data.join("journal");
    fs::create_dir_all(&review).unwrap();
    fs::create_dir_all(&trash).unwrap();
    let source = review.join("unreadable.pdf");
    fs::write(&source, b"PDF TO REVIEW").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant_scope = scope("rfi.review");
    let document_grant = authority
        .issue_grant(&grant_scope, &documents, GrantAccess::ReadWrite)
        .unwrap();
    let trash_grant = authority
        .issue_grant(&grant_scope, &app_data, GrantAccess::ReadWrite)
        .unwrap();
    let preview = recoverable_delete_plan(
        &documents,
        &trash,
        std::slice::from_ref(&source),
        "20260825T010203000004",
    )
    .expect("review delete should produce an inert plan");

    assert!(source.exists(), "preview must not move the review file");
    assert_eq!(preview.steps.len(), 1);
    assert_eq!(preview.steps[0].kind, StepKind::Move);
    let trashed = std::path::PathBuf::from(preview.steps[0].dst.as_ref().unwrap());
    assert_eq!(
        trashed.parent(),
        Some(fs::canonicalize(&trash).unwrap().as_path())
    );

    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                "rfi.review",
                vec![document_grant.grant_id, trash_grant.grant_id],
            ),
            preview,
        )
        .unwrap();
    let applied = authority.apply(&cached.plan_id, &caller()).unwrap();

    assert_eq!((applied.applied, applied.failed), (1, 0));
    assert!(!source.exists());
    assert_eq!(fs::read(&trashed).unwrap(), b"PDF TO REVIEW");

    let undone = authority
        .undo(&applied.journal_id, &caller(), None)
        .unwrap();
    assert_eq!((undone.applied, undone.failed), (1, 0));
    assert_eq!(fs::read(&source).unwrap(), b"PDF TO REVIEW");
    assert!(!trashed.exists());
}

#[test]
fn legacy_file_step_creates_journals_and_undoes_missing_parent_directories() {
    let fixture = TempDir::new().expect("temporary fixture");
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    let source = root.join("scan.pdf");
    let destination = root.join("_review/nested/scan.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"UNREADABLE PDF").unwrap();

    let authority = FilesystemAuthority::new(&journal_dir, Duration::from_secs(300));
    let grant = authority
        .issue_grant(&scope("rfi.process_scans"), &root, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                "rfi.process_scans",
                vec![grant.grant_id],
            ),
            plan(
                "rfi.process_scans",
                vec![step(StepKind::Move, Some(&source), Some(&destination))],
            ),
        )
        .unwrap();

    let applied = authority.apply(&cached.plan_id, &caller()).unwrap();
    assert_eq!((applied.applied, applied.failed), (1, 0));
    assert_eq!(fs::read(&destination).unwrap(), b"UNREADABLE PDF");
    let journal = authority.journals().read(&applied.journal_id).unwrap();
    assert_eq!(
        journal
            .steps
            .iter()
            .map(|entry| entry.kind)
            .collect::<Vec<_>>(),
        vec![StepKind::Mkdir, StepKind::Mkdir, StepKind::Move]
    );

    let undone = authority
        .undo(&applied.journal_id, &caller(), None)
        .unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"UNREADABLE PDF");
    assert!(!root.join("_review").exists());
    assert_eq!((undone.applied, undone.failed), (3, 0));
}
