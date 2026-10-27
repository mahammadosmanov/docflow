use std::io::{Read, Seek, SeekFrom};

use sha2::{Digest, Sha256};

use crate::{
    cap_fs::{AnchoredPath, CreationSecurityDescriptor, FileSecuritySnapshot, SecuredCreatedFile},
    errors::BackendError,
    journal::{
        FilesystemObjectIdentity, JournalStore, RecoveryDisposition, RecoveryEntry, RecoveryNotice,
        RetainedArtifactEvidence, RetainedArtifactRole, RetainedPublicationPhase,
        RetainedPublicationPurpose, S5RecoveryTerminal, S5Transition, ValidatedS5Record,
    },
    progress::CancellationToken,
};

pub(crate) enum PublicationCancellation<'a> {
    Apply(&'a CancellationToken),
    NotCancellable,
}

pub(crate) enum RetainedPublicationOutcome {
    Published {
        created_identity: crate::journal::FilesystemObjectIdentity,
    },
    CancelledBeforeDisplacement,
    RecoveryRequired(RecoveryNotice),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_retained_publication<R, B, V>(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
    canonical: &AnchoredPath,
    retained: &AnchoredPath,
    publication_source: &mut R,
    destination_creation: &CreationSecurityDescriptor,
    destination_snapshot: &FileSecuritySnapshot,
    cancellation: PublicationCancellation<'_>,
    before_canonical_create: B,
    verify_destination: V,
) -> Result<RetainedPublicationOutcome, BackendError>
where
    R: Read + Seek,
    B: FnOnce() -> Result<(), BackendError>,
    V: FnOnce(&mut SecuredCreatedFile) -> Result<(), BackendError>,
{
    if let PublicationCancellation::Apply(token) = cancellation
        && token.checkpoint().is_err()
    {
        return Ok(RetainedPublicationOutcome::CancelledBeforeDisplacement);
    }
    let publication = record
        .pending_publication()
        .cloned()
        .ok_or_else(|| BackendError::journal_invalid("S5 publication is not prepared"))?;
    let pending_retained = record
        .pending_retained_artifact()
        .cloned()
        .ok_or_else(|| BackendError::journal_invalid("S5 retained intent is missing"))?;
    let expected_role = match publication.purpose {
        RetainedPublicationPurpose::Apply => RetainedArtifactRole::DisplacedOriginal,
        RetainedPublicationPurpose::Undo => RetainedArtifactRole::DisplacedApplied,
    };
    if publication.phase != RetainedPublicationPhase::Prepared
        || pending_retained.role != expected_role
        || pending_retained.path != publication.retained
    {
        return Err(BackendError::journal_invalid(
            "S5 retained publication authority is inconsistent",
        ));
    }
    transition_phase(
        journals,
        record,
        RetainedPublicationPhase::Prepared,
        RetainedPublicationPhase::Displacing,
    )?;
    let fail = |step: &str, detail: String| {
        note_failure(step, &detail);
        recovery_required(&pending_retained.locator)
    };
    if let Err(error) = canonical.retain_current_name_no_clobber(retained) {
        return Ok(fail("retain_original", format!("{error:?}")));
    }
    let retained_file = match retained.open_read() {
        Ok(file) => file,
        Err(error) => return Ok(fail("open_retained", error.to_string())),
    };
    let retained_identity = match crate::cap_fs::file_identity(&retained_file) {
        Ok(identity) => identity,
        Err(error) => return Ok(fail("retained_identity", error.to_string())),
    };
    let retained_bytes = retained_file.metadata().map(|metadata| metadata.len());
    let retained_security = FileSecuritySnapshot::capture(&retained_file, None);
    let retained_hash = sha256_reader(retained_file);
    let (retained_bytes, retained_security, retained_hash) =
        match (retained_bytes, retained_security, retained_hash) {
            (Ok(bytes), Ok(security), Ok(hash)) => (bytes, security, hash),
            (bytes, security, hash) => {
                return Ok(fail(
                    "read_retained",
                    format!(
                        "metadata={:?} security={:?} hash={:?}",
                        bytes.err(),
                        security.err(),
                        hash.err()
                    ),
                ));
            }
        };
    if retained_identity != publication.expected_canonical_identity
        || retained_hash != publication.expected_canonical_sha256
        || retained_security.sha256() != publication.expected_canonical_security_sha256
    {
        return Ok(fail(
            "retained_mismatch",
            format!(
                "identity_same={} hash_same={} security_same={}",
                retained_identity == publication.expected_canonical_identity,
                retained_hash == publication.expected_canonical_sha256,
                retained_security.sha256() == publication.expected_canonical_security_sha256
            ),
        ));
    }
    let auxiliary_charge_bytes = retained_security.auxiliary_charge_bytes();
    let logical_charge_bytes = match retained_bytes.checked_add(auxiliary_charge_bytes) {
        Some(value) => value,
        None => return Ok(fail("retained_size", String::new())),
    };
    let evidence = RetainedArtifactEvidence {
        role: expected_role,
        path: publication.retained.clone(),
        identity: retained_identity,
        sha256: retained_hash,
        security_sha256: retained_security.sha256(),
        default_stream_bytes: retained_bytes,
        auxiliary_charge_bytes,
        logical_charge_bytes,
        locator: pending_retained.locator.clone(),
    };
    transition(
        journals,
        record,
        S5Transition::RecordRetainedPublicationEvidence(evidence),
    )?;
    before_canonical_create()?;
    transition_phase(
        journals,
        record,
        RetainedPublicationPhase::Retained,
        RetainedPublicationPhase::CreatingCanonical,
    )?;

    let mut created = match canonical.create_new_secured(destination_creation) {
        Ok(file) => file,
        Err(error) => return Ok(fail("create_canonical", error.to_string())),
    };
    let created_identity = match created.identity() {
        Ok(identity) => identity,
        Err(error) => return Ok(fail("created_identity", error.to_string())),
    };
    if let Err(error) = destination_snapshot.verify_created_access(&created) {
        return Ok(fail("verify_created_access", error.to_string()));
    }
    transition(
        journals,
        record,
        S5Transition::RecordCanonicalSecured {
            identity: created_identity.clone(),
            descriptor_sha256: destination_snapshot.access_sha256(),
        },
    )?;
    if let Err(error) = canonical.sync_parent() {
        return Ok(fail("sync_parent", error.to_string()));
    }
    if let Err(error) = destination_snapshot.replay_and_verify_auxiliary(&mut created) {
        return Ok(fail("replay_auxiliary", error.to_string()));
    }
    transition(
        journals,
        record,
        S5Transition::RecordCanonicalAuxiliaryReady {
            auxiliary_sha256: destination_snapshot.auxiliary_sha256(),
            auxiliary_charge_bytes: destination_snapshot.auxiliary_charge_bytes(),
        },
    )?;
    transition(journals, record, S5Transition::MarkCanonicalBytesStarted)?;
    publication_source
        .seek(SeekFrom::Start(0))
        .and_then(|_| std::io::copy(publication_source, &mut created).map(|_| ()))
        .and_then(|_| created.sync_all())
        .map_err(|error| {
            note_failure("write_canonical", &error.to_string());
            BackendError::validation("error.rfi.followUp.recoveryRequired")
        })?;
    transition(journals, record, S5Transition::MarkCanonicalVerifying)?;
    verify_destination(&mut created)?;
    if let Err(error) = destination_snapshot.verify_complete(&created) {
        return Ok(fail("verify_complete", error.to_string()));
    }
    if created.identity().ok().as_ref() != Some(&created_identity) {
        return Ok(fail("created_identity_changed", String::new()));
    }
    let created_hash = sha256_reader(&mut created).map_err(|error| {
        note_failure("hash_canonical", &error.to_string());
        BackendError::validation("error.rfi.followUp.recoveryRequired")
    })?;
    if created_hash != publication.publication_sha256 {
        return Ok(fail("published_hash_mismatch", String::new()));
    }
    if canonical.identity().ok().as_ref() != Some(&created_identity) {
        return Ok(fail("canonical_identity_mismatch", String::new()));
    }
    Ok(RetainedPublicationOutcome::Published { created_identity })
}

pub(crate) fn reconcile_retained_publication(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
) -> Result<Option<RecoveryEntry>, BackendError> {
    let publication = record
        .pending_publication()
        .cloned()
        .ok_or_else(|| BackendError::journal_invalid("S5 publication is not pending"))?;
    let locator = record
        .pending_retained_artifact()
        .map(|pending| pending.locator.clone())
        .or_else(|| {
            record
                .retained_evidence()
                .iter()
                .find(|evidence| evidence.path == publication.retained)
                .map(|evidence| evidence.locator.clone())
        })
        .ok_or_else(|| BackendError::journal_invalid("S5 recovery locator is missing"))?;
    let canonical = observe_recorded_path(record, &publication.canonical);
    let retained = observe_recorded_path(record, &publication.retained);
    let backup = observe_recorded_path(record, &publication.backup);
    let source = observe_recorded_path(record, &publication.publication_source);
    if publication.phase == RetainedPublicationPhase::RestoringOriginal {
        let notice = RecoveryNotice {
            warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
            locator,
        };
        let restored = canonical.matches(
            &publication.expected_canonical_identity,
            &publication.expected_canonical_sha256,
            &publication.expected_canonical_security_sha256,
        ) && matches!(retained, PathObservation::Absent);
        if restored && sync_recorded_parent(record, &publication.canonical) {
            transition(
                journals,
                record,
                S5Transition::FinalizeRecovery {
                    terminal: S5RecoveryTerminal::CompletedRestore,
                    notice: None,
                    evidence: Vec::new(),
                },
            )?;
            return Ok(None);
        }
        return Ok(Some(RecoveryEntry {
            journal_id: record.journal_id().to_string(),
            disposition: RecoveryDisposition::OutcomeUnknown,
            warning_key: Some(notice.warning_key),
            locator: Some(notice.locator),
            restore_original_available: true,
        }));
    }
    let backup_exact = backup.matches(
        &publication.expected_backup_identity,
        &publication.expected_backup_sha256,
        &publication.expected_backup_security_sha256,
    );
    let source_exact = source.matches(
        &publication.publication_source_identity,
        &publication.publication_sha256,
        &publication.publication_security_sha256,
    );
    let retained_exact = match (
        publication.retained_identity.as_ref(),
        publication.retained_sha256.as_deref(),
        publication.retained_security_sha256.as_deref(),
    ) {
        (Some(identity), Some(hash), Some(security)) => {
            retained.matches(identity, hash, security)
                && identity == &publication.expected_canonical_identity
                && hash == publication.expected_canonical_sha256
                && security == publication.expected_canonical_security_sha256
        }
        _ => false,
    };
    let exact_destination =
        publication
            .created_canonical_identity
            .as_ref()
            .is_some_and(|identity| {
                canonical.matches(
                    identity,
                    &publication.publication_sha256,
                    &publication.expected_backup_security_sha256,
                )
            });
    if backup_exact && source_exact && retained_exact && exact_destination {
        let terminal = match publication.purpose {
            RetainedPublicationPurpose::Apply => S5RecoveryTerminal::CompletedApply,
            RetainedPublicationPurpose::Undo => S5RecoveryTerminal::CompletedUndo,
        };
        transition(
            journals,
            record,
            S5Transition::FinalizeRecovery {
                terminal,
                notice: None,
                evidence: Vec::new(),
            },
        )?;
        return Ok(None);
    }
    let no_canonical_effect = publication.created_canonical_identity.is_none()
        && canonical.matches(
            &publication.expected_canonical_identity,
            &publication.expected_canonical_sha256,
            &publication.expected_canonical_security_sha256,
        )
        && matches!(retained, PathObservation::Absent)
        && backup_exact
        && source_exact;
    let notice = RecoveryNotice {
        warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
        locator,
    };
    transition(
        journals,
        record,
        S5Transition::FinalizeRecovery {
            terminal: if no_canonical_effect {
                S5RecoveryTerminal::FailedPartial
            } else {
                S5RecoveryTerminal::OutcomeUnknown
            },
            notice: Some(notice.clone()),
            evidence: Vec::new(),
        },
    )?;
    Ok(Some(RecoveryEntry {
        journal_id: record.journal_id().to_string(),
        disposition: RecoveryDisposition::OutcomeUnknown,
        warning_key: Some(notice.warning_key),
        locator: Some(notice.locator),
        restore_original_available: false,
    }))
}

fn sync_recorded_parent(record: &ValidatedS5Record, raw_path: &str) -> bool {
    let path = std::path::Path::new(raw_path);
    record.roots().is_some_and(|roots| {
        roots.roots().iter().any(|root| {
            root.authorize(path)
                .ok()
                .and_then(|authorized| authorized.anchor().ok())
                .is_some_and(|anchor| anchor.sync_parent().is_ok())
        })
    })
}

enum PathObservation {
    Absent,
    Present {
        identity: FilesystemObjectIdentity,
        sha256: String,
        security_sha256: String,
    },
    Unknown,
}

impl PathObservation {
    fn matches(&self, identity: &FilesystemObjectIdentity, hash: &str, security: &str) -> bool {
        matches!(
            self,
            Self::Present {
                identity: observed_identity,
                sha256,
                security_sha256,
            } if observed_identity == identity && sha256 == hash && security_sha256 == security
        )
    }
}

fn observe_recorded_path(record: &ValidatedS5Record, raw_path: &str) -> PathObservation {
    let Some(roots) = record.roots() else {
        return PathObservation::Unknown;
    };
    let path = std::path::Path::new(raw_path);
    let Some(anchored) = roots
        .roots()
        .iter()
        .find_map(|root| root.authorize(path).ok())
        .and_then(|authorized| authorized.anchor().ok())
    else {
        return PathObservation::Unknown;
    };
    let file = match anchored.open_read() {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return PathObservation::Absent;
        }
        Err(_) => return PathObservation::Unknown,
    };
    let identity = match crate::cap_fs::file_identity(&file) {
        Ok(identity) => identity,
        Err(_) => return PathObservation::Unknown,
    };
    let security = match FileSecuritySnapshot::capture(&file, None) {
        Ok(security) => security,
        Err(_) => return PathObservation::Unknown,
    };
    let sha256 = match sha256_reader(file) {
        Ok(hash) => hash,
        Err(_) => return PathObservation::Unknown,
    };
    PathObservation::Present {
        identity,
        sha256,
        security_sha256: security.sha256(),
    }
}

