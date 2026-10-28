use super::*;

mod editor;
pub(super) use editor::*;

pub(super) fn matches_discipline_sheet(sheet_name: &str, discipline: &str) -> bool {
    let name = sheet_name.trim().to_uppercase();
    if name == discipline {
        return true;
    }
    let tokens: Vec<&str> = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '.' || c == '/')
        .filter(|t| !t.is_empty())
        .collect();
    let has_token = |patterns: &[&str]| {
        tokens
            .iter()
            .any(|t| patterns.iter().any(|p| *t == *p || t.starts_with(p)))
    };
    match discipline {
        "CIV" => {
            name.starts_with("CIV")
                || has_token(&[
                    "CIV",
                    "CIVIL",
                    "INSAAT",
                    "İNŞAAT",
                    "STRUCT",
                    "STRUCTURAL",
                    "CONCRETE",
                ])
        }
        "MEC" => {
            name.starts_with("MEC")
                || has_token(&[
                    "MEC",
                    "MECH",
                    "MECHANICAL",
                    "PIPE",
                    "PIPING",
                    "MEKANIK",
                    "MEKANİK",
                    "HVAC",
                ])
        }
        "ELE" => {
            name.starts_with("ELE")
                || has_token(&[
                    "ELE",
                    "ELEC",
                    "ELECTRICAL",
                    "INST",
                    "INSTRUMENTATION",
                    "ELEKTRIK",
                    "ELEKTRİK",
                ])
        }
        "PRC" => {
            name.starts_with("PRC")
                || name.starts_with("PROC")
                || has_token(&["PRC", "PROC", "PROCESS", "PROSES", "COMMISSIONING"])
        }
        _ => false,
    }
}

pub(super) fn detect_rfi_and_status_columns(
    worksheet: &WorksheetData,
    default_rfi_col: u32,
    default_status_col: u32,
) -> (u32, u32) {
    let scan_rows = worksheet.max_row.min(100);
    let has_rfi_in_default = (1..=scan_rows).any(|row| {
        worksheet.cell(row, default_rfi_col).is_some_and(|cell| {
            canonical_rfi(&cell.value).is_some() || canonical_follow_up_rfi(&cell.value).is_some()
        })
    });
    let rfi_column = if has_rfi_in_default {
        default_rfi_col
    } else {
        (1..=10.min(worksheet.max_row))
            .find_map(|row| {
                (1..=64.min(worksheet.max_column)).find(|&col| {
                    worksheet
                        .cell(row, col)
                        .is_some_and(|cell| is_follow_up_rfi_header(&normalize_header(&cell.value)))
                })
            })
            .or_else(|| {
                (1..=64.min(worksheet.max_column)).find(|&col| {
                    (1..=scan_rows).any(|row| {
                        worksheet.cell(row, col).is_some_and(|cell| {
                            canonical_rfi(&cell.value).is_some()
                                || canonical_follow_up_rfi(&cell.value).is_some()
                        })
                    })
                })
            })
            .unwrap_or(default_rfi_col)
    };

    let mut detected_status = None;
    for row in 1..=worksheet.max_row.min(10) {
        for col in 1..=worksheet.max_column.min(64) {
            if col == rfi_column {
                continue;
            }
            if let Some(cell) = worksheet.cell(row, col) {
                let token = normalize_header(&cell.value);
                if is_follow_up_worley_status_header(&token)
                    || is_follow_up_generic_status_header(&token)
                    || token == "NOTLAR"
                    || token == "AÇIKLAMA"
                    || token == "ACIKLAMA"
                {
                    detected_status = Some(col);
                    break;
                }
            }
        }
        if detected_status.is_some() {
            break;
        }
    }
    let status_column = detected_status.unwrap_or(default_status_col);
    (rfi_column, status_column)
}

