use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    fs_ops::file_sha256,
    naming::filename_for,
    operations::process_scans::{OcrAdapter, OcrFailure},
    pdf::{PdfInspection, inspect},
    pdf_publish::{same_file_identity, temporary_pdf_name_matches},
    rfi_code::RfiMethod,
};

const OPERATION_ID: &str = "rfi.stage_signed_first_pages";
const MIN_CONFIDENCE: f64 = 0.8;
const MAX_SIGNED_PDFS: usize = 10_000;
const MAX_ARCHIVE_ENTRIES: usize = 250_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageSignedPagesParams {
    pub signed_scans: PathBuf,
    pub archive_root: PathBuf,
    pub review_root: PathBuf,
    pub scan_subfolders: bool,
}

struct Candidate {
    path: PathBuf,
    confidence: f64,
    method: String,
}

pub fn preview<A, P, C>(
    params: &StageSignedPagesParams,
    ocr: &A,
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let signed_root = canonical_directory(&params.signed_scans, "signed scans")?;
    let archive_root = canonical_directory(&params.archive_root, "archive root")?;
    let review_root = canonical_directory(&params.review_root, "review root")?;
    if overlaps(&review_root, &archive_root) || overlaps(&review_root, &signed_root) {
        return Ok(Plan {
            warnings: vec![warning("error.rfi.reviewRootMustBeSeparate", "")],
            ..empty_plan()
        });
    }
    let pdfs = signed_pdfs(&signed_root, params.scan_subfolders)?;
    let mut plan = empty_plan();
    if pdfs.is_empty() {
        plan.warnings.push(warning(
            "warn.rfi.noSignedPdfs",
            &signed_root.to_string_lossy(),
        ));
        return Ok(plan);
    }

    let mut groups = BTreeMap::<String, Vec<Candidate>>::new();
    for (index, pdf) in pdfs.iter().enumerate() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: index as u64,
            total: pdfs.len() as u64,
            subject: display_name(pdf),
        });
        let read = match ocr.read(pdf) {
            Ok(read) => read,
            Err(OcrFailure::EngineMissing) => {
                return Ok(Plan {
                    warnings: vec![warning("warn.no_ocr", &ocr.diagnostics())],
                    ..empty_plan()
                });
            }
            Err(OcrFailure::Document(error)) => {
                plan.warnings.push(warning(
                    "warn.rfi.signedUnreadable",
                    &truncate(&format!("{} — {error}", display_name(pdf)), 300),
                ));
                continue;
            }
        };
        if !matches!(read.matched.method, RfiMethod::Strict | RfiMethod::Repaired) {
            plan.warnings
                .push(warning("warn.no_code", &display_name(pdf)));
            continue;
        }
        if read.matched.confidence < MIN_CONFIDENCE {
            plan.warnings
                .push(warning("warn.low_confidence", &display_name(pdf)));
            continue;
        }
        groups
            .entry(read.matched.code)
            .or_default()
            .push(Candidate {
                path: pdf.clone(),
                confidence: read.matched.confidence,
                method: read.source,
            });
    }

    let mut unique = BTreeMap::new();
    for (code, candidates) in groups {
        if candidates.len() != 1 {
            let names = candidates
                .iter()
                .map(|candidate| display_name(&candidate.path))
                .collect::<Vec<_>>()
                .join(", ");
            plan.warnings.push(warning(
                "warn.rfi.signedDuplicate",
                &truncate(&format!("{code} — {names}"), 300),
            ));
        } else {
            unique.insert(code, candidates.into_iter().next().expect("one candidate"));
        }
    }

    let archive_matches = archive_index(&archive_root, unique.keys().cloned().collect())?;
    let mut planned_directories = BTreeSet::new();
    let mut editor_rows = Vec::new();
    for (code, candidate) in unique {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        let matches = archive_matches.get(&code).cloned().unwrap_or_default();
        if matches.is_empty() {
            plan.warnings
                .push(warning("warn.rfi.archiveMissing", &code));
            continue;
        }
        if matches.len() > 1 {
            let names = matches
                .iter()
                .filter_map(|path| path.strip_prefix(&archive_root).ok())
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(", ");
            plan.warnings.push(warning(
                "warn.rfi.archiveAmbiguous",
                &truncate(&format!("{code} — {names}"), 300),
            ));
            continue;
        }
        let archive_pdf = &matches[0];
        let relative = archive_pdf
            .strip_prefix(&archive_root)
            .map_err(|_| BackendError::path_invalid("archive match escaped the selected root"))?;
        let destination = review_root.join(relative);
        if destination.exists() {
            let stale_temps = stale_hardlink_temps(&destination, &review_root);
            let destination_hash = if stale_temps.is_empty() {
                None
            } else {
                file_sha256(&destination).ok()
            };
            if let Some(destination_hash) = destination_hash {
                for stale_temp in stale_temps {
                    let mut meta = JsonObject::new();
                    meta.insert(
                        "reviewRoot".to_string(),
                        Value::String(review_root.to_string_lossy().into_owned()),
                    );
                    meta.insert(
                        "destinationSha256".to_string(),
                        Value::String(destination_hash.clone()),
                    );
                    plan.steps.push(Step {
                        kind: StepKind::CleanupTemp,
                        src: Some(stale_temp.to_string_lossy().into_owned()),
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: Some("note.rfi.cleanupStaleTemp".to_string()),
                        meta,
                    });
                }
                plan.warnings.push(warning(
                    "warn.rfi.staleTempFound",
                    &relative.to_string_lossy(),
                ));
            } else {
                plan.warnings.push(warning(
                    "warn.rfi.reviewExists",
                    &relative.to_string_lossy(),
                ));
            }
            continue;
        }
        if destination
            .ancestors()
            .take_while(|path| *path != review_root)
            .any(|path| {
                fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
            })
        {
            plan.warnings.push(warning(
                "warn.rfi.reviewPathBlocked",
                &relative.to_string_lossy(),
            ));
            continue;
        }
        let archive_facts = match inspect(archive_pdf) {
            Ok(facts) if fidelity_supported(&facts, PdfRole::Archive) => facts,
            Ok(facts) => {
                plan.warnings.push(warning(
                    "warn.rfi.pdfFidelityUnsupported",
                    &truncate(
                        &format!("{code} — {}", fidelity_reason(&facts, PdfRole::Archive)),
                        300,
                    ),
                ));
                continue;
            }
            Err(error) => {
                plan.warnings.push(warning(
                    "warn.rfi.pdfFidelityUnsupported",
                    &truncate(&format!("{code} — {error}"), 300),
                ));
                continue;
            }
        };
        let signed_facts = match inspect(&candidate.path) {
            Ok(facts) if fidelity_supported(&facts, PdfRole::SignedScan) => facts,
            Ok(facts) => {
                plan.warnings.push(warning(
                    "warn.rfi.pdfFidelityUnsupported",
                    &truncate(
                        &format!(
                            "{} — {}",
                            display_name(&candidate.path),
                            fidelity_reason(&facts, PdfRole::SignedScan)
                        ),
                        300,
                    ),
                ));
                continue;
            }
            Err(error) => {
                plan.warnings.push(warning(
                    "warn.rfi.pdfFidelityUnsupported",
                    &truncate(&format!("{} — {error}", display_name(&candidate.path)), 300),
                ));
                continue;
            }
        };
        if archive_facts.page_count == 0 || signed_facts.page_count == 0 {
            plan.warnings.push(warning(
                "warn.rfi.pdfFidelityUnsupported",
                &format!("{code} — PDF has no pages"),
            ));
            continue;
        }
        if archive_facts.pages[0].widgets > 0 || signed_facts.pages[0].widgets > 0 {
            plan.warnings.push(warning(
                "warn.rfi.pdfFidelityUnsupported",
                &format!("{code} — page-one form widgets require manual review"),
            ));
            continue;
        }
        let archive_hash = file_sha256(archive_pdf)
            .map_err(|_| BackendError::path_invalid("archive PDF could not be hashed"))?;
        let signed_hash = file_sha256(&candidate.path)
            .map_err(|_| BackendError::path_invalid("signed PDF could not be hashed"))?;

        add_directory_steps(
            destination
                .parent()
                .ok_or_else(|| BackendError::path_invalid("review destination has no parent"))?,
            &review_root,
            &mut planned_directories,
            &mut plan.steps,
        )?;
        if signed_facts.page_count > 1 {
            plan.warnings.push(warning(
                "warn.rfi.signedExtraPages",
                &display_name(&candidate.path),
            ));
        }
        if page_layout_differs(&archive_facts, &signed_facts) {
            plan.warnings
                .push(warning("warn.rfi.firstPageLayoutDiffers", &code));
        }
        let row_id = format!("signed-{code}");
        let mut meta = JsonObject::new();
        meta.insert("code".to_string(), Value::String(code.clone()));
        meta.insert(
            "signedPdf".to_string(),
            Value::String(candidate.path.to_string_lossy().into_owned()),
        );
        meta.insert(
            "signedRoot".to_string(),
            Value::String(signed_root.to_string_lossy().into_owned()),
        );
        meta.insert(
            "archiveRoot".to_string(),
            Value::String(archive_root.to_string_lossy().into_owned()),
        );
        meta.insert(
            "reviewRoot".to_string(),
            Value::String(review_root.to_string_lossy().into_owned()),
        );
        meta.insert(
            "relativePath".to_string(),
            Value::String(relative.to_string_lossy().into_owned()),
        );
        meta.insert("confidence".to_string(), Value::from(candidate.confidence));
        meta.insert("method".to_string(), Value::String(candidate.method));
        meta.insert("archiveSha256".to_string(), Value::String(archive_hash));
        meta.insert("signedSha256".to_string(), Value::String(signed_hash));
        meta.insert(
            "archivePageCount".to_string(),
            Value::from(archive_facts.page_count as u64),
        );
        meta.insert(
            "signedPageCount".to_string(),
            Value::from(signed_facts.page_count as u64),
        );
        meta.insert("rowId".to_string(), Value::String(row_id.clone()));
        plan.steps.push(Step {
            kind: StepKind::PdfFirstPageReplace,
            src: Some(archive_pdf.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.signedFirstPageStaged".to_string()),
            meta,
        });
        editor_rows.push(json!({
            "_rowId": row_id,
            "enabled": true,
            "code": code,
            "signedFile": display_name(&candidate.path),
            "archiveFile": relative.to_string_lossy(),
        }));
    }
    plan.meta.insert(
        "editor".to_string(),
        json!({
            "titleKey": "rfi.signed.editor.title",
            "rowCountKey": "rfi.signed.editor.rowCount",
            "groupField": "code",
            "selectionEnabled": false,
            "bulkEnabled": false,
            "duplicateEnabled": false,
            "columns": [
                {"key": "enabled", "labelKey": "rfi.signed.column.enabled", "kind": "bool"},
                {"key": "code", "labelKey": "rfi.signed.column.code", "kind": "readonly"},
                {"key": "signedFile", "labelKey": "rfi.signed.column.scan", "kind": "readonly"},
                {"key": "archiveFile", "labelKey": "rfi.signed.column.archive", "kind": "readonly"}
            ],
            "rows": editor_rows,
        }),
    );
    progress(ProgressEvent {
        phase: "preview".to_string(),
        done: pdfs.len() as u64,
        total: pdfs.len() as u64,
        subject: String::new(),
    });
    Ok(plan)
}

