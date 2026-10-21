use super::*;

impl FilesystemAuthority {
    pub(super) fn execute_step(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        match step.kind {
            StepKind::Mkdir => self
                .execute_mkdir(step, binding, caller, token, record)
                .map(|()| 1),
            StepKind::Copy | StepKind::Move | StepKind::Rename | StepKind::Write => self
                .execute_file_effect(step, binding, caller, token, record)
                .map(|()| 1),
            StepKind::WorkbookUpdate => {
                self.execute_workbook_update(step, binding, caller, token, record)
            }
            StepKind::PdfFirstPageReplace => {
                self.execute_pdf_first_page_replace(step, binding, caller, token, record)
            }
            StepKind::PdfPageRotate => {
                self.execute_pdf_page_rotate(step, binding, caller, token, record)
            }
            StepKind::PdfPagesExtract => {
                self.execute_pdf_pages_extract(step, binding, caller, token, record)
            }
            StepKind::ReplicaPublish => {
                self.execute_replica_publish(step, binding, caller, token, record)
            }
            StepKind::CleanupTemp => {
                self.execute_cleanup_temp(step, binding, caller, token, record)
            }
            _ => Err(StepFailure::NoEffect(format!(
                "unsupported Rust filesystem action: {:?}",
                step.kind
            ))),
        }
    }

    pub(super) fn preflight_replica_steps(
        &self,
        steps: &[Step],
        binding: &PlanBinding,
        caller: &CallerIdentity,
    ) -> Result<(), BackendError> {
        for step in steps {
            if step.kind != StepKind::ReplicaPublish {
                continue;
            }
            let meta = replica_publish::decode_step_meta(step)?;
            let source =
                PathBuf::from(step.src.as_deref().ok_or_else(|| {
                    BackendError::validation("error.replica.approvedStepInvalid")
                })?);
            let target =
                PathBuf::from(step.dst.as_deref().ok_or_else(|| {
                    BackendError::validation("error.replica.approvedStepInvalid")
                })?);
            let source = self
                .grants
                .authorize_capability_for_plan(
                    &binding.grant_ids,
                    caller,
                    &binding.operation_id,
                    &source,
                    PathIntent::Read,
                )?
                .anchor()
                .map_err(|_| BackendError::validation("error.replica.sourceChanged"))?;
            let target = self
                .grants
                .authorize_capability_for_plan(
                    &binding.grant_ids,
                    caller,
                    &binding.operation_id,
                    &target,
                    PathIntent::Write,
                )?
                .anchor()
                .map_err(|_| BackendError::validation("error.replica.targetChanged"))?;
            if source
                .sha256()
                .map_err(|_| BackendError::validation("error.replica.sourceChanged"))?
                != meta.source_sha256
            {
                return Err(BackendError::validation("error.replica.sourceChanged"));
            }
            if target
                .sha256()
                .map_err(|_| BackendError::validation("error.replica.targetChanged"))?
                != meta.expected_target_sha256
            {
                return Err(BackendError::validation("error.replica.targetChanged"));
            }
        }
        Ok(())
    }

