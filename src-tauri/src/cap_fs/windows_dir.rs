#![allow(unused_imports)]
use super::*;

#[cfg(windows)]
pub(super) struct PlatformDirectory(File);

#[cfg(windows)]
impl PlatformDirectory {
    pub(super) fn open_ambient(path: &Path, access: DirectoryAccess) -> io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::{
            Foundation::{GENERIC_READ, GENERIC_WRITE},
            Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
            },
        };

        let mut options = std::fs::OpenOptions::new();
        options
            .access_mode(
                GENERIC_READ
                    | FILE_TRAVERSE
                    | if access == DirectoryAccess::ReadWrite {
                        GENERIC_WRITE
                    } else {
                        0
                    },
            )
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .and_then(|file| {
                use std::os::windows::fs::MetadataExt;
                use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

                if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "capability root is a reparse point",
                    ))
                } else {
                    Ok(Self(file))
                }
            })
    }

    pub(super) fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    pub(super) fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.0.metadata()
    }

    pub(super) fn entry_names(&self) -> io::Result<Vec<OsString>> {
        use std::{
            mem::{offset_of, size_of},
            os::windows::{ffi::OsStringExt, io::AsRawHandle},
        };
        use windows_sys::Win32::{
            Foundation::{ERROR_NO_MORE_FILES, HANDLE},
            Storage::FileSystem::{
                FILE_ID_BOTH_DIR_INFO, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo,
                GetFileInformationByHandleEx,
            },
        };

        let mut names = Vec::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        let mut restart = true;
        loop {
            let information_class = if restart {
                FileIdBothDirectoryRestartInfo
            } else {
                FileIdBothDirectoryInfo
            };
            restart = false;
            // SAFETY: the handle is a live directory handle owned by self, and buffer is a writable byte buffer whose length is passed in.
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    self.0.as_raw_handle() as HANDLE,
                    information_class,
                    buffer.as_mut_ptr().cast(),
                    u32::try_from(buffer.len()).expect("directory buffer fits u32"),
                )
            };
            if ok == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    break;
                }
                return Err(error);
            }
            let mut offset = 0_usize;
            loop {
                if offset + size_of::<FILE_ID_BOTH_DIR_INFO>() > buffer.len() {
                    return Err(io::Error::other("directory enumeration record is invalid"));
                }
                // SAFETY: offset plus the record size is checked against buffer.len() just above, so the record lies inside buffer. buffer is a Vec<u8>, so this cast assumes the OS places records on FILE_ID_BOTH_DIR_INFO's alignment; see KNOWN_ISSUES.md.
                let entry =
                    unsafe { &*buffer.as_ptr().add(offset).cast::<FILE_ID_BOTH_DIR_INFO>() };
                let name_len = usize::try_from(entry.FileNameLength)
                    .map_err(|_| io::Error::other("directory name is invalid"))?
                    / 2;
                let name_offset = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
                if offset
                    .checked_add(name_offset)
                    .and_then(|start| start.checked_add(name_len.saturating_mul(2)))
                    .is_none_or(|end| end > buffer.len())
                {
                    return Err(io::Error::other(
                        "directory name exceeded the bounded record",
                    ));
                }
                // SAFETY: name_len UTF-16 units lie inside buffer, checked above; the same alignment assumption as the record cast applies (KNOWN_ISSUES.md).
                let name_units =
                    unsafe { std::slice::from_raw_parts(entry.FileName.as_ptr(), name_len) };
                let name = OsString::from_wide(name_units);
                if name != OsStr::new(".") && name != OsStr::new("..") {
                    names.push(name);
                }
                if entry.NextEntryOffset == 0 {
                    break;
                }
                offset = offset
                    .checked_add(entry.NextEntryOffset as usize)
                    .filter(|next| *next < buffer.len())
                    .ok_or_else(|| io::Error::other("directory enumeration offset is invalid"))?;
            }
        }
        Ok(names)
    }

    pub(super) fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        let (volume_serial, file_index) = windows_file_identity(&self.0)?;
        Ok(FilesystemObjectIdentity::Windows {
            volume_serial,
            file_index,
        })
    }

    pub(super) fn open_dir_no_follow(
        &self,
        name: &OsStr,
        access: DirectoryAccess,
    ) -> io::Result<Self> {
        nt_open_relative(
            &self.0,
            name,
            if access == DirectoryAccess::ReadWrite {
                NtLeafKind::MutableDirectory
            } else {
                NtLeafKind::Directory
            },
            NtDisposition::Open,
        )
        .map(Self)
    }

    pub(super) fn open_file_no_follow(&self, name: &OsStr, mutable: bool) -> io::Result<File> {
        nt_open_relative(
            &self.0,
            name,
            if mutable {
                NtLeafKind::MutableFile
            } else {
                NtLeafKind::ReadFile
            },
            NtDisposition::Open,
        )
    }

    pub(super) fn open_zone_identifier_stream(&self, name: &OsStr) -> io::Result<File> {
        let mut stream_name = name.to_os_string();
        stream_name.push(":Zone.Identifier:$DATA");
        nt_open_relative(
            &self.0,
            &stream_name,
            NtLeafKind::ReadFile,
            NtDisposition::Open,
        )
    }

    pub(super) fn create_file_new(&self, name: &OsStr) -> io::Result<File> {
        nt_open_relative(
            &self.0,
            name,
            NtLeafKind::MutableFile,
            NtDisposition::Create,
        )
    }

    pub(super) fn create_dir(&self, name: &OsStr) -> io::Result<()> {
        nt_open_relative(&self.0, name, NtLeafKind::Directory, NtDisposition::Create).map(drop)
    }

    pub(super) fn create_dir_identified(
        &self,
        name: &OsStr,
    ) -> io::Result<Option<FilesystemObjectIdentity>> {
        let directory =
            nt_open_relative(&self.0, name, NtLeafKind::Directory, NtDisposition::Create)?;
        Ok(PlatformDirectory(directory).identity().ok())
    }

    pub(super) fn remove_file(&self, name: &OsStr) -> io::Result<()> {
        let file = self.open_file_no_follow(name, true)?;
        delete_open_handle(&file)
    }

    pub(super) fn remove_dir(&self, name: &OsStr) -> io::Result<()> {
        let directory = nt_open_relative(
            &self.0,
            name,
            NtLeafKind::MutableDirectory,
            NtDisposition::Open,
        )?;
        delete_open_handle(&directory)
    }

    pub(super) fn rename(
        &self,
        source: &OsStr,
        destination_dir: &Self,
        destination: &OsStr,
        replace: bool,
    ) -> io::Result<()> {
        let destination_parent = duplicate_rename_target_directory(&destination_dir.0)?;
        retry_windows_sharing_violation(|| {
            let source =
                nt_open_relative(&self.0, source, NtLeafKind::MutableAny, NtDisposition::Open)?;
            rename_open_handle(&source, &destination_parent, destination, replace)
        })
        .map_err(|error| preserve_os_error("commit relative rename", error))
    }

    pub(super) fn sync_all(&self) -> io::Result<()> {
        injected_directory_sync_failure()?;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::HANDLE, Storage::FileSystem::FlushFileBuffers};

        // SAFETY: self.0 is a live directory handle owned by this wrapper.
        if unsafe { FlushFileBuffers(self.0.as_raw_handle() as HANDLE) } == 0 {
            let error = io::Error::last_os_error();
            // Remote shares cannot flush directory handles; the rename itself
            // has already completed and every later hash/identity check still runs.
            if directory_flush_unsupported(error.raw_os_error()) {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        } else {
            Ok(())
        }
    }
}

