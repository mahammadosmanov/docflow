use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::Value;

use crate::{
    contracts::{JsonObject, Plan, ReviewItem, Step, StepKind, Warning},
    errors::BackendError,
    naming::{folder_for, target_for},
    operations::process_scans::OcrAdapter,
    rfi_code::{RfiMethod, canonical_and_find},
};

const MAX_REVIEW_ITEMS: usize = 10_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewAction {
    Put,
    Delete,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecision {
    pub path: PathBuf,
    pub action: ReviewAction,
    pub code: String,
}

pub fn scan<A: OcrAdapter>(
    destination_root: &Path,
    ocr: &A,
) -> Result<Vec<ReviewItem>, BackendError> {
    let destination_root = fs::canonicalize(destination_root)
        .map_err(|_| BackendError::path_invalid("review destination does not exist"))?;
    let review_root = destination_root.join("_review");
    if !review_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = fs::read_dir(&review_root)
        .map_err(|_| BackendError::path_invalid("review directory could not be read"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            (entry.metadata().ok()?.is_file()
                && !name.starts_with('.')
                && path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf")))
            .then_some(path)
        })
        .take(MAX_REVIEW_ITEMS + 1)
        .collect::<Vec<_>>();
    enforce_item_limit(files.len())?;
    files.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(files
        .into_iter()
        .map(|path| {
            let read = ocr.read(&path).ok();
            let matched = read.as_ref().map(|read| &read.matched);
            let code = matched
                .filter(|matched| matched.ok())
                .map(|matched| matched.code.clone())
                .unwrap_or_default();
            let target = (!code.is_empty()).then(|| target_for(&destination_root, &code));
            ReviewItem {
                name: path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string(),
                path: path.to_string_lossy().into_owned(),
                code,
                confidence: matched.map_or(0.0, |matched| matched.confidence),
                target_exists: target.as_ref().is_some_and(|target| target.exists()),
                target: target.map(|target| target.to_string_lossy().into_owned()),
            }
        })
        .collect())
}

pub fn preview(
    destination_root: &Path,
    trash_root: &Path,
    decisions: &[ReviewDecision],
    timestamp: &str,
) -> Result<Plan, BackendError> {
    enforce_item_limit(decisions.len())?;
    validate_timestamp(timestamp)?;
    let destination_root = fs::canonicalize(destination_root)
        .map_err(|_| BackendError::path_invalid("review destination does not exist"))?;
    let review_root = fs::canonicalize(destination_root.join("_review"))
        .map_err(|_| BackendError::path_invalid("review directory does not exist"))?;
    let trash_root = fs::canonicalize(trash_root)
        .map_err(|_| BackendError::path_invalid("recoverable trash does not exist"))?;
    let mut plan = empty_plan();

    for decision in decisions {
        let source = validate_review_source(&review_root, &decision.path)?;
        let source_name = source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| BackendError::path_invalid("review source name is invalid"))?;
        match decision.action {
            ReviewAction::Delete => {
                plan.steps.push(review_step(
                    StepKind::Move,
                    Some(&source),
                    Some(&trash_root.join(format!("{timestamp}_{source_name}"))),
                    "note.deleted",
                    "delete",
                    None,
                ));
            }
            ReviewAction::Put => {
                let (code, matched) = canonical_and_find(decision.code.trim());
                if code.is_empty() || matched.method != RfiMethod::Strict || matched.code != code {
                    plan.warnings.push(Warning {
                        key: "warn.no_code".to_string(),
                        subject: source_name.to_string(),
                    });
                    continue;
                }
                let target = target_for(&destination_root, &code);
                let folder = folder_for(&destination_root, &code);
                if target.exists() {
                    plan.steps.push(review_step(
                        StepKind::Move,
                        Some(&target),
                        Some(&trash_root.join(format!(
                            "{timestamp}_{}",
                            target
                                .file_name()
                                .and_then(|value| value.to_str())
                                .unwrap_or("RFI.pdf")
                        ))),
                        "note.displaced",
                        "put",
                        Some(&code),
                    ));
                }
                if !folder.is_dir() {
                    plan.steps.push(review_step(
                        StepKind::Mkdir,
                        None,
                        Some(&folder),
                        "note.folder_created",
                        "put",
                        Some(&code),
                    ));
                }
                plan.steps.push(review_step(
                    StepKind::Move,
                    Some(&source),
                    Some(&target),
                    "note.renamed",
                    "put",
                    Some(&code),
                ));
            }
        }
    }
    Ok(plan)
}

/// Build the mutation-free portion of a review decision that moves unreadable
/// PDFs into DocFlow's recoverable trash. The caller still has to cache and
/// apply this plan through `FilesystemAuthority` with grants for both roots.
pub fn recoverable_delete_plan(
    destination_root: &Path,
    trash_root: &Path,
    sources: &[std::path::PathBuf],
    timestamp: &str,
) -> Result<Plan, BackendError> {
    enforce_item_limit(sources.len())?;
    validate_timestamp(timestamp)?;
    let destination_root = fs::canonicalize(destination_root)
        .map_err(|_| BackendError::path_invalid("review destination does not exist"))?;
    let review_root = fs::canonicalize(destination_root.join("_review"))
        .map_err(|_| BackendError::path_invalid("review directory does not exist"))?;
    let trash_root = fs::canonicalize(trash_root)
        .map_err(|_| BackendError::path_invalid("recoverable trash does not exist"))?;
    if !review_root.starts_with(&destination_root) {
        return Err(BackendError::path_escape());
    }

    let mut steps = Vec::with_capacity(sources.len());
    for source in sources {
        let metadata = fs::symlink_metadata(source)
            .map_err(|_| BackendError::path_invalid("review source does not exist"))?;
        if !metadata.file_type().is_file() {
            return Err(BackendError::path_invalid(
                "review source must be a regular file",
            ));
        }
        let source = fs::canonicalize(source)
            .map_err(|_| BackendError::path_invalid("review source cannot be resolved"))?;
        if source.parent() != Some(review_root.as_path()) {
            return Err(BackendError::path_escape());
        }
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|name| !name.starts_with('.'))
            .ok_or_else(|| BackendError::path_invalid("review source name is invalid"))?;
        if !source
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        {
            return Err(BackendError::path_invalid("review source must be a PDF"));
        }
        let destination = trash_root.join(format!("{timestamp}_{name}"));
        let mut meta = JsonObject::new();
        meta.insert("action".to_string(), Value::String("delete".to_string()));
        steps.push(Step {
            kind: StepKind::Move,
            src: Some(source.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: Some("note.deleted".to_string()),
            meta,
        });
    }

    Ok(Plan {
        plan_id: String::new(),
        op_id: "rfi.review".to_string(),
        steps,
        warnings: Vec::new(),
        meta: JsonObject::new(),
    })
}

fn enforce_item_limit(count: usize) -> Result<(), BackendError> {
    if count > MAX_REVIEW_ITEMS {
        Err(BackendError::validation(format!(
            "review request exceeds the {MAX_REVIEW_ITEMS}-item limit"
        )))
    } else {
        Ok(())
    }
}

fn validate_timestamp(timestamp: &str) -> Result<(), BackendError> {
    if timestamp.is_empty()
        || timestamp.len() > 32
        || !timestamp.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        Err(BackendError::validation(
            "review trash timestamp is invalid",
        ))
    } else {
        Ok(())
    }
}

