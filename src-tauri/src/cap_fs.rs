use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use crate::journal::FilesystemObjectIdentity;

#[cfg(unix)]
const MAX_SECURITY_AUXILIARY_RECORDS: usize = 128;
#[cfg(any(unix, test))]
const MAX_SECURITY_AUXILIARY_BYTES: u64 = 1024 * 1024;

#[cfg(test)]
thread_local! {
    static DIRECTORY_SYNC_FAILURE_COUNTDOWN: std::cell::Cell<Option<usize>> = const {
        std::cell::Cell::new(None)
    };
    static POST_CREATE_IDENTITY_FAILURE: std::cell::Cell<bool> = const {
        std::cell::Cell::new(false)
    };
}

#[cfg(test)]
pub(crate) fn inject_directory_sync_failure_after(successful_syncs: usize) {
    DIRECTORY_SYNC_FAILURE_COUNTDOWN.with(|countdown| countdown.set(Some(successful_syncs)));
}

#[cfg(all(test, not(windows)))]
fn inject_post_create_identity_failure_once() {
    POST_CREATE_IDENTITY_FAILURE.with(|injected| injected.set(true));
}

#[cfg(test)]
fn take_post_create_identity_failure() -> bool {
    POST_CREATE_IDENTITY_FAILURE.with(|injected| injected.replace(false))
}

#[cfg(not(test))]
fn take_post_create_identity_failure() -> bool {
    false
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
    access: DirectoryAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryAccess {
    ReadOnly,
    ReadWrite,
}

impl CapabilityRoot {
    pub(crate) fn open(canonical: &Path) -> io::Result<Self> {
        Self::open_with_access(canonical, DirectoryAccess::ReadWrite)
    }

    pub(crate) fn open_with_access(canonical: &Path, access: DirectoryAccess) -> io::Result<Self> {
        if !canonical.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "capability root must be absolute",
            ));
        }
        Ok(Self {
            canonical: canonical.to_path_buf(),
            directory: Arc::new(PlatformDirectory::open_ambient(canonical, access)?),
            access,
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
            directory = directory.open_dir_no_follow(name, self.root.access)?;
        }
        Ok(AnchoredPath {
            absolute: self.absolute.clone(),
            parent: directory,
            name,
            expected_identity: self.expected_identity.clone(),
            access: self.root.access,
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
            match parent.open_dir_no_follow(name, self.root.access) {
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
                        access: self.root.access,
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(MissingDirectoryCursor {
            parent,
            missing: Vec::new(),
            index: 0,
            access: self.root.access,
        })
    }

    pub(crate) fn create_artifact_parent_no_clobber(
        &self,
    ) -> Result<ArtifactParentCreateOutcome, CapabilityEffectError> {
        let anchored = self.anchor().map_err(CapabilityEffectError::no_effect)?;
        anchored
            .require_write()
            .map_err(CapabilityEffectError::no_effect)?;
        match anchored.parent.create_dir_identified(&anchored.name) {
            Ok(Some(identity)) => Ok(ArtifactParentCreateOutcome::Created { identity }),
            Ok(None) => Ok(ArtifactParentCreateOutcome::CreatedIdentityUnknown),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Ok(ArtifactParentCreateOutcome::AlreadyExists)
            }
            Err(error) => Err(CapabilityEffectError::no_effect(error)),
        }
    }

    pub(crate) fn s5_retained_storage_usage(
        &self,
        max_entries: usize,
    ) -> io::Result<RetainedStorageUsage> {
        let directory = self.open_directory_no_follow()?;
        let mut usage = RetainedStorageUsage {
            file_count: 0,
            logical_charge_bytes: 0,
        };
        for name in directory.entry_names()? {
            let Some(name_text) = name.to_str() else {
                continue;
            };
            if !S5_RETAINED_PREFIXES
                .iter()
                .any(|prefix| name_text.starts_with(prefix))
            {
                continue;
            }
            usage.file_count = usage
                .file_count
                .checked_add(1)
                .ok_or_else(|| io::Error::other("retained artifact count overflowed"))?;
            if usage.file_count > max_entries {
                return Err(io::Error::other(
                    "retained artifact count exceeded the limit",
                ));
            }
            let file = directory.open_file_no_follow(&name, false)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                return Err(io::Error::other("retained artifact is not a regular file"));
            }
            let snapshot = FileSecuritySnapshot::capture(&file, None)?;
            let charge = metadata
                .len()
                .checked_add(snapshot.auxiliary_charge_bytes())
                .ok_or_else(|| io::Error::other("retained artifact charge overflowed"))?;
            usage.logical_charge_bytes = usage
                .logical_charge_bytes
                .checked_add(charge)
                .ok_or_else(|| io::Error::other("retained storage usage overflowed"))?;
        }
        Ok(usage)
    }

    fn open_directory_no_follow(&self) -> io::Result<PlatformDirectory> {
        let mut directory = self.root.directory.try_clone()?;
        for component in self.relative.components() {
            let Component::Normal(name) = component else {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "invalid capability-relative directory",
                ));
            };
            directory = directory.open_dir_no_follow(name, self.root.access)?;
        }
        Ok(directory)
    }
}