/// `ERROR_INVALID_FUNCTION` (1) and `ERROR_NOT_SUPPORTED` (50) from flushing a
/// directory handle mean the filesystem has no directory flush (SMB/network).
#[cfg(any(windows, test))]
pub(super) fn directory_flush_unsupported(code: Option<i32>) -> bool {
    matches!(code, Some(1 | 50))
}

#[cfg(windows)]
#[derive(Clone, Copy)]
pub(super) enum NtLeafKind {
    Directory,
    MutableDirectory,
    ReadFile,
    MutableFile,
    MutableAny,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
pub(super) enum NtDisposition {
    Open,
    Create,
}

#[cfg(windows)]
pub(super) fn nt_open_relative(
    parent: &File,
    name: &OsStr,
    kind: NtLeafKind,
    disposition: NtDisposition,
) -> io::Result<File> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle, io::FromRawHandle};
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
                FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{
                GENERIC_READ, GENERIC_WRITE, HANDLE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE,
                UNICODE_STRING,
            },
            Storage::FileSystem::{
                DELETE, FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, SYNCHRONIZE,
            },
            System::IO::IO_STATUS_BLOCK,
        },
    };

    let mut wide = name.encode_wide().collect::<Vec<_>>();
    if wide.is_empty() || wide.contains(&0) || wide.len() > (u16::MAX / 2) as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid capability-relative Windows name",
        ));
    }
    let unicode = UNICODE_STRING {
        Length: (wide.len() * 2) as u16,
        MaximumLength: (wide.len() * 2) as u16,
        Buffer: wide.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: &unicode,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let (access, options) = match kind {
        NtLeafKind::Directory => (
            GENERIC_READ | FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE,
            FILE_DIRECTORY_FILE,
        ),
        NtLeafKind::MutableDirectory => (
            GENERIC_READ
                | GENERIC_WRITE
                | FILE_READ_ATTRIBUTES
                | FILE_TRAVERSE
                | DELETE
                | SYNCHRONIZE,
            FILE_DIRECTORY_FILE,
        ),
        NtLeafKind::ReadFile => (
            GENERIC_READ | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_NON_DIRECTORY_FILE,
        ),
        NtLeafKind::MutableFile => (
            GENERIC_READ | GENERIC_WRITE | DELETE | SYNCHRONIZE,
            FILE_NON_DIRECTORY_FILE,
        ),
        NtLeafKind::MutableAny => (GENERIC_READ | GENERIC_WRITE | DELETE | SYNCHRONIZE, 0),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: handle, status_block, and attributes are live out-parameters or inputs for the call, and the name in attributes outlives it.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            access,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            match disposition {
                NtDisposition::Open => FILE_OPEN,
                NtDisposition::Create => FILE_CREATE,
            },
            options | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(ntstatus_error(status));
    }
    if handle.is_null() {
        return Err(io::Error::other("NtCreateFile returned an invalid handle"));
    }
    // SAFETY: the status was successful and handle is non-null, so NtCreateFile transferred a fresh handle that File now owns.
    let file = unsafe { File::from_raw_handle(handle) };
    if matches!(disposition, NtDisposition::Open) {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "capability-relative path is a reparse point",
            ));
        }
    }
    Ok(file)
}

