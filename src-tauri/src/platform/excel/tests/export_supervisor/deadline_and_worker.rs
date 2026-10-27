use super::*;

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

    assert_eq!(first.kind, crate::print_queue::PrintFailureKind::Failed);
    assert_eq!(first.message, "error.print.excelWorkerStopped");
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
fn pre_submit_cleanup_failure_preserves_primary_failure_and_trips_breaker() {
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

    let mut source_file = source.open_read().unwrap();
    let first = pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation.clone())
        .unwrap_err();
    let cleanup = first
        .cleanup_review
        .as_ref()
        .expect("cleanup review warning");
    assert_eq!(first.kind, crate::print_queue::PrintFailureKind::Failed);
    assert_eq!(first.message, "error.print.excelResponseStale");
    assert!(!first.uncertain);
    assert_eq!(
        cleanup.warning_key,
        "error.print.exportCleanupRequiresReview"
    );
    assert!(cleanup.locator.starts_with("docflow-excel-"));

    let mut source_file = source.open_read().unwrap();
    let second = pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap_err();
    assert_eq!(
        second.kind,
        crate::print_queue::PrintFailureKind::Unsupported
    );
    assert_eq!(second.message, "error.print.excelExportQuarantined");
    assert!(second.cleanup_review.is_none());

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
    assert_eq!(first.kind, crate::print_queue::PrintFailureKind::Failed);
    assert_eq!(first.message, "fake export failed");
    assert!(!first.uncertain);
    let review = first.cleanup_review.as_ref().expect("cleanup review");
    assert_eq!(
        review.warning_key,
        "error.print.exportCleanupRequiresReview"
    );
    assert!(review.locator.starts_with("docflow-excel-"));

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
fn retry_reexports_and_never_reuses_a_prior_controlled_pdf() {
    let directory = tempfile::tempdir().unwrap();
    let workbook = directory.path().join("book.xlsx");
    write_workbook(&workbook);
    let events = Arc::new(Mutex::new(Vec::new()));
    let export_attempts = Arc::new(AtomicUsize::new(0));
    let submit_attempts = Arc::new(AtomicUsize::new(0));
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Arc::new(ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(SequencedPdfExporter {
            events: Arc::clone(&events),
            attempts: Arc::clone(&export_attempts),
        }),
        Arc::new(FailOnceCapturingSubmitter {
            events: Arc::clone(&events),
            attempts: Arc::clone(&submit_attempts),
            submitted: Arc::clone(&submitted),
        }),
    ));
    let service = PrintService::new(
        directory.path().join("queues"),
        directory.path().join("journals"),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        Arc::new(PreviewAdapter::default()),
    )
    .with_excel_pipeline(pipeline);
    let caller = CallerIdentity::new("operator", "session");
    let plan = service
        .preview(
            &caller,
            BatchPrintParams {
                files: vec![workbook],
                printer: "Office Printer".to_string(),
                retries: 1,
                timeout_minutes: 1,
                ..BatchPrintParams::default()
            },
            |_| {},
        )
        .unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[2]["enabled"] = serde_json::Value::Bool(false);
    let revised = service.revise(&caller, &plan.plan_id, &rows).unwrap();

    let result = service.apply(&revised.plan_id, &caller, |_| {}).unwrap();

    assert_eq!(result.applied, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(export_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(submit_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(
        *events.lock().unwrap(),
        ["policy", "export", "submit", "policy", "export", "submit"]
    );
    let submitted = submitted.lock().unwrap();
    assert_eq!(submitted.len(), 2);
    assert_ne!(submitted[0], submitted[1]);
}

#[test]
fn post_submit_cleanup_failure_is_completed_with_non_sensitive_warning_locator() {
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
            outcome: Ok(()),
        }),
    );
    let cancellation = CancellationRegistry::default().register("plan");
    let (_directory, source, signature) = source_fixture();
    let job = worksheet_job(source.absolute(), signature, "MEC", 1);
    let mut source_file = source.open_read().unwrap();

    let outcome = pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap();
    let review = outcome.cleanup_review.expect("cleanup review warning");

    assert_eq!(
        review.warning_key,
        "error.print.exportCleanupRequiresReview"
    );
    assert!(review.locator.starts_with("docflow-excel-"));
    assert!(!review.locator.contains(std::path::MAIN_SEPARATOR));
    assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
    pipeline.clear_test_quarantine();
}

#[test]
fn completed_cleanup_warning_is_reported_without_retrying_export_or_submit() {
    let directory = tempfile::tempdir().unwrap();
    let workbook = directory.path().join("book.xlsx");
    write_workbook(&workbook);
    let events = Arc::new(Mutex::new(Vec::new()));
    let adapter = Arc::new(PreviewAdapter::default());
    let pipeline = Arc::new(ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(CleanupFailingExporter {
            events: Arc::clone(&events),
        }),
        Arc::new(OutcomeSubmitter {
            events: Arc::clone(&events),
            outcome: Ok(()),
        }),
    ));
    let queue_root = directory.path().join("queues");
    let journal_root = directory.path().join("journals");
    let service = PrintService::new(
        queue_root.clone(),
        journal_root.clone(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        adapter,
    )
    .with_excel_pipeline(pipeline.clone());
    let caller = CallerIdentity::new("operator", "session");
    let plan = service
        .preview(
            &caller,
            BatchPrintParams {
                files: vec![workbook],
                printer: "Office Printer".to_string(),
                retries: 3,
                timeout_minutes: 1,
                ..BatchPrintParams::default()
            },
            |_| {},
        )
        .unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[2]["enabled"] = serde_json::Value::Bool(false);
    let revised = service.revise(&caller, &plan.plan_id, &rows).unwrap();

    let result = service.apply(&revised.plan_id, &caller, |_| {}).unwrap();

    assert_eq!((result.applied, result.failed), (1, 0));
    assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(
        result.errors[0].key,
        "error.print.exportCleanupRequiresReview"
    );
    assert!(result.errors[0].subject.starts_with("docflow-excel-"));
    assert!(!result.errors[0].subject.contains(std::path::MAIN_SEPARATOR));

    let queue_id = result.meta["queueId"].as_str().unwrap();
    let queue: serde_json::Value = serde_json::from_slice(
        &std::fs::read(queue_root.join(format!("{queue_id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        queue["jobs"][0]["cleanupReview"]["warningKey"],
        "error.print.exportCleanupRequiresReview"
    );
    assert_eq!(
        queue["jobs"][0]["cleanupReview"]["locator"],
        result.errors[0].subject
    );
    let journal_id = result.meta["auditJournalId"].as_str().unwrap();
    let journal = crate::journal::JournalStore::new(journal_root)
        .read(journal_id)
        .unwrap();
    let journal = serde_json::to_value(journal).unwrap();
    assert_eq!(
        journal["cleanupReviews"][0]["warningKey"],
        "error.print.exportCleanupRequiresReview"
    );
    assert_eq!(
        journal["cleanupReviews"][0]["locator"],
        result.errors[0].subject
    );
    pipeline.clear_test_quarantine();
}
