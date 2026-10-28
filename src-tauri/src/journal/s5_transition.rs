use super::*;

/// Lease facts that gate which S5 transitions a caller may apply.
#[derive(Clone, Copy)]
pub(super) struct S5TransitionLeases {
    pub(super) apply_lease: bool,
    pub(super) undo_lease: bool,
    pub(super) undo_mode: Option<S5UndoMode>,
    pub(super) recovery_lease: bool,
}

impl S5TransitionLeases {
    pub(super) fn from_lease(lease: &S5MutationLease) -> Self {
        Self {
            apply_lease: matches!(lease, S5MutationLease::Apply(_)),
            undo_lease: matches!(lease, S5MutationLease::Undo { .. }),
            undo_mode: match lease {
                S5MutationLease::Undo { mode, .. } => Some(*mode),
                _ => None,
            },
            recovery_lease: matches!(lease, S5MutationLease::Recovery { .. }),
        }
    }
}

/// Applies one transition to a candidate record; the caller validates and commits it.
pub(super) fn apply_candidate_transition(
    candidate: &mut JournalRecord,
    transition: S5Transition,
    leases: S5TransitionLeases,
) -> Result<(), BackendError> {
    if matches!(
        transition,
        S5Transition::FinalizeApplySuccess { .. }
            | S5Transition::FinalizeUndoSuccess { .. }
            | S5Transition::BeginFailedApplyRestore
            | S5Transition::FinalizeFailedApplyRestore
            | S5Transition::FinalizeRecovery { .. }
            | S5Transition::PrepareFailedPartial(_)
            | S5Transition::PrepareOutcomeUnknown(_)
    ) {
        completion_transition(candidate, transition, leases)
    } else {
        staging_transition(candidate, transition, leases)
    }
}

/// Routes staging transitions to their artifact-parent, retained-artifact, or publication group.
fn staging_transition(
    candidate: &mut JournalRecord,
    transition: S5Transition,
    leases: S5TransitionLeases,
) -> Result<(), BackendError> {
    match transition {
        S5Transition::SetPendingArtifactParent(_) | S5Transition::ResolveArtifactParent(_) => {
            artifact_parent_transition(candidate, transition)
        }
        S5Transition::SetPendingRetainedArtifact(_)
        | S5Transition::MarkPendingRetainedCreated(_)
        | S5Transition::MarkPendingRetainedIdentityUnknown
        | S5Transition::FinalizeRetainedArtifact { .. } => {
            retained_artifact_transition(candidate, transition)
        }
        _ => publication_transition(candidate, transition, leases),
    }
}

/// Artifact-parent intent and creation-outcome transitions.
fn artifact_parent_transition(
    candidate: &mut JournalRecord,
    transition: S5Transition,
) -> Result<(), BackendError> {
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
        _ => unreachable!("routed by staging_transition"),
    }
    Ok(())
}

/// Retained-artifact intent, creation, and finalization transitions.
fn retained_artifact_transition(
    candidate: &mut JournalRecord,
    transition: S5Transition,
) -> Result<(), BackendError> {
    match transition {
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
        _ => unreachable!("routed by staging_transition"),
    }
    Ok(())
}

/// Retained publication and canonical-write transitions.
fn publication_transition(
    candidate: &mut JournalRecord,
    transition: S5Transition,
    leases: S5TransitionLeases,
) -> Result<(), BackendError> {
    let S5TransitionLeases {
        apply_lease,
        undo_lease,
        undo_mode,
        ..
    } = leases;
    match transition {
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
                if undo_mode != Some(S5UndoMode::CommittedApply) {
                    return Err(BackendError::journal_invalid(
                        "S5 retained undo publication requires committed undo authority",
                    ));
                }
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
        _ => unreachable!("routed by staging_transition"),
    }
    Ok(())
}

