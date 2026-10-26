//! Read-only inbox watcher: detects waiting work and keeps prepared Previews in a
//! queue. This module has no path to Apply, journal writes, or filesystem mutation.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

pub const SETTINGS_SECTION: &str = "watchQueue";
pub const DEFAULT_POLL_SECS: u64 = 300;
pub const MIN_POLL_SECS: u64 = 60;
pub const MAX_POLL_SECS: u64 = 3600;
pub const SETTLE_MS: i64 = 60_000;
/// Prepared plans are dropped by `plan_store` after 30 minutes; mark stale first.
pub const PLAN_FRESH_SECS: i64 = 25 * 60;
pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_QUEUE: usize = 8;

const WATCHABLE: &[(&str, &[(&str, bool)])] = &[
    ("rfi.process_scans_fast_covers", &[("source", true)]),
    ("rfi.group_files", &[("source", true)]),
    ("rfi.stage_signed_first_pages", &[("signed_scans", true)]),
    (
        "rfi.complete_status_sync",
        &[("rfi_system_root", true), ("rfi_log_workbook", false)],
    ),
];

pub fn watchable_ids() -> Vec<&'static str> {
    WATCHABLE.iter().map(|(id, _)| *id).collect()
}

pub fn is_watchable(operation_id: &str) -> bool {
    WATCHABLE.iter().any(|(id, _)| *id == operation_id)
}

/// Remembered-parameter names (and whether to walk subfolders) that hold an
/// operation's input.
pub fn input_params(operation_id: &str) -> &'static [(&'static str, bool)] {
    WATCHABLE
        .iter()
        .find(|(id, _)| *id == operation_id)
        .map_or(&[], |(_, params)| params)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchError {
    TooManyEntries,
    Unreadable,
}

impl WatchError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TooManyEntries => "watch.tooManyEntries",
            Self::Unreadable => "watch.unreadable",
        }
    }
}

#[derive(Clone, Debug)]
pub struct WatchRoot {
    pub path: PathBuf,
    pub recursive: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub fingerprint: String,
    pub entries: usize,
    pub newest_mtime_ms: i64,
}

fn ignored_name(name: &str) -> bool {
    name.starts_with('.')
        || name.starts_with("~$")
        || name.eq_ignore_ascii_case("thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
}

fn mtime_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

struct Collected {
    rows: Vec<(String, u64, i64)>,
    cap: usize,
}

impl Collected {
    fn push(&mut self, path: &Path, metadata: &fs::Metadata) -> Result<(), WatchError> {
        if self.rows.len() >= self.cap {
            return Err(WatchError::TooManyEntries);
        }
        self.rows.push((
            path.to_string_lossy().into_owned(),
            metadata.len(),
            mtime_ms(metadata),
        ));
        Ok(())
    }
}

fn walk(dir: &Path, recursive: bool, out: &mut Collected) -> Result<(), WatchError> {
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries = fs::read_dir(&current).map_err(|_| WatchError::Unreadable)?;
        for entry in entries {
            let entry = entry.map_err(|_| WatchError::Unreadable)?;
            if ignored_name(&entry.file_name().to_string_lossy()) {
                continue;
            }
            let metadata =
                fs::symlink_metadata(entry.path()).map_err(|_| WatchError::Unreadable)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if recursive {
                    pending.push(entry.path());
                }
            } else if metadata.is_file() {
                out.push(&entry.path(), &metadata)?;
            }
        }
    }
    Ok(())
}

/// Metadata-only listing; file contents are never read.
pub fn take_snapshot(roots: &[WatchRoot], cap: usize) -> Result<Snapshot, WatchError> {
    let mut out = Collected {
        rows: Vec::new(),
        cap,
    };
    for root in roots {
        let metadata = fs::symlink_metadata(&root.path).map_err(|_| WatchError::Unreadable)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            out.push(&root.path, &metadata)?;
        } else if metadata.is_dir() {
            walk(&root.path, root.recursive, &mut out)?;
        } else {
            return Err(WatchError::Unreadable);
        }
    }
    out.rows.sort();
    let mut hasher = Sha256::new();
    let mut newest = 0;
    for (path, len, mtime) in &out.rows {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(len.to_le_bytes());
        hasher.update(mtime.to_le_bytes());
        newest = newest.max(*mtime);
    }
    let fingerprint = hasher
        .finalize()
        .iter()
        .fold(String::new(), |mut text, byte| {
            use std::fmt::Write as _;
            let _ = write!(text, "{byte:02x}");
            text
        });
    Ok(Snapshot {
        fingerprint,
        entries: out.rows.len(),
        newest_mtime_ms: newest,
    })
}

