use super::*;

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
    assert_eq!(requests[0].safety, safety());
    assert_eq!(requests[0].worksheet, descriptor("CIV", 2));
    assert!(requests[0].deadline > Instant::now());
    assert_eq!(requests[0].workbook_bytes, b"approved workbook bytes");
    assert!(!requests[0].cancelled);
    let expected_pdf = multi_page_pdf(2);
    let submitted = submitted.lock().unwrap();
    assert_eq!(submitted.len(), 2);
    for (index, (submission, expected_name)) in submitted.iter().zip(["CIV", "MEC"]).enumerate() {
        assert_eq!(submission.worksheet, expected_name);
        assert_eq!(submission.printer, "Office Printer");
        assert_eq!(submission.settings.paper, "A4");
        assert_eq!(submission.settings.orientation, "auto");
        assert_eq!(submission.settings.scaling, "fit");
        assert_eq!(submission.settings.color, "printer");
        assert_eq!(submission.settings.duplex, "printer");
        assert_eq!(submission.settings.page_range, "all");
        assert_eq!(submission.settings.timeout_minutes, 1);
        assert_eq!(submission.page_range, "all");
        assert_eq!(submission.pdf, expected_pdf);
        assert!(submission.cancellation.same_state(&cancellation));
        assert_eq!(submission.deadline, requests[index].deadline);
    }
}

