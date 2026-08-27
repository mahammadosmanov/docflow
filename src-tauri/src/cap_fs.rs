use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use crate::journal::FilesystemObjectIdentity;

#[cfg(test)]
thread_local! {
    static DIRECTORY_SYNC_FAILURE_COUNTDOWN: std::cell::Cell<Option<usize>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
pub(crate) fn inject_directory_sync_failure_after(successful_syncs: usize) {
    DIRECTORY_SYNC_FAILURE_COUNTDOWN.with(|countdown| countdown.set(Some(successful_syncs)));
}

#[cfg(test)]
fn injected_directory_sync_failure() -> io::Result<()> {
    DIRECTORY_SYNC_FAILURE_COUNTDOWN.with(|countdown| match countdown.get() {
        Some(0) => {
            countdown.set(None);
            Err(io::Error::other(
                "injected post-rename directory sync failure",
            ))
        }
        Some(remaining) => {
            countdown.set(Some(remaining - 1));
            Ok(())
        }
        None => Ok(()),
    })
}

#[cfg(not(test))]
fn injected_directory_sync_failure() -> io::Result<()> {
    Ok(())
}

#[derive(Clone)]
pub(crate) struct CapabilityRoot {
    canonical: PathBuf,
    directory: Arc<PlatformDirectory>,
}

impl CapabilityRoot {
    pub(crate) fn open(canonical: &Path) -> io::Result<Self> {
        if !canonical.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "capability root must be absolute",
            ));
        }
        Ok(Self {
            canonical: canonical.to_path_buf(),
            directory: Arc::new(PlatformDirectory::open_ambient(canonical)?),
        })
    }

    pub(crate) fn canonical(&self) -> &Path {
        &self.canonical
    }

    pub(crate) fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        self.directory.identity()
    }

    pub(crate) fn authorize(&self, absolute: &Path) -> io::Result<AuthorizedPath> {
        self.authorize_inner(absolute, false)
    }

    pub(crate) fn authorize_directory(&self, absolute: &Path) -> io::Result<AuthorizedPath> {
        self.authorize_inner(absolute, true)
    }

    fn authorize_inner(&self, absolute: &Path, allow_root: bool) -> io::Result<AuthorizedPath> {
        if !absolute.is_absolute()
            || absolute
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "path is not capability-relative",
            ));
        }
        let relative = absolute.strip_prefix(&self.canonical).map_err(|_| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "path escapes capability root",
            )
        })?;
        if relative.as_os_str().is_empty() && !allow_root {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "capability root itself is not a leaf path",
            ));
        }
        validate_relative(relative)?;
        Ok(AuthorizedPath {
            absolute: absolute.to_path_buf(),
            relative: relative.to_path_buf(),
            root: self.clone(),
            expected_identity: None,
        })
    }
}

#[derive(Clone)]
pub(crate) struct AuthorizedPath {
    absolute: PathBuf,
    relative: PathBuf,
    root: CapabilityRoot,
    expected_identity: Option<FilesystemObjectIdentity>,
}

impl AuthorizedPath {
    pub(crate) fn with_expected_identity(
        mut self,
        expected_identity: FilesystemObjectIdentity,
    ) -> Self {
        self.expected_identity = Some(expected_identity);
        self
    }

