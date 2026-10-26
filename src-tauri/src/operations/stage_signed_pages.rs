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
    ocr::render_page_thumbnail_data_url,
    operations::process_scans::{OcrAdapter, OcrFailure},
    parallel::{bounded_map, bounded_map_stream},
    pdf::{PdfInspection, inspect},
    pdf_publish::{same_file_identity, temporary_pdf_name_matches},
    rfi_code::{PREFIX, RfiMatch, RfiMethod, normalize_candidate_number, normalize_full_code},
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
            let read = match ocr.read_identity_page(pdf, page_index) {
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
            let Some(code) = canonical_stage_code(&read.matched) else {
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
            };
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
                    Some(&code),
                    &detail,
                );
                continue;
            }
            groups.entry(code).or_default().push(Candidate {
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
            let signed = SignedSource {
                code: &code,
                row_id: &row_id,
                facts: &signed_facts,
                hash: &signed_hash,
                root: &signed_root,
            };
            if let Some(candidate_plan) = prepare_archive_candidate(
                &signed,
                &candidate,
                archive_pdf,
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

mod revise;
mod scan;
mod support;
mod warnings;
pub use revise::*;
use scan::*;
use support::*;
use warnings::*;
