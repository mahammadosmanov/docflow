use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, ipc::Channel};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::{
    contracts::{AuthState, ProgressEvent},
    errors::BackendError,
    native::{NativeBackend, PathSelectionMode},
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub(crate) enum BackendMethod {
    #[serde(rename = "app/bootstrap")]
    AppBootstrap,
    #[serde(rename = "settings/update")]
    SettingsUpdate,
    #[serde(rename = "settings/reset")]
    SettingsReset,
    #[serde(rename = "settings/params")]
    SettingsParams,
    #[serde(rename = "settings/saveParams")]
    SettingsSaveParams,
    #[serde(rename = "auth/signIn")]
    AuthSignIn,
    #[serde(rename = "auth/signUp")]
    AuthSignUp,
    #[serde(rename = "auth/signOut")]
    AuthSignOut,
    #[serde(rename = "auth/usernameAvailable")]
    AuthUsernameAvailable,
    #[serde(rename = "auth/loadMe")]
    AuthLoadMe,
    #[serde(rename = "auth/setUsername")]
    AuthSetUsername,
    #[serde(rename = "auth/listUsers")]
    AuthListUsers,
    #[serde(rename = "auth/setFlags")]
    AuthSetFlags,
    #[serde(rename = "engine/preview")]
    EnginePreview,
    #[serde(rename = "engine/apply")]
    EngineApply,
    #[serde(rename = "engine/cancel")]
    EngineCancel,
    #[serde(rename = "engine/printQueueLatest")]
    EnginePrintQueueLatest,
    #[serde(rename = "engine/printQueueDiscard")]
    EnginePrintQueueDiscard,
    #[serde(rename = "engine/printQueueResume")]
    EnginePrintQueueResume,
    #[serde(rename = "engine/revise")]
    EngineRevise,
    #[serde(rename = "engine/copyTable")]
    EngineCopyTable,
    #[serde(rename = "engine/reviewScan")]
    EngineReviewScan,
    #[serde(rename = "engine/reviewPreview")]
    EngineReviewPreview,
    #[serde(rename = "engine/undo")]
    EngineUndo,
    #[serde(rename = "history/list")]
    HistoryList,
    #[serde(rename = "paths/select")]
    PathsSelect,
    #[serde(rename = "paths/reveal")]
    PathsReveal,
    #[serde(rename = "recovery/list")]
    RecoveryList,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MethodOwner {
    Rust,
}

impl BackendMethod {
    pub(crate) const ALL: [Self; 28] = [
        Self::AppBootstrap,
        Self::AuthUsernameAvailable,
        Self::AuthSignIn,
        Self::AuthSignUp,
        Self::AuthSignOut,
        Self::AuthLoadMe,
        Self::AuthSetUsername,
        Self::AuthListUsers,
        Self::AuthSetFlags,
        Self::SettingsUpdate,
        Self::SettingsReset,
        Self::SettingsParams,
        Self::SettingsSaveParams,
        Self::EnginePreview,
        Self::EngineApply,
        Self::EngineCancel,
        Self::EnginePrintQueueLatest,
        Self::EnginePrintQueueDiscard,
        Self::EnginePrintQueueResume,
        Self::EngineRevise,
        Self::EngineCopyTable,
        Self::EngineReviewScan,
        Self::EngineReviewPreview,
        Self::EngineUndo,
        Self::HistoryList,
        Self::PathsSelect,
        Self::PathsReveal,
        Self::RecoveryList,
    ];

    pub(crate) const fn owner(self) -> MethodOwner {
        MethodOwner::Rust
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AppBootstrap => "app/bootstrap",
            Self::SettingsUpdate => "settings/update",
            Self::SettingsReset => "settings/reset",
            Self::SettingsParams => "settings/params",
            Self::SettingsSaveParams => "settings/saveParams",
            Self::AuthSignIn => "auth/signIn",
            Self::AuthSignUp => "auth/signUp",
            Self::AuthSignOut => "auth/signOut",
            Self::AuthUsernameAvailable => "auth/usernameAvailable",
            Self::AuthLoadMe => "auth/loadMe",
            Self::AuthSetUsername => "auth/setUsername",
            Self::AuthListUsers => "auth/listUsers",
            Self::AuthSetFlags => "auth/setFlags",
            Self::EnginePreview => "engine/preview",
            Self::EngineApply => "engine/apply",
            Self::EngineCancel => "engine/cancel",
            Self::EnginePrintQueueLatest => "engine/printQueueLatest",
            Self::EnginePrintQueueDiscard => "engine/printQueueDiscard",
            Self::EnginePrintQueueResume => "engine/printQueueResume",
            Self::EngineRevise => "engine/revise",
            Self::EngineCopyTable => "engine/copyTable",
            Self::EngineReviewScan => "engine/reviewScan",
            Self::EngineReviewPreview => "engine/reviewPreview",
            Self::EngineUndo => "engine/undo",
            Self::HistoryList => "history/list",
            Self::PathsSelect => "paths/select",
            Self::PathsReveal => "paths/reveal",
            Self::RecoveryList => "recovery/list",
        }
    }

    fn authorize(self, state: &AuthState) -> Result<(), BackendError> {
        if matches!(
            self,
            Self::AppBootstrap
                | Self::SettingsUpdate
                | Self::SettingsReset
                | Self::AuthSignIn
                | Self::AuthSignUp
                | Self::AuthSignOut
                | Self::AuthUsernameAvailable
                | Self::AuthLoadMe
                | Self::AuthSetUsername
                | Self::AuthListUsers
                | Self::AuthSetFlags
        ) {
            return Ok(());
        }
        if !state.signed_in {
            return Err(BackendError::auth_required());
        }
        if !state.allowed {
            return Err(BackendError::permission_denied());
        }
        Ok(())
    }
}

pub(crate) struct BackendRouter {
    app: AppHandle,
    native: Arc<NativeBackend>,
}

impl BackendRouter {
    pub(crate) fn new(app: AppHandle) -> Result<Self, BackendError> {
        Ok(Self {
            app,
            native: Arc::new(NativeBackend::production()?),
        })
    }

    pub(crate) async fn call(
        &self,
        method: BackendMethod,
        params: Value,
        progress: Channel<ProgressEvent>,
    ) -> Result<Value, BackendError> {
        method.authorize(&self.native.auth_state())?;
        match method {
            BackendMethod::EnginePreview => self.preview(params, progress).await,
            BackendMethod::EngineApply => self.apply(params, progress).await,
            BackendMethod::EngineCancel => self.cancel(params).await,
            BackendMethod::EngineRevise | BackendMethod::EngineCopyTable => {
                let native = Arc::clone(&self.native);
                tauri::async_runtime::spawn_blocking(move || match method {
                    BackendMethod::EngineRevise => native.revise_native_plan(params),
                    BackendMethod::EngineCopyTable => native.copy_native_table(params),
                    _ => unreachable!("native plan method checked by caller"),
                })
                .await
                .map_err(|_| BackendError::request_failed("native plan task failed"))?
            }
            BackendMethod::EngineUndo => self.undo(params).await,
            BackendMethod::PathsSelect => self.select_paths(params).await,
            BackendMethod::PathsReveal => self.reveal_path(params).await,
            _ => {
                let native = Arc::clone(&self.native);
                tauri::async_runtime::spawn_blocking(move || native.dispatch(method, params))
                    .await
                    .map_err(|_| BackendError::request_failed("native backend task failed"))?
            }
        }
    }

    async fn apply(
        &self,
        params: Value,
        progress: Channel<ProgressEvent>,
    ) -> Result<Value, BackendError> {
        let (plan_id, op_id) = validate_apply_envelope(&params)?;
        let native = Arc::clone(&self.native);
        let plan_id = plan_id.to_string();
        let op_id = op_id.map(str::to_string);
        let progress_channel = progress.clone();
        let (mut result, approved_op_id) = tauri::async_runtime::spawn_blocking(move || {
            native.apply_native_plan(&plan_id, op_id.as_deref(), |event| {
                let _ = progress_channel.send(event);
            })
        })
        .await
        .map_err(|_| BackendError::request_failed("native apply task failed"))??;
        let native = Arc::clone(&self.native);
        tauri::async_runtime::spawn_blocking(move || {
            native.sync_result(&approved_op_id, &mut result);
            result
        })
        .await
        .map_err(|_| BackendError::request_failed("cloud sync task failed"))
    }

    async fn cancel(&self, params: Value) -> Result<Value, BackendError> {
        let object = params
            .as_object()
            .ok_or_else(BackendError::invalid_params)?;
        if object.keys().any(|key| key != "planId") {
            return Err(BackendError::invalid_params());
        }
        let plan_id = object
            .get("planId")
            .and_then(Value::as_str)
            .filter(|plan_id| !plan_id.is_empty())
            .ok_or_else(BackendError::invalid_params)?;
        Ok(serde_json::json!({
            "accepted": self.native.cancel_native_plan(plan_id)
        }))
    }

    async fn undo(&self, params: Value) -> Result<Value, BackendError> {
        let native = Arc::clone(&self.native);
        let native_params = params;
        if let Some(result) =
            tauri::async_runtime::spawn_blocking(move || native.try_native_undo(&native_params))
                .await
                .map_err(|_| BackendError::request_failed("native undo task failed"))??
        {
            return Ok(result);
        }
        Err(BackendError::validation(
            "journal contains an unsupported legacy recovery action",
        ))
    }

    async fn select_paths(&self, params: Value) -> Result<Value, BackendError> {
        let request: SelectPathsParams =
            serde_json::from_value(params).map_err(|_| BackendError::invalid_params())?;
        let mode = self
            .native
            .selection_mode(&request.op_id, &request.param_name)?;
        let app = self.app.clone();
        let paths = tauri::async_runtime::spawn_blocking(move || {
            let dialog = app.dialog().file();
            let selected = match mode {
                PathSelectionMode::Folder => dialog
                    .blocking_pick_folder()
                    .into_iter()
                    .collect::<Vec<_>>(),
                PathSelectionMode::File => {
                    dialog.blocking_pick_file().into_iter().collect::<Vec<_>>()
                }
                PathSelectionMode::Files => dialog.blocking_pick_files().unwrap_or_default(),
            };
            selected
                .into_iter()
                .map(|path| {
                    path.into_path()
                        .map_err(|_| BackendError::path_invalid("selected path is invalid"))
                })
                .collect::<Result<Vec<_>, BackendError>>()
        })
        .await
        .map_err(|_| BackendError::request_failed("path selection task failed"))??;
        serde_json::to_value(self.native.register_selected_paths(
            &request.op_id,
            &request.param_name,
            paths,
        )?)
        .map_err(|_| BackendError::protocol("path selection response encoding failed"))
    }

    async fn reveal_path(&self, params: Value) -> Result<Value, BackendError> {
        let request: RevealPathParams =
            serde_json::from_value(params).map_err(|_| BackendError::invalid_params())?;
        let path = self.native.reveal_path(&request.selection_id)?;
        let app = self.app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            app.opener()
                .open_path(path.to_string_lossy().into_owned(), None::<String>)
                .map_err(|_| BackendError::request_failed("selected path could not be opened"))
        })
        .await
        .map_err(|_| BackendError::request_failed("path reveal task failed"))??;
        Ok(serde_json::json!({"ok": true}))
    }

    async fn preview(
        &self,
        params: Value,
        progress: Channel<ProgressEvent>,
    ) -> Result<Value, BackendError> {
        let Some(object) = params.as_object() else {
            return Err(BackendError::invalid_params());
        };
        let operation_id = object
            .get("opId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let native = Arc::clone(&self.native);
        let progress_channel = progress.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let send = |event| {
                let _ = progress_channel.send(event);
            };
            match operation_id.as_str() {
                "rfi.process_scans" => native.preview_process_scans(params, send),
                "rfi.log_add" => native.preview_rfi_log(params, send),
                "rfi.stage_signed_first_pages" => native.preview_stage_signed_pages(params, send),
                "files.batch_print" => native.preview_batch_print(params, send),
                _ => Err(BackendError::validation("unknown operation ID")),
            }
        })
        .await
        .map_err(|_| BackendError::request_failed("native preview task failed"))?
    }
}

