use super::*;

pub(super) fn clean_header(value: &str) -> String {
    value.nfc().collect::<String>().trim().to_ascii_uppercase()
}

pub(super) fn cell_is_merged(sheet: &WorksheetData, row: u32, col: u32) -> bool {
    sheet.merged_ranges.iter().any(|range| {
        range.first_row <= row
            && row <= range.last_row
            && range.first_column <= col
            && col <= range.last_column
    })
}

pub(super) fn discover_header(
    sheet: &WorksheetData,
    accepted_value_headers: &[&str],
) -> Result<HeaderLayout, BackendError> {
    let mut layouts = Vec::new();
    for row in 1..=sheet.max_row.min(MAX_HEADER_ROWS) {
        let mut rfi_columns = Vec::new();
        let mut value_columns = Vec::new();
        for col in 1..=sheet.max_column.min(MAX_HEADER_COLUMNS) {
            let Some(cell) = sheet.cell(row, col) else {
                continue;
            };
            let header = clean_header(&cell.value);
            if matches!(header.as_str(), "RFI NO." | "RFI NO") {
                rfi_columns.push(col);
            }
            if accepted_value_headers.contains(&header.as_str()) {
                value_columns.push(col);
            }
        }
        if rfi_columns.len() == 1 && value_columns.len() == 1 {
            layouts.push(HeaderLayout {
                row,
                rfi_col: rfi_columns[0],
                value_col: value_columns[0],
            });
        }
    }
    if layouts.len() != 1 {
        return Err(BackendError::validation(
            "error.rfi.systemSync.workbookLayoutInvalid",
        ));
    }
    Ok(layouts[0])
}

