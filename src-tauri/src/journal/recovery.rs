use super::*;

pub(super) fn s5_record_needs_recovery(record: &JournalRecord) -> bool {
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

pub(super) fn invalid_s5_notice_name(filename: &str) -> bool {
    let Some(hash) = filename
        .strip_prefix(".invalid-s5-")
        .and_then(|value| value.strip_suffix(".json"))
    else {
        return false;
    };
    valid_sha256(hash)
}

pub(super) fn recovery_entry(record: &JournalRecord, notice: RecoveryNotice) -> RecoveryEntry {
    RecoveryEntry {
        journal_id: record.journal_id.clone(),
        disposition: RecoveryDisposition::OutcomeUnknown,
        warning_key: Some(notice.warning_key),
        locator: Some(notice.locator),
        restore_original_available: failed_apply_restore_candidate(record),
    }
}

pub(super) fn failed_apply_restore_candidate(record: &JournalRecord) -> bool {
    let Some(publication) = record.pending_publication.as_ref() else {
        return false;
    };
    let state_ready = matches!(
        (record.state, record.recovery_notice.is_some()),
        (Some(JournalState::RecoveryRequired), true) | (Some(JournalState::Applying), false)
    );
    state_ready
        && record.op_id == "rfi.follow_up_sync"
        && record.steps.is_empty()
        && record.pending_step.is_none()
        && record.pending_undo.is_none()
        && record.undone_steps.is_empty()
        && publication.purpose == RetainedPublicationPurpose::Apply
        && matches!(
            publication.phase,
            RetainedPublicationPhase::RecoveryRequired
                | RetainedPublicationPhase::RestoringOriginal
        )
        && record
            .pending_retained_artifact
            .as_ref()
            .is_some_and(|pending| {
                pending.role == RetainedArtifactRole::DisplacedOriginal
                    && pending.path == publication.retained
            })
}

pub(super) fn validate_record(record: &JournalRecord) -> Result<(), BackendError> {
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

pub(super) fn has_s5_discriminator(record: &JournalRecord) -> bool {
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

pub(super) fn validate_s5_fields(record: &JournalRecord) -> Result<(), BackendError> {
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

pub(super) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn valid_recovery_locator(journal_id: &str, locator: &str) -> bool {
    locator.chars().count() <= MAX_RECOVERY_LOCATOR_SCALARS
        && !locator.chars().any(char::is_control)
        && locator.starts_with(&format!("journal:{journal_id};root:"))
        && locator.contains(";leaf:")
        && !locator.contains('/')
        && !locator.contains('\\')
}

pub(super) fn path_in_recorded_roots(path: &str, roots: &[String]) -> bool {
    let path = Path::new(path);
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        && roots.iter().any(|root| path.starts_with(Path::new(root)))
}

pub(super) fn path_has_access(
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

pub(super) fn validate_journal_id(journal_id: &str) -> Result<(), BackendError> {
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

pub(super) fn read_file(path: &Path) -> Result<JournalRecord, BackendError> {
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

pub(super) fn publish_no_clobber(temporary: &Path, target: &Path) -> Result<(), BackendError> {
    fs::hard_link(temporary, target)
        .map_err(|error| journal_io("journal could not be published", error))?;
    fs::remove_file(temporary)
        .map_err(|error| journal_io("journal temporary file could not be removed", error))?;
    Ok(())
}