pub fn revise(plan: &Plan, payload: &Value) -> Result<Plan, BackendError> {
    if plan.op_id != OPERATION_ID {
        return Err(BackendError::validation(
            "signed-page plan identity is invalid",
        ));
    }
    let existing_rows = plan
        .meta
        .get("editor")
        .and_then(Value::as_object)
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("signed-page review rows are missing"))?;
    let supplied_rows = payload
        .as_object()
        .and_then(|object| object.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("signed-page review rows are invalid"))?;
    if existing_rows.len() != supplied_rows.len() {
        return Err(BackendError::validation(
            "signed-page review rows cannot be added or removed",
        ));
    }
    let existing_ids = existing_rows
        .iter()
        .filter_map(|row| row.get("_rowId").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let supplied_ids = supplied_rows
        .iter()
        .filter_map(|row| row.get("_rowId").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    if existing_ids.len() != existing_rows.len() || supplied_ids != existing_ids {
        return Err(BackendError::validation(
            "signed-page review row identity is invalid",
        ));
    }
    let enabled = supplied_rows
        .iter()
        .filter(|row| row.get("enabled").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|row| row.get("_rowId").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let enabled_destinations = plan
        .steps
        .iter()
        .filter(|step| step.kind == StepKind::PdfFirstPageReplace)
        .filter(|step| {
            step.meta
                .get("rowId")
                .and_then(Value::as_str)
                .is_some_and(|row_id| enabled.contains(row_id))
        })
        .filter_map(|step| step.dst.as_deref())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let steps = plan
        .steps
        .iter()
        .filter(|step| match step.kind {
            StepKind::PdfFirstPageReplace => step
                .meta
                .get("rowId")
                .and_then(Value::as_str)
                .is_some_and(|row_id| enabled.contains(row_id)),
            StepKind::Mkdir => step.dst.as_deref().is_some_and(|directory| {
                let directory = Path::new(directory);
                enabled_destinations
                    .iter()
                    .any(|destination| destination.starts_with(directory))
            }),
            StepKind::CleanupTemp => true,
            _ => false,
        })
        .cloned()
        .collect();
    let mut meta = plan.meta.clone();
    let mut revised_rows = existing_rows.clone();
    for row in &mut revised_rows {
        let row_id = row
            .get("_rowId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        row.as_object_mut().expect("validated editor row").insert(
            "enabled".to_string(),
            Value::Bool(enabled.contains(row_id.as_str())),
        );
    }
    meta.get_mut("editor")
        .and_then(Value::as_object_mut)
        .expect("validated editor")
        .insert("rows".to_string(), Value::Array(revised_rows));
    Ok(Plan {
        plan_id: String::new(),
        op_id: plan.op_id.clone(),
        steps,
        warnings: plan.warnings.clone(),
        meta,
    })
}

fn empty_plan() -> Plan {
    let mut meta = JsonObject::new();
    meta.insert("archiveReadOnly".to_string(), Value::Bool(true));
    meta.insert("preservesArchiveLayout".to_string(), Value::Bool(true));
    meta.insert("usesOnlySignedFirstPage".to_string(), Value::Bool(true));
    Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
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

fn stale_hardlink_temps(destination: &Path, review_root: &Path) -> Vec<PathBuf> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        _ => return Vec::new(),
    }
    if !fs::canonicalize(destination).is_ok_and(|path| path.starts_with(review_root)) {
        return Vec::new();
    }
    let Some(parent) = destination.parent() else {
        return Vec::new();
    };
    let Some(destination_name) = destination.file_name().and_then(|value| value.to_str()) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut stale = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| temporary_pdf_name_matches(name, destination_name))
        })
        .filter(|candidate| {
            fs::symlink_metadata(candidate).is_ok_and(|metadata| {
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && same_file_identity(destination, candidate)
            })
        })
        .filter(|candidate| {
            fs::canonicalize(candidate).is_ok_and(|path| path.starts_with(review_root))
        })
        .collect::<Vec<_>>();
    stale.sort();
    stale
}

