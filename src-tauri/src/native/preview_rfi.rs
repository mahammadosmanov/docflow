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
        let mut plan = rfi_follow_up::preview_from_open_files_with_backup(
            &request.values.rfi_log_workbook,
            &request.values.follow_up_workbook,
            backup_directory.as_deref(),
            &mut source,
            &mut target,
        )?;
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

    pub(crate) fn preview_rfi_system_sync<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiSystemSyncPreviewValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_system_sync::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI System Sync backend",
            ));
        }
        let system_root = fs::canonicalize(&request.values.rfi_system_root).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.systemRootInvalid: cannot resolve root: {e}"
            ))
        })?;
        let source_path = fs::canonicalize(&request.values.rfi_log_workbook).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.workbookInvalid: cannot resolve RFI Log: {e}"
            ))
        })?;
        let target_path = fs::canonicalize(&request.values.follow_up_workbook).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.workbookInvalid: cannot resolve Follow Up: {e}"
            ))
        })?;
        let source_root = source_path
            .parent()
            .ok_or_else(|| BackendError::path_invalid("RFI Log workbook has no parent"))?
            .to_path_buf();
        let target_root = target_path
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

        let system_needs_write =
            source_path.starts_with(&system_root) || target_path.starts_with(&system_root);
        let system_access = if system_needs_write {
            GrantAccess::ReadWrite
        } else {
            GrantAccess::Read
        };

        let mut grant_requests: Vec<(PathBuf, GrantAccess)> =
            vec![(system_root.clone(), system_access)];

        let source_covered = system_needs_write && source_path.starts_with(&system_root);
        if !source_covered {
            grant_requests.push((source_root.clone(), GrantAccess::ReadWrite));
        }

        let target_covered = (system_needs_write && target_path.starts_with(&system_root))
            || (!source_covered && target_path.starts_with(&source_root));
        if !target_covered {
            grant_requests.push((target_root.clone(), GrantAccess::ReadWrite));
        }

        let provisional = self
            .filesystem
            .issue_provisional_grants(&scope, &grant_requests)?;

        let source_grant = provisional
            .grants()
            .iter()
            .find(|g| g.access.allows(PathIntent::Write) && source_path.starts_with(&g.root))
            .ok_or_else(|| BackendError::path_invalid("No write grant covering RFI Log"))?;
        let target_grant = provisional
            .grants()
            .iter()
            .find(|g| g.access.allows(PathIntent::Write) && target_path.starts_with(&g.root))
            .ok_or_else(|| BackendError::path_invalid("No write grant covering Follow Up"))?;

        let source_anchor =
            self.filesystem
                .anchor_granted_path(source_grant, &source_path, PathIntent::Write)?;
        let target_anchor =
            self.filesystem
                .anchor_granted_path(target_grant, &target_path, PathIntent::Write)?;
        if source_anchor.same_file_as(&target_anchor).map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.workbookInvalid: file check failed: {e}"
            ))
        })? {
            return Err(BackendError::validation(
                "error.rfi.followUp.workbookInvalid: RFI Log and Follow Up workbooks must be different files (the filesystem may report the same file identity for both)",
            ));
        }
        let mut source = source_anchor.open_mutable().map_err(|e| {
            BackendError::validation(format!(
                "error.rfi.systemSync.workbookLocked: cannot open RFI Log: {e}"
            ))
        })?;
        let mut target = target_anchor
            .open_mutable()
            .map_err(|_| BackendError::validation("error.rfi.systemSync.workbookLocked"))?;
        let mut plan = rfi_system_sync::preview_system_sync_indexed(
            &system_root,
            &source_path,
            &target_path,
            &mut source,
            &mut target,
            rfi_system_sync::SystemSyncScan {
                index: Some(&self.scan_index),
                owner_id: &caller.owner_id,
                force_rescan: request.values.force_rescan,
            },
            &mut progress,
        )?;
        drop(source);
        drop(target);
        let reservation = self.filesystem.reserve_plan_id();
        reservation.assign_to_empty(&mut plan)?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_reserved_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                caller.owner_id.clone(),
                caller.session_id.clone(),
                request.op_id,
                grant_ids,
            ),
            reservation,
            plan,
        )?;
        provisional.commit();
        encode(cached)
    }

    pub(crate) fn preview_rfi_complete_status_sync<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiCompleteStatusPreviewValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_system_sync::COMPLETE_STATUS_OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI completion-status backend",
            ));
        }
        let system_root = fs::canonicalize(&request.values.rfi_system_root).map_err(|error| {
            BackendError::validation(format!(
                "error.rfi.completeStatus.systemRootInvalid: cannot resolve root: {error}"
            ))
        })?;
        let workbook_path =
            fs::canonicalize(&request.values.rfi_log_workbook).map_err(|error| {
                BackendError::validation(format!(
                    "error.rfi.completeStatus.workbookInvalid: cannot resolve RFI Log: {error}"
                ))
            })?;
        let workbook_root = workbook_path
            .parent()
            .ok_or_else(|| BackendError::path_invalid("RFI Log workbook has no parent"))?
            .to_path_buf();
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let workbook_in_system = workbook_path.starts_with(&system_root);
        let mut grant_requests = vec![(
            system_root.clone(),
            if workbook_in_system {
                GrantAccess::ReadWrite
            } else {
                GrantAccess::Read
            },
        )];
        if !workbook_in_system {
            grant_requests.push((workbook_root, GrantAccess::ReadWrite));
        }
        let provisional = self
            .filesystem
            .issue_provisional_grants(&scope, &grant_requests)?;
        let workbook_grant = provisional
            .grants()
            .iter()
            .find(|grant| {
                grant.access.allows(PathIntent::Write) && workbook_path.starts_with(&grant.root)
            })
            .ok_or_else(|| BackendError::path_invalid("No write grant covering RFI Log"))?;
        let workbook_anchor = self.filesystem.anchor_granted_path(
            workbook_grant,
            &workbook_path,
            PathIntent::Write,
        )?;
        let mut workbook = workbook_anchor
            .open_mutable()
            .map_err(|_| BackendError::validation("error.rfi.completeStatus.workbookLocked"))?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let mut plan = rfi_system_sync::preview_complete_status_sync(
            &system_root,
            &workbook_path,
            &mut workbook,
            progress,
            || cancellation.is_cancelled(),
        )?;
        drop(workbook);
        let reservation = self.filesystem.reserve_plan_id();
        reservation.assign_to_empty(&mut plan)?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_reserved_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                caller.owner_id,
                caller.session_id,
                request.op_id.clone(),
                grant_ids,
            ),
            reservation,
            plan,
        )?;
        provisional.commit();
        encode(cached)
    }
    pub(crate) fn preview_rfi_publish_status<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiPublishStatusPreviewValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_system_sync::PUBLISH_STATUS_OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI publish-status backend",
            ));
        }
        let staging_root = fs::canonicalize(&request.values.staging_folder).map_err(|error| {
            BackendError::validation(format!(
                "error.rfi.publishStatus.stagingInvalid: cannot resolve root: {error}"
            ))
        })?;
        let workbook_path =
            fs::canonicalize(&request.values.rfi_log_workbook).map_err(|error| {
                BackendError::validation(format!(
                    "error.rfi.publishStatus.workbookInvalid: cannot resolve RFI Log: {error}"
                ))
            })?;
        let workbook_root = workbook_path
            .parent()
            .ok_or_else(|| BackendError::path_invalid("RFI Log workbook has no parent"))?
            .to_path_buf();
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let workbook_in_staging = workbook_path.starts_with(&staging_root);
        let mut grant_requests = vec![(
            staging_root.clone(),
            if workbook_in_staging {
                GrantAccess::ReadWrite
            } else {
                GrantAccess::Read
            },
        )];
        if !workbook_in_staging {
            grant_requests.push((workbook_root, GrantAccess::ReadWrite));
        }
        let provisional = self
            .filesystem
            .issue_provisional_grants(&scope, &grant_requests)?;
        let workbook_grant = provisional
            .grants()
            .iter()
            .find(|grant| {
                grant.access.allows(PathIntent::Write) && workbook_path.starts_with(&grant.root)
            })
            .ok_or_else(|| BackendError::path_invalid("No write grant covering RFI Log"))?;
        let workbook_anchor = self.filesystem.anchor_granted_path(
            workbook_grant,
            &workbook_path,
            PathIntent::Write,
        )?;
        let mut workbook = workbook_anchor
            .open_mutable()
            .map_err(|_| BackendError::validation("error.rfi.publishStatus.workbookLocked"))?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let mut plan = rfi_system_sync::preview_publish_status(
            &staging_root,
            &workbook_path,
            &mut workbook,
            progress,
            || cancellation.is_cancelled(),
        )?;
        drop(workbook);
        let reservation = self.filesystem.reserve_plan_id();
        reservation.assign_to_empty(&mut plan)?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_reserved_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                caller.owner_id,
                caller.session_id,
                request.op_id.clone(),
                grant_ids,
            ),
            reservation,
            plan,
        )?;
        provisional.commit();
        encode(cached)
    }
    #[allow(dead_code)]
    pub(crate) fn preview_outlook_reply_draft<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<OutlookReplyDraftValues> = decode(params)?;
        if request.op_id != outlook_reply_draft::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust Outlook draft backend",
            ));
        }
        progress(ProgressEvent {
            phase: "outlook_selection".to_string(),
            done: 0,
            total: 1,
            subject: String::new(),
        });
        let plan = self
            .outlook
            .preview(&self.caller_identity()?, &request.values.reply_text)?;
        progress(ProgressEvent {
            phase: "preview_ready".to_string(),
            done: 1,
            total: 1,
            subject: String::new(),
        });
        encode(plan)
    }
    #[allow(dead_code)]
    pub(crate) fn preview_outlook_archive<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<OutlookArchiveValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != outlook_reply_draft::ARCHIVE_OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust Outlook archive backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let provisional = self.filesystem.issue_provisional_grants(
            &scope,
            &[(request.values.destination.clone(), GrantAccess::Write)],
        )?;
        progress(ProgressEvent {
            phase: "outlook_selection".to_string(),
            done: 0,
            total: 1,
            subject: String::new(),
        });
        let prepared = self
            .outlook
            .prepare_archive(&caller, &request.values.destination)?;
        let mut plan = prepared.plan.clone();
        let reservation = self.filesystem.reserve_plan_id();
        reservation.assign_to_empty(&mut plan)?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_reserved_plan(
            BackendOrigin::Rust,
            PlanBinding::new(caller.owner_id, caller.session_id, request.op_id, grant_ids),
            reservation,
            plan,
        )?;
        self.outlook.bind_archive(&cached.plan_id, prepared);
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
        let plan = preview_stage_signed_pages(
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
