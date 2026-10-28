use super::*;

pub(super) struct ScanContext<'a, A> {
    pub(super) params: &'a ProcessScansParams,
    pub(super) ocr: &'a A,
    pub(super) operation_id: &'a str,
    pub(super) source_sha256: Option<&'a BTreeMap<PathBuf, String>>,
    pub(super) destination: &'a Path,
    pub(super) transfer_kind: StepKind,
    pub(super) total_pdfs: usize,
    pub(super) fast_cover_mode: bool,
}

pub(super) struct ScanState {
    pub(super) plan: Plan,
    pub(super) groups: Vec<(String, Vec<Candidate>)>,
    pub(super) failures: Vec<(PathBuf, String)>,
    pub(super) review_pages: Vec<ReviewPages>,
    pub(super) split_sources: BTreeMap<PathBuf, String>,
    pub(super) scanned_pages: usize,
}

fn modified_time(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
}

/// Adds a candidate under its RFI code, opening a new group when the code is unseen.
fn push_group_candidate(
    groups: &mut Vec<(String, Vec<Candidate>)>,
    code: String,
    candidate: Candidate,
) {
    if let Some((_, candidates)) = groups.iter_mut().find(|(value, _)| value == &code) {
        candidates.push(candidate);
    } else {
        groups.push((code, vec![candidate]));
    }
}

/// Queues one PDF as a needs-review step under the review directory.
fn push_review_step(
    plan: &mut Plan,
    transfer_kind: StepKind,
    destination: &Path,
    pdf: &Path,
    meta: JsonObject,
) {
    plan.steps.push(step(
        transfer_kind,
        Some(pdf),
        Some(&destination.join(REVIEW_DIR).join(display_name(pdf))),
        "note.needs_review",
        meta,
    ));
}

