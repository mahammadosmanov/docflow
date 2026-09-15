//! Durable, owner-scoped analysis cache and scan audit ledger.
//!
//! The index is an optimization only. Mutating operations must continue to pin
//! and revalidate their own hashes and filesystem identities before Apply.

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::{cap_fs::file_identity, journal::FilesystemObjectIdentity};

const SCHEMA_VERSION: i64 = 1;
const MAX_KEY_CHARS: usize = 32_768;
const MAX_OUTCOME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileFingerprint {
    pub size: u64,
    pub modified_ns: u64,
    pub identity: FilesystemObjectIdentity,
}

impl FileFingerprint {
    pub fn read(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("open fingerprint source: {error}"))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("read fingerprint metadata: {error}"))?;
        if !metadata.is_file() {
            return Err("fingerprint source is not a regular file".to_string());
        }
        let modified_ns = metadata
            .modified()
            .map_err(|error| format!("read fingerprint modification time: {error}"))?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "fingerprint modification time predates the Unix epoch".to_string())?
            .as_nanos()
            .try_into()
            .map_err(|_| "fingerprint modification time exceeds its bound".to_string())?;
        Ok(Self {
            size: metadata.len(),
            modified_ns,
            identity: file_identity(&file)
                .map_err(|error| format!("read fingerprint identity: {error}"))?,
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanIndexStats {
    pub discovered: u64,
    pub reused: u64,
    pub analyzed: u64,
    pub failed: u64,
    pub prior_interrupted_runs: u64,
    pub force_rescan: bool,
    #[serde(default)]
    pub parallel_analysis: bool,
    #[serde(default)]
    pub analysis_workers: u64,
}

pub struct ScanRun<'a> {
    index: &'a ScanIndex,
    pub id: String,
    owner_id: String,
    operation_id: String,
    profile: String,
    root: String,
    completed: bool,
}

impl ScanRun<'_> {
    pub fn lookup<T: DeserializeOwned>(
        &self,
        relative_path: &Path,
        fingerprint: &FileFingerprint,
    ) -> Result<Option<T>, String> {
        self.index.lookup(
            &self.owner_id,
            &self.operation_id,
            &self.profile,
            &self.root,
            relative_path,
            fingerprint,
        )
    }

    pub fn record_success<T: Serialize>(
        &self,
        relative_path: &Path,
        fingerprint: &FileFingerprint,
        outcome: &T,
    ) -> Result<(), String> {
        self.index.record_success(
            &self.id,
            &self.owner_id,
            &self.operation_id,
            &self.profile,
            &self.root,
            relative_path,
            fingerprint,
            outcome,
        )
    }

    pub fn record_failure(&self, relative_path: &Path, message: &str) -> Result<(), String> {
        self.index.record_failure(
            &self.id,
            &self.owner_id,
            &self.operation_id,
            &self.profile,
            &self.root,
            relative_path,
            message,
        )
    }

    pub fn complete(mut self, stats: &ScanIndexStats) -> Result<(), String> {
        self.index.complete_run(&self.id, stats)?;
        self.completed = true;
        Ok(())
    }
}

impl Drop for ScanRun<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.index.abandon_run(&self.id);
        }
    }
}

pub struct ScanIndex {
    connection: Mutex<Connection>,
    path: PathBuf,
    instance_id: String,
}

