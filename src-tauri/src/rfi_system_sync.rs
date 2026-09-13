//! RFI-system scanning and exact, guarded workbook synchronization.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom},
    path::Path,
    sync::OnceLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{
    contracts::{JsonObject, Plan, Step, StepKind, Warning},
    errors::BackendError,
    ooxml::{
        CellData, CellUpdate, GuardedCellUpdate, WorkbookData, WorksheetData,
        patch_workbook_sheets_guarded_text, read_workbook_from_reader,
        verify_guarded_workbook_delta_text,
    },
    rfi_follow_up::{WorkbookFingerprint, fingerprint_open_file},
};

pub const OPERATION_ID: &str = "rfi.system_sync";
pub const WORLEY_SIGNED_MARKER: &str = "Signed by Worley";
pub const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];
const MAX_HEADER_ROWS: u32 = 32;
const MAX_HEADER_COLUMNS: u32 = 64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScannedRfiRecord {
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
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)(?:^|.*-RFI-|\b)(CIV|MEC|ELE|PRC)[-_ ]*0*([1-9]\d*)\b").unwrap()
    });
    let caps = re.captures(&clean)?;
    let discipline = match caps.get(1)?.as_str().to_ascii_uppercase().as_str() {
        "CIV" => "CIV",
        "MEC" => "MEC",
        "ELE" => "ELE",
        "PRC" => "PRC",
        _ => return None,
    };
    Some((discipline, caps.get(2)?.as_str().parse().ok()?))
}

