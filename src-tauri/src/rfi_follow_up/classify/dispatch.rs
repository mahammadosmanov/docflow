use super::*;

pub(in crate::rfi_follow_up) fn find_sheet_flexible<'a>(
    workbook: &'a WorkbookData,
    target: &str,
) -> Option<&'a WorksheetData> {
    workbook.sheet(target).or_else(|| {
        let norm = target.trim().to_uppercase();
        workbook.sheets.iter().find_map(|(name, sheet)| {
            if name.trim().to_uppercase() == norm {
                Some(sheet)
            } else {
                None
            }
        })
    })
}

pub(in crate::rfi_follow_up) fn supports_full_sync_schema(workbook: &WorkbookData) -> bool {
    ["CIV", "MEC"].iter().all(|name| {
        find_sheet_flexible(workbook, name).is_some_and(|sheet| {
            find_header_column(sheet, &["RFI NO.", "RFI NO", "RFI #"]).is_some()
                && find_header_column(sheet, &["ITP NO:", "ITP NO.", "ITP NO"]).is_some()
                && find_header_column(sheet, &["ITEM NO.", "ITEM NO", "ITEM #"]).is_some()
                && find_header_column(sheet, &["INSPECTION STATUS", "STATUS"]).is_some()
                && find_header_column(sheet, &["REMARKS", "REMARK"]).is_some()
        })
    })
}

pub fn classify_workbooks(
    source_discovery: &OrderedWorkbookDiscovery,
    source_workbook: &WorkbookData,
    target_discovery: &OrderedWorkbookDiscovery,
    target_workbook: &WorkbookData,
    target_has_signature: bool,
) -> Result<Value, BackendError> {
    if supports_full_sync_schema(source_workbook)
        && target_workbook
            .sheets
            .keys()
            .any(|name| approved_sheet(name.trim()))
    {
        classify_workbooks_full(
            source_discovery,
            source_workbook,
            target_discovery,
            target_workbook,
            target_has_signature,
        )
    } else {
        classify_workbooks_legacy(
            source_discovery,
            source_workbook,
            target_discovery,
            target_workbook,
            target_has_signature,
        )
    }
}

pub(in crate::rfi_follow_up) fn classify_workbooks_legacy(
    source_discovery: &OrderedWorkbookDiscovery,
    source_workbook: &WorkbookData,
    target_discovery: &OrderedWorkbookDiscovery,
    target_workbook: &WorkbookData,
    target_has_signature: bool,
) -> Result<Value, BackendError> {
    let source = scan_source(source_discovery, source_workbook)?;
    let mut merge_budget = MergeQueryBudget {
        remaining: MAX_FOLLOW_UP_MERGE_COMPARISONS,
    };
    let mut analyses = BTreeMap::new();
    for (rfi, evidence) in &source {
        analyses.insert(
            rfi.clone(),
            analyze_source(evidence, source_workbook, &mut merge_budget)?,
        );
    }
    let empty_analysis = SourceAnalysis {
        statuses: Vec::new(),
        proposed_status: None,
        reason: None,
    };
    let layouts = discover_layouts(
        target_discovery,
        target_workbook,
        target_has_signature,
        &mut merge_budget,
    )?;
    let mut all_pending = Vec::new();
    for layout in &layouts {
        let worksheet = target_workbook
            .sheet(&layout.worksheet.name)
            .ok_or_else(workbook_invalid)?;
        let pending = scan_target_rows(worksheet, layout)?;
        for row in pending {
            all_pending.push((layout, worksheet, row));
        }
    }
    let duplicate_counts = all_pending
        .iter()
        .filter_map(|(_, _, row)| row.canonical_rfi.as_ref())
        .fold(BTreeMap::<String, usize>::new(), |mut counts, rfi| {
            *counts.entry(rfi.clone()).or_default() += 1;
            counts
        });
    let borrowed_rows = all_pending
        .iter()
        .map(|(layout, worksheet, pending)| {
            let evidence = pending
                .canonical_rfi
                .as_ref()
                .and_then(|rfi| source.get(rfi))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let analysis = pending
                .canonical_rfi
                .as_ref()
                .and_then(|rfi| analyses.get(rfi))
                .unwrap_or(&empty_analysis);
            classify_borrowed_row(
                pending,
                layout,
                worksheet,
                evidence,
                analysis,
                &duplicate_counts,
                &mut merge_budget,
            )
        })
        .collect::<Result<Vec<_>, BackendError>>()?;

    let manual_count = borrowed_rows
        .iter()
        .filter(|row| row.disposition == FollowUpDisposition::ManualReview)
        .count();
    let enabled_count = borrowed_rows.iter().filter(|row| row.enabled).count();
    let mut warnings = Vec::new();
    if manual_count > 0 {
        warnings.push(Warning {
            key: "warn.rfi.followUp.manualRows".to_string(),
            subject: manual_count.to_string(),
        });
    }
    if enabled_count == 0 {
        warnings.push(Warning {
            key: "warn.rfi.followUp.noRowsSelected".to_string(),
            subject: String::new(),
        });
    }
    let columns = editor_columns_legacy();
    let primary_layout = layouts.first().ok_or_else(workbook_invalid)?;
    {
        let projection = BorrowedAuthorityProjection {
            layout: primary_layout,
            layouts: &layouts,
            rows: &borrowed_rows,
            warnings: &warnings,
            editor: borrowed_editor(&columns, &borrowed_rows),
            approved_rows: borrowed_rows.iter().filter(|row| row.enabled).collect(),
        };
        validate_projection_budget(&projection)?;
    }
    let rows = borrowed_rows
        .iter()
        .map(BorrowedFollowUpRow::materialize)
        .collect::<Vec<_>>();
    let editor = editor_metadata(&rows, columns);
    Ok(json!({
        "layout": primary_layout,
        "layouts": layouts,
        "rows": rows,
        "warnings": warnings,
        "editor": editor,
    }))
}
