use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    fs_ops::file_sha256,
    parallel::bounded_map_stream,
    pdf::inspect,
    scan_index::{FileFingerprint, ScanIndex, ScanIndexStats},
};

pub const OPERATION_ID: &str = "rfi.normalize_pdf_orientation";
const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
const MIN_ORIENTATION_CONFIDENCE: f64 = 10.0;
const MAX_DIRECTORY_ENTRIES: usize = 500_000;
const MAX_PDFS: usize = 50_000;
const MAX_PAGES: usize = 250_000;
const ORIENTATION_SCAN_PROFILE: &str = "content-osd-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfOrientationParams {
    pub rfi_system_root: PathBuf,
    pub review_root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    source_hash: Option<String>,
    cached: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedPageOrientation {
    current_rotation: i64,
    detection: Option<OrientationDetection>,
    error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedPdfOrientation {
    page_count: usize,
    source_sha256: Option<String>,
    pages: Vec<CachedPageOrientation>,
    unsupported: Option<String>,
}

pub fn preview<D, P, C>(
    params: &PdfOrientationParams,
    detector: &D,
    progress: P,
    cancelled: C,
) -> Result<Plan, BackendError>
where
    D: OrientationDetector,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    preview_indexed(params, detector, progress, cancelled, None, "local", false)
}

