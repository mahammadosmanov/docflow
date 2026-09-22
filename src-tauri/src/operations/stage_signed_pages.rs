use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    fs_ops::file_sha256,
    naming::filename_for,
    ocr::render_page_thumbnail_data_url,
    operations::process_scans::{OcrAdapter, OcrFailure},
    parallel::{bounded_map, bounded_map_stream},
    pdf::{PdfInspection, inspect},
    pdf_publish::{same_file_identity, temporary_pdf_name_matches},
    rfi_code::RfiMethod,
};

const OPERATION_ID: &str = "rfi.stage_signed_first_pages";
const MIN_CONFIDENCE: f64 = 0.8;
const MAX_SIGNED_PDFS: usize = 10_000;
const MAX_SIGNED_PAGES: usize = 20_000;
const MAX_ARCHIVE_ENTRIES: usize = 250_000;
const MAX_PAGE_COMPARISONS: usize = 20;
const MAX_PAGE_PREVIEW_BYTES: usize = 12 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageSignedPagesParams {
    pub signed_scans: PathBuf,
    pub archive_root: PathBuf,
    pub review_root: PathBuf,
    pub scan_subfolders: bool,
}

struct Candidate {
    path: PathBuf,
    page_index: usize,
    confidence: f64,
    method: String,
}

struct PreparedArchive {
    archive: PathBuf,
    relative: PathBuf,
    steps: Vec<Step>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WarningReviewReason {
    key: String,
    page_number: Option<usize>,
    code: Option<String>,
    detail: String,
}

struct CachedPageOcr {
    reads: BTreeMap<(PathBuf, usize), Result<super::process_scans::OcrRead, OcrFailure>>,
    page_previews: bool,
    diagnostics: String,
}

impl OcrAdapter for CachedPageOcr {
    fn read(&self, path: &Path) -> Result<super::process_scans::OcrRead, OcrFailure> {
        self.read_page(path, 0)
    }

    fn supports_page_scan(&self) -> bool {
        true
    }

    fn supports_page_previews(&self) -> bool {
        self.page_previews
    }

    fn read_page(
        &self,
        path: &Path,
        page_index: usize,
    ) -> Result<super::process_scans::OcrRead, OcrFailure> {
        self.reads
            .get(&(path.to_path_buf(), page_index))
            .cloned()
            .unwrap_or_else(|| {
                Err(OcrFailure::Document(
                    "parallel page OCR cache entry is missing".to_string(),
                ))
            })
    }

    fn diagnostics(&self) -> String {
        self.diagnostics.clone()
    }
}

pub fn preview<A, P, C>(
    params: &StageSignedPagesParams,
    ocr: &A,
    mut progress: P,
    mut cancelled: C,
) -> Result<Plan, BackendError>
where
    A: OcrAdapter,
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let signed_root = canonical_directory(&params.signed_scans, "signed scans")?;
    let archive_root = canonical_directory(&params.archive_root, "archive root")?;
    let review_root = canonical_directory(&params.review_root, "review root")?;
    if overlaps(&review_root, &archive_root) || overlaps(&review_root, &signed_root) {
        return Ok(Plan {
            warnings: vec![warning("error.rfi.reviewRootMustBeSeparate", "")],
            ..empty_plan()
        });
    }
    progress(ProgressEvent {
        phase: "signed_discovery".to_string(),
        done: 0,
        total: 0,
        subject: signed_root.to_string_lossy().into_owned(),
    });
    let (pdfs, discovery_workers) =
        signed_pdfs(&signed_root, params.scan_subfolders, &mut progress)?;
    progress(ProgressEvent {
        phase: "signed_discovery".to_string(),
        done: pdfs.len() as u64,
        total: pdfs.len() as u64,
        subject: signed_root.to_string_lossy().into_owned(),
    });
    let mut plan = empty_plan();
    let mut warning_reviews = BTreeMap::<PathBuf, Vec<WarningReviewReason>>::new();
    if pdfs.is_empty() {
        plan.warnings.push(warning(
            "warn.rfi.noSignedPdfs",
            &signed_root.to_string_lossy(),
        ));
        return Ok(plan);
    }

    if ocr.supports_parallel_analysis() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        let mut inspected = 0_u64;
        let (inspection_workers, inspections) = bounded_map_stream(
            &pdfs,
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
            |(pdf, page_index)| ocr.read_page(pdf, *page_index),
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
        let mut plan = preview(params, &cached, progress, cancelled)?;
        plan.meta.insert(
            "workerCount".to_string(),
            Value::from(worker_count.max(inspection_workers).max(discovery_workers) as u64),
        );
        plan.meta
            .insert("parallelAnalysis".to_string(), Value::Bool(true));
        return Ok(plan);
    }

