use super::*;

impl BackendRouter {
    pub(super) async fn select_paths(&self, params: Value) -> Result<Value, BackendError> {
        let request: SelectPathsParams =
            serde_json::from_value(params).map_err(|_| BackendError::invalid_params())?;
        let mode = self
            .native
            .selection_mode(&request.op_id, &request.param_name)?;
        let filter = path_dialog_filter(&request.op_id, &request.param_name);
        let paths = if request.from_drop {
            let dropped = self
                .drop_inbox
                .take(std::time::Instant::now())
                .ok_or_else(|| BackendError::path_invalid("no recent file drop"))?;
            validate_dropped_paths(dropped, mode, filter)?
        } else {
            let app = self.app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let dialog = app.dialog().file();
                let dialog = match filter {
                    Some((name, extensions)) => dialog.add_filter(name, extensions),
                    None => dialog,
                };
                let selected = match mode {
                    PathSelectionMode::Folder => dialog
                        .blocking_pick_folder()
                        .into_iter()
                        .collect::<Vec<_>>(),
                    PathSelectionMode::File => {
                        dialog.blocking_pick_file().into_iter().collect::<Vec<_>>()
                    }
                    PathSelectionMode::Files => dialog.blocking_pick_files().unwrap_or_default(),
                };
                selected
                    .into_iter()
                    .map(|path| {
                        path.into_path()
                            .map_err(|_| BackendError::path_invalid("selected path is invalid"))
                    })
                    .collect::<Result<Vec<_>, BackendError>>()
            })
            .await
            .map_err(|_| BackendError::request_failed("path selection task failed"))??
        };
        serde_json::to_value(self.native.register_selected_paths(
            &request.op_id,
            &request.param_name,
            paths,
        )?)
        .map_err(|_| BackendError::protocol("path selection response encoding failed"))
    }

    /// Template upload: Rust opens the picker and reads the file, so the
    /// webview never handles a path or file bytes.
    pub(super) async fn template_file(
        &self,
        method: BackendMethod,
        params: Value,
    ) -> Result<Value, BackendError> {
        let create = method == BackendMethod::TemplatesCreate;
        let new_template = if create {
            Some(
                serde_json::from_value::<crate::templates::editor::NewTemplate>(params.clone())
                    .map_err(|_| BackendError::invalid_params())?,
            )
        } else {
            None
        };
        let version = if create {
            None
        } else {
            Some(
                serde_json::from_value::<crate::native::TemplateVersionParams>(params)
                    .map_err(|_| BackendError::invalid_params())?,
            )
        };
        let app = self.app.clone();
        let picked = tauri::async_runtime::spawn_blocking(move || {
            let Some(file) = app
                .dialog()
                .file()
                .add_filter("Word / Excel", &["docx", "xlsx"])
                .blocking_pick_file()
            else {
                return Ok(None);
            };
            let path = file
                .into_path()
                .map_err(|_| BackendError::path_invalid("selected path is invalid"))?;
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size = std::fs::metadata(&path)
                .map_err(|_| BackendError::path_invalid("selected file is unavailable"))?
                .len();
            if size > crate::templates::MAX_TEMPLATE_BYTES {
                return Err(BackendError::validation("error.templates.fileSize"));
            }
            let bytes = std::fs::read(&path)
                .map_err(|_| BackendError::path_invalid("selected file is unavailable"))?;
            Ok(Some((name, bytes)))
        })
        .await
        .map_err(|_| BackendError::request_failed("template file task failed"))??;
        let Some((name, bytes)) = picked else {
            return Ok(Value::Null);
        };
        let native = Arc::clone(&self.native);
        tauri::async_runtime::spawn_blocking(move || match (new_template, version) {
            (Some(request), _) => native.create_template(&request, &name, &bytes),
            (_, Some(version)) => native.replace_template_file(&version.version_id, &name, &bytes),
            _ => Err(BackendError::invalid_params()),
        })
        .await
        .map_err(|_| BackendError::request_failed("template upload task failed"))?
    }

    pub(super) async fn reveal_path(&self, params: Value) -> Result<Value, BackendError> {
        let request: RevealPathParams =
            serde_json::from_value(params).map_err(|_| BackendError::invalid_params())?;
        let path = self.native.reveal_path(&request.selection_id)?;
        let app = self.app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            app.opener()
                .open_path(path.to_string_lossy().into_owned(), None::<String>)
                .map_err(|_| BackendError::request_failed("selected path could not be opened"))
        })
        .await
        .map_err(|_| BackendError::request_failed("path reveal task failed"))??;
        Ok(serde_json::json!({"ok": true}))
    }
}

