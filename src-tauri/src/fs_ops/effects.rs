use super::*;

impl FilesystemAuthority {
    /// Authorizes a plan path for I/O and anchors it, mapping failures to no-effect step failures.
    pub(super) fn anchor_plan_path(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        path: &Path,
        intent: PathIntent,
    ) -> Result<AnchoredPath, StepFailure> {
        self.grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                path,
                intent,
            )
            .map_err(step_no_effect)?
            .anchor()
            .map_err(io_no_effect)
    }

    /// Authorizes a plan path for writing, mapping denial to a no-effect step failure.
    fn authorize_write_path(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        path: &Path,
    ) -> Result<PathBuf, StepFailure> {
        self.grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                path,
                PathIntent::Write,
            )
            .map_err(step_no_effect)
    }

    pub(super) fn execute_file_effect(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<(), StepFailure> {
        if step.meta.contains_key("contentBase64")
            && (binding.operation_id != CONCRETE_DOCUMENT_OPERATION
                || step.kind != StepKind::Write
                || step.note_key.as_deref() != Some(CONCRETE_DOCUMENT_NOTE)
                || step.meta.contains_key("content")
                || !step.meta.get("contentBase64").is_some_and(Value::is_string))
        {
            return Err(StepFailure::NoEffect(
                "encoded generated payload is not authorized".to_string(),
            ));
        }
        let destination = required_path(step.dst.as_deref(), "step destination")?;
        let destination = self.authorize_write_path(binding, caller, &destination)?;
        let source = if step.kind == StepKind::Write {
            None
        } else {
            let source = required_path(step.src.as_deref(), "step source")?;
            let intent = if step.kind == StepKind::Copy {
                PathIntent::Read
            } else {
                PathIntent::Write
            };
            Some(
                self.grants
                    .authorize_for_plan(
                        &binding.grant_ids,
                        caller,
                        &binding.operation_id,
                        &source,
                        intent,
                    )
                    .map_err(step_no_effect)?,
            )
        };
        if source.as_deref() == Some(destination.as_path()) {
            return Err(StepFailure::NoEffect(
                "source and destination are the same path".to_string(),
            ));
        }
        let parent = destination.parent().ok_or_else(|| {
            StepFailure::NoEffect("destination has no parent directory".to_string())
        })?;
        self.create_missing_directories(parent, step, binding, caller, token, record)?;
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        let mut actual = step.clone();
        actual.src = source
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        actual.dst = Some(destination.to_string_lossy().into_owned());
        actual.meta.remove("content");
        actual.meta.remove("contentBase64");
        let source_anchor = source
            .as_deref()
            .map(|source| {
                self.grants
                    .authorize_capability_for_plan(
                        &binding.grant_ids,
                        caller,
                        &binding.operation_id,
                        source,
                        if step.kind == StepKind::Copy {
                            PathIntent::Read
                        } else {
                            PathIntent::Write
                        },
                    )
                    .map_err(step_no_effect)?
                    .anchor()
                    .map_err(io_no_effect)
            })
            .transpose()?;
        let destination_anchor =
            self.anchor_plan_path(binding, caller, &destination, PathIntent::Write)?;
        if let Some(source) = source_anchor.as_ref() {
            if let Some(expected) = step
                .meta
                .get("expectedSourceSha256")
                .and_then(Value::as_str)
            {
                let hash = source.sha256().map_err(io_no_effect)?;
                if hash != expected {
                    return Err(StepFailure::NoEffect(
                        "copy source changed after preview; preview again".to_string(),
                    ));
                }
                actual
                    .meta
                    .insert("sourceSha256".to_string(), Value::String(hash));
            } else if let Ok(hash) = source.sha256() {
                actual
                    .meta
                    .insert("sourceSha256".to_string(), Value::String(hash));
            }
        }
        record.pending_step = Some(actual.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;
        #[cfg(test)]
        self.run_before_capability_effect();
        let effect = match step.kind {
            StepKind::Copy => destination_anchor
                .copy_no_clobber_from(source_anchor.as_ref().expect("copy source")),
            StepKind::Move | StepKind::Rename => source_anchor
                .as_ref()
                .expect("move source")
                .move_no_clobber_to(&destination_anchor),
            StepKind::Write => {
                if let Some(encoded) = step.meta.get("contentBase64").and_then(Value::as_str) {
                    let bytes = match decode_concrete_document_base64(encoded) {
                        Ok(bytes) => bytes,
                        Err(GeneratedWriteFailure::NoEffect(key))
                        | Err(GeneratedWriteFailure::OutcomeUnknown(key)) => {
                            record.pending_step = None;
                            self.journals.write(record).map_err(step_unknown)?;
                            return Err(StepFailure::NoEffect(key.to_string()));
                        }
                    };
                    destination_anchor.write_no_clobber(&bytes)
                } else if let Some(contents) = step.meta.get("content").and_then(Value::as_str) {
                    destination_anchor.write_no_clobber(contents.as_bytes())
                } else {
                    record.pending_step = None;
                    self.journals.write(record).map_err(step_unknown)?;
                    return Err(StepFailure::NoEffect(
                        "write action has no string content".to_string(),
                    ));
                }
            }
            _ => unreachable!("file effect kind checked by caller"),
        };
        if let Err(error) = effect {
            let outcome_unknown = error.kind() != io::ErrorKind::AlreadyExists
                && destination_anchor.open_read().is_ok();
            if outcome_unknown {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    destination.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(io_no_effect(error));
        }
        #[cfg(test)]
        if self
            .fail_after_effect
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(StepFailure::OutcomeUnknown(
                "injected failure after filesystem effect".to_string(),
            ));
        }
        let hash = destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })?;
        actual
            .meta
            .insert("undoSha256".to_string(), Value::String(hash));
        record.steps.push(actual);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(())
    }

    pub(super) fn execute_workbook_update(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        let workbook = required_path(step.src.as_deref(), "workbook source")?;
        let workbook = self.authorize_write_path(binding, caller, &workbook)?;
        let backup = required_path(step.dst.as_deref(), "workbook backup")?;
        let backup = self.authorize_write_path(binding, caller, &backup)?;
        if workbook == backup {
            return Err(StepFailure::NoEffect(
                "workbook and backup are the same path".to_string(),
            ));
        }
        let signature = step
            .meta
            .get("sourceSignature")
            .and_then(Value::as_object)
            .ok_or_else(|| StepFailure::NoEffect("workbook signature is missing".to_string()))?;
        let expected_size = signature
            .get("size")
            .and_then(Value::as_u64)
            .ok_or_else(|| StepFailure::NoEffect("workbook size is missing".to_string()))?;
        let expected_hash = signature
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| StepFailure::NoEffect("workbook hash is missing".to_string()))?;
        let workbook_capability = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &workbook,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let workbook_anchor = workbook_capability.anchor().map_err(io_no_effect)?;
        let actual_size = workbook_anchor.len().map_err(io_no_effect)?;
        let actual_hash = workbook_anchor.sha256().map_err(io_no_effect)?;
        if actual_size != expected_size || actual_hash != expected_hash {
            return Err(StepFailure::NoEffect(
                "workbook changed after preview".to_string(),
            ));
        }
        workbook_anchor.open_mutable().map(drop).map_err(|_| {
            StepFailure::WarningNoEffect(Warning {
                key: "error.rfi.workbookLocked".to_string(),
                subject: workbook
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string(),
            })
        })?;

        let backup_parent = backup
            .parent()
            .ok_or_else(|| StepFailure::NoEffect("workbook backup has no parent".to_string()))?;
        self.create_missing_directories(backup_parent, step, binding, caller, token, record)?;
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;

        let mut backup_step = Step {
            kind: StepKind::Copy,
            src: Some(workbook.to_string_lossy().into_owned()),
            dst: Some(backup.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.workbookUpdate".to_string()),
            meta: JsonObject::new(),
        };
        backup_step.meta.insert(
            "sourceSha256".to_string(),
            Value::String(actual_hash.clone()),
        );
        record.pending_step = Some(backup_step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;
        let backup_anchor = self.anchor_plan_path(binding, caller, &backup, PathIntent::Write)?;
        if let Err(error) = backup_anchor.copy_no_clobber_from(&workbook_anchor) {
            if error.kind() != io::ErrorKind::AlreadyExists && backup_anchor.open_read().is_ok() {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    backup.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::NoEffect(format!(
                "workbook backup {}: {error}",
                backup.display()
            )));
        }
        let backup_hash = backup_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", backup.display()))
        })?;
        if backup_hash != actual_hash {
            return Err(StepFailure::OutcomeUnknown(
                "workbook backup verification failed".to_string(),
            ));
        }
        backup_step
            .meta
            .insert("undoSha256".to_string(), Value::String(backup_hash));
        record.steps.push(backup_step);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;

        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        let workspace = tempfile::tempdir().map_err(io_no_effect)?;
        let workspace_capability =
            crate::cap_fs::CapabilityRoot::open(workspace.path()).map_err(io_no_effect)?;
        let input = workspace.path().join("approved-input.xlsm");
        let output = workspace.path().join("completed-output.xlsm");
        let input_anchor = workspace_capability
            .authorize(&input)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        input_anchor
            .copy_no_clobber_from(&workbook_anchor)
            .map_err(|error| {
                StepFailure::NoEffect(format!("workbook staging {}: {error}", input.display()))
            })?;
        let updates = workbook_step_updates(step).map_err(step_no_effect)?;
        if let Err(error) = patch_workbook_sheets(&input, &output, &updates, Some(1)) {
            return Err(StepFailure::NoEffect(format!(
                "workbook patch {}: {error}",
                output.display()
            )));
        }
        if let Err(error) = verify_workbook_step(step, &output) {
            return Err(StepFailure::NoEffect(format!(
                "workbook verification {}: {}",
                output.display(),
                error.message
            )));
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&output)
            .and_then(|file| file.sync_all())
            .map_err(|error| {
                StepFailure::NoEffect(format!("workbook sync {}: {error}", output.display()))
            })?;
        let output_anchor = workspace_capability
            .authorize(&output)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let output_hash = output_anchor.sha256().map_err(|error| {
            StepFailure::NoEffect(format!("workbook hash {}: {error}", output.display()))
        })?;
        if token.checkpoint().is_err() {
            return Err(StepFailure::Cancelled);
        }

        let mut restore_step = Step {
            kind: StepKind::RestoreBackup,
            src: Some(backup.to_string_lossy().into_owned()),
            dst: Some(workbook.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.restoreBackup".to_string()),
            meta: JsonObject::new(),
        };
        restore_step
            .meta
            .insert("originalSha256".to_string(), Value::String(actual_hash));
        record.pending_step = Some(restore_step.clone());
        self.journals.write(record).map_err(step_unknown)?;
        if let Err(error) = atomic_replace_capability_with_retry(&output_anchor, &workbook_anchor) {
            return Err(replacement_failure(&workbook_anchor, error));
        }
        let applied_hash = match workbook_anchor.sha256() {
            Ok(hash) => hash,
            Err(error) => {
                if self.rollback_workbook_after_failure(
                    record,
                    &backup_anchor,
                    &workbook_anchor,
                    expected_hash,
                )? {
                    return Err(io_no_effect(error));
                }
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    workbook.display()
                )));
            }
        };
        if applied_hash != output_hash {
            if self.rollback_workbook_after_failure(
                record,
                &backup_anchor,
                &workbook_anchor,
                expected_hash,
            )? {
                return Err(StepFailure::NoEffect(
                    "workbook replacement verification failed".to_string(),
                ));
            }
            return Err(StepFailure::OutcomeUnknown(
                "workbook replacement verification failed".to_string(),
            ));
        }
        restore_step
            .meta
            .insert("appliedSha256".to_string(), Value::String(applied_hash));
        record.steps.push(restore_step);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        workbook_record_count(step).map_err(step_no_effect)
    }

    /// Restores the backup over the workbook after a failed replacement.
    /// Returns `Ok(false)` when the restore itself failed, leaving the outcome unknown.
    fn rollback_workbook_after_failure(
        &self,
        record: &mut JournalRecord,
        backup: &AnchoredPath,
        workbook: &AnchoredPath,
        expected_hash: &str,
    ) -> Result<bool, StepFailure> {
        if rollback_from_backup_capability(backup, workbook, expected_hash).is_err() {
            return Ok(false);
        }
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(true)
    }

    pub(super) fn execute_cleanup_temp(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        if binding.operation_id != "rfi.stage_signed_first_pages" {
            return Err(StepFailure::NoEffect(
                "temporary PDF cleanup is not authorized for this operation".to_string(),
            ));
        }
        let temporary = required_path(step.src.as_deref(), "stale PDF temporary path")?;
        let temporary = self.authorize_write_path(binding, caller, &temporary)?;
        let destination = required_path(step.dst.as_deref(), "completed review PDF")?;
        let destination = self.authorize_write_path(binding, caller, &destination)?;
        let review_root = step
            .meta
            .get("reviewRoot")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| StepFailure::NoEffect("review root is missing".to_string()))?;
        let review_root = self.authorize_write_path(binding, caller, &review_root)?;
        if !temporary.starts_with(&review_root)
            || !destination.starts_with(&review_root)
            || temporary.parent() != destination.parent()
        {
            return Err(StepFailure::NoEffect(
                "temporary PDF cleanup escaped the approved review path".to_string(),
            ));
        }
        let destination_name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StepFailure::NoEffect("review PDF name is invalid".to_string()))?;
        let temporary_name = temporary
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StepFailure::NoEffect("temporary PDF name is invalid".to_string()))?;
        if !temporary_pdf_name_matches(temporary_name, destination_name) {
            return Err(StepFailure::NoEffect(
                "temporary PDF name is outside the cleanup contract".to_string(),
            ));
        }
        let temporary_anchor =
            self.anchor_plan_path(binding, caller, &temporary, PathIntent::Write)?;
        let destination_anchor =
            self.anchor_plan_path(binding, caller, &destination, PathIntent::Write)?;
        if !temporary_anchor
            .same_file_as(&destination_anchor)
            .map_err(io_no_effect)?
        {
            return Err(StepFailure::NoEffect(
                "temporary PDF is not the completed review PDF hard link".to_string(),
            ));
        }
        let expected_hash = step
            .meta
            .get("destinationSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("review PDF hash is missing".to_string()))?;
        if destination_anchor.sha256().map_err(io_no_effect)? != expected_hash {
            return Err(StepFailure::NoEffect(
                "completed review PDF changed after preview".to_string(),
            ));
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        record.pending_step = Some(step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;
        if let Err(error) = remove_anchored_file_with_access_retry(&temporary_anchor, 40) {
            if temporary_anchor.open_read().is_ok()
                && destination_anchor
                    .sha256()
                    .is_ok_and(|hash| hash == expected_hash)
            {
                record.pending_step = None;
                self.journals.write(record).map_err(step_unknown)?;
                return Err(io_no_effect(error));
            }
            return Err(StepFailure::OutcomeUnknown(format!(
                "{}: {error}",
                temporary.display()
            )));
        }
        if destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })? != expected_hash
        {
            return Err(StepFailure::OutcomeUnknown(
                "completed review PDF changed during temporary cleanup".to_string(),
            ));
        }
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }
}
