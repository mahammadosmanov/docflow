use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    contracts::{
        JsonObject, OperationResult, Plan, PrintQueueSummary, ProgressEvent, Step, StepKind,
        Warning,
    },
    errors::BackendError,
    fs_ops::atomic_replace,
    journal::{JournalRecord, JournalState, JournalStore},
    operations::batch_print,
    paths::{CallerIdentity, GrantAccess, GrantScope, PathGrantStore, PathIntent},
    plan_store::{BackendOrigin, PlanBinding, PlanStore},
    progress::{CancellationRegistry, GlobalMutationLock},
};

pub const DEFAULT_TIMEOUT_MINUTES: u32 = 30;

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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintSettings {
    pub paper: String,
    pub orientation: String,
    pub scaling: String,
    pub color: String,
    pub duplex: String,
    pub timeout_minutes: u32,
}

impl PrintJobSupport {
    pub fn controlled() -> Self {
        Self {
            verb: "docflow".to_string(),
            settings_limited: false,
        }
    }

    pub fn native_application() -> Self {
        Self {
            verb: "printto".to_string(),
            settings_limited: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrintFailureKind {
    Failed,
    Unsupported,
    UnsupportedOptions,
    Uncertain,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintFailure {
    pub kind: PrintFailureKind,
    pub message: String,
    pub uncertain: bool,
}

impl PrintFailure {
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            kind: PrintFailureKind::Failed,
            message: message.into(),
            uncertain: false,
        }
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self {
            kind: PrintFailureKind::Unsupported,
            message: message.into(),
            uncertain: false,
        }
    }

    pub fn unsupported_options(message: impl Into<String>) -> Self {
        Self {
            kind: PrintFailureKind::UnsupportedOptions,
            message: message.into(),
            uncertain: false,
        }
    }

    pub fn uncertain(message: impl Into<String>) -> Self {
        Self {
            kind: PrintFailureKind::Uncertain,
            message: message.into(),
            uncertain: true,
        }
    }

    pub fn cancelled(message: impl Into<String>, uncertain: bool) -> Self {
        Self {
            kind: PrintFailureKind::Cancelled,
            message: message.into(),
            uncertain,
        }
    }

    pub fn is_unsupported(&self) -> bool {
        self.kind == PrintFailureKind::Unsupported
    }

    pub fn is_unsupported_options(&self) -> bool {
        self.kind == PrintFailureKind::UnsupportedOptions
    }

    pub fn into_backend_error(self) -> BackendError {
        match self.kind {
            PrintFailureKind::Unsupported | PrintFailureKind::UnsupportedOptions => {
                BackendError::validation(self.message)
            }
            _ => BackendError::request_failed(self.message),
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PrintJob {
    pub job_id: String,
    pub path: String,
    pub printer: String,
    pub verb: String,
    pub format: String,
    pub paper: String,
    pub orientation: String,
    pub scaling: String,
    pub color: String,
    pub duplex: String,
    pub timeout_minutes: u32,
    pub retries: u32,
    pub on_error: String,
    pub copy: u32,
    #[serde(default)]
    pub signature: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for PrintJob {
    fn default() -> Self {
        Self {
            job_id: String::new(),
            path: String::new(),
            printer: String::new(),
            verb: String::new(),
            format: String::new(),
            paper: "A4".to_string(),
            orientation: "auto".to_string(),
            scaling: "fit".to_string(),
            color: "printer".to_string(),
            duplex: "printer".to_string(),
            timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
            retries: 0,
            on_error: "pause".to_string(),
            copy: 1,
            signature: Map::new(),
            status: None,
            attempts: None,
            error: None,
            completed_at: None,
            extra: Map::new(),
        }
    }
}

impl PrintJob {
    pub fn settings(&self) -> PrintSettings {
        PrintSettings {
            paper: self.paper.clone(),
            orientation: self.orientation.clone(),
            scaling: self.scaling.clone(),
            color: self.color.clone(),
            duplex: self.duplex.clone(),
            timeout_minutes: self.timeout_minutes,
        }
    }

    fn normalize_legacy(&mut self, index: usize) {
        if self.job_id.is_empty() {
            self.job_id = format!("legacy-{}", index + 1);
        }
        if self.paper.is_empty() {
            self.paper = "A4".to_string();
        }
        if self.orientation.is_empty() {
            self.orientation = "auto".to_string();
        }
        if self.scaling.is_empty() {
            self.scaling = "fit".to_string();
        }
        if self.color.is_empty() {
            self.color = "printer".to_string();
        }
        if self.duplex.is_empty() {
            self.duplex = "printer".to_string();
        }
        if self.timeout_minutes == 0 {
            self.timeout_minutes = DEFAULT_TIMEOUT_MINUTES;
        }
        if self.on_error.is_empty() {
            self.on_error = "pause".to_string();
        }
        if self.copy == 0 {
            self.copy = 1;
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
    #[serde(flatten)]
    extra: Map<String, Value>,
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
}

impl PrintQueueStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            memory: Mutex::new(HashMap::new()),
            active: Mutex::new(HashSet::new()),
            writes: Mutex::new(()),
        }
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
                serde_json::from_slice::<QueueState>(&encoded).ok()
            }
            Ok(_) => None,
            Err(_) => None,
        }
        .or_else(|| lock(&self.memory).get(queue_id).cloned())
        .ok_or_else(|| BackendError::validation("invalid saved print queue"))?;
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
            let Ok(mut state) = self.load_adopted(caller, queue_id) else {
                continue;
            };
            if state.recover_interrupted() {
                self.save(&mut state)?;
            }
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
            let Ok(mut state) = self.adopt(caller, state) else {
                continue;
            };
            if state.recover_interrupted() {
                self.save(&mut state)?;
            }
            let summary = state.summary();
            if summary.resumable > 0 {
                return Ok(Some(summary));
            }
        }
        Ok(None)
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

pub struct PrintService {
    queues: PrintQueueStore,
    journals: JournalStore,
    plans: PlanStore,
    grants: PathGrantStore,
    mutation_lock: GlobalMutationLock,
    cancellations: CancellationRegistry,
    adapter: Arc<dyn PrintAdapter>,
}

impl PrintService {
    pub fn new(
        queue_root: impl AsRef<Path>,
        journal_root: impl AsRef<Path>,
        ttl: Duration,
        mutation_lock: GlobalMutationLock,
        adapter: Arc<dyn PrintAdapter>,
    ) -> Self {
        Self {
            queues: PrintQueueStore::new(queue_root),
            journals: JournalStore::new(journal_root),
            plans: PlanStore::new(ttl),
            grants: PathGrantStore::new(ttl),
            mutation_lock,
            cancellations: CancellationRegistry::default(),
            adapter,
        }
    }

    pub fn inventory(&self) -> Result<PrinterInventory, BackendError> {
        self.adapter
            .inventory()
            .map(PrinterInventory::normalized)
            .map_err(PrintFailure::into_backend_error)
    }

    pub fn preview<F>(
        &self,
        caller: &CallerIdentity,
        params: BatchPrintParams,
        progress: F,
    ) -> Result<Plan, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let inventory = self.inventory()?;
        let plan = batch_print::preview(&params, &inventory, self.adapter.as_ref(), progress)?;
        self.cache_with_file_grants(caller, plan)
    }

    pub fn revise(
        &self,
        caller: &CallerIdentity,
        plan_id: &str,
        rows: &[Value],
    ) -> Result<Plan, BackendError> {
        let claimed = self.resolve(plan_id, caller)?;
        let binding = claimed.binding.ok_or_else(BackendError::plan_unavailable)?;
        let inventory = self.inventory()?;
        let revised = batch_print::revise(&claimed.plan, rows, &inventory, self.adapter.as_ref())?;
        self.plans
            .insert_bound(BackendOrigin::Rust, binding, revised)
    }

    pub fn resume(&self, caller: &CallerIdentity, queue_id: &str) -> Result<Plan, BackendError> {
        if self.queues.is_active(queue_id) {
            return Err(BackendError::operation_in_progress());
        }
        let mut state = self.queues.load_adopted(caller, queue_id)?;
        if state.recover_interrupted() {
            self.queues.save(&mut state)?;
        }
        let inventory = self.inventory()?;
        let value = serde_json::to_value(&state)
            .map_err(|_| BackendError::protocol("print queue encoding failed"))?;
        let plan = batch_print::resume_plan(&value, &inventory)?;
        self.cache_with_file_grants(caller, plan)
    }

    pub fn latest(
        &self,
        caller: &CallerIdentity,
    ) -> Result<Option<PrintQueueSummary>, BackendError> {
        self.queues.latest(caller)
    }

    pub fn discard(
        &self,
        caller: &CallerIdentity,
        queue_id: &str,
    ) -> Result<PrintQueueSummary, BackendError> {
        let _permit = self.mutation_lock.try_acquire()?;
        self.queues.discard(caller, queue_id)
    }

    pub fn contains_plan(&self, plan_id: &str) -> bool {
        self.plans
            .snapshot(plan_id)
            .is_some_and(|snapshot| snapshot.origin == BackendOrigin::Rust)
    }

    pub fn cancel(&self, plan_id: &str) -> bool {
        self.cancellations.cancel(plan_id)
    }

    pub fn revoke(&self, caller: &CallerIdentity) {
        self.plans.revoke(caller);
        self.grants.revoke(caller);
        self.cancellations.revoke_all();
        lock(&self.queues.memory).retain(|_, state| {
            state.owner_id != caller.owner_id || state.session_id != caller.session_id
        });
    }

    pub fn apply<F>(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        mut progress: F,
    ) -> Result<OperationResult, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let claim = self
            .plans
            .claim_for_apply_bound(plan_id, caller, &self.mutation_lock)?;
        if claim.claimed.origin != BackendOrigin::Rust
            || claim.claimed.plan.op_id != "files.batch_print"
        {
            return Err(BackendError::validation(
                "plan is not owned by the Rust print backend",
            ));
        }
        let binding = claim
            .claimed
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        let roots =
            self.grants
                .roots_for_plan(&binding.grant_ids, caller, &binding.operation_id)?;
        let root_identities = self.grants.root_identities_for_plan(
            &binding.grant_ids,
            caller,
            &binding.operation_id,
        )?;
        let token = self.cancellations.register(plan_id);
        let _registration = PrintCancellationRegistration {
            registry: &self.cancellations,
            plan_id,
        };
        let raw_jobs = claim
            .claimed
            .plan
            .meta
            .get("jobs")
            .cloned()
            .ok_or_else(|| BackendError::validation("print plan has no jobs"))?;
        let mut jobs: Vec<PrintJob> = serde_json::from_value(raw_jobs)
            .map_err(|_| BackendError::validation("print plan contains invalid jobs"))?;
        for (index, job) in jobs.iter_mut().enumerate() {
            job.normalize_legacy(index);
            batch_print::validate_job(job)?;
        }
        let mut state = self.state_for_plan(&claim.claimed.plan, &jobs, caller)?;
        self.queues.begin(&state.queue_id)?;
        let _active_queue = PrintQueueActiveRegistration {
            store: &self.queues,
            queue_id: state.queue_id.clone(),
        };
        state.status = "running".to_string();
        self.queues.save(&mut state)?;
        let mut record = JournalRecord::fresh(
            "files.batch_print",
            &binding.owner_id,
            &binding.session_id,
            &claim.claimed.digest,
        );
        record.grant_roots = roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        record.grant_root_identities = root_identities;
        record.state = Some(JournalState::Applying);
        self.journals.write(&record)?;

        let mut result = OperationResult {
            applied: 0,
            failed: 0,
            journal_id: String::new(),
            errors: Vec::new(),
            meta: JsonObject::new(),
            cloud_synced: None,
        };
        let mut paused = false;
        let mut cancelled_state = None;
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: 0,
            total: jobs.len() as u64,
            subject: String::new(),
        });
        for (index, job) in jobs.iter().enumerate() {
            if token.checkpoint().is_err() {
                paused = true;
                cancelled_state = Some(if record.steps.is_empty() {
                    JournalState::CancelledClean
                } else {
                    JournalState::CancelledPartial
                });
                result.errors.push(warning("warn.print.cancelled", ""));
                break;
            }
            progress(ProgressEvent {
                phase: "print_waiting".to_string(),
                done: index as u64,
                total: jobs.len() as u64,
                subject: display_name(&job.path),
            });
            let saved_index = state
                .jobs
                .iter()
                .position(|saved| saved.job_id == job.job_id)
                .ok_or_else(|| BackendError::validation("saved print job no longer exists"))?;
            let attempts_allowed = 1 + job.retries;
            let mut completed = false;
            for _ in 0..attempts_allowed {
                {
                    let saved = &mut state.jobs[saved_index];
                    saved.status = Some("printing".to_string());
                    saved.attempts = Some(saved.attempts.unwrap_or_default() + 1);
                    saved.error = Some(String::new());
                    state.current_job_id = Some(saved.job_id.clone());
                }
                self.queues.save(&mut state)?;
                let pending = journal_step(job);
                record.pending_step = Some(pending.clone());
                record.state = Some(JournalState::Applying);
                self.journals.write(&record)?;

                let submitted =
                    self.submit_approved_job(job, binding, caller, || token.checkpoint().is_err());
                match submitted {
                    Ok(()) => {
                        let saved = &mut state.jobs[saved_index];
                        saved.status = Some("completed".to_string());
                        saved.completed_at = Some(utc_now());
                        saved.error = Some(String::new());
                        state.current_job_id = None;
                        record.steps.push(pending);
                        record.pending_step = None;
                        self.journals.write(&record)?;
                        self.queues.save(&mut state)?;
                        result.applied += 1;
                        completed = true;
                        break;
                    }
                    Err(error) if error.kind == PrintFailureKind::Cancelled => {
                        let saved = &mut state.jobs[saved_index];
                        saved.status = Some(
                            if error.uncertain {
                                "uncertain"
                            } else {
                                "cancelled"
                            }
                            .to_string(),
                        );
                        saved.error = Some(redact_error(&error.message));
                        state.current_job_id = None;
                        state.status = "paused".to_string();
                        record.pending_step = if error.uncertain { Some(pending) } else { None };
                        record.state = Some(if error.uncertain {
                            JournalState::RecoveryRequired
                        } else if record.steps.is_empty() {
                            JournalState::CancelledClean
                        } else {
                            JournalState::CancelledPartial
                        });
                        if !error.uncertain {
                            cancelled_state = record.state;
                        }
                        self.journals.write(&record)?;
                        self.queues.save(&mut state)?;
                        result.errors.push(warning(
                            if error.uncertain {
                                "warn.print.cancelledUncertain"
                            } else {
                                "warn.print.cancelled"
                            },
                            display_name(&job.path),
                        ));
                        paused = true;
                        break;
                    }
                    Err(error) if error.kind == PrintFailureKind::Uncertain => {
                        let saved = &mut state.jobs[saved_index];
                        saved.status = Some("uncertain".to_string());
                        saved.error = Some(redact_error(&error.message));
                        state.current_job_id = None;
                        state.status = "paused".to_string();
                        record.pending_step = Some(pending);
                        record.state = Some(JournalState::RecoveryRequired);
                        self.journals.write(&record)?;
                        self.queues.save(&mut state)?;
                        result.errors.push(warning(
                            "warn.print.stateUncertain",
                            display_name(&job.path),
                        ));
                        paused = true;
                        break;
                    }
                    Err(error) => {
                        let saved = &mut state.jobs[saved_index];
                        saved.error = Some(redact_error(&error.message));
                        state.current_job_id = None;
                        record.pending_step = None;
                        self.journals.write(&record)?;
                        if saved.attempts.unwrap_or_default() < attempts_allowed
                            && error.kind == PrintFailureKind::Failed
                            && token.checkpoint().is_ok()
                        {
                            saved.status = Some("pending".to_string());
                            self.queues.save(&mut state)?;
                            continue;
                        }
                        saved.status = Some("failed".to_string());
                        self.queues.save(&mut state)?;
                        result.failed += 1;
                        result.errors.push(warning(
                            "error.print.failed",
                            format!(
                                "{}: {}",
                                display_name(&job.path),
                                redact_error(&error.message)
                            ),
                        ));
                        if job.on_error == "pause" {
                            result.errors.push(warning(
                                "error.print.queuePaused",
                                (jobs.len() - index - 1).to_string(),
                            ));
                            paused = true;
                        }
                        break;
                    }
                }
            }
            if paused {
                break;
            }
            if !completed && job.on_error != "skip" {
                break;
            }
        }

        state.current_job_id = None;
        let counts = queue_counts(&state.jobs);
        let resumable = ["pending", "failed", "cancelled", "uncertain"]
            .into_iter()
            .map(|status| counts.get(status).copied().unwrap_or_default())
            .sum::<u64>();
        state.status = if resumable == 0 {
            "completed"
        } else if paused || counts.get("pending").copied().unwrap_or_default() > 0 {
            "paused"
        } else {
            "completed_with_errors"
        }
        .to_string();
        self.queues.save(&mut state)?;
        if record.state != Some(JournalState::RecoveryRequired) {
            record.pending_step = None;
            record.state = Some(cancelled_state.unwrap_or({
                if result.failed == 0 && resumable == 0 {
                    JournalState::Committed
                } else if !record.steps.is_empty() {
                    JournalState::PartialCommitted
                } else {
                    JournalState::NoEffectRetryable
                }
            }));
            self.journals.write(&record)?;
        }
        let summary = state.summary();
        result.meta = serde_json::to_value(&summary)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        result
            .meta
            .insert("resumeAvailable".to_string(), json!(resumable > 0));
        result.meta.insert("nonUndoable".to_string(), json!(true));
        result
            .meta
            .insert("auditJournalId".to_string(), json!(record.journal_id));
        result.meta.insert(
            "effectState".to_string(),
            json!(journal_effect_state(record.state)),
        );
        progress(ProgressEvent {
            phase: "apply".to_string(),
            done: jobs.len() as u64 - counts.get("pending").copied().unwrap_or_default(),
            total: jobs.len() as u64,
            subject: String::new(),
        });
        Ok(result)
    }

