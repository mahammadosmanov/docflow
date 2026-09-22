use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use docflow_lib::{
    ocr::NativeOcr,
    operations::process_scans::{OcrAdapter, OcrFailure},
    rfi_code::{RfiMethod, find},
};
use rayon::prelude::*;
use serde::Serialize;

#[derive(Clone)]
struct Candidate {
    path: PathBuf,
    relative_path: PathBuf,
    expected: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditRow {
    relative_path: String,
    expected: String,
    observed: String,
    outcome: &'static str,
    evidence: String,
    elapsed_ms: u128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditReport {
    corpus_root: String,
    discovered: usize,
    audited: usize,
    reference_agreements: usize,
    reference_mismatches: usize,
    review: usize,
    errors: usize,
    automatic_reference_agreement: f64,
    reference_coverage: f64,
    rows: Vec<AuditRow>,
}

fn main() -> ExitCode {
    match run() {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(2),
        Err(error) => {
            eprintln!("OCR corpus audit failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<usize, String> {
    let mut args = env::args_os().skip(1);
    let root = args.next().map(PathBuf::from).ok_or_else(|| {
        "usage: ocr_corpus_audit <RFI System> [limit] [output.json] [workers]".to_string()
    })?;
    let limit = args
        .next()
        .map(|value| value.to_string_lossy().parse::<usize>())
        .transpose()
        .map_err(|_| "limit must be a non-negative integer".to_string())?
        .unwrap_or(0);
    let output = args.next().map(PathBuf::from);
    let workers = args
        .next()
        .map(|value| value.to_string_lossy().parse::<usize>())
        .transpose()
        .map_err(|_| "workers must be a positive integer".to_string())?
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(1, usize::from)
                .min(4)
        });
    if workers == 0 {
        return Err("workers must be greater than zero".to_string());
    }
    let root = root
        .canonicalize()
        .map_err(|error| format!("corpus root: {error}"))?;
    if !root.is_dir() {
        return Err("corpus root is not a directory".to_string());
    }
    if let Some(output) = output.as_ref() {
        let parent = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()
            .map_err(|error| format!("output directory: {error}"))?;
        if parent.starts_with(&root) {
            return Err("audit output must stay outside the read-only corpus".to_string());
        }
    }

    let mut candidates = Vec::new();
    for discipline in ["CIV", "MEC", "ELE", "PRC"] {
        discover(&root, &root.join(discipline), &mut candidates)?;
    }
    candidates.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let discovered = candidates.len();
    let selected = evenly_spaced(candidates, limit);
    let total = selected.len();
    if total == 0 {
        return Err("no exact RFI cover PDFs were found".to_string());
    }

    eprintln!("Auditing {total} of {discovered} exact RFI cover PDFs with {workers} workers");
    let completed = AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|error| format!("worker pool: {error}"))?;
    let ocr = env::var_os("DOCFLOW_OPERATOR_TESSERACT")
        .map(NativeOcr::with_tesseract)
        .unwrap_or_else(NativeOcr::discover);
    let mut rows = pool.install(|| {
        selected
            .par_iter()
            .map(|candidate| {
                let started = Instant::now();
                let read = ocr.read(&candidate.path);
                let elapsed_ms = started.elapsed().as_millis();
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                if done == total || done.is_multiple_of(10) {
                    eprintln!("Completed {done}/{total}");
                }
                match read {
                    Ok(read) if read.matched.code == candidate.expected => AuditRow {
                        relative_path: candidate.relative_path.to_string_lossy().into_owned(),
                        expected: candidate.expected.clone(),
                        observed: read.matched.code,
                        outcome: "reference-agreement",
                        evidence: read.source,
                        elapsed_ms,
                    },
                    Ok(read) if read.matched.method == RfiMethod::Miss => AuditRow {
                        relative_path: candidate.relative_path.to_string_lossy().into_owned(),
                        expected: candidate.expected.clone(),
                        observed: String::new(),
                        outcome: "review",
                        evidence: read.source,
                        elapsed_ms,
                    },
                    Ok(read) => AuditRow {
                        relative_path: candidate.relative_path.to_string_lossy().into_owned(),
                        expected: candidate.expected.clone(),
                        observed: read.matched.code,
                        outcome: "reference-mismatch",
                        evidence: read.source,
                        elapsed_ms,
                    },
                    Err(error) => AuditRow {
                        relative_path: candidate.relative_path.to_string_lossy().into_owned(),
                        expected: candidate.expected.clone(),
                        observed: String::new(),
                        outcome: "error",
                        evidence: describe_failure(error),
                        elapsed_ms,
                    },
                }
            })
            .collect::<Vec<_>>()
    });
    rows.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let agreements = rows
        .iter()
        .filter(|row| row.outcome == "reference-agreement")
        .count();
    let mismatches = rows
        .iter()
        .filter(|row| row.outcome == "reference-mismatch")
        .count();
    let review = rows.iter().filter(|row| row.outcome == "review").count();
    let errors = rows.iter().filter(|row| row.outcome == "error").count();
    let automatic = agreements + mismatches;
    let report = AuditReport {
        corpus_root: root.to_string_lossy().into_owned(),
        discovered,
        audited: total,
        reference_agreements: agreements,
        reference_mismatches: mismatches,
        review,
        errors,
        automatic_reference_agreement: ratio(agreements, automatic),
        reference_coverage: ratio(agreements, total),
        rows,
    };
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("serialize report: {error}"))?;
    if let Some(output) = output {
        fs::write(&output, format!("{json}\n"))
            .map_err(|error| format!("write {}: {error}", output.display()))?;
        eprintln!("Report: {}", output.display());
    } else {
        println!("{json}");
    }
    eprintln!(
        "Reference agreements={agreements} Mismatches={mismatches} Review={review} Errors={errors} Agreement={:.2}% Coverage={:.2}%",
        report.automatic_reference_agreement * 100.0,
        report.reference_coverage * 100.0
    );
    Ok(mismatches)
}

fn discover(root: &Path, directory: &Path, candidates: &mut Vec<Candidate>) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("read {} entry: {error}", directory.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("metadata {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            discover(root, &path, candidates)?;
            continue;
        }
        if !file_type.is_file()
            || !is_pdf(&path)
            || entry.file_name().to_string_lossy().starts_with('.')
        {
            continue;
        }
        let Some(parent_name) = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        let expected = find(parent_name);
        if expected.method != RfiMethod::Strict || expected.code != parent_name {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_uppercase();
        if !file_name.starts_with("RFI NO") || !file_name.contains(&expected.code) {
            continue;
        }
        candidates.push(Candidate {
            relative_path: path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
            path,
            expected: expected.code,
        });
    }
    Ok(())
}

fn evenly_spaced(candidates: Vec<Candidate>, limit: usize) -> Vec<Candidate> {
    if limit == 0 || limit >= candidates.len() {
        return candidates;
    }
    let count = candidates.len();
    (0..limit)
        .map(|index| candidates[index * count / limit].clone())
        .collect()
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

fn describe_failure(error: OcrFailure) -> String {
    match error {
        OcrFailure::EngineMissing => "OCR engine missing".to_string(),
        OcrFailure::Document(message) => message,
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_is_deterministic_and_spans_the_corpus() {
        let candidates = (0..10)
            .map(|number| Candidate {
                path: PathBuf::from(number.to_string()),
                relative_path: PathBuf::from(number.to_string()),
                expected: number.to_string(),
            })
            .collect();
        let selected = evenly_spaced(candidates, 3);
        assert_eq!(
            selected
                .iter()
                .map(|candidate| candidate.expected.as_str())
                .collect::<Vec<_>>(),
            ["0", "3", "6"]
        );
    }
}
