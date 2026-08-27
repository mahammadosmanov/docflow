use std::{
    collections::BTreeSet,
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, TryLockError,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

use uuid::Uuid;

use crate::{
    cap_fs::{AnchoredPath, CapabilityRoot},
    ooxml::{WorkbookSafetyFacts, WorksheetDescriptor},
    print_queue::{FileSignature, PrintFailure, PrintJob, file_signature_from_open_file},
    progress::CancellationToken,
};

const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(10);

static PROCESS_EXCEL_BREAKER: OnceLock<Arc<AtomicBool>> = OnceLock::new();
static PROCESS_EXCEL_OPERATION: OnceLock<Arc<Mutex<()>>> = OnceLock::new();
static PROCESS_EXCEL_QUARANTINE: OnceLock<Arc<Mutex<BTreeSet<PathBuf>>>> = OnceLock::new();

struct ExcelProcessGuards {
    breaker: Arc<AtomicBool>,
    operation: Arc<Mutex<()>>,
    quarantine: Arc<Mutex<BTreeSet<PathBuf>>>,
}

fn process_excel_guards() -> ExcelProcessGuards {
    ExcelProcessGuards {
        breaker: Arc::clone(PROCESS_EXCEL_BREAKER.get_or_init(|| Arc::new(AtomicBool::new(false)))),
        operation: Arc::clone(PROCESS_EXCEL_OPERATION.get_or_init(|| Arc::new(Mutex::new(())))),
        quarantine: Arc::clone(
            PROCESS_EXCEL_QUARANTINE.get_or_init(|| Arc::new(Mutex::new(BTreeSet::new()))),
        ),
    }
}

#[cfg(test)]
fn isolated_excel_guards() -> ExcelProcessGuards {
    ExcelProcessGuards {
        breaker: Arc::new(AtomicBool::new(false)),
        operation: Arc::new(Mutex::new(())),
        quarantine: Arc::new(Mutex::new(BTreeSet::new())),
    }
}

pub(crate) struct ExcelExportRequest {
    pub(crate) request_id: Uuid,
    pub(crate) generation: u64,
    pub(crate) staged_workbook: StagedWorkbook,
    pub(crate) approved_signature: FileSignature,
    pub(crate) safety: WorkbookSafetyFacts,
    pub(crate) worksheet: WorksheetDescriptor,
    pub(crate) deadline: Instant,
    pub(crate) cancellation: CancellationToken,
}

pub(crate) trait WorksheetPdfExporter: Send + Sync {
    fn export_pdf(&self, request: ExcelExportRequest)
    -> Result<ExportedWorksheetPdf, PrintFailure>;
}

pub(crate) trait WorkbookSourcePolicy: Send + Sync {
    fn validate_source(
        &self,
        source: &AnchoredPath,
        safety: &WorkbookSafetyFacts,
    ) -> Result<(), PrintFailure>;
}

pub(crate) trait ControlledPdfSubmitter: Send + Sync {
    fn submit_pdf(
        &self,
        job: &PrintJob,
        pdf: &ExportedWorksheetPdf,
        cancellation: CancellationToken,
    ) -> Result<(), PrintFailure>;
}

pub(crate) trait MonotonicClock: Send + Sync {
    fn now(&self) -> Instant;
}

struct SystemMonotonicClock;

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub(crate) trait ExcelWorkerRuntime: Send + Sync {
    fn launch(
        &self,
        exporter: Arc<dyn WorksheetPdfExporter>,
        request: ExcelExportRequest,
    ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure>;
}

pub(crate) trait ExcelWorkerHandle: Send {
    fn poll(&mut self, wait: Duration) -> WorkerPoll;
    fn join_status(&mut self) -> WorkerJoin;
}

pub(crate) enum WorkerPoll {
    Response(Box<WorkerResponse>),
    Pending,
    Disconnected,
}

pub(crate) enum WorkerJoin {
    Pending,
    Finished,
    Failed,
}

struct ThreadWorkerRuntime;

impl ExcelWorkerRuntime for ThreadWorkerRuntime {
    fn launch(
        &self,
        exporter: Arc<dyn WorksheetPdfExporter>,
        request: ExcelExportRequest,
    ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
        let request_id = request.request_id;
        let generation = request.generation;
        let (sender, receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("docflow-excel-export".to_string())
            .spawn(move || {
                let result = exporter.export_pdf(request);
                let _ = sender.send(WorkerResponse {
                    request_id,
                    generation,
                    result,
                });
            })
            .map_err(|_| PrintFailure::failed("Excel export worker could not start"))?;
        Ok(Box::new(ThreadWorkerHandle {
            receiver,
            join: Some(join),
            join_result: None,
        }))
    }
}

struct ThreadWorkerHandle {
    receiver: mpsc::Receiver<WorkerResponse>,
    join: Option<thread::JoinHandle<()>>,
    join_result: Option<bool>,
}

impl ExcelWorkerHandle for ThreadWorkerHandle {
    fn poll(&mut self, wait: Duration) -> WorkerPoll {
        match self.receiver.recv_timeout(wait) {
            Ok(response) => WorkerPoll::Response(Box::new(response)),
            Err(mpsc::RecvTimeoutError::Timeout) => WorkerPoll::Pending,
            Err(mpsc::RecvTimeoutError::Disconnected) => WorkerPoll::Disconnected,
        }
    }

    fn join_status(&mut self) -> WorkerJoin {
        if let Some(join) = self.join.as_ref()
            && !join.is_finished()
        {
            return WorkerJoin::Pending;
        }
        if let Some(join) = self.join.take() {
            self.join_result = Some(join.join().is_ok());
        }
        match self.join_result {
            Some(true) => WorkerJoin::Finished,
            Some(false) | None => WorkerJoin::Failed,
        }
    }
}

pub(crate) struct ExcelExportPipeline {
    policy: Arc<dyn WorkbookSourcePolicy>,
    exporter: Arc<dyn WorksheetPdfExporter>,
    submitter: Arc<dyn ControlledPdfSubmitter>,
    generation: AtomicU64,
    breaker: Arc<AtomicBool>,
    operation: Arc<Mutex<()>>,
    quarantine: Arc<Mutex<BTreeSet<PathBuf>>>,
    clock: Arc<dyn MonotonicClock>,
    runtime: Arc<dyn ExcelWorkerRuntime>,
    teardown_grace: Duration,
    #[cfg(test)]
    on_operation_contended: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ExcelExportPipeline {
    pub(crate) fn new(
        policy: Arc<dyn WorkbookSourcePolicy>,
        exporter: Arc<dyn WorksheetPdfExporter>,
        submitter: Arc<dyn ControlledPdfSubmitter>,
    ) -> Self {
        Self::with_breaker(
            policy,
            exporter,
            submitter,
            process_excel_guards(),
            Arc::new(SystemMonotonicClock),
            Arc::new(ThreadWorkerRuntime),
            Duration::from_secs(2),
        )
    }

    #[cfg(test)]
    fn for_testing(
        policy: Arc<dyn WorkbookSourcePolicy>,
        exporter: Arc<dyn WorksheetPdfExporter>,
        submitter: Arc<dyn ControlledPdfSubmitter>,
    ) -> Self {
        Self::with_breaker(
            policy,
            exporter,
            submitter,
            isolated_excel_guards(),
            Arc::new(SystemMonotonicClock),
            Arc::new(ThreadWorkerRuntime),
            Duration::from_secs(2),
        )
    }

    #[cfg(test)]
    fn for_testing_with_runtime(
        policy: Arc<dyn WorkbookSourcePolicy>,
        exporter: Arc<dyn WorksheetPdfExporter>,
        submitter: Arc<dyn ControlledPdfSubmitter>,
        clock: Arc<dyn MonotonicClock>,
        runtime: Arc<dyn ExcelWorkerRuntime>,
        teardown_grace: Duration,
    ) -> Self {
        Self::with_breaker(
            policy,
            exporter,
            submitter,
            isolated_excel_guards(),
            clock,
            runtime,
            teardown_grace,
        )
    }

    fn with_breaker(
        policy: Arc<dyn WorkbookSourcePolicy>,
        exporter: Arc<dyn WorksheetPdfExporter>,
        submitter: Arc<dyn ControlledPdfSubmitter>,
        guards: ExcelProcessGuards,
        clock: Arc<dyn MonotonicClock>,
        runtime: Arc<dyn ExcelWorkerRuntime>,
        teardown_grace: Duration,
    ) -> Self {
        Self {
            policy,
            exporter,
            submitter,
            generation: AtomicU64::new(0),
            breaker: guards.breaker,
            operation: guards.operation,
            quarantine: guards.quarantine,
            clock,
            runtime,
            teardown_grace,
            #[cfg(test)]
            on_operation_contended: None,
        }
    }

    #[cfg(test)]
    fn with_operation_contended_hook(mut self, hook: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.on_operation_contended = Some(hook);
        self
    }

    #[cfg(test)]
    fn clear_test_quarantine(&self) {
        let retained = self
            .quarantine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for path in retained {
            if path.exists() {
                std::fs::remove_dir_all(&path).expect("test quarantine cleanup must succeed");
            }
            self.quarantine
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&path);
        }
    }

    pub(crate) fn export_and_submit(
        &self,
        job: &PrintJob,
        source: &AnchoredPath,
        source_file: &mut File,
        cancellation: CancellationToken,
    ) -> Result<(), PrintFailure> {
        let deadline = self
            .clock
            .now()
            .checked_add(Duration::from_secs(u64::from(job.timeout_minutes) * 60))
            .ok_or_else(|| PrintFailure::unsupported("Excel export deadline is invalid"))?;
        if self.breaker.load(Ordering::Acquire) {
            return Err(PrintFailure::unsupported(
                "error.print.excelExportQuarantined",
            ));
        }
        let _operation = self.acquire_operation(deadline, &cancellation)?;
        if job.page_range != "all" {
            return Err(PrintFailure::unsupported_options(
                "error.print.worksheetPageRangeAllOnly",
            ));
        }
        let worksheet = job
            .worksheet
            .clone()
            .ok_or_else(|| PrintFailure::unsupported("worksheet export metadata is missing"))?;
        let safety = job
            .workbook_safety
            .clone()
            .ok_or_else(|| PrintFailure::unsupported("workbook safety metadata is missing"))?;
        self.pre_worker_fence(deadline, &cancellation)?;
        self.policy.validate_source(source, &safety)?;
        self.pre_worker_fence(deadline, &cancellation)?;

        let staged_workbook = match StagedWorkbook::stage(
            source_file,
            &job.path,
            &job.signature,
            Arc::clone(&self.breaker),
            Arc::clone(&self.quarantine),
        ) {
            Ok(staged_workbook) => staged_workbook,
            Err(failure) => return Err(self.prefer_cleanup_review(failure)),
        };
        if let Err(failure) = self.pre_worker_fence(deadline, &cancellation) {
            drop(staged_workbook);
            return Err(self.prefer_cleanup_review(failure));
        }
        let previous_generation =
            match self
                .generation
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
                    generation.checked_add(1)
                }) {
                Ok(generation) => generation,
                Err(_) => {
                    drop(staged_workbook);
                    return Err(self.prefer_cleanup_review(PrintFailure::unsupported(
                        "Excel export generation is exhausted",
                    )));
                }
            };
        let generation = previous_generation + 1;
        let request_id = Uuid::new_v4();
        let request = ExcelExportRequest {
            request_id,
            generation,
            staged_workbook,
            approved_signature: job.signature.clone(),
            safety,
            worksheet,
            deadline,
            cancellation: cancellation.clone(),
        };
        let mut worker = match self.runtime.launch(Arc::clone(&self.exporter), request) {
            Ok(worker) => worker,
            Err(failure) => return Err(self.prefer_cleanup_review(failure)),
        };
        let response = match self.wait_for_response(worker.as_mut(), deadline, &cancellation) {
            WorkerWait::Response(response) => response,
            WorkerWait::Cancelled => {
                self.invalidate_generation(generation);
                let teardown = self.teardown_loser(worker.as_mut());
                return Err(self.classify_loser(teardown, clean_cancelled()));
            }
            WorkerWait::TimedOut => {
                self.invalidate_generation(generation);
                let teardown = self.teardown_loser(worker.as_mut());
                return Err(self.classify_loser(
                    teardown,
                    PrintFailure::failed("error.print.excelExportTimedOut"),
                ));
            }
            WorkerWait::Disconnected => {
                let teardown = self.teardown_loser(worker.as_mut());
                return Err(self.classify_loser(
                    teardown,
                    PrintFailure::failed("Excel export worker stopped"),
                ));
            }
        };
        if !self.finish_worker(worker.as_mut()) {
            self.invalidate_generation(generation);
            let cleanup_failed = match response.result {
                Ok(mut artifact) => artifact.cleanup().is_err(),
                Err(_) => self.breaker.load(Ordering::Acquire),
            };
            self.trip_breaker();
            return Err(if cleanup_failed {
                cleanup_review_failure()
            } else {
                quarantined_failure()
            });
        }
        let response_request_id = response.request_id;
        let response_generation = response.generation;
        if response_request_id != request_id
            || response_generation != generation
            || self.generation.load(Ordering::Acquire) != generation
        {
            self.invalidate_generation(generation);
            return match response.result {
                Ok(mut artifact) => finish_without_submit(
                    &self.breaker,
                    &mut artifact,
                    PrintFailure::failed("Excel export response is stale"),
                ),
                Err(_) => Err(self
                    .prefer_cleanup_review(PrintFailure::failed("Excel export response is stale"))),
            };
        }
        let mut artifact = match response.result {
            Ok(artifact) => artifact,
            Err(failure) => return Err(self.prefer_cleanup_review(failure)),
        };
        if let Some(failure) = self.export_fence_failure(
            request_id,
            generation,
            response_request_id,
            response_generation,
            deadline,
            &cancellation,
        ) {
            self.invalidate_generation(generation);
            return finish_without_submit(&self.breaker, &mut artifact, failure);
        }

        let source_is_unchanged = match source.open_read() {
            Ok(mut reopened_source) => file_signature_from_open_file(&mut reopened_source, true)
                .map(|signature| signature == job.signature)
                .unwrap_or(false),
            Err(_) => false,
        };
        if !source_is_unchanged {
            return finish_without_submit(
                &self.breaker,
                &mut artifact,
                PrintFailure::failed("error.print.workbookChanged"),
            );
        }
        if let Some(failure) = self.export_fence_failure(
            request_id,
            generation,
            response_request_id,
            response_generation,
            deadline,
            &cancellation,
        ) {
            self.invalidate_generation(generation);
            return finish_without_submit(&self.breaker, &mut artifact, failure);
        }

        let mut controlled_job = job.clone();
        controlled_job.page_range = "all".to_string();
        let submitted = self
            .submitter
            .submit_pdf(&controlled_job, &artifact, cancellation.clone());
        let cleanup = artifact.cleanup();
        match (submitted, cleanup) {
            (result, Ok(())) => result,
            (Ok(()), Err(_)) => {
                self.breaker.store(true, Ordering::Release);
                Err(PrintFailure::uncertain(
                    "error.print.exportCleanupRequiresReview",
                ))
            }
            (Err(failure), Err(_)) if failure.uncertain => {
                self.breaker.store(true, Ordering::Release);
                Err(failure)
            }
            (Err(_), Err(_)) => {
                self.breaker.store(true, Ordering::Release);
                Err(cleanup_review_failure())
            }
        }
    }

    fn prefer_cleanup_review(&self, failure: PrintFailure) -> PrintFailure {
        if self.breaker.load(Ordering::Acquire) {
            cleanup_review_failure()
        } else {
            failure
        }
    }

    fn acquire_operation(
        &self,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<MutexGuard<'_, ()>, PrintFailure> {
        loop {
            self.pre_worker_fence(deadline, cancellation)?;
            match self.operation.try_lock() {
                Ok(operation) => {
                    self.pre_worker_fence(deadline, cancellation)?;
                    return Ok(operation);
                }
                Err(TryLockError::WouldBlock) => {
                    #[cfg(test)]
                    if let Some(hook) = self.on_operation_contended.as_ref() {
                        hook();
                    }
                    let now = self.clock.now();
                    if now >= deadline {
                        return Err(PrintFailure::failed("error.print.excelExportTimedOut"));
                    }
                    thread::sleep(
                        deadline
                            .saturating_duration_since(now)
                            .min(WORKER_POLL_INTERVAL),
                    );
                }
                Err(TryLockError::Poisoned(_)) => {
                    self.trip_breaker();
                    return Err(quarantined_failure());
                }
            }
        }
    }

    fn pre_worker_fence(
        &self,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), PrintFailure> {
        if self.breaker.load(Ordering::Acquire) {
            return Err(quarantined_failure());
        }
        if cancellation.is_cancelled() {
            return Err(clean_cancelled());
        }
        if self.clock.now() >= deadline {
            return Err(PrintFailure::failed("error.print.excelExportTimedOut"));
        }
        Ok(())
    }

    fn export_fence_failure(
        &self,
        request_id: Uuid,
        generation: u64,
        response_request_id: Uuid,
        response_generation: u64,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Option<PrintFailure> {
        if self.breaker.load(Ordering::Acquire) {
            return Some(quarantined_failure());
        }
        if response_request_id != request_id
            || response_generation != generation
            || self.generation.load(Ordering::Acquire) != generation
        {
            return Some(PrintFailure::failed("Excel export response is stale"));
        }
        if cancellation.is_cancelled() {
            return Some(clean_cancelled());
        }
        if self.clock.now() >= deadline {
            return Some(PrintFailure::failed("error.print.excelExportTimedOut"));
        }
        None
    }

    fn wait_for_response(
        &self,
        worker: &mut dyn ExcelWorkerHandle,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> WorkerWait {
        loop {
            if cancellation.is_cancelled() {
                return WorkerWait::Cancelled;
            }
            let now = self.clock.now();
            if now >= deadline {
                return WorkerWait::TimedOut;
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(WORKER_POLL_INTERVAL);
            match worker.poll(wait) {
                WorkerPoll::Response(response) => return WorkerWait::Response(response),
                WorkerPoll::Pending => {}
                WorkerPoll::Disconnected => return WorkerWait::Disconnected,
            }
        }
    }

    fn finish_worker(&self, worker: &mut dyn ExcelWorkerHandle) -> bool {
        let Some(deadline) = self.clock.now().checked_add(self.teardown_grace) else {
            return false;
        };
        loop {
            match worker.join_status() {
                WorkerJoin::Finished => return true,
                WorkerJoin::Failed => return false,
                WorkerJoin::Pending => {}
            }
            let now = self.clock.now();
            if now >= deadline {
                return false;
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(WORKER_POLL_INTERVAL);
            let _ = worker.poll(wait);
        }
    }

    fn teardown_loser(&self, worker: &mut dyn ExcelWorkerHandle) -> LoserTeardown {
        let Some(deadline) = self.clock.now().checked_add(self.teardown_grace) else {
            return if self.breaker.load(Ordering::Acquire) {
                LoserTeardown::CleanupFailed
            } else {
                LoserTeardown::Failed
            };
        };
        let mut cleanup_failed = false;
        loop {
            match worker.join_status() {
                WorkerJoin::Finished => {
                    cleanup_failed |= self.drain_loser_responses(worker);
                    return if cleanup_failed || self.breaker.load(Ordering::Acquire) {
                        LoserTeardown::CleanupFailed
                    } else {
                        LoserTeardown::Finished
                    };
                }
                WorkerJoin::Failed => {
                    cleanup_failed |= self.drain_loser_responses(worker);
                    return if cleanup_failed || self.breaker.load(Ordering::Acquire) {
                        LoserTeardown::CleanupFailed
                    } else {
                        LoserTeardown::Failed
                    };
                }
                WorkerJoin::Pending => {}
            }
            let now = self.clock.now();
            if now >= deadline {
                return if cleanup_failed || self.breaker.load(Ordering::Acquire) {
                    LoserTeardown::CleanupFailed
                } else {
                    LoserTeardown::Failed
                };
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(WORKER_POLL_INTERVAL);
            match worker.poll(wait) {
                WorkerPoll::Response(response) => {
                    cleanup_failed |= self.cleanup_loser_response(response);
                }
                WorkerPoll::Pending | WorkerPoll::Disconnected => {}
            }
        }
    }

    fn drain_loser_responses(&self, worker: &mut dyn ExcelWorkerHandle) -> bool {
        let mut cleanup_failed = false;
        loop {
            match worker.poll(Duration::ZERO) {
                WorkerPoll::Response(response) => {
                    cleanup_failed |= self.cleanup_loser_response(response);
                }
                WorkerPoll::Pending | WorkerPoll::Disconnected => return cleanup_failed,
            }
        }
    }

    fn cleanup_loser_response(&self, response: Box<WorkerResponse>) -> bool {
        if let Ok(mut artifact) = response.result
            && artifact.cleanup().is_err()
        {
            self.trip_breaker();
            return true;
        }
        false
    }

    fn classify_loser(
        &self,
        teardown: LoserTeardown,
        finished_failure: PrintFailure,
    ) -> PrintFailure {
        match teardown {
            LoserTeardown::CleanupFailed => cleanup_review_failure(),
            LoserTeardown::Finished => self.prefer_cleanup_review(finished_failure),
            LoserTeardown::Failed if self.breaker.load(Ordering::Acquire) => {
                cleanup_review_failure()
            }
            LoserTeardown::Failed => {
                self.trip_breaker();
                quarantined_failure()
            }
        }
    }

    fn invalidate_generation(&self, generation: u64) {
        let _ = self.generation.compare_exchange(
            generation,
            generation.saturating_add(1),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn trip_breaker(&self) {
        self.breaker.store(true, Ordering::Release);
    }
}

enum WorkerWait {
    Response(Box<WorkerResponse>),
    Cancelled,
    TimedOut,
    Disconnected,
}

enum LoserTeardown {
    Finished,
    Failed,
    CleanupFailed,
}

pub(crate) struct WorkerResponse {
    request_id: Uuid,
    generation: u64,
    result: Result<ExportedWorksheetPdf, PrintFailure>,
}

fn finish_without_submit(
    breaker: &AtomicBool,
    artifact: &mut ExportedWorksheetPdf,
    failure: PrintFailure,
) -> Result<(), PrintFailure> {
    match artifact.cleanup() {
        Ok(()) => Err(failure),
        Err(_) => {
            breaker.store(true, Ordering::Release);
            Err(cleanup_review_failure())
        }
    }
}

fn cleanup_review_failure() -> PrintFailure {
    PrintFailure::unsupported("error.print.exportCleanupRequiresReview")
}

fn clean_cancelled() -> PrintFailure {
    PrintFailure::cancelled("error.print.excelExportCancelled", false)
}

fn quarantined_failure() -> PrintFailure {
    PrintFailure::unsupported("error.print.excelExportQuarantined")
}

pub(crate) struct StagedWorkbook {
    // These are Options because cleanup must close capability handles in this exact order
    // before TempDir attempts directory removal (required by Windows handle semantics).
    workbook: Option<AnchoredPath>,
    root: Option<CapabilityRoot>,
    workspace: Option<TempDir>,
    workbook_path: PathBuf,
    cleanup_breaker: Arc<AtomicBool>,
    cleanup_quarantine: Arc<Mutex<BTreeSet<PathBuf>>>,
    #[cfg(test)]
    drop_events: Option<std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>>,
    #[cfg(test)]
    cleanup_failure: bool,
}

impl StagedWorkbook {
    fn stage(
        source_file: &mut File,
        source_path: &str,
        approved_signature: &FileSignature,
        cleanup_breaker: Arc<AtomicBool>,
        cleanup_quarantine: Arc<Mutex<BTreeSet<PathBuf>>>,
    ) -> Result<Self, PrintFailure> {
        let workspace = tempfile::tempdir()
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        let mut staged = Self {
            workbook: None,
            root: None,
            workspace: Some(workspace),
            workbook_path: PathBuf::new(),
            cleanup_breaker,
            cleanup_quarantine,
            #[cfg(test)]
            drop_events: None,
            #[cfg(test)]
            cleanup_failure: false,
        };
        let root = CapabilityRoot::open(staged.workspace_path())
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        staged.root = Some(root);
        let source_name = Path::new(source_path)
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| PrintFailure::failed("print file name is unavailable"))?;
        staged.workbook_path = staged.workspace_path().join(source_name);
        let workbook = staged
            .root
            .as_ref()
            .expect("live staged workbook owns its capability root")
            .authorize(&staged.workbook_path)
            .and_then(|path| path.anchor())
            .map_err(|_| PrintFailure::failed("private print staging is unavailable"))?;
        staged.workbook = Some(workbook);
        let workbook = staged
            .workbook
            .as_ref()
            .expect("live staged workbook owns its workbook anchor");
        workbook
            .copy_no_clobber_from_file(source_file)
            .map_err(|_| PrintFailure::failed("private print staging failed"))?;
        if workbook
            .sha256()
            .map_err(|_| PrintFailure::failed("private print staging verification failed"))?
            != approved_signature.sha256.as_deref().unwrap_or_default()
        {
            return Err(PrintFailure::failed(
                "private print staging verification failed",
            ));
        }
        Ok(staged)
    }

    pub(crate) fn workbook_path(&self) -> &Path {
        &self.workbook_path
    }

    pub(crate) fn pdf_output_path(&self) -> PathBuf {
        self.workspace_path().join("worksheet.pdf")
    }

    pub(crate) fn into_exported_pdf(self) -> Result<ExportedWorksheetPdf, PrintFailure> {
        let pdf_path = self.pdf_output_path();
        let pdf = self
            .root
            .as_ref()
            .expect("live staged workbook owns its capability root")
            .authorize(&pdf_path)
            .and_then(|path| path.anchor())
            .map_err(|_| PrintFailure::failed("private PDF export is unavailable"))?;
        if pdf
            .len()
            .map_err(|_| PrintFailure::failed("private PDF export is unavailable"))?
            == 0
        {
            return Err(PrintFailure::failed("worksheet PDF export is empty"));
        }
        Ok(ExportedWorksheetPdf {
            pdf: Some(pdf),
            workspace: Some(self),
            pdf_path,
        })
    }

    fn cleanup(&mut self) -> io::Result<()> {
        if let Some(workbook) = self.workbook.take() {
            drop(workbook);
            self.record_drop("workbook");
        }
        if let Some(root) = self.root.take() {
            drop(root);
            self.record_drop("root");
        }
        let Some(workspace) = self.workspace.take() else {
            return Ok(());
        };
        self.record_drop("workspace");
        let workspace_path = workspace.path().to_path_buf();
        self.cleanup_quarantine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(workspace_path.clone());
        #[cfg(test)]
        let result = if self.cleanup_failure {
            let retained_path = workspace.keep();
            debug_assert_eq!(retained_path, workspace_path);
            Err(io::Error::other(
                "injected private workspace cleanup failure",
            ))
        } else {
            workspace.close()
        };
        #[cfg(not(test))]
        let result = workspace.close();
        match result {
            Ok(()) => {
                self.cleanup_quarantine
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&workspace_path);
                Ok(())
            }
            Err(error) => {
                self.cleanup_breaker.store(true, Ordering::Release);
                Err(error)
            }
        }
    }

    #[cfg(test)]
    fn fixture_with_drop_probe() -> (Self, std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>) {
        let workspace = tempfile::tempdir().unwrap();
        let root = CapabilityRoot::open(workspace.path()).unwrap();
        let workbook_path = workspace.path().join("book.xlsx");
        let workbook = root.authorize(&workbook_path).unwrap().anchor().unwrap();
        workbook.write_no_clobber(b"workbook").unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            Self {
                workbook: Some(workbook),
                root: Some(root),
                workspace: Some(workspace),
                workbook_path,
                cleanup_breaker: Arc::new(AtomicBool::new(false)),
                cleanup_quarantine: Arc::new(Mutex::new(BTreeSet::new())),
                drop_events: Some(std::sync::Arc::clone(&events)),
                cleanup_failure: false,
            },
            events,
        )
    }

    #[cfg(test)]
    fn fixture_exported_pdf(self) -> ExportedWorksheetPdf {
        let pdf_path = self.pdf_output_path();
        std::fs::write(&pdf_path, b"%PDF-1.4\n").unwrap();
        self.into_exported_pdf().unwrap()
    }

    #[cfg(test)]
    fn fixture_exported_pdf_with_cleanup_failure(self) -> ExportedWorksheetPdf {
        let mut exported = self.fixture_exported_pdf();
        exported
            .workspace
            .as_mut()
            .expect("fixture PDF owns its staged workbook")
            .cleanup_failure = true;
        exported
    }

    fn workspace_path(&self) -> &Path {
        self.workspace
            .as_ref()
            .expect("live staged workbook owns its workspace")
            .path()
    }

    #[cfg(test)]
    fn record_drop(&self, event: &'static str) {
        if let Some(events) = self.drop_events.as_ref() {
            events.lock().unwrap().push(event);
        }
    }

    #[cfg(not(test))]
    fn record_drop(&self, _event: &'static str) {}
}

impl Drop for StagedWorkbook {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

pub(crate) struct ExportedWorksheetPdf {
    // The PDF anchor closes before StagedWorkbook closes the workbook/root/workspace.
    pdf: Option<AnchoredPath>,
    workspace: Option<StagedWorkbook>,
    pdf_path: PathBuf,
}

impl ExportedWorksheetPdf {
    pub(crate) fn pdf_path(&self) -> &Path {
        &self.pdf_path
    }

    fn cleanup(&mut self) -> io::Result<()> {
        if let Some(pdf) = self.pdf.take() {
            drop(pdf);
            if let Some(workspace) = self.workspace.as_ref() {
                workspace.record_drop("pdf");
            }
        }
        let Some(mut workspace) = self.workspace.take() else {
            return Ok(());
        };
        workspace.cleanup()
    }
}

impl Drop for ExportedWorksheetPdf {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    mod export_supervisor {
        use std::{
            collections::BTreeSet,
            fs::File,
            io::{Seek, Write},
            path::{Path, PathBuf},
            process::Command,
            sync::{
                Arc, Mutex,
                atomic::{AtomicBool, AtomicUsize, Ordering},
                mpsc,
            },
            time::{Duration, Instant},
        };

        use crate::{
            cap_fs::CapabilityRoot,
            journal::FilesystemObjectIdentity,
            ooxml::{WorkbookSafetyFacts, WorksheetDescriptor, WorksheetVisibility},
            page_range::PageRangeCapability,
            paths::CallerIdentity,
            print_queue::{
                BatchPrintParams, FileSignature, PrintAdapter, PrintFailure, PrintJob,
                PrintJobSupport, PrintService, PrintSettings, PrinterInventory,
            },
            progress::{CancellationRegistry, GlobalMutationLock},
        };
        use zip::{ZipWriter, write::SimpleFileOptions};

        use super::super::{
            ControlledPdfSubmitter, ExcelExportPipeline, ExcelExportRequest, ExcelProcessGuards,
            ExcelWorkerHandle, ExcelWorkerRuntime, ExportedWorksheetPdf, MonotonicClock,
            StagedWorkbook, SystemMonotonicClock, ThreadWorkerRuntime, WorkbookSourcePolicy,
            WorkerJoin, WorkerPoll, WorkerResponse, WorksheetPdfExporter, process_excel_guards,
        };

        #[derive(Clone, Debug, Eq, PartialEq)]
        struct RequestObservation {
            request_id: uuid::Uuid,
            generation: u64,
            signature: FileSignature,
            safety: WorkbookSafetyFacts,
            worksheet: WorksheetDescriptor,
            deadline: Instant,
            workbook_bytes: Vec<u8>,
            cancelled: bool,
        }

        struct RecordingPolicy {
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        #[derive(Clone, Debug, Eq, PartialEq)]
        struct PolicyObservation {
            absolute: PathBuf,
            identity: FilesystemObjectIdentity,
            safety: WorkbookSafetyFacts,
        }

        struct CapturingPolicy {
            events: Arc<Mutex<Vec<&'static str>>>,
            observations: Arc<Mutex<Vec<PolicyObservation>>>,
        }

        impl WorkbookSourcePolicy for RecordingPolicy {
            fn validate_source(
                &self,
                _source: &crate::cap_fs::AnchoredPath,
                _safety: &WorkbookSafetyFacts,
            ) -> Result<(), PrintFailure> {
                self.events.lock().unwrap().push("policy");
                Ok(())
            }
        }

        impl WorkbookSourcePolicy for CapturingPolicy {
            fn validate_source(
                &self,
                source: &crate::cap_fs::AnchoredPath,
                safety: &WorkbookSafetyFacts,
            ) -> Result<(), PrintFailure> {
                self.events.lock().unwrap().push("policy");
                self.observations.lock().unwrap().push(PolicyObservation {
                    absolute: source.absolute().to_path_buf(),
                    identity: source
                        .identity()
                        .map_err(|_| PrintFailure::failed("fake policy identity failed"))?,
                    safety: safety.clone(),
                });
                Ok(())
            }
        }

        struct RejectingPolicy {
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl WorkbookSourcePolicy for RejectingPolicy {
            fn validate_source(
                &self,
                _source: &crate::cap_fs::AnchoredPath,
                _safety: &WorkbookSafetyFacts,
            ) -> Result<(), PrintFailure> {
                self.events.lock().unwrap().push("policy");
                Err(PrintFailure::unsupported("source policy rejected workbook"))
            }
        }

        struct RecordingExporter {
            events: Arc<Mutex<Vec<&'static str>>>,
            requests: Arc<Mutex<Vec<RequestObservation>>>,
        }

        impl WorksheetPdfExporter for RecordingExporter {
            fn export_pdf(
                &self,
                request: ExcelExportRequest,
            ) -> Result<ExportedWorksheetPdf, PrintFailure> {
                self.events.lock().unwrap().push("export");
                self.requests.lock().unwrap().push(RequestObservation {
                    request_id: request.request_id,
                    generation: request.generation,
                    signature: request.approved_signature.clone(),
                    safety: request.safety.clone(),
                    worksheet: request.worksheet.clone(),
                    deadline: request.deadline,
                    workbook_bytes: std::fs::read(request.staged_workbook.workbook_path()).unwrap(),
                    cancelled: request.cancellation.is_cancelled(),
                });
                Ok(request.staged_workbook.fixture_exported_pdf())
            }
        }

        struct SourceMutatingExporter {
            events: Arc<Mutex<Vec<&'static str>>>,
            source_path: PathBuf,
            staged_paths: Arc<Mutex<Vec<PathBuf>>>,
        }

        impl WorksheetPdfExporter for SourceMutatingExporter {
            fn export_pdf(
                &self,
                request: ExcelExportRequest,
            ) -> Result<ExportedWorksheetPdf, PrintFailure> {
                self.events.lock().unwrap().push("export");
                self.staged_paths
                    .lock()
                    .unwrap()
                    .push(request.staged_workbook.workspace_path().to_path_buf());
                let artifact = request.staged_workbook.fixture_exported_pdf();
                std::fs::write(&self.source_path, b"changed after export").unwrap();
                Ok(artifact)
            }
        }

        struct CleanupFailingExporter {
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl WorksheetPdfExporter for CleanupFailingExporter {
            fn export_pdf(
                &self,
                request: ExcelExportRequest,
            ) -> Result<ExportedWorksheetPdf, PrintFailure> {
                self.events.lock().unwrap().push("export");
                Ok(request
                    .staged_workbook
                    .fixture_exported_pdf_with_cleanup_failure())
            }
        }

        struct ErrorWithCleanupFailureExporter {
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl WorksheetPdfExporter for ErrorWithCleanupFailureExporter {
            fn export_pdf(
                &self,
                mut request: ExcelExportRequest,
            ) -> Result<ExportedWorksheetPdf, PrintFailure> {
                self.events.lock().unwrap().push("export");
                request.staged_workbook.cleanup_failure = true;
                Err(PrintFailure::failed("fake export failed"))
            }
        }

        struct PanickingExporter {
            events: Arc<Mutex<Vec<&'static str>>>,
            staged_paths: Arc<Mutex<Vec<PathBuf>>>,
        }

        impl WorksheetPdfExporter for PanickingExporter {
            fn export_pdf(
                &self,
                request: ExcelExportRequest,
            ) -> Result<ExportedWorksheetPdf, PrintFailure> {
                self.events.lock().unwrap().push("export");
                self.staged_paths
                    .lock()
                    .unwrap()
                    .push(request.staged_workbook.workspace_path().to_path_buf());
                panic!("fake Excel worker panic");
            }
        }

        struct PanickingCleanupFailingExporter {
            events: Arc<Mutex<Vec<&'static str>>>,
            staged_paths: Arc<Mutex<Vec<PathBuf>>>,
        }

        impl WorksheetPdfExporter for PanickingCleanupFailingExporter {
            fn export_pdf(
                &self,
                mut request: ExcelExportRequest,
            ) -> Result<ExportedWorksheetPdf, PrintFailure> {
                self.events.lock().unwrap().push("export");
                self.staged_paths
                    .lock()
                    .unwrap()
                    .push(request.staged_workbook.workspace_path().to_path_buf());
                request.staged_workbook.cleanup_failure = true;
                panic!("fake Excel worker panic with cleanup failure");
            }
        }

        struct CancellationBlockingExporter {
            events: Arc<Mutex<Vec<&'static str>>>,
            gate: Arc<(Mutex<bool>, std::sync::Condvar)>,
        }

        impl WorksheetPdfExporter for CancellationBlockingExporter {
            fn export_pdf(
                &self,
                request: ExcelExportRequest,
            ) -> Result<ExportedWorksheetPdf, PrintFailure> {
                self.events.lock().unwrap().push("export");
                let (entered, changed) = &*self.gate;
                let mut entered = entered.lock().unwrap();
                *entered = true;
                changed.notify_all();
                while !request.cancellation.is_cancelled() {
                    entered = changed
                        .wait_timeout(entered, Duration::from_millis(10))
                        .unwrap()
                        .0;
                }
                Err(PrintFailure::cancelled("fake export cancelled", false))
            }
        }

        struct OwnershipBlockingExporter {
            events: Arc<Mutex<Vec<&'static str>>>,
            gate: Arc<(Mutex<bool>, std::sync::Condvar)>,
            staged_paths: Arc<Mutex<Vec<PathBuf>>>,
        }

        impl WorksheetPdfExporter for OwnershipBlockingExporter {
            fn export_pdf(
                &self,
                request: ExcelExportRequest,
            ) -> Result<ExportedWorksheetPdf, PrintFailure> {
                self.events.lock().unwrap().push("export");
                self.staged_paths
                    .lock()
                    .unwrap()
                    .push(request.staged_workbook.workspace_path().to_path_buf());
                let (entered, changed) = &*self.gate;
                let mut entered = entered.lock().unwrap();
                *entered = true;
                changed.notify_all();
                while !request.cancellation.is_cancelled() {
                    entered = changed
                        .wait_timeout(entered, Duration::from_millis(10))
                        .unwrap()
                        .0;
                }
                Err(PrintFailure::cancelled("fake export cancelled", false))
            }
        }

        type SubmittedPdfs = Arc<Mutex<Vec<(String, String, Vec<u8>)>>>;

        struct RecordingSubmitter {
            events: Arc<Mutex<Vec<&'static str>>>,
            submitted: SubmittedPdfs,
        }

        struct OutcomeSubmitter {
            events: Arc<Mutex<Vec<&'static str>>>,
            outcome: Result<(), PrintFailure>,
        }

        #[derive(Default)]
        struct PreviewAdapter {
            native_submits: Mutex<Vec<PrintJob>>,
        }

        impl PrintAdapter for PreviewAdapter {
            fn inventory(&self) -> Result<PrinterInventory, PrintFailure> {
                Ok(PrinterInventory {
                    printers: vec!["Office Printer".to_string()],
                    default_printer: Some("Office Printer".to_string()),
                })
            }

            fn inspect(
                &self,
                _path: &Path,
                _printer: &str,
                _settings: &PrintSettings,
            ) -> Result<PrintJobSupport, PrintFailure> {
                Ok(PrintJobSupport {
                    verb: "docflow".to_string(),
                    settings_limited: false,
                    page_range: PageRangeCapability::AllOnly,
                })
            }

            fn submit(
                &self,
                job: &PrintJob,
                _cancelled: &dyn Fn() -> bool,
            ) -> Result<(), PrintFailure> {
                self.native_submits.lock().unwrap().push(job.clone());
                Ok(())
            }
        }

        struct FixedClock(Instant);

        impl MonotonicClock for FixedClock {
            fn now(&self) -> Instant {
                self.0
            }
        }

        struct AdvancingClock(Mutex<AdvancingClockState>);

        struct AdvancingClockState {
            now: Instant,
            ticking: bool,
        }

        impl AdvancingClock {
            fn new() -> Self {
                Self(Mutex::new(AdvancingClockState {
                    now: Instant::now(),
                    ticking: false,
                }))
            }

            fn advance(&self) {
                let mut state = self.0.lock().unwrap();
                state.now += Duration::from_secs(61);
                state.ticking = true;
            }
        }

        impl MonotonicClock for AdvancingClock {
            fn now(&self) -> Instant {
                let mut state = self.0.lock().unwrap();
                let now = state.now;
                if state.ticking {
                    state.now += Duration::from_secs(1);
                }
                now
            }
        }

        struct DeadlineCrossingClock {
            origin: Instant,
            calls: AtomicUsize,
        }

        impl DeadlineCrossingClock {
            fn new() -> Self {
                Self {
                    origin: Instant::now(),
                    calls: AtomicUsize::new(0),
                }
            }
        }

        impl MonotonicClock for DeadlineCrossingClock {
            fn now(&self) -> Instant {
                if self.calls.fetch_add(1, Ordering::AcqRel) >= 9 {
                    self.origin + Duration::from_secs(61)
                } else {
                    self.origin
                }
            }
        }

        struct MismatchedRuntime;

        impl ExcelWorkerRuntime for MismatchedRuntime {
            fn launch(
                &self,
                exporter: Arc<dyn WorksheetPdfExporter>,
                request: ExcelExportRequest,
            ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
                let request_id = request.request_id;
                let generation = request.generation;
                let result = exporter.export_pdf(request);
                Ok(Box::new(ImmediateWorker(Some(WorkerResponse {
                    request_id,
                    generation: generation + 1,
                    result,
                }))))
            }
        }

        struct ImmediateRuntime;

        impl ExcelWorkerRuntime for ImmediateRuntime {
            fn launch(
                &self,
                exporter: Arc<dyn WorksheetPdfExporter>,
                request: ExcelExportRequest,
            ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
                let request_id = request.request_id;
                let generation = request.generation;
                let result = exporter.export_pdf(request);
                Ok(Box::new(ImmediateWorker(Some(WorkerResponse {
                    request_id,
                    generation,
                    result,
                }))))
            }
        }

        struct ImmediateWorker(Option<WorkerResponse>);

        impl ExcelWorkerHandle for ImmediateWorker {
            fn poll(&mut self, _wait: Duration) -> WorkerPoll {
                self.0
                    .take()
                    .map(|response| WorkerPoll::Response(Box::new(response)))
                    .unwrap_or(WorkerPoll::Disconnected)
            }

            fn join_status(&mut self) -> WorkerJoin {
                WorkerJoin::Finished
            }
        }

        struct FailedJoinRuntime;

        impl ExcelWorkerRuntime for FailedJoinRuntime {
            fn launch(
                &self,
                exporter: Arc<dyn WorksheetPdfExporter>,
                request: ExcelExportRequest,
            ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
                let request_id = request.request_id;
                let generation = request.generation;
                let result = exporter.export_pdf(request);
                Ok(Box::new(FailedJoinWorker {
                    response: Some(WorkerResponse {
                        request_id,
                        generation,
                        result,
                    }),
                }))
            }
        }

        struct FailedJoinWorker {
            response: Option<WorkerResponse>,
        }

        impl ExcelWorkerHandle for FailedJoinWorker {
            fn poll(&mut self, _wait: Duration) -> WorkerPoll {
                self.response
                    .take()
                    .map(|response| WorkerPoll::Response(Box::new(response)))
                    .unwrap_or(WorkerPoll::Disconnected)
            }

            fn join_status(&mut self) -> WorkerJoin {
                WorkerJoin::Failed
            }
        }

        struct LateResponseRuntime {
            clock: Arc<AdvancingClock>,
        }

        impl ExcelWorkerRuntime for LateResponseRuntime {
            fn launch(
                &self,
                exporter: Arc<dyn WorksheetPdfExporter>,
                request: ExcelExportRequest,
            ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
                self.clock.advance();
                let request_id = request.request_id;
                let generation = request.generation;
                let result = exporter.export_pdf(request);
                Ok(Box::new(ImmediateWorker(Some(WorkerResponse {
                    request_id,
                    generation,
                    result,
                }))))
            }
        }

        struct CancellingResponseRuntime {
            cancellations: Arc<CancellationRegistry>,
            request_id: &'static str,
        }

        impl ExcelWorkerRuntime for CancellingResponseRuntime {
            fn launch(
                &self,
                exporter: Arc<dyn WorksheetPdfExporter>,
                request: ExcelExportRequest,
            ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
                let request_id = request.request_id;
                let generation = request.generation;
                let result = exporter.export_pdf(request);
                assert!(self.cancellations.cancel(self.request_id));
                Ok(Box::new(ImmediateWorker(Some(WorkerResponse {
                    request_id,
                    generation,
                    result,
                }))))
            }
        }

        struct CancellingFailedJoinRuntime {
            cancellations: Arc<CancellationRegistry>,
            request_id: &'static str,
        }

        impl ExcelWorkerRuntime for CancellingFailedJoinRuntime {
            fn launch(
                &self,
                exporter: Arc<dyn WorksheetPdfExporter>,
                request: ExcelExportRequest,
            ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
                let request_id = request.request_id;
                let generation = request.generation;
                let result = exporter.export_pdf(request);
                assert!(self.cancellations.cancel(self.request_id));
                Ok(Box::new(FailedJoinWorker {
                    response: Some(WorkerResponse {
                        request_id,
                        generation,
                        result,
                    }),
                }))
            }
        }

        struct LateFailedJoinRuntime {
            clock: Arc<AdvancingClock>,
        }

        impl ExcelWorkerRuntime for LateFailedJoinRuntime {
            fn launch(
                &self,
                exporter: Arc<dyn WorksheetPdfExporter>,
                request: ExcelExportRequest,
            ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
                self.clock.advance();
                let request_id = request.request_id;
                let generation = request.generation;
                let result = exporter.export_pdf(request);
                Ok(Box::new(FailedJoinWorker {
                    response: Some(WorkerResponse {
                        request_id,
                        generation,
                        result,
                    }),
                }))
            }
        }

        struct StuckRuntime {
            clock: Arc<AdvancingClock>,
        }

        impl ExcelWorkerRuntime for StuckRuntime {
            fn launch(
                &self,
                _exporter: Arc<dyn WorksheetPdfExporter>,
                _request: ExcelExportRequest,
            ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
                self.clock.advance();
                Ok(Box::new(StuckWorker))
            }
        }

        struct StuckWorker;

        impl ExcelWorkerHandle for StuckWorker {
            fn poll(&mut self, _wait: Duration) -> WorkerPoll {
                WorkerPoll::Pending
            }

            fn join_status(&mut self) -> WorkerJoin {
                WorkerJoin::Pending
            }
        }

        impl ControlledPdfSubmitter for RecordingSubmitter {
            fn submit_pdf(
                &self,
                job: &PrintJob,
                pdf: &ExportedWorksheetPdf,
                cancellation: crate::progress::CancellationToken,
            ) -> Result<(), PrintFailure> {
                assert!(!cancellation.is_cancelled());
                self.events.lock().unwrap().push("submit");
                self.submitted.lock().unwrap().push((
                    job.worksheet.as_ref().unwrap().name.clone(),
                    job.page_range.clone(),
                    std::fs::read(pdf.pdf_path()).unwrap(),
                ));
                Ok(())
            }
        }

        impl ControlledPdfSubmitter for OutcomeSubmitter {
            fn submit_pdf(
                &self,
                _job: &PrintJob,
                _pdf: &ExportedWorksheetPdf,
                _cancellation: crate::progress::CancellationToken,
            ) -> Result<(), PrintFailure> {
                self.events.lock().unwrap().push("submit");
                self.outcome.clone()
            }
        }

        fn descriptor(name: &str, workbook_index: u32) -> WorksheetDescriptor {
            WorksheetDescriptor {
                workbook_index,
                sheet_id: workbook_index,
                relationship_id: format!("rId{workbook_index}"),
                relationship_type: "worksheet".to_string(),
                part_path: format!("xl/worksheets/sheet{workbook_index}.xml"),
                name: name.to_string(),
                visibility: WorksheetVisibility::Visible,
            }
        }

        fn safety() -> WorkbookSafetyFacts {
            WorkbookSafetyFacts::default()
        }

        fn worksheet_job(
            source: &std::path::Path,
            signature: FileSignature,
            name: &str,
            workbook_index: u32,
        ) -> PrintJob {
            PrintJob {
                job_id: format!("worksheet-{workbook_index}"),
                path: source.to_string_lossy().into_owned(),
                printer: "Office Printer".to_string(),
                verb: "docflow".to_string(),
                format: "XLSX".to_string(),
                page_range: "all".to_string(),
                signature,
                worksheet: Some(descriptor(name, workbook_index)),
                workbook_safety: Some(safety()),
                timeout_minutes: 1,
                extra: serde_json::Map::new(),
                ..PrintJob::default()
            }
        }

        fn source_fixture() -> (
            tempfile::TempDir,
            crate::cap_fs::AnchoredPath,
            FileSignature,
        ) {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("book.xlsx");
            std::fs::write(&path, b"approved workbook bytes").unwrap();
            let root = CapabilityRoot::open(directory.path()).unwrap();
            let source = root.authorize(&path).unwrap().anchor().unwrap();
            let mut file = File::open(path).unwrap();
            let signature =
                crate::print_queue::file_signature_from_open_file(&mut file, true).unwrap();
            (directory, source, signature)
        }

        fn write_workbook(path: &Path) {
            const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
            const REL: &str =
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
            let workbook = format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="MEC" sheetId="17" r:id="rId1"/><sheet name="Hidden" sheetId="18" state="hidden" r:id="rId2"/><sheet name="CIV" sheetId="19" r:id="rId3"/></sheets></workbook>"#
            );
            let relationships = format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Type="{REL}" Target="worksheets/sheet3.xml"/></Relationships>"#
            );
            let package_relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
            let file = File::create(path).unwrap();
            let mut writer = ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            for (name, bytes) in [
                ("_rels/.rels", package_relationships.as_bytes()),
                ("xl/workbook.xml", workbook.as_bytes()),
                ("xl/_rels/workbook.xml.rels", relationships.as_bytes()),
            ] {
                writer.start_file(name, options).unwrap();
                writer.write_all(bytes).unwrap();
            }
            for index in 1..=3 {
                writer
                    .start_file(format!("xl/worksheets/sheet{index}.xml"), options)
                    .unwrap();
                writer
                    .write_all(b"<worksheet><sheetData/></worksheet>")
                    .unwrap();
            }
            writer.finish().unwrap();
        }

        #[test]
        fn production_guard_acquisitions_share_process_lifetime_state() {
            let first = process_excel_guards();
            let second = process_excel_guards();

            assert!(Arc::ptr_eq(&first.breaker, &second.breaker));
            assert!(Arc::ptr_eq(&first.operation, &second.operation));
            assert!(Arc::ptr_eq(&first.quarantine, &second.quarantine));
        }

        #[test]
        fn production_constructors_share_fence_runtime_ownership_and_breaker() {
            const CHILD_ENV: &str = "DOCFLOW_EXCEL_PROCESS_GUARD_CHILD";
            const TEST_NAME: &str = "platform::excel::tests::export_supervisor::production_constructors_share_fence_runtime_ownership_and_breaker";

            if std::env::var_os(CHILD_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
                let first_events = Arc::new(Mutex::new(Vec::new()));
                let second_events = Arc::new(Mutex::new(Vec::new()));
                let submitted = Arc::new(Mutex::new(Vec::new()));
                let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
                let staged_paths = Arc::new(Mutex::new(Vec::new()));
                let first_pipeline = Arc::new(ExcelExportPipeline::new(
                    Arc::new(RecordingPolicy {
                        events: Arc::clone(&first_events),
                    }),
                    Arc::new(OwnershipBlockingExporter {
                        events: Arc::clone(&first_events),
                        gate: Arc::clone(&gate),
                        staged_paths: Arc::clone(&staged_paths),
                    }),
                    Arc::new(RecordingSubmitter {
                        events: Arc::clone(&first_events),
                        submitted: Arc::clone(&submitted),
                    }),
                ));
                let (contended_sender, contended_receiver) = mpsc::sync_channel(1);
                let contended_sender = Arc::new(Mutex::new(Some(contended_sender)));
                let second_pipeline = Arc::new(
                    ExcelExportPipeline::new(
                        Arc::new(RecordingPolicy {
                            events: Arc::clone(&second_events),
                        }),
                        Arc::new(RecordingExporter {
                            events: Arc::clone(&second_events),
                            requests: Arc::new(Mutex::new(Vec::new())),
                        }),
                        Arc::new(RecordingSubmitter {
                            events: Arc::clone(&second_events),
                            submitted: Arc::clone(&submitted),
                        }),
                    )
                    .with_operation_contended_hook(Arc::new(move || {
                        if let Some(sender) = contended_sender.lock().unwrap().take() {
                            sender.send(()).unwrap();
                        }
                    })),
                );
                let cancellations = Arc::new(CancellationRegistry::default());
                let first_token = cancellations.register("production-first");
                let second_token = cancellations.register("production-second");
                let (first_directory, first_source, first_signature) = source_fixture();
                let first_job = worksheet_job(first_source.absolute(), first_signature, "MEC", 1);
                let first = std::thread::spawn(move || {
                    let _directory = first_directory;
                    let mut source_file = first_source.open_read().unwrap();
                    first_pipeline.export_and_submit(
                        &first_job,
                        &first_source,
                        &mut source_file,
                        first_token,
                    )
                });
                let (entered, changed) = &*gate;
                let entered = entered.lock().unwrap();
                let (entered, wait) = changed
                    .wait_timeout_while(entered, Duration::from_secs(2), |entered| !*entered)
                    .unwrap();
                assert!(
                    *entered && !wait.timed_out(),
                    "production worker did not enter"
                );
                drop(entered);
                let staged_path = staged_paths.lock().unwrap()[0].clone();
                assert!(staged_path.exists());

                let (second_directory, second_source, second_signature) = source_fixture();
                let second_job =
                    worksheet_job(second_source.absolute(), second_signature, "CIV", 2);
                let (result_sender, result_receiver) = mpsc::sync_channel(1);
                let second = std::thread::spawn(move || {
                    let _directory = second_directory;
                    let mut source_file = second_source.open_read().unwrap();
                    let result = second_pipeline.export_and_submit(
                        &second_job,
                        &second_source,
                        &mut source_file,
                        second_token,
                    );
                    result_sender.send(result).unwrap();
                });
                contended_receiver
                    .recv_timeout(Duration::from_secs(2))
                    .expect("production pipelines did not share the operation fence");
                assert!(second_events.lock().unwrap().is_empty());
                assert!(cancellations.cancel("production-second"));
                let second_failure = result_receiver
                    .recv_timeout(Duration::from_secs(2))
                    .expect("cancelled production waiter did not terminate")
                    .unwrap_err();
                second.join().unwrap();
                assert_eq!(
                    second_failure.kind,
                    crate::print_queue::PrintFailureKind::Cancelled
                );
                assert!(staged_path.exists());

                assert!(cancellations.cancel("production-first"));
                let first_failure = first.join().unwrap().unwrap_err();
                assert_eq!(
                    first_failure.kind,
                    crate::print_queue::PrintFailureKind::Cancelled
                );
                assert!(!staged_path.exists());

                let panic_events = Arc::new(Mutex::new(Vec::new()));
                let panic_paths = Arc::new(Mutex::new(Vec::new()));
                let panicking_pipeline = ExcelExportPipeline::new(
                    Arc::new(RecordingPolicy {
                        events: Arc::clone(&panic_events),
                    }),
                    Arc::new(PanickingExporter {
                        events: Arc::clone(&panic_events),
                        staged_paths: Arc::clone(&panic_paths),
                    }),
                    Arc::new(RecordingSubmitter {
                        events: Arc::clone(&panic_events),
                        submitted: Arc::clone(&submitted),
                    }),
                );
                let cancellation = CancellationRegistry::default().register("production-panic");
                let (_directory, source, signature) = source_fixture();
                let job = worksheet_job(source.absolute(), signature, "MEC", 1);
                let mut source_file = source.open_read().unwrap();
                let panic_failure = panicking_pipeline
                    .export_and_submit(&job, &source, &mut source_file, cancellation)
                    .unwrap_err();
                assert_eq!(
                    panic_failure.kind,
                    crate::print_queue::PrintFailureKind::Unsupported
                );
                assert!(!panic_paths.lock().unwrap()[0].exists());

                let rejected_events = Arc::new(Mutex::new(Vec::new()));
                let rejected = ExcelExportPipeline::new(
                    Arc::new(RecordingPolicy {
                        events: Arc::clone(&rejected_events),
                    }),
                    Arc::new(RecordingExporter {
                        events: Arc::clone(&rejected_events),
                        requests: Arc::new(Mutex::new(Vec::new())),
                    }),
                    Arc::new(RecordingSubmitter {
                        events: Arc::clone(&rejected_events),
                        submitted,
                    }),
                );
                let cancellation = CancellationRegistry::default().register("production-rejected");
                let (_directory, source, signature) = source_fixture();
                let job = worksheet_job(source.absolute(), signature, "MEC", 1);
                let mut source_file = source.open_read().unwrap();
                let rejected_failure = rejected
                    .export_and_submit(&job, &source, &mut source_file, cancellation)
                    .unwrap_err();
                assert_eq!(
                    rejected_failure.kind,
                    crate::print_queue::PrintFailureKind::Unsupported
                );
                assert!(rejected_events.lock().unwrap().is_empty());
                return;
            }

            let output = Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(TEST_NAME)
                .arg("--nocapture")
                .env(CHILD_ENV, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated production guard test failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[test]
        fn exports_enabled_worksheets_in_exact_order() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::new(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests: Arc::clone(&requests),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();

            for (name, index) in [("CIV", 2), ("MEC", 1)] {
                let job = worksheet_job(source.absolute(), signature.clone(), name, index);
                let mut source_file = source.open_read().unwrap();
                pipeline
                    .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
                    .unwrap();
            }

            assert_eq!(
                *events.lock().unwrap(),
                ["policy", "export", "submit", "policy", "export", "submit"]
            );
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), 2);
            assert_ne!(requests[0].request_id, requests[1].request_id);
            assert_eq!(requests[0].generation, 1);
            assert_eq!(requests[1].generation, 2);
            assert_eq!(requests[0].signature, signature);
            assert_eq!(requests[0].safety, safety());
            assert_eq!(requests[0].worksheet, descriptor("CIV", 2));
            assert!(requests[0].deadline > Instant::now());
            assert_eq!(requests[0].workbook_bytes, b"approved workbook bytes");
            assert!(!requests[0].cancelled);
            assert_eq!(
                *submitted.lock().unwrap(),
                [
                    ("CIV".to_string(), "all".to_string(), b"%PDF-1.4\n".to_vec()),
                    ("MEC".to_string(), "all".to_string(), b"%PDF-1.4\n".to_vec()),
                ]
            );
        }

        #[test]
        fn source_policy_receives_exact_anchored_identity_and_safety_facts() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let observations = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing(
                Arc::new(CapturingPolicy {
                    events: Arc::clone(&events),
                    observations: Arc::clone(&observations),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests,
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted,
                }),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let expected_safety = WorkbookSafetyFacts {
                has_vba_project: true,
                ..WorkbookSafetyFacts::default()
            };
            let mut job = worksheet_job(source.absolute(), signature, "MEC", 1);
            job.workbook_safety = Some(expected_safety.clone());
            let expected = PolicyObservation {
                absolute: source.absolute().to_path_buf(),
                identity: source.identity().unwrap(),
                safety: expected_safety,
            };
            let mut source_file = source.open_read().unwrap();

            pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap();

            assert_eq!(*observations.lock().unwrap(), [expected]);
            assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
        }

        #[test]
        fn source_policy_rejection_precedes_staging_and_worker_launch() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing(
                Arc::new(RejectingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests: Arc::clone(&requests),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(source_file.stream_position().unwrap(), 0);
            assert_eq!(*events.lock().unwrap(), ["policy"]);
            assert!(requests.lock().unwrap().is_empty());
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn non_all_range_is_rejected_before_policy_staging_or_worker_launch() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests: Arc::clone(&requests),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let mut job = worksheet_job(source.absolute(), signature, "MEC", 1);
            job.page_range = "1-2".to_string();
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::UnsupportedOptions
            );
            assert_eq!(source_file.stream_position().unwrap(), 0);
            assert!(events.lock().unwrap().is_empty());
            assert!(requests.lock().unwrap().is_empty());
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn artifact_drop_closes_anchors_and_root_before_workspace_cleanup() {
            let (staged, events) = StagedWorkbook::fixture_with_drop_probe();
            let exported = staged.fixture_exported_pdf();

            drop(exported);

            assert_eq!(
                *events.lock().unwrap(),
                ["pdf", "workbook", "root", "workspace"]
            );
        }

        #[test]
        fn cleanup_failure_retains_a_process_owned_workspace_locator() {
            let workspace = tempfile::tempdir().unwrap();
            let workspace_path = workspace.path().to_path_buf();
            let root = CapabilityRoot::open(&workspace_path).unwrap();
            let workbook_path = workspace_path.join("book.xlsx");
            let workbook = root.authorize(&workbook_path).unwrap().anchor().unwrap();
            workbook.write_no_clobber(b"private workbook").unwrap();
            let breaker = Arc::new(AtomicBool::new(false));
            let quarantine = Arc::new(Mutex::new(BTreeSet::new()));
            let mut staged = StagedWorkbook {
                workbook: Some(workbook),
                root: Some(root),
                workspace: Some(workspace),
                workbook_path,
                cleanup_breaker: Arc::clone(&breaker),
                cleanup_quarantine: Arc::clone(&quarantine),
                drop_events: None,
                cleanup_failure: true,
            };

            assert!(staged.cleanup().is_err());

            assert!(breaker.load(Ordering::Acquire));
            assert_eq!(
                quarantine
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>(),
                std::slice::from_ref(&workspace_path)
            );
            assert_eq!(
                std::fs::read(workspace_path.join("book.xlsx")).unwrap(),
                b"private workbook"
            );
            std::fs::remove_dir_all(&workspace_path).unwrap();
            quarantine
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clear();
        }

        #[test]
        fn generation_mismatch_destroys_pdf_without_submit() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests,
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                Arc::new(FixedClock(Instant::now())),
                Arc::new(MismatchedRuntime),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
            assert!(!failure.uncertain);
            assert!(submitted.lock().unwrap().is_empty());
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
        }

        #[test]
        fn deadline_crossing_during_source_revalidation_prevents_submit() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests,
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                Arc::new(DeadlineCrossingClock::new()),
                Arc::new(ImmediateRuntime),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
            assert_eq!(failure.message, "error.print.excelExportTimedOut");
            assert!(!failure.uncertain);
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn worker_join_failure_after_response_trips_breaker_without_submit() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests,
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                Arc::new(FixedClock(Instant::now())),
                Arc::new(FailedJoinRuntime),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);

            let mut source_file = source.open_read().unwrap();
            let first = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
                .unwrap_err();
            assert_eq!(
                first.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );

            let mut source_file = source.open_read().unwrap();
            let second = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();
            assert_eq!(
                second.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn real_worker_panic_disconnects_cleans_staging_and_trips_breaker() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let staged_paths = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(PanickingExporter {
                    events: Arc::clone(&events),
                    staged_paths: Arc::clone(&staged_paths),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);

            let mut source_file = source.open_read().unwrap();
            let first = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
                .unwrap_err();
            let mut source_file = source.open_read().unwrap();
            let second = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_eq!(
                first.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(
                second.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
            let staged_paths = staged_paths.lock().unwrap();
            assert_eq!(staged_paths.len(), 1);
            assert!(!staged_paths[0].exists());
        }

        #[test]
        fn pre_submit_cleanup_failure_trips_breaker_without_retry_or_submit() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(CleanupFailingExporter {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                Arc::new(FixedClock(Instant::now())),
                Arc::new(MismatchedRuntime),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);

            for _ in 0..2 {
                let mut source_file = source.open_read().unwrap();
                let failure = pipeline
                    .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
                    .unwrap_err();
                assert_eq!(
                    failure.kind,
                    crate::print_queue::PrintFailureKind::Unsupported
                );
                assert!(!failure.uncertain);
            }

            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
            pipeline.clear_test_quarantine();
        }

        #[test]
        fn exporter_error_cleanup_failure_trips_breaker_before_retry() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(ErrorWithCleanupFailureExporter {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);

            let mut source_file = source.open_read().unwrap();
            let first = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
                .unwrap_err();
            assert_eq!(
                first.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(first.message, "error.print.exportCleanupRequiresReview");

            let mut source_file = source.open_read().unwrap();
            let second = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_eq!(
                second.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
            pipeline.clear_test_quarantine();
        }

        #[test]
        fn post_submit_cleanup_failure_preserves_or_conservatively_classifies_effect() {
            let cases = [
                (
                    Ok(()),
                    PrintFailure::uncertain("error.print.exportCleanupRequiresReview"),
                ),
                (
                    Err(PrintFailure::uncertain("printer outcome unknown")),
                    PrintFailure::uncertain("printer outcome unknown"),
                ),
                (
                    Err(PrintFailure::cancelled("cancel outcome unknown", true)),
                    PrintFailure::cancelled("cancel outcome unknown", true),
                ),
                (
                    Err(PrintFailure::failed("printer rejected before effect")),
                    PrintFailure::unsupported("error.print.exportCleanupRequiresReview"),
                ),
            ];

            for (outcome, expected) in cases {
                let events = Arc::new(Mutex::new(Vec::new()));
                let pipeline = ExcelExportPipeline::for_testing(
                    Arc::new(RecordingPolicy {
                        events: Arc::clone(&events),
                    }),
                    Arc::new(CleanupFailingExporter {
                        events: Arc::clone(&events),
                    }),
                    Arc::new(OutcomeSubmitter {
                        events: Arc::clone(&events),
                        outcome,
                    }),
                );
                let cancellation = CancellationRegistry::default().register("plan");
                let (_directory, source, signature) = source_fixture();
                let job = worksheet_job(source.absolute(), signature, "MEC", 1);
                let mut source_file = source.open_read().unwrap();

                let actual = pipeline
                    .export_and_submit(&job, &source, &mut source_file, cancellation)
                    .unwrap_err();

                assert_eq!(actual, expected);
                assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
                pipeline.clear_test_quarantine();
            }
        }

        #[test]
        fn print_service_exports_only_enabled_rows_in_revised_order() {
            let directory = tempfile::tempdir().unwrap();
            let workbook = directory.path().join("book.xlsx");
            write_workbook(&workbook);
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let adapter = Arc::new(PreviewAdapter::default());
            let pipeline = Arc::new(ExcelExportPipeline::for_testing(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests: Arc::clone(&requests),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
            ));
            let service = PrintService::new(
                directory.path().join("queues"),
                directory.path().join("journals"),
                Duration::from_secs(300),
                GlobalMutationLock::default(),
                adapter.clone(),
            )
            .with_excel_pipeline(pipeline);
            let caller = CallerIdentity::new("operator", "session");
            let plan = service
                .preview(
                    &caller,
                    BatchPrintParams {
                        files: vec![workbook],
                        printer: "Office Printer".to_string(),
                        timeout_minutes: 1,
                        ..BatchPrintParams::default()
                    },
                    |_| {},
                )
                .unwrap();
            let rows = plan.meta["editor"]["rows"].as_array().unwrap();
            let revised = service
                .revise(
                    &caller,
                    &plan.plan_id,
                    &[rows[2].clone(), rows[1].clone(), rows[0].clone()],
                )
                .unwrap();

            let result = service.apply(&revised.plan_id, &caller, |_| {}).unwrap();

            assert_eq!(result.applied, 2);
            assert_eq!(
                requests
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|request| request.worksheet.name.as_str())
                    .collect::<Vec<_>>(),
                ["CIV", "MEC"]
            );
            assert_eq!(
                submitted
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(name, range, _)| (name.as_str(), range.as_str()))
                    .collect::<Vec<_>>(),
                [("CIV", "all"), ("MEC", "all")]
            );
            assert!(adapter.native_submits.lock().unwrap().is_empty());
        }

        #[test]
        fn print_service_source_mutation_after_export_blocks_all_submission() {
            let directory = tempfile::tempdir().unwrap();
            let workbook = directory.path().join("book.xlsx");
            write_workbook(&workbook);
            let events = Arc::new(Mutex::new(Vec::new()));
            let staged_paths = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let adapter = Arc::new(PreviewAdapter::default());
            let pipeline = Arc::new(ExcelExportPipeline::for_testing(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(SourceMutatingExporter {
                    events: Arc::clone(&events),
                    source_path: workbook.clone(),
                    staged_paths: Arc::clone(&staged_paths),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
            ));
            let service = PrintService::new(
                directory.path().join("queues"),
                directory.path().join("journals"),
                Duration::from_secs(300),
                GlobalMutationLock::default(),
                adapter.clone(),
            )
            .with_excel_pipeline(pipeline);
            let caller = CallerIdentity::new("operator", "session");
            let plan = service
                .preview(
                    &caller,
                    BatchPrintParams {
                        files: vec![workbook],
                        printer: "Office Printer".to_string(),
                        timeout_minutes: 1,
                        ..BatchPrintParams::default()
                    },
                    |_| {},
                )
                .unwrap();
            let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
            rows[1]["enabled"] = serde_json::Value::Bool(false);
            rows[2]["enabled"] = serde_json::Value::Bool(false);
            let revised = service.revise(&caller, &plan.plan_id, &rows).unwrap();

            let result = service.apply(&revised.plan_id, &caller, |_| {}).unwrap();

            assert_eq!((result.applied, result.failed), (0, 1));
            assert_eq!(result.errors[0].key, "error.print.workbookChanged");
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
            assert!(adapter.native_submits.lock().unwrap().is_empty());
            let staged_paths = staged_paths.lock().unwrap();
            assert_eq!(staged_paths.len(), 1);
            assert!(!staged_paths[0].exists());
        }

        #[test]
        fn print_service_cancel_reaches_blocked_export_token_cleanly() {
            let directory = tempfile::tempdir().unwrap();
            let workbook = directory.path().join("book.xlsx");
            write_workbook(&workbook);
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
            let adapter = Arc::new(PreviewAdapter::default());
            let pipeline = Arc::new(ExcelExportPipeline::for_testing(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(CancellationBlockingExporter {
                    events: Arc::clone(&events),
                    gate: Arc::clone(&gate),
                }),
                Arc::new(RecordingSubmitter {
                    events,
                    submitted: Arc::clone(&submitted),
                }),
            ));
            let service = Arc::new(
                PrintService::new(
                    directory.path().join("queues"),
                    directory.path().join("journals"),
                    Duration::from_secs(300),
                    GlobalMutationLock::default(),
                    adapter,
                )
                .with_excel_pipeline(pipeline),
            );
            let caller = CallerIdentity::new("operator", "session");
            let plan = service
                .preview(
                    &caller,
                    BatchPrintParams {
                        files: vec![workbook],
                        printer: "Office Printer".to_string(),
                        timeout_minutes: 1,
                        ..BatchPrintParams::default()
                    },
                    |_| {},
                )
                .unwrap();
            let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
            rows[1]["enabled"] = serde_json::Value::Bool(false);
            rows[2]["enabled"] = serde_json::Value::Bool(false);
            let revised = service.revise(&caller, &plan.plan_id, &rows).unwrap();
            let plan_id = revised.plan_id.clone();
            let apply_service = Arc::clone(&service);
            let apply_caller = caller.clone();
            let apply_plan_id = plan_id.clone();
            let apply = std::thread::spawn(move || {
                apply_service.apply(&apply_plan_id, &apply_caller, |_| {})
            });
            let (entered, changed) = &*gate;
            let entered = entered.lock().unwrap();
            let (entered, wait) = changed
                .wait_timeout_while(entered, Duration::from_secs(2), |entered| !*entered)
                .unwrap();
            assert!(
                *entered && !wait.timed_out(),
                "Excel exporter did not enter"
            );
            drop(entered);

            assert!(service.cancel(&plan_id));
            let result = apply.join().unwrap().unwrap();
            let queue_id = result.meta["queueId"].as_str().unwrap();
            let saved: serde_json::Value = serde_json::from_slice(
                &std::fs::read(
                    directory
                        .path()
                        .join("queues")
                        .join(format!("{queue_id}.json")),
                )
                .unwrap(),
            )
            .unwrap();

            assert_eq!(saved["jobs"][0]["status"], "cancelled");
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn distinct_pipelines_share_the_process_operation_fence() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
            let breaker = Arc::new(AtomicBool::new(false));
            let operation = Arc::new(Mutex::new(()));
            let quarantine = Arc::new(Mutex::new(BTreeSet::new()));
            let policy: Arc<dyn WorkbookSourcePolicy> = Arc::new(RecordingPolicy {
                events: Arc::clone(&events),
            });
            let exporter: Arc<dyn WorksheetPdfExporter> = Arc::new(CancellationBlockingExporter {
                events: Arc::clone(&events),
                gate: Arc::clone(&gate),
            });
            let submitter: Arc<dyn ControlledPdfSubmitter> = Arc::new(RecordingSubmitter {
                events: Arc::clone(&events),
                submitted: Arc::clone(&submitted),
            });
            let first_pipeline = Arc::new(ExcelExportPipeline::with_breaker(
                Arc::clone(&policy),
                Arc::clone(&exporter),
                Arc::clone(&submitter),
                ExcelProcessGuards {
                    breaker: Arc::clone(&breaker),
                    operation: Arc::clone(&operation),
                    quarantine: Arc::clone(&quarantine),
                },
                Arc::new(SystemMonotonicClock),
                Arc::new(ThreadWorkerRuntime),
                Duration::from_secs(2),
            ));
            let (contended_sender, contended_receiver) = mpsc::sync_channel(1);
            let contended_sender = Arc::new(Mutex::new(Some(contended_sender)));
            let second_pipeline = Arc::new(
                ExcelExportPipeline::with_breaker(
                    policy,
                    exporter,
                    submitter,
                    ExcelProcessGuards {
                        breaker,
                        operation,
                        quarantine,
                    },
                    Arc::new(SystemMonotonicClock),
                    Arc::new(ThreadWorkerRuntime),
                    Duration::from_secs(2),
                )
                .with_operation_contended_hook(Arc::new(move || {
                    if let Some(sender) = contended_sender.lock().unwrap().take() {
                        sender.send(()).unwrap();
                    }
                })),
            );
            let cancellations = Arc::new(CancellationRegistry::default());
            let first_token = cancellations.register("first");
            let second_token = cancellations.register("second");
            let (first_directory, first_source, first_signature) = source_fixture();
            let first_job = worksheet_job(first_source.absolute(), first_signature, "MEC", 1);
            let first_pipeline = Arc::clone(&first_pipeline);
            let first = std::thread::spawn(move || {
                let _directory = first_directory;
                let mut source_file = first_source.open_read().unwrap();
                first_pipeline.export_and_submit(
                    &first_job,
                    &first_source,
                    &mut source_file,
                    first_token,
                )
            });
            let (entered, changed) = &*gate;
            let entered = entered.lock().unwrap();
            let (entered, wait) = changed
                .wait_timeout_while(entered, Duration::from_secs(2), |entered| !*entered)
                .unwrap();
            assert!(
                *entered && !wait.timed_out(),
                "first exporter did not enter"
            );
            drop(entered);

            let (second_directory, second_source, second_signature) = source_fixture();
            let second_job = worksheet_job(second_source.absolute(), second_signature, "CIV", 2);
            let second_pipeline = Arc::clone(&second_pipeline);
            let second = std::thread::spawn(move || {
                let _directory = second_directory;
                let mut source_file = second_source.open_read().unwrap();
                second_pipeline.export_and_submit(
                    &second_job,
                    &second_source,
                    &mut source_file,
                    second_token,
                )
            });
            contended_receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("second pipeline did not contend for the shared operation");
            let events_while_first_live = events.lock().unwrap().clone();

            assert!(cancellations.cancel("first"));
            assert!(cancellations.cancel("second"));
            let first_failure = first.join().unwrap().unwrap_err();
            let second_failure = second.join().unwrap().unwrap_err();

            assert_eq!(
                events_while_first_live,
                ["policy", "export"],
                "a second request reached policy/export while the first generation was live"
            );
            assert_eq!(
                first_failure.kind,
                crate::print_queue::PrintFailureKind::Cancelled
            );
            assert_eq!(
                second_failure.kind,
                crate::print_queue::PrintFailureKind::Cancelled
            );
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn cancelled_waiter_exits_before_the_live_operation_is_released() {
            let first_events = Arc::new(Mutex::new(Vec::new()));
            let second_events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
            let breaker = Arc::new(AtomicBool::new(false));
            let operation = Arc::new(Mutex::new(()));
            let quarantine = Arc::new(Mutex::new(BTreeSet::new()));
            let first_pipeline = Arc::new(ExcelExportPipeline::with_breaker(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&first_events),
                }),
                Arc::new(CancellationBlockingExporter {
                    events: Arc::clone(&first_events),
                    gate: Arc::clone(&gate),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&first_events),
                    submitted: Arc::clone(&submitted),
                }),
                ExcelProcessGuards {
                    breaker: Arc::clone(&breaker),
                    operation: Arc::clone(&operation),
                    quarantine: Arc::clone(&quarantine),
                },
                Arc::new(SystemMonotonicClock),
                Arc::new(ThreadWorkerRuntime),
                Duration::from_secs(2),
            ));
            let (attempted_sender, attempted_receiver) = mpsc::sync_channel(1);
            let attempted_sender = Arc::new(Mutex::new(Some(attempted_sender)));
            let second_pipeline = Arc::new(
                ExcelExportPipeline::with_breaker(
                    Arc::new(RecordingPolicy {
                        events: Arc::clone(&second_events),
                    }),
                    Arc::new(RecordingExporter {
                        events: Arc::clone(&second_events),
                        requests: Arc::new(Mutex::new(Vec::new())),
                    }),
                    Arc::new(RecordingSubmitter {
                        events: Arc::clone(&second_events),
                        submitted: Arc::clone(&submitted),
                    }),
                    ExcelProcessGuards {
                        breaker,
                        operation,
                        quarantine,
                    },
                    Arc::new(SystemMonotonicClock),
                    Arc::new(ThreadWorkerRuntime),
                    Duration::from_secs(2),
                )
                .with_operation_contended_hook(Arc::new(move || {
                    if let Some(sender) = attempted_sender.lock().unwrap().take() {
                        sender.send(()).unwrap();
                    }
                })),
            );
            let cancellations = Arc::new(CancellationRegistry::default());
            let first_token = cancellations.register("first");
            let second_token = cancellations.register("second");
            let (first_directory, first_source, first_signature) = source_fixture();
            let first_job = worksheet_job(first_source.absolute(), first_signature, "MEC", 1);
            let first = std::thread::spawn(move || {
                let _directory = first_directory;
                let mut source_file = first_source.open_read().unwrap();
                first_pipeline.export_and_submit(
                    &first_job,
                    &first_source,
                    &mut source_file,
                    first_token,
                )
            });
            let (entered, changed) = &*gate;
            let entered = entered.lock().unwrap();
            let (entered, wait) = changed
                .wait_timeout_while(entered, Duration::from_secs(2), |entered| !*entered)
                .unwrap();
            assert!(
                *entered && !wait.timed_out(),
                "first exporter did not enter"
            );
            drop(entered);

            let (second_directory, second_source, second_signature) = source_fixture();
            let second_job = worksheet_job(second_source.absolute(), second_signature, "CIV", 2);
            let (result_sender, result_receiver) = mpsc::sync_channel(1);
            let second = std::thread::spawn(move || {
                let _directory = second_directory;
                let mut source_file = second_source.open_read().unwrap();
                let result = second_pipeline.export_and_submit(
                    &second_job,
                    &second_source,
                    &mut source_file,
                    second_token,
                );
                result_sender.send(result).unwrap();
            });
            attempted_receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("second pipeline did not contend for the live operation");
            assert!(cancellations.cancel("second"));
            let result_before_release = result_receiver.recv_timeout(Duration::from_millis(250));

            assert!(cancellations.cancel("first"));
            let first_failure = first.join().unwrap().unwrap_err();
            let returned_before_release = result_before_release.is_ok();
            let second_result = match result_before_release {
                Ok(result) => result,
                Err(_) => result_receiver
                    .recv_timeout(Duration::from_secs(2))
                    .expect("second pipeline did not terminate after release"),
            };
            second.join().unwrap();
            let second_failure = second_result.unwrap_err();

            assert!(
                returned_before_release,
                "cancelled waiter remained blocked on the live operation"
            );
            assert_eq!(
                first_failure.kind,
                crate::print_queue::PrintFailureKind::Cancelled
            );
            assert_eq!(
                second_failure.kind,
                crate::print_queue::PrintFailureKind::Cancelled
            );
            assert!(second_events.lock().unwrap().is_empty());
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn operation_acquisition_deadline_expires_before_policy_or_staging() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let operation = Arc::new(Mutex::new(()));
            let held_operation = operation.lock().unwrap();
            let pipeline = Arc::new(ExcelExportPipeline::with_breaker(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests: Arc::new(Mutex::new(Vec::new())),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                ExcelProcessGuards {
                    breaker: Arc::new(AtomicBool::new(false)),
                    operation: Arc::clone(&operation),
                    quarantine: Arc::new(Mutex::new(BTreeSet::new())),
                },
                Arc::new(SystemMonotonicClock),
                Arc::new(ThreadWorkerRuntime),
                Duration::from_secs(2),
            ));
            let cancellation = CancellationRegistry::default().register("deadline");
            let (directory, source, signature) = source_fixture();
            let mut job = worksheet_job(source.absolute(), signature, "MEC", 1);
            job.timeout_minutes = 0;
            let (result_sender, result_receiver) = mpsc::sync_channel(1);
            let call = std::thread::spawn(move || {
                let _directory = directory;
                let mut source_file = source.open_read().unwrap();
                let result =
                    pipeline.export_and_submit(&job, &source, &mut source_file, cancellation);
                result_sender.send(result).unwrap();
            });

            let result_before_release = result_receiver.recv_timeout(Duration::from_millis(250));
            let returned_before_release = result_before_release.is_ok();
            drop(held_operation);
            let result = match result_before_release {
                Ok(result) => result,
                Err(_) => result_receiver
                    .recv_timeout(Duration::from_secs(2))
                    .expect("deadline waiter did not terminate after lock release"),
            };
            call.join().unwrap();
            let failure = result.unwrap_err();

            assert!(
                returned_before_release,
                "deadline waiter remained blocked on the operation mutex"
            );
            assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
            assert_eq!(failure.message, "error.print.excelExportTimedOut");
            assert!(events.lock().unwrap().is_empty());
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn breaker_trip_in_one_pipeline_rejects_a_distinct_pipeline() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let breaker = Arc::new(AtomicBool::new(false));
            let operation = Arc::new(Mutex::new(()));
            let quarantine = Arc::new(Mutex::new(BTreeSet::new()));
            let policy: Arc<dyn WorkbookSourcePolicy> = Arc::new(RecordingPolicy {
                events: Arc::clone(&events),
            });
            let exporter: Arc<dyn WorksheetPdfExporter> = Arc::new(RecordingExporter {
                events: Arc::clone(&events),
                requests,
            });
            let submitter: Arc<dyn ControlledPdfSubmitter> = Arc::new(RecordingSubmitter {
                events: Arc::clone(&events),
                submitted: Arc::clone(&submitted),
            });
            let first = ExcelExportPipeline::with_breaker(
                Arc::clone(&policy),
                Arc::clone(&exporter),
                Arc::clone(&submitter),
                ExcelProcessGuards {
                    breaker: Arc::clone(&breaker),
                    operation: Arc::clone(&operation),
                    quarantine: Arc::clone(&quarantine),
                },
                Arc::new(FixedClock(Instant::now())),
                Arc::new(FailedJoinRuntime),
                Duration::from_secs(1),
            );
            let second = ExcelExportPipeline::with_breaker(
                policy,
                exporter,
                submitter,
                ExcelProcessGuards {
                    breaker,
                    operation,
                    quarantine,
                },
                Arc::new(FixedClock(Instant::now())),
                Arc::new(ImmediateRuntime),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);

            let mut source_file = source.open_read().unwrap();
            let first_failure = first
                .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
                .unwrap_err();
            let mut source_file = source.open_read().unwrap();
            let second_failure = second
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_eq!(
                first_failure.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(
                second_failure.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn panic_unwind_cleanup_failure_outranks_failed_join_quarantine() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let staged_paths = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(PanickingCleanupFailingExporter {
                    events: Arc::clone(&events),
                    staged_paths: Arc::clone(&staged_paths),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
            );
            let cancellation = CancellationRegistry::default().register("cleanup-panic-join");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_eq!(staged_paths.lock().unwrap().len(), 1);
            assert!(staged_paths.lock().unwrap()[0].exists());
            assert_cleanup_review_failure(&pipeline, &failure, &events, &submitted);
        }

        #[test]
        fn error_response_cleanup_failure_outranks_failed_join_quarantine() {
            const REQUEST_ID: &str = "cleanup-error-failed-join";
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let cancellations = Arc::new(CancellationRegistry::default());
            let cancellation = cancellations.register(REQUEST_ID);
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(ErrorWithCleanupFailureExporter {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                Arc::new(SystemMonotonicClock),
                Arc::new(CancellingFailedJoinRuntime {
                    cancellations,
                    request_id: REQUEST_ID,
                }),
                Duration::from_secs(1),
            );
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_cleanup_review_failure(&pipeline, &failure, &events, &submitted);
        }

        #[test]
        fn received_response_cleanup_failure_outranks_failed_join() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(CleanupFailingExporter {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                Arc::new(SystemMonotonicClock),
                Arc::new(FailedJoinRuntime),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("cleanup-response-join");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_cleanup_review_failure(&pipeline, &failure, &events, &submitted);
        }

        #[test]
        fn cancelled_failed_join_loser_cleanup_failure_outranks_quarantine() {
            const REQUEST_ID: &str = "cleanup-cancel-failed-join";
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let cancellations = Arc::new(CancellationRegistry::default());
            let cancellation = cancellations.register(REQUEST_ID);
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(CleanupFailingExporter {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                Arc::new(SystemMonotonicClock),
                Arc::new(CancellingFailedJoinRuntime {
                    cancellations,
                    request_id: REQUEST_ID,
                }),
                Duration::from_secs(1),
            );
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_cleanup_review_failure(&pipeline, &failure, &events, &submitted);
        }

        #[test]
        fn timeout_failed_join_loser_cleanup_failure_outranks_quarantine() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let clock = Arc::new(AdvancingClock::new());
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(CleanupFailingExporter {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                clock.clone(),
                Arc::new(LateFailedJoinRuntime { clock }),
                Duration::from_secs(1),
            );
            let cancellation =
                CancellationRegistry::default().register("cleanup-timeout-failed-join");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();

            assert_cleanup_review_failure(&pipeline, &failure, &events, &submitted);
        }

        fn assert_cleanup_review_failure(
            pipeline: &ExcelExportPipeline,
            failure: &PrintFailure,
            events: &Arc<Mutex<Vec<&'static str>>>,
            submitted: &SubmittedPdfs,
        ) {
            let retained = pipeline
                .quarantine
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(failure.message, "error.print.exportCleanupRequiresReview");
            assert!(pipeline.breaker.load(Ordering::Acquire));
            assert_eq!(retained.len(), 1);
            assert!(retained[0].exists());
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
            pipeline.clear_test_quarantine();
        }

        #[test]
        fn completed_timeout_loser_surfaces_queued_artifact_cleanup_failure() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let clock = Arc::new(AdvancingClock::new());
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(CleanupFailingExporter {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                clock.clone(),
                Arc::new(LateResponseRuntime { clock }),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("cleanup-timeout");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();
            let retained = pipeline
                .quarantine
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .cloned()
                .collect::<Vec<_>>();

            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(failure.message, "error.print.exportCleanupRequiresReview");
            assert!(pipeline.breaker.load(Ordering::Acquire));
            assert_eq!(retained.len(), 1);
            assert!(retained[0].exists());
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
            pipeline.clear_test_quarantine();
        }

        #[test]
        fn completed_cancelled_loser_surfaces_queued_artifact_cleanup_failure() {
            const REQUEST_ID: &str = "cleanup-cancel";
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let cancellations = Arc::new(CancellationRegistry::default());
            let cancellation = cancellations.register(REQUEST_ID);
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(CleanupFailingExporter {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                Arc::new(SystemMonotonicClock),
                Arc::new(CancellingResponseRuntime {
                    cancellations,
                    request_id: REQUEST_ID,
                }),
                Duration::from_secs(1),
            );
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);
            let mut source_file = source.open_read().unwrap();

            let failure = pipeline
                .export_and_submit(&job, &source, &mut source_file, cancellation)
                .unwrap_err();
            let retained = pipeline
                .quarantine
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .cloned()
                .collect::<Vec<_>>();

            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(failure.message, "error.print.exportCleanupRequiresReview");
            assert!(pipeline.breaker.load(Ordering::Acquire));
            assert_eq!(retained.len(), 1);
            assert!(retained[0].exists());
            assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
            assert!(submitted.lock().unwrap().is_empty());
            pipeline.clear_test_quarantine();
        }

        #[test]
        fn timeout_late_result_is_destroyed_and_retry_stays_bounded() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let clock = Arc::new(AdvancingClock::new());
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests: Arc::clone(&requests),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                clock.clone(),
                Arc::new(LateResponseRuntime { clock }),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);

            for _ in 0..2 {
                let mut source_file = source.open_read().unwrap();
                let failure = pipeline
                    .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
                    .unwrap_err();
                assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
                assert!(!failure.uncertain);
            }

            assert_eq!(requests.lock().unwrap().len(), 2);
            assert!(submitted.lock().unwrap().is_empty());
        }

        #[test]
        fn timeout_live_loser_trips_one_way_breaker() {
            let events = Arc::new(Mutex::new(Vec::new()));
            let submitted = Arc::new(Mutex::new(Vec::new()));
            let clock = Arc::new(AdvancingClock::new());
            let pipeline = ExcelExportPipeline::for_testing_with_runtime(
                Arc::new(RecordingPolicy {
                    events: Arc::clone(&events),
                }),
                Arc::new(RecordingExporter {
                    events: Arc::clone(&events),
                    requests: Arc::new(Mutex::new(Vec::new())),
                }),
                Arc::new(RecordingSubmitter {
                    events: Arc::clone(&events),
                    submitted: Arc::clone(&submitted),
                }),
                clock.clone(),
                Arc::new(StuckRuntime { clock }),
                Duration::from_secs(1),
            );
            let cancellation = CancellationRegistry::default().register("plan");
            let (_directory, source, signature) = source_fixture();
            let job = worksheet_job(source.absolute(), signature, "MEC", 1);

            for _ in 0..2 {
                let mut source_file = source.open_read().unwrap();
                let failure = pipeline
                    .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
                    .unwrap_err();
                assert_eq!(
                    failure.kind,
                    crate::print_queue::PrintFailureKind::Unsupported
                );
                assert!(!failure.uncertain);
            }

            assert_eq!(*events.lock().unwrap(), ["policy"]);
            assert!(submitted.lock().unwrap().is_empty());
        }
    }
}
