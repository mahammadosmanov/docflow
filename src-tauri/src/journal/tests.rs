use super::*;

fn test_identity(value: u64) -> FilesystemObjectIdentity {
    FilesystemObjectIdentity::Unix {
        device: 1,
        inode: value,
    }
}

fn retained_publication_record(root: &Path) -> JournalRecord {
    let source = root.join("source.xlsx");
    let target = root.join("target.xlsx");
    let backup = root.join(".docflow-s5-backup-test.xlsx");
    let step = Step {
        kind: crate::contracts::StepKind::WorkbookUpdate,
        src: Some(target.to_string_lossy().into_owned()),
        dst: Some(backup.to_string_lossy().into_owned()),
        note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
        meta: serde_json::Map::new(),
    };
    let mut record = JournalRecord::new(
        "journal-s5",
        "rfi.follow_up_sync",
        "2026-08-29T00-00-00Z",
        "owner",
        "session",
        "a".repeat(64),
    );
    let root_text = root.to_string_lossy().into_owned();
    record.state = Some(JournalState::Applying);
    record.grant_roots = vec![root_text.clone()];
    record.grant_root_identities = vec![GrantRootIdentity {
        root: root_text.clone(),
        identity: test_identity(1),
    }];
    record.grant_root_accesses = vec![GrantRootAccessRecord {
        root: root_text,
        identity: test_identity(1),
        access: PersistedRootAccess::ReadWrite,
    }];
    record.s5_authority = Some(S5AuthorityRecord {
        initial_step: step,
        source_path: source.to_string_lossy().into_owned(),
        source_identity: test_identity(2),
        target_path: target.to_string_lossy().into_owned(),
        target_identity: test_identity(3),
        approved_updates_sha256: "b".repeat(64),
    });
    record
}

fn failed_apply_restore_record(root: &Path) -> JournalRecord {
    let source = root.join("source.xlsx");
    let target = root.join("target.xlsx");
    let backup = root.join(".docflow-s5-backup-journal-failed-apply.xlsx");
    let retained = root.join(".docflow-s5-retained-original-journal-failed-apply.xlsx");
    let private = root.join(".docflow-backups/.docflow-s5-private-output.xlsx");
    let locator = "journal:journal-failed-apply;root:0;leaf:.docflow-s5-retained-original-journal-failed-apply.xlsx";
    let step = Step {
        kind: crate::contracts::StepKind::WorkbookUpdate,
        src: Some(target.to_string_lossy().into_owned()),
        dst: Some(backup.to_string_lossy().into_owned()),
        note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
        meta: serde_json::Map::new(),
    };
    let root_text = root.to_string_lossy().into_owned();
    let mut record = JournalRecord::new(
        "journal-failed-apply",
        "rfi.follow_up_sync",
        "2026-08-29T00-00-00Z",
        "owner",
        "session",
        "a".repeat(64),
    );
    record.state = Some(JournalState::RecoveryRequired);
    record.grant_roots = vec![root_text.clone()];
    record.grant_root_identities = vec![GrantRootIdentity {
        root: root_text.clone(),
        identity: test_identity(1),
    }];
    record.grant_root_accesses = vec![GrantRootAccessRecord {
        root: root_text,
        identity: test_identity(1),
        access: PersistedRootAccess::ReadWrite,
    }];
    record.s5_authority = Some(S5AuthorityRecord {
        initial_step: step,
        source_path: source.to_string_lossy().into_owned(),
        source_identity: test_identity(2),
        target_path: target.to_string_lossy().into_owned(),
        target_identity: test_identity(3),
        approved_updates_sha256: "b".repeat(64),
    });
    record.pending_retained_artifact = Some(PendingRetainedArtifact {
        role: RetainedArtifactRole::DisplacedOriginal,
        path: retained.to_string_lossy().into_owned(),
        locator: locator.to_string(),
        creation_state: PendingArtifactCreationState::Created(test_identity(3)),
    });
    record.pending_publication = Some(RetainedPublication {
        purpose: RetainedPublicationPurpose::Apply,
        canonical: target.to_string_lossy().into_owned(),
        retained: retained.to_string_lossy().into_owned(),
        backup: backup.to_string_lossy().into_owned(),
        publication_source: private.to_string_lossy().into_owned(),
        expected_canonical_identity: test_identity(3),
        expected_canonical_sha256: "c".repeat(64),
        expected_backup_identity: test_identity(4),
        expected_backup_sha256: "c".repeat(64),
        publication_source_identity: test_identity(5),
        publication_sha256: "e".repeat(64),
        expected_canonical_security_sha256: "d".repeat(64),
        expected_backup_security_sha256: "d".repeat(64),
        publication_security_sha256: "f".repeat(64),
        expected_canonical_default_stream_bytes: 1,
        expected_canonical_auxiliary_charge_bytes: 0,
        expected_canonical_logical_charge_bytes: 1,
        destination_access_sha256: "1".repeat(64),
        destination_auxiliary_sha256: "2".repeat(64),
        destination_auxiliary_charge_bytes: 0,
        retained_identity: Some(test_identity(3)),
        retained_sha256: Some("c".repeat(64)),
        retained_security_sha256: Some("d".repeat(64)),
        created_canonical_identity: None,
        phase: RetainedPublicationPhase::RecoveryRequired,
    });
    record.recovery_notice = Some(RecoveryNotice {
        warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
        locator: locator.to_string(),
    });
    record.follow_up_phase = Some(FollowUpJournalPhase::PublicationActive);
    record
}