    let mut groups = BTreeMap::<String, Vec<Candidate>>::new();
    let mut scanned_pages = 0_usize;
    for (index, pdf) in pdfs.iter().enumerate() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        progress(ProgressEvent {
            phase: "signed_classification".to_string(),
            done: index as u64,
            total: pdfs.len() as u64,
            subject: display_name(pdf),
        });
        let page_count = if ocr.supports_page_scan() {
            match inspect(pdf) {
                Ok(facts) if fidelity_supported(&facts, PdfRole::SignedScan) => facts.page_count,
                Ok(facts) => {
                    push_warning_review(
                        &mut plan,
                        &mut warning_reviews,
                        &signed_root,
                        "warn.rfi.pdfFidelityUnsupported",
                        pdf,
                        None,
                        None,
                        fidelity_reason(&facts, PdfRole::SignedScan),
                    );
                    continue;
                }
                Err(error) => {
                    push_warning_review(
                        &mut plan,
                        &mut warning_reviews,
                        &signed_root,
                        "warn.rfi.pdfFidelityUnsupported",
                        pdf,
                        None,
                        None,
                        &error.to_string(),
                    );
                    continue;
                }
            }
        } else {
            1
        };
        if page_count == 0 {
            push_warning_review(
                &mut plan,
                &mut warning_reviews,
                &signed_root,
                "warn.rfi.pdfFidelityUnsupported",
                pdf,
                None,
                None,
                "PDF has no pages",
            );
            continue;
        }
        scanned_pages = scanned_pages.saturating_add(page_count);
        if scanned_pages > MAX_SIGNED_PAGES {
            return Err(BackendError::validation(format!(
                "signed scans exceed the {MAX_SIGNED_PAGES}-page limit"
            )));
        }
        if page_count > 1 {
            plan.warnings.push(warning(
                "warn.rfi.signedMultiPage",
                &format!("{} — {page_count}", display_name(pdf)),
            ));
        }
        for page_index in 0..page_count {
            if cancelled() {
                return Err(BackendError::cancelled());
            }
            progress(ProgressEvent {
                phase: "signed_classification".to_string(),
                done: index as u64,
                total: pdfs.len() as u64,
                subject: candidate_display(pdf, page_index),
            });
            let read = match ocr.read_page(pdf, page_index) {
                Ok(read) => read,
                Err(OcrFailure::EngineMissing) => {
                    return Ok(Plan {
                        warnings: vec![warning("warn.no_ocr", &ocr.diagnostics())],
                        ..empty_plan()
                    });
                }
                Err(OcrFailure::Document(error)) => {
                    push_warning_review(
                        &mut plan,
                        &mut warning_reviews,
                        &signed_root,
                        "warn.rfi.signedUnreadable",
                        pdf,
                        Some(page_index),
                        None,
                        &error,
                    );
                    continue;
                }
            };
            if read.matched.method != RfiMethod::Strict {
                push_warning_review(
                    &mut plan,
                    &mut warning_reviews,
                    &signed_root,
                    "warn.no_code",
                    pdf,
                    Some(page_index),
                    None,
                    "No conflict-free exact canonical RFI code was recognized",
                );
                continue;
            }
            if read.matched.confidence < MIN_CONFIDENCE {
                let detail = format!(
                    "OCR score {:.0}% is below the 80% threshold",
                    read.matched.confidence * 100.0
                );
                push_warning_review(
                    &mut plan,
                    &mut warning_reviews,
                    &signed_root,
                    "warn.low_confidence",
                    pdf,
                    Some(page_index),
                    Some(&read.matched.code),
                    &detail,
                );
                continue;
            }
            groups
                .entry(read.matched.code)
                .or_default()
                .push(Candidate {
                    path: pdf.clone(),
                    page_index,
                    confidence: read.matched.confidence,
                    method: read.source,
                });
        }
    }

    let mut unique = BTreeMap::new();
    for (code, candidates) in groups {
        if candidates.len() != 1 {
            let names = candidates
                .iter()
                .map(|candidate| candidate_display(&candidate.path, candidate.page_index))
                .collect::<Vec<_>>()
                .join(", ");
            for candidate in &candidates {
                record_warning_review(
                    &mut warning_reviews,
                    &candidate.path,
                    Some(candidate.page_index),
                    Some(&code),
                    "warn.rfi.signedDuplicate",
                    "The same RFI code was recognized on more than one signed page",
                );
            }
            plan.warnings.push(warning(
                "warn.rfi.signedDuplicate",
                &truncate(&format!("{code} — {names} — duplicate signed pages"), 300),
            ));
        } else {
            unique.insert(code, candidates.into_iter().next().expect("one candidate"));
        }
    }

    let archive_scan = archive_index(
        &archive_root,
        unique.keys().cloned().collect(),
        &mut progress,
        &mut cancelled,
    )?;
    plan.meta
        .insert("archiveRecursive".to_string(), Value::Bool(true));
    plan.meta.insert(
        "archiveWorkerCount".to_string(),
        Value::from(archive_scan.worker_count as u64),
    );
    plan.meta.insert(
        "archiveDirectoriesScanned".to_string(),
        Value::from(archive_scan.directories_scanned as u64),
    );
    plan.meta.insert(
        "archiveEntriesScanned".to_string(),
        Value::from(archive_scan.entries_scanned as u64),
    );
    let archive_matches = archive_scan.matches;
    let mut editor_rows = Vec::new();
    let mut archive_choices = JsonObject::new();
    let mut archive_choice_labels = JsonObject::from_iter([(
        String::new(),
        Value::String("rfi.signed.archive.choose".to_string()),
    )]);
    let mut candidate_steps = JsonObject::new();
    let mut page_comparisons = JsonObject::new();
    let mut page_preview_bytes = 0_usize;
    let mut page_preview_omitted = 0_u64;
    let mut page_preview_rows_attempted = 0_usize;
    let comparison_total = unique.len() as u64;
    for (comparison_index, (code, candidate)) in unique.into_iter().enumerate() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        progress(ProgressEvent {
            phase: "comparison_preparation".to_string(),
            done: comparison_index as u64,
            total: comparison_total,
            subject: format!(
                "{} · {code}",
                candidate_display(&candidate.path, candidate.page_index)
            ),
        });
        let matches = archive_matches.get(&code).cloned().unwrap_or_default();
        if matches.is_empty() {
            push_warning_review(
                &mut plan,
                &mut warning_reviews,
                &signed_root,
                "warn.rfi.archiveMissing",
                &candidate.path,
                Some(candidate.page_index),
                Some(&code),
                "No exact archive PDF filename was found anywhere below the selected RFI System",
            );
            continue;
        }
        let signed_facts = match inspect(&candidate.path) {
            Ok(facts) if fidelity_supported(&facts, PdfRole::SignedScan) => facts,
            Ok(facts) => {
                push_warning_review(
                    &mut plan,
                    &mut warning_reviews,
                    &signed_root,
                    "warn.rfi.pdfFidelityUnsupported",
                    &candidate.path,
                    Some(candidate.page_index),
                    Some(&code),
                    fidelity_reason(&facts, PdfRole::SignedScan),
                );
                continue;
            }
            Err(error) => {
                push_warning_review(
                    &mut plan,
                    &mut warning_reviews,
                    &signed_root,
                    "warn.rfi.pdfFidelityUnsupported",
                    &candidate.path,
                    Some(candidate.page_index),
                    Some(&code),
                    &error.to_string(),
                );
                continue;
            }
        };
        if signed_facts.page_count == 0 {
            push_warning_review(
                &mut plan,
                &mut warning_reviews,
                &signed_root,
                "warn.rfi.pdfFidelityUnsupported",
                &candidate.path,
                Some(candidate.page_index),
                Some(&code),
                "PDF has no pages",
            );
            continue;
        }
        if signed_facts
            .pages
            .get(candidate.page_index)
            .is_none_or(|page| page.widgets > 0)
        {
            push_warning_review(
                &mut plan,
                &mut warning_reviews,
                &signed_root,
                "warn.rfi.pdfFidelityUnsupported",
                &candidate.path,
                Some(candidate.page_index),
                Some(&code),
                "The signed page contains form widgets and requires manual review",
            );
            continue;
        }
        let signed_hash = file_sha256(&candidate.path)
            .map_err(|_| BackendError::path_invalid("signed PDF could not be hashed"))?;
        let row_id = format!("signed-{code}");
        let candidate_warning_start = plan.warnings.len();
        let mut prepared = Vec::new();
        for archive_pdf in &matches {
            if let Some(candidate_plan) = prepare_archive_candidate(
                &code,
                &row_id,
                &candidate,
                archive_pdf,
                &signed_facts,
                &signed_hash,
                &signed_root,
                &archive_root,
                &review_root,
                &mut plan,
            )? {
                prepared.push(candidate_plan);
            }
        }
        if prepared.is_empty() {
            if let Some(source_warning) = plan.warnings[candidate_warning_start..]
                .iter()
                .find(|warning| warning.key != "warn.rfi.staleTempFound")
            {
                record_warning_review(
                    &mut warning_reviews,
                    &candidate.path,
                    Some(candidate.page_index),
                    Some(&code),
                    &source_warning.key,
                    "No archive candidate passed all fidelity, destination, and safety checks",
                );
            }
            continue;
        }
        if signed_facts.page_count > 1 && !ocr.supports_page_scan() {
            push_warning_review(
                &mut plan,
                &mut warning_reviews,
                &signed_root,
                "warn.rfi.signedExtraPages",
                &candidate.path,
                Some(candidate.page_index),
                Some(&code),
                "Additional signed-scan pages could not be independently identified",
            );
        }
        let ambiguous = prepared.len() > 1;
        if ambiguous {
            let names = prepared
                .iter()
                .map(|item| item.relative.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(", ");
            plan.warnings.push(warning(
                "warn.rfi.archiveAmbiguous",
                &truncate(
                    &format!(
                        "{} — {code} — {names} — choose the correct archive candidate",
                        candidate_display(&candidate.path, candidate.page_index)
                    ),
                    300,
                ),
            ));
            record_warning_review(
                &mut warning_reviews,
                &candidate.path,
                Some(candidate.page_index),
                Some(&code),
                "warn.rfi.archiveAmbiguous",
                "More than one exact archive PDF exists; choose the correct candidate",
            );
        }
        let mut selected = String::new();
        let mut choices = Vec::with_capacity(prepared.len() + usize::from(ambiguous));
        if ambiguous {
            choices.push(Value::String(String::new()));
        }
        let mut row_candidates = JsonObject::new();
        let preview_allowed =
            ocr.supports_page_previews() && page_preview_rows_attempted < MAX_PAGE_COMPARISONS;
        if preview_allowed {
            page_preview_rows_attempted += 1;
        }
        let signed_preview = preview_allowed
            .then(|| render_page_thumbnail_data_url(&candidate.path, candidate.page_index))
            .and_then(Result::ok)
            .and_then(|value| accept_page_preview(value, &mut page_preview_bytes));
        let mut archive_previews = JsonObject::new();
        for (index, item) in prepared.iter().enumerate() {
            let token = format!("{row_id}-candidate-{}", index + 1);
            let relative = item.relative.to_string_lossy().into_owned();
            if !ambiguous && index == 0 {
                selected.clone_from(&token);
            }
            choices.push(Value::String(token.clone()));
            archive_choice_labels.insert(token.clone(), Value::String(relative));
            if preview_allowed
                && let Some(value) = render_page_thumbnail_data_url(&item.archive, 0)
                    .ok()
                    .and_then(|value| accept_page_preview(value, &mut page_preview_bytes))
            {
                archive_previews.insert(token.clone(), Value::String(value));
            }
            row_candidates.insert(
                token,
                serde_json::to_value(&item.steps).expect("candidate steps serialize"),
            );
        }
        if !ambiguous {
            extend_candidate_steps(&mut plan.steps, &prepared[0].steps);
        }
        archive_choices.insert(code.clone(), Value::Array(choices));
        candidate_steps.insert(row_id.clone(), Value::Object(row_candidates));
        if signed_preview.is_some() || !archive_previews.is_empty() {
            page_comparisons.insert(
                row_id.clone(),
                json!({
                    "signed": signed_preview,
                    "archives": archive_previews,
                }),
            );
        } else {
            page_preview_omitted += 1;
        }
        editor_rows.push(json!({
            "_rowId": row_id,
            "enabled": true,
            "code": code,
            "confidence": candidate.confidence,
            "signedFile": candidate_display(&candidate.path, candidate.page_index),
            "archiveFile": selected,
        }));
    }
    add_warning_review_steps(&mut plan, &warning_reviews, &signed_root, &review_root)?;
    plan.meta.insert(
        "archiveCandidateSteps".to_string(),
        Value::Object(candidate_steps),
    );
    plan.meta.insert(
        "pageComparisons".to_string(),
        Value::Object(page_comparisons),
    );
    plan.meta.insert(
        "pagePreviewOmitted".to_string(),
        Value::from(page_preview_omitted),
    );
    plan.meta.insert(
        "editor".to_string(),
        json!({
            "titleKey": "rfi.signed.editor.title",
            "rowCountKey": "rfi.signed.editor.rowCount",
            "groupField": "code",
            "selectionEnabled": false,
            "bulkEnabled": false,
            "duplicateEnabled": false,
            "requiredFields": ["archiveFile"],
            "choicesByValue": {
                "archiveFile": {
                    "dependsOn": "code",
                    "values": archive_choices,
                }
            },
            "choiceLabels": {
                "archiveFile": archive_choice_labels
            },
            "columns": [
                {"key": "enabled", "labelKey": "rfi.signed.column.enabled", "kind": "bool"},
                {"key": "code", "labelKey": "rfi.signed.column.code", "kind": "readonly"},
                {"key": "confidence", "labelKey": "ocr.confidence", "kind": "readonly", "display": "percent", "width": 112},
                {"key": "signedFile", "labelKey": "rfi.signed.column.scan", "kind": "readonly"},
                {"key": "archiveFile", "labelKey": "rfi.signed.column.archive", "kind": "choice", "bulk": false}
            ],
            "rows": editor_rows,
        }),
    );
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: pdfs.len() as u64,
        total: pdfs.len() as u64,
        subject: String::new(),
    });
    Ok(plan)
}

