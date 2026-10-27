use std::{path::PathBuf, sync::Arc};

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, ipc::Channel};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::{
    clipboard,
    contracts::{AuthState, ProgressEvent},
    diagnostics::{DiagnosticsInput, RecentErrorCodes, build_bundle, validate_bundle_content},
    errors::BackendError,
    native::{NativeBackend, PathSelectionMode},
    ocr::NativeOcr,
    registry::first_missing_required,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub(crate) enum BackendMethod {
    #[serde(rename = "app/bootstrap")]
    AppBootstrap,
    #[serde(rename = "settings/update")]
    SettingsUpdate,
    #[serde(rename = "settings/reset")]
    SettingsReset,
    #[serde(rename = "settings/replicaStatus")]
    SettingsReplicaStatus,
    #[serde(rename = "settings/params")]
    SettingsParams,
    #[serde(rename = "settings/saveParams")]
    SettingsSaveParams,
    #[serde(rename = "settings/presets")]
    SettingsPresets,
    #[serde(rename = "settings/savePreset")]
    SettingsSavePreset,
    #[serde(rename = "settings/deletePreset")]
    SettingsDeletePreset,
    #[serde(rename = "support/diagnostics")]
    SupportDiagnostics,
    #[serde(rename = "support/copyDiagnostics")]
    SupportCopyDiagnostics,
    #[serde(rename = "support/activityLog")]
    SupportActivityLog,
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
    #[serde(rename = "engine/rfiReplyPreview")]
    EngineRfiReplyPreview,
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
    #[serde(rename = "rfi/printFind")]
    RfiPrintFind,
    #[serde(rename = "watch/status")]
    WatchStatus,
    #[serde(rename = "watch/update")]
    WatchUpdate,
    #[serde(rename = "watch/open")]
    WatchOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MethodOwner {
    Rust,
}

impl BackendMethod {
    pub(crate) const ALL: [Self; 40] = [
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
        Self::SettingsReplicaStatus,
        Self::SettingsParams,
        Self::SettingsSaveParams,
        Self::SettingsPresets,
        Self::SettingsSavePreset,
        Self::SettingsDeletePreset,
        Self::SupportDiagnostics,
        Self::SupportCopyDiagnostics,
        Self::SupportActivityLog,
        Self::EnginePreview,
        Self::EngineApply,
        Self::EngineCancel,
        Self::EnginePrintQueueLatest,
        Self::EnginePrintQueueDiscard,
        Self::EnginePrintQueueResume,
        Self::EngineRevise,
        Self::EngineCopyTable,
        Self::EngineRfiReplyPreview,
        Self::EngineReviewScan,
        Self::EngineReviewPreview,
        Self::EngineUndo,
        Self::HistoryList,
        Self::PathsSelect,
        Self::PathsReveal,
        Self::RecoveryList,
        Self::RfiPrintFind,
        Self::WatchStatus,
        Self::WatchUpdate,
        Self::WatchOpen,
    ];

    pub(crate) const fn owner(self) -> MethodOwner {
        MethodOwner::Rust
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AppBootstrap => "app/bootstrap",
            Self::SettingsUpdate => "settings/update",
            Self::SettingsReset => "settings/reset",
            Self::SettingsReplicaStatus => "settings/replicaStatus",
            Self::SettingsParams => "settings/params",
            Self::SettingsSaveParams => "settings/saveParams",
            Self::SettingsPresets => "settings/presets",
            Self::SettingsSavePreset => "settings/savePreset",
            Self::SettingsDeletePreset => "settings/deletePreset",
            Self::SupportDiagnostics => "support/diagnostics",
            Self::SupportCopyDiagnostics => "support/copyDiagnostics",
            Self::SupportActivityLog => "support/activityLog",
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
            Self::EngineRfiReplyPreview => "engine/rfiReplyPreview",
            Self::EngineReviewScan => "engine/reviewScan",
            Self::EngineReviewPreview => "engine/reviewPreview",
            Self::EngineUndo => "engine/undo",
            Self::HistoryList => "history/list",
            Self::PathsSelect => "paths/select",
            Self::PathsReveal => "paths/reveal",
            Self::RfiPrintFind => "rfi/printFind",
            Self::RecoveryList => "recovery/list",
            Self::WatchStatus => "watch/status",
            Self::WatchUpdate => "watch/update",
            Self::WatchOpen => "watch/open",
        }
    }

    fn authorize(self, state: &AuthState) -> Result<(), BackendError> {
        if matches!(
            self,
            Self::AppBootstrap
                | Self::SettingsUpdate
                | Self::SettingsReset
                | Self::SettingsReplicaStatus
                | Self::SupportDiagnostics
                | Self::SupportCopyDiagnostics
                | Self::SupportActivityLog
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
    recent_error_codes: RecentErrorCodes,
    activity_log: crate::activity_log::ActivityLog,
}

fn path_dialog_filter(
    operation_id: &str,
    parameter_name: &str,
) -> Option<(&'static str, &'static [&'static str])> {
    match (operation_id, parameter_name) {
        ("rfi.log_add", "workbook")
        | ("rfi.follow_up_sync", "rfi_log_workbook" | "follow_up_workbook")
        | ("rfi.system_sync", "rfi_log_workbook" | "follow_up_workbook")
        | ("rfi.complete_status_sync", "rfi_log_workbook")
        | ("rfi.publish_status", "rfi_log_workbook")
        | ("rfi.generate_concrete_documents", "rfi_log_workbook" | "follow_up_workbook") => {
            Some(("Excel (.xlsm, .xlsx)", &["xlsm", "xlsx"]))
        }
        ("rfi.generate_concrete_documents", "template_010" | "template_013") => {
            Some(("Excel (.xlsx)", &["xlsx"]))
        }
        ("rfi.generate_concrete_documents", "template_011" | "template_014") => {
            Some(("Word (.docx)", &["docx"]))
        }
        _ => None,
    }
}

impl BackendRouter {
    /// Starts the read-only inbox check loop. It only prepares Previews.
    pub(crate) fn start_watch(&self) {
        let native = Arc::clone(&self.native);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(30));
            loop {
                native.watch_tick();
                std::thread::sleep(native.watch_poll_interval());
            }
        });
    }

    pub(crate) fn new(app: AppHandle) -> Result<Self, BackendError> {
        let native = Arc::new(NativeBackend::production()?);
        Ok(Self {
            app,
            native,
            recent_error_codes: RecentErrorCodes::default(),
            activity_log: crate::activity_log::ActivityLog::new(
                crate::activity_log::ActivityLog::default_dir(),
            ),
        })
    }

    pub(crate) async fn call(
        &self,
        method: BackendMethod,
        params: Value,
        progress: Channel<ProgressEvent>,
    ) -> Result<Value, BackendError> {
        method.authorize(&self.native.auth_state())?;
        let _operator = matches!(
            method,
            BackendMethod::EnginePreview
                | BackendMethod::EngineApply
                | BackendMethod::EngineUndo
                | BackendMethod::EngineReviewScan
                | BackendMethod::EngineReviewPreview
        )
        .then(|| self.native.watch_operator_guard());
        match method {
            BackendMethod::WatchUpdate => {
                let native = Arc::clone(&self.native);
                tauri::async_runtime::spawn_blocking(move || {
                    let status = native.dispatch(BackendMethod::WatchUpdate, params);
                    if status.is_ok() {
                        let background = Arc::clone(&native);
                        std::thread::spawn(move || background.watch_tick());
                    }
                    status
                })
                .await
                .map_err(|_| BackendError::request_failed("watch update task failed"))?
            }
            BackendMethod::SupportDiagnostics => self.support_diagnostics(params).await,
            BackendMethod::SupportCopyDiagnostics => self.copy_diagnostics(params).await,
            BackendMethod::SupportActivityLog => {
                validate_empty_params(&params)?;
                Ok(serde_json::json!({
                    "fileName": "docflow-activity-log.jsonl",
                    "mediaType": "application/x-ndjson",
                    "content": self.activity_log.export(),
                }))
            }
            BackendMethod::EnginePreview => self.preview(params, progress).await,
            BackendMethod::EngineApply => self.apply(params, progress).await,
            BackendMethod::EngineCancel => self.cancel(params).await,
            BackendMethod::EngineRevise
            | BackendMethod::EngineCopyTable
            | BackendMethod::EngineRfiReplyPreview => {
                let native = Arc::clone(&self.native);
                tauri::async_runtime::spawn_blocking(move || match method {
                    BackendMethod::EngineRevise => native.revise_native_plan(params),
                    BackendMethod::EngineCopyTable => native.copy_native_table(params),
                    BackendMethod::EngineRfiReplyPreview => native.preview_rfi_reply(params),
                    _ => unreachable!("native plan method checked by caller"),
                })
                .await
                .map_err(|_| BackendError::request_failed("native plan task failed"))?
            }
            BackendMethod::EngineUndo => self.undo(params).await,
            BackendMethod::PathsSelect => self.select_paths(params).await,
            BackendMethod::PathsReveal => self.reveal_path(params).await,
            BackendMethod::RfiPrintFind => {
                let native = Arc::clone(&self.native);
                tauri::async_runtime::spawn_blocking(move || native.find_rfi_print_files(params))
                    .await
                    .map_err(|_| BackendError::request_failed("RFI document discovery failed"))?
            }
            _ => {
                let native = Arc::clone(&self.native);
                tauri::async_runtime::spawn_blocking(move || native.dispatch(method, params))
                    .await
                    .map_err(|_| BackendError::request_failed("native backend task failed"))?
            }
        }
    }

    async fn support_diagnostics(&self, params: Value) -> Result<Value, BackendError> {
        validate_empty_params(&params)?;
        let recent_error_codes = self.recent_error_codes.snapshot();
        let printer = self.native.printer_diagnostics();
        tauri::async_runtime::spawn_blocking(move || {
            let ocr = NativeOcr::discover();
            let (ocr_available, ocr_language_data_available) = ocr.support_status();
            serde_json::to_value(build_bundle(DiagnosticsInput {
                version: env!("CARGO_PKG_VERSION").to_string(),
                platform_os: std::env::consts::OS.to_string(),
                platform_arch: std::env::consts::ARCH.to_string(),
                ocr_available,
                ocr_language_data_available,
                recent_error_codes,
                printer,
            })?)
            .map_err(|_| BackendError::protocol("diagnostics preview encoding failed"))
        })
        .await
        .map_err(|_| BackendError::request_failed("diagnostics collection task failed"))?
    }

    async fn copy_diagnostics(&self, params: Value) -> Result<Value, BackendError> {
        let request: CopyDiagnosticsParams =
            serde_json::from_value(params).map_err(|_| BackendError::invalid_params())?;
        validate_bundle_content(&request.content)?;
        tauri::async_runtime::spawn_blocking(move || clipboard::write_plain(&request.content))
            .await
            .map_err(|_| BackendError::request_failed("diagnostics clipboard task failed"))??;
        Ok(serde_json::json!({"ok": true}))
    }

    async fn apply(
        &self,
        params: Value,
        progress: Channel<ProgressEvent>,
    ) -> Result<Value, BackendError> {
        let (plan_id, op_id) = validate_apply_envelope(&params)?;
        let native = Arc::clone(&self.native);
        let plan_id = plan_id.to_string();
        let params_plan_id = Some(plan_id.clone());
        let op_id = op_id.map(str::to_string);
        let progress_channel = progress.clone();
        let (mut result, approved_op_id) = tauri::async_runtime::spawn_blocking(move || {
            native.apply_native_plan(&plan_id, op_id.as_deref(), |event| {
                let _ = progress_channel.send(event);
            })
        })
        .await
        .map_err(|_| BackendError::request_failed("native apply task failed"))??;
        if let Some(plan_id) = params_plan_id.as_deref() {
            self.native.watch_forget_plan(plan_id);
        }
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
        if let Some(plan_id) = object
            .get("planId")
            .and_then(Value::as_str)
            .filter(|plan_id| !plan_id.is_empty())
        {
            return Ok(serde_json::json!({
                "accepted": self.native.cancel_native_plan(plan_id)
            }));
        }
        if let Some(op_id) = object
            .get("opId")
            .and_then(Value::as_str)
            .filter(|op_id| !op_id.is_empty())
        {
            return Ok(serde_json::json!({
                "accepted": self.native.cancel_native_operation(op_id)
            }));
        }
        Err(BackendError::invalid_params())
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
        let filter = path_dialog_filter(&request.op_id, &request.param_name);
        let paths = if !request.dropped_paths.is_empty() {
            if request.dropped_paths.len() > 1000 {
                return Err(BackendError::invalid_params());
            }
            if mode != PathSelectionMode::Files && request.dropped_paths.len() > 1 {
                return Err(BackendError::invalid_params());
            }
            let mut resolved = Vec::new();
            for path_str in request.dropped_paths {
                let path = PathBuf::from(path_str);
                if !path.exists() {
                    return Err(BackendError::path_invalid("dropped path does not exist"));
                }
                match mode {
                    PathSelectionMode::Folder if !path.is_dir() => {
                        return Err(BackendError::path_invalid("dropped path must be a folder"));
                    }
                    PathSelectionMode::File | PathSelectionMode::Files if !path.is_file() => {
                        return Err(BackendError::path_invalid("dropped path must be a file"));
                    }
                    _ => {}
                }
                if let Some((_, allowed_extensions)) = filter {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_ascii_lowercase)
                        .unwrap_or_default();
                    if !allowed_extensions
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(&ext))
                    {
                        return Err(BackendError::path_invalid(
                            "dropped file type is not accepted",
                        ));
                    }
                }
                resolved.push(path);
            }
            resolved
        } else {
            let app = self.app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let dialog = app.dialog().file();
                let dialog = match filter {
                    Some((name, extensions)) => dialog.add_filter(name, extensions),
                    None => dialog,
                };
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
            .map_err(|_| BackendError::request_failed("path selection task failed"))??
        };
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
        // Required flags come from the registry schema, so every preview enforces
        // them before any operation-specific work starts.
        let values = object
            .get("values")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(missing) = first_missing_required(&operation_id, &values) {
            return Err(BackendError::validation(missing));
        }
        let native = Arc::clone(&self.native);
        let progress_channel = progress.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let send = |event| {
                let _ = progress_channel.send(event);
            };
            match operation_id.as_str() {
                "rfi.process_scans_fast_covers" => native.preview_process_scans(params, send),
                "rfi.group_files" => native.preview_rfi_file_group(params, send),
                "rfi.log_add" => native.preview_rfi_log(params, send),
                "rfi.follow_up_sync" => native.preview_rfi_follow_up(params, send),
                "rfi.generate_concrete_documents" => {
                    native.preview_concrete_documents(params, send)
                }
                "rfi.system_sync" => native.preview_rfi_system_sync(params, send),
                "rfi.complete_status_sync" => native.preview_rfi_complete_status_sync(params, send),
                "rfi.publish_status" => native.preview_rfi_publish_status(params, send),
                "rfi.stage_signed_first_pages" => native.preview_stage_signed_pages(params, send),
                "rfi.normalize_pdf_orientation" => native.preview_pdf_orientation(params, send),
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
    #[serde(default)]
    dropped_paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevealPathParams {
    selection_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyDiagnosticsParams {
    content: String,
}

fn unix_seconds_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("unix:{seconds}")
}

fn validate_empty_params(params: &Value) -> Result<(), BackendError> {
    if params.as_object().is_some_and(serde_json::Map::is_empty) {
        Ok(())
    } else {
        Err(BackendError::invalid_params())
    }
}

#[tauri::command(rename_all = "camelCase")]
pub(crate) async fn frontend_call(
    router: tauri::State<'_, BackendRouter>,
    request: FrontendRequest,
    on_progress: Channel<ProgressEvent>,
) -> Result<Value, BackendError> {
    debug_assert_eq!(request.method.owner(), MethodOwner::Rust);
    debug_assert!(BackendMethod::ALL.contains(&request.method));
    let method_name = request.method.as_str();
    let op_id = request
        .params
        .get("opId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let started = std::time::Instant::now();
    let method = request.method;
    let result = router.call(method, request.params, on_progress).await;
    if let Err(error) = &result {
        router.recent_error_codes.record(error.code);
    }
    if method != BackendMethod::SupportActivityLog {
        let mut entry = serde_json::json!({
            "at": unix_seconds_stamp(),
            "version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "method": method_name,
            "opId": op_id,
            "ms": started.elapsed().as_millis() as u64,
        });
        match &result {
            Ok(value) => {
                entry["ok"] = Value::Bool(true);
                if matches!(
                    method,
                    BackendMethod::EnginePreview | BackendMethod::EngineRevise
                ) {
                    entry["plan"] = crate::activity_log::summarize_plan(value);
                }
            }
            Err(error) => {
                entry["ok"] = Value::Bool(false);
                entry["errorCode"] = Value::String(error.code.to_string());
                entry["error"] = Value::String(error.message.clone());
            }
        }
        let trace = crate::ocr::take_cover_trace();
        if !trace.is_empty() {
            entry["ocrCoverTrace"] = serde_json::json!(trace);
        }
        if let Some(failure) = crate::leaf_transaction::take_last_failure() {
            entry["publicationFailure"] = Value::String(failure);
        }
        router.activity_log.record(entry);
    }
    result
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod empty_params_tests {
    use super::*;

    #[test]
    fn accepts_only_an_empty_object() {
        assert!(validate_empty_params(&serde_json::json!({})).is_ok());
    }

    #[test]
    fn rejects_extra_fields_null_and_non_object_params() {
        assert!(validate_empty_params(&serde_json::json!({ "extra": 1 })).is_err());
        assert!(validate_empty_params(&Value::Null).is_err());
        assert!(validate_empty_params(&serde_json::json!([])).is_err());
    }
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;

    #[test]
    fn timestamp_is_unix_seconds_with_a_fixed_prefix() {
        let stamp = unix_seconds_stamp();
        let seconds = stamp
            .strip_prefix("unix:")
            .expect("timestamp keeps the unix: prefix")
            .parse::<u64>()
            .expect("timestamp suffix is whole seconds");
        assert!(seconds > 1_700_000_000);
    }
}
