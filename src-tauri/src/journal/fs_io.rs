use super::*;

#[cfg(unix)]
pub(super) fn atomic_replace(temporary: &Path, target: &Path) -> Result<(), BackendError> {
    fs::rename(temporary, target)
        .map_err(|error| journal_io("journal could not be atomically replaced", error))
}

#[cfg(windows)]
pub(super) fn atomic_replace(temporary: &Path, target: &Path) -> Result<(), BackendError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    retry_windows_atomic_replace(
        || {
            // SAFETY: temporary and target are NUL-terminated UTF-16 paths that live across the call.
            let result = unsafe {
                MoveFileExW(
                    temporary.as_ptr(),
                    target.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            };
            if result == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        },
        std::thread::sleep,
    )
    .map_err(|error| journal_io("journal could not be atomically replaced", error))
}

#[cfg(windows)]
pub(super) fn retry_windows_atomic_replace<R, W>(mut replace: R, mut wait: W) -> io::Result<()>
where
    R: FnMut() -> io::Result<()>,
    W: FnMut(std::time::Duration),
{
    const MAX_ATTEMPTS: usize = 20;
    const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

    for attempt in 0..MAX_ATTEMPTS {
        match replace() {
            Ok(()) => return Ok(()),
            Err(error)
                if matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && attempt + 1 < MAX_ATTEMPTS =>
            {
                wait(RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("journal replacement retry loop always returns")
}

#[cfg(unix)]
pub(super) fn sync_directory(path: &Path) -> Result<(), BackendError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| journal_io("journal directory could not be synchronized", error))
}

#[cfg(windows)]
pub(super) fn sync_directory(_path: &Path) -> Result<(), BackendError> {
    Ok(())
}

pub(super) fn journal_io(context: &str, error: io::Error) -> BackendError {
    BackendError::journal_invalid(format!("{context}: {error}"))
}
