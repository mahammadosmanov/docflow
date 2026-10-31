use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use uuid::Uuid;

mod pipeline;
mod staged;
mod worker;
pub(crate) use pipeline::*;
pub(crate) use staged::ExportedWorksheetPdf;
use staged::StagedWorkbook;
#[cfg(any(not(windows), test))]
use staged::read_pdf_bytes_bounded;
use worker::*;

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
