use super::*;

impl BackendRouter {
    pub(super) async fn select_paths(&self, params: Value) -> Result<Value, BackendError> {
        let request: SelectPathsParams =
            serde_json::from_value(params).map_err(|_| BackendError::invalid_params())?;
        let mode = self
            .native
            .selection_mode(&request.op_id, &request.param_name)?;
        let filter = path_dialog_filter(&request.op_id, &request.param_name);
        let paths = if !request.dropped_paths.is_empty() {
            if request.dropped_paths.len() > 1000 {
                return Err(BackendError::invalid_params());
            }
            if mode != PathSelectionMode::Files && request.dropped_paths.len() > 1 {
                return Err(BackendError::invalid_params());
            }
            let mut resolved = Vec::new();
            for path_str in request.dropped_paths {
                let path = PathBuf::from(path_str);
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
                        .map(str::to_ascii_lowercase)
                        .unwrap_or_default();
                    if !allowed_extensions
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(&ext))
                    {
                        return Err(BackendError::path_invalid(
                            "dropped file type is not accepted",
                        ));
                    }
                }
                resolved.push(path);
            }
            resolved
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