#[test]
fn failed_follow_up_apply_with_exact_displaced_original_is_eligible_for_restore_undo() {
    let fixture = tempfile::tempdir().unwrap();
    let store = JournalStore::new(fixture.path());
    let permits = store.bind_s5_permits(
        crate::progress::GlobalMutationLock::default()
            .try_acquire()
            .unwrap(),
        store.try_acquire_mutation_lock().unwrap(),
    );
    let mut token =
        ValidatedS5Record::for_test(failed_apply_restore_record(fixture.path()), permits).unwrap();
    store.write_s5_cas(&mut token, 0).unwrap();
    drop(token);

    let permits = store.bind_s5_permits(
        crate::progress::GlobalMutationLock::default()
            .try_acquire()
            .unwrap(),
        store.try_acquire_mutation_lock().unwrap(),
    );
    let pending = store
        .inspect_s5_for_undo(
            "journal-failed-apply",
            &crate::paths::CallerIdentity::new("owner", "session"),
            permits,
        )
        .unwrap();

    assert_eq!(pending.mode(), S5UndoMode::RestoreFailedApply);
}

struct FailedApplyRestoreFixture {
    _directory: tempfile::TempDir,
    authority: crate::fs_ops::FilesystemAuthority,
    store: JournalStore,
    caller: crate::paths::CallerIdentity,
    target: PathBuf,
    retained: PathBuf,
    backup: PathBuf,
    private: PathBuf,
    original: Vec<u8>,
}