pub(super) fn scan_source(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
) -> Result<BTreeMap<String, Vec<SourceStatusEvidence>>, BackendError> {
    if discovery.safety.has_workbook_protection {
        return Err(protected());
    }
    let mut grouped = BTreeMap::<String, Vec<SourceStatusEvidence>>::new();
    let mut scanned_sheets = BTreeSet::new();
    for projection in status_columns() {
        let descriptor = match discovery
            .worksheets
            .iter()
            .find(|worksheet| worksheet.name.trim().eq_ignore_ascii_case(projection.sheet))
            .or_else(|| {
                discovery
                    .worksheets
                    .iter()
                    .find(|worksheet| matches_discipline_sheet(&worksheet.name, projection.sheet))
            }) {
            Some(d) => d,
            None => continue,
        };
        let worksheet = match workbook.sheet(&descriptor.name) {
            Some(w) => w,
            None => continue,
        };
        if !scanned_sheets.insert(descriptor.name.clone()) {
            continue;
        }
        if worksheet.has_sheet_protection {
            return Err(protected());
        }
        validate_cell_text(worksheet)?;
        let last_allowed = SOURCE_FIRST_DATA_ROW + MAX_SOURCE_DATA_ROWS_PER_SHEET - 1;
        if worksheet.max_row > last_allowed {
            return Err(limit_exceeded_for(format!(
                "RFI Log sheet '{}' reaches row {}; at most row {last_allowed}",
                descriptor.name, worksheet.max_row
            )));
        }
        let (rfi_col, status_col) = detect_rfi_and_status_columns(
            worksheet,
            projection.rfi_column,
            projection.status_column,
        );
        for row in 1..=worksheet.max_row {
            let Some(rfi_cell) = worksheet.cell(row, rfi_col) else {
                continue;
            };
            let Some(rfi) = canonical_rfi(&rfi_cell.value).or_else(|| {
                canonical_follow_up_rfi(&rfi_cell.value).map(|(canonical, _)| canonical)
            }) else {
                continue;
            };
            let status_cell = worksheet.cell(row, status_col);
            let evidence = grouped.entry(rfi).or_default();
            if evidence.len() >= MAX_SOURCE_EVIDENCE_PER_RFI {
                return Err(limit_exceeded_for(format!(
                    "one RFI appears in more than {MAX_SOURCE_EVIDENCE_PER_RFI} RFI Log rows"
                )));
            }
            evidence.push(SourceStatusEvidence {
                sheet: descriptor.name.clone(),
                row,
                rfi_column: rfi_col,
                status_column: status_col,
                rfi_cell: rfi_cell.into(),
                status_cell: status_cell.map(Into::into),
            });
        }
    }

    if scanned_sheets.is_empty() {
        // Fallback for consolidated or custom RFI Log workbooks (e.g. single-sheet "RFI Log", "RFIs", "Sheet1")
        for descriptor in &discovery.worksheets {
            if descriptor.visibility != WorksheetVisibility::Visible {
                continue;
            }
            let Some(worksheet) = workbook.sheet(&descriptor.name) else {
                continue;
            };
            if worksheet.has_sheet_protection {
                continue;
            }
            let (rfi_col, status_col) = detect_rfi_and_status_columns(worksheet, 1, 20);
            let last_allowed = SOURCE_FIRST_DATA_ROW + MAX_SOURCE_DATA_ROWS_PER_SHEET - 1;
            if worksheet.max_row > last_allowed {
                return Err(limit_exceeded_for(format!(
                    "RFI Log sheet '{}' reaches row {}; at most row {last_allowed}",
                    descriptor.name, worksheet.max_row
                )));
            }
            let mut sheet_had_rfis = false;
            for row in 1..=worksheet.max_row {
                let Some(rfi_cell) = worksheet.cell(row, rfi_col) else {
                    continue;
                };
                let Some(rfi) = canonical_rfi(&rfi_cell.value).or_else(|| {
                    canonical_follow_up_rfi(&rfi_cell.value).map(|(canonical, _)| canonical)
                }) else {
                    continue;
                };
                let status_cell = worksheet.cell(row, status_col);
                let evidence = grouped.entry(rfi).or_default();
                if evidence.len() >= MAX_SOURCE_EVIDENCE_PER_RFI {
                    return Err(limit_exceeded_for(format!(
                        "one RFI appears in more than {MAX_SOURCE_EVIDENCE_PER_RFI} RFI Log rows"
                    )));
                }
                evidence.push(SourceStatusEvidence {
                    sheet: descriptor.name.clone(),
                    row,
                    rfi_column: rfi_col,
                    status_column: status_col,
                    rfi_cell: rfi_cell.into(),
                    status_cell: status_cell.map(Into::into),
                });
                sheet_had_rfis = true;
            }
            if sheet_had_rfis {
                validate_cell_text(worksheet)?;
                scanned_sheets.insert(descriptor.name.clone());
            }
        }
    }

    if scanned_sheets.is_empty() {
        return Err(BackendError::validation(
            "error.rfi.followUp.workbookInvalid: no sheets containing valid RFI entries found in RFI Log",
        ));
    }
    Ok(grouped)
}

