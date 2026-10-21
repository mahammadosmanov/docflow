use super::*;

pub(crate) struct AnchoredPath {
    pub(super) absolute: PathBuf,
    pub(super) parent: PlatformDirectory,
    pub(super) name: OsString,
    pub(super) expected_identity: Option<FilesystemObjectIdentity>,
    pub(super) access: DirectoryAccess,
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

    #[cfg(any(windows, test))]
    pub(crate) fn remove_verified(&self, expected_sha256: &str) -> io::Result<()> {
        self.require_write()?;
        let mut file = self.open_mutable()?;
        if sha256_open_file(&mut file)? != expected_sha256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file changed before pinned deletion",
            ));
        }
        #[cfg(windows)]
        delete_open_handle(&file)?;
        #[cfg(not(windows))]
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
        #[cfg(windows)]
        if self.parent.identity()? != destination.parent.identity()? {
            let mut input = self.open_read()?;
            destination.copy_no_clobber_from_file(&mut input)?;
            drop(input);
            self.remove_file()?;
            self.parent.sync_all()?;
            return Ok(());
        }
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

    pub(crate) fn restore_original_name_no_clobber(
        &self,
        canonical: &AnchoredPath,
    ) -> Result<(), CapabilityEffectError> {
        self.require_write()
            .map_err(CapabilityEffectError::no_effect)?;
        canonical
            .require_write()
            .map_err(CapabilityEffectError::no_effect)?;
        let retained_parent = self
            .parent
            .identity()
            .map_err(CapabilityEffectError::no_effect)?;
        let canonical_parent = canonical
            .parent
            .identity()
            .map_err(CapabilityEffectError::no_effect)?;
        if retained_parent != canonical_parent {
            return Err(CapabilityEffectError::no_effect(io::Error::new(
                io::ErrorKind::InvalidInput,
                "original restoration must remain in the canonical parent",
            )));
        }
        self.parent
            .rename(&self.name, &canonical.parent, &canonical.name, false)
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

    pub(super) fn validate_expected_identity(&self, file: &File) -> io::Result<()> {
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

    pub(super) fn require_write(&self) -> io::Result<()> {
        if self.access == DirectoryAccess::ReadWrite {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read-only capability cannot mutate the filesystem",
            ))
        }
    }

    pub(super) fn create_temporary(&self) -> io::Result<(OsString, File)> {
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

pub(super) fn sha256_open_file(file: &mut File) -> io::Result<String> {
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

pub(super) fn validate_relative(path: &Path) -> io::Result<()> {
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
