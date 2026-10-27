use super::*;

pub(super) fn destination_contains_exact_rfi_folder(destination: &Path) -> bool {
    exact_rfi_folder_in(destination)
        || DISCIPLINE_FOLDERS
            .iter()
            .any(|discipline| exact_rfi_folder_in(&destination.join(discipline)))
}

pub(super) fn exact_rfi_folder_in(directory: &Path) -> bool {
    fs::read_dir(directory).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            let Ok(file_type) = entry.file_type() else {
                return false;
            };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                return false;
            };
            let matched = crate::rfi_code::find(&name);
            file_type.is_dir() && matched.method == RfiMethod::Strict && matched.code == name
        })
    })
}

pub(super) fn flush_page_group(active: &mut Option<PageGroup>, groups: &mut Vec<PageGroup>) {
    if let Some(group) = active.take() {
        groups.push(group);
    }
}

pub(super) fn is_confirmed_cover(read: &OcrRead) -> bool {
    let canonical = crate::rfi_code::find(&read.matched.code);
    matches!(read.matched.method, RfiMethod::Strict | RfiMethod::OcrAlias)
        && read.matched.confidence >= MIN_CONFIDENCE
        && canonical.code == read.matched.code
        && canonical.method == RfiMethod::Strict
}

pub(super) fn process_scan_targets(
    destination: &Path,
    code: &str,
    organize: bool,
) -> (Option<PathBuf>, PathBuf) {
    let discipline = crate::rfi_code::find(code).discipline;
    let discipline_root = DISCIPLINE_FOLDERS
        .contains(&discipline.as_str())
        .then(|| destination.join(&discipline));
    let root = discipline_root.as_deref().unwrap_or(destination);
    if organize {
        let folder = folder_for(root, code);
        (Some(folder), target_for(root, code))
    } else {
        let target = flat_target_for(root, code);
        (discipline_root, target)
    }
}

pub(super) fn review_target(destination: &Path, source: &Path, pages: &[usize]) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("scan");
    let first = pages.first().copied().unwrap_or(0) + 1;
    let last = pages.last().copied().unwrap_or(first - 1) + 1;
    destination
        .join(REVIEW_DIR)
        .join(format!("{stem} - pages {first:04}-{last:04}.pdf"))
}

pub(super) fn page_range(pages: &[usize]) -> String {
    match (pages.first(), pages.last()) {
        (Some(first), Some(last)) => format!("pages {}-{}", first + 1, last + 1),
        _ => "pages unavailable".to_string(),
    }
}

pub(super) fn push_candidate_output(
    plan: &mut Plan,
    candidate: &Candidate,
    destination: &Path,
    note_key: &str,
    mut meta: JsonObject,
    keep_original: bool,
) {
    if let Some(pages) = candidate.pages.as_deref() {
        meta.insert(
            "pageIndexes".to_string(),
            Value::Array(pages.iter().map(|page| Value::from(*page as u64)).collect()),
        );
        meta.insert(
            "sourceSha256".to_string(),
            Value::String(
                candidate
                    .source_hash
                    .as_deref()
                    .expect("split page candidate has a Preview source hash")
                    .to_string(),
            ),
        );
        if !candidate.rotations.is_empty() {
            let rotations = pages
                .iter()
                .enumerate()
                .filter_map(|(output_page, source_page)| {
                    candidate
                        .rotations
                        .iter()
                        .find(|(page, _, _)| page == source_page)
                        .map(|(_, target, confidence)| {
                            serde_json::json!({
                                "pageIndex": output_page,
                                "targetRotation": target,
                                "confidence": confidence,
                            })
                        })
                })
                .collect::<Vec<_>>();
            meta.insert("rotations".to_string(), Value::Array(rotations));
        }
        plan.steps.push(step(
            StepKind::PdfPagesExtract,
            Some(&candidate.path),
            Some(destination),
            note_key,
            meta,
        ));
    } else {
        plan.steps.push(step(
            if keep_original {
                StepKind::Copy
            } else {
                StepKind::Move
            },
            Some(&candidate.path),
            Some(destination),
            note_key,
            meta,
        ));
    }
}

pub(super) fn hash_pdf(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > crate::pdf::MAX_PDF_BYTES {
        return Err("source PDF is not a bounded regular file".to_string());
    }
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BackendError> {
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

pub(super) fn empty_plan(operation_id: &str) -> Plan {
    Plan {
        plan_id: String::new(),
        op_id: operation_id.to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
        meta: JsonObject::new(),
    }
}

pub(super) fn step(
    kind: StepKind,
    source: Option<&Path>,
    destination: Option<&Path>,
    note_key: &str,
    meta: JsonObject,
) -> Step {
    Step {
        kind,
        src: source.map(|path| path.to_string_lossy().into_owned()),
        dst: destination.map(|path| path.to_string_lossy().into_owned()),
        note_key: Some(note_key.to_string()),
        meta,
    }
}

pub(super) fn method_name(method: RfiMethod) -> &'static str {
    match method {
        RfiMethod::Strict => "strict",
        RfiMethod::Repaired => "repaired",
        RfiMethod::OcrAlias => "ocr_alias",
        RfiMethod::Fuzzy => "fuzzy",
        RfiMethod::Miss => "miss",
    }
}

pub(super) fn same_file(left: &Path, right: &Path) -> bool {
    if !right.exists() {
        return false;
    }
    fs::canonicalize(left)
        .and_then(|left| fs::canonicalize(right).map(|right| left == right))
        .unwrap_or(false)
}
