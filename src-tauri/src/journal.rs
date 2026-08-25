use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{contracts::Step, errors::BackendError};

const MAX_JOURNAL_BYTES: u64 = 8 * 1024 * 1024;
const MAX_JOURNAL_STEPS: usize = 100_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    Applying,
    Committed,
    PartialCommitted,
    NoEffectRetryable,
    CancelledClean,
    CancelledPartial,
    RecoveryRequired,
    Undone,
    UndoPartial,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum FilesystemObjectIdentity {
    Unix { device: u64, inode: u64 },
    Windows { volume_serial: u32, file_index: u64 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantRootIdentity {
    pub root: String,
    pub identity: FilesystemObjectIdentity,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoRecoveryPhase {
    Prepared,
    Quarantined,
    Restoring,
    Publishing,
    Deleting,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoRecovery {
    pub index: usize,
    pub original: String,
    pub quarantine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub phase: UndoRecoveryPhase,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalRecord {
    pub journal_id: String,
    pub op_id: String,
    pub ts: String,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_step: Option<Step>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<JournalState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grant_roots: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grant_root_identities: Vec<GrantRootIdentity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undone_steps: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_undo: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo_recovery: Option<UndoRecovery>,
}

impl JournalRecord {
    pub fn new(
        journal_id: impl Into<String>,
        op_id: impl Into<String>,
        ts: impl Into<String>,
        owner_id: impl Into<String>,
        session_id: impl Into<String>,
        plan_digest: impl Into<String>,
    ) -> Self {
        Self {
            journal_id: journal_id.into(),
            op_id: op_id.into(),
            ts: ts.into(),
            steps: Vec::new(),
            pending_step: None,
            state: None,
            owner_id: Some(owner_id.into()),
            session_id: Some(session_id.into()),
            plan_digest: Some(plan_digest.into()),
            grant_roots: Vec::new(),
            grant_root_identities: Vec::new(),
            undone_steps: Vec::new(),
            pending_undo: None,
            undo_recovery: None,
        }
    }

    pub fn fresh(op_id: &str, owner_id: &str, session_id: &str, plan_digest: &str) -> Self {
        Self::new(
            Uuid::new_v4().to_string(),
            op_id,
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false),
            owner_id,
            session_id,
            plan_digest,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDisposition {
    NoEffect,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntry {
    pub journal_id: String,
    pub disposition: RecoveryDisposition,
}

#[derive(Clone, Debug)]
pub struct JournalStore {
    root: PathBuf,
}

impl JournalStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write(&self, record: &JournalRecord) -> Result<(), BackendError> {
        validate_record(record)?;
        fs::create_dir_all(&self.root)
            .map_err(|error| journal_io("journal directory could not be created", error))?;
        let target = self.path_for(record);
        if target.exists() {
            let current = read_file(&target)?;
            if current.journal_id != record.journal_id {
                return Err(BackendError::journal_invalid(
                    "journal filename collision was detected",
                ));
            }
        }
        let temporary = self
            .root
            .join(format!(".docflow-journal-{}.tmp", Uuid::new_v4()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| journal_io("journal temporary file could not be created", error))?;
        let write_result = (|| -> Result<(), BackendError> {
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, record)
                .map_err(|_| BackendError::journal_invalid("journal could not be encoded"))?;
            writer
                .flush()
                .map_err(|error| journal_io("journal could not be flushed", error))?;
            writer
                .get_ref()
                .sync_all()
                .map_err(|error| journal_io("journal could not be synchronized", error))?;
            if target.exists() {
                atomic_replace(&temporary, &target)?;
            } else {
                publish_no_clobber(&temporary, &target)?;
            }
            sync_directory(&self.root)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }

    pub fn read(&self, journal_id: &str) -> Result<JournalRecord, BackendError> {
        validate_journal_id(journal_id)?;
        let entries = fs::read_dir(&self.root)
            .map_err(|error| journal_io("journal directory could not be read", error))?;
        let prefix = &journal_id[..journal_id.len().min(8)];
        let mut matching = Vec::new();
        for entry in entries {
            let entry = entry
                .map_err(|error| journal_io("journal directory entry could not be read", error))?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !filename.ends_with(&format!("_{prefix}.json")) {
                continue;
            }
            let record = read_file(&path)?;
            if record.journal_id == journal_id {
                matching.push(record);
            }
        }
        match matching.len() {
            1 => Ok(matching.remove(0)),
            0 => Err(BackendError::journal_invalid("journal was not found")),
            _ => Err(BackendError::journal_invalid(
                "duplicate local journal identities were found",
            )),
        }
    }

    pub fn recover_pending(&self) -> Result<Vec<RecoveryEntry>, BackendError> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Ok(Vec::new());
        };
        let mut recovery = Vec::new();
        for entry in entries {
            let entry = entry
                .map_err(|error| journal_io("journal directory entry could not be read", error))?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let record = read_file(&path)?;
            let disposition = if let Some(step) = record.pending_step.as_ref() {
                pending_disposition(step)
            } else if record.pending_undo.is_some() || record.undo_recovery.is_some() {
                RecoveryDisposition::OutcomeUnknown
            } else {
                continue;
            };
            recovery.push(RecoveryEntry {
                journal_id: record.journal_id,
                disposition,
            });
        }
        recovery.sort_by(|left, right| left.journal_id.cmp(&right.journal_id));
        Ok(recovery)
    }

    fn path_for(&self, record: &JournalRecord) -> PathBuf {
        let timestamp = record.ts.replace(':', "-");
        let prefix = &record.journal_id[..record.journal_id.len().min(8)];
        self.root.join(format!("{timestamp}_{prefix}.json"))
    }
}

fn pending_disposition(step: &Step) -> RecoveryDisposition {
    let src_exists = step
        .src
        .as_deref()
        .is_some_and(|path| Path::new(path).exists());
    let dst_exists = step
        .dst
        .as_deref()
        .is_some_and(|path| Path::new(path).exists());
    match step.kind {
        crate::contracts::StepKind::Mkdir if !dst_exists => RecoveryDisposition::NoEffect,
        crate::contracts::StepKind::Copy if !dst_exists => RecoveryDisposition::NoEffect,
        crate::contracts::StepKind::Move | crate::contracts::StepKind::Rename
            if src_exists && !dst_exists =>
        {
            RecoveryDisposition::NoEffect
        }
        _ => RecoveryDisposition::OutcomeUnknown,
    }
}

fn validate_record(record: &JournalRecord) -> Result<(), BackendError> {
    validate_journal_id(&record.journal_id)?;
    if record.ts.is_empty()
        || record.ts.len() > 128
        || record
            .ts
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(BackendError::journal_invalid(
            "journal timestamp is invalid",
        ));
    }
    if record.op_id.is_empty() || record.op_id.len() > 128 {
        return Err(BackendError::journal_invalid(
            "journal operation identity is invalid",
        ));
    }
    if record.steps.len() > MAX_JOURNAL_STEPS {
        return Err(BackendError::journal_invalid(
            "journal contains too many steps",
        ));
    }
    if record
        .undone_steps
        .iter()
        .any(|index| *index >= record.steps.len())
    {
        return Err(BackendError::journal_invalid(
            "journal undo progress is invalid",
        ));
    }
    if let Some(recovery) = record.undo_recovery.as_ref()
        && (record.pending_undo != Some(recovery.index) || recovery.index >= record.steps.len())
    {
        return Err(BackendError::journal_invalid(
            "journal undo recovery metadata is invalid",
        ));
    }
    Ok(())
}

fn validate_journal_id(journal_id: &str) -> Result<(), BackendError> {
    if journal_id.is_empty()
        || journal_id.len() > 128
        || !journal_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(BackendError::journal_invalid("journal identity is invalid"));
    }
    Ok(())
}

fn read_file(path: &Path) -> Result<JournalRecord, BackendError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| journal_io("journal metadata could not be read", error))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_JOURNAL_BYTES {
        return Err(BackendError::journal_invalid("journal file is invalid"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| journal_io("journal could not be read", error))?;
    let record: JournalRecord = serde_json::from_slice(&bytes)
        .map_err(|_| BackendError::journal_invalid("journal JSON is invalid"))?;
    validate_record(&record)?;
    Ok(record)
}

fn publish_no_clobber(temporary: &Path, target: &Path) -> Result<(), BackendError> {
    fs::hard_link(temporary, target)
        .map_err(|error| journal_io("journal could not be published", error))?;
    fs::remove_file(temporary)
        .map_err(|error| journal_io("journal temporary file could not be removed", error))?;
    Ok(())
}

#[cfg(unix)]
fn atomic_replace(temporary: &Path, target: &Path) -> Result<(), BackendError> {
    fs::rename(temporary, target)
        .map_err(|error| journal_io("journal could not be atomically replaced", error))
}

#[cfg(windows)]
fn atomic_replace(temporary: &Path, target: &Path) -> Result<(), BackendError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(journal_io(
            "journal could not be atomically replaced",
            io::Error::last_os_error(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), BackendError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| journal_io("journal directory could not be synchronized", error))
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), BackendError> {
    Ok(())
}

fn journal_io(context: &str, error: io::Error) -> BackendError {
    BackendError::journal_invalid(format!("{context}: {error}"))
}
