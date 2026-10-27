use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    errors::BackendError,
    paths::{CallerIdentity, expired},
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SelectionKind {
    Folder,
    File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionScope {
    ReviewDestination,
    ReviewItem,
    RevealReviewItem,
    UndoDestination,
}

impl SelectionScope {
    fn exact(self) -> (&'static str, &'static str, SelectionKind) {
        match self {
            Self::ReviewDestination => ("rfi.review", "destination", SelectionKind::Folder),
            Self::ReviewItem | Self::RevealReviewItem => {
                ("rfi.review", "item", SelectionKind::File)
            }
            Self::UndoDestination => ("history.undo", "destination", SelectionKind::Folder),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PathSelection {
    pub selection_id: String,
    pub display_path: String,
    pub name: String,
}

#[derive(Clone)]
struct StoredSelection {
    path: PathBuf,
    caller: CallerIdentity,
    operation_id: String,
    parameter_name: String,
    kind: SelectionKind,
    created_at: SystemTime,
}

pub(crate) struct PathSelectionStore {
    ttl: Duration,
    selections: Mutex<HashMap<String, StoredSelection>>,
}

impl PathSelectionStore {
    pub(crate) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            selections: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn issue(
        &self,
        caller: &CallerIdentity,
        operation_id: &str,
        parameter_name: &str,
        path: &Path,
        kind: SelectionKind,
    ) -> Result<PathSelection, BackendError> {
        validate_scope(operation_id, parameter_name)?;
        let path = fs::canonicalize(path)
            .map_err(|_| BackendError::path_invalid("selected path does not exist"))?;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| BackendError::path_invalid("selected path cannot be inspected"))?;
        let kind_matches = match kind {
            SelectionKind::Folder => metadata.is_dir(),
            SelectionKind::File => metadata.is_file(),
        };
        if !kind_matches {
            return Err(BackendError::path_invalid(
                "selected path has the wrong kind",
            ));
        }

        let selection_id = Uuid::new_v4().to_string();
        let selection = StoredSelection {
            path: path.clone(),
            caller: caller.clone(),
            operation_id: operation_id.to_string(),
            parameter_name: parameter_name.to_string(),
            kind,
            created_at: SystemTime::now(),
        };
        let mut selections = lock(&self.selections);
        let now = SystemTime::now();
        selections.retain(|_, value| !expired(value.created_at, now, self.ttl));
        selections.insert(selection_id.clone(), selection);
        Ok(PathSelection {
            selection_id,
            display_path: path.to_string_lossy().into_owned(),
            name: path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_else(|| path.to_str().unwrap_or_default())
                .to_string(),
        })
    }

    pub(crate) fn resolve(
        &self,
        reference: &PathSelection,
        caller: &CallerIdentity,
        operation_id: &str,
        parameter_name: &str,
        kind: SelectionKind,
    ) -> Result<PathBuf, BackendError> {
        self.resolve_id(
            &reference.selection_id,
            caller,
            operation_id,
            parameter_name,
            kind,
        )
    }

    pub(crate) fn resolve_id(
        &self,
        selection_id: &str,
        caller: &CallerIdentity,
        operation_id: &str,
        parameter_name: &str,
        kind: SelectionKind,
    ) -> Result<PathBuf, BackendError> {
        let selection = self.resolve_stored(selection_id, caller)?;
        if selection.operation_id != operation_id
            || selection.parameter_name != parameter_name
            || selection.kind != kind
        {
            return Err(BackendError::path_grant_scope_mismatch());
        }
        Ok(selection.path)
    }

    pub(crate) fn resolve_scope(
        &self,
        selection_id: &str,
        caller: &CallerIdentity,
        scope: SelectionScope,
    ) -> Result<PathBuf, BackendError> {
        let (operation_id, parameter_name, kind) = scope.exact();
        self.resolve_id(selection_id, caller, operation_id, parameter_name, kind)
    }

    pub(crate) fn revoke(&self, caller: &CallerIdentity) {
        lock(&self.selections).retain(|_, selection| selection.caller != *caller);
    }
    pub(crate) fn revoke_selection(&self, selection_id: &str, caller: &CallerIdentity) {
        let mut selections = lock(&self.selections);
        if selections
            .get(selection_id)
            .is_some_and(|selection| selection.caller == *caller)
        {
            selections.remove(selection_id);
        }
    }

    fn resolve_stored(
        &self,
        selection_id: &str,
        caller: &CallerIdentity,
    ) -> Result<StoredSelection, BackendError> {
        let now = SystemTime::now();
        let mut selections = lock(&self.selections);
        if selections
            .get(selection_id)
            .is_some_and(|selection| expired(selection.created_at, now, self.ttl))
        {
            selections.remove(selection_id);
        }
        let selection = selections
            .get(selection_id)
            .cloned()
            .ok_or_else(BackendError::path_grant_unavailable)?;
        if selection.caller != *caller {
            return Err(BackendError::path_grant_scope_mismatch());
        }
        Ok(selection)
    }
}

fn validate_scope(operation_id: &str, parameter_name: &str) -> Result<(), BackendError> {
    if operation_id.is_empty()
        || operation_id.len() > 128
        || parameter_name.is_empty()
        || parameter_name.len() > 128
    {
        return Err(BackendError::invalid_params());
    }
    Ok(())
}

fn lock(
    selections: &Mutex<HashMap<String, StoredSelection>>,
) -> MutexGuard<'_, HashMap<String, StoredSelection>> {
    selections
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_paths_are_opaque_exact_scoped_and_caller_bound() {
        let temporary = tempfile::tempdir().unwrap();
        let selected = temporary.path().join("selected.pdf");
        fs::write(&selected, b"pdf").unwrap();
        let sibling = temporary.path().join("sibling.pdf");
        fs::write(&sibling, b"other").unwrap();
        let caller = CallerIdentity::new("owner", "session");
        let store = PathSelectionStore::new(Duration::from_secs(60));
        let reference = store
            .issue(
                &caller,
                "files.batch_print",
                "files",
                &selected,
                SelectionKind::File,
            )
            .unwrap();

        assert_ne!(reference.selection_id, selected.to_string_lossy());
        assert_eq!(
            store
                .resolve(
                    &reference,
                    &caller,
                    "files.batch_print",
                    "files",
                    SelectionKind::File,
                )
                .unwrap(),
            fs::canonicalize(&selected).unwrap()
        );
        assert_eq!(
            store
                .resolve(
                    &reference,
                    &caller,
                    "rfi.log_add",
                    "workbook",
                    SelectionKind::File,
                )
                .unwrap_err()
                .code,
            "path_grant_scope_mismatch"
        );
        assert_eq!(
            store
                .resolve_scope(
                    &reference.selection_id,
                    &CallerIdentity::new("other", "s"),
                    SelectionScope::RevealReviewItem,
                )
                .unwrap_err()
                .code,
            "path_grant_scope_mismatch"
        );
        assert_ne!(
            fs::canonicalize(selected).unwrap(),
            fs::canonicalize(sibling).unwrap()
        );
    }

    #[test]
    fn review_reveal_and_undo_consumers_reject_cross_operation_and_parameter_capabilities() {
        let temporary = tempfile::tempdir().unwrap();
        let file = temporary.path().join("review.pdf");
        fs::write(&file, b"pdf").unwrap();
        let caller = CallerIdentity::new("owner", "session");
        let store = PathSelectionStore::new(Duration::from_secs(60));
        let process_destination = store
            .issue(
                &caller,
                "rfi.process_scans",
                "dest",
                temporary.path(),
                SelectionKind::Folder,
            )
            .unwrap();
        let review_destination = store
            .issue(
                &caller,
                "rfi.review",
                "destination",
                temporary.path(),
                SelectionKind::Folder,
            )
            .unwrap();
        let review_item = store
            .issue(&caller, "rfi.review", "item", &file, SelectionKind::File)
            .unwrap();
        let undo_destination = store
            .issue(
                &caller,
                "history.undo",
                "destination",
                temporary.path(),
                SelectionKind::Folder,
            )
            .unwrap();

        for (selection_id, scope) in [
            (
                process_destination.selection_id.as_str(),
                SelectionScope::ReviewDestination,
            ),
            (
                review_item.selection_id.as_str(),
                SelectionScope::ReviewDestination,
            ),
            (
                review_destination.selection_id.as_str(),
                SelectionScope::ReviewItem,
            ),
            (
                undo_destination.selection_id.as_str(),
                SelectionScope::RevealReviewItem,
            ),
            (
                review_destination.selection_id.as_str(),
                SelectionScope::UndoDestination,
            ),
        ] {
            assert_eq!(
                store
                    .resolve_scope(selection_id, &caller, scope)
                    .unwrap_err()
                    .code,
                "path_grant_scope_mismatch"
            );
        }

        assert!(
            store
                .resolve_scope(
                    &review_destination.selection_id,
                    &caller,
                    SelectionScope::ReviewDestination,
                )
                .unwrap()
                .is_dir()
        );
        assert_eq!(
            store
                .resolve_scope(
                    &review_item.selection_id,
                    &caller,
                    SelectionScope::RevealReviewItem,
                )
                .unwrap(),
            fs::canonicalize(file).unwrap()
        );
        assert!(
            store
                .resolve_scope(
                    &undo_destination.selection_id,
                    &caller,
                    SelectionScope::UndoDestination,
                )
                .unwrap()
                .is_dir()
        );
    }
}
