use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use crate::{
    contracts::{
        JsonObject, OperationResult, Plan, PrintQueueSummary, ProgressEvent, Step, StepKind,
        Warning,
    },
    diagnostics::{PrintJobDiagnostic, PrintQueueDiagnostic, PrinterDiagnostic},
    errors::BackendError,
    fs_ops::atomic_replace,
    journal::{
        CleanupReviewEvidence, FilesystemObjectIdentity, GrantRootIdentity, JournalCleanupReview,
        JournalRecord, JournalState, JournalStore,
    },
    ooxml::{
        OrderedWorkbookDiscovery, WorkbookSafetyFacts, WorksheetDescriptor,
        ordered_workbook_discovery,
    },
    operations::batch_print,
    page_range::{PageRangeCapability, canonical_for},
    paths::{
        CallerIdentity, GrantAccess, GrantScope, PathGrantStore, PathIntent, ProvisionalPathGrants,
        VerifiedPersistedSource,
    },
    plan_store::{BackendOrigin, PlanBinding, PlanStore},
    progress::{CancellationRegistry, CancellationToken, GlobalMutationLock},
};

pub const DEFAULT_TIMEOUT_MINUTES: u32 = 30;

fn default_page_range() -> String {
    "all".to_string()
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FileSignature {
    pub size: u64,
    pub mtime_ns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl FileSignature {
    fn is_legacy_empty(&self) -> bool {
        self.size == 0 && self.mtime_ns == 0 && self.sha256.is_none()
    }

    pub fn has_valid_sha256(&self) -> bool {
        self.sha256.as_deref().is_some_and(|sha256| {
            sha256.len() == 64
                && sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PrinterInventory {
    pub printers: Vec<String>,
    pub default_printer: Option<String>,
}

impl PrinterInventory {
    pub fn normalized(mut self) -> Self {
        self.printers.retain(|printer| valid_printer_name(printer));
        self.printers.sort_by_key(|printer| printer.to_lowercase());
        self.printers
            .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        if let Some(default) = self.default_printer.as_ref()
            && let Some(index) = self.printers.iter().position(|printer| printer == default)
        {
            let default = self.printers.remove(index);
            self.printers.insert(0, default);
        }
        if self
            .default_printer
            .as_ref()
            .is_some_and(|default| !self.printers.contains(default))
        {
            self.default_printer = None;
        }
        self
    }

    pub fn contains(&self, printer: &str) -> bool {
        valid_printer_name(printer) && self.printers.iter().any(|known| known == printer)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintJobSupport {
    pub verb: String,
    pub settings_limited: bool,
    pub page_range: PageRangeCapability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintSettings {
    pub paper: String,
    pub orientation: String,
    pub scaling: String,
    pub color: String,
    pub duplex: String,
    pub page_range: String,
    pub timeout_minutes: u32,
}

impl PrintJobSupport {
    pub fn controlled() -> Self {
        Self {
            verb: "docflow".to_string(),
            settings_limited: false,
            page_range: PageRangeCapability::AllOnly,
        }
    }

    pub fn controlled_pdf(page_count: Option<u32>) -> Self {
        Self {
            verb: "docflow".to_string(),
            settings_limited: false,
            page_range: PageRangeCapability::Pdf { page_count },
        }
    }

    pub fn single_page() -> Self {
        Self {
            verb: "docflow".to_string(),
            settings_limited: false,
            page_range: PageRangeCapability::SinglePage,
        }
    }

    pub fn native_application() -> Self {
        Self {
            verb: "printto".to_string(),
            settings_limited: true,
            page_range: PageRangeCapability::AllOnly,
        }
    }
}

pub trait PrintAdapter: Send + Sync {
    fn inventory(&self) -> Result<PrinterInventory, PrintFailure>;
    fn inspect(
        &self,
        path: &Path,
        printer: &str,
        settings: &PrintSettings,
    ) -> Result<PrintJobSupport, PrintFailure>;
    fn submit(&self, job: &PrintJob, cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure>;
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BatchPrintParams {
    pub files: Vec<PathBuf>,
    pub printer: String,
    pub paper: String,
    pub orientation: String,
    pub scaling: String,
    pub color: String,
    pub duplex: String,
    pub copies: u32,
    pub collate: bool,
    pub retries: u32,
    pub on_error: String,
    pub timeout_minutes: u32,
}

impl Default for BatchPrintParams {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            printer: String::new(),
            paper: "A4".to_string(),
            orientation: "auto".to_string(),
            scaling: "fit".to_string(),
            color: "printer".to_string(),
            duplex: "printer".to_string(),
            copies: 1,
            collate: true,
            retries: 0,
            on_error: "pause".to_string(),
            timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueState {
    version: u32,
    queue_id: String,
    #[serde(default)]
    owner_id: String,
    #[serde(default)]
    session_id: String,
    created_at: String,
    updated_at: String,
    status: String,
    current_job_id: Option<String>,
    jobs: Vec<PrintJob>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    grant_root_identities: Vec<GrantRootIdentity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    source_identities: Vec<QueueSourceIdentity>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueSourceIdentity {
    path: String,
    canonical_path: String,
    identity: FilesystemObjectIdentity,
}

impl QueueState {
    fn authorize_owner(&self, caller: &CallerIdentity) -> Result<(), BackendError> {
        if self.owner_id.is_empty() || self.owner_id != caller.owner_id {
            return Err(BackendError::plan_unavailable());
        }
        Ok(())
    }

    fn authorize_session(&self, caller: &CallerIdentity) -> Result<(), BackendError> {
        self.authorize_owner(caller)?;
        if self.session_id.is_empty() || self.session_id != caller.session_id {
            return Err(BackendError::plan_unavailable());
        }
        Ok(())
    }

    fn recover_interrupted(&mut self) -> bool {
        let mut changed = false;
        for job in &mut self.jobs {
            if job.status.as_deref() == Some("printing") {
                job.status = Some("uncertain".to_string());
                job.error = Some("application closed while this job was active".to_string());
                changed = true;
            }
        }
        if self.status == "running" {
            self.status = "paused".to_string();
            changed = true;
        }
        if changed {
            self.current_job_id = None;
        }
        changed
    }

    fn summary(&self) -> PrintQueueSummary {
        let counts = queue_counts(&self.jobs);
        let resumable = ["pending", "failed", "cancelled", "uncertain"]
            .into_iter()
            .map(|status| counts.get(status).copied().unwrap_or_default())
            .sum();
        let file_count = self
            .jobs
            .iter()
            .filter(|job| !job.path.is_empty())
            .map(|job| &job.path)
            .collect::<HashSet<_>>()
            .len() as u64;
        PrintQueueSummary {
            queue_id: self.queue_id.clone(),
            status: self.status.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            counts,
            resumable,
            file_count,
        }
    }
}

pub struct PrintQueueStore {
    root: PathBuf,
    memory: Mutex<HashMap<String, QueueState>>,
    active: Mutex<HashSet<String>>,
    writes: Mutex<()>,
    #[cfg(test)]
    save_fail_on: AtomicUsize,
    #[cfg(test)]
    save_count: AtomicUsize,
}

impl PrintQueueStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            memory: Mutex::new(HashMap::new()),
            active: Mutex::new(HashSet::new()),
            writes: Mutex::new(()),
            #[cfg(test)]
            save_fail_on: AtomicUsize::new(0),
            #[cfg(test)]
            save_count: AtomicUsize::new(0),
        }
    }

    #[cfg(test)]
    fn fail_on_save(&self, save_number: usize) {
        self.save_fail_on.store(save_number, AtomicOrdering::SeqCst);
        self.save_count.store(0, AtomicOrdering::SeqCst);
    }

    fn begin(&self, queue_id: &str) -> Result<(), BackendError> {
        let mut active = lock(&self.active);
        if !active.insert(queue_id.to_string()) {
            return Err(BackendError::operation_in_progress());
        }
        Ok(())
    }

    fn finish(&self, queue_id: &str) {
        lock(&self.active).remove(queue_id);
    }

    fn is_active(&self, queue_id: &str) -> bool {
        lock(&self.active).contains(queue_id)
    }

    fn save(&self, state: &mut QueueState) -> Result<(), BackendError> {
        state.updated_at = utc_now();
        validate_queue_id(&state.queue_id)?;
        let encoded = serde_json::to_vec(state)
            .map_err(|_| BackendError::journal_invalid("print queue could not be encoded"))?;
        if encoded.len() > 16 * 1024 * 1024 {
            return Err(BackendError::validation(
                "print queue exceeds the size limit",
            ));
        }
        #[cfg(test)]
        if self.save_count.fetch_add(1, AtomicOrdering::SeqCst) + 1
            == self.save_fail_on.load(AtomicOrdering::SeqCst)
        {
            return Err(BackendError::request_failed(
                "injected print queue save failure",
            ));
        }
        let _write = lock(&self.writes);
        fs::create_dir_all(&self.root)
            .map_err(|_| BackendError::request_failed("print queue directory is unavailable"))?;
        let target = self.path(&state.queue_id)?;
        let temporary = self.root.join(format!(
            ".{}.json.{}.tmp",
            state.queue_id,
            Uuid::new_v4().simple()
        ));
        let write_result = (|| -> io::Result<()> {
            let file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            let mut writer = BufWriter::new(file);
            writer.write_all(&encoded)?;
            writer.flush()?;
            writer.get_ref().sync_all()?;
            drop(writer);
            atomic_replace(&temporary, &target)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result.map_err(|_| BackendError::request_failed("print queue could not be saved"))?;
        lock(&self.memory).insert(state.queue_id.clone(), state.clone());
        Ok(())
    }

    fn load(&self, queue_id: &str) -> Result<QueueState, BackendError> {
        let path = self.path(queue_id)?;
        let decoded = match fs::read(&path) {
            Ok(encoded) if encoded.len() <= 16 * 1024 * 1024 => {
                serde_json::from_slice::<QueueState>(&encoded)
                    .map_err(|_| BackendError::validation("invalid saved print queue"))?
            }
            Ok(_) => return Err(BackendError::validation("invalid saved print queue")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => lock(&self.memory)
                .get(queue_id)
                .cloned()
                .ok_or_else(|| BackendError::validation("invalid saved print queue"))?,
            Err(_) => return Err(BackendError::validation("invalid saved print queue")),
        };
        if decoded.queue_id != queue_id || decoded.jobs.len() > 25_000 {
            return Err(BackendError::validation("invalid saved print queue"));
        }
        Ok(decoded)
    }

    fn adopt(
        &self,
        caller: &CallerIdentity,
        mut state: QueueState,
    ) -> Result<QueueState, BackendError> {
        state.authorize_owner(caller)?;
        if state.session_id != caller.session_id {
            state.session_id = caller.session_id.clone();
            self.save(&mut state)?;
        }
        state.authorize_session(caller)?;
        Ok(state)
    }

    fn load_adopted(
        &self,
        caller: &CallerIdentity,
        queue_id: &str,
    ) -> Result<QueueState, BackendError> {
        self.adopt(caller, self.load(queue_id)?)
    }

    fn path(&self, queue_id: &str) -> Result<PathBuf, BackendError> {
        validate_queue_id(queue_id)?;
        Ok(self.root.join(format!("{queue_id}.json")))
    }

    pub fn latest(
        &self,
        caller: &CallerIdentity,
    ) -> Result<Option<PrintQueueSummary>, BackendError> {
        let mut candidates = match fs::read_dir(&self.root) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.path().extension().and_then(|value| value.to_str()) == Some("json")
                })
                .filter_map(|entry| {
                    let modified = entry.metadata().ok()?.modified().ok()?;
                    Some((modified, entry.path()))
                })
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
        let mut seen = HashSet::new();
        for (_, path) in candidates {
            let Some(queue_id) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            seen.insert(queue_id.to_string());
            if self.is_active(queue_id) {
                continue;
            }
            let Ok(mut state) = self.load(queue_id) else {
                continue;
            };
            if state.authorize_owner(caller).is_err() {
                continue;
            }
            state.recover_interrupted();
            let summary = state.summary();
            if summary.resumable > 0 {
                return Ok(Some(summary));
            }
        }
        let mut memory = lock(&self.memory)
            .values()
            .filter(|state| !seen.contains(&state.queue_id))
            .cloned()
            .collect::<Vec<_>>();
        memory.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        for state in memory {
            if self.is_active(&state.queue_id) {
                continue;
            }
            let mut state = state;
            if state.authorize_owner(caller).is_err() {
                continue;
            }
            state.recover_interrupted();
            let summary = state.summary();
            if summary.resumable > 0 {
                return Ok(Some(summary));
            }
        }
        Ok(None)
    }

    fn latest_owned_state(
        &self,
        caller: &CallerIdentity,
    ) -> Result<Option<QueueState>, BackendError> {
        let mut candidates = match fs::read_dir(&self.root) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.path().extension().and_then(|value| value.to_str()) == Some("json")
                })
                .filter_map(|entry| {
                    let modified = entry.metadata().ok()?.modified().ok()?;
                    Some((modified, entry.path()))
                })
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
        let mut seen = HashSet::new();
        for (_, path) in candidates {
            let Some(queue_id) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            seen.insert(queue_id.to_string());
            let Ok(state) = self.load(queue_id) else {
                continue;
            };
            if state.authorize_owner(caller).is_ok() {
                return Ok(Some(state));
            }
        }
        let mut memory = lock(&self.memory)
            .values()
            .filter(|state| !seen.contains(&state.queue_id))
            .cloned()
            .collect::<Vec<_>>();
        memory.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(memory
            .into_iter()
            .find(|state| state.authorize_owner(caller).is_ok()))
    }

    pub fn discard(
        &self,
        caller: &CallerIdentity,
        queue_id: &str,
    ) -> Result<PrintQueueSummary, BackendError> {
        if self.is_active(queue_id) {
            return Err(BackendError::operation_in_progress());
        }
        let mut state = self.load_adopted(caller, queue_id)?;
        state.recover_interrupted();
        for job in &mut state.jobs {
            if matches!(
                job.status.as_deref(),
                Some("pending" | "failed" | "cancelled" | "uncertain")
            ) {
                job.status = Some("abandoned".to_string());
            }
        }
        state.status = "abandoned".to_string();
        state.current_job_id = None;
        self.save(&mut state)?;
        Ok(state.summary())
    }
}

mod apply;
mod binding;
mod failure;
mod job;
mod service;
mod support;
mod verify;
pub use failure::*;
pub use job::*;
pub use service::*;
pub use support::*;

#[cfg(test)]
mod tests;
