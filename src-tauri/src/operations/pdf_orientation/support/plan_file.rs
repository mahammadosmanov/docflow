use super::*;

/// Running totals for the orientation plan's metadata.
#[derive(Default)]
pub(in crate::operations::pdf_orientation) struct PlanTotals {
    pub(in crate::operations::pdf_orientation) verified_pages: u64,
    pub(in crate::operations::pdf_orientation) rotated_pages: u64,
    pub(in crate::operations::pdf_orientation) actionable_files: u64,
    pub(in crate::operations::pdf_orientation) manual_review_files: u64,
}

/// Adds one PDF's approved rotation steps or review warning to the plan.
pub(in crate::operations::pdf_orientation) fn plan_file(
    item: &PdfWork,
    page_results: Vec<PageResult>,
    source_root: &Path,
    review_root: &Path,
    created_directories: &mut BTreeSet<PathBuf>,
    plan: &mut Plan,
    totals: &mut PlanTotals,
) -> Result<(), BackendError> {
    if item.cached {
        let expected = item.source_hash.as_deref().ok_or_else(|| {
            BackendError::request_failed("cached orientation result has no source hash")
        })?;
        let current = file_sha256(&item.path)
            .map_err(|error| BackendError::path_invalid(error.to_string()))?;
        if current != expected {
            return Err(BackendError::path_invalid(
                "source PDF content changed during orientation scan",
            ));
        }
    }
    let (approved, uncertain, verified) = approve_file_pages(page_results);
    totals.verified_pages += verified;
    if !uncertain.is_empty() {
        totals.manual_review_files += 1;
        plan.warnings.push(warning(
            "warn.rfi.orientationNeedsReview",
            &truncate(
                &format!("{} — {}", item.relative.display(), uncertain.join(", ")),
                360,
            ),
        ));
        return Ok(());
    }
    if approved.is_empty() {
        return Ok(());
    }
    let destination = review_root.join(&item.relative);
    if destination.exists() {
        plan.warnings.push(warning(
            "warn.rfi.orientationReviewExists",
            &item.relative.to_string_lossy(),
        ));
        return Ok(());
    }
    add_directory_steps(
        destination
            .parent()
            .ok_or_else(|| BackendError::path_invalid("review PDF has no parent"))?,
        review_root,
        created_directories,
        &mut plan.steps,
    )?;
    let source_hash = item.source_hash.clone().ok_or_else(|| {
        BackendError::request_failed("orientation scan did not pin a source hash")
    })?;
    totals.rotated_pages += approved.len() as u64;
    totals.actionable_files += 1;
    plan.steps.push(rotation_step(
        item,
        &destination,
        source_root,
        review_root,
        source_hash,
        approved,
    ));
    Ok(())
}
