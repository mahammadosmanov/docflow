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
pub(crate) const MAX_S5_RETAINED_FILES: usize = 64;
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
    FailedPartial,
    OutcomeUnknown,
}

impl ValidatedS5Record {
    #[cfg(test)]
    fn for_test(record: JournalRecord, permits: S5PermitBundle) -> Result<Self, BackendError> {
        validate_record(&record)?;
        Ok(Self {
            record,
            lease: S5MutationLease::Test { _permits: permits },
        })
    }

    pub(crate) fn apply_transition(
        &mut self,
        transition: S5Transition,
    ) -> Result<(), BackendError> {
        let apply_lease = matches!(self.lease, S5MutationLease::Apply(_));
        let undo_lease = matches!(self.lease, S5MutationLease::Undo { .. });
        let recovery_lease = matches!(self.lease, S5MutationLease::Recovery { .. });
        let mut candidate = self.record.clone();
        match transition {
            S5Transition::SetPendingArtifactParent(pending) => {
                if candidate.pending_artifact_parent.is_some()
                    || candidate.created_artifact_parent.is_some()
                {
                    return Err(BackendError::journal_invalid(
                        "S5 artifact parent transition is invalid",
                    ));
                }
                candidate.pending_artifact_parent = Some(pending);
            }
            S5Transition::ResolveArtifactParent(outcome) => {
                let pending = candidate.pending_artifact_parent.take().ok_or_else(|| {
                    BackendError::journal_invalid("S5 artifact parent intent is missing")
                })?;
                match outcome {
                    ArtifactParentCreateOutcome::Created { identity } => {
                        candidate.created_artifact_parent = Some(CreatedArtifactParent {
                            path: pending.path,
                            locator: pending.locator,
                            identity,
                        });
                    }
                    ArtifactParentCreateOutcome::AlreadyExists => {}
                    ArtifactParentCreateOutcome::CreatedIdentityUnknown => {
                        candidate.pending_artifact_parent = Some(pending);
                        candidate.recovery_notice = Some(RecoveryNotice {
                            warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
                            locator: candidate
                                .pending_artifact_parent
                                .as_ref()
                                .expect("pending intent was restored")
                                .locator
                                .clone(),
                        });
                        candidate.state = Some(JournalState::RecoveryRequired);
                    }
                }
                candidate.follow_up_phase = Some(FollowUpJournalPhase::ArtifactParentReady);
            }
            S5Transition::SetPendingRetainedArtifact(pending) => {
                if candidate.pending_retained_artifact.is_some() {
                    return Err(BackendError::journal_invalid(
                        "S5 retained artifact transition is invalid",
                    ));
                }
                candidate.pending_retained_artifact = Some(pending);
            }
            S5Transition::MarkPendingRetainedCreated(identity) => {
                let pending = candidate
                    .pending_retained_artifact
                    .as_mut()
                    .ok_or_else(|| {
                        BackendError::journal_invalid("S5 retained artifact intent is missing")
                    })?;
                if pending.creation_state != PendingArtifactCreationState::Intent {
                    return Err(BackendError::journal_invalid(
                        "S5 retained artifact creation state is invalid",
                    ));
                }
                pending.creation_state = PendingArtifactCreationState::Created(identity);
            }
            S5Transition::MarkPendingRetainedIdentityUnknown => {
                let pending = candidate
                    .pending_retained_artifact
                    .as_mut()
                    .ok_or_else(|| {
                        BackendError::journal_invalid("S5 retained artifact intent is missing")
                    })?;
                pending.creation_state = PendingArtifactCreationState::CreatedIdentityUnknown;
            }
            S5Transition::FinalizeRetainedArtifact { evidence, phase } => {
                let pending = candidate.pending_retained_artifact.take().ok_or_else(|| {
                    BackendError::journal_invalid("S5 retained artifact state is missing")
                })?;
                if pending.role != evidence.role
                    || pending.path != evidence.path
                    || pending.locator != evidence.locator
                    || !matches!(
                        pending.creation_state,
                        PendingArtifactCreationState::Created(_)
                    )
                {
                    return Err(BackendError::journal_invalid(
                        "S5 retained artifact evidence does not match its intent",
                    ));
                }
                upsert_evidence(&mut candidate.retained_evidence, evidence)?;
                candidate.follow_up_phase = Some(phase);
            }
            S5Transition::InitializePublication(publication) => {
                if candidate.pending_publication.is_some()
                    || publication.phase != RetainedPublicationPhase::Prepared
                    || (publication.purpose == RetainedPublicationPurpose::Apply && !apply_lease)
                    || (publication.purpose == RetainedPublicationPurpose::Undo && !undo_lease)
                {
                    return Err(BackendError::journal_invalid(
                        "S5 publication initialization is invalid",
                    ));
                }
                if publication.purpose == RetainedPublicationPurpose::Undo {
                    let index = candidate.steps.len().checked_sub(1).ok_or_else(|| {
                        BackendError::journal_invalid("S5 undo step authority is missing")
                    })?;
                    if candidate.undone_steps.contains(&index) || candidate.pending_undo.is_some() {
                        return Err(BackendError::journal_invalid(
                            "S5 undo publication state is invalid",
                        ));
                    }
                    candidate.pending_undo = Some(index);
                }
                candidate.pending_publication = Some(*publication);
                candidate.follow_up_phase = Some(FollowUpJournalPhase::PublicationActive);
            }
            S5Transition::RecordRetainedPublicationEvidence(evidence) => {
                let publication = candidate
                    .pending_publication
                    .as_mut()
                    .ok_or_else(|| BackendError::journal_invalid("S5 publication is not active"))?;
                if evidence.path != publication.retained {
                    return Err(BackendError::journal_invalid(
                        "S5 retained publication evidence path is invalid",
                    ));
                }
                publication.retained_identity = Some(evidence.identity.clone());
                publication.retained_sha256 = Some(evidence.sha256.clone());
                publication.retained_security_sha256 = Some(evidence.security_sha256.clone());
                publication.phase = RetainedPublicationPhase::Retained;
                upsert_evidence(&mut candidate.retained_evidence, evidence)?;
            }
            S5Transition::RecordCanonicalSecured {
                identity,
                descriptor_sha256,
            } => {
                if !valid_sha256(&descriptor_sha256) {
                    return Err(BackendError::journal_invalid(
                        "S5 descriptor predicate is invalid",
                    ));
                }
                let publication = candidate
                    .pending_publication
                    .as_mut()
                    .ok_or_else(|| BackendError::journal_invalid("S5 publication is not active"))?;
                publication.created_canonical_identity = Some(identity);
                publication.phase = RetainedPublicationPhase::CanonicalSecured;
            }
            S5Transition::RecordCanonicalAuxiliaryReady {
                auxiliary_sha256,
                auxiliary_charge_bytes,
            } => {
                let publication = candidate
                    .pending_publication
                    .as_mut()
                    .ok_or_else(|| BackendError::journal_invalid("S5 publication is not active"))?;
                if publication.phase != RetainedPublicationPhase::CanonicalSecured
                    || publication.destination_auxiliary_sha256 != auxiliary_sha256
                    || publication.destination_auxiliary_charge_bytes != auxiliary_charge_bytes
                {
                    return Err(BackendError::journal_invalid(
                        "S5 canonical auxiliary predicate is invalid",
                    ));
                }
                publication.phase = RetainedPublicationPhase::CanonicalAuxiliaryReady;
            }
            S5Transition::MarkCanonicalBytesStarted => {
                candidate
                    .pending_publication
                    .as_mut()
                    .ok_or_else(|| BackendError::journal_invalid("S5 publication is not active"))?
                    .phase = RetainedPublicationPhase::WritingCanonical;
            }
            S5Transition::MarkCanonicalVerifying => {
                candidate
                    .pending_publication
                    .as_mut()
                    .ok_or_else(|| BackendError::journal_invalid("S5 publication is not active"))?
                    .phase = RetainedPublicationPhase::VerifyingCanonical;
            }
            S5Transition::FinalizeApplySuccess {
                completed_step,
                evidence,
            } => {
                if !apply_lease {
                    return Err(BackendError::journal_invalid(
                        "S5 apply completion requires apply authority",
                    ));
                }
                for item in evidence {
                    upsert_evidence(&mut candidate.retained_evidence, item)?;
                }
                candidate.steps.push(completed_step);
                candidate.pending_publication = None;
                candidate.pending_artifact_parent = None;
                candidate.pending_retained_artifact = None;
                candidate.follow_up_phase = None;
                candidate.recovery_notice = None;
                candidate.pending_undo = None;
                candidate.state = Some(JournalState::Committed);
            }
            S5Transition::FinalizeUndoSuccess {
                completed_step,
                evidence,
            } => {
                if !undo_lease {
                    return Err(BackendError::journal_invalid(
                        "S5 undo completion requires undo authority",
                    ));
                }
                for item in evidence {
                    upsert_evidence(&mut candidate.retained_evidence, item)?;
                }
                let index = candidate.pending_undo.ok_or_else(|| {
                    BackendError::journal_invalid("S5 pending undo authority is missing")
                })?;
                if candidate.steps.get(index) != Some(&completed_step)
                    || candidate.undone_steps.contains(&index)
                {
                    return Err(BackendError::journal_invalid(
                        "S5 completed undo step is invalid",
                    ));
                }
                candidate.undone_steps.push(index);
                candidate.pending_publication = None;
                candidate.pending_artifact_parent = None;
                candidate.pending_retained_artifact = None;
                candidate.follow_up_phase = None;
                candidate.recovery_notice = None;
                candidate.pending_undo = None;
                candidate.state = Some(JournalState::Undone);
            }
            S5Transition::FinalizeRecovery {
                terminal,
                notice,
                evidence,
            } => {
                if !recovery_lease {
                    return Err(BackendError::journal_invalid(
                        "S5 recovery completion requires recovery authority",
                    ));
                }
                for item in evidence {
                    upsert_evidence(&mut candidate.retained_evidence, item)?;
                }
                let publication = candidate.pending_publication.as_ref();
                match terminal {
                    S5RecoveryTerminal::CompletedApply => {
                        let publication = publication
                            .filter(|item| item.purpose == RetainedPublicationPurpose::Apply)
                            .ok_or_else(|| {
                                BackendError::journal_invalid(
                                    "S5 recovered apply publication is missing",
                                )
                            })?;
                        if candidate.steps.is_empty() {
                            candidate.steps.push(recovered_restore_step(publication)?);
                        }
                        candidate.state = Some(JournalState::Committed);
                        candidate.recovery_notice = None;
                    }
                    S5RecoveryTerminal::CompletedUndo => {
                        let _publication = publication
                            .filter(|item| item.purpose == RetainedPublicationPurpose::Undo)
                            .ok_or_else(|| {
                                BackendError::journal_invalid(
                                    "S5 recovered undo publication is missing",
                                )
                            })?;
                        let index = candidate.pending_undo.ok_or_else(|| {
                            BackendError::journal_invalid("S5 recovered undo index is missing")
                        })?;
                        if !candidate.undone_steps.contains(&index) {
                            candidate.undone_steps.push(index);
                        }
                        candidate.state = Some(JournalState::Undone);
                        candidate.recovery_notice = None;
                    }
                    S5RecoveryTerminal::FailedPartial => {
                        candidate.state = Some(JournalState::PartialCommitted);
                        candidate.recovery_notice = Some(notice.ok_or_else(|| {
                            BackendError::journal_invalid("S5 partial recovery notice is missing")
                        })?);
                    }
                    S5RecoveryTerminal::OutcomeUnknown => {
                        candidate.state = Some(JournalState::RecoveryRequired);
                        candidate.recovery_notice = Some(notice.ok_or_else(|| {
                            BackendError::journal_invalid("S5 unknown recovery notice is missing")
                        })?);
                    }
                }
                candidate.pending_publication = None;
                candidate.pending_artifact_parent = None;
                candidate.pending_retained_artifact = None;
                candidate.follow_up_phase = None;
                candidate.pending_undo = None;
            }
            S5Transition::PrepareFailedPartial(notice) => {
                candidate.recovery_notice = Some(notice);
                candidate.pending_publication = None;
                candidate.pending_artifact_parent = None;
                candidate.pending_retained_artifact = None;
                candidate.follow_up_phase = None;
                candidate.state = Some(JournalState::PartialCommitted);
            }
            S5Transition::PrepareOutcomeUnknown(notice) => {
                if let Some(publication) = candidate.pending_publication.as_mut() {
                    publication.phase = RetainedPublicationPhase::RecoveryRequired;
                }
                candidate.recovery_notice = Some(notice);
                candidate.state = Some(JournalState::RecoveryRequired);
            }
        }
        validate_record(&candidate)?;
        self.record = candidate;
        Ok(())
    }

