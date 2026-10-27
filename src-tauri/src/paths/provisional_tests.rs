use super::*;

#[test]
fn issue_provisional_revokes_uncommitted_grants_and_preserves_committed_grants() {
    let fixture = tempfile::tempdir().unwrap();
    let first = fixture.path().join("first");
    let second = fixture.path().join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    let store = PathGrantStore::new(Duration::from_secs(300));
    let scope = GrantScope::new("owner", "session", "request", "op.test");

    let dropped_id = {
        let provisional = store
            .issue_provisional(
                &scope,
                &[
                    (first.clone(), GrantAccess::Read),
                    (second.clone(), GrantAccess::ReadWrite),
                ],
            )
            .unwrap();
        provisional.grants()[0].grant_id.clone()
    };
    assert!(
        store
            .authorize(&dropped_id, &scope, &first, PathIntent::Read)
            .is_err()
    );

    let provisional = store
        .issue_provisional(&scope, &[(first.clone(), GrantAccess::Read)])
        .unwrap();
    let committed_id = provisional.grants()[0].grant_id.clone();
    provisional.commit();
    assert_eq!(
        store
            .authorize(&committed_id, &scope, &first, PathIntent::Read)
            .unwrap(),
        fs::canonicalize(first).unwrap()
    );
}

#[cfg(windows)]
#[test]
fn dual_same_root_grants_preserve_the_write_capability_for_artifact_creation() {
    let fixture = tempfile::tempdir().unwrap();
    let store = PathGrantStore::new(Duration::from_secs(300));
    let scope = GrantScope::new("owner", "session", "request", "op.test");
    let provisional = store
        .issue_provisional(
            &scope,
            &[
                (fixture.path().to_path_buf(), GrantAccess::Read),
                (fixture.path().to_path_buf(), GrantAccess::ReadWrite),
            ],
        )
        .unwrap();
    let grant_ids = provisional
        .grants()
        .iter()
        .map(|grant| grant.grant_id.clone())
        .collect::<Vec<_>>();
    let caller = CallerIdentity {
        owner_id: "owner".to_string(),
        session_id: "session".to_string(),
    };
    let parent = fixture.path().join(".docflow-backups");
    let authorized = store
        .authorize_capability_for_plan(&grant_ids, &caller, "op.test", &parent, PathIntent::Write)
        .unwrap();
    let outcome = authorized.create_artifact_parent_no_clobber().unwrap();
    assert!(
        matches!(
            outcome,
            crate::cap_fs::ArtifactParentCreateOutcome::Created { .. }
        ),
        "{outcome:?}"
    );
}
