use super::*;

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
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .cloned()
        .collect::<Vec<_>>();

    assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
    assert_eq!(failure.message, "error.print.excelExportTimedOut");
    assert!(!failure.uncertain);
    let review = failure.cleanup_review.as_ref().expect("cleanup review");
    assert_eq!(
        review.warning_key,
        "error.print.exportCleanupRequiresReview"
    );
    assert!(review.locator.starts_with("docflow-excel-"));
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
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .cloned()
        .collect::<Vec<_>>();

    assert_eq!(
        failure.kind,
        crate::print_queue::PrintFailureKind::Cancelled
    );
    assert_eq!(failure.message, "error.print.excelExportCancelled");
    assert!(!failure.uncertain);
    let review = failure.cleanup_review.as_ref().expect("cleanup review");
    assert_eq!(
        review.warning_key,
        "error.print.exportCleanupRequiresReview"
    );
    assert!(review.locator.starts_with("docflow-excel-"));
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

    for expected in [
        (
            crate::print_queue::PrintFailureKind::Failed,
            "error.print.excelExportTimedOut",
        ),
        (
            crate::print_queue::PrintFailureKind::Unsupported,
            "error.print.excelExportQuarantined",
        ),
    ] {
        let mut source_file = source.open_read().unwrap();
        let failure = pipeline
            .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
            .unwrap_err();
        assert_eq!(failure.kind, expected.0);
        assert_eq!(failure.message, expected.1);
        assert!(!failure.uncertain);
    }

    assert_eq!(*events.lock().unwrap(), ["policy"]);
    assert!(submitted.lock().unwrap().is_empty());
}

#[test]
fn timeout_live_request_reports_retained_workspace_locator() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(AdvancingClock::new());
    let runtime = Arc::new(RetainingRequestRuntime::new(Arc::clone(&clock)));
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
        clock,
        runtime.clone(),
        Duration::from_secs(1),
    );
    let cancellation = CancellationRegistry::default().register("live-request");
    let (_directory, source, signature) = source_fixture();
    let job = worksheet_job(source.absolute(), signature, "MEC", 1);
    let mut source_file = source.open_read().unwrap();

    let failure = pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap_err();
    let retained_path = runtime
        .retained_path
        .lock()
        .unwrap()
        .clone()
        .expect("worker-owned request path");
    let retained_locator = retained_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap()
        .to_string();
    let workspace_was_live = retained_path.exists();
    let cleanup_review = failure.cleanup_review.clone();
    runtime.release_and_wait();

    assert!(
        workspace_was_live,
        "request was not retained past teardown grace"
    );
    assert!(!retained_path.exists());
    assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
    assert_eq!(failure.message, "error.print.excelExportTimedOut");
    let cleanup_review = cleanup_review.expect("unproven cleanup review evidence");
    assert_eq!(cleanup_review.locator, retained_locator);
    assert!(cleanup_review.locator.starts_with("docflow-excel-"));
    assert!(!cleanup_review.locator.contains(std::path::MAIN_SEPARATOR));
    assert_eq!(
        cleanup_review.warning_key,
        "error.print.exportCleanupRequiresReview"
    );
    assert_eq!(*events.lock().unwrap(), ["policy"]);
    assert!(submitted.lock().unwrap().is_empty());
}

struct PlainFailingExporter;

impl WorksheetPdfExporter for PlainFailingExporter {
    fn export_pdf(
        &self,
        _request: ExcelExportRequest,
    ) -> Result<ExportedWorksheetPdf, PrintFailure> {
        Err(PrintFailure::failed("error.print.syntheticExporterFailure"))
    }
}

struct PollCancellingRuntime {
    cancellations: Arc<CancellationRegistry>,
    request_id: &'static str,
}