    fn resolve(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
    ) -> Result<crate::plan_store::ClaimedPlan, BackendError> {
        let claimed = self.plans.resolve_bound(plan_id, caller)?;
        if claimed.origin != BackendOrigin::Rust || claimed.plan.op_id != "files.batch_print" {
            return Err(BackendError::validation(
                "plan is not owned by the Rust print backend",
            ));
        }
        Ok(claimed)
    }

    fn cache_with_file_grants(
        &self,
        caller: &CallerIdentity,
        plan: Plan,
    ) -> Result<Plan, BackendError> {
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            Uuid::new_v4().to_string(),
            "files.batch_print",
        );
        let mut candidate_paths = plan
            .steps
            .iter()
            .filter_map(|step| step.src.clone())
            .collect::<Vec<_>>();
        if let Some(jobs) = plan.meta.get("jobs").and_then(Value::as_array) {
            candidate_paths.extend(jobs.iter().filter_map(row_path));
        }
        if let Some(rows) = plan
            .meta
            .get("editor")
            .and_then(Value::as_object)
            .and_then(|editor| editor.get("rows"))
            .and_then(Value::as_array)
        {
            candidate_paths.extend(rows.iter().filter_map(row_path));
        }
        let mut parents = Vec::new();
        let mut seen = HashSet::new();
        for path in candidate_paths {
            let path = Path::new(&path);
            let Some(parent) = path.parent() else {
                return Err(BackendError::path_invalid(
                    "print file has no parent directory",
                ));
            };
            let canonical = fs::canonicalize(parent)
                .map_err(|_| BackendError::path_invalid("print file directory is unavailable"))?;
            if seen.insert(canonical.clone()) {
                parents.push(canonical);
            }
        }
        let grant_ids = parents
            .iter()
            .map(|parent| self.grants.issue(&scope, parent, GrantAccess::Read))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|grant| grant.grant_id)
            .collect::<Vec<_>>();
        self.plans.insert_bound(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                "files.batch_print",
                grant_ids,
            ),
            plan,
        )
    }

    fn state_for_plan(
        &self,
        plan: &Plan,
        jobs: &[PrintJob],
        caller: &CallerIdentity,
    ) -> Result<QueueState, BackendError> {
        let queue_id = plan
            .meta
            .get("queueId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let resume_queue_id = plan
            .meta
            .get("resumeQueueId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !resume_queue_id.is_empty() {
            let mut state = self.queues.load(resume_queue_id)?;
            state.authorize_session(caller)?;
            for planned in jobs {
                let saved = state
                    .jobs
                    .iter_mut()
                    .find(|saved| saved.job_id == planned.job_id)
                    .ok_or_else(|| BackendError::validation("saved print job no longer exists"))?;
                let attempts = saved.attempts;
                *saved = planned.clone();
                saved.status = Some("pending".to_string());
                saved.attempts = attempts;
                saved.error = Some(String::new());
            }
            state.status = "ready".to_string();
            return Ok(state);
        }
        let created = utc_now();
        Ok(QueueState {
            version: 1,
            queue_id,
            owner_id: caller.owner_id.clone(),
            session_id: caller.session_id.clone(),
            created_at: created.clone(),
            updated_at: created,
            status: "ready".to_string(),
            current_job_id: None,
            jobs: jobs
                .iter()
                .cloned()
                .map(|mut job| {
                    job.status = Some("pending".to_string());
                    job.attempts = Some(0);
                    job.error = Some(String::new());
                    job
                })
                .collect(),
            extra: Map::new(),
        })
    }

    fn submit_approved_job<C>(
        &self,
        job: &PrintJob,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        cancelled: C,
    ) -> Result<(), PrintFailure>
    where
        C: Fn() -> bool,
    {
        let canonical_path = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                Path::new(&job.path),
                PathIntent::Read,
            )
            .map_err(|error| PrintFailure::failed(error.message))?;
        let source = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &canonical_path,
                PathIntent::Read,
            )
            .map_err(|error| PrintFailure::failed(error.message))?
            .anchor()
            .map_err(|_| PrintFailure::failed("print file is unavailable"))?;
        let mut source_file = source
            .open_read()
            .map_err(|_| PrintFailure::failed("print file is unavailable"))?;
        let metadata = source_file
            .metadata()
            .map_err(|_| PrintFailure::failed("print file is unavailable"))?;
        if !metadata.is_file() {
            return Err(PrintFailure::failed("print file is unavailable"));
        }
        if !job.signature.is_empty()
            && file_signature_from_open_file(&source_file)
                .map_err(|error| PrintFailure::failed(error.message))?
                != job.signature
        {
            return Err(PrintFailure::failed(
                "file changed after the queue was previewed",
            ));
        }
        let workspace = tempfile::tempdir()
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        let workspace_capability = crate::cap_fs::CapabilityRoot::open(workspace.path())
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        let source_name = Path::new(&job.path)
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| PrintFailure::failed("print file name is unavailable"))?;
        let staged_path = workspace.path().join(source_name);
        let staged = workspace_capability
            .authorize(&staged_path)
            .and_then(|path| path.anchor())
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        staged
            .copy_no_clobber_from_file(&mut source_file)
            .map_err(|_| PrintFailure::failed("private print staging failed"))?;
        let inventory = self.adapter.inventory().map(PrinterInventory::normalized)?;
        if !inventory.contains(&job.printer) {
            return Err(PrintFailure::failed("printer is no longer available"));
        }
        let support = self
            .adapter
            .inspect(&staged_path, &job.printer, &job.settings())?;
        if support.verb != job.verb {
            return Err(PrintFailure::failed(
                "print capability changed after preview",
            ));
        }
        let mut staged_job = job.clone();
        staged_job.path = staged_path.to_string_lossy().into_owned();
        self.adapter.submit(&staged_job, &cancelled)
    }
}