#[cfg(windows)]
pub(super) fn duplicate_rename_target_directory(parent: &File) -> io::Result<File> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows_sys::Win32::{
        Foundation::{DuplicateHandle, HANDLE},
        Storage::FileSystem::{
            FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_READ_ATTRIBUTES, FILE_TRAVERSE, SYNCHRONIZE,
        },
        System::Threading::GetCurrentProcess,
    };

    // SAFETY: GetCurrentProcess takes no arguments and returns a pseudo-handle that needs no closing.
    let process = unsafe { GetCurrentProcess() };
    let mut duplicated: HANDLE = std::ptr::null_mut();
    // SAFETY: process is the current-process pseudo-handle, parent is a live handle, and duplicated is a live out-parameter.
    let ok = unsafe {
        DuplicateHandle(
            process,
            parent.as_raw_handle() as HANDLE,
            process,
            &mut duplicated,
            FILE_ADD_FILE
                | FILE_ADD_SUBDIRECTORY
                | FILE_TRAVERSE
                | FILE_READ_ATTRIBUTES
                | SYNCHRONIZE,
            0,
            0,
        )
    };
    if ok == 0 {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!("duplicate rename target: {error}"),
        ));
    }
    if duplicated.is_null() {
        return Err(io::Error::other(
            "DuplicateHandle returned an invalid rename target",
        ));
    }
    // SAFETY: DuplicateHandle succeeded and duplicated is non-null, so this File is the only owner of the new handle.
    Ok(unsafe { File::from_raw_handle(duplicated) })
}

