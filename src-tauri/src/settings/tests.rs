use std::fs;

use serde_json::json;

use super::*;

fn operation(id: &str) -> OperationDef {
    crate::registry::operation_registry(crate::registry::RegistryFacts {
        printers: vec!["Office Printer".to_string()],
        default_printer: Some("Office Printer".to_string()),
        ..crate::registry::RegistryFacts::default()
    })
    .into_iter()
    .find(|operation| operation.id == id)
    .expect("test operation should exist")
}

#[test]
fn operation_presets_round_trip_without_losing_unknown_settings() {
    let directory = tempfile::tempdir().expect("temporary settings directory");
    let path = directory.path().join("settings.json");
    fs::write(&path, br#"{"custom":{"keep":true}}"#).unwrap();
    let store = SettingsStore::new(path.clone()).unwrap();

    let saved = store
        .save_preset(
            &operation("files.batch_print"),
            "  A4 duplex  ",
            serde_json::from_value(json!({
                "printer": "Office Printer",
                "paper": "A4",
                "duplex": "long",
                "copies": 2
            }))
            .unwrap(),
            false,
        )
        .unwrap();

    assert_eq!(saved.name, "A4 duplex");
    assert!(uuid::Uuid::parse_str(&saved.id).is_ok());
    assert_eq!(
        store.presets_for(&operation("files.batch_print")).unwrap(),
        vec![saved.clone()]
    );
    assert_eq!(
        SettingsStore::new(path.clone())
            .unwrap()
            .presets_for(&operation("files.batch_print"))
            .unwrap(),
        vec![saved]
    );
    let persisted: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(persisted["custom"], json!({"keep": true}));
}

#[test]
fn operation_presets_reject_duplicates_and_replace_preserves_id() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().join("settings.json")).unwrap();
    let first = store
        .save_preset(
            &operation("files.batch_print"),
            "Office",
            serde_json::from_value(json!({"copies": 1})).unwrap(),
            false,
        )
        .unwrap();

    assert!(matches!(
        store.save_preset(
            &operation("files.batch_print"),
            "office",
            serde_json::from_value(json!({"copies": 2})).unwrap(),
            false,
        ),
        Err(PresetStoreError::DuplicateName)
    ));

    let replaced = store
        .save_preset(
            &operation("files.batch_print"),
            "office",
            serde_json::from_value(json!({"copies": 2})).unwrap(),
            true,
        )
        .unwrap();
    assert_eq!(replaced.id, first.id);
    assert_eq!(replaced.values["copies"], 2);
    assert_eq!(
        store.presets_for(&operation("files.batch_print")).unwrap(),
        vec![replaced]
    );
}

#[test]
fn operation_presets_use_unicode_default_case_folding_for_duplicates() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().join("settings.json")).unwrap();
    let batch_print = operation("files.batch_print");
    store
        .save_preset(
            &batch_print,
            "Straße",
            serde_json::from_value(json!({"copies": 1})).unwrap(),
            false,
        )
        .unwrap();

    assert!(matches!(
        store.save_preset(
            &batch_print,
            "STRASSE",
            serde_json::from_value(json!({"copies": 2})).unwrap(),
            false,
        ),
        Err(PresetStoreError::DuplicateName)
    ));
}

#[test]
fn operation_presets_enforce_name_and_capacity_limits() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().join("settings.json")).unwrap();
    let values = || serde_json::from_value(json!({"copies": 1})).unwrap();

    for invalid in ["   ".to_string(), "x".repeat(65)] {
        assert!(matches!(
            store.save_preset(&operation("files.batch_print"), &invalid, values(), false),
            Err(PresetStoreError::InvalidName)
        ));
    }
    store
        .save_preset(
            &operation("files.batch_print"),
            &"🖨".repeat(64),
            values(),
            false,
        )
        .unwrap();
    for index in 1..20 {
        store
            .save_preset(
                &operation("files.batch_print"),
                &format!("Preset {index:02}"),
                values(),
                false,
            )
            .unwrap();
    }
    assert!(matches!(
        store.save_preset(&operation("files.batch_print"), "Overflow", values(), false,),
        Err(PresetStoreError::LimitReached)
    ));
}

