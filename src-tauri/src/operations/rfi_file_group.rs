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
    rfi_code::{RfiMethod, exact_codes as shared_exact_codes},
};

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

#[derive(Clone, Debug)]
struct Detection {
    code: String,
    method_key: String,
    confidence: f64,
}

#[derive(Clone, Debug)]
struct FileAnalysis {
    path: PathBuf,
    detections: Vec<Detection>,
    issue: Option<String>,
    is_mail: bool,
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
                continue;
            }
            if review_target.exists() {
                plan.warnings.push(Warning {
                    key: "warn.rfi.fileGroup.exists".to_string(),
                    subject: review_target.to_string_lossy().into_owned(),
                });
                continue;
            }
            if !review_directory.is_dir() && !review_directory_planned {
                review_directory_planned = true;
                plan.steps
                    .push(directory_step(&review_directory, None, true));
            }
            plan.steps
                .push(copy_step(&analysis.path, &review_target, None, None, true));
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
            let directory = destination.join(&detection.code);
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
    plan.meta.insert(
        "editor".to_string(),
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
        }),
    );
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
        let allowed = match step.kind {
            StepKind::Mkdir => {
                let code = step
                    .meta
                    .get("code")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        BackendError::validation("RFI file-group folder step is invalid")
                    })?;
                selected_codes.contains(code)
            }
            StepKind::Copy => {
                let row_id = step
                    .meta
                    .get("rowId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        BackendError::validation("RFI file-group copy step is invalid")
                    })?;
                enabled.contains(row_id)
            }
            _ => {
                return Err(BackendError::validation(
                    "RFI file-group offered step is invalid",
                ));
            }
        };
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

fn discover_files(root: &Path) -> Result<Vec<PathBuf>, BackendError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)
        .map_err(|_| BackendError::path_invalid("RFI file source could not be read"))?
    {
        let entry = entry
            .map_err(|_| BackendError::path_invalid("RFI file source entry could not be read"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|_| BackendError::path_invalid("RFI file source entry changed"))?;
        if metadata.file_type().is_symlink() {
            return Err(BackendError::path_invalid(
                "RFI file source contains a symbolic link",
            ));
        }
        let hidden = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'));
        if metadata.is_file() && !hidden {
            files.push(entry.path());
            if files.len() > MAX_FILES {
                return Err(BackendError::validation(format!(
                    "RFI file source exceeds the {MAX_FILES}-file limit"
                )));
            }
        }
    }
    Ok(files)
}

fn analyze_file<A: OcrAdapter>(path: &Path, ocr: &A) -> FileAnalysis {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let filename_codes = exact_codes(file_name);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_mail = matches!(extension.as_str(), "eml" | "msg");
    let inspect_content = filename_codes.is_empty() || is_mail;
    let mut detections = filename_codes
        .into_iter()
        .map(|code| Detection {
            code,
            method_key: "rfi.fileGroup.method.filename".to_string(),
            confidence: 1.0,
        })
        .map(|detection| (detection.code.clone(), detection))
        .collect::<BTreeMap<_, _>>();
    let mut issue = None;
    if inspect_content {
        let content = match extension.as_str() {
            "pdf" => pdf_codes(path, ocr),
            "docx" | "xlsx" | "xlsm" | "pptx" => package_codes(path),
            _ => raw_codes(path),
        };
        match content {
            Ok(codes) => {
                let method_key = match extension.as_str() {
                    "pdf" => "rfi.fileGroup.method.pdf",
                    "eml" | "msg" => "rfi.fileGroup.method.mail",
                    "docx" | "xlsx" | "xlsm" | "pptx" => "rfi.fileGroup.method.office",
                    _ => "rfi.fileGroup.method.content",
                };
                for code in codes {
                    detections.entry(code.clone()).or_insert(Detection {
                        code,
                        method_key: method_key.to_string(),
                        confidence: 1.0,
                    });
                }
            }
            Err(error) => {
                if is_mail {
                    detections.clear();
                }
                issue = Some(error);
            }
        }
    }
    FileAnalysis {
        path: path.to_path_buf(),
        detections: detections.into_values().collect(),
        issue,
        is_mail,
    }
}

fn summarize_file_names(file_names: &BTreeSet<String>) -> String {
    const SHOWN: usize = 3;
    let shown = file_names.iter().take(SHOWN).cloned().collect::<Vec<_>>();
    let remaining = file_names.len().saturating_sub(shown.len());
    if remaining == 0 {
        shown.join(", ")
    } else {
        format!("{} (+{remaining})", shown.join(", "))
    }
}

