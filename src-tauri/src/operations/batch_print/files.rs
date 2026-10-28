use super::*;

pub(super) fn ordered_unique_files(raw: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    raw.iter()
        .filter_map(|path| {
            let absolute = absolute_path(path);
            let key = if cfg!(target_os = "windows") {
                absolute.to_string_lossy().to_lowercase()
            } else {
                absolute.to_string_lossy().into_owned()
            };
            seen.insert(key).then_some(absolute)
        })
        .collect()
}

pub(super) fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub(super) fn regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

pub(super) fn file_format(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("file")
        .to_uppercase()
}

pub(super) fn is_excel_workbook(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "xlsx" | "xlsm"))
}