#[test]
fn operation_presets_list_deterministically_and_delete_exact_ids() {
    let directory = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(directory.path().join("settings.json")).unwrap();
    let values = || serde_json::from_value(json!({"copies": 1})).unwrap();
    let zulu = store
        .save_preset(&operation("files.batch_print"), "zulu", values(), false)
        .unwrap();
    let alpha = store
        .save_preset(&operation("files.batch_print"), "Alpha", values(), false)
        .unwrap();
    store
        .save_preset(&operation("files.batch_print"), "bravo", values(), false)
        .unwrap();

    assert_eq!(
        store
            .presets_for(&operation("files.batch_print"))
            .unwrap()
            .iter()
            .map(|preset| preset.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Alpha", "bravo", "zulu"]
    );
    assert!(matches!(
        store.delete_preset(&operation("rfi.process_scans_fast_covers"), &alpha.id),
        Err(PresetStoreError::Missing)
    ));
    assert!(matches!(
        store.delete_preset(&operation("files.batch_print"), "stale-id"),
        Err(PresetStoreError::Missing)
    ));
    store
        .delete_preset(&operation("files.batch_print"), &zulu.id)
        .unwrap();
    assert_eq!(
        store
            .presets_for(&operation("files.batch_print"))
            .unwrap()
            .iter()
            .map(|preset| preset.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Alpha", "bravo"]
    );
}

#[test]
fn operation_presets_drop_only_malformed_stored_entries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let valid_id = uuid::Uuid::new_v4().to_string();
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "operationPresets": {
                "files.batch_print": [
                    {
                        "id": valid_id,
                        "name": "Valid",
                        "values": {
                            "copies": 2,
                            "files": {"selectionId": "forbidden"},
                            "orientation": "diagonal",
                            "printer": "bad\nprinter"
                        }
                    },
                    {"id": uuid::Uuid::new_v4().to_string(), "name": "Missing values"},
                    {"id": "not-a-uuid", "name": "Bad ID", "values": {}},
                    {"id": uuid::Uuid::new_v4().to_string(), "name": "", "values": {}},
                    42
                ]
            },
            "custom": {"keep": true}
        }))
        .unwrap(),
    )
    .unwrap();

    let presets = SettingsStore::new(path.clone())
        .unwrap()
        .presets_for(&operation("files.batch_print"))
        .unwrap();
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].id, valid_id);
    assert_eq!(presets[0].name, "Valid");
    assert_eq!(
        presets[0].values,
        json!({"copies": 2}).as_object().unwrap().clone()
    );

    let persisted: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        persisted["operationPresets"]["files.batch_print"],
        json!([{"id": valid_id, "name": "Valid", "values": {"copies": 2}}])
    );
    assert_eq!(persisted["custom"], json!({"keep": true}));
}

#[test]
fn operation_presets_scrub_write_failure_preserves_memory_and_disk() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let original = serde_json::to_vec(&json!({
        "operationPresets": {
            "files.batch_print": [{
                "id": uuid::Uuid::new_v4().to_string(),
                "name": "Unsafe",
                "values": {"copies": 2, "files": {"selectionId": "forbidden"}}
            }]
        },
        "custom": {"keep": true}
    }))
    .unwrap();
    fs::write(&path, &original).unwrap();
    let store = SettingsStore::try_new_with_writer(path.clone(), |_path, _data| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "injected scrub write failure",
        ))
    })
    .unwrap();

    assert!(matches!(
        store.presets_for(&operation("files.batch_print")),
        Err(PresetStoreError::Write(error))
            if error.kind() == io::ErrorKind::PermissionDenied
    ));
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(
        store.section("operationPresets").unwrap()["files.batch_print"][0]["values"]["files"],
        json!({"selectionId": "forbidden"})
    );
}

