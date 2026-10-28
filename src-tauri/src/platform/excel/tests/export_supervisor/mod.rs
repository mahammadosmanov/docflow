mod cleanup_journal;
mod cleanup_review;
mod constructors_and_guards;
mod deadline_and_worker;
mod export_flow;
mod fixtures;
mod operation_fence;
mod print_service;
mod timeouts;
use fixtures::*;

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

use crate::platform::excel_windows_contract::{AnchoredWorkbookSourcePolicy, ZoneIdentifierReader};
use crate::{
    cap_fs::CapabilityRoot,
    journal::FilesystemObjectIdentity,
    ooxml::{WorkbookSafetyFacts, WorksheetDescriptor, WorksheetVisibility},
    page_range::PageRangeCapability,
    paths::CallerIdentity,
    print_queue::{
        BatchPrintParams, FileSignature, PrintAdapter, PrintFailure, PrintJob, PrintJobSupport,
        PrintService, PrintSettings, PrinterInventory,
    },
    progress::{CancellationRegistry, GlobalMutationLock},
};
use zip::{ZipWriter, write::SimpleFileOptions};

use super::super::{
    ControlledPdfSubmitter, ExcelExportPipeline, ExcelExportRequest, ExcelProcessGuards,
    ExcelWorkerHandle, ExcelWorkerRuntime, ExportedWorksheetPdf, MonotonicClock, StagedWorkbook,
    SystemMonotonicClock, ThreadWorkerRuntime, WorkbookSourcePolicy, WorkerJoin, WorkerPoll,
    WorkerResponse, WorksheetPdfExporter, process_excel_guards, read_pdf_bytes_bounded,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct RequestObservation {
    request_id: uuid::Uuid,
    generation: u64,
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

struct InternetZoneReader;

impl ZoneIdentifierReader for InternetZoneReader {
    fn read_zone_identifier(
        &self,
        _source: &crate::cap_fs::AnchoredPath,
    ) -> std::io::Result<Option<Vec<u8>>> {
        Ok(Some(b"[ZoneTransfer]\r\nZoneId=3\r\n".to_vec()))
    }
}

struct LaunchCountingRuntime {
    launches: Arc<AtomicUsize>,
}

impl ExcelWorkerRuntime for LaunchCountingRuntime {
    fn launch(
        &self,
        _exporter: Arc<dyn WorksheetPdfExporter>,
        _request: ExcelExportRequest,
    ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
        self.launches.fetch_add(1, Ordering::AcqRel);
        Err(PrintFailure::failed("worker must not launch"))
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
            safety: request.safety.clone(),
            worksheet: request.worksheet.clone(),
            deadline: request.deadline,
            workbook_bytes: std::fs::read(request.staged_workbook.workbook_path()).unwrap(),
            cancelled: request.cancellation.is_cancelled(),
        });
        Ok(request.staged_workbook.fixture_exported_pdf())
    }
}

impl WorksheetPdfExporter for SequencedPdfExporter {
    fn export_pdf(
        &self,
        request: ExcelExportRequest,
    ) -> Result<ExportedWorksheetPdf, PrintFailure> {
        self.events.lock().unwrap().push("export");
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        let mut bytes = multi_page_pdf(2);
        bytes.extend_from_slice(format!("\n% attempt-{attempt}\n").as_bytes());
        let pdf_path = request.staged_workbook.pdf_output_path();
        std::fs::write(pdf_path, bytes).unwrap();
        request.staged_workbook.into_exported_pdf()
    }
}

struct SourceMutatingExporter {
    events: Arc<Mutex<Vec<&'static str>>>,
    source_path: PathBuf,
    staged_paths: Arc<Mutex<Vec<PathBuf>>>,
}

