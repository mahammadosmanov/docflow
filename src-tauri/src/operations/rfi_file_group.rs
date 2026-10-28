use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use quick_xml::{Reader, events::Event};
use regex::Regex;
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;
use zip::ZipArchive;

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    operations::process_scans::{OcrAdapter, OcrFailure},
    parallel::bounded_map_stream,
    pdf,
    rfi_code::{RfiMethod, exact_codes as shared_exact_codes, normalize_candidate_number},
};

mod analysis;
mod steps;
use analysis::*;
use steps::*;

pub const OPERATION_ID: &str = "rfi.group_files";

const REVIEW_DIR: &str = "_review";
const MAX_FILES: usize = 10_000;
const MAX_RAW_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PACKAGE_ENTRIES: usize = 4_096;
const MAX_PACKAGE_TEXT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PDF_PAGES: usize = 2_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupRfiFilesParams {
    pub source: PathBuf,
    pub destination: PathBuf,
}

pub fn preview<A, P, C>(
    params: &GroupRfiFilesParams,
    ocr: &A,
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let source = canonical_directory(&params.source, "RFI file source")?;
    let destination = canonical_directory(&params.destination, "RFI file destination")?;
    if overlaps(&source, &destination) {
        return Err(BackendError::path_invalid(
            "RFI file source and destination must be separate",
        ));
    }
    progress(ProgressEvent {
        phase: "file_group_discovery".to_string(),
        done: 0,
        total: 0,
        subject: source.to_string_lossy().into_owned(),
    });
    let mut files = discover_files(&source)?;
    files.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    progress(ProgressEvent {
        phase: "file_group_discovery".to_string(),
        done: files.len() as u64,
        total: files.len() as u64,
        subject: String::new(),
    });
    if cancelled() {
        return Err(BackendError::cancelled());
    }

    let total = files.len() as u64;
    let mut completed = 0_u64;
    let (worker_count, analyses) = bounded_map_stream(
        &files,
        |path| analyze_file(path, ocr),
        |index, _| {
            completed += 1;
            progress(ProgressEvent {
                phase: "file_group_analysis".to_string(),
                done: completed,
                total,
                subject: files[index].to_string_lossy().into_owned(),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    if cancelled() {
        return Err(BackendError::cancelled());
    }

    progress(ProgressEvent {
        phase: "file_group_planning".to_string(),
        done: 0,
        total,
        subject: String::new(),
    });
    let mut plan = Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
        meta: JsonObject::new(),
    };
    let mut offered_steps = Vec::new();
    let mut rows = Vec::new();
    let mut planned_directories = BTreeSet::new();
    let mut review_directory_planned = false;
    let mail_codes = analyses
        .iter()
        .filter(|analysis| analysis.is_mail)
        .flat_map(|analysis| analysis.detections.iter())
        .map(|detection| detection.code.clone())
        .collect::<BTreeSet<_>>();
    let mut files_without_mail = BTreeMap::<String, BTreeSet<String>>::new();
    for analysis in &analyses {
        let file_name = analysis
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("<invalid file name>");
        for detection in &analysis.detections {
            if !mail_codes.contains(&detection.code) {
                files_without_mail
                    .entry(detection.code.clone())
                    .or_default()
                    .insert(file_name.to_string());
            }
        }
    }
    for (code, file_names) in &files_without_mail {
        plan.warnings.push(Warning {
            key: "warn.rfi.fileGroup.missingEmail".to_string(),
            subject: format!("{code} — {}", summarize_file_names(file_names)),
        });
    }

    for (file_index, analysis) in analyses.into_iter().enumerate() {
        progress(ProgressEvent {
            phase: "file_group_planning".to_string(),
            done: file_index as u64 + 1,
            total,
            subject: analysis.path.to_string_lossy().into_owned(),
        });
        let file_name = analysis
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| BackendError::path_invalid("RFI file name is not valid Unicode"))?;
        if analysis.detections.is_empty() {
            plan_unmatched_file(
                &mut plan,
                &destination,
                &analysis,
                file_name,
                &mut review_directory_planned,
            );
            continue;
        }
        if let Some(issue) = &analysis.issue {
            plan.warnings.push(Warning {
                key: "warn.rfi.fileGroup.partialInspection".to_string(),
                subject: format!("{file_name} — {issue}"),
            });
        }
        for (detection_index, detection) in analysis.detections.iter().enumerate() {
            if !mail_codes.contains(&detection.code) {
                continue;
            }
            let discipline = detection.code.split('-').nth(5).unwrap();
            let directory = destination.join(discipline).join(&detection.code);
            let target = directory.join(file_name);
            if unsafe_existing_directory(&directory) {
                plan.warnings.push(Warning {
                    key: "warn.rfi.fileGroup.exists".to_string(),
                    subject: directory.to_string_lossy().into_owned(),
                });
                continue;
            }
            if target.exists() {
                plan.warnings.push(Warning {
                    key: "warn.rfi.fileGroup.exists".to_string(),
                    subject: target.to_string_lossy().into_owned(),
                });
                continue;
            }
            if !directory.is_dir() && planned_directories.insert(detection.code.clone()) {
                offered_steps.push(directory_step(&directory, Some(&detection.code), false));
            }
            let row_id = format!("file-group-{file_index}-{detection_index}");
            offered_steps.push(copy_step(
                &analysis.path,
                &target,
                Some(&row_id),
                Some(&detection.code),
                false,
            ));
            rows.push(json!({
                "_rowId": row_id,
                "enabled": true,
                "rfiCode": detection.code,
                "sourceFile": file_name,
                "method": detection.method_key,
                "confidence": detection.confidence,
            }));
        }
    }
    plan.steps.extend(offered_steps.iter().cloned());
    if files.is_empty() {
        plan.warnings.push(Warning {
            key: "warn.rfi.fileGroup.noFiles".to_string(),
            subject: source.to_string_lossy().into_owned(),
        });
    }
    plan.meta.insert(
        "offeredSteps".to_string(),
        serde_json::to_value(&offered_steps)
            .map_err(|error| BackendError::protocol(error.to_string()))?,
    );
    plan.meta.insert(
        "parallelAnalysis".to_string(),
        Value::Bool(!files.is_empty()),
    );
    plan.meta
        .insert("workerCount".to_string(), Value::from(worker_count as u64));
    plan.meta
        .insert("sourceReadOnly".to_string(), Value::Bool(true));
    plan.meta.insert(
        "missingEmailRfiCount".to_string(),
        Value::from(files_without_mail.len() as u64),
    );
    plan.meta
        .insert("editor".to_string(), file_group_editor(rows));
    progress(ProgressEvent {
        phase: "file_group_planning".to_string(),
        done: total,
        total,
        subject: String::new(),
    });
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(plan)
}

pub fn revise(plan: &Plan, payload: &Value) -> Result<Plan, BackendError> {
    if plan.op_id != OPERATION_ID {
        return Err(BackendError::validation(
            "RFI file-group plan identity is invalid",
        ));
    }
    let original_rows = plan
        .meta
        .get("editor")
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("RFI file-group rows are missing"))?;
    let submitted_rows = payload
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("RFI file-group rows are invalid"))?;
    if original_rows.len() != submitted_rows.len() {
        return Err(BackendError::validation(
            "RFI file-group rows cannot be added or removed",
        ));
    }
    let originals = original_rows
        .iter()
        .map(|row| {
            row.get("_rowId")
                .and_then(Value::as_str)
                .map(|row_id| (row_id.to_string(), row.clone()))
                .ok_or_else(|| BackendError::validation("RFI file-group row identity is invalid"))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut enabled = BTreeSet::new();
    let mut revised_rows = Vec::with_capacity(submitted_rows.len());
    let mut seen = BTreeSet::new();
    for submitted in submitted_rows {
        let row_id = submitted
            .get("_rowId")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation("RFI file-group row identity is invalid"))?;
        if !seen.insert(row_id.to_string()) {
            return Err(BackendError::validation(
                "RFI file-group row identity is duplicated",
            ));
        }
        let original = originals
            .get(row_id)
            .ok_or_else(|| BackendError::validation("RFI file-group row is not approved"))?;
        let requested = submitted
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| BackendError::validation("RFI file-group row state is invalid"))?;
        let mut expected = original.clone();
        expected["enabled"] = Value::Bool(requested);
        if submitted != &expected {
            return Err(BackendError::validation(
                "RFI file-group row fields cannot be changed",
            ));
        }
        if requested {
            enabled.insert(row_id.to_string());
        }
        revised_rows.push(expected);
    }
    let offered: Vec<Step> = serde_json::from_value(
        plan.meta
            .get("offeredSteps")
            .cloned()
            .ok_or_else(|| BackendError::validation("RFI file-group steps are missing"))?,
    )
    .map_err(|_| BackendError::validation("RFI file-group steps are invalid"))?;
    let selected_codes = originals
        .iter()
        .filter(|(row_id, _)| enabled.contains(*row_id))
        .filter_map(|(_, row)| row.get("rfiCode").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut steps = plan
        .steps
        .iter()
        .filter(|step| step.meta.get("warningReview").and_then(Value::as_bool) == Some(true))
        .cloned()
        .collect::<Vec<_>>();
    for step in offered {
        let allowed = offered_step_allowed(&step, &enabled, &selected_codes)?;
        if allowed {
            steps.push(step);
        }
    }
    let mut meta = plan.meta.clone();
    meta["editor"]["rows"] = Value::Array(revised_rows);
    Ok(Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps,
        warnings: plan.warnings.clone(),
        meta,
    })
}

