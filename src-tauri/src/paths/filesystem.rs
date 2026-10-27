use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime},
};

use crate::errors::BackendError;

use super::*;

pub(super) fn persisted_source_reselection_required() -> BackendError {
    BackendError::validation("error.print.queueSourceReselectionRequired")
}

pub(super) fn reject_lexical_escape(path: &Path) -> Result<(), BackendError> {
    if !path.is_absolute() {
        return Err(BackendError::path_invalid("path must be absolute"));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(BackendError::path_traversal());
    }
    Ok(())
}

pub(super) fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, BackendError> {
    let mut probe = path.to_path_buf();
    let mut suffix = Vec::<OsString>::new();
    loop {
        match fs::symlink_metadata(&probe) {
            Ok(_) => {
                let mut resolved = fs::canonicalize(&probe)
                    .map_err(|_| BackendError::path_invalid("path cannot be canonicalized"))?;
                for component in suffix.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = probe
                    .file_name()
                    .ok_or_else(|| BackendError::path_invalid("path has no existing ancestor"))?;
                suffix.push(name.to_os_string());
                if !probe.pop() {
                    return Err(BackendError::path_invalid("path has no existing ancestor"));
                }
            }
            Err(_) => {
                return Err(BackendError::path_invalid(
                    "path metadata could not be inspected",
                ));
            }
        }
    }
}

pub(crate) fn revalidate_under_root(
    root: &Path,
    candidate: &Path,
    intent: PathIntent,
) -> Result<(), BackendError> {
    if !candidate.starts_with(root) {
        return Err(BackendError::path_escape());
    }
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| BackendError::path_escape())?;
    let mut current = root.to_path_buf();
    inspect_ancestor(&current, root, true)?;
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if is_link_or_reparse(&metadata) {
                    return Err(BackendError::path_escape());
                }
                let canonical = fs::canonicalize(&current)
                    .map_err(|_| BackendError::path_invalid("path cannot be canonicalized"))?;
                if !canonical.starts_with(root) {
                    return Err(BackendError::path_escape());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => {
                return Err(BackendError::path_invalid(
                    "path metadata could not be inspected",
                ));
            }
        }
    }
    if intent == PathIntent::Read && !candidate.exists() {
        return Err(BackendError::path_invalid("source path does not exist"));
    }
    Ok(())
}

pub(super) fn inspect_ancestor(
    current: &Path,
    root: &Path,
    must_be_directory: bool,
) -> Result<(), BackendError> {
    let metadata = fs::symlink_metadata(current)
        .map_err(|_| BackendError::path_invalid("authorized root is unavailable"))?;
    if is_link_or_reparse(&metadata) || must_be_directory && !metadata.is_dir() {
        return Err(BackendError::path_escape());
    }
    let canonical = fs::canonicalize(current)
        .map_err(|_| BackendError::path_invalid("authorized root cannot be canonicalized"))?;
    if canonical != root {
        return Err(BackendError::path_escape());
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(super) fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub(crate) fn expired(created_at: SystemTime, now: SystemTime, ttl: Duration) -> bool {
    now.duration_since(created_at).is_ok_and(|age| age >= ttl)
}
