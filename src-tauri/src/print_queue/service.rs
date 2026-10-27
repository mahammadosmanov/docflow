use super::*;

pub struct PrintService {
    pub(in crate::print_queue) queues: PrintQueueStore,
    pub(in crate::print_queue) journals: JournalStore,
    pub(in crate::print_queue) plans: PlanStore,
    pub(in crate::print_queue) grants: PathGrantStore,
    pub(in crate::print_queue) mutation_lock: GlobalMutationLock,
    pub(in crate::print_queue) cancellations: CancellationRegistry,
    pub(in crate::print_queue) adapter: Arc<dyn PrintAdapter>,
    #[cfg(any(target_os = "windows", test))]
    pub(in crate::print_queue) excel_pipeline:
        Option<Arc<crate::platform::excel::ExcelExportPipeline>>,
    #[cfg(test)]
    pub(in crate::print_queue) journal_write_fail_on: Option<usize>,
    #[cfg(test)]
    pub(in crate::print_queue) journal_write_count: AtomicUsize,
}

impl PrintService {
    pub fn new(
        queue_root: impl AsRef<Path>,
        journal_root: impl AsRef<Path>,
        ttl: Duration,
        mutation_lock: GlobalMutationLock,
        adapter: Arc<dyn PrintAdapter>,
    ) -> Self {
        Self {
            queues: PrintQueueStore::new(queue_root),
            journals: JournalStore::new(journal_root),
            plans: PlanStore::new(ttl),
            grants: PathGrantStore::new(ttl),
            mutation_lock,
            cancellations: CancellationRegistry::default(),
            adapter,
            #[cfg(any(target_os = "windows", test))]
            excel_pipeline: None,
            #[cfg(test)]
            journal_write_fail_on: None,
            #[cfg(test)]
            journal_write_count: AtomicUsize::new(0),
        }
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn with_excel_pipeline(
        mut self,
        pipeline: Arc<crate::platform::excel::ExcelExportPipeline>,
    ) -> Self {
        self.excel_pipeline = Some(pipeline);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_test_journal_failure_on_write(mut self, write_number: usize) -> Self {
        self.journal_write_fail_on = Some(write_number);
        self.journal_write_count.store(0, AtomicOrdering::SeqCst);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_test_queue_failure_on_save(self, save_number: usize) -> Self {
        self.queues.fail_on_save(save_number);
        self
    }

    pub(in crate::print_queue) fn write_journal(
        &self,
        record: &JournalRecord,
    ) -> Result<(), BackendError> {
        #[cfg(test)]
        if self.journal_write_fail_on.is_some_and(|write_number| {
            self.journal_write_count
                .fetch_add(1, AtomicOrdering::SeqCst)
                + 1
                == write_number
        }) {
            return Err(BackendError::journal_invalid(
                "injected print journal write failure",
            ));
        }
        self.journals.write(record)
    }

    pub(in crate::print_queue) fn persist_cleanup_review(
        &self,
        state: &mut QueueState,
        record: &JournalRecord,
        primary_classification: &'static str,
        review: &CleanupReview,
    ) -> Result<(), BackendError> {
        let journal_error = self.write_journal(record).err();
        let queue_error = self.queues.save(state).err();
        let (mut error, secondary) = match (journal_error, queue_error) {
            (None, None) => return Ok(()),
            (Some(error), None) | (None, Some(error)) => (error, None),
            (Some(journal), Some(queue)) => (
                queue,
                Some(format!(
                    "cleanup evidence journal also failed: {}",
                    journal.message
                )),
            ),
        };
        if let Some(secondary) = secondary {
            error.message = format!("{}; {secondary}", error.message);
        }
        error.message = format!(
            "{} [cleanupEvidence: primaryClassification={primary_classification}; warningKey={}; locator={}]",
            error.message, review.warning_key, review.locator
        );
        Err(error)
    }

    pub fn inventory(&self) -> Result<PrinterInventory, BackendError> {
        self.adapter
            .inventory()
            .map(PrinterInventory::normalized)
            .map_err(PrintFailure::into_backend_error)
    }

    pub(crate) fn diagnostics(&self, caller: Option<&CallerIdentity>) -> PrinterDiagnostic {
        let inventory = self.adapter.inventory().map(PrinterInventory::normalized);
        let latest_queue = caller
            .and_then(|caller| self.queues.latest_owned_state(caller).ok().flatten())
            .map(queue_diagnostic);
        match inventory {
            Ok(inventory) => PrinterDiagnostic {
                inventory_status: "available".to_string(),
                printer_count: inventory.printers.len() as u64,
                default_printer_available: inventory.default_printer.is_some(),
                latest_queue,
            },
            Err(_) => PrinterDiagnostic {
                inventory_status: "unavailable".to_string(),
                latest_queue,
                ..PrinterDiagnostic::default()
            },
        }
    }

    pub fn preview<F>(
        &self,
        caller: &CallerIdentity,
        params: BatchPrintParams,
        mut progress: F,
    ) -> Result<Plan, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let inventory = self.inventory()?;
        let binding = self.issue_file_binding(caller, &params.files)?;
        let mut workbook_files = HashMap::<PathBuf, File>::new();
        for path in &params.files {
            if !matches!(
                path.extension().and_then(|value| value.to_str()),
                Some(value) if value.eq_ignore_ascii_case("xlsx") || value.eq_ignore_ascii_case("xlsm")
            ) {
                continue;
            }
            let opened = self
                .grants
                .authorize_for_plan(
                    &binding.grant_ids,
                    caller,
                    &binding.operation_id,
                    path,
                    PathIntent::Read,
                )
                .and_then(|canonical_path| {
                    self.grants.authorize_capability_for_plan(
                        &binding.grant_ids,
                        caller,
                        &binding.operation_id,
                        &canonical_path,
                        PathIntent::Read,
                    )
                })
                .and_then(|authorized| {
                    authorized
                        .anchor()
                        .map_err(|_| BackendError::path_invalid("print file is unavailable"))
                })
                .and_then(|anchored| {
                    anchored
                        .open_read()
                        .map_err(|_| BackendError::path_invalid("print file is unavailable"))
                });
            if let Ok(file) = opened {
                workbook_files.entry(path.clone()).or_insert(file);
            }
        }
        progress(ProgressEvent {
            phase: "printer_check".to_string(),
            done: 1,
            total: 1,
            subject: params.printer.clone(),
        });
        let plan = batch_print::preview_with_workbook_opener(
            &params,
            &inventory,
            self.adapter.as_ref(),
            progress,
            |path| {
                workbook_files
                    .remove(path)
                    .ok_or("warn.print.workbookDiscoveryFailed")
            },
        )?;
        self.plans.insert_bound(BackendOrigin::Rust, binding, plan)
    }

    pub fn revise(
        &self,
        caller: &CallerIdentity,
        plan_id: &str,
        rows: &[Value],
    ) -> Result<Plan, BackendError> {
        let claimed = self.resolve(plan_id, caller)?;
        let binding = claimed.binding.ok_or_else(BackendError::plan_unavailable)?;
        let inventory = self.inventory()?;
        let revised = batch_print::revise(&claimed.plan, rows, &inventory, self.adapter.as_ref())?;
        self.revalidate_worksheet_plan(&binding, caller, &revised)?;
        self.plans
            .insert_bound(BackendOrigin::Rust, binding, revised)
    }

    pub fn resume(&self, caller: &CallerIdentity, queue_id: &str) -> Result<Plan, BackendError> {
        self.queues.begin(queue_id)?;
        let _active_queue = PrintQueueActiveRegistration {
            store: &self.queues,
            queue_id: queue_id.to_string(),
        };
        let mut state = self.queues.load(queue_id)?;
        state.authorize_owner(caller)?;
        for (index, job) in state.jobs.iter_mut().enumerate() {
            job.normalize_legacy(index);
            batch_print::validate_job(job)?;
        }
        state.recover_interrupted();
        let inventory = self.inventory()?;
        let value = serde_json::to_value(&state)
            .map_err(|_| BackendError::protocol("print queue encoding failed"))?;
        let plan = batch_print::resume_plan(&value, &inventory)?;
        let (binding, provisional_grants) =
            self.prepare_resume_file_binding(caller, &state, &plan)?;
        state.session_id = caller.session_id.clone();
        state.authorize_session(caller)?;
        self.queues.save(&mut state)?;
        let cached = self
            .plans
            .insert_bound(BackendOrigin::Rust, binding, plan)?;
        provisional_grants.commit();
        Ok(cached)
    }

    pub fn latest(
        &self,
        caller: &CallerIdentity,
    ) -> Result<Option<PrintQueueSummary>, BackendError> {
        self.queues.latest(caller)
    }

    pub fn discard(
        &self,
        caller: &CallerIdentity,
        queue_id: &str,
    ) -> Result<PrintQueueSummary, BackendError> {
        let _permit = self.mutation_lock.try_acquire()?;
        self.queues.discard(caller, queue_id)
    }

    pub fn contains_plan(&self, plan_id: &str) -> bool {
        self.plans
            .snapshot(plan_id)
            .is_some_and(|snapshot| snapshot.origin == BackendOrigin::Rust)
    }

    #[doc(hidden)]
    #[cfg(debug_assertions)]
    pub fn active_grant_count_for_testing(&self) -> usize {
        self.grants.active_grant_count_for_testing()
    }

    #[doc(hidden)]
    #[cfg(debug_assertions)]
    pub fn active_plan_count_for_testing(&self) -> usize {
        self.plans.active_plan_count_for_testing()
    }

    pub fn cancel(&self, plan_id: &str) -> bool {
        self.cancellations.cancel(plan_id)
    }

    pub fn revoke(&self, caller: &CallerIdentity) {
        self.plans.revoke(caller);
        self.grants.revoke(caller);
        self.cancellations.revoke_all();
        lock(&self.queues.memory).retain(|_, state| {
            state.owner_id != caller.owner_id || state.session_id != caller.session_id
        });
    }
}
