use super::*;

#[test]
fn settings_locale_distinguishes_omission_from_explicit_null() {
    let omitted: SettingsUpdate = decode(json!({})).expect("omitted locale should decode");
    let cleared: SettingsUpdate =
        decode(json!({"locale": null})).expect("null locale should decode");

    assert_eq!(omitted.locale, PatchField::Missing);
    assert_eq!(cleared.locale, PatchField::Value(None));
}

#[test]
fn saved_operation_params_are_schema_filtered_and_never_include_secrets() {
    let filtered = filter_saved_operation_params(
        "rfi.log_add",
        serde_json::from_value(json!({
            "email_provider": "gemini",
            "gemini_api_key": "must-not-persist",
            "unknown": "must-not-persist"
        }))
        .unwrap(),
    )
    .expect("known operation should filter saved params");

    assert_eq!(filtered.get("email_provider"), Some(&json!("gemini")));
    assert!(!filtered.contains_key("gemini_api_key"));
    assert!(!filtered.contains_key("unknown"));

    let print = filter_saved_operation_params(
        "files.batch_print",
        serde_json::from_value(json!({
            "files": ["C:/stale/old-job.pdf"],
            "printer": "EPSON L15160 Series",
            "paper": "A3",
            "copies": 2
        }))
        .unwrap(),
    )
    .expect("print settings should exclude task-local file selections");
    assert!(!print.contains_key("files"));
    assert_eq!(print.get("printer"), Some(&json!("EPSON L15160 Series")));
    assert_eq!(print.get("paper"), Some(&json!("A3")));
    assert_eq!(print.get("copies"), Some(&json!(2)));
}

#[test]
fn settings_presets_commands_are_capability_checked_filtered_and_typed() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);

    let saved = backend
        .dispatch(
            BackendMethod::SettingsSavePreset,
            json!({
                "opId": "files.batch_print",
                "name": "Office",
                "values": {
                    "paper": "A4",
                    "copies": 2,
                    "collate": true,
                    "files": [{"selectionId": "forged"}],
                    "email_text": "must not persist",
                    "nested": {"bad": true}
                }
            }),
        )
        .unwrap();
    assert_eq!(saved["name"], "Office");
    assert_eq!(
        saved["values"],
        json!({
            "paper": "A4",
            "copies": 2,
            "collate": true
        })
    );

    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsSavePreset,
                json!({
                    "opId": "files.batch_print",
                    "name": "office",
                    "values": {"copies": 3}
                }),
            )
            .unwrap_err()
            .message,
        "error.preset.duplicate"
    );
    let replaced = backend
        .dispatch(
            BackendMethod::SettingsSavePreset,
            json!({
                "opId": "files.batch_print",
                "name": "office",
                "values": {"copies": 3},
                "replace": true
            }),
        )
        .unwrap();
    assert_eq!(replaced["id"], saved["id"]);
    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsPresets,
                json!({"opId": "files.batch_print"}),
            )
            .unwrap(),
        json!([replaced])
    );

    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsSavePreset,
                json!({
                    "opId": "files.batch_print",
                    "name": "Bad choice",
                    "values": {"paper": "Tabloid"}
                }),
            )
            .unwrap_err()
            .code,
        "validation"
    );
    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsPresets,
                json!({"opId": "rfi.process_scans"}),
            )
            .unwrap_err()
            .code,
        "validation"
    );

    backend
        .dispatch(
            BackendMethod::SettingsDeletePreset,
            json!({
                "opId": "files.batch_print",
                "presetId": saved["id"]
            }),
        )
        .unwrap();
    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsPresets,
                json!({"opId": "files.batch_print"}),
            )
            .unwrap(),
        json!([])
    );
}

#[test]
fn settings_save_preset_sanitizes_existing_stored_values_before_rewrite() {
    let temporary = tempfile::tempdir().unwrap();
    let settings_path = temporary.path().join("settings.json");
    let existing_id = uuid::Uuid::new_v4().to_string();
    std::fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "operationPresets": {
                "files.batch_print": [{
                    "id": existing_id.clone(),
                    "name": "Stored",
                    "values": {
                        "paper": "A4",
                        "copies": 2,
                        "collate": true,
                        "files": [{"selectionId": "forged"}],
                        "email_text": "must not persist",
                        "gemini_api_key": "must not persist",
                        "retries": "3",
                        "duplex": "sideways",
                        "unknownArray": [1, 2, 3],
                        "unknownObject": {"bad": true}
                    }
                }]
            },
            "custom": {"keep": true}
        }))
        .unwrap(),
    )
    .unwrap();
    let backend = test_backend(&temporary);

    backend
        .dispatch(
            BackendMethod::SettingsSavePreset,
            json!({
                "opId": "files.batch_print",
                "name": "New",
                "values": {"copies": 1}
            }),
        )
        .unwrap();

    let persisted: Value = serde_json::from_slice(&std::fs::read(settings_path).unwrap()).unwrap();
    let stored = persisted["operationPresets"]["files.batch_print"]
        .as_array()
        .unwrap()
        .iter()
        .find(|preset| preset["id"] == existing_id)
        .unwrap();
    assert_eq!(
        stored["values"],
        json!({
            "paper": "A4",
            "copies": 2,
            "collate": true
        })
    );
    assert_eq!(persisted["custom"], json!({"keep": true}));
}