#[allow(clippy::too_many_arguments)]
pub fn preview_indexed<D, P, C>(
    params: &PdfOrientationParams,
    detector: &D,
    mut progress: P,
    mut cancelled: C,
    scan_index: Option<&ScanIndex>,
    owner_id: &str,
    force_rescan: bool,
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

    let mut stats = ScanIndexStats {
        discovered: pdfs.len() as u64,
        force_rescan,
        ..ScanIndexStats::default()
    };
    let run = scan_index
        .map(|index| {
            index
                .begin_run(
                    owner_id,
                    OPERATION_ID,
                    ORIENTATION_SCAN_PROFILE,
                    &source_root,
                    force_rescan,
                )
                .map(|(run, prior)| {
                    stats.prior_interrupted_runs = prior;
                    run
                })
        })
        .transpose()
        .map_err(|error| {
            BackendError::request_failed(format!("scan index unavailable: {error}"))
        })?;
    let mut work = Vec::new();
    let mut tasks = Vec::new();
    let mut initial_fingerprints = BTreeMap::<PathBuf, FileFingerprint>::new();
    let mut by_file =
        BTreeMap::<PathBuf, Vec<(PageTask, Result<OrientationDetection, String>)>>::new();
    for path in pdfs {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        let relative = path
            .strip_prefix(&source_root)
            .map_err(|_| BackendError::path_invalid("PDF escaped the selected RFI root"))?
            .to_path_buf();
        let fingerprint = FileFingerprint::read(&path).map_err(|error| {
            BackendError::path_invalid(format!("orientation fingerprint failed: {error}"))
        })?;
        let cached = if force_rescan {
            None
        } else {
            run.as_ref()
                .map(|run| run.lookup::<CachedPdfOrientation>(&relative, &fingerprint))
                .transpose()
                .map_err(|error| {
                    BackendError::request_failed(format!("scan index read failed: {error}"))
                })?
                .flatten()
        };
        if let Some(cached) = cached {
            stats.reused += 1;
            if let Some(reason) = cached.unsupported {
                plan.warnings.push(warning(
                    "warn.rfi.orientationUnsupported",
                    &format!("{} — {reason}", relative.display()),
                ));
                continue;
            }
            if cached.page_count == 0 || cached.pages.len() != cached.page_count {
                return Err(BackendError::request_failed(
                    "cached orientation result is structurally invalid",
                ));
            }
            let mut cached_results = Vec::with_capacity(cached.page_count);
            for (page_index, page) in cached.pages.into_iter().enumerate() {
                let result = match (page.detection, page.error) {
                    (Some(detection), None) => Ok(detection),
                    (None, Some(error)) => Err(error),
                    _ => {
                        return Err(BackendError::request_failed(
                            "cached orientation page is structurally invalid",
                        ));
                    }
                };
                cached_results.push((
                    PageTask {
                        path: path.clone(),
                        page_index,
                        current_rotation: normalize_rotation(page.current_rotation)?,
                    },
                    result,
                ));
            }
            by_file.insert(path.clone(), cached_results);
            work.push(PdfWork {
                path,
                relative,
                page_count: cached.page_count,
                source_hash: cached.source_sha256,
                cached: true,
            });
            continue;
        }
        match inspect(&path) {
            Ok(facts) if facts.encrypted => record_unsupported(
                run.as_ref(),
                &relative,
                &fingerprint,
                "encrypted PDF",
                &mut stats,
                &mut plan,
            )?,
            Ok(facts) if facts.signed => record_unsupported(
                run.as_ref(),
                &relative,
                &fingerprint,
                "digitally signed PDF",
                &mut stats,
                &mut plan,
            )?,
            Ok(facts) if facts.page_count == 0 => record_unsupported(
                run.as_ref(),
                &relative,
                &fingerprint,
                "PDF has no pages",
                &mut stats,
                &mut plan,
            )?,
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
                initial_fingerprints.insert(path.clone(), fingerprint);
                work.push(PdfWork {
                    path,
                    relative,
                    page_count: facts.page_count,
                    source_hash: None,
                    cached: false,
                });
            }
            Err(error) => {
                if let Some(run) = &run {
                    run.record_failure(&relative, &error.to_string())
                        .map_err(|failure| {
                            BackendError::request_failed(format!(
                                "scan checkpoint failed: {failure}"
                            ))
                        })?;
                }
                stats.failed += 1;
                plan.warnings.push(warning(
                    "warn.rfi.orientationUnsupported",
                    &format!("{} — {error}", relative.display()),
                ));
            }
        }
    }

    let cached_pages = by_file.values().map(Vec::len).sum::<usize>();
    let total_pages = cached_pages.saturating_add(tasks.len()) as u64;
    for (index, (task, _)) in by_file.values().flatten().enumerate() {
        progress(ProgressEvent {
            phase: "preview".to_string(),
            done: (index + 1) as u64,
            total: total_pages,
            subject: format!("{} · {}", task.path.display(), task.page_index + 1),
        });
    }
    let mut completed_pages = cached_pages as u64;
    let task_rotations = tasks
        .iter()
        .map(|task| ((task.path.clone(), task.page_index), task.current_rotation))
        .collect::<BTreeMap<_, _>>();
    let mut remaining = work
        .iter()
        .filter(|item| !item.cached)
        .map(|item| (item.path.clone(), item.page_count))
        .collect::<BTreeMap<_, _>>();
    let mut checkpoint_pages = work
        .iter()
        .filter(|item| !item.cached)
        .map(|item| {
            (
                item.path.clone(),
                std::iter::repeat_with(|| None)
                    .take(item.page_count)
                    .collect::<Vec<Option<Result<OrientationDetection, String>>>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut source_hashes = BTreeMap::<PathBuf, Option<String>>::new();
    let (worker_count, results) = bounded_map_stream(
        &tasks,
        |task| detector.detect_page(&task.path, task.page_index),
        |index, result| {
            completed_pages += 1;
            let task = &tasks[index];
            progress(ProgressEvent {
                phase: "preview".to_string(),
                done: completed_pages,
                total: total_pages,
                subject: format!("{} · {}", task.path.display(), task.page_index + 1),
            });
            let pages = checkpoint_pages
                .get_mut(&task.path)
                .ok_or_else(|| "orientation checkpoint has no file bucket".to_string())?;
            pages[task.page_index] = Some(result.clone());
            let remaining_pages = remaining
                .get_mut(&task.path)
                .ok_or_else(|| "orientation checkpoint has no remaining count".to_string())?;
            *remaining_pages = remaining_pages.saturating_sub(1);
            if *remaining_pages == 0 {
                let initial = initial_fingerprints.get(&task.path).ok_or_else(|| {
                    "orientation checkpoint has no initial fingerprint".to_string()
                })?;
                let current = FileFingerprint::read(&task.path)?;
                if &current != initial {
                    if let Some(run) = &run {
                        let relative = task.path.strip_prefix(&source_root).map_err(|_| {
                            "orientation checkpoint path escaped its root".to_string()
                        })?;
                        run.record_failure(relative, "source changed during orientation scan")?;
                    }
                    stats.failed += 1;
                    return Err("source PDF changed during orientation scan".to_string());
                }
                let cached_pages = pages
                    .iter()
                    .enumerate()
                    .map(|(page_index, result)| {
                        let result = result.as_ref().ok_or_else(|| {
                            "orientation checkpoint page is incomplete".to_string()
                        })?;
                        let current_rotation = task_rotations
                            .get(&(task.path.clone(), page_index))
                            .copied()
                            .ok_or_else(|| {
                                "orientation checkpoint page metadata is missing".to_string()
                            })?;
                        Ok(match result {
                            Ok(detection) => CachedPageOrientation {
                                current_rotation,
                                detection: Some(detection.clone()),
                                error: None,
                            },
                            Err(error) => CachedPageOrientation {
                                current_rotation,
                                detection: None,
                                error: Some(error.clone()),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let has_detection_error = cached_pages.iter().any(|page| page.error.is_some());
                let actionable = !has_detection_error
                    && cached_pages.iter().all(|page| {
                        page.detection.as_ref().is_some_and(|detection| {
                            detection.confidence.is_finite()
                                && detection.confidence >= MIN_ORIENTATION_CONFIDENCE
                                && matches!(detection.rotate_clockwise, 0 | 90 | 180 | 270)
                        })
                    })
                    && cached_pages.iter().any(|page| {
                        page.detection
                            .as_ref()
                            .is_some_and(|detection| detection.rotate_clockwise != 0)
                    });
                let source_hash = actionable
                    .then(|| {
                        file_sha256(&task.path)
                            .map_err(|error| format!("source PDF could not be hashed: {error}"))
                    })
                    .transpose()?;
                if let Some(run) = &run {
                    let relative = task
                        .path
                        .strip_prefix(&source_root)
                        .map_err(|_| "orientation checkpoint path escaped its root".to_string())?;
                    if has_detection_error {
                        run.record_failure(relative, "one or more page detectors failed")?;
                    } else {
                        run.record_success(
                            relative,
                            initial,
                            &CachedPdfOrientation {
                                page_count: pages.len(),
                                source_sha256: source_hash.clone(),
                                pages: cached_pages,
                                unsupported: None,
                            },
                        )?;
                    }
                }
                source_hashes.insert(task.path.clone(), source_hash);
                if has_detection_error {
                    stats.failed += 1;
                } else {
                    stats.analyzed += 1;
                }
            }
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;

    for (task, result) in tasks.iter().cloned().zip(results) {
        by_file
            .entry(task.path.clone())
            .or_default()
            .push((task, result));
    }
    for item in work.iter_mut().filter(|item| !item.cached) {
        item.source_hash = source_hashes.remove(&item.path).flatten();
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
        let source_hash = item.source_hash.ok_or_else(|| {
            BackendError::request_failed("orientation scan did not pin a source hash")
        })?;
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
        (
            "scanIndex".to_string(),
            serde_json::to_value(&stats)
                .map_err(|error| BackendError::protocol(error.to_string()))?,
        ),
    ]));
    if let Some(run) = run {
        run.complete(&stats).map_err(|error| {
            BackendError::request_failed(format!("scan run completion failed: {error}"))
        })?;
    }
    Ok(plan)
}

fn record_unsupported(
    run: Option<&crate::scan_index::ScanRun<'_>>,
    relative: &Path,
    fingerprint: &FileFingerprint,
    reason: &str,
    stats: &mut ScanIndexStats,
    plan: &mut Plan,
) -> Result<(), BackendError> {
    if let Some(run) = run {
        run.record_success(
            relative,
            fingerprint,
            &CachedPdfOrientation {
                page_count: 0,
                source_sha256: None,
                pages: Vec::new(),
                unsupported: Some(reason.to_string()),
            },
        )
        .map_err(|error| {
            BackendError::request_failed(format!("scan checkpoint failed: {error}"))
        })?;
    }
    stats.analyzed += 1;
    plan.warnings.push(warning(
        "warn.rfi.orientationUnsupported",
        &format!("{} — {reason}", relative.display()),
    ));
    Ok(())
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