pub(super) fn is_preferred_follow_up_sheet_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let trimmed = upper.trim();
    trimmed.contains("SIGNED BY")
        || trimmed.contains("FOLLOW UP")
        || trimmed.contains("FOLLOW-UP")
        || trimmed.contains("FOLLOWUP")
        || trimmed == "CIV"
        || trimmed == "MEC"
        || trimmed == "ELE"
        || trimmed == "PRC"
        || trimmed.starts_with("CIV ")
        || trimmed.starts_with("MEC ")
        || trimmed.starts_with("ELE ")
        || trimmed.starts_with("PRC ")
        || trimmed.ends_with(" CIV")
        || trimmed.ends_with(" MEC")
        || trimmed.ends_with(" ELE")
        || trimmed.ends_with(" PRC")
        || trimmed.ends_with("-CIV")
        || trimmed.ends_with("-MEC")
        || trimmed.ends_with("-ELE")
        || trimmed.ends_with("-PRC")
}

pub(super) fn discover_layouts(
    discovery: &OrderedWorkbookDiscovery,
    workbook: &WorkbookData,
    has_signature: bool,
    merge_budget: &mut MergeQueryBudget,
) -> Result<Vec<FollowUpLayout>, BackendError> {
    if has_signature {
        return Err(signed_package());
    }
    if discovery.safety.has_workbook_protection {
        return Err(protected());
    }
    let mut candidates = Vec::new();
    let mut duplicate_header_sheets = BTreeSet::new();
    for descriptor in &discovery.worksheets {
        let worksheet = workbook
            .sheet(&descriptor.name)
            .ok_or_else(workbook_invalid)?;
        validate_cell_text(worksheet)?;
        for row in 1..=worksheet.max_row.min(MAX_HEADER_ROWS) {
            let mut rfi_columns = Vec::new();
            let mut status_columns = Vec::new();
            for column in 1..=MAX_HEADER_COLUMNS {
                let Some(cell) = worksheet.cell(row, column) else {
                    continue;
                };
                let token = normalize_header(&cell.value);
                if token != "RFI NO." && token != "STATUS" {
                    continue;
                }
                let safe =
                    cell.formula.is_none() && !merge_budget.contains(worksheet, row, column)?;
                if token == "RFI NO." {
                    rfi_columns.push((column, safe));
                } else {
                    status_columns.push((column, safe));
                }
            }
            if rfi_columns.len() > 1 || status_columns.len() > 1 {
                duplicate_header_sheets.insert(descriptor.name.clone());
            }
            if rfi_columns.len() == 1
                && status_columns.len() == 1
                && rfi_columns[0].1
                && status_columns[0].1
            {
                if descriptor.visibility != WorksheetVisibility::Visible
                    || worksheet.has_sheet_protection
                {
                    return Err(protected());
                }
                candidates.push(FollowUpLayout {
                    worksheet: descriptor.clone(),
                    header_row: row,
                    rfi_column: rfi_columns[0].0,
                    status_column: status_columns[0].0,
                    clause: None,
                });
            }
        }
    }
    let mut sheet_candidate_counts = BTreeMap::<String, usize>::new();
    for candidate in &candidates {
        *sheet_candidate_counts
            .entry(candidate.worksheet.name.clone())
            .or_default() += 1;
    }
    for (sheet_name, count) in sheet_candidate_counts {
        if count > 1 {
            duplicate_header_sheets.insert(sheet_name);
        }
    }
    candidates.retain(|candidate| !duplicate_header_sheets.contains(&candidate.worksheet.name));
    if candidates.is_empty() {
        if !duplicate_header_sheets.is_empty() {
            return Err(BackendError::validation(
                "error.rfi.followUp.layoutAmbiguous",
            ));
        }
        return Err(BackendError::validation("error.rfi.followUp.layoutMissing"));
    }
    if candidates.len() == 1 {
        return Ok(candidates);
    }
    let preferred: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| is_preferred_follow_up_sheet_name(&candidate.worksheet.name))
        .collect();
    if !preferred.is_empty() {
        return Ok(preferred);
    }
    Err(BackendError::validation(
        "error.rfi.followUp.layoutAmbiguous",
    ))
}