#[test]
fn operation_presets_preserve_a_temporarily_unavailable_printer() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let preset_id = uuid::Uuid::new_v4().to_string();
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "operationPresets": {
                "files.batch_print": [{
                    "id": preset_id,
                    "name": "Offline queue",
                    "values": {
                        "printer": "Temporarily Disconnected Printer",
                        "orientation": "landscape"
                    }
                }]
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let presets = SettingsStore::new(path.clone())
        .unwrap()
        .presets_for(&operation("files.batch_print"))
        .unwrap();

    assert_eq!(
        presets[0].values["printer"],
        "Temporarily Disconnected Printer"
    );
    let persisted: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        persisted["operationPresets"]["files.batch_print"][0]["values"]["printer"],
        "Temporarily Disconnected Printer"
    );
}

#[test]
fn operation_presets_failed_write_rolls_back_memory_and_disk() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let original = br#"{"custom":{"keep":true}}"#.to_vec();
    fs::write(&path, &original).unwrap();
    let store = SettingsStore::try_new_with_writer(path.clone(), |_path, _data| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "injected atomic write failure",
        ))
    })
    .unwrap();

    assert!(matches!(
        store.save_preset(
            &operation("files.batch_print"),
            "Office",
            serde_json::from_value(json!({"copies": 2})).unwrap(),
            false,
        ),
        Err(PresetStoreError::Write(error))
            if error.kind() == io::ErrorKind::PermissionDenied
    ));
    assert!(
        store
            .presets_for(&operation("files.batch_print"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn atomic_write_accepts_verified_parent_sync_failure_after_persist() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    fs::write(&path, br#"{"before":true}"#).unwrap();
    let next: JsonObject = serde_json::from_value(json!({
        "after": true,
        "nested": {"value": 2}
    }))
    .unwrap();
    let serialized = serde_json::to_vec(&next).unwrap();

    let result = atomic_write_with_parent_sync(&path, &next, |_parent| {
        Err(io::Error::other("injected parent sync failure"))
    });

    assert!(result.is_ok());
    assert_eq!(fs::read(path).unwrap(), serialized);
}

#[test]
fn atomic_write_fails_closed_when_parent_sync_failure_readback_mismatches() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    fs::write(&path, br#"{"before":true}"#).unwrap();
    let next: JsonObject = serde_json::from_value(json!({"after": true})).unwrap();
    let serialized = serde_json::to_vec(&next).unwrap();

    let result = atomic_write_with_parent_sync(&path, &next, |_parent| {
        fs::write(&path, br#"{"unexpected":true}"#)?;
        Err(io::Error::other("injected parent sync failure"))
    });

    assert!(result.is_err());
    assert_ne!(fs::read(path).unwrap(), serialized);
}

#[test]
fn settings_round_trip_is_atomic_compatible_and_filters_operation_params() {
    let directory = tempfile::tempdir().expect("temporary settings directory");
    let path = directory.path().join("settings.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "prefs": {"locale": "az", "fontScale": 1.25},
            "session": {"accessToken": "legacy-access", "refreshToken": "legacy-refresh"},
            "custom": [1, 2, 3],
            "params": {"other.operation": {"keep": "value"}}
        }))
        .expect("legacy settings should encode"),
    )
    .expect("legacy settings should be written");

    let store = SettingsStore::new(path.clone()).unwrap();
    store
        .save_params(
            "rfi.process_scans_fast_covers",
            serde_json::from_value(json!({
                "scan_mode": "multi_rfi",
                "source": "/in",
                "dest": "/out",
                "keep_original": true,
                "nullable": null,
                "unsafe_list": [1, 2, 3],
                "unsafe_object": {"path": "/forged"}
            }))
            .expect("parameters should be an object"),
        )
        .expect("settings save should succeed");

    let restored = SettingsStore::new(path.clone()).unwrap();
    assert_eq!(
        restored.params_for("rfi.process_scans_fast_covers"),
        serde_json::from_value(json!({
            "scan_mode": "multi_rfi",
            "source": "/in",
            "dest": "/out",
            "keep_original": true,
            "nullable": null
        }))
        .expect("expected parameters should be an object")
    );
    assert_eq!(
        restored.params_for("other.operation"),
        serde_json::from_value(json!({"keep": "value"}))
            .expect("expected parameters should be an object")
    );

    let encoded = fs::read_to_string(&path).expect("settings should be readable");
    let persisted: serde_json::Value =
        serde_json::from_str(&encoded).expect("settings should remain valid JSON");
    assert_eq!(persisted["prefs"]["locale"], "az");
    assert_eq!(persisted["session"]["accessToken"], "legacy-access");
    assert_eq!(persisted["custom"], json!([1, 2, 3]));
    assert!(!encoded.contains("unsafe_list"));
    assert!(!encoded.contains("unsafe_object"));
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("directory listing")
            .count(),
        1
    );
}