#[derive(Clone, Debug, Default)]
pub struct ItemState {
    pub prepared_fingerprint: Option<String>,
    pub failed_fingerprint: Option<String>,
    pub dismissed_fingerprint: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    Nothing,
    Settling,
    Prepare,
}

pub fn decide(state: &ItemState, snapshot: &Snapshot, now_ms: i64, settle_ms: i64) -> Decision {
    if snapshot.entries == 0 {
        return Decision::Nothing;
    }
    let known = [
        &state.prepared_fingerprint,
        &state.failed_fingerprint,
        &state.dismissed_fingerprint,
    ];
    if known
        .iter()
        .any(|value| value.as_deref() == Some(snapshot.fingerprint.as_str()))
    {
        return Decision::Nothing;
    }
    if now_ms.saturating_sub(snapshot.newest_mtime_ms) < settle_ms {
        return Decision::Settling;
    }
    Decision::Prepare
}

#[derive(Clone, Debug)]
pub struct WatchSettings {
    pub enabled: Vec<String>,
    pub poll: Duration,
}

impl WatchSettings {
    pub fn from_section(section: Option<Map<String, Value>>) -> Self {
        let section = section.unwrap_or_default();
        let mut enabled: Vec<String> = Vec::new();
        if let Some(list) = section.get("enabled").and_then(Value::as_array) {
            for id in list.iter().filter_map(Value::as_str) {
                if is_watchable(id) && !enabled.iter().any(|known| known == id) {
                    enabled.push(id.to_string());
                }
            }
        }
        let poll = section
            .get("pollSeconds")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_POLL_SECS)
            .clamp(MIN_POLL_SECS, MAX_POLL_SECS);
        Self {
            enabled,
            poll: Duration::from_secs(poll),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemStatus {
    Preparing,
    Ready,
    Stale,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanSummary {
    pub plan_id: String,
    pub steps: usize,
    pub warnings: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub op_id: String,
    pub status: ItemStatus,
    pub plan_id: Option<String>,
    pub steps: usize,
    pub warnings: usize,
    pub error_code: Option<String>,
    pub prepared_at: i64,
    #[serde(skip)]
    pub fingerprint: String,
    #[serde(skip)]
    pub preview: Option<Value>,
}

#[derive(Default)]
struct Entry {
    state: ItemState,
    item: Option<QueueItem>,
}

#[derive(Default)]
pub struct WatchQueue {
    entries: Mutex<BTreeMap<String, Entry>>,
    operator_tasks: Arc<AtomicUsize>,
    ticking: Arc<AtomicBool>,
}

pub struct TickGuard {
    ticking: Arc<AtomicBool>,
}

impl Drop for TickGuard {
    fn drop(&mut self) {
        self.ticking.store(false, Ordering::Release);
    }
}

pub struct OperatorGuard {
    tasks: Arc<AtomicUsize>,
}

impl Drop for OperatorGuard {
    fn drop(&mut self) {
        self.tasks.fetch_sub(1, Ordering::AcqRel);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl WatchQueue {
    pub fn operator_guard(&self) -> OperatorGuard {
        self.operator_tasks.fetch_add(1, Ordering::AcqRel);
        OperatorGuard {
            tasks: Arc::clone(&self.operator_tasks),
        }
    }

    /// Only one check runs at a time (timer and "check now" never overlap).
    pub fn try_begin_tick(&self) -> Option<TickGuard> {
        self.ticking
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| TickGuard {
                ticking: Arc::clone(&self.ticking),
            })
    }

    pub fn operator_busy(&self) -> bool {
        self.operator_tasks.load(Ordering::Acquire) > 0
    }

    pub fn state(&self, op_id: &str) -> ItemState {
        lock(&self.entries)
            .get(op_id)
            .map(|entry| entry.state.clone())
            .unwrap_or_default()
    }

    pub fn begin_prepare(&self, op_id: &str, fingerprint: &str) {
        let mut entries = lock(&self.entries);
        if !entries.contains_key(op_id) && entries.len() >= MAX_QUEUE {
            return;
        }
        let entry = entries.entry(op_id.to_string()).or_default();
        entry.item = Some(QueueItem {
            op_id: op_id.to_string(),
            status: ItemStatus::Preparing,
            plan_id: None,
            steps: 0,
            warnings: 0,
            error_code: None,
            prepared_at: 0,
            fingerprint: fingerprint.to_string(),
            preview: None,
        });
    }

    pub fn finish_prepared(
        &self,
        op_id: &str,
        fingerprint: &str,
        summary: PlanSummary,
        now_secs: i64,
    ) {
        self.finish_prepared_with(op_id, fingerprint, summary, now_secs, None);
    }

    pub fn finish_prepared_with(
        &self,
        op_id: &str,
        fingerprint: &str,
        summary: PlanSummary,
        now_secs: i64,
        preview: Option<Value>,
    ) {
        let mut entries = lock(&self.entries);
        let Some(entry) = entries.get_mut(op_id) else {
            return;
        };
        entry.state.prepared_fingerprint = Some(fingerprint.to_string());
        entry.state.failed_fingerprint = None;
        if summary.steps == 0 {
            entry.item = None;
            return;
        }
        entry.item = Some(QueueItem {
            op_id: op_id.to_string(),
            status: ItemStatus::Ready,
            plan_id: Some(summary.plan_id),
            steps: summary.steps,
            warnings: summary.warnings,
            error_code: None,
            prepared_at: now_secs,
            fingerprint: fingerprint.to_string(),
            preview,
        });
    }

    pub fn finish_failed(&self, op_id: &str, fingerprint: &str, error_code: &str) {
        let mut entries = lock(&self.entries);
        let Some(entry) = entries.get_mut(op_id) else {
            return;
        };
        entry.state.failed_fingerprint = Some(fingerprint.to_string());
        entry.item = Some(QueueItem {
            op_id: op_id.to_string(),
            status: ItemStatus::Failed,
            plan_id: None,
            steps: 0,
            warnings: 0,
            error_code: Some(error_code.to_string()),
            prepared_at: 0,
            fingerprint: fingerprint.to_string(),
            preview: None,
        });
    }

    pub fn items(&self) -> Vec<QueueItem> {
        lock(&self.entries)
            .values()
            .filter_map(|entry| entry.item.clone())
            .collect()
    }

    pub fn item(&self, op_id: &str) -> Option<QueueItem> {
        lock(&self.entries)
            .get(op_id)
            .and_then(|entry| entry.item.clone())
    }

    pub fn ready_count(&self) -> usize {
        lock(&self.entries)
            .values()
            .filter(|entry| {
                entry
                    .item
                    .as_ref()
                    .is_some_and(|item| item.status == ItemStatus::Ready)
            })
            .count()
    }

    pub fn dismiss(&self, op_id: &str) {
        if let Some(entry) = lock(&self.entries).get_mut(op_id)
            && let Some(item) = entry.item.take()
        {
            entry.state.dismissed_fingerprint = Some(item.fingerprint);
        }
    }

    /// Forget what was prepared so the next check prepares again.
    pub fn reset(&self, op_id: &str) {
        if let Some(entry) = lock(&self.entries).get_mut(op_id) {
            entry.state = ItemState::default();
        }
    }

    fn make_stale(item: &mut QueueItem) {
        if matches!(item.status, ItemStatus::Ready) {
            item.status = ItemStatus::Stale;
            item.plan_id = None;
            item.preview = None;
        }
    }

    pub fn expire(&self, now_secs: i64) {
        for entry in lock(&self.entries).values_mut() {
            if let Some(item) = entry.item.as_mut()
                && now_secs.saturating_sub(item.prepared_at) >= PLAN_FRESH_SECS
            {
                Self::make_stale(item);
            }
        }
    }

    pub fn mark_drift(&self, op_id: &str, fingerprint: &str) {
        if let Some(item) = lock(&self.entries)
            .get_mut(op_id)
            .and_then(|entry| entry.item.as_mut())
            && item.fingerprint != fingerprint
        {
            Self::make_stale(item);
        }
    }

    /// A plan that was applied or otherwise consumed leaves the queue.
    pub fn forget_plan(&self, plan_id: &str) {
        for entry in lock(&self.entries).values_mut() {
            if entry.item.as_ref().and_then(|item| item.plan_id.as_deref()) == Some(plan_id) {
                entry.item = None;
            }
        }
    }
}

/// Resolve the existing paths stored in an operation's remembered parameters.
pub fn roots_from_saved(operation_id: &str, saved: &Map<String, Value>) -> Option<Vec<WatchRoot>> {
    let mut roots = Vec::new();
    for (name, recursive) in input_params(operation_id) {
        let path = saved.get(*name).and_then(Value::as_str)?;
        let path = PathBuf::from(path);
        if !path.exists() {
            return None;
        }
        roots.push(WatchRoot {
            path,
            recursive: *recursive,
        });
    }
    (!roots.is_empty()).then_some(roots)
}