#[test]
fn saved_operation_params_preserve_paths_and_reissue_active_selections_for_caller() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);

    let source_dir = temporary.path().join("source");
    fs::create_dir(&source_dir).unwrap();
    let dest_dir = temporary.path().join("dest");
    fs::create_dir(&dest_dir).unwrap();

    let params: Map<String, Value> = serde_json::from_value(json!({
        "scan_mode": "multi_rfi",
        "source": source_dir.to_str().unwrap(),
        "dest": dest_dir.to_str().unwrap(),
        "organize_into_rfi_folders": false,
        "keep_original": true,
        "secret_key": "forbidden",
        "unknown": "drop-me"
    }))
    .unwrap();

    let filtered = filter_saved_operation_params("rfi.process_scans_fast_covers", params)
        .expect("known operation should filter saved params");
    assert_eq!(
        filtered.get("source"),
        Some(&json!(source_dir.to_str().unwrap()))
    );
    assert_eq!(
        filtered.get("dest"),
        Some(&json!(dest_dir.to_str().unwrap()))
    );
    assert_eq!(filtered.get("keep_original"), Some(&json!(true)));
    assert_eq!(
        filtered.get("organize_into_rfi_folders"),
        Some(&json!(false))
    );
    assert!(!filtered.contains_key("secret_key"));
    assert!(!filtered.contains_key("unknown"));

    backend
        .settings
        .save_params("rfi.process_scans_fast_covers", filtered)
        .unwrap();

    let restored = backend
        .selected_params("rfi.process_scans_fast_covers")
        .unwrap();
    let source_selection: PathSelection =
        serde_json::from_value(restored.get("source").unwrap().clone()).unwrap();
    let dest_selection: PathSelection =
        serde_json::from_value(restored.get("dest").unwrap().clone()).unwrap();

    assert_eq!(
        source_selection.display_path,
        fs::canonicalize(&source_dir)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(
        dest_selection.display_path,
        fs::canonicalize(&dest_dir)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    );

    backend
        .settings
        .save_params(
            "rfi.process_scans_fast_covers",
            serde_json::from_value(json!({
                "source": "/non/existent/path/never/created",
                "dest": dest_dir.to_str().unwrap(),
                "keep_original": true
            }))
            .unwrap(),
        )
        .unwrap();
    let restored2 = backend
        .selected_params("rfi.process_scans_fast_covers")
        .unwrap();
    assert!(!restored2.contains_key("source"));
    assert!(restored2.contains_key("dest"));
    assert_eq!(restored2.get("keep_original"), Some(&json!(true)));
}

#[test]
fn follow_up_exception_preview_keeps_legacy_codes_and_never_saves_temporary_codes() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let legacy = serde_json::from_value(json!({
        "codes": ["AZE-SUN-INT-QMS-RFI-CIV-0346"]
    }))
    .unwrap();
    backend
        .settings
        .save_section("followUpExceptions", Some(legacy))
        .unwrap();
    let settings_path = temporary.path().join("settings.json");
    let before = std::fs::read(&settings_path).unwrap();

    let (exceptions, shared_available) = backend
        .resolve_follow_up_exceptions(Some("CIV-0267"))
        .unwrap();
    assert!(!shared_available);
    assert_eq!(exceptions.len(), 2);
    assert!(exceptions.contains("AZE-SUN-INT-QMS-RFI-CIV-0346"));
    assert!(exceptions.contains("AZE-SUN-INT-QMS-RFI-CIV-0267"));
    assert!(
        backend
            .resolve_follow_up_exceptions(Some("-CIV-0346"))
            .is_err()
    );
    assert!(
        !backend
            .selected_params("rfi.follow_up_sync")
            .unwrap()
            .contains_key("exception_rfis")
    );
    assert_eq!(std::fs::read(settings_path).unwrap(), before);
}