fn pdf_codes<A: OcrAdapter>(path: &Path, ocr: &A) -> Result<BTreeSet<String>, String> {
    let facts = pdf::inspect(path).map_err(|error| error.to_string())?;
    if facts.page_count > MAX_PDF_PAGES {
        return Err(format!(
            "PDF exceeds the {MAX_PDF_PAGES}-page inspection limit"
        ));
    }
    let mut codes = BTreeSet::new();
    for page_index in 0..facts.page_count {
        match ocr.read_page(path, page_index) {
            Ok(read) if read.matched.method == RfiMethod::Strict => {
                codes.insert(read.matched.code);
            }
            Ok(_) => {}
            Err(OcrFailure::EngineMissing) => return Err(ocr.diagnostics()),
            Err(OcrFailure::Document(error)) => return Err(error),
        }
    }
    Ok(codes)
}

fn package_codes(path: &Path) -> Result<BTreeSet<String>, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("metadata: {error}"))?;
    if metadata.len() > MAX_PACKAGE_BYTES {
        return Err("Office package exceeds the size limit".to_string());
    }
    let file = fs::File::open(path).map_err(|error| format!("open: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("Office package: {error}"))?;
    if archive.len() > MAX_PACKAGE_ENTRIES {
        return Err("Office package has too many entries".to_string());
    }
    let mut total = 0_u64;
    let mut codes = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Office package entry: {error}"))?;
        if !entry.name().to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| "Office package text size overflow".to_string())?;
        if total > MAX_PACKAGE_TEXT_BYTES {
            return Err("Office package text exceeds the size limit".to_string());
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Office package read: {error}"))?;
        codes.extend(exact_codes(&xml_text(&bytes)?));
    }
    Ok(codes)
}

fn xml_text(bytes: &[u8]) -> Result<String, String> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(value)) => {
                let value = value
                    .decode()
                    .map_err(|error| format!("Office package XML text: {error}"))?;
                text.push_str(&value);
            }
            Ok(Event::CData(value)) => {
                let value = value
                    .decode()
                    .map_err(|error| format!("Office package XML CDATA: {error}"))?;
                text.push_str(&value);
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("Office package XML: {error}")),
            _ => {}
        }
    }
    Ok(text)
}

fn raw_codes(path: &Path) -> Result<BTreeSet<String>, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("metadata: {error}"))?;
    if metadata.len() > MAX_RAW_BYTES {
        return Err("file exceeds the content-inspection size limit".to_string());
    }
    let bytes = fs::read(path).map_err(|error| format!("read: {error}"))?;
    let mut codes = exact_codes(&String::from_utf8_lossy(&bytes));
    for offset in 0..=1 {
        if bytes.len() <= offset + 1 {
            continue;
        }
        let little = bytes[offset..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        let big = bytes[offset..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        codes.extend(exact_codes(&String::from_utf16_lossy(&little)));
        codes.extend(exact_codes(&String::from_utf16_lossy(&big)));
    }
    Ok(codes)
}

fn exact_codes(value: &str) -> BTreeSet<String> {
    static RFI: OnceLock<Regex> = OnceLock::new();
    let text = value.nfc().collect::<String>().to_uppercase();
    let regex = RFI.get_or_init(|| {
        Regex::new(r"AZE-SUN-INT-QMS-RFI-([A-Z]{3})-([0-9]{4})").expect("exact RFI regex is valid")
    });
    let mut codes = shared_exact_codes(&text);
    for captures in regex.captures_iter(&text) {
        let matched = captures.get(0).expect("RFI match exists");
        let before_invalid = text[..matched.start()]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
        let after_invalid = text[matched.end()..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric);
        if before_invalid || after_invalid {
            continue;
        }
        let discipline = &captures[1];
        codes.insert(matched.as_str().to_string());
        let bytes = text.as_bytes();
        let mut cursor = matched.end();
        loop {
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            if !bytes
                .get(cursor)
                .is_some_and(|byte| matches!(*byte, b'_' | b'/' | b',' | b';'))
            {
                break;
            }
            cursor += 1;
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            let Some(number) = bytes.get(cursor..cursor.saturating_add(4)) else {
                break;
            };
            if !number.iter().all(u8::is_ascii_digit)
                || text[cursor + 4..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric)
            {
                break;
            }
            let number = std::str::from_utf8(number).expect("ASCII digits are UTF-8");
            codes.insert(format!("AZE-SUN-INT-QMS-RFI-{discipline}-{number}"));
            cursor += 4;
        }
    }
    codes
}

fn directory_step(destination: &Path, code: Option<&str>, warning_review: bool) -> Step {
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

fn copy_step(
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

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BackendError> {
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

fn overlaps(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

fn unsafe_existing_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_dir())
}
