use super::s5_transition::{S5TransitionLeases, apply_candidate_transition};
use super::*;

impl ValidatedS5Record {
    #[cfg(test)]
    pub(super) fn for_test(
        record: JournalRecord,
        permits: S5PermitBundle,
    ) -> Result<Self, BackendError> {
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
        let leases = S5TransitionLeases::from_lease(&self.lease);
        let mut candidate = self.record.clone();
        apply_candidate_transition(&mut candidate, transition, leases)?;
        validate_record(&candidate)?;
        self.record = candidate;
        Ok(())
    }
}

pub(super) fn upsert_evidence(
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

pub(super) fn recovered_restore_step(
    publication: &RetainedPublication,
) -> Result<Step, BackendError> {
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
    pub(super) root: PathBuf,
    /// Identifies this store (clones share it) for lock re-entry.
    pub(super) instance: u64,
}
