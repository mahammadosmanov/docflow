#![cfg(not(test))]

// This crate is a compile-only production boundary mirror. The real root crate's
// Windows `--all-targets` gate owns test-cfg compilation; this nested harness has
// no independent test scaffolding to compile or maintain.

#[cfg(target_os = "windows")]
pub mod page_range {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum PageRangeCapability {
        Pdf { page_count: Option<u32> },
        SinglePage,
        AllOnly,
    }

    pub struct PageRangeError {
        pub message: String,
    }

    pub fn selected_pdf_pages(_raw: &str, page_count: u32) -> Result<Vec<u32>, PageRangeError> {
        Ok((0..page_count).collect())
    }
}

#[cfg(target_os = "windows")]
pub mod print_queue {
    use std::{fs::File, path::Path};

    use crate::{
        ooxml::{WorkbookSafetyFacts, WorksheetDescriptor},
        page_range::PageRangeCapability,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PrintFailureKind {
        Failed,
        Unsupported,
        UnsupportedOptions,
        Uncertain,
        Cancelled,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum RetryDisposition {
        CleanupQuarantined,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct CleanupReview {
        pub(crate) warning_key: &'static str,
        pub(crate) locator: String,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct PrintSubmissionOutcome {
        pub(crate) cleanup_review: Option<CleanupReview>,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PrintFailure {
        pub kind: PrintFailureKind,
        pub message: String,
        pub uncertain: bool,
        cleanup_review: Option<CleanupReview>,
    }

    impl PrintFailure {
        fn new(kind: PrintFailureKind, message: impl Into<String>, uncertain: bool) -> Self {
            Self {
                kind,
                message: message.into(),
                uncertain,
                cleanup_review: None,
            }
        }

        pub fn failed(message: impl Into<String>) -> Self {
            Self::new(PrintFailureKind::Failed, message, false)
        }

        pub fn unsupported(message: impl Into<String>) -> Self {
            Self::new(PrintFailureKind::Unsupported, message, false)
        }

        pub fn unsupported_options(message: impl Into<String>) -> Self {
            Self::new(PrintFailureKind::UnsupportedOptions, message, false)
        }

        pub fn uncertain(message: impl Into<String>) -> Self {
            Self::new(PrintFailureKind::Uncertain, message, true)
        }

        pub fn cancelled(message: impl Into<String>, uncertain: bool) -> Self {
            Self::new(PrintFailureKind::Cancelled, message, uncertain)
        }

        pub(crate) fn with_cleanup_review(mut self, cleanup_review: CleanupReview) -> Self {
            self.cleanup_review = Some(cleanup_review);
            self
        }

        pub(crate) fn has_cleanup_review(&self) -> bool {
            self.cleanup_review.is_some()
        }

        pub(crate) fn with_retry_disposition(self, _disposition: RetryDisposition) -> Self {
            self
        }
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct FileSignature {
        pub size: u64,
        pub mtime_ns: u64,
        pub sha256: Option<String>,
    }

    pub fn file_signature_from_open_file(
        _file: &mut File,
        _include_sha256: bool,
    ) -> std::io::Result<FileSignature> {
        Ok(FileSignature::default())
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PrintSettings {
        pub paper: String,
        pub orientation: String,
        pub scaling: String,
        pub color: String,
        pub duplex: String,
        pub page_range: String,
        pub timeout_minutes: u32,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PrintJobSupport {
        pub verb: String,
        pub settings_limited: bool,
        pub page_range: PageRangeCapability,
    }

    impl PrintJobSupport {
        pub fn controlled() -> Self {
            Self {
                verb: "docflow".to_string(),
                settings_limited: false,
                page_range: PageRangeCapability::AllOnly,
            }
        }

        pub fn controlled_pdf(page_count: Option<u32>) -> Self {
            Self {
                verb: "docflow".to_string(),
                settings_limited: false,
                page_range: PageRangeCapability::Pdf { page_count },
            }
        }

        pub fn single_page() -> Self {
            Self {
                verb: "docflow".to_string(),
                settings_limited: false,
                page_range: PageRangeCapability::SinglePage,
            }
        }

        pub fn native_application() -> Self {
            Self {
                verb: "printto".to_string(),
                settings_limited: true,
                page_range: PageRangeCapability::AllOnly,
            }
        }
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct PrinterInventory {
        pub printers: Vec<String>,
        pub default_printer: Option<String>,
    }

    impl PrinterInventory {
        pub fn normalized(mut self) -> Self {
            self.printers.sort();
            self.printers.dedup();
            self
        }

        pub fn contains(&self, printer: &str) -> bool {
            self.printers.iter().any(|candidate| candidate == printer)
        }
    }

    #[derive(Clone, Debug)]
    pub struct PrintJob {
        pub path: String,
        pub printer: String,
        pub verb: String,
        pub paper: String,
        pub orientation: String,
        pub scaling: String,
        pub color: String,
        pub duplex: String,
        pub page_range: String,
        pub timeout_minutes: u32,
        pub signature: FileSignature,
        pub worksheet: Option<WorksheetDescriptor>,
        pub workbook_safety: Option<WorkbookSafetyFacts>,
    }

    impl PrintJob {
        pub fn settings(&self) -> PrintSettings {
            PrintSettings {
                paper: self.paper.clone(),
                orientation: self.orientation.clone(),
                scaling: self.scaling.clone(),
                color: self.color.clone(),
                duplex: self.duplex.clone(),
                page_range: self.page_range.clone(),
                timeout_minutes: self.timeout_minutes,
            }
        }
    }

    pub trait PrintAdapter: Send + Sync {
        fn inventory(&self) -> Result<PrinterInventory, PrintFailure>;
        fn inspect(
            &self,
            path: &Path,
            printer: &str,
            settings: &PrintSettings,
        ) -> Result<PrintJobSupport, PrintFailure>;
        fn submit(&self, job: &PrintJob, cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure>;
    }
}

#[cfg(target_os = "windows")]
#[path = "../../../src/windows_pdf.rs"]
pub mod windows_pdf;

pub mod pdf {
    use std::sync::Arc;

    pub(crate) const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;

    pub struct PdfInspection {
        pub page_count: u32,
    }

    pub(crate) fn inspect_bytes(bytes: Arc<[u8]>) -> Result<PdfInspection, String> {
        if bytes.is_empty() {
            Err("empty PDF".to_string())
        } else {
            Ok(PdfInspection { page_count: 1 })
        }
    }
}

#[cfg(target_os = "windows")]
pub mod journal {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum FilesystemObjectIdentity {
        Unix { device: u64, inode: u64 },
        Windows { volume_serial: u32, file_index: u64 },
    }
}

#[cfg(target_os = "windows")]
#[path = "../../../src/cap_fs.rs"]
pub mod cap_fs;

#[cfg(target_os = "windows")]
pub mod ooxml {
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct WorkbookSafetyFacts {
        pub has_vba_project: bool,
        pub has_xlm_macro_sheet: bool,
        pub has_workbook_protection: bool,
        pub external_link_count: u32,
        pub connection_count: u32,
        pub query_table_count: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WorksheetVisibility {
        Visible,
        Hidden,
        VeryHidden,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct WorksheetDescriptor {
        pub workbook_index: u32,
        pub sheet_id: u32,
        pub relationship_id: String,
        pub relationship_type: String,
        pub part_path: String,
        pub name: String,
        pub visibility: WorksheetVisibility,
    }
}

#[cfg(target_os = "windows")]
pub mod progress {
    #[derive(Clone, Default)]
    pub struct CancellationToken;

    impl CancellationToken {
        pub fn is_cancelled(&self) -> bool {
            false
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn platform_pdf_api_fixture() {
    let _ = pdf::inspect_bytes;
    let _ = pdf::MAX_PDF_BYTES;
}

#[cfg(target_os = "windows")]
#[path = "../../../src/platform/windows/printing.rs"]
pub mod printing_impl;

#[cfg(target_os = "windows")]
#[path = "../../../src/platform/controlled_submit.rs"]
pub(crate) mod controlled_submit_impl;

#[cfg(target_os = "windows")]
#[path = "../../../src/platform/excel_windows_contract.rs"]
pub mod excel_windows_contract_impl;

#[cfg(target_os = "windows")]
#[path = "../../../src/platform/excel.rs"]
pub mod excel_impl;

#[cfg(target_os = "windows")]
#[path = "../../../src/platform/windows/excel.rs"]
pub mod excel_windows_impl;

#[cfg(target_os = "windows")]
pub mod platform {
    pub(crate) use crate::controlled_submit_impl as controlled_submit;

    pub(crate) fn consume_controlled_page_bytes<T>(
        render: impl FnOnce() -> Result<Vec<u8>, crate::print_queue::PrintFailure>,
        consume: impl FnOnce(&[u8]) -> Result<T, crate::print_queue::PrintFailure>,
    ) -> Result<T, crate::print_queue::PrintFailure> {
        let rendered = render()?;
        consume(&rendered)
    }

    pub(crate) fn map_controlled_print_failure(
        mut failure: crate::print_queue::PrintFailure,
    ) -> crate::print_queue::PrintFailure {
        if failure.message.starts_with("error.print.") {
            return failure;
        }
        failure.message = match failure.kind {
            crate::print_queue::PrintFailureKind::Failed => {
                "error.print.excelPrintPreparationFailed"
            }
            crate::print_queue::PrintFailureKind::Unsupported => {
                "error.print.excelControlledPdfUnavailable"
            }
            crate::print_queue::PrintFailureKind::UnsupportedOptions => {
                "error.print.excelPrintOptionsRejected"
            }
            crate::print_queue::PrintFailureKind::Cancelled if failure.uncertain => {
                "error.print.excelPrintCancelledUnconfirmed"
            }
            crate::print_queue::PrintFailureKind::Cancelled => "error.print.excelExportCancelled",
            crate::print_queue::PrintFailureKind::Uncertain => {
                "error.print.excelPrintCompletionUnconfirmed"
            }
        }
        .to_string();
        failure
    }


    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum WindowsSubmittedJobStatus {
        Complete,
        Uncertain,
        Pending,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum WindowsPostSubmissionObservation {
        PrinterOpen,
        JobEnumeration,
        JobRead,
        StatusFault,
    }

    pub(crate) fn windows_post_submission_observation_error(
        stage: WindowsPostSubmissionObservation,
    ) -> crate::print_queue::PrintFailure {
        let message = match stage {
            WindowsPostSubmissionObservation::PrinterOpen => {
                "Windows could not open the printer after job submission"
            }
            WindowsPostSubmissionObservation::JobEnumeration => {
                "Windows print queue could not be enumerated after job submission"
            }
            WindowsPostSubmissionObservation::JobRead => {
                "Windows spool job state could not be read after submission"
            }
            WindowsPostSubmissionObservation::StatusFault => {
                "Windows spooler reported a fault after the job was submitted"
            }
        };
        crate::print_queue::PrintFailure::uncertain(message)
    }

    pub(crate) fn windows_post_submission_failure(
        failure: crate::print_queue::PrintFailure,
    ) -> crate::print_queue::PrintFailure {
        if failure.kind == crate::print_queue::PrintFailureKind::Cancelled {
            crate::print_queue::PrintFailure::cancelled(failure.message, true)
        } else {
            crate::print_queue::PrintFailure::uncertain(failure.message)
        }
    }

    pub(crate) fn windows_post_submission_cancellation(
        request_accepted: bool,
    ) -> crate::print_queue::PrintFailure {
        crate::print_queue::PrintFailure::cancelled(
            if request_accepted {
                "Windows accepted cancellation of an active spool job"
            } else {
                "Windows could not confirm cancellation of an active spool job"
            },
            true,
        )
    }

    pub(crate) const fn classify_windows_submitted_job_status(
        status: u32,
    ) -> WindowsSubmittedJobStatus {
        const UNCERTAIN: u32 = 0x0000_0002
            | 0x0000_0004
            | 0x0000_0020
            | 0x0000_0040
            | 0x0000_0100
            | 0x0000_0200
            | 0x0000_0400;
        const COMPLETE: u32 = 0x0000_0080 | 0x0000_1000;

        if status & UNCERTAIN != 0 {
            WindowsSubmittedJobStatus::Uncertain
        } else if status & COMPLETE != 0 {
            WindowsSubmittedJobStatus::Complete
        } else {
            WindowsSubmittedJobStatus::Pending
        }
    }

    pub(crate) fn windows_missing_submitted_job() -> crate::print_queue::PrintFailure {
        crate::print_queue::PrintFailure::uncertain(
            "Windows can no longer observe the submitted spool job",
        )
    }

    pub(crate) fn validate_windows_image_frame_count(
        frame_count: u32,
    ) -> Result<(), crate::print_queue::PrintFailure> {
        if frame_count == 1 {
            Ok(())
        } else {
            Err(crate::print_queue::PrintFailure::unsupported(
                "Windows controlled image printing requires exactly one image frame",
            ))
        }
    }

    pub use crate::excel_impl as excel;
    pub use crate::excel_windows_contract_impl as excel_windows_contract;

    pub mod windows {
        pub use crate::excel_windows_impl as excel;
        pub use crate::printing_impl as printing;
    }
}

#[cfg(target_os = "windows")]
pub use printing_impl as printing;

#[cfg(target_os = "windows")]
pub fn platform_page_range_api_fixture() {
    let settings = print_queue::PrintSettings {
        paper: "A4".to_string(),
        orientation: "auto".to_string(),
        scaling: "fit".to_string(),
        color: "printer".to_string(),
        duplex: "printer".to_string(),
        page_range: "all".to_string(),
        timeout_minutes: 30,
    };
    let pdf = print_queue::PrintJobSupport::controlled_pdf(Some(4));
    let image = print_queue::PrintJobSupport::single_page();

    let _fixture = (settings, pdf, image);
}

#[cfg(target_os = "windows")]
pub fn platform_excel_api_fixture() {
    let _ = pdf::inspect_bytes;
    let _ = excel_windows_impl::production_excel_pipeline();
    let _ = platform::excel::ExcelExportPipeline::export_and_submit;
    let _ = cap_fs::CapabilityRoot::canonical;
    let _ = cap_fs::CapabilityRoot::identity;
    let _ = cap_fs::CapabilityRoot::authorize_directory;
    let _ = cap_fs::AuthorizedPath::with_expected_identity;
    let _ = cap_fs::AuthorizedPath::missing_directories;
    let _ = cap_fs::MissingDirectoryCursor::next_path;
    let _ = cap_fs::MissingDirectoryCursor::create_next;
    let _ = |error: cap_fs::CapabilityEffectError| {
        let _ = error.is_outcome_unknown();
        let _ = error.into_inner();
    };
    let _ = cap_fs::AnchoredPath::absolute;
    let _ = cap_fs::AnchoredPath::open_mutable;
    let _ = cap_fs::AnchoredPath::identity;
    let _ = cap_fs::AnchoredPath::same_file_as;
    let _ = cap_fs::AnchoredPath::remove_file;
    let _ = cap_fs::AnchoredPath::remove_dir;
    let _ = cap_fs::AnchoredPath::remove_file_durable;
    let _ = cap_fs::AnchoredPath::remove_dir_durable;
    let _ = cap_fs::AnchoredPath::copy_no_clobber_from;
    let _ = cap_fs::AnchoredPath::write_no_clobber;
    let _ = cap_fs::AnchoredPath::move_no_clobber_to;
    let _ = cap_fs::AnchoredPath::replace_with;
    let _ = cap_fs::AnchoredPath::temporary_sibling;
    let _ = platform::excel::ExportedWorksheetPdf::validated_bytes;
}
