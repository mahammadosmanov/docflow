//! Pure RFI System scanning, signature detection, and atomic multi-workbook synchronization.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::OnceLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::{
    contracts::{JsonObject, Plan, Step, StepKind, Warning},
    errors::BackendError,
    ooxml::{CellUpdate, WorksheetData, read_workbook_from_reader},
    rfi_follow_up::{WorkbookFingerprint, fingerprint_open_file},
    rfi_log::layout,
};

pub const OPERATION_ID: &str = "rfi.system_sync";
pub const WORLEY_SIGNED_MARKER: &str = "Signed by Worley";
pub const DISCIPLINES: [&str; 4] = ["CIV", "MEC", "ELE", "PRC"];

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemSyncStepMeta {
    pub target_path: String,
    pub backup_path: String,
    pub source_fingerprint: WorkbookFingerprint,
    pub updates: BTreeMap<String, BTreeMap<String, GuardedCellUpdateDto>>,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GuardedCellUpdateDto {
    pub expected_value: String,
    pub replacement_value: String,
    pub row: u32,
    pub col: u32,
}

pub(crate) fn canonical_rfi_prefix() -> &'static str {
    "AZE-SUN-INT-QMS-RFI-"
}

pub(crate) fn parse_rfi_discipline_and_number(name: &str) -> Option<(&'static str, u32)> {
    let normalized = name.nfc().collect::<String>();
    let clean = normalized.trim().to_ascii_uppercase();
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)(?:^|.*-RFI-|\b)(CIV|MEC|ELE|PRC)[-_ ]*0*([1-9]\d*)\b").unwrap()
    });
    let caps = re.captures(&clean)?;
    let disc_str = caps.get(1)?.as_str().to_ascii_uppercase();
    let num: u32 = caps.get(2)?.as_str().parse().ok()?;
    let matched_disc = match disc_str.as_str() {
        "CIV" => "CIV",
        "MEC" => "MEC",
        "ELE" => "ELE",
        "PRC" => "PRC",
        _ => return None,
    };
    Some((matched_disc, num))
}

pub(crate) fn scan_system_root(root: &Path) -> Result<Vec<ScannedRfiRecord>, BackendError> {
    if !root.is_dir() {
        return Err(BackendError::path_invalid(format!(
            "RFI system root is not a directory: {}",
            root.display()
        )));
    }
    let mut scanned_rfis = Vec::new();
    let mut seen_canonical = BTreeSet::new();

    let disc_subdirs = ["CIV", "MEC", "ELE", "PRC"];
    let mut search_dirs = Vec::new();

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_uppercase();
                for &d in &disc_subdirs {
                    if file_name == d {
                        search_dirs.push((d, path.clone()));
                    }
                }
            }
        }
    }

    if search_dirs.is_empty() {
        let root_name = root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_uppercase();
        for &d in &disc_subdirs {
            if root_name == d {
                search_dirs.push((d, root.to_path_buf()));
            }
        }
    }

    let temp_dir = tempfile::tempdir().map_err(|e| {
        BackendError::protocol(format!("Failed to create temporary directory: {e}"))
    })?;

    for (discipline, disc_path) in search_dirs {
        let Ok(entries) = fs::read_dir(&disc_path) else {
            continue;
        };
        for entry in entries.flatten() {
            let rfi_dir = entry.path();
            if !rfi_dir.is_dir() {
                continue;
            }
            let dir_name = rfi_dir.file_name().unwrap_or_default().to_string_lossy();
            let Some((disc, num)) = parse_rfi_discipline_and_number(&dir_name) else {
                continue;
            };
            if disc != discipline {
                continue;
            }
            let canonical = format!("{}{}-{:04}", canonical_rfi_prefix(), disc, num);
            let short = format!("{}-{:04}", disc, num);
            if !seen_canonical.insert(canonical.clone()) {
                continue;
            }

            let mut scan_pdf = None;
            if let Ok(files) = fs::read_dir(&rfi_dir) {
                let mut candidates = Vec::new();
                for f in files.flatten() {
                    let f_path = f.path();
                    if f_path.is_file() {
                        let f_name = f_path.file_name().unwrap_or_default().to_string_lossy();
                        if f_name.to_ascii_lowercase().ends_with(".pdf") {
                            candidates.push((f_name.to_string(), f_path));
                        }
                    }
                }
                candidates.sort_by(|a, b| a.0.cmp(&b.0));
                for (name, path) in &candidates {
                    let upper = name.to_ascii_uppercase();
                    if upper.starts_with("RFI NO") || upper.starts_with("RFI_NO") {
                        scan_pdf = Some(path.clone());
                        break;
                    }
                }
                if scan_pdf.is_none() {
                    scan_pdf = candidates.first().map(|(_, path)| path.clone());
                }
            }

            let (signed, reason) = if let Some(ref pdf) = scan_pdf {
                detect_worley_signature(pdf, temp_dir.path())
            } else {
                (false, "No scanned PDF found in folder".to_string())
            };

            scanned_rfis.push(ScannedRfiRecord {
                discipline: disc.to_string(),
                number: num,
                canonical_rfi: canonical,
                short_rfi: short,
                rfi_folder: rfi_dir.to_string_lossy().to_string(),
                pdf_path: scan_pdf.map(|p| p.to_string_lossy().to_string()),
                signed_by_worley: signed,
                signature_reason: reason,
            });
        }
    }

    scanned_rfis.sort_by(|a, b| {
        a.discipline
            .cmp(&b.discipline)
            .then_with(|| a.number.cmp(&b.number))
    });

    Ok(scanned_rfis)
}

