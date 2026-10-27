use super::*;

#[test]
fn panic_unwind_cleanup_failure_attaches_review_to_worker_stopped_primary() {
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
    assert_cleanup_review_failure(
        &pipeline,
        &failure,
        crate::print_queue::PrintFailureKind::Failed,
        "error.print.excelWorkerStopped",
        &events,
        &submitted,
    );
}

#[test]
fn error_response_cleanup_failure_attaches_review_to_cancelled_primary() {
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

    assert_cleanup_review_failure(
        &pipeline,
        &failure,
        crate::print_queue::PrintFailureKind::Cancelled,
        "error.print.excelExportCancelled",
        &events,
        &submitted,
    );
}

#[test]
fn received_response_cleanup_failure_attaches_review_to_quarantined_primary() {
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

    assert_cleanup_review_failure(
        &pipeline,
        &failure,
        crate::print_queue::PrintFailureKind::Unsupported,
        "error.print.excelExportQuarantined",
        &events,
        &submitted,
    );
}

#[test]
fn cancelled_failed_join_loser_cleanup_failure_preserves_cancelled_primary() {
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

    assert_cleanup_review_failure(
        &pipeline,
        &failure,
        crate::print_queue::PrintFailureKind::Cancelled,
        "error.print.excelExportCancelled",
        &events,
        &submitted,
    );
}

#[test]
fn timeout_failed_join_loser_cleanup_failure_preserves_timeout_primary() {
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
    let cancellation = CancellationRegistry::default().register("cleanup-timeout-failed-join");
    let (_directory, source, signature) = source_fixture();
    let job = worksheet_job(source.absolute(), signature, "MEC", 1);
    let mut source_file = source.open_read().unwrap();

    let failure = pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap_err();

    assert_cleanup_review_failure(
        &pipeline,
        &failure,
        crate::print_queue::PrintFailureKind::Failed,
        "error.print.excelExportTimedOut",
        &events,
        &submitted,
    );
}