impl ExcelWorkerRuntime for PollCancellingRuntime {
    fn launch(
        &self,
        exporter: Arc<dyn WorksheetPdfExporter>,
        request: ExcelExportRequest,
    ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
        let request_id = request.request_id;
        let generation = request.generation;
        let result = exporter.export_pdf(request);
        Ok(Box::new(PollCancellingWorker {
            response: Some(WorkerResponse {
                request_id,
                generation,
                result,
            }),
            cancellations: Arc::clone(&self.cancellations),
            request_id: self.request_id,
        }))
    }
}

struct PollCancellingWorker {
    response: Option<WorkerResponse>,
    cancellations: Arc<CancellationRegistry>,
    request_id: &'static str,
}

impl ExcelWorkerHandle for PollCancellingWorker {
    fn poll(&mut self, _wait: Duration) -> WorkerPoll {
        let Some(response) = self.response.take() else {
            return WorkerPoll::Pending;
        };
        assert!(self.cancellations.cancel(self.request_id));
        WorkerPoll::Response(Box::new(response))
    }

    fn join_status(&mut self) -> WorkerJoin {
        WorkerJoin::Finished
    }
}

#[test]
fn cancellation_during_response_poll_preserves_cancelled_primary_for_error_response() {
    const REQUEST_ID: &str = "poll-cancel-error-response";
    let events = Arc::new(Mutex::new(Vec::new()));
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let cancellations = Arc::new(CancellationRegistry::default());
    let cancellation = cancellations.register(REQUEST_ID);
    let pipeline = ExcelExportPipeline::for_testing_with_runtime(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(PlainFailingExporter),
        Arc::new(RecordingSubmitter {
            events: Arc::clone(&events),
            submitted: Arc::clone(&submitted),
        }),
        Arc::new(SystemMonotonicClock),
        Arc::new(PollCancellingRuntime {
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

    assert_eq!(
        failure.kind,
        crate::print_queue::PrintFailureKind::Cancelled
    );
    assert_eq!(failure.message, "error.print.excelExportCancelled");
    assert!(submitted.lock().unwrap().is_empty());
}

struct PollExpiringRuntime {
    clock: Arc<AdvancingClock>,
}

impl ExcelWorkerRuntime for PollExpiringRuntime {
    fn launch(
        &self,
        exporter: Arc<dyn WorksheetPdfExporter>,
        request: ExcelExportRequest,
    ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
        let request_id = request.request_id;
        let generation = request.generation;
        let result = exporter.export_pdf(request);
        Ok(Box::new(PollExpiringWorker {
            response: Some(WorkerResponse {
                request_id,
                generation,
                result,
            }),
            clock: Arc::clone(&self.clock),
        }))
    }
}

struct PollExpiringWorker {
    response: Option<WorkerResponse>,
    clock: Arc<AdvancingClock>,
}

impl ExcelWorkerHandle for PollExpiringWorker {
    fn poll(&mut self, _wait: Duration) -> WorkerPoll {
        let Some(response) = self.response.take() else {
            return WorkerPoll::Pending;
        };
        self.clock.advance();
        WorkerPoll::Response(Box::new(response))
    }

    fn join_status(&mut self) -> WorkerJoin {
        WorkerJoin::Finished
    }
}

#[test]
fn deadline_during_response_poll_preserves_timeout_primary_for_error_response() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(AdvancingClock::new());
    let pipeline = ExcelExportPipeline::for_testing_with_runtime(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(PlainFailingExporter),
        Arc::new(RecordingSubmitter {
            events: Arc::clone(&events),
            submitted: Arc::clone(&submitted),
        }),
        Arc::clone(&clock) as Arc<dyn MonotonicClock>,
        Arc::new(PollExpiringRuntime { clock }),
        Duration::from_secs(1),
    );
    let cancellation = CancellationRegistry::default().register("poll-expire-error-response");
    let (_directory, source, signature) = source_fixture();
    let job = worksheet_job(source.absolute(), signature, "MEC", 1);
    let mut source_file = source.open_read().unwrap();

    let failure = pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap_err();

    assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
    assert_eq!(failure.message, "error.print.excelExportTimedOut");
    assert!(submitted.lock().unwrap().is_empty());
}
