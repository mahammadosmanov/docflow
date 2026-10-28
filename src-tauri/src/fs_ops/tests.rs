use super::*;

mod pdf_publish;

#[test]
fn generated_outlook_msg_write_is_no_clobber_and_undoable() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("archive");
    fs::create_dir(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let destination = root.join("RFI response.msg");
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let grant = authority
        .issue_grant(
            &GrantScope::new(
                "owner",
                "session",
                "request",
                "rfi.outlook_archive_selected",
            ),
            &root,
            GrantAccess::Write,
        )
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner",
                "session",
                "rfi.outlook_archive_selected",
                vec![grant.grant_id],
            ),
            Plan {
                plan_id: String::new(),
                op_id: "rfi.outlook_archive_selected".to_string(),
                steps: vec![Step {
                    kind: StepKind::Write,
                    src: Some("RFI response".to_string()),
                    dst: Some(destination.to_string_lossy().into_owned()),
                    note_key: Some("step.rfi.outlookArchive".to_string()),
                    meta: serde_json::json!({
                        "generatedBy": "outlook_msg",
                        "format": "msg"
                    })
                    .as_object()
                    .unwrap()
                    .clone(),
                }],
                warnings: Vec::new(),
                meta: JsonObject::new(),
            },
        )
        .unwrap();
    let caller = CallerIdentity::new("owner", "session");
    let bytes = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1fixture".to_vec();

    let result = authority
        .apply_generated_binary_write(
            &cached.plan_id,
            &caller,
            "rfi.outlook_archive_selected",
            |_| {},
            || Ok(bytes.clone()),
        )
        .unwrap();

    assert_eq!(result.applied, 1);
    assert_eq!(fs::read(&destination).unwrap(), bytes);
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.steps[0].kind, StepKind::Write);
    assert!(!journal.steps[0].meta.contains_key("generatedBy"));
    let undone = authority.undo(&result.journal_id, &caller, None).unwrap();
    assert_eq!(undone.applied, 1);
    assert!(!destination.exists());

    fs::write(&destination, b"existing").unwrap();
    let collision_grant = authority
        .issue_grant(
            &GrantScope::new(
                "owner",
                "session",
                "collision-request",
                "rfi.outlook_archive_selected",
            ),
            &root,
            GrantAccess::Write,
        )
        .unwrap();
    let collision = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner",
                "session",
                "rfi.outlook_archive_selected",
                vec![collision_grant.grant_id],
            ),
            Plan {
                plan_id: String::new(),
                op_id: "rfi.outlook_archive_selected".to_string(),
                steps: vec![Step {
                    kind: StepKind::Write,
                    src: Some("RFI response".to_string()),
                    dst: Some(destination.to_string_lossy().into_owned()),
                    note_key: Some("step.rfi.outlookArchive".to_string()),
                    meta: serde_json::json!({"generatedBy": "outlook_msg"})
                        .as_object()
                        .unwrap()
                        .clone(),
                }],
                warnings: Vec::new(),
                meta: JsonObject::new(),
            },
        )
        .unwrap();
    let collision_result = authority
        .apply_generated_binary_write(
            &collision.plan_id,
            &caller,
            "rfi.outlook_archive_selected",
            |_| {},
            || Ok(bytes.clone()),
        )
        .unwrap();
    assert_eq!(collision_result.applied, 0);
    assert_eq!(collision_result.failed, 1);
    assert_eq!(fs::read(&destination).unwrap(), b"existing");
}

#[test]
fn workbook_output_sync_uses_a_writable_handle_for_windows() {
    let source = include_str!("effects.rs");
    let workbook_update = source
        .split("fn execute_workbook_update(")
        .nth(1)
        .expect("workbook update implementation must remain inspectable")
        .split("fn execute_pdf_first_page_replace(")
        .next()
        .expect("workbook update implementation must remain bounded");

    assert!(workbook_update.contains("OpenOptions::new()"));
    assert!(workbook_update.contains(".read(true)"));
    assert!(workbook_update.contains(".write(true)"));
    assert!(!workbook_update.contains("File::open(&output)"));
}

#[test]
fn a_fault_after_a_move_effect_keeps_the_pending_step_for_recovery() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    let source = root.join("source.pdf");
    let destination = root.join("destination.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"PDF").unwrap();

    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let scope = GrantScope::new("owner", "session", "request", "op.test");
    let grant = authority
        .issue_grant(&scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner", "session", "op.test", vec![grant.grant_id]),
            Plan {
                plan_id: String::new(),
                op_id: "op.test".to_string(),
                steps: vec![Step {
                    kind: StepKind::Move,
                    src: Some(source.to_string_lossy().into_owned()),
                    dst: Some(destination.to_string_lossy().into_owned()),
                    note_key: None,
                    meta: JsonObject::new(),
                }],
                warnings: Vec::new(),
                meta: JsonObject::new(),
            },
        )
        .unwrap();
    authority.inject_post_effect_failure();

    let result = authority
        .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
        .unwrap();

    assert_eq!(result.meta["effectState"], "outcome_unknown");
    assert!(!source.exists());
    assert_eq!(fs::read(destination).unwrap(), b"PDF");
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.state, Some(JournalState::RecoveryRequired));
    assert_eq!(journal.steps.len(), 0);
    assert_eq!(journal.pending_step.unwrap().kind, StepKind::Move);
}

