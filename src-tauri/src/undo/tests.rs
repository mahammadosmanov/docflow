use super::*;
#[cfg(not(windows))]
use crate::{
    contracts::JsonObject,
    journal::{GrantRootIdentity, JournalRecord},
};

#[cfg(not(windows))]
fn committed_copy_record(root: &Path, destination: &Path) -> JournalRecord {
    let capability = CapabilityRoot::open(root).unwrap();
    let mut meta = JsonObject::new();
    meta.insert(
        "undoSha256".to_string(),
        Value::String(
            capability
                .authorize(destination)
                .unwrap()
                .anchor()
                .unwrap()
                .sha256()
                .unwrap(),
        ),
    );
    let mut record = JournalRecord::new(
        "undo-recovery-test",
        "op.test",
        "2026-08-25T00:00:00+00:00",
        "owner",
        "session",
        "digest",
    );
    record.steps.push(Step {
        kind: StepKind::Copy,
        src: Some(root.join("source.txt").to_string_lossy().into_owned()),
        dst: Some(destination.to_string_lossy().into_owned()),
        note_key: None,
        meta,
    });
    record.grant_roots = vec![root.to_string_lossy().into_owned()];
    record.grant_root_identities = vec![GrantRootIdentity {
        root: root.to_string_lossy().into_owned(),
        identity: capability.identity().unwrap(),
    }];
    record.state = Some(JournalState::Committed);
    record
}

fn assert_durable_outcome_unknown(journals: &JournalStore) -> serde_json::Value {
    let record = journals.read("undo-recovery-test").unwrap();
    assert_eq!(record.state, Some(JournalState::RecoveryRequired));
    assert_eq!(record.pending_undo, Some(0));
    let encoded = serde_json::to_value(record).unwrap();
    let recovery = encoded
        .get("undoRecovery")
        .expect("quarantine recovery locator must remain durable")
        .clone();
    #[cfg(not(windows))]
    assert!(Path::new(recovery["quarantine"].as_str().unwrap()).exists());
    assert_eq!(
        journals.recover_pending().unwrap()[0].disposition,
        crate::journal::RecoveryDisposition::OutcomeUnknown
    );
    recovery
}

#[cfg(not(windows))]
#[test]
fn rename_success_followed_by_directory_sync_failure_keeps_durable_undo_recovery() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(root.join("source.txt"), b"APPROVED").unwrap();
    let destination = root.join("copy.txt");
    std::fs::write(&destination, b"APPROVED").unwrap();
    let journals = JournalStore::new(&journal_dir);
    journals
        .write(&committed_copy_record(&root, &destination))
        .unwrap();
    crate::cap_fs::inject_directory_sync_failure_after(0);

    let result = undo_journal(
        &journals,
        "undo-recovery-test",
        &CallerIdentity::new("owner", "restart"),
        None,
    )
    .unwrap();

    assert_eq!(result.meta["effectState"], "outcome_unknown");
    let recovery = assert_durable_outcome_unknown(&journals);
    let quarantine = recovery["quarantine"].as_str().unwrap();
    assert!(Path::new(quarantine).exists());
}

#[cfg(not(windows))]
#[test]
fn hash_failure_and_quarantine_restore_collision_remain_recoverable() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(root.join("source.txt"), b"APPROVED").unwrap();
    let destination = root.join("copy.txt");
    std::fs::write(&destination, b"APPROVED").unwrap();
    let journals = JournalStore::new(&journal_dir);
    let mut record = committed_copy_record(&root, &destination);
    record.steps[0].meta.insert(
        "undoSha256".to_string(),
        Value::String("not-the-approved-hash".to_string()),
    );
    journals.write(&record).unwrap();
    let collision = destination.clone();
    inject_before_quarantine_restore(move || {
        std::fs::write(collision, b"USER COLLISION").unwrap();
    });

    let result = undo_journal(
        &journals,
        "undo-recovery-test",
        &CallerIdentity::new("owner", "restart"),
        None,
    )
    .unwrap();

    assert_eq!(result.meta["effectState"], "outcome_unknown");
    let recovery = assert_durable_outcome_unknown(&journals);
    assert_eq!(recovery["original"], destination.to_string_lossy().as_ref());
    assert_eq!(std::fs::read(destination).unwrap(), b"USER COLLISION");
}

#[test]
fn move_target_failure_retains_quarantine_and_recovery_without_restoring() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let source = root.join("source.txt");
    let destination = root.join("moved.txt");
    std::fs::write(&source, b"USER TARGET COLLISION").unwrap();
    std::fs::write(&destination, b"APPROVED MOVE").unwrap();
    let capability = CapabilityRoot::open(&root).unwrap();
    let mut meta = JsonObject::new();
    meta.insert(
        "undoSha256".to_string(),
        Value::String(
            capability
                .authorize(&destination)
                .unwrap()
                .anchor()
                .unwrap()
                .sha256()
                .unwrap(),
        ),
    );
    let mut record = JournalRecord::new(
        "undo-recovery-test",
        "op.test",
        "2026-08-25T00:00:00+00:00",
        "owner",
        "session",
        "digest",
    );
    record.steps.push(Step {
        kind: StepKind::Move,
        src: Some(source.to_string_lossy().into_owned()),
        dst: Some(destination.to_string_lossy().into_owned()),
        note_key: None,
        meta,
    });
    record.grant_roots = vec![root.to_string_lossy().into_owned()];
    record.grant_root_identities = vec![GrantRootIdentity {
        root: root.to_string_lossy().into_owned(),
        identity: capability.identity().unwrap(),
    }];
    record.state = Some(JournalState::Committed);
    let journals = JournalStore::new(&journal_dir);
    journals.write(&record).unwrap();

    let result = undo_journal(
        &journals,
        "undo-recovery-test",
        &CallerIdentity::new("owner", "restart"),
        None,
    )
    .unwrap();

    assert_eq!(result.meta["effectState"], "outcome_unknown");
    let recovery = assert_durable_outcome_unknown(&journals);
    assert_eq!(recovery["target"], source.to_string_lossy().as_ref());
    assert_eq!(std::fs::read(source).unwrap(), b"USER TARGET COLLISION");
    #[cfg(not(windows))]
    assert!(!destination.exists());
    #[cfg(windows)]
    assert_eq!(std::fs::read(&destination).unwrap(), b"APPROVED MOVE");
    #[cfg(not(windows))]
    assert!(Path::new(recovery["quarantine"].as_str().unwrap()).exists());
}

#[cfg(not(windows))]
#[test]
fn quarantine_delete_failure_retains_the_durable_locator() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    let journal_dir = fixture.path().join("journal");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(root.join("source.txt"), b"APPROVED").unwrap();
    let destination = root.join("copy.txt");
    std::fs::write(&destination, b"APPROVED").unwrap();
    let journals = JournalStore::new(&journal_dir);
    journals
        .write(&committed_copy_record(&root, &destination))
        .unwrap();
    inject_quarantine_delete_failure();

    let result = undo_journal(
        &journals,
        "undo-recovery-test",
        &CallerIdentity::new("owner", "restart"),
        None,
    )
    .unwrap();

    assert_eq!(result.meta["effectState"], "outcome_unknown");
    assert_durable_outcome_unknown(&journals);
}
