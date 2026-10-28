use super::*;

impl ValidatedS5Record {
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

    pub(crate) fn undo_mode(&self) -> Option<S5UndoMode> {
        match &self.lease {
            S5MutationLease::Undo { mode, .. } => Some(*mode),
            _ => None,
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