struct SourceReplacingExporter {
    events: Arc<Mutex<Vec<&'static str>>>,
    source_path: PathBuf,
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

impl WorksheetPdfExporter for SourceReplacingExporter {
    fn export_pdf(
        &self,
        request: ExcelExportRequest,
    ) -> Result<ExportedWorksheetPdf, PrintFailure> {
        self.events.lock().unwrap().push("export");
        let artifact = request.staged_workbook.fixture_exported_pdf();
        std::fs::rename(
            &self.source_path,
            self.source_path.with_extension("approved-original"),
        )
        .unwrap();
        std::fs::write(&self.source_path, b"valid replacement workbook bytes").unwrap();
        Ok(artifact)
    }
}

struct CleanupFailingExporter {
    events: Arc<Mutex<Vec<&'static str>>>,
}

struct BytesExporter {
    events: Arc<Mutex<Vec<&'static str>>>,
    bytes: Vec<u8>,
}

impl WorksheetPdfExporter for BytesExporter {
    fn export_pdf(
        &self,
        request: ExcelExportRequest,
    ) -> Result<ExportedWorksheetPdf, PrintFailure> {
        self.events.lock().unwrap().push("export");
        let pdf_path = request.staged_workbook.pdf_output_path();
        std::fs::write(pdf_path, &self.bytes).unwrap();
        request.staged_workbook.into_exported_pdf()
    }
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

struct SubmittedPdf {
    worksheet: String,
    printer: String,
    settings: PrintSettings,
    page_range: String,
    pdf: Vec<u8>,
    cancellation: crate::progress::CancellationToken,
    deadline: Instant,
}

type SubmittedPdfs = Arc<Mutex<Vec<SubmittedPdf>>>;

struct RecordingSubmitter {
    events: Arc<Mutex<Vec<&'static str>>>,
    submitted: SubmittedPdfs,
}

struct DeadlineRecordingSubmitter {
    deadlines: Arc<Mutex<Vec<Instant>>>,
}

impl ControlledPdfSubmitter for DeadlineRecordingSubmitter {
    fn submit_pdf(
        &self,
        _job: &PrintJob,
        pdf: &ExportedWorksheetPdf,
        _cancellation: crate::progress::CancellationToken,
        deadline: Instant,
    ) -> Result<(), PrintFailure> {
        let _ = pdf.validated_bytes()?;
        self.deadlines.lock().unwrap().push(deadline);
        Ok(())
    }
}

struct OutcomeSubmitter {
    events: Arc<Mutex<Vec<&'static str>>>,
    outcome: Result<(), PrintFailure>,
}

struct SequencedPdfExporter {
    events: Arc<Mutex<Vec<&'static str>>>,
    attempts: Arc<AtomicUsize>,
}

struct FailOnceCapturingSubmitter {
    events: Arc<Mutex<Vec<&'static str>>>,
    attempts: Arc<AtomicUsize>,
    submitted: Arc<Mutex<Vec<Vec<u8>>>>,
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

