use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    naming::{flat_target_for, folder_for, target_for},
    parallel::bounded_map_stream,
    rfi_code::{RfiMatch, RfiMethod},
};

const MIN_CONFIDENCE: f64 = 0.8;
const REVIEW_DIR: &str = "_review";
const MAX_SCAN_PDFS: usize = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OcrFailure {
    EngineMissing,
    Document(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct OcrRead {
    pub matched: RfiMatch,
    pub source: String,
}

pub trait OcrAdapter: Sync {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure>;
    fn supports_page_scan(&self) -> bool {
        false
    }
    fn supports_page_previews(&self) -> bool {
        false
    }
    fn supports_parallel_analysis(&self) -> bool {
        false
    }
    fn read_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        if page_index == 0 {
            self.read(path)
        } else {
            Err(OcrFailure::Document(
                "page-level OCR is unavailable".to_string(),
            ))
        }
    }
    fn diagnostics(&self) -> String;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessScansParams {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub organize_into_rfi_folders: bool,
    pub keep_original: bool,
}

struct Candidate {
    path: PathBuf,
    modified: SystemTime,
    confidence: f64,
    source: String,
}

struct CachedFileOcr {
    reads: BTreeMap<PathBuf, Result<OcrRead, OcrFailure>>,
    diagnostics: String,
}

impl OcrAdapter for CachedFileOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        self.reads.get(path).cloned().unwrap_or_else(|| {
            Err(OcrFailure::Document(
                "parallel OCR cache entry is missing".to_string(),
            ))
        })
    }

    fn diagnostics(&self) -> String {
        self.diagnostics.clone()
    }
}

