use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom},
    path::PathBuf,
};

#[test]
#[ignore = "requires /tmp/docflow_mac_proof files"]
fn mac_live_proof_runs_follow_up_sync_losslessly() {
    let log_path = PathBuf::from("/tmp/docflow_mac_proof/RFI Log.xlsm");
    let follow_up_path = PathBuf::from("/tmp/docflow_mac_proof/Inspection Status Follow Up.xlsm");
    let output_path =
        PathBuf::from("/tmp/docflow_mac_proof/Inspection Status Follow Up - Synced.xlsm");

    assert!(
        log_path.exists(),
        "RFI Log must exist in /tmp/docflow_mac_proof"
    );
    assert!(
        follow_up_path.exists(),
        "Follow Up must exist in /tmp/docflow_mac_proof"
    );

    let mut log_file = File::open(&log_path).expect("open RFI Log read-only");
    let mut follow_up_file = File::open(&follow_up_path).expect("open Follow Up read-only");

    println!("==> STEP 1: Running docflow_lib::rfi_follow_up::preview_from_open_files...");
    let plan = docflow_lib::rfi_follow_up::preview_from_open_files(
        &log_path,
        &follow_up_path,
        &mut log_file,
        &mut follow_up_file,
    )
    .expect("preview_from_open_files must succeed");

    println!(
        "==> Plan generated: op_id={}, warnings={:?}",
        plan.op_id, plan.warnings
    );
    assert_eq!(plan.op_id, "rfi.follow_up_sync");
    assert!(
        plan.warnings.is_empty(),
        "Expected 0 warnings, got {:?}",
        plan.warnings
    );

    let submitted = plan.meta["editor"]["rows"]
        .as_array()
        .expect("editor rows")
        .clone();

    println!(
        "==> STEP 2: Inspecting all preview rows (count={}):",
        submitted.len()
    );
    for (idx, row) in submitted.iter().enumerate() {
        println!(
            "  [{}] Sheet={:<20} Row={:<3} RFI={:<10} CurrentStatus={:<12} ProposedStatus={:<12} ProposedRfi={:<10} Reason={}",
            idx + 1,
            row["_sheet"].as_str().unwrap_or("?"),
            row["_row"],
            row["rfiNo"].as_str().unwrap_or("?"),
            row["currentStatus"].as_str().unwrap_or("?"),
            row["proposedStatus"].as_str().unwrap_or("?"),
            row["proposedRfiNo"].as_str().unwrap_or("?"),
            row["reasonKey"].as_str().unwrap_or("?")
        );
    }

    println!("==> STEP 3: Revising plan (approving all valid rows)...");
    let mut revise_log = File::open(&log_path).expect("reopen RFI Log");
    let mut revise_follow_up = File::open(&follow_up_path).expect("reopen Follow Up");
    let revised = docflow_lib::rfi_follow_up::revise_from_open_files(
        &plan,
        &submitted,
        &mut revise_log,
        &mut revise_follow_up,
    )
    .expect("revise_from_open_files must succeed");

    assert_eq!(revised.steps.len(), 1, "Expected exactly 1 execution step");
    let step = &revised.steps[0];
    let updates = docflow_lib::rfi_follow_up::guarded_updates(step)
        .expect("guarded_updates must decode cleanly");

    println!("==> STEP 4: Guarded cell updates by worksheet:");
    for (sheet, cell_map) in &updates {
        println!("  Worksheet '{}': {} cell updates", sheet, cell_map.len());
        for ((r, c), update) in cell_map {
            println!(
                "    Cell (row {}, col {}): expected={:?} -> replacement={:?}",
                r, c, update.expected, update.replacement
            );
        }
    }

    println!("==> STEP 5: Patching workbook with patch_workbook_sheets_guarded_text...");
    let mut original = File::open(&follow_up_path).expect("reopen original");
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&output_path)
        .expect("create synced output file");

    docflow_lib::ooxml::patch_workbook_sheets_guarded_text(&mut original, &mut output, &updates)
        .expect("patch_workbook_sheets_guarded_text must succeed");

    println!(
        "==> STEP 6: Verifying OOXML package delta with verify_guarded_workbook_delta_text..."
    );
    original.seek(SeekFrom::Start(0)).expect("rewind original");
    output.seek(SeekFrom::Start(0)).expect("rewind output");

    docflow_lib::ooxml::verify_guarded_workbook_delta_text(&mut original, &mut output, &updates)
        .expect("OOXML package delta verification must pass with zero defects");

    println!("==> PROOF COMPLETE: All 6 steps passed with 100% mathematical fidelity!");
}