pub(super) fn scan_target_rows(
    worksheet: &WorksheetData,
    layout: &FollowUpLayout,
) -> Result<Vec<PendingTargetRow>, BackendError> {
    let last_allowed = layout.header_row + MAX_FOLLOW_UP_DATA_ROWS;
    if worksheet.max_row > last_allowed {
        return Err(limit_exceeded_for(format!(
            "Follow Up sheet '{}' reaches row {}; at most row {last_allowed}",
            layout.worksheet.name, worksheet.max_row
        )));
    }
    let mut rows = Vec::new();
    for row in (layout.header_row + 1)..=worksheet.max_row {
        let rfi_cell = worksheet.cell(row, layout.rfi_column);
        let status_cell = worksheet.cell(row, layout.status_column);
        let raw_rfi = rfi_cell
            .map(|cell| normalize_status(&cell.value))
            .unwrap_or_default();
        let (canonical_rfi, visible_rfi) = match canonical_follow_up_rfi(&raw_rfi) {
            Some((canonical, short)) => (Some(canonical), short),
            None => (None, raw_rfi),
        };
        let current_status = status_cell
            .map(|cell| normalize_status(&cell.value))
            .unwrap_or_default();
        if visible_rfi.is_empty() && current_status.is_empty() {
            continue;
        }
        if rows.len() >= MAX_PREVIEW_ROWS {
            return Err(limit_exceeded_for(format!(
                "Follow Up sheet '{}' has more than {MAX_PREVIEW_ROWS} reviewable rows",
                layout.worksheet.name
            )));
        }
        rows.push(PendingTargetRow {
            row,
            canonical_rfi,
            visible_rfi,
            current_status,
            rfi_cell: rfi_cell.map(Into::into),
            status_cell: status_cell.map(Into::into),
        });
    }
    Ok(rows)
}

pub(super) fn classify_borrowed_row<'a>(
    pending: &'a PendingTargetRow,
    layout: &'a FollowUpLayout,
    target_worksheet: &WorksheetData,
    evidence: &'a [SourceStatusEvidence],
    analysis: &'a SourceAnalysis,
    duplicate_counts: &BTreeMap<String, usize>,
    merge_budget: &mut MergeQueryBudget,
) -> Result<BorrowedFollowUpRow<'a>, BackendError> {
    let duplicate = pending
        .canonical_rfi
        .as_ref()
        .and_then(|rfi| duplicate_counts.get(rfi))
        .is_some_and(|count| *count > 1);
    let rfi_formula = pending
        .rfi_cell
        .as_ref()
        .is_some_and(|cell| cell.formula.is_some());
    let rfi_merged = merge_budget.contains(target_worksheet, pending.row, layout.rfi_column)?;
    let status_formula = pending
        .status_cell
        .as_ref()
        .is_some_and(|cell| cell.formula.is_some());
    let status_merged =
        merge_budget.contains(target_worksheet, pending.row, layout.status_column)?;

    let (disposition, reason) = if rfi_formula {
        manual("targetRfiFormula")
    } else if rfi_merged {
        manual("targetRfiMerged")
    } else if pending.visible_rfi.is_empty() {
        manual("blankRfi")
    } else if pending.canonical_rfi.is_none() {
        manual("invalidRfi")
    } else if analysis.proposed_status.as_deref() == Some(pending.current_status.as_str()) {
        (
            FollowUpDisposition::AlreadySynced,
            "warn.rfi.followUp.reason.alreadySynced".to_string(),
        )
    } else if duplicate {
        manual("targetDuplicate")
    } else if evidence.is_empty() {
        manual("sourceMissing")
    } else if let Some(reason) = analysis.reason {
        manual(reason)
    } else if pending.status_cell.is_none() {
        manual("targetCellMissing")
    } else if status_formula {
        manual("targetFormula")
    } else if status_merged {
        manual("targetMerged")
    } else if !pending.current_status.is_empty()
        && pending.current_status != "Signed"
        && pending.current_status != "Not Signed"
    {
        manual("targetStatusUnknown")
    } else {
        let reason = if analysis.proposed_status.as_deref() == Some("Signed") {
            "warn.rfi.followUp.reason.readySigned"
        } else {
            "warn.rfi.followUp.reason.readyNotSigned"
        };
        (FollowUpDisposition::Ready, reason.to_string())
    };
    let selectable = disposition == FollowUpDisposition::Ready;
    let rfi_no = pending.visible_rfi.as_str();
    Ok(BorrowedFollowUpRow {
        row_id: format!(
            "rfi-follow-up:{}:{}:{}",
            layout.worksheet.sheet_id, pending.row, layout.status_column
        ),
        sheet: &layout.worksheet.name,
        row: pending.row,
        rfi_no,
        source_statuses: &analysis.statuses,
        current_status: &pending.current_status,
        proposed_status: analysis.proposed_status.as_deref(),
        disposition,
        reason_key: reason,
        selectable,
        enabled: selectable,
        target: BorrowedReviewTarget {
            worksheet: &layout.worksheet,
            row: pending.row,
            rfi_column: layout.rfi_column,
            status_column: layout.status_column,
            rfi_cell: pending.rfi_cell.as_ref(),
            status_cell: pending.status_cell.as_ref(),
        },
        source_evidence: evidence,
        mapping_details: None,
    })
}

