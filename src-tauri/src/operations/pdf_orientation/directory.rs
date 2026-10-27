use super::*;

pub(super) fn record_unsupported(
    run: Option<&crate::scan_index::ScanRun<'_>>,
    relative: &Path,
    fingerprint: &FileFingerprint,
    source_sha256: &str,
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
                source_sha256: Some(source_sha256.to_string()),
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

pub(super) fn collect_discipline_pdfs<P, C>(
    root: &Path,
    progress: &mut P,
    cancelled: &mut C,
) -> Result<(Vec<PathBuf>, usize), BackendError>
where
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let root_is_discipline = root
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_uppercase())
        .is_some_and(|name| DISCIPLINES.contains(&name.as_str()));
    let mut directories = vec![(root.to_path_buf(), root_is_discipline)];
    let mut pdfs = BTreeSet::new();
    let mut visited = 0_usize;
    let mut scanned_directories = 0_u64;
    let mut worker_count = 1_usize;
    while !directories.is_empty() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        directories.sort();
        let current_count = directories.len();
        let (workers, batches) = bounded_map(&directories, |(directory, in_discipline)| {
            scan_orientation_directory(directory, *in_discipline)
        })
        .map_err(BackendError::request_failed)?;
        worker_count = worker_count.max(workers);
        let mut next = Vec::new();
        for batch in batches {
            let batch = batch?;
            visited = visited
                .checked_add(batch.entries)
                .ok_or_else(|| BackendError::validation("RFI system entry count overflow"))?;
            if visited > MAX_DIRECTORY_ENTRIES {
                return Err(BackendError::validation(
                    "RFI system is too large to scan safely",
                ));
            }
            pdfs.extend(batch.pdfs);
            next.extend(batch.directories);
        }
        if pdfs.len() > MAX_PDFS {
            return Err(BackendError::validation(format!(
                "orientation scan exceeds the {MAX_PDFS}-PDF limit"
            )));
        }
        scanned_directories += current_count as u64;
        progress(ProgressEvent {
            phase: "orientation_discovery".to_string(),
            done: scanned_directories,
            total: scanned_directories.saturating_add(next.len() as u64),
            subject: format!("{} PDF", pdfs.len()),
        });
        directories = next;
    }
    Ok((pdfs.into_iter().collect(), worker_count))
}

pub(super) struct OrientationDirectoryBatch {
    pub(super) directories: Vec<(PathBuf, bool)>,
    pub(super) pdfs: Vec<PathBuf>,
    pub(super) entries: usize,
}

pub(super) fn scan_orientation_directory(
    directory: &Path,
    in_discipline: bool,
) -> Result<OrientationDirectoryBatch, BackendError> {
    let mut batch = OrientationDirectoryBatch {
        directories: Vec::new(),
        pdfs: Vec::new(),
        entries: 0,
    };
    for entry in fs::read_dir(directory)
        .map_err(|_| BackendError::path_invalid("RFI system folder could not be read"))?
    {
        let entry = entry.map_err(|_| BackendError::path_invalid("RFI folder entry failed"))?;
        batch.entries += 1;
        let file_type = entry
            .file_type()
            .map_err(|_| BackendError::path_invalid("RFI folder metadata failed"))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let child_is_discipline = in_discipline
                || DISCIPLINES.contains(
                    &entry
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_uppercase()
                        .as_str(),
                );
            batch.directories.push((entry.path(), child_is_discipline));
        } else if in_discipline
            && file_type.is_file()
            && entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
        {
            batch.pdfs.push(entry.path());
        }
    }
    Ok(batch)
}

pub(super) fn add_directory_steps(
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