fn seed_failed_apply_restore_fixture() -> FailedApplyRestoreFixture {
    use std::time::Duration;

    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    let journal_root = root.join("journals");
    let source = root.join("source.xlsx");
    let target = root.join("target.xlsx");
    let backup = root.join(".docflow-s5-backup-journal-failed-apply.xlsx");
    let retained = root.join(".docflow-s5-retained-original-journal-failed-apply.xlsx");
    let private = root.join(".docflow-backups/.docflow-s5-private-output.xlsx");
    let original = b"original Follow Up workbook bytes".to_vec();
    let applied = b"approved updated Follow Up bytes";
    fs::create_dir_all(private.parent().unwrap()).unwrap();
    fs::write(&source, b"RFI Log workbook bytes").unwrap();
    fs::write(&target, &original).unwrap();
    fs::copy(&target, &backup).unwrap();
    fs::write(&private, applied).unwrap();

    let authority =
        crate::fs_ops::FilesystemAuthority::new(&journal_root, Duration::from_secs(300));
    let caller = crate::paths::CallerIdentity::new("owner", "session");
    authority
        .issue_grant(
            &crate::paths::GrantScope::new("owner", "session", "request", "rfi.follow_up_sync"),
            &root,
            crate::paths::GrantAccess::ReadWrite,
        )
        .unwrap();

    let source_file = File::open(&source).unwrap();
    let target_file = File::open(&target).unwrap();
    let backup_file = File::open(&backup).unwrap();
    let private_file = File::open(&private).unwrap();
    let source_identity = crate::cap_fs::file_identity(&source_file).unwrap();
    let target_identity = crate::cap_fs::file_identity(&target_file).unwrap();
    let backup_identity = crate::cap_fs::file_identity(&backup_file).unwrap();
    let private_identity = crate::cap_fs::file_identity(&private_file).unwrap();
    let original_snapshot =
        crate::cap_fs::FileSecuritySnapshot::capture(&target_file, None).unwrap();
    let backup_snapshot = crate::cap_fs::FileSecuritySnapshot::capture(&backup_file, None).unwrap();
    let private_snapshot =
        crate::cap_fs::FileSecuritySnapshot::capture(&private_file, None).unwrap();
    let original_hash = format!("{:x}", Sha256::digest(&original));
    let backup_hash = format!("{:x}", Sha256::digest(fs::read(&backup).unwrap()));
    let private_hash = format!("{:x}", Sha256::digest(applied));
    let root_identity = crate::cap_fs::CapabilityRoot::open(&root)
        .unwrap()
        .identity()
        .unwrap();
    fs::rename(&target, &retained).unwrap();

    let mut record = failed_apply_restore_record(&root);
    record.grant_root_identities[0].identity = root_identity.clone();
    record.grant_root_accesses[0].identity = root_identity;
    record.s5_authority.as_mut().unwrap().source_identity = source_identity;
    record.s5_authority.as_mut().unwrap().target_identity = target_identity.clone();
    let publication = record.pending_publication.as_mut().unwrap();
    publication.expected_canonical_identity = target_identity.clone();
    publication.expected_canonical_sha256 = original_hash.clone();
    publication.expected_canonical_security_sha256 = original_snapshot.sha256();
    publication.expected_backup_identity = backup_identity;
    publication.expected_backup_sha256 = backup_hash;
    publication.expected_backup_security_sha256 = backup_snapshot.sha256();
    publication.publication_source_identity = private_identity;
    publication.publication_sha256 = private_hash;
    publication.publication_security_sha256 = private_snapshot.sha256();
    publication.expected_canonical_default_stream_bytes = u64::try_from(original.len()).unwrap();
    publication.expected_canonical_auxiliary_charge_bytes =
        original_snapshot.auxiliary_charge_bytes();
    publication.expected_canonical_logical_charge_bytes = u64::try_from(original.len())
        .unwrap()
        .checked_add(original_snapshot.auxiliary_charge_bytes())
        .unwrap();
    publication.destination_access_sha256 = original_snapshot.access_sha256();
    publication.destination_auxiliary_sha256 = original_snapshot.auxiliary_sha256();
    publication.destination_auxiliary_charge_bytes = original_snapshot.auxiliary_charge_bytes();
    publication.retained_identity = Some(target_identity.clone());
    publication.retained_sha256 = Some(original_hash);
    publication.retained_security_sha256 = Some(original_snapshot.sha256());
    let pending = record.pending_retained_artifact.as_mut().unwrap();
    pending.creation_state = PendingArtifactCreationState::Created(target_identity);

    let store = JournalStore::new(&journal_root);
    let permits = store.bind_s5_permits(
        crate::progress::GlobalMutationLock::default()
            .try_acquire()
            .unwrap(),
        store.try_acquire_mutation_lock().unwrap(),
    );
    let mut token = ValidatedS5Record::for_test(record, permits).unwrap();
    store.write_s5_cas(&mut token, 0).unwrap();

    FailedApplyRestoreFixture {
        _directory: directory,
        authority,
        store,
        caller,
        target,
        retained,
        backup,
        private,
        original,
    }
}

