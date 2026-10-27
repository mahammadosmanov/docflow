//! Destination directory and copy step construction for grouped RFI files.

use super::*;

pub(super) fn directory_step(destination: &Path, code: Option<&str>, warning_review: bool) -> Step {
    let mut meta = JsonObject::new();
    if let Some(code) = code {
        meta.insert("code".to_string(), Value::String(code.to_string()));
    }
    if warning_review {
        meta.insert("warningReview".to_string(), Value::Bool(true));
    }
    Step {
        kind: StepKind::Mkdir,
        src: None,
        dst: Some(destination.to_string_lossy().into_owned()),
        note_key: Some("note.rfi.fileGroup.folderCreated".to_string()),
        meta,
    }
}

pub(super) fn copy_step(
    source: &Path,
    destination: &Path,
    row_id: Option<&str>,
    code: Option<&str>,
    warning_review: bool,
) -> Step {
    let mut meta = JsonObject::new();
    if let Some(row_id) = row_id {
        meta.insert("rowId".to_string(), Value::String(row_id.to_string()));
    }
    if let Some(code) = code {
        meta.insert("code".to_string(), Value::String(code.to_string()));
    }
    if warning_review {
        meta.insert("warningReview".to_string(), Value::Bool(true));
    }
    Step {
        kind: StepKind::Copy,
        src: Some(source.to_string_lossy().into_owned()),
        dst: Some(destination.to_string_lossy().into_owned()),
        note_key: Some(if warning_review {
            "note.rfi.fileGroup.reviewCopy".to_string()
        } else {
            "note.rfi.fileGroup.copy".to_string()
        }),
        meta,
    }
}

pub(super) fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BackendError> {
    let path = fs::canonicalize(path)
        .map_err(|_| BackendError::path_invalid(format!("{label} does not exist")))?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err(BackendError::path_invalid(format!(
            "{label} is not a directory"
        )))
    }
}

pub(super) fn overlaps(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

pub(super) fn unsafe_existing_directory(path: &Path) -> bool {
    let mut candidate = Some(path);
    while let Some(current) = candidate {
        if fs::symlink_metadata(current)
            .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_dir())
        {
            return true;
        }
        candidate = current.parent();
    }
    false
}
