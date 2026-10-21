use base64::Engine;
use std::io::{Cursor, Read};
use std::{
    collections::{HashMap, HashSet},
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
    concrete_document_sources::{
        has_con_civ_010_form, read_civil_records, read_follow_up_records, requested_rfis,
    },
    concrete_documents::{self, ConcreteBatch, DocumentType, GeneratedFile, SourceRecord},
    contracts::{
        AppPrefs, Bootstrap, JsonObject, OperationDef, ParamKind, Plan, PrintQueueSummary,
        ProgressEvent, RfiPrintCandidate, RfiPrintFindRequest, RfiPrintFindResult,
        RfiPrintResolution, Step, StepKind, Warning,
    },
    credentials::OsCredentialStore,
    diagnostics::PrinterDiagnostic,
    errors::BackendError,
    fs_ops::{FilesystemAuthority, GeneratedWriteFailure},
    gemini::{DEFAULT_MODEL, FALLBACK_MODELS, GeminiClient},
    http::{HttpClient, HttpConfig, JsonHttp},
    journal::JournalStore,
    ocr::NativeOcr,
    operations::pdf_orientation::{
        PdfOrientationParams, preview_indexed as preview_pdf_orientation,
    },
    operations::process_scans::{
        FAST_COVER_OPERATION_ID, ProcessScansParams, ScanMode, preview_fast_covers,
    },
    operations::replica_publish::{self, ReplicaPublishParams},
    operations::rfi_file_group::{self, GroupRfiFilesParams, preview as preview_rfi_file_group},
    operations::rfi_print_find::{self, DocumentType as RfiPrintDocumentType, IncludeTypes},
    operations::stage_signed_pages::{
        StageSignedPagesParams, preview as preview_stage_signed_pages,
        revise as revise_stage_signed_pages,
    },
    outlook_reply_draft::{self, OutlookDraftService, OutlookFailureKind},
    path_selection::{PathSelection, PathSelectionStore, SelectionKind, SelectionScope},
    paths::{CallerIdentity, GrantAccess, GrantScope, PathIntent},
    plan_store::{BackendOrigin, PlanBinding},
    platform,
    print_queue::{BatchPrintParams, PrintService, default_queue_root},
    progress::GlobalMutationLock,
    registry::{RegistryFacts, operation_registry},
    review::{
        ReviewDecision as NativeReviewDecision, preview as preview_review, scan as scan_review,
    },
    rfi_follow_up, rfi_log, rfi_system_sync,
    scan_index::ScanIndex,
    settings::{PresetStoreError, SettingsStore, default_settings_path},
};

fn concrete_documents_error(detail: &str) -> BackendError {
    BackendError::validation(format!("error.rfi.documents.invalidInput: {detail}"))
}

fn required_selected_path(
    values: &Map<String, Value>,
    name: &str,
) -> Result<PathBuf, BackendError> {
    values
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(BackendError::invalid_params)
}

fn optional_selected_path(values: &Map<String, Value>, name: &str) -> Option<PathBuf> {
    values
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

fn optional_requested_rfis(raw: &str) -> Result<Vec<String>, BackendError> {
    if raw.trim().is_empty() {
        Ok(Vec::new())
    } else {
        requested_rfis(raw).map_err(|error| concrete_documents_error(&error))
    }
}

fn parse_ambient_groups(
    raw: &str,
    selected_rfis: &[String],
) -> Result<HashMap<String, (u8, u8)>, BackendError> {
    let mut groups = HashMap::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let (rfis, range) = line.split_once('=').ok_or_else(|| {
            concrete_documents_error("Ambient groups must use `RFI[, RFI...]=MIN-MAX` per line")
        })?;
        let (minimum, maximum) = range
            .trim()
            .split_once('-')
            .ok_or_else(|| concrete_documents_error("Ambient ranges must use MIN-MAX"))?;
        let minimum = minimum.trim().parse::<u8>().map_err(|_| {
            concrete_documents_error("Ambient range values must be whole degrees from 0 to 100")
        })?;
        let maximum = maximum.trim().parse::<u8>().map_err(|_| {
            concrete_documents_error("Ambient range values must be whole degrees from 0 to 100")
        })?;
        if maximum > 100 || minimum > maximum {
            return Err(concrete_documents_error(
                "Ambient range must be between 0 and 100 °C with minimum no greater than maximum",
            ));
        }
        for rfi in requested_rfis(rfis).map_err(|error| concrete_documents_error(&error))? {
            if !selected_rfis.contains(&rfi) {
                return Err(concrete_documents_error(&format!(
                    "Ambient group includes an RFI not selected for CON-CIV-011: {rfi}"
                )));
            }
            if groups.insert(rfi.clone(), (minimum, maximum)).is_some() {
                return Err(concrete_documents_error(&format!(
                    "Ambient range was specified more than once for {rfi}"
                )));
            }
        }
    }
    Ok(groups)
}

fn add_grant_root(roots: &mut Vec<(PathBuf, GrantAccess)>, root: PathBuf, access: GrantAccess) {
    if let Some((_, existing)) = roots.iter_mut().find(|(candidate, _)| *candidate == root) {
        if access == GrantAccess::ReadWrite {
            *existing = GrantAccess::ReadWrite;
        }
        return;
    }
    roots.push((root, access));
}

fn parse_water_ratios(raw: &str) -> Result<HashMap<String, String>, BackendError> {
    let mut result = HashMap::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let (rfi, ratio) = line.split_once('=').ok_or_else(|| {
            concrete_documents_error(
                "Water/cement ratios must use one `RFI = ratio` entry per line",
            )
        })?;
        let mut normalized =
            requested_rfis(rfi).map_err(|error| concrete_documents_error(&error))?;
        if normalized.len() != 1 {
            return Err(concrete_documents_error(
                "Each water/cement ratio line must name exactly one RFI",
            ));
        }
        let rfi = normalized.pop().expect("one normalized RFI");
        let ratio = ratio.trim();
        let parsed = ratio
            .parse::<f64>()
            .map_err(|_| concrete_documents_error("Water/cement ratios must be numeric"))?;
        if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) || parsed == 0.0 {
            return Err(concrete_documents_error(
                "Water/cement ratios must be greater than 0 and no greater than 1",
            ));
        }
        if result.insert(rfi.clone(), ratio.to_string()).is_some() {
            return Err(concrete_documents_error(&format!(
                "Water/cement ratio was entered more than once for {rfi}"
            )));
        }
    }
    Ok(result)
}

