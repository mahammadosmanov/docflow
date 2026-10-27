use super::*;

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
            .map(|submission| {
                (
                    submission.worksheet.as_str(),
                    submission.page_range.as_str(),
                )
            })
            .collect::<Vec<_>>(),
        [("CIV", "all"), ("MEC", "all")]
    );
    assert!(adapter.native_submits.lock().unwrap().is_empty());
}

#[test]
fn persisted_worksheet_resume_revalidates_reexports_and_submits_in_revised_order() {
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
        Arc::new(BytesExporter {
            events: Arc::clone(&first_events),
            bytes: multi_page_pdf(1),
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
    .with_excel_pipeline(first_pipeline);
    let plan = first_service
        .preview(
            &caller,
            BatchPrintParams {
                files: vec![workbook.clone()],
                printer: "Office Printer".to_string(),
                timeout_minutes: 1,
                ..BatchPrintParams::default()
            },
            |_| {},
        )
        .unwrap();
    let first = first_service.apply(&plan.plan_id, &caller, |_| {}).unwrap();
    assert_eq!((first.applied, first.failed), (0, 1));
    assert_eq!(
        *first_events.lock().unwrap(),
        ["policy", "export", "submit"]
    );
    let queue_id = first.meta["queueId"].as_str().unwrap();
    let queue_path = queue_root.join(format!("{queue_id}.json"));
    let adversarial_pdf = directory.path().join("ambient-export.pdf");
    std::fs::write(&adversarial_pdf, multi_page_pdf(1)).unwrap();
    let mut persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    for job in persisted["jobs"].as_array_mut().unwrap() {
        job["exportedPdfPath"] = serde_json::json!(adversarial_pdf);
        job["cleanupReview"] = serde_json::json!({
            "warningKey": "error.print.exportCleanupRequiresReview",
            "locator": "ambient-export.pdf"
        });
    }
    std::fs::write(&queue_path, serde_json::to_vec_pretty(&persisted).unwrap()).unwrap();

    let resumed_events = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let resumed_pipeline = Arc::new(ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&resumed_events),
        }),
        Arc::new(RecordingExporter {
            events: Arc::clone(&resumed_events),
            requests: Arc::clone(&requests),
        }),
        Arc::new(RecordingSubmitter {
            events: Arc::clone(&resumed_events),
            submitted: Arc::clone(&submitted),
        }),
    ));
    let resumed_service = PrintService::new(
        queue_root.clone(),
        journal_root,
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        Arc::new(PreviewAdapter::default()),
    )
    .with_excel_pipeline(resumed_pipeline);
    let resumed = resumed_service.resume(&caller, queue_id).unwrap();
    let rows = resumed.meta["editor"]["rows"].as_array().unwrap();
    let by_name = |name: &str| {
        rows.iter()
            .find(|row| row["worksheetName"] == name)
            .unwrap()
            .clone()
    };
    let civ = by_name("CIV");
    let mec = by_name("MEC");
    let revised = resumed_service
        .revise(&caller, &resumed.plan_id, &[civ, mec])
        .unwrap();

    let result = resumed_service
        .apply(&revised.plan_id, &caller, |_| {})
        .unwrap();

    assert_eq!((result.applied, result.failed), (2, 0));
    assert_eq!(
        *resumed_events.lock().unwrap(),
        ["policy", "export", "submit", "policy", "export", "submit"]
    );
    let approved_source = std::fs::read(&workbook).unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0].request_id, requests[1].request_id);
    assert_eq!(requests[0].worksheet.name, "CIV");
    assert_eq!(requests[1].worksheet.name, "MEC");
    assert!(
        requests
            .iter()
            .all(|request| request.workbook_bytes == approved_source)
    );
    let submitted = submitted.lock().unwrap();
    assert_eq!(submitted.len(), 2);
    assert_eq!(submitted[0].worksheet, "CIV");
    assert_eq!(submitted[1].worksheet, "MEC");
    assert!(
        submitted
            .iter()
            .all(|submission| submission.pdf == multi_page_pdf(2))
    );
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(queue_path).unwrap()).unwrap();
    assert_eq!(
        persisted["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["worksheet"]["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["CIV", "MEC"]
    );
    assert!(persisted["jobs"][0].get("exportedPdfPath").is_none());
    assert_eq!(
        persisted["jobs"][0]["cleanupReview"],
        serde_json::json!({
            "warningKey": "error.print.exportCleanupRequiresReview",
            "locator": "ambient-export.pdf"
        })
    );
    assert!(persisted["jobs"][1].get("exportedPdfPath").is_none());
    assert_eq!(
        persisted["jobs"][1]["cleanupReview"],
        serde_json::json!({
            "warningKey": "error.print.exportCleanupRequiresReview",
            "locator": "ambient-export.pdf"
        })
    );
}

#[test]
fn source_leaf_substitution_after_export_blocks_all_submission() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let (_directory, source, signature) = source_fixture();
    let pipeline = ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(SourceReplacingExporter {
            events: Arc::clone(&events),
            source_path: source.absolute().to_path_buf(),
        }),
        Arc::new(RecordingSubmitter {
            events: Arc::clone(&events),
            submitted: Arc::clone(&submitted),
        }),
    );
    let cancellation = CancellationRegistry::default().register("source-swap");
    let job = worksheet_job(source.absolute(), signature, "MEC", 1);
    let mut source_file = source.open_read().unwrap();

    let failure = pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap_err();

    assert_eq!(failure.message, "error.print.workbookChanged");
    assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
    assert!(submitted.lock().unwrap().is_empty());
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
    let apply =
        std::thread::spawn(move || apply_service.apply(&apply_plan_id, &apply_caller, |_| {}));
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