impl ScanIndex {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create scan index directory: {error}"))?;
        }
        let connection =
            Connection::open(&path).map_err(|error| format!("open durable scan index: {error}"))?;
        let existing_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| format!("read durable scan index version: {error}"))?;
        if existing_version > SCHEMA_VERSION {
            return Err(format!(
                "scan index schema {existing_version} is newer than supported schema {SCHEMA_VERSION}"
            ));
        }
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=FULL;
                 PRAGMA foreign_keys=ON;
                 PRAGMA busy_timeout=5000;
                 CREATE TABLE IF NOT EXISTS scan_runs (
                   run_id TEXT PRIMARY KEY,
                   instance_id TEXT NOT NULL,
                   owner_id TEXT NOT NULL,
                   operation_id TEXT NOT NULL,
                   profile TEXT NOT NULL,
                   root TEXT NOT NULL,
                   started_at_ms INTEGER NOT NULL,
                   finished_at_ms INTEGER,
                   status TEXT NOT NULL CHECK(status IN ('running','completed','interrupted')),
                   force_rescan INTEGER NOT NULL,
                   discovered INTEGER NOT NULL DEFAULT 0,
                   reused INTEGER NOT NULL DEFAULT 0,
                   analyzed INTEGER NOT NULL DEFAULT 0,
                   failed INTEGER NOT NULL DEFAULT 0,
                   prior_interrupted_runs INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS scan_runs_scope
                   ON scan_runs(owner_id, operation_id, profile, root, started_at_ms);
                 CREATE TABLE IF NOT EXISTS scan_entries (
                   owner_id TEXT NOT NULL,
                   operation_id TEXT NOT NULL,
                   profile TEXT NOT NULL,
                   root TEXT NOT NULL,
                   relative_path TEXT NOT NULL,
                   fingerprint_json TEXT NOT NULL,
                   outcome_json TEXT NOT NULL,
                   updated_at_ms INTEGER NOT NULL,
                   run_id TEXT NOT NULL,
                   PRIMARY KEY(owner_id, operation_id, profile, root, relative_path)
                 );
                 CREATE TABLE IF NOT EXISTS scan_events (
                   event_id INTEGER PRIMARY KEY AUTOINCREMENT,
                   run_id TEXT NOT NULL,
                   owner_id TEXT NOT NULL,
                   operation_id TEXT NOT NULL,
                   profile TEXT NOT NULL,
                   root TEXT NOT NULL,
                   relative_path TEXT NOT NULL,
                   recorded_at_ms INTEGER NOT NULL,
                   outcome TEXT NOT NULL CHECK(outcome IN ('analyzed','failed')),
                   detail TEXT NOT NULL
                 );",
            )
            .map_err(|error| format!("initialize durable scan index: {error}"))?;
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|error| format!("version durable scan index: {error}"))?;
        Ok(Self {
            connection: Mutex::new(connection),
            path,
            instance_id: Uuid::new_v4().to_string(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn begin_run<'a>(
        &'a self,
        owner_id: &str,
        operation_id: &str,
        profile: &str,
        root: &Path,
        force_rescan: bool,
    ) -> Result<(ScanRun<'a>, u64), String> {
        validate_key(owner_id)?;
        validate_key(operation_id)?;
        validate_key(profile)?;
        let root = root.to_string_lossy().into_owned();
        validate_key(&root)?;
        let now = now_ms();
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("start scan-index transaction: {error}"))?;
        let active_here: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM scan_runs
                 WHERE owner_id=?1 AND operation_id=?2 AND profile=?3 AND root=?4
                   AND status='running' AND instance_id=?5",
                params![owner_id, operation_id, profile, root, self.instance_id],
                |row| row.get(0),
            )
            .map_err(|error| format!("check active scan runs: {error}"))?;
        if active_here != 0 {
            return Err("the same scan is already running in this app instance".to_string());
        }
        transaction
            .execute(
                "UPDATE scan_runs SET status='interrupted', finished_at_ms=?1
                 WHERE owner_id=?2 AND operation_id=?3 AND profile=?4 AND root=?5
                   AND status='running' AND instance_id<>?6",
                params![now, owner_id, operation_id, profile, root, self.instance_id],
            )
            .map_err(|error| format!("close interrupted scan runs: {error}"))?;
        let prior: u64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM scan_runs
                 WHERE owner_id=?1 AND operation_id=?2 AND profile=?3 AND root=?4
                   AND status='interrupted'
                   AND started_at_ms > COALESCE((
                     SELECT MAX(started_at_ms) FROM scan_runs
                     WHERE owner_id=?1 AND operation_id=?2 AND profile=?3 AND root=?4
                       AND status='completed'
                   ), -1)",
                params![owner_id, operation_id, profile, root],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| format!("count interrupted scan runs: {error}"))?
            .try_into()
            .unwrap_or(0);
        let id = Uuid::new_v4().to_string();
        transaction
            .execute(
                "INSERT INTO scan_runs
                 (run_id,instance_id,owner_id,operation_id,profile,root,started_at_ms,status,force_rescan,prior_interrupted_runs)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,'running',?8,?9)",
                params![id, self.instance_id, owner_id, operation_id, profile, root, now, force_rescan, count_i64(prior)],
            )
            .map_err(|error| format!("record scan run: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("commit scan run: {error}"))?;
        drop(connection);
        Ok((
            ScanRun {
                index: self,
                id,
                owner_id: owner_id.to_string(),
                operation_id: operation_id.to_string(),
                profile: profile.to_string(),
                root,
                completed: false,
            },
            prior,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn lookup<T: DeserializeOwned>(
        &self,
        owner_id: &str,
        operation_id: &str,
        profile: &str,
        root: &str,
        relative_path: &Path,
        fingerprint: &FileFingerprint,
    ) -> Result<Option<T>, String> {
        let relative = normalized_relative(relative_path)?;
        let fingerprint_json = encode_bounded(fingerprint)?;
        let connection = self.lock()?;
        let outcome = connection
            .query_row(
                "SELECT outcome_json FROM scan_entries
                 WHERE owner_id=?1 AND operation_id=?2 AND profile=?3 AND root=?4
                   AND relative_path=?5 AND fingerprint_json=?6",
                params![
                    owner_id,
                    operation_id,
                    profile,
                    root,
                    relative,
                    fingerprint_json
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("read scan index: {error}"))?;
        outcome
            .map(|value| {
                serde_json::from_str(&value)
                    .map_err(|error| format!("decode cached scan outcome: {error}"))
            })
            .transpose()
    }

    #[allow(clippy::too_many_arguments)]
    fn record_success<T: Serialize>(
        &self,
        run_id: &str,
        owner_id: &str,
        operation_id: &str,
        profile: &str,
        root: &str,
        relative_path: &Path,
        fingerprint: &FileFingerprint,
        outcome: &T,
    ) -> Result<(), String> {
        let relative = normalized_relative(relative_path)?;
        let fingerprint_json = encode_bounded(fingerprint)?;
        let outcome_json = encode_bounded(outcome)?;
        let now = now_ms();
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("start scan checkpoint: {error}"))?;
        transaction
            .execute(
                "INSERT INTO scan_entries
                 (owner_id,operation_id,profile,root,relative_path,fingerprint_json,outcome_json,updated_at_ms,run_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(owner_id,operation_id,profile,root,relative_path) DO UPDATE SET
                   fingerprint_json=excluded.fingerprint_json,
                   outcome_json=excluded.outcome_json,
                   updated_at_ms=excluded.updated_at_ms,
                   run_id=excluded.run_id",
                params![owner_id, operation_id, profile, root, relative, fingerprint_json, outcome_json, now, run_id],
            )
            .map_err(|error| format!("write scan checkpoint: {error}"))?;
        transaction
            .execute(
                "INSERT INTO scan_events
                 (run_id,owner_id,operation_id,profile,root,relative_path,recorded_at_ms,outcome,detail)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,'analyzed','')",
                params![run_id, owner_id, operation_id, profile, root, relative, now],
            )
            .map_err(|error| format!("write scan audit event: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("commit scan checkpoint: {error}"))
    }

    #[allow(clippy::too_many_arguments)]
    fn record_failure(
        &self,
        run_id: &str,
        owner_id: &str,
        operation_id: &str,
        profile: &str,
        root: &str,
        relative_path: &Path,
        message: &str,
    ) -> Result<(), String> {
        let relative = normalized_relative(relative_path)?;
        let detail = message.chars().take(2048).collect::<String>();
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO scan_events
                 (run_id,owner_id,operation_id,profile,root,relative_path,recorded_at_ms,outcome,detail)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,'failed',?8)",
                params![run_id, owner_id, operation_id, profile, root, relative, now_ms(), detail],
            )
            .map_err(|error| format!("write failed scan audit event: {error}"))?;
        Ok(())
    }

    fn complete_run(&self, run_id: &str, stats: &ScanIndexStats) -> Result<(), String> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE scan_runs SET status='completed', finished_at_ms=?1,
                   discovered=?2,reused=?3,analyzed=?4,failed=?5,prior_interrupted_runs=?6
                 WHERE run_id=?7 AND status='running'",
                params![
                    now_ms(),
                    count_i64(stats.discovered),
                    count_i64(stats.reused),
                    count_i64(stats.analyzed),
                    count_i64(stats.failed),
                    count_i64(stats.prior_interrupted_runs),
                    run_id
                ],
            )
            .map_err(|error| format!("finish scan run: {error}"))?;
        if changed != 1 {
            return Err("scan run was not active while completing".to_string());
        }
        Ok(())
    }

    fn abandon_run(&self, run_id: &str) -> Result<(), String> {
        let connection = self.lock()?;
        connection
            .execute(
                "UPDATE scan_runs SET status='interrupted', finished_at_ms=?1
                 WHERE run_id=?2 AND status='running'",
                params![now_ms(), run_id],
            )
            .map_err(|error| format!("mark scan run interrupted: {error}"))?;
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "scan index lock is poisoned".to_string())
    }
}

