use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    auth::{AuthConfig, AuthService},
    backend::BackendMethod,
    clipboard,
    cloud::CloudHistory,
    contracts::{
        AppPrefs, Bootstrap, JsonObject, OperationDef, ParamKind, PrintQueueSummary, ProgressEvent,
        StepKind,
    },
    credentials::OsCredentialStore,
    errors::BackendError,
    fs_ops::FilesystemAuthority,
    gemini::{DEFAULT_MODEL, FALLBACK_MODELS, GeminiClient},
    http::{HttpClient, HttpConfig, JsonHttp},
    journal::JournalStore,
    ocr::NativeOcr,
    operations::process_scans::{ProcessScansParams, preview as preview_process_scans},
    operations::stage_signed_pages::{
        StageSignedPagesParams, preview as preview_stage_signed_pages,
        revise as revise_stage_signed_pages,
    },
    outlook_reply_draft::{self, OutlookDraftService},
    path_selection::{PathSelection, PathSelectionStore, SelectionKind, SelectionScope},
    paths::{CallerIdentity, GrantAccess, GrantScope, PathIntent},
    plan_store::{BackendOrigin, PlanBinding},
    platform,
    print_queue::{BatchPrintParams, PrintService, default_queue_root},
    progress::GlobalMutationLock,
    readiness::{ReadinessFacts, ReadinessReport, build_readiness},
    registry::{RegistryFacts, operation_registry},
    review::{
        ReviewDecision as NativeReviewDecision, preview as preview_review, scan as scan_review,
    },
    rfi_follow_up, rfi_log, rfi_system_sync,
    settings::{PresetStoreError, SettingsStore, default_settings_path},
    work_items::{self, WorkItemExportInput, WorkItemInput, WorkItemQueues},
};

const SUPABASE_URL: &str = "https://ndpkgbfmshceffwhpqoz.supabase.co";
const SUPABASE_KEY: &str = "sb_publishable_sK0uEH4t-Hw7LVBpnEaMog_z_2MQwxq";
const GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
const MAX_OPERATION_ID_CHARS: usize = 128;

pub(crate) struct NativeBackend {
    settings: Arc<SettingsStore>,
    auth: Arc<AuthService>,
    cloud: Arc<CloudHistory>,
    gemini: Arc<GeminiClient>,
    journals: JournalStore,
    filesystem: FilesystemAuthority,
    selections: PathSelectionStore,
    print: PrintService,
    outlook: OutlookDraftService,
    process_session_id: Mutex<String>,
    app_data_dir: PathBuf,
}