#[test]
fn submitter_receives_the_exact_export_request_deadline() {
    let base = Instant::now();
    let events = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let deadlines = Arc::new(Mutex::new(Vec::new()));
    let pipeline = ExcelExportPipeline::for_testing_with_runtime(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(RecordingExporter {
            events,
            requests: Arc::clone(&requests),
        }),
        Arc::new(DeadlineRecordingSubmitter {
            deadlines: Arc::clone(&deadlines),
        }),
        Arc::new(FixedClock(base)),
        Arc::new(ImmediateRuntime),
        Duration::from_secs(1),
    );
    let cancellation = CancellationRegistry::default().register("deadline");
    let (_directory, source, signature) = source_fixture();
    let job = worksheet_job(source.absolute(), signature, "MEC", 1);
    let mut source_file = source.open_read().unwrap();

    pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap();

    let expected = base + Duration::from_secs(60);
    assert_eq!(requests.lock().unwrap()[0].deadline, expected);
    assert_eq!(*deadlines.lock().unwrap(), [expected]);
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
fn cancellation_before_export_has_zero_policy_export_and_submit_calls() {
    const REQUEST_ID: &str = "cancel-before-export";
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
    let cancellations = CancellationRegistry::default();
    let cancellation = cancellations.register(REQUEST_ID);
    assert!(cancellations.cancel(REQUEST_ID));
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
    assert!(!failure.uncertain);
    assert_eq!(source_file.stream_position().unwrap(), 0);
    assert!(events.lock().unwrap().is_empty());
    assert!(requests.lock().unwrap().is_empty());
    assert!(submitted.lock().unwrap().is_empty());
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
fn internet_zone_policy_rejection_has_zero_staging_worker_com_and_submit_calls() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let launches = Arc::new(AtomicUsize::new(0));
    let pipeline = ExcelExportPipeline::for_testing_with_runtime(
        Arc::new(AnchoredWorkbookSourcePolicy::new(InternetZoneReader)),
        Arc::new(RecordingExporter {
            events: Arc::clone(&events),
            requests: Arc::clone(&requests),
        }),
        Arc::new(RecordingSubmitter {
            events: Arc::clone(&events),
            submitted: Arc::clone(&submitted),
        }),
        Arc::new(SystemMonotonicClock),
        Arc::new(LaunchCountingRuntime {
            launches: Arc::clone(&launches),
        }),
        Duration::from_secs(2),
    );
    let cancellation = CancellationRegistry::default().register("plan");
    let (_directory, source, signature) = source_fixture();
    let job = worksheet_job(source.absolute(), signature, "MEC", 1);
    let mut source_file = source.open_read().unwrap();

    let failure = pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap_err();

    assert_eq!(failure.message, "error.print.excelSourceMotwBlocked");
    assert_eq!(source_file.stream_position().unwrap(), 0);
    assert_eq!(launches.load(Ordering::Acquire), 0);
    assert!(events.lock().unwrap().is_empty());
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
fn validated_pdf_bytes_survive_visible_leaf_substitution() {
    let (staged, _events) = StagedWorkbook::fixture_with_drop_probe();
    let expected = multi_page_pdf(2);
    let pdf_path = staged.pdf_output_path();
    std::fs::write(&pdf_path, &expected).unwrap();
    let mut exported = staged.into_exported_pdf().unwrap();

    exported.validate_for_submission().unwrap();
    let displaced = pdf_path.with_extension("validated.pdf");
    std::fs::rename(&pdf_path, &displaced).unwrap();
    std::fs::write(&pdf_path, b"%PDF-1.4\nsubstituted").unwrap();

    assert_eq!(exported.validated_bytes().unwrap(), expected);
    assert_ne!(std::fs::read(&pdf_path).unwrap(), expected);
    std::fs::remove_file(&pdf_path).unwrap();
}

#[test]
fn exported_pdf_rejects_visible_leaf_substitution_before_validation() {
    let (staged, _events) = StagedWorkbook::fixture_with_drop_probe();
    let pdf_path = staged.pdf_output_path();
    std::fs::write(&pdf_path, multi_page_pdf(2)).unwrap();
    let mut exported = staged.into_exported_pdf().unwrap();

    let displaced = pdf_path.with_extension("original.pdf");
    std::fs::rename(&pdf_path, &displaced).unwrap();
    std::fs::write(&pdf_path, multi_page_pdf(1)).unwrap();

    let failure = exported.validate_for_submission().unwrap_err();
    assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
    assert!(!failure.uncertain);
    assert!(exported.validated_bytes().is_err());
    std::fs::remove_file(&pdf_path).unwrap();
}

#[test]
fn empty_or_malformed_pdf_never_reaches_submitter() {
    for bytes in [Vec::new(), b"%PDF-1.4\nmalformed".to_vec()] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let pipeline = ExcelExportPipeline::for_testing(
            Arc::new(RecordingPolicy {
                events: Arc::clone(&events),
            }),
            Arc::new(BytesExporter {
                events: Arc::clone(&events),
                bytes,
            }),
            Arc::new(RecordingSubmitter {
                events: Arc::clone(&events),
                submitted: Arc::clone(&submitted),
            }),
        );
        let cancellation = CancellationRegistry::default().register("invalid-pdf");
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
}

#[test]
fn excel_pdf_envelope_is_delegated_to_the_platform_decoder() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let pipeline = ExcelExportPipeline::for_testing(
        Arc::new(RecordingPolicy {
            events: Arc::clone(&events),
        }),
        Arc::new(BytesExporter {
            events: Arc::clone(&events),
            bytes: b"%PDF-1.7\nExcel-compatible objects\nstartxref\n0\n%%EOF\r\n".to_vec(),
        }),
        Arc::new(RecordingSubmitter {
            events: Arc::clone(&events),
            submitted: Arc::clone(&submitted),
        }),
    );
    let cancellation = CancellationRegistry::default().register("platform-pdf-decoder");
    let (_directory, source, signature) = source_fixture();
    let job = worksheet_job(source.absolute(), signature, "MEC", 1);
    let mut source_file = source.open_read().unwrap();

    pipeline
        .export_and_submit(&job, &source, &mut source_file, cancellation)
        .unwrap();

    assert_eq!(*events.lock().unwrap(), ["policy", "export", "submit"]);
    assert_eq!(submitted.lock().unwrap().len(), 1);
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
fn exporter_teardown_failure_marker_trips_the_process_breaker() {
    let (staged, _events) = StagedWorkbook::fixture_with_drop_probe();
    assert!(!staged.cleanup_breaker.load(Ordering::Acquire));

    staged.quarantine_after_teardown_failure();

    assert!(staged.cleanup_breaker.load(Ordering::Acquire));
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}
