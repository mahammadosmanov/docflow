use super::*;

pub(crate) fn canonical_rfi_prefix() -> &'static str {
    "AZE-SUN-INT-QMS-RFI-"
}

pub(crate) fn parse_rfi_discipline_and_number(name: &str) -> Option<(&'static str, u32)> {
    let clean = name.nfc().collect::<String>().trim().to_ascii_uppercase();
    static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"\A(?:AZE-SUN-INT-QMS-RFI-|RFI-)?(CIV|MEC|ELE|PRC)[-_ ]*([0-9]{1,4})\z")
            .unwrap()
    });
    let caps = RE.captures(&clean)?;
    let discipline = match caps.get(1)?.as_str() {
        "CIV" => "CIV",
        "MEC" => "MEC",
        "ELE" => "ELE",
        "PRC" => "PRC",
        _ => return None,
    };
    Some((
        discipline,
        normalize_candidate_number(caps.get(2)?.as_str())?
            .parse()
            .ok()?,
    ))
}

pub(super) fn exact_canonical_rfi(value: &str) -> Option<String> {
    let clean = value.nfc().collect::<String>().trim().to_ascii_uppercase();
    let (discipline, number) = parse_rfi_discipline_and_number(&clean)?;
    Some(format!(
        "{}{}-{number:04}",
        canonical_rfi_prefix(),
        discipline
    ))
}

pub(super) fn completion_code_from_filename(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let prefix = FILE_PREFIX.to_ascii_lowercase();
    if !lower.starts_with(&prefix) || !lower.ends_with(".pdf") {
        return None;
    }
    let code = name.get(FILE_PREFIX.len()..name.len().checked_sub(4)?)?;
    exact_canonical_rfi(code)
}

pub(super) fn discover_completion_pdfs<P, C>(
    root: &Path,
    mut progress: P,
    mut cancelled: C,
) -> Result<CompletionPdfDiscovery, BackendError>
where
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let root = fs::canonicalize(root)
        .map_err(|_| BackendError::path_invalid("RFI system root could not be resolved"))?;
    if !root.is_dir() {
        return Err(BackendError::path_invalid(
            "RFI system root is not a directory",
        ));
    }
    let mut directories = vec![root.clone()];
    let mut matches = BTreeMap::<String, Vec<PathBuf>>::new();
    let mut invalid_names = Vec::new();
    let mut worker_count = 1_usize;
    let mut directories_scanned = 0_usize;
    let mut entries_scanned = 0_usize;
    progress(ProgressEvent {
        phase: "completion_pdf_discovery".to_string(),
        done: 0,
        total: 1,
        subject: root.to_string_lossy().into_owned(),
    });
    while !directories.is_empty() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        directories.sort();
        let current_count = directories.len();
        let (workers, batches) = bounded_map(&directories, |directory| {
            scan_completion_directory(&root, directory)
        })
        .map_err(BackendError::request_failed)?;
        worker_count = worker_count.max(workers);
        let mut next = Vec::new();
        for batch in batches {
            let batch = batch?;
            entries_scanned = entries_scanned
                .checked_add(batch.entries_scanned)
                .ok_or_else(|| BackendError::validation("RFI system entry count overflow"))?;
            if entries_scanned > MAX_COMPLETION_ENTRIES {
                return Err(BackendError::validation(format!(
                    "RFI system exceeds the {MAX_COMPLETION_ENTRIES}-entry completion scan limit"
                )));
            }
            next.extend(batch.directories);
            invalid_names.extend(batch.invalid_names);
            for (code, path) in batch.matches {
                matches.entry(code).or_default().push(path);
            }
        }
        directories_scanned = directories_scanned
            .checked_add(current_count)
            .ok_or_else(|| BackendError::validation("RFI system directory count overflow"))?;
        next.sort();
        progress(ProgressEvent {
            phase: "completion_pdf_discovery".to_string(),
            done: directories_scanned as u64,
            total: directories_scanned.saturating_add(next.len()) as u64,
            subject: format!("{} exact RFI PDF", matches.len()),
        });
        directories = next;
    }
    for paths in matches.values_mut() {
        paths.sort();
        paths.dedup();
    }
    invalid_names.sort();
    invalid_names.dedup();
    Ok(CompletionPdfDiscovery {
        matches,
        invalid_names,
        worker_count,
        directories_scanned,
        entries_scanned,
    })
}

pub(super) fn scan_completion_directory(
    root: &Path,
    directory: &Path,
) -> Result<CompletionDirectoryBatch, BackendError> {
    let mut batch = CompletionDirectoryBatch {
        directories: Vec::new(),
        matches: Vec::new(),
        invalid_names: Vec::new(),
        entries_scanned: 0,
    };
    for entry in fs::read_dir(directory)
        .map_err(|_| BackendError::path_invalid("RFI system folder could not be read"))?
    {
        let entry = entry.map_err(|_| BackendError::path_invalid("RFI system entry failed"))?;
        batch.entries_scanned += 1;
        let file_type = entry
            .file_type()
            .map_err(|_| BackendError::path_invalid("RFI system entry metadata failed"))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            batch.directories.push(entry.path());
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_named_pdf = name.to_ascii_lowercase().starts_with("rfi no. ")
            && name.to_ascii_lowercase().ends_with(".pdf");
        let Some(code) = completion_code_from_filename(name) else {
            if is_named_pdf {
                batch.invalid_names.push(entry.path());
            }
            continue;
        };
        let path = fs::canonicalize(entry.path())
            .map_err(|_| BackendError::path_invalid("RFI PDF could not be resolved"))?;
        if !path.starts_with(root) || !path.is_file() {
            return Err(BackendError::path_escape());
        }
        batch.matches.push((code, path));
    }
    batch.directories.sort();
    batch.matches.sort();
    batch.invalid_names.sort();
    Ok(batch)
}