    pub(crate) fn anchor(&self) -> io::Result<AnchoredPath> {
        let name = self
            .relative
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no leaf name"))?
            .to_os_string();
        let parent = self.relative.parent().unwrap_or_else(|| Path::new(""));
        let mut directory = self.root.directory.try_clone()?;
        for component in parent.components() {
            let Component::Normal(name) = component else {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "invalid capability-relative ancestor",
                ));
            };
            directory = directory.open_dir_no_follow(name)?;
        }
        Ok(AnchoredPath {
            absolute: self.absolute.clone(),
            parent: directory,
            name,
            expected_identity: self.expected_identity.clone(),
        })
    }

    pub(crate) fn missing_directories(&self) -> io::Result<MissingDirectoryCursor> {
        let mut parent = self.root.directory.try_clone()?;
        let mut absolute = self.root.canonical.clone();
        let mut components = self.relative.components();
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "invalid capability-relative directory",
                ));
            };
            absolute.push(name);
            match parent.open_dir_no_follow(name) {
                Ok(directory) => parent = directory,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let mut missing = vec![(absolute.clone(), name.to_os_string())];
                    for component in components {
                        let Component::Normal(name) = component else {
                            return Err(io::Error::new(
                                io::ErrorKind::PermissionDenied,
                                "invalid capability-relative directory",
                            ));
                        };
                        absolute.push(name);
                        missing.push((absolute.clone(), name.to_os_string()));
                    }
                    return Ok(MissingDirectoryCursor {
                        parent,
                        missing,
                        index: 0,
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(MissingDirectoryCursor {
            parent,
            missing: Vec::new(),
            index: 0,
        })
    }
}

pub(crate) struct MissingDirectoryCursor {
    parent: PlatformDirectory,
    missing: Vec<(PathBuf, OsString)>,
    index: usize,
}

impl MissingDirectoryCursor {
    pub(crate) fn next_path(&self) -> Option<&Path> {
        self.missing.get(self.index).map(|(path, _)| path.as_path())
    }

    pub(crate) fn create_next(&mut self) -> Result<(), CapabilityEffectError> {
        let (_, name) = self.missing.get(self.index).ok_or_else(|| {
            CapabilityEffectError::no_effect(io::Error::other("no directory pending"))
        })?;
        self.parent
            .create_dir(name)
            .map_err(CapabilityEffectError::no_effect)?;
        let child = self
            .parent
            .open_dir_no_follow(name)
            .map_err(CapabilityEffectError::outcome_unknown)?;
        self.parent
            .sync_all()
            .map_err(CapabilityEffectError::outcome_unknown)?;
        self.parent = child;
        self.index += 1;
        Ok(())
    }
}

pub(crate) struct CapabilityEffectError {
    error: io::Error,
    outcome_unknown: bool,
}

impl CapabilityEffectError {
    fn no_effect(error: io::Error) -> Self {
        Self {
            error,
            outcome_unknown: false,
        }
    }

    fn outcome_unknown(error: io::Error) -> Self {
        Self {
            error,
            outcome_unknown: true,
        }
    }

    pub(crate) fn is_outcome_unknown(&self) -> bool {
        self.outcome_unknown
    }

    pub(crate) fn into_inner(self) -> io::Error {
        self.error
    }
}

pub(crate) struct AnchoredPath {
    absolute: PathBuf,
    parent: PlatformDirectory,
    name: OsString,
    expected_identity: Option<FilesystemObjectIdentity>,
}

impl AnchoredPath {
    pub(crate) fn absolute(&self) -> &Path {
        &self.absolute
    }

    pub(crate) fn open_read(&self) -> io::Result<File> {
        let file = self.parent.open_file_no_follow(&self.name, false)?;
        self.validate_expected_identity(&file)?;
        Ok(file)
    }

    pub(crate) fn open_mutable(&self) -> io::Result<File> {
        let file = self.parent.open_file_no_follow(&self.name, true)?;
        self.validate_expected_identity(&file)?;
        Ok(file)
    }

    pub(crate) fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        let file = self.open_read()?;
        file_identity(&file)
    }

    pub(crate) fn len(&self) -> io::Result<u64> {
        self.open_read()?.metadata().map(|metadata| metadata.len())
    }

    pub(crate) fn same_file_as(&self, other: &AnchoredPath) -> io::Result<bool> {
        same_file(&self.open_read()?, &other.open_read()?)
    }

    pub(crate) fn remove_file(&self) -> io::Result<()> {
        self.parent.remove_file(&self.name)
    }

    pub(crate) fn remove_dir(&self) -> io::Result<()> {
        self.parent.remove_dir(&self.name)
    }

    pub(crate) fn remove_file_durable(&self) -> io::Result<()> {
        self.parent.remove_file(&self.name)?;
        self.parent.sync_all()
    }

    pub(crate) fn remove_dir_durable(&self) -> io::Result<()> {
        self.parent.remove_dir(&self.name)?;
        self.parent.sync_all()
    }

    pub(crate) fn copy_no_clobber_from(&self, source: &AnchoredPath) -> io::Result<()> {
        let mut input = source.open_read()?;
        self.copy_no_clobber_from_file(&mut input)
    }

    pub(crate) fn copy_no_clobber_from_file(&self, input: &mut File) -> io::Result<()> {
        input.seek(SeekFrom::Start(0))?;
        let (temporary_name, mut output) = self.create_temporary()?;
        let result = (|| {
            io::copy(input, &mut output)?;
            output.set_permissions(input.metadata()?.permissions())?;
            output.sync_all()?;
            drop(output);
            self.parent
                .rename(&temporary_name, &self.parent, &self.name, false)?;
            self.parent.sync_all()
        })();
        if result.is_err() {
            let _ = self.parent.remove_file(&temporary_name);
        }
        result
    }

    pub(crate) fn write_no_clobber(&self, contents: &[u8]) -> io::Result<()> {
        let (temporary_name, mut output) = self.create_temporary()?;
        let result = (|| {
            output.write_all(contents)?;
            output.sync_all()?;
            drop(output);
            self.parent
                .rename(&temporary_name, &self.parent, &self.name, false)?;
            self.parent.sync_all()
        })();
        if result.is_err() {
            let _ = self.parent.remove_file(&temporary_name);
        }
        result
    }

    pub(crate) fn move_no_clobber_to(&self, destination: &AnchoredPath) -> io::Result<()> {
        match self
            .parent
            .rename(&self.name, &destination.parent, &destination.name, false)
        {
            Ok(()) => {
                destination.parent.sync_all()?;
                self.parent.sync_all()
            }
            Err(error) if is_cross_device(&error) => {
                destination.copy_no_clobber_from(self)?;
                self.remove_file()?;
                self.parent.sync_all()
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn replace_with(&self, source: &AnchoredPath) -> io::Result<()> {
        source
            .parent
            .rename(&source.name, &self.parent, &self.name, true)?;
        self.parent.sync_all()
    }

    pub(crate) fn sha256(&self) -> io::Result<String> {
        use sha2::{Digest, Sha256};

        let mut file = self.open_read()?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        Ok(format!("{:x}", digest.finalize()))
    }

    pub(crate) fn temporary_sibling(&self, label: &str) -> io::Result<Self> {
        let display = self.name.to_string_lossy();
        let name = OsString::from(format!(
            ".{display}.docflow-{label}-{}.tmp",
            uuid::Uuid::new_v4()
        ));
        Ok(Self {
            absolute: self
                .absolute
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(&name),
            parent: self.parent.try_clone()?,
            name,
            expected_identity: None,
        })
    }

    fn validate_expected_identity(&self, file: &File) -> io::Result<()> {
        let Some(expected) = self.expected_identity.as_ref() else {
            return Ok(());
        };
        if &file_identity(file)? != expected {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "capability leaf identity changed",
            ));
        }
        Ok(())
    }

    fn create_temporary(&self) -> io::Result<(OsString, File)> {
        let display = self.name.to_string_lossy();
        for _ in 0..16 {
            let name = OsString::from(format!(".{display}.docflow-{}.tmp", uuid::Uuid::new_v4()));
            match self.parent.create_file_new(&name) {
                Ok(file) => return Ok((name, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a capability-relative temporary file",
        ))
    }
}

fn file_identity(file: &File) -> io::Result<FilesystemObjectIdentity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let metadata = file.metadata()?;
        Ok(FilesystemObjectIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        let (volume_serial, file_index) = windows_file_identity(file)?;
        Ok(FilesystemObjectIdentity::Windows {
            volume_serial,
            file_index,
        })
    }
}

fn validate_relative(path: &Path) -> io::Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "invalid capability-relative path",
        ));
    }
    Ok(())
}