#[cfg(not(windows))]
#[test]
fn post_rename_sync_failures_keep_copy_write_and_move_pending_for_recovery() {
    for kind in [StepKind::Copy, StepKind::Write, StepKind::Move] {
        let fixture = tempfile::TempDir::new().unwrap();
        let root = fixture.path().join("documents");
        let source = root.join("source.txt");
        let destination = root.join("destination.txt");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"APPROVED BYTES").unwrap();
        let authority =
            FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
        let grant = authority
            .issue_grant(
                &GrantScope::new("owner", "session", "request", "op.test"),
                &root,
                GrantAccess::ReadWrite,
            )
            .unwrap();
        let mut meta = JsonObject::new();
        if kind == StepKind::Write {
            meta.insert(
                "content".to_string(),
                Value::String("APPROVED BYTES".to_string()),
            );
        }
        let cached = authority
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new("owner", "session", "op.test", vec![grant.grant_id]),
                Plan {
                    plan_id: String::new(),
                    op_id: "op.test".to_string(),
                    steps: vec![Step {
                        kind,
                        src: (kind != StepKind::Write)
                            .then(|| source.to_string_lossy().into_owned()),
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: None,
                        meta,
                    }],
                    warnings: Vec::new(),
                    meta: JsonObject::new(),
                },
            )
            .unwrap();
        crate::cap_fs::inject_directory_sync_failure_after(0);

        let result = authority
            .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
            .unwrap();

        assert_eq!(result.meta["effectState"], "outcome_unknown", "{kind:?}");
        assert_eq!(
            fs::read(&destination).unwrap(),
            b"APPROVED BYTES",
            "{kind:?}"
        );
        let journal = authority.journals().read(&result.journal_id).unwrap();
        assert_eq!(
            journal.state,
            Some(JournalState::RecoveryRequired),
            "{kind:?}"
        );
        assert_eq!(
            journal.pending_step.as_ref().map(|step| step.kind),
            Some(kind)
        );
    }
}

#[cfg(not(windows))]
#[test]
fn workbook_backup_post_rename_sync_failure_remains_outcome_unknown() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    let workbook = root.join("RFI Log.xlsm");
    let backup = root.join("RFI Log.backup.xlsm");
    fs::create_dir_all(&root).unwrap();
    fs::write(&workbook, b"APPROVED WORKBOOK PACKAGE").unwrap();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let grant = authority
        .issue_grant(
            &GrantScope::new("owner", "session", "request", "rfi.log_add"),
            &root,
            GrantAccess::ReadWrite,
        )
        .unwrap();
    let mut meta = JsonObject::new();
    meta.insert(
        "sourceSignature".to_string(),
        serde_json::json!({
            "size": fs::metadata(&workbook).unwrap().len(),
            "sha256": file_sha256(&workbook).unwrap()
        }),
    );
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner", "session", "rfi.log_add", vec![grant.grant_id]),
            Plan {
                plan_id: String::new(),
                op_id: "rfi.log_add".to_string(),
                steps: vec![Step {
                    kind: StepKind::WorkbookUpdate,
                    src: Some(workbook.to_string_lossy().into_owned()),
                    dst: Some(backup.to_string_lossy().into_owned()),
                    note_key: None,
                    meta,
                }],
                warnings: Vec::new(),
                meta: JsonObject::new(),
            },
        )
        .unwrap();
    crate::cap_fs::inject_directory_sync_failure_after(0);

    let result = authority
        .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
        .unwrap();

    assert_eq!(result.meta["effectState"], "outcome_unknown");
    assert_eq!(fs::read(&backup).unwrap(), b"APPROVED WORKBOOK PACKAGE");
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.state, Some(JournalState::RecoveryRequired));
    assert_eq!(
        journal.pending_step.as_ref().map(|step| step.kind),
        Some(StepKind::Copy)
    );
}

