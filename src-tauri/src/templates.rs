//! Shared document templates: Supabase manifest sync and the local offline
//! cache (ADR 0055). Rust alone talks to Supabase and owns the cached files;
//! the webview only sees summaries.

use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    auth::{AuthConfig, AuthService},
    errors::BackendError,
    http::{BytesHttp, JsonHttp},
};

/// Largest template file accepted, matching the Storage bucket limit.
pub const MAX_TEMPLATE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TEMPLATES: usize = 500;
const MAX_FIELDS: usize = 500;

pub mod editor;
pub mod field_map;
const BUCKET: &str = "doc-templates";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Xlsx,
    Docx,
}

impl FileKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Docx => "docx",
        }
    }

    fn main_part(self) -> &'static str {
        match self {
            Self::Xlsx => "xl/workbook.xml",
            Self::Docx => "word/document.xml",
        }
    }
}

/// One published template version, as listed by `doc_template_manifest`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TemplateEntry {
    pub template_id: String,
    pub code: String,
    pub title: String,
    pub discipline: String,
    pub file_kind: FileKind,
    pub version_id: String,
    pub version: u32,
    pub file_sha256: String,
    pub file_size: u64,
    pub storage_path: String,
    pub field_map: Value,
    #[serde(default)]
    pub published_at: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct CachedManifest {
    synced_at: Option<i64>,
    entries: Vec<TemplateEntry>,
}

/// What the Templates screen shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateStatus {
    pub online: bool,
    pub syncing: bool,
    pub last_synced_at: Option<i64>,
    pub last_error: Option<String>,
    pub failed_codes: Vec<String>,
    pub templates: Vec<TemplateSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSummary {
    pub code: String,
    pub title: String,
    pub discipline: String,
    pub file_kind: FileKind,
    pub version: u32,
    pub published_at: Option<String>,
    pub field_count: usize,
    pub cached: bool,
}

/// The Supabase side, behind a trait so sync can be tested offline.
pub trait TemplateRemote: Send + Sync {
    fn manifest(&self, now: i64) -> Result<Value, BackendError>;
    fn download(&self, storage_path: &str, now: i64) -> Result<Vec<u8>, BackendError>;

    /// A PostgREST call (`path` is relative to `/rest/v1/`), used by editing.
    fn rest(
        &self,
        _method: &str,
        _path: &str,
        _body: Option<Value>,
        _now: i64,
    ) -> Result<Value, BackendError> {
        Err(BackendError::offline())
    }

    /// Stores a template file. Paths are content-addressed, so an existing
    /// object at the same path already holds these bytes.
    fn upload(
        &self,
        _storage_path: &str,
        _kind: FileKind,
        _bytes: &[u8],
        _now: i64,
    ) -> Result<(), BackendError> {
        Err(BackendError::offline())
    }
}

pub struct SupabaseTemplates {
    auth: Arc<AuthService>,
    json: Arc<dyn JsonHttp>,
    bytes: Arc<dyn BytesHttp>,
    config: AuthConfig,
}

impl SupabaseTemplates {
    pub fn new(
        auth: Arc<AuthService>,
        json: Arc<dyn JsonHttp>,
        bytes: Arc<dyn BytesHttp>,
        config: AuthConfig,
    ) -> Self {
        Self {
            auth,
            json,
            bytes,
            config,
        }
    }

    fn headers(&self, now: i64) -> Result<Vec<(String, String)>, BackendError> {
        let session = self.auth.session_for_cloud(now)?;
        Ok(vec![
            ("apikey".to_string(), self.config.key().to_string()),
            (
                "Authorization".to_string(),
                format!("Bearer {}", session.access_token()),
            ),
        ])
    }
}

impl TemplateRemote for SupabaseTemplates {
    fn manifest(&self, now: i64) -> Result<Value, BackendError> {
        let headers = self.headers(now)?;
        let response = self.json.send_json(
            "GET",
            &format!(
                "{}/rest/v1/doc_template_manifest?select=*&order=code.asc&limit={MAX_TEMPLATES}",
                self.config.url()
            ),
            &headers,
            None,
        )?;
        match response.status {
            200 => response
                .json()
                .map_err(|_| BackendError::request_failed("template manifest is invalid")),
            401 | 403 => Err(BackendError::auth_required()),
            404 => Err(BackendError::request_failed("templates_not_configured")),
            _ => Err(BackendError::request_failed(
                "template manifest request failed",
            )),
        }
    }

