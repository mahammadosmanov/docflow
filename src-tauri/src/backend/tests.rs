use super::*;

fn auth_state(signed_in: bool, allowed: bool) -> crate::contracts::AuthState {
    crate::contracts::AuthState {
        signed_in,
        allowed,
        is_admin: false,
        session: signed_in.then(|| crate::contracts::AuthSessionView {
            user_id: "user-1".to_string(),
            email: "user@example.com".to_string(),
            expires_at: i64::MAX,
        }),
        user: None,
    }
}

#[test]
fn direct_ipc_requires_an_allowed_authenticated_caller_for_every_protected_method() {
    let signed_out = auth_state(false, false);
    let pending = auth_state(true, false);
    let allowed = auth_state(true, true);
    let public = [
        BackendMethod::AppBootstrap,
        BackendMethod::SettingsUpdate,
        BackendMethod::SettingsReset,
        BackendMethod::SettingsReplicaStatus,
        BackendMethod::SupportDiagnostics,
        BackendMethod::SupportCopyDiagnostics,
        BackendMethod::SupportActivityLog,
        BackendMethod::AuthSignIn,
        BackendMethod::AuthSignUp,
        BackendMethod::AuthSignOut,
        BackendMethod::AuthUsernameAvailable,
        BackendMethod::AuthLoadMe,
        BackendMethod::AuthSetUsername,
        BackendMethod::AuthListUsers,
        BackendMethod::AuthSetFlags,
    ];

    for method in BackendMethod::ALL {
        if public.contains(&method) {
            assert!(method.authorize(&signed_out).is_ok(), "{}", method.as_str());
            continue;
        }
        assert_eq!(
            method.authorize(&signed_out).unwrap_err().code,
            "auth_required",
            "{}",
            method.as_str()
        );
        assert_eq!(
            method.authorize(&pending).unwrap_err().code,
            "permission_denied",
            "{}",
            method.as_str()
        );
        assert!(method.authorize(&allowed).is_ok(), "{}", method.as_str());
    }
}

#[test]
fn frontend_method_allowlist_is_exact_and_rejects_internal_or_legacy_methods() {
    for method in BackendMethod::ALL {
        let name = method.as_str();
        let parsed: BackendMethod = serde_json::from_value(serde_json::json!(name))
            .unwrap_or_else(|error| panic!("{name} should be allowed: {error}"));
        assert_eq!(parsed, method);
    }
    for rejected in [
        "shell/exec",
        "system/ping",
        "system/diagnostics",
        "auth/state",
        "settings/get",
        "engine/listOperations",
        "engine/previewTable",
        "internal/rfiLogGeminiContext",
        "support/readiness",
        "workItems/list",
    ] {
        assert!(
            serde_json::from_value::<BackendMethod>(serde_json::json!(rejected)).is_err(),
            "{rejected} must not be React-reachable"
        );
    }
}

#[test]
fn support_diagnostics_preview_and_copy_are_closed_public_rust_methods() {
    let signed_out = auth_state(false, false);
    for name in ["support/diagnostics", "support/copyDiagnostics"] {
        let method: BackendMethod = serde_json::from_value(serde_json::json!(name))
            .unwrap_or_else(|error| panic!("{name} should be allowed: {error}"));
        assert_eq!(method.owner(), MethodOwner::Rust);
        assert!(method.authorize(&signed_out).is_ok());
    }
    for rejected in [
        "support/diagnostics/upload",
        "support/diagnostics/scan",
        "support/sendDiagnostics",
    ] {
        assert!(
            serde_json::from_value::<BackendMethod>(serde_json::json!(rejected)).is_err(),
            "{rejected} must not be React-reachable"
        );
    }
    assert!(
        serde_json::from_value::<CopyDiagnosticsParams>(serde_json::json!({
            "content": "{}\n"
        }))
        .is_ok()
    );
    assert!(
        serde_json::from_value::<CopyDiagnosticsParams>(serde_json::json!({
            "content": "{}\n",
            "destination": "/tmp/forged"
        }))
        .is_err()
    );
}

#[test]
fn slice_nine_react_surface_is_entirely_rust_owned() {
    for method in BackendMethod::ALL {
        assert_eq!(method.owner(), MethodOwner::Rust, "{}", method.as_str());
    }
}

