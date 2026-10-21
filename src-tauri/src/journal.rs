use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    cap_fs::ArtifactParentCreateOutcome,
    contracts::{Plan, Step},
    errors::BackendError,
    paths::ValidatedGrantRootRoster,
    plan_store::{ClaimedS5Apply, PlanBinding},
    progress::GlobalMutationPermit,
};

const MAX_JOURNAL_BYTES: u64 = 8 * 1024 * 1024;
const MAX_JOURNAL_STEPS: usize = 100_000;
const MAX_CLEANUP_REVIEW_EVIDENCE_PER_JOB: usize = 16;
pub(crate) const MAX_RETAINED_ARTIFACTS_PER_JOURNAL: usize = 8;
pub(crate) const MAX_RECOVERY_LOCATOR_SCALARS: usize = 512;
pub(crate) const MAX_S5_RETAINED_FILES: usize = 256;
pub(crate) const MAX_S5_RETAINED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub(crate) const MAX_JOURNAL_ROOT_ENTRIES_SCANNED: usize = 4096;
pub(crate) const MAX_PENDING_S5_RECORDS: usize = 256;
pub(crate) const MAX_INVALID_S5_NOTICES: usize = 64;
pub(crate) const MAX_INVALID_S5_NOTICE_BYTES: u64 = 4096;
pub(crate) const MAX_RECOVERY_ENTRIES: usize = 256;

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
pub enum PersistedRootAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantRootAccessRecord {
    pub(crate) root: String,
    pub(crate) identity: FilesystemObjectIdentity,
    pub(crate) access: PersistedRootAccess,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedPublicationPurpose {
    Apply,
    Undo,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedPublicationPhase {
    Prepared,
    Displacing,
    Retained,
    CreatingCanonical,
    CanonicalSecured,
    CanonicalAuxiliaryReady,
    WritingCanonical,
    VerifyingCanonical,
    Applied,
    RestoringOriginal,
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedArtifactRole {
    Backup,
    PrivateOutput,
    DisplacedOriginal,
    DisplacedApplied,
}

impl RetainedArtifactRole {
    pub(crate) fn projection_key(self) -> &'static str {
        match self {
            Self::Backup => "backup",
            Self::PrivateOutput => "privateOutput",
            Self::DisplacedOriginal => "displacedOriginal",
            Self::DisplacedApplied => "displacedApplied",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowUpJournalPhase {
    ArtifactParentReady,
    PrivateOutputReady,
    BackupReady,
    PublicationActive,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct S5AuthorityRecord {
    pub(crate) initial_step: Step,
    pub(crate) source_path: String,
    pub(crate) source_identity: FilesystemObjectIdentity,
    pub(crate) target_path: String,
    pub(crate) target_identity: FilesystemObjectIdentity,
    pub(crate) approved_updates_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingArtifactParent {
    pub(crate) path: String,
    pub(crate) locator: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatedArtifactParent {
    pub(crate) path: String,
    pub(crate) locator: String,
    pub(crate) identity: FilesystemObjectIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingArtifactCreationState {
    Intent,
    Created(FilesystemObjectIdentity),
    CreatedIdentityUnknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingRetainedArtifact {
    pub(crate) role: RetainedArtifactRole,
    pub(crate) path: String,
    pub(crate) locator: String,
    pub(crate) creation_state: PendingArtifactCreationState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetainedArtifactEvidence {
    pub(crate) role: RetainedArtifactRole,
    pub(crate) path: String,
    pub(crate) identity: FilesystemObjectIdentity,
    pub(crate) sha256: String,
    pub(crate) security_sha256: String,
    pub(crate) default_stream_bytes: u64,
    pub(crate) auxiliary_charge_bytes: u64,
    pub(crate) logical_charge_bytes: u64,
    pub(crate) locator: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetainedPublication {
    pub(crate) purpose: RetainedPublicationPurpose,
    pub(crate) canonical: String,
    pub(crate) retained: String,
    pub(crate) backup: String,
    pub(crate) publication_source: String,
    pub(crate) expected_canonical_identity: FilesystemObjectIdentity,
    pub(crate) expected_canonical_sha256: String,
    pub(crate) expected_backup_identity: FilesystemObjectIdentity,
    pub(crate) expected_backup_sha256: String,
    pub(crate) publication_source_identity: FilesystemObjectIdentity,
    pub(crate) publication_sha256: String,
    pub(crate) expected_canonical_security_sha256: String,
    pub(crate) expected_backup_security_sha256: String,
    pub(crate) publication_security_sha256: String,
    pub(crate) expected_canonical_default_stream_bytes: u64,
    pub(crate) expected_canonical_auxiliary_charge_bytes: u64,
    pub(crate) expected_canonical_logical_charge_bytes: u64,
    pub(crate) destination_access_sha256: String,
    pub(crate) destination_auxiliary_sha256: String,
    pub(crate) destination_auxiliary_charge_bytes: u64,
    pub(crate) retained_identity: Option<FilesystemObjectIdentity>,
    pub(crate) retained_sha256: Option<String>,
    pub(crate) retained_security_sha256: Option<String>,
    pub(crate) created_canonical_identity: Option<FilesystemObjectIdentity>,
    pub(crate) phase: RetainedPublicationPhase,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryNotice {
    pub warning_key: String,
    pub locator: String,
}

impl RecoveryNotice {
    pub(crate) fn from_warning(warning: &crate::contracts::Warning) -> Self {
        Self {
            warning_key: warning.key.clone(),
            locator: warning.subject.clone(),
        }
    }
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReviewEntry {
    pub warning_key: String,
    pub locator: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CleanupReviewEvidence {
    entries: Vec<CleanupReviewEntry>,
}

impl CleanupReviewEvidence {
    pub(crate) fn single(warning_key: impl Into<String>, locator: impl Into<String>) -> Self {
        Self {
            entries: vec![CleanupReviewEntry {
                warning_key: warning_key.into(),
                locator: locator.into(),
            }],
        }
    }

    pub(crate) fn append(
        &mut self,
        warning_key: impl Into<String>,
        locator: impl Into<String>,
    ) -> Result<bool, ()> {
        let locator = locator.into();
        if self.entries.iter().any(|entry| entry.locator == locator) {
            return Ok(false);
        }
        if self.entries.len() >= MAX_CLEANUP_REVIEW_EVIDENCE_PER_JOB {
            return Err(());
        }
        self.entries.push(CleanupReviewEntry {
            warning_key: warning_key.into(),
            locator,
        });
        Ok(true)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &CleanupReviewEntry> {
        self.entries.iter()
    }
}

impl std::ops::Deref for CleanupReviewEvidence {
    type Target = CleanupReviewEntry;

    fn deref(&self) -> &Self::Target {
        self.entries
            .last()
            .expect("cleanup-review evidence is never empty")
    }
}

impl Serialize for CleanupReviewEvidence {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.entries.as_slice() {
            [single] => single.serialize(serializer),
            entries => entries.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for CleanupReviewEvidence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum WireEvidence {
            Single(CleanupReviewEntry),
            Multiple(Vec<CleanupReviewEntry>),
        }

        let entries = match WireEvidence::deserialize(deserializer)? {
            WireEvidence::Single(entry) => vec![entry],
            WireEvidence::Multiple(entries) => entries,
        };
        if entries.is_empty() {
            return Err(serde::de::Error::custom(
                "cleanup review evidence must not be empty",
            ));
        }
        let mut deduplicated = Vec::with_capacity(entries.len());
        for entry in entries {
            if deduplicated
                .iter()
                .all(|existing: &CleanupReviewEntry| existing.locator != entry.locator)
            {
                deduplicated.push(entry);
            }
        }
        if deduplicated.len() > MAX_CLEANUP_REVIEW_EVIDENCE_PER_JOB {
            return Err(serde::de::Error::custom(format!(
                "cleanup review evidence limit is {MAX_CLEANUP_REVIEW_EVIDENCE_PER_JOB} distinct locators"
            )));
        }
        Ok(Self {
            entries: deduplicated,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JournalCleanupReview {
    pub(crate) job_id: String,
    pub(crate) warning_key: String,
    pub(crate) locator: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) cleanup_reviews: Vec<JournalCleanupReview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) s5_authority: Option<S5AuthorityRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending_publication: Option<RetainedPublication>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending_artifact_parent: Option<PendingArtifactParent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) created_artifact_parent: Option<CreatedArtifactParent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending_retained_artifact: Option<PendingRetainedArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) retained_evidence: Vec<RetainedArtifactEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recovery_notice: Option<RecoveryNotice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) follow_up_phase: Option<FollowUpJournalPhase>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) grant_root_accesses: Vec<GrantRootAccessRecord>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) revision: u64,
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
            cleanup_reviews: Vec::new(),
            s5_authority: None,
            pending_publication: None,
            pending_artifact_parent: None,
            created_artifact_parent: None,
            pending_retained_artifact: None,
            retained_evidence: Vec::new(),
            recovery_notice: None,
            follow_up_phase: None,
            grant_root_accesses: Vec::new(),
            revision: 0,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    #[serde(default)]
    pub restore_original_available: bool,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug)]
pub(crate) struct InterprocessMutationPermit {
    _file: File,
}

#[derive(Debug)]
pub(crate) struct S5PermitBundle {
    _process: GlobalMutationPermit,
    _interprocess: InterprocessMutationPermit,
}

pub(crate) struct PendingS5Undo {
    record: JournalRecord,
    permits: S5PermitBundle,
    mode: S5UndoMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum S5UndoMode {
    CommittedApply,
    RestoreFailedApply,
}

impl PendingS5Undo {
    pub(crate) fn mode(&self) -> S5UndoMode {
        self.mode
    }
}

impl PendingS5Undo {
    pub(crate) fn recorded_root_accesses(&self) -> &[GrantRootAccessRecord] {
        &self.record.grant_root_accesses
    }
}

enum S5MutationLease {
    Apply(Box<ClaimedS5Apply>),
    Undo {
        _permits: S5PermitBundle,
        roots: ValidatedGrantRootRoster,
        mode: S5UndoMode,
    },
    Recovery {
        roots: ValidatedGrantRootRoster,
    },
    #[cfg(test)]
    Test {
        _permits: S5PermitBundle,
    },
}

pub(crate) struct ValidatedS5Record {
    record: JournalRecord,
    lease: S5MutationLease,
}

pub(crate) struct RetainedLocators(Vec<String>);

pub(crate) struct S5RecoverySession {
    store: JournalStore,
    _permits: S5PermitBundle,
    entries: VecDeque<PendingS5Entry>,
}

enum PendingS5Entry {
    Valid(Box<JournalRecord>),
    Legacy(RecoveryEntry),
}

impl RetainedLocators {
    pub(crate) fn first(&self) -> &str {
        self.0
            .first()
            .expect("validated retained locator lists are nonempty")
    }

    pub(crate) fn as_slice(&self) -> &[String] {
        &self.0
    }
}

pub(crate) enum S5Transition {
    SetPendingArtifactParent(PendingArtifactParent),
    ResolveArtifactParent(ArtifactParentCreateOutcome),
    SetPendingRetainedArtifact(PendingRetainedArtifact),
    MarkPendingRetainedCreated(FilesystemObjectIdentity),
    MarkPendingRetainedIdentityUnknown,
    FinalizeRetainedArtifact {
        evidence: RetainedArtifactEvidence,
        phase: FollowUpJournalPhase,
    },
    InitializePublication(Box<RetainedPublication>),
    RecordRetainedPublicationEvidence(RetainedArtifactEvidence),
    RecordCanonicalSecured {
        identity: FilesystemObjectIdentity,
        descriptor_sha256: String,
    },
    RecordCanonicalAuxiliaryReady {
        auxiliary_sha256: String,
        auxiliary_charge_bytes: u64,
    },
    MarkCanonicalBytesStarted,
    MarkCanonicalVerifying,
    FinalizeApplySuccess {
        completed_step: Step,
        evidence: Vec<RetainedArtifactEvidence>,
    },
    FinalizeUndoSuccess {
        completed_step: Step,
        evidence: Vec<RetainedArtifactEvidence>,
    },
    BeginFailedApplyRestore,
    FinalizeFailedApplyRestore,
    FinalizeRecovery {
        terminal: S5RecoveryTerminal,
        notice: Option<RecoveryNotice>,
        evidence: Vec<RetainedArtifactEvidence>,
    },
    PrepareFailedPartial(RecoveryNotice),
    PrepareOutcomeUnknown(RecoveryNotice),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum S5RecoveryTerminal {
    CompletedApply,
    CompletedUndo,
    CompletedRestore,
    FailedPartial,
    OutcomeUnknown,
}

mod fs_io;
mod lock;
mod recovery;
mod s5;
mod s5_session;
mod store;
use fs_io::*;
use lock::*;
use recovery::*;
pub use s5::*;
use s5_session::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod s5_tests;
