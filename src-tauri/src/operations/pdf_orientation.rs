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
    parallel::bounded_map,
    pdf::inspect,
};

pub const OPERATION_ID: &str = "rfi.normalize_pdf_orientation";
const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
const MIN_ORIENTATION_CONFIDENCE: f64 = 10.0;
const MAX_DIRECTORY_ENTRIES: usize = 500_000;
const MAX_PDFS: usize = 50_000;
const MAX_PAGES: usize = 250_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfOrientationParams {
    pub rfi_system_root: PathBuf,
    pub review_root: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrientationDetection {
    pub rotate_clockwise: i64,
    pub confidence: f64,
    pub method: String,
}

pub trait OrientationDetector: Sync {
    fn detect_page(&self, path: &Path, page_index: usize) -> Result<OrientationDetection, String>;

    fn diagnostics(&self) -> String;
}

#[derive(Clone)]
struct PageTask {
    path: PathBuf,
    page_index: usize,
    current_rotation: i64,
}

struct PdfWork {
    path: PathBuf,
    relative: PathBuf,
    page_count: usize,
}

pub fn preview<D, P, C>(
    params: &PdfOrientationParams,
    detector: &D,
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    D: OrientationDetector,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let source_root = canonical_directory(&params.rfi_system_root, "RFI system root")?;
    let review_root = canonical_directory(&params.review_root, "orientation review root")?;
    if overlaps(&source_root, &review_root) {
        return Ok(Plan {
            warnings: vec![warning("error.rfi.orientationReviewRootMustBeSeparate", "")],
            ..empty_plan()
        });
    }
    let pdfs = collect_discipline_pdfs(&source_root)?;
    let mut plan = empty_plan();
    if pdfs.is_empty() {
        plan.warnings.push(warning(
            "warn.rfi.orientationNoPdfs",
            &source_root.to_string_lossy(),
        ));
        return Ok(plan);
    }

    let mut work = Vec::new();
    let mut tasks = Vec::new();
    for path in pdfs {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        let relative = path
            .strip_prefix(&source_root)
            .map_err(|_| BackendError::path_invalid("PDF escaped the selected RFI root"))?
            .to_path_buf();
        match inspect(&path) {
            Ok(facts) if facts.encrypted => plan.warnings.push(warning(
                "warn.rfi.orientationUnsupported",
                &format!("{} — encrypted PDF", relative.display()),
            )),
            Ok(facts) if facts.signed => plan.warnings.push(warning(
                "warn.rfi.orientationUnsupported",
                &format!("{} — digitally signed PDF", relative.display()),
            )),
            Ok(facts) if facts.page_count == 0 => plan.warnings.push(warning(
                "warn.rfi.orientationUnsupported",
                &format!("{} — PDF has no pages", relative.display()),
            )),
            Ok(facts) => {
                if tasks.len().saturating_add(facts.page_count) > MAX_PAGES {
                    return Err(BackendError::validation(format!(
                        "orientation scan exceeds the {MAX_PAGES}-page limit"
                    )));
                }
                for (page_index, page) in facts.pages.iter().enumerate() {
                    tasks.push(PageTask {
                        path: path.clone(),
                        page_index,
                        current_rotation: normalize_rotation(page.rotation)?,
                    });
                }
                work.push(PdfWork {
                    path,
                    relative,
                    page_count: facts.page_count,
                });
            }
            Err(error) => plan.warnings.push(warning(
                "warn.rfi.orientationUnsupported",
                &format!("{} — {error}", relative.display()),
            )),
        }
    }

    let (worker_count, results) = bounded_map(&tasks, |task| {
        detector.detect_page(&task.path, task.page_index)
    })
    .map_err(BackendError::request_failed)?;

    let total_pages = tasks.len() as u64;
    let mut by_file =
        BTreeMap::<PathBuf, Vec<(PageTask, Result<OrientationDetection, String>)>>::new();
    for (index, (task, result)) in tasks.into_iter().zip(results).enumerate() {
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: (index + 1) as u64,
            total: total_pages,
            subject: format!("{} · {}", task.path.display(), task.page_index + 1),
        });
        by_file
            .entry(task.path.clone())
            .or_default()
            .push((task, result));
    }

    let mut created_directories = BTreeSet::new();
    let mut verified_pages = 0_u64;
    let mut rotated_pages = 0_u64;
    let mut actionable_files = 0_u64;
    let mut manual_review_files = 0_u64;
    for item in work {
        let page_results = by_file.remove(&item.path).unwrap_or_default();
        let mut approved = Vec::new();
        let mut uncertain = Vec::new();
        for (task, result) in page_results {
            match result {
                Ok(detection)
                    if detection.confidence.is_finite()
                        && detection.confidence >= MIN_ORIENTATION_CONFIDENCE
                        && matches!(detection.rotate_clockwise, 0 | 90 | 180 | 270) =>
                {
                    verified_pages += 1;
                    if detection.rotate_clockwise != 0 {
                        let target = (task.current_rotation + detection.rotate_clockwise) % 360;
                        approved.push(json!({
                            "pageIndex": task.page_index,
                            "fromRotation": task.current_rotation,
                            "targetRotation": target,
                            "rotateClockwise": detection.rotate_clockwise,
                            "confidence": detection.confidence,
                            "method": detection.method,
                        }));
                    }
                }
                Ok(detection) => uncertain.push(format!(
                    "page {} confidence {:.1}",
                    task.page_index + 1,
                    detection.confidence
                )),
                Err(error) => uncertain.push(format!("page {} {error}", task.page_index + 1)),
            }
        }
        if !uncertain.is_empty() {
            manual_review_files += 1;
            plan.warnings.push(warning(
                "warn.rfi.orientationNeedsReview",
                &truncate(
                    &format!("{} — {}", item.relative.display(), uncertain.join(", ")),
                    360,
                ),
            ));
            continue;
        }
        if approved.is_empty() {
            continue;
        }
        let destination = review_root.join(&item.relative);
        if destination.exists() {
            plan.warnings.push(warning(
                "warn.rfi.orientationReviewExists",
                &item.relative.to_string_lossy(),
            ));
            continue;
        }
        add_directory_steps(
            destination
                .parent()
                .ok_or_else(|| BackendError::path_invalid("review PDF has no parent"))?,
            &review_root,
            &mut created_directories,
            &mut plan.steps,
        )?;
        let source_hash = file_sha256(&item.path)
            .map_err(|_| BackendError::path_invalid("source PDF could not be hashed"))?;
        rotated_pages += approved.len() as u64;
        actionable_files += 1;
        plan.steps.push(Step {
            kind: StepKind::PdfPageRotate,
            src: Some(item.path.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.pdfOrientationNormalized".to_string()),
            meta: JsonObject::from_iter([
                ("sourceSha256".to_string(), Value::String(source_hash)),
                (
                    "sourceRoot".to_string(),
                    Value::String(source_root.to_string_lossy().into_owned()),
                ),
                (
                    "reviewRoot".to_string(),
                    Value::String(review_root.to_string_lossy().into_owned()),
                ),
                (
                    "relativePath".to_string(),
                    Value::String(item.relative.to_string_lossy().into_owned()),
                ),
                ("pageCount".to_string(), Value::from(item.page_count as u64)),
                ("rotations".to_string(), Value::Array(approved)),
            ]),
        });
    }
    plan.meta.extend(JsonObject::from_iter([
        ("workerCount".to_string(), Value::from(worker_count as u64)),
        ("verifiedPages".to_string(), Value::from(verified_pages)),
        ("rotatedPages".to_string(), Value::from(rotated_pages)),
        ("actionableFiles".to_string(), Value::from(actionable_files)),
        (
            "manualReviewFiles".to_string(),
            Value::from(manual_review_files),
        ),
        (
            "detectorDiagnostics".to_string(),
            Value::String(detector.diagnostics()),
        ),
    ]));
    Ok(plan)
}