fn journal_effect_state(state: Option<JournalState>) -> &'static str {
    match state {
        Some(JournalState::Committed | JournalState::Undone) => "committed",
        Some(JournalState::PartialCommitted | JournalState::UndoPartial) => "partial_committed",
        Some(JournalState::NoEffectRetryable) => "no_effect_retryable",
        Some(JournalState::CancelledClean) => "cancelled_clean",
        Some(JournalState::CancelledPartial) => "cancelled_partial",
        Some(JournalState::RecoveryRequired | JournalState::Applying) | None => "outcome_unknown",
    }
}

fn row_path(row: &Value) -> Option<String> {
    row.as_object()
        .and_then(|object| object.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

struct PrintCancellationRegistration<'a> {
    registry: &'a CancellationRegistry,
    plan_id: &'a str,
}

impl Drop for PrintCancellationRegistration<'_> {
    fn drop(&mut self) {
        self.registry.finish(self.plan_id);
    }
}

struct PrintQueueActiveRegistration<'a> {
    store: &'a PrintQueueStore,
    queue_id: String,
}

impl Drop for PrintQueueActiveRegistration<'_> {
    fn drop(&mut self) {
        self.store.finish(&self.queue_id);
    }
}

pub fn file_signature(path: &Path) -> Result<Map<String, Value>, BackendError> {
    let file =
        File::open(path).map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
    file_signature_from_open_file(&file)
}