    fn download(&self, storage_path: &str, now: i64) -> Result<Vec<u8>, BackendError> {
        let headers = self.headers(now)?;
        let encoded = encode_path(storage_path);
        let response = self.bytes.send_bytes(
            "GET",
            &format!(
                "{}/storage/v1/object/authenticated/{BUCKET}/{encoded}",
                self.config.url()
            ),
            &headers,
            None,
        )?;
        if response.status != 200 {
            return Err(BackendError::request_failed("template download failed"));
        }
        Ok(response.into_bytes())
    }

    fn rest(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        now: i64,
    ) -> Result<Value, BackendError> {
        let mut headers = self.headers(now)?;
        headers.push(("Prefer".to_string(), "return=representation".to_string()));
        let response = self.json.send_json(
            method,
            &format!("{}/rest/v1/{path}", self.config.url()),
            &headers,
            body.as_ref(),
        )?;
        match response.status {
            200 | 201 => response
                .json()
                .map_err(|_| BackendError::request_failed("template response is invalid")),
            204 => Ok(Value::Null),
            401 => Err(BackendError::auth_required()),
            403 => Err(BackendError::permission_denied()),
            404 => Err(BackendError::request_failed("templates_not_configured")),
            409 => Err(BackendError::validation("error.templates.codeTaken")),
            _ => Err(BackendError::validation("error.templates.editFailed")),
        }
    }

    fn upload(
        &self,
        storage_path: &str,
        kind: FileKind,
        bytes: &[u8],
        now: i64,
    ) -> Result<(), BackendError> {
        let mut headers = self.headers(now)?;
        headers.push(("x-upsert".to_string(), "false".to_string()));
        let mime = match kind {
            FileKind::Docx => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            FileKind::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        };
        let response = self.bytes.send_bytes(
            "POST",
            &format!(
                "{}/storage/v1/object/{BUCKET}/{}",
                self.config.url(),
                encode_path(storage_path)
            ),
            &headers,
            Some((mime, bytes)),
        )?;
        let status = response.status;
        let body = response.into_bytes();
        match status {
            200 | 201 => Ok(()),
            // Storage answers 400 or 409 "Duplicate" for an existing object.
            400 | 409 if String::from_utf8_lossy(&body).contains("Duplicate") => Ok(()),
            401 => Err(BackendError::auth_required()),
            403 => Err(BackendError::permission_denied()),
            _ => Err(BackendError::validation("error.templates.uploadFailed")),
        }
    }
}

fn encode_path(storage_path: &str) -> String {
    storage_path
        .split('/')
        .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Default)]
struct SyncState {
    syncing: bool,
    online: bool,
    last_error: Option<String>,
    failed_codes: Vec<String>,
}

pub struct TemplateStore {
    root: PathBuf,
    remote: Arc<dyn TemplateRemote>,
    state: Mutex<SyncState>,
}

