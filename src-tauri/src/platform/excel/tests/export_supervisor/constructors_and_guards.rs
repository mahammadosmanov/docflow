use super::*;

#[test]
fn oversized_pdf_is_rejected_before_reader_is_touched() {
    struct PanicOnRead;

    impl std::io::Read for PanicOnRead {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            panic!("oversized PDF must fail before reading or allocating");
        }
    }

    let failure = read_pdf_bytes_bounded(PanicOnRead, 65, 64).unwrap_err();

    assert_eq!(failure.message, "error.print.excelExportMalformedPdf");
}

#[test]
fn concurrently_growing_pdf_is_rejected_by_bounded_read() {
    let reader = std::io::Cursor::new(vec![b'x'; 65]);

    let failure = read_pdf_bytes_bounded(reader, 64, 64).unwrap_err();

    assert_eq!(failure.message, "error.print.excelExportMalformedPdf");
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
            crate::print_queue::PrintFailureKind::Failed
        );
        assert_eq!(panic_failure.message, "error.print.excelWorkerStopped");
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