#[test]
fn preferences_validate_clamp_round_trip_and_reset_compatibly() {
    let directory = tempfile::tempdir().expect("temporary settings directory");
    let path = directory.path().join("settings.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "prefs": {
                "locale": 42,
                "theme": "neon",
                "font": "comic",
                "fontScale": "large",
                "compact": "yes"
            }
        }))
        .expect("legacy settings should encode"),
    )
    .expect("legacy settings should be written");
    let store = SettingsStore::new(path.clone()).unwrap();
    assert_eq!(store.preferences(), AppPrefs::default());

    store
        .save_preferences(AppPrefs {
            locale: Some("az".to_string()),
            theme: "dark".to_string(),
            font: "mono".to_string(),
            font_scale: 9.0,
            compact: true,
            start_with_windows: true,
            replica_sync_enabled: true,
        })
        .expect("preferences should save");
    assert_eq!(
        SettingsStore::new(path.clone()).unwrap().preferences(),
        AppPrefs {
            locale: Some("az".to_string()),
            theme: "dark".to_string(),
            font: "mono".to_string(),
            font_scale: 1.6,
            compact: true,
            start_with_windows: true,
            replica_sync_enabled: true,
        }
    );

    store
        .save_preferences(AppPrefs::default())
        .expect("default preferences should save");
    let reset: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("reset settings should be readable"))
            .expect("reset settings should be valid JSON");
    assert_eq!(reset["prefs"]["locale"], serde_json::Value::Null);
}

#[test]
fn legacy_operation_secrets_are_scrubbed_from_memory_and_disk_on_load() {
    let directory = tempfile::tempdir().expect("temporary settings directory");
    let path = directory.path().join("settings.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "params": {
                "rfi.log_add": {
                    "email_provider": "gemini",
                    "gemini_api_key": "legacy-secret"
                }
            },
            "custom": {"keep": true}
        }))
        .unwrap(),
    )
    .unwrap();

    let store = SettingsStore::new(path.clone()).unwrap();
    assert_eq!(
        store.params_for("rfi.log_add"),
        serde_json::from_value(json!({"email_provider": "gemini"})).unwrap()
    );
    let encoded = fs::read_to_string(path).unwrap();
    assert!(!encoded.contains("gemini_api_key"));
    assert!(!encoded.contains("legacy-secret"));
    assert!(encoded.contains("\"custom\""));
}

#[test]
fn startup_surfaces_injected_legacy_secret_scrub_write_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let original = serde_json::to_vec(&json!({
        "params": {
            "rfi.log_add": {
                "email_provider": "gemini",
                "gemini_api_key": "legacy-plaintext-secret"
            }
        },
        "custom": {"keep": true}
    }))
    .unwrap();
    fs::write(&path, &original).unwrap();

    let result = SettingsStore::try_new_with_writer(path.clone(), |_path, _data| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "injected atomic write failure",
        ))
    });

    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(
        fs::read_to_string(path)
            .unwrap()
            .contains("legacy-plaintext-secret")
    );
}
