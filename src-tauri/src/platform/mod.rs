use std::sync::Arc;

#[cfg(any(target_os = "windows", test))]
use std::collections::HashSet;

use crate::{outlook_reply_draft::OutlookAdapter, print_queue::PrintAdapter};

#[cfg(not(target_os = "windows"))]
use crate::outlook_reply_draft::{
    DraftIdentity, MailItemIdentity, OutlookDraft, OutlookFailure, SelectedMailItem,
};

#[cfg(any(target_os = "windows", test))]
pub(crate) mod controlled_submit;
#[cfg(any(target_os = "windows", test))]
pub(crate) mod excel;
#[cfg(any(target_os = "windows", test))]
pub(crate) mod excel_windows_contract;

#[cfg(any(target_os = "windows", test))]
pub(crate) fn consume_controlled_page_bytes<T>(
    render: impl FnOnce() -> Result<Vec<u8>, crate::print_queue::PrintFailure>,
    consume: impl FnOnce(&[u8]) -> Result<T, crate::print_queue::PrintFailure>,
) -> Result<T, crate::print_queue::PrintFailure> {
    let rendered = render()?;
    consume(&rendered)
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn map_controlled_print_failure(
    mut failure: crate::print_queue::PrintFailure,
) -> crate::print_queue::PrintFailure {
    if failure.message.starts_with("error.print.") {
        return failure;
    }
    failure.message = match failure.kind {
        crate::print_queue::PrintFailureKind::Failed => "error.print.excelPrintPreparationFailed",
        crate::print_queue::PrintFailureKind::Unsupported => {
            "error.print.excelControlledPdfUnavailable"
        }
        crate::print_queue::PrintFailureKind::UnsupportedOptions => {
            "error.print.excelPrintOptionsRejected"
        }
        crate::print_queue::PrintFailureKind::SubmittedUnconfirmed => {
            "error.print.excelPrintCompletionUnconfirmed"
        }
        crate::print_queue::PrintFailureKind::Uncertain => {
            "error.print.excelPrintCompletionUnconfirmed"
        }
        crate::print_queue::PrintFailureKind::Cancelled if failure.uncertain => {
            "error.print.excelPrintCancelledUnconfirmed"
        }
        crate::print_queue::PrintFailureKind::Cancelled => "error.print.excelExportCancelled",
    }
    .to_string();
    failure
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsSubmittedJobStatus {
    Complete,
    Uncertain,
    Pending,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsPostSubmissionObservation {
    PrinterOpen,
    JobEnumeration,
    JobRead,
    StatusFault,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsJobQueryError {
    Released,
    RetryWithLargerBuffer,
    Uncertain,
}

#[cfg(any(target_os = "windows", test))]
pub(crate) const fn classify_windows_job_query_error(error: u32) -> WindowsJobQueryError {
    match error {
        2 | 87 => WindowsJobQueryError::Released,
        122 => WindowsJobQueryError::RetryWithLargerBuffer,
        _ => WindowsJobQueryError::Uncertain,
    }
}

#[cfg(any(target_os = "windows", test))]
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
    if stage == WindowsPostSubmissionObservation::StatusFault {
        crate::print_queue::PrintFailure::uncertain(message)
    } else {
        crate::print_queue::PrintFailure::submitted_unconfirmed(message)
    }
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn windows_post_submission_failure(
    failure: crate::print_queue::PrintFailure,
) -> crate::print_queue::PrintFailure {
    if failure.kind == crate::print_queue::PrintFailureKind::Cancelled {
        crate::print_queue::PrintFailure::cancelled(failure.message, true)
    } else {
        crate::print_queue::PrintFailure::uncertain(failure.message)
    }
}

#[cfg(any(target_os = "windows", test))]
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

#[cfg(any(target_os = "windows", test))]
pub(crate) const fn classify_windows_submitted_job_status(
    status: u32,
) -> WindowsSubmittedJobStatus {
    // JOB_STATUS_* is a stable Win32 bitmask. Generic error, offline, paper-out,
    // blocked, and user-intervention states can be transient or stale while the
    // device wakes or refreshes consumable state, so observation waits until
    // release, completion, cancellation, or timeout. Only destructive deletion
    // states make the already-submitted outcome immediately uncertain.
    const UNCERTAIN: u32 = 0x0000_0004 // DELETING
        | 0x0000_0100; // DELETED
    const COMPLETE: u32 = 0x0000_0080 // PRINTED
        | 0x0000_1000; // COMPLETE

    if status & UNCERTAIN != 0 {
        WindowsSubmittedJobStatus::Uncertain
    } else if status & COMPLETE != 0 {
        WindowsSubmittedJobStatus::Complete
    } else {
        WindowsSubmittedJobStatus::Pending
    }
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn select_windows_submitted_job(
    known_ids: &HashSet<u32>,
    candidates: &[(u32, String)],
    expected_title: &str,
) -> Option<u32> {
    let fresh = candidates
        .iter()
        .filter(|(id, _)| !known_ids.contains(id))
        .collect::<Vec<_>>();
    let expected_title = expected_title.to_ascii_lowercase();
    let named = fresh
        .iter()
        .filter(|(_, document)| {
            let document = document.to_ascii_lowercase();
            !document.is_empty()
                && (document.contains(&expected_title) || expected_title.contains(&document))
        })
        .collect::<Vec<_>>();
    if named.len() == 1 {
        Some(named[0].0)
    } else if named.is_empty() && fresh.len() == 1 {
        Some(fresh[0].0)
    } else {
        None
    }
}

#[cfg(any(target_os = "windows", test))]
pub(crate) const fn classify_windows_submitted_job_observation(
    status: Option<u32>,
) -> WindowsSubmittedJobStatus {
    match status {
        // GetJob returns FILE_NOT_FOUND/INVALID_PARAMETER after many drivers
        // release a completed job. This helper is called only after EndDoc has
        // successfully transferred the complete document to the spooler.
        None => WindowsSubmittedJobStatus::Complete,
        Some(status) => classify_windows_submitted_job_status(status),
    }
}

#[cfg(any(target_os = "windows", test))]
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

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub fn production_print_adapter() -> Arc<dyn PrintAdapter> {
    Arc::new(macos::printing::MacOsPrintAdapter)
}

#[cfg(target_os = "windows")]
pub fn production_print_adapter() -> Arc<dyn PrintAdapter> {
    Arc::new(windows::printing::WindowsPrintAdapter)
}

#[cfg(target_os = "windows")]
pub fn production_outlook_adapter() -> Arc<dyn OutlookAdapter> {
    Arc::new(windows::outlook::WindowsOutlookAdapter)
}

#[cfg(not(target_os = "windows"))]
pub fn production_outlook_adapter() -> Arc<dyn OutlookAdapter> {
    Arc::new(UnsupportedOutlookAdapter)
}

#[cfg(not(target_os = "windows"))]
struct UnsupportedOutlookAdapter;

#[cfg(not(target_os = "windows"))]
impl OutlookAdapter for UnsupportedOutlookAdapter {
    fn selected_mail_item(&self) -> Result<SelectedMailItem, OutlookFailure> {
        Err(outlook_unsupported())
    }

    fn reply_all(
        &self,
        _expected: &MailItemIdentity,
    ) -> Result<Box<dyn OutlookDraft>, OutlookFailure> {
        Err(outlook_unsupported())
    }

    fn delete_unchanged_draft(&self, _draft: &DraftIdentity) -> Result<(), OutlookFailure> {
        Err(outlook_unsupported())
    }
}

#[cfg(not(target_os = "windows"))]
fn outlook_unsupported() -> OutlookFailure {
    OutlookFailure::unsupported("error.rfi.outlookDraft.unsupportedPlatform")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn production_print_adapter() -> Arc<dyn PrintAdapter> {
    Arc::new(UnsupportedPrintAdapter)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct UnsupportedPrintAdapter;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl PrintAdapter for UnsupportedPrintAdapter {
    fn inventory(
        &self,
    ) -> Result<crate::print_queue::PrinterInventory, crate::print_queue::PrintFailure> {
        Ok(crate::print_queue::PrinterInventory::default())
    }

    fn inspect(
        &self,
        _path: &std::path::Path,
        _printer: &str,
        _settings: &crate::print_queue::PrintSettings,
    ) -> Result<crate::print_queue::PrintJobSupport, crate::print_queue::PrintFailure> {
        Err(crate::print_queue::PrintFailure::unsupported(
            "printing is unsupported on this platform",
        ))
    }

    fn submit(
        &self,
        _job: &crate::print_queue::PrintJob,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<(), crate::print_queue::PrintFailure> {
        Err(crate::print_queue::PrintFailure::failed(
            "printing is unsupported on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_route_maps_raw_helper_failures_before_and_after_start_doc() {
        let cases = [
            (
                crate::print_queue::PrintFailure::failed("raw devmode failure"),
                crate::print_queue::PrintFailureKind::Failed,
                false,
                "error.print.excelPrintPreparationFailed",
            ),
            (
                crate::print_queue::PrintFailure::unsupported_options("raw option failure"),
                crate::print_queue::PrintFailureKind::UnsupportedOptions,
                false,
                "error.print.excelPrintOptionsRejected",
            ),
            (
                windows_post_submission_failure(crate::print_queue::PrintFailure::failed(
                    "raw renderer failure",
                )),
                crate::print_queue::PrintFailureKind::Uncertain,
                true,
                "error.print.excelPrintCompletionUnconfirmed",
            ),
            (
                windows_post_submission_failure(crate::print_queue::PrintFailure::cancelled(
                    "raw cancellation failure",
                    false,
                )),
                crate::print_queue::PrintFailureKind::Cancelled,
                true,
                "error.print.excelPrintCancelledUnconfirmed",
            ),
        ];

        for (raw, expected_kind, expected_uncertainty, expected_key) in cases {
            let mapped = map_controlled_print_failure(raw);
            assert_eq!(mapped.kind, expected_kind);
            assert_eq!(mapped.uncertain, expected_uncertainty);
            assert_eq!(mapped.message, expected_key);
        }

        let already_stable =
            crate::print_queue::PrintFailure::failed("error.print.excelPrinterContextUnavailable");
        assert_eq!(
            map_controlled_print_failure(already_stable.clone()),
            already_stable
        );
    }

    #[test]
    fn controlled_route_error_keys_have_all_operator_localizations() {
        let catalogs = [
            include_str!("../../../frontend/src/locales/app_en.arb"),
            include_str!("../../../frontend/src/locales/app_az.arb"),
            include_str!("../../../frontend/src/locales/app_ru.arb"),
            include_str!("../../../frontend/src/locales/app_tr.arb"),
        ];
        let keys = [
            "error.print.excelPrintPreparationFailed",
            "error.print.excelPrintOptionsRejected",
            "error.print.excelPrintCancelledUnconfirmed",
            "error.print.excelPrintCompletionUnconfirmed",
        ];

        for catalog in catalogs {
            let catalog: serde_json::Value = serde_json::from_str(catalog).unwrap();
            for key in keys {
                let translation = catalog[key]
                    .as_str()
                    .expect("localized controlled-route key");
                assert!(!translation.trim().is_empty());
                assert_ne!(translation, key);
            }
        }
    }

    #[test]
    fn controlled_page_adapter_consumes_exact_rendered_bytes_without_path_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let ambient = directory.path().join("page.png");
        std::fs::write(&ambient, b"validated rendered page").unwrap();
        let consumed = std::sync::Mutex::new(Vec::new());

        consume_controlled_page_bytes(
            || Ok(std::fs::read(&ambient).unwrap()),
            |bytes| {
                let displaced = ambient.with_extension("approved-original");
                std::fs::rename(&ambient, displaced).unwrap();
                std::fs::write(&ambient, b"substituted rendered page").unwrap();
                consumed.lock().unwrap().extend_from_slice(bytes);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(*consumed.lock().unwrap(), b"validated rendered page");
        assert_eq!(
            std::fs::read(ambient).unwrap(),
            b"substituted rendered page"
        );
    }

    #[test]
    fn submitted_windows_destructive_terminal_statuses_are_uncertain() {
        for status in [0x0000_0004, 0x0000_0100] {
            assert_eq!(
                classify_windows_submitted_job_status(status),
                WindowsSubmittedJobStatus::Uncertain
            );
        }
        assert_eq!(
            classify_windows_submitted_job_status(0x0000_0002),
            WindowsSubmittedJobStatus::Pending,
            "a transient generic driver error must be observed until release or timeout"
        );
        assert_eq!(
            classify_windows_submitted_job_status(0x0000_0002 | 0x0000_1000),
            WindowsSubmittedJobStatus::Complete,
            "an explicit completion bit must win over a stale generic driver error"
        );
        assert_eq!(
            classify_windows_submitted_job_status(0x0000_0080),
            WindowsSubmittedJobStatus::Complete
        );
        assert_eq!(
            classify_windows_submitted_job_status(0x0000_0010),
            WindowsSubmittedJobStatus::Pending
        );
    }

    #[test]
    fn native_handler_job_selection_accepts_one_new_job_without_trusting_ambiguous_jobs() {
        let known = std::collections::HashSet::from([7, 8]);
        assert_eq!(
            select_windows_submitted_job(
                &known,
                &[(7, "older".to_string()), (9, "Microsoft Word".to_string())],
                "report.docx",
            ),
            Some(9),
            "a native handler may replace the filename with an application title"
        );
        assert_eq!(
            select_windows_submitted_job(
                &known,
                &[
                    (9, "Microsoft Word".to_string()),
                    (10, "unrelated".to_string()),
                ],
                "report.docx",
            ),
            None,
            "multiple unnamed new jobs are ambiguous"
        );
        assert_eq!(
            select_windows_submitted_job(
                &known,
                &[
                    (9, "report.docx".to_string()),
                    (10, "unrelated".to_string()),
                ],
                "report.docx",
            ),
            Some(9),
            "one exact title match remains authoritative among concurrent jobs"
        );
    }

    #[test]
    fn submitted_windows_recoverable_device_statuses_keep_waiting() {
        for status in [
            0x0000_0001, // PAUSED
            0x0000_0008, // SPOOLING
            0x0000_0010, // PRINTING
            0x0000_0020, // OFFLINE
            0x0000_0040, // PAPEROUT
            0x0000_0200, // BLOCKED_DEVQ
            0x0000_0400, // USER_INTERVENTION
            0x0000_2000, // RETAINED
        ] {
            assert_eq!(
                classify_windows_submitted_job_status(status),
                WindowsSubmittedJobStatus::Pending
            );
        }
    }

    #[test]
    fn submitted_windows_job_released_after_end_doc_is_complete() {
        assert_eq!(
            classify_windows_submitted_job_observation(None),
            WindowsSubmittedJobStatus::Complete
        );
        assert_eq!(
            classify_windows_submitted_job_observation(Some(0x0000_0002)),
            WindowsSubmittedJobStatus::Pending
        );
        assert_eq!(
            classify_windows_job_query_error(2),
            WindowsJobQueryError::Released
        );
        assert_eq!(
            classify_windows_job_query_error(87),
            WindowsJobQueryError::Released
        );
        assert_eq!(
            classify_windows_job_query_error(122),
            WindowsJobQueryError::RetryWithLargerBuffer
        );
        assert_eq!(
            classify_windows_job_query_error(5),
            WindowsJobQueryError::Uncertain
        );
    }

    #[test]
    fn post_submission_observation_loss_continues_but_explicit_fault_is_uncertain() {
        for stage in [
            WindowsPostSubmissionObservation::PrinterOpen,
            WindowsPostSubmissionObservation::JobEnumeration,
            WindowsPostSubmissionObservation::JobRead,
        ] {
            let failure = windows_post_submission_observation_error(stage);
            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::SubmittedUnconfirmed
            );
            assert!(failure.uncertain);
        }
        let fault = windows_post_submission_observation_error(
            WindowsPostSubmissionObservation::StatusFault,
        );
        assert_eq!(fault.kind, crate::print_queue::PrintFailureKind::Uncertain);
        assert!(fault.uncertain);
    }

    #[test]
    fn every_windows_render_failure_after_start_doc_is_uncertain() {
        for failure in [
            crate::print_queue::PrintFailure::failed("render failed"),
            crate::print_queue::PrintFailure::unsupported("decoder failed"),
            crate::print_queue::PrintFailure::unsupported_options("device mode changed"),
            crate::print_queue::PrintFailure::uncertain("already uncertain"),
        ] {
            let classified = windows_post_submission_failure(failure);
            assert_eq!(
                classified.kind,
                crate::print_queue::PrintFailureKind::Uncertain
            );
            assert!(classified.uncertain);
        }

        let cancelled = windows_post_submission_failure(
            crate::print_queue::PrintFailure::cancelled("render cancelled", false),
        );
        assert_eq!(
            cancelled.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(cancelled.uncertain);
    }

    #[test]
    fn windows_post_submission_cancellation_is_uncertain_even_when_request_is_accepted() {
        for request_accepted in [false, true] {
            let failure = windows_post_submission_cancellation(request_accepted);
            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::Cancelled
            );
            assert!(failure.uncertain);
        }
    }

    #[test]
    fn windows_image_frame_count_preflight_requires_exactly_one_frame() {
        assert!(validate_windows_image_frame_count(1).is_ok());

        for frame_count in [0, 2] {
            let failure = validate_windows_image_frame_count(frame_count).unwrap_err();
            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::Unsupported
            );
            assert_eq!(
                failure.message,
                "Windows controlled image printing requires exactly one image frame"
            );
            assert!(!failure.uncertain);
        }
    }

    #[test]
    fn windows_image_frame_preflight_precedes_printer_context_and_spool_submission() {
        let source = include_str!("windows/printing.rs");
        let print_image = source
            .split("fn print_image(")
            .nth(1)
            .expect("Windows image print implementation")
            .split("fn draw_image_frames(")
            .next()
            .expect("Windows image print body");
        let preflight = print_image
            .find("validate_windows_image_frame_count(frame_count)?")
            .expect("image frame-count preflight");

        assert!(preflight < print_image.find("validated_devmode(").unwrap());
        assert!(preflight < print_image.find("CreateDCW(").unwrap());
        assert!(preflight < print_image.find("StartDocW(").unwrap());
    }

    #[test]
    fn windows_image_inspection_preflights_frames_before_driver_validation() {
        let source = include_str!("windows/printing.rs");
        let inspect = source
            .split("fn inspect(")
            .nth(1)
            .expect("Windows image inspection implementation")
            .split("fn submit(")
            .next()
            .expect("Windows image inspection body");
        let image_branch = inspect
            .split("if CONTROLLED_IMAGE_FORMATS.contains(&extension.as_str())")
            .nth(1)
            .expect("controlled image inspection branch")
            .split("if !safe_extension")
            .next()
            .expect("controlled image inspection body");
        let preflight = image_branch
            .find("validate_windows_image_frame_count(frame_count)?")
            .expect("image frame-count preflight");

        assert!(preflight < image_branch.find("validated_devmode(").unwrap());
    }

    #[test]
    fn windows_root_identity_uses_volume_serial_and_file_index_from_the_open_handle() {
        let source = include_str!("../cap_fs.rs");
        let implementation = source
            .split("fn windows_file_identity(")
            .nth(1)
            .expect("Windows root identity implementation");
        for required in [
            "GetFileInformationByHandle",
            "dwVolumeSerialNumber",
            "nFileIndexHigh",
            "nFileIndexLow",
        ] {
            assert!(implementation.contains(required), "missing {required}");
        }
    }

    #[test]
    fn windows_excel_adapter_is_direct_late_bound_com_without_process_or_visibility_mutation() {
        let source = include_str!("windows/excel.rs");
        for required in [
            "CoInitializeEx",
            "CoUninitialize",
            "CLSIDFromProgID",
            "CoCreateInstance",
            "IDispatch",
            "DISPPARAMS",
            "VARIANT",
            "DISPID_PROPERTYPUT",
            "DISP_E_PARAMNOTFOUND",
            "encode_dispatch_parameters",
            "build_com_operation_plan",
            "validate_dispatch_member",
            "open_zone_identifier",
            "into_exported_pdf",
            "quarantine_after_teardown_failure",
        ] {
            assert!(source.contains(required), "missing {required}");
        }
        for forbidden in [
            "powershell",
            "vbscript",
            "std::process",
            "Command::new",
            "Worksheet.PrintOut",
            "CalculateFull",
            "RefreshAll",
            "UpdateLink",
            "ProtectedViewWindows",
            "Kill",
        ] {
            assert!(!source.contains(forbidden), "forbidden {forbidden}");
        }
    }

    #[test]
    fn windows_platform_check_compiles_real_excel_capability_and_supervisor_boundaries() {
        let harness = include_str!("../../platform-checks/windows-printing/src/lib.rs");
        assert!(harness.contains("../../../src/cap_fs.rs"));
        assert!(harness.contains("../../../src/platform/excel.rs"));
        assert!(!harness.contains("pub mod cap_fs {"));
        assert!(!harness.contains("pub(crate) struct ExcelExportRequest"));
        assert!(!harness.contains("pub(crate) struct StagedWorkbook"));
    }

    #[test]
    fn production_excel_pipeline_uses_only_validated_in_memory_controlled_pdf_submission() {
        let excel = include_str!("windows/excel.rs");
        let construction = excel
            .split("pub(crate) fn production_excel_pipeline()")
            .nth(1)
            .expect("production Excel pipeline constructor")
            .split("fn bounded_path(")
            .next()
            .expect("bounded production Excel construction");
        assert!(construction.contains("WindowsControlledPdfSubmitter"));
        assert!(!construction.contains("ControlledPdfUnavailable"));

        let printing = include_str!("windows/printing.rs");
        let submitter = printing
            .split("impl ControlledPdfSubmitter for WindowsControlledPdfSubmitter")
            .nth(1)
            .expect("controlled PDF submitter implementation")
            .split("fn enumerate_printers(")
            .next()
            .expect("bounded controlled PDF submitter implementation");
        assert!(submitter.contains("pdf.validated_bytes()?"));
        assert!(submitter.contains("submit_controlled_pdf("));
        for forbidden in [
            "pdf.pdf_path()",
            "job.path",
            "WindowsPrintAdapter",
            ".inspect(",
            ".submit(",
            "print_with_native_application",
        ] {
            assert!(
                !submitter.contains(forbidden),
                "controlled PDF submitter used forbidden route {forbidden}"
            );
        }
    }

    #[test]
    fn production_print_service_installs_excel_pipeline_only_on_windows() {
        let source = include_str!("../native.rs");
        let construction = source
            .split("let print = PrintService::new(")
            .nth(1)
            .expect("production PrintService constructor")
            .split("Ok(Self {")
            .next()
            .expect("bounded production construction");
        assert!(construction.contains("platform::production_print_adapter()"));
        assert!(construction.contains("#[cfg(target_os = \"windows\")]"));
        assert!(construction.contains(
            ".with_excel_pipeline(platform::windows::excel::production_excel_pipeline())"
        ));
    }
}