#[cfg(windows)]
pub(super) fn rename_open_handle(
    source: &File,
    destination_parent: &File,
    destination: &OsStr,
    replace: bool,
) -> io::Result<()> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::{
        Wdk::Storage::FileSystem::{
            FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
        },
        Win32::{Foundation::HANDLE, System::IO::IO_STATUS_BLOCK},
    };

    let wide = destination.encode_wide().collect::<Vec<_>>();
    if wide.is_empty() || wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid capability-relative Windows destination",
        ));
    }
    let bytes = wide.len() * 2;
    let buffer_len = std::mem::size_of::<FILE_RENAME_INFORMATION>() + bytes;
    let mut buffer = vec![0_usize; buffer_len.div_ceil(std::mem::size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    // SAFETY: buffer is a Vec<usize>, so the pointer is aligned for FILE_RENAME_INFORMATION, and buffer_len bytes were allocated; the wide name is copied within them.
    unsafe {
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = destination_parent.as_raw_handle() as HANDLE;
        (*info).FileNameLength = bytes as u32;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), (*info).FileName.as_mut_ptr(), wide.len());
    }
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: source is a live handle, buffer holds buffer_len bytes laid out as FILE_RENAME_INFORMATION, and status_block is a live out-parameter.
    let status = unsafe {
        NtSetInformationFile(
            source.as_raw_handle() as HANDLE,
            &mut status_block,
            buffer.as_ptr().cast(),
            buffer_len as u32,
            FileRenameInformation,
        )
    };
    if status < 0 {
        Err(ntstatus_error(status))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(super) fn delete_open_handle(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
            FILE_DISPOSITION_INFO_EX, FileDispositionInfoEx, SetFileInformationByHandle,
        },
    };

    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    // SAFETY: file is a live handle, and disposition is a live struct of the stated size that is read only during the call.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileDispositionInfoEx,
            (&disposition as *const FILE_DISPOSITION_INFO_EX).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(super) fn ntstatus_error(status: windows_sys::Win32::Foundation::NTSTATUS) -> io::Error {
    // SAFETY: RtlNtStatusToDosError takes an NTSTATUS value and has no pointer arguments.
    let code = unsafe { windows_sys::Win32::Foundation::RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(code as i32)
}

#[cfg(any(windows, test))]
pub(super) fn retry_windows_sharing_violation(
    mut operation: impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    const MAX_ATTEMPTS: usize = 5;
    for attempt in 0..MAX_ATTEMPTS {
        match operation() {
            Ok(()) => return Ok(()),
            Err(error) if error.raw_os_error() == Some(32) && attempt + 1 < MAX_ATTEMPTS => {
                #[cfg(windows)]
                std::thread::sleep(std::time::Duration::from_millis(50 * (attempt as u64 + 1)));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("bounded retry loop always returns")
}

#[cfg(any(windows, test))]
pub(super) fn preserve_os_error(context: &str, error: io::Error) -> io::Error {
    match error.raw_os_error() {
        Some(code) => io::Error::from_raw_os_error(code),
        None => io::Error::new(error.kind(), format!("{context}: {error}")),
    }
}

pub(super) fn is_cross_device(error: &io::Error) -> bool {
    #[cfg(unix)]
    return error.raw_os_error() == Some(libc::EXDEV);
    #[cfg(windows)]
    return error.raw_os_error() == Some(17);
    #[allow(unreachable_code)]
    false
}

#[cfg(unix)]
pub(super) fn same_file(left: &File, right: &File) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(windows)]
pub(super) fn same_file(left: &File, right: &File) -> io::Result<bool> {
    Ok(windows_file_identity(left)? == windows_file_identity(right)?)
}

#[cfg(windows)]
pub(super) fn windows_file_identity(file: &File) -> io::Result<(u32, u64)> {
    use std::{mem::MaybeUninit, os::windows::io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };

    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: the handle is live, and information points to writable MaybeUninit storage for the result.
    if unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as HANDLE, information.as_mut_ptr())
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GetFileInformationByHandle succeeded above, so the struct is fully initialized.
    let information = unsafe { information.assume_init() };
    Ok((
        information.dwVolumeSerialNumber,
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    ))
}

#[cfg(test)]
mod directory_flush_tests {
    use super::directory_flush_unsupported;

    #[test]
    fn only_unsupported_function_codes_are_tolerated() {
        assert!(directory_flush_unsupported(Some(1)));
        assert!(directory_flush_unsupported(Some(50)));
        for code in [Some(5), Some(32), Some(112), Some(1117), None] {
            assert!(!directory_flush_unsupported(code), "{code:?}");
        }
    }
}