#[test]
fn failed_follow_up_apply_undo_restores_the_exact_original_and_keeps_backup() {
    let fixture = seed_failed_apply_restore_fixture();

    let result = fixture
        .authority
        .undo("journal-failed-apply", &fixture.caller, None)
        .unwrap();

    assert_eq!((result.applied, result.failed), (1, 0));
    assert_eq!(fs::read(&fixture.target).unwrap(), fixture.original);
    assert!(!fixture.retained.exists());
    assert!(fixture.backup.exists());
    assert!(fixture.private.exists());
    let record = fixture
        .store
        .read_s5_for_test("journal-failed-apply")
        .unwrap();
    assert_eq!(record.state, Some(JournalState::Undone));
    assert!(record.pending_publication.is_none());
    assert!(
        record
            .retained_evidence
            .iter()
            .all(|evidence| { evidence.role != RetainedArtifactRole::DisplacedOriginal })
    );
    assert!(
        fixture
            .authority
            .recover_pending(&fixture.caller)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn recovery_finalizes_an_exact_user_initiated_restore_without_moving_files() {
    let fixture = seed_failed_apply_restore_fixture();
    fs::rename(&fixture.retained, &fixture.target).unwrap();

    let mut record = fixture
        .store
        .read_s5_for_test("journal-failed-apply")
        .unwrap();
    record.state = Some(JournalState::Applying);
    record.recovery_notice = None;
    record.pending_publication.as_mut().unwrap().phase =
        RetainedPublicationPhase::RestoringOriginal;
    let permits = fixture.store.bind_s5_permits(
        crate::progress::GlobalMutationLock::default()
            .try_acquire()
            .unwrap(),
        fixture.store.try_acquire_mutation_lock().unwrap(),
    );
    let mut token = ValidatedS5Record::for_test(record, permits).unwrap();
    let revision = token.revision();
    fixture.store.write_s5_cas(&mut token, revision).unwrap();
    drop(token);

    let entries = fixture.authority.recover_pending(&fixture.caller).unwrap();
    assert!(entries.is_empty(), "{entries:?}");

    assert_eq!(fs::read(&fixture.target).unwrap(), fixture.original);
    assert!(!fixture.retained.exists());
    assert!(fixture.backup.exists());
    assert!(fixture.private.exists());
    let recovered = fixture
        .store
        .read_s5_for_test("journal-failed-apply")
        .unwrap();
    assert_eq!(recovered.state, Some(JournalState::Undone));
    assert!(recovered.pending_publication.is_none());
}

#[test]
fn failed_follow_up_apply_restore_never_clobbers_a_new_canonical_file() {
    let fixture = seed_failed_apply_restore_fixture();
    fs::write(&fixture.target, b"a later user file").unwrap();

    assert!(
        fixture
            .authority
            .undo("journal-failed-apply", &fixture.caller, None)
            .is_err()
    );

    assert_eq!(fs::read(&fixture.target).unwrap(), b"a later user file");
    assert_eq!(fs::read(&fixture.retained).unwrap(), fixture.original);
    let entries = fixture.authority.recover_pending(&fixture.caller).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].restore_original_available);
}

#[cfg(windows)]
#[test]
fn journal_replace_retries_transient_windows_access_denied() {
    use std::{cell::Cell, time::Duration};

    let attempts = Cell::new(0);
    let mut waits = Vec::new();

    retry_windows_atomic_replace(
        || {
            let attempt = attempts.get() + 1;
            attempts.set(attempt);
            if attempt < 3 {
                Err(io::Error::from_raw_os_error(5))
            } else {
                Ok(())
            }
        },
        |duration| waits.push(duration),
    )
    .unwrap();

    assert_eq!(attempts.get(), 3);
    assert_eq!(
        waits,
        [Duration::from_millis(100), Duration::from_millis(100)]
    );
}

#[cfg(windows)]
#[test]
fn journal_store_survives_a_transient_windows_file_lock() {
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;
    use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

    let fixture = tempfile::tempdir().unwrap();
    let store = JournalStore::new(fixture.path());
    let mut record = JournalRecord::new(
        "journal-lock-test",
        "rfi.process_scans",
        "2026-08-26T00-00-00Z",
        "owner",
        "session",
        "digest",
    );
    record.state = Some(JournalState::Applying);
    store.write(&record).unwrap();

    let target = store.path_for(&record);
    let blocker = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(target)
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        drop(blocker);
    });

    record.state = Some(JournalState::Committed);
    store.write(&record).unwrap();
    release.join().unwrap();

    assert_eq!(
        store.read(&record.journal_id).unwrap().state,
        Some(JournalState::Committed)
    );
}

