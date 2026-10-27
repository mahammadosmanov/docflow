//! Creation-time security descriptors, file and parent security snapshots, and secured created files.

use super::*;

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

pub(crate) struct SecuredCreatedFile(pub(super) File);

pub(crate) enum SecuredCreateOutcome {
    Created {
        file: SecuredCreatedFile,
        identity: FilesystemObjectIdentity,
    },
    CreatedIdentityUnknown {
        file: SecuredCreatedFile,
    },
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
        let mut buffer = vec![0_u8; 64 * 1024];
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

pub(super) fn apply_creation_security(
    file: &File,
    security: &CreationSecurityDescriptor,
) -> io::Result<()> {
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

pub(super) fn parent_security_snapshot(
    parent: &PlatformDirectory,
) -> io::Result<ParentSecuritySnapshot> {
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
pub(super) fn windows_backup_stream_auxiliary_charge(
    records: &[(u32, &str, u64)],
) -> io::Result<u64> {
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