fn offered_step_allowed(
    step: &Step,
    enabled: &BTreeSet<String>,
    selected_codes: &BTreeSet<&str>,
) -> Result<bool, BackendError> {
    match step.kind {
        StepKind::Mkdir => {
            let code = step
                .meta
                .get("code")
                .and_then(Value::as_str)
                .ok_or_else(|| BackendError::validation("RFI file-group folder step is invalid"))?;
            Ok(selected_codes.contains(code))
        }
        StepKind::Copy => {
            let row_id = step
                .meta
                .get("rowId")
                .and_then(Value::as_str)
                .ok_or_else(|| BackendError::validation("RFI file-group copy step is invalid"))?;
            Ok(enabled.contains(row_id))
        }
        _ => Err(BackendError::validation(
            "RFI file-group offered step is invalid",
        )),
    }
}

fn file_group_editor(rows: Vec<Value>) -> Value {
    json!({
        "kind": "recordTable",
        "layout": "compact",
        "titleKey": "rfi.fileGroup.editor.title",
        "rowCountKey": "rfi.fileGroup.editor.rowCount",
        "groupField": "rfiCode",
        "selectionGroupFields": ["rfiCode", "sourceFile"],
        "selectionEnabled": true,
        "bulkEnabled": true,
        "duplicateEnabled": false,
        "reorderable": false,
        "detailsEnabled": false,
        "copyEnabled": false,
        "requiredFields": ["enabled"],
        "columns": [
            {"key": "enabled", "labelKey": "rfi.fileGroup.column.enabled", "kind": "bool", "width": 64},
            {"key": "rfiCode", "labelKey": "rfi.fileGroup.column.rfiCode", "kind": "readonly", "width": 260, "bulk": false},
            {"key": "sourceFile", "labelKey": "rfi.fileGroup.column.sourceFile", "kind": "readonly", "width": 300, "bulk": false},
            {"key": "method", "labelKey": "rfi.fileGroup.column.method", "kind": "readonly", "display": "localizedKey", "width": 120, "bulk": false},
            {"key": "confidence", "labelKey": "ocr.confidence", "kind": "readonly", "display": "percent", "width": 112, "bulk": false}
        ],
        "rows": rows,
    })
}

