use super::*;

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