const S5_RETAINED_PREFIXES: [&str; 4] = [
    ".docflow-s5-backup-",
    ".docflow-s5-private-output-",
    ".docflow-s5-retained-original-",
    ".docflow-s5-retained-applied-",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ArtifactParentCreateOutcome {
    Created { identity: FilesystemObjectIdentity },
    AlreadyExists,
    CreatedIdentityUnknown,
}

pub(crate) struct MissingDirectoryCursor {
    parent: PlatformDirectory,
    missing: Vec<(PathBuf, OsString)>,
    index: usize,
    access: DirectoryAccess,
}

impl MissingDirectoryCursor {
    pub(crate) fn next_path(&self) -> Option<&Path> {
        self.missing.get(self.index).map(|(path, _)| path.as_path())
    }

    pub(crate) fn create_next(&mut self) -> Result<(), CapabilityEffectError> {
        if self.access != DirectoryAccess::ReadWrite {
            return Err(CapabilityEffectError::no_effect(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read-only capability cannot create a directory",
            )));
        }
        let (_, name) = self.missing.get(self.index).ok_or_else(|| {
            CapabilityEffectError::no_effect(io::Error::other("no directory pending"))
        })?;
        self.parent
            .create_dir(name)
            .map_err(CapabilityEffectError::no_effect)?;
        let child = self
            .parent
            .open_dir_no_follow(name, self.access)
            .map_err(CapabilityEffectError::outcome_unknown)?;
        self.parent
            .sync_all()
            .map_err(CapabilityEffectError::outcome_unknown)?;
        self.parent = child;
        self.index += 1;
        Ok(())
    }
}

