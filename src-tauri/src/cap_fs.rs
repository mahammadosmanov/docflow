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
#[cfg(unix)]
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

#[cfg(test)]
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
        match anchored.parent.create_dir(&anchored.name) {
            Ok(()) => match anchored
                .parent
                .open_dir_no_follow(&anchored.name, DirectoryAccess::ReadWrite)
                .and_then(|directory| directory.identity())
            {
                Ok(identity) => Ok(ArtifactParentCreateOutcome::Created { identity }),
                Err(_) => Ok(ArtifactParentCreateOutcome::CreatedIdentityUnknown),
            },
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
        let access = PlatformFileAccess::Unix {
            mode: u32::from(libc::S_IFREG | 0o600),
            uid: unsafe { libc::geteuid() },
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
            && unsafe { libc::fchown(descriptor, uid, gid) } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let permission_mode = libc::mode_t::try_from(mode & 0o7777)
            .map_err(|_| io::Error::other("secured file mode is invalid"))?;
        if unsafe { libc::fchmod(descriptor, permission_mode) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(windows)]
    {
        let PlatformFileAccess::Windows { attributes } = security.access;
        use std::os::windows::fs::MetadataExt;
        if file.metadata()?.file_attributes() != attributes {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "complete Windows creation security requires the secured NtCreateFile path",
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
    let list_size = unsafe { libc::flistxattr(descriptor, std::ptr::null_mut(), 0, 0) };
    #[cfg(not(target_os = "macos"))]
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
        let read =
            unsafe { libc::flistxattr(descriptor, names.as_mut_ptr().cast(), names.len(), 0) };
        #[cfg(not(target_os = "macos"))]
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
        .map(|name| name.to_vec())
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
        let value_size =
            unsafe { libc::fgetxattr(descriptor, c_name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
        #[cfg(not(target_os = "macos"))]
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

pub(crate) struct AnchoredPath {
    absolute: PathBuf,
    parent: PlatformDirectory,
    name: OsString,
    expected_identity: Option<FilesystemObjectIdentity>,
    access: DirectoryAccess,
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

    #[cfg(windows)]
    pub(crate) fn open_zone_identifier(&self) -> io::Result<Option<File>> {
        let verified_source = self.open_read()?;
        let stream = match self.parent.open_zone_identifier_stream(&self.name) {
            Ok(stream) => Ok(Some(stream)),
            Err(error) if error.raw_os_error() == Some(2) => Ok(None),
            Err(error) => Err(error),
        };
        drop(verified_source);
        stream
    }

    pub(crate) fn open_mutable(&self) -> io::Result<File> {
        self.require_write()?;
        let file = self.parent.open_file_no_follow(&self.name, true)?;
        self.validate_expected_identity(&file)?;
        Ok(file)
    }

    pub(crate) fn create_new(&self) -> io::Result<File> {
        self.require_write()?;
        self.parent.create_file_new(&self.name)
    }

    pub(crate) fn create_new_secured(
        &self,
        security: &CreationSecurityDescriptor,
    ) -> io::Result<SecuredCreatedFile> {
        let file = self.create_new()?;
        apply_creation_security(&file, security)?;
        Ok(SecuredCreatedFile(file))
    }

    pub(crate) fn create_new_secured_identified(
        &self,
        security: &CreationSecurityDescriptor,
    ) -> Result<SecuredCreateOutcome, CapabilityEffectError> {
        self.require_write()
            .map_err(CapabilityEffectError::no_effect)?;
        let raw = self
            .parent
            .create_file_new(&self.name)
            .map_err(CapabilityEffectError::no_effect)?;
        apply_creation_security(&raw, security).map_err(CapabilityEffectError::outcome_unknown)?;
        let file = SecuredCreatedFile(raw);
        if take_post_create_identity_failure() {
            return Ok(SecuredCreateOutcome::CreatedIdentityUnknown { file });
        }
        match file.identity() {
            Ok(identity) => Ok(SecuredCreateOutcome::Created { file, identity }),
            Err(_) => Ok(SecuredCreateOutcome::CreatedIdentityUnknown { file }),
        }
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
        self.require_write()?;
        self.parent.remove_file(&self.name)
    }

    pub(crate) fn remove_dir(&self) -> io::Result<()> {
        self.require_write()?;
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
        self.require_write()?;
        input.seek(SeekFrom::Start(0))?;
        #[cfg(windows)]
        {
            let mut output = self.parent.create_file_new(&self.name)?;
            let result = (|| {
                io::copy(input, &mut output)?;
                output.set_permissions(input.metadata()?.permissions())?;
                output.sync_all()?;
                self.parent.sync_all()
            })();
            if result.is_err() {
                let _ = delete_open_handle(&output);
                let _ = self.parent.sync_all();
            }
            result
        }
        #[cfg(not(windows))]
        {
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
    }

    pub(crate) fn write_no_clobber(&self, contents: &[u8]) -> io::Result<()> {
        self.require_write()?;
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
        self.require_write()?;
        destination.require_write()?;
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

    #[cfg(any(windows, test))]
    pub(crate) fn overwrite_verified_from(
        &self,
        source: &AnchoredPath,
        expected_current_sha256: &str,
    ) -> io::Result<()> {
        self.require_write()?;
        let mut input = source.open_read()?;
        let source_sha256 = sha256_open_file(&mut input)?;
        let source_permissions = input.metadata()?.permissions();
        let mut output = self.open_mutable()?;
        if sha256_open_file(&mut output)? != expected_current_sha256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "destination changed before pinned restore",
            ));
        }
        input.seek(SeekFrom::Start(0))?;
        output.seek(SeekFrom::Start(0))?;
        output.set_len(0)?;
        io::copy(&mut input, &mut output)?;
        output.set_permissions(source_permissions)?;
        output.sync_all()?;
        if sha256_open_file(&mut output)? != source_sha256 {
            return Err(io::Error::other("pinned restore verification failed"));
        }
        self.parent.sync_all()
    }

    pub(crate) fn retain_current_name_no_clobber(
        &self,
        retained: &AnchoredPath,
    ) -> Result<(), CapabilityEffectError> {
        self.require_write()
            .map_err(CapabilityEffectError::no_effect)?;
        retained
            .require_write()
            .map_err(CapabilityEffectError::no_effect)?;
        let source_parent = self
            .parent
            .identity()
            .map_err(CapabilityEffectError::no_effect)?;
        let retained_parent = retained
            .parent
            .identity()
            .map_err(CapabilityEffectError::no_effect)?;
        if source_parent != retained_parent {
            return Err(CapabilityEffectError::no_effect(io::Error::new(
                io::ErrorKind::InvalidInput,
                "retained publication must remain in the canonical parent",
            )));
        }
        self.parent
            .rename(&self.name, &retained.parent, &retained.name, false)
            .map_err(CapabilityEffectError::no_effect)?;
        self.parent
            .sync_all()
            .map_err(CapabilityEffectError::outcome_unknown)
    }

    pub(crate) fn sync_parent(&self) -> io::Result<()> {
        self.require_write()?;
        self.parent.sync_all()
    }

    pub(crate) fn capture_parent_security(&self) -> io::Result<ParentSecuritySnapshot> {
        parent_security_snapshot(&self.parent)
    }

    pub(crate) fn verify_parent_security(
        &self,
        expected: &ParentSecuritySnapshot,
    ) -> io::Result<()> {
        let actual = self.capture_parent_security()?;
        if &actual == expected {
            Ok(())
        } else {
            Err(io::Error::other("capability parent security changed"))
        }
    }

    pub(crate) fn replace_with(&self, source: &AnchoredPath) -> io::Result<()> {
        self.require_write()?;
        source.require_write()?;
        source
            .parent
            .rename(&source.name, &self.parent, &self.name, true)?;
        self.parent.sync_all()
    }

    pub(crate) fn sha256(&self) -> io::Result<String> {
        let mut file = self.open_read()?;
        sha256_open_file(&mut file)
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
            access: self.access,
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

    fn require_write(&self) -> io::Result<()> {
        if self.access == DirectoryAccess::ReadWrite {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read-only capability cannot mutate the filesystem",
            ))
        }
    }

    fn create_temporary(&self) -> io::Result<(OsString, File)> {
        self.require_write()?;
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

pub(crate) fn file_identity(file: &File) -> io::Result<FilesystemObjectIdentity> {
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

fn sha256_open_file(file: &mut File) -> io::Result<String> {
    use sha2::{Digest, Sha256};

    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(format!("{:x}", digest.finalize()))
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

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
fn unix_errno_pointer() -> *mut libc::c_int {
    unsafe { libc::__error() }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn unix_errno_pointer() -> *mut libc::c_int {
    unsafe { libc::__errno_location() }
}

#[cfg(unix)]
fn set_unix_errno(value: libc::c_int) {
    unsafe { *unix_errno_pointer() = value };
}

#[cfg(unix)]
fn unix_errno() -> libc::c_int {
    unsafe { *unix_errno_pointer() }
}

#[cfg(unix)]
impl PlatformDirectory {
    fn open_ambient(path: &Path, _access: DirectoryAccess) -> io::Result<Self> {
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

    fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.0.metadata()
    }

    fn entry_names(&self) -> io::Result<Vec<OsString>> {
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

    fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        use std::os::unix::fs::MetadataExt;

        let metadata = self.0.metadata()?;
        Ok(FilesystemObjectIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    fn open_dir_no_follow(&self, name: &OsStr, _access: DirectoryAccess) -> io::Result<Self> {
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
    fn open_ambient(path: &Path, access: DirectoryAccess) -> io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        let mut options = std::fs::OpenOptions::new();
        options
            .read(true)
            .write(access == DirectoryAccess::ReadWrite)
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

    fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.0.metadata()
    }

    fn entry_names(&self) -> io::Result<Vec<OsString>> {
        use std::{
            mem::{offset_of, size_of},
            os::windows::{ffi::OsStringExt, io::AsRawHandle},
        };
        use windows_sys::Win32::{
            Foundation::{ERROR_NO_MORE_FILES, HANDLE},
            Storage::FileSystem::{
                FILE_ID_BOTH_DIR_INFO, FileIdBothDirectoryInfo, GetFileInformationByHandleEx,
            },
        };

        let mut names = Vec::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    self.0.as_raw_handle() as HANDLE,
                    FileIdBothDirectoryInfo,
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

    fn identity(&self) -> io::Result<FilesystemObjectIdentity> {
        let (volume_serial, file_index) = windows_file_identity(&self.0)?;
        Ok(FilesystemObjectIdentity::Windows {
            volume_serial,
            file_index,
        })
    }

    fn open_dir_no_follow(&self, name: &OsStr, access: DirectoryAccess) -> io::Result<Self> {
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

    fn open_zone_identifier_stream(&self, name: &OsStr) -> io::Result<File> {
        let mut stream_name = name.to_os_string();
        stream_name.push(":Zone.Identifier:$DATA");
        nt_open_relative(
            &self.0,
            &stream_name,
            NtLeafKind::ReadFile,
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
        retry_windows_sharing_violation(|| {
            let source =
                nt_open_relative(&self.0, source, NtLeafKind::MutableAny, NtDisposition::Open)?;
            rename_open_handle(&source, &destination_dir.0, destination, replace)
        })
    }

    fn sync_all(&self) -> io::Result<()> {
        injected_directory_sync_failure()?;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::HANDLE, Storage::FileSystem::FlushFileBuffers};

        if unsafe { FlushFileBuffers(self.0.as_raw_handle() as HANDLE) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
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
                DELETE, FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, SYNCHRONIZE,
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

#[cfg(any(windows, test))]
fn retry_windows_sharing_violation(
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
    use std::io::{Read, Write};

    use tempfile::tempdir;

    use super::{
        ArtifactParentCreateOutcome, CapabilityRoot, DirectoryAccess, FileSecuritySnapshot,
        RetainedStorageReservation, SecuredCreateOutcome,
    };

    #[test]
    fn secured_create_anchored_create_new_no_clobber() {
        let fixture = tempdir().unwrap();
        let root = CapabilityRoot::open(fixture.path()).unwrap();
        let path = fixture.path().join("created.xlsx");
        let anchored = root.authorize(&path).unwrap().anchor().unwrap();

        let mut created = anchored.create_new().unwrap();
        created.write_all(b"owned bytes").unwrap();
        created.sync_all().unwrap();
        drop(created);

        let error = anchored.create_new().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        let mut contents = Vec::new();
        anchored
            .open_read()
            .unwrap()
            .read_to_end(&mut contents)
            .unwrap();
        assert_eq!(contents, b"owned bytes");
    }

    #[test]
    fn retained_publication_retain_current_name_is_same_parent_and_no_clobber() {
        let fixture = tempdir().unwrap();
        let root = CapabilityRoot::open(fixture.path()).unwrap();
        let canonical_path = fixture.path().join("Follow Up.xlsx");
        let retained_path = fixture
            .path()
            .join(".docflow-s5-retained-original-test.xlsx");
        std::fs::write(&canonical_path, b"original").unwrap();
        let canonical = root.authorize(&canonical_path).unwrap().anchor().unwrap();
        let retained = root.authorize(&retained_path).unwrap().anchor().unwrap();

        canonical.retain_current_name_no_clobber(&retained).unwrap();

        assert!(!canonical_path.exists());
        assert_eq!(std::fs::read(&retained_path).unwrap(), b"original");

        std::fs::write(&canonical_path, b"racer").unwrap();
        let error = canonical
            .retain_current_name_no_clobber(&retained)
            .unwrap_err();
        assert!(!error.is_outcome_unknown());
        assert_eq!(std::fs::read(&canonical_path).unwrap(), b"racer");
        assert_eq!(std::fs::read(&retained_path).unwrap(), b"original");
    }

    #[test]
    fn directory_access_and_retained_quota_read_only_root_rejects_mutation() {
        let fixture = tempdir().unwrap();
        let read_only =
            CapabilityRoot::open_with_access(fixture.path(), DirectoryAccess::ReadOnly).unwrap();
        let denied_path = fixture.path().join("denied.xlsx");
        let denied = read_only.authorize(&denied_path).unwrap().anchor().unwrap();
        assert_eq!(
            denied.create_new().unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        let protected_path = fixture.path().join("protected.xlsx");
        std::fs::write(&protected_path, b"must remain").unwrap();
        let protected = read_only
            .authorize(&protected_path)
            .unwrap()
            .anchor()
            .unwrap();
        assert_eq!(
            protected.remove_file().unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert_eq!(std::fs::read(&protected_path).unwrap(), b"must remain");

        let read_write =
            CapabilityRoot::open_with_access(fixture.path(), DirectoryAccess::ReadWrite).unwrap();
        let allowed_path = fixture.path().join("allowed.xlsx");
        read_write
            .authorize(&allowed_path)
            .unwrap()
            .anchor()
            .unwrap()
            .create_new()
            .unwrap();

        let legacy_path = fixture.path().join("legacy.xlsx");
        CapabilityRoot::open(fixture.path())
            .unwrap()
            .authorize(&legacy_path)
            .unwrap()
            .anchor()
            .unwrap()
            .create_new()
            .unwrap();
    }

    #[test]
    fn directory_access_and_retained_quota_artifact_parent_is_typed_and_no_clobber() {
        let fixture = tempdir().unwrap();
        let root = CapabilityRoot::open(fixture.path()).unwrap();
        let artifact_path = fixture.path().join(".docflow-backups");
        let artifact = root.authorize(&artifact_path).unwrap();

        let created = artifact.create_artifact_parent_no_clobber().unwrap();
        assert!(matches!(
            created,
            ArtifactParentCreateOutcome::Created { .. }
        ));
        assert!(artifact_path.is_dir());

        let existing = artifact.create_artifact_parent_no_clobber().unwrap();
        assert_eq!(existing, ArtifactParentCreateOutcome::AlreadyExists);
    }

    #[test]
    fn secured_create_identified_retains_live_handle_when_post_create_identity_is_unknown() {
        let fixture = tempdir().unwrap();
        let root = CapabilityRoot::open(fixture.path()).unwrap();
        let path = fixture.path().join("unknown-identity.xlsx");
        let anchored = root.authorize(&path).unwrap().anchor().unwrap();
        let descriptor = super::CreationSecurityDescriptor::current_owner_only().unwrap();
        super::inject_post_create_identity_failure_once();

        let SecuredCreateOutcome::CreatedIdentityUnknown { mut file } =
            anchored.create_new_secured_identified(&descriptor).unwrap()
        else {
            panic!("injected identity failure must preserve the live handle");
        };
        file.write_all(b"retained").unwrap();
        file.sync_all().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"retained");
    }

    #[test]
    fn pinned_overwrite_verifies_current_handle_before_restoring_exact_source_bytes() {
        let fixture = tempdir().unwrap();
        let root = CapabilityRoot::open(fixture.path()).unwrap();
        let backup_path = fixture.path().join("backup.xlsx");
        let workbook_path = fixture.path().join("workbook.xlsx");
        std::fs::write(&backup_path, b"original workbook").unwrap();
        std::fs::write(&workbook_path, b"applied workbook").unwrap();
        let backup = root.authorize(&backup_path).unwrap().anchor().unwrap();
        let workbook = root.authorize(&workbook_path).unwrap().anchor().unwrap();
        let applied_hash = workbook.sha256().unwrap();

        workbook
            .overwrite_verified_from(&backup, &applied_hash)
            .unwrap();
        assert_eq!(std::fs::read(&workbook_path).unwrap(), b"original workbook");

        std::fs::write(&workbook_path, b"later edit").unwrap();
        let error = workbook
            .overwrite_verified_from(&backup, &applied_hash)
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(std::fs::read(&workbook_path).unwrap(), b"later edit");
    }

    #[test]
    fn backup_stream_skip_unnamed_default_payload_is_excluded_from_auxiliary_budget() {
        let default_bytes = 4_u64 * 1024 * 1024 * 1024;
        let charge = super::windows_backup_stream_auxiliary_charge(&[
            (1, "", default_bytes),
            (1, ":audit:$DATA", 17),
            (3, "", 23),
        ])
        .unwrap();

        assert_eq!(charge, 17 + ":audit:$DATA".len() as u64 + 16 + 23 + 16);
    }

    #[test]
    fn directory_access_and_retained_quota_apply_reservation_charges_three_auxiliary_copies() {
        let reservation = RetainedStorageReservation::for_apply(100, 20).unwrap();
        assert_eq!(reservation.file_count, 3);
        assert_eq!(
            reservation.logical_charge_bytes,
            crate::ooxml::MAX_PACKAGE_BYTES + 260
        );
    }

    #[test]
    fn directory_access_and_retained_quota_scan_counts_only_bounded_s5_artifacts() {
        let fixture = tempdir().unwrap();
        let mut expected_charge = 0_u64;
        for (name, bytes) in [
            (".docflow-s5-backup-a.xlsx", b"a".as_slice()),
            (".docflow-s5-private-output-a.xlsx", b"bb".as_slice()),
            (".docflow-s5-retained-original-a.xlsx", b"ccc".as_slice()),
            (".docflow-s5-retained-applied-a.xlsx", b"dddd".as_slice()),
            ("ordinary.xlsx", b"not retained".as_slice()),
        ] {
            let path = fixture.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            if name.starts_with(".docflow-s5-") {
                let file = std::fs::File::open(path).unwrap();
                expected_charge += bytes.len() as u64
                    + FileSecuritySnapshot::capture(&file, None)
                        .unwrap()
                        .auxiliary_charge_bytes();
            }
        }
        let root = CapabilityRoot::open(fixture.path()).unwrap();
        let authorized = root.authorize_directory(fixture.path()).unwrap();

        let usage = authorized.s5_retained_storage_usage(4).unwrap();
        assert_eq!(usage.file_count, 4);
        assert_eq!(usage.logical_charge_bytes, expected_charge);
        assert!(authorized.s5_retained_storage_usage(3).is_err());
    }

    #[test]
    fn directory_access_and_retained_quota_undo_reservation_charges_retained_applied_auxiliary() {
        let reservation = RetainedStorageReservation::for_undo(100, 20).unwrap();
        assert_eq!(reservation.file_count, 1);
        assert_eq!(reservation.logical_charge_bytes, 120);
    }

    #[cfg(unix)]
    #[test]
    fn file_security_snapshot_round_trips_mode_and_complete_digest() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = tempdir().unwrap();
        let root = CapabilityRoot::open(fixture.path()).unwrap();
        let source_path = fixture.path().join("source.xlsx");
        std::fs::write(&source_path, b"source bytes").unwrap();
        std::fs::set_permissions(&source_path, std::fs::Permissions::from_mode(0o640)).unwrap();
        let source = root.authorize(&source_path).unwrap().anchor().unwrap();
        let source_file = source.open_read().unwrap();
        #[cfg(target_os = "macos")]
        set_test_xattr(&source_file, "user.docflow", b"retained metadata");
        let snapshot = FileSecuritySnapshot::capture(&source_file, None).unwrap();
        let destination_path = fixture.path().join("destination.xlsx");
        let destination = root.authorize(&destination_path).unwrap().anchor().unwrap();
        let parent = destination.capture_parent_security().unwrap();
        let descriptor = snapshot.creation_descriptor(&parent).unwrap();

        let mut created = destination.create_new_secured(&descriptor).unwrap();
        snapshot.verify_created_access(&created).unwrap();
        snapshot.replay_and_verify_auxiliary(&mut created).unwrap();
        created.write_all(b"source bytes").unwrap();
        created.sync_all().unwrap();
        snapshot.verify_complete(&created).unwrap();

        assert_eq!(
            std::fs::metadata(&destination_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        assert_eq!(snapshot.sha256().len(), 64);
        assert_eq!(snapshot.access_sha256().len(), 64);
        assert_eq!(snapshot.auxiliary_sha256().len(), 64);
        #[cfg(target_os = "macos")]
        assert_eq!(
            get_test_xattr(&created.0, "user.docflow"),
            b"retained metadata"
        );
    }

    #[cfg(target_os = "macos")]
    fn set_test_xattr(file: &std::fs::File, name: &str, value: &[u8]) {
        use std::{ffi::CString, os::fd::AsRawFd};

        let name = CString::new(name).unwrap();
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        assert_eq!(result, 0, "{}", std::io::Error::last_os_error());
    }

    #[cfg(target_os = "macos")]
    fn get_test_xattr(file: &std::fs::File, name: &str) -> Vec<u8> {
        use std::{ffi::CString, os::fd::AsRawFd};

        let name = CString::new(name).unwrap();
        let size = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                name.as_ptr(),
                std::ptr::null_mut(),
                0,
                0,
                0,
            )
        };
        assert!(size >= 0, "{}", std::io::Error::last_os_error());
        let mut value = vec![0_u8; size as usize];
        let read = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        assert_eq!(read, size, "{}", std::io::Error::last_os_error());
        value
    }

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

    #[test]
    fn windows_zone_identifier_open_is_identity_checked_and_root_relative() {
        let source = include_str!("cap_fs.rs")
            .rsplit_once("#[cfg(test)]\nmod tests")
            .expect("production capability source")
            .0;
        let anchored = source
            .split("pub(crate) fn open_zone_identifier(")
            .nth(1)
            .expect("AnchoredPath must expose one bounded Zone.Identifier reader")
            .split("pub(crate) fn identity(")
            .next()
            .expect("Zone.Identifier reader must remain in the anchored API");
        assert!(anchored.contains("self.open_read()?"));
        assert!(anchored.contains("self.parent.open_zone_identifier_stream(&self.name)"));
        let identity_open = anchored.find("self.open_read()?").unwrap();
        let stream_open = anchored
            .find("self.parent.open_zone_identifier_stream(&self.name)")
            .unwrap();
        let identity_close = anchored.find("drop(verified_source)").unwrap();
        assert!(identity_open < stream_open);
        assert!(stream_open < identity_close);

        let windows_directory = source
            .split("fn open_zone_identifier_stream(")
            .nth(1)
            .expect("Windows directory capability must own ADS opening")
            .split("fn create_file_new(")
            .next()
            .expect("ADS opening must remain bounded");
        assert!(windows_directory.contains("nt_open_relative("));
        assert!(windows_directory.contains("Zone.Identifier:$DATA"));
        assert!(!windows_directory.contains("self.canonical"));
        assert!(!windows_directory.contains("std::fs::File::open"));
    }

    #[test]
    fn windows_directory_sync_uses_flush_file_buffers() {
        let source = include_str!("cap_fs.rs");
        let windows = source
            .split("#[cfg(windows)]\nimpl PlatformDirectory")
            .nth(1)
            .expect("Windows directory implementation")
            .split("#[cfg(windows)]\n#[derive(Clone, Copy)]")
            .next()
            .expect("bounded Windows directory implementation");
        let sync = windows
            .split("fn sync_all(&self)")
            .nth(1)
            .expect("Windows directory sync")
            .split("}\n}")
            .next()
            .expect("bounded Windows directory sync");
        assert!(sync.contains("injected_directory_sync_failure()?"));
        assert!(sync.contains("FlushFileBuffers"));
        assert!(sync.contains("io::Error::last_os_error()"));
        assert!(
            sync.find("injected_directory_sync_failure()?").unwrap()
                < sync.find("FlushFileBuffers").unwrap()
        );
    }

    #[test]
    fn windows_capability_handles_share_delete_for_handle_relative_renames() {
        let source = include_str!("cap_fs.rs");
        let windows = source
            .split("#[cfg(windows)]\nimpl PlatformDirectory")
            .nth(1)
            .expect("Windows directory implementation");
        let ambient_open = windows
            .split("fn open_ambient")
            .nth(1)
            .and_then(|value| value.split("fn try_clone").next())
            .expect("ambient directory open");
        let relative_open = windows
            .split("fn nt_open_relative")
            .nth(1)
            .and_then(|value| value.split("fn rename_open_handle").next())
            .expect("relative NT open");

        assert!(ambient_open.contains("FILE_SHARE_DELETE"));
        assert!(relative_open.contains("FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE"));
    }

    #[test]
    fn windows_sharing_violation_retry_is_bounded_and_error_exact() {
        let mut attempts = 0;
        super::retry_windows_sharing_violation(|| {
            attempts += 1;
            if attempts < 3 {
                Err(std::io::Error::from_raw_os_error(32))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(attempts, 3);

        let mut denied_attempts = 0;
        let denied = super::retry_windows_sharing_violation(|| {
            denied_attempts += 1;
            Err(std::io::Error::from_raw_os_error(5))
        })
        .unwrap_err();
        assert_eq!(denied.raw_os_error(), Some(5));
        assert_eq!(denied_attempts, 1);
    }

    #[test]
    fn windows_copy_no_clobber_uses_the_owned_destination_handle_without_rename() {
        let source = include_str!("cap_fs.rs");
        let copy = source
            .split("pub(crate) fn copy_no_clobber_from_file")
            .nth(1)
            .and_then(|value| value.split("pub(crate) fn write_no_clobber").next())
            .expect("bounded copy implementation");
        let windows = copy
            .split("#[cfg(windows)]")
            .nth(1)
            .and_then(|value| value.split("#[cfg(not(windows))]").next())
            .expect("Windows copy branch");

        assert!(windows.contains("create_file_new"));
        assert!(windows.contains("output.sync_all()?"));
        assert!(!windows.contains("rename("));
    }
}