#[test]
fn rust_and_typescript_frontend_method_rosters_match_exactly() {
    let commands = include_str!("../../../frontend/src/lib/commands.ts");
    let roster = commands
        .split("// BEGIN RUST-VERIFIED FRONTEND METHODS")
        .nth(1)
        .and_then(|tail| tail.split("// END RUST-VERIFIED FRONTEND METHODS").next())
        .expect("TypeScript method roster markers must exist");
    let mut typescript = roster
        .lines()
        .filter_map(|line| line.trim().strip_prefix('"'))
        .filter_map(|line| line.split('"').next())
        .collect::<Vec<_>>();
    let mut rust = BackendMethod::ALL
        .into_iter()
        .map(BackendMethod::as_str)
        .collect::<Vec<_>>();
    typescript.sort_unstable();
    rust.sort_unstable();
    assert_eq!(rust, typescript);
}

#[test]
fn slice_nine_bridge_has_no_generic_string_or_fallback_command() {
    let bridge = include_str!("../../../frontend/src/lib/bridge.ts");
    assert!(!bridge.contains("method: string"));
    assert!(!bridge.contains("backend_call"));
    assert!(bridge.contains("frontend_call"));
}

#[test]
fn rfi_log_workbook_picker_offers_macro_enabled_and_standard_excel_files() {
    assert_eq!(
        path_dialog_filter("rfi.log_add", "workbook"),
        Some(("Excel (.xlsm, .xlsx)", &["xlsm", "xlsx"][..]))
    );
    assert_eq!(
        path_dialog_filter("rfi.follow_up_sync", "rfi_log_workbook"),
        Some(("Excel (.xlsm, .xlsx)", &["xlsm", "xlsx"][..]))
    );
    assert_eq!(
        path_dialog_filter("rfi.system_sync", "rfi_log_workbook"),
        Some(("Excel (.xlsm, .xlsx)", &["xlsm", "xlsx"][..]))
    );
    assert_eq!(
        path_dialog_filter("rfi.system_sync", "follow_up_workbook"),
        Some(("Excel (.xlsm, .xlsx)", &["xlsm", "xlsx"][..]))
    );
    assert_eq!(
        path_dialog_filter("rfi.complete_status_sync", "rfi_log_workbook"),
        Some(("Excel (.xlsm, .xlsx)", &["xlsm", "xlsx"][..]))
    );
    assert_eq!(
        path_dialog_filter("rfi.publish_status", "rfi_log_workbook"),
        Some(("Excel (.xlsm, .xlsx)", &["xlsm", "xlsx"][..]))
    );
    assert_eq!(path_dialog_filter("rfi.process_scans", "source"), None);
}

#[test]
fn final_router_contains_no_fallback_or_process_runtime() {
    let backend = include_str!("../backend.rs");
    let production = backend
        .split("#[cfg(test)]")
        .next()
        .expect("backend production source must remain inspectable");
    let app = include_str!("../lib.rs");

    for removed in [
        "FallbackSupervisor",
        "CommandChild",
        "CommandEvent",
        "tauri_plugin_shell",
        "ensure_running",
    ] {
        assert!(!production.contains(removed), "removed runtime: {removed}");
        assert!(
            !app.contains(removed),
            "removed application plugin: {removed}"
        );
    }
    assert!(app.contains("generate_handler![backend::frontend_call]"));
}

#[test]
fn final_build_manifests_reject_fallback_configuration() {
    let cargo = include_str!("../../Cargo.toml");
    let tauri = include_str!("../../tauri.conf.json");
    let package = include_str!("../../../package.json");

    for removed in ["tauri-plugin-shell", "externalBin", "docflow-sidecar"] {
        assert!(!cargo.contains(removed), "Cargo manifest: {removed}");
        assert!(!tauri.contains(removed), "Tauri manifest: {removed}");
        assert!(!package.contains(removed), "npm manifest: {removed}");
    }
}

#[test]
fn apply_envelope_rejects_client_authored_executable_fields() {
    let error = validate_apply_envelope(&serde_json::json!({
        "planId": "opaque-plan",
        "opId": "rfi.process_scans",
        "steps": [{"kind": "move", "src": "/forged", "dst": "/victim"}]
    }))
    .expect_err("apply accepts identities, never client-authored actions");

    assert_eq!(error.code, "backend_invalid_params");
    assert!(
        validate_apply_envelope(&serde_json::json!({
            "planId": "opaque-plan",
            "opId": "rfi.process_scans"
        }))
        .is_ok()
    );
}
