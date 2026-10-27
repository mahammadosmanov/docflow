use std::{
    collections::BTreeSet,
    fs::File,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, TryLockError,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use uuid::Uuid;

mod staged;
use staged::{ExportedWorksheetPdf, StagedWorkbook, read_pdf_bytes_bounded};

use crate::{
    cap_fs::AnchoredPath,
    ooxml::{WorkbookSafetyFacts, WorksheetDescriptor},
    print_queue::{
        CleanupReview, PrintFailure, PrintJob, PrintSubmissionOutcome, RetryDisposition,
        file_signature_from_open_file,
    },
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
    pub(crate) safety: WorkbookSafetyFacts,
    pub(crate) worksheet: WorksheetDescriptor,
    pub(crate) deadline: Instant,
    pub(crate) cancellation: CancellationToken,
}

impl ExcelExportRequest {
    fn cleanup_review(&self) -> CleanupReview {
        CleanupReview {
            warning_key: "error.print.exportCleanupRequiresReview",
            locator: self.staged_workbook.cleanup_locator(),
        }
    }
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
        deadline: Instant,
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
            .map_err(|_| PrintFailure::failed("error.print.excelWorkerStartFailed"))?;
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for path in retained {
            if path.exists() {
                std::fs::remove_dir_all(&path).expect("test quarantine cleanup must succeed");
            }
            self.quarantine
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&path);
        }
    }

    pub(crate) fn export_and_submit(
        &self,
        job: &PrintJob,
        source: &AnchoredPath,
        source_file: &mut File,
        cancellation: CancellationToken,
    ) -> Result<PrintSubmissionOutcome, PrintFailure> {
        let deadline = self
            .clock
            .now()
            .checked_add(Duration::from_secs(u64::from(job.timeout_minutes) * 60))
            .ok_or_else(|| PrintFailure::unsupported("error.print.excelDeadlineInvalid"))?;
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
        let worksheet = job.worksheet.clone().ok_or_else(|| {
            PrintFailure::unsupported("error.print.excelWorksheetMetadataMissing")
        })?;
        let safety = job
            .workbook_safety
            .clone()
            .ok_or_else(|| PrintFailure::unsupported("error.print.excelSafetyMetadataMissing"))?;
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
        let previous_generation = if let Ok(generation) =
            self.generation
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
                    generation.checked_add(1)
                }) {
            generation
        } else {
            drop(staged_workbook);
            return Err(self.prefer_cleanup_review(PrintFailure::unsupported(
                "error.print.excelGenerationExhausted",
            )));
        };
        let generation = previous_generation + 1;
        let request_id = Uuid::new_v4();
        let request = ExcelExportRequest {
            request_id,
            generation,
            staged_workbook,
            safety,
            worksheet,
            deadline,
            cancellation: cancellation.clone(),
        };
        let unproven_cleanup_review = request.cleanup_review();
        let mut worker = match self.runtime.launch(Arc::clone(&self.exporter), request) {
            Ok(worker) => worker,
            Err(failure) => return Err(self.prefer_cleanup_review(failure)),
        };
        let response = match self.wait_for_response(worker.as_mut(), deadline, &cancellation) {
            WorkerWait::Response(response) => response,
            WorkerWait::Cancelled => {
                self.invalidate_generation(generation);
                let teardown = self.teardown_loser(worker.as_mut());
                return Err(self.classify_loser(
                    teardown,
                    clean_cancelled(),
                    unproven_cleanup_review,
                ));
            }
            WorkerWait::TimedOut => {
                self.invalidate_generation(generation);
                let teardown = self.teardown_loser(worker.as_mut());
                return Err(self.classify_loser(
                    teardown,
                    PrintFailure::failed("error.print.excelExportTimedOut"),
                    unproven_cleanup_review,
                ));
            }
            WorkerWait::Disconnected => {
                let teardown = self.teardown_loser(worker.as_mut());
                return Err(self.classify_loser(
                    teardown,
                    PrintFailure::failed("error.print.excelWorkerStopped"),
                    unproven_cleanup_review,
                ));
            }
        };
        if !self.finish_worker(worker.as_mut()) {
            self.invalidate_generation(generation);
            let failure = match response.result {
                Ok(mut artifact) => {
                    let _ = artifact.cleanup();
                    quarantined_failure()
                }
                Err(failure) => failure,
            };
            self.trip_breaker();
            return Err(self.prefer_cleanup_review(failure));
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
                    PrintFailure::failed("error.print.excelResponseStale"),
                ),
                Err(_) => Err(self
                    .prefer_cleanup_review(PrintFailure::failed("error.print.excelResponseStale"))),
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
                .is_ok_and(|signature| signature == job.signature),
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

        if let Err(failure) = artifact.validate_for_submission() {
            return finish_without_submit(&self.breaker, &mut artifact, failure);
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
        let submitted =
            self.submitter
                .submit_pdf(&controlled_job, &artifact, cancellation.clone(), deadline);
        let cleanup_locator = artifact.cleanup_locator();
        let cleanup = artifact.cleanup();
        match (submitted, cleanup) {
            (result, Ok(())) => result.map(|()| PrintSubmissionOutcome::default()),
            (Ok(()), Err(_)) => {
                self.breaker.store(true, Ordering::Release);
                Ok(PrintSubmissionOutcome {
                    cleanup_review: Some(CleanupReview {
                        warning_key: "error.print.exportCleanupRequiresReview",
                        locator: cleanup_locator,
                    }),
                })
            }
            (Err(failure), Err(_)) => {
                self.breaker.store(true, Ordering::Release);
                Err(failure
                    .with_retry_disposition(RetryDisposition::CleanupQuarantined)
                    .with_cleanup_review(CleanupReview {
                        warning_key: "error.print.exportCleanupRequiresReview",
                        locator: cleanup_locator,
                    }))
            }
        }
    }

    fn prefer_cleanup_review(&self, failure: PrintFailure) -> PrintFailure {
        self.cleanup_review()
            .map(|review| {
                failure
                    .clone()
                    .with_retry_disposition(RetryDisposition::CleanupQuarantined)
                    .with_cleanup_review(review)
            })
            .unwrap_or(failure)
    }

    fn cleanup_review(&self) -> Option<CleanupReview> {
        self.quarantine
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty())
                    .map(|locator| CleanupReview {
                        warning_key: "error.print.exportCleanupRequiresReview",
                        locator: locator.to_string(),
                    })
            })
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
            return Some(PrintFailure::failed("error.print.excelResponseStale"));
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
            let poll = worker.poll(wait);
            if cancellation.is_cancelled() {
                return WorkerWait::Cancelled;
            }
            if self.clock.now() >= deadline {
                return WorkerWait::TimedOut;
            }
            match poll {
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
        unproven_cleanup_review: CleanupReview,
    ) -> PrintFailure {
        match teardown {
            LoserTeardown::CleanupFailed | LoserTeardown::Finished => {
                self.prefer_cleanup_review(finished_failure)
            }
            LoserTeardown::Failed => {
                self.trip_breaker();
                attach_unproven_cleanup_review(
                    self.prefer_cleanup_review(finished_failure),
                    unproven_cleanup_review,
                )
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
) -> Result<PrintSubmissionOutcome, PrintFailure> {
    let cleanup_locator = artifact.cleanup_locator();
    if let Ok(()) = artifact.cleanup() {
        Err(failure)
    } else {
        breaker.store(true, Ordering::Release);
        Err(failure
            .with_retry_disposition(RetryDisposition::CleanupQuarantined)
            .with_cleanup_review(CleanupReview {
                warning_key: "error.print.exportCleanupRequiresReview",
                locator: cleanup_locator,
            }))
    }
}

fn clean_cancelled() -> PrintFailure {
    PrintFailure::cancelled("error.print.excelExportCancelled", false)
}

fn quarantined_failure() -> PrintFailure {
    PrintFailure::unsupported("error.print.excelExportQuarantined")
}

fn attach_unproven_cleanup_review(
    failure: PrintFailure,
    cleanup_review: CleanupReview,
) -> PrintFailure {
    if failure.has_cleanup_review() {
        failure
    } else {
        failure
            .with_retry_disposition(RetryDisposition::CleanupQuarantined)
            .with_cleanup_review(cleanup_review)
    }
}

#[cfg(test)]
pub(super) fn fixture_pdf_bytes(page_count: usize) -> Vec<u8> {
    let page_ids = (0..page_count).map(|index| index + 3).collect::<Vec<_>>();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {page_count} >>",
            page_ids
                .iter()
                .map(|id| format!("{id} 0 R"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    ];
    objects.extend(
        page_ids
            .iter()
            .map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>".to_string()),
    );
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

#[cfg(test)]
mod tests;
