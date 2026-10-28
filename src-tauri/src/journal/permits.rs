use super::*;

/// Holds the cross-process S5 journal lock. A store re-entering its own held lock
/// on the holding thread counts up instead of re-locking, so nested journal writes
/// inside one apply do not contend with themselves. A different store instance on
/// the same thread still contends. The permit is `!Send`, so its count always
/// drops on the thread that took it.
#[derive(Debug)]
pub(crate) struct InterprocessMutationPermit {
    pub(super) key: HeldKey,
    pub(super) _thread_bound: PhantomData<*const ()>,
}

impl InterprocessMutationPermit {
    /// Re-enters a lock this store already holds on this thread; `None` otherwise.
    pub(crate) fn reenter(instance: u64, path: &Path) -> Option<Self> {
        let key = (instance, path.to_path_buf());
        INTERPROCESS_HELD.with(|held| {
            let mut held = held.borrow_mut();
            let (_, count) = held.get_mut(&key)?;
            *count += 1;
            Some(Self::new(key))
        })
    }

    /// Records a freshly locked file as held by this store on this thread.
    pub(crate) fn hold(instance: u64, path: &Path, file: File) -> Self {
        let key = (instance, path.to_path_buf());
        INTERPROCESS_HELD.with(|held| {
            held.borrow_mut().insert(key.clone(), (file, 1));
        });
        Self::new(key)
    }

    fn new(key: HeldKey) -> Self {
        Self {
            key,
            _thread_bound: PhantomData,
        }
    }
}

impl Drop for InterprocessMutationPermit {
    fn drop(&mut self) {
        INTERPROCESS_HELD.with(|held| {
            let mut held = held.borrow_mut();
            if let Some((_, count)) = held.get_mut(&self.key) {
                *count -= 1;
                if *count == 0 {
                    held.remove(&self.key);
                }
            }
        });
    }
}

#[derive(Debug)]
pub(crate) struct S5PermitBundle {
    pub(super) _process: GlobalMutationPermit,
    pub(super) _interprocess: InterprocessMutationPermit,
}

pub(crate) struct PendingS5Undo {
    pub(super) record: JournalRecord,
    pub(super) permits: S5PermitBundle,
    pub(super) mode: S5UndoMode,
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

pub(super) enum S5MutationLease {
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
    pub(super) record: JournalRecord,
    pub(super) lease: S5MutationLease,
}

pub(crate) struct RetainedLocators(pub(super) Vec<String>);

pub(crate) struct S5RecoverySession {
    pub(super) store: JournalStore,
    pub(super) _permits: S5PermitBundle,
    pub(super) entries: VecDeque<PendingS5Entry>,
}

pub(super) enum PendingS5Entry {
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
