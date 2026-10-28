use super::*;
use crate::scan_index::ScanRun;

pub(super) fn empty_plan() -> Plan {
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

pub(super) fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BackendError> {
    let path = fs::canonicalize(path)
        .map_err(|_| BackendError::path_invalid(format!("{label} does not exist")))?;
    if !path.is_dir() {
        return Err(BackendError::path_invalid(format!(
            "{label} is not a directory"
        )));
    }
    Ok(path)
}

pub(super) fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

pub(super) fn normalize_rotation(rotation: i64) -> Result<i64, BackendError> {
    let normalized = rotation.rem_euclid(360);
    if matches!(normalized, 0 | 90 | 180 | 270) {
        Ok(normalized)
    } else {
        Err(BackendError::validation(
            "PDF page rotation is not a right angle",
        ))
    }
}

pub(super) fn warning(key: &str, subject: &str) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.to_string(),
    }
}

pub(super) fn truncate(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push('…');
    }
    result
}

/// Splits one file's page results into approved rotations, uncertain-page reasons,
/// and the count of pages whose target rotation verified.
pub(super) fn approve_file_pages(page_results: Vec<PageResult>) -> (Vec<Value>, Vec<String>, u64) {
    let mut approved = Vec::new();
    let mut uncertain = Vec::new();
    let mut verified = 0_u64;
    for (task, result) in page_results {
        match result {
            Ok(detection) => match orientation_target_rotation(task.current_rotation, &detection) {
                Ok(target) => {
                    verified += 1;
                    if let Some(target) = target {
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
                Err(error) => uncertain.push(format!("page {} {error}", task.page_index + 1)),
            },
            Err(error) => uncertain.push(format!("page {} {error}", task.page_index + 1)),
        }
    }
    (approved, uncertain, verified)
}

/// One page's orientation outcome, keyed by the page task it was computed for.
pub(super) type PageResult = (PageTask, Result<OrientationDetection, String>);

/// Rebuilds per-page orientation results from a reused scan-index entry.
pub(super) fn cached_page_results(
    path: &Path,
    pages: Vec<CachedPageOrientation>,
) -> Result<Vec<PageResult>, BackendError> {
    let mut cached_results = Vec::with_capacity(pages.len());
    for (page_index, page) in pages.into_iter().enumerate() {
        let result = match (page.detection, page.error) {
            (Some(detection), None) => Ok(detection),
            (None, Some(error)) => Err(error),
            _ => {
                return Err(BackendError::request_failed(
                    "cached orientation page is structurally invalid",
                ));
            }
        };
        let current_rotation = page.current_rotation;
        let (current_rotation, result) = match normalize_rotation(current_rotation) {
            Ok(rotation) => (rotation, result),
            Err(error) => (current_rotation, Err(error.message)),
        };
        cached_results.push((
            PageTask {
                path: path.to_path_buf(),
                page_index,
                current_rotation,
            },
            result,
        ));
    }
    Ok(cached_results)
}

/// Builds the approved PDF page-rotation step for one source file.
pub(super) fn rotation_step(
    item: &PdfWork,
    destination: &Path,
    source_root: &Path,
    review_root: &Path,
    source_hash: String,
    approved: Vec<Value>,
) -> Step {
    let (source, relative, page_count) = (&item.path, &item.relative, item.page_count);
    Step {
        kind: StepKind::PdfPageRotate,
        src: Some(source.to_string_lossy().into_owned()),
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
                Value::String(relative.to_string_lossy().into_owned()),
            ),
            ("pageCount".to_string(), Value::from(page_count as u64)),
            ("rotations".to_string(), Value::Array(approved)),
        ]),
    }
}