fn collect_discipline_pdfs(root: &Path) -> Result<Vec<PathBuf>, BackendError> {
    let mut disciplines = Vec::new();
    let mut visited = 0_usize;
    find_discipline_roots(root, &mut disciplines, &mut visited)?;
    disciplines.sort();
    disciplines.dedup();
    let mut pdfs = BTreeSet::new();
    for discipline in disciplines {
        collect_pdfs(&discipline, &mut pdfs, &mut visited)?;
        if pdfs.len() > MAX_PDFS {
            return Err(BackendError::validation(format!(
                "orientation scan exceeds the {MAX_PDFS}-PDF limit"
            )));
        }
    }
    Ok(pdfs.into_iter().collect())
}

fn find_discipline_roots(
    root: &Path,
    found: &mut Vec<PathBuf>,
    visited: &mut usize,
) -> Result<(), BackendError> {
    for entry in fs::read_dir(root)
        .map_err(|_| BackendError::path_invalid("RFI system folder could not be read"))?
    {
        let entry = entry.map_err(|_| BackendError::path_invalid("RFI folder entry failed"))?;
        *visited += 1;
        if *visited > MAX_DIRECTORY_ENTRIES {
            return Err(BackendError::validation(
                "RFI system is too large to scan safely",
            ));
        }
        let file_type = entry
            .file_type()
            .map_err(|_| BackendError::path_invalid("RFI folder metadata failed"))?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_uppercase();
        if DISCIPLINES.contains(&name.as_str()) {
            found.push(entry.path());
        } else {
            find_discipline_roots(&entry.path(), found, visited)?;
        }
    }
    Ok(())
}

