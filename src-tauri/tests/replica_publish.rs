use std::{fs, time::Duration};

use docflow_lib::{
    fs_ops::FilesystemAuthority,
    operations::replica_publish::{self, ReplicaPublishParams},
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    rfi_system_sync::DISCIPLINES,
};

fn caller() -> CallerIdentity {
    CallerIdentity::new("owner", "session")
}

#[test]
fn publication_is_backup_first_hash_verified_and_losslessly_undoable() {
    let fixture = tempfile::tempdir().unwrap();
    let roots = ["a", "b", "c"].map(|name| fixture.path().join(name));
    let backup = fixture.path().join("backup");
    for root in &roots {
        for discipline in DISCIPLINES {
            fs::create_dir_all(root.join(discipline)).unwrap();
        }
    }
    fs::create_dir_all(&backup).unwrap();
    fs::write(roots[0].join("CIV/changed.pdf"), b"new").unwrap();
    fs::write(roots[0].join("MEC/missing.pdf"), b"missing").unwrap();
    fs::write(roots[1].join("CIV/changed.pdf"), b"old").unwrap();
    fs::write(roots[2].join("CIV/changed.pdf"), b"old").unwrap();
    let params = ReplicaPublishParams {
        root_a: roots[0].clone(),
        root_b: roots[1].clone(),
        root_c: roots[2].clone(),
        source_root: "a".to_string(),
        backup_root: backup.clone(),
        root_files: String::new(),
        unattended: false,
        interval_minutes: 15,
    };
    let plan = replica_publish::preview(&params, |_| {}).unwrap();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let scope = GrantScope::new("owner", "session", "request", replica_publish::OPERATION_ID);
    let grants = vec![
        authority
            .issue_grant(&scope, &roots[0], GrantAccess::Read)
            .unwrap(),
        authority
            .issue_grant(&scope, &roots[1], GrantAccess::ReadWrite)
            .unwrap(),
        authority
            .issue_grant(&scope, &roots[2], GrantAccess::ReadWrite)
            .unwrap(),
        authority
            .issue_grant(&scope, &backup, GrantAccess::ReadWrite)
            .unwrap(),
    ];
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner",
                "session",
                replica_publish::OPERATION_ID,
                grants.into_iter().map(|grant| grant.grant_id).collect(),
            ),
            plan,
        )
        .unwrap();

    let applied = authority.apply(&cached.plan_id, &caller()).unwrap();
    assert_eq!(applied.failed, 0);
    for target in &roots[1..] {
        assert_eq!(fs::read(target.join("CIV/changed.pdf")).unwrap(), b"new");
        assert_eq!(
            fs::read(target.join("MEC/missing.pdf")).unwrap(),
            b"missing"
        );
    }
    let backups = walk_files(&backup);
    assert_eq!(backups.len(), 2);
    assert!(backups.iter().all(|path| fs::read(path).unwrap() == b"old"));

    authority
        .undo(&applied.journal_id, &caller(), None)
        .unwrap();
    for target in &roots[1..] {
        assert_eq!(fs::read(target.join("CIV/changed.pdf")).unwrap(), b"old");
        assert!(!target.join("MEC/missing.pdf").exists());
    }
    assert_eq!(
        walk_files(&backup).len(),
        2,
        "retained backups are never auto-deleted"
    );
}

fn walk_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    files
}
