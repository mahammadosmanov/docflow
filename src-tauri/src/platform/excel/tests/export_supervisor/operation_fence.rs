use super::*;

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
        first_pipeline.export_and_submit(&first_job, &first_source, &mut source_file, first_token)
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
        first_pipeline.export_and_submit(&first_job, &first_source, &mut source_file, first_token)
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
        let result = pipeline.export_and_submit(&job, &source, &mut source_file, cancellation);
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