pub fn revise(plan: &Plan, payload: &Value) -> Result<Plan, BackendError> {
    if plan.op_id != OPERATION_ID {
        return Err(BackendError::validation(
            "signed-page plan identity is invalid",
        ));
    }
    let existing_rows = plan
        .meta
        .get("editor")
        .and_then(Value::as_object)
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("signed-page review rows are missing"))?;
    let supplied_rows = payload
        .as_object()
        .and_then(|object| object.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("signed-page review rows are invalid"))?;
    if existing_rows.len() != supplied_rows.len() {
        return Err(BackendError::validation(
            "signed-page review rows cannot be added or removed",
        ));
    }
    let existing_ids = existing_rows
        .iter()
        .filter_map(|row| row.get("_rowId").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let supplied_ids = supplied_rows
        .iter()
        .filter_map(|row| row.get("_rowId").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    if existing_ids.len() != existing_rows.len() || supplied_ids != existing_ids {
        return Err(BackendError::validation(
            "signed-page review row identity is invalid",
        ));
    }
    let cached_candidates = plan
        .meta
        .get("archiveCandidateSteps")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("signed-page archive candidates are missing"))?;
    let mut steps = plan
        .steps
        .iter()
        .filter(|step| {
            step.kind == StepKind::CleanupTemp
                || step.meta.get("warningReview").and_then(Value::as_bool) == Some(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut meta = plan.meta.clone();
    let mut revised_rows = existing_rows.clone();
    for row in &mut revised_rows {
        let row_id = row
            .get("_rowId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let supplied = supplied_rows
            .iter()
            .find(|candidate| {
                candidate.get("_rowId").and_then(Value::as_str) == Some(row_id.as_str())
            })
            .expect("validated row identities");
        let enabled = supplied
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| BackendError::validation("signed-page row state is invalid"))?;
        let selected = supplied
            .get("archiveFile")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation("signed-page archive selection is invalid"))?;
        let choices = cached_candidates
            .get(&row_id)
            .and_then(Value::as_object)
            .ok_or_else(|| BackendError::validation("signed-page archive choices are invalid"))?;
        if !selected.is_empty() && !choices.contains_key(selected) {
            return Err(BackendError::validation(
                "signed-page archive selection was not offered by preview",
            ));
        }
        if enabled {
            if selected.is_empty() {
                return Err(BackendError::validation(
                    "signed-page archive selection is required",
                ));
            }
            let selected_steps: Vec<Step> = serde_json::from_value(
                choices
                    .get(selected)
                    .expect("selected candidate is allowlisted")
                    .clone(),
            )
            .map_err(|_| BackendError::validation("signed-page candidate plan is invalid"))?;
            if selected_steps.iter().any(|step| {
                !matches!(step.kind, StepKind::Mkdir | StepKind::PdfFirstPageReplace)
                    || (step.kind == StepKind::PdfFirstPageReplace
                        && step.meta.get("rowId").and_then(Value::as_str) != Some(row_id.as_str()))
            }) {
                return Err(BackendError::validation(
                    "signed-page candidate plan contains an invalid step",
                ));
            }
            extend_candidate_steps(&mut steps, &selected_steps);
        }
        let revised = row.as_object_mut().expect("validated editor row");
        revised.insert("enabled".to_string(), Value::Bool(enabled));
        revised.insert(
            "archiveFile".to_string(),
            Value::String(selected.to_string()),
        );
    }
    meta.get_mut("editor")
        .and_then(Value::as_object_mut)
        .expect("validated editor")
        .insert("rows".to_string(), Value::Array(revised_rows));
    Ok(Plan {
        plan_id: String::new(),
        op_id: plan.op_id.clone(),
        steps,
        warnings: plan.warnings.clone(),
        meta,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_archive_candidate(
    code: &str,
    row_id: &str,
    candidate: &Candidate,
    archive_pdf: &Path,
    signed_facts: &PdfInspection,
    signed_hash: &str,
    signed_root: &Path,
    archive_root: &Path,
    review_root: &Path,
    plan: &mut Plan,
) -> Result<Option<PreparedArchive>, BackendError> {
    let relative = archive_pdf
        .strip_prefix(archive_root)
        .map_err(|_| BackendError::path_invalid("archive match escaped the selected root"))?
        .to_path_buf();
    let destination = review_root.join(&relative);
    if destination.exists() {
        let stale_temps = stale_hardlink_temps(&destination, review_root);
        let destination_hash = if stale_temps.is_empty() {
            None
        } else {
            file_sha256(&destination).ok()
        };
        if let Some(destination_hash) = destination_hash {
            for stale_temp in stale_temps {
                let mut meta = JsonObject::new();
                meta.insert(
                    "reviewRoot".to_string(),
                    Value::String(review_root.to_string_lossy().into_owned()),
                );
                meta.insert(
                    "destinationSha256".to_string(),
                    Value::String(destination_hash.clone()),
                );
                plan.steps.push(Step {
                    kind: StepKind::CleanupTemp,
                    src: Some(stale_temp.to_string_lossy().into_owned()),
                    dst: Some(destination.to_string_lossy().into_owned()),
                    note_key: Some("note.rfi.cleanupStaleTemp".to_string()),
                    meta,
                });
            }
            plan.warnings.push(warning(
                "warn.rfi.staleTempFound",
                &relative.to_string_lossy(),
            ));
        } else {
            plan.warnings.push(warning(
                "warn.rfi.reviewExists",
                &relative.to_string_lossy(),
            ));
        }
        return Ok(None);
    }
    if destination
        .ancestors()
        .take_while(|path| *path != review_root)
        .any(|path| {
            fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        })
    {
        plan.warnings.push(warning(
            "warn.rfi.reviewPathBlocked",
            &relative.to_string_lossy(),
        ));
        return Ok(None);
    }
    let archive_facts = match inspect(archive_pdf) {
        Ok(facts) if fidelity_supported(&facts, PdfRole::Archive) => facts,
        Ok(facts) => {
            plan.warnings.push(warning(
                "warn.rfi.pdfFidelityUnsupported",
                &truncate(
                    &format!("{code} — {}", fidelity_reason(&facts, PdfRole::Archive)),
                    300,
                ),
            ));
            return Ok(None);
        }
        Err(error) => {
            plan.warnings.push(warning(
                "warn.rfi.pdfFidelityUnsupported",
                &truncate(&format!("{code} — {error}"), 300),
            ));
            return Ok(None);
        }
    };
    if archive_facts.page_count == 0 {
        plan.warnings.push(warning(
            "warn.rfi.pdfFidelityUnsupported",
            &format!("{code} — PDF has no pages"),
        ));
        return Ok(None);
    }
    if archive_facts.pages[0].widgets > 0 {
        plan.warnings.push(warning(
            "warn.rfi.pdfFidelityUnsupported",
            &format!("{code} — page-one form widgets require manual review"),
        ));
        return Ok(None);
    }
    let archive_hash = file_sha256(archive_pdf)
        .map_err(|_| BackendError::path_invalid("archive PDF could not be hashed"))?;
    let mut steps = Vec::new();
    add_directory_steps(
        destination
            .parent()
            .ok_or_else(|| BackendError::path_invalid("review destination has no parent"))?,
        review_root,
        &mut BTreeSet::new(),
        &mut steps,
    )?;
    let mut meta = JsonObject::new();
    meta.insert("code".to_string(), Value::String(code.to_string()));
    meta.insert(
        "signedPdf".to_string(),
        Value::String(candidate.path.to_string_lossy().into_owned()),
    );
    meta.insert(
        "signedRoot".to_string(),
        Value::String(signed_root.to_string_lossy().into_owned()),
    );
    meta.insert(
        "archiveRoot".to_string(),
        Value::String(archive_root.to_string_lossy().into_owned()),
    );
    meta.insert(
        "reviewRoot".to_string(),
        Value::String(review_root.to_string_lossy().into_owned()),
    );
    meta.insert(
        "relativePath".to_string(),
        Value::String(relative.to_string_lossy().into_owned()),
    );
    meta.insert("confidence".to_string(), Value::from(candidate.confidence));
    meta.insert(
        "method".to_string(),
        Value::String(candidate.method.clone()),
    );
    meta.insert("archiveSha256".to_string(), Value::String(archive_hash));
    meta.insert(
        "signedSha256".to_string(),
        Value::String(signed_hash.to_string()),
    );
    meta.insert(
        "archivePageCount".to_string(),
        Value::from(archive_facts.page_count as u64),
    );
    meta.insert(
        "signedPageCount".to_string(),
        Value::from(signed_facts.page_count as u64),
    );
    meta.insert(
        "signedPageIndex".to_string(),
        Value::from(candidate.page_index as u64),
    );
    meta.insert("rowId".to_string(), Value::String(row_id.to_string()));
    steps.push(Step {
        kind: StepKind::PdfFirstPageReplace,
        src: Some(archive_pdf.to_string_lossy().into_owned()),
        dst: Some(destination.to_string_lossy().into_owned()),
        note_key: Some("note.rfi.signedFirstPageStaged".to_string()),
        meta,
    });
    Ok(Some(PreparedArchive {
        archive: archive_pdf.to_path_buf(),
        relative,
        steps,
    }))
}

fn accept_page_preview(value: String, used: &mut usize) -> Option<String> {
    let next = used.checked_add(value.len())?;
    if next > MAX_PAGE_PREVIEW_BYTES {
        return None;
    }
    *used = next;
    Some(value)
}

fn extend_candidate_steps(target: &mut Vec<Step>, candidate: &[Step]) {
    for step in candidate {
        if step.kind == StepKind::Mkdir
            && target.iter().any(|existing| {
                existing.kind == StepKind::Mkdir && existing.dst.as_deref() == step.dst.as_deref()
            })
        {
            continue;
        }
        target.push(step.clone());
    }
}

#[allow(clippy::too_many_arguments)]
fn push_warning_review(
    plan: &mut Plan,
    reviews: &mut BTreeMap<PathBuf, Vec<WarningReviewReason>>,
    signed_root: &Path,
    key: &str,
    source: &Path,
    page_index: Option<usize>,
    code: Option<&str>,
    detail: &str,
) {
    record_warning_review(reviews, source, page_index, code, key, detail);
    let relative = source.strip_prefix(signed_root).unwrap_or(source);
    let mut context = vec![relative.to_string_lossy().into_owned()];
    if let Some(page_index) = page_index {
        context.push(format!("page {} (#{})", page_index + 1, page_index + 1));
    }
    if let Some(code) = code {
        context.push(code.to_string());
    }
    if !detail.is_empty() {
        context.push(detail.to_string());
    }
    plan.warnings
        .push(warning(key, &truncate(&context.join(" · "), 500)));
}

fn record_warning_review(
    reviews: &mut BTreeMap<PathBuf, Vec<WarningReviewReason>>,
    source: &Path,
    page_index: Option<usize>,
    code: Option<&str>,
    key: &str,
    detail: &str,
) {
    let reason = WarningReviewReason {
        key: key.to_string(),
        page_number: page_index.map(|index| index + 1),
        code: code.map(str::to_string),
        detail: truncate(detail, 300),
    };
    let reasons = reviews.entry(source.to_path_buf()).or_default();
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn add_warning_review_steps(
    plan: &mut Plan,
    reviews: &BTreeMap<PathBuf, Vec<WarningReviewReason>>,
    signed_root: &Path,
    review_root: &Path,
) -> Result<(), BackendError> {
    let warning_root = review_root.join("_review");
    let mut items = Vec::new();
    let mut planned_directories = BTreeSet::new();
    for (source, reasons) in reviews {
        let relative = source.strip_prefix(signed_root).map_err(|_| {
            BackendError::path_invalid("warning review source escaped signed scans")
        })?;
        let destination = warning_root.join(relative);
        let reason_values = reasons
            .iter()
            .map(|reason| {
                json!({
                    "key": reason.key,
                    "pageNumber": reason.page_number,
                    "code": reason.code,
                    "detail": reason.detail,
                })
            })
            .collect::<Vec<_>>();
        let mut status = "planned";
        if destination.exists() {
            status = "exists";
            plan.warnings.push(warning(
                "warn.rfi.warningReviewExists",
                &relative.to_string_lossy(),
            ));
        } else if review_destination_blocked(&destination, review_root) {
            status = "blocked";
            plan.warnings.push(warning(
                "warn.rfi.warningReviewBlocked",
                &relative.to_string_lossy(),
            ));
        } else {
            let mut review_steps = Vec::new();
            add_directory_steps(
                destination.parent().ok_or_else(|| {
                    BackendError::path_invalid("warning review destination has no parent")
                })?,
                review_root,
                &mut planned_directories,
                &mut review_steps,
            )?;
            for step in &mut review_steps {
                step.meta
                    .insert("warningReview".to_string(), Value::Bool(true));
            }
            extend_candidate_steps(&mut plan.steps, &review_steps);
            let source_hash = file_sha256(source).map_err(|_| {
                BackendError::path_invalid("warning review PDF could not be hashed")
            })?;
            plan.steps.push(Step {
                kind: StepKind::Copy,
                src: Some(source.to_string_lossy().into_owned()),
                dst: Some(destination.to_string_lossy().into_owned()),
                note_key: Some("note.rfi.signedWarningReviewCopy".to_string()),
                meta: JsonObject::from_iter([
                    ("warningReview".to_string(), Value::Bool(true)),
                    (
                        "expectedSourceSha256".to_string(),
                        Value::String(source_hash),
                    ),
                    ("reasons".to_string(), Value::Array(reason_values.clone())),
                ]),
            });
        }
        items.push(json!({
            "sourceFile": relative.to_string_lossy(),
            "destination": destination.strip_prefix(review_root).unwrap_or(&destination).to_string_lossy(),
            "status": status,
            "reasons": reason_values,
        }));
    }
    plan.meta
        .insert("warningReviewItems".to_string(), Value::Array(items));
    Ok(())
}

fn review_destination_blocked(destination: &Path, review_root: &Path) -> bool {
    destination
        .ancestors()
        .skip(1)
        .take_while(|path| *path != review_root)
        .any(|path| match fs::symlink_metadata(path) {
            Ok(metadata) => !metadata.is_dir() || metadata.file_type().is_symlink(),
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        })
}

fn empty_plan() -> Plan {
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

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BackendError> {
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

fn overlaps(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

fn stale_hardlink_temps(destination: &Path, review_root: &Path) -> Vec<PathBuf> {
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

fn signed_pdfs<P>(
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

struct SignedDirectoryBatch {
    directories: Vec<PathBuf>,
    files: Vec<PathBuf>,
}

fn scan_signed_directory(
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
        let metadata = entry
            .metadata()
            .map_err(|_| BackendError::path_invalid("signed scan metadata could not be read"))?;
        let name = entry.file_name();
        let hidden = name.to_str().is_some_and(|name| name.starts_with('.'));
        if metadata.is_file()
            && !hidden
            && entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
        {
            batch.files.push(entry.path());
        } else if recursive && metadata.is_dir() {
            batch.directories.push(entry.path());
        }
    }
    Ok(batch)
}

struct ArchiveIndexResult {
    matches: BTreeMap<String, Vec<PathBuf>>,
    worker_count: usize,
    directories_scanned: usize,
    entries_scanned: usize,
}

struct ArchiveDirectoryBatch {
    directories: Vec<PathBuf>,
    matches: Vec<(String, PathBuf)>,
    entries_scanned: usize,
}

fn archive_index<P, C>(
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
        .map(|code| (filename_for(code).to_ascii_lowercase(), code.clone()))
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

fn scan_archive_directory(
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
        let metadata = entry
            .metadata()
            .map_err(|_| BackendError::path_invalid("archive metadata could not be read"))?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            batch.directories.push(entry.path());
            continue;
        }
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_ascii_lowercase) else {
            continue;
        };
        let Some(code) = wanted.get(&name) else {
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

fn add_directory_steps(
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

#[derive(Clone, Copy)]
enum PdfRole {
    Archive,
    SignedScan,
}

fn fidelity_supported(facts: &PdfInspection, role: PdfRole) -> bool {
    !facts.encrypted
        && !facts.signed
        && !facts.catalog_entry_hashes.contains_key("StructTreeRoot")
        && (!matches!(role, PdfRole::SignedScan)
            || !facts.catalog_entry_hashes.contains_key("OCProperties"))
}

fn fidelity_reason(facts: &PdfInspection, role: PdfRole) -> &'static str {
    if facts.encrypted {
        "encrypted PDF"
    } else if facts.signed {
        "digitally signed PDF"
    } else if facts.catalog_entry_hashes.contains_key("StructTreeRoot") {
        "tagged PDF page structure cannot be safely preserved"
    } else if matches!(role, PdfRole::SignedScan)
        && facts.catalog_entry_hashes.contains_key("OCProperties")
    {
        "signed page depends on document-level optional-content state"
    } else {
        "unsupported PDF fidelity case"
    }
}

fn warning(key: &str, subject: &str) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.to_string(),
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn candidate_display(path: &Path, page_index: usize) -> String {
    format!("{} — #{}", display_name(path), page_index + 1)
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