/// Reads fingerprint and content hash, then reuses a checkpoint or inspects one PDF.
pub(super) fn prepare_pdf(
    path: &Path,
    source_root: &Path,
    force_rescan: bool,
    run: Option<&crate::scan_index::ScanRun<'_>>,
) -> Result<PreparedPdf, BackendError> {
    let relative = path
        .strip_prefix(source_root)
        .map_err(|_| BackendError::path_invalid("PDF escaped the selected RFI root"))?
        .to_path_buf();
    let fingerprint = FileFingerprint::read(path).map_err(|error| {
        BackendError::path_invalid(format!("orientation fingerprint failed: {error}"))
    })?;
    let source_sha256 = file_sha256(path).map_err(|error| {
        BackendError::path_invalid(format!("orientation source hash failed: {error}"))
    })?;
    let cached = if force_rescan {
        None
    } else {
        run.map(|run| run.lookup::<CachedPdfOrientation>(&relative, &fingerprint))
            .transpose()
            .map_err(|error| {
                BackendError::request_failed(format!("scan index read failed: {error}"))
            })?
            .flatten()
    };
    let cached =
        cached.filter(|cached| cached.source_sha256.as_deref() == Some(source_sha256.as_str()));
    let inspection = cached.is_none().then(|| inspect(path));
    Ok::<_, BackendError>(PreparedPdf {
        path: path.to_path_buf(),
        relative,
        fingerprint,
        source_sha256,
        cached,
        inspection,
    })
}

/// Immutable per-scan inputs the checkpoint closure reads for each finished file.
pub(super) struct CheckpointInputs<'a> {
    pub(super) source_root: &'a Path,
    pub(super) initial_fingerprints: &'a BTreeMap<PathBuf, FileFingerprint>,
    pub(super) initial_hashes: &'a BTreeMap<PathBuf, String>,
    pub(super) task_rotations: &'a BTreeMap<(PathBuf, usize), i64>,
}

/// Verifies a fully analyzed PDF is unchanged, checkpoints it, and returns its pinned hash and
/// whether any page detector failed.
pub(super) fn finish_checkpointed_file(
    inputs: &CheckpointInputs,
    run: Option<&ScanRun<'_>>,
    path: &Path,
    pages: &[Option<Result<OrientationDetection, String>>],
    stats: &mut ScanIndexStats,
) -> Result<(Option<String>, bool), String> {
    let initial = inputs
        .initial_fingerprints
        .get(path)
        .ok_or_else(|| "orientation checkpoint has no initial fingerprint".to_string())?;
    let current = FileFingerprint::read(path)?;
    if &current != initial {
        if let Some(run) = run {
            let relative = path
                .strip_prefix(inputs.source_root)
                .map_err(|_| "orientation checkpoint path escaped its root".to_string())?;
            run.record_failure(relative, "source changed during orientation scan")?;
        }
        stats.failed += 1;
        return Err("source PDF changed during orientation scan".to_string());
    }
    let initial_hash = inputs
        .initial_hashes
        .get(path)
        .ok_or_else(|| "orientation checkpoint has no initial content hash".to_string())?;
    let current_hash =
        file_sha256(path).map_err(|error| format!("source PDF could not be hashed: {error}"))?;
    if &current_hash != initial_hash {
        if let Some(run) = run {
            let relative = path
                .strip_prefix(inputs.source_root)
                .map_err(|_| "orientation checkpoint path escaped its root".to_string())?;
            run.record_failure(relative, "source content changed during orientation scan")?;
        }
        stats.failed += 1;
        return Err("source PDF content changed during orientation scan".to_string());
    }
    let cached_pages = pages
        .iter()
        .enumerate()
        .map(|(page_index, result)| {
            let result = result
                .as_ref()
                .ok_or_else(|| "orientation checkpoint page is incomplete".to_string())?;
            let current_rotation = inputs
                .task_rotations
                .get(&(path.to_path_buf(), page_index))
                .copied()
                .ok_or_else(|| "orientation checkpoint page metadata is missing".to_string())?;
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
                orientation_target_rotation(page.current_rotation, detection).is_ok()
            })
        })
        && cached_pages.iter().any(|page| {
            page.detection.as_ref().is_some_and(|detection| {
                matches!(
                    orientation_target_rotation(page.current_rotation, detection),
                    Ok(Some(_))
                )
            })
        });
    let source_hash = actionable.then(|| initial_hash.to_owned());
    if let Some(run) = run {
        let relative = path
            .strip_prefix(inputs.source_root)
            .map_err(|_| "orientation checkpoint path escaped its root".to_string())?;
        if has_detection_error {
            run.record_failure(relative, "one or more page detectors failed")?;
        } else {
            run.record_success(
                relative,
                initial,
                &CachedPdfOrientation {
                    page_count: pages.len(),
                    source_sha256: Some(initial_hash.to_owned()),
                    pages: cached_pages,
                    unsupported: None,
                },
            )?;
        }
    }
    Ok((source_hash, has_detection_error))
}