pub(super) fn scan_one_pdf<A, P, C>(
    ctx: &ScanContext<'_, A>,
    state: &mut ScanState,
    index: usize,
    pdf: &PathBuf,
    progress: &mut P,
    cancelled: &mut C,
) -> Result<Option<Plan>, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let ScanContext {
        params,
        ocr,
        operation_id,
        source_sha256,
        destination,
        transfer_kind,
        total_pdfs,
        fast_cover_mode,
    } = *ctx;
    let ScanState {
        plan,
        groups,
        failures,
        review_pages,
        split_sources,
        scanned_pages,
    } = state;
    if cancelled() {
        return Err(BackendError::cancelled());
    }
    progress(ProgressEvent {
        phase: "scan_planning".to_string(),
        done: index as u64,
        total: total_pdfs as u64,
        subject: String::new(),
    });
    let inspection = if ocr.supports_page_scan() {
        Some(
            ocr.inspect_pdf(pdf)
                .unwrap_or_else(|| crate::pdf::inspect(pdf).map_err(|error| error.to_string())),
        )
    } else {
        None
    };
    let facts = match inspection {
        Some(Ok(facts)) => Some(facts),
        Some(Err(error)) => {
            failures.push((pdf.clone(), error));
            return Ok(None);
        }
        None => None,
    };
    let page_count = facts.as_ref().map_or(1, |facts| facts.page_count);
    *scanned_pages = scanned_pages.saturating_add(page_count);
    if *scanned_pages > MAX_SCAN_PAGES {
        return Err(BackendError::validation(format!(
            "scan source exceeds the {MAX_SCAN_PAGES}-page limit"
        )));
    }
    if page_count == 0 {
        failures.push((pdf.clone(), "PDF has no pages".to_string()));
        return Ok(None);
    }
    let mut rotations = Vec::new();
    let should_correct_orientation =
        params.scan_mode == ScanMode::MultiRfi && params.correct_orientation;
    let orientation_source_hash = if should_correct_orientation {
        Some(match source_sha256.and_then(|hashes| hashes.get(pdf)) {
            Some(expected) => expected.clone(),
            None => hash_pdf_checked(pdf)?,
        })
    } else {
        None
    };
    if should_correct_orientation {
        let orientation_results = (0..page_count)
            .map(|page_index| {
                let detected = ocr.detect_orientation(pdf, page_index)?;
                let current = facts
                    .as_ref()
                    .and_then(|facts| facts.pages.get(page_index))
                    .map_or(0, |page| page.rotation);
                let target = crate::operations::pdf_orientation::orientation_target_rotation(
                    current, &detected,
                )?;
                Ok((page_index, target, detected.confidence))
            })
            .collect::<Result<Vec<_>, String>>()
            .and_then(|results| {
                let rotations = results
                    .into_iter()
                    .filter_map(|(page, target, confidence)| {
                        target.map(|target| (page, target, confidence))
                    })
                    .collect::<Vec<_>>();
                if !rotations.is_empty()
                    && facts.as_ref().is_some_and(|facts| {
                        facts.signed || facts.encrypted || !crate::pdf::extraction_supported(facts)
                    })
                {
                    Err("PDF features prevent safe orientation correction".to_string())
                } else {
                    Ok(rotations)
                }
            });
        match orientation_results {
            Ok(detected_rotations) => {
                rotations = detected_rotations;
                if let Some(expected) = orientation_source_hash.as_deref()
                    && hash_pdf_checked(pdf)? != expected
                {
                    return Err(BackendError::path_invalid(
                        "scan source changed during orientation analysis",
                    ));
                }
            }
            Err(reason) => {
                plan.warnings.push(Warning {
                    key: "warn.rfi.processScansOrientationReview".to_string(),
                    subject: format!("{} — {reason}", display_name(pdf)),
                });
                let mut meta = JsonObject::new();
                meta.insert("reason".to_string(), Value::String(reason));
                push_review_step(plan, transfer_kind, destination, pdf, meta);
                return Ok(None);
            }
        }
    }
    if page_count > 1 && params.scan_mode == ScanMode::MultiRfi {
        if !facts.as_ref().is_some_and(crate::pdf::extraction_supported) {
            failures.push((
                pdf.clone(),
                "PDF features prevent safe page separation".to_string(),
            ));
            return Ok(None);
        }
        let source_hash = hash_pdf_checked(pdf)?;
        if orientation_source_hash
            .as_deref()
            .is_some_and(|expected| expected != source_hash.as_str())
        {
            return Err(BackendError::path_invalid(
                "scan source changed during Preview",
            ));
        }
        let Some(local_groups) = group_pages(ocr, pdf, page_count, fast_cover_mode, cancelled)?
        else {
            return Ok(Some(no_ocr_plan(operation_id, ocr)));
        };
        if hash_pdf_checked(pdf)? != source_hash {
            return Err(BackendError::path_invalid(
                "scan source changed during Preview",
            ));
        }
        split_sources.insert(pdf.clone(), source_hash.clone());
        for group in local_groups {
            if let Some(code) = group.code {
                let pages = group.pages;
                let group_rotations = rotations
                    .iter()
                    .filter(|(page, _, _)| pages.contains(page))
                    .copied()
                    .collect::<Vec<_>>();
                let candidate = Candidate {
                    path: pdf.clone(),
                    pages: Some(pages),
                    source_hash: Some(source_hash.clone()),
                    modified: modified_time(pdf),
                    confidence: group.confidence,
                    source: group.source,
                    rotations: group_rotations,
                };
                push_group_candidate(groups, code, candidate);
            } else {
                let group_rotations = rotations
                    .iter()
                    .filter(|(page, _, _)| group.pages.contains(page))
                    .copied()
                    .collect();
                plan.warnings.push(Warning {
                    key: "warn.no_code".to_string(),
                    subject: format!("{} — {}", display_name(pdf), page_range(&group.pages)),
                });
                review_pages.push(ReviewPages {
                    path: pdf.clone(),
                    pages: group.pages,
                    reason: group.reason,
                    rotations: group_rotations,
                });
            }
        }
        return Ok(None);
    }

    let read = match ocr.read_page(pdf, 0) {
        Ok(read) => read,
        Err(OcrFailure::EngineMissing) => return Ok(Some(no_ocr_plan(operation_id, ocr))),
        Err(OcrFailure::Document(error)) => {
            failures.push((pdf.clone(), error));
            return Ok(None);
        }
    };
    if !matches!(read.matched.method, RfiMethod::Strict | RfiMethod::OcrAlias)
        || read.matched.confidence < MIN_CONFIDENCE
    {
        plan.warnings.push(Warning {
            key: if read.matched.ok() {
                "warn.low_confidence".to_string()
            } else {
                "warn.no_code".to_string()
            },
            subject: display_name(pdf),
        });
        let mut meta = JsonObject::new();
        meta.insert(
            "confidence".to_string(),
            Value::from(read.matched.confidence),
        );
        meta.insert(
            "method".to_string(),
            Value::String(method_name(read.matched.method).to_string()),
        );
        meta.insert("source".to_string(), Value::String(read.source));
        push_review_step(plan, transfer_kind, destination, pdf, meta);
        return Ok(None);
    }

    let source_hash = (!rotations.is_empty())
        .then(|| orientation_source_hash.clone())
        .flatten();
    if let Some(expected) = source_hash.as_deref()
        && hash_pdf_checked(pdf)? != expected
    {
        return Err(BackendError::path_invalid(
            "scan source changed during Preview",
        ));
    }
    if let Some(source_hash) = source_hash.as_ref() {
        split_sources.insert(pdf.clone(), source_hash.clone());
    }
    let candidate = Candidate {
        path: pdf.clone(),
        pages: source_hash.as_ref().map(|_| vec![0]),
        source_hash,
        modified: modified_time(pdf),
        confidence: read.matched.confidence,
        source: read.source,
        rotations,
    };
    push_group_candidate(groups, read.matched.code, candidate);
    Ok(None)
}

