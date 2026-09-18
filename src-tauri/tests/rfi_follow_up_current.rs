use std::{collections::BTreeMap, fs::File, io::Seek, path::PathBuf};

#[test]
#[ignore = "requires operator-supplied current workbook paths"]
fn current_workbooks_produce_a_bounded_guarded_follow_up_plan() {
    let log = PathBuf::from(
        std::env::var("DOCFLOW_CURRENT_RFI_LOG").expect("DOCFLOW_CURRENT_RFI_LOG is required"),
    );
    let follow_up = PathBuf::from(
        std::env::var("DOCFLOW_CURRENT_FOLLOW_UP").expect("DOCFLOW_CURRENT_FOLLOW_UP is required"),
    );
    let mut log_file = File::open(&log).expect("open RFI Log read-only");
    let mut follow_up_file = File::open(&follow_up).expect("open Follow Up read-only");
    let plan = docflow_lib::rfi_follow_up::preview_from_open_files(
        &log,
        &follow_up,
        &mut log_file,
        &mut follow_up_file,
    )
    .expect("current workbooks must produce a safe preview");

    assert_eq!(plan.op_id, "rfi.follow_up_sync");
    assert!(plan.steps.len() <= 1);
    eprintln!(
        "current-template preview: {} approved row pair(s), warnings={:?}",
        plan.steps
            .first()
            .and_then(|step| step.meta.get("updates"))
            .and_then(|updates| updates.as_array())
            .map_or(0, Vec::len),
        plan.warnings
    );
    let submitted = plan.meta["editor"]["rows"]
        .as_array()
        .expect("editor rows")
        .clone();
    let mut rows_by_sheet = BTreeMap::<String, usize>::new();
    let mut rows_by_reason = BTreeMap::<String, usize>::new();
    for row in &submitted {
        *rows_by_sheet
            .entry(row["_sheet"].as_str().unwrap_or("unknown").to_string())
            .or_default() += 1;
        *rows_by_reason
            .entry(row["reasonKey"].as_str().unwrap_or("unknown").to_string())
            .or_default() += 1;
    }
    eprintln!("rows by sheet={rows_by_sheet:?}; rows by reason={rows_by_reason:?}");
    let mut revise_log = File::open(&log).expect("reopen RFI Log read-only");
    let mut revise_follow_up = File::open(&follow_up).expect("reopen Follow Up read-only");
    let revised = docflow_lib::rfi_follow_up::revise_from_open_files(
        &plan,
        &submitted,
        &mut revise_log,
        &mut revise_follow_up,
    )
    .expect("unchanged workbooks must preserve the approved full-schema revision");
    assert_eq!(revised.plan_id, plan.plan_id);

    if let Some(step) = revised.steps.first() {
        let updates = docflow_lib::rfi_follow_up::guarded_updates(step)
            .expect("approved step metadata must decode");
        assert!(!updates.is_empty());
        assert!(updates.keys().all(|sheet| matches!(
            sheet.as_str(),
            "CIV" | "MEC" | "PCU" | "Road" | "Signed By Worley CIV" | "Signed By Worley MEC"
        )));
        assert!(!updates.contains_key("POT test"));
        assert!(!updates.contains_key("STAT"));
        assert!(!updates.contains_key("Changelog"));

        let mut original = File::open(&follow_up).expect("reopen Follow Up read-only");
        let mut output = tempfile::tempfile().expect("create private temporary output");
        docflow_lib::ooxml::patch_workbook_sheets_guarded_text(
            &mut original,
            &mut output,
            &updates,
        )
        .expect("patch only approved literal cells");
        original.rewind().expect("rewind original");
        output.rewind().expect("rewind output");
        docflow_lib::ooxml::verify_guarded_workbook_delta_text(
            &mut original,
            &mut output,
            &updates,
        )
        .expect("no unrelated OOXML package part may change");
    }
}
