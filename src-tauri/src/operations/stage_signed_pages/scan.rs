use super::*;

pub(super) fn empty_plan() -> Plan {
    let mut meta = JsonObject::new();
    meta.insert("archiveReadOnly".to_string(), Value::Bool(true));
    meta.insert("preservesArchiveLayout".to_string(), Value::Bool(true));
    meta.insert("usesOnlySignedFirstPage".to_string(), Value::Bool(true));
    Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
        meta,
    }
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

pub(super) fn overlaps(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

pub(super) fn stale_hardlink_temps(destination: &Path, review_root: &Path) -> Vec<PathBuf> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        _ => return Vec::new(),
    }
    if !fs::canonicalize(destination).is_ok_and(|path| path.starts_with(review_root)) {
        return Vec::new();
    }
    let Some(parent) = destination.parent() else {
        return Vec::new();
    };
    let Some(destination_name) = destination.file_name().and_then(|value| value.to_str()) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut stale = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| temporary_pdf_name_matches(name, destination_name))
        })
        .filter(|candidate| {
            fs::symlink_metadata(candidate).is_ok_and(|metadata| {
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && same_file_identity(destination, candidate)
            })
        })
        .filter(|candidate| {
            fs::canonicalize(candidate).is_ok_and(|path| path.starts_with(review_root))
        })
        .collect::<Vec<_>>();
    stale.sort();
    stale
}

pub(super) fn signed_pdfs<P>(
    root: &Path,
    recursive: bool,
    progress: &mut P,
) -> Result<(Vec<PathBuf>, usize), BackendError>
where
    P: FnMut(ProgressEvent),
{
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    let mut scanned = 0_u64;
    let mut worker_count = 1_usize;
    while !directories.is_empty() {
        directories.sort();
        let current_count = directories.len() as u64;
        let (workers, batches) = bounded_map(&directories, |directory| {
            scan_signed_directory(directory, recursive)
        })
        .map_err(BackendError::request_failed)?;
        worker_count = worker_count.max(workers);
        let mut next = Vec::new();
        for batch in batches {
            let batch = batch?;
            files.extend(batch.files);
            next.extend(batch.directories);
        }
        if files.len() > MAX_SIGNED_PDFS {
            return Err(BackendError::validation(format!(
                "signed scans exceed the {MAX_SIGNED_PDFS}-PDF limit"
            )));
        }
        scanned += current_count;
        progress(ProgressEvent {
            phase: "signed_discovery".to_string(),
            done: scanned,
            total: scanned.saturating_add(next.len() as u64),
            subject: format!("{} PDF", files.len()),
        });
        directories = next;
    }
    files.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    Ok((files, worker_count))
}

pub(super) struct SignedDirectoryBatch {
    pub(super) directories: Vec<PathBuf>,
    pub(super) files: Vec<PathBuf>,
}

pub(super) fn scan_signed_directory(
    directory: &Path,
    recursive: bool,
) -> Result<SignedDirectoryBatch, BackendError> {
    let entries = fs::read_dir(directory)
        .map_err(|_| BackendError::path_invalid("signed scans could not be read"))?;
    let mut batch = SignedDirectoryBatch {
        directories: Vec::new(),
        files: Vec::new(),
    };
    for entry in entries {
        let entry =
            entry.map_err(|_| BackendError::path_invalid("signed scan entry could not be read"))?;
        let file_type = entry
            .file_type()
            .map_err(|_| BackendError::path_invalid("signed scan type could not be read"))?;
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let hidden = name.to_str().is_some_and(|name| name.starts_with('.'));
        if file_type.is_file()
            && !hidden
            && entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
        {
            batch.files.push(entry.path());
        } else if recursive && file_type.is_dir() {
            batch.directories.push(entry.path());
        }
    }
    Ok(batch)
}

pub(super) struct ArchiveIndexResult {
    pub(super) matches: BTreeMap<String, Vec<PathBuf>>,
    pub(super) worker_count: usize,
    pub(super) directories_scanned: usize,
    pub(super) entries_scanned: usize,
}

pub(super) struct ArchiveDirectoryBatch {
    pub(super) directories: Vec<PathBuf>,
    pub(super) matches: Vec<(String, PathBuf)>,
    pub(super) entries_scanned: usize,
}

pub(super) fn canonical_stage_code(matched: &RfiMatch) -> Option<String> {
    if !matches!(matched.method, RfiMethod::Strict | RfiMethod::OcrAlias) {
        return None;
    }
    normalize_full_code(&matched.code)
}

pub(super) fn canonical_archive_filename_code(filename: &str) -> Option<String> {
    let body = strip_ascii_prefix_case_insensitive(filename, "RFI No. ")?;
    let body = strip_ascii_suffix_case_insensitive(body, ".pdf")?;
    let rfi = strip_ascii_prefix_case_insensitive(body, "AZE-SUN-INT-QMS-RFI-")?;
    let (discipline, number) = rfi.split_once('-')?;
    if !["CIV", "MEC", "ELE", "PRC"]
        .iter()
        .any(|supported| discipline.eq_ignore_ascii_case(supported))
    {
        return None;
    }
    let number = normalize_candidate_number(number)?;
    Some(format!(
        "{PREFIX}-{}-{number}",
        discipline.to_ascii_uppercase()
    ))
}

pub(super) fn strip_ascii_prefix_case_insensitive<'a>(
    value: &'a str,
    prefix: &str,
) -> Option<&'a str> {
    value
        .get(..prefix.len())?
        .eq_ignore_ascii_case(prefix)
        .then(|| &value[prefix.len()..])
}