pub fn preview<A, P, C>(
    params: &ProcessScansParams,
    ocr: &A,
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
    let mut pdfs = fs::read_dir(&source)
        .map_err(|_| BackendError::path_invalid("scan source could not be read"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let metadata = entry.metadata().ok()?;
            (metadata.is_file()
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
    progress(ProgressEvent {
        phase: "scan_discovery".to_string(),
        done: pdfs.len() as u64,
        total: pdfs.len() as u64,
        subject: String::new(),
    });

    if ocr.supports_parallel_analysis() && !pdfs.is_empty() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        let mut completed = 0_u64;
        let total = pdfs.len() as u64;
        let (worker_count, results) = bounded_map_stream(
            &pdfs,
            |pdf| ocr.read(pdf),
            |index, _| {
                completed += 1;
                progress(ProgressEvent {
                    phase: "scan_ocr".to_string(),
                    done: completed,
                    total,
                    subject: pdfs[index].to_string_lossy().into_owned(),
                });
                Ok(())
            },
        )
        .map_err(BackendError::request_failed)?;
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        let cached = CachedFileOcr {
            reads: pdfs.iter().cloned().zip(results).collect(),
            diagnostics: ocr.diagnostics(),
        };
        let mut plan = preview(params, &cached, progress, cancelled)?;
        plan.meta
            .insert("workerCount".to_string(), Value::from(worker_count as u64));
        plan.meta
            .insert("parallelAnalysis".to_string(), Value::Bool(true));
        return Ok(plan);
    }

    let transfer_kind = if params.keep_original {
        StepKind::Copy
    } else {
        StepKind::Move
    };
    let mut plan = empty_plan();
    let mut groups: Vec<(String, Vec<Candidate>)> = Vec::new();
    let mut failures = Vec::<(PathBuf, String)>::new();
    for (index, pdf) in pdfs.iter().enumerate() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        progress(ProgressEvent {
            phase: "scan_planning".to_string(),
            done: index as u64,
            total: pdfs.len() as u64,
            subject: String::new(),
        });
        let read = match ocr.read(pdf) {
            Ok(read) => read,
            Err(OcrFailure::EngineMissing) => {
                return Ok(Plan {
                    warnings: vec![Warning {
                        key: "warn.no_ocr".to_string(),
                        subject: ocr.diagnostics(),
                    }],
                    ..empty_plan()
                });
            }
            Err(OcrFailure::Document(error)) => {
                failures.push((pdf.clone(), error));
                continue;
            }
        };
        if !read.matched.ok() || read.matched.confidence < MIN_CONFIDENCE {
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
            plan.steps.push(step(
                transfer_kind,
                Some(pdf),
                Some(&destination.join(REVIEW_DIR).join(display_name(pdf))),
                "note.needs_review",
                meta,
            ));
            continue;
        }

        let candidate = Candidate {
            path: pdf.clone(),
            modified: fs::metadata(pdf)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(UNIX_EPOCH),
            confidence: read.matched.confidence,
            source: read.source,
        };
        if let Some((_, files)) = groups
            .iter_mut()
            .find(|(code, _)| code == &read.matched.code)
        {
            files.push(candidate);
        } else {
            groups.push((read.matched.code, vec![candidate]));
        }
    }
    if pdfs.len() > 1 && failures.len() == pdfs.len() {
        return Ok(Plan {
            warnings: vec![Warning {
                key: "warn.ocr_broken".to_string(),
                subject: format!("{}  ||  {}", failures[0].1, ocr.diagnostics()),
            }],
            ..empty_plan()
        });
    }

    for (pdf, error) in failures {
        let truncated = error.chars().take(200).collect::<String>();
        plan.warnings.push(Warning {
            key: "warn.unreadable".to_string(),
            subject: format!("{} — {truncated}", display_name(&pdf)),
        });
        let mut meta = JsonObject::new();
        meta.insert("error".to_string(), Value::String(error));
        plan.steps.push(step(
            transfer_kind,
            Some(&pdf),
            Some(&destination.join(REVIEW_DIR).join(display_name(&pdf))),
            "note.unreadable",
            meta,
        ));
    }

    for (code, mut files) in groups {
        files.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
        let winner = files.remove(0);
        let folder = params
            .organize_into_rfi_folders
            .then(|| folder_for(&destination, &code));
        let target = if params.organize_into_rfi_folders {
            target_for(&destination, &code)
        } else {
            flat_target_for(&destination, &code)
        };
        if same_file(&winner.path, &target) {
            continue;
        }
        if target.exists() {
            plan.warnings.push(Warning {
                key: "warn.exists".to_string(),
                subject: display_name(&winner.path),
            });
            let mut meta = JsonObject::new();
            meta.insert("code".to_string(), Value::String(code.clone()));
            meta.insert("confidence".to_string(), Value::from(winner.confidence));
            meta.insert("source".to_string(), Value::String(winner.source.clone()));
            meta.insert(
                "reason".to_string(),
                Value::String("target_exists".to_string()),
            );
            meta.insert(
                "target".to_string(),
                Value::String(target.to_string_lossy().into_owned()),
            );
            plan.steps.push(step(
                transfer_kind,
                Some(&winner.path),
                Some(
                    &destination
                        .join(REVIEW_DIR)
                        .join(display_name(&winner.path)),
                ),
                "note.needs_review",
                meta,
            ));
        } else {
            let folder_created = folder.as_ref().is_some_and(|folder| !folder.is_dir());
            if let Some(folder) = folder.as_ref().filter(|folder| !folder.is_dir()) {
                let mut meta = JsonObject::new();
                meta.insert("code".to_string(), Value::String(code.clone()));
                plan.steps.push(step(
                    StepKind::Mkdir,
                    None,
                    Some(folder),
                    "note.folder_created",
                    meta,
                ));
            }
            let mut meta = JsonObject::new();
            meta.insert("code".to_string(), Value::String(code.clone()));
            meta.insert("confidence".to_string(), Value::from(winner.confidence));
            meta.insert("source".to_string(), Value::String(winner.source));
            meta.insert(
                "intoFolder".to_string(),
                Value::Bool(params.organize_into_rfi_folders),
            );
            meta.insert("folderCreated".to_string(), Value::Bool(folder_created));
            plan.steps.push(step(
                transfer_kind,
                Some(&winner.path),
                Some(&target),
                "note.renamed",
                meta,
            ));
        }

        for older in files {
            plan.warnings.push(Warning {
                key: "warn.duplicate".to_string(),
                subject: display_name(&older.path),
            });
            let mut meta = JsonObject::new();
            meta.insert("code".to_string(), Value::String(code.clone()));
            meta.insert("confidence".to_string(), Value::from(older.confidence));
            meta.insert("source".to_string(), Value::String(older.source.clone()));
            meta.insert(
                "reason".to_string(),
                Value::String("older_revision".to_string()),
            );
            meta.insert(
                "kept".to_string(),
                Value::String(display_name(&winner.path)),
            );
            plan.steps.push(step(
                transfer_kind,
                Some(&older.path),
                Some(&destination.join(REVIEW_DIR).join(display_name(&older.path))),
                "note.needs_review",
                meta,
            ));
        }
    }

    if pdfs.is_empty() {
        plan.warnings.push(Warning {
            key: "warn.no_pdfs".to_string(),
            subject: source.to_string_lossy().into_owned(),
        });
    }
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(plan)
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

fn empty_plan() -> Plan {
    Plan {
        plan_id: String::new(),
        op_id: "rfi.process_scans".to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
        meta: JsonObject::new(),
    }
}

fn step(
    kind: StepKind,
    source: Option<&Path>,
    destination: Option<&Path>,
    note_key: &str,
    meta: JsonObject,
) -> Step {
    Step {
        kind,
        src: source.map(|path| path.to_string_lossy().into_owned()),
        dst: destination.map(|path| path.to_string_lossy().into_owned()),
        note_key: Some(note_key.to_string()),
        meta,
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn method_name(method: RfiMethod) -> &'static str {
    match method {
        RfiMethod::Strict => "strict",
        RfiMethod::Repaired => "repaired",
        RfiMethod::Fuzzy => "fuzzy",
        RfiMethod::Miss => "miss",
    }
}

fn same_file(left: &Path, right: &Path) -> bool {
    if !right.exists() {
        return false;
    }
    fs::canonicalize(left)
        .and_then(|left| fs::canonicalize(right).map(|right| left == right))
        .unwrap_or(false)
}
