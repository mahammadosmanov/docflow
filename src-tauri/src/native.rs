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
    ocr_cache::CachingOcr,
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
        .map_or_else(
            || value.to_string(),
            |date| date.format("%d.%m.%Y").to_string(),
        )
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
    watch: crate::watch::WatchQueue,
}

impl NativeBackend {
    pub(crate) fn production() -> Result<Self, BackendError> {
        let settings_path = default_settings_path();
        let app_data_dir = settings_path.parent().map_or_else(
            || env::temp_dir().join("docflow"),
            std::path::Path::to_path_buf,
        );
        let _ = fs::create_dir_all(app_data_dir.join("trash"));
        let scan_index =
            ScanIndex::new(app_data_dir.join("scan-index.sqlite3")).map_err(|error| {
                BackendError::protocol(format!("scan index initialization: {error}"))
            })?;
        let journal_dir = settings_path.parent().map_or_else(
            || env::temp_dir().join("docflow-journal"),
            |parent| parent.join("journal"),
        );
        let settings = Arc::new(
            SettingsStore::new(settings_path).map_err(|_| BackendError::settings_write())?,
        );
        let supabase_http: Arc<dyn JsonHttp> =
            Arc::new(HttpClient::new(HttpConfig::default()).map_err(|error| {
                BackendError::protocol(format!("Supabase HTTP initialization: {error}"))
            })?);
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
            .map_err(|error| {
                BackendError::protocol(format!("Gemini HTTP initialization: {error}"))
            })?,
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
            watch: crate::watch::WatchQueue::default(),
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
            BackendMethod::WatchStatus => self.watch_dispatch_status(params),
            BackendMethod::WatchUpdate => self.watch_dispatch_update(params),
            BackendMethod::WatchOpen => self.watch_dispatch_open(params),
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
    // Unattended replica publication is accepted design but not an active operation (docs/KNOWN_ISSUES.md); keep until enabled or retired.
    // Unattended replica publication is accepted design but not an active operation (docs/KNOWN_ISSUES.md); keep until enabled or retired.
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
}

mod free;
mod params;
mod plan;
mod preview;
mod preview_rfi;
mod session;
mod watch;
pub(crate) use free::*;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod watch_tests;
