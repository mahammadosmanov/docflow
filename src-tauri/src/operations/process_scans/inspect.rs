use super::*;

pub(super) fn plan_ocr_tasks(
    pdfs: &[PathBuf],
    inspections: &PdfInspections,
    scan_mode: ScanMode,
    fast_cover_mode: bool,
) -> Result<(OcrTasks, BTreeSet<PathBuf>), BackendError> {
    let mut tasks = Vec::new();
    let mut fast_cover_pdfs = BTreeSet::new();
    let mut total_pages = 0_usize;
    for pdf in pdfs {
        let page_count = inspections
            .get(pdf)
            .and_then(|inspection| inspection.as_ref().ok())
            .map_or(1, |facts| facts.page_count);
        total_pages = total_pages.saturating_add(page_count);
        if total_pages > MAX_SCAN_PAGES {
            return Err(BackendError::validation(format!(
                "scan source exceeds the {MAX_SCAN_PAGES}-page limit"
            )));
        }
        if inspections.get(pdf).is_none_or(Result::is_ok) && page_count > 0 {
            if fast_cover_mode && scan_mode == ScanMode::MultiRfi && page_count > 1 {
                fast_cover_pdfs.insert(pdf.clone());
            }
            let ocr_page_count = if scan_mode == ScanMode::SingleRfi {
                1
            } else {
                page_count
            };
            tasks.extend((0..ocr_page_count).map(|page| (pdf.clone(), page)));
        }
    }
    Ok((tasks, fast_cover_pdfs))
}

pub(super) fn discover_pdfs(source: &Path) -> Result<Vec<PathBuf>, BackendError> {
    let mut pdfs = fs::read_dir(source)
        .map_err(|_| BackendError::path_invalid("scan source could not be read"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let file_type = entry.file_type().ok()?;
            (file_type.is_file()
                && !name.starts_with('.')
                && entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf")))
            .then_some(entry.path())
        })
        .take(MAX_SCAN_PDFS + 1)
        .collect::<Vec<_>>();
    if pdfs.len() > MAX_SCAN_PDFS {
        return Err(BackendError::validation(format!(
            "scan source exceeds the {MAX_SCAN_PDFS}-PDF limit"
        )));
    }
    pdfs.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(pdfs)
}

pub(super) fn inspect_pdfs<P>(
    pdfs: &[PathBuf],
    progress: &mut P,
) -> Result<(usize, PdfInspections), BackendError>
where
    P: FnMut(ProgressEvent),
{
    let mut completed = 0_u64;
    let (workers, results) = bounded_map_stream(
        pdfs,
        |pdf| crate::pdf::inspect(pdf),
        |index, _| {
            completed += 1;
            progress(ProgressEvent {
                phase: "scan_discovery".to_string(),
                done: completed,
                total: pdfs.len() as u64,
                subject: display_name(&pdfs[index]),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    let inspections = pdfs
        .iter()
        .cloned()
        .zip(
            results
                .into_iter()
                .map(|result| result.map_err(|error| error.to_string())),
        )
        .collect();
    Ok((workers, inspections))
}

type OrientationDetections = BTreeMap<
    (PathBuf, usize),
    Result<crate::operations::pdf_orientation::OrientationDetection, String>,
>;

pub(super) fn detect_orientations<A, P>(
    ocr: &A,
    tasks: &[(PathBuf, usize)],
    progress: &mut P,
) -> Result<(usize, OrientationDetections), BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
{
    let mut completed = 0_u64;
    let (workers, results) = bounded_map_stream(
        tasks,
        |(pdf, page)| ocr.detect_orientation(pdf, *page),
        |index, _| {
            completed += 1;
            let (pdf, page) = &tasks[index];
            progress(ProgressEvent {
                phase: "scan_orientation".to_string(),
                done: completed,
                total: tasks.len() as u64,
                subject: format!("{} — #{}", display_name(pdf), page + 1),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    Ok((workers, tasks.iter().cloned().zip(results).collect()))
}