    fn submit(&self, job: &PrintJob, _cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure> {
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
        self.0.take().map_or(WorkerPoll::Disconnected, |response| {
            WorkerPoll::Response(Box::new(response))
        })
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
            .map_or(WorkerPoll::Disconnected, |response| {
                WorkerPoll::Response(Box::new(response))
            })
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

struct RetainingRequestRuntime {
    clock: Arc<AdvancingClock>,
    retained_path: Arc<Mutex<Option<PathBuf>>>,
    release: Mutex<Option<mpsc::Sender<()>>>,
    finished: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

impl RetainingRequestRuntime {
    fn new(clock: Arc<AdvancingClock>) -> Self {
        Self {
            clock,
            retained_path: Arc::new(Mutex::new(None)),
            release: Mutex::new(None),
            finished: Arc::new((Mutex::new(false), std::sync::Condvar::new())),
        }
    }

    fn release_and_wait(&self) {
        self.release
            .lock()
            .unwrap()
            .take()
            .expect("retained worker release sender")
            .send(())
            .unwrap();
        let (finished, changed) = &*self.finished;
        let finished = finished.lock().unwrap();
        let (finished, wait) = changed
            .wait_timeout_while(finished, Duration::from_secs(2), |finished| !*finished)
            .unwrap();
        assert!(
            *finished && !wait.timed_out(),
            "retained worker did not exit"
        );
    }
}

impl ExcelWorkerRuntime for RetainingRequestRuntime {
    fn launch(
        &self,
        _exporter: Arc<dyn WorksheetPdfExporter>,
        request: ExcelExportRequest,
    ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
        *self.retained_path.lock().unwrap() =
            Some(request.staged_workbook.workspace_path().to_path_buf());
        let (release, released) = mpsc::channel();
        *self.release.lock().unwrap() = Some(release);
        let finished = Arc::clone(&self.finished);
        let join = std::thread::spawn(move || {
            let _ = released.recv();
            drop(request);
            let (done, changed) = &*finished;
            *done.lock().unwrap() = true;
            changed.notify_all();
        });
        self.clock.advance();
        Ok(Box::new(RetainingRequestWorker { join: Some(join) }))
    }
}

struct RetainingRequestWorker {
    join: Option<std::thread::JoinHandle<()>>,
}

impl ExcelWorkerHandle for RetainingRequestWorker {
    fn poll(&mut self, _wait: Duration) -> WorkerPoll {
        WorkerPoll::Pending
    }

    fn join_status(&mut self) -> WorkerJoin {
        let Some(join) = self.join.as_ref() else {
            return WorkerJoin::Finished;
        };
        if !join.is_finished() {
            return WorkerJoin::Pending;
        }
        let join = self.join.take().expect("finished retained worker join");
        if join.join().is_ok() {
            WorkerJoin::Finished
        } else {
            WorkerJoin::Failed
        }
    }
}

impl ControlledPdfSubmitter for RecordingSubmitter {
    fn submit_pdf(
        &self,
        job: &PrintJob,
        pdf: &ExportedWorksheetPdf,
        cancellation: crate::progress::CancellationToken,
        deadline: Instant,
    ) -> Result<(), PrintFailure> {
        assert!(!cancellation.is_cancelled());
        assert_eq!(job.printer, "Office Printer");
        assert_eq!(job.paper, "A4");
        assert_eq!(job.orientation, "auto");
        assert_eq!(job.scaling, "fit");
        assert_eq!(job.color, "printer");
        assert_eq!(job.duplex, "printer");
        assert_eq!(job.page_range, "all");
        self.events.lock().unwrap().push("submit");
        self.submitted.lock().unwrap().push(SubmittedPdf {
            worksheet: job.worksheet.as_ref().unwrap().name.clone(),
            printer: job.printer.clone(),
            settings: job.settings(),
            page_range: job.page_range.clone(),
            pdf: pdf.validated_bytes()?.to_vec(),
            cancellation,
            deadline,
        });
        Ok(())
    }
}

impl ControlledPdfSubmitter for OutcomeSubmitter {
    fn submit_pdf(
        &self,
        _job: &PrintJob,
        _pdf: &ExportedWorksheetPdf,
        _cancellation: crate::progress::CancellationToken,
        _deadline: Instant,
    ) -> Result<(), PrintFailure> {
        self.events.lock().unwrap().push("submit");
        self.outcome.clone()
    }
}

impl ControlledPdfSubmitter for FailOnceCapturingSubmitter {
    fn submit_pdf(
        &self,
        _job: &PrintJob,
        pdf: &ExportedWorksheetPdf,
        _cancellation: crate::progress::CancellationToken,
        _deadline: Instant,
    ) -> Result<(), PrintFailure> {
        self.events.lock().unwrap().push("submit");
        self.submitted
            .lock()
            .unwrap()
            .push(pdf.validated_bytes()?.to_vec());
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(PrintFailure::failed(
                "error.print.syntheticPreEffectFailure",
            ))
        } else {
            Ok(())
        }
    }
}
