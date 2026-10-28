use super::*;

/// One uncached PDF after discovery and inspection, ready to be classified.
pub(super) struct UncachedPdf {
    pub(super) path: PathBuf,
    pub(super) relative: PathBuf,
    pub(super) fingerprint: FileFingerprint,
    pub(super) source_sha256: String,
    pub(super) inspection: Result<PdfInspection, PdfError>,
}

/// Page tasks and per-file work collected while classifying inspected PDFs.
#[derive(Default)]
pub(super) struct InspectionAccumulator {
    pub(super) tasks: Vec<PageTask>,
    pub(super) work: Vec<PdfWork>,
    pub(super) initial_hashes: BTreeMap<PathBuf, String>,
    pub(super) initial_fingerprints: BTreeMap<PathBuf, FileFingerprint>,
}

/// Records one uncached PDF's inspection: unsupported files are reported, and supported files queue page analysis.
pub(super) fn classify_uncached(
    run: Option<&crate::scan_index::ScanRun<'_>>,
    prepared: UncachedPdf,
    acc: &mut InspectionAccumulator,
    stats: &mut ScanIndexStats,
    plan: &mut Plan,
) -> Result<(), BackendError> {
    let UncachedPdf {
        path,
        relative,
        fingerprint,
        source_sha256,
        inspection,
    } = prepared;
    match inspection {
        Ok(facts) if facts.encrypted => record_unsupported(
            run,
            &relative,
            &fingerprint,
            &source_sha256,
            "encrypted PDF",
            stats,
            plan,
        )?,
        Ok(facts) if facts.signed => record_unsupported(
            run,
            &relative,
            &fingerprint,
            &source_sha256,
            "digitally signed PDF",
            stats,
            plan,
        )?,
        Ok(facts) if facts.page_count == 0 => record_unsupported(
            run,
            &relative,
            &fingerprint,
            &source_sha256,
            "PDF has no pages",
            stats,
            plan,
        )?,
        Ok(facts) => {
            let current_rotations = facts
                .pages
                .iter()
                .map(|page| normalize_rotation(page.rotation))
                .collect::<Result<Vec<_>, _>>();
            let current_rotations = match current_rotations {
                Ok(rotations) => rotations,
                Err(error) => {
                    record_unsupported(
                        run,
                        &relative,
                        &fingerprint,
                        &source_sha256,
                        &error.message,
                        stats,
                        plan,
                    )?;
                    return Ok(());
                }
            };
            if acc.tasks.len().saturating_add(facts.page_count) > MAX_PAGES {
                return Err(BackendError::validation(format!(
                    "orientation scan exceeds the {MAX_PAGES}-page limit"
                )));
            }
            for (page_index, current_rotation) in current_rotations.into_iter().enumerate() {
                acc.tasks.push(PageTask {
                    path: path.clone(),
                    page_index,
                    current_rotation,
                });
            }
            acc.initial_hashes.insert(path.clone(), source_sha256);
            acc.initial_fingerprints.insert(path.clone(), fingerprint);
            acc.work.push(PdfWork {
                path,
                relative,
                page_count: facts.page_count,
                source_hash: None,
                cached: false,
            });
        }
        Err(error) => {
            if let Some(run) = &run {
                run.record_failure(&relative, &error.to_string())
                    .map_err(|failure| {
                        BackendError::request_failed(format!("scan checkpoint failed: {failure}"))
                    })?;
            }
            stats.failed += 1;
            plan.warnings.push(warning(
                "warn.rfi.orientationUnsupported",
                &format!("{} — {error}", relative.display()),
            ));
        }
    }
    Ok(())
}