fn collect_pdfs(
    root: &Path,
    output: &mut BTreeSet<PathBuf>,
    visited: &mut usize,
) -> Result<(), BackendError> {
    for entry in fs::read_dir(root)
        .map_err(|_| BackendError::path_invalid("discipline folder could not be read"))?
    {
        let entry = entry.map_err(|_| BackendError::path_invalid("discipline entry failed"))?;
        *visited += 1;
        if *visited > MAX_DIRECTORY_ENTRIES {
            return Err(BackendError::validation(
                "RFI system is too large to scan safely",
            ));
        }
        let file_type = entry
            .file_type()
            .map_err(|_| BackendError::path_invalid("discipline metadata failed"))?;
        if file_type.is_dir() && !file_type.is_symlink() {
            collect_pdfs(&entry.path(), output, visited)?;
        } else if file_type.is_file()
            && entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
        {
            output.insert(entry.path());
        }
    }
    Ok(())
}

fn add_directory_steps(
    directory: &Path,
    root: &Path,
    created: &mut BTreeSet<PathBuf>,
    steps: &mut Vec<Step>,
) -> Result<(), BackendError> {
    if directory == root || directory.exists() {
        return Ok(());
    }
    let parent = directory
        .parent()
        .ok_or_else(|| BackendError::path_invalid("review directory has no parent"))?;
    add_directory_steps(parent, root, created, steps)?;
    if created.insert(directory.to_path_buf()) {
        steps.push(Step {
            kind: StepKind::Mkdir,
            src: None,
            dst: Some(directory.to_string_lossy().into_owned()),
            note_key: Some("note.create_folder".to_string()),
            meta: JsonObject::new(),
        });
    }
    Ok(())
}

fn empty_plan() -> Plan {
    Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
        meta: JsonObject::from_iter([
            ("sourceReadOnly".to_string(), Value::Bool(true)),
            ("preservesDirectoryLayout".to_string(), Value::Bool(true)),
            (
                "confidenceThreshold".to_string(),
                Value::from(MIN_ORIENTATION_CONFIDENCE),
            ),
        ]),
    }
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BackendError> {
    let path = fs::canonicalize(path)
        .map_err(|_| BackendError::path_invalid(format!("{label} does not exist")))?;
    if !path.is_dir() {
        return Err(BackendError::path_invalid(format!(
            "{label} is not a directory"
        )));
    }
    Ok(path)
}

fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn normalize_rotation(rotation: i64) -> Result<i64, BackendError> {
    let normalized = rotation.rem_euclid(360);
    if matches!(normalized, 0 | 90 | 180 | 270) {
        Ok(normalized)
    } else {
        Err(BackendError::validation(
            "PDF page rotation is not a right angle",
        ))
    }
}

fn warning(key: &str, subject: &str) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.to_string(),
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push('…');
    }
    result
}
