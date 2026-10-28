use super::*;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsJobQueryError {
    Released,
    RetryWithLargerBuffer,
    Uncertain,
}

pub(crate) const fn classify_windows_job_query_error(error: u32) -> WindowsJobQueryError {
    match error {
        2 | 87 => WindowsJobQueryError::Released,
        122 => WindowsJobQueryError::RetryWithLargerBuffer,
        _ => WindowsJobQueryError::Uncertain,
    }
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
    if stage == WindowsPostSubmissionObservation::StatusFault {
        crate::print_queue::PrintFailure::uncertain(message)
    } else {
        crate::print_queue::PrintFailure::submitted_unconfirmed(message)
    }
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
