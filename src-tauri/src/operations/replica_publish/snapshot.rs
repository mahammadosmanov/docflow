use super::*;

pub(super) fn snapshot_tree(
    root: &Path,
    root_files: &BTreeSet<String>,
    require_disciplines: bool,
) -> Result<TreeSnapshot, BackendError> {
    let mut snapshot = TreeSnapshot::default();
    let mut pending = Vec::new();
    for discipline in DISCIPLINES {
        let path = root.join(discipline);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                let relative = PathBuf::from(discipline);
                snapshot.directories.insert(relative.clone());
                pending.push((path, relative));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !require_disciplines => {}
            _ => return Err(BackendError::validation("error.replica.disciplineMissing")),
        }
    }
    let mut entry_count = 0_usize;
    while let Some((directory, relative_directory)) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|_| BackendError::path_invalid("error.replica.scanIncomplete"))?;
        for entry in entries {
            let entry =
                entry.map_err(|_| BackendError::path_invalid("error.replica.scanIncomplete"))?;
            entry_count = entry_count.saturating_add(1);
            if entry_count > MAX_ENTRIES {
                return Err(BackendError::validation("error.replica.entryLimit"));
            }
            let kind = entry
                .file_type()
                .map_err(|_| BackendError::path_invalid("error.replica.scanIncomplete"))?;
            if kind.is_symlink() {
                return Err(BackendError::validation("error.replica.symlinkRefused"));
            }
            let relative = relative_directory.join(entry.file_name());
            if kind.is_dir() {
                snapshot.directories.insert(relative.clone());
                pending.push((entry.path(), relative));
            } else if kind.is_file() {
                insert_file(&mut snapshot.files, relative, entry.path())?;
            }
        }
    }
    for name in root_files {
        let path = root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                insert_file(&mut snapshot.files, PathBuf::from(name), path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !require_disciplines => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(BackendError::validation(
                    "error.replica.sourceRootFileMissing",
                ));
            }
            _ => return Err(BackendError::validation("error.replica.rootFileInvalid")),
        }
    }
    Ok(snapshot)
}

pub(super) fn insert_file(
    files: &mut BTreeMap<PathBuf, FileSnapshot>,
    relative: PathBuf,
    path: PathBuf,
) -> Result<(), BackendError> {
    let metadata = fs::metadata(&path)
        .map_err(|_| BackendError::path_invalid("error.replica.scanIncomplete"))?;
    let snapshot = FileSnapshot {
        path,
        hash: String::new(),
        len: metadata.len(),
        modified: metadata.modified().ok(),
    };
    if files.insert(relative, snapshot).is_some() {
        return Err(BackendError::validation("error.replica.pathCollision"));
    }
    Ok(())
}

pub(super) fn ensure_source_settled(
    snapshot: &TreeSnapshot,
    settle_seconds: u64,
) -> Result<(), BackendError> {
    let now = SystemTime::now();
    let settled = snapshot.files.values().all(|file| {
        file.modified
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age.as_secs() >= settle_seconds)
    });
    if settled {
        Ok(())
    } else {
        Err(BackendError::validation("error.replica.sourceSettling"))
    }
}

