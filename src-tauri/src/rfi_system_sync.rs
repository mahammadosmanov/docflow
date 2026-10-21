//! RFI-system scanning and exact, guarded workbook synchronization.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom},
    path::{Path, PathBuf},
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    fs_ops::file_sha256,
    naming::FILE_PREFIX,
    ooxml::{
        CellData, CellUpdate, GuardedCellUpdate, WorkbookData, WorksheetData,
        patch_workbook_sheets_guarded_text, read_workbook_from_reader,
        verify_guarded_workbook_delta_text,
    },
    parallel::{bounded_map, bounded_map_stream},
    rfi_code::normalize_candidate_number,
    rfi_follow_up::{WorkbookFingerprint, fingerprint_open_file},
    scan_index::{FileFingerprint, ScanIndex, ScanIndexStats},
};

pub const OPERATION_ID: &str = "rfi.system_sync";
pub const COMPLETE_STATUS_OPERATION_ID: &str = "rfi.complete_status_sync";
pub const WORLEY_SIGNED_MARKER: &str = "Signed by Worley";
pub const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
const MAX_HEADER_ROWS: u32 = 32;
const MAX_HEADER_COLUMNS: u32 = 64;
const SYSTEM_SCAN_PROFILE: &str = "worley-first-page-v2-others-only";
const MAX_COMPLETION_ENTRIES: usize = 250_000;
const MAX_COMPLETION_WARNINGS: usize = 100;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[doc(hidden)]
pub struct ScannedRfiRecord {
    pub discipline: String,
    pub number: u32,
    pub canonical_rfi: String,
    pub short_rfi: String,
    pub rfi_folder: String,
    pub pdf_path: Option<String>,
    pub signed_by_worley: bool,
    pub signature_reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedWorleyDetection {
    signed_by_worley: bool,
    signature_reason: String,
    source_sha256: Option<String>,
}

#[derive(Clone, Debug)]
struct CandidateAnalysis {
    record: ScannedRfiRecord,
    relative: Option<PathBuf>,
    fingerprint: Option<FileFingerprint>,
    detection: Option<CachedWorleyDetection>,
    reused: bool,
    transient_failure: bool,
    subject: String,
}

#[derive(Clone, Debug)]
struct CandidateAnalysisError {
    relative: Option<PathBuf>,
    message: String,
}

impl CandidateAnalysisError {
    fn new(relative: Option<&Path>, message: impl Into<String>) -> Self {
        Self {
            relative: relative.map(Path::to_path_buf),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemSyncRow {
    #[serde(rename = "_rowId")]
    pub row_id: String,
    pub discipline: String,
    pub rfi_no: String,
    pub canonical_rfi: String,
    pub worley_status: String,
    pub rfi_log_action: String,
    pub follow_up_action: String,
    pub disposition: String,
    pub reason_key: String,
    pub enabled: bool,
    pub selectable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SyncWorkbookKind {
    RfiLog,
    RfiLogStatus,
    FollowUp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SystemSyncStepMeta {
    pub target_path: String,
    pub backup_path: String,
    pub source_fingerprint: WorkbookFingerprint,
    pub workbook_kind: SyncWorkbookKind,
    pub updates: BTreeMap<String, BTreeMap<String, GuardedCellUpdateDto>>,
    #[serde(default)]
    pub canonical_rfi_codes: Vec<String>,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GuardedCellUpdateDto {
    pub row_id: String,
    pub expected_value: String,
    pub expected_formula: Option<String>,
    pub expected_style: Option<String>,
    pub replacement_value: String,
    pub row: u32,
    pub col: u32,
}

#[derive(Clone, Debug)]
struct RfiLogRowInfo {
    sheet: String,
    row: u32,
    canonical_rfi: String,
    remarks_col: u32,
    remarks_cell: Option<CellData>,
    remarks_merged: bool,
}

#[derive(Clone, Debug)]
struct RfiLogStatusRowInfo {
    sheet: String,
    row: u32,
    canonical_rfi: String,
    status_col: u32,
    status_cell: Option<CellData>,
    status_merged: bool,
}

struct CompletionPdfDiscovery {
    matches: BTreeMap<String, Vec<PathBuf>>,
    invalid_names: Vec<PathBuf>,
    worker_count: usize,
    directories_scanned: usize,
    entries_scanned: usize,
}

struct CompletionDirectoryBatch {
    directories: Vec<PathBuf>,
    matches: Vec<(String, PathBuf)>,
    invalid_names: Vec<PathBuf>,
    entries_scanned: usize,
}

#[derive(Clone, Debug)]
struct FollowUpTarget {
    sheet: String,
    row: u32,
    status_col: u32,
    status_cell: Option<CellData>,
    status_merged: bool,
}

#[derive(Clone, Copy)]
struct HeaderLayout {
    row: u32,
    rfi_col: u32,
    value_col: u32,
}

pub(crate) fn canonical_rfi_prefix() -> &'static str {
    "AZE-SUN-INT-QMS-RFI-"
}

pub(crate) fn parse_rfi_discipline_and_number(name: &str) -> Option<(&'static str, u32)> {
    let clean = name.nfc().collect::<String>().trim().to_ascii_uppercase();
    static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"\A(?:AZE-SUN-INT-QMS-RFI-|RFI-)?(CIV|MEC|ELE|PRC)[-_ ]*([0-9]{1,4})\z")
            .unwrap()
    });
    let caps = RE.captures(&clean)?;
    let discipline = match caps.get(1)?.as_str() {
        "CIV" => "CIV",
        "MEC" => "MEC",
        "ELE" => "ELE",
        "PRC" => "PRC",
        _ => return None,
    };
    Some((
        discipline,
        normalize_candidate_number(caps.get(2)?.as_str())?
            .parse()
            .ok()?,
    ))
}

fn exact_canonical_rfi(value: &str) -> Option<String> {
    let clean = value.nfc().collect::<String>().trim().to_ascii_uppercase();
    let (discipline, number) = parse_rfi_discipline_and_number(&clean)?;
    Some(format!(
        "{}{}-{number:04}",
        canonical_rfi_prefix(),
        discipline
    ))
}

fn completion_code_from_filename(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let prefix = FILE_PREFIX.to_ascii_lowercase();
    if !lower.starts_with(&prefix) || !lower.ends_with(".pdf") {
        return None;
    }
    let code = name.get(FILE_PREFIX.len()..name.len().checked_sub(4)?)?;
    exact_canonical_rfi(code)
}

fn discover_completion_pdfs<P, C>(
    root: &Path,
    mut progress: P,
    mut cancelled: C,
) -> Result<CompletionPdfDiscovery, BackendError>
where
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let root = fs::canonicalize(root)
        .map_err(|_| BackendError::path_invalid("RFI system root could not be resolved"))?;
    if !root.is_dir() {
        return Err(BackendError::path_invalid(
            "RFI system root is not a directory",
        ));
    }
    let mut directories = vec![root.clone()];
    let mut matches = BTreeMap::<String, Vec<PathBuf>>::new();
    let mut invalid_names = Vec::new();
    let mut worker_count = 1_usize;
    let mut directories_scanned = 0_usize;
    let mut entries_scanned = 0_usize;
    progress(ProgressEvent {
        phase: "completion_pdf_discovery".to_string(),
        done: 0,
        total: 1,
        subject: root.to_string_lossy().into_owned(),
    });
    while !directories.is_empty() {
        if cancelled() {
            return Err(BackendError::cancelled());
        }
        directories.sort();
        let current_count = directories.len();
        let (workers, batches) = bounded_map(&directories, |directory| {
            scan_completion_directory(&root, directory)
        })
        .map_err(BackendError::request_failed)?;
        worker_count = worker_count.max(workers);
        let mut next = Vec::new();
        for batch in batches {
            let batch = batch?;
            entries_scanned = entries_scanned
                .checked_add(batch.entries_scanned)
                .ok_or_else(|| BackendError::validation("RFI system entry count overflow"))?;
            if entries_scanned > MAX_COMPLETION_ENTRIES {
                return Err(BackendError::validation(format!(
                    "RFI system exceeds the {MAX_COMPLETION_ENTRIES}-entry completion scan limit"
                )));
            }
            next.extend(batch.directories);
            invalid_names.extend(batch.invalid_names);
            for (code, path) in batch.matches {
                matches.entry(code).or_default().push(path);
            }
        }
        directories_scanned = directories_scanned
            .checked_add(current_count)
            .ok_or_else(|| BackendError::validation("RFI system directory count overflow"))?;
        next.sort();
        progress(ProgressEvent {
            phase: "completion_pdf_discovery".to_string(),
            done: directories_scanned as u64,
            total: directories_scanned.saturating_add(next.len()) as u64,
            subject: format!("{} exact RFI PDF", matches.len()),
        });
        directories = next;
    }
    for paths in matches.values_mut() {
        paths.sort();
        paths.dedup();
    }
    invalid_names.sort();
    invalid_names.dedup();
    Ok(CompletionPdfDiscovery {
        matches,
        invalid_names,
        worker_count,
        directories_scanned,
        entries_scanned,
    })
}

fn scan_completion_directory(
    root: &Path,
    directory: &Path,
) -> Result<CompletionDirectoryBatch, BackendError> {
    let mut batch = CompletionDirectoryBatch {
        directories: Vec::new(),
        matches: Vec::new(),
        invalid_names: Vec::new(),
        entries_scanned: 0,
    };
    for entry in fs::read_dir(directory)
        .map_err(|_| BackendError::path_invalid("RFI system folder could not be read"))?
    {
        let entry = entry.map_err(|_| BackendError::path_invalid("RFI system entry failed"))?;
        batch.entries_scanned += 1;
        let file_type = entry
            .file_type()
            .map_err(|_| BackendError::path_invalid("RFI system entry metadata failed"))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            batch.directories.push(entry.path());
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_named_pdf = name.to_ascii_lowercase().starts_with("rfi no. ")
            && name.to_ascii_lowercase().ends_with(".pdf");
        let Some(code) = completion_code_from_filename(name) else {
            if is_named_pdf {
                batch.invalid_names.push(entry.path());
            }
            continue;
        };
        let path = fs::canonicalize(entry.path())
            .map_err(|_| BackendError::path_invalid("RFI PDF could not be resolved"))?;
        if !path.starts_with(root) || !path.is_file() {
            return Err(BackendError::path_escape());
        }
        batch.matches.push((code, path));
    }
    batch.directories.sort();
    batch.matches.sort();
    batch.invalid_names.sort();
    Ok(batch)
}

fn clean_header(value: &str) -> String {
    value.nfc().collect::<String>().trim().to_ascii_uppercase()
}

fn cell_is_merged(sheet: &WorksheetData, row: u32, col: u32) -> bool {
    sheet.merged_ranges.iter().any(|range| {
        range.first_row <= row
            && row <= range.last_row
            && range.first_column <= col
            && col <= range.last_column
    })
}

fn discover_header(
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

fn scan_system_discipline(
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

fn read_rfi_log_rows(workbook: &WorkbookData) -> Result<Vec<RfiLogRowInfo>, BackendError> {
    let mut rows = Vec::new();
    for discipline in DISCIPLINES {
        let sheet = workbook.sheet(discipline).ok_or_else(|| {
            BackendError::validation("error.rfi.systemSync.workbookLayoutInvalid")
        })?;
        let layout = discover_header(sheet, &["REMARKS"])?;
        for row in (layout.row + 1)..=sheet.max_row {
            let Some(rfi_cell) = sheet.cell(row, layout.rfi_col) else {
                continue;
            };
            let Some(canonical_rfi) = exact_canonical_rfi(&rfi_cell.value) else {
                continue;
            };
            if !canonical_rfi.contains(&format!("-RFI-{discipline}-")) {
                continue;
            }
            rows.push(RfiLogRowInfo {
                sheet: discipline.to_string(),
                row,
                canonical_rfi,
                remarks_col: layout.value_col,
                remarks_cell: sheet.cell(row, layout.value_col).cloned(),
                remarks_merged: cell_is_merged(sheet, row, layout.value_col),
            });
        }
    }
    Ok(rows)
}

fn read_rfi_log_status_rows(
    workbook: &WorkbookData,
) -> Result<Vec<RfiLogStatusRowInfo>, BackendError> {
    let mut rows = Vec::new();
    for discipline in DISCIPLINES {
        let sheet = workbook.sheet(discipline).ok_or_else(|| {
            BackendError::validation("error.rfi.completeStatus.workbookLayoutInvalid")
        })?;
        let layout = discover_header(sheet, &["INSPECTION STATUS"]).map_err(|_| {
            BackendError::validation("error.rfi.completeStatus.workbookLayoutInvalid")
        })?;
        for row in (layout.row + 1)..=sheet.max_row {
            let Some(rfi_cell) = sheet.cell(row, layout.rfi_col) else {
                continue;
            };
            let Some(canonical_rfi) = exact_canonical_rfi(&rfi_cell.value) else {
                continue;
            };
            if !canonical_rfi.contains(&format!("-RFI-{discipline}-")) {
                continue;
            }
            rows.push(RfiLogStatusRowInfo {
                sheet: discipline.to_string(),
                row,
                canonical_rfi,
                status_col: layout.value_col,
                status_cell: sheet.cell(row, layout.value_col).cloned(),
                status_merged: cell_is_merged(sheet, row, layout.value_col),
            });
        }
    }
    Ok(rows)
}

fn read_follow_up_targets(
    workbook: &WorkbookData,
) -> Result<BTreeMap<String, Vec<FollowUpTarget>>, BackendError> {
    let mut targets = BTreeMap::<String, Vec<FollowUpTarget>>::new();
    for discipline in ["CIV", "MEC"] {
        let candidates = workbook
            .sheets
            .iter()
            .filter(|(name, _)| {
                let name = clean_header(name);
                name.contains("SIGNED BY WORLEY") && name.ends_with(discipline)
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(BackendError::validation(
                "error.rfi.systemSync.followUpLayoutInvalid",
            ));
        }
        let (sheet_name, sheet) = candidates[0];
        let value_headers: &[&str] = if discipline == "MEC" {
            &["WORLEY STATUS"]
        } else {
            &["STATUS"]
        };
        let layout = discover_header(sheet, value_headers)?;
        for row in (layout.row + 1)..=sheet.max_row {
            let Some(rfi_cell) = sheet.cell(row, layout.rfi_col) else {
                continue;
            };
            let short = clean_header(&rfi_cell.value);
            let Some((parsed_discipline, number)) = parse_rfi_discipline_and_number(&short) else {
                continue;
            };
            if parsed_discipline != discipline || short != format!("{discipline}-{number:04}") {
                continue;
            }
            let canonical = format!("{}{short}", canonical_rfi_prefix());
            targets.entry(canonical).or_default().push(FollowUpTarget {
                sheet: sheet_name.to_string(),
                row,
                status_col: layout.value_col,
                status_cell: sheet.cell(row, layout.value_col).cloned(),
                status_merged: cell_is_merged(sheet, row, layout.value_col),
            });
        }
    }
    Ok(targets)
}

fn safe_cell(cell: &Option<CellData>, merged: bool) -> bool {
    cell.as_ref().is_some_and(|cell| cell.formula.is_none()) && !merged
}

fn dto(
    row_id: &str,
    cell: &CellData,
    replacement: String,
    row: u32,
    col: u32,
) -> GuardedCellUpdateDto {
    GuardedCellUpdateDto {
        row_id: row_id.to_string(),
        expected_value: cell.value.clone(),
        expected_formula: cell.formula.clone(),
        expected_style: cell.style.clone(),
        replacement_value: replacement,
        row,
        col,
    }
}

fn append_signed_marker(value: &str) -> String {
    if value
        .lines()
        .any(|line| line.trim() == WORLEY_SIGNED_MARKER)
    {
        value.to_string()
    } else if value.is_empty() {
        WORLEY_SIGNED_MARKER.to_string()
    } else {
        format!("{value}\n{WORLEY_SIGNED_MARKER}")
    }
}

pub fn preview_complete_status_sync<P, C>(
    system_root: &Path,
    rfi_log_path: &Path,
    rfi_log_file: &mut File,
    mut progress: P,
    cancelled: C,
) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
    C: FnMut() -> bool,
{
    let discovery = discover_completion_pdfs(system_root, &mut progress, cancelled)?;
    progress(ProgressEvent {
        phase: "completion_workbook_read".to_string(),
        done: 0,
        total: 1,
        subject: rfi_log_path.to_string_lossy().into_owned(),
    });
    rfi_log_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| BackendError::protocol(error.to_string()))?;
    let fingerprint = fingerprint_open_file(rfi_log_file)?;
    rfi_log_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| BackendError::protocol(error.to_string()))?;
    let workbook = read_workbook_from_reader(rfi_log_file).map_err(|error| {
        BackendError::validation(format!("error.rfi.completeStatus.workbookInvalid: {error}"))
    })?;
    let log_rows = read_rfi_log_status_rows(&workbook)?;
    progress(ProgressEvent {
        phase: "completion_workbook_read".to_string(),
        done: 1,
        total: 1,
        subject: rfi_log_path.to_string_lossy().into_owned(),
    });

    let mut updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut review_rows = Vec::new();
    let total = discovery.matches.len() as u64;
    for (index, (canonical_rfi, pdf_paths)) in discovery.matches.iter().enumerate() {
        progress(ProgressEvent {
            phase: "completion_mapping".to_string(),
            done: index as u64,
            total,
            subject: canonical_rfi.clone(),
        });
        let row_id = format!("complete-row-{}", index + 1);
        let matching = log_rows
            .iter()
            .filter(|row| row.canonical_rfi == *canonical_rfi)
            .collect::<Vec<_>>();
        let current_values = matching
            .iter()
            .filter_map(|row| row.status_cell.as_ref())
            .map(|cell| cell.value.nfc().collect::<String>().trim().to_string())
            .collect::<BTreeSet<_>>();
        let current_status = if current_values.is_empty() {
            "Missing cell".to_string()
        } else {
            current_values
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ")
        };
        let safe_cells = !matching.is_empty()
            && matching
                .iter()
                .all(|row| safe_cell(&row.status_cell, row.status_merged));
        let known_values = current_values
            .iter()
            .all(|value| matches!(value.as_str(), "" | "Ongoing" | "Completed"));
        let mut pending = Vec::new();
        if safe_cells && known_values {
            for row in &matching {
                let cell = row.status_cell.as_ref().expect("safe status cell exists");
                if cell.value.nfc().collect::<String>().trim() != "Completed" {
                    pending.push((
                        row.sheet.clone(),
                        format!("{}:{}", row.row, row.status_col),
                        dto(
                            &row_id,
                            cell,
                            "Completed".to_string(),
                            row.row,
                            row.status_col,
                        ),
                    ));
                }
            }
        }
        let (disposition, action, reason, selectable) = if matching.is_empty() {
            (
                "rfi.completeStatus.disposition.manualReview",
                "RFI was not found in the RFI Log",
                "Exact completed PDF exists, but no exact RFI Log row was found",
                false,
            )
        } else if !safe_cells {
            (
                "rfi.completeStatus.disposition.manualReview",
                "Inspection Status cell is missing, merged, or formula-bearing",
                "No unsafe workbook cell was authorized",
                false,
            )
        } else if !known_values {
            (
                "rfi.completeStatus.disposition.manualReview",
                "Current status conflicts with automatic completion",
                "Only blank, Ongoing, or Completed status cells may be synchronized automatically",
                false,
            )
        } else if pending.is_empty() {
            (
                "rfi.completeStatus.disposition.alreadyCompleted",
                "Already Completed",
                "Every matching RFI Log row is already Completed",
                false,
            )
        } else {
            (
                "rfi.completeStatus.disposition.ready",
                "Set matching Inspection Status cell(s) to Completed",
                "Exact completed RFI PDF and safe RFI Log mapping found",
                true,
            )
        };
        if selectable {
            for (sheet, key, update) in pending {
                updates.entry(sheet).or_default().insert(key, update);
            }
        }
        let (discipline, number) = parse_rfi_discipline_and_number(canonical_rfi)
            .expect("exact canonical completion code is parseable");
        let relative_paths = pdf_paths
            .iter()
            .filter_map(|path| path.strip_prefix(system_root).ok())
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        review_rows.push(json!({
            "_rowId": row_id,
            "discipline": discipline,
            "rfiNo": format!("{discipline}-{number:04}"),
            "canonicalRfi": canonical_rfi,
            "pdfPath": relative_paths.first().cloned().unwrap_or_else(|| pdf_paths[0].to_string_lossy().into_owned()),
            "pdfCount": pdf_paths.len(),
            "pdfPaths": relative_paths.join("\n"),
            "currentStatus": current_status,
            "proposedStatus": "Completed",
            "action": action,
            "disposition": disposition,
            "reasonKey": reason,
            "enabled": selectable,
            "selectable": selectable,
        }));
    }
    progress(ProgressEvent {
        phase: "completion_mapping".to_string(),
        done: total,
        total,
        subject: String::new(),
    });

    let mut steps = Vec::new();
    let count = updates.values().map(BTreeMap::len).sum::<usize>();
    if count > 0 {
        let extension = rfi_log_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("xlsm");
        let backup_path = rfi_log_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!(
                ".docflow-completion-sync-backup-{}.{}",
                Uuid::new_v4().simple(),
                extension
            ));
        let update_row_ids = updates
            .values()
            .flat_map(BTreeMap::values)
            .map(|update| update.row_id.as_str())
            .collect::<BTreeSet<_>>();
        let canonical_rfi_codes = review_rows
            .iter()
            .filter(|row| {
                row.get("_rowId")
                    .and_then(Value::as_str)
                    .is_some_and(|row_id| update_row_ids.contains(row_id))
            })
            .filter_map(|row| row.get("canonicalRfi").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        let step_meta = SystemSyncStepMeta {
            target_path: rfi_log_path.to_string_lossy().into_owned(),
            backup_path: backup_path.to_string_lossy().into_owned(),
            source_fingerprint: fingerprint,
            workbook_kind: SyncWorkbookKind::RfiLogStatus,
            updates,
            canonical_rfi_codes,
            summary: format!("Update {count} RFI Log Inspection Status cell(s)"),
        };
        let mut meta = JsonObject::new();
        meta.insert(
            "syncStep".to_string(),
            serde_json::to_value(step_meta)
                .map_err(|error| BackendError::protocol(error.to_string()))?,
        );
        steps.push(Step {
            kind: StepKind::WorkbookUpdate,
            src: Some(rfi_log_path.to_string_lossy().into_owned()),
            dst: Some(backup_path.to_string_lossy().into_owned()),
            note_key: Some("note.rfi.completeStatus.rfiLog".to_string()),
            meta,
        });
    }

    let mut warnings = discovery
        .invalid_names
        .iter()
        .take(MAX_COMPLETION_WARNINGS)
        .map(|path| Warning {
            key: "warn.rfi.completeStatus.invalidPdfName".to_string(),
            subject: path.to_string_lossy().into_owned(),
        })
        .collect::<Vec<_>>();
    if discovery.invalid_names.len() > MAX_COMPLETION_WARNINGS {
        warnings.push(Warning {
            key: "warn.rfi.completeStatus.invalidPdfNameOverflow".to_string(),
            subject: format!(
                "{} additional invalid RFI PDF name(s)",
                discovery.invalid_names.len() - MAX_COMPLETION_WARNINGS
            ),
        });
    }
    if steps.is_empty() {
        warnings.push(Warning {
            key: "warn.rfi.completeStatus.noChangesNeeded".to_string(),
            subject: "No safe RFI Log status changes are ready".to_string(),
        });
    }
    let columns = vec![
        json!({"key":"enabled","labelKey":"rfi.completeStatus.column.enabled","kind":"bool","bulk":true,"disabledWhen":{"field":"selectable","equals":false}}),
        json!({"key":"discipline","labelKey":"rfi.completeStatus.column.discipline","kind":"readonly"}),
        json!({"key":"rfiNo","labelKey":"rfi.completeStatus.column.rfiNo","kind":"readonly"}),
        json!({"key":"pdfPath","labelKey":"rfi.completeStatus.column.pdfPath","kind":"readonly"}),
        json!({"key":"pdfCount","labelKey":"rfi.completeStatus.column.pdfCount","kind":"readonly"}),
        json!({"key":"currentStatus","labelKey":"rfi.completeStatus.column.currentStatus","kind":"readonly"}),
        json!({"key":"proposedStatus","labelKey":"rfi.completeStatus.column.proposedStatus","kind":"readonly"}),
        json!({"key":"disposition","labelKey":"rfi.completeStatus.column.disposition","kind":"readonly","display":"localizedKey"}),
        json!({"key":"action","labelKey":"rfi.completeStatus.column.action","kind":"readonly","detail":true}),
        json!({"key":"pdfPaths","labelKey":"rfi.completeStatus.column.pdfPaths","kind":"readonly","detail":true}),
        json!({"key":"reasonKey","labelKey":"rfi.completeStatus.column.reason","kind":"readonly","detail":true}),
    ];
    let editor = json!({
        "kind":"recordTable","layout":"compact","titleKey":"rfi.completeStatus.editor.title",
        "rowCountKey":"rfi.completeStatus.editor.rowCount","requiredFields":["enabled"],
        "groupField":"discipline","selectionGroupFields":[],"columns":columns,"rows":review_rows,
        "duplicateEnabled":false,"selectionEnabled":true,"bulkEnabled":true,"reorderable":false,
        "detailsEnabled":true,"detailsDefaultVisible":false,"copyEnabled":false
    });
    let mut meta = JsonObject::new();
    meta.insert("editor".to_string(), editor);
    meta.insert(
        "workerCount".to_string(),
        Value::from(discovery.worker_count as u64),
    );
    meta.insert(
        "directoriesScanned".to_string(),
        Value::from(discovery.directories_scanned as u64),
    );
    meta.insert(
        "entriesScanned".to_string(),
        Value::from(discovery.entries_scanned as u64),
    );
    meta.insert(
        "exactPdfCount".to_string(),
        Value::from(discovery.matches.values().map(Vec::len).sum::<usize>() as u64),
    );
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(Plan {
        plan_id: String::new(),
        op_id: COMPLETE_STATUS_OPERATION_ID.to_string(),
        steps,
        warnings,
        meta,
    })
}

pub fn preview_system_sync(
    system_root: &Path,
    rfi_log_path: &Path,
    follow_up_path: &Path,
    rfi_log_file: &mut File,
    follow_up_file: &mut File,
) -> Result<Plan, BackendError> {
    preview_system_sync_indexed(
        system_root,
        rfi_log_path,
        follow_up_path,
        rfi_log_file,
        follow_up_file,
        None,
        "local",
        false,
        |_| {},
    )
}

#[allow(clippy::too_many_arguments)]
pub fn preview_system_sync_indexed<P>(
    system_root: &Path,
    rfi_log_path: &Path,
    follow_up_path: &Path,
    rfi_log_file: &mut File,
    follow_up_file: &mut File,
    scan_index: Option<&ScanIndex>,
    owner_id: &str,
    force_rescan: bool,
    mut progress: P,
) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
{
    let (scanned_rfis, scan_stats) = scan_system_root_indexed(
        system_root,
        scan_index,
        owner_id,
        force_rescan,
        &mut progress,
    )?;
    progress(ProgressEvent {
        phase: "system_workbook_read".to_string(),
        done: 0,
        total: 2,
        subject: rfi_log_path.to_string_lossy().into_owned(),
    });
    rfi_log_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let rfi_log_fingerprint = fingerprint_open_file(rfi_log_file)?;
    rfi_log_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let rfi_log_workbook = read_workbook_from_reader(rfi_log_file).map_err(|e| {
        BackendError::validation(format!("error.rfi.systemSync.workbookInvalid: {e}"))
    })?;
    let rfi_log_rows = read_rfi_log_rows(&rfi_log_workbook)?;
    progress(ProgressEvent {
        phase: "system_workbook_read".to_string(),
        done: 1,
        total: 2,
        subject: follow_up_path.to_string_lossy().into_owned(),
    });

    follow_up_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let follow_up_fingerprint = fingerprint_open_file(follow_up_file)?;
    follow_up_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let follow_up_workbook = read_workbook_from_reader(follow_up_file).map_err(|e| {
        BackendError::validation(format!("error.rfi.systemSync.workbookInvalid: {e}"))
    })?;
    let follow_up_targets = read_follow_up_targets(&follow_up_workbook)?;
    progress(ProgressEvent {
        phase: "system_workbook_read".to_string(),
        done: 2,
        total: 2,
        subject: String::new(),
    });

    let mut log_updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut follow_up_updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut review_rows = Vec::new();

    for (index, scanned) in scanned_rfis.iter().enumerate() {
        progress(ProgressEvent {
            phase: "system_mapping".to_string(),
            done: index as u64,
            total: scanned_rfis.len() as u64,
            subject: scanned.canonical_rfi.clone(),
        });
        let row_id = format!("sync-row-{}", index + 1);
        let matching_log = rfi_log_rows
            .iter()
            .filter(|row| row.canonical_rfi == scanned.canonical_rfi)
            .collect::<Vec<_>>();
        let matching_follow_up = follow_up_targets
            .get(&scanned.canonical_rfi)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut log_action = "Not found in RFI Log".to_string();
        let mut follow_up_action = if matches!(scanned.discipline.as_str(), "CIV" | "MEC") {
            "Not found in Follow Up control table".to_string()
        } else {
            "Not applicable for ELE/PRC".to_string()
        };
        let mut ready = false;
        let mut unsafe_target = false;
        let mut pending_log_updates = Vec::new();
        let mut pending_follow_up_updates = Vec::new();

        if scanned.signed_by_worley && !matching_log.is_empty() {
            if matching_log
                .iter()
                .all(|row| safe_cell(&row.remarks_cell, row.remarks_merged))
            {
                let mut changed = 0usize;
                for row in &matching_log {
                    let cell = row.remarks_cell.as_ref().expect("safe cell is present");
                    let replacement = append_signed_marker(&cell.value);
                    if replacement != cell.value {
                        pending_log_updates.push((
                            row.sheet.clone(),
                            format!("{}:{}", row.row, row.remarks_col),
                            dto(&row_id, cell, replacement, row.row, row.remarks_col),
                        ));
                        changed += 1;
                    }
                }
                log_action = if changed == 0 {
                    "Already signed in RFI Log".to_string()
                } else {
                    format!("Add exact Worley remark to {changed} RFI Log row(s)")
                };
                ready |= changed > 0;
            } else {
                log_action =
                    "Manual review: REMARKS cell missing, merged, or formula-bearing".to_string();
                unsafe_target = true;
            }
        }

        if !matching_follow_up.is_empty() {
            if matching_follow_up
                .iter()
                .all(|target| safe_cell(&target.status_cell, target.status_merged))
            {
                let mut changed = 0usize;
                for target in matching_follow_up {
                    let cell = target.status_cell.as_ref().expect("safe cell is present");
                    let current = cell.value.nfc().collect::<String>().trim().to_string();
                    if !matches!(current.as_str(), "" | "Signed" | "Not Signed") {
                        unsafe_target = true;
                        continue;
                    }
                    let desired = if scanned.signed_by_worley {
                        "Signed"
                    } else if current == "Signed" {
                        // A weak/unsigned scan can never revoke a prior signature.
                        "Signed"
                    } else {
                        "Not Signed"
                    };
                    if current != desired {
                        pending_follow_up_updates.push((
                            target.sheet.clone(),
                            format!("{}:{}", target.row, target.status_col),
                            dto(
                                &row_id,
                                cell,
                                desired.to_string(),
                                target.row,
                                target.status_col,
                            ),
                        ));
                        changed += 1;
                    }
                }
                follow_up_action = if unsafe_target {
                    "Manual review: unexpected Follow Up status".to_string()
                } else if changed == 0 {
                    format!(
                        "Already {} in Follow Up",
                        if scanned.signed_by_worley {
                            "Signed"
                        } else {
                            "Not Signed or preserved Signed"
                        }
                    )
                } else {
                    format!(
                        "Set {changed} Follow Up control status cell(s) to {}",
                        if scanned.signed_by_worley {
                            "Signed"
                        } else {
                            "Not Signed"
                        }
                    )
                };
                ready |= changed > 0;
            } else {
                follow_up_action =
                    "Manual review: Follow Up status cell missing, merged, or formula-bearing"
                        .to_string();
                unsafe_target = true;
            }
        } else if matches!(scanned.discipline.as_str(), "CIV" | "MEC") {
            follow_up_action = "Manual review: no existing Follow Up control row".to_string();
            unsafe_target = true;
        }

        if unsafe_target || matching_log.is_empty() {
            ready = false;
        } else {
            for (sheet, key, update) in pending_log_updates {
                log_updates.entry(sheet).or_default().insert(key, update);
            }
            for (sheet, key, update) in pending_follow_up_updates {
                follow_up_updates
                    .entry(sheet)
                    .or_default()
                    .insert(key, update);
            }
        }

        let (disposition, reason_key, selectable) = if unsafe_target || matching_log.is_empty() {
            (
                "rfi.systemSync.disposition.manualReview",
                "Unsafe or incomplete workbook mapping; no unsafe cell was authorized".to_string(),
                false,
            )
        } else if ready {
            (
                "rfi.systemSync.disposition.ready",
                if scanned.signed_by_worley {
                    "Worley signature detected; exact guarded updates are ready".to_string()
                } else {
                    "No Worley signature detected; exact Not Signed update is ready".to_string()
                },
                true,
            )
        } else {
            (
                "rfi.systemSync.disposition.alreadySynced",
                "Already synchronized".to_string(),
                false,
            )
        };
        review_rows.push(SystemSyncRow {
            row_id,
            discipline: scanned.discipline.clone(),
            rfi_no: scanned.short_rfi.clone(),
            canonical_rfi: scanned.canonical_rfi.clone(),
            worley_status: if scanned.signed_by_worley {
                WORLEY_SIGNED_MARKER.to_string()
            } else {
                "Unsigned".to_string()
            },
            rfi_log_action: log_action,
            follow_up_action,
            disposition: disposition.to_string(),
            reason_key,
            enabled: selectable,
            selectable,
        });
    }

    let mut steps = Vec::new();
    for (kind, path, fingerprint, updates, note, label) in [
        (
            SyncWorkbookKind::RfiLog,
            rfi_log_path,
            rfi_log_fingerprint,
            log_updates,
            "note.rfi.systemSync.rfiLog",
            "RFI Log REMARKS",
        ),
        (
            SyncWorkbookKind::FollowUp,
            follow_up_path,
            follow_up_fingerprint,
            follow_up_updates,
            "note.rfi.systemSync.followUp",
            "Follow Up status",
        ),
    ] {
        let count = updates.values().map(BTreeMap::len).sum::<usize>();
        if count == 0 {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("xlsm");
        let backup_path = path.parent().unwrap_or(Path::new(".")).join(format!(
            ".docflow-system-sync-backup-{}.{}",
            Uuid::new_v4().simple(),
            extension
        ));
        let update_row_ids = updates
            .values()
            .flat_map(BTreeMap::values)
            .map(|update| update.row_id.as_str())
            .collect::<BTreeSet<_>>();
        let canonical_rfi_codes = review_rows
            .iter()
            .filter(|row| update_row_ids.contains(row.row_id.as_str()))
            .map(|row| row.canonical_rfi.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let step_meta = SystemSyncStepMeta {
            target_path: path.to_string_lossy().into_owned(),
            backup_path: backup_path.to_string_lossy().into_owned(),
            source_fingerprint: fingerprint,
            workbook_kind: kind,
            updates,
            canonical_rfi_codes,
            summary: format!("Update {count} {label} cell(s)"),
        };
        let mut meta = JsonObject::new();
        meta.insert(
            "syncStep".to_string(),
            serde_json::to_value(step_meta).map_err(|e| BackendError::protocol(e.to_string()))?,
        );
        steps.push(Step {
            kind: StepKind::WorkbookUpdate,
            src: Some(path.to_string_lossy().into_owned()),
            dst: Some(backup_path.to_string_lossy().into_owned()),
            note_key: Some(note.to_string()),
            meta,
        });
    }

    let warnings = if steps.is_empty() {
        vec![Warning {
            key: "warn.rfi.systemSync.noChangesNeeded".to_string(),
            subject:
                "All detected RFIs are already synchronized, unsigned, or require manual review"
                    .to_string(),
        }]
    } else {
        Vec::new()
    };
    let columns = vec![
        json!({"key":"enabled","labelKey":"rfi.systemSync.column.enabled","kind":"bool","bulk":false,"disabledWhen":{"field":"selectable","equals":false}}),
        json!({"key":"discipline","labelKey":"rfi.systemSync.column.discipline","kind":"readonly"}),
        json!({"key":"rfiNo","labelKey":"rfi.systemSync.column.rfiNo","kind":"readonly"}),
        json!({"key":"worleyStatus","labelKey":"rfi.systemSync.column.worleyStatus","kind":"readonly"}),
        json!({"key":"rfiLogAction","labelKey":"rfi.systemSync.column.rfiLogAction","kind":"readonly"}),
        json!({"key":"followUpAction","labelKey":"rfi.systemSync.column.followUpAction","kind":"readonly"}),
        json!({"key":"disposition","labelKey":"rfi.systemSync.column.disposition","kind":"readonly","display":"localizedKey"}),
        json!({"key":"reasonKey","labelKey":"rfi.systemSync.column.reason","kind":"readonly","detail":true}),
    ];
    let editor = json!({
        "kind":"recordTable","layout":"compact","titleKey":"rfi.systemSync.editor.title",
        "rowCountKey":"rfi.systemSync.editor.rowCount","requiredFields":["enabled"],
        "groupField":"discipline","selectionGroupFields":[],"columns":columns,"rows":review_rows,
        "duplicateEnabled":false,"selectionEnabled":false,"bulkEnabled":false,"reorderable":false,
        "detailsEnabled":true,"detailsDefaultVisible":false,"copyEnabled":false
    });
    let mut meta = JsonObject::new();
    meta.insert("editor".to_string(), editor);
    meta.insert(
        "scanIndex".to_string(),
        serde_json::to_value(&scan_stats)
            .map_err(|error| BackendError::protocol(error.to_string()))?,
    );
    meta.insert(
        "parallelAnalysis".to_string(),
        Value::Bool(scan_stats.parallel_analysis),
    );
    meta.insert(
        "workerCount".to_string(),
        Value::from(scan_stats.analysis_workers),
    );
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps,
        warnings,
        meta,
    })
}

pub(crate) fn decode_step_meta(step: &Step) -> Result<SystemSyncStepMeta, BackendError> {
    let value = step
        .meta
        .get("syncStep")
        .ok_or_else(|| BackendError::validation("syncStep metadata missing in plan step"))?;
    serde_json::from_value(value.clone())
        .map_err(|error| BackendError::validation(format!("Invalid syncStep metadata: {error}")))
}

#[doc(hidden)]
pub fn revise(plan: &Plan, rows: &[Value]) -> Result<Plan, BackendError> {
    let completion = plan.op_id == COMPLETE_STATUS_OPERATION_ID;
    let invalid_key = if completion {
        "error.rfi.completeStatus.approvedUpdateInvalid"
    } else {
        "error.rfi.systemSync.approvedUpdateInvalid"
    };
    let no_rows_key = if completion {
        "warn.rfi.completeStatus.noRowsSelected"
    } else {
        "warn.rfi.systemSync.noRowsSelected"
    };
    let original_rows = plan
        .meta
        .get("editor")
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation(invalid_key))?;
    if rows.len() != original_rows.len() {
        return Err(BackendError::validation(invalid_key));
    }
    let originals = original_rows
        .iter()
        .map(|row| {
            row.get("_rowId")
                .and_then(Value::as_str)
                .map(|row_id| (row_id.to_string(), row.clone()))
                .ok_or_else(|| BackendError::validation(invalid_key))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut enabled = BTreeSet::new();
    let mut revised_rows = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for submitted in rows {
        let row_id = submitted
            .get("_rowId")
            .and_then(Value::as_str)
            .ok_or_else(|| BackendError::validation(invalid_key))?;
        if !seen.insert(row_id.to_string()) {
            return Err(BackendError::validation(invalid_key));
        }
        let original = originals
            .get(row_id)
            .ok_or_else(|| BackendError::validation(invalid_key))?;
        let requested = submitted
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| BackendError::validation(invalid_key))?;
        let selectable = original
            .get("selectable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if requested && !selectable {
            return Err(BackendError::validation(invalid_key));
        }
        let mut expected = original.clone();
        expected["enabled"] = Value::Bool(requested);
        if submitted != &expected {
            return Err(BackendError::validation(invalid_key));
        }
        if requested {
            enabled.insert(row_id.to_string());
        }
        revised_rows.push(expected);
    }

    let mut revised = plan.clone();
    revised.meta["editor"]["rows"] = Value::Array(revised_rows);
    revised.steps.clear();
    for original_step in &plan.steps {
        let mut step = original_step.clone();
        let mut meta = decode_step_meta(&step)?;
        for updates in meta.updates.values_mut() {
            updates.retain(|_, update| enabled.contains(&update.row_id));
        }
        meta.updates.retain(|_, updates| !updates.is_empty());
        if meta.updates.is_empty() {
            continue;
        }
        let update_row_ids = meta
            .updates
            .values()
            .flat_map(BTreeMap::values)
            .map(|update| update.row_id.as_str())
            .collect::<BTreeSet<_>>();
        meta.canonical_rfi_codes = originals
            .values()
            .filter(|row| {
                row.get("_rowId")
                    .and_then(Value::as_str)
                    .is_some_and(|row_id| update_row_ids.contains(row_id))
            })
            .filter_map(|row| row.get("canonicalRfi").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        step.meta.insert(
            "syncStep".to_string(),
            serde_json::to_value(meta).map_err(|_| BackendError::validation(invalid_key))?,
        );
        revised.steps.push(step);
    }
    revised.warnings.retain(|warning| {
        warning.key != "warn.rfi.systemSync.noRowsSelected"
            && warning.key != "warn.rfi.completeStatus.noRowsSelected"
    });
    if revised.steps.is_empty() {
        revised.warnings.push(Warning {
            key: no_rows_key.to_string(),
            subject: if completion {
                "No completed RFI rows are selected for synchronization".to_string()
            } else {
                "No signed RFI rows are selected for synchronization".to_string()
            },
        });
    }
    Ok(revised)
}

fn validate_replacement(kind: SyncWorkbookKind, update: &GuardedCellUpdateDto) -> bool {
    match kind {
        SyncWorkbookKind::FollowUp => match update.replacement_value.as_str() {
            "Signed" => matches!(update.expected_value.trim(), "" | "Signed" | "Not Signed"),
            "Not Signed" => matches!(update.expected_value.trim(), "" | "Not Signed"),
            _ => false,
        },
        SyncWorkbookKind::RfiLogStatus => {
            update.replacement_value == "Completed"
                && matches!(
                    update.expected_value.nfc().collect::<String>().trim(),
                    "" | "Ongoing" | "Completed"
                )
        }
        SyncWorkbookKind::RfiLog => {
            update.replacement_value == WORLEY_SIGNED_MARKER
                || update
                    .replacement_value
                    .ends_with(&format!("\n{WORLEY_SIGNED_MARKER}"))
                    && !update.expected_value.is_empty()
                    && update.replacement_value
                        == format!("{}\n{WORLEY_SIGNED_MARKER}", update.expected_value)
        }
    }
}

#[doc(hidden)]
pub fn execute_sync_step(
    step: &Step,
    source_path: &Path,
    dest_path: &Path,
) -> Result<(), BackendError> {
    let meta = decode_step_meta(step)?;
    let mut updates = BTreeMap::<String, BTreeMap<(u32, u32), GuardedCellUpdate>>::new();
    for (sheet, sheet_updates) in &meta.updates {
        for update in sheet_updates.values() {
            if update.row == 0
                || update.col == 0
                || update.expected_formula.is_some()
                || !validate_replacement(meta.workbook_kind, update)
            {
                let key = if meta.workbook_kind == SyncWorkbookKind::RfiLogStatus {
                    "error.rfi.completeStatus.approvedUpdateInvalid"
                } else {
                    "error.rfi.systemSync.approvedUpdateInvalid"
                };
                return Err(BackendError::validation(key));
            }
            let guarded = GuardedCellUpdate {
                expected: CellData {
                    value: update.expected_value.clone(),
                    formula: update.expected_formula.clone(),
                    style: update.expected_style.clone(),
                },
                replacement: CellUpdate::Text(update.replacement_value.clone()),
                create_if_absent: false,
            };
            if updates
                .entry(sheet.clone())
                .or_default()
                .insert((update.row, update.col), guarded)
                .is_some()
            {
                return Err(BackendError::validation(
                    "error.rfi.systemSync.approvedUpdateInvalid",
                ));
            }
        }
    }
    let mut source = File::open(source_path)
        .map_err(|e| BackendError::validation(format!("Failed to open staged workbook: {e}")))?;
    let mut destination = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dest_path)
        .map_err(|e| BackendError::validation(format!("Failed to create staged workbook: {e}")))?;
    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates)
        .map_err(|e| BackendError::validation(format!("Failed to patch workbook: {e}")))?;
    source
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    destination
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).map_err(|e| {
        BackendError::validation(format!("Workbook delta verification failed: {e}"))
    })?;
    Ok(())
}
#[cfg(test)]
mod candidate_normalization_tests;