impl NativeBackend {
    pub(crate) fn production() -> Result<Self, BackendError> {
        let settings_path = default_settings_path();
        let app_data_dir = settings_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| env::temp_dir().join("docflow"));
        let _ = fs::create_dir_all(app_data_dir.join("trash"));
        let journal_dir = settings_path
            .parent()
            .map(|parent| parent.join("journal"))
            .unwrap_or_else(|| env::temp_dir().join("docflow-journal"));
        let settings = Arc::new(
            SettingsStore::new(settings_path).map_err(|_| BackendError::settings_write())?,
        );
        let supabase_http: Arc<dyn JsonHttp> = Arc::new(
            HttpClient::new(HttpConfig::default()).expect("valid Supabase HTTP configuration"),
        );
        let auth_config = AuthConfig::new(SUPABASE_URL, SUPABASE_KEY);
        let auth = Arc::new(AuthService::new(
            settings.clone(),
            Arc::new(OsCredentialStore::new()),
            supabase_http.clone(),
            auth_config.clone(),
        ));
        let journals = JournalStore::new(&journal_dir);
        let cloud = Arc::new(CloudHistory::new(
            auth.clone(),
            supabase_http,
            auth_config,
            journal_dir.clone(),
        ));
        let gemini_http: Arc<dyn JsonHttp> = Arc::new(
            HttpClient::new(HttpConfig {
                request_timeout: Duration::from_secs(45),
                ..HttpConfig::default()
            })
            .expect("valid Gemini HTTP configuration"),
        );
        let gemini = Arc::new(GeminiClient::new(
            gemini_http,
            GEMINI_URL,
            configured_gemini_models(),
        ));
        let mutation_lock = GlobalMutationLock::default();
        let filesystem = FilesystemAuthority::with_mutation_lock(
            &journal_dir,
            Duration::from_secs(30 * 60),
            mutation_lock.clone(),
        );
        let selections = PathSelectionStore::new(Duration::from_secs(30 * 60));
        let print = PrintService::new(
            default_queue_root(&app_data_dir),
            &journal_dir,
            Duration::from_secs(30 * 60),
            mutation_lock.clone(),
            platform::production_print_adapter(),
        );
        let outlook = OutlookDraftService::new(
            &journal_dir,
            Duration::from_secs(30 * 60),
            mutation_lock,
            platform::production_outlook_adapter(),
        );
        #[cfg(target_os = "windows")]
        let print =
            print.with_excel_pipeline(platform::windows::excel::production_excel_pipeline());
        Ok(Self {
            settings,
            auth,
            cloud,
            gemini,
            journals,
            filesystem,
            selections,
            print,
            outlook,
            process_session_id: Mutex::new(uuid::Uuid::new_v4().to_string()),
            app_data_dir,
        })
    }

    pub(crate) fn dispatch(
        &self,
        method: BackendMethod,
        params: Value,
    ) -> Result<Value, BackendError> {
        match method {
            BackendMethod::SupportDiagnostics | BackendMethod::SupportCopyDiagnostics => Err(
                BackendError::protocol("support command reached the operation backend"),
            ),
            BackendMethod::SupportReadiness => {
                decode_empty(params)?;
                encode(self.readiness()?)
            }
            BackendMethod::WorkItemsList => {
                decode_empty(params)?;
                encode(self.work_items()?)
            }
            BackendMethod::AppBootstrap => {
                decode_empty(params)?;
                let auth = self.auth.restore(now())?;
                encode(Bootstrap {
                    protocol: 1,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    auth,
                    prefs: self.settings.preferences(),
                    operations: operation_registry(self.registry_facts()),
                })
            }
            BackendMethod::SettingsUpdate => self.update_settings(params),
            BackendMethod::SettingsReset => {
                decode_empty(params)?;
                self.settings
                    .save_preferences(AppPrefs::default())
                    .map_err(|_| BackendError::settings_write())?;
                encode(self.settings.preferences())
            }
            BackendMethod::SettingsParams => {
                let request: OperationIdParams = decode(params)?;
                validate_operation_id(&request.op_id)?;
                Ok(Value::Object(self.selected_params(&request.op_id)?))
            }
            BackendMethod::SettingsSaveParams => {
                let request: SaveParams = decode(params)?;
                validate_operation_id(&request.op_id)?;
                let values =
                    self.sanitize_and_materialize_save_params(&request.op_id, request.values)?;
                self.settings
                    .save_params(&request.op_id, values)
                    .map_err(|_| BackendError::settings_write())?;
                Ok(json!({"ok": true}))
            }
            BackendMethod::SettingsPresets => {
                let request: OperationIdParams = decode(params)?;
                validate_operation_id(&request.op_id)?;
                let operation = self.preset_operation(&request.op_id)?;
                encode(
                    self.settings
                        .presets_for(&operation)
                        .map_err(preset_store_error)?,
                )
            }
            BackendMethod::SettingsSavePreset => {
                let request: SavePresetParams = decode(params)?;
                validate_operation_id(&request.op_id)?;
                let operation = self.preset_operation(&request.op_id)?;
                encode(
                    self.settings
                        .save_preset(&operation, &request.name, request.values, request.replace)
                        .map_err(preset_store_error)?,
                )
            }
            BackendMethod::SettingsDeletePreset => {
                let request: DeletePresetParams = decode(params)?;
                validate_operation_id(&request.op_id)?;
                let operation = self.preset_operation(&request.op_id)?;
                self.settings
                    .delete_preset(&operation, &request.preset_id)
                    .map_err(preset_store_error)?;
                Ok(json!({"ok": true}))
            }
            BackendMethod::AuthSignIn => {
                if let Ok(caller) = self.signed_in_caller_identity() {
                    self.revoke_session_authority(&caller);
                }
                self.rotate_session_authority();
                let request: SignInParams = decode(params)?;
                encode(
                    self.auth
                        .sign_in(&request.identifier, &request.password, now())?,
                )
            }
            BackendMethod::AuthSignUp => {
                let request: SignUpParams = decode(params)?;
                self.auth
                    .sign_up(&request.email, &request.password, &request.username)?;
                Ok(json!({"ok": true}))
            }
            BackendMethod::AuthSignOut => {
                decode_empty(params)?;
                let caller = self.signed_in_caller_identity().ok();
                if let Some(caller) = caller.as_ref() {
                    self.revoke_session_authority(caller);
                }
                self.rotate_session_authority();
                encode(self.auth.sign_out()?)
            }
            BackendMethod::AuthUsernameAvailable => {
                let request: UsernameParams = decode(params)?;
                Ok(Value::Bool(self.auth.username_available(&request.username)))
            }
            BackendMethod::AuthLoadMe => {
                decode_empty(params)?;
                encode(self.auth.load_me(now())?)
            }
            BackendMethod::AuthSetUsername => {
                let request: UsernameParams = decode(params)?;
                encode(self.auth.set_username(&request.username, now())?)
            }
            BackendMethod::AuthListUsers => {
                decode_empty(params)?;
                encode(self.auth.list_users(now())?)
            }
            BackendMethod::AuthSetFlags => {
                let request: SetFlagsParams = decode(params)?;
                self.auth
                    .set_flags(&request.user_id, request.allowed, request.is_admin, now())?;
                Ok(json!({"ok": true}))
            }
            BackendMethod::EnginePrintQueueLatest => {
                decode_empty(params)?;
                encode(self.print.latest(&self.caller_identity()?)?)
            }
            BackendMethod::EnginePrintQueueDiscard => {
                let request: QueueIdParams = decode(params)?;
                encode(
                    self.print
                        .discard(&self.caller_identity()?, &request.queue_id)?,
                )
            }
            BackendMethod::EnginePrintQueueResume => {
                let request: QueueIdParams = decode(params)?;
                encode(
                    self.print
                        .resume(&self.caller_identity()?, &request.queue_id)?,
                )
            }
            BackendMethod::EngineReviewScan => self.review_scan(params),
            BackendMethod::EngineReviewPreview => self.review_preview(params),
            BackendMethod::HistoryList => {
                let request: HistoryParams = decode(params)?;
                encode(self.cloud.list(request.limit.unwrap_or(50), now())?)
            }
            BackendMethod::RecoveryList => {
                decode_empty(params)?;
                encode(self.recovery()?)
            }
            _ => Err(BackendError::protocol(
                "method is not owned by the Rust native backend",
            )),
        }
    }

    pub(crate) fn auth_state(&self) -> crate::contracts::AuthState {
        self.auth.state()
    }

    pub(crate) fn sync_result(&self, approved_op_id: &str, result: &mut Value) {
        sync_result_metadata(
            result,
            |journal_id| {
                self.journals
                    .read(journal_id)
                    .ok()
                    .map(|journal| journal.steps.len())
            },
            |applied, failed, journal_id, summary| {
                self.cloud
                    .push(approved_op_id, applied, failed, journal_id, summary, now())
            },
        );
    }

    pub(crate) fn mark_undone(&self, cloud_id: &str) {
        self.cloud.mark_undone(cloud_id, now());
    }

    pub(crate) fn selection_mode(
        &self,
        operation_id: &str,
        parameter_name: &str,
    ) -> Result<PathSelectionMode, BackendError> {
        match path_parameter_kind(operation_id, parameter_name) {
            Some(ParamKind::Folder) => Ok(PathSelectionMode::Folder),
            Some(ParamKind::File) => Ok(PathSelectionMode::File),
            Some(ParamKind::Files) => Ok(PathSelectionMode::Files),
            _ => Err(BackendError::invalid_params()),
        }
    }

    pub(crate) fn register_selected_paths(
        &self,
        operation_id: &str,
        parameter_name: &str,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<PathSelection>, BackendError> {
        let mode = self.selection_mode(operation_id, parameter_name)?;
        if mode != PathSelectionMode::Files && paths.len() > 1 {
            return Err(BackendError::invalid_params());
        }
        let kind = if mode == PathSelectionMode::Folder {
            SelectionKind::Folder
        } else {
            SelectionKind::File
        };
        let caller = self.caller_identity()?;
        paths
            .into_iter()
            .map(|path| {
                self.selections
                    .issue(&caller, operation_id, parameter_name, &path, kind)
            })
            .collect()
    }

    pub(crate) fn reveal_path(&self, selection_id: &str) -> Result<PathBuf, BackendError> {
        self.selections.resolve_scope(
            selection_id,
            &self.caller_identity()?,
            SelectionScope::RevealReviewItem,
        )
    }

    fn selected_params(&self, operation_id: &str) -> Result<Map<String, Value>, BackendError> {
        let operation = operation_registry(self.registry_facts())
            .into_iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
        let caller = self.caller_identity()?;
        let saved = self.settings.params_for(operation_id);

        let mut result = Map::new();
        for param in operation.schema.params {
            if !param.remember {
                continue;
            }
            let Some(raw) = saved.get(&param.name).cloned() else {
                continue;
            };
            if raw.is_null() {
                continue;
            }
            match param.kind {
                ParamKind::Secret | ParamKind::LongText => {
                    continue;
                }
                ParamKind::Folder | ParamKind::File => {
                    let selection_kind = if param.kind == ParamKind::Folder {
                        SelectionKind::Folder
                    } else {
                        SelectionKind::File
                    };
                    if let Some(selection) = raw
                        .as_str()
                        .filter(|p| Path::new(p).exists())
                        .and_then(|p| {
                            self.selections
                                .issue(
                                    &caller,
                                    operation_id,
                                    &param.name,
                                    Path::new(p),
                                    selection_kind,
                                )
                                .ok()
                        })
                    {
                        result.insert(param.name, json!(selection));
                    }
                }
                ParamKind::Files => {
                    if let Some(arr) = raw.as_array() {
                        let mut selections = Vec::new();
                        for item in arr {
                            if let Some(selection) = item
                                .as_str()
                                .filter(|p| Path::new(p).exists())
                                .and_then(|p| {
                                    self.selections
                                        .issue(
                                            &caller,
                                            operation_id,
                                            &param.name,
                                            Path::new(p),
                                            SelectionKind::File,
                                        )
                                        .ok()
                                })
                            {
                                selections.push(selection);
                            }
                        }
                        if !selections.is_empty() {
                            result.insert(param.name, json!(selections));
                        }
                    }
                }
                ParamKind::SheetCounts => {
                    if let Some(counts) = raw.as_object() {
                        let is_valid = counts
                            .values()
                            .all(|v| v.as_u64().is_some() || v.as_i64().is_some_and(|n| n >= 0));
                        if is_valid {
                            result.insert(param.name, Value::Object(counts.clone()));
                        }
                    }
                }
                ParamKind::Bool => {
                    if raw.is_boolean() {
                        result.insert(param.name, raw);
                    }
                }
                ParamKind::Choice => {
                    if let Some(_choice_str) = raw.as_str().filter(|c| {
                        param.choices.is_empty() || param.choices.iter().any(|choice| choice == *c)
                    }) {
                        result.insert(param.name, raw);
                    }
                }
                ParamKind::Int => {
                    if raw.is_number() {
                        result.insert(param.name, raw);
                    }
                }
                ParamKind::Text => {
                    if raw.is_string() {
                        result.insert(param.name, raw);
                    }
                }
            }
        }
        Ok(result)
    }

    fn sanitize_and_materialize_save_params(
        &self,
        operation_id: &str,
        mut values: Map<String, Value>,
    ) -> Result<Map<String, Value>, BackendError> {
        let operation = operation_registry(self.registry_facts())
            .into_iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
        let caller = self.caller_identity()?;

        let mut sanitized = Map::new();
        for param in operation.schema.params {
            if !param.remember {
                continue;
            }
            let Some(raw) = values.remove(&param.name) else {
                continue;
            };
            if raw.is_null() {
                continue;
            }
            match param.kind {
                ParamKind::Secret | ParamKind::LongText => {
                    continue;
                }
                ParamKind::Folder | ParamKind::File => {
                    let selection_kind = if param.kind == ParamKind::Folder {
                        SelectionKind::Folder
                    } else {
                        SelectionKind::File
                    };
                    if let Ok(selection) = decode::<PathSelection>(raw.clone()) {
                        if let Ok(path) = self.selections.resolve(
                            &selection,
                            &caller,
                            operation_id,
                            &param.name,
                            selection_kind,
                        ) {
                            sanitized.insert(
                                param.name,
                                Value::String(path.to_string_lossy().into_owned()),
                            );
                        } else if let Some(previous_saved) = self
                            .settings
                            .params_for(operation_id)
                            .get(&param.name)
                            .and_then(Value::as_str)
                            .filter(|saved| {
                                *saved == selection.display_path && Path::new(saved).exists()
                            })
                        {
                            sanitized.insert(param.name, Value::String(previous_saved.to_string()));
                        }
                    } else if let Some(canonical) = raw
                        .as_str()
                        .filter(|p| Path::new(p).exists())
                        .and_then(|p| fs::canonicalize(Path::new(p)).ok())
                    {
                        sanitized.insert(
                            param.name,
                            Value::String(canonical.to_string_lossy().into_owned()),
                        );
                    }
                }
                ParamKind::Files => {
                    if let Ok(selections) = decode::<Vec<PathSelection>>(raw.clone()) {
                        let mut paths = Vec::new();
                        for selection in selections {
                            if let Ok(path) = self.selections.resolve(
                                &selection,
                                &caller,
                                operation_id,
                                &param.name,
                                SelectionKind::File,
                            ) {
                                paths.push(Value::String(path.to_string_lossy().into_owned()));
                            } else if let Some(_saved_array) = self
                                .settings
                                .params_for(operation_id)
                                .get(&param.name)
                                .and_then(Value::as_array)
                                .filter(|arr| {
                                    arr.iter().any(|item| {
                                        item.as_str().is_some_and(|p| p == selection.display_path)
                                    }) && Path::new(&selection.display_path).exists()
                                })
                            {
                                paths.push(Value::String(selection.display_path.clone()));
                            }
                        }
                        if !paths.is_empty() {
                            sanitized.insert(param.name, Value::Array(paths));
                        }
                    } else if let Some(arr) = raw.as_array() {
                        let mut paths = Vec::new();
                        for item in arr {
                            if let Some(canonical) = item
                                .as_str()
                                .filter(|p| Path::new(p).exists())
                                .and_then(|p| fs::canonicalize(Path::new(p)).ok())
                            {
                                paths.push(Value::String(canonical.to_string_lossy().into_owned()));
                            }
                        }
                        if !paths.is_empty() {
                            sanitized.insert(param.name, Value::Array(paths));
                        }
                    }
                }
                ParamKind::SheetCounts => {
                    if let Some(counts) = raw.as_object() {
                        let is_valid = counts
                            .values()
                            .all(|v| v.as_u64().is_some() || v.as_i64().is_some_and(|n| n >= 0));
                        if is_valid {
                            sanitized.insert(param.name, Value::Object(counts.clone()));
                        }
                    }
                }
                ParamKind::Bool => {
                    if raw.is_boolean() {
                        sanitized.insert(param.name, raw);
                    }
                }
                ParamKind::Choice => {
                    if let Some(_choice_str) = raw.as_str().filter(|c| {
                        param.choices.is_empty() || param.choices.iter().any(|choice| choice == *c)
                    }) {
                        sanitized.insert(param.name, raw);
                    }
                }
                ParamKind::Int => {
                    if raw.is_number() {
                        sanitized.insert(param.name, raw);
                    }
                }
                ParamKind::Text => {
                    if raw.is_string() {
                        sanitized.insert(param.name, raw);
                    }
                }
            }
        }
        Ok(sanitized)
    }

    fn preset_operation(&self, operation_id: &str) -> Result<OperationDef, BackendError> {
        let operation = operation_registry(self.registry_facts())
            .into_iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
        if !operation.supports_presets {
            return Err(BackendError::validation(
                "operation does not support presets",
            ));
        }
        Ok(operation)
    }

    fn materialize_preview(&self, params: Value) -> Result<Value, BackendError> {
        let mut object = params
            .as_object()
            .cloned()
            .ok_or_else(BackendError::invalid_params)?;
        let operation_id = object
            .get("opId")
            .and_then(Value::as_str)
            .ok_or_else(BackendError::invalid_params)?
            .to_string();
        let values = object
            .remove("values")
            .and_then(|value| value.as_object().cloned())
            .ok_or_else(BackendError::invalid_params)?;
        object.insert(
            "values".to_string(),
            Value::Object(self.materialize_operation_values(&operation_id, values)?),
        );
        Ok(Value::Object(object))
    }

    fn materialize_operation_values(
        &self,
        operation_id: &str,
        mut values: Map<String, Value>,
    ) -> Result<Map<String, Value>, BackendError> {
        let caller = self.caller_identity()?;
        for (name, kind) in operation_path_parameters(operation_id)? {
            let Some(raw) = values.get(&name).cloned() else {
                continue;
            };
            if raw.is_null() {
                continue;
            }
            let materialized = match kind {
                ParamKind::Folder | ParamKind::File => {
                    let selection: PathSelection = decode(raw)?;
                    let path = self.selections.resolve(
                        &selection,
                        &caller,
                        operation_id,
                        &name,
                        if kind == ParamKind::Folder {
                            SelectionKind::Folder
                        } else {
                            SelectionKind::File
                        },
                    )?;
                    Value::String(path.to_string_lossy().into_owned())
                }
                ParamKind::Files => {
                    let selections: Vec<PathSelection> = decode(raw)?;
                    Value::Array(
                        selections
                            .into_iter()
                            .map(|selection| {
                                self.selections
                                    .resolve(
                                        &selection,
                                        &caller,
                                        operation_id,
                                        &name,
                                        SelectionKind::File,
                                    )
                                    .map(|path| Value::String(path.to_string_lossy().into_owned()))
                            })
                            .collect::<Result<Vec<_>, BackendError>>()?,
                    )
                }
                _ => continue,
            };
            values.insert(name, materialized);
        }
        Ok(values)
    }

    fn recovery(&self) -> Result<RecoveryPayload, BackendError> {
        let caller = self.caller_identity()?;
        Ok(RecoveryPayload {
            journals: self.filesystem.recover_pending(&caller)?,
            print_queue: self.print.latest(&caller)?,
        })
    }

    fn readiness(&self) -> Result<ReadinessReport, BackendError> {
        let ocr = NativeOcr::discover();
        let (ocr_available, ocr_language_data_available) = ocr.support_status();
        let printer_inventory = self.print.inventory();
        let (printer_inventory_available, printer_count) = match printer_inventory {
            Ok(inventory) => (true, inventory.printers.len() as u64),
            Err(_) => (false, 0),
        };
        let recovery = self.recovery()?;
        let recovery_count =
            recovery.journals.len() as u64 + u64::from(recovery.print_queue.is_some());
        Ok(build_readiness(ReadinessFacts {
            ocr_available,
            ocr_language_data_available,
            printer_inventory_available,
            printer_count,
            recovery_count,
        }))
    }

    fn work_items(&self) -> Result<WorkItemQueues, BackendError> {
        work_items::project_with_journals(
            &self.app_data_dir.join("work-items"),
            &self.caller_identity()?.owner_id,
            chrono::Local::now().date_naive(),
            &self.journals,
        )
    }

    pub(crate) fn preview_work_item<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<WorkItemInput> = decode(params)?;
        if request.op_id != work_items::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust work-item backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let grant =
            self.filesystem
                .issue_grant(&scope, &self.app_data_dir, GrantAccess::ReadWrite)?;
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 0,
            total: 1,
            subject: request.values.title.clone(),
        });
        let source_journal = if request.values.source_journal_id.trim().is_empty() {
            None
        } else {
            Some(self.journals.read(&request.values.source_journal_id)?)
        };
        let plan = work_items::preview_event_with_journal(
            &self.app_data_dir.join("work-items"),
            &caller.owner_id,
            request.values,
            &chrono::Utc::now().to_rfc3339(),
            source_journal.as_ref(),
        )?;
        let cached = self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![grant.grant_id],
            ),
            plan,
        )?;
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(cached)
    }

    pub(crate) fn preview_work_item_export<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<WorkItemExportInput> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != work_items::EXPORT_OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust work-item export backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let destination = PathBuf::from(&request.values.destination);
        let grant = self
            .filesystem
            .issue_grant(&scope, &destination, GrantAccess::ReadWrite)?;
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 0,
            total: 1,
            subject: destination.to_string_lossy().into_owned(),
        });
        let plan = work_items::preview_export(
            &self.app_data_dir.join("work-items"),
            &caller.owner_id,
            request.values,
            &chrono::Utc::now().to_rfc3339(),
        )?;
        let cached = self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![grant.grant_id],
            ),
            plan,
        )?;
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(cached)
    }

    pub(crate) fn preview_process_scans<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<ProcessScansValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != "rfi.process_scans" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust Process Scans backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let source_access = process_scans_source_access();
        let source_grant =
            self.filesystem
                .issue_grant(&scope, &request.values.source, source_access)?;
        let destination_grant =
            self.filesystem
                .issue_grant(&scope, &request.values.dest, GrantAccess::ReadWrite)?;
        let plan = preview_process_scans(
            &ProcessScansParams {
                source: request.values.source,
                destination: request.values.dest,
                organize_into_rfi_folders: request.values.organize_into_rfi_folders,
                keep_original: request.values.keep_original,
            },
            &NativeOcr::discover(),
            &mut progress,
            || false,
        )?;
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![source_grant.grant_id, destination_grant.grant_id],
            ),
            plan,
        )?)
    }

    pub(crate) fn preview_rfi_log<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<Map<String, Value>> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != "rfi.log_add" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI Log backend",
            ));
        }
        let workbook = request
            .values
            .get("workbook")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(BackendError::invalid_params)?;
        let root = workbook
            .parent()
            .ok_or_else(|| BackendError::path_invalid("workbook has no parent directory"))?;
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let grant = self
            .filesystem
            .issue_grant(&scope, root, GrantAccess::ReadWrite)?;
        let plan = rfi_log::preview(
            &request.values,
            chrono::Local::now().date_naive(),
            Some(&self.gemini),
            progress,
            || false,
        )?;
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![grant.grant_id],
            ),
            plan,
        )?)
    }

    pub(crate) fn preview_rfi_follow_up<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiFollowUpPreviewValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_follow_up::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI Follow Up backend",
            ));
        }
        let source_root = request
            .values
            .rfi_log_workbook
            .parent()
            .ok_or_else(|| BackendError::path_invalid("RFI Log workbook has no parent"))?
            .to_path_buf();
        let target_root = request
            .values
            .follow_up_workbook
            .parent()
            .ok_or_else(|| BackendError::path_invalid("Follow Up workbook has no parent"))?
            .to_path_buf();
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let provisional = self.filesystem.issue_provisional_grants(
            &scope,
            &[
                (source_root, GrantAccess::Read),
                (target_root, GrantAccess::ReadWrite),
            ],
        )?;
        let source_grant = &provisional.grants()[0];
        let target_grant = &provisional.grants()[1];
        let source_anchor = self.filesystem.anchor_granted_path(
            source_grant,
            &request.values.rfi_log_workbook,
            PathIntent::Read,
        )?;
        let target_anchor = self.filesystem.anchor_granted_path(
            target_grant,
            &request.values.follow_up_workbook,
            PathIntent::Write,
        )?;
        if source_anchor.same_file_as(&target_anchor).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.followUp.workbookInvalid: file check failed: {e}"
            ))
        })? {
            return Err(BackendError::validation(
                "RFI Log and Follow Up workbooks must be different files",
            ));
        }
        let mut source = source_anchor.open_read().map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.followUp.workbookInvalid: cannot open RFI Log: {e}"
            ))
        })?;
        let mut target = target_anchor
            .open_mutable()
            .map_err(|_| BackendError::validation("error.rfi.followUp.workbookLocked"))?;
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 0,
            total: 1,
            subject: request
                .values
                .follow_up_workbook
                .to_string_lossy()
                .into_owned(),
        });
        let mut plan = rfi_follow_up::preview_from_open_files(
            &request.values.rfi_log_workbook,
            &request.values.follow_up_workbook,
            &mut source,
            &mut target,
        )?;
        let reservation = self.filesystem.reserve_plan_id();
        reservation.assign_to_empty(&mut plan)?;
        rfi_follow_up::validate_owned_plan_budget(&plan)?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_reserved_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                grant_ids,
            ),
            reservation,
            plan,
        )?;
        provisional.commit();
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(cached)
    }

    pub(crate) fn preview_rfi_system_sync<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiSystemSyncPreviewValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_system_sync::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI System Sync backend",
            ));
        }
        let system_root = fs::canonicalize(&request.values.rfi_system_root).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.systemRootInvalid: cannot resolve root: {e}"
            ))
        })?;
        let source_path = fs::canonicalize(&request.values.rfi_log_workbook).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.workbookInvalid: cannot resolve RFI Log: {e}"
            ))
        })?;
        let target_path = fs::canonicalize(&request.values.follow_up_workbook).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.workbookInvalid: cannot resolve Follow Up: {e}"
            ))
        })?;
        let source_root = source_path
            .parent()
            .ok_or_else(|| BackendError::path_invalid("RFI Log workbook has no parent"))?
            .to_path_buf();
        let target_root = target_path
            .parent()
            .ok_or_else(|| BackendError::path_invalid("Follow Up workbook has no parent"))?
            .to_path_buf();
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );

        let system_needs_write =
            source_path.starts_with(&system_root) || target_path.starts_with(&system_root);
        let system_access = if system_needs_write {
            GrantAccess::ReadWrite
        } else {
            GrantAccess::Read
        };

        let mut grant_requests: Vec<(PathBuf, GrantAccess)> =
            vec![(system_root.clone(), system_access)];

        let source_covered = system_needs_write && source_path.starts_with(&system_root);
        if !source_covered {
            grant_requests.push((source_root.clone(), GrantAccess::ReadWrite));
        }

        let target_covered = (system_needs_write && target_path.starts_with(&system_root))
            || (!source_covered && target_path.starts_with(&source_root));
        if !target_covered {
            grant_requests.push((target_root.clone(), GrantAccess::ReadWrite));
        }

        let provisional = self
            .filesystem
            .issue_provisional_grants(&scope, &grant_requests)?;

        let source_grant = provisional
            .grants()
            .iter()
            .find(|g| g.access.allows(PathIntent::Write) && source_path.starts_with(&g.root))
            .ok_or_else(|| BackendError::path_invalid("No write grant covering RFI Log"))?;
        let target_grant = provisional
            .grants()
            .iter()
            .find(|g| g.access.allows(PathIntent::Write) && target_path.starts_with(&g.root))
            .ok_or_else(|| BackendError::path_invalid("No write grant covering Follow Up"))?;

        let source_anchor =
            self.filesystem
                .anchor_granted_path(source_grant, &source_path, PathIntent::Write)?;
        let target_anchor =
            self.filesystem
                .anchor_granted_path(target_grant, &target_path, PathIntent::Write)?;
        if source_anchor.same_file_as(&target_anchor).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.workbookInvalid: file check failed: {e}"
            ))
        })? {
            return Err(BackendError::validation(
                "RFI Log and Follow Up workbooks must be different files",
            ));
        }
        let mut source = source_anchor.open_mutable().map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.workbookLocked: cannot open RFI Log: {e}"
            ))
        })?;
        let mut target = target_anchor
            .open_mutable()
            .map_err(|_| BackendError::validation("error.rfi.systemSync.workbookLocked"))?;
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 0,
            total: 1,
            subject: system_root.to_string_lossy().into_owned(),
        });
        let mut plan = rfi_system_sync::preview_system_sync(
            &system_root,
            &source_path,
            &target_path,
            &mut source,
            &mut target,
        )?;
        let reservation = self.filesystem.reserve_plan_id();
        reservation.assign_to_empty(&mut plan)?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_reserved_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                caller.owner_id.clone(),
                caller.session_id.clone(),
                request.op_id,
                grant_ids,
            ),
            reservation,
            plan,
        )?;
        provisional.commit();
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(cached)
    }

    pub(crate) fn preview_outlook_reply_draft<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<OutlookReplyDraftValues> = decode(params)?;
        if request.op_id != outlook_reply_draft::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust Outlook draft backend",
            ));
        }
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 0,
            total: 1,
            subject: String::new(),
        });
        let plan = self
            .outlook
            .preview(&self.caller_identity()?, &request.values.reply_text)?;
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(plan)
    }

    pub(crate) fn preview_stage_signed_pages<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<StageSignedPagesValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != "rfi.stage_signed_first_pages" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust signed-page backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let signed_grant =
            self.filesystem
                .issue_grant(&scope, &request.values.signed_scans, GrantAccess::Read)?;
        let archive_grant =
            self.filesystem
                .issue_grant(&scope, &request.values.archive_root, GrantAccess::Read)?;
        let review_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.review_root,
            GrantAccess::ReadWrite,
        )?;
        let plan = preview_stage_signed_pages(
            &StageSignedPagesParams {
                signed_scans: request.values.signed_scans,
                archive_root: request.values.archive_root,
                review_root: request.values.review_root,
                scan_subfolders: request.values.scan_subfolders,
            },
            &NativeOcr::discover(),
            progress,
            || false,
        )?;
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![
                    signed_grant.grant_id,
                    archive_grant.grant_id,
                    review_grant.grant_id,
                ],
            ),
            plan,
        )?)
    }

    pub(crate) fn preview_batch_print<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<BatchPrintParams> = decode(self.materialize_preview(params)?)?;
        if request.op_id != "files.batch_print" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust print backend",
            ));
        }
        encode(
            self.print
                .preview(&self.caller_identity()?, request.values, progress)?,
        )
    }

    pub(crate) fn revise_native_plan(&self, params: Value) -> Result<Value, BackendError> {
        let request: ReviseParams = decode(params)?;
        let caller = self.caller_identity()?;
        if self.print.contains_plan(&request.plan_id) {
            return encode(
                self.print
                    .revise(&caller, &request.plan_id, &request.rows)?,
            );
        }
        let claimed = self
            .filesystem
            .resolve_native_plan(&request.plan_id, &caller)?;
        let binding = claimed.binding.ok_or_else(BackendError::plan_unavailable)?;
        if claimed.plan.op_id == rfi_follow_up::OPERATION_ID {
            let (source_path, target_path) = rfi_follow_up::plan_workbook_paths(&claimed.plan)?;
            let source_anchor = self.filesystem.anchor_bound_path(
                &binding,
                &caller,
                std::path::Path::new(&source_path),
                PathIntent::Read,
            )?;
            let target_anchor = self.filesystem.anchor_bound_path(
                &binding,
                &caller,
                std::path::Path::new(&target_path),
                PathIntent::Write,
            )?;
            let mut source = source_anchor
                .open_read()
                .map_err(|_| BackendError::validation("error.rfi.followUp.workbookInvalid"))?;
            let mut target = target_anchor
                .open_mutable()
                .map_err(|_| BackendError::validation("error.rfi.followUp.workbookLocked"))?;
            let revised = rfi_follow_up::revise_from_open_files(
                &claimed.plan,
                &request.rows,
                &mut source,
                &mut target,
            )?;
            return encode(self.filesystem.replace_cached_plan(
                &binding,
                &caller,
                &request.plan_id,
                &claimed.digest,
                revised,
            )?);
        }
        let payload = json!({"rows": request.rows});
        let revised = match claimed.plan.op_id.as_str() {
            "rfi.log_add" => rfi_log::revise(&claimed.plan, &payload)?,
            rfi_system_sync::OPERATION_ID => rfi_system_sync::revise(
                &claimed.plan,
                payload["rows"]
                    .as_array()
                    .ok_or_else(BackendError::invalid_params)?,
            )?,
            "rfi.stage_signed_first_pages" => revise_stage_signed_pages(&claimed.plan, &payload)?,
            _ => {
                return Err(BackendError::validation(
                    "native plan does not support revision",
                ));
            }
        };
        encode(
            self.filesystem
                .cache_plan(BackendOrigin::Rust, binding, revised)?,
        )
    }

    pub(crate) fn copy_native_table(&self, params: Value) -> Result<Value, BackendError> {
        let request: TableParams = decode(params)?;
        let claimed = self
            .filesystem
            .resolve_native_plan(&request.plan_id, &self.caller_identity()?)?;
        if claimed.plan.op_id != "rfi.log_add" {
            return Err(BackendError::validation(
                "native plan does not contain an RFI table",
            ));
        }
        let payload = rfi_log::copy_payload(&claimed.plan, request.sheet.as_deref())?;
        clipboard::write(&payload)?;
        Ok(json!({"count": payload.count}))
    }

    pub(crate) fn apply_native_plan<F>(
        &self,
        plan_id: &str,
        operation_id: Option<&str>,
        mut progress: F,
    ) -> Result<(Value, String), BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let caller = self.caller_identity()?;
        if self.outlook.contains_plan(plan_id) {
            let result = self
                .outlook
                .apply(plan_id, operation_id, &caller, &mut progress)?;
            return Ok((
                encode(result)?,
                outlook_reply_draft::OPERATION_ID.to_string(),
            ));
        }
        if self.print.contains_plan(plan_id) {
            if operation_id.is_some_and(|operation_id| operation_id != "files.batch_print") {
                return Err(BackendError::validation(
                    "apply operation does not match the approved plan",
                ));
            }
            return Ok((
                encode(self.print.apply(plan_id, &caller, &mut progress)?)?,
                "files.batch_print".to_string(),
            ));
        }
        let approved_op_id =
            self.filesystem
                .validate_plan_operation(plan_id, &caller, operation_id)?;
        let result = encode(self.filesystem.apply_with_progress(
            plan_id,
            &caller,
            &mut progress,
        )?)?;
        Ok((result, approved_op_id))
    }

    pub(crate) fn cancel_native_plan(&self, plan_id: &str) -> bool {
        self.outlook.cancel(plan_id)
            || self.print.cancel(plan_id)
            || self.filesystem.cancel(plan_id)
    }

    fn review_scan(&self, params: Value) -> Result<Value, BackendError> {
        let request: ReviewScanParams = decode(params)?;
        let caller = self.caller_identity()?;
        let destination = self.selections.resolve_scope(
            &request.destination.selection_id,
            &caller,
            SelectionScope::ReviewDestination,
        )?;
        let items = scan_review(&destination, &NativeOcr::discover())?
            .into_iter()
            .map(|item| {
                let selection = self.selections.issue(
                    &caller,
                    "rfi.review",
                    "item",
                    std::path::Path::new(&item.path),
                    SelectionKind::File,
                )?;
                Ok(FrontendReviewItem {
                    selection_id: selection.selection_id,
                    name: item.name,
                    path: item.path,
                    code: item.code,
                    target_exists: item.target_exists,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        encode(items)
    }

    fn review_preview(&self, params: Value) -> Result<Value, BackendError> {
        let request: ReviewPreviewParams = decode(params)?;
        let caller = self.caller_identity()?;
        let destination = self.selections.resolve_scope(
            &request.destination.selection_id,
            &caller,
            SelectionScope::ReviewDestination,
        )?;
        let decisions = request
            .decisions
            .into_iter()
            .map(|decision| {
                Ok(NativeReviewDecision {
                    path: self.selections.resolve_scope(
                        &decision.item_id,
                        &caller,
                        SelectionScope::ReviewItem,
                    )?,
                    action: decision.action,
                    code: decision.code,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            "rfi.review",
        );
        let destination_grant =
            self.filesystem
                .issue_grant(&scope, &destination, GrantAccess::ReadWrite)?;
        let app_data_grant =
            self.filesystem
                .issue_grant(&scope, &self.app_data_dir, GrantAccess::ReadWrite)?;
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%6f").to_string();
        let plan = preview_review(
            &destination,
            &self.app_data_dir.join("trash"),
            &decisions,
            &timestamp,
        )?;
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                "rfi.review",
                vec![destination_grant.grant_id, app_data_grant.grant_id],
            ),
            plan,
        )?)
    }

    pub(crate) fn try_native_undo(&self, params: &Value) -> Result<Option<Value>, BackendError> {
        let object = params
            .as_object()
            .ok_or_else(BackendError::invalid_params)?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "journalId" | "cloudId" | "destination"))
        {
            return Err(BackendError::invalid_params());
        }
        let journal_id = object
            .get("journalId")
            .and_then(Value::as_str)
            .ok_or_else(BackendError::invalid_params)?;
        let operation_id = self.journals.inspect_operation(journal_id)?;
        if operation_id == outlook_reply_draft::OPERATION_ID && object.contains_key("destination") {
            return Err(BackendError::validation(
                "Outlook draft undo does not accept a destination",
            ));
        }
        if operation_id == outlook_reply_draft::OPERATION_ID {
            let cloud_id = object
                .get("cloudId")
                .and_then(Value::as_str)
                .map(str::to_string);
            let result = self.outlook.undo(journal_id, &self.caller_identity()?)?;
            if result.failed == 0
                && let Some(cloud_id) = cloud_id
            {
                self.mark_undone(&cloud_id);
            }
            return encode(result).map(Some);
        }
        if operation_id == rfi_follow_up::OPERATION_ID && object.contains_key("destination") {
            return Err(BackendError::validation(
                "RFI Follow Up undo does not accept a destination",
            ));
        }
        let caller = self.caller_identity()?;
        if operation_id == rfi_follow_up::OPERATION_ID {
            let cloud_id = object
                .get("cloudId")
                .and_then(Value::as_str)
                .map(str::to_string);
            let result = self.filesystem.undo(journal_id, &caller, None)?;
            if result.failed == 0
                && let Some(cloud_id) = cloud_id
            {
                self.mark_undone(&cloud_id);
            }
            return encode(result).map(Some);
        }
        let record = self.journals.read(journal_id)?;
        if record.steps.iter().any(|step| step.kind == StepKind::Print) {
            return Err(BackendError::validation(
                "print audit journals are not undoable",
            ));
        }
        if record.steps.iter().any(|step| {
            !matches!(
                step.kind,
                StepKind::Mkdir
                    | StepKind::Move
                    | StepKind::Copy
                    | StepKind::Rename
                    | StepKind::Write
                    | StepKind::RestoreBackup
            )
        }) {
            return Ok(None);
        }
        let destination = object
            .get("destination")
            .cloned()
            .map(|value| {
                let selection: PathSelection = decode(value)?;
                self.selections.resolve_scope(
                    &selection.selection_id,
                    &caller,
                    SelectionScope::UndoDestination,
                )
            })
            .transpose()?;
        let cloud_id = object
            .get("cloudId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let result = self
            .filesystem
            .undo(journal_id, &caller, destination.as_deref())?;
        if result.failed == 0
            && let Some(cloud_id) = cloud_id
        {
            self.mark_undone(&cloud_id);
        }
        encode(result).map(Some)
    }

    fn caller_identity(&self) -> Result<CallerIdentity, BackendError> {
        let session_id = self
            .process_session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        authenticated_caller_identity(&self.auth.state(), &session_id)
    }

    fn signed_in_caller_identity(&self) -> Result<CallerIdentity, BackendError> {
        let session_id = self
            .process_session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        signed_in_caller_identity(&self.auth.state(), &session_id)
    }

    fn revoke_session_authority(&self, caller: &CallerIdentity) {
        self.filesystem.revoke(caller);
        self.selections.revoke(caller);
        self.print.revoke(caller);
        self.outlook.revoke(caller);
    }

    fn rotate_session_authority(&self) {
        *self
            .process_session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = uuid::Uuid::new_v4().to_string();
    }

    fn registry_facts(&self) -> RegistryFacts {
        self.print.inventory().map_or_else(
            |_| RegistryFacts::default(),
            |inventory| RegistryFacts {
                printers: inventory.printers,
                default_printer: inventory.default_printer,
                ..RegistryFacts::default()
            },
        )
    }

    fn update_settings(&self, params: Value) -> Result<Value, BackendError> {
        let request: SettingsUpdate = decode(params)?;
        let mut preferences = self.settings.preferences();
        if let PatchField::Value(locale) = request.locale {
            preferences.locale = locale;
        }
        if let Some(theme) = request.theme {
            preferences.theme = theme;
        }
        if let Some(font) = request.font {
            preferences.font = font;
        }
        if let Some(font_scale) = request.font_scale {
            preferences.font_scale = font_scale;
        }
        if let Some(compact) = request.compact {
            preferences.compact = compact;
        }
        self.settings
            .save_preferences(preferences)
            .map_err(|_| BackendError::settings_write())?;
        encode(self.settings.preferences())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathSelectionMode {
    Folder,
    File,
    Files,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryPayload {
    journals: Vec<crate::journal::RecoveryEntry>,
    print_queue: Option<PrintQueueSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrontendReviewItem {
    selection_id: String,
    name: String,
    path: String,
    code: String,
    target_exists: bool,
}

fn path_parameter_kind(operation_id: &str, parameter_name: &str) -> Option<ParamKind> {
    if operation_id == "rfi.review" && parameter_name == "destination"
        || operation_id == "history.undo" && parameter_name == "destination"
    {
        return Some(ParamKind::Folder);
    }
    operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == operation_id)
        .and_then(|operation| {
            operation
                .schema
                .params
                .into_iter()
                .find(|parameter| parameter.name == parameter_name)
        })
        .map(|parameter| parameter.kind)
}

fn operation_path_parameters(operation_id: &str) -> Result<Vec<(String, ParamKind)>, BackendError> {
    let operation = operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == operation_id)
        .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
    Ok(operation
        .schema
        .params
        .into_iter()
        .filter(|parameter| {
            matches!(
                parameter.kind,
                ParamKind::Folder | ParamKind::File | ParamKind::Files
            )
        })
        .map(|parameter| (parameter.name, parameter.kind))
        .collect())
}

#[cfg(test)]
fn filter_saved_operation_params(
    operation_id: &str,
    mut values: Map<String, Value>,
) -> Result<Map<String, Value>, BackendError> {
    let operation = operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == operation_id)
        .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
    let allowed = operation
        .schema
        .params
        .into_iter()
        .filter(|parameter| {
            parameter.remember && !matches!(parameter.kind, ParamKind::Secret | ParamKind::LongText)
        })
        .map(|parameter| parameter.name)
        .collect::<HashSet<_>>();
    values.retain(|name, _| allowed.contains(name));
    Ok(values)
}

fn preset_store_error(error: PresetStoreError) -> BackendError {
    match error {
        PresetStoreError::InvalidName => BackendError::validation("error.preset.nameInvalid"),
        PresetStoreError::InvalidValue => BackendError::validation("preset value is invalid"),
        PresetStoreError::DuplicateName => BackendError::validation("error.preset.duplicate"),
        PresetStoreError::LimitReached => BackendError::validation("error.preset.limit"),
        PresetStoreError::Missing => BackendError::validation("error.preset.missing"),
        PresetStoreError::Write(_) => BackendError::settings_write(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationIdParams {
    op_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveParams {
    op_id: String,
    values: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavePresetParams {
    op_id: String,
    name: String,
    values: JsonObject,
    #[serde(default)]
    replace: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeletePresetParams {
    op_id: String,
    preset_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignInParams {
    identifier: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignUpParams {
    email: String,
    password: String,
    username: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UsernameParams {
    username: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetFlagsParams {
    user_id: String,
    #[serde(default)]
    allowed: Option<bool>,
    #[serde(default)]
    is_admin: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryParams {
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QueueIdParams {
    queue_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreviewEnvelope<T> {
    op_id: String,
    values: T,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessScansValues {
    source: std::path::PathBuf,
    dest: std::path::PathBuf,
    #[serde(default = "default_true")]
    organize_into_rfi_folders: bool,
    #[serde(default)]
    keep_original: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RfiFollowUpPreviewValues {
    #[serde(rename = "rfi_log_workbook")]
    rfi_log_workbook: PathBuf,
    #[serde(rename = "follow_up_workbook")]
    follow_up_workbook: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RfiSystemSyncPreviewValues {
    #[serde(rename = "rfi_system_root")]
    rfi_system_root: PathBuf,
    #[serde(rename = "rfi_log_workbook")]
    rfi_log_workbook: PathBuf,
    #[serde(rename = "follow_up_workbook")]
    follow_up_workbook: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutlookReplyDraftValues {
    reply_text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StageSignedPagesValues {
    signed_scans: PathBuf,
    archive_root: PathBuf,
    review_root: PathBuf,
    #[serde(default)]
    scan_subfolders: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviseParams {
    plan_id: String,
    rows: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TableParams {
    plan_id: String,
    #[serde(default)]
    sheet: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewScanParams {
    destination: PathSelection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewPreviewParams {
    destination: PathSelection,
    decisions: Vec<FrontendReviewDecision>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FrontendReviewDecision {
    item_id: String,
    action: crate::review::ReviewAction,
    code: String,
}

const fn default_true() -> bool {
    true
}

const fn process_scans_source_access() -> GrantAccess {
    // Move mode removes every classified or review-bound source; copy mode
    // preserves every source, including unreadable and low-confidence files.
    GrantAccess::ReadWrite
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingsUpdate {
    #[serde(default)]
    locale: PatchField<Option<String>>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    font: Option<String>,
    #[serde(default)]
    font_scale: Option<f64>,
    #[serde(default)]
    compact: Option<bool>,
}

#[derive(Debug, Default, PartialEq)]
enum PatchField<T> {
    #[default]
    Missing,
    Value(T),
}

impl<'de, T> Deserialize<'de> for PatchField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Self::Value)
    }
}

fn decode<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, BackendError> {
    serde_json::from_value(params).map_err(|_| BackendError::invalid_params())
}

fn decode_empty(params: Value) -> Result<(), BackendError> {
    decode::<EmptyParams>(params).map(|_| ())
}

fn encode(value: impl serde::Serialize) -> Result<Value, BackendError> {
    serde_json::to_value(value).map_err(|_| BackendError::protocol("response encoding failed"))
}

fn validate_operation_id(op_id: &str) -> Result<(), BackendError> {
    if op_id.is_empty() || op_id.chars().count() > MAX_OPERATION_ID_CHARS {
        return Err(BackendError::validation("operation ID is invalid"));
    }
    Ok(())
}

fn configured_gemini_models() -> Vec<String> {
    let configured = env::var("DOCFLOW_GEMINI_MODEL").unwrap_or_default();
    let mut models = [configured.as_str(), DEFAULT_MODEL]
        .into_iter()
        .chain(FALLBACK_MODELS.iter().copied())
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    models.retain(|model| seen.insert(model.clone()));
    models
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn authenticated_caller_identity(
    state: &crate::contracts::AuthState,
    session_id: &str,
) -> Result<CallerIdentity, BackendError> {
    let caller = signed_in_caller_identity(state, session_id)?;
    if !state.allowed {
        return Err(BackendError::permission_denied());
    }
    Ok(caller)
}

fn signed_in_caller_identity(
    state: &crate::contracts::AuthState,
    session_id: &str,
) -> Result<CallerIdentity, BackendError> {
    if !state.signed_in {
        return Err(BackendError::auth_required());
    }
    let owner_id = state
        .session
        .as_ref()
        .map(|session| session.user_id.as_str())
        .filter(|owner_id| !owner_id.is_empty() && *owner_id != "anonymous")
        .ok_or_else(BackendError::auth_required)?;
    if session_id.is_empty() {
        return Err(BackendError::auth_required());
    }
    Ok(CallerIdentity::new(owner_id, session_id))
}

fn cloud_result_summary(applied: u64, journal_step_count: usize) -> String {
    format!("{applied}/{journal_step_count}")
}

fn sync_result_metadata<L, P>(result: &mut Value, mut load_steps: L, mut push: P)
where
    L: FnMut(&str) -> Option<usize>,
    P: FnMut(u64, u64, &str, &str) -> bool,
{
    let Some(payload) = result.as_object_mut() else {
        return;
    };
    let applied = payload
        .get("applied")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let failed = payload
        .get("failed")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let journal_id = payload
        .get("journalId")
        .and_then(Value::as_str)
        .or_else(|| {
            payload
                .get("meta")
                .and_then(Value::as_object)
                .and_then(|meta| meta.get("auditJournalId"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string();
    let Some(step_count) = load_steps(&journal_id) else {
        payload.insert("cloudSynced".to_string(), Value::Bool(false));
        return;
    };
    let summary = cloud_result_summary(applied, step_count);
    let synced = push(applied, failed, &journal_id, &summary);
    payload.insert("cloudSynced".to_string(), Value::Bool(synced));
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::{
        credentials::{CredentialError, CredentialStore, StoredSession},
        http::{HttpError, HttpResponse},
    };
    use serde_json::json;

    use super::*;

    struct TestCredentials(Mutex<Option<StoredSession>>);

    impl CredentialStore for TestCredentials {
        fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn write(&self, session: &StoredSession) -> Result<(), CredentialError> {
            *self.0.lock().unwrap() = Some(session.clone());
            Ok(())
        }

        fn delete(&self) -> Result<(), CredentialError> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    struct TestHttp(Mutex<VecDeque<Result<HttpResponse, HttpError>>>);

    impl JsonHttp for TestHttp {
        fn send_json(
            &self,
            _method: &str,
            _url: &str,
            _headers: &[(String, String)],
            _body: Option<&Value>,
        ) -> Result<HttpResponse, HttpError> {
            self.0
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(HttpError::InvalidResponse))
        }
    }

    fn auth_responses(owner_id: &str) -> [Result<HttpResponse, HttpError>; 2] {
        auth_responses_with_allowed(owner_id, true)
    }

    fn auth_responses_with_allowed(
        owner_id: &str,
        allowed: bool,
    ) -> [Result<HttpResponse, HttpError>; 2] {
        [
            Ok(HttpResponse::from_json(
                200,
                &json!({
                    "access_token": "access-secret",
                    "refresh_token": "refresh-secret",
                    "expires_at": i64::MAX,
                    "user": {"id": owner_id, "email": "user@example.com"}
                }),
            )),
            Ok(HttpResponse::from_json(
                200,
                &json!([{
                    "id": owner_id,
                    "email": "user@example.com",
                    "allowed": allowed,
                    "is_admin": false
                }]),
            )),
        ]
    }

    fn test_backend(temporary: &tempfile::TempDir) -> NativeBackend {
        let responses = auth_responses("owner")
            .into_iter()
            .chain(auth_responses("owner"))
            .collect();
        test_backend_with_responses(temporary, responses)
    }

    fn test_backend_with_responses(
        temporary: &tempfile::TempDir,
        responses: VecDeque<Result<HttpResponse, HttpError>>,
    ) -> NativeBackend {
        let settings =
            Arc::new(SettingsStore::new(temporary.path().join("settings.json")).unwrap());
        let http: Arc<dyn JsonHttp> = Arc::new(TestHttp(Mutex::new(responses)));
        let config = AuthConfig::new("http://supabase.test", "publishable");
        let auth = Arc::new(AuthService::new(
            settings.clone(),
            Arc::new(TestCredentials(Mutex::new(None))),
            http.clone(),
            config.clone(),
        ));
        auth.sign_in("user@example.com", "password", now()).unwrap();
        let journal_dir = temporary.path().join("journal");
        let mutation_lock = GlobalMutationLock::default();
        NativeBackend {
            settings,
            auth: auth.clone(),
            cloud: Arc::new(CloudHistory::new(
                auth,
                http.clone(),
                config,
                journal_dir.clone(),
            )),
            gemini: Arc::new(GeminiClient::new(
                http,
                "http://gemini.test",
                vec![DEFAULT_MODEL.to_string()],
            )),
            journals: JournalStore::new(&journal_dir),
            filesystem: FilesystemAuthority::with_mutation_lock(
                &journal_dir,
                Duration::from_secs(60),
                mutation_lock.clone(),
            ),
            selections: PathSelectionStore::new(Duration::from_secs(60)),
            print: PrintService::new(
                temporary.path().join("print-queues"),
                &journal_dir,
                Duration::from_secs(60),
                mutation_lock.clone(),
                platform::production_print_adapter(),
            ),
            outlook: OutlookDraftService::new(
                &journal_dir,
                Duration::from_secs(60),
                mutation_lock,
                platform::production_outlook_adapter(),
            ),
            process_session_id: Mutex::new("epoch-initial".to_string()),
            app_data_dir: temporary.path().to_path_buf(),
        }
    }

    fn signed_in_state(owner_id: &str) -> crate::contracts::AuthState {
        crate::contracts::AuthState {
            signed_in: true,
            allowed: true,
            is_admin: false,
            session: Some(crate::contracts::AuthSessionView {
                user_id: owner_id.to_string(),
                email: "user@example.com".to_string(),
                expires_at: i64::MAX,
            }),
            user: None,
        }
    }

    #[test]
    fn work_item_preview_apply_list_and_undo_use_the_native_journal_boundary() {
        let temporary = tempfile::tempdir().unwrap();
        let backend = test_backend(&temporary);
        let plan = backend
            .preview_work_item(
                json!({
                    "opId": work_items::OPERATION_ID,
                    "values": {
                        "action": "create",
                        "itemId": "",
                        "title": "MEC inspection request",
                        "rfiCodes": "AZE-SUN-INT-QMS-RFI-MEC-0249",
                        "stage": "request_captured",
                        "dueDate": "",
                        "needsReview": false,
                        "reason": ""
                    }
                }),
                |_| {},
            )
            .unwrap();
        let plan_id = plan["planId"].as_str().unwrap();
        let (result, operation_id) = backend
            .apply_native_plan(plan_id, Some(work_items::OPERATION_ID), |_| {})
            .unwrap();
        assert_eq!(operation_id, work_items::OPERATION_ID);
        assert_eq!(backend.work_items().unwrap().resume.len(), 1);

        let undone = backend
            .try_native_undo(&json!({"journalId": result["journalId"]}))
            .unwrap();
        assert!(undone.is_some());
        assert!(backend.work_items().unwrap().resume.is_empty());
    }

    #[test]
    fn work_item_audit_export_uses_selected_folder_journal_and_undo() {
        let temporary = tempfile::tempdir().unwrap();
        let backend = test_backend(&temporary);
        let export_root = temporary.path().join("audit-export");
        fs::create_dir(&export_root).unwrap();
        let selection = backend
            .register_selected_paths(
                work_items::EXPORT_OPERATION_ID,
                "destination",
                vec![export_root.clone()],
            )
            .unwrap()
            .remove(0);
        let plan = backend
            .preview_work_item_export(
                json!({
                    "opId": work_items::EXPORT_OPERATION_ID,
                    "values": {"destination": selection}
                }),
                |_| {},
            )
            .unwrap();
        let plan_id = plan["planId"].as_str().unwrap();
        let (result, operation_id) = backend
            .apply_native_plan(plan_id, Some(work_items::EXPORT_OPERATION_ID), |_| {})
            .unwrap();
        assert_eq!(operation_id, work_items::EXPORT_OPERATION_ID);
        assert_eq!(fs::read_dir(&export_root).unwrap().count(), 2);

        backend
            .try_native_undo(&json!({"journalId": result["journalId"]}))
            .unwrap()
            .expect("export should be undoable");
        assert_eq!(fs::read_dir(&export_root).unwrap().count(), 0);
    }

    #[test]
    fn caller_identity_never_falls_back_to_an_authorized_anonymous_identity() {
        let signed_out = crate::contracts::AuthState {
            signed_in: false,
            allowed: false,
            is_admin: false,
            session: None,
            user: None,
        };
        assert_eq!(
            authenticated_caller_identity(&signed_out, "session")
                .unwrap_err()
                .code,
            "auth_required"
        );
        assert_eq!(
            authenticated_caller_identity(&signed_in_state("owner"), "session").unwrap(),
            CallerIdentity::new("owner", "session")
        );
    }

    #[test]
    fn follow_up_preview_values_accept_only_exact_snake_case_keys() {
        let exact: RfiFollowUpPreviewValues = decode(json!({
            "rfi_log_workbook": "/source.xlsx",
            "follow_up_workbook": "/target.xlsx"
        }))
        .unwrap();
        assert_eq!(exact.rfi_log_workbook, PathBuf::from("/source.xlsx"));
        assert_eq!(exact.follow_up_workbook, PathBuf::from("/target.xlsx"));
        assert!(
            decode::<RfiFollowUpPreviewValues>(json!({
                "rfiLogWorkbook": "/source.xlsx",
                "followUpWorkbook": "/target.xlsx"
            }))
            .is_err()
        );
        assert!(
            decode::<RfiFollowUpPreviewValues>(json!({
                "rfi_log_workbook": "/source.xlsx",
                "follow_up_workbook": "/target.xlsx",
                "unknown": true
            }))
            .is_err()
        );
    }

    #[test]
    fn sign_out_and_re_sign_in_revoke_old_plan_and_selection_ids() {
        let temporary = tempfile::tempdir().unwrap();
        let old = CallerIdentity::new("owner", "session-old");
        let fresh = CallerIdentity::new("owner", "session-fresh");
        let selections = PathSelectionStore::new(Duration::from_secs(60));
        let selected = selections
            .issue(
                &old,
                "rfi.review",
                "destination",
                temporary.path(),
                SelectionKind::Folder,
            )
            .unwrap();
        let plans = crate::plan_store::PlanStore::new(Duration::from_secs(60));
        let plan = plans
            .insert_bound(
                BackendOrigin::Rust,
                PlanBinding::new("owner", "session-old", "rfi.review", Vec::new()),
                crate::contracts::Plan {
                    plan_id: "backend-plan".to_string(),
                    op_id: "rfi.review".to_string(),
                    steps: Vec::new(),
                    warnings: Vec::new(),
                    meta: Map::new(),
                },
            )
            .unwrap();

        selections.revoke(&old);
        plans.revoke(&old);

        assert_eq!(
            selections
                .resolve_scope(
                    &selected.selection_id,
                    &fresh,
                    SelectionScope::ReviewDestination,
                )
                .unwrap_err()
                .code,
            "path_grant_not_found_or_expired"
        );
        assert_eq!(
            plans.resolve_bound(&plan.plan_id, &fresh).unwrap_err().code,
            "plan_not_found_or_expired"
        );
    }

    #[test]
    fn attempted_and_same_uid_sign_in_replacements_rotate_and_revoke_capabilities() {
        let temporary = tempfile::tempdir().unwrap();
        let backend = test_backend(&temporary);
        let initial = backend.caller_identity().unwrap();
        let selected = backend
            .selections
            .issue(
                &initial,
                "rfi.review",
                "destination",
                temporary.path(),
                SelectionKind::Folder,
            )
            .unwrap();
        let grant = backend
            .filesystem
            .issue_grant(
                &GrantScope::new(&initial.owner_id, &initial.session_id, "request", "op.test"),
                temporary.path(),
                GrantAccess::ReadWrite,
            )
            .unwrap();
        let plan = backend
            .filesystem
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new(
                    &initial.owner_id,
                    &initial.session_id,
                    "op.test",
                    vec![grant.grant_id],
                ),
                crate::contracts::Plan {
                    plan_id: "ignored".to_string(),
                    op_id: "op.test".to_string(),
                    steps: Vec::new(),
                    warnings: Vec::new(),
                    meta: Map::new(),
                },
            )
            .unwrap();

        assert_eq!(
            backend
                .dispatch(
                    BackendMethod::AuthSignIn,
                    json!({"identifier": "not-an-email", "password": "password"}),
                )
                .unwrap_err()
                .code,
            "auth_failed"
        );
        let after_attempt = backend.caller_identity().unwrap();
        assert_ne!(after_attempt.session_id, initial.session_id);
        assert_eq!(
            backend
                .selections
                .resolve_scope(
                    &selected.selection_id,
                    &after_attempt,
                    SelectionScope::ReviewDestination,
                )
                .unwrap_err()
                .code,
            "path_grant_not_found_or_expired"
        );
        assert_eq!(
            backend
                .filesystem
                .apply(&plan.plan_id, &after_attempt)
                .unwrap_err()
                .code,
            "plan_not_found_or_expired"
        );

        let selected_after_attempt = backend
            .selections
            .issue(
                &after_attempt,
                "rfi.review",
                "destination",
                temporary.path(),
                SelectionKind::Folder,
            )
            .unwrap();
        let grant_after_attempt = backend
            .filesystem
            .issue_grant(
                &GrantScope::new(
                    &after_attempt.owner_id,
                    &after_attempt.session_id,
                    "request-after-attempt",
                    "op.test",
                ),
                temporary.path(),
                GrantAccess::ReadWrite,
            )
            .unwrap();
        let plan_after_attempt = backend
            .filesystem
            .cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new(
                    &after_attempt.owner_id,
                    &after_attempt.session_id,
                    "op.test",
                    vec![grant_after_attempt.grant_id],
                ),
                crate::contracts::Plan {
                    plan_id: "ignored-again".to_string(),
                    op_id: "op.test".to_string(),
                    steps: Vec::new(),
                    warnings: Vec::new(),
                    meta: Map::new(),
                },
            )
            .unwrap();

        backend
            .dispatch(
                BackendMethod::AuthSignIn,
                json!({"identifier": "user@example.com", "password": "password"}),
            )
            .unwrap();
        let after_success = backend.caller_identity().unwrap();
        assert_eq!(after_success.owner_id, initial.owner_id);
        assert_ne!(after_success.session_id, after_attempt.session_id);
        assert_eq!(
            backend
                .selections
                .resolve_scope(
                    &selected_after_attempt.selection_id,
                    &after_success,
                    SelectionScope::ReviewDestination,
                )
                .unwrap_err()
                .code,
            "path_grant_not_found_or_expired"
        );
        assert_eq!(
            backend
                .filesystem
                .apply(&plan_after_attempt.plan_id, &after_success)
                .unwrap_err()
                .code,
            "plan_not_found_or_expired"
        );
    }

    #[test]
    fn disallowed_same_uid_reauthentication_cannot_revive_old_capabilities() {
        let temporary = tempfile::tempdir().unwrap();
        let responses = auth_responses("owner")
            .into_iter()
            .chain([Ok(HttpResponse::from_json(
                200,
                &json!([{
                    "id": "owner",
                    "email": "user@example.com",
                    "allowed": false,
                    "is_admin": false
                }]),
            ))])
            .chain(auth_responses("owner"))
            .collect();
        let backend = test_backend_with_responses(&temporary, responses);
        let initial = backend.caller_identity().unwrap();
        let selected = backend
            .selections
            .issue(
                &initial,
                "rfi.review",
                "destination",
                temporary.path(),
                SelectionKind::Folder,
            )
            .unwrap();

        let denied = backend.auth.load_me(now()).unwrap();
        assert!(denied.signed_in);
        assert!(!denied.allowed);
        backend
            .dispatch(
                BackendMethod::AuthSignIn,
                json!({"identifier": "user@example.com", "password": "password"}),
            )
            .unwrap();

        let replacement = backend.caller_identity().unwrap();
        assert_ne!(replacement.session_id, initial.session_id);
        assert_eq!(
            backend
                .selections
                .resolve_scope(
                    &selected.selection_id,
                    &replacement,
                    SelectionScope::ReviewDestination,
                )
                .unwrap_err()
                .code,
            "path_grant_not_found_or_expired"
        );
    }

    #[test]
    fn dispatch_sign_out_stays_revoked_when_settings_cleanup_fails() {
        let temporary = tempfile::tempdir().unwrap();
        let backend = test_backend(&temporary);
        let initial = backend.caller_identity().unwrap();
        let initial_epoch = initial.session_id.clone();
        let selected = backend
            .selections
            .issue(
                &initial,
                "rfi.review",
                "destination",
                temporary.path(),
                SelectionKind::Folder,
            )
            .unwrap();

        let settings_path = temporary.path().join("settings.json");
        std::fs::remove_file(&settings_path).unwrap();
        std::fs::create_dir(&settings_path).unwrap();

        assert_eq!(
            backend
                .dispatch(BackendMethod::AuthSignOut, json!({}))
                .unwrap_err()
                .code,
            "settings_write_failed"
        );
        assert!(!backend.auth_state().signed_in);
        assert_ne!(
            *backend
                .process_session_id
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            initial_epoch
        );
        assert_eq!(
            backend
                .selections
                .resolve_scope(
                    &selected.selection_id,
                    &initial,
                    SelectionScope::ReviewDestination,
                )
                .unwrap_err()
                .code,
            "path_grant_not_found_or_expired"
        );
    }

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

        let work_item = filter_saved_operation_params(
            "rfi.work_item_update",
            serde_json::from_value(json!({
                "action": "transition",
                "item_id": "work-1",
                "stage": "checked",
                "due_date": "2026-09-14",
                "needs_review": false
            }))
            .unwrap(),
        )
        .expect("work-item form values should be recognized but transient");
        assert!(work_item.is_empty());
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

        let persisted: Value =
            serde_json::from_slice(&std::fs::read(settings_path).unwrap()).unwrap();
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
            "source": source_dir.to_str().unwrap(),
            "dest": dest_dir.to_str().unwrap(),
            "organize_into_rfi_folders": false,
            "keep_original": true,
            "secret_key": "forbidden",
            "unknown": "drop-me"
        }))
        .unwrap();

        let filtered = filter_saved_operation_params("rfi.process_scans", params)
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
            .save_params("rfi.process_scans", filtered)
            .unwrap();

        let restored = backend.selected_params("rfi.process_scans").unwrap();
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
                "rfi.process_scans",
                serde_json::from_value(json!({
                    "source": "/non/existent/path/never/created",
                    "dest": dest_dir.to_str().unwrap(),
                    "keep_original": true
                }))
                .unwrap(),
            )
            .unwrap();
        let restored2 = backend.selected_params("rfi.process_scans").unwrap();
        assert!(!restored2.contains_key("source"));
        assert!(restored2.contains_key("dest"));
        assert_eq!(restored2.get("keep_original"), Some(&json!(true)));
    }

    #[test]
    fn process_scans_source_grant_allows_review_moves_even_in_copy_mode() {
        assert_eq!(process_scans_source_access(), GrantAccess::ReadWrite);
    }

    #[test]
    fn cloud_summary_preserves_legacy_journal_step_count() {
        assert_eq!(cloud_result_summary(1, 3), "1/3");
    }

    #[test]
    fn batch_print_audit_result_finds_and_pushes_the_nested_journal_id() {
        let mut result = json!({
            "applied": 2,
            "failed": 1,
            "meta": {"auditJournalId": "print-audit-1"}
        });
        let mut pushed = None;

        sync_result_metadata(
            &mut result,
            |journal_id| (journal_id == "print-audit-1").then_some(3),
            |applied, failed, journal_id, summary| {
                pushed = Some((applied, failed, journal_id.to_string(), summary.to_string()));
                true
            },
        );

        assert_eq!(
            pushed,
            Some((2, 1, "print-audit-1".to_string(), "2/3".to_string()))
        );
        assert_eq!(result["cloudSynced"], true);
    }

    #[test]
    fn preview_rfi_system_sync_with_nested_workbooks_succeeds_and_applies() {
        use std::io::{Cursor, Write};
        use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

        fn create_xlsx(sheets: &[(&str, &str)]) -> Vec<u8> {
            let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let mut workbook_sheets = String::new();
            let mut relationships = String::new();
            for (index, (name, content)) in sheets.iter().enumerate() {
                let ordinal = index + 1;
                workbook_sheets.push_str(&format!(
                    r#"<sheet name="{name}" sheetId="{ordinal}" r:id="rId{ordinal}"/>"#
                ));
                relationships.push_str(&format!(
                    r#"<Relationship Id="rId{ordinal}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{ordinal}.xml"/>"#
                ));
                writer
                    .start_file(format!("xl/worksheets/sheet{ordinal}.xml"), options)
                    .unwrap();
                let ws = format!(
                    r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{content}</sheetData></worksheet>"#
                );
                writer.write_all(ws.as_bytes()).unwrap();
            }
            let pkg_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
            let wb = format!(
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{workbook_sheets}</sheets></workbook>"#
            );
            let rels = format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}</Relationships>"#
            );
            for (name, bytes) in [
                ("[Content_Types].xml", b"content-types".as_slice()),
                ("_rels/.rels", pkg_rels.as_bytes()),
                ("xl/workbook.xml", wb.as_bytes()),
                ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
            ] {
                writer.start_file(name, options).unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap().into_inner()
        }

        let temporary = tempfile::tempdir().unwrap();
        let backend = test_backend(&temporary);
        let temp_path = fs::canonicalize(temporary.path()).unwrap();
        let system_root = temp_path.join("RFI_SYSTEM");
        let civ_dir = system_root.join("CIV");
        fs::create_dir_all(&civ_dir).unwrap();

        let civ_103_dir = civ_dir.join("AZE-SUN-INT-QMS-RFI-CIV-0103");
        fs::create_dir_all(&civ_103_dir).unwrap();
        let civ_103_pdf = civ_103_dir.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0103.pdf");
        {
            let stream = "BT /F1 18 Tf 460 100 Td (E.Mammadov Signed Worley) Tj ET\n";
            let objects = [
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
                format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
            ];
            let mut bytes = b"%PDF-1.4\n".to_vec();
            let mut offsets = Vec::new();
            for (index, object) in objects.iter().enumerate() {
                offsets.push(bytes.len());
                bytes.extend_from_slice(
                    format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes(),
                );
            }
            let xref = bytes.len();
            bytes.extend_from_slice(
                format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
            );
            for offset in offsets {
                bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            }
            bytes.extend_from_slice(
                format!(
                    "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                    objects.len() + 1
                )
                .as_bytes(),
            );
            fs::write(&civ_103_pdf, bytes).unwrap();
        }

        // Place workbooks INSIDE system_root to test nested grant handling
        let rfi_log_path = system_root.join("RFI_Log.xlsx");
        let follow_up_path = system_root.join("Follow_Up.xlsx");

        let civ_log_row = r#"<row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="V9" t="inlineStr"><is><t>REMARKS</t></is></c></row><row r="10"><c r="A10" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0103</t></is></c><c r="V10" t="inlineStr"><is><t></t></is></c></row>"#;
        let mec_log_header = r#"<row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="W9" t="inlineStr"><is><t>REMARKS</t></is></c></row>"#;
        let ele_log_header = r#"<row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="V9" t="inlineStr"><is><t>REMARKS</t></is></c></row>"#;
        let prc_log_header = r#"<row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="W9" t="inlineStr"><is><t>REMARKS</t></is></c></row>"#;
        let rfi_log_bytes = create_xlsx(&[
            ("CIV", civ_log_row),
            ("MEC", mec_log_header),
            ("ELE", ele_log_header),
            ("PRC", prc_log_header),
        ]);
        fs::write(&rfi_log_path, &rfi_log_bytes).unwrap();

        let fu_civ_row = r#"<row r="4"><c r="C4" t="inlineStr"><is><t>RFI No.</t></is></c><c r="D4" t="inlineStr"><is><t>STATUS</t></is></c></row><row r="5"><c r="C5" t="inlineStr"><is><t>CIV-0103</t></is></c><c r="D5" t="inlineStr"><is><t>Not Signed</t></is></c></row>"#;
        let fu_mec_header = r#"<row r="3"><c r="S3" t="inlineStr"><is><t>RFI No.</t></is></c><c r="T3" t="inlineStr"><is><t>Worley Status</t></is></c></row>"#;
        let fu_bytes = create_xlsx(&[
            ("Signed By Worley CIV", fu_civ_row),
            ("Signed By Worley MEC", fu_mec_header),
        ]);
        fs::write(&follow_up_path, &fu_bytes).unwrap();

        let caller = backend.caller_identity().unwrap();
        let system_root_sel = backend
            .selections
            .issue(
                &caller,
                "rfi.system_sync",
                "rfi_system_root",
                &system_root,
                SelectionKind::Folder,
            )
            .unwrap();
        let rfi_log_sel = backend
            .selections
            .issue(
                &caller,
                "rfi.system_sync",
                "rfi_log_workbook",
                &rfi_log_path,
                SelectionKind::File,
            )
            .unwrap();
        let fu_sel = backend
            .selections
            .issue(
                &caller,
                "rfi.system_sync",
                "follow_up_workbook",
                &follow_up_path,
                SelectionKind::File,
            )
            .unwrap();

        let params = json!({
            "opId": "rfi.system_sync",
            "values": {
                "rfi_system_root": system_root_sel,
                "rfi_log_workbook": rfi_log_sel,
                "follow_up_workbook": fu_sel,
            }
        });

        let preview_result = backend
            .preview_rfi_system_sync(params, |_| {})
            .expect("preview must not fail with path grant error");
        assert_eq!(preview_result["opId"], "rfi.system_sync");
        assert!(
            preview_result["steps"]
                .as_array()
                .unwrap()
                .iter()
                .all(|step| {
                    step["meta"]["syncStep"]["canonicalRfiCodes"]
                        == json!(["AZE-SUN-INT-QMS-RFI-CIV-0103"])
                })
        );
        let plan_id = preview_result["planId"].as_str().unwrap();

        // Apply
        let apply_result = backend
            .filesystem
            .apply_with_progress(plan_id, &caller, |_| {})
            .expect("apply must succeed");
        assert_eq!(apply_result.applied, 2);
        assert_eq!(apply_result.failed, 0);
        let journal = backend.journals.read(&apply_result.journal_id).unwrap();
        let evidence = journal
            .steps
            .iter()
            .filter_map(|step| step.meta.get("workItemEvidence"))
            .collect::<Vec<_>>();
        assert_eq!(evidence.len(), 2);
        assert!(
            evidence.iter().all(|item| {
                item["canonicalRfiCodes"] == json!(["AZE-SUN-INT-QMS-RFI-CIV-0103"])
            })
        );

        // Undo
        let undo_result = backend
            .filesystem
            .undo(&apply_result.journal_id, &caller, None)
            .expect("undo must succeed");
        assert_eq!(undo_result.applied, 4);
        assert_eq!(undo_result.failed, 0);
    }
}
