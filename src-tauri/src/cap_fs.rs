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

mod anchored;
mod retained_storage;
mod security;
mod unix_dir;
#[cfg(unix)]
mod unix_xattrs;
mod windows_dir;
#[allow(unused_imports)]
pub use anchored::*;
pub(crate) use retained_storage::*;
pub(crate) use security::*;
#[allow(unused_imports)]
pub use unix_dir::*;
#[cfg(unix)]
use unix_xattrs::{capture_unix_xattrs, replay_unix_xattrs};
#[allow(unused_imports)]
pub use windows_dir::*;

#[cfg(test)]
mod tests;
