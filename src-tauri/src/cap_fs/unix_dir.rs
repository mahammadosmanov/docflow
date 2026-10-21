#![allow(unused_imports)]
use super::*;

#[cfg(unix)]
pub(super) struct PlatformDirectory(File);

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
pub(super) fn unix_errno_pointer() -> *mut libc::c_int {
    unsafe { libc::__error() }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn unix_errno_pointer() -> *mut libc::c_int {
    unsafe { libc::__errno_location() }
}

#[cfg(unix)]
pub(super) fn set_unix_errno(value: libc::c_int) {
    unsafe { *unix_errno_pointer() = value };
}

#[cfg(unix)]
pub(super) fn unix_errno() -> libc::c_int {
    unsafe { *unix_errno_pointer() }
}

#[cfg(unix)]
impl PlatformDirectory {
    pub(super) fn open_ambient(path: &Path, _access: DirectoryAccess) -> io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;

        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map(Self)
    }

    pub(super) fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    pub(super) fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.0.metadata()
    }

    pub(super) fn entry_names(&self) -> io::Result<Vec<OsString>> {
        use std::os::fd::AsRawFd;

        let dot = c".";
        let duplicated = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                dot.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if duplicated < 0 {
            return Err(io::Error::last_os_error());
        }
        let stream = unsafe { libc::fdopendir(duplicated) };
        if stream.is_null() {
            unsafe { libc::close(duplicated) };
            return Err(io::Error::last_os_error());
        }
        struct DirectoryStream(*mut libc::DIR);
        impl Drop for DirectoryStream {
            fn drop(&mut self) {
                unsafe { libc::closedir(self.0) };
            }
        }
        let stream = DirectoryStream(stream);
        let mut names = Vec::new();
        loop {
            set_unix_errno(0);
            let entry = unsafe { libc::readdir(stream.0) };
            if entry.is_null() {
                let errno = unix_errno();
                if errno == 0 {
                    break;
                }
                return Err(io::Error::from_raw_os_error(errno));
            }
            let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            use std::os::unix::ffi::OsStringExt;
            names.push(OsString::from_vec(name.to_bytes().to_vec()));
        }
        Ok(names)
    }

    pub(super) fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        use std::os::unix::fs::MetadataExt;

        let metadata = self.0.metadata()?;
        Ok(FilesystemObjectIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    pub(super) fn open_dir_no_follow(
        &self,
        name: &OsStr,
        _access: DirectoryAccess,
    ) -> io::Result<Self> {
        let name = unix_name(name)?;
        let descriptor = unsafe {
            libc::openat(
                std::os::fd::AsRawFd::as_raw_fd(&self.0),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(unsafe {
                std::os::fd::FromRawFd::from_raw_fd(descriptor)
            }))
        }
    }

    pub(super) fn open_file_no_follow(&self, name: &OsStr, mutable: bool) -> io::Result<File> {
        let name = unix_name(name)?;
        let access = if mutable {
            libc::O_RDWR
        } else {
            libc::O_RDONLY
        };
        let descriptor = unsafe {
            libc::openat(
                std::os::fd::AsRawFd::as_raw_fd(&self.0),
                name.as_ptr(),
                access | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { std::os::fd::FromRawFd::from_raw_fd(descriptor) })
        }
    }

    pub(super) fn create_file_new(&self, name: &OsStr) -> io::Result<File> {
        let name = unix_name(name)?;
        let descriptor = unsafe {
            libc::openat(
                std::os::fd::AsRawFd::as_raw_fd(&self.0),
                name.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if descriptor < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { std::os::fd::FromRawFd::from_raw_fd(descriptor) })
        }
    }

    pub(super) fn create_dir(&self, name: &OsStr) -> io::Result<()> {
        let name = unix_name(name)?;
        if unsafe {
            libc::mkdirat(
                std::os::fd::AsRawFd::as_raw_fd(&self.0),
                name.as_ptr(),
                0o755,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn create_dir_identified(
        &self,
        name: &OsStr,
    ) -> io::Result<Option<FilesystemObjectIdentity>> {
        self.create_dir(name)?;
        Ok(self
            .open_dir_no_follow(name, DirectoryAccess::ReadWrite)
            .and_then(|directory| directory.identity())
            .ok())
    }

    pub(super) fn remove_file(&self, name: &OsStr) -> io::Result<()> {
        self.unlink(name, 0)
    }

    pub(super) fn remove_dir(&self, name: &OsStr) -> io::Result<()> {
        self.unlink(name, libc::AT_REMOVEDIR)
    }

    pub(super) fn unlink(&self, name: &OsStr, flags: i32) -> io::Result<()> {
        let name = unix_name(name)?;
        if unsafe {
            libc::unlinkat(
                std::os::fd::AsRawFd::as_raw_fd(&self.0),
                name.as_ptr(),
                flags,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn rename(
        &self,
        source: &OsStr,
        destination_dir: &Self,
        destination: &OsStr,
        replace: bool,
    ) -> io::Result<()> {
        let source = unix_name(source)?;
        let destination = unix_name(destination)?;
        let source_fd = std::os::fd::AsRawFd::as_raw_fd(&self.0);
        let destination_fd = std::os::fd::AsRawFd::as_raw_fd(&destination_dir.0);
        let result = if replace {
            unsafe {
                libc::renameat(
                    source_fd,
                    source.as_ptr(),
                    destination_fd,
                    destination.as_ptr(),
                )
            }
        } else {
            rename_at_no_replace(source_fd, &source, destination_fd, &destination)
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(super) fn sync_all(&self) -> io::Result<()> {
        injected_directory_sync_failure()?;
        self.0.sync_all()
    }
}

#[cfg(target_os = "macos")]
pub(super) fn rename_at_no_replace(
    source_fd: i32,
    source: &std::ffi::CString,
    destination_fd: i32,
    destination: &std::ffi::CString,
) -> i32 {
    unsafe {
        libc::renameatx_np(
            source_fd,
            source.as_ptr(),
            destination_fd,
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    }
}

#[cfg(target_os = "linux")]
pub(super) fn rename_at_no_replace(
    source_fd: i32,
    source: &std::ffi::CString,
    destination_fd: i32,
    destination: &std::ffi::CString,
) -> i32 {
    unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            source_fd,
            source.as_ptr(),
            destination_fd,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        ) as i32
    }
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
pub(super) fn rename_at_no_replace(
    source_fd: i32,
    source: &std::ffi::CString,
    destination_fd: i32,
    destination: &std::ffi::CString,
) -> i32 {
    let linked = unsafe {
        libc::linkat(
            source_fd,
            source.as_ptr(),
            destination_fd,
            destination.as_ptr(),
            0,
        )
    };
    if linked != 0 {
        return linked;
    }
    unsafe { libc::unlinkat(source_fd, source.as_ptr(), 0) }
}

#[cfg(unix)]
pub(super) fn unix_name(name: &OsStr) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;

    if name.is_empty() || name.as_bytes().contains(&0) || name == OsStr::new(".") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid capability-relative name",
        ));
    }
    std::ffi::CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains NUL"))
}
