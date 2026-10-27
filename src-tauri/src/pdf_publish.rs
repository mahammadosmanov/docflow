#[cfg(any(test, unix))]
use std::fs;
#[cfg(any(test, windows))]
use std::io;
use std::path::Path;
#[cfg(test)]
use std::{
    fs::{File, OpenOptions},
    time::Duration,
};

#[cfg(test)]
pub(crate) fn publish_pdf_no_clobber_with<L, S>(
    temporary: &Path,
    destination: &Path,
    attempts: usize,
    mut link: L,
    mut wait: S,
) -> io::Result<()>
where
    L: FnMut(&Path, &Path) -> io::Result<()>,
    S: FnMut(Duration),
{
    let attempts = attempts.max(1);
    for attempt in 0..attempts {
        match link(temporary, destination) {
            Ok(()) => {
                remove_with_access_retry(temporary, 40);
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Err(error),
            Err(error) if destination.exists() => {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, error));
            }
            Err(error) if is_temporary_pdf_access_error(&error) && attempt + 1 < attempts => {
                wait(Duration::from_millis(150));
            }
            Err(error)
                if is_temporary_pdf_access_error(&error)
                    || matches!(
                        error.kind(),
                        io::ErrorKind::Unsupported
                            | io::ErrorKind::CrossesDevices
                            | io::ErrorKind::PermissionDenied
                    ) =>
            {
                break;
            }
            Err(error) => return Err(error),
        }
    }
    copy_prepared_pdf_exclusive(temporary, destination)
}

#[cfg(test)]
pub(crate) fn remove_file_with_access_retry(path: &Path, attempts: usize) -> io::Result<()> {
    for attempt in 0..attempts {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) if is_temporary_pdf_access_error(&error) && attempt + 1 < attempts => {
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("temporary PDF could not be removed"))
}

pub(crate) fn temporary_pdf_name_matches(candidate: &str, destination: &str) -> bool {
    candidate.starts_with(&format!(".{destination}.docflow-")) && candidate.ends_with(".tmp")
}

#[cfg(unix)]
pub(crate) fn same_file_identity(left: &Path, right: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    fs::symlink_metadata(left)
        .and_then(|left| {
            fs::symlink_metadata(right)
                .map(|right| left.dev() == right.dev() && left.ino() == right.ino())
        })
        .unwrap_or(false)
}

#[cfg(windows)]
pub(crate) fn same_file_identity(left: &Path, right: &Path) -> bool {
    windows_file_identity(left)
        .and_then(|left| windows_file_identity(right).map(|right| left == right))
        .unwrap_or(false)
}

#[cfg(windows)]
fn windows_file_identity(path: &Path) -> io::Result<(u32, u64)> {
    use std::{mem::MaybeUninit, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            GetFileInformationByHandle, OPEN_EXISTING,
        },
    };

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: path is a NUL-terminated UTF-16 string that lives across the call; the handle is checked against INVALID_HANDLE_VALUE below.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: handle is a valid open handle, and information points to writable storage for the result.
    let result = unsafe { GetFileInformationByHandle(handle, information.as_mut_ptr()) };
    let error = (result == 0).then(io::Error::last_os_error);
    // SAFETY: handle was opened above and is closed exactly once, after the query.
    unsafe { CloseHandle(handle) };
    if let Some(error) = error {
        return Err(error);
    }
    // SAFETY: GetFileInformationByHandle succeeded above, so information is fully initialized.
    let information = unsafe { information.assume_init() };
    Ok((
        information.dwVolumeSerialNumber,
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    ))
}

#[cfg(test)]
fn copy_prepared_pdf_exclusive(temporary: &Path, destination: &Path) -> io::Result<()> {
    let mut input = open_source_no_follow(temporary)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| {
        io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(output);
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    drop(output);
    remove_with_access_retry(temporary, 40);
    Ok(())
}

#[cfg(test)]
fn remove_with_access_retry(path: &Path, attempts: usize) -> bool {
    remove_file_with_access_retry(path, attempts).is_ok()
}

#[cfg(test)]
fn is_temporary_pdf_access_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        || matches!(error.raw_os_error(), Some(5 | 13 | 16 | 32 | 33))
}

#[cfg(unix)]
#[cfg(test)]
fn open_source_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
#[cfg(test)]
fn open_source_no_follow(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}
