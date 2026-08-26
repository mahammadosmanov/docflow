use std::sync::Arc;

use crate::print_queue::PrintAdapter;

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
    // JOB_STATUS_* is a stable Win32 bitmask. Once submission has occurred,
    // none of these fault/deletion states proves that paper was not produced.
    const UNCERTAIN: u32 = 0x0000_0002 // ERROR
        | 0x0000_0004 // DELETING
        | 0x0000_0020 // OFFLINE
        | 0x0000_0040 // PAPEROUT
        | 0x0000_0100 // DELETED
        | 0x0000_0200 // BLOCKED_DEVQ
        | 0x0000_0400; // USER_INTERVENTION
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
pub(crate) fn windows_missing_submitted_job() -> crate::print_queue::PrintFailure {
    crate::print_queue::PrintFailure::uncertain(
        "Windows can no longer observe the submitted spool job",
    )
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
    fn submitted_windows_printer_fault_statuses_are_always_uncertain() {
        for status in [
            0x0000_0002,
            0x0000_0020,
            0x0000_0040,
            0x0000_0100,
            0x0000_0200,
            0x0000_0400,
        ] {
            assert_eq!(
                classify_windows_submitted_job_status(status),
                WindowsSubmittedJobStatus::Uncertain
            );
        }
        assert_eq!(
            classify_windows_submitted_job_status(0x0000_0002 | 0x0000_1000),
            WindowsSubmittedJobStatus::Uncertain,
            "a fault bit must win over a simultaneous completion bit"
        );
        assert_eq!(
            classify_windows_submitted_job_status(0x0000_0080),
            WindowsSubmittedJobStatus::Complete
        );
        assert_eq!(
            classify_windows_submitted_job_status(0x0000_0010),
            WindowsSubmittedJobStatus::Pending
        );
        assert!(windows_missing_submitted_job().uncertain);
    }

    #[test]
    fn every_windows_post_submission_observation_error_is_uncertain() {
        for stage in [
            WindowsPostSubmissionObservation::PrinterOpen,
            WindowsPostSubmissionObservation::JobEnumeration,
            WindowsPostSubmissionObservation::JobRead,
            WindowsPostSubmissionObservation::StatusFault,
        ] {
            let failure = windows_post_submission_observation_error(stage);
            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::Uncertain
            );
            assert!(failure.uncertain);
        }
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
    fn windows_source_routes_successful_abort_and_set_job_through_uncertain_classifiers() {
        let source = include_str!("windows/printing.rs");
        assert_eq!(
            source
                .matches("windows_post_submission_failure(error)")
                .count(),
            2,
            "both PDF and image render branches must classify errors after StartDocW"
        );
        assert!(source.contains("windows_post_submission_cancellation(cancel_requested)"));
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
}
