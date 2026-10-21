use super::*;

impl PrintService {
    pub(super) fn source_identities_for_plan(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        jobs: &[PrintJob],
        all_root_identities: &[GrantRootIdentity],
    ) -> Result<(Vec<GrantRootIdentity>, Vec<QueueSourceIdentity>), BackendError> {
        let mut seen = HashSet::new();
        let mut sources = Vec::new();
        for job in jobs {
            if !seen.insert(job.path.clone()) {
                continue;
            }
            let canonical_path = self.grants.authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                Path::new(&job.path),
                PathIntent::Read,
            )?;
            let identity = self
                .grants
                .authorize_capability_for_plan(
                    &binding.grant_ids,
                    caller,
                    &binding.operation_id,
                    &canonical_path,
                    PathIntent::Read,
                )?
                .anchor()
                .and_then(|source| source.identity())
                .map_err(|_| BackendError::path_invalid("print file identity is unavailable"))?;
            sources.push(QueueSourceIdentity {
                path: job.path.clone(),
                canonical_path: canonical_path.to_string_lossy().into_owned(),
                identity,
            });
        }
        let root_paths = sources
            .iter()
            .filter_map(|source| Path::new(&source.canonical_path).parent())
            .map(|parent| parent.to_string_lossy().into_owned())
            .collect::<HashSet<_>>();
        let roots = all_root_identities
            .iter()
            .filter(|root| root_paths.contains(&root.root))
            .cloned()
            .collect::<Vec<_>>();
        if roots.len() != root_paths.len() || sources.is_empty() {
            return Err(BackendError::path_invalid(
                "print file capability identity is unavailable",
            ));
        }
        Ok((roots, sources))
    }

    pub(super) fn revalidate_worksheet_plan(
        &self,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        plan: &Plan,
    ) -> Result<(), BackendError> {
        let rows = plan
            .meta
            .get("editor")
            .and_then(Value::as_object)
            .and_then(|editor| editor.get("rows"))
            .and_then(Value::as_array)
            .ok_or_else(|| BackendError::validation("print plan has no rows"))?;
        let mut discoveries = HashMap::<String, (FileSignature, OrderedWorkbookDiscovery)>::new();
        for row in rows {
            let Some(worksheet_value) = row.get("worksheet") else {
                continue;
            };
            let path = row
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| BackendError::validation("invalid worksheet print row"))?;
            let expected_signature: FileSignature =
                serde_json::from_value(row.get("signature").cloned().unwrap_or(Value::Null))
                    .map_err(|_| BackendError::validation("invalid worksheet file signature"))?;
            let expected_worksheet: WorksheetDescriptor =
                serde_json::from_value(worksheet_value.clone())
                    .map_err(|_| BackendError::validation("invalid worksheet descriptor"))?;
            let expected_safety: WorkbookSafetyFacts =
                serde_json::from_value(row.get("workbookSafety").cloned().unwrap_or(Value::Null))
                    .map_err(|_| BackendError::validation("invalid workbook safety facts"))?;
            if !discoveries.contains_key(path) {
                let canonical_path = self.grants.authorize_for_plan(
                    &binding.grant_ids,
                    caller,
                    &binding.operation_id,
                    Path::new(path),
                    PathIntent::Read,
                )?;
                let source = self
                    .grants
                    .authorize_capability_for_plan(
                        &binding.grant_ids,
                        caller,
                        &binding.operation_id,
                        &canonical_path,
                        PathIntent::Read,
                    )?
                    .anchor()
                    .map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
                let mut source_file = source
                    .open_read()
                    .map_err(|_| BackendError::path_invalid("print file is unavailable"))?;
                let actual_signature = file_signature_from_open_file(&mut source_file, true)?;
                if actual_signature != expected_signature {
                    return Err(BackendError::validation("error.print.workbookChanged"));
                }
                let discovery = ordered_workbook_discovery(&mut source_file)
                    .map_err(|_| BackendError::validation("error.print.worksheetChanged"))?;
                discoveries.insert(path.to_string(), (actual_signature, discovery));
            }
            let (actual_signature, discovery) = discoveries
                .get(path)
                .ok_or_else(|| BackendError::validation("error.print.worksheetChanged"))?;
            if actual_signature != &expected_signature {
                return Err(BackendError::validation("error.print.workbookChanged"));
            }
            if discovery.safety != expected_safety
                || workbook_safety_is_unsafe(&discovery.safety)
                || !discovery.worksheets.contains(&expected_worksheet)
            {
                return Err(BackendError::validation("error.print.worksheetChanged"));
            }
        }
        Ok(())
    }

    pub(super) fn submit_approved_job(
        &self,
        job: &PrintJob,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        cancellation: CancellationToken,
    ) -> Result<PrintSubmissionOutcome, PrintFailure> {
        let canonical_path = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                Path::new(&job.path),
                PathIntent::Read,
            )
            .map_err(|error| PrintFailure::failed(error.message))?;
        let source = self
            .grants
            .authorize_capability_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &canonical_path,
                PathIntent::Read,
            )
            .map_err(|error| PrintFailure::failed(error.message))?
            .anchor()
            .map_err(|_| PrintFailure::failed("print file is unavailable"))?;
        let mut source_file = source
            .open_read()
            .map_err(|_| PrintFailure::failed("print file is unavailable"))?;
        let metadata = source_file
            .metadata()
            .map_err(|_| PrintFailure::failed("print file is unavailable"))?;
        if !metadata.is_file() {
            return Err(PrintFailure::failed("print file is unavailable"));
        }
        if !job.signature.is_legacy_empty()
            && file_signature_from_open_file(&mut source_file, job.worksheet.is_some())
                .map_err(|error| PrintFailure::failed(error.message))?
                != job.signature
        {
            return Err(PrintFailure::failed(if job.worksheet.is_some() {
                "error.print.workbookChanged"
            } else {
                "file changed after the queue was previewed"
            }));
        }
        if let Some(worksheet) = job.worksheet.as_ref() {
            let discovery = ordered_workbook_discovery(&mut source_file)
                .map_err(|_| PrintFailure::failed("error.print.worksheetChanged"))?;
            if job.workbook_safety.as_ref() != Some(&discovery.safety)
                || workbook_safety_is_unsafe(&discovery.safety)
                || !discovery.worksheets.contains(worksheet)
            {
                return Err(PrintFailure::failed("error.print.worksheetChanged"));
            }
            #[cfg(any(target_os = "windows", test))]
            {
                let pipeline = self
                    .excel_pipeline
                    .as_ref()
                    .ok_or_else(|| PrintFailure::unsupported("error.print.excelUnavailable"))?;
                return pipeline.export_and_submit(job, &source, &mut source_file, cancellation);
            }
            #[cfg(not(any(target_os = "windows", test)))]
            return Err(PrintFailure::unsupported("error.print.excelUnavailable"));
        }
        let workspace = tempfile::tempdir()
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        let workspace_capability = crate::cap_fs::CapabilityRoot::open(workspace.path())
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        let source_name = Path::new(&job.path)
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| PrintFailure::failed("print file name is unavailable"))?;
        let staged_path = workspace.path().join(source_name);
        let staged = workspace_capability
            .authorize(&staged_path)
            .and_then(|path| path.anchor())
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        staged
            .copy_no_clobber_from_file(&mut source_file)
            .map_err(|_| PrintFailure::failed("private print staging failed"))?;
        let inventory = self.adapter.inventory().map(PrinterInventory::normalized)?;
        if !inventory.contains(&job.printer) {
            return Err(PrintFailure::failed("printer is no longer available"));
        }
        let support = self
            .adapter
            .inspect(&staged_path, &job.printer, &job.settings())?;
        if support.verb != job.verb {
            return Err(PrintFailure::failed(
                "print capability changed after preview",
            ));
        }
        let canonical = canonical_for(&job.page_range, &support.page_range)
            .map_err(|error| PrintFailure::failed(error.message))?;
        if canonical != job.page_range {
            return Err(PrintFailure::failed("error.print.pageRangeInvalid"));
        }
        let mut staged_job = job.clone();
        staged_job.path = staged_path.to_string_lossy().into_owned();
        self.adapter
            .submit(&staged_job, &|| cancellation.is_cancelled())
            .map(|()| PrintSubmissionOutcome::default())
    }
}