fn file_signature_from_open_file(file: &File) -> Result<Map<String, Value>, BackendError> {
    let metadata = file
        .metadata()
        .map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
    let modified = metadata
        .modified()
        .map_err(|_| BackendError::path_invalid("print file timestamp is unavailable"))?;
    let mtime_ns = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut signature = Map::new();
    signature.insert("size".to_string(), json!(metadata.len()));
    signature.insert("mtimeNs".to_string(), json!(mtime_ns));
    Ok(signature)
}

pub fn default_queue_root(app_data_dir: &Path) -> PathBuf {
    std::env::var_os("DOCFLOW_PRINT_QUEUE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app_data_dir.join("print-queues"))
}

fn valid_printer_name(printer: &str) -> bool {
    !printer.is_empty()
        && printer.chars().count() <= 256
        && !printer
            .chars()
            .any(|character| character.is_control() || character == '\0' || character == '"')
}

fn validate_queue_id(queue_id: &str) -> Result<(), BackendError> {
    if queue_id.is_empty()
        || queue_id.len() > 128
        || !queue_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(BackendError::validation("invalid print queue id"));
    }
    Ok(())
}

fn queue_counts(jobs: &[PrintJob]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::from([
        ("pending".to_string(), 0),
        ("printing".to_string(), 0),
        ("completed".to_string(), 0),
        ("failed".to_string(), 0),
        ("cancelled".to_string(), 0),
        ("uncertain".to_string(), 0),
    ]);
    for job in jobs {
        if let Some(count) = counts.get_mut(job.status.as_deref().unwrap_or("pending")) {
            *count += 1;
        }
    }
    counts
}

fn journal_step(job: &PrintJob) -> Step {
    let mut meta = serde_json::to_value(job)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    meta.remove("path");
    Step {
        kind: StepKind::Print,
        src: Some(job.path.clone()),
        dst: Some(job.path.clone()),
        note_key: Some("note.print.queued".to_string()),
        meta,
    }
}

fn utc_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

fn display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn warning(key: &str, subject: impl Into<String>) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.into(),
    }
}

fn redact_error(message: &str) -> String {
    let single_line = message
        .chars()
        .filter(|character| !matches!(character, '\r' | '\n'))
        .take(512)
        .collect::<String>();
    if single_line.is_empty() {
        "print adapter failed".to_string()
    } else {
        single_line
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
