use super::*;

impl NativeBackend {
    pub(crate) fn preview_rfi_follow_up<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiFollowUpPreviewValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_follow_up::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI Follow Up backend",
            ));
        }
        let source_root = request
            .values
            .rfi_log_workbook
            .parent()
            .ok_or_else(|| BackendError::path_invalid("RFI Log workbook has no parent"))?
            .to_path_buf();
        let target_root = request
            .values
            .follow_up_workbook
            .parent()
            .ok_or_else(|| BackendError::path_invalid("Follow Up workbook has no parent"))?
            .to_path_buf();
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let backup_directory = request
            .values
            .backup_directory
            .as_ref()
            .map(fs::canonicalize)
            .transpose()
            .map_err(|_| BackendError::validation("error.rfi.followUp.backupDirectoryInvalid"))?;
        let mut roots = vec![
            (source_root, GrantAccess::Read),
            (target_root, GrantAccess::ReadWrite),
        ];
        if let Some(directory) = &backup_directory {
            if !directory.is_dir() {
                return Err(BackendError::validation(
                    "error.rfi.followUp.backupDirectoryInvalid",
                ));
            }
            roots.push((directory.clone(), GrantAccess::ReadWrite));
        }
        let provisional = self.filesystem.issue_provisional_grants(&scope, &roots)?;
        let source_grant = &provisional.grants()[0];
        let target_grant = &provisional.grants()[1];
        let source_anchor = self.filesystem.anchor_granted_path(
            source_grant,
            &request.values.rfi_log_workbook,
            PathIntent::Read,
        )?;
        let target_anchor = self.filesystem.anchor_granted_path(
            target_grant,
            &request.values.follow_up_workbook,
            PathIntent::Write,
        )?;
        if source_anchor.same_file_as(&target_anchor).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.followUp.workbookInvalid: file check failed: {e}"
            ))
        })? {
            return Err(BackendError::validation(
                "error.rfi.followUp.workbookInvalid: RFI Log and Follow Up workbooks must be different files (the filesystem may report the same file identity for both)",
            ));
        }
        let (exceptions, shared_available) =
            self.resolve_follow_up_exceptions(request.values.exception_rfis.as_deref())?;
        let mut source = source_anchor.open_read().map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.followUp.workbookInvalid: cannot open RFI Log: {e}"
            ))
        })?;
        let mut target = target_anchor
            .open_mutable()
            .map_err(|error| open_failure("error.rfi.followUp.workbookLocked", &error))?;
        progress(ProgressEvent {
            phase: "follow_up_analysis".to_string(),
            done: 0,
            total: 1,
            subject: request
                .values
                .follow_up_workbook
                .to_string_lossy()
                .into_owned(),
        });
        let mut plan = rfi_follow_up::preview_from_open_files_with_exceptions(
            &request.values.rfi_log_workbook,
            &request.values.follow_up_workbook,
            backup_directory.as_deref(),
            &exceptions,
            &mut source,
            &mut target,
        )?;
        if !shared_available {
            plan.warnings.push(Warning {
                key: rfi_follow_up::SHARED_EXCEPTIONS_UNAVAILABLE_WARN.to_string(),
                subject: String::new(),
            });
        }
        let reservation = self.filesystem.reserve_plan_id();
        reservation.assign_to_empty(&mut plan)?;
        rfi_follow_up::validate_owned_plan_budget(&plan)?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_reserved_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                grant_ids,
            ),
            reservation,
            plan,
        )?;
        provisional.commit();
        progress(ProgressEvent {
            phase: "preview_ready".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(cached)
    }

    pub(crate) fn preview_stage_signed_pages<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<StageSignedPagesValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != "rfi.stage_signed_first_pages" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust signed-page backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let signed_grant =
            self.filesystem
                .issue_grant(&scope, &request.values.signed_scans, GrantAccess::Read)?;
        let archive_grant =
            self.filesystem
                .issue_grant(&scope, &request.values.archive_root, GrantAccess::Read)?;
        let review_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.review_root,
            GrantAccess::ReadWrite,
        )?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let native_ocr = NativeOcr::discover();
        let ocr = CachingOcr::new(
            &native_ocr,
            &self.scan_index,
            &caller.owner_id,
            &request.values.signed_scans,
        );
        let mut plan = preview_stage_signed_pages(
            &StageSignedPagesParams {
                signed_scans: request.values.signed_scans,
                archive_root: request.values.archive_root,
                review_root: request.values.review_root,
                scan_subfolders: request.values.scan_subfolders,
            },
            &ocr,
            progress,
            || cancellation.is_cancelled(),
        )?;
        ocr.attach_stats(&mut plan);
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![
                    signed_grant.grant_id,
                    archive_grant.grant_id,
                    review_grant.grant_id,
                ],
            ),
            plan,
        )?)
    }

    pub(crate) fn preview_pdf_orientation<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<PdfOrientationValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != "rfi.normalize_pdf_orientation" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust PDF orientation backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let _source_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.rfi_system_root,
            GrantAccess::Read,
        )?;
        let _review_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.review_root,
            GrantAccess::ReadWrite,
        )?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let plan = preview_pdf_orientation(
            &PdfOrientationParams {
                rfi_system_root: request.values.rfi_system_root.clone(),
                review_root: request.values.review_root.clone(),
            },
            &NativeOcr::discover(),
            progress,
            || cancellation.is_cancelled(),
            Some(&self.scan_index),
            &caller.owner_id,
            request.values.force_rescan,
        )?;
        // Large RFI trees can take longer than the grant TTL to inspect. Bind the
        // finished plan to fresh grants for the exact same selected roots; Apply
        // still revalidates the approved plan and filesystem identities.
        let source_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.rfi_system_root,
            GrantAccess::Read,
        )?;
        let review_grant = self.filesystem.issue_grant(
            &scope,
            &request.values.review_root,
            GrantAccess::ReadWrite,
        )?;
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![source_grant.grant_id, review_grant.grant_id],
            ),
            plan,
        )?)
    }
}