impl TemplateStore {
    pub fn new(root: PathBuf, remote: Arc<dyn TemplateRemote>) -> Self {
        Self {
            root,
            remote,
            state: Mutex::new(SyncState::default()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, SyncState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    fn file_path(&self, entry: &TemplateEntry) -> PathBuf {
        self.root.join("files").join(format!(
            "{}.{}",
            entry.file_sha256,
            entry.file_kind.extension()
        ))
    }

    fn read_manifest(&self) -> CachedManifest {
        fs::read(self.manifest_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CachedManifest>(&bytes).ok())
            .map(|mut manifest| {
                manifest
                    .entries
                    .retain(|entry| validate_entry(entry).is_ok());
                manifest
            })
            .unwrap_or_default()
    }

    fn write_manifest(&self, manifest: &CachedManifest) -> Result<(), BackendError> {
        fs::create_dir_all(&self.root)
            .map_err(|_| BackendError::request_failed("template cache is not writable"))?;
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|_| BackendError::protocol("template manifest encoding failed"))?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.root)
            .map_err(|_| BackendError::request_failed("template cache is not writable"))?;
        temp.write_all(&bytes)
            .and_then(|()| temp.as_file().sync_all())
            .map_err(|_| BackendError::request_failed("template cache is not writable"))?;
        temp.persist(self.manifest_path())
            .map_err(|_| BackendError::request_failed("template cache is not writable"))?;
        Ok(())
    }

    /// A cached file is usable only if it still hashes to its manifest checksum.
    fn cached_file(&self, entry: &TemplateEntry) -> Option<PathBuf> {
        let path = self.file_path(entry);
        let bytes = fs::read(&path).ok()?;
        verify_file(entry, &bytes).ok()?;
        Some(path)
    }

    fn install(&self, entry: &TemplateEntry, bytes: &[u8]) -> Result<(), BackendError> {
        verify_file(entry, bytes)?;
        let directory = self.root.join("files");
        fs::create_dir_all(&directory)
            .map_err(|_| BackendError::request_failed("template cache is not writable"))?;
        let mut temp = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|_| BackendError::request_failed("template cache is not writable"))?;
        temp.write_all(bytes)
            .and_then(|()| temp.as_file().sync_all())
            .map_err(|_| BackendError::request_failed("template cache is not writable"))?;
        match temp.persist_noclobber(self.file_path(entry)) {
            Ok(_) => Ok(()),
            // Files are content-addressed: an existing file with this name holds
            // the same bytes unless it was damaged, which `cached_file` rejects.
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => self
                .cached_file(entry)
                .map(|_| ())
                .ok_or_else(|| BackendError::request_failed("cached template file is damaged")),
            Err(_) => Err(BackendError::request_failed(
                "template cache is not writable",
            )),
        }
    }

    pub fn status(&self) -> TemplateStatus {
        let manifest = self.read_manifest();
        let state = self.state();
        TemplateStatus {
            online: state.online,
            syncing: state.syncing,
            last_synced_at: manifest.synced_at,
            last_error: state.last_error.clone(),
            failed_codes: state.failed_codes.clone(),
            templates: manifest
                .entries
                .iter()
                .map(|entry| TemplateSummary {
                    code: entry.code.clone(),
                    title: entry.title.clone(),
                    discipline: entry.discipline.clone(),
                    file_kind: entry.file_kind,
                    version: entry.version,
                    published_at: entry.published_at.clone(),
                    field_count: field_count(&entry.field_map),
                    cached: self.cached_file(entry).is_some(),
                })
                .collect(),
        }
    }

    /// Pulls the published manifest and any missing files. Offline or failed
    /// syncs keep the cached manifest, and a template whose new file fails to
    /// download keeps its previous cached version.
    pub fn sync(&self, now: i64) -> TemplateStatus {
        {
            let mut state = self.state();
            if state.syncing {
                drop(state);
                return self.status();
            }
            state.syncing = true;
        }
        let outcome = self.sync_inner(now);
        {
            let mut state = self.state();
            state.syncing = false;
            match outcome {
                Ok(failed) => {
                    state.online = true;
                    state.last_error = None;
                    state.failed_codes = failed;
                }
                Err(error) => {
                    state.online = false;
                    state.last_error = Some(sync_error_code(&error).to_string());
                }
            }
        }
        self.status()
    }