fn normalized_relative(path: &Path) -> Result<String, String> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("scan index path must be relative and confined".to_string());
    }
    let value = path.to_string_lossy().replace('\\', "/");
    validate_key(&value)?;
    Ok(value)
}

fn validate_key(value: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().count() > MAX_KEY_CHARS || value.contains('\0') {
        return Err("scan index key is invalid or exceeds its bound".to_string());
    }
    Ok(())
}

fn encode_bounded<T: Serialize>(value: &T) -> Result<String, String> {
    let encoded = serde_json::to_string(value)
        .map_err(|error| format!("encode scan index value: {error}"))?;
    if encoded.len() > MAX_OUTCOME_BYTES {
        return Err("scan index value exceeds its storage bound".to_string());
    }
    Ok(encoded)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn count_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct Outcome {
        value: String,
    }

    #[test]
    fn completed_items_survive_restart_and_changed_files_miss() {
        let fixture = tempfile::tempdir().unwrap();
        let db = fixture.path().join("scan-index.sqlite3");
        let root = fixture.path().join("root");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("MEC/item.pdf");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"first").unwrap();
        let fingerprint = FileFingerprint::read(&file).unwrap();
        {
            let index = ScanIndex::new(&db).unwrap();
            let (run, prior) = index
                .begin_run("owner", "operation", "profile-v1", &root, false)
                .unwrap();
            assert_eq!(prior, 0);
            run.record_success(
                Path::new("MEC/item.pdf"),
                &fingerprint,
                &Outcome { value: "ok".into() },
            )
            .unwrap();
            run.complete(&ScanIndexStats {
                discovered: 1,
                analyzed: 1,
                ..ScanIndexStats::default()
            })
            .unwrap();
        }
        let index = ScanIndex::new(&db).unwrap();
        let (run, _) = index
            .begin_run("owner", "operation", "profile-v1", &root, false)
            .unwrap();
        assert_eq!(
            run.lookup::<Outcome>(Path::new("MEC/item.pdf"), &fingerprint)
                .unwrap(),
            Some(Outcome { value: "ok".into() })
        );
        fs::write(&file, b"changed bytes").unwrap();
        let changed = FileFingerprint::read(&file).unwrap();
        assert_eq!(
            run.lookup::<Outcome>(Path::new("MEC/item.pdf"), &changed)
                .unwrap(),
            None
        );
    }

    #[test]
    fn new_run_marks_abandoned_run_interrupted() {
        let fixture = tempfile::tempdir().unwrap();
        let index = ScanIndex::new(fixture.path().join("scan-index.sqlite3")).unwrap();
        let root = fixture.path();
        let (abandoned, prior) = index
            .begin_run("owner", "operation", "profile-v1", root, false)
            .unwrap();
        assert_eq!(prior, 0);
        drop(abandoned);
        let (_replacement, prior) = index
            .begin_run("owner", "operation", "profile-v1", root, false)
            .unwrap();
        assert_eq!(prior, 1);
    }
}