fn validate_review_source(review_root: &Path, source: &Path) -> Result<PathBuf, BackendError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|_| BackendError::path_invalid("review source does not exist"))?;
    if !metadata.file_type().is_file() {
        return Err(BackendError::path_invalid(
            "review source must be a regular file",
        ));
    }
    let source = fs::canonicalize(source)
        .map_err(|_| BackendError::path_invalid("review source cannot be resolved"))?;
    if source.parent() != Some(review_root) {
        return Err(BackendError::path_escape());
    }
    if !source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return Err(BackendError::path_invalid("review source must be a PDF"));
    }
    Ok(source)
}

fn empty_plan() -> Plan {
    Plan {
        plan_id: String::new(),
        op_id: "rfi.review".to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
        meta: JsonObject::new(),
    }
}

fn review_step(
    kind: StepKind,
    source: Option<&Path>,
    destination: Option<&Path>,
    note_key: &str,
    action: &str,
    code: Option<&str>,
) -> Step {
    let mut meta = JsonObject::new();
    meta.insert("action".to_string(), Value::String(action.to_string()));
    if let Some(code) = code {
        meta.insert("code".to_string(), Value::String(code.to_string()));
    }
    Step {
        kind,
        src: source.map(|path| path.to_string_lossy().into_owned()),
        dst: destination.map(|path| path.to_string_lossy().into_owned()),
        note_key: Some(note_key.to_string()),
        meta,
    }
}