#[test]
fn follow_up_shared_edits_are_single_code_idempotent_and_leave_local_protections() {
    use std::collections::BTreeSet;

    type RemoteState = (BTreeSet<String>, Vec<(String, String)>);
    struct ExceptionRemote(Mutex<RemoteState>);

    impl crate::templates::TemplateRemote for ExceptionRemote {
        fn manifest(&self, _now: i64) -> Result<Value, BackendError> {
            Err(BackendError::offline())
        }

        fn download(&self, _path: &str, _now: i64) -> Result<Vec<u8>, BackendError> {
            Err(BackendError::offline())
        }

        fn rest(
            &self,
            method: &str,
            path: &str,
            body: Option<Value>,
            _now: i64,
        ) -> Result<Value, BackendError> {
            let mut state = self.0.lock().unwrap();
            let equals = path
                .split("code=eq.")
                .nth(1)
                .map(|s| s.split('&').next().unwrap());
            match method {
                "GET" => {
                    let after = path.split("code=gt.").nth(1);
                    Ok(json!(
                        state
                            .0
                            .iter()
                            .filter(|code| {
                                equals.is_none_or(|selected| code.as_str() == selected)
                                    && after.is_none_or(|last| code.as_str() > last)
                            })
                            .map(|code| json!({"code": code}))
                            .collect::<Vec<_>>()
                    ))
                }
                "POST" => {
                    assert_eq!(path, "follow_up_exceptions");
                    let body = body.unwrap();
                    let code = body["code"].as_str().unwrap();
                    state.0.insert(code.to_string());
                    state.1.push(("POST".to_string(), code.to_string()));
                    // Server committed the row, but the response was lost.
                    Err(BackendError::network_timeout())
                }
                "DELETE" => {
                    let code = equals.unwrap();
                    state.0.remove(code);
                    state.1.push(("DELETE".to_string(), code.to_string()));
                    Ok(Value::Null)
                }
                _ => panic!("unexpected mutation method"),
            }
        }
    }

    let temporary = tempfile::tempdir().unwrap();
    let mut backend = test_backend(&temporary);
    let retained = "AZE-SUN-INT-QMS-RFI-ELE-0099".to_string();
    let selected = "AZE-SUN-INT-QMS-RFI-CIV-0267".to_string();
    let remote = Arc::new(ExceptionRemote(Mutex::new((
        BTreeSet::from([retained.clone()]),
        Vec::new(),
    ))));
    backend.templates = Arc::new(crate::templates::TemplateStore::new(
        temporary.path().join("mock-templates"),
        remote.clone(),
    ));
    backend
        .settings
        .save_section(
            "followUpExceptions",
            Some(
                serde_json::from_value(
                    json!({"codes": ["MEC-0346", "invalid legacy"], "keep": true}),
                )
                .unwrap(),
            ),
        )
        .unwrap();

    let (codes, shared_available) = backend
        .resolve_follow_up_exceptions(Some("PRC-0004"))
        .unwrap();
    assert!(shared_available);
    assert_eq!(
        codes,
        BTreeSet::from([
            retained.clone(),
            "AZE-SUN-INT-QMS-RFI-MEC-0346".to_string(),
            "AZE-SUN-INT-QMS-RFI-PRC-0004".to_string(),
        ])
    );
    assert!(remote.0.lock().unwrap().1.is_empty());

    for change in ["addShared", "addShared", "removeShared", "removeShared"] {
        backend
            .dispatch(
                BackendMethod::SettingsChangeFollowUpException,
                json!({"code": "CIV-0267", "change": change}),
            )
            .unwrap();
    }
    {
        let state = remote.0.lock().unwrap();
        assert_eq!(state.0, BTreeSet::from([retained]));
        assert_eq!(
            state.1,
            [
                ("POST".to_string(), selected.clone()),
                ("DELETE".to_string(), selected),
            ]
        );
    }
    let listed = backend
        .dispatch(BackendMethod::SettingsFollowUpExceptions, json!({}))
        .unwrap();
    assert_eq!(
        listed,
        json!({"shared": ["ELE-0099"], "local": ["MEC-0346"]})
    );

    backend
        .dispatch(
            BackendMethod::SettingsChangeFollowUpException,
            json!({"code": "MEC-0346", "change": "removeLocal"}),
        )
        .unwrap();
    let section = backend.settings.section("followUpExceptions").unwrap();
    assert_eq!(section["codes"], json!(["invalid legacy"]));
    assert_eq!(section["keep"], json!(true));
}