fn signed_pdfs(root: &Path, recursive: bool) -> Result<Vec<PathBuf>, BackendError> {
    let mut files = Vec::new();
    collect_pdfs(root, recursive, &mut files)?;
    files.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    if files.len() > MAX_SIGNED_PDFS {
        return Err(BackendError::validation(format!(
            "signed scans exceed the {MAX_SIGNED_PDFS}-PDF limit"
        )));
    }
    Ok(files)
}

fn collect_pdfs(
    root: &Path,
    recursive: bool,
    output: &mut Vec<PathBuf>,
) -> Result<(), BackendError> {
    let entries = fs::read_dir(root)
        .map_err(|_| BackendError::path_invalid("signed scans could not be read"))?;
    for entry in entries {
        let entry =
            entry.map_err(|_| BackendError::path_invalid("signed scan entry could not be read"))?;
        let metadata = entry
            .metadata()
            .map_err(|_| BackendError::path_invalid("signed scan metadata could not be read"))?;
        let name = entry.file_name();
        let hidden = name.to_str().is_some_and(|name| name.starts_with('.'));
        if metadata.is_file()
            && !hidden
            && entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
        {
            output.push(entry.path());
        } else if recursive && metadata.is_dir() && !metadata.file_type().is_symlink() {
            collect_pdfs(&entry.path(), true, output)?;
        }
        if output.len() > MAX_SIGNED_PDFS {
            break;
        }
    }
    Ok(())
}

