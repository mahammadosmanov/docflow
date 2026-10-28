use super::*;

pub(super) fn preview_with_cover_mode<A, P, C>(
    params: &ProcessScansParams,
    ocr: &A,
    operation_id: &str,
    fast_cover_mode: bool,
    source_sha256: Option<&BTreeMap<PathBuf, String>>,
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let source = canonical_directory(&params.source, "scan source")?;
    let destination = canonical_directory(&params.destination, "scan destination")?;
    progress(ProgressEvent {
        phase: "scan_discovery".to_string(),
        done: 0,
        total: 0,
        subject: source.to_string_lossy().into_owned(),
    });
    let pdfs = discover_pdfs(&source)?;
    progress(ProgressEvent {
        phase: "scan_discovery".to_string(),
        done: pdfs.len() as u64,
        total: pdfs.len() as u64,
        subject: String::new(),
    });

    if ocr.supports_parallel_analysis() && !pdfs.is_empty() {
        return preview_parallel(
            params,
            ocr,
            operation_id,
            fast_cover_mode,
            &pdfs,
            progress,
            cancelled,
        );
    }

    let transfer_kind = if params.keep_original {
        StepKind::Copy
    } else {
        StepKind::Move
    };
    let requires_existing_rfi_folder = !fast_cover_mode
        && params.organize_into_rfi_folders
        && destination_contains_exact_rfi_folder(&destination);
    let plan = empty_plan(operation_id);
    let groups: Vec<(String, Vec<Candidate>)> = Vec::new();
    let failures = Vec::<(PathBuf, String)>::new();
    let review_pages = Vec::<ReviewPages>::new();
    let split_sources = BTreeMap::<PathBuf, String>::new();
    let scanned_pages = 0_usize;
    let ctx = ScanContext {
        params,
        ocr,
        operation_id,
        source_sha256,
        destination: &destination,
        transfer_kind,
        total_pdfs: pdfs.len(),
        fast_cover_mode,
    };
    let mut state = ScanState {
        plan,
        groups,
        failures,
        review_pages,
        split_sources,
        scanned_pages,
    };
    for (index, pdf) in pdfs.iter().enumerate() {
        if let Some(early) =
            scan_one_pdf(&ctx, &mut state, index, pdf, &mut progress, &mut cancelled)?
        {
            return Ok(early);
        }
    }
    let ScanState {
        mut plan,
        groups,
        failures,
        review_pages,
        split_sources,
        ..
    } = state;
    if pdfs.len() > 1 && failures.len() == pdfs.len() {
        return Ok(Plan {
            warnings: vec![Warning {
                key: "warn.ocr_broken".to_string(),
                subject: format!("{}  ||  {}", failures[0].1, ocr.diagnostics()),
            }],
            ..empty_plan(operation_id)
        });
    }

    push_unreadable_outputs(&mut plan, failures, &destination, transfer_kind);

    push_review_outputs(
        &mut plan,
        review_pages,
        &split_sources,
        &destination,
        params.keep_original,
    );

    for (code, files) in groups {
        push_group_outputs(
            &mut plan,
            &destination,
            params,
            requires_existing_rfi_folder,
            code,
            files,
        );
    }

    if !params.keep_original && !split_sources.is_empty() {
        push_split_source_trash(&mut plan, params, split_sources)?;
    }

    if pdfs.is_empty() {
        plan.warnings.push(Warning {
            key: "warn.no_pdfs".to_string(),
            subject: source.to_string_lossy().into_owned(),
        });
    }
    attach_scan_editor(&mut plan, &destination, params.organize_into_rfi_folders);
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(plan)
}

pub(super) fn preview_parallel<A, P, C>(
    params: &ProcessScansParams,
    ocr: &A,
    operation_id: &str,
    fast_cover_mode: bool,
    pdfs: &[PathBuf],
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    if cancelled() {
        return Err(BackendError::cancelled());
    }
    let mut orientation_source_hashes = BTreeMap::new();
    if params.scan_mode == ScanMode::MultiRfi
        && params.correct_orientation
        && ocr.supports_orientation()
    {
        for pdf in pdfs {
            orientation_source_hashes.insert(pdf.clone(), hash_pdf_checked(pdf)?);
        }
    }
    let (inspection_workers, inspections) = if ocr.supports_page_scan() {
        inspect_pdfs(pdfs, &mut progress)?
    } else {
        (0, BTreeMap::new())
    };
    let (tasks, fast_cover_pdfs) =
        plan_ocr_tasks(pdfs, &inspections, params.scan_mode, fast_cover_mode)?;
    let (orientation_workers, orientations) = if params.scan_mode == ScanMode::MultiRfi
        && params.correct_orientation
        && ocr.supports_orientation()
    {
        detect_orientations(ocr, &tasks, &mut progress)?
    } else {
        (0, BTreeMap::new())
    };
    let mut completed = 0_u64;
    let (ocr_workers, results) = if tasks.is_empty() {
        (0, Vec::new())
    } else {
        bounded_map_stream(
            &tasks,
            |(pdf, page)| {
                if fast_cover_pdfs.contains(pdf) {
                    ocr.read_cover_page(pdf, *page)
                } else {
                    ocr.read_page(pdf, *page)
                }
            },
            |index, _| {
                completed += 1;
                let (pdf, page) = &tasks[index];
                progress(ProgressEvent {
                    phase: "scan_ocr".to_string(),
                    done: completed,
                    total: tasks.len() as u64,
                    subject: format!("{} — #{}", display_name(pdf), page + 1),
                });
                Ok(())
            },
        )
        .map_err(BackendError::request_failed)?
    };
    if cancelled() {
        return Err(BackendError::cancelled());
    }
    for (pdf, expected) in &orientation_source_hashes {
        if hash_pdf_checked(pdf)? != *expected {
            return Err(BackendError::path_invalid(format!(
                "scan source changed during orientation or OCR analysis: {}",
                display_name(pdf)
            )));
        }
    }
    let cached = CachedPageOcr {
        reads: tasks.into_iter().zip(results).collect(),
        inspections,
        page_scan: ocr.supports_page_scan(),
        diagnostics: ocr.diagnostics(),
        orientations,
    };
    let mut plan = preview_with_cover_mode(
        params,
        &cached,
        operation_id,
        fast_cover_mode,
        Some(&orientation_source_hashes),
        progress,
        cancelled,
    )?;
    plan.meta.insert(
        "workerCount".to_string(),
        Value::from(ocr_workers.max(inspection_workers) as u64),
    );
    plan.meta
        .insert("parallelAnalysis".to_string(), Value::Bool(true));
    plan.meta.insert(
        "orientationWorkerCount".to_string(),
        Value::from(orientation_workers as u64),
    );
    Ok(plan)
}
