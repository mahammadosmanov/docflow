use super::*;

#[cfg(any(windows, test))]
pub(in crate::fs_ops) fn overwrite_existing_capability(
    source: &AnchoredPath,
    destination: &AnchoredPath,
    expected_destination_sha256: &str,
) -> io::Result<()> {
    destination.overwrite_verified_from(source, expected_destination_sha256)?;
    source.remove_file_durable()
}

pub(in crate::fs_ops) fn create_temporary(
    parent: &Path,
    destination: &Path,
) -> io::Result<(PathBuf, File)> {
    if !parent.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "destination parent does not exist",
        ));
    }
    for _ in 0..16 {
        let name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("file");
        let temporary = parent.join(format!(".{name}.docflow-{}.tmp", Uuid::new_v4()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary file",
    ))
}

pub(in crate::fs_ops) fn rollback_from_backup_capability(
    backup: &AnchoredPath,
    workbook: &AnchoredPath,
    expected_hash: &str,
) -> io::Result<()> {
    let temporary = workbook.temporary_sibling("restore")?;
    temporary.copy_no_clobber_from(backup)?;
    let result = (|| {
        if temporary.sha256()? != expected_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "workbook rollback copy did not match the approved backup",
            ));
        }
        atomic_replace_capability_with_retry(&temporary, workbook)?;
        if workbook.sha256()? != expected_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "workbook rollback verification failed",
            ));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = temporary.remove_file();
    }
    result
}

pub(in crate::fs_ops) fn atomic_replace_capability_with_retry(
    source: &AnchoredPath,
    destination: &AnchoredPath,
) -> io::Result<()> {
    for attempt in 0..20 {
        match destination.replace_with(source) {
            Ok(()) => return Ok(()),
            Err(error) if is_workbook_access_error(&error) && attempt < 19 => {
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("replacement retry loop always returns")
}

pub(crate) fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    atomic_replace_platform(source, destination)?;
    if let Some(parent) = destination.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

pub(in crate::fs_ops) fn is_workbook_access_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        || matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

pub(in crate::fs_ops) fn remove_anchored_file_with_access_retry(
    path: &AnchoredPath,
    attempts: usize,
) -> io::Result<()> {
    for attempt in 0..attempts.max(1) {
        match path.remove_file() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) if is_workbook_access_error(&error) && attempt + 1 < attempts.max(1) => {
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(
        "capability-relative file could not be removed",
    ))
}

#[cfg(unix)]
pub(in crate::fs_ops) fn atomic_replace_platform(
    source: &Path,
    destination: &Path,
) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
pub(in crate::fs_ops) fn atomic_replace_platform(
    source: &Path,
    destination: &Path,
) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: source and destination are NUL-terminated UTF-16 paths that live across the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(in crate::fs_ops) fn publish_no_clobber(
    temporary: &Path,
    destination: &Path,
) -> io::Result<()> {
    fs::hard_link(temporary, destination)?;
    fs::remove_file(temporary)
}

#[cfg(unix)]
pub(in crate::fs_ops) fn open_source_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
pub(in crate::fs_ops) fn open_source_no_follow(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(target_os = "macos")]
pub(in crate::fs_ops) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source contains NUL"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination contains NUL"))?;
    // SAFETY: source and destination are NUL-terminated paths that live across the call.
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
pub(in crate::fs_ops) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source contains NUL"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination contains NUL"))?;
    // SAFETY: source and destination are NUL-terminated paths that live across the call.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
pub(in crate::fs_ops) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: source and destination are NUL-terminated UTF-16 paths that live across the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
pub(in crate::fs_ops) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    if source.is_file() {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "atomic no-replace directory rename is unavailable",
        ))
    }
}

#[cfg(unix)]
pub(in crate::fs_ops) fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
pub(in crate::fs_ops) fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}