/// Apply/undo completion, failed-apply restore, recovery, and failure-preparation transitions.
fn completion_transition(
    candidate: &mut JournalRecord,
    transition: S5Transition,
    leases: S5TransitionLeases,
) -> Result<(), BackendError> {
    let S5TransitionLeases {
        apply_lease,
        undo_lease,
        undo_mode,
        recovery_lease,
    } = leases;
    match transition {
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
            if !undo_lease || undo_mode != Some(S5UndoMode::CommittedApply) {
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
        S5Transition::BeginFailedApplyRestore => {
            let first_admission = candidate.state == Some(JournalState::RecoveryRequired)
                && candidate.recovery_notice.is_some()
                && candidate
                    .pending_publication
                    .as_ref()
                    .is_some_and(|publication| {
                        publication.phase == RetainedPublicationPhase::RecoveryRequired
                    });
            let repeated_admission = candidate.state == Some(JournalState::Applying)
                && candidate.recovery_notice.is_none()
                && candidate
                    .pending_publication
                    .as_ref()
                    .is_some_and(|publication| {
                        publication.phase == RetainedPublicationPhase::RestoringOriginal
                    });
            let retry_after_unknown = candidate.state == Some(JournalState::RecoveryRequired)
                && candidate.recovery_notice.is_some()
                && candidate
                    .pending_publication
                    .as_ref()
                    .is_some_and(|publication| {
                        publication.phase == RetainedPublicationPhase::RestoringOriginal
                    });
            let displaced_original_pending =
                candidate
                    .pending_publication
                    .as_ref()
                    .is_some_and(|publication| {
                        publication.purpose == RetainedPublicationPurpose::Apply
                            && matches!(
                                publication.phase,
                                RetainedPublicationPhase::RecoveryRequired
                                    | RetainedPublicationPhase::RestoringOriginal
                            )
                            && candidate
                                .pending_retained_artifact
                                .as_ref()
                                .is_some_and(|pending| {
                                    pending.role == RetainedArtifactRole::DisplacedOriginal
                                        && pending.path == publication.retained
                                })
                    });
            if undo_mode != Some(S5UndoMode::RestoreFailedApply)
                || (!first_admission && !repeated_admission && !retry_after_unknown)
                || !displaced_original_pending
                || !candidate.steps.is_empty()
                || candidate.pending_undo.is_some()
                || candidate.pending_step.is_some()
                || !candidate.undone_steps.is_empty()
            {
                return Err(BackendError::journal_invalid(
                    "S5 failed apply restore admission is invalid",
                ));
            }
            let publication = candidate
                .pending_publication
                .as_mut()
                .filter(|publication| {
                    publication.purpose == RetainedPublicationPurpose::Apply
                        && matches!(
                            publication.phase,
                            RetainedPublicationPhase::RecoveryRequired
                                | RetainedPublicationPhase::RestoringOriginal
                        )
                })
                .ok_or_else(|| {
                    BackendError::journal_invalid("S5 failed apply restore publication is invalid")
                })?;
            publication.phase = RetainedPublicationPhase::RestoringOriginal;
            candidate.recovery_notice = None;
            candidate.state = Some(JournalState::Applying);
        }
        S5Transition::FinalizeFailedApplyRestore => {
            if undo_mode != Some(S5UndoMode::RestoreFailedApply)
                || candidate.state != Some(JournalState::Applying)
                || !candidate.steps.is_empty()
                || candidate.pending_step.is_some()
                || candidate.pending_undo.is_some()
                || !candidate.undone_steps.is_empty()
                || candidate.recovery_notice.is_some()
                || candidate
                    .pending_publication
                    .as_ref()
                    .is_none_or(|publication| {
                        publication.purpose != RetainedPublicationPurpose::Apply
                            || publication.phase != RetainedPublicationPhase::RestoringOriginal
                            || candidate
                                .pending_retained_artifact
                                .as_ref()
                                .is_none_or(|pending| {
                                    pending.role != RetainedArtifactRole::DisplacedOriginal
                                        || pending.path != publication.retained
                                })
                    })
            {
                return Err(BackendError::journal_invalid(
                    "S5 failed apply restore completion is invalid",
                ));
            }
            candidate
                .retained_evidence
                .retain(|evidence| evidence.role != RetainedArtifactRole::DisplacedOriginal);
            candidate.pending_publication = None;
            candidate.pending_artifact_parent = None;
            candidate.pending_retained_artifact = None;
            candidate.follow_up_phase = None;
            candidate.recovery_notice = None;
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
                S5RecoveryTerminal::CompletedRestore => {
                    let publication = publication
                        .filter(|item| {
                            item.purpose == RetainedPublicationPurpose::Apply
                                && item.phase == RetainedPublicationPhase::RestoringOriginal
                        })
                        .ok_or_else(|| {
                            BackendError::journal_invalid(
                                "S5 restored apply publication is missing",
                            )
                        })?;
                    if !candidate.steps.is_empty()
                        || candidate.pending_step.is_some()
                        || candidate.pending_undo.is_some()
                        || !candidate.undone_steps.is_empty()
                        || !matches!(
                            (candidate.state, candidate.recovery_notice.is_some()),
                            (Some(JournalState::Applying), false)
                                | (Some(JournalState::RecoveryRequired), true)
                        )
                        || candidate
                            .pending_retained_artifact
                            .as_ref()
                            .is_none_or(|pending| {
                                pending.role != RetainedArtifactRole::DisplacedOriginal
                                    || pending.path != publication.retained
                            })
                    {
                        return Err(BackendError::journal_invalid(
                            "S5 completed restore state is invalid",
                        ));
                    }
                    candidate.retained_evidence.retain(|evidence| {
                        evidence.role != RetainedArtifactRole::DisplacedOriginal
                    });
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
            if let Some(publication) =
                candidate
                    .pending_publication
                    .as_mut()
                    .filter(|publication| {
                        publication.phase != RetainedPublicationPhase::RestoringOriginal
                    })
            {
                publication.phase = RetainedPublicationPhase::RecoveryRequired;
            }
            candidate.recovery_notice = Some(notice);
            candidate.state = Some(JournalState::RecoveryRequired);
        }
        _ => unreachable!("routed by apply_candidate_transition"),
    }
    Ok(())
}
