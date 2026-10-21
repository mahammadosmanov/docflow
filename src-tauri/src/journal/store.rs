use super::*;

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

    pub(super) fn write_record_durable(&self, record: &JournalRecord) -> Result<(), BackendError> {
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
        let common = record.op_id == "rfi.follow_up_sync"
            && record.owner_id.as_deref() == Some(caller.owner_id.as_str())
            && record.pending_step.is_none()
            && record.pending_undo.is_none()
            && record.undone_steps.is_empty();
        let committed_apply = common
            && record.state == Some(JournalState::Committed)
            && record.steps.len() == 1
            && record.steps[0].kind == crate::contracts::StepKind::RestoreBackup
            && record.pending_publication.is_none()
            && record.recovery_notice.is_none();
        let failed_apply_restore_state = record.steps.is_empty()
            && ((record.state == Some(JournalState::RecoveryRequired)
                && record.recovery_notice.is_some())
                || (record.state == Some(JournalState::Applying)
                    && record.recovery_notice.is_none()));
        let restore_failed_apply = common
            && failed_apply_restore_state
            && record
                .pending_publication
                .as_ref()
                .is_some_and(|publication| {
                    publication.purpose == RetainedPublicationPurpose::Apply
                        && (publication.phase == RetainedPublicationPhase::RecoveryRequired
                            || publication.phase == RetainedPublicationPhase::RestoringOriginal)
                        && record
                            .pending_retained_artifact
                            .as_ref()
                            .is_some_and(|pending| {
                                pending.role == RetainedArtifactRole::DisplacedOriginal
                                    && pending.path == publication.retained
                            })
                        && (publication.phase != RetainedPublicationPhase::RecoveryRequired
                            || record.state == Some(JournalState::RecoveryRequired))
                        && (publication.phase != RetainedPublicationPhase::RestoringOriginal
                            || matches!(
                                record.state,
                                Some(JournalState::Applying | JournalState::RecoveryRequired)
                            ))
                });
        if !committed_apply && !restore_failed_apply {
            return Err(BackendError::journal_invalid(
                "S5 journal is not eligible for exact undo",
            ));
        }
        validate_s5_fields(&record)?;
        Ok(PendingS5Undo {
            record,
            permits,
            mode: if committed_apply {
                S5UndoMode::CommittedApply
            } else {
                S5UndoMode::RestoreFailedApply
            },
        })
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
                mode: pending.mode,
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

    pub(super) fn scan_pending_recovery_entries(
        &self,
    ) -> Result<VecDeque<PendingS5Entry>, BackendError> {
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
        paths.sort_by_key(std::fs::DirEntry::file_name);
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
    pub(super) fn read_s5_for_test(&self, journal_id: &str) -> Result<JournalRecord, BackendError> {
        self.read_raw(journal_id)
    }

    pub(super) fn read_raw(&self, journal_id: &str) -> Result<JournalRecord, BackendError> {
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

    pub(super) fn path_for(&self, record: &JournalRecord) -> PathBuf {
        let timestamp = record.ts.replace(':', "-");
        let prefix = &record.journal_id[..record.journal_id.len().min(8)];
        self.root.join(format!("{timestamp}_{prefix}.json"))
    }
}
