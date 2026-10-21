use super::*;

#[test]
fn rfi_candidates_pad_only_supported_anchored_rfi_identifiers() {
    assert_eq!(
        canonical_rfi("AZE-SUN-INT-QMS-RFI-CIV-901").as_deref(),
        Some("AZE-SUN-INT-QMS-RFI-CIV-0901")
    );
    assert_eq!(
        canonical_follow_up_rfi("CIV-9").map(|(_, short)| short),
        Some("CIV-0009".to_string())
    );
    assert_eq!(
        canonical_follow_up_rfi("CIV-0009").map(|(_, short)| short),
        Some("CIV-0009".to_string())
    );
    assert!(canonical_rfi("AZE-SUN-INT-QMS-RFI-CIV-09010").is_none());
    assert!(canonical_follow_up_rfi("ITP-CIV-901").is_none());
    assert!(canonical_follow_up_rfi("2026-09-01").is_none());
    assert!(parse_rfi_group("CIV-901/902").is_some());
    assert!(parse_rfi_group("ITP-CIV-901").is_none());
    assert_eq!(
        canonical_follow_up_rfi("RFI-CIV-901").map(|(canonical, _)| canonical),
        Some("AZE-SUN-INT-QMS-RFI-CIV-0901".to_string())
    );
    assert_eq!(
        parse_rfi_group("RFI-CIV-901").unwrap(),
        vec!["AZE-SUN-INT-QMS-RFI-CIV-0901"]
    );
}

#[test]
fn compound_rfi_parser_expands_historical_forms_without_guessing() {
    let parsed = parse_rfi_group("CIV-71-77/ 100/101/162/163").unwrap();
    assert_eq!(parsed.len(), 11);
    assert_eq!(parsed.first().unwrap(), "AZE-SUN-INT-QMS-RFI-CIV-0071");
    assert_eq!(parsed.last().unwrap(), "AZE-SUN-INT-QMS-RFI-CIV-0163");
    assert_eq!(
        format_rfi_group(&parse_rfi_group("MEC-0370/0565").unwrap()).as_deref(),
        Some("MEC-0370/0565")
    );
    assert!(parse_rfi_group("CVI-0028").is_none());
    assert!(parse_rfi_group("MEC-02Â49").is_none());
    assert!(parse_rfi_group("MEC˗0249").is_none());
    assert!(parse_rfi_group("CIV-0100/MEC-0101").is_none());
    assert!(parse_rfi_group("CIV-0200-0100").is_none());
}

#[test]
fn approved_status_policy_removes_terminal_failures_and_aggregates_groups() {
    assert_eq!(log_state("Cancelled"), Some(LogState::Remove));
    assert_eq!(log_state("Rejected"), Some(LogState::Remove));
    assert_eq!(
        aggregate_status([LogState::Pending]).as_deref(),
        Some("Pending")
    );
    assert_eq!(
        aggregate_status([LogState::Completed]).as_deref(),
        Some("Completed")
    );
    assert_eq!(
        aggregate_status([LogState::Completed, LogState::Pending]).as_deref(),
        Some("In Progress")
    );
    assert_eq!(aggregate_status([LogState::Remove]).as_deref(), Some(""));
}

#[test]
fn worley_marker_requires_an_exact_line() {
    assert_eq!(mapped_follow_up_status("Signed by Worley"), "Signed");
    assert_eq!(
        mapped_follow_up_status("Note\nSigned by Worley\nOther"),
        "Signed"
    );
    assert_eq!(
        mapped_follow_up_status("Not Signed by Worley"),
        "Not Signed"
    );
    assert_eq!(
        mapped_follow_up_status("Signed by Worley team"),
        "Not Signed"
    );
}

fn descriptor() -> WorksheetDescriptor {
    WorksheetDescriptor {
        workbook_index: 1,
        sheet_id: 7,
        relationship_id: "rId7".to_string(),
        relationship_type: "worksheet".to_string(),
        part_path: "xl/worksheets/sheet7.xml".to_string(),
        name: "Follow Up".to_string(),
        visibility: WorksheetVisibility::Visible,
    }
}

fn fingerprint() -> WorkbookFingerprint {
    WorkbookFingerprint {
        identity: FilesystemObjectIdentity::Unix {
            device: 1,
            inode: 2,
        },
        size: 3,
        mtime_ns: 4,
        sha256: "00".repeat(32),
    }
}

