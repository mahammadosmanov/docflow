use super::*;

impl NativeBackend {
    pub(crate) fn find_rfi_print_files(&self, params: Value) -> Result<Value, BackendError> {
        let request: RfiPrintFindRequest = decode(params)?;
        let caller = self.caller_identity()?;
        let root = self.selections.resolve(
            &request.root,
            &caller,
            "files.batch_print",
            "rfi_system_root",
            SelectionKind::Folder,
        )?;
        let canonical_root = fs::canonicalize(root)
            .map_err(|_| BackendError::validation("error.print.rfi.changed"))?;
        let discovery = rfi_print_find::discover(
            &canonical_root,
            &request.numbers,
            IncludeTypes {
                first_page: request.first_page,
                sketch: request.sketch,
            },
        )
        .map_err(|_| BackendError::validation("error.print.rfi.invalidRequest"))?;
        let mut candidates = Vec::with_capacity(discovery.candidates.len());
        for candidate in discovery.candidates {
            let mut selection = self
                .selections
                .issue(
                    &caller,
                    "files.batch_print",
                    "files",
                    &candidate.path,
                    SelectionKind::File,
                )
                .map_err(|_| BackendError::validation("error.print.rfi.changed"))?;
            let issued_path = resolve_scoped_rfi_print_file(
                &self.selections,
                &selection,
                &caller,
                &canonical_root,
            )?;
            let resolution = discovery
                .resolutions
                .iter()
                .find(|resolution| resolution.requested == candidate.requested)
                .ok_or_else(BackendError::invalid_params)?;
            let selected =
                !discovery.truncated && rfi_print_find::initially_selected(resolution, &candidate);
            let display_path = issued_path
                .strip_prefix(&canonical_root)
                .unwrap_or(&issued_path)
                .to_string_lossy()
                .into_owned();
            selection.display_path = display_path.clone();
            candidates.push(RfiPrintCandidate {
                requested: candidate.requested,
                rfi_code: candidate.rfi_code,
                document_type: match candidate.document_type {
                    RfiPrintDocumentType::FirstPage => "firstPage",
                    RfiPrintDocumentType::Sketch => "sketch",
                }
                .to_string(),
                display_path,
                selection,
                selected,
            });
        }
        let resolutions = discovery
            .resolutions
            .into_iter()
            .map(|resolution| RfiPrintResolution {
                requested: resolution.requested,
                rfi_codes: resolution.rfi_codes,
                missing_first_page: resolution.missing_first_page,
                missing_sketch: resolution.missing_sketch,
                ambiguous: resolution.ambiguous,
            })
            .collect::<Vec<_>>();
        encode(RfiPrintFindResult {
            candidates,
            resolutions,
            visited: discovery.visited,
            truncated: discovery.truncated,
        })
    }