pub(crate) fn detect_worley_signature(pdf_path: &Path, temp_dir: &Path) -> (bool, String) {
    let png_name = format!("{}.png", Uuid::new_v4());
    let png_path = temp_dir.join(png_name);
    if let Err(e) = crate::ocr::render_first_page_png(pdf_path, &png_path, 150) {
        return (false, format!("PDF render error: {e}"));
    }
    let gray_image = match crate::ocr::read_png_gray(&png_path) {
        Ok(img) => img,
        Err(e) => return (false, format!("Image read error: {e}")),
    };
    let _ = fs::remove_file(&png_path);

    let w = gray_image.width;
    let h = gray_image.height;
    if w < 100 || h < 100 {
        return (false, "Image dimensions too small".to_string());
    }

    let sig_x_start = (w as f64 * 0.65) as usize;
    let sig_x_end = (w as f64 * 0.96) as usize;
    let sig_y_start = (h as f64 * 0.76) as usize;
    let sig_y_end = (h as f64 * 0.93) as usize;

    let mut dark_count = 0usize;
    let mut total_count = 0usize;

    for y in sig_y_start..sig_y_end.min(h) {
        for x in sig_x_start..sig_x_end.min(w) {
            let pixel = gray_image.pixels[y * w + x];
            total_count += 1;
            if pixel < 175 {
                dark_count += 1;
            }
        }
    }

    if total_count == 0 {
        return (false, "Empty inspection area".to_string());
    }

    let dark_ratio = dark_count as f64 / total_count as f64;
    let is_signed = dark_count >= 160 && dark_ratio >= 0.007;

    if is_signed {
        (
            true,
            format!(
                "Signed by Worley (ink detected: {dark_count} px, {:.1}%)",
                dark_ratio * 100.0
            ),
        )
    } else {
        (
            false,
            format!(
                "Unsigned / Box empty (ink: {dark_count} px, {:.2}%)",
                dark_ratio * 100.0
            ),
        )
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct RfiLogRowInfo {
    pub sheet: String,
    pub row: u32,
    pub rfi_no: String,
    pub canonical_rfi: String,
    pub discipline: String,
    pub number: u32,
    pub itp_no: String,
    pub item_no: String,
    pub tag_no: String,
    pub area: String,
    pub location: String,
    pub current_remarks: String,
    pub remarks_col: u32,
}

pub(crate) fn read_rfi_log_data<R: Read + Seek>(
    reader: &mut R,
) -> Result<Vec<RfiLogRowInfo>, BackendError> {
    let workbook = read_workbook_from_reader(reader)
        .map_err(|e| BackendError::validation(format!("Failed to read RFI Log workbook: {e}")))?;

    let mut rows = Vec::new();
    for &discipline in &DISCIPLINES {
        let Some(layout) = layout(discipline) else {
            continue;
        };
        let Some(sheet) = workbook.sheet(layout.sheet) else {
            continue;
        };

        for row in 10..=sheet.max_row {
            let rfi_raw = sheet.value(row, 1).trim();
            if rfi_raw.is_empty() {
                continue;
            }
            let Some((disc, num)) = parse_rfi_discipline_and_number(rfi_raw) else {
                continue;
            };
            if disc != discipline {
                continue;
            }
            let canonical = format!("{}{}-{:04}", canonical_rfi_prefix(), disc, num);
            let item_no = sheet.value(row, 3).trim().to_string();
            let tag_no = sheet.value(row, 6).trim().to_string();
            let area = sheet.value(row, 7).trim().to_string();
            let location = sheet.value(row, layout.location_col).trim().to_string();
            let itp_no = sheet.value(row, 2).trim().to_string();
            let current_remarks = sheet.value(row, layout.remarks_col).trim().to_string();

            rows.push(RfiLogRowInfo {
                sheet: layout.sheet.to_string(),
                row,
                rfi_no: rfi_raw.to_string(),
                canonical_rfi: canonical,
                discipline: disc.to_string(),
                number: num,
                itp_no,
                item_no,
                tag_no,
                area,
                location,
                current_remarks,
                remarks_col: layout.remarks_col,
            });
        }
    }

    Ok(rows)
}

#[derive(Clone, Debug)]
pub(crate) struct CivilFollowUpLayout {
    pub sheet_name: String,
    pub header_row: u32,
    pub col_3_1: u32,
    pub col_area: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct MechanicalFollowUpLayout {
    pub sheet_name: String,
    pub header_row: u32,
    pub col_3_5: Option<u32>,
    pub col_3_1: Option<u32>,
    pub col_3_2: Option<u32>,
    pub col_pcs: Option<u32>,
    pub col_section: Option<u32>,
    pub col_area: Option<u32>,
}

fn clean_match(s: &str) -> String {
    s.nfc().collect::<String>().trim().to_ascii_uppercase()
}

pub(crate) fn find_civil_layout(
    sheet_name: &str,
    sheet: &WorksheetData,
) -> Option<CivilFollowUpLayout> {
    for row in 1..=sheet.max_row.min(10) {
        let mut col_3_1 = None;
        let mut col_area = None;
        for col in 1..=sheet.max_column.min(60) {
            let val = sheet.value(row, col).trim();
            let clean = clean_match(val);
            if clean == "3.1" || clean.contains("3.1") {
                col_3_1 = Some(col);
            }
            if clean == "AREA" || (clean.contains("AREA") && !clean.contains("TOTAL")) {
                col_area = Some(col);
            }
        }
        if let (Some(c31), Some(carea)) = (col_3_1, col_area) {
            return Some(CivilFollowUpLayout {
                sheet_name: sheet_name.to_string(),
                header_row: row,
                col_3_1: c31,
                col_area: carea,
            });
        }
    }
    None
}

pub(crate) fn find_mechanical_layout(
    sheet_name: &str,
    sheet: &WorksheetData,
) -> Option<MechanicalFollowUpLayout> {
    for row in 1..=sheet.max_row.min(10) {
        let mut col_3_5 = None;
        let mut col_3_1 = None;
        let mut col_3_2 = None;
        let mut col_pcs = None;
        let mut col_section = None;
        let mut col_area = None;

        for col in 1..=sheet.max_column.min(80) {
            let val = sheet.value(row, col).trim();
            let clean = clean_match(val);
            if clean == "3.5" || clean.contains("3.5") {
                col_3_5 = Some(col);
            }
            if clean == "3.1" || clean.contains("3.1") {
                col_3_1 = Some(col);
            }
            if clean == "3.2" || clean.contains("3.2") {
                col_3_2 = Some(col);
            }
            if clean.contains("PCS") {
                col_pcs = Some(col);
            }
            if clean.contains("SECTION") {
                col_section = Some(col);
            }
            if clean == "AREA"
                || (clean.contains("AREA")
                    && !clean.contains("TOTAL")
                    && !clean.contains("SECTION"))
            {
                col_area = Some(col);
            }
        }

        if (col_3_5.is_some() || col_3_1.is_some() || col_3_2.is_some())
            && (col_pcs.is_some() || col_area.is_some())
        {
            return Some(MechanicalFollowUpLayout {
                sheet_name: sheet_name.to_string(),
                header_row: row,
                col_3_5,
                col_3_1,
                col_3_2,
                col_pcs,
                col_section,
                col_area,
            });
        }
    }
    None
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
    let rfi_log_rows = read_rfi_log_data(rfi_log_file)?;

    follow_up_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let follow_up_fingerprint = fingerprint_open_file(follow_up_file)?;
    follow_up_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| BackendError::protocol(e.to_string()))?;
    let follow_up_wb = read_workbook_from_reader(follow_up_file)
        .map_err(|e| BackendError::validation(format!("Failed to read Follow Up workbook: {e}")))?;

    let civ_follow_up_layout = follow_up_wb
        .sheet("Signed by CIV")
        .map(|s| ("Signed by CIV", s))
        .or_else(|| {
            follow_up_wb
                .sheets
                .iter()
                .find(|(name, _)| clean_match(name) == "SIGNED BY CIV")
                .map(|(name, s)| (name.as_str(), s))
        })
        .and_then(|(name, sheet)| find_civil_layout(name, sheet));

    let mec_follow_up_layout = follow_up_wb
        .sheet("Signed by MEC")
        .map(|s| ("Signed by MEC", s))
        .or_else(|| {
            follow_up_wb
                .sheets
                .iter()
                .find(|(name, _)| clean_match(name) == "SIGNED BY MEC")
                .map(|(name, s)| (name.as_str(), s))
        })
        .and_then(|(name, sheet)| find_mechanical_layout(name, sheet));

    let mut rfi_log_updates: BTreeMap<String, BTreeMap<String, GuardedCellUpdateDto>> =
        BTreeMap::new();
    let mut follow_up_updates: BTreeMap<String, BTreeMap<String, GuardedCellUpdateDto>> =
        BTreeMap::new();

    let mut review_rows = Vec::new();
    let mut row_idx = 0usize;

    for scanned in &scanned_rfis {
        row_idx += 1;
        let canonical = &scanned.canonical_rfi;
        let matching_log_rows: Vec<_> = rfi_log_rows
            .iter()
            .filter(|r| r.canonical_rfi == *canonical)
            .collect();

        let mut rfi_log_action = "Not found in RFI Log".to_string();
        let mut follow_up_action = "None".to_string();
        let mut disposition = "rfi.systemSync.disposition.unsigned".to_string();
        let mut reason_key = scanned.signature_reason.clone();

        if !scanned.signed_by_worley {
            if !matching_log_rows.is_empty() {
                rfi_log_action = "Unsigned (remarks unchanged)".to_string();
            }
            review_rows.push(SystemSyncRow {
                row_id: format!("sync-row-{row_idx}"),
                discipline: scanned.discipline.clone(),
                rfi_no: scanned.short_rfi.clone(),
                canonical_rfi: canonical.clone(),
                worley_status: "Unsigned".to_string(),
                rfi_log_action,
                follow_up_action,
                disposition,
                reason_key,
                enabled: false,
                selectable: false,
            });
            continue;
        }

        let mut has_log_update = false;
        for log_row in &matching_log_rows {
            if clean_match(&log_row.current_remarks) == clean_match(WORLEY_SIGNED_MARKER) {
                rfi_log_action = "Already signed in RFI Log".to_string();
            } else {
                rfi_log_action = format!(
                    "Set Remarks ({}:{}) -> '{}'",
                    log_row.sheet, log_row.row, WORLEY_SIGNED_MARKER
                );
                let sheet_updates = rfi_log_updates.entry(log_row.sheet.clone()).or_default();
                sheet_updates.insert(
                    format!("{}:{}", log_row.row, log_row.remarks_col),
                    GuardedCellUpdateDto {
                        expected_value: log_row.current_remarks.clone(),
                        replacement_value: WORLEY_SIGNED_MARKER.to_string(),
                        row: log_row.row,
                        col: log_row.remarks_col,
                    },
                );
                has_log_update = true;
            }

            if scanned.discipline == "CIV" {
                if log_row.item_no == "2.3.1" || log_row.item_no.starts_with("2.3.1") {
                    if let Some(ref layout) = civ_follow_up_layout {
                        if let Some(fu_sheet) = follow_up_wb.sheet(&layout.sheet_name) {
                            let mut found_match = false;
                            for r in (layout.header_row + 1)..=fu_sheet.max_row {
                                let fu_area = fu_sheet.value(r, layout.col_area).trim();
                                if !fu_area.is_empty()
                                    && clean_match(fu_area) == clean_match(&log_row.tag_no)
                                {
                                    found_match = true;
                                    let current_val = fu_sheet.value(r, layout.col_3_1).trim();
                                    if current_val.contains(&scanned.short_rfi) {
                                        follow_up_action = format!(
                                            "Already present in {} (row {r})",
                                            layout.sheet_name
                                        );
                                    } else {
                                        let new_val = if current_val.is_empty() {
                                            scanned.short_rfi.clone()
                                        } else {
                                            format!("{current_val}, {}", scanned.short_rfi)
                                        };
                                        follow_up_action = format!(
                                            "Add to {} (row {r}, col 3.1)",
                                            layout.sheet_name
                                        );
                                        let fu_sheet_updates = follow_up_updates
                                            .entry(layout.sheet_name.clone())
                                            .or_default();
                                        fu_sheet_updates.insert(
                                            format!("{r}:{}", layout.col_3_1),
                                            GuardedCellUpdateDto {
                                                expected_value: current_val.to_string(),
                                                replacement_value: new_val,
                                                row: r,
                                                col: layout.col_3_1,
                                            },
                                        );
                                    }
                                    break;
                                }
                            }
                            if !found_match {
                                follow_up_action = format!(
                                    "No matching row in {} (Tag No: '{}')",
                                    layout.sheet_name, log_row.tag_no
                                );
                            }
                        }
                    } else {
                        follow_up_action = "Sheet 'Signed by CIV' layout not found".to_string();
                    }
                } else {
                    follow_up_action = format!("Not Final ITP (Item {})", log_row.item_no);
                }
            } else if scanned.discipline == "MEC" {
                let is_itp_135 = log_row.item_no == "1.3.5" || log_row.item_no.starts_with("1.3.5");
                let is_itp_331 = log_row.item_no == "3.3.1" || log_row.item_no.starts_with("3.3.1");
                let is_itp_432 = log_row.item_no == "4.3.2" || log_row.item_no.starts_with("4.3.2");

                if is_itp_135 || is_itp_331 || is_itp_432 {
                    if let Some(ref layout) = mec_follow_up_layout {
                        let target_col = if is_itp_135 {
                            layout.col_3_5
                        } else if is_itp_331 {
                            layout.col_3_1
                        } else {
                            layout.col_3_2
                        };

                        if let Some(t_col) = target_col {
                            if let Some(fu_sheet) = follow_up_wb.sheet(&layout.sheet_name) {
                                let mut found_match = false;
                                for r in (layout.header_row + 1)..=fu_sheet.max_row {
                                    let fu_pcs = layout
                                        .col_pcs
                                        .map(|c| fu_sheet.value(r, c).trim())
                                        .unwrap_or_default();
                                    let fu_sec = layout
                                        .col_section
                                        .map(|c| fu_sheet.value(r, c).trim())
                                        .unwrap_or_default();
                                    let fu_area = layout
                                        .col_area
                                        .map(|c| fu_sheet.value(r, c).trim())
                                        .unwrap_or_default();

                                    let pcs_match = layout.col_pcs.is_none()
                                        || (!fu_pcs.is_empty()
                                            && clean_match(fu_pcs) == clean_match(&log_row.tag_no));
                                    let sec_match = layout.col_section.is_none()
                                        || (!fu_sec.is_empty()
                                            && clean_match(fu_sec) == clean_match(&log_row.area));
                                    let area_match = layout.col_area.is_none()
                                        || (!fu_area.is_empty()
                                            && clean_match(fu_area)
                                                == clean_match(&log_row.location));

                                    if pcs_match && sec_match && area_match {
                                        found_match = true;
                                        let current_val = fu_sheet.value(r, t_col).trim();
                                        if current_val.contains(&scanned.short_rfi) {
                                            follow_up_action = format!(
                                                "Already present in {} (row {r})",
                                                layout.sheet_name
                                            );
                                        } else {
                                            let new_val = if current_val.is_empty() {
                                                scanned.short_rfi.clone()
                                            } else {
                                                format!("{current_val}, {}", scanned.short_rfi)
                                            };
                                            follow_up_action =
                                                format!("Add to {} (row {r})", layout.sheet_name);
                                            let fu_sheet_updates = follow_up_updates
                                                .entry(layout.sheet_name.clone())
                                                .or_default();
                                            fu_sheet_updates.insert(
                                                format!("{r}:{t_col}"),
                                                GuardedCellUpdateDto {
                                                    expected_value: current_val.to_string(),
                                                    replacement_value: new_val,
                                                    row: r,
                                                    col: t_col,
                                                },
                                            );
                                        }
                                        break;
                                    }
                                }
                                if !found_match {
                                    follow_up_action = format!(
                                        "No matching row in {} (PCS: '{}', Sec: '{}', Area: '{}')",
                                        layout.sheet_name,
                                        log_row.tag_no,
                                        log_row.area,
                                        log_row.location
                                    );
                                }
                            }
                        } else {
                            follow_up_action =
                                "Target item column not found in Signed by MEC".to_string();
                        }
                    } else {
                        follow_up_action = "Sheet 'Signed by MEC' layout not found".to_string();
                    }
                } else {
                    follow_up_action = format!("Not Final ITP (Item {})", log_row.item_no);
                }
            } else {
                follow_up_action = "Excluded (ELE/PRC not synced to Follow Up)".to_string();
            }
        }

        if has_log_update || follow_up_action.starts_with("Add to") {
            disposition = "rfi.systemSync.disposition.ready".to_string();
            reason_key = "Worley signed; ready to synchronize".to_string();
        } else if rfi_log_action.starts_with("Already")
            && (follow_up_action.starts_with("Already")
                || follow_up_action.starts_with("Not Final")
                || follow_up_action.starts_with("Excluded"))
        {
            disposition = "rfi.systemSync.disposition.alreadySynced".to_string();
            reason_key = "Already synchronized".to_string();
        }

        let is_ready = disposition == "rfi.systemSync.disposition.ready";
        review_rows.push(SystemSyncRow {
            row_id: format!("sync-row-{row_idx}"),
            discipline: scanned.discipline.clone(),
            rfi_no: scanned.short_rfi.clone(),
            canonical_rfi: canonical.clone(),
            worley_status: "Signed by Worley".to_string(),
            rfi_log_action,
            follow_up_action,
            disposition,
            reason_key,
            enabled: is_ready,
            selectable: is_ready,
        });
    }

    let mut steps = Vec::new();
    let mut warnings = Vec::new();

    let rfi_log_ext = rfi_log_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("xlsm");
    let rfi_log_backup = rfi_log_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(
            ".docflow-system-sync-log-backup-{}.{}",
            Uuid::new_v4().simple(),
            rfi_log_ext
        ));

    let follow_up_ext = follow_up_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("xlsx");
    let follow_up_backup = follow_up_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(
            ".docflow-system-sync-fu-backup-{}.{}",
            Uuid::new_v4().simple(),
            follow_up_ext
        ));

    let rfi_log_updates_count: usize = rfi_log_updates.values().map(|m| m.len()).sum();
    let follow_up_updates_count: usize = follow_up_updates.values().map(|m| m.len()).sum();

    if rfi_log_updates_count > 0 {
        let mut meta = JsonObject::new();
        let step_meta = SystemSyncStepMeta {
            target_path: rfi_log_path.to_string_lossy().to_string(),
            backup_path: rfi_log_backup.to_string_lossy().to_string(),
            source_fingerprint: rfi_log_fingerprint,
            updates: rfi_log_updates,
            summary: format!("Update {rfi_log_updates_count} REMARKS in RFI Log"),
        };
        meta.insert(
            "syncStep".to_string(),
            serde_json::to_value(&step_meta).map_err(|e| BackendError::protocol(e.to_string()))?,
        );
        steps.push(Step {
            kind: StepKind::WorkbookUpdate,
            src: Some(rfi_log_path.to_string_lossy().to_string()),
            dst: Some(rfi_log_backup.to_string_lossy().to_string()),
            note_key: Some("note.rfi.systemSync.rfiLog".to_string()),
            meta,
        });
    }

    if follow_up_updates_count > 0 {
        let mut meta = JsonObject::new();
        let step_meta = SystemSyncStepMeta {
            target_path: follow_up_path.to_string_lossy().to_string(),
            backup_path: follow_up_backup.to_string_lossy().to_string(),
            source_fingerprint: follow_up_fingerprint,
            updates: follow_up_updates,
            summary: format!("Update {follow_up_updates_count} cells in Follow Up"),
        };
        meta.insert(
            "syncStep".to_string(),
            serde_json::to_value(&step_meta).map_err(|e| BackendError::protocol(e.to_string()))?,
        );
        steps.push(Step {
            kind: StepKind::WorkbookUpdate,
            src: Some(follow_up_path.to_string_lossy().to_string()),
            dst: Some(follow_up_backup.to_string_lossy().to_string()),
            note_key: Some("note.rfi.systemSync.followUp".to_string()),
            meta,
        });
    }

    if steps.is_empty() {
        warnings.push(Warning {
            key: "warn.rfi.systemSync.noChangesNeeded".to_string(),
            subject: "All detected RFIs are already synchronized or unsigned".to_string(),
        });
    }

    let columns = vec![
        json!({
            "key": "enabled",
            "labelKey": "rfi.systemSync.column.enabled",
            "kind": "bool",
            "bulk": false,
            "disabledWhen": {"field": "selectable", "equals": false},
        }),
        json!({"key": "discipline", "labelKey": "rfi.systemSync.column.discipline", "kind": "readonly"}),
        json!({"key": "rfiNo", "labelKey": "rfi.systemSync.column.rfiNo", "kind": "readonly"}),
        json!({"key": "worleyStatus", "labelKey": "rfi.systemSync.column.worleyStatus", "kind": "readonly"}),
        json!({"key": "rfiLogAction", "labelKey": "rfi.systemSync.column.rfiLogAction", "kind": "readonly"}),
        json!({"key": "followUpAction", "labelKey": "rfi.systemSync.column.followUpAction", "kind": "readonly"}),
        json!({
            "key": "disposition",
            "labelKey": "rfi.systemSync.column.disposition",
            "kind": "readonly",
            "display": "localizedKey",
        }),
        json!({
            "key": "reasonKey",
            "labelKey": "rfi.systemSync.column.reason",
            "kind": "readonly",
            "detail": true,
        }),
    ];

    let editor = json!({
        "kind": "recordTable",
        "layout": "compact",
        "titleKey": "rfi.systemSync.editor.title",
        "rowCountKey": "rfi.systemSync.editor.rowCount",
        "requiredFields": ["enabled"],
        "groupField": "discipline",
        "selectionGroupFields": [],
        "columns": columns,
        "rows": review_rows,
        "duplicateEnabled": false,
        "selectionEnabled": false,
        "bulkEnabled": false,
        "reorderable": false,
        "detailsEnabled": true,
        "detailsDefaultVisible": false,
        "copyEnabled": false,
    });

    let mut plan_meta = JsonObject::new();
    plan_meta.insert("editor".to_string(), editor);

    Ok(Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps,
        warnings,
        meta: plan_meta,
    })
}