#[doc(hidden)]
pub fn scan_system_root_indexed<P>(
    root: &Path,
    scan_index: Option<&ScanIndex>,
    owner_id: &str,
    force_rescan: bool,
    mut progress: P,
) -> Result<(Vec<ScannedRfiRecord>, ScanIndexStats), BackendError>
where
    P: FnMut(ProgressEvent),
{
    if !root.is_dir() {
        return Err(BackendError::path_invalid(format!(
            "RFI system root is not a directory: {}",
            root.display()
        )));
    }
    let mut search_dirs = Vec::new();
    for entry in
        fs::read_dir(root).map_err(|error| BackendError::path_invalid(error.to_string()))?
    {
        let entry = entry.map_err(|error| BackendError::path_invalid(error.to_string()))?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_uppercase();
        if let Some(discipline) = DISCIPLINES.iter().find(|discipline| **discipline == name) {
            search_dirs.push((*discipline, entry.path()));
        }
    }
    if search_dirs.is_empty() {
        let name = root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_uppercase();
        if let Some(discipline) = DISCIPLINES.iter().find(|discipline| **discipline == name) {
            search_dirs.push((*discipline, root.to_path_buf()));
        }
    }

    progress(ProgressEvent {
        phase: "system_discovery".to_string(),
        done: 0,
        total: 0,
        subject: root.to_string_lossy().into_owned(),
    });
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    let mut discovered_dirs = 0_u64;
    let (discovery_workers, batches) = bounded_map_stream(
        &search_dirs,
        |(discipline, path)| scan_system_discipline(discipline, path),
        |index, _| {
            discovered_dirs += 1;
            progress(ProgressEvent {
                phase: "system_discovery".to_string(),
                done: discovered_dirs,
                total: search_dirs.len() as u64,
                subject: search_dirs[index].1.to_string_lossy().into_owned(),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    for batch in batches {
        for (record, folder) in batch.map_err(BackendError::path_invalid)? {
            if !seen.insert(record.canonical_rfi.clone()) {
                continue;
            }
            candidates.push((record, folder));
        }
    }
    let temp_dir = tempfile::tempdir()
        .map_err(|error| BackendError::protocol(format!("temporary directory: {error}")))?;
    candidates.sort_by(|(left, _), (right, _)| {
        left.discipline
            .cmp(&right.discipline)
            .then(left.number.cmp(&right.number))
    });
    progress(ProgressEvent {
        phase: "system_discovery".to_string(),
        done: candidates.len() as u64,
        total: candidates.len() as u64,
        subject: String::new(),
    });
    let mut stats = ScanIndexStats {
        discovered: candidates.len() as u64,
        force_rescan,
        ..ScanIndexStats::default()
    };
    let run = scan_index
        .map(|index| {
            index
                .begin_run(
                    owner_id,
                    OPERATION_ID,
                    SYSTEM_SCAN_PROFILE,
                    root,
                    force_rescan,
                )
                .map(|(run, prior)| {
                    stats.prior_interrupted_runs = prior;
                    run
                })
        })
        .transpose()
        .map_err(|error| {
            BackendError::request_failed(format!("scan index unavailable: {error}"))
        })?;
    let total = candidates.len() as u64;
    let mut completed = 0_u64;
    let (worker_count, analyzed) = bounded_map_stream(
        &candidates,
        |(record, rfi_folder)| {
            let mut record = record.clone();
            let Some(pdf_path) = record.pdf_path.as_ref().map(PathBuf::from) else {
                record.signature_reason = "Exact RFI No. PDF not found in folder".to_string();
                return Ok(CandidateAnalysis {
                    record,
                    relative: None,
                    fingerprint: None,
                    detection: None,
                    reused: false,
                    transient_failure: false,
                    subject: rfi_folder.to_string_lossy().into_owned(),
                });
            };
            let relative = pdf_path
                .strip_prefix(root)
                .map_err(|_| {
                    CandidateAnalysisError::new(None, "RFI scan escaped the selected system root")
                })?
                .to_path_buf();
            let fingerprint = FileFingerprint::read(&pdf_path).map_err(|error| {
                CandidateAnalysisError::new(
                    Some(&relative),
                    format!("RFI scan fingerprint failed: {error}"),
                )
            })?;
            let mut cached = if force_rescan {
                None
            } else {
                run.as_ref()
                    .map(|run| run.lookup::<CachedWorleyDetection>(&relative, &fingerprint))
                    .transpose()
                    .map_err(|error| {
                        CandidateAnalysisError::new(
                            Some(&relative),
                            format!("scan index read failed: {error}"),
                        )
                    })?
                    .flatten()
            };
            if cached.as_ref().is_some_and(|value| value.signed_by_worley) {
                let current_hash = file_sha256(&pdf_path).map_err(|error| {
                    CandidateAnalysisError::new(
                        Some(&relative),
                        format!("signed RFI hash failed: {error}"),
                    )
                })?;
                if cached
                    .as_ref()
                    .and_then(|value| value.source_sha256.as_deref())
                    != Some(current_hash.as_str())
                {
                    cached = None;
                }
            }
            let (detection, reused, transient_failure) = if let Some(cached) = cached {
                (cached, true, false)
            } else {
                let (signed_by_worley, signature_reason) =
                    detect_worley_signature(&pdf_path, temp_dir.path());
                let current_fingerprint = FileFingerprint::read(&pdf_path).map_err(|error| {
                    CandidateAnalysisError::new(
                        Some(&relative),
                        format!("RFI scan fingerprint changed: {error}"),
                    )
                })?;
                if current_fingerprint != fingerprint {
                    return Err(CandidateAnalysisError::new(
                        Some(&relative),
                        "RFI PDF changed during system scan; preview again",
                    ));
                }
                let source_sha256 = signed_by_worley
                    .then(|| file_sha256(&pdf_path))
                    .transpose()
                    .map_err(|error| {
                        CandidateAnalysisError::new(
                            Some(&relative),
                            format!("signed RFI hash failed: {error}"),
                        )
                    })?;
                let detection = CachedWorleyDetection {
                    signed_by_worley,
                    signature_reason,
                    source_sha256,
                };
                let transient_failure = detection.signature_reason.starts_with("PDF render error:")
                    || detection.signature_reason.starts_with("Image read error:");
                (detection, false, transient_failure)
            };
            record.signed_by_worley = detection.signed_by_worley;
            record.signature_reason = detection.signature_reason.clone();
            Ok(CandidateAnalysis {
                record,
                relative: Some(relative.clone()),
                fingerprint: Some(fingerprint),
                detection: Some(detection),
                reused,
                transient_failure,
                subject: relative.to_string_lossy().into_owned(),
            })
        },
        |_, result: &Result<CandidateAnalysis, CandidateAnalysisError>| {
            let item = match result {
                Ok(item) => item,
                Err(error) => {
                    if let (Some(run), Some(relative)) = (&run, &error.relative) {
                        run.record_failure(relative, &error.message)
                            .map_err(|failure| format!("scan checkpoint failed: {failure}"))?;
                    }
                    return Err(error.message.clone());
                }
            };
            if item.reused {
                stats.reused += 1;
            } else if let (Some(relative), Some(fingerprint), Some(detection)) =
                (&item.relative, &item.fingerprint, &item.detection)
            {
                if let Some(run) = &run {
                    if item.transient_failure {
                        run.record_failure(relative, &detection.signature_reason)
                    } else {
                        run.record_success(relative, fingerprint, detection)
                    }
                    .map_err(|error| format!("scan checkpoint failed: {error}"))?;
                }
                if item.transient_failure {
                    stats.failed += 1;
                } else {
                    stats.analyzed += 1;
                }
            }
            completed += 1;
            progress(ProgressEvent {
                phase: "system_signature_analysis".to_string(),
                done: completed,
                total,
                subject: item.subject.clone(),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    stats.parallel_analysis = !candidates.is_empty();
    stats.analysis_workers = worker_count.max(discovery_workers) as u64;
    let mut records = analyzed
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| BackendError::request_failed(error.message))?
        .into_iter()
        .map(|item| item.record)
        .collect::<Vec<_>>();
    if let Some(run) = run {
        run.complete(&stats).map_err(|error| {
            BackendError::request_failed(format!("scan run completion failed: {error}"))
        })?;
    }
    records.sort_by(|left, right| {
        left.discipline
            .cmp(&right.discipline)
            .then(left.number.cmp(&right.number))
    });
    Ok((records, stats))
}

pub(super) fn scan_system_discipline(
    discipline: &str,
    discipline_path: &Path,
) -> Result<Vec<(ScannedRfiRecord, PathBuf)>, String> {
    let entries = fs::read_dir(discipline_path).map_err(|error| error.to_string())?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let rfi_folder = entry.path();
        let folder_name = entry.file_name().to_string_lossy().to_string();
        let Some((parsed_discipline, number)) = parse_rfi_discipline_and_number(&folder_name)
        else {
            continue;
        };
        if parsed_discipline != discipline {
            continue;
        }
        let canonical_rfi = format!(
            "{}{}-{number:04}",
            canonical_rfi_prefix(),
            parsed_discipline
        );
        let expected_pdf = format!("RFI No. {canonical_rfi}.pdf");
        let selected_pdf = fs::read_dir(&rfi_folder)
            .into_iter()
            .flatten()
            .flatten()
            .find_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .is_some_and(|kind| kind.is_file() && !kind.is_symlink())
                    .then(|| entry.path())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.eq_ignore_ascii_case(&expected_pdf))
                    })
            });
        candidates.push((
            ScannedRfiRecord {
                discipline: parsed_discipline.to_string(),
                number,
                canonical_rfi,
                short_rfi: format!("{parsed_discipline}-{number:04}"),
                rfi_folder: rfi_folder.to_string_lossy().into_owned(),
                pdf_path: selected_pdf.map(|path| path.to_string_lossy().into_owned()),
                signed_by_worley: false,
                signature_reason: String::new(),
            },
            rfi_folder,
        ));
    }
    Ok(candidates)
}

pub(crate) fn detect_worley_signature(pdf_path: &Path, temp_dir: &Path) -> (bool, String) {
    let png_path = temp_dir.join(format!("{}.png", Uuid::new_v4()));
    if let Err(error) = crate::ocr::render_first_page_png(pdf_path, &png_path, 150) {
        return (false, format!("PDF render error: {error}"));
    }
    let image = match crate::ocr::read_png_gray(&png_path) {
        Ok(image) => image,
        Err(error) => return (false, format!("Image read error: {error}")),
    };
    let _ = fs::remove_file(&png_path);
    if image.width < 100 || image.height < 100 {
        return (false, "Image dimensions too small".to_string());
    }
    // The approved form's Worley evidence is the inner name/signature area of
    // the bottom-right OTHERS cell. Everything outside this region—including
    // contractor signatures and the general-notes area—is non-authoritative.
    let (x1, x2) = (
        (image.width as f64 * 0.74) as usize,
        (image.width as f64 * 0.92) as usize,
    );
    let (y1, y2) = (
        (image.height as f64 * 0.72) as usize,
        (image.height as f64 * 0.77) as usize,
    );
    let width = x2.saturating_sub(x1);
    let height = y2.saturating_sub(y1);
    if width == 0 || height == 0 {
        return (false, "Empty OTHERS inspection area".to_string());
    }
    let mut row_dark = vec![0usize; height];
    let mut column_dark = vec![0usize; width];
    for (row, y) in (y1..y2.min(image.height)).enumerate() {
        for (column, x) in (x1..x2.min(image.width)).enumerate() {
            if image.pixels[y * image.width + x] < 175 {
                row_dark[row] += 1;
                column_dark[column] += 1;
            }
        }
    }
    let ignored_rows = row_dark
        .iter()
        .map(|dark| *dark as f64 / width as f64 >= 0.35)
        .collect::<Vec<_>>();
    let ignored_columns = column_dark
        .iter()
        .map(|dark| *dark as f64 / height as f64 >= 0.35)
        .collect::<Vec<_>>();
    let mut dark = 0usize;
    let mut total = 0usize;
    for (row, y) in (y1..y2.min(image.height)).enumerate() {
        for (column, x) in (x1..x2.min(image.width)).enumerate() {
            if ignored_rows[row] || ignored_columns[column] {
                continue;
            }
            total += 1;
            dark += usize::from(image.pixels[y * image.width + x] < 175);
        }
    }
    if total == 0 {
        return (false, "Empty OTHERS inspection area".to_string());
    }
    let ratio = dark as f64 / total as f64;
    let signed = dark >= 500 && ratio >= 0.025;
    let reason = if signed {
        format!(
            "Signed by Worley (OTHERS ink: {dark} px, {:.1}%)",
            ratio * 100.0
        )
    } else {
        format!(
            "Unsigned / OTHERS empty (ink: {dark} px, {:.2}%)",
            ratio * 100.0
        )
    };
    (signed, reason)
}