    pub(crate) fn preview_batch_print<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<BatchPrintParams> = decode(self.materialize_preview(params)?)?;
        if request.op_id != "files.batch_print" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust print backend",
            ));
        }
        encode(
            self.print
                .preview(&self.caller_identity()?, request.values, progress)?,
        )
    }
    #[allow(dead_code)]
    pub(crate) fn preview_replica_publish<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<ReplicaPublishParams> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != replica_publish::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust replica publication backend",
            ));
        }
        let caller = self.caller_identity()?;
        encode(self.cache_replica_plan(request.values, &caller, false, progress)?)
    }
    #[allow(dead_code)]
    pub(super) fn cache_replica_plan<F>(
        &self,
        values: ReplicaPublishParams,
        caller: &CallerIdentity,
        automatic: bool,
        progress: F,
    ) -> Result<crate::contracts::Plan, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let roots = [
            fs::canonicalize(&values.root_a),
            fs::canonicalize(&values.root_b),
            fs::canonicalize(&values.root_c),
        ];
        let roots = roots
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| BackendError::path_invalid("error.replica.rootInvalid"))?;
        let backup = fs::canonicalize(&values.backup_root)
            .map_err(|_| BackendError::path_invalid("error.replica.backupInvalid"))?;
        let source_index = match values.source_root.as_str() {
            "a" => 0,
            "b" => 1,
            "c" => 2,
            _ => return Err(BackendError::validation("error.replica.sourceInvalid")),
        };
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            replica_publish::OPERATION_ID,
        );
        let mut grants = roots
            .iter()
            .enumerate()
            .map(|(index, root)| {
                (
                    root.clone(),
                    if index == source_index {
                        GrantAccess::Read
                    } else {
                        GrantAccess::ReadWrite
                    },
                )
            })
            .collect::<Vec<_>>();
        grants.push((backup, GrantAccess::ReadWrite));
        let provisional = self.filesystem.issue_provisional_grants(&scope, &grants)?;
        let plan = if automatic {
            replica_publish::preview_automatic_with_index(
                &values,
                &caller.owner_id,
                &self.scan_index,
                progress,
            )?
        } else {
            replica_publish::preview_with_index(
                &values,
                &caller.owner_id,
                &self.scan_index,
                progress,
            )?
        };
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                replica_publish::OPERATION_ID,
                grant_ids,
            ),
            plan,
        )?;
        provisional.commit();
        Ok(cached)
    }

    pub(crate) fn revise_native_plan(&self, params: Value) -> Result<Value, BackendError> {
        let request: ReviseParams = decode(params)?;
        let caller = self.caller_identity()?;
        if self.print.contains_plan(&request.plan_id) {
            return encode(
                self.print
                    .revise(&caller, &request.plan_id, &request.rows)?,
            );
        }
        let claimed = self
            .filesystem
            .resolve_native_plan(&request.plan_id, &caller)?;
        let binding = claimed.binding.ok_or_else(BackendError::plan_unavailable)?;
        if claimed.plan.op_id == rfi_follow_up::OPERATION_ID {
            let (source_path, target_path) = rfi_follow_up::plan_workbook_paths(&claimed.plan)?;
            let source_anchor = self.filesystem.anchor_bound_path(
                &binding,
                &caller,
                std::path::Path::new(&source_path),
                PathIntent::Read,
            )?;
            let target_anchor = self.filesystem.anchor_bound_path(
                &binding,
                &caller,
                std::path::Path::new(&target_path),
                PathIntent::Write,
            )?;
            let mut source = source_anchor
                .open_read()
                .map_err(|_| BackendError::validation("error.rfi.followUp.workbookInvalid"))?;
            let mut target = target_anchor
                .open_mutable()
                .map_err(|_| BackendError::validation("error.rfi.followUp.workbookLocked"))?;
            let revised = rfi_follow_up::revise_from_open_files(
                &claimed.plan,
                &request.rows,
                &mut source,
                &mut target,
            )?;
            return encode(self.filesystem.replace_cached_plan(
                &binding,
                &caller,
                &request.plan_id,
                &claimed.digest,
                revised,
            )?);
        }
        let payload = json!({"rows": request.rows});
        let revised = match claimed.plan.op_id.as_str() {
            "rfi.log_add" => rfi_log::revise(&claimed.plan, &payload)?,
            rfi_file_group::OPERATION_ID => rfi_file_group::revise(&claimed.plan, &payload)?,
            FAST_COVER_OPERATION_ID => {
                crate::operations::process_scans::revise(&claimed.plan, &payload)?
            }
            rfi_system_sync::OPERATION_ID => rfi_system_sync::revise(
                &claimed.plan,
                payload["rows"]
                    .as_array()
                    .ok_or_else(BackendError::invalid_params)?,
            )?,
            rfi_system_sync::COMPLETE_STATUS_OPERATION_ID => rfi_system_sync::revise(
                &claimed.plan,
                payload["rows"]
                    .as_array()
                    .ok_or_else(BackendError::invalid_params)?,
            )?,
            "rfi.stage_signed_first_pages" => revise_stage_signed_pages(&claimed.plan, &payload)?,
            _ => {
                return Err(BackendError::validation(
                    "native plan does not support revision",
                ));
            }
        };
        encode(
            self.filesystem
                .cache_plan(BackendOrigin::Rust, binding, revised)?,
        )
    }

    pub(crate) fn copy_native_table(&self, params: Value) -> Result<Value, BackendError> {
        let request: TableParams = decode(params)?;
        let claimed = self
            .filesystem
            .resolve_native_plan(&request.plan_id, &self.caller_identity()?)?;
        if claimed.plan.op_id != "rfi.log_add" {
            return Err(BackendError::validation(
                "native plan does not contain an RFI table",
            ));
        }
        let payload = rfi_log::copy_payload(&claimed.plan, request.sheet.as_deref())?;
        clipboard::write(&payload)?;
        Ok(json!({"count": payload.count}))
    }

    pub(crate) fn preview_rfi_reply(&self, params: Value) -> Result<Value, BackendError> {
        let request: RfiReplyPreviewParams = decode(params)?;
        let caller = self.caller_identity()?;
        let claimed = self
            .filesystem
            .resolve_native_plan(&request.plan_id, &caller)?;
        if claimed.plan.op_id != "rfi.log_add" {
            return Err(BackendError::validation(
                "native plan does not contain an RFI table",
            ));
        }
        let journal = self.filesystem.journals().read(&request.journal_id)?;
        if !committed_rfi_reply_matches(&journal, &caller, &claimed.digest) {
            return Err(BackendError::validation(
                "error.rfi.outlookDraft.rfiLogNotCommitted",
            ));
        }
        let payload = rfi_log::reply_payload(&claimed.plan)?;
        let content = outlook_reply_draft::ReplyContent {
            plain_text: payload.plain,
            html: payload.html,
        };
        encode(self.outlook.preview_content(&caller, content)?)
    }

    pub(crate) fn apply_native_plan<F>(
        &self,
        plan_id: &str,
        operation_id: Option<&str>,
        mut progress: F,
    ) -> Result<(Value, String), BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let caller = self.caller_identity()?;
        if self.outlook.contains_plan(plan_id) {
            let result = self
                .outlook
                .apply(plan_id, operation_id, &caller, &mut progress)?;
            return Ok((
                encode(result)?,
                outlook_reply_draft::OPERATION_ID.to_string(),
            ));
        }
        if self.print.contains_plan(plan_id) {
            if operation_id.is_some_and(|operation_id| operation_id != "files.batch_print") {
                return Err(BackendError::validation(
                    "apply operation does not match the approved plan",
                ));
            }
            return Ok((
                encode(self.print.apply(plan_id, &caller, &mut progress)?)?,
                "files.batch_print".to_string(),
            ));
        }
        if self
            .filesystem
            .validate_plan_operation(plan_id, &caller, operation_id)?
            == outlook_reply_draft::ARCHIVE_OPERATION_ID
        {
            let result = self.filesystem.apply_generated_binary_write(
                plan_id,
                &caller,
                outlook_reply_draft::ARCHIVE_OPERATION_ID,
                &mut progress,
                || {
                    self.outlook
                        .export_archive(plan_id, &caller)
                        .map_err(|failure| match failure.kind {
                            OutlookFailureKind::OutcomeUnknown => {
                                GeneratedWriteFailure::OutcomeUnknown(failure.key)
                            }
                            OutlookFailureKind::Unsupported | OutlookFailureKind::NoEffect => {
                                GeneratedWriteFailure::NoEffect(failure.key)
                            }
                        })
                },
            )?;
            return Ok((
                encode(result)?,
                outlook_reply_draft::ARCHIVE_OPERATION_ID.to_string(),
            ));
        }
        let approved_op_id =
            self.filesystem
                .validate_plan_operation(plan_id, &caller, operation_id)?;
        let automation_profile = if approved_op_id == replica_publish::OPERATION_ID {
            self.filesystem
                .resolve_native_plan(plan_id, &caller)?
                .plan
                .meta
                .get("automationProfile")
                .cloned()
        } else {
            None
        };
        let mut result = self
            .filesystem
            .apply_with_progress(plan_id, &caller, &mut progress)?;
        if result.failed == 0
            && let Some(profile) = automation_profile
        {
            let mut section = JsonObject::new();
            section.insert("profile".to_string(), profile);
            section.insert("lastAttemptUnix".to_string(), Value::from(0));
            let activated = self
                .settings
                .save_section("replicaAutomation", Some(section));
            if activated.is_ok() {
                let mut preferences = self.settings.preferences();
                preferences.replica_sync_enabled = true;
                if self.settings.save_preferences(preferences).is_err() {
                    result.failed += 1;
                    result.errors.push(crate::contracts::Warning {
                        key: "error.replica.activationFailed".to_string(),
                        subject: String::new(),
                    });
                }
            } else {
                result.failed += 1;
                result.errors.push(crate::contracts::Warning {
                    key: "error.replica.activationFailed".to_string(),
                    subject: String::new(),
                });
            }
        }
        Ok((encode(result)?, approved_op_id))
    }

    pub(crate) fn cancel_native_plan(&self, plan_id: &str) -> bool {
        self.outlook.cancel(plan_id)
            || self.print.cancel(plan_id)
            || self.filesystem.cancel(plan_id)
    }

    pub(crate) fn cancel_native_operation(&self, op_id: &str) -> bool {
        self.filesystem.cancel(op_id)
    }

    pub(super) fn review_scan(&self, params: Value) -> Result<Value, BackendError> {
        let request: ReviewScanParams = decode(params)?;
        let caller = self.caller_identity()?;
        let destination = self.selections.resolve_scope(
            &request.destination.selection_id,
            &caller,
            SelectionScope::ReviewDestination,
        )?;
        let items = scan_review(&destination, &NativeOcr::discover())?
            .into_iter()
            .map(|item| {
                let selection = self.selections.issue(
                    &caller,
                    "rfi.review",
                    "item",
                    std::path::Path::new(&item.path),
                    SelectionKind::File,
                )?;
                Ok(FrontendReviewItem {
                    selection_id: selection.selection_id,
                    name: item.name,
                    path: item.path,
                    code: item.code,
                    confidence: item.confidence,
                    target_exists: item.target_exists,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        encode(items)
    }

    pub(super) fn review_preview(&self, params: Value) -> Result<Value, BackendError> {
        let request: ReviewPreviewParams = decode(params)?;
        let caller = self.caller_identity()?;
        let destination = self.selections.resolve_scope(
            &request.destination.selection_id,
            &caller,
            SelectionScope::ReviewDestination,
        )?;
        let decisions = request
            .decisions
            .into_iter()
            .map(|decision| {
                Ok(NativeReviewDecision {
                    path: self.selections.resolve_scope(
                        &decision.item_id,
                        &caller,
                        SelectionScope::ReviewItem,
                    )?,
                    action: decision.action,
                    code: decision.code,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            "rfi.review",
        );
        let destination_grant =
            self.filesystem
                .issue_grant(&scope, &destination, GrantAccess::ReadWrite)?;
        let app_data_grant =
            self.filesystem
                .issue_grant(&scope, &self.app_data_dir, GrantAccess::ReadWrite)?;
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%6f").to_string();
        let plan = preview_review(
            &destination,
            &self.app_data_dir.join("trash"),
            &decisions,
            &timestamp,
        )?;
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                "rfi.review",
                vec![destination_grant.grant_id, app_data_grant.grant_id],
            ),
            plan,
        )?)
    }
}