#[derive(Debug)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileSecuritySnapshot {
    access: PlatformFileAccess,
    auxiliary: Vec<(Vec<u8>, Vec<u8>)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PlatformFileAccess {
    #[cfg(unix)]
    Unix {
        mode: u32,
        uid: u32,
        gid: u32,
        links: u64,
    },
    #[cfg(windows)]
    Windows { attributes: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParentSecuritySnapshot {
    identity: FilesystemObjectIdentity,
    access: PlatformParentAccess,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PlatformParentAccess {
    #[cfg(unix)]
    Unix { mode: u32, uid: u32, gid: u32 },
    #[cfg(windows)]
    Windows { attributes: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreationSecurityDescriptor {
    access: PlatformFileAccess,
    auxiliary: Vec<(Vec<u8>, Vec<u8>)>,
}

pub(crate) struct SecuredCreatedFile(File);

pub(crate) enum SecuredCreateOutcome {
    Created {
        file: SecuredCreatedFile,
        identity: FilesystemObjectIdentity,
    },
    CreatedIdentityUnknown {
        file: SecuredCreatedFile,
    },
}

pub(crate) struct RetainedStorageUsage {
    pub(crate) file_count: usize,
    pub(crate) logical_charge_bytes: u64,
}

pub(crate) struct RetainedStorageReservation {
    pub(crate) file_count: usize,
    pub(crate) logical_charge_bytes: u64,
}

impl RetainedStorageReservation {
    pub(crate) fn for_apply(
        original_default_stream_bytes: u64,
        original_auxiliary_charge_bytes: u64,
    ) -> io::Result<Self> {
        let logical_charge_bytes = original_default_stream_bytes
            .checked_mul(2)
            .and_then(|value| value.checked_add(crate::ooxml::MAX_PACKAGE_BYTES))
            .and_then(|value| {
                original_auxiliary_charge_bytes
                    .checked_mul(3)
                    .and_then(|auxiliary| value.checked_add(auxiliary))
            })
            .ok_or_else(|| io::Error::other("retained storage reservation overflowed"))?;
        Ok(Self {
            file_count: 3,
            logical_charge_bytes,
        })
    }

    pub(crate) fn for_undo(
        current_applied_default_stream_bytes: u64,
        current_applied_auxiliary_charge_bytes: u64,
    ) -> io::Result<Self> {
        Ok(Self {
            file_count: 1,
            logical_charge_bytes: current_applied_default_stream_bytes
                .checked_add(current_applied_auxiliary_charge_bytes)
                .ok_or_else(|| io::Error::other("retained storage reservation overflowed"))?,
        })
    }
}

impl FileSecuritySnapshot {
    pub(crate) fn capture(data: &File, _security: Option<&File>) -> io::Result<Self> {
        let metadata = data.metadata()?;
        #[cfg(unix)]
        let access = {
            use std::os::unix::fs::MetadataExt;

            if metadata.nlink() != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "secured workbook must have exactly one hard link",
                ));
            }
            PlatformFileAccess::Unix {
                mode: metadata.mode(),
                uid: metadata.uid(),
                gid: metadata.gid(),
                links: metadata.nlink(),
            }
        };
        #[cfg(windows)]
        let access = {
            use std::os::windows::fs::MetadataExt;

            PlatformFileAccess::Windows {
                attributes: metadata.file_attributes(),
            }
        };
        #[cfg(unix)]
        let auxiliary = capture_unix_xattrs(data)?;
        #[cfg(windows)]
        let auxiliary = Vec::new();
        Ok(Self { access, auxiliary })
    }

    pub(crate) fn creation_descriptor(
        &self,
        _parent: &ParentSecuritySnapshot,
    ) -> io::Result<CreationSecurityDescriptor> {
        Ok(CreationSecurityDescriptor {
            access: self.access.clone(),
            auxiliary: self.auxiliary.clone(),
        })
    }

    pub(crate) fn verify_created_access(&self, file: &SecuredCreatedFile) -> io::Result<()> {
        let actual = Self::capture(&file.0, None)?;
        if actual.access == self.access {
            Ok(())
        } else {
            Err(io::Error::other("secured file access metadata changed"))
        }
    }

    pub(crate) fn replay_and_verify_auxiliary(
        &self,
        file: &mut SecuredCreatedFile,
    ) -> io::Result<()> {
        #[cfg(unix)]
        replay_unix_xattrs(&file.0, &self.auxiliary)?;
        let actual = Self::capture(&file.0, None)?;
        if actual.auxiliary == self.auxiliary {
            Ok(())
        } else {
            Err(io::Error::other("secured file auxiliary metadata changed"))
        }
    }

    pub(crate) fn verify_complete(&self, file: &SecuredCreatedFile) -> io::Result<()> {
        let actual = Self::capture(&file.0, None)?;
        if &actual == self {
            Ok(())
        } else {
            Err(io::Error::other("secured file metadata changed"))
        }
    }

    pub(crate) fn access_sha256(&self) -> String {
        security_digest(format!("{:?}", self.access).as_bytes())
    }

    pub(crate) fn auxiliary_sha256(&self) -> String {
        security_digest(format!("{:?}", self.auxiliary).as_bytes())
    }

    pub(crate) fn auxiliary_charge_bytes(&self) -> u64 {
        self.auxiliary.iter().fold(0_u64, |total, (name, value)| {
            total
                .saturating_add(name.len() as u64)
                .saturating_add(value.len() as u64)
                .saturating_add(16)
        })
    }

    pub(crate) fn sha256(&self) -> String {
        security_digest(format!("{:?}:{:?}", self.access, self.auxiliary).as_bytes())
    }
}

impl CreationSecurityDescriptor {
    pub(crate) fn current_owner_only() -> io::Result<Self> {
        #[cfg(unix)]
        // libc exposes S_IFREG as u32 on Linux and i32 on macOS.
        #[allow(clippy::useless_conversion)]
        let mode = u32::from(libc::S_IFREG | 0o600);
        #[cfg(unix)]
        let access = PlatformFileAccess::Unix {
            mode,
            // SAFETY: geteuid takes no arguments and cannot fail.
            uid: unsafe { libc::geteuid() },
            // SAFETY: getegid takes no arguments and cannot fail.
            gid: unsafe { libc::getegid() },
            links: 1,
        };
        #[cfg(windows)]
        let access = PlatformFileAccess::Windows {
            attributes: windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
        };
        Ok(Self {
            access,
            auxiliary: Vec::new(),
        })
    }
}

impl SecuredCreatedFile {
    pub(crate) fn sync_all(&self) -> io::Result<()> {
        self.0.sync_all()
    }

    pub(crate) fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        file_identity(&self.0)
    }

    pub(crate) fn security_snapshot(&self) -> io::Result<FileSecuritySnapshot> {
        FileSecuritySnapshot::capture(&self.0, None)
    }

    pub(crate) fn len(&self) -> io::Result<u64> {
        Ok(self.0.metadata()?.len())
    }

    pub(crate) fn sha256(&mut self) -> io::Result<String> {
        use sha2::{Digest, Sha256};

        self.seek(SeekFrom::Start(0))?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = self.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        self.seek(SeekFrom::Start(0))?;
        Ok(format!("{:x}", digest.finalize()))
    }
}

impl Read for SecuredCreatedFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl Write for SecuredCreatedFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Seek for SecuredCreatedFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.0.seek(position)
    }
}