pub(crate) fn execute_sync_step(
    step: &Step,
    source_path: &Path,
    dest_path: &Path,
) -> Result<(), BackendError> {
    let step_meta_val = step
        .meta
        .get("syncStep")
        .ok_or_else(|| BackendError::validation("syncStep metadata missing in plan step"))?;
    let step_meta: SystemSyncStepMeta = serde_json::from_value(step_meta_val.clone())
        .map_err(|e| BackendError::validation(format!("Invalid syncStep metadata: {e}")))?;

    let mut updates: BTreeMap<String, BTreeMap<(u32, u32), CellUpdate>> = BTreeMap::new();
    for (sheet, updates_map) in &step_meta.updates {
        let sheet_map = updates.entry(sheet.clone()).or_default();
        for update_dto in updates_map.values() {
            sheet_map.insert(
                (update_dto.row, update_dto.col),
                CellUpdate::Text(update_dto.replacement_value.clone()),
            );
        }
    }

    crate::ooxml::patch_workbook_sheets(source_path, dest_path, &updates, None)
        .map_err(|e| BackendError::validation(format!("Failed to patch workbook: {e}")))?;

    // Verify patched workbook
    let mut patched_file = File::open(dest_path)
        .map_err(|e| BackendError::validation(format!("Failed to open patched workbook: {e}")))?;
    let workbook_data = read_workbook_from_reader(&mut patched_file)
        .map_err(|e| BackendError::validation(format!("Failed to read patched workbook: {e}")))?;

    for (sheet, updates_map) in &step_meta.updates {
        let sheet_data = workbook_data.sheet(sheet).ok_or_else(|| {
            BackendError::validation(format!("Patched worksheet missing: {sheet}"))
        })?;
        for update_dto in updates_map.values() {
            let actual_value = sheet_data.value(update_dto.row, update_dto.col);
            if actual_value != update_dto.replacement_value {
                return Err(BackendError::validation(format!(
                    "Patched cell verification failed at {}!{}:{}: expected '{}', got '{}'",
                    sheet,
                    update_dto.row,
                    update_dto.col,
                    update_dto.replacement_value,
                    actual_value
                )));
            }
        }
    }

    Ok(())
}