fn document_location(document_type: DocumentType, area: &str, tag: &str) -> String {
    let area = area.trim();
    let tag = tag.trim();
    match (area.is_empty(), tag.is_empty()) {
        (true, true) => String::new(),
        (true, false) => tag.to_string(),
        (false, true) => area.to_string(),
        (false, false) => match document_type {
            DocumentType::Civ010 => format!("{area} / {tag}"),
            DocumentType::Civ011 => format!("{area} {tag}"),
            DocumentType::Civ013 => format!("{tag} ({area})"),
            DocumentType::Civ014 => format!("({area}) {tag}"),
        },
    }
}

fn excel_date_text(value: &str) -> String {
    let Ok(serial) = value.trim().parse::<f64>() else {
        return value.to_string();
    };
    if !serial.is_finite() || !(1.0..=2_958_466.0).contains(&serial) {
        return value.to_string();
    }
    let Some(epoch) = chrono::NaiveDate::from_ymd_opt(1899, 12, 30) else {
        return value.to_string();
    };
    epoch
        .checked_add_signed(chrono::Duration::days(serial.floor() as i64))
        .map(|date| date.format("%d.%m.%Y").to_string())
        .unwrap_or_else(|| value.to_string())
}

fn excel_time_text(value: &str) -> String {
    let Ok(serial) = value.trim().parse::<f64>() else {
        return value.to_string();
    };
    if !serial.is_finite() || !(0.0..1.0).contains(&serial) {
        return value.to_string();
    }
    let minutes = (serial * 24.0 * 60.0).round() as u32 % (24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

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
    scan_index: ScanIndex,
}

impl NativeBackend {
    pub(crate) fn production() -> Result<Self, BackendError> {
        let settings_path = default_settings_path();
        let app_data_dir = settings_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| env::temp_dir().join("docflow"));
        let _ = fs::create_dir_all(app_data_dir.join("trash"));
        let scan_index =
            ScanIndex::new(app_data_dir.join("scan-index.sqlite3")).map_err(|error| {
                BackendError::protocol(format!("scan index initialization: {error}"))
            })?;
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
            scan_index,
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
                let previous = self.settings.preferences();
                platform::set_start_with_windows(false)
                    .map_err(|_| BackendError::settings_write())?;
                if self.settings.save_preferences(AppPrefs::default()).is_err() {
                    let _ = platform::set_start_with_windows(previous.start_with_windows);
                    return Err(BackendError::settings_write());
                }
                encode(self.settings.preferences())
            }
            BackendMethod::SettingsReplicaStatus => {
                decode_empty(params)?;
                let preferences = self.settings.preferences();
                let section = self.settings.section("replicaAutomation");
                let configured = section
                    .as_ref()
                    .is_some_and(|value| value.contains_key("profile"));
                let status = if !configured {
                    "unconfigured"
                } else if !preferences.replica_sync_enabled {
                    "disabled"
                } else {
                    section
                        .as_ref()
                        .and_then(|value| value.get("status"))
                        .and_then(Value::as_str)
                        .unwrap_or("idle")
                };
                Ok(json!({
                    "enabled": preferences.replica_sync_enabled,
                    "configured": configured,
                    "status": status,
                    "lastSuccessUnix": section.as_ref().and_then(|value| value.get("lastSuccessUnix")).and_then(Value::as_i64),
                    "lastErrorCode": section.as_ref().and_then(|value| value.get("lastErrorCode")).and_then(Value::as_str),
                }))
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
            BackendMethod::RfiPrintFind => self.find_rfi_print_files(params),
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

    pub(crate) fn printer_diagnostics(&self) -> PrinterDiagnostic {
        let caller = self.caller_identity().ok();
        self.print.diagnostics(caller.as_ref())
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
    #[allow(dead_code)]
    pub(crate) fn automatic_replica_tick(&self) {
        if !self.settings.preferences().replica_sync_enabled {
            return;
        }
        let Some(mut section) = self.settings.section("replicaAutomation") else {
            self.pause_replica_automation("error.replica.profileMissing");
            return;
        };
        let Some(profile) = section.get("profile").cloned() else {
            self.pause_replica_automation("error.replica.profileMissing");
            return;
        };
        let Ok(params) = serde_json::from_value::<ReplicaPublishParams>(profile) else {
            self.pause_replica_automation("error.replica.profileInvalid");
            return;
        };
        let current = now();
        let last_attempt = section
            .get("lastAttemptUnix")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let interval = i64::try_from(params.interval_minutes)
            .unwrap_or(i64::MAX)
            .saturating_mul(60);
        if current.saturating_sub(last_attempt) < interval {
            return;
        }
        section.insert("lastAttemptUnix".to_string(), Value::from(current));
        section.insert("status".to_string(), Value::String("running".to_string()));
        let _ = self
            .settings
            .save_section("replicaAutomation", Some(section.clone()));

        let caller = match self.caller_identity() {
            Ok(caller) => caller,
            Err(error) => {
                section.insert(
                    "status".to_string(),
                    Value::String("waitingForSignIn".to_string()),
                );
                section.insert(
                    "lastErrorCode".to_string(),
                    Value::String(error.code.to_string()),
                );
                let _ = self
                    .settings
                    .save_section("replicaAutomation", Some(section));
                return;
            }
        };
        let outcome = self
            .cache_replica_plan(params, &caller, true, |_| {})
            .and_then(|plan| {
                let plan_id = plan.plan_id.clone();
                self.filesystem
                    .apply_with_progress(&plan_id, &caller, |_| {})
            });
        match outcome {
            Ok(result) if result.failed == 0 => {
                section.insert("status".to_string(), Value::String("idle".to_string()));
                section.insert("lastSuccessUnix".to_string(), Value::from(current));
                section.insert(
                    "lastJournalId".to_string(),
                    Value::String(result.journal_id),
                );
                section.remove("lastErrorCode");
                let _ = self
                    .settings
                    .save_section("replicaAutomation", Some(section));
            }
            Ok(result) => {
                let code = result
                    .errors
                    .first()
                    .map(|warning| warning.key.as_str())
                    .unwrap_or("error.replica.automaticApplyFailed");
                self.pause_replica_automation(code);
            }
            Err(error) if matches!(error.code, "operation_in_progress" | "auth_required") => {
                section.insert("status".to_string(), Value::String("deferred".to_string()));
                section.insert(
                    "lastErrorCode".to_string(),
                    Value::String(error.code.to_string()),
                );
                let _ = self
                    .settings
                    .save_section("replicaAutomation", Some(section));
            }
            Err(error) if error.message == "error.replica.sourceSettling" => {
                section.insert("status".to_string(), Value::String("deferred".to_string()));
                section.insert("lastErrorCode".to_string(), Value::String(error.message));
                let _ = self
                    .settings
                    .save_section("replicaAutomation", Some(section));
            }
            Err(error) => self.pause_replica_automation(error.message.as_str()),
        }
    }
    #[allow(dead_code)]
    fn pause_replica_automation(&self, error_code: &str) {
        if let Some(mut section) = self.settings.section("replicaAutomation") {
            section.insert("status".to_string(), Value::String("paused".to_string()));
            section.insert(
                "lastErrorCode".to_string(),
                Value::String(error_code.to_string()),
            );
            let _ = self
                .settings
                .save_section("replicaAutomation", Some(section));
        }
        let mut preferences = self.settings.preferences();
        preferences.replica_sync_enabled = false;
        let _ = self.settings.save_preferences(preferences);
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
        if request.op_id != FAST_COVER_OPERATION_ID {
            return Err(BackendError::validation("unknown operation ID"));
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
        let trash_grant = if request.values.keep_original {
            None
        } else {
            Some(self.filesystem.issue_grant(
                &scope,
                &self.app_data_dir.join("trash"),
                GrantAccess::ReadWrite,
            )?)
        };
        let scan_params = ProcessScansParams {
            source: request.values.source,
            destination: request.values.dest,
            scan_mode: request.values.scan_mode,
            organize_into_rfi_folders: request.values.organize_into_rfi_folders,
            keep_original: request.values.keep_original,
            correct_orientation: request.values.correct_orientation,
            trash_root: (!request.values.keep_original).then(|| self.app_data_dir.join("trash")),
        };
        let ocr = NativeOcr::discover();
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let plan = preview_fast_covers(&scan_params, &ocr, &mut progress, || {
            cancellation.is_cancelled()
        })?;
        encode(
            self.filesystem.cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new(
                    &caller.owner_id,
                    &caller.session_id,
                    &request.op_id,
                    [
                        Some(source_grant.grant_id),
                        Some(destination_grant.grant_id),
                        trash_grant.map(|grant| grant.grant_id),
                    ]
                    .into_iter()
                    .flatten()
                    .collect(),
                ),
                plan,
            )?,
        )
    }

    pub(crate) fn preview_rfi_file_group<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiFileGroupValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_file_group::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI file-group backend",
            ));
        }
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
                (request.values.source.clone(), GrantAccess::Read),
                (request.values.destination.clone(), GrantAccess::ReadWrite),
            ],
        )?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let plan = preview_rfi_file_group(
            &GroupRfiFilesParams {
                source: request.values.source,
                destination: request.values.destination,
            },
            &NativeOcr::discover(),
            progress,
            || cancellation.is_cancelled(),
        )?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                grant_ids,
            ),
            plan,
        )?;
        provisional.commit();
        encode(cached)
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
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let plan = rfi_log::preview(
            &request.values,
            chrono::Local::now().date_naive(),
            Some(&self.gemini),
            progress,
            || cancellation.is_cancelled(),
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
    pub(crate) fn preview_concrete_documents<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        const OPERATION_ID: &str = "rfi.generate_concrete_documents";
        let request: PreviewEnvelope<Map<String, Value>> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the concrete document backend",
            ));
        }
        let get_text = |name: &str| {
            request
                .values
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
        };
        let rfis_010 = optional_requested_rfis(get_text("rfis_010"))?;
        let rfis_011 = optional_requested_rfis(get_text("rfis_011"))?;
        let rfis_013 = optional_requested_rfis(get_text("rfis_013"))?;
        let rfis_014 = optional_requested_rfis(get_text("rfis_014"))?;
        if rfis_010.is_empty() && rfis_011.is_empty() && rfis_013.is_empty() && rfis_014.is_empty()
        {
            return Err(concrete_documents_error(
                "Enter RFI numbers for at least one CON-CIV type",
            ));
        }
        let ambient_min = request
            .values
            .get("ambient_min")
            .and_then(Value::as_i64)
            .unwrap_or(19);
        let ambient_max = request
            .values
            .get("ambient_max")
            .and_then(Value::as_i64)
            .unwrap_or(24);
        if !(0..=100).contains(&ambient_min)
            || !(0..=100).contains(&ambient_max)
            || ambient_min > ambient_max
        {
            return Err(concrete_documents_error(
                "Ambient temperature range must be between 0 and 100 °C with minimum no greater than maximum",
            ));
        }
        let ambient_groups = parse_ambient_groups(get_text("ambient_groups"), &rfis_011)?;
        let rfi_log = required_selected_path(&request.values, "rfi_log_workbook")?;
        let output_directory = required_selected_path(&request.values, "output_directory")?;
        if !output_directory.is_dir() {
            return Err(concrete_documents_error("Output folder is unavailable"));
        }
        let mut selections = Vec::<(DocumentType, Vec<String>, PathBuf)>::new();
        for (document_type, numbers, parameter) in [
            (DocumentType::Civ010, rfis_010.clone(), "template_010"),
            (DocumentType::Civ011, rfis_011.clone(), "template_011"),
            (DocumentType::Civ013, rfis_013.clone(), "template_013"),
            (DocumentType::Civ014, rfis_014.clone(), "template_014"),
        ] {
            if numbers.is_empty() {
                continue;
            }
            let template = required_selected_path(&request.values, parameter).map_err(|_| {
                concrete_documents_error(
                    "Select an example template for every requested document type",
                )
            })?;
            if !template.is_file() {
                return Err(concrete_documents_error(
                    "A selected template file is unavailable",
                ));
            }
            selections.push((document_type, numbers, template));
        }
        let needs_follow_up = selections
            .iter()
            .any(|(kind, _, _)| *kind == DocumentType::Civ010);
        let follow_up = if needs_follow_up {
            optional_selected_path(&request.values, "follow_up_workbook")
        } else {
            None
        };
        if !rfi_log.is_file() {
            return Err(concrete_documents_error(
                "The selected RFI Log workbook is unavailable",
            ));
        }
        let mut requested = selections
            .iter()
            .flat_map(|(_, numbers, _)| numbers.iter().cloned())
            .collect::<Vec<_>>();
        requested.sort();
        requested.dedup();

        let mut roots = Vec::<(PathBuf, GrantAccess)>::new();
        for path in std::iter::once(&rfi_log)
            .chain(follow_up.iter().filter(|path| path.is_file()))
            .chain(selections.iter().map(|(_, _, path)| path))
        {
            let root = path
                .parent()
                .ok_or_else(|| BackendError::path_invalid("Selected input has no parent folder"))?
                .to_path_buf();
            add_grant_root(&mut roots, root, GrantAccess::Read);
        }
        add_grant_root(&mut roots, output_directory.clone(), GrantAccess::ReadWrite);
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            OPERATION_ID,
        );
        let provisional = self.filesystem.issue_provisional_grants(&scope, &roots)?;
        let read_selected = |path: &Path| -> Result<Vec<u8>, BackendError> {
            let grant = provisional
                .grants()
                .iter()
                .filter(|grant| {
                    path.starts_with(&grant.root) && grant.access.allows(PathIntent::Read)
                })
                .max_by_key(|grant| grant.root.components().count())
                .ok_or_else(|| {
                    BackendError::path_invalid("No read grant covers a selected input")
                })?;
            let anchor = self
                .filesystem
                .anchor_granted_path(grant, path, PathIntent::Read)?;
            let mut file = anchor.open_read().map_err(|error| {
                concrete_documents_error(&format!("Cannot read selected input: {error}"))
            })?;
            let length = file
                .metadata()
                .map_err(|_| concrete_documents_error("Cannot inspect selected input"))?
                .len();
            if length > crate::ooxml::MAX_PACKAGE_BYTES {
                return Err(concrete_documents_error(
                    "A selected Office package is larger than the supported limit",
                ));
            }
            let mut bytes = Vec::with_capacity(length as usize);
            file.read_to_end(&mut bytes)
                .map_err(|_| concrete_documents_error("Could not read a selected input"))?;
            Ok(bytes)
        };

        let log_bytes = read_selected(&rfi_log)?;
        let civil_records = read_civil_records(&mut Cursor::new(log_bytes), &requested)
            .map_err(|error| concrete_documents_error(&error))?;
        let rfis_requiring_follow_up = rfis_010
            .iter()
            .filter(|rfi| {
                civil_records
                    .get(*rfi)
                    .is_some_and(|record| has_con_civ_010_form(&record.forms))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !rfis_requiring_follow_up.is_empty() {
            let path = follow_up.as_ref().ok_or_else(|| {
                BackendError::validation("error.param.required.follow_up_workbook")
            })?;
            if !path.is_file() {
                return Err(concrete_documents_error(
                    "The Concrete Specimen Test Follow-Up workbook is unavailable",
                ));
            }
        }
        let mut warnings = rfis_010
            .iter()
            .filter(|rfi| !rfis_requiring_follow_up.contains(rfi))
            .map(|rfi| Warning {
                key: "warn.rfi.documents.formNotRequired".to_string(),
                subject: rfi.clone(),
            })
            .collect::<Vec<_>>();
        let follow_records = if let Some(path) = follow_up.as_ref() {
            if rfis_requiring_follow_up.is_empty() {
                None
            } else {
                let bytes = read_selected(path)?;
                Some(
                    read_follow_up_records(&mut Cursor::new(bytes), &rfis_requiring_follow_up)
                        .map_err(|error| concrete_documents_error(&error))?,
                )
            }
        } else {
            None
        };
        let ratios = parse_water_ratios(get_text("water_cement_ratios"))?;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let mut generated = Vec::new();
        for (document_type, numbers, template_path) in selections {
            let template_bytes = read_selected(&template_path)?;
            let mut records = Vec::with_capacity(numbers.len());
            for rfi in numbers {
                let civil = civil_records
                    .get(&rfi)
                    .ok_or_else(|| concrete_documents_error("RFI Log row is missing"))?;
                if document_type == DocumentType::Civ010 && !has_con_civ_010_form(&civil.forms) {
                    continue;
                }
                let location = document_location(document_type, &civil.area, &civil.tag);
                if location.is_empty() {
                    return Err(concrete_documents_error(&format!(
                        "The RFI Log has no usable location for {rfi}"
                    )));
                }
                let follow_rows = follow_records.as_ref().and_then(|rows| rows.get(&rfi));
                if document_type == DocumentType::Civ010 && follow_rows.is_none() {
                    warnings.push(Warning {
                        key: "warn.rfi.documents.followUpMissing".to_string(),
                        subject: rfi.clone(),
                    });
                    continue;
                }
                let follow = follow_rows.and_then(|rows| rows.first());
                let field = |key: &str| {
                    follow
                        .and_then(|row| row.get(key))
                        .map(String::as_str)
                        .unwrap_or("")
                };
                let additional_batches = if document_type == DocumentType::Civ010 {
                    follow_rows
                        .into_iter()
                        .flatten()
                        .skip(1)
                        .map(|row| {
                            let value = |key: &str| {
                                row.get(key)
                                    .map(String::as_str)
                                    .filter(|value| !value.trim().is_empty())
                                    .unwrap_or_else(|| {
                                        if matches!(key, "location" | "concrete_class" | "supplier")
                                        {
                                            field(key)
                                        } else {
                                            ""
                                        }
                                    })
                                    .to_string()
                            };
                            ConcreteBatch {
                                tag: value("location"),
                                concrete_class: value("concrete_class"),
                                supplier: value("supplier"),
                                batch_ticket: value("batch_ticket"),
                                truck: value("truck"),
                                quantity: value("quantity"),
                                total_quantity: value("total_quantity"),
                                batch_time: excel_time_text(&value("batch_time")),
                                start_time: excel_time_text(&value("start_time")),
                                end_time: excel_time_text(&value("end_time")),
                                plant_slump: value("plant_slump"),
                                in_situ_slump: value("in_situ_slump"),
                                plant_temp: value("plant_temp"),
                                in_situ_temp: value("in_situ_temp"),
                                samples: value("samples"),
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                records.push(SourceRecord {
                    rfi: rfi.clone(),
                    location,
                    tag: if document_type == DocumentType::Civ010 {
                        field("location").to_string()
                    } else {
                        civil.tag.clone()
                    },
                    concrete_class: field("concrete_class").to_string(),
                    supplier: field("supplier").to_string(),
                    batch_ticket: field("batch_ticket").to_string(),
                    truck: field("truck").to_string(),
                    quantity: field("quantity").to_string(),
                    total_quantity: field("total_quantity").to_string(),
                    batch_time: excel_time_text(field("batch_time")),
                    pour_date: excel_date_text(field("pour_date")),
                    start_time: excel_time_text(field("start_time")),
                    end_time: excel_time_text(field("end_time")),
                    plant_slump: field("plant_slump").to_string(),
                    in_situ_slump: field("in_situ_slump").to_string(),
                    plant_temp: field("plant_temp").to_string(),
                    in_situ_temp: field("in_situ_temp").to_string(),
                    samples: field("samples").to_string(),
                    water_cement_ratio: ratios.get(&rfi).cloned(),
                    ambient_range: ambient_groups.get(&rfi).copied(),
                    drawing: field("drawing").to_string(),
                    additional_batches,
                });
            }
            if records.is_empty() {
                continue;
            }
            generated.extend(
                concrete_documents::generate(
                    document_type,
                    &template_bytes,
                    &records,
                    (ambient_min as u8, ambient_max as u8),
                    seed ^ (generated.len() as u64 + 1),
                )
                .map_err(|error| concrete_documents_error(&error))?,
            );
        }

        let generated_count = generated.len() as u64;
        let mut steps = Vec::new();
        let mut destinations = HashSet::new();
        for GeneratedFile { filename, bytes } in generated {
            if Path::new(&filename)
                .file_name()
                .and_then(|name| name.to_str())
                != Some(filename.as_str())
            {
                return Err(concrete_documents_error("Generated output name is invalid"));
            }
            let destination = output_directory.join(&filename);
            if !destinations.insert(destination.clone()) {
                return Err(concrete_documents_error(
                    "Two selected records produce the same output filename",
                ));
            }
            if destination.exists() {
                warnings.push(Warning {
                    key: "warn.rfi.documents.outputExists".to_string(),
                    subject: filename,
                });
                continue;
            }
            let mut meta = JsonObject::new();
            meta.insert(
                "contentBase64".to_string(),
                Value::String(base64::engine::general_purpose::STANDARD.encode(bytes)),
            );
            steps.push(Step {
                kind: StepKind::Write,
                src: None,
                dst: Some(destination.to_string_lossy().into_owned()),
                note_key: Some("step.rfi.generatedConcreteDocument".to_string()),
                meta,
            });
        }
        progress(ProgressEvent {
            phase: "concrete_document_generation".to_string(),
            done: steps.len() as u64,
            total: steps.len() as u64,
            subject: output_directory.to_string_lossy().into_owned(),
        });
        let plan = Plan {
            plan_id: String::new(),
            op_id: OPERATION_ID.to_string(),
            steps,
            warnings,
            meta: serde_json::from_value(json!({"generatedCount": generated_count})).map_err(
                |_| BackendError::protocol("concrete document plan metadata encoding failed"),
            )?,
        };
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                OPERATION_ID,
                grant_ids,
            ),
            plan,
        )?;
        provisional.commit();
        encode(cached)
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
            phase: "follow_up_analysis".to_string(),
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
            phase: "preview_ready".to_string(),
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
        let mut plan = rfi_system_sync::preview_system_sync_indexed(
            &system_root,
            &source_path,
            &target_path,
            &mut source,
            &mut target,
            Some(&self.scan_index),
            &caller.owner_id,
            request.values.force_rescan,
            &mut progress,
        )?;
        drop(source);
        drop(target);
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
        encode(cached)
    }

    pub(crate) fn preview_rfi_complete_status_sync<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiCompleteStatusPreviewValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_system_sync::COMPLETE_STATUS_OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI completion-status backend",
            ));
        }
        let system_root = fs::canonicalize(&request.values.rfi_system_root).map_err(|error| {
            BackendError::validation(format!(
                "error.rfi.completeStatus.systemRootInvalid: cannot resolve root: {error}"
            ))
        })?;
        let workbook_path =
            fs::canonicalize(&request.values.rfi_log_workbook).map_err(|error| {
                BackendError::validation(format!(
                    "error.rfi.completeStatus.workbookInvalid: cannot resolve RFI Log: {error}"
                ))
            })?;
        let workbook_root = workbook_path
            .parent()
            .ok_or_else(|| BackendError::path_invalid("RFI Log workbook has no parent"))?
            .to_path_buf();
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let workbook_in_system = workbook_path.starts_with(&system_root);
        let mut grant_requests = vec![(
            system_root.clone(),
            if workbook_in_system {
                GrantAccess::ReadWrite
            } else {
                GrantAccess::Read
            },
        )];
        if !workbook_in_system {
            grant_requests.push((workbook_root, GrantAccess::ReadWrite));
        }
        let provisional = self
            .filesystem
            .issue_provisional_grants(&scope, &grant_requests)?;
        let workbook_grant = provisional
            .grants()
            .iter()
            .find(|grant| {
                grant.access.allows(PathIntent::Write) && workbook_path.starts_with(&grant.root)
            })
            .ok_or_else(|| BackendError::path_invalid("No write grant covering RFI Log"))?;
        let workbook_anchor = self.filesystem.anchor_granted_path(
            workbook_grant,
            &workbook_path,
            PathIntent::Write,
        )?;
        let mut workbook = workbook_anchor
            .open_mutable()
            .map_err(|_| BackendError::validation("error.rfi.completeStatus.workbookLocked"))?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let mut plan = rfi_system_sync::preview_complete_status_sync(
            &system_root,
            &workbook_path,
            &mut workbook,
            progress,
            || cancellation.is_cancelled(),
        )?;
        drop(workbook);
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
                caller.owner_id,
                caller.session_id,
                request.op_id.clone(),
                grant_ids,
            ),
            reservation,
            plan,
        )?;
        provisional.commit();
        encode(cached)
    }
    #[allow(dead_code)]
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
            phase: "outlook_selection".to_string(),
            done: 0,
            total: 1,
            subject: String::new(),
        });
        let plan = self
            .outlook
            .preview(&self.caller_identity()?, &request.values.reply_text)?;
        progress(ProgressEvent {
            phase: "preview_ready".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(plan)
    }
    #[allow(dead_code)]
    pub(crate) fn preview_outlook_archive<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<OutlookArchiveValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != outlook_reply_draft::ARCHIVE_OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust Outlook archive backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let provisional = self.filesystem.issue_provisional_grants(
            &scope,
            &[(request.values.destination.clone(), GrantAccess::Write)],
        )?;
        progress(ProgressEvent {
            phase: "outlook_selection".to_string(),
            done: 0,
            total: 1,
            subject: String::new(),
        });
        let prepared = self
            .outlook
            .prepare_archive(&caller, &request.values.destination)?;
        let mut plan = prepared.plan.clone();
        let reservation = self.filesystem.reserve_plan_id();
        reservation.assign_to_empty(&mut plan)?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_reserved_plan(
            BackendOrigin::Rust,
            PlanBinding::new(caller.owner_id, caller.session_id, request.op_id, grant_ids),
            reservation,
            plan,
        )?;
        self.outlook.bind_archive(&cached.plan_id, prepared);
        provisional.commit();
        progress(ProgressEvent {
            phase: "preview_ready".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(cached)
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
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let plan = preview_stage_signed_pages(
            &StageSignedPagesParams {
                signed_scans: request.values.signed_scans,
                archive_root: request.values.archive_root,
                review_root: request.values.review_root,
                scan_subfolders: request.values.scan_subfolders,
            },
            &NativeOcr::discover(),
            progress,
            || cancellation.is_cancelled(),
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

    pub(crate) fn preview_pdf_orientation<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<PdfOrientationValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != "rfi.normalize_pdf_orientation" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust PDF orientation backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let _source_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.rfi_system_root,
            GrantAccess::Read,
        )?;
        let _review_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.review_root,
            GrantAccess::ReadWrite,
        )?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let plan = preview_pdf_orientation(
            &PdfOrientationParams {
                rfi_system_root: request.values.rfi_system_root.clone(),
                review_root: request.values.review_root.clone(),
            },
            &NativeOcr::discover(),
            progress,
            || cancellation.is_cancelled(),
            Some(&self.scan_index),
            &caller.owner_id,
            request.values.force_rescan,
        )?;
        // Large RFI trees can take longer than the grant TTL to inspect. Bind the
        // finished plan to fresh grants for the exact same selected roots; Apply
        // still revalidates the approved plan and filesystem identities.
        let source_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.rfi_system_root,
            GrantAccess::Read,
        )?;
        let review_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.review_root,
            GrantAccess::ReadWrite,
        )?;
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![source_grant.grant_id, review_grant.grant_id],
            ),
            plan,
        )?)
    }
    pub(crate) fn find_rfi_print_files(&self, params: Value) -> Result<Value, BackendError> {
        let request: RfiPrintFindRequest = decode(params)?;
        let caller = self.caller_identity()?;
        let root = self.selections.resolve(
            &request.root,
            &caller,
            "files.batch_print",
            "rfi_system_root",
            SelectionKind::Folder,
        )?;
        let canonical_root = fs::canonicalize(root)
            .map_err(|_| BackendError::validation("error.print.rfi.changed"))?;
        let discovery = rfi_print_find::discover(
            &canonical_root,
            &request.numbers,
            IncludeTypes {
                first_page: request.first_page,
                sketch: request.sketch,
            },
        )
        .map_err(|_| BackendError::validation("error.print.rfi.invalidRequest"))?;
        let mut candidates = Vec::with_capacity(discovery.candidates.len());
        for candidate in discovery.candidates {
            let mut selection = self
                .selections
                .issue(
                    &caller,
                    "files.batch_print",
                    "files",
                    &candidate.path,
                    SelectionKind::File,
                )
                .map_err(|_| BackendError::validation("error.print.rfi.changed"))?;
            let issued_path = resolve_scoped_rfi_print_file(
                &self.selections,
                &selection,
                &caller,
                &canonical_root,
            )?;
            let resolution = discovery
                .resolutions
                .iter()
                .find(|resolution| resolution.requested == candidate.requested)
                .ok_or_else(BackendError::invalid_params)?;
            let selected =
                !discovery.truncated && rfi_print_find::initially_selected(resolution, &candidate);
            let display_path = issued_path
                .strip_prefix(&canonical_root)
                .unwrap_or(&issued_path)
                .to_string_lossy()
                .into_owned();
            selection.display_path = display_path.clone();
            candidates.push(RfiPrintCandidate {
                requested: candidate.requested,
                rfi_code: candidate.rfi_code,
                document_type: match candidate.document_type {
                    RfiPrintDocumentType::FirstPage => "firstPage",
                    RfiPrintDocumentType::Sketch => "sketch",
                }
                .to_string(),
                display_path,
                selection,
                selected,
            });
        }
        let resolutions = discovery
            .resolutions
            .into_iter()
            .map(|resolution| RfiPrintResolution {
                requested: resolution.requested,
                rfi_codes: resolution.rfi_codes,
                missing_first_page: resolution.missing_first_page,
                missing_sketch: resolution.missing_sketch,
                ambiguous: resolution.ambiguous,
            })
            .collect::<Vec<_>>();
        encode(RfiPrintFindResult {
            candidates,
            resolutions,
            visited: discovery.visited,
            truncated: discovery.truncated,
        })
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
    #[allow(dead_code)]
    pub(crate) fn preview_replica_publish<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<ReplicaPublishParams> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != replica_publish::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust replica publication backend",
            ));
        }
        let caller = self.caller_identity()?;
        encode(self.cache_replica_plan(request.values, &caller, false, progress)?)
    }
    #[allow(dead_code)]
    fn cache_replica_plan<F>(
        &self,
        values: ReplicaPublishParams,
        caller: &CallerIdentity,
        automatic: bool,
        progress: F,
    ) -> Result<crate::contracts::Plan, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let roots = [
            fs::canonicalize(&values.root_a),
            fs::canonicalize(&values.root_b),
            fs::canonicalize(&values.root_c),
        ];
        let roots = roots
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| BackendError::path_invalid("error.replica.rootInvalid"))?;
        let backup = fs::canonicalize(&values.backup_root)
            .map_err(|_| BackendError::path_invalid("error.replica.backupInvalid"))?;
        let source_index = match values.source_root.as_str() {
            "a" => 0,
            "b" => 1,
            "c" => 2,
            _ => return Err(BackendError::validation("error.replica.sourceInvalid")),
        };
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            replica_publish::OPERATION_ID,
        );
        let mut grants = roots
            .iter()
            .enumerate()
            .map(|(index, root)| {
                (
                    root.clone(),
                    if index == source_index {
                        GrantAccess::Read
                    } else {
                        GrantAccess::ReadWrite
                    },
                )
            })
            .collect::<Vec<_>>();
        grants.push((backup, GrantAccess::ReadWrite));
        let provisional = self.filesystem.issue_provisional_grants(&scope, &grants)?;
        let plan = if automatic {
            replica_publish::preview_automatic_with_index(
                &values,
                &caller.owner_id,
                &self.scan_index,
                progress,
            )?
        } else {
            replica_publish::preview_with_index(
                &values,
                &caller.owner_id,
                &self.scan_index,
                progress,
            )?
        };
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                replica_publish::OPERATION_ID,
                grant_ids,
            ),
            plan,
        )?;
        provisional.commit();
        Ok(cached)
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
            rfi_file_group::OPERATION_ID => rfi_file_group::revise(&claimed.plan, &payload)?,
            FAST_COVER_OPERATION_ID => {
                crate::operations::process_scans::revise(&claimed.plan, &payload)?
            }
            rfi_system_sync::OPERATION_ID => rfi_system_sync::revise(
                &claimed.plan,
                payload["rows"]
                    .as_array()
                    .ok_or_else(BackendError::invalid_params)?,
            )?,
            rfi_system_sync::COMPLETE_STATUS_OPERATION_ID => rfi_system_sync::revise(
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

    pub(crate) fn preview_rfi_reply(&self, params: Value) -> Result<Value, BackendError> {
        let request: RfiReplyPreviewParams = decode(params)?;
        let caller = self.caller_identity()?;
        let claimed = self
            .filesystem
            .resolve_native_plan(&request.plan_id, &caller)?;
        if claimed.plan.op_id != "rfi.log_add" {
            return Err(BackendError::validation(
                "native plan does not contain an RFI table",
            ));
        }
        let journal = self.filesystem.journals().read(&request.journal_id)?;
        if !committed_rfi_reply_matches(&journal, &caller, &claimed.digest) {
            return Err(BackendError::validation(
                "error.rfi.outlookDraft.rfiLogNotCommitted",
            ));
        }
        let payload = rfi_log::reply_payload(&claimed.plan)?;
        let content = outlook_reply_draft::ReplyContent {
            plain_text: payload.plain,
            html: payload.html,
        };
        encode(self.outlook.preview_content(&caller, content)?)
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
        if self
            .filesystem
            .validate_plan_operation(plan_id, &caller, operation_id)?
            == outlook_reply_draft::ARCHIVE_OPERATION_ID
        {
            let result = self.filesystem.apply_generated_binary_write(
                plan_id,
                &caller,
                outlook_reply_draft::ARCHIVE_OPERATION_ID,
                &mut progress,
                || {
                    self.outlook
                        .export_archive(plan_id, &caller)
                        .map_err(|failure| match failure.kind {
                            OutlookFailureKind::OutcomeUnknown => {
                                GeneratedWriteFailure::OutcomeUnknown(failure.key)
                            }
                            OutlookFailureKind::Unsupported | OutlookFailureKind::NoEffect => {
                                GeneratedWriteFailure::NoEffect(failure.key)
                            }
                        })
                },
            )?;
            return Ok((
                encode(result)?,
                outlook_reply_draft::ARCHIVE_OPERATION_ID.to_string(),
            ));
        }
        let approved_op_id =
            self.filesystem
                .validate_plan_operation(plan_id, &caller, operation_id)?;
        let automation_profile = if approved_op_id == replica_publish::OPERATION_ID {
            self.filesystem
                .resolve_native_plan(plan_id, &caller)?
                .plan
                .meta
                .get("automationProfile")
                .cloned()
        } else {
            None
        };
        let mut result = self
            .filesystem
            .apply_with_progress(plan_id, &caller, &mut progress)?;
        if result.failed == 0
            && let Some(profile) = automation_profile
        {
            let mut section = JsonObject::new();
            section.insert("profile".to_string(), profile);
            section.insert("lastAttemptUnix".to_string(), Value::from(0));
            let activated = self
                .settings
                .save_section("replicaAutomation", Some(section));
            if activated.is_ok() {
                let mut preferences = self.settings.preferences();
                preferences.replica_sync_enabled = true;
                if self.settings.save_preferences(preferences).is_err() {
                    result.failed += 1;
                    result.errors.push(crate::contracts::Warning {
                        key: "error.replica.activationFailed".to_string(),
                        subject: String::new(),
                    });
                }
            } else {
                result.failed += 1;
                result.errors.push(crate::contracts::Warning {
                    key: "error.replica.activationFailed".to_string(),
                    subject: String::new(),
                });
            }
        }
        Ok((encode(result)?, approved_op_id))
    }

    pub(crate) fn cancel_native_plan(&self, plan_id: &str) -> bool {
        self.outlook.cancel(plan_id)
            || self.print.cancel(plan_id)
            || self.filesystem.cancel(plan_id)
    }

    pub(crate) fn cancel_native_operation(&self, op_id: &str) -> bool {
        self.filesystem.cancel(op_id)
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
                    confidence: item.confidence,
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
        let previous_startup = preferences.start_with_windows;
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
        if let Some(start_with_windows) = request.start_with_windows {
            platform::set_start_with_windows(start_with_windows)
                .map_err(|_| BackendError::settings_write())?;
            preferences.start_with_windows = start_with_windows;
        }
        if let Some(replica_sync_enabled) = request.replica_sync_enabled {
            if replica_sync_enabled
                && self
                    .settings
                    .section("replicaAutomation")
                    .and_then(|section| section.get("profile").cloned())
                    .is_none()
            {
                return Err(BackendError::validation("error.replica.profileMissing"));
            }
            preferences.replica_sync_enabled = replica_sync_enabled;
        }
        if self.settings.save_preferences(preferences).is_err() {
            if request.start_with_windows.is_some() {
                let _ = platform::set_start_with_windows(previous_startup);
            }
            return Err(BackendError::settings_write());
        }
        encode(self.settings.preferences())
    }
}
fn resolve_scoped_rfi_print_file(
    selections: &PathSelectionStore,
    selection: &PathSelection,
    caller: &CallerIdentity,
    canonical_root: &Path,
) -> Result<PathBuf, BackendError> {
    let issued_path = selections.resolve(
        selection,
        caller,
        "files.batch_print",
        "files",
        SelectionKind::File,
    )?;
    let canonical_path = match fs::canonicalize(&issued_path) {
        Ok(path) => path,
        Err(_) => {
            selections.revoke_selection(&selection.selection_id, caller);
            return Err(BackendError::validation("error.print.rfi.changed"));
        }
    };
    if canonical_path != issued_path || !canonical_path.starts_with(canonical_root) {
        selections.revoke_selection(&selection.selection_id, caller);
        return Err(BackendError::validation("error.print.rfi.rootEscape"));
    }
    Ok(canonical_path)
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
    confidence: f64,
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
    #[serde(default)]
    scan_mode: ScanMode,
    #[serde(default = "default_true")]
    organize_into_rfi_folders: bool,
    #[serde(default)]
    keep_original: bool,
    #[serde(default = "default_true")]
    correct_orientation: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RfiFileGroupValues {
    source: PathBuf,
    destination: PathBuf,
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
    #[serde(default)]
    force_rescan: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RfiCompleteStatusPreviewValues {
    #[serde(rename = "rfi_system_root")]
    rfi_system_root: PathBuf,
    #[serde(rename = "rfi_log_workbook")]
    rfi_log_workbook: PathBuf,
}

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(deny_unknown_fields)]
struct OutlookReplyDraftValues {
    reply_text: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(deny_unknown_fields)]
struct OutlookArchiveValues {
    destination: PathBuf,
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
#[serde(deny_unknown_fields)]
struct PdfOrientationValues {
    rfi_system_root: PathBuf,
    review_root: PathBuf,
    #[serde(default)]
    force_rescan: bool,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RfiReplyPreviewParams {
    plan_id: String,
    journal_id: String,
}

fn committed_rfi_reply_matches(
    journal: &crate::journal::JournalRecord,
    caller: &CallerIdentity,
    plan_digest: &str,
) -> bool {
    journal.op_id == "rfi.log_add"
        && journal.state == Some(crate::journal::JournalState::Committed)
        && journal.owner_id.as_deref() == Some(caller.owner_id.as_str())
        && journal.session_id.as_deref() == Some(caller.session_id.as_str())
        && journal.plan_digest.as_deref() == Some(plan_digest)
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
    #[serde(default)]
    start_with_windows: Option<bool>,
    #[serde(default)]
    replica_sync_enabled: Option<bool>,
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
mod tests;