/// Groups a multi-RFI PDF's pages by exact cover identity; `None` when no OCR engine is available.
fn group_pages<A, C>(
    ocr: &A,
    pdf: &Path,
    page_count: usize,
    fast_cover_mode: bool,
    cancelled: &mut C,
) -> Result<Option<Vec<PageGroup>>, BackendError>
where
    A: OcrAdapter,
    C: FnMut() -> bool,
{
    let mut local_groups = Vec::<PageGroup>::new();
    let mut active = None::<PageGroup>;
    let mut seen_codes = std::collections::BTreeSet::new();
    for page_index in 0..page_count {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        let page_read = if fast_cover_mode {
            ocr.read_cover_page(pdf, page_index)
        } else {
            ocr.read_page(pdf, page_index)
        };
        let read = match page_read {
            Ok(read) => read,
            Err(OcrFailure::EngineMissing) => return Ok(None),
            Err(OcrFailure::Document(error)) => {
                if let Some(group) = active.as_mut() {
                    group.pages.push(page_index);
                } else {
                    active = Some(PageGroup {
                        code: None,
                        pages: vec![page_index],
                        confidence: 0.0,
                        source: String::new(),
                        reason: format!("unreadable page: {error}"),
                    });
                }
                continue;
            }
        };
        // A strict, high-confidence RFI identity is the only group boundary.
        // Once a cover starts a group, every page stays with it until another
        // confirmed cover appears, even when continuation OCR is uncertain.
        if fast_cover_mode && read.source.starts_with("ocr:fast-cover:uncertain") {
            flush_page_group(&mut active, &mut local_groups);
            active = Some(PageGroup {
                code: None,
                pages: vec![page_index],
                confidence: read.matched.confidence,
                source: read.source,
                reason: "possible RFI cover could not be confirmed".to_string(),
            });
        } else if is_confirmed_cover(&read) {
            let code = read.matched.code.clone();
            let continues = active
                .as_ref()
                .is_some_and(|group| group.code.as_ref() == Some(&code));
            if continues {
                active
                    .as_mut()
                    .expect("current group exists")
                    .pages
                    .push(page_index);
                continue;
            }
            flush_page_group(&mut active, &mut local_groups);
            if !seen_codes.insert(code.clone()) {
                active = Some(PageGroup {
                    code: None,
                    pages: vec![page_index],
                    confidence: read.matched.confidence,
                    source: read.source,
                    reason: format!("duplicate RFI page for {code}"),
                });
            } else {
                active = Some(PageGroup {
                    code: Some(code),
                    pages: vec![page_index],
                    confidence: read.matched.confidence,
                    source: read.source,
                    reason: String::new(),
                });
            }
        } else if active.as_ref().is_some_and(|group| group.code.is_some()) {
            active
                .as_mut()
                .expect("current RFI group exists")
                .pages
                .push(page_index);
        } else if read.matched.method == RfiMethod::Miss
            && !read.source.starts_with("ocr:ambiguous-or-unconfirmed:")
        {
            if let Some(group) = active.as_mut() {
                group.pages.push(page_index);
            } else {
                active = Some(PageGroup {
                    code: None,
                    pages: vec![page_index],
                    confidence: 0.0,
                    source: read.source,
                    reason: "page precedes the first exact RFI identity".to_string(),
                });
            }
        } else {
            flush_page_group(&mut active, &mut local_groups);
            active = Some(PageGroup {
                code: None,
                pages: vec![page_index],
                confidence: read.matched.confidence,
                source: read.source,
                reason: format!(
                    "uncertain RFI evidence: {}",
                    method_name(read.matched.method)
                ),
            });
        }
    }
    flush_page_group(&mut active, &mut local_groups);
    Ok(Some(local_groups))
}

/// Plan returned when OCR is unavailable, carrying the engine diagnostics.
fn no_ocr_plan(operation_id: &str, ocr: &impl OcrAdapter) -> Plan {
    Plan {
        warnings: vec![Warning {
            key: "warn.no_ocr".to_string(),
            subject: ocr.diagnostics(),
        }],
        ..empty_plan(operation_id)
    }
}