    pub(super) fn execute_replica_publish(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        let meta = replica_publish::decode_step_meta(step).map_err(step_no_effect)?;
        let source_path = required_path(step.src.as_deref(), "replica source")?;
        let target_path = required_path(step.dst.as_deref(), "replica target")?;
        let backup_path = PathBuf::from(&meta.backup_path);
        let source = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &source_path,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let target = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &target_path,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let backup = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &backup_path,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        if source.sha256().map_err(io_no_effect)? != meta.source_sha256 {
            return Err(StepFailure::NoEffect(
                "error.replica.sourceChanged".to_string(),
            ));
        }
        if target.sha256().map_err(io_no_effect)? != meta.expected_target_sha256 {
            return Err(StepFailure::NoEffect(
                "error.replica.targetChanged".to_string(),
            ));
        }
        match backup.open_read() {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(StepFailure::NoEffect(
                    "error.replica.backupExists".to_string(),
                ));
            }
            Err(error) => return Err(io_no_effect(error)),
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        backup.copy_no_clobber_from(&target).map_err(io_no_effect)?;
        if backup.sha256().map_err(io_no_effect)? != meta.expected_target_sha256 {
            return Err(StepFailure::OutcomeUnknown(
                "error.replica.backupVerificationFailed".to_string(),
            ));
        }

        let temporary = target.temporary_sibling("replica").map_err(io_no_effect)?;
        temporary
            .copy_no_clobber_from(&source)
            .map_err(io_no_effect)?;
        if temporary.sha256().map_err(io_no_effect)? != meta.source_sha256 {
            let _ = temporary.remove_file();
            return Err(StepFailure::NoEffect(
                "error.replica.copyVerificationFailed".to_string(),
            ));
        }
        if token.checkpoint().is_err() {
            let _ = temporary.remove_file();
            return Err(StepFailure::Cancelled);
        }
        let mut restore = Step {
            kind: StepKind::RestoreBackup,
            src: Some(meta.backup_path.clone()),
            dst: Some(target_path.to_string_lossy().into_owned()),
            note_key: Some("note.replica.restore".to_string()),
            meta: JsonObject::new(),
        };
        restore.meta.insert(
            "originalSha256".to_string(),
            Value::String(meta.expected_target_sha256.clone()),
        );
        restore.meta.insert(
            "appliedSha256".to_string(),
            Value::String(meta.source_sha256.clone()),
        );
        record.pending_step = Some(restore.clone());
        self.journals.write(record).map_err(step_unknown)?;
        #[cfg(windows)]
        let replacement =
            overwrite_existing_capability(&temporary, &target, &meta.expected_target_sha256);
        #[cfg(not(windows))]
        let replacement = atomic_replace_capability_with_retry(&temporary, &target);
        if let Err(error) = replacement {
            return Err(replacement_failure(&target, error));
        }
        match target.sha256() {
            Ok(hash) if hash == meta.source_sha256 => {}
            Ok(_applied_hash) => {
                #[cfg(windows)]
                let rollback = target.overwrite_verified_from(&backup, &_applied_hash);
                #[cfg(not(windows))]
                let rollback =
                    rollback_from_backup_capability(&backup, &target, &meta.expected_target_sha256);
                if rollback.is_ok() {
                    record.pending_step = None;
                    self.journals.write(record).map_err(step_unknown)?;
                    return Err(StepFailure::NoEffect(
                        "error.replica.publishVerificationFailed".to_string(),
                    ));
                }
                return Err(StepFailure::OutcomeUnknown(
                    "error.replica.publishVerificationFailed".to_string(),
                ));
            }
            Err(_) => {
                return Err(StepFailure::OutcomeUnknown(
                    "error.replica.publishVerificationFailed".to_string(),
                ));
            }
        }
        record.steps.push(restore);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }

    pub(super) fn execute_mkdir(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<(), StepFailure> {
        let destination = required_path(step.dst.as_deref(), "mkdir destination")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        self.create_missing_directories(&destination, step, binding, caller, token, record)
    }

    pub(super) fn create_missing_directories(
        &self,
        destination: &Path,
        template: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<(), StepFailure> {
        let authorized = self
            .grants
            .authorize_directory_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let mut missing = authorized.missing_directories().map_err(io_no_effect)?;
        while let Some(directory) = missing.next_path().map(Path::to_path_buf) {
            token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
            let actual = Step {
                kind: StepKind::Mkdir,
                src: None,
                dst: Some(directory.to_string_lossy().into_owned()),
                note_key: template.note_key.clone(),
                meta: template.meta.clone(),
            };
            record.pending_step = Some(actual.clone());
            record.state = Some(JournalState::Applying);
            self.journals.write(record).map_err(step_unknown)?;
            #[cfg(test)]
            self.run_before_capability_effect();
            if let Err(error) = missing.create_next() {
                if error.is_outcome_unknown() {
                    return Err(StepFailure::OutcomeUnknown(format!(
                        "{}: {}",
                        directory.display(),
                        error.into_inner()
                    )));
                }
                record.pending_step = None;
                self.journals.write(record).map_err(step_unknown)?;
                return Err(StepFailure::NoEffect(format!(
                    "could not create {}: {}",
                    directory.display(),
                    error.into_inner()
                )));
            }
            record.steps.push(actual);
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
        }
        Ok(())
    }
}