#[test]
fn byte_identical_replace_with_post_rename_sync_failure_is_still_outcome_unknown() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("replacement.xlsm");
    let destination = fixture.path().join("workbook.xlsm");
    fs::write(&source, b"IDENTICAL PACKAGE").unwrap();
    fs::write(&destination, b"IDENTICAL PACKAGE").unwrap();
    let capability = crate::cap_fs::CapabilityRoot::open(fixture.path()).unwrap();
    let source = capability.authorize(&source).unwrap().anchor().unwrap();
    let destination = capability
        .authorize(&destination)
        .unwrap()
        .anchor()
        .unwrap();
    crate::cap_fs::inject_directory_sync_failure_after(0);

    let error = destination
        .replace_with(&source)
        .expect_err("rename succeeded but directory sync was injected to fail");
    let classified = replacement_failure(&destination, error);

    assert!(matches!(classified, StepFailure::OutcomeUnknown(_)));
    assert_eq!(destination.sha256().unwrap(), {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(b"IDENTICAL PACKAGE"))
    });
    assert!(source.open_read().is_err());
}

#[cfg(unix)]
#[test]
fn nested_mkdirs_stay_on_the_pinned_parent_after_visible_parent_swap() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    let visible_parent = root.join("approved-parent");
    let pinned_parent = root.join("pinned-parent");
    let outside = fixture.path().join("outside");
    fs::create_dir_all(&visible_parent).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let destination = visible_parent.join("nested/deeper");
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let grant = authority
        .issue_grant(
            &GrantScope::new("owner", "session", "request", "op.test"),
            &root,
            GrantAccess::ReadWrite,
        )
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner", "session", "op.test", vec![grant.grant_id]),
            Plan {
                plan_id: String::new(),
                op_id: "op.test".to_string(),
                steps: vec![Step {
                    kind: StepKind::Mkdir,
                    src: None,
                    dst: Some(destination.to_string_lossy().into_owned()),
                    note_key: None,
                    meta: JsonObject::new(),
                }],
                warnings: Vec::new(),
                meta: JsonObject::new(),
            },
        )
        .unwrap();
    let outside_target = outside.clone();
    let swapped_parent = pinned_parent.clone();
    authority.inject_before_capability_effect(move || {
        fs::rename(&visible_parent, &swapped_parent).unwrap();
        symlink(&outside_target, &visible_parent).unwrap();
    });

    let result = authority
        .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
        .unwrap();

    assert_eq!((result.applied, result.failed), (1, 0));
    assert!(pinned_parent.join("nested/deeper").is_dir());
    assert!(!outside.join("nested").exists());
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(
        journal
            .steps
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![StepKind::Mkdir, StepKind::Mkdir]
    );
}

#[cfg(unix)]
#[test]
fn pinned_parent_capability_defeats_post_acquisition_path_swap() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    let visible_parent = root.join("approved-parent");
    let pinned_parent = root.join("pinned-parent");
    let outside = fixture.path().join("outside");
    fs::create_dir_all(&visible_parent).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let destination = visible_parent.join("created.txt");
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let grant = authority
        .issue_grant(
            &GrantScope::new("owner", "session", "request", "op.test"),
            &root,
            GrantAccess::ReadWrite,
        )
        .unwrap();
    let mut meta = JsonObject::new();
    meta.insert("content".to_string(), Value::String("PINNED".to_string()));
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner", "session", "op.test", vec![grant.grant_id]),
            Plan {
                plan_id: String::new(),
                op_id: "op.test".to_string(),
                steps: vec![Step {
                    kind: StepKind::Write,
                    src: None,
                    dst: Some(destination.to_string_lossy().into_owned()),
                    note_key: None,
                    meta,
                }],
                warnings: Vec::new(),
                meta: JsonObject::new(),
            },
        )
        .unwrap();
    let swapped_parent = pinned_parent.clone();
    let outside_target = outside.clone();
    authority.inject_before_capability_effect(move || {
        fs::rename(&visible_parent, &swapped_parent).unwrap();
        symlink(&outside_target, &visible_parent).unwrap();
    });

    let result = authority
        .apply(&cached.plan_id, &CallerIdentity::new("owner", "session"))
        .unwrap();

    assert!(!outside.join("created.txt").exists());
    if result.applied == 1 {
        assert_eq!(
            fs::read(pinned_parent.join("created.txt")).unwrap(),
            b"PINNED"
        );
    } else {
        assert_eq!(result.meta["effectState"], "no_effect_retryable");
    }
}

#[test]
fn file_effect_source_metadata_is_read_only_through_the_pinned_capability() {
    let source = include_str!("effects.rs");
    let function = source
        .split("fn execute_file_effect(")
        .nth(1)
        .and_then(|tail| tail.split("fn execute_workbook_update(").next())
        .expect("file effect implementation");

    assert!(!function.contains("file_sha256(source)"));
    assert!(function.contains("source_anchor"));
    assert!(function.contains(".sha256()"));
}