fn exact_canonical_rfi(value: &str) -> Option<String> {
    let clean = value.nfc().collect::<String>().trim().to_ascii_uppercase();
    let (discipline, number) = parse_rfi_discipline_and_number(&clean)?;
    let canonical = format!("{}{}-{number:04}", canonical_rfi_prefix(), discipline);
    (clean == canonical).then_some(canonical)
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

pub(crate) fn scan_system_root(root: &Path) -> Result<Vec<ScannedRfiRecord>, BackendError> {
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

    let temp_dir = tempfile::tempdir()
        .map_err(|error| BackendError::protocol(format!("temporary directory: {error}")))?;
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for (discipline, discipline_path) in search_dirs {
        for entry in fs::read_dir(&discipline_path)
            .map_err(|error| BackendError::path_invalid(error.to_string()))?
        {
            let entry = entry.map_err(|error| BackendError::path_invalid(error.to_string()))?;
            let rfi_folder = entry.path();
            if !rfi_folder.is_dir() {
                continue;
            }
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
            if !seen.insert(canonical_rfi.clone()) {
                continue;
            }
            let mut pdfs = fs::read_dir(&rfi_folder)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
                })
                .collect::<Vec<_>>();
            pdfs.sort();
            let selected_pdf = pdfs
                .iter()
                .find(|path| {
                    path.file_name().is_some_and(|name| {
                        name.to_string_lossy()
                            .to_ascii_uppercase()
                            .starts_with("RFI NO")
                    })
                })
                .cloned()
                .or_else(|| pdfs.first().cloned());
            let (signed_by_worley, signature_reason) = selected_pdf
                .as_ref()
                .map(|path| detect_worley_signature(path, temp_dir.path()))
                .unwrap_or_else(|| (false, "No scanned PDF found in folder".to_string()));
            records.push(ScannedRfiRecord {
                discipline: parsed_discipline.to_string(),
                number,
                canonical_rfi,
                short_rfi: format!("{parsed_discipline}-{number:04}"),
                rfi_folder: rfi_folder.to_string_lossy().into_owned(),
                pdf_path: selected_pdf.map(|path| path.to_string_lossy().into_owned()),
                signed_by_worley,
                signature_reason,
            });
        }
    }
    records.sort_by(|left, right| {
        left.discipline
            .cmp(&right.discipline)
            .then(left.number.cmp(&right.number))
    });
    Ok(records)
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
    let (x1, x2) = (
        (image.width as f64 * 0.65) as usize,
        (image.width as f64 * 0.96) as usize,
    );
    let (y1, y2) = (
        (image.height as f64 * 0.76) as usize,
        (image.height as f64 * 0.93) as usize,
    );
    let mut dark = 0usize;
    let mut total = 0usize;
    for y in y1..y2.min(image.height) {
        for x in x1..x2.min(image.width) {
            total += 1;
            dark += usize::from(image.pixels[y * image.width + x] < 175);
        }
    }
    if total == 0 {
        return (false, "Empty inspection area".to_string());
    }
    let ratio = dark as f64 / total as f64;
    let signed = dark >= 160 && ratio >= 0.007;
    let reason = if signed {
        format!(
            "Signed by Worley (ink detected: {dark} px, {:.1}%)",
            ratio * 100.0
        )
    } else {
        format!(
            "Unsigned / Box empty (ink: {dark} px, {:.2}%)",
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

pub fn preview_system_sync(
    system_root: &Path,
    rfi_log_path: &Path,
    follow_up_path: &Path,
    rfi_log_file: &mut File,
    follow_up_file: &mut File,
) -> Result<Plan, BackendError> {
    let scanned_rfis = scan_system_root(system_root)?;
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

    let mut log_updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut follow_up_updates = BTreeMap::<String, BTreeMap<String, GuardedCellUpdateDto>>::new();
    let mut review_rows = Vec::new();

    for (index, scanned) in scanned_rfis.iter().enumerate() {
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

        if scanned.signed_by_worley {
            if !matching_log.is_empty() {
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
                    log_action = "Manual review: REMARKS cell missing, merged, or formula-bearing"
                        .to_string();
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
                        if current != "Signed" {
                            pending_follow_up_updates.push((
                                target.sheet.clone(),
                                format!("{}:{}", target.row, target.status_col),
                                dto(
                                    &row_id,
                                    cell,
                                    "Signed".to_string(),
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
                        "Already Signed in Follow Up".to_string()
                    } else {
                        format!("Set {changed} Follow Up control status cell(s) to Signed")
                    };
                    ready |= changed > 0;
                } else {
                    follow_up_action =
                        "Manual review: Follow Up status cell missing, merged, or formula-bearing"
                            .to_string();
                    unsafe_target = true;
                }
            }
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

        let (disposition, reason_key, selectable) = if !scanned.signed_by_worley {
            (
                "rfi.systemSync.disposition.unsigned",
                scanned.signature_reason.clone(),
                false,
            )
        } else if unsafe_target || matching_log.is_empty() {
            (
                "rfi.systemSync.disposition.manualReview",
                "Unsafe or incomplete workbook mapping; no unsafe cell was authorized".to_string(),
                false,
            )
        } else if ready {
            (
                "rfi.systemSync.disposition.ready",
                "Worley signature detected; exact guarded updates are ready".to_string(),
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
    let original_rows = plan
        .meta
        .get("editor")
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .ok_or_else(|| BackendError::validation("error.rfi.systemSync.approvedUpdateInvalid"))?;
    if rows.len() != original_rows.len() {
        return Err(BackendError::validation(
            "error.rfi.systemSync.approvedUpdateInvalid",
        ));
    }
    let originals = original_rows
        .iter()
        .map(|row| {
            row.get("_rowId")
                .and_then(Value::as_str)
                .map(|row_id| (row_id.to_string(), row.clone()))
                .ok_or_else(|| {
                    BackendError::validation("error.rfi.systemSync.approvedUpdateInvalid")
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut enabled = BTreeSet::new();
    let mut revised_rows = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for submitted in rows {
        let row_id = submitted
            .get("_rowId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                BackendError::validation("error.rfi.systemSync.approvedUpdateInvalid")
            })?;
        if !seen.insert(row_id.to_string()) {
            return Err(BackendError::validation(
                "error.rfi.systemSync.approvedUpdateInvalid",
            ));
        }
        let original = originals.get(row_id).ok_or_else(|| {
            BackendError::validation("error.rfi.systemSync.approvedUpdateInvalid")
        })?;
        let requested = submitted
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                BackendError::validation("error.rfi.systemSync.approvedUpdateInvalid")
            })?;
        let selectable = original
            .get("selectable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if requested && !selectable {
            return Err(BackendError::validation(
                "error.rfi.systemSync.approvedUpdateInvalid",
            ));
        }
        let mut expected = original.clone();
        expected["enabled"] = Value::Bool(requested);
        if submitted != &expected {
            return Err(BackendError::validation(
                "error.rfi.systemSync.approvedUpdateInvalid",
            ));
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
            serde_json::to_value(meta).map_err(|_| {
                BackendError::validation("error.rfi.systemSync.approvedUpdateInvalid")
            })?,
        );
        revised.steps.push(step);
    }
    revised
        .warnings
        .retain(|warning| warning.key != "warn.rfi.systemSync.noRowsSelected");
    if revised.steps.is_empty() {
        revised.warnings.push(Warning {
            key: "warn.rfi.systemSync.noRowsSelected".to_string(),
            subject: "No signed RFI rows are selected for synchronization".to_string(),
        });
    }
    Ok(revised)
}

fn validate_replacement(kind: SyncWorkbookKind, update: &GuardedCellUpdateDto) -> bool {
    match kind {
        SyncWorkbookKind::FollowUp => update.replacement_value == "Signed",
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
                return Err(BackendError::validation(
                    "error.rfi.systemSync.approvedUpdateInvalid",
                ));
            }
            let guarded = GuardedCellUpdate {
                expected: CellData {
                    value: update.expected_value.clone(),
                    formula: update.expected_formula.clone(),
                    style: update.expected_style.clone(),
                },
                replacement: CellUpdate::Text(update.replacement_value.clone()),
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
