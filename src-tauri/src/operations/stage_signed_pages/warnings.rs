use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn push_warning_review(
    plan: &mut Plan,
    reviews: &mut BTreeMap<PathBuf, Vec<WarningReviewReason>>,
    signed_root: &Path,
    key: &str,
    source: &Path,
    page_index: Option<usize>,
    code: Option<&str>,
    detail: &str,
) {
    record_warning_review(reviews, source, page_index, code, key, detail);
    let relative = source.strip_prefix(signed_root).unwrap_or(source);
    let mut context = vec![relative.to_string_lossy().into_owned()];
    if let Some(page_index) = page_index {
        context.push(format!("page {} (#{})", page_index + 1, page_index + 1));
    }
    if let Some(code) = code {
        context.push(code.to_string());
    }
    if !detail.is_empty() {
        context.push(detail.to_string());
    }
    plan.warnings
        .push(warning(key, &truncate(&context.join(" · "), 500)));
}

pub(super) fn record_warning_review(
    reviews: &mut BTreeMap<PathBuf, Vec<WarningReviewReason>>,
    source: &Path,
    page_index: Option<usize>,
    code: Option<&str>,
    key: &str,
    detail: &str,
) {
    let reason = WarningReviewReason {
        key: key.to_string(),
        page_number: page_index.map(|index| index + 1),
        code: code.map(str::to_string),
        detail: truncate(detail, 300),
    };
    let reasons = reviews.entry(source.to_path_buf()).or_default();
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

pub(super) fn add_warning_review_steps(
    plan: &mut Plan,
    reviews: &BTreeMap<PathBuf, Vec<WarningReviewReason>>,
    signed_root: &Path,
    review_root: &Path,
) -> Result<(), BackendError> {
    let warning_root = review_root.join("_review");
    let mut items = Vec::new();
    let mut planned_directories = BTreeSet::new();
    for (source, reasons) in reviews {
        let relative = source.strip_prefix(signed_root).map_err(|_| {
            BackendError::path_invalid("warning review source escaped signed scans")
        })?;
        let destination = warning_root.join(relative);
        let reason_values = reasons
            .iter()
            .map(|reason| {
                json!({
                    "key": reason.key,
                    "pageNumber": reason.page_number,
                    "code": reason.code,
                    "detail": reason.detail,
                })
            })
            .collect::<Vec<_>>();
        let mut status = "planned";
        if destination.exists() {
            status = "exists";
            plan.warnings.push(warning(
                "warn.rfi.warningReviewExists",
                &relative.to_string_lossy(),
            ));
        } else if review_destination_blocked(&destination, review_root) {
            status = "blocked";
            plan.warnings.push(warning(
                "warn.rfi.warningReviewBlocked",
                &relative.to_string_lossy(),
            ));
        } else {
            let mut review_steps = Vec::new();
            add_directory_steps(
                destination.parent().ok_or_else(|| {
                    BackendError::path_invalid("warning review destination has no parent")
                })?,
                review_root,
                &mut planned_directories,
                &mut review_steps,
            )?;
            for step in &mut review_steps {
                step.meta
                    .insert("warningReview".to_string(), Value::Bool(true));
            }
            extend_candidate_steps(&mut plan.steps, &review_steps);
            let source_hash = file_sha256(source).map_err(|_| {
                BackendError::path_invalid("warning review PDF could not be hashed")
            })?;
            plan.steps.push(Step {
                kind: StepKind::Copy,
                src: Some(source.to_string_lossy().into_owned()),
                dst: Some(destination.to_string_lossy().into_owned()),
                note_key: Some("note.rfi.signedWarningReviewCopy".to_string()),
                meta: JsonObject::from_iter([
                    ("warningReview".to_string(), Value::Bool(true)),
                    (
                        "expectedSourceSha256".to_string(),
                        Value::String(source_hash),
                    ),
                    ("reasons".to_string(), Value::Array(reason_values.clone())),
                ]),
            });
        }
        items.push(json!({
            "sourceFile": relative.to_string_lossy(),
            "destination": destination.strip_prefix(review_root).unwrap_or(&destination).to_string_lossy(),
            "status": status,
            "reasons": reason_values,
        }));
    }
    plan.meta
        .insert("warningReviewItems".to_string(), Value::Array(items));
    Ok(())
}

pub(super) fn review_destination_blocked(destination: &Path, review_root: &Path) -> bool {
    destination
        .ancestors()
        .skip(1)
        .take_while(|path| *path != review_root)
        .any(|path| match fs::symlink_metadata(path) {
            Ok(metadata) => !metadata.is_dir() || metadata.file_type().is_symlink(),
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        })
}