impl BorrowedFollowUpRow<'_> {
    pub(super) fn materialize(&self) -> FollowUpRow {
        FollowUpRow {
            row_id: self.row_id.clone(),
            sheet: self.sheet.to_string(),
            row: self.row,
            rfi_no: self.rfi_no.to_string(),
            source_statuses: self.source_statuses.to_vec(),
            current_status: self.current_status.to_string(),
            proposed_status: self.proposed_status.map(str::to_string),
            proposed_rfi_no: None,
            disposition: self.disposition.clone(),
            reason_key: self.reason_key.clone(),
            selectable: self.selectable,
            enabled: self.enabled,
            remark: None,
            formula_fixes: Vec::new(),
            target: FollowUpReviewTarget {
                worksheet: self.target.worksheet.clone(),
                row: self.target.row,
                rfi_column: self.target.rfi_column,
                status_column: self.target.status_column,
                rfi_cell: self.target.rfi_cell.cloned(),
                status_cell: self.target.status_cell.cloned(),
            },
            source_evidence: self.source_evidence.to_vec(),
            mapping_details: self.mapping_details.map(str::to_string),
        }
    }
}

pub(super) fn analyze_source(
    evidence: &[SourceStatusEvidence],
    workbook: &WorkbookData,
    merge_budget: &mut MergeQueryBudget,
) -> Result<SourceAnalysis, BackendError> {
    #[cfg(test)]
    SOURCE_ANALYSIS_CALLS.fetch_add(1, Ordering::SeqCst);

    let statuses = evidence
        .iter()
        .map(|evidence| {
            mapped_follow_up_status(
                evidence
                    .status_cell
                    .as_ref()
                    .map(|cell| cell.value.as_str())
                    .unwrap_or_default(),
            )
            .to_string()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if evidence.is_empty() {
        return Ok(SourceAnalysis {
            statuses,
            proposed_status: None,
            reason: None,
        });
    }
    if evidence.iter().any(|evidence| {
        evidence.rfi_cell.formula.is_some()
            || evidence
                .status_cell
                .as_ref()
                .is_some_and(|cell| cell.formula.is_some())
    }) {
        return Ok(SourceAnalysis {
            statuses,
            proposed_status: None,
            reason: Some("sourceFormula"),
        });
    }
    let mut source_merged = false;
    for evidence in evidence {
        let Some(worksheet) = workbook.sheet(&evidence.sheet) else {
            continue;
        };
        if merge_budget.contains(worksheet, evidence.row, evidence.rfi_column)?
            || merge_budget.contains(worksheet, evidence.row, evidence.status_column)?
        {
            source_merged = true;
            break;
        }
    }
    if source_merged {
        return Ok(SourceAnalysis {
            statuses,
            proposed_status: None,
            reason: Some("sourceMerged"),
        });
    }
    if statuses.len() == 1 {
        return Ok(SourceAnalysis {
            proposed_status: statuses.first().cloned(),
            statuses,
            reason: None,
        });
    }
    Ok(SourceAnalysis {
        statuses,
        proposed_status: None,
        reason: Some("sourceConflict"),
    })
}