fn validate_apply_envelope(params: &Value) -> Result<(&str, Option<&str>), BackendError> {
    let object = params
        .as_object()
        .ok_or_else(BackendError::invalid_params)?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "planId" | "opId"))
    {
        return Err(BackendError::invalid_params());
    }
    let plan_id = object
        .get("planId")
        .and_then(Value::as_str)
        .filter(|plan_id| !plan_id.is_empty())
        .ok_or_else(BackendError::invalid_params)?;
    let op_id = match object.get("opId") {
        Some(Value::String(op_id)) if !op_id.is_empty() => Some(op_id.as_str()),
        Some(_) => return Err(BackendError::invalid_params()),
        None => None,
    };
    Ok((plan_id, op_id))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrontendRequest {
    method: BackendMethod,
    params: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectPathsParams {
    op_id: String,
    param_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevealPathParams {
    selection_id: String,
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn frontend_call(
    router: tauri::State<'_, BackendRouter>,
    request: FrontendRequest,
    on_progress: Channel<ProgressEvent>,
) -> Result<Value, BackendError> {
    debug_assert_eq!(request.method.owner(), MethodOwner::Rust);
    debug_assert!(BackendMethod::ALL.contains(&request.method));
    let _method_name = request.method.as_str();
    router
        .call(request.method, request.params, on_progress)
        .await
}

#[cfg(test)]
mod tests {
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
        ] {
            assert!(
                serde_json::from_value::<BackendMethod>(serde_json::json!(rejected)).is_err(),
                "{rejected} must not be React-reachable"
            );
        }
    }

    #[test]
    fn slice_nine_react_surface_is_entirely_rust_owned() {
        for method in BackendMethod::ALL {
            assert_eq!(method.owner(), MethodOwner::Rust, "{}", method.as_str());
        }
    }

    #[test]
    fn rust_and_typescript_frontend_method_rosters_match_exactly() {
        let commands = include_str!("../../frontend/src/lib/commands.ts");
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
        let bridge = include_str!("../../frontend/src/lib/bridge.ts");
        assert!(!bridge.contains("method: string"));
        assert!(!bridge.contains("backend_call"));
        assert!(bridge.contains("frontend_call"));
    }

    #[test]
    fn final_router_contains_no_fallback_or_process_runtime() {
        let backend = include_str!("backend.rs");
        let production = backend
            .split("#[cfg(test)]")
            .next()
            .expect("backend production source must remain inspectable");
        let app = include_str!("lib.rs");

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
        let cargo = include_str!("../Cargo.toml");
        let tauri = include_str!("../tauri.conf.json");
        let package = include_str!("../../package.json");

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
}