fn transition(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
    transition: S5Transition,
) -> Result<(), BackendError> {
    let expected = record.revision();
    record.apply_transition(transition)?;
    journals.write_s5_cas(record, expected)
}

fn transition_phase(
    journals: &JournalStore,
    record: &mut ValidatedS5Record,
    expected_phase: RetainedPublicationPhase,
    next_phase: RetainedPublicationPhase,
) -> Result<(), BackendError> {
    let expected_revision = record.revision();
    record.mark_publication_phase(expected_phase, next_phase)?;
    journals.write_s5_cas(record, expected_revision)
}

static LAST_FAILURE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Records which publication step failed and the OS cause, for the activity
/// log only. Never feeds journal state, authority, or user-visible messages.
pub(crate) fn note_failure(step: &str, detail: &str) {
    if std::env::var_os("DOCFLOW_DEBUG_S5").is_some() {
        eprintln!("S5FAIL {step}: {detail}");
    }
    *LAST_FAILURE.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("{step}: {detail}"));
}

pub(crate) fn take_last_failure() -> Option<String> {
    LAST_FAILURE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

fn recovery_required(locator: &str) -> RetainedPublicationOutcome {
    RetainedPublicationOutcome::RecoveryRequired(RecoveryNotice {
        warning_key: "error.rfi.followUp.recoveryRequired".to_string(),
        locator: locator.to_string(),
    })
}

fn sha256_reader<R: Read + Seek>(mut reader: R) -> std::io::Result<String> {
    reader.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    reader.seek(SeekFrom::Start(0))?;
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod failure_note_tests {
    use super::*;

    #[test]
    fn failure_note_is_taken_once() {
        note_failure("sync_parent", "os error 1");
        assert_eq!(
            take_last_failure().as_deref(),
            Some("sync_parent: os error 1")
        );
        assert_eq!(take_last_failure(), None);
    }
}