fn approved_update() -> FollowUpApprovedUpdate {
    let canonical_rfi = "AZE-SUN-INT-QMS-RFI-CIV-0001".to_string();
    FollowUpApprovedUpdate {
        row_id: "rfi-follow-up:7:2:2".to_string(),
        canonical_rfi: canonical_rfi.clone(),
        proposed_status: "Signed".to_string(),
        proposed_rfi: "CIV-0001".to_string(),
        target: FollowUpCellTarget {
            worksheet: descriptor(),
            row: 2,
            rfi_column: 1,
            status_column: 2,
            rfi_cell: CellSnapshot {
                value: "CIV-0001".to_string(),
                ..CellSnapshot::default()
            },
            status_cell: CellSnapshot::default(),
        },
        source_evidence: Vec::new(),
    }
}

fn step_meta(updates: Vec<FollowUpApprovedUpdate>) -> FollowUpStepMeta {
    FollowUpStepMeta {
        rfi_log_workbook: "/source.xlsx".to_string(),
        rfi_log_fingerprint: fingerprint(),
        follow_up_fingerprint: fingerprint(),
        layout: FollowUpLayout {
            worksheet: descriptor(),
            header_row: 1,
            rfi_column: 1,
            status_column: 2,
            clause: None,
        },
        layouts: Vec::new(),
        updates,
    }
}

#[test]
fn projection_budget_exact_boundary_and_one_byte_over() {
    let mut writer = CountingLimitWriter {
        count: 0,
        limit: MAX_PREVIEW_AUTHORITY_BYTES,
    };
    let chunk = [0_u8; 4096];
    for _ in 0..(MAX_PREVIEW_AUTHORITY_BYTES / chunk.len() as u64) {
        writer.write_all(&chunk).unwrap();
    }
    assert_eq!(writer.count, MAX_PREVIEW_AUTHORITY_BYTES);
    assert!(writer.write_all(&[0]).is_err());
    assert_eq!(writer.count, MAX_PREVIEW_AUTHORITY_BYTES);
}

#[test]
fn follow_up_authority_rejects_unknown_fields_and_duplicate_updates() {
    let valid_meta = serde_json::to_value(step_meta(vec![approved_update()]))
        .unwrap()
        .as_object()
        .cloned()
        .unwrap();
    let valid = Step {
        kind: StepKind::WorkbookUpdate,
        src: Some("/target.xlsx".to_string()),
        dst: Some("/backup.xlsx".to_string()),
        note_key: Some("note.rfi.followUp.workbookUpdate".to_string()),
        meta: valid_meta,
    };
    assert!(decode_step_meta(&valid).is_ok());

    let mut unknown = valid.clone();
    unknown
        .meta
        .insert("unknown".to_string(), Value::Bool(true));
    assert!(decode_step_meta(&unknown).is_err());

    let update = approved_update();
    let mut duplicate = valid;
    duplicate.meta = serde_json::to_value(step_meta(vec![update.clone(), update]))
        .unwrap()
        .as_object()
        .cloned()
        .unwrap();
    assert!(decode_step_meta(&duplicate).is_err());
}

#[test]
fn plan_from_classification_emits_one_exact_workbook_step_for_enabled_rows() {
    let layout = FollowUpLayout {
        worksheet: descriptor(),
        header_row: 1,
        rfi_column: 1,
        status_column: 2,
        clause: None,
    };
    let update = approved_update();
    let row = FollowUpRow {
        row_id: update.row_id.clone(),
        sheet: "Follow Up".to_string(),
        row: 2,
        rfi_no: "CIV-0001".to_string(),
        source_statuses: vec!["Signed".to_string()],
        current_status: String::new(),
        proposed_status: Some(update.proposed_status.clone()),
        proposed_rfi_no: Some(update.proposed_rfi.clone()),
        disposition: FollowUpDisposition::Ready,
        reason_key: "warn.rfi.followUp.reason.readySigned".to_string(),
        selectable: true,
        enabled: true,
        target: FollowUpReviewTarget {
            worksheet: descriptor(),
            row: 2,
            rfi_column: 1,
            status_column: 2,
            rfi_cell: Some(update.target.rfi_cell.clone()),
            status_cell: Some(update.target.status_cell.clone()),
        },
        source_evidence: Vec::new(),
        mapping_details: None,
    };
    let columns = editor_columns();
    let classification = json!({
        "layout": layout,
        "rows": [row],
        "warnings": [],
        "editor": editor_metadata(&[row], columns),
    });

    let plan = plan_from_classification(
        "/source.xlsx",
        "/follow-up.xlsx",
        "/.docflow-s5-backup-test.xlsx",
        fingerprint(),
        fingerprint(),
        classification,
    )
    .unwrap();

    assert_eq!(plan.op_id, OPERATION_ID);
    assert!(plan.plan_id.is_empty());
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(
        plan.steps[0].kind,
        crate::contracts::StepKind::WorkbookUpdate
    );
    assert_eq!(plan.steps[0].src.as_deref(), Some("/follow-up.xlsx"));
    assert_eq!(
        plan.steps[0].dst.as_deref(),
        Some("/.docflow-s5-backup-test.xlsx")
    );
    assert_eq!(
        plan.steps[0].note_key.as_deref(),
        Some("note.rfi.followUp.workbookUpdate")
    );
    assert_eq!(
        decode_step_meta(&plan.steps[0]).unwrap().updates,
        vec![update]
    );
    assert!(plan.meta.contains_key("editor"));
    assert!(plan.meta.contains_key("rfiFollowUp"));
}