    fn sync_inner(&self, now: i64) -> Result<Vec<String>, BackendError> {
        let raw = self.remote.manifest(now)?;
        let rows = raw
            .as_array()
            .ok_or_else(|| BackendError::request_failed("template manifest is invalid"))?;
        if rows.len() > MAX_TEMPLATES {
            return Err(BackendError::request_failed(
                "template manifest is too large",
            ));
        }
        let previous = self
            .read_manifest()
            .entries
            .into_iter()
            .map(|entry| (entry.template_id.clone(), entry))
            .collect::<BTreeMap<_, _>>();
        let mut entries = Vec::new();
        let mut failed = Vec::new();
        for row in rows {
            let Ok(entry) = serde_json::from_value::<TemplateEntry>(row.clone()) else {
                failed.push(row_code(row));
                continue;
            };
            if validate_entry(&entry).is_err() {
                failed.push(entry.code);
                continue;
            }
            let ready = self.cached_file(&entry).is_some()
                || self
                    .remote
                    .download(&entry.storage_path, now)
                    .and_then(|bytes| self.install(&entry, &bytes))
                    .is_ok();
            if ready {
                entries.push(entry);
            } else {
                failed.push(entry.code.clone());
                if let Some(old) = previous.get(&entry.template_id)
                    && self.cached_file(old).is_some()
                {
                    entries.push(old.clone());
                }
            }
        }
        self.write_manifest(&CachedManifest {
            synced_at: Some(now),
            entries,
        })?;
        Ok(failed)
    }

    /// The cached file and entry for a template code, for generation.
    pub fn resolve(&self, code: &str) -> Option<(TemplateEntry, PathBuf)> {
        let entry = self
            .read_manifest()
            .entries
            .into_iter()
            .find(|entry| entry.code.eq_ignore_ascii_case(code))?;
        let path = self.cached_file(&entry)?;
        Some((entry, path))
    }
}

fn sync_error_code(error: &BackendError) -> &'static str {
    match error.code {
        "auth_required" | "session_expired" | "permission_denied" => "signedOut",
        "offline" | "network_timeout" => "offline",
        _ if error.message.contains("templates_not_configured") => "notConfigured",
        _ => "requestFailed",
    }
}

fn row_code(row: &Value) -> String {
    row.get("code")
        .and_then(Value::as_str)
        .filter(|code| code.len() <= 64)
        .unwrap_or("?")
        .to_string()
}

fn field_count(field_map: &Value) -> usize {
    field_map
        .get("fields")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn validate_entry(entry: &TemplateEntry) -> Result<(), BackendError> {
    let invalid = || BackendError::request_failed("template manifest entry is invalid");
    let code = Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{1,63}$").expect("static regex is valid");
    let hex = Regex::new(r"^[0-9a-f]{64}$").expect("static regex is valid");
    let segment = Regex::new(r"^[A-Za-z0-9._-]{1,128}$").expect("static regex is valid");
    let segments = entry.storage_path.split('/').collect::<Vec<_>>();
    let valid = code.is_match(&entry.code)
        && !entry.title.trim().is_empty()
        && entry.title.chars().count() <= 200
        && entry.discipline.len() <= 8
        && hex.is_match(&entry.file_sha256)
        && (1..=MAX_TEMPLATE_BYTES).contains(&entry.file_size)
        && entry.version > 0
        && segments.len() == 3
        && segments
            .iter()
            .all(|part| segment.is_match(part) && *part != "..")
        && segments[0] == entry.template_id
        && segments[2].starts_with(&entry.file_sha256)
        && entry.field_map.get("schema").and_then(Value::as_u64) == Some(1)
        && entry
            .field_map
            .get("fields")
            .and_then(Value::as_array)
            .is_some_and(|fields| fields.len() <= MAX_FIELDS);
    valid.then_some(()).ok_or_else(invalid)
}

fn verify_file(entry: &TemplateEntry, bytes: &[u8]) -> Result<(), BackendError> {
    let mismatch = || BackendError::request_failed("template file does not match its checksum");
    if bytes.len() as u64 != entry.file_size {
        return Err(mismatch());
    }
    let digest = Sha256::digest(bytes);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if hex != entry.file_sha256 {
        return Err(mismatch());
    }
    verify_package(entry.file_kind, bytes)
}

/// The file must open as an OOXML package of the declared kind.
pub fn verify_package(kind: FileKind, bytes: &[u8]) -> Result<(), BackendError> {
    let invalid = || BackendError::validation("template is not a valid Office file");
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
    for name in ["[Content_Types].xml", kind.main_part()] {
        archive.by_name(name).map_err(|_| invalid())?;
    }
    Ok(())
}

/// The cache root under the app data directory.
pub fn default_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("templates")
}

#[cfg(test)]
mod tests;