#[cfg(unix)]
struct PlatformDirectory(File);

#[cfg(unix)]
impl PlatformDirectory {
    fn open_ambient(path: &Path) -> io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;

        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map(Self)
    }

    fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        use std::os::unix::fs::MetadataExt;

        let metadata = self.0.metadata()?;
        Ok(FilesystemObjectIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    fn open_dir_no_follow(&self, name: &OsStr) -> io::Result<Self> {
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

    fn open_file_no_follow(&self, name: &OsStr, mutable: bool) -> io::Result<File> {
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

    fn create_file_new(&self, name: &OsStr) -> io::Result<File> {
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

    fn create_dir(&self, name: &OsStr) -> io::Result<()> {
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

    fn remove_file(&self, name: &OsStr) -> io::Result<()> {
        self.unlink(name, 0)
    }

    fn remove_dir(&self, name: &OsStr) -> io::Result<()> {
        self.unlink(name, libc::AT_REMOVEDIR)
    }

    fn unlink(&self, name: &OsStr, flags: i32) -> io::Result<()> {
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

    fn rename(
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

    fn sync_all(&self) -> io::Result<()> {
        injected_directory_sync_failure()?;
        self.0.sync_all()
    }
}

#[cfg(target_os = "macos")]
fn rename_at_no_replace(
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
fn rename_at_no_replace(
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
fn rename_at_no_replace(
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
fn unix_name(name: &OsStr) -> io::Result<std::ffi::CString> {
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

#[cfg(windows)]
struct PlatformDirectory(File);

#[cfg(windows)]
impl PlatformDirectory {
    fn open_ambient(path: &Path) -> io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };

        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
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

    fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        let (volume_serial, file_index) = windows_file_identity(&self.0)?;
        Ok(FilesystemObjectIdentity::Windows {
            volume_serial,
            file_index,
        })
    }

    fn open_dir_no_follow(&self, name: &OsStr) -> io::Result<Self> {
        nt_open_relative(&self.0, name, NtLeafKind::Directory, NtDisposition::Open).map(Self)
    }

    fn open_file_no_follow(&self, name: &OsStr, mutable: bool) -> io::Result<File> {
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

    fn create_file_new(&self, name: &OsStr) -> io::Result<File> {
        nt_open_relative(
            &self.0,
            name,
            NtLeafKind::MutableFile,
            NtDisposition::Create,
        )
    }

    fn create_dir(&self, name: &OsStr) -> io::Result<()> {
        nt_open_relative(&self.0, name, NtLeafKind::Directory, NtDisposition::Create).map(drop)
    }

    fn remove_file(&self, name: &OsStr) -> io::Result<()> {
        let file = self.open_file_no_follow(name, true)?;
        delete_open_handle(&file)
    }

    fn remove_dir(&self, name: &OsStr) -> io::Result<()> {
        let directory = nt_open_relative(
            &self.0,
            name,
            NtLeafKind::MutableDirectory,
            NtDisposition::Open,
        )?;
        delete_open_handle(&directory)
    }

    fn rename(
        &self,
        source: &OsStr,
        destination_dir: &Self,
        destination: &OsStr,
        replace: bool,
    ) -> io::Result<()> {
        let source =
            nt_open_relative(&self.0, source, NtLeafKind::MutableAny, NtDisposition::Open)?;
        rename_open_handle(&source, &destination_dir.0, destination, replace)
    }

    fn sync_all(&self) -> io::Result<()> {
        injected_directory_sync_failure()?;
        Ok(())
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum NtLeafKind {
    Directory,
    MutableDirectory,
    ReadFile,
    MutableFile,
    MutableAny,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum NtDisposition {
    Open,
    Create,
}

#[cfg(windows)]
fn nt_open_relative(
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
                DELETE, FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
                FILE_SHARE_WRITE, SYNCHRONIZE,
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
            GENERIC_READ | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_DIRECTORY_FILE,
        ),
        NtLeafKind::MutableDirectory => (
            GENERIC_READ | GENERIC_WRITE | DELETE | SYNCHRONIZE,
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
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            access,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
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
fn rename_open_handle(
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
    unsafe {
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = destination_parent.as_raw_handle() as HANDLE;
        (*info).FileNameLength = bytes as u32;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), (*info).FileName.as_mut_ptr(), wide.len());
    }
    let mut status_block = IO_STATUS_BLOCK::default();
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
fn delete_open_handle(file: &File) -> io::Result<()> {
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
fn ntstatus_error(status: windows_sys::Win32::Foundation::NTSTATUS) -> io::Error {
    let code = unsafe { windows_sys::Win32::Foundation::RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(code as i32)
}

fn is_cross_device(error: &io::Error) -> bool {
    #[cfg(unix)]
    return error.raw_os_error() == Some(libc::EXDEV);
    #[cfg(windows)]
    return error.raw_os_error() == Some(17);
    #[allow(unreachable_code)]
    false
}

#[cfg(unix)]
fn same_file(left: &File, right: &File) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(windows)]
fn same_file(left: &File, right: &File) -> io::Result<bool> {
    Ok(windows_file_identity(left)? == windows_file_identity(right)?)
}

#[cfg(windows)]
fn windows_file_identity(file: &File) -> io::Result<(u32, u64)> {
    use std::{mem::MaybeUninit, os::windows::io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };

    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    if unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as HANDLE, information.as_mut_ptr())
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let information = unsafe { information.assume_init() };
    Ok((
        information.dwVolumeSerialNumber,
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_rename_uses_the_native_root_relative_information_contract() {
        let source = include_str!("cap_fs.rs");
        let rename = source
            .split("fn rename_open_handle(")
            .nth(1)
            .expect("Windows rename implementation must remain inspectable")
            .split("fn delete_open_handle(")
            .next()
            .expect("Windows rename implementation must remain bounded");

        assert!(rename.contains("NtSetInformationFile"));
        assert!(rename.contains("FILE_RENAME_INFORMATION"));
        assert!(rename.contains("FileRenameInformation"));
        assert!(rename.contains("RootDirectory"));
        assert!(!rename.contains("SetFileInformationByHandle"));
    }
}
