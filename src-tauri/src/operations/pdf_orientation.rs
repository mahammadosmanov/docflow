use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    fs_ops::file_sha256,
    parallel::{bounded_map, bounded_map_stream, worker_count},
    pdf::{PdfError, PdfInspection, inspect},
    scan_index::{FileFingerprint, ScanIndex, ScanIndexStats},
};

mod directory;
use directory::*;
mod uncached;
use uncached::*;
mod support;
use support::*;

pub const OPERATION_ID: &str = "rfi.normalize_pdf_orientation";
const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
pub const MIN_ORIENTATION_CONFIDENCE: f64 = 10.0;
const MAX_DIRECTORY_ENTRIES: usize = 500_000;
const MAX_PDFS: usize = 50_000;
const MAX_PAGES: usize = 250_000;
const ORIENTATION_SCAN_PROFILE: &str = "content-orientation-cascade-v5";

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
/// Converts an OCR correction relative to the rendered page into an absolute PDF rotation.
/// Returns `None` when the rendered page is already upright.
pub fn orientation_target_rotation(
    current_rotation: i64,
    detection: &OrientationDetection,
) -> Result<Option<i64>, String> {
    let current = normalize_rotation(current_rotation).map_err(|error| error.message)?;
    if !detection.confidence.is_finite() || detection.confidence < MIN_ORIENTATION_CONFIDENCE {
        return Err(format!(
            "orientation is uncertain (confidence {:.1})",
            detection.confidence
        ));
    }
    if !matches!(detection.rotate_clockwise, 0 | 90 | 180 | 270) {
        return Err("orientation correction is not a right angle".to_string());
    }
    let target = (current + detection.rotate_clockwise).rem_euclid(360);
    Ok((target != current).then_some(target))
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

struct PreparedPdf {
    path: PathBuf,
    relative: PathBuf,
    fingerprint: FileFingerprint,
    source_sha256: String,
    cached: Option<CachedPdfOrientation>,
    inspection: Option<Result<PdfInspection, PdfError>>,
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

/// Plan metadata for the orientation preview: worker counts, page totals, and scan-index stats.
fn orientation_meta<D: OrientationDetector>(
    detector: &D,
    stats: &ScanIndexStats,
    totals: &PlanTotals,
    worker_count: usize,
    inspection_workers: usize,
) -> Result<JsonObject, BackendError> {
    Ok(JsonObject::from_iter([
        ("workerCount".to_string(), Value::from(worker_count as u64)),
        (
            "inspectionWorkers".to_string(),
            Value::from(inspection_workers as u64),
        ),
        (
            "verifiedPages".to_string(),
            Value::from(totals.verified_pages),
        ),
        (
            "rotatedPages".to_string(),
            Value::from(totals.rotated_pages),
        ),
        (
            "actionableFiles".to_string(),
            Value::from(totals.actionable_files),
        ),
        (
            "manualReviewFiles".to_string(),
            Value::from(totals.manual_review_files),
        ),
        (
            "detectorDiagnostics".to_string(),
            Value::String(detector.diagnostics()),
        ),
        (
            "scanIndex".to_string(),
            serde_json::to_value(stats)
                .map_err(|error| BackendError::protocol(error.to_string()))?,
        ),
    ]))
}

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
    progress(ProgressEvent {
        phase: "orientation_discovery".to_string(),
        done: 0,
        total: 0,
        subject: source_root.to_string_lossy().into_owned(),
    });
    let (pdfs, discovery_workers) =
        collect_discipline_pdfs(&source_root, &mut progress, &mut cancelled)?;
    progress(ProgressEvent {
        phase: "orientation_discovery".to_string(),
        done: pdfs.len() as u64,
        total: pdfs.len() as u64,
        subject: String::new(),
    });
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
    let mut acc = InspectionAccumulator::default();
    let mut by_file =
        BTreeMap::<PathBuf, Vec<(PageTask, Result<OrientationDetection, String>)>>::new();
    let total_pdfs = pdfs.len() as u64;
    let mut inspected_pdfs = 0_u64;
    progress(ProgressEvent {
        phase: "orientation_inspection".to_string(),
        done: 0,
        total: total_pdfs,
        subject: String::new(),
    });
    let inspection_workers = worker_count(pdfs.len()).min(2);
    let inspection_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(inspection_workers)
        .thread_name(|index| format!("docflow-pdf-inspection-{index}"))
        .build()
        .map_err(|error| {
            BackendError::request_failed(format!("PDF inspection workers could not start: {error}"))
        })?;
    // Keep at most two large parsed PDFs in flight; page OCR uses its own pool.
    for batch in pdfs.chunks(2) {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        let prepared = inspection_pool.install(|| {
            batch
                .par_iter()
                .map(|path| prepare_pdf(path, &source_root, force_rescan, run.as_ref()))
                .collect::<Vec<_>>()
        });
        for prepared in prepared {
            if cancelled() {
                return Err(BackendError::cancelled());
            }
            let PreparedPdf {
                path,
                relative,
                fingerprint,
                source_sha256,
                cached,
                inspection,
            } = prepared?;
            inspected_pdfs += 1;
            progress(ProgressEvent {
                phase: "orientation_inspection".to_string(),
                done: inspected_pdfs,
                total: total_pdfs,
                subject: relative.to_string_lossy().into_owned(),
            });
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
                let cached_results = cached_page_results(&path, cached.pages)?;
                by_file.insert(path.clone(), cached_results);
                acc.work.push(PdfWork {
                    path,
                    relative,
                    page_count: cached.page_count,
                    source_hash: Some(source_sha256),
                    cached: true,
                });
                continue;
            }
            let inspection = inspection.ok_or_else(|| {
                BackendError::request_failed("uncached PDF has no inspection result")
            })?;
            classify_uncached(
                run.as_ref(),
                UncachedPdf {
                    path,
                    relative,
                    fingerprint,
                    source_sha256,
                    inspection,
                },
                &mut acc,
                &mut stats,
                &mut plan,
            )?;
        }
    }

    let InspectionAccumulator {
        tasks,
        mut work,
        initial_hashes,
        initial_fingerprints,
    } = acc;
    let cached_pages = by_file.values().map(Vec::len).sum::<usize>();
    let total_pages = cached_pages.saturating_add(tasks.len()) as u64;
    for (index, (task, _)) in by_file.values().flatten().enumerate() {
        let done = index + 1;
        if done == 1 || done.is_multiple_of(128) || done == cached_pages {
            progress(ProgressEvent {
                phase: "orientation_analysis".to_string(),
                done: done as u64,
                total: total_pages,
                subject: format!("{} · {}", task.path.display(), task.page_index + 1),
            });
        }
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
    let checkpoint_inputs = CheckpointInputs {
        source_root: &source_root,
        initial_fingerprints: &initial_fingerprints,
        initial_hashes: &initial_hashes,
        task_rotations: &task_rotations,
    };
    let (worker_count, results) = bounded_map_stream(
        &tasks,
        |task| detector.detect_page(&task.path, task.page_index),
        |index, result| {
            completed_pages += 1;
            let task = &tasks[index];
            progress(ProgressEvent {
                phase: "orientation_analysis".to_string(),
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
                let (source_hash, has_detection_error) = finish_checkpointed_file(
                    &checkpoint_inputs,
                    run.as_ref(),
                    &task.path,
                    pages,
                    &mut stats,
                )?;
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
    let mut totals = PlanTotals::default();
    for item in work {
        let page_results = by_file.remove(&item.path).unwrap_or_default();
        plan_file(
            &item,
            page_results,
            &source_root,
            &review_root,
            &mut created_directories,
            &mut plan,
            &mut totals,
        )?;
    }
    stats.parallel_analysis = !tasks.is_empty();
    stats.analysis_workers = worker_count as u64;
    plan.meta.extend(orientation_meta(
        detector,
        &stats,
        &totals,
        worker_count.max(discovery_workers).max(inspection_workers),
        inspection_workers,
    )?);
    if let Some(run) = run {
        run.complete(&stats).map_err(|error| {
            BackendError::request_failed(format!("scan run completion failed: {error}"))
        })?;
    }
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(plan)
}