#[test]
fn verified_backup_rollback_atomically_restores_the_original_bytes() {
    let fixture = tempfile::TempDir::new().unwrap();
    let workbook = fixture.path().join("RFI Log.xlsm");
    let backup = fixture.path().join("backup.xlsm");
    fs::write(&workbook, b"changed package").unwrap();
    fs::write(&backup, b"original package").unwrap();
    let expected = file_sha256(&backup).unwrap();

    let capability = crate::cap_fs::CapabilityRoot::open(fixture.path()).unwrap();
    let backup = capability.authorize(&backup).unwrap().anchor().unwrap();
    let workbook = capability.authorize(&workbook).unwrap().anchor().unwrap();
    rollback_from_backup_capability(&backup, &workbook, &expected).unwrap();

    assert_eq!(workbook.sha256().unwrap(), expected);
    assert_eq!(backup.sha256().unwrap(), expected);
    assert!(fixture.path().read_dir().unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("restore-")
    }));
}

#[test]
fn verified_in_place_replacement_removes_the_staged_copy() {
    let fixture = tempfile::TempDir::new().unwrap();
    let target = fixture.path().join("target.pdf");
    let staged = fixture.path().join(".target.pdf.docflow-replica.tmp");
    fs::write(&target, b"old").unwrap();
    fs::write(&staged, b"new").unwrap();
    let expected = file_sha256(&target).unwrap();

    let capability = crate::cap_fs::CapabilityRoot::open(fixture.path()).unwrap();
    let target = capability.authorize(&target).unwrap().anchor().unwrap();
    let staged = capability.authorize(&staged).unwrap().anchor().unwrap();
    overwrite_existing_capability(&staged, &target, &expected).unwrap();

    assert_eq!(fs::read(target.absolute()).unwrap(), b"new");
    assert!(!staged.absolute().exists());
}

#[test]
fn generated_concrete_documents_apply_batch_binary_and_undo() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("output");
    fs::create_dir(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let destinations = [root.join("one.docx"), root.join("two.docx")];
    let payloads = [
        b"first OOXML \0\xff".to_vec(),
        b"second OOXML \xfe\xfd".to_vec(),
    ];
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let grant = authority
        .issue_grant(
            &GrantScope::new("owner", "session", "request", CONCRETE_DOCUMENT_OPERATION),
            &root,
            GrantAccess::Write,
        )
        .unwrap();
    let steps = destinations.iter().zip(payloads.iter()).map(|(destination, bytes)| Step {
        kind: StepKind::Write,
        src: None,
        dst: Some(destination.to_string_lossy().into_owned()),
        note_key: Some(CONCRETE_DOCUMENT_NOTE.to_string()),
        meta: serde_json::json!({
            "contentBase64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
        }).as_object().unwrap().clone(),
    }).collect();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner",
                "session",
                CONCRETE_DOCUMENT_OPERATION,
                vec![grant.grant_id],
            ),
            Plan {
                plan_id: String::new(),
                op_id: CONCRETE_DOCUMENT_OPERATION.to_string(),
                steps,
                warnings: Vec::new(),
                meta: JsonObject::new(),
            },
        )
        .unwrap();
    let caller = CallerIdentity::new("owner", "session");
    let result = authority.apply(&cached.plan_id, &caller).unwrap();
    assert_eq!(result.applied, 2);
    for (destination, expected) in destinations.iter().zip(payloads.iter()) {
        assert_eq!(&fs::read(destination).unwrap(), expected);
    }
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.steps.len(), 2);
    assert!(
        journal
            .steps
            .iter()
            .all(|step| !step.meta.contains_key("contentBase64"))
    );
    assert_eq!(
        authority
            .undo(&result.journal_id, &caller, None)
            .unwrap()
            .applied,
        2
    );
    assert!(destinations.iter().all(|destination| !destination.exists()));
}

#[test]
fn generated_base64_is_rejected_for_non_concrete_operations() {
    let fixture = tempfile::TempDir::new().unwrap();
    let root = fixture.path().join("output");
    fs::create_dir(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let operation = "rfi.log_add";
    let grant = authority
        .issue_grant(
            &GrantScope::new("owner", "session", "request", operation),
            &root,
            GrantAccess::Write,
        )
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner", "session", operation, vec![grant.grant_id]),
            Plan {
                plan_id: String::new(),
                op_id: operation.to_string(),
                steps: vec![Step {
                    kind: StepKind::Write,
                    src: None,
                    dst: Some(root.join("output.docx").to_string_lossy().into_owned()),
                    note_key: Some(CONCRETE_DOCUMENT_NOTE.to_string()),
                    meta: serde_json::json!({"contentBase64": "UEsDBA=="})
                        .as_object()
                        .unwrap()
                        .clone(),
                }],
                warnings: Vec::new(),
                meta: JsonObject::new(),
            },
        )
        .unwrap();
    let result = authority.apply(&cached.plan_id, &CallerIdentity::new("owner", "session"));
    assert!(result.is_err());
    assert!(!root.join("output.docx").exists());
}
