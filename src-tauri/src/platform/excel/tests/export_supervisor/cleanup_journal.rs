use super::*;

#[test]
fn cleanup_locator_is_durable_when_outcome_journal_write_fails() {
    let directory = tempfile::tempdir().unwrap();
    let workbook = directory.path().join("book.xlsx");
    write_workbook(&workbook);
    let events = Arc::new(Mutex::new(Vec::new()));
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
    let service = PrintService::new(
        queue_root.clone(),
        directory.path().join("journals"),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        Arc::new(PreviewAdapter::default()),
    )
    .with_excel_pipeline(pipeline.clone())
    .with_test_journal_failure_on_write(3);
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
    rows[2]["enabled"] = serde_json::Value::Bool(false);
    let revised = service.revise(&caller, &plan.plan_id, &rows).unwrap();
    let queue_id = revised.meta["queueId"].as_str().unwrap().to_string();

    service
        .apply(&revised.plan_id, &caller, |_| {})
        .expect_err("injected outcome journal write failure");

    let queue: serde_json::Value = serde_json::from_slice(
        &std::fs::read(queue_root.join(format!("{queue_id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        queue["jobs"][0]["cleanupReview"]["warningKey"],
        "error.print.exportCleanupRequiresReview"
    );
    let locator = queue["jobs"][0]["cleanupReview"]["locator"]
        .as_str()
        .expect("durable cleanup locator");
    assert!(locator.starts_with("docflow-excel-"));
    assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
    pipeline.clear_test_quarantine();
}

#[test]
fn cleanup_locator_is_journaled_when_evidence_queue_checkpoint_fails() {
    let directory = tempfile::tempdir().unwrap();
    let workbook = directory.path().join("book.xlsx");
    write_workbook(&workbook);
    let events = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Arc::new(ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(CleanupFailingExporter {
            events: Arc::clone(&events),
        }),
        Arc::new(OutcomeSubmitter {
            events: Arc::clone(&events),
            outcome: Err(PrintFailure::failed(
                "error.print.syntheticPreEffectFailure",
            )),
        }),
    ));
    let journal_root = directory.path().join("journals");
    let service = PrintService::new(
        directory.path().join("queues"),
        journal_root.clone(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        Arc::new(PreviewAdapter::default()),
    )
    .with_excel_pipeline(pipeline.clone())
    .with_test_queue_failure_on_save(3);
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
    rows[2]["enabled"] = serde_json::Value::Bool(false);
    let revised = service.revise(&caller, &plan.plan_id, &rows).unwrap();

    let error = service
        .apply(&revised.plan_id, &caller, |_| {})
        .expect_err("injected cleanup-evidence queue checkpoint failure");

    let journals = std::fs::read_dir(&journal_root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    assert_eq!(journals.len(), 1);
    let journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(journals[0].path()).unwrap()).unwrap();
    let locator = journal["cleanupReviews"][0]["locator"]
        .as_str()
        .expect("cleanup locator must be journaled before queue checkpoint");
    assert!(locator.starts_with("docflow-excel-"));
    assert!(!locator.contains(std::path::MAIN_SEPARATOR));

    assert_eq!(error.code, "backend_request_failed");
    assert!(error.message.contains("injected print queue save failure"));
    assert!(error.message.contains("primaryClassification=failed"));
    assert!(
        error
            .message
            .contains("warningKey=error.print.exportCleanupRequiresReview")
    );
    assert!(error.message.contains(&format!("locator={locator}")));
    assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
    pipeline.clear_test_quarantine();
}

#[test]
fn restart_resume_second_cleanup_failure_preserves_both_distinct_locators() {
    let directory = tempfile::tempdir().unwrap();
    let workbook = directory.path().join("book.xlsx");
    write_workbook(&workbook);
    let queue_root = directory.path().join("queues");
    let journal_root = directory.path().join("journals");
    let caller = CallerIdentity::new("operator", "session");

    let first_events = Arc::new(Mutex::new(Vec::new()));
    let first_pipeline = Arc::new(ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&first_events),
        }),
        Arc::new(CleanupFailingExporter {
            events: Arc::clone(&first_events),
        }),
        Arc::new(OutcomeSubmitter {
            events: Arc::clone(&first_events),
            outcome: Err(PrintFailure::failed(
                "error.print.syntheticPreEffectFailure",
            )),
        }),
    ));
    let first_service = PrintService::new(
        queue_root.clone(),
        journal_root.clone(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        Arc::new(PreviewAdapter::default()),
    )
    .with_excel_pipeline(first_pipeline.clone())
    .with_test_journal_failure_on_write(3);
    let plan = first_service
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
    rows[2]["enabled"] = serde_json::Value::Bool(false);
    let revised = first_service.revise(&caller, &plan.plan_id, &rows).unwrap();
    let queue_id = revised.meta["queueId"].as_str().unwrap().to_string();

    first_service
        .apply(&revised.plan_id, &caller, |_| {})
        .expect_err("first outcome journal write must fail");
    let queue_path = queue_root.join(format!("{queue_id}.json"));
    let first_queue: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    let first_locator = first_queue["jobs"][0]["cleanupReview"]["locator"]
        .as_str()
        .expect("first cleanup locator")
        .to_string();

    let second_events = Arc::new(Mutex::new(Vec::new()));
    let second_pipeline = Arc::new(ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&second_events),
        }),
        Arc::new(CleanupFailingExporter {
            events: Arc::clone(&second_events),
        }),
        Arc::new(OutcomeSubmitter {
            events: Arc::clone(&second_events),
            outcome: Err(PrintFailure::failed(
                "error.print.syntheticPreEffectFailure",
            )),
        }),
    ));
    let restarted = PrintService::new(
        queue_root,
        journal_root.clone(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        Arc::new(PreviewAdapter::default()),
    )
    .with_excel_pipeline(second_pipeline.clone());
    let resumed = restarted.resume(&caller, &queue_id).unwrap();
    assert!(
        resumed.meta["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|job| job.get("cleanupReview").is_none())
    );
    let mut resume_rows = resumed.meta["editor"]["rows"].as_array().unwrap().clone();
    resume_rows[0]["enabled"] = serde_json::Value::Bool(true);
    let revised = restarted
        .revise(&caller, &resumed.plan_id, &resume_rows)
        .unwrap();
    assert!(
        revised.meta["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|job| job.get("cleanupReview").is_none())
    );

    let result = restarted.apply(&revised.plan_id, &caller, |_| {}).unwrap();
    let second_locator = result
        .errors
        .iter()
        .find(|warning| warning.key == "error.print.exportCleanupRequiresReview")
        .expect("second cleanup warning")
        .subject
        .clone();
    assert_ne!(first_locator, second_locator);

    let second_queue: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    let queue_locators = second_queue["jobs"][0]["cleanupReview"]
        .as_array()
        .expect("multiple cleanup reviews serialize as a collection")
        .iter()
        .map(|review| review["locator"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(queue_locators, [&first_locator, &second_locator]);

    let journal_id = result.meta["auditJournalId"].as_str().unwrap();
    let journal = crate::journal::JournalStore::new(journal_root)
        .read(journal_id)
        .unwrap();
    assert_eq!(
        journal
            .cleanup_reviews
            .iter()
            .map(|review| review.locator.as_str())
            .collect::<Vec<_>>(),
        [&first_locator, &second_locator]
    );
    assert_eq!(
        *first_events.lock().unwrap(),
        ["policy", "export", "submit"]
    );
    assert_eq!(
        *second_events.lock().unwrap(),
        ["policy", "export", "submit"]
    );
    first_pipeline.clear_test_quarantine();
    second_pipeline.clear_test_quarantine();
}

#[test]
fn failed_cleanup_warning_is_not_retried_or_counted_as_another_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let workbook = directory.path().join("book.xlsx");
    write_workbook(&workbook);
    let events = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Arc::new(ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(CleanupFailingExporter {
            events: Arc::clone(&events),
        }),
        Arc::new(OutcomeSubmitter {
            events: Arc::clone(&events),
            outcome: Err(PrintFailure::failed(
                "error.print.syntheticPreEffectFailure",
            )),
        }),
    ));
    let queue_root = directory.path().join("queues");
    let journal_root = directory.path().join("journals");
    let service = PrintService::new(
        queue_root.clone(),
        journal_root.clone(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        Arc::new(PreviewAdapter::default()),
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
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let queue: serde_json::Value = serde_json::from_slice(
        &std::fs::read(queue_root.join(format!("{queue_id}.json"))).unwrap(),
    )
    .unwrap();

    assert_eq!(queue["jobs"][0]["attempts"], 1);
    assert_eq!(
        queue["jobs"][0]["cleanupReview"]["warningKey"],
        "error.print.exportCleanupRequiresReview"
    );
    let journal_id = result.meta["auditJournalId"].as_str().unwrap();
    let journal = crate::journal::JournalStore::new(journal_root)
        .read(journal_id)
        .unwrap();
    assert_eq!(journal.cleanup_reviews.len(), 1);
    assert_eq!(
        journal.cleanup_reviews[0].warning_key,
        "error.print.exportCleanupRequiresReview"
    );
    assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
    assert_eq!(result.failed, 1);
    assert!(
        result
            .errors
            .iter()
            .any(|warning| { warning.key == "error.print.exportCleanupRequiresReview" })
    );
    pipeline.clear_test_quarantine();
}

#[test]
fn uncertain_cleanup_warning_is_reported_without_retrying_export_or_submit() {
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
            outcome: Err(PrintFailure::uncertain("printer outcome unknown")),
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

    assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
    assert!(result.errors.iter().any(|warning| {
        warning.key == "warn.print.stateUncertain" && warning.subject == "book.xlsx"
    }));
    let cleanup = result
        .errors
        .iter()
        .find(|warning| warning.key == "error.print.exportCleanupRequiresReview")
        .expect("cleanup warning");
    assert!(cleanup.subject.starts_with("docflow-excel-"));
    assert!(!cleanup.subject.contains(std::path::MAIN_SEPARATOR));
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let queue: serde_json::Value = serde_json::from_slice(
        &std::fs::read(queue_root.join(format!("{queue_id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(queue["jobs"][0]["status"], "uncertain");
    assert_eq!(
        queue["jobs"][0]["cleanupReview"]["locator"],
        cleanup.subject
    );
    let journal_id = result.meta["auditJournalId"].as_str().unwrap();
    let journal = crate::journal::JournalStore::new(journal_root)
        .read(journal_id)
        .unwrap();
    assert_eq!(
        journal.state,
        Some(crate::journal::JournalState::RecoveryRequired)
    );
    assert_eq!(journal.cleanup_reviews.len(), 1);
    assert_eq!(journal.cleanup_reviews[0].locator, cleanup.subject);
    pipeline.clear_test_quarantine();
}

#[test]
fn cancelled_cleanup_warning_survives_queue_and_journal_readback() {
    let directory = tempfile::tempdir().unwrap();
    let workbook = directory.path().join("book.xlsx");
    write_workbook(&workbook);
    let events = Arc::new(Mutex::new(Vec::new()));
    let pipeline = Arc::new(ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(CleanupFailingExporter {
            events: Arc::clone(&events),
        }),
        Arc::new(OutcomeSubmitter {
            events: Arc::clone(&events),
            outcome: Err(PrintFailure::cancelled(
                "error.print.syntheticCancelled",
                false,
            )),
        }),
    ));
    let queue_root = directory.path().join("queues");
    let journal_root = directory.path().join("journals");
    let service = PrintService::new(
        queue_root.clone(),
        journal_root.clone(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        Arc::new(PreviewAdapter::default()),
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
    let cleanup = result
        .errors
        .iter()
        .find(|warning| warning.key == "error.print.exportCleanupRequiresReview")
        .expect("cleanup warning");
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let queue: serde_json::Value = serde_json::from_slice(
        &std::fs::read(queue_root.join(format!("{queue_id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(queue["jobs"][0]["status"], "cancelled");
    assert_eq!(queue["jobs"][0]["attempts"], 1);
    assert_eq!(
        queue["jobs"][0]["cleanupReview"]["locator"],
        cleanup.subject
    );
    let journal_id = result.meta["auditJournalId"].as_str().unwrap();
    let journal = crate::journal::JournalStore::new(journal_root)
        .read(journal_id)
        .unwrap();
    assert_eq!(
        journal.state,
        Some(crate::journal::JournalState::CancelledClean)
    );
    assert_eq!(journal.cleanup_reviews.len(), 1);
    assert_eq!(journal.cleanup_reviews[0].locator, cleanup.subject);
    assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
    pipeline.clear_test_quarantine();
}

#[test]
fn post_submit_cleanup_failure_preserves_renderer_failure_classification() {
    for renderer_failure in [
        PrintFailure::uncertain("printer outcome unknown"),
        PrintFailure::cancelled("cancel outcome unknown", true),
        PrintFailure::failed("printer rejected before effect"),
    ] {
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
                outcome: Err(renderer_failure.clone()),
            }),
        );
        let cancellation = CancellationRegistry::default().register("plan");
        let (_directory, source, signature) = source_fixture();
        let job = worksheet_job(source.absolute(), signature, "MEC", 1);
        let mut source_file = source.open_read().unwrap();

        let actual = pipeline
            .export_and_submit(&job, &source, &mut source_file, cancellation)
            .unwrap_err();
        let review = actual
            .cleanup_review
            .as_ref()
            .expect("cleanup review warning");

        assert_eq!(actual.kind, renderer_failure.kind);
        assert_eq!(actual.message, renderer_failure.message);
        assert_eq!(actual.uncertain, renderer_failure.uncertain);
        assert_eq!(
            review.warning_key,
            "error.print.exportCleanupRequiresReview"
        );
        assert!(review.locator.starts_with("docflow-excel-"));
        assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
        pipeline.clear_test_quarantine();
    }
}