#[test]
fn retained_publication_legacy_json_defaults_additive_s5_fields_and_omits_them_again() {
    let legacy = serde_json::json!({
        "journalId": "legacy-journal",
        "opId": "rfi.process_scans",
        "ts": "2026-08-29T00-00-00Z",
        "steps": []
    });

    let record: JournalRecord = serde_json::from_value(legacy).unwrap();
    assert_eq!(record.revision, 0);
    assert!(record.s5_authority.is_none());
    assert!(record.pending_publication.is_none());
    assert!(record.pending_artifact_parent.is_none());
    assert!(record.created_artifact_parent.is_none());
    assert!(record.pending_retained_artifact.is_none());
    assert!(record.retained_evidence.is_empty());
    assert!(record.recovery_notice.is_none());
    assert!(record.follow_up_phase.is_none());
    assert!(record.grant_root_accesses.is_empty());

    let encoded = serde_json::to_value(record).unwrap();
    for key in [
        "s5Authority",
        "pendingPublication",
        "pendingArtifactParent",
        "createdArtifactParent",
        "pendingRetainedArtifact",
        "retainedEvidence",
        "recoveryNotice",
        "followUpPhase",
        "grantRootAccesses",
        "revision",
    ] {
        assert!(encoded.get(key).is_none(), "legacy output exposed {key}");
    }
}

#[test]
fn retained_publication_recovery_projection_is_additive_and_wire_exact() {
    let entry = RecoveryEntry {
        journal_id: "journal-s5".to_string(),
        disposition: RecoveryDisposition::OutcomeUnknown,
        warning_key: Some("error.rfi.followUp.recoveryRequired".to_string()),
        locator: Some("journal:journal-s5;root:0;leaf:evidence.xlsx".to_string()),
        restore_original_available: false,
    };

    assert_eq!(
        serde_json::to_value(entry).unwrap(),
        serde_json::json!({
            "journalId": "journal-s5",
            "disposition": "outcome_unknown",
            "warningKey": "error.rfi.followUp.recoveryRequired",
            "locator": "journal:journal-s5;root:0;leaf:evidence.xlsx",
            "restoreOriginalAvailable": false
        })
    );
}