/// How long a captured OS drop stays claimable by the field it landed on.
const DROP_FRESHNESS: std::time::Duration = std::time::Duration::from_secs(10);

/// The most recent OS file drop, captured from the native window event.
/// A drop is claimed at most once and only while fresh.
#[derive(Default)]
pub(crate) struct DropInbox(Mutex<Option<(std::time::Instant, Vec<PathBuf>)>>);

impl DropInbox {
    pub(crate) fn record(&self, paths: Vec<PathBuf>, now: std::time::Instant) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((now, paths));
    }

    pub(crate) fn take(&self, now: std::time::Instant) -> Option<Vec<PathBuf>> {
        let (at, paths) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()?;
        (now.saturating_duration_since(at) <= DROP_FRESHNESS && !paths.is_empty()).then_some(paths)
    }
}

fn validate_dropped_paths(
    dropped: Vec<PathBuf>,
    mode: PathSelectionMode,
    filter: Option<(&'static str, &'static [&'static str])>,
) -> Result<Vec<PathBuf>, BackendError> {
    if dropped.len() > 1000 || (mode != PathSelectionMode::Files && dropped.len() > 1) {
        return Err(BackendError::invalid_params());
    }
    for path in &dropped {
        if !path.exists() {
            return Err(BackendError::path_invalid("dropped path does not exist"));
        }
        match mode {
            PathSelectionMode::Folder if !path.is_dir() => {
                return Err(BackendError::path_invalid("dropped path must be a folder"));
            }
            PathSelectionMode::File | PathSelectionMode::Files if !path.is_file() => {
                return Err(BackendError::path_invalid("dropped path must be a file"));
            }
            _ => {}
        }
        if let Some((_, allowed_extensions)) = filter {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            if !allowed_extensions
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(ext))
            {
                return Err(BackendError::path_invalid(
                    "dropped file type is not accepted",
                ));
            }
        }
    }
    Ok(dropped)
}

#[cfg(test)]
mod drop_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_drop_is_claimed_once_and_only_while_fresh() {
        let inbox = DropInbox::default();
        let start = Instant::now();
        inbox.record(vec![PathBuf::from("/a")], start);
        assert_eq!(
            inbox.take(start + Duration::from_secs(1)),
            Some(vec![PathBuf::from("/a")])
        );
        assert_eq!(
            inbox.take(start + Duration::from_secs(1)),
            None,
            "claimed twice"
        );
        inbox.record(vec![PathBuf::from("/b")], start);
        assert_eq!(
            inbox.take(start + DROP_FRESHNESS + Duration::from_millis(1)),
            None,
            "stale drop"
        );
        inbox.record(Vec::new(), start);
        assert_eq!(inbox.take(start), None, "empty drop");
    }

    #[test]
    fn dropped_paths_must_match_the_field() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let file = dir.path().join("log.xlsx");
        std::fs::write(&file, b"x").expect("write file");
        let folder = dir.path().to_path_buf();
        let excel = Some(("Excel", &["xlsm", "xlsx"][..]));

        assert!(
            validate_dropped_paths(vec![folder.clone()], PathSelectionMode::Folder, None).is_ok()
        );
        assert!(
            validate_dropped_paths(vec![file.clone()], PathSelectionMode::Folder, None).is_err()
        );
        assert!(
            validate_dropped_paths(vec![folder.clone()], PathSelectionMode::File, None).is_err()
        );
        assert!(validate_dropped_paths(vec![file.clone()], PathSelectionMode::File, excel).is_ok());
        assert!(
            validate_dropped_paths(
                vec![file.clone()],
                PathSelectionMode::File,
                Some(("PDF", &["pdf"][..]))
            )
            .is_err()
        );
        assert!(
            validate_dropped_paths(
                vec![file.clone(), file.clone()],
                PathSelectionMode::File,
                None
            )
            .is_err()
        );
        assert!(
            validate_dropped_paths(
                vec![dir.path().join("missing")],
                PathSelectionMode::Files,
                None
            )
            .is_err()
        );
    }
}
