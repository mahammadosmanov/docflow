use super::*;
use crate::scan_index::ScanRun;

/// Immutable per-scan inputs the checkpoint closure reads for each finished file.
pub(in crate::operations::pdf_orientation) struct CheckpointInputs<'a> {
    pub(in crate::operations::pdf_orientation) source_root: &'a Path,
    pub(in crate::operations::pdf_orientation) initial_fingerprints:
        &'a BTreeMap<PathBuf, FileFingerprint>,
    pub(in crate::operations::pdf_orientation) initial_hashes: &'a BTreeMap<PathBuf, String>,
    pub(in crate::operations::pdf_orientation) task_rotations: &'a BTreeMap<(PathBuf, usize), i64>,
}

/// Verifies a fully analyzed PDF is unchanged, checkpoints it, and returns its pinned hash and
/// whether any page detector failed.
pub(in crate::operations::pdf_orientation) fn finish_checkpointed_file(
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