pub(super) fn strip_ascii_suffix_case_insensitive<'a>(
    value: &'a str,
    suffix: &str,
) -> Option<&'a str> {
    let start = value.len().checked_sub(suffix.len())?;
    value
        .get(start..)?
        .eq_ignore_ascii_case(suffix)
        .then(|| &value[..start])
}

pub(super) fn archive_index<P, C>(
    root: &Path,
    codes: BTreeSet<String>,
    progress: &mut P,
    cancelled: &mut C,
) -> Result<ArchiveIndexResult, BackendError>
where
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let wanted = codes
        .iter()
        .cloned()
        .map(|code| (code.clone(), code))
        .collect::<BTreeMap<_, _>>();
    let mut matches = codes
        .into_iter()
        .map(|code| (code, Vec::new()))
        .collect::<BTreeMap<_, _>>();
    if wanted.is_empty() {
        return Ok(ArchiveIndexResult {
            matches,
            worker_count: 0,
            directories_scanned: 0,
            entries_scanned: 0,
        });
    }
    let mut directories = vec![root.to_path_buf()];
    let mut entries_scanned = 0usize;
    let mut directories_scanned = 0usize;
    let mut worker_count = 1usize;
    while !directories.is_empty() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        directories.sort();
        let (workers, batches) = bounded_map(&directories, |directory| {
            scan_archive_directory(root, directory, &wanted)
        })
        .map_err(BackendError::request_failed)?;
        worker_count = worker_count.max(workers);
        directories_scanned = directories_scanned
            .checked_add(directories.len())
            .ok_or_else(|| BackendError::validation("archive directory count overflow"))?;
        let mut next = Vec::new();
        for batch in batches {
            let batch = batch?;
            entries_scanned = entries_scanned
                .checked_add(batch.entries_scanned)
                .ok_or_else(|| BackendError::validation("archive entry count overflow"))?;
            if entries_scanned > MAX_ARCHIVE_ENTRIES {
                return Err(BackendError::validation(format!(
                    "archive exceeds the {MAX_ARCHIVE_ENTRIES}-entry limit"
                )));
            }
            next.extend(batch.directories);
            for (code, path) in batch.matches {
                matches.entry(code).or_default().push(path);
            }
        }
        next.sort();
        progress(ProgressEvent {
            phase: "archive_scan".to_string(),
            done: directories_scanned as u64,
            total: directories_scanned.saturating_add(next.len()) as u64,
            subject: root.to_string_lossy().into_owned(),
        });
        directories = next;
    }
    for values in matches.values_mut() {
        values.sort();
    }
    Ok(ArchiveIndexResult {
        matches,
        worker_count,
        directories_scanned,
        entries_scanned,
    })
}

pub(super) fn scan_archive_directory(
    root: &Path,
    directory: &Path,
    wanted: &BTreeMap<String, String>,
) -> Result<ArchiveDirectoryBatch, BackendError> {
    let mut batch = ArchiveDirectoryBatch {
        directories: Vec::new(),
        matches: Vec::new(),
        entries_scanned: 0,
    };
    for entry in fs::read_dir(directory)
        .map_err(|_| BackendError::path_invalid("archive directory could not be read"))?
    {
        let entry =
            entry.map_err(|_| BackendError::path_invalid("archive entry could not be read"))?;
        batch.entries_scanned += 1;
        if batch.entries_scanned > MAX_ARCHIVE_ENTRIES {
            return Err(BackendError::validation(format!(
                "archive exceeds the {MAX_ARCHIVE_ENTRIES}-entry limit"
            )));
        }
        let file_type = entry
            .file_type()
            .map_err(|_| BackendError::path_invalid("archive metadata could not be read"))?;
        if file_type.is_dir() && !file_type.is_symlink() {
            batch.directories.push(entry.path());
            continue;
        }
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        let filename = entry.file_name();
        let Some(name) = filename.to_str() else {
            continue;
        };
        let Some(code) = canonical_archive_filename_code(name).and_then(|code| wanted.get(&code))
        else {
            continue;
        };
        let entry_path = entry.path();
        let resolved = fs::canonicalize(entry_path)
            .map_err(|_| BackendError::path_invalid("archive PDF could not be resolved"))?;
        if !resolved.starts_with(root) || !resolved.is_file() {
            return Err(BackendError::path_escape());
        }
        batch.matches.push((code.clone(), resolved));
    }
    batch.directories.sort();
    batch.matches.sort();
    Ok(batch)
}

pub(super) fn add_directory_steps(
    parent: &Path,
    review_root: &Path,
    planned: &mut BTreeSet<PathBuf>,
    steps: &mut Vec<Step>,
) -> Result<(), BackendError> {
    let relative = parent
        .strip_prefix(review_root)
        .map_err(|_| BackendError::path_escape())?;
    let mut current = review_root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if planned.contains(&current) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => continue,
            Ok(_) => return Err(BackendError::path_invalid("review path is blocked")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(BackendError::path_invalid(
                    "review path could not be inspected",
                ));
            }
        }
        let mut meta = JsonObject::new();
        meta.insert(
            "reviewRoot".to_string(),
            Value::String(review_root.to_string_lossy().into_owned()),
        );
        steps.push(Step {
            kind: StepKind::Mkdir,
            src: None,
            dst: Some(current.to_string_lossy().into_owned()),
            note_key: Some("note.folder_created".to_string()),
            meta,
        });
        planned.insert(current.clone());
    }
    Ok(())
}
