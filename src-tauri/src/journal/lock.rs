use super::*;

#[cfg(unix)]
pub(super) fn try_lock_mutation_file(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
pub(super) fn try_lock_mutation_file(file: &File) -> io::Result<()> {
    use std::{mem::zeroed, os::windows::io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx},
        System::IO::OVERLAPPED,
    };

    let mut overlapped: OVERLAPPED = unsafe { zeroed() };
    if unsafe {
        LockFileEx(
            file.as_raw_handle() as HANDLE,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
pub(super) fn mutation_lock_contended(error: &io::Error) -> bool {
    error
        .raw_os_error()
        .is_some_and(|code| code == libc::EWOULDBLOCK || code == libc::EAGAIN)
}

/// Tries the lock for up to about 200 ms before reporting contention. A child
/// process spawned by another thread can hold a copy of the lock file descriptor
/// until it execs, which keeps the `flock` alive for a few milliseconds after
/// this process has released it. Real holders still fail after the window.
pub(super) fn try_lock_mutation_file_briefly(file: &File) -> io::Result<()> {
    const ATTEMPTS: u32 = 200;
    let mut result = try_lock_mutation_file(file);
    for _ in 1..ATTEMPTS {
        match &result {
            Err(error) if mutation_lock_contended(error) => {}
            _ => break,
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        result = try_lock_mutation_file(file);
    }
    result
}

#[cfg(windows)]
pub(super) fn mutation_lock_contended(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32 | 33 | 158))
}

pub(super) fn pending_disposition(step: &Step) -> RecoveryDisposition {
    let src_exists = step
        .src
        .as_deref()
        .is_some_and(|path| Path::new(path).exists());
    let dst_exists = step
        .dst
        .as_deref()
        .is_some_and(|path| Path::new(path).exists());
    match step.kind {
        crate::contracts::StepKind::Mkdir if !dst_exists => RecoveryDisposition::NoEffect,
        crate::contracts::StepKind::Copy if !dst_exists => RecoveryDisposition::NoEffect,
        crate::contracts::StepKind::Move | crate::contracts::StepKind::Rename
            if src_exists && !dst_exists =>
        {
            RecoveryDisposition::NoEffect
        }
        _ => RecoveryDisposition::OutcomeUnknown,
    }
}

pub(super) fn legacy_recovery_entry(record: &JournalRecord) -> Option<RecoveryEntry> {
    let disposition = if let Some(step) = record.pending_step.as_ref() {
        pending_disposition(step)
    } else if record.pending_undo.is_some() || record.undo_recovery.is_some() {
        RecoveryDisposition::OutcomeUnknown
    } else {
        return None;
    };
    Some(RecoveryEntry {
        journal_id: record.journal_id.clone(),
        disposition,
        warning_key: None,
        locator: None,
        restore_original_available: false,
    })
}