fn apply_creation_security(file: &File, security: &CreationSecurityDescriptor) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::{fd::AsRawFd, unix::fs::MetadataExt};

        let PlatformFileAccess::Unix { mode, uid, gid, .. } = security.access;
        let descriptor = file.as_raw_fd();
        let metadata = file.metadata()?;
        if (metadata.uid() != uid || metadata.gid() != gid)
            // SAFETY: descriptor is the live fd of file, which is borrowed for this call.
            && unsafe { libc::fchown(descriptor, uid, gid) } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let permission_mode = libc::mode_t::try_from(mode & 0o7777)
            .map_err(|_| io::Error::other("secured file mode is invalid"))?;
        // SAFETY: descriptor is the live fd of file, which is borrowed for this call.
        if unsafe { libc::fchmod(descriptor, permission_mode) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(windows)]
    {
        let PlatformFileAccess::Windows { attributes } = security.access;
        use std::{
            mem::size_of,
            os::windows::{fs::MetadataExt, io::AsRawHandle},
        };
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_BASIC_INFO, FileBasicInfo, GetFileInformationByHandleEx,
            SetFileInformationByHandle,
        };

        // SAFETY: FILE_BASIC_INFO is a plain C struct for which all-zero bytes are a valid value.
        let mut basic: FILE_BASIC_INFO = unsafe { std::mem::zeroed() };
        // SAFETY: the handle is borrowed from file and the buffer is a live FILE_BASIC_INFO of the stated size.
        if unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileBasicInfo,
                (&raw mut basic).cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        basic.FileAttributes = attributes;
        // SAFETY: as for GetFileInformationByHandleEx; the buffer is only read during the call.
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileBasicInfo,
                (&raw const basic).cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        file.sync_all()?;
        if file.metadata()?.file_attributes() != attributes {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Windows creation attributes could not be verified on the owned handle",
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn capture_unix_xattrs(file: &File) -> io::Result<Vec<(Vec<u8>, Vec<u8>)>> {
    use std::os::fd::AsRawFd;

    let descriptor = file.as_raw_fd();
    #[cfg(target_os = "macos")]
    // SAFETY: descriptor is the live fd of file; a null list with size 0 only queries the required length.
    let list_size = unsafe { libc::flistxattr(descriptor, std::ptr::null_mut(), 0, 0) };
    #[cfg(not(target_os = "macos"))]
    // SAFETY: descriptor is the live fd of file; a null list with size 0 only queries the required length.
    let list_size = unsafe { libc::flistxattr(descriptor, std::ptr::null_mut(), 0) };
    if list_size < 0 {
        return Err(io::Error::last_os_error());
    }
    if u64::try_from(list_size).unwrap_or(u64::MAX) > MAX_SECURITY_AUXILIARY_BYTES {
        return Err(io::Error::other(
            "file auxiliary metadata exceeded the size limit",
        ));
    }
    let mut names = vec![0_u8; list_size as usize];
    if list_size > 0 {
        #[cfg(target_os = "macos")]
        // SAFETY: names holds names.len() bytes, the length reported by the size query above.
        let read =
            unsafe { libc::flistxattr(descriptor, names.as_mut_ptr().cast(), names.len(), 0) };
        #[cfg(not(target_os = "macos"))]
        // SAFETY: names holds names.len() bytes, the length reported by the size query above.
        let read = unsafe { libc::flistxattr(descriptor, names.as_mut_ptr().cast(), names.len()) };
        if read != list_size {
            return Err(io::Error::other(
                "file auxiliary metadata changed during capture",
            ));
        }
    }
    let mut records = names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(<[u8]>::to_vec)
        .collect::<Vec<_>>();
    records.sort();
    records.dedup();
    if records.len() > MAX_SECURITY_AUXILIARY_RECORDS {
        return Err(io::Error::other(
            "file auxiliary metadata record count exceeded the limit",
        ));
    }

    let mut total = 0_u64;
    let mut output = Vec::with_capacity(records.len());
    for name in records {
        let c_name = std::ffi::CString::new(name.clone())
            .map_err(|_| io::Error::other("file auxiliary metadata name is invalid"))?;
        #[cfg(target_os = "macos")]
        // SAFETY: descriptor is live and c_name is NUL-terminated; a null buffer with size 0 only queries the value length.
        let value_size =
            unsafe { libc::fgetxattr(descriptor, c_name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
        #[cfg(not(target_os = "macos"))]
        // SAFETY: descriptor is live and c_name is NUL-terminated; a null buffer with size 0 only queries the value length.
        let value_size =
            unsafe { libc::fgetxattr(descriptor, c_name.as_ptr(), std::ptr::null_mut(), 0) };
        if value_size < 0 {
            return Err(io::Error::last_os_error());
        }
        total = total
            .checked_add(name.len() as u64)
            .and_then(|value| value.checked_add(value_size as u64))
            .and_then(|value| value.checked_add(16))
            .filter(|value| *value <= MAX_SECURITY_AUXILIARY_BYTES)
            .ok_or_else(|| io::Error::other("file auxiliary metadata exceeded the size limit"))?;
        let mut value = vec![0_u8; value_size as usize];
        if value_size > 0 {
            #[cfg(target_os = "macos")]
            // SAFETY: value holds value.len() bytes, the length from the size query, and c_name is NUL-terminated.
            let read = unsafe {
                libc::fgetxattr(
                    descriptor,
                    c_name.as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                    0,
                    0,
                )
            };
            #[cfg(not(target_os = "macos"))]
            // SAFETY: value holds value.len() bytes, the length from the size query, and c_name is NUL-terminated.
            let read = unsafe {
                libc::fgetxattr(
                    descriptor,
                    c_name.as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                )
            };
            if read != value_size {
                return Err(io::Error::other(
                    "file auxiliary metadata changed during capture",
                ));
            }
        }
        output.push((name, value));
    }
    Ok(output)
}

#[cfg(unix)]
fn replay_unix_xattrs(file: &File, records: &[(Vec<u8>, Vec<u8>)]) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    if records.len() > MAX_SECURITY_AUXILIARY_RECORDS {
        return Err(io::Error::other(
            "file auxiliary metadata record count exceeded the limit",
        ));
    }
    for (name, value) in records {
        let c_name = std::ffi::CString::new(name.clone())
            .map_err(|_| io::Error::other("file auxiliary metadata name is invalid"))?;
        #[cfg(target_os = "macos")]
        // SAFETY: the descriptor is live, c_name is NUL-terminated, and value is a live slice of value.len() bytes.
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c_name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        #[cfg(not(target_os = "macos"))]
        // SAFETY: the descriptor is live, c_name is NUL-terminated, and value is a live slice of value.len() bytes.
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c_name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn parent_security_snapshot(parent: &PlatformDirectory) -> io::Result<ParentSecuritySnapshot> {
    let metadata = parent.metadata()?;
    #[cfg(unix)]
    let access = {
        use std::os::unix::fs::MetadataExt;

        PlatformParentAccess::Unix {
            mode: metadata.mode(),
            uid: metadata.uid(),
            gid: metadata.gid(),
        }
    };
    #[cfg(windows)]
    let access = {
        use std::os::windows::fs::MetadataExt;

        PlatformParentAccess::Windows {
            attributes: metadata.file_attributes(),
        }
    };
    Ok(ParentSecuritySnapshot {
        identity: parent.identity()?,
        access,
    })
}

fn security_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
fn windows_backup_stream_auxiliary_charge(records: &[(u32, &str, u64)]) -> io::Result<u64> {
    const BACKUP_DATA: u32 = 1;
    let mut unnamed_default_seen = false;
    let mut charge = 0_u64;
    for (stream_id, name, payload_bytes) in records {
        if *stream_id == BACKUP_DATA && name.is_empty() {
            if unnamed_default_seen {
                return Err(io::Error::other(
                    "duplicate unnamed backup data stream was reported",
                ));
            }
            unnamed_default_seen = true;
            continue;
        }
        charge = charge
            .checked_add(name.len() as u64)
            .and_then(|value| value.checked_add(*payload_bytes))
            .and_then(|value| value.checked_add(16))
            .filter(|value| *value <= MAX_SECURITY_AUXILIARY_BYTES)
            .ok_or_else(|| io::Error::other("backup auxiliary stream budget was exceeded"))?;
    }
    if !unnamed_default_seen {
        return Err(io::Error::other(
            "unnamed backup data stream was not reported",
        ));
    }
    Ok(charge)
}

mod anchored;
mod unix_dir;
mod windows_dir;
#[allow(unused_imports)]
pub use anchored::*;
#[allow(unused_imports)]
pub use unix_dir::*;
#[allow(unused_imports)]
pub use windows_dir::*;

#[cfg(test)]
mod tests;