    pub(crate) fn journal_id(&self) -> &str {
        &self.record.journal_id
    }

    pub(crate) fn approved_step(&self) -> &Step {
        &self
            .record
            .s5_authority
            .as_ref()
            .expect("validated S5 records contain authority")
            .initial_step
    }

    pub(crate) fn completed_steps(&self) -> &[Step] {
        &self.record.steps
    }

    pub(crate) fn plan(&self) -> Option<&Plan> {
        match &self.lease {
            S5MutationLease::Apply(claim) => Some(claim.plan()),
            _ => None,
        }
    }

    pub(crate) fn binding(&self) -> Option<&PlanBinding> {
        match &self.lease {
            S5MutationLease::Apply(claim) => Some(claim.binding()),
            _ => None,
        }
    }

    pub(crate) fn roots(&self) -> Option<&ValidatedGrantRootRoster> {
        match &self.lease {
            S5MutationLease::Apply(claim) => Some(claim.roots()),
            S5MutationLease::Undo { roots, .. } => Some(roots),
            S5MutationLease::Recovery { roots } => Some(roots),
            #[cfg(test)]
            S5MutationLease::Test { .. } => None,
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.record.revision
    }

    pub(crate) fn phase(&self) -> Option<FollowUpJournalPhase> {
        self.record.follow_up_phase
    }

    pub(crate) fn pending_publication(&self) -> Option<&RetainedPublication> {
        self.record.pending_publication.as_ref()
    }

    pub(crate) fn pending_artifact_parent(&self) -> Option<&PendingArtifactParent> {
        self.record.pending_artifact_parent.as_ref()
    }

    pub(crate) fn pending_retained_artifact(&self) -> Option<&PendingRetainedArtifact> {
        self.record.pending_retained_artifact.as_ref()
    }

    pub(crate) fn retained_evidence(&self) -> &[RetainedArtifactEvidence] {
        &self.record.retained_evidence
    }

    pub(crate) fn retained_evidence_for(
        &self,
        role: RetainedArtifactRole,
    ) -> Option<&RetainedArtifactEvidence> {
        self.record
            .retained_evidence
            .iter()
            .find(|evidence| evidence.role == role)
    }

    pub(crate) fn retained_locators(&self) -> Result<RetainedLocators, BackendError> {
        let mut locators = self
            .record
            .retained_evidence
            .iter()
            .map(|evidence| evidence.locator.clone())
            .collect::<Vec<_>>();
        if let Some(pending) = self.record.pending_artifact_parent.as_ref() {
            locators.push(pending.locator.clone());
        }
        if let Some(created) = self.record.created_artifact_parent.as_ref() {
            locators.push(created.locator.clone());
        }
        if let Some(pending) = self.record.pending_retained_artifact.as_ref() {
            locators.push(pending.locator.clone());
        }
        locators.sort();
        locators.dedup();
        if locators.is_empty()
            || locators.len() > MAX_RETAINED_ARTIFACTS_PER_JOURNAL + 2
            || locators
                .iter()
                .any(|locator| !valid_recovery_locator(&self.record.journal_id, locator))
        {
            return Err(BackendError::journal_invalid(
                "S5 retained locator projection is invalid",
            ));
        }
        Ok(RetainedLocators(locators))
    }

    pub(crate) fn mark_publication_phase(
        &mut self,
        expected: RetainedPublicationPhase,
        next: RetainedPublicationPhase,
    ) -> Result<(), BackendError> {
        let mut candidate = self.record.clone();
        let publication = candidate
            .pending_publication
            .as_mut()
            .ok_or_else(|| BackendError::journal_invalid("S5 publication is not active"))?;
        if publication.phase != expected {
            return Err(BackendError::journal_invalid(
                "S5 publication phase transition is invalid",
            ));
        }
        publication.phase = next;
        validate_record(&candidate)?;
        self.record = candidate;
        Ok(())
    }

    pub(crate) fn prepare_failed_partial(
        &mut self,
        notice: RecoveryNotice,
    ) -> Result<(), BackendError> {
        self.apply_transition(S5Transition::PrepareFailedPartial(notice))
    }

    pub(crate) fn prepare_outcome_unknown(
        &mut self,
        notice: RecoveryNotice,
    ) -> Result<(), BackendError> {
        self.apply_transition(S5Transition::PrepareOutcomeUnknown(notice))
    }

    pub(crate) fn prepare_no_effect(&mut self) -> Result<(), BackendError> {
        let mut candidate = self.record.clone();
        if !candidate.retained_evidence.is_empty()
            || candidate.created_artifact_parent.is_some()
            || candidate.pending_publication.is_some()
        {
            return Err(BackendError::journal_invalid(
                "S5 no-effect classification conflicts with durable effects",
            ));
        }
        candidate.pending_artifact_parent = None;
        candidate.pending_retained_artifact = None;
        candidate.follow_up_phase = None;
        candidate.recovery_notice = None;
        candidate.state = Some(JournalState::NoEffectRetryable);
        validate_record(&candidate)?;
        self.record = candidate;
        Ok(())
    }

    pub(crate) fn prepare_cancelled(&mut self) -> Result<(), BackendError> {
        let mut candidate = self.record.clone();
        let partial =
            !candidate.retained_evidence.is_empty() || candidate.created_artifact_parent.is_some();
        candidate.pending_artifact_parent = None;
        candidate.pending_retained_artifact = None;
        candidate.follow_up_phase = None;
        candidate.recovery_notice = None;
        candidate.state = Some(if partial {
            JournalState::CancelledPartial
        } else {
            JournalState::CancelledClean
        });
        validate_record(&candidate)?;
        self.record = candidate;
        Ok(())
    }
}

fn upsert_evidence(
    evidence: &mut Vec<RetainedArtifactEvidence>,
    item: RetainedArtifactEvidence,
) -> Result<(), BackendError> {
    if let Some(existing) = evidence.iter().find(|existing| existing.role == item.role) {
        if existing == &item {
            return Ok(());
        }
        return Err(BackendError::journal_invalid(
            "S5 retained evidence role conflicts with existing evidence",
        ));
    }
    if evidence.len() >= MAX_RETAINED_ARTIFACTS_PER_JOURNAL {
        return Err(BackendError::journal_invalid(
            "S5 retained evidence limit was exceeded",
        ));
    }
    evidence.push(item);
    Ok(())
}

fn recovered_restore_step(publication: &RetainedPublication) -> Result<Step, BackendError> {
    let created_identity = publication
        .created_canonical_identity
        .as_ref()
        .ok_or_else(|| BackendError::journal_invalid("S5 created identity is missing"))?;
    Ok(Step {
        kind: crate::contracts::StepKind::RestoreBackup,
        src: Some(publication.backup.clone()),
        dst: Some(publication.canonical.clone()),
        note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
        meta: serde_json::Map::from_iter([
            (
                "originalSha256".to_string(),
                serde_json::Value::String(publication.expected_canonical_sha256.clone()),
            ),
            (
                "appliedSha256".to_string(),
                serde_json::Value::String(publication.publication_sha256.clone()),
            ),
            (
                "createdCanonicalIdentity".to_string(),
                serde_json::to_value(created_identity).map_err(|_| {
                    BackendError::journal_invalid("S5 created identity could not be encoded")
                })?,
            ),
        ]),
    })
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

    pub(crate) fn try_acquire_mutation_lock(
        &self,
    ) -> Result<InterprocessMutationPermit, BackendError> {
        fs::create_dir_all(&self.root)
            .map_err(|error| journal_io("journal directory could not be created", error))?;
        let lock_path = self.root.join(".docflow-s5-mutation.lock");
        let file = open_mutation_lock_file(&lock_path)
            .map_err(|error| journal_io("S5 mutation lock could not be opened", error))?;
        match try_lock_mutation_file(&file) {
            Ok(()) => Ok(InterprocessMutationPermit { _file: file }),
            Err(error) if mutation_lock_contended(&error) => {
                Err(BackendError::operation_in_progress())
            }
            Err(error) => Err(journal_io("S5 mutation lock could not be acquired", error)),
        }
    }

    pub(crate) fn bind_s5_permits(
        &self,
        process: GlobalMutationPermit,
        interprocess: InterprocessMutationPermit,
    ) -> S5PermitBundle {
        S5PermitBundle {
            _process: process,
            _interprocess: interprocess,
        }
    }

    pub(crate) fn begin_s5(
        &self,
        claim: ClaimedS5Apply,
        caller: &crate::paths::CallerIdentity,
    ) -> Result<ValidatedS5Record, BackendError> {
        if claim.caller() != caller
            || claim.plan().steps.len() != 1
            || claim.created_at() > std::time::SystemTime::now()
        {
            return Err(BackendError::plan_unavailable());
        }
        let step = claim.plan().steps[0].clone();
        let source_path = step
            .meta
            .get("rfiLogWorkbook")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| BackendError::journal_invalid("S5 source authority is missing"))?
            .to_string();
        let source_identity: FilesystemObjectIdentity = serde_json::from_value(
            step.meta
                .get("rfiLogFingerprint")
                .and_then(|fingerprint| fingerprint.get("identity"))
                .cloned()
                .ok_or_else(|| BackendError::journal_invalid("S5 source identity is missing"))?,
        )
        .map_err(|_| BackendError::journal_invalid("S5 source identity is invalid"))?;
        let target_path = step
            .src
            .clone()
            .ok_or_else(|| BackendError::journal_invalid("S5 target authority is missing"))?;
        let target_identity: FilesystemObjectIdentity = serde_json::from_value(
            step.meta
                .get("followUpFingerprint")
                .and_then(|fingerprint| fingerprint.get("identity"))
                .cloned()
                .ok_or_else(|| BackendError::journal_invalid("S5 target identity is missing"))?,
        )
        .map_err(|_| BackendError::journal_invalid("S5 target identity is invalid"))?;
        let updates = step
            .meta
            .get("updates")
            .ok_or_else(|| BackendError::journal_invalid("S5 approved updates are missing"))?;
        let approved_updates_sha256 = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(updates).map_err(|_| {
                BackendError::journal_invalid("S5 approved updates could not be encoded")
            })?)
        );
        let roots = claim.roots().records().to_vec();
        let mut record = JournalRecord::fresh(
            &claim.plan().op_id,
            &claim.binding().owner_id,
            &claim.binding().session_id,
            claim.digest(),
        );
        record.state = Some(JournalState::Applying);
        record.grant_roots = roots.iter().map(|root| root.root.clone()).collect();
        record.grant_root_identities = roots
            .iter()
            .map(|root| GrantRootIdentity {
                root: root.root.clone(),
                identity: root.identity.clone(),
            })
            .collect();
        record.grant_root_accesses = roots;
        record.s5_authority = Some(S5AuthorityRecord {
            initial_step: step,
            source_path,
            source_identity,
            target_path,
            target_identity,
            approved_updates_sha256,
        });
        let mut validated = ValidatedS5Record {
            record,
            lease: S5MutationLease::Apply(Box::new(claim)),
        };
        self.write_s5_cas(&mut validated, 0)?;
        Ok(validated)
    }

    pub fn write(&self, record: &JournalRecord) -> Result<(), BackendError> {
        validate_record(record)?;
        if has_s5_discriminator(record) {
            return Err(BackendError::journal_invalid(
                "S5 journals require revision-CAS authority",
            ));
        }
        let _permit = self.try_acquire_mutation_lock()?;
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
            if has_s5_discriminator(&current) {
                return Err(BackendError::journal_invalid(
                    "S5 journal authority cannot be downgraded",
                ));
            }
        }
        self.write_record_durable(record)
    }

    fn write_record_durable(&self, record: &JournalRecord) -> Result<(), BackendError> {
        fs::create_dir_all(&self.root)
            .map_err(|error| journal_io("journal directory could not be created", error))?;
        let target = self.path_for(record);
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
        let record = self.read_raw(journal_id)?;
        if has_s5_discriminator(&record) {
            return Err(BackendError::journal_invalid(
                "S5 journal requires targeted validated admission",
            ));
        }
        Ok(record)
    }

    pub(crate) fn inspect_operation(&self, journal_id: &str) -> Result<String, BackendError> {
        Ok(self.read_raw(journal_id)?.op_id)
    }

    pub(crate) fn inspect_s5_for_undo(
        &self,
        journal_id: &str,
        caller: &crate::paths::CallerIdentity,
        permits: S5PermitBundle,
    ) -> Result<PendingS5Undo, BackendError> {
        let record = self.read_raw(journal_id)?;
        if record.op_id != "rfi.follow_up_sync"
            || record.owner_id.as_deref() != Some(caller.owner_id.as_str())
            || record.state != Some(JournalState::Committed)
            || record.steps.len() != 1
            || record.steps[0].kind != crate::contracts::StepKind::RestoreBackup
            || !record.undone_steps.is_empty()
            || record.pending_step.is_some()
            || record.pending_undo.is_some()
            || record.pending_publication.is_some()
            || record.recovery_notice.is_some()
        {
            return Err(BackendError::journal_invalid(
                "S5 journal is not eligible for exact undo",
            ));
        }
        validate_s5_fields(&record)?;
        Ok(PendingS5Undo { record, permits })
    }

    pub(crate) fn admit_s5_undo(
        &self,
        pending: PendingS5Undo,
        roots: ValidatedGrantRootRoster,
    ) -> Result<ValidatedS5Record, BackendError> {
        if roots.records() != pending.record.grant_root_accesses {
            return Err(BackendError::journal_invalid(
                "S5 replayed root authority does not match the journal",
            ));
        }
        validate_record(&pending.record)?;
        Ok(ValidatedS5Record {
            record: pending.record,
            lease: S5MutationLease::Undo {
                _permits: pending.permits,
                roots,
            },
        })
    }

    pub(crate) fn begin_s5_recovery(
        &self,
        permits: S5PermitBundle,
    ) -> Result<S5RecoverySession, BackendError> {
        let entries = self.scan_pending_recovery_entries()?;
        Ok(S5RecoverySession {
            store: self.clone(),
            _permits: permits,
            entries,
        })
    }

    fn scan_pending_recovery_entries(&self) -> Result<VecDeque<PendingS5Entry>, BackendError> {
        let Ok(directory) = fs::read_dir(&self.root) else {
            return Ok(VecDeque::new());
        };
        let mut paths = directory
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| journal_io("journal directory entry could not be read", error))?;
        if paths.len() > MAX_JOURNAL_ROOT_ENTRIES_SCANNED {
            return Err(BackendError::journal_invalid(
                "journal recovery root entry limit was exceeded",
            ));
        }
        paths.sort_by_key(|entry| entry.file_name());
        let mut entries = VecDeque::new();
        let mut pending_s5 = 0_usize;
        let mut invalid_notices = 0_usize;
        for entry in paths {
            let path = entry.path();
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if invalid_s5_notice_name(filename) {
                invalid_notices = invalid_notices.checked_add(1).ok_or_else(|| {
                    BackendError::journal_invalid("invalid S5 notice count overflowed")
                })?;
                let metadata = fs::symlink_metadata(&path).map_err(|error| {
                    journal_io("invalid S5 notice could not be inspected", error)
                })?;
                if invalid_notices > MAX_INVALID_S5_NOTICES
                    || !metadata.file_type().is_file()
                    || metadata.len() > MAX_INVALID_S5_NOTICE_BYTES
                {
                    return Err(BackendError::journal_invalid(
                        "invalid S5 notice bound was exceeded",
                    ));
                }
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let record = read_file(&path)?;
            if has_s5_discriminator(&record) {
                if !s5_record_needs_recovery(&record) {
                    continue;
                }
                pending_s5 = pending_s5.checked_add(1).ok_or_else(|| {
                    BackendError::journal_invalid("pending S5 journal count overflowed")
                })?;
                if pending_s5 > MAX_PENDING_S5_RECORDS {
                    return Err(BackendError::journal_invalid(
                        "pending S5 journal limit was exceeded",
                    ));
                }
                entries.push_back(PendingS5Entry::Valid(Box::new(record)));
            } else if let Some(entry) = legacy_recovery_entry(&record) {
                entries.push_back(PendingS5Entry::Legacy(entry));
            }
            if entries.len() > MAX_RECOVERY_ENTRIES {
                return Err(BackendError::journal_invalid(
                    "recovery response limit was exceeded",
                ));
            }
        }
        Ok(entries)
    }

    #[cfg(test)]
    fn read_s5_for_test(&self, journal_id: &str) -> Result<JournalRecord, BackendError> {
        self.read_raw(journal_id)
    }

    fn read_raw(&self, journal_id: &str) -> Result<JournalRecord, BackendError> {
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

    pub(crate) fn write_s5_cas(
        &self,
        record: &mut ValidatedS5Record,
        expected_revision: u64,
    ) -> Result<(), BackendError> {
        if record.record.revision != expected_revision {
            return Err(BackendError::journal_invalid(
                "S5 journal revision token is stale",
            ));
        }
        let next_revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| BackendError::journal_invalid("S5 journal revision overflowed"))?;
        let mut candidate = record.record.clone();
        candidate.revision = next_revision;
        validate_record(&candidate)?;
        let target = self.path_for(&candidate);
        if expected_revision == 0 {
            if target.exists() {
                return Err(BackendError::journal_invalid(
                    "S5 journal initial revision already exists",
                ));
            }
        } else {
            let current = read_file(&target)?;
            if !has_s5_discriminator(&current)
                || current.revision != expected_revision
                || !same_s5_provenance(&current, &candidate)
            {
                return Err(BackendError::journal_invalid(
                    "S5 journal revision or provenance changed",
                ));
            }
        }
        self.write_record_durable(&candidate)?;
        record.record = candidate;
        Ok(())
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
            if let Some(entry) = legacy_recovery_entry(&record) {
                recovery.push(entry);
            }
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

impl S5RecoverySession {
    pub(crate) fn has_next(&self) -> bool {
        !self.entries.is_empty()
    }

    pub(crate) fn next_root_accesses(&self) -> Option<&[GrantRootAccessRecord]> {
        match self.entries.front() {
            Some(PendingS5Entry::Valid(record)) if record.recovery_notice.is_none() => {
                Some(&record.grant_root_accesses)
            }
            Some(PendingS5Entry::Legacy(_)) | None => None,
            Some(PendingS5Entry::Valid(_)) => None,
        }
    }

    pub(crate) fn recover_next(
        &mut self,
        roots: Option<ValidatedGrantRootRoster>,
        caller: &crate::paths::CallerIdentity,
    ) -> Result<Option<RecoveryEntry>, BackendError> {
        let Some(entry) = self.entries.pop_front() else {
            return Ok(None);
        };
        let record = match entry {
            PendingS5Entry::Legacy(entry) => {
                if roots.is_some() {
                    return Err(BackendError::journal_invalid(
                        "legacy recovery cannot consume S5 root authority",
                    ));
                }
                return Ok(Some(entry));
            }
            PendingS5Entry::Valid(record) => *record,
        };
        if record.owner_id.as_deref() != Some(caller.owner_id.as_str()) {
            return Err(BackendError::journal_owner_mismatch());
        }
        if let Some(notice) = record.recovery_notice.clone() {
            return Ok(Some(recovery_entry(&record, notice)));
        }
        let roots = roots.ok_or_else(|| {
            BackendError::journal_invalid("S5 recovery root authority is missing")
        })?;
        if roots.records() != record.grant_root_accesses {
            return Err(BackendError::journal_invalid(
                "S5 recovery root authority changed",
            ));
        }
        let mut token = ValidatedS5Record {
            record,
            lease: S5MutationLease::Recovery { roots },
        };
        if token.pending_publication().is_some() {
            return crate::leaf_transaction::reconcile_retained_publication(
                &self.store,
                &mut token,
            );
        }
        let known_effect = !token.record.retained_evidence.is_empty()
            || token.record.created_artifact_parent.is_some()
            || token
                .record
                .pending_retained_artifact
                .as_ref()
                .is_some_and(|pending| {
                    matches!(
                        pending.creation_state,
                        PendingArtifactCreationState::Created(_)
                    )
                });
        let locator = token
            .retained_locators()
            .map(|locators| locators.first().to_string())
            .unwrap_or_else(|_| {
                format!("journal:{};root:0;leaf:recovery", token.record.journal_id)
            });
        let notice = RecoveryNotice {
            warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
            locator,
        };
        let terminal = if known_effect {
            S5RecoveryTerminal::FailedPartial
        } else {
            S5RecoveryTerminal::OutcomeUnknown
        };
        let expected = token.revision();
        token.apply_transition(S5Transition::FinalizeRecovery {
            terminal,
            notice: Some(notice.clone()),
            evidence: Vec::new(),
        })?;
        self.store.write_s5_cas(&mut token, expected)?;
        Ok(Some(recovery_entry(&token.record, notice)))
    }
}

fn same_s5_provenance(left: &JournalRecord, right: &JournalRecord) -> bool {
    left.journal_id == right.journal_id
        && left.op_id == right.op_id
        && left.ts == right.ts
        && left.owner_id == right.owner_id
        && left.session_id == right.session_id
        && left.plan_digest == right.plan_digest
        && left.grant_roots == right.grant_roots
        && left.grant_root_identities == right.grant_root_identities
        && left.grant_root_accesses == right.grant_root_accesses
        && left.s5_authority == right.s5_authority
}

#[cfg(unix)]
fn open_mutation_lock_file(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
fn open_mutation_lock_file(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .attributes(FILE_ATTRIBUTE_NORMAL)
        .open(path)
}

#[cfg(unix)]
fn try_lock_mutation_file(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn try_lock_mutation_file(file: &File) -> io::Result<()> {
    use std::{mem::zeroed, os::windows::io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx},
        System::IO::OVERLAPPED,
    };

    let mut overlapped: OVERLAPPED = unsafe { zeroed() };
    if unsafe {
        LockFileEx(
            file.as_raw_handle() as HANDLE,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn mutation_lock_contended(error: &io::Error) -> bool {
    error
        .raw_os_error()
        .is_some_and(|code| code == libc::EWOULDBLOCK || code == libc::EAGAIN)
}

#[cfg(windows)]
fn mutation_lock_contended(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32 | 33 | 158))
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

fn legacy_recovery_entry(record: &JournalRecord) -> Option<RecoveryEntry> {
    let disposition = if let Some(step) = record.pending_step.as_ref() {
        pending_disposition(step)
    } else if record.pending_undo.is_some() || record.undo_recovery.is_some() {
        RecoveryDisposition::OutcomeUnknown
    } else {
        return None;
    };
    Some(RecoveryEntry {
        journal_id: record.journal_id.clone(),
        disposition,
        warning_key: None,
        locator: None,
    })
}

fn s5_record_needs_recovery(record: &JournalRecord) -> bool {
    record.pending_publication.is_some()
        || record.pending_artifact_parent.is_some()
        || record.pending_retained_artifact.is_some()
        || record.follow_up_phase.is_some()
        || record.recovery_notice.is_some()
        || matches!(
            record.state,
            Some(JournalState::Applying | JournalState::RecoveryRequired)
        )
}

fn invalid_s5_notice_name(filename: &str) -> bool {
    let Some(hash) = filename
        .strip_prefix(".invalid-s5-")
        .and_then(|value| value.strip_suffix(".json"))
    else {
        return false;
    };
    valid_sha256(hash)
}

fn recovery_entry(record: &JournalRecord, notice: RecoveryNotice) -> RecoveryEntry {
    RecoveryEntry {
        journal_id: record.journal_id.clone(),
        disposition: RecoveryDisposition::OutcomeUnknown,
        warning_key: Some(notice.warning_key),
        locator: Some(notice.locator),
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
    if has_s5_discriminator(record) {
        validate_s5_fields(record)?;
    }
    Ok(())
}

fn has_s5_discriminator(record: &JournalRecord) -> bool {
    record.op_id == "rfi.follow_up_sync"
        || record.s5_authority.is_some()
        || record.pending_publication.is_some()
        || record.pending_artifact_parent.is_some()
        || record.created_artifact_parent.is_some()
        || record.pending_retained_artifact.is_some()
        || !record.retained_evidence.is_empty()
        || record.recovery_notice.is_some()
        || record.follow_up_phase.is_some()
        || !record.grant_root_accesses.is_empty()
        || record.revision != 0
}

fn validate_s5_fields(record: &JournalRecord) -> Result<(), BackendError> {
    let invalid = || BackendError::journal_invalid("S5 journal transaction metadata is invalid");
    if record.op_id != "rfi.follow_up_sync" {
        return Err(BackendError::journal_invalid(
            "S5 operation identity is invalid",
        ));
    }
    if record.owner_id.as_deref().is_none_or(str::is_empty)
        || record.session_id.as_deref().is_none_or(str::is_empty)
        || record
            .plan_digest
            .as_deref()
            .is_none_or(|value| !valid_sha256(value))
    {
        return Err(BackendError::journal_invalid(
            "S5 immutable provenance is invalid",
        ));
    }
    if record.grant_roots.is_empty()
        || record.grant_roots.len() != record.grant_root_identities.len()
        || record.grant_roots.len() != record.grant_root_accesses.len()
        || record.grant_roots.len() > 8
    {
        return Err(BackendError::journal_invalid(
            "S5 grant root roster is invalid",
        ));
    }
    if record.s5_authority.is_none()
        || record.pending_step.is_some()
        || record.undo_recovery.is_some()
    {
        return Err(BackendError::journal_invalid(
            "S5 authority state is invalid",
        ));
    }

    let mut seen_roots = std::collections::HashSet::new();
    for access in &record.grant_root_accesses {
        if !seen_roots.insert(access.root.as_str())
            || !Path::new(&access.root).is_absolute()
            || !record.grant_roots.iter().any(|root| root == &access.root)
            || !record.grant_root_identities.iter().any(|identity| {
                identity.root == access.root && identity.identity == access.identity
            })
        {
            return Err(BackendError::journal_invalid(
                "S5 grant root access parity is invalid",
            ));
        }
    }

    let authority = record.s5_authority.as_ref().ok_or_else(invalid)?;
    if authority.initial_step.kind != crate::contracts::StepKind::WorkbookUpdate
        || authority.initial_step.src.as_deref() != Some(authority.target_path.as_str())
        || authority.initial_step.dst.as_deref().is_none()
    {
        return Err(BackendError::journal_invalid(
            "S5 initial step authority is invalid",
        ));
    }
    if !valid_sha256(&authority.approved_updates_sha256) {
        return Err(BackendError::journal_invalid(
            "S5 approved update digest is invalid",
        ));
    }
    if !path_in_recorded_roots(&authority.source_path, &record.grant_roots)
        || !path_in_recorded_roots(&authority.target_path, &record.grant_roots)
    {
        return Err(BackendError::journal_invalid(
            "S5 workbook path authority is invalid",
        ));
    }
    if !path_has_access(
        &authority.target_path,
        &record.grant_root_accesses,
        PersistedRootAccess::ReadWrite,
    ) {
        return Err(BackendError::journal_invalid(
            "S5 target write authority is invalid",
        ));
    }
    let target_parent = Path::new(&authority.target_path)
        .parent()
        .ok_or_else(invalid)?;
    let backup = Path::new(authority.initial_step.dst.as_deref().ok_or_else(invalid)?);
    if backup.parent() != Some(target_parent)
        || !path_in_recorded_roots(backup.to_string_lossy().as_ref(), &record.grant_roots)
    {
        return Err(BackendError::journal_invalid(
            "S5 backup authority is invalid",
        ));
    }

    if record.retained_evidence.len() > MAX_RETAINED_ARTIFACTS_PER_JOURNAL {
        return Err(invalid());
    }
    let mut evidence_roles = std::collections::HashSet::new();
    let mut evidence_paths = std::collections::HashSet::new();
    let mut evidence_locators = std::collections::HashSet::new();
    for evidence in &record.retained_evidence {
        if !evidence_roles.insert(evidence.role)
            || !evidence_paths.insert(evidence.path.as_str())
            || !evidence_locators.insert(evidence.locator.as_str())
            || !valid_sha256(&evidence.sha256)
            || !valid_sha256(&evidence.security_sha256)
            || evidence
                .default_stream_bytes
                .checked_add(evidence.auxiliary_charge_bytes)
                != Some(evidence.logical_charge_bytes)
            || !valid_recovery_locator(&record.journal_id, &evidence.locator)
            || !path_in_recorded_roots(&evidence.path, &record.grant_roots)
        {
            return Err(invalid());
        }
    }

    for (path, locator) in record
        .pending_artifact_parent
        .as_ref()
        .map(|pending| (pending.path.as_str(), pending.locator.as_str()))
        .into_iter()
        .chain(
            record
                .created_artifact_parent
                .as_ref()
                .map(|created| (created.path.as_str(), created.locator.as_str())),
        )
        .chain(
            record
                .pending_retained_artifact
                .as_ref()
                .map(|pending| (pending.path.as_str(), pending.locator.as_str())),
        )
    {
        if !path_in_recorded_roots(path, &record.grant_roots)
            || !valid_recovery_locator(&record.journal_id, locator)
        {
            return Err(invalid());
        }
    }
    if record.pending_artifact_parent.is_some() && record.created_artifact_parent.is_some() {
        return Err(invalid());
    }
    if let Some(notice) = record.recovery_notice.as_ref()
        && (notice.warning_key.is_empty()
            || notice.warning_key.len() > 256
            || !valid_recovery_locator(&record.journal_id, &notice.locator))
    {
        return Err(invalid());
    }
    if let Some(publication) = record.pending_publication.as_ref() {
        let hashes = [
            publication.expected_canonical_sha256.as_str(),
            publication.expected_backup_sha256.as_str(),
            publication.publication_sha256.as_str(),
            publication.expected_canonical_security_sha256.as_str(),
            publication.expected_backup_security_sha256.as_str(),
            publication.publication_security_sha256.as_str(),
            publication.destination_access_sha256.as_str(),
            publication.destination_auxiliary_sha256.as_str(),
        ];
        if hashes.into_iter().any(|hash| !valid_sha256(hash))
            || publication
                .expected_canonical_default_stream_bytes
                .checked_add(publication.expected_canonical_auxiliary_charge_bytes)
                != Some(publication.expected_canonical_logical_charge_bytes)
            || Path::new(&publication.canonical).parent()
                != Path::new(&publication.retained).parent()
            || Path::new(&publication.canonical).parent() != Path::new(&publication.backup).parent()
            || [
                publication.canonical.as_str(),
                publication.retained.as_str(),
                publication.backup.as_str(),
                publication.publication_source.as_str(),
            ]
            .into_iter()
            .any(|path| !path_in_recorded_roots(path, &record.grant_roots))
            || publication.retained_identity.is_some() != publication.retained_sha256.is_some()
            || publication.retained_identity.is_some()
                != publication.retained_security_sha256.is_some()
            || publication
                .retained_sha256
                .as_deref()
                .is_some_and(|hash| !valid_sha256(hash))
            || publication
                .retained_security_sha256
                .as_deref()
                .is_some_and(|hash| !valid_sha256(hash))
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_recovery_locator(journal_id: &str, locator: &str) -> bool {
    locator.chars().count() <= MAX_RECOVERY_LOCATOR_SCALARS
        && !locator.chars().any(char::is_control)
        && locator.starts_with(&format!("journal:{journal_id};root:"))
        && locator.contains(";leaf:")
        && !locator.contains('/')
        && !locator.contains('\\')
}

fn path_in_recorded_roots(path: &str, roots: &[String]) -> bool {
    let path = Path::new(path);
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        && roots.iter().any(|root| path.starts_with(Path::new(root)))
}

fn path_has_access(
    path: &str,
    roots: &[GrantRootAccessRecord],
    required: PersistedRootAccess,
) -> bool {
    roots.iter().any(|root| {
        Path::new(path).starts_with(Path::new(&root.root))
            && (required == PersistedRootAccess::ReadOnly
                || root.access == PersistedRootAccess::ReadWrite)
    })
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
    retry_windows_atomic_replace(
        || {
            let result = unsafe {
                MoveFileExW(
                    temporary.as_ptr(),
                    target.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            };
            if result == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        },
        std::thread::sleep,
    )
    .map_err(|error| journal_io("journal could not be atomically replaced", error))
}

#[cfg(windows)]
fn retry_windows_atomic_replace<R, W>(mut replace: R, mut wait: W) -> io::Result<()>
where
    R: FnMut() -> io::Result<()>,
    W: FnMut(std::time::Duration),
{
    const MAX_ATTEMPTS: usize = 20;
    const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

    for attempt in 0..MAX_ATTEMPTS {
        match replace() {
            Ok(()) => return Ok(()),
            Err(error)
                if matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && attempt + 1 < MAX_ATTEMPTS =>
            {
                wait(RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("journal replacement retry loop always returns")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_identity(value: u64) -> FilesystemObjectIdentity {
        FilesystemObjectIdentity::Unix {
            device: 1,
            inode: value,
        }
    }

    fn retained_publication_record(root: &Path) -> JournalRecord {
        let source = root.join("source.xlsx");
        let target = root.join("target.xlsx");
        let backup = root.join(".docflow-s5-backup-test.xlsx");
        let step = Step {
            kind: crate::contracts::StepKind::WorkbookUpdate,
            src: Some(target.to_string_lossy().into_owned()),
            dst: Some(backup.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
            meta: serde_json::Map::new(),
        };
        let mut record = JournalRecord::new(
            "journal-s5",
            "rfi.follow_up_sync",
            "2026-08-29T00-00-00Z",
            "owner",
            "session",
            "a".repeat(64),
        );
        let root_text = root.to_string_lossy().into_owned();
        record.state = Some(JournalState::Applying);
        record.grant_roots = vec![root_text.clone()];
        record.grant_root_identities = vec![GrantRootIdentity {
            root: root_text.clone(),
            identity: test_identity(1),
        }];
        record.grant_root_accesses = vec![GrantRootAccessRecord {
            root: root_text,
            identity: test_identity(1),
            access: PersistedRootAccess::ReadWrite,
        }];
        record.s5_authority = Some(S5AuthorityRecord {
            initial_step: step,
            source_path: source.to_string_lossy().into_owned(),
            source_identity: test_identity(2),
            target_path: target.to_string_lossy().into_owned(),
            target_identity: test_identity(3),
            approved_updates_sha256: "b".repeat(64),
        });
        record
    }

    #[cfg(windows)]
    #[test]
    fn journal_replace_retries_transient_windows_access_denied() {
        use std::{cell::Cell, time::Duration};

        let attempts = Cell::new(0);
        let mut waits = Vec::new();

        retry_windows_atomic_replace(
            || {
                let attempt = attempts.get() + 1;
                attempts.set(attempt);
                if attempt < 3 {
                    Err(io::Error::from_raw_os_error(5))
                } else {
                    Ok(())
                }
            },
            |duration| waits.push(duration),
        )
        .unwrap();

        assert_eq!(attempts.get(), 3);
        assert_eq!(
            waits,
            [Duration::from_millis(100), Duration::from_millis(100)]
        );
    }

    #[cfg(windows)]
    #[test]
    fn journal_store_survives_a_transient_windows_file_lock() {
        use std::os::windows::fs::OpenOptionsExt;
        use std::time::Duration;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

        let fixture = tempfile::tempdir().unwrap();
        let store = JournalStore::new(fixture.path());
        let mut record = JournalRecord::new(
            "journal-lock-test",
            "rfi.process_scans",
            "2026-08-26T00-00-00Z",
            "owner",
            "session",
            "digest",
        );
        record.state = Some(JournalState::Applying);
        store.write(&record).unwrap();

        let target = store.path_for(&record);
        let blocker = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(target)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            drop(blocker);
        });

        record.state = Some(JournalState::Committed);
        store.write(&record).unwrap();
        release.join().unwrap();

        assert_eq!(
            store.read(&record.journal_id).unwrap().state,
            Some(JournalState::Committed)
        );
    }

    #[test]
    fn retained_publication_legacy_json_defaults_additive_s5_fields_and_omits_them_again() {
        let legacy = serde_json::json!({
            "journalId": "legacy-journal",
            "opId": "rfi.process_scans",
            "ts": "2026-08-29T00-00-00Z",
            "steps": []
        });

        let record: JournalRecord = serde_json::from_value(legacy).unwrap();
        assert_eq!(record.revision, 0);
        assert!(record.s5_authority.is_none());
        assert!(record.pending_publication.is_none());
        assert!(record.pending_artifact_parent.is_none());
        assert!(record.created_artifact_parent.is_none());
        assert!(record.pending_retained_artifact.is_none());
        assert!(record.retained_evidence.is_empty());
        assert!(record.recovery_notice.is_none());
        assert!(record.follow_up_phase.is_none());
        assert!(record.grant_root_accesses.is_empty());

        let encoded = serde_json::to_value(record).unwrap();
        for key in [
            "s5Authority",
            "pendingPublication",
            "pendingArtifactParent",
            "createdArtifactParent",
            "pendingRetainedArtifact",
            "retainedEvidence",
            "recoveryNotice",
            "followUpPhase",
            "grantRootAccesses",
            "revision",
        ] {
            assert!(encoded.get(key).is_none(), "legacy output exposed {key}");
        }
    }

    #[test]
    fn retained_publication_recovery_projection_is_additive_and_wire_exact() {
        let entry = RecoveryEntry {
            journal_id: "journal-s5".to_string(),
            disposition: RecoveryDisposition::OutcomeUnknown,
            warning_key: Some("error.rfi.followUp.recoveryRequired".to_string()),
            locator: Some("journal:journal-s5;root:0;leaf:evidence.xlsx".to_string()),
        };

        assert_eq!(
            serde_json::to_value(entry).unwrap(),
            serde_json::json!({
                "journalId": "journal-s5",
                "disposition": "outcome_unknown",
                "warningKey": "error.rfi.followUp.recoveryRequired",
                "locator": "journal:journal-s5;root:0;leaf:evidence.xlsx"
            })
        );
    }

    #[test]
    fn final_notice_recovery_needs_no_root_reopen() {
        let fixture = tempfile::tempdir().unwrap();
        let store = JournalStore::new(fixture.path().join("journals"));
        let process_lock = crate::progress::GlobalMutationLock::default();
        let permits = store.bind_s5_permits(
            process_lock.try_acquire().unwrap(),
            store.try_acquire_mutation_lock().unwrap(),
        );
        let mut token =
            ValidatedS5Record::for_test(retained_publication_record(fixture.path()), permits)
                .unwrap();
        token
            .prepare_failed_partial(RecoveryNotice {
                warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
                locator: "journal:journal-s5;root:0;leaf:target.xlsx".to_string(),
            })
            .unwrap();

        store.write_s5_cas(&mut token, 0).unwrap();
        let path = store.path_for(&token.record);
        drop(token);
        let before = fs::read(&path).unwrap();

        assert_eq!(fs::read(&path).unwrap(), before);

        let process_lock = crate::progress::GlobalMutationLock::default();
        let permits = store.bind_s5_permits(
            process_lock.try_acquire().unwrap(),
            store.try_acquire_mutation_lock().unwrap(),
        );
        let mut session = store.begin_s5_recovery(permits).unwrap();
        assert!(session.next_root_accesses().is_none());
        let recovered = session
            .recover_next(None, &crate::paths::CallerIdentity::new("owner", "session"))
            .unwrap()
            .unwrap();
        assert_eq!(recovered.journal_id, "journal-s5");
        assert_eq!(fs::read(path).unwrap(), before);
    }

    #[test]
    fn retained_publication_validation_rejects_roster_hash_charge_and_locator_drift() {
        let fixture = tempfile::tempdir().unwrap();
        let mut record = retained_publication_record(fixture.path());
        validate_record(&record).unwrap();

        record
            .grant_root_accesses
            .push(record.grant_root_accesses[0].clone());
        assert!(validate_record(&record).is_err());
        record.grant_root_accesses.pop();

        record.retained_evidence.push(RetainedArtifactEvidence {
            role: RetainedArtifactRole::Backup,
            path: fixture
                .path()
                .join(".docflow-s5-backup-evidence.xlsx")
                .to_string_lossy()
                .into_owned(),
            identity: test_identity(4),
            sha256: "c".repeat(64),
            security_sha256: "d".repeat(64),
            default_stream_bytes: 10,
            auxiliary_charge_bytes: 5,
            logical_charge_bytes: 14,
            locator: "journal:journal-s5;root:0;leaf:.docflow-s5-backup-evidence.xlsx".to_string(),
        });
        assert!(validate_record(&record).is_err());
        record.retained_evidence[0].logical_charge_bytes = 15;
        validate_record(&record).unwrap();

        record.retained_evidence[0].sha256 = "UPPER".to_string();
        assert!(validate_record(&record).is_err());
        record.retained_evidence[0].sha256 = "c".repeat(64);
        record.retained_evidence[0].locator = "x".repeat(MAX_RECOVERY_LOCATOR_SCALARS + 1);
        assert!(validate_record(&record).is_err());
    }

    #[test]
    fn validated_s5_record_cas_is_monotonic_and_legacy_routes_reject_s5() {
        let fixture = tempfile::tempdir().unwrap();
        let store = JournalStore::new(fixture.path().join("journals"));
        let mut raw = retained_publication_record(fixture.path());
        let process_lock = crate::progress::GlobalMutationLock::default();
        let permits = store.bind_s5_permits(
            process_lock.try_acquire().unwrap(),
            store.try_acquire_mutation_lock().unwrap(),
        );
        let mut token = ValidatedS5Record::for_test(raw.clone(), permits).unwrap();

        store.write_s5_cas(&mut token, 0).unwrap();
        assert_eq!(token.revision(), 1);
        assert_eq!(store.read_s5_for_test("journal-s5").unwrap().revision, 1);

        token
            .apply_transition(S5Transition::SetPendingArtifactParent(
                PendingArtifactParent {
                    path: fixture
                        .path()
                        .join(".docflow-backups")
                        .to_string_lossy()
                        .into_owned(),
                    locator: "journal:journal-s5;root:0;leaf:.docflow-backups".to_string(),
                },
            ))
            .unwrap();
        assert!(store.write_s5_cas(&mut token, 0).is_err());
        assert_eq!(token.revision(), 1);
        assert!(
            store
                .read_s5_for_test("journal-s5")
                .unwrap()
                .pending_artifact_parent
                .is_none()
        );
        store.write_s5_cas(&mut token, 1).unwrap();
        assert_eq!(token.revision(), 2);

        assert!(store.read("journal-s5").is_err());
        raw.revision = 2;
        assert!(store.write(&raw).is_err());
    }
}

#[cfg(test)]
mod s5_tests {
    use super::*;

    #[test]
    fn interprocess_mutation_lock_contends_across_store_instances_and_releases_on_drop() {
        let fixture = tempfile::tempdir().unwrap();
        let first_store = JournalStore::new(fixture.path());
        let second_store = JournalStore::new(fixture.path());

        let first = first_store.try_acquire_mutation_lock().unwrap();
        let error = second_store.try_acquire_mutation_lock().unwrap_err();
        assert_eq!(error.code, "operation_in_progress");

        drop(first);
        second_store.try_acquire_mutation_lock().unwrap();
    }
}
