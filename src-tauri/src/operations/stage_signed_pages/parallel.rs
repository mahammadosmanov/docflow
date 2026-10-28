use super::*;

/// Inspects every signed PDF and OCRs each supported page in parallel.
/// Returns the analysis worker count and a page-read cache for the serial classification pass.
pub(super) fn parallel_identity_cache<A, P, C>(
    pdfs: &[PathBuf],
    ocr: &A,
    progress: &mut P,
    cancelled: &mut C,
) -> Result<(usize, CachedPageOcr), BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    if cancelled() {
        return Err(BackendError::cancelled());
    }
    let mut inspected = 0_u64;
    let (inspection_workers, inspections) = bounded_map_stream(
        pdfs,
        |pdf| inspect(pdf),
        |index, _| {
            inspected += 1;
            progress(ProgressEvent {
                phase: "signed_inspection".to_string(),
                done: inspected,
                total: pdfs.len() as u64,
                subject: display_name(&pdfs[index]),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    let mut tasks = Vec::new();
    for (pdf, inspection) in pdfs.iter().zip(inspections) {
        if let Ok(facts) = inspection
            && fidelity_supported(&facts, PdfRole::SignedScan)
        {
            if tasks.len().saturating_add(facts.page_count) > MAX_SIGNED_PAGES {
                return Err(BackendError::validation(format!(
                    "signed scans exceed the {MAX_SIGNED_PAGES}-page limit"
                )));
            }
            tasks.extend((0..facts.page_count).map(|page_index| (pdf.clone(), page_index)));
        }
    }
    let mut completed = 0_u64;
    let (worker_count, results) = bounded_map_stream(
        &tasks,
        |(pdf, page_index)| ocr.read_identity_page(pdf, *page_index),
        |index, _| {
            completed += 1;
            let (pdf, page_index) = &tasks[index];
            progress(ProgressEvent {
                phase: "signed_ocr".to_string(),
                done: completed,
                total: tasks.len() as u64,
                subject: candidate_display(pdf, *page_index),
            });
            Ok(())
        },
    )
    .map_err(BackendError::request_failed)?;
    if cancelled() {
        return Err(BackendError::cancelled());
    }
    let cached = CachedPageOcr {
        reads: tasks.into_iter().zip(results).collect(),
        page_previews: ocr.supports_page_previews(),
        diagnostics: ocr.diagnostics(),
    };
    Ok((worker_count.max(inspection_workers), cached))
}
