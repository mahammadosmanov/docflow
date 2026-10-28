use super::*;

impl FilesystemAuthority {
    pub(super) fn apply_rfi_system_sync<F>(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
        mut progress: F,
    ) -> Result<OperationResult, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let claim = self
            .plans
            .claim_for_apply_bound(plan_id, caller, &self.mutation_lock)?;
        if claim.claimed.origin != BackendOrigin::Rust {
            return Err(BackendError::validation(
                "plan is not owned by the Rust filesystem backend",
            ));
        }
        let binding = claim
            .claimed
            .binding
            .as_ref()
            .ok_or_else(BackendError::plan_unavailable)?;
        let roots =
            self.grants
                .roots_for_plan(&binding.grant_ids, caller, &binding.operation_id)?;
        let root_identities = self.grants.root_identities_for_plan(
            &binding.grant_ids,
            caller,
            &binding.operation_id,
        )?;
        let token = self.cancellations.register(plan_id);
        let _registration = CancellationRegistration {
            registry: &self.cancellations,
            request_id: plan_id,
        };
        let mut record = JournalRecord::fresh(
            &claim.claimed.plan.op_id,
            &binding.owner_id,
            &binding.session_id,
            &claim.claimed.digest,
        );
        record.grant_roots = roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        record.grant_root_identities = root_identities;
        record.state = Some(JournalState::Applying);
        self.journals.write(&record)?;

        let mut result = OperationResult {
            applied: 0,
            failed: 0,
            journal_id: record.journal_id.clone(),
            errors: Vec::new(),
            meta: JsonObject::new(),
            cloud_synced: None,
        };

        // Validate every approved workbook before creating a backup or changing
        // either canonical file. This prevents a stale two-workbook plan from
        // partially applying its first step.
        for step in &claim.claimed.plan.steps {
            let step_meta = rfi_system_sync::decode_step_meta(step)?;
            let workbook_path = PathBuf::from(
                step.src
                    .as_deref()
                    .ok_or_else(|| BackendError::validation("Step is missing source path"))?,
            );
            let backup_path = PathBuf::from(
                step.dst
                    .as_deref()
                    .ok_or_else(|| BackendError::validation("Step is missing backup path"))?,
            );
            if workbook_path.as_path() != Path::new(&step_meta.target_path)
                || backup_path.as_path() != Path::new(&step_meta.backup_path)
            {
                return Err(BackendError::validation(
                    "error.rfi.systemSync.approvedUpdateInvalid",
                ));
            }
            let capability = self.grants.authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &workbook_path,
                PathIntent::Write,
            )?;
            let anchor = capability
                .anchor()
                .map_err(|error| BackendError::validation(error.to_string()))?;
            let mut workbook = anchor
                .open_mutable()
                .map_err(|_| BackendError::validation("error.rfi.systemSync.workbookLocked"))?;
            if rfi_follow_up::fingerprint_open_file(&mut workbook)? != step_meta.source_fingerprint
            {
                return Err(BackendError::validation(
                    "error.rfi.systemSync.workbookChanged",
                ));
            }
        }

        let total_steps = claim.claimed.plan.steps.len();
        for (step_idx, step) in claim.claimed.plan.steps.iter().enumerate() {
            if token.checkpoint().is_err() {
                record.state = Some(JournalState::CancelledPartial);
                let _ = self.journals.write(&record);
                return Err(BackendError::stopped());
            }

            let src_path_str = step
                .src
                .as_deref()
                .ok_or_else(|| BackendError::validation("Step is missing source path"))?;
            let dst_path_str = step
                .dst
                .as_deref()
                .ok_or_else(|| BackendError::validation("Step is missing backup path"))?;
            let workbook_path = PathBuf::from(src_path_str);
            let backup_path = PathBuf::from(dst_path_str);

            let workbook_cap = self.grants.authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &workbook_path,
                PathIntent::Write,
            )?;
            let workbook_anchor = workbook_cap
                .anchor()
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let step_meta = rfi_system_sync::decode_step_meta(step)?;
            let mut current_workbook = workbook_anchor
                .open_mutable()
                .map_err(|_| BackendError::validation("error.rfi.systemSync.workbookLocked"))?;
            if rfi_follow_up::fingerprint_open_file(&mut current_workbook)?
                != step_meta.source_fingerprint
            {
                return Err(BackendError::validation(
                    "error.rfi.systemSync.workbookChanged",
                ));
            }
            drop(current_workbook);

            let backup_cap = self.grants.authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &backup_path,
                PathIntent::Write,
            )?;
            let backup_anchor = backup_cap
                .anchor()
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let actual_hash = workbook_anchor
                .sha256()
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let mut backup_step = Step {
                kind: StepKind::Copy,
                src: Some(workbook_path.to_string_lossy().into_owned()),
                dst: Some(backup_path.to_string_lossy().into_owned()),
                note_key: Some("note.rfi.systemSync.backup".to_string()),
                meta: JsonObject::new(),
            };
            backup_step.meta.insert(
                "sourceSha256".to_string(),
                Value::String(actual_hash.clone()),
            );
            record.pending_step = Some(backup_step.clone());
            self.journals.write(&record)?;

            if let Err(error) = backup_anchor.copy_no_clobber_from(&workbook_anchor) {
                record.pending_step = None;
                let _ = self.journals.write(&record);
                return Err(BackendError::validation(format!(
                    "Workbook backup failed for {}: {error}",
                    backup_path.display()
                )));
            }

            let backup_hash = backup_anchor
                .sha256()
                .map_err(|e| BackendError::validation(e.to_string()))?;
            if backup_hash != actual_hash {
                record.pending_step = None;
                let _ = self.journals.write(&record);
                return Err(BackendError::validation(
                    "Backup verification hash mismatch",
                ));
            }
            backup_step
                .meta
                .insert("undoSha256".to_string(), Value::String(backup_hash.clone()));
            record.steps.push(backup_step);
            record.pending_step = None;
            self.journals.write(&record)?;

            let workspace =
                tempfile::tempdir().map_err(|e| BackendError::validation(e.to_string()))?;
            let staged_input = workspace.path().join("staged-input.xlsx");
            let staged_output = workspace.path().join("staged-output.xlsx");

            fs::copy(&workbook_path, &staged_input)
                .map_err(|e| BackendError::validation(e.to_string()))?;

            rfi_system_sync::execute_sync_step(step, &staged_input, &staged_output)?;

            fs::copy(&staged_output, &workbook_path)
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let applied_hash = workbook_anchor
                .sha256()
                .map_err(|e| BackendError::validation(e.to_string()))?;

            let mut restore_step = Step {
                kind: StepKind::RestoreBackup,
                src: Some(backup_path.to_string_lossy().into_owned()),
                dst: Some(workbook_path.to_string_lossy().into_owned()),
                note_key: Some("note.rfi.systemSync.restore".to_string()),
                meta: JsonObject::new(),
            };
            restore_step
                .meta
                .insert("originalSha256".to_string(), Value::String(backup_hash));
            restore_step
                .meta
                .insert("appliedSha256".to_string(), Value::String(applied_hash));
            record.steps.push(restore_step);
            self.journals.write(&record)?;

            result.applied += 1;
            progress(ProgressEvent {
                phase: "apply".to_string(),
                done: (step_idx + 1) as u64,
                total: total_steps as u64,
                subject: workbook_path.to_string_lossy().into_owned(),
            });
        }

        record.state = Some(JournalState::Committed);
        self.journals.write(&record)?;

        result.journal_id = record.journal_id;
        Ok(result)
    }
}