pub(super) fn enforce_automatic_change_limit(
    changed_file_count: u64,
    changed_bytes: u64,
    params: &ReplicaPublishParams,
) -> Result<(), BackendError> {
    if changed_file_count > params.max_changed_files
        || changed_bytes > params.max_changed_mib.saturating_mul(1024 * 1024)
    {
        Err(BackendError::validation(
            "error.replica.changeLimitExceeded",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn hash_snapshot<P>(
    root: &Path,
    snapshot: &mut TreeSnapshot,
    cache: Option<(&str, &ScanIndex)>,
    aggregate: &mut ScanIndexStats,
    progress: &mut P,
    phase: &str,
) -> Result<(), BackendError>
where
    P: FnMut(ProgressEvent),
{
    let items = snapshot
        .files
        .iter()
        .map(|(relative, file)| (relative.clone(), file.clone()))
        .collect::<Vec<_>>();
    let tree_total = items.len() as u64;
    aggregate.discovered = aggregate.discovered.saturating_add(items.len() as u64);
    let mut run = cache
        .map(|(owner_id, index)| index.begin_run(owner_id, OPERATION_ID, HASH_PROFILE, root, false))
        .transpose()
        .map_err(|error| BackendError::request_failed(format!("replica index: {error}")))?;
    if let Some((_, prior)) = &run {
        aggregate.prior_interrupted_runs = aggregate.prior_interrupted_runs.saturating_add(*prior);
    }
    let mut pending = Vec::new();
    let mut tree_reused = 0_u64;
    for (relative, file) in items {
        let fingerprint = FileFingerprint::read(&file.path).map_err(|error| {
            BackendError::validation(format!("error.replica.fileUnstable: {error}"))
        })?;
        let cached = run
            .as_ref()
            .map(|(run, _)| run.lookup::<CachedHash>(&relative, &fingerprint))
            .transpose()
            .map_err(|error| BackendError::request_failed(format!("replica index: {error}")))?
            .flatten()
            .filter(|cached| valid_sha256(&cached.sha256));
        if let Some(cached) = cached {
            snapshot
                .files
                .get_mut(&relative)
                .expect("cached file exists")
                .hash = cached.sha256;
            aggregate.reused = aggregate.reused.saturating_add(1);
            tree_reused = tree_reused.saturating_add(1);
            progress(ProgressEvent {
                phase: phase.to_string(),
                done: tree_reused,
                total: tree_total,
                subject: relative.to_string_lossy().into_owned(),
            });
        } else {
            pending.push((relative, file, fingerprint));
        }
    }
    let mut completed = 0_u64;
    let (_, results) = bounded_map_stream(
        &pending,
        |(relative, file, fingerprint)| {
            let hash = file_sha256(&file.path).map_err(|error| error.to_string())?;
            let metadata = fs::metadata(&file.path).map_err(|error| error.to_string())?;
            if metadata.len() != file.len || metadata.modified().ok() != file.modified {
                return Err("file changed during scan".to_string());
            }
            Ok((relative.clone(), fingerprint.clone(), hash))
        },
        |item_index, result| {
            completed += 1;
            if let Some((run, _)) = &run {
                match result {
                    Ok((relative, fingerprint, hash)) => run
                        .record_success(
                            relative,
                            fingerprint,
                            &CachedHash {
                                sha256: hash.clone(),
                            },
                        )
                        .map_err(|error| format!("replica index: {error}"))?,
                    Err(message) => run
                        .record_failure(&pending[item_index].0, message)
                        .map_err(|error| format!("replica index: {error}"))?,
                }
            }
            progress(ProgressEvent {
                phase: phase.to_string(),
                done: tree_reused.saturating_add(completed),
                total: tree_total,
                subject: pending[item_index].0.to_string_lossy().into_owned(),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    for result in results {
        let (relative, _, hash) = result.map_err(|message| {
            aggregate.failed = aggregate.failed.saturating_add(1);
            BackendError::validation(format!("error.replica.fileUnstable: {message}"))
        })?;
        aggregate.analyzed = aggregate.analyzed.saturating_add(1);
        snapshot
            .files
            .get_mut(&relative)
            .expect("hashed file exists")
            .hash = hash;
    }
    if !pending.is_empty() {
        aggregate.parallel_analysis = true;
        aggregate.analysis_workers = aggregate
            .analysis_workers
            .max(crate::parallel::worker_count(pending.len()) as u64);
    }
    if let Some((run, _)) = run.take() {
        let run_stats = ScanIndexStats {
            discovered: snapshot.files.len() as u64,
            reused: snapshot
                .files
                .values()
                .filter(|file| !file.hash.is_empty())
                .count()
                .saturating_sub(pending.len()) as u64,
            analyzed: pending.len() as u64,
            failed: 0,
            prior_interrupted_runs: 0,
            force_rescan: false,
            parallel_analysis: !pending.is_empty(),
            analysis_workers: crate::parallel::worker_count(pending.len()) as u64,
        };
        run.complete(&run_stats)
            .map_err(|error| BackendError::request_failed(format!("replica index: {error}")))?;
    }
    Ok(())
}

pub(super) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn collect_missing_directories(
    stop: &Path,
    leaf: &Path,
    result: &mut BTreeSet<PathBuf>,
) -> Result<(), BackendError> {
    let mut current = leaf.to_path_buf();
    while current != stop {
        if !current.starts_with(stop) {
            return Err(BackendError::path_escape());
        }
        if current.exists() {
            break;
        }
        result.insert(current.clone());
        current = current
            .parent()
            .ok_or_else(BackendError::path_escape)?
            .to_path_buf();
    }
    Ok(())
}
