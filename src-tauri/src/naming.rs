use std::path::{Path, PathBuf};

pub const FILE_PREFIX: &str = "RFI No. ";

pub fn filename_for(code: &str) -> String {
    format!("{FILE_PREFIX}{code}.pdf")
}

pub fn folder_for(destination: &Path, code: &str) -> PathBuf {
    destination.join(code)
}

pub fn target_for(destination: &Path, code: &str) -> PathBuf {
    folder_for(destination, code).join(filename_for(code))
}

pub fn flat_target_for(destination: &Path, code: &str) -> PathBuf {
    destination.join(filename_for(code))
}

/// Final path component as text, or an empty string when there is none.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}