#[test]
fn source_group_analysis_runs_once_for_500_duplicate_targets() {
    SOURCE_ANALYSIS_CALLS.store(0, Ordering::SeqCst);
    let make_descriptor = |name: &str, id: u32| WorksheetDescriptor {
        workbook_index: id,
        sheet_id: id,
        relationship_id: format!("rId{id}"),
        relationship_type: "worksheet".to_string(),
        part_path: format!("xl/worksheets/sheet{id}.xml"),
        name: name.to_string(),
        visibility: WorksheetVisibility::Visible,
    };
    let source_columns = status_columns();
    let source_names = source_columns.map(|columns| columns.sheet);
    let civ_status_column = source_columns[0].status_column;
    let source_discovery = OrderedWorkbookDiscovery {
        worksheets: source_names
            .iter()
            .enumerate()
            .map(|(index, name)| make_descriptor(name, index as u32 + 1))
            .collect(),
        safety: Default::default(),
    };
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    let mut source_workbook = WorkbookData::default();
    for name in source_names {
        source_workbook
            .sheets
            .insert(name.to_string(), WorksheetData::default());
    }
    let civ = source_workbook.sheets.get_mut("CIV").unwrap();
    civ.max_row = 10;
    civ.max_column = civ_status_column;
    civ.cells.insert(
        (10, 1),
        CellData {
            value: rfi.to_string(),
            ..Default::default()
        },
    );
    civ.cells.insert(
        (10, civ_status_column),
        CellData {
            value: "Signed by Worley".to_string(),
            ..Default::default()
        },
    );

    let target_discovery = OrderedWorkbookDiscovery {
        worksheets: vec![make_descriptor("Follow Up", 1)],
        safety: Default::default(),
    };
    let mut target = WorksheetData {
        max_row: 501,
        max_column: 2,
        ..Default::default()
    };
    target.cells.insert(
        (1, 1),
        CellData {
            value: "RFI NO.".to_string(),
            ..Default::default()
        },
    );
    target.cells.insert(
        (1, 2),
        CellData {
            value: "STATUS".to_string(),
            ..Default::default()
        },
    );
    for row in 2..=501 {
        target.cells.insert(
            (row, 1),
            CellData {
                value: rfi.to_string(),
                ..Default::default()
            },
        );
        target.cells.insert((row, 2), CellData::default());
    }
    let mut target_workbook = WorkbookData::default();
    target_workbook
        .sheets
        .insert("Follow Up".to_string(), target);

    classify_workbooks_legacy(
        &source_discovery,
        &source_workbook,
        &target_discovery,
        &target_workbook,
        false,
    )
    .unwrap();
    assert_eq!(SOURCE_ANALYSIS_CALLS.load(Ordering::SeqCst), 1);
}

#[test]
fn merge_query_budget_fails_at_the_first_comparison_over_limit() {
    let worksheet = WorksheetData {
        merged_ranges: vec![crate::ooxml::CellRange {
            first_row: 1,
            first_column: 1,
            last_row: 1,
            last_column: 1,
        }],
        ..Default::default()
    };
    let mut budget = MergeQueryBudget { remaining: 1 };
    assert!(budget.contains(&worksheet, 1, 1).unwrap());
    assert_eq!(
        budget.contains(&worksheet, 1, 1).unwrap_err().message,
        "error.rfi.followUp.limitExceeded"
    );
}