fn archive_index(
    root: &Path,
    codes: BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<PathBuf>>, BackendError> {
    let wanted = codes
        .iter()
        .map(|code| (filename_for(code).to_ascii_lowercase(), code.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut matches = codes
        .into_iter()
        .map(|code| (code, Vec::new()))
        .collect::<BTreeMap<_, _>>();
    let mut visited = 0usize;
    collect_archive(root, root, &wanted, &mut matches, &mut visited)?;
    for values in matches.values_mut() {
        values.sort();
    }
    Ok(matches)
}

fn collect_archive(
    root: &Path,
    directory: &Path,
    wanted: &BTreeMap<String, String>,
    output: &mut BTreeMap<String, Vec<PathBuf>>,
    visited: &mut usize,
) -> Result<(), BackendError> {
    for entry in fs::read_dir(directory)
        .map_err(|_| BackendError::path_invalid("archive directory could not be read"))?
    {
        let entry =
            entry.map_err(|_| BackendError::path_invalid("archive entry could not be read"))?;
        *visited += 1;
        if *visited > MAX_ARCHIVE_ENTRIES {
            return Err(BackendError::validation(format!(
                "archive exceeds the {MAX_ARCHIVE_ENTRIES}-entry limit"
            )));
        }
        let metadata = entry
            .metadata()
            .map_err(|_| BackendError::path_invalid("archive metadata could not be read"))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            collect_archive(root, &entry.path(), wanted, output, visited)?;
            continue;
        }
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_ascii_lowercase) else {
            continue;
        };
        let Some(code) = wanted.get(&name) else {
            continue;
        };
        let resolved = fs::canonicalize(entry.path())
            .map_err(|_| BackendError::path_invalid("archive PDF could not be resolved"))?;
        if !resolved.starts_with(root) || !resolved.is_file() {
            return Err(BackendError::path_escape());
        }
        output.entry(code.clone()).or_default().push(resolved);
    }
    Ok(())
}

fn add_directory_steps(
    parent: &Path,
    review_root: &Path,
    planned: &mut BTreeSet<PathBuf>,
    steps: &mut Vec<Step>,
) -> Result<(), BackendError> {
    let relative = parent
        .strip_prefix(review_root)
        .map_err(|_| BackendError::path_escape())?;
    let mut current = review_root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if planned.contains(&current) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => continue,
            Ok(_) => return Err(BackendError::path_invalid("review path is blocked")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(BackendError::path_invalid(
                    "review path could not be inspected",
                ));
            }
        }
        let mut meta = JsonObject::new();
        meta.insert(
            "reviewRoot".to_string(),
            Value::String(review_root.to_string_lossy().into_owned()),
        );
        steps.push(Step {
            kind: StepKind::Mkdir,
            src: None,
            dst: Some(current.to_string_lossy().into_owned()),
            note_key: Some("note.folder_created".to_string()),
            meta,
        });
        planned.insert(current.clone());
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum PdfRole {
    Archive,
    SignedScan,
}

fn fidelity_supported(facts: &PdfInspection, role: PdfRole) -> bool {
    !facts.encrypted
        && !facts.signed
        && !facts.catalog_entry_hashes.contains_key("StructTreeRoot")
        && (!matches!(role, PdfRole::SignedScan)
            || !facts.catalog_entry_hashes.contains_key("OCProperties"))
}

fn fidelity_reason(facts: &PdfInspection, role: PdfRole) -> &'static str {
    if facts.encrypted {
        "encrypted PDF"
    } else if facts.signed {
        "digitally signed PDF"
    } else if facts.catalog_entry_hashes.contains_key("StructTreeRoot") {
        "tagged PDF page structure cannot be safely preserved"
    } else if matches!(role, PdfRole::SignedScan)
        && facts.catalog_entry_hashes.contains_key("OCProperties")
    {
        "signed page depends on document-level optional-content state"
    } else {
        "unsupported PDF fidelity case"
    }
}

fn page_layout_differs(archive: &PdfInspection, signed: &PdfInspection) -> bool {
    archive.pages[0].media_box != signed.pages[0].media_box
        || archive.pages[0].crop_box != signed.pages[0].crop_box
        || archive.pages[0].rotation != signed.pages[0].rotation
}

fn warning(key: &str, subject: &str) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.to_string(),
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
