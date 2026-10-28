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