fn plan_unmatched_file(
    plan: &mut Plan,
    destination: &Path,
    analysis: &FileAnalysis,
    file_name: &str,
    review_directory_planned: &mut bool,
) {
    let warning_key = if analysis.issue.is_some() {
        "warn.rfi.fileGroup.unreadable"
    } else {
        "warn.rfi.fileGroup.noCode"
    };
    plan.warnings.push(Warning {
        key: warning_key.to_string(),
        subject: analysis.issue.as_deref().map_or_else(
            || file_name.to_string(),
            |issue| format!("{file_name} — {issue}"),
        ),
    });
    let review_directory = destination.join(REVIEW_DIR);
    let review_target = review_directory.join(file_name);
    if unsafe_existing_directory(&review_directory) {
        plan.warnings.push(Warning {
            key: "warn.rfi.fileGroup.exists".to_string(),
            subject: review_directory.to_string_lossy().into_owned(),
        });
        return;
    }
    if review_target.exists() {
        plan.warnings.push(Warning {
            key: "warn.rfi.fileGroup.exists".to_string(),
            subject: review_target.to_string_lossy().into_owned(),
        });
        return;
    }
    if !review_directory.is_dir() && !*review_directory_planned {
        *review_directory_planned = true;
        plan.steps
            .push(directory_step(&review_directory, None, true));
    }
    plan.steps
        .push(copy_step(&analysis.path, &review_target, None, None, true));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_group_identifies_short_rfi_suffixes_but_not_other_numbers() {
        assert_eq!(
            exact_codes("AZE-SUN-INT-QMS-RFI-CIV-901"),
            BTreeSet::from(["AZE-SUN-INT-QMS-RFI-CIV-0901".to_string()])
        );
        assert_eq!(
            exact_codes("AZE-SUN-INT-QMS-RFI-CIV-0901"),
            exact_codes("AZE-SUN-INT-QMS-RFI-CIV-901")
        );
        assert!(exact_codes("ITP-CIV-901").is_empty());
        assert!(exact_codes("2026-09-01").is_empty());
        assert!(exact_codes("AZE-SUN-INT-QMS-RFI-CIV-09010").is_empty());
    }
}

#[cfg(test)]
mod offered_step_tests {
    use super::offered_step_allowed;
    use super::steps::{copy_step, directory_step};
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn folder_step_follows_selected_codes() {
        let step = directory_step(Path::new("/out/CIV-1"), Some("CIV-1"), false);
        let selected = BTreeSet::from(["CIV-1"]);
        assert!(offered_step_allowed(&step, &BTreeSet::new(), &selected).unwrap());
        assert!(!offered_step_allowed(&step, &BTreeSet::new(), &BTreeSet::new()).unwrap());
    }

    #[test]
    fn copy_step_follows_enabled_rows() {
        let step = copy_step(
            Path::new("/src/a.pdf"),
            Path::new("/out/a.pdf"),
            Some("row-1"),
            Some("CIV-1"),
            false,
        );
        let enabled = BTreeSet::from(["row-1".to_string()]);
        assert!(offered_step_allowed(&step, &enabled, &BTreeSet::new()).unwrap());
        assert!(!offered_step_allowed(&step, &BTreeSet::new(), &BTreeSet::new()).unwrap());
    }
}