#[test]
fn final_notice_recovery_needs_no_root_reopen() {
    let fixture = tempfile::tempdir().unwrap();
    let store = JournalStore::new(fixture.path().join("journals"));
    let process_lock = crate::progress::GlobalMutationLock::default();
    let permits = store.bind_s5_permits(
        process_lock.try_acquire().unwrap(),
        store.try_acquire_mutation_lock().unwrap(),
    );
    let mut token =
        ValidatedS5Record::for_test(retained_publication_record(fixture.path()), permits).unwrap();
    token
        .prepare_failed_partial(RecoveryNotice {
            warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
            locator: "journal:journal-s5;root:0;leaf:target.xlsx".to_string(),
        })
        .unwrap();

    store.write_s5_cas(&mut token, 0).unwrap();
    let path = store.path_for(&token.record);
    drop(token);
    let before = fs::read(&path).unwrap();

    assert_eq!(fs::read(&path).unwrap(), before);

    let process_lock = crate::progress::GlobalMutationLock::default();
    let permits = store.bind_s5_permits(
        process_lock.try_acquire().unwrap(),
        store.try_acquire_mutation_lock().unwrap(),
    );
    let mut session = store.begin_s5_recovery(permits).unwrap();
    assert!(session.next_root_accesses().is_none());
    let recovered = session
        .recover_next(None, &crate::paths::CallerIdentity::new("owner", "session"))
        .unwrap()
        .unwrap();
    assert_eq!(recovered.journal_id, "journal-s5");
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn retained_publication_validation_rejects_roster_hash_charge_and_locator_drift() {
    let fixture = tempfile::tempdir().unwrap();
    let mut record = retained_publication_record(fixture.path());
    validate_record(&record).unwrap();

    record
        .grant_root_accesses
        .push(record.grant_root_accesses[0].clone());
    assert!(validate_record(&record).is_err());
    record.grant_root_accesses.pop();

    record.retained_evidence.push(RetainedArtifactEvidence {
        role: RetainedArtifactRole::Backup,
        path: fixture
            .path()
            .join(".docflow-s5-backup-evidence.xlsx")
            .to_string_lossy()
            .into_owned(),
        identity: test_identity(4),
        sha256: "c".repeat(64),
        security_sha256: "d".repeat(64),
        default_stream_bytes: 10,
        auxiliary_charge_bytes: 5,
        logical_charge_bytes: 14,
        locator: "journal:journal-s5;root:0;leaf:.docflow-s5-backup-evidence.xlsx".to_string(),
    });
    assert!(validate_record(&record).is_err());
    record.retained_evidence[0].logical_charge_bytes = 15;
    validate_record(&record).unwrap();

    record.retained_evidence[0].sha256 = "UPPER".to_string();
    assert!(validate_record(&record).is_err());
    record.retained_evidence[0].sha256 = "c".repeat(64);
    record.retained_evidence[0].locator = "x".repeat(MAX_RECOVERY_LOCATOR_SCALARS + 1);
    assert!(validate_record(&record).is_err());
}

#[test]
fn validated_s5_record_cas_is_monotonic_and_legacy_routes_reject_s5() {
    let fixture = tempfile::tempdir().unwrap();
    let store = JournalStore::new(fixture.path().join("journals"));
    let mut raw = retained_publication_record(fixture.path());
    let process_lock = crate::progress::GlobalMutationLock::default();
    let permits = store.bind_s5_permits(
        process_lock.try_acquire().unwrap(),
        store.try_acquire_mutation_lock().unwrap(),
    );
    let mut token = ValidatedS5Record::for_test(raw.clone(), permits).unwrap();

    store.write_s5_cas(&mut token, 0).unwrap();
    assert_eq!(token.revision(), 1);
    assert_eq!(store.read_s5_for_test("journal-s5").unwrap().revision, 1);

    token
        .apply_transition(S5Transition::SetPendingArtifactParent(
            PendingArtifactParent {
                path: fixture
                    .path()
                    .join(".docflow-backups")
                    .to_string_lossy()
                    .into_owned(),
                locator: "journal:journal-s5;root:0;leaf:.docflow-backups".to_string(),
            },
        ))
        .unwrap();
    assert!(store.write_s5_cas(&mut token, 0).is_err());
    assert_eq!(token.revision(), 1);
    assert!(
        store
            .read_s5_for_test("journal-s5")
            .unwrap()
            .pending_artifact_parent
            .is_none()
    );
    store.write_s5_cas(&mut token, 1).unwrap();
    assert_eq!(token.revision(), 2);

    assert!(store.read("journal-s5").is_err());
    raw.revision = 2;
    assert!(store.write(&raw).is_err());
}

#[test]
fn interprocess_lock_waits_briefly_for_a_release_on_another_thread() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let store = JournalStore::new(root);
    let (locked_tx, locked_rx) = std::sync::mpsc::channel::<()>();

    std::thread::scope(|scope| {
        let holder = scope.spawn(move || {
            let permit = JournalStore::new(root).try_acquire_mutation_lock().unwrap();
            locked_tx.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30));
            drop(permit);
        });
        locked_rx.recv().unwrap();
        let waited = store.try_acquire_mutation_lock();
        assert!(
            waited.is_ok(),
            "a short hold must be waited out, not reported as contention"
        );
        drop(waited);
        holder.join().unwrap();
    });
}

#[test]
fn interprocess_lock_reenters_on_holding_thread_and_excludes_other_threads() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let contended_elsewhere = || {
        std::thread::scope(|scope| {
            scope
                .spawn(|| JournalStore::new(root).try_acquire_mutation_lock().is_err())
                .join()
                .unwrap()
        })
    };
    let store = JournalStore::new(root);

    let outer = store.try_acquire_mutation_lock().unwrap();
    let nested = store
        .try_acquire_mutation_lock()
        .expect("the holding thread must re-enter its own lock");
    assert!(contended_elsewhere(), "another thread must stay excluded");

    drop(nested);
    assert!(contended_elsewhere(), "outer permit still excludes others");

    drop(outer);
    let free_elsewhere = std::thread::scope(|scope| {
        scope
            .spawn(|| JournalStore::new(root).try_acquire_mutation_lock().is_ok())
            .join()
            .unwrap()
    });
    assert!(
        free_elsewhere,
        "lock must release after the last permit drops"
    );
}
