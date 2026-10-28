use super::*;

#[test]
fn sign_out_and_re_sign_in_revoke_old_plan_and_selection_ids() {
    let temporary = tempfile::tempdir().unwrap();
    let old = CallerIdentity::new("owner", "session-old");
    let fresh = CallerIdentity::new("owner", "session-fresh");
    let selections = PathSelectionStore::new(Duration::from_secs(60));
    let selected = selections
        .issue(
            &old,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();
    let plans = crate::plan_store::PlanStore::new(Duration::from_secs(60));
    let plan = plans
        .insert_bound(
            BackendOrigin::Rust,
            PlanBinding::new("owner", "session-old", "rfi.review", Vec::new()),
            crate::contracts::Plan {
                plan_id: "backend-plan".to_string(),
                op_id: "rfi.review".to_string(),
                steps: Vec::new(),
                warnings: Vec::new(),
                meta: Map::new(),
            },
        )
        .unwrap();

    selections.revoke(&old);
    plans.revoke(&old);

    assert_eq!(
        selections
            .resolve_scope(
                &selected.selection_id,
                &fresh,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
    assert_eq!(
        plans.resolve_bound(&plan.plan_id, &fresh).unwrap_err().code,
        "plan_not_found_or_expired"
    );
}

#[test]
fn attempted_and_same_uid_sign_in_replacements_rotate_and_revoke_capabilities() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let initial = backend.caller_identity().unwrap();
    let selected = backend
        .selections
        .issue(
            &initial,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();
    let grant = backend
        .filesystem
        .issue_grant(
            &GrantScope::new(&initial.owner_id, &initial.session_id, "request", "op.test"),
            temporary.path(),
            GrantAccess::ReadWrite,
        )
        .unwrap();
    let plan = backend
        .filesystem
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &initial.owner_id,
                &initial.session_id,
                "op.test",
                vec![grant.grant_id],
            ),
            crate::contracts::Plan {
                plan_id: "ignored".to_string(),
                op_id: "op.test".to_string(),
                steps: Vec::new(),
                warnings: Vec::new(),
                meta: Map::new(),
            },
        )
        .unwrap();

    assert_eq!(
        backend
            .dispatch(
                BackendMethod::AuthSignIn,
                json!({"identifier": "not-an-email", "password": "password"}),
            )
            .unwrap_err()
            .code,
        "auth_failed"
    );
    let after_attempt = backend.caller_identity().unwrap();
    assert_ne!(after_attempt.session_id, initial.session_id);
    assert_eq!(
        backend
            .selections
            .resolve_scope(
                &selected.selection_id,
                &after_attempt,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
    assert_eq!(
        backend
            .filesystem
            .apply(&plan.plan_id, &after_attempt)
            .unwrap_err()
            .code,
        "plan_not_found_or_expired"
    );

    let selected_after_attempt = backend
        .selections
        .issue(
            &after_attempt,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();
    let grant_after_attempt = backend
        .filesystem
        .issue_grant(
            &GrantScope::new(
                &after_attempt.owner_id,
                &after_attempt.session_id,
                "request-after-attempt",
                "op.test",
            ),
            temporary.path(),
            GrantAccess::ReadWrite,
        )
        .unwrap();
    let plan_after_attempt = backend
        .filesystem
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &after_attempt.owner_id,
                &after_attempt.session_id,
                "op.test",
                vec![grant_after_attempt.grant_id],
            ),
            crate::contracts::Plan {
                plan_id: "ignored-again".to_string(),
                op_id: "op.test".to_string(),
                steps: Vec::new(),
                warnings: Vec::new(),
                meta: Map::new(),
            },
        )
        .unwrap();

    backend
        .dispatch(
            BackendMethod::AuthSignIn,
            json!({"identifier": "user@example.com", "password": "password"}),
        )
        .unwrap();
    let after_success = backend.caller_identity().unwrap();
    assert_eq!(after_success.owner_id, initial.owner_id);
    assert_ne!(after_success.session_id, after_attempt.session_id);
    assert_eq!(
        backend
            .selections
            .resolve_scope(
                &selected_after_attempt.selection_id,
                &after_success,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
    assert_eq!(
        backend
            .filesystem
            .apply(&plan_after_attempt.plan_id, &after_success)
            .unwrap_err()
            .code,
        "plan_not_found_or_expired"
    );
}

#[test]
fn disallowed_same_uid_reauthentication_cannot_revive_old_capabilities() {
    let temporary = tempfile::tempdir().unwrap();
    let responses = auth_responses("owner")
        .into_iter()
        .chain([Ok(HttpResponse::from_json(
            200,
            &json!([{
                "id": "owner",
                "email": "user@example.com",
                "allowed": false,
                "is_admin": false
            }]),
        ))])
        .chain(auth_responses("owner"))
        .collect();
    let backend = test_backend_with_responses(&temporary, responses);
    let initial = backend.caller_identity().unwrap();
    let selected = backend
        .selections
        .issue(
            &initial,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();

    let denied = backend.auth.load_me(now()).unwrap();
    assert!(denied.signed_in);
    assert!(!denied.allowed);
    backend
        .dispatch(
            BackendMethod::AuthSignIn,
            json!({"identifier": "user@example.com", "password": "password"}),
        )
        .unwrap();

    let replacement = backend.caller_identity().unwrap();
    assert_ne!(replacement.session_id, initial.session_id);
    assert_eq!(
        backend
            .selections
            .resolve_scope(
                &selected.selection_id,
                &replacement,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
}

#[test]
fn dispatch_sign_out_stays_revoked_when_settings_cleanup_fails() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let initial = backend.caller_identity().unwrap();
    let initial_epoch = initial.session_id.clone();
    let selected = backend
        .selections
        .issue(
            &initial,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();

    let settings_path = temporary.path().join("settings.json");
    std::fs::remove_file(&settings_path).unwrap();
    std::fs::create_dir(&settings_path).unwrap();

    assert_eq!(
        backend
            .dispatch(BackendMethod::AuthSignOut, json!({}))
            .unwrap_err()
            .code,
        "settings_write_failed"
    );
    assert!(!backend.auth_state().signed_in);
    assert_ne!(
        *backend
            .process_session_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        initial_epoch
    );
    assert_eq!(
        backend
            .selections
            .resolve_scope(
                &selected.selection_id,
                &initial,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
}
