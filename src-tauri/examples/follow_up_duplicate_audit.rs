use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use docflow_lib::{
    contracts::Plan,
    fs_ops::FilesystemAuthority,
    ooxml::{
        patch_workbook_sheets_guarded_text, read_workbook_selective,
        verify_guarded_workbook_delta_text,
    },
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    rfi_follow_up::{self, preview_from_open_files, revise_from_open_files},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const RFI_LOG_SHA256: &str = "446b4afd223b47992194d484e9e367cf1dbfae6f09d8f2f1bb0bc84cd8fe8ba1";
const FOLLOW_UP_SHA256: &str = "a97d89464b7b3cc12e4ecb27ac1342e7e38f079ef076d801729fbb4cbb6ad1f6";

#[derive(Clone)]
struct SourceContext {
    row: u32,
    full_rfi: String,
    itp: String,
    item: String,
    description: String,
    tag: String,
    area: String,
    location: String,
    lifecycle: String,
    remarks: String,
}
#[derive(Clone, Copy)]
struct SourceColumns {
    rfi: u32,
    itp: u32,
    item: u32,
    description: u32,
    tag: u32,
    area: u32,
    location: u32,
    lifecycle: u32,
    remarks: u32,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("follow-up duplicate audit failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let workspace = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage)?
        .canonicalize()
        .map_err(|error| format!("workspace: {error}"))?;
    let mode = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage)?;
    let journal_id = args
        .next()
        .map(|value| value.into_string().map_err(|_| usage()))
        .transpose()?;
    if args.next().is_some()
        || !matches!(
            mode.as_str(),
            "baseline" | "apply" | "journal" | "resume-undo"
        )
    {
        return Err(usage());
    }
    if mode == "resume-undo" {
        let journal_id = journal_id.ok_or_else(usage)?;
        return run_resume_undo(&workspace, &journal_id);
    }
    if journal_id.is_some() {
        return Err(usage());
    }

    let rfi_log_path = workspace.join("RFI Log.xlsm");
    let follow_up_path = workspace.join("Inspection Status Follow Up.xlsm");
    ensure_private_copy(&workspace, &rfi_log_path, RFI_LOG_SHA256)?;
    ensure_private_copy(&workspace, &follow_up_path, FOLLOW_UP_SHA256)?;

    let inventory = build_inventory(&rfi_log_path, &follow_up_path)?;
    write_new_json(&workspace.join("duplicate-inventory.json"), &inventory)?;

    let mut rfi_log = File::open(&rfi_log_path).map_err(|error| error.to_string())?;
    let mut follow_up = File::open(&follow_up_path).map_err(|error| error.to_string())?;
    let plan =
        preview_from_open_files(&rfi_log_path, &follow_up_path, &mut rfi_log, &mut follow_up)
            .map_err(|error| format!("preview: {error:?}"))?;
    let rows = editor_rows(&plan)?;
    let preview_report = editor_report(&plan, &rows, &inventory);
    write_new_json(&workspace.join("preview-before.json"), &preview_report)?;
    println!(
        "preview rows={} enabled={} manualReview={} ready={}",
        rows.len(),
        count_bool(&rows, "enabled", true),
        count_eq(
            &rows,
            "disposition",
            "rfi.followUp.disposition.manualReview"
        ),
        count_eq(&rows, "disposition", "rfi.followUp.disposition.ready")
    );
    if mode == "baseline" {
        println!("baseline artifacts written under {}", workspace.display());
        return Ok(());
    }
    if mode == "journal" {
        return run_journal(&workspace, &inventory);
    }

    let mut revised = revise_from_open_files(&plan, &rows, &mut rfi_log, &mut follow_up)
        .map_err(|error| format!("revise: {error:?}"))?;
    assert_required_0767(&revised, &inventory)?;
    let enabled_rows = editor_rows(&revised)?
        .into_iter()
        .filter(|row| row.get("enabled").and_then(Value::as_bool) == Some(true))
        .collect::<Vec<_>>();
    if enabled_rows.is_empty() {
        return Err("revised plan has no enabled rows".to_string());
    }
    let submission = editor_rows(&revised)?;
    revised = revise_from_open_files(&revised, &submission, &mut rfi_log, &mut follow_up)
        .map_err(|error| format!("final revise: {error:?}"))?;
    let selected = editor_rows(&revised)?
        .into_iter()
        .filter(|row| row.get("enabled").and_then(Value::as_bool) == Some(true))
        .collect::<Vec<_>>();
    let step = revised
        .steps
        .first()
        .ok_or_else(|| "revised plan contains no guarded workbook step".to_string())?;
    if revised.steps.len() != 1 {
        return Err(format!(
            "expected one workbook step, got {}",
            revised.steps.len()
        ));
    }
    let updates = rfi_follow_up::guarded_updates(step)
        .map_err(|error| format!("guarded updates: {error:?}"))?;
    let out_path = workspace.join("Inspection Status Follow Up patched.xlsm");
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&out_path)
        .map_err(|error| format!("private output must be new: {error}"))?;
    patch_workbook_sheets_guarded_text(&mut follow_up, &mut output, &updates)
        .map_err(|error| format!("guarded patch: {error}"))?;
    verify_guarded_workbook_delta_text(&mut follow_up, &mut output, &updates)
        .map_err(|error| format!("guarded delta verification: {error}"))?;
    output.sync_all().map_err(|error| error.to_string())?;

    let mut patched_target = File::open(&out_path).map_err(|error| error.to_string())?;
    let (patched_discovery, patched_data) =
        docflow_lib::ooxml::read_workbook_and_discovery(&mut patched_target)
            .map_err(|error| format!("read patched workbook: {error}"))?;
    let mec = patched_data
        .sheet("Signed By Worley MEC")
        .ok_or_else(|| "patched workbook missing Signed By Worley MEC".to_string())?;
    let independent_routes_applied = assert_independent_routes_applied(&inventory, mec)?;
    if mec.value(80, 14) != "MEC-0767" || mec.value(80, 15) != "Signed" {
        return Err(format!(
            "expected N80/O80 MEC-0767/Signed, got {:?}/{:?}",
            mec.value(80, 14),
            mec.value(80, 15)
        ));
    }
    if mec.value(77, 14) != "MEC-0767" || mec.value(77, 15) != "Signed" {
        return Err("existing N77/O77 MEC-0767/Signed changed unexpectedly".to_string());
    }

    let mut patched_rfi = File::open(&rfi_log_path).map_err(|error| error.to_string())?;
    let mut patched_follow_up = File::open(&out_path).map_err(|error| error.to_string())?;
    let repeated = preview_from_open_files(
        &rfi_log_path,
        &out_path,
        &mut patched_rfi,
        &mut patched_follow_up,
    )
    .map_err(|error| format!("repeat preview: {error:?}"))?;
    let repeated_rows = editor_rows(&repeated)?;
    for row in &selected {
        let target = row
            .get("target")
            .ok_or_else(|| "selected editor row has no target".to_string())?;
        let worksheet = target
            .pointer("/worksheet/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let row_number = target
            .get("row")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let rfi_column = target
            .get("rfiColumn")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let status_column = target
            .get("statusColumn")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let still_ready = repeated_rows.iter().any(|candidate| {
            let candidate_target = &candidate["target"];
            candidate_target
                .pointer("/worksheet/name")
                .and_then(Value::as_str)
                == Some(worksheet)
                && candidate_target.get("row").and_then(Value::as_u64) == Some(row_number)
                && candidate_target.get("rfiColumn").and_then(Value::as_u64) == Some(rfi_column)
                && candidate_target.get("statusColumn").and_then(Value::as_u64)
                    == Some(status_column)
                && (candidate.get("disposition").and_then(Value::as_str)
                    == Some("rfi.followUp.disposition.ready")
                    || candidate.get("enabled").and_then(Value::as_bool) == Some(true))
        });
        if still_ready {
            return Err(format!(
                "previously enabled target remains ready/enabled: {worksheet} row {row_number}"
            ));
        }
    }
    if count_eq(
        &repeated_rows,
        "disposition",
        "rfi.followUp.disposition.ready",
    ) != 0
    {
        return Err("repeat preview still contains ready rows".to_string());
    }
    let repeated_report = editor_report(&repeated, &repeated_rows, &inventory);
    write_new_json(&workspace.join("preview-after.json"), &repeated_report)?;
    let source_hash_after = sha256(&rfi_log_path)?;
    let follow_hash_after = sha256(&follow_up_path)?;
    if source_hash_after != RFI_LOG_SHA256 || follow_hash_after != FOLLOW_UP_SHA256 {
        return Err("private inputs changed during audit".to_string());
    }
    let delta_report = json!({
        "mode": "guarded-ooxml-patch-only-not-journal-apply",
        "output": out_path,
        "outputSha256": sha256(&out_path)?,
        "inputSha256Unchanged": {"rfiLog": source_hash_after, "followUp": follow_hash_after},
        "guardedCellCount": updates.values().map(BTreeMap::len).sum::<usize>(),
        "enabledRows": selected,
        "required0767": {"row77N_O": [mec.value(77,14), mec.value(77,15)], "row80N_O": [mec.value(80,14), mec.value(80,15)]},
        "independentEligibleRoutesVerified": independent_routes_applied,
        "repeatPreviewReadyRows": count_eq(&repeated_rows, "disposition", "rfi.followUp.disposition.ready"),
        "repeatPreviewEnabledRows": count_bool(&repeated_rows, "enabled", true),
        "idempotence": "passed for every previously enabled row",
        "discovery": patched_discovery,
    });
    write_new_json(&workspace.join("patch-audit.json"), &delta_report)?;
    println!(
        "patch verified cells={} selectedRows={} repeatedReady={} privateOutput={}",
        updates.values().map(BTreeMap::len).sum::<usize>(),
        selected.len(),
        count_eq(
            &repeated_rows,
            "disposition",
            "rfi.followUp.disposition.ready"
        ),
        out_path.display()
    );
    Ok(())
}
fn run_journal(workspace: &Path, inventory: &Value) -> Result<(), String> {
    let runtime = workspace.join("journal-runtime");
    fs::create_dir(&runtime)
        .map_err(|error| format!("create private runtime directory: {error}"))?;
    let rfi_log_path = runtime.join("RFI Log.xlsm");
    let follow_up_path = runtime.join("Inspection Status Follow Up.xlsm");
    fs::copy(workspace.join("RFI Log.xlsm"), &rfi_log_path)
        .map_err(|error| format!("copy private RFI Log: {error}"))?;
    fs::copy(
        workspace.join("Inspection Status Follow Up.xlsm"),
        &follow_up_path,
    )
    .map_err(|error| format!("copy private Follow Up: {error}"))?;
    ensure_private_copy(&runtime, &rfi_log_path, RFI_LOG_SHA256)?;
    ensure_private_copy(&runtime, &follow_up_path, FOLLOW_UP_SHA256)?;
    let original_source = fs::read(&rfi_log_path).map_err(|error| error.to_string())?;
    let original_target = fs::read(&follow_up_path).map_err(|error| error.to_string())?;

    let mut source = File::open(&rfi_log_path).map_err(|error| error.to_string())?;
    let mut target = File::open(&follow_up_path).map_err(|error| error.to_string())?;
    let preview = preview_from_open_files(&rfi_log_path, &follow_up_path, &mut source, &mut target)
        .map_err(|error| format!("journal preview: {error:?}"))?;
    let submitted = editor_rows(&preview)?;
    let revised = revise_from_open_files(&preview, &submitted, &mut source, &mut target)
        .map_err(|error| format!("journal revise: {error:?}"))?;
    assert_required_0767(&revised, inventory)?;
    let selected = editor_rows(&revised)?
        .into_iter()
        .filter(|row| row.get("enabled").and_then(Value::as_bool) == Some(true))
        .collect::<Vec<_>>();
    let authority_path = runtime.join("journals");
    let authority = FilesystemAuthority::new(&authority_path, Duration::from_secs(300));
    let caller = CallerIdentity::new("follow-up-audit-owner", "follow-up-audit-session");
    let scope = GrantScope::new(
        "follow-up-audit-owner",
        "follow-up-audit-session",
        "follow-up-audit-request",
        "rfi.follow_up_sync",
    );
    let source_grant = authority
        .issue_grant(&scope, &runtime, GrantAccess::Read)
        .map_err(|error| format!("source grant: {error:?}"))?;
    let target_grant = authority
        .issue_grant(&scope, &runtime, GrantAccess::ReadWrite)
        .map_err(|error| format!("target grant: {error:?}"))?;
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "follow-up-audit-owner",
                "follow-up-audit-session",
                "rfi.follow_up_sync",
                vec![source_grant.grant_id, target_grant.grant_id],
            ),
            revised,
        )
        .map_err(|error| format!("cache approved plan: {error:?}"))?;
    let journal_step = cached
        .steps
        .first()
        .ok_or_else(|| "cached journal plan has no guarded workbook step".to_string())?;
    let journal_updates = rfi_follow_up::guarded_updates(journal_step)
        .map_err(|error| format!("journal guarded updates: {error:?}"))?;
    if cached.steps.len() != 1 {
        return Err(format!(
            "expected one cached workbook step, got {}",
            cached.steps.len()
        ));
    }
    let applied = authority
        .apply(&cached.plan_id, &caller)
        .map_err(|error| format!("journal Apply: {error:?}"))?;
    if applied.failed != 0
        || !applied.errors.is_empty()
        || applied.meta.get("effectState").and_then(Value::as_str) != Some("committed")
    {
        return Err(format!(
            "journal Apply is not fully committed: failed={} errors={:?} effectState={:?}",
            applied.failed,
            applied.errors,
            applied.meta.get("effectState")
        ));
    }
    let mut original_target_reader = Cursor::new(original_target.as_slice());
    let mut applied_target_reader =
        File::open(&follow_up_path).map_err(|error| error.to_string())?;
    verify_guarded_workbook_delta_text(
        &mut original_target_reader,
        &mut applied_target_reader,
        &journal_updates,
    )
    .map_err(|error| format!("native journal guarded delta verification: {error:?}"))?;
    let applied_data = docflow_lib::ooxml::read_workbook(&follow_up_path)
        .map_err(|error| format!("read applied workbook: {error:?}"))?;
    let applied_mec = applied_data
        .sheet("Signed By Worley MEC")
        .ok_or_else(|| "applied workbook missing Signed By Worley MEC".to_string())?;
    if applied_mec.value(80, 14) != "MEC-0767"
        || applied_mec.value(80, 15) != "Signed"
        || applied_mec.value(77, 14) != "MEC-0767"
        || applied_mec.value(77, 15) != "Signed"
    {
        return Err("journal Apply failed required MEC-0767 rows 77 and 80".to_string());
    }
    let independent_routes_applied = assert_independent_routes_applied(inventory, applied_mec)?;
    if fs::read(&rfi_log_path).map_err(|error| error.to_string())? != original_source {
        return Err("journal Apply modified the private RFI Log".to_string());
    }
    let applied_target = fs::read(&follow_up_path).map_err(|error| error.to_string())?;
    let mut applied_source_file = File::open(&rfi_log_path).map_err(|error| error.to_string())?;
    let mut applied_target_file = File::open(&follow_up_path).map_err(|error| error.to_string())?;
    let repeated = preview_from_open_files(
        &rfi_log_path,
        &follow_up_path,
        &mut applied_source_file,
        &mut applied_target_file,
    )
    .map_err(|error| format!("journal repeat preview: {error:?}"))?;
    let repeated_rows = editor_rows(&repeated)?;
    for row in &selected {
        let target = row
            .get("target")
            .ok_or_else(|| "selected journal row has no target".to_string())?;
        let worksheet = target
            .pointer("/worksheet/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let row_number = target
            .get("row")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let rfi_column = target
            .get("rfiColumn")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let status_column = target
            .get("statusColumn")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let still_ready = repeated_rows.iter().any(|candidate| {
            let candidate_target = &candidate["target"];
            candidate_target
                .pointer("/worksheet/name")
                .and_then(Value::as_str)
                == Some(worksheet)
                && candidate_target.get("row").and_then(Value::as_u64) == Some(row_number)
                && candidate_target.get("rfiColumn").and_then(Value::as_u64) == Some(rfi_column)
                && candidate_target.get("statusColumn").and_then(Value::as_u64)
                    == Some(status_column)
                && (candidate.get("disposition").and_then(Value::as_str)
                    == Some("rfi.followUp.disposition.ready")
                    || candidate.get("enabled").and_then(Value::as_bool) == Some(true))
        });
        if still_ready {
            return Err(format!(
                "journal-applied target remains ready/enabled: {worksheet} row {row_number}"
            ));
        }
    }
    if count_eq(
        &repeated_rows,
        "disposition",
        "rfi.followUp.disposition.ready",
    ) != 0
    {
        return Err("journal repeat preview still contains ready rows".to_string());
    }
    let restarted = FilesystemAuthority::new(&authority_path, Duration::from_secs(300));
    let undone = restarted
        .undo(&applied.journal_id, &caller, None)
        .map_err(|error| format!("journal Undo: {error:?}"))?;
    if undone.failed != 0
        || !undone.errors.is_empty()
        || undone.meta.get("effectState").and_then(Value::as_str) != Some("committed")
    {
        return Err(format!(
            "journal Undo is not fully committed: failed={} errors={:?} effectState={:?}",
            undone.failed,
            undone.errors,
            undone.meta.get("effectState")
        ));
    }
    if fs::read(&rfi_log_path).map_err(|error| error.to_string())? != original_source
        || fs::read(&follow_up_path).map_err(|error| error.to_string())? != original_target
    {
        return Err("journal Undo did not restore byte-identical private workbooks".to_string());
    }
    let report = json!({
        "mode": "native-journal-apply-restart-undo",
        "runtimeDirectory": runtime,
        "journalId": applied.journal_id,
        "applied": applied.applied,
        "undone": undone.applied,
        "appliedFollowUpSha256": sha256_bytes(&applied_target),
        "restoredFollowUpSha256": sha256(&follow_up_path)?,
        "restoredRfiLogSha256": sha256(&rfi_log_path)?,
        "byteIdenticalRestoration": true,
        "sourceLogUnchangedDuringApplyAndUndo": true,
        "guardedDeltaVerifiedAgainstOriginalBytes": true,
        "guardedCellCount": journal_updates.values().map(BTreeMap::len).sum::<usize>(),
        "row77": {"rfi": applied_mec.value(77,14), "status": applied_mec.value(77,15)},
        "row80": {"rfi": applied_mec.value(80,14), "status": applied_mec.value(80,15)},
        "independentEligibleRoutesVerified": independent_routes_applied,
        "selectedRows": selected,
        "repeatPreview": editor_report(&repeated, &repeated_rows, inventory),
        "undoMeta": undone.meta,
    });
    write_new_json(&workspace.join("journal-apply-undo-audit.json"), &report)?;
    println!(
        "journal Apply/Undo passed applied={} undone={} restoredTargetSha256={}",
        applied.applied,
        undone.applied,
        sha256(&follow_up_path)?
    );
    Ok(())
}
fn run_resume_undo(workspace: &Path, journal_id: &str) -> Result<(), String> {
    let original_source_path = workspace.join("RFI Log.xlsm");
    let original_target_path = workspace.join("Inspection Status Follow Up.xlsm");
    ensure_private_copy(workspace, &original_source_path, RFI_LOG_SHA256)?;
    ensure_private_copy(workspace, &original_target_path, FOLLOW_UP_SHA256)?;
    let original_source = fs::read(&original_source_path).map_err(|error| error.to_string())?;
    let original_target = fs::read(&original_target_path).map_err(|error| error.to_string())?;

    let runtime = workspace.join("journal-runtime");
    let source_path = runtime.join("RFI Log.xlsm");
    let target_path = runtime.join("Inspection Status Follow Up.xlsm");
    ensure_private_copy(&runtime, &source_path, RFI_LOG_SHA256)?;
    if !target_path.is_file() {
        return Err("resume-undo requires the existing private runtime target".to_string());
    }
    let applied_target = fs::read(&target_path).map_err(|error| error.to_string())?;
    if applied_target == original_target {
        return Err(
            "runtime target is already byte-identical to the untouched input; refusing resume-undo"
                .to_string(),
        );
    }

    let inventory = build_inventory(&original_source_path, &original_target_path)?;
    let mut applied_file = File::open(&target_path).map_err(|error| error.to_string())?;
    let (applied_discovery, applied_data) =
        docflow_lib::ooxml::read_workbook_and_discovery(&mut applied_file)
            .map_err(|error| format!("read committed private target: {error:?}"))?;
    let applied_mec = applied_data
        .sheet("Signed By Worley MEC")
        .ok_or_else(|| "committed private target lacks Signed By Worley MEC".to_string())?;
    if applied_mec.value(80, 14) != "MEC-0767"
        || applied_mec.value(80, 15) != "Signed"
        || applied_mec.value(77, 14) != "MEC-0767"
        || applied_mec.value(77, 15) != "Signed"
    {
        return Err(
            "existing committed target does not contain required MEC-0767 row77/row80 values"
                .to_string(),
        );
    }
    let independent_routes_applied = assert_independent_routes_applied(&inventory, applied_mec)?;

    let mut original_source_file =
        File::open(&original_source_path).map_err(|error| error.to_string())?;
    let mut original_target_file =
        File::open(&original_target_path).map_err(|error| error.to_string())?;
    let original_plan = preview_from_open_files(
        &original_source_path,
        &original_target_path,
        &mut original_source_file,
        &mut original_target_file,
    )
    .map_err(|error| format!("reconstruct original approved preview: {error:?}"))?;
    let original_rows = editor_rows(&original_plan)?;
    let revised_plan = revise_from_open_files(
        &original_plan,
        &original_rows,
        &mut original_source_file,
        &mut original_target_file,
    )
    .map_err(|error| format!("reconstruct original revised plan: {error:?}"))?;
    assert_required_0767(&revised_plan, &inventory)?;
    let step = revised_plan
        .steps
        .first()
        .ok_or_else(|| "reconstructed original plan has no guarded step".to_string())?;
    if revised_plan.steps.len() != 1 {
        return Err(format!(
            "expected one reconstructed guarded step, got {}",
            revised_plan.steps.len()
        ));
    }
    let updates = rfi_follow_up::guarded_updates(step)
        .map_err(|error| format!("reconstructed guarded updates: {error:?}"))?;
    let mut before_delta = Cursor::new(original_target.as_slice());
    let mut actual_after_delta = File::open(&target_path).map_err(|error| error.to_string())?;
    verify_guarded_workbook_delta_text(&mut before_delta, &mut actual_after_delta, &updates)
        .map_err(|error| format!("resume-undo guarded delta verification: {error:?}"))?;

    let mut runtime_source = File::open(&source_path).map_err(|error| error.to_string())?;
    let mut runtime_target = File::open(&target_path).map_err(|error| error.to_string())?;
    let repeated = preview_from_open_files(
        &source_path,
        &target_path,
        &mut runtime_source,
        &mut runtime_target,
    )
    .map_err(|error| format!("committed-state repeat preview: {error:?}"))?;
    let repeated_rows = editor_rows(&repeated)?;
    let repeated_ready = count_eq(
        &repeated_rows,
        "disposition",
        "rfi.followUp.disposition.ready",
    );
    if repeated_ready != 0 || count_bool(&repeated_rows, "enabled", true) != 0 {
        return Err(format!(
            "committed private target is not idempotent: ready={repeated_ready} enabled={}",
            count_bool(&repeated_rows, "enabled", true)
        ));
    }

    let authority_path = runtime.join("journals");
    let restarted = FilesystemAuthority::new(&authority_path, Duration::from_secs(300));
    let caller = CallerIdentity::new("follow-up-audit-owner", "follow-up-audit-session");
    let undone = restarted
        .undo(journal_id, &caller, None)
        .map_err(|error| format!("resume committed journal Undo: {error:?}"))?;
    if undone.failed != 0
        || !undone.errors.is_empty()
        || undone.meta.get("effectState").and_then(Value::as_str) != Some("committed")
    {
        return Err(format!(
            "resumed Undo is not fully committed: failed={} errors={:?} effectState={:?}",
            undone.failed,
            undone.errors,
            undone.meta.get("effectState")
        ));
    }
    if fs::read(&source_path).map_err(|error| error.to_string())? != original_source
        || fs::read(&target_path).map_err(|error| error.to_string())? != original_target
        || sha256(&original_source_path)? != RFI_LOG_SHA256
        || sha256(&original_target_path)? != FOLLOW_UP_SHA256
    {
        return Err("resume Undo did not preserve source and restore runtime byte-identically to untouched outer inputs".to_string());
    }
    let report = json!({
        "mode": "resume-existing-committed-private-journal-undo-only",
        "journalId": journal_id,
        "runtimeDirectory": runtime,
        "appliedTargetSha256BeforeUndo": sha256_bytes(&applied_target),
        "guardedDeltaVerifiedAgainstOuterUntouchedTarget": true,
        "guardedCellCount": updates.values().map(BTreeMap::len).sum::<usize>(),
        "independentEligibleRoutesVerified": independent_routes_applied,
        "zeroRepeatReadyRows": repeated_ready == 0,
        "restoredRuntimeFollowUpSha256": sha256(&target_path)?,
        "restoredRuntimeRfiLogSha256": sha256(&source_path)?,
        "outerInputHashesUnchanged": {"rfiLog": sha256(&original_source_path)?, "followUp": sha256(&original_target_path)?},
        "byteIdenticalUndo": true,
        "undoMeta": undone.meta,
        "discovery": applied_discovery,
        "repeatPreview": editor_report(&repeated, &repeated_rows, &inventory),
    });
    write_new_json(&workspace.join("resume-undo-audit.json"), &report)?;
    println!(
        "resumed existing journal Undo committed; runtime restored byte-identically; Apply was not invoked"
    );
    Ok(())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn usage() -> String {
    "usage: follow_up_duplicate_audit <private-workspace> <baseline|apply|journal|resume-undo [journal-id]>".to_string()
}

fn ensure_private_copy(workspace: &Path, path: &Path, expected_sha: &str) -> Result<(), String> {
    if !path.starts_with(workspace) || !path.is_file() {
        return Err(format!(
            "input is not a private workspace copy: {}",
            path.display()
        ));
    }
    let actual = sha256(path)?;
    if actual != expected_sha {
        return Err(format!(
            "input SHA-256 mismatch for {}: {actual}",
            path.display()
        ));
    }
    Ok(())
}

fn sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn write_new_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create-new {}: {error}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn editor_rows(plan: &Plan) -> Result<Vec<Value>, String> {
    plan.meta
        .get("editor")
        .and_then(|editor| editor.get("rows"))
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "plan metadata lacks editor rows".to_string())
}

fn count_eq(rows: &[Value], key: &str, value: &str) -> usize {
    rows.iter()
        .filter(|row| row.get(key).and_then(Value::as_str) == Some(value))
        .count()
}

fn count_bool(rows: &[Value], key: &str, value: bool) -> usize {
    rows.iter()
        .filter(|row| row.get(key).and_then(Value::as_bool) == Some(value))
        .count()
}

fn editor_report(plan: &Plan, rows: &[Value], inventory: &Value) -> Value {
    let mut manual_reasons = BTreeMap::<String, usize>::new();
    for row in rows.iter().filter(|row| {
        row.get("disposition").and_then(Value::as_str)
            == Some("rfi.followUp.disposition.manualReview")
    }) {
        let reason = row
            .get("reasonKey")
            .and_then(Value::as_str)
            .unwrap_or("missingReasonKey")
            .to_string();
        *manual_reasons.entry(reason).or_default() += 1;
    }
    json!({
        "planId": plan.plan_id,
        "operationId": plan.op_id,
        "steps": plan.steps,
        "warnings": plan.warnings,
        "editor": plan.meta.get("editor"),
        "rowCounts": {
            "total": rows.len(),
            "enabled": count_bool(rows, "enabled", true),
            "ready": count_eq(rows, "disposition", "rfi.followUp.disposition.ready"),
            "alreadySynced": count_eq(rows, "disposition", "rfi.followUp.disposition.alreadySynced"),
            "manualReview": count_eq(rows, "disposition", "rfi.followUp.disposition.manualReview"),
            "manualReasons": manual_reasons,
        },
        "independentSourceContextInventory": inventory,
    })
}
fn discover_source_columns(
    sheet: &docflow_lib::ooxml::WorksheetData,
    discipline: &str,
) -> Result<SourceColumns, String> {
    Ok(SourceColumns {
        rfi: unique_header_column(sheet, discipline, &["RFI No."])?,
        itp: unique_header_column(sheet, discipline, &["ITP No:"])?,
        item: unique_header_column(sheet, discipline, &["Item No."])?,
        description: unique_header_column(sheet, discipline, &["INSPECTION DESCRIPTION"])?,
        tag: unique_header_column(sheet, discipline, &["TAG NO"])?,
        area: unique_header_column(sheet, discipline, &["AREA"])?,
        location: unique_header_column(
            sheet,
            discipline,
            &["Inspection Location", "INSP. LOC./AREA"],
        )?,
        lifecycle: unique_header_column(sheet, discipline, &["Inspection Status"])?,
        remarks: unique_header_column(sheet, discipline, &["REMARKS"])?,
    })
}

fn unique_header_column(
    sheet: &docflow_lib::ooxml::WorksheetData,
    discipline: &str,
    aliases: &[&str],
) -> Result<u32, String> {
    let aliases = aliases
        .iter()
        .map(|alias| normalize_header(alias))
        .collect::<BTreeSet<_>>();
    let matches = (1..=sheet.max_column)
        .filter(|column| aliases.contains(&normalize_header(sheet.value(9, *column))))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "{discipline} header aliases {aliases:?} matched {} columns; expected one unique row-9 header",
            matches.len()
        ));
    }
    Ok(matches[0])
}

fn normalize_header(header: &str) -> String {
    header
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

fn build_inventory(rfi_log_path: &Path, follow_up_path: &Path) -> Result<Value, String> {
    let source = read_workbook_selective(rfi_log_path, &["MEC", "CIV"])
        .map_err(|error| format!("read source inventory: {error}"))?;
    let targets = read_workbook_selective(follow_up_path, &["Signed By Worley MEC"])
        .map_err(|error| format!("read destination inventory: {error}"))?;
    let source_sheet = source
        .sheet("MEC")
        .ok_or_else(|| "source MEC sheet missing".to_string())?;
    let civ_sheet = source
        .sheet("CIV")
        .ok_or_else(|| "source CIV sheet missing".to_string())?;
    let target_sheet = targets
        .sheet("Signed By Worley MEC")
        .ok_or_else(|| "Signed By Worley MEC sheet missing".to_string())?;
    let mec_columns = discover_source_columns(source_sheet, "MEC")?;
    let civ_columns = discover_source_columns(civ_sheet, "CIV")?;
    let mut groups = BTreeMap::<String, Vec<SourceContext>>::new();
    for row in 10..=source_sheet.max_row {
        let full_rfi = source_sheet.value(row, mec_columns.rfi).trim();
        if !full_rfi.starts_with("AZE-SUN-INT-QMS-RFI-MEC-") {
            continue;
        }
        groups
            .entry(full_rfi.to_string())
            .or_default()
            .push(SourceContext {
                row,
                full_rfi: full_rfi.to_string(),
                itp: source_sheet.value(row, mec_columns.itp).to_string(),
                item: source_sheet.value(row, mec_columns.item).to_string(),
                description: source_sheet.value(row, mec_columns.description).to_string(),
                tag: source_sheet.value(row, mec_columns.tag).to_string(),
                area: source_sheet.value(row, mec_columns.area).to_string(),
                location: source_sheet.value(row, mec_columns.location).to_string(),
                lifecycle: source_sheet.value(row, mec_columns.lifecycle).to_string(),
                remarks: source_sheet.value(row, mec_columns.remarks).to_string(),
            });
    }
    let mut duplicates = Vec::new();
    let mut all_contexts = Vec::new();
    let mut duplicate_context_count = 0_usize;
    let mut eligible_context_count = 0_usize;
    let mut destination_gap_count = 0_usize;
    for (full_rfi, rows) in &groups {
        let mut distinct = BTreeSet::new();
        for source_record in rows {
            distinct.insert((
                source_record.itp.clone(),
                source_record.item.clone(),
                source_record.tag.clone(),
                source_record.area.clone(),
                source_record.location.clone(),
            ));
            let route = route_columns(source_record, target_sheet)?;
            let (rfi_column, status_column, category) = route.unwrap_or((0, 0, "unsupported"));
            let expected_status = if source_record
                .remarks
                .to_ascii_lowercase()
                .contains("signed by worley")
            {
                "Signed"
            } else {
                "Not Signed"
            };
            let candidates = if route.is_some()
                && source_record.lifecycle.eq_ignore_ascii_case("Completed")
                && !source_record.tag.trim().is_empty()
                && !source_record.area.trim().is_empty()
                && !source_record.location.trim().is_empty()
            {
                (5..=target_sheet.max_row)
                    .filter(|row| {
                        target_sheet.value(*row, 2) == source_record.location
                            && target_sheet.value(*row, 3) == source_record.tag
                            && target_sheet.value(*row, 4) == source_record.area
                    })
                    .map(|row| {
                        let current_rfi = target_sheet.value(row, rfi_column);
                        let current_status = target_sheet.value(row, status_column);
                        let expected_rfi = format!("MEC-{}", short_rfi(full_rfi));
                        let disposition = if current_rfi.is_empty() && current_status.is_empty() {
                            "prepared-blank"
                        } else if target_contains_rfi(current_rfi, &expected_rfi)
                            && current_status == expected_status
                        {
                            "already-matched"
                        } else if target_contains_rfi(current_rfi, &expected_rfi) {
                            "same-rfi-status-differs"
                        } else if current_rfi.is_empty() {
                            "status-only-conflict"
                        } else {
                            "occupied-conflict"
                        };
                        json!({
                            "worksheet": "Signed By Worley MEC",
                            "row": row,
                            "context": {
                                "zone": target_sheet.value(row, 1),
                                "area": target_sheet.value(row, 2),
                                "tag": target_sheet.value(row, 3),
                                "section": target_sheet.value(row, 4),
                            },
                            "category": category,
                            "rfiCell": cell_name(rfi_column, row),
                            "rfiCurrent": current_rfi,
                            "statusCell": cell_name(status_column, row),
                            "statusCurrent": current_status,
                            "expectedRfi": expected_rfi,
                            "expectedStatus": expected_status,
                            "disposition": disposition,
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let source_route = if route.is_none() {
                "unsupported-or-unknown-route"
            } else if !source_record.lifecycle.eq_ignore_ascii_case("Completed") {
                "excluded-noncompleted-lifecycle"
            } else if source_record.tag.trim().is_empty()
                || source_record.area.trim().is_empty()
                || source_record.location.trim().is_empty()
            {
                "unknown-incomplete-source-context"
            } else if candidates.is_empty() {
                "no-prepared-exact-context"
            } else if candidates.len() > 1 {
                "ambiguous-multiple-exact-contexts"
            } else if candidates[0]["disposition"] == "occupied-conflict"
                || candidates[0]["disposition"] == "status-only-conflict"
                || candidates[0]["disposition"] == "same-rfi-status-differs"
            {
                "real-target-conflict-or-status-review"
            } else {
                "eligible-exact-context"
            };
            let eligible = source_route == "eligible-exact-context";
            let gap = eligible && candidates[0]["disposition"] == "prepared-blank";
            eligible_context_count += eligible as usize;
            destination_gap_count += gap as usize;
            all_contexts.push(json!({
                "sourceRow": source_record.row,
                "rfi": full_rfi,
                "itp": source_record.itp,
                "item": source_record.item,
                "description": source_record.description,
                "lifecycle": source_record.lifecycle,
                "worleyRemarks": source_record.remarks,
                "tag": source_record.tag,
                "area": source_record.area,
                "inspectionLocation": source_record.location,
                "routeCategory": category,
                "expectedRfi": format!("MEC-{}", short_rfi(full_rfi)),
                "targetRfiColumn": if route.is_some() { json!(rfi_column) } else { Value::Null },
                "targetStatusColumn": if route.is_some() { json!(status_column) } else { Value::Null },
                "sourceRouteDisposition": source_route,
                "destinations": candidates,
            }));
        }
        if rows.len() > 1 {
            duplicate_context_count += distinct.len();
            duplicates.push(json!({
                "rfi": full_rfi,
                "sourceRowCount": rows.len(),
                "distinctSourceContextCount": distinct.len(),
                "contexts": rows.iter().map(|source_record| json!({
                    "sourceRow": source_record.row,
                    "itp": source_record.itp,
                    "item": source_record.item,
                    "tag": source_record.tag,
                    "area": source_record.area,
                    "inspectionLocation": source_record.location,
                })).collect::<Vec<_>>(),
            }));
        }
    }
    let mut civ_records = Vec::new();
    let mut civ_groups = BTreeMap::<String, Vec<u32>>::new();
    for row in 10..=civ_sheet.max_row {
        let rfi = civ_sheet.value(row, civ_columns.rfi).trim();
        if rfi.starts_with("AZE-SUN-INT-QMS-RFI-CIV-") {
            civ_groups.entry(rfi.to_string()).or_default().push(row);
            civ_records.push(json!({
                "sourceRow": row,
                "rfi": rfi,
                "itp": civ_sheet.value(row, civ_columns.itp),
                "item": civ_sheet.value(row, civ_columns.item),
                "description": civ_sheet.value(row, civ_columns.description),
                "tag": civ_sheet.value(row, civ_columns.tag),
                "area": civ_sheet.value(row, civ_columns.area),
                "inspectionLocation": civ_sheet.value(row, civ_columns.location),
                "lifecycle": civ_sheet.value(row, civ_columns.lifecycle),
                "worleyRemarks": civ_sheet.value(row, civ_columns.remarks),
                "sourceRouteDisposition": "excluded-discipline-CIV-not-routed-to-MEC-signed-target",
            }));
        }
    }
    let source_context_count = all_contexts.len();
    let source_0767 = groups
        .get("AZE-SUN-INT-QMS-RFI-MEC-0767")
        .ok_or_else(|| "MEC-0767 source rows missing from independent inventory".to_string())?;
    if source_0767.len() != 2 {
        return Err(format!(
            "expected two MEC-0767 source rows, found {}",
            source_0767.len()
        ));
    }
    let mec767_targets = json!({
        "header": {"worksheet": "Signed By Worley MEC", "rfi": "N4", "status": "O4", "itp": "N1", "stage": "N2", "clause": "N3"},
        "rows": [
            {"row":77,"context":{"zone":target_sheet.value(77,1),"area":target_sheet.value(77,2),"tag":target_sheet.value(77,3),"section":target_sheet.value(77,4)},"rfiCell":"N77","rfi":target_sheet.value(77,14),"statusCell":"O77","status":target_sheet.value(77,15)},
            {"row":80,"context":{"zone":target_sheet.value(80,1),"area":target_sheet.value(80,2),"tag":target_sheet.value(80,3),"section":target_sheet.value(80,4)},"rfiCell":"N80","rfi":target_sheet.value(80,14),"statusCell":"O80","status":target_sheet.value(80,15)},
        ]
    });
    Ok(json!({
        "basis": "independently read source MEC/CIV rows and Signed By Worley MEC target contexts; CIV is explicitly out of this MEC target scope",
        "sourceRecordCount": source_context_count + civ_records.len(),
        "mecSourceRecordCount": source_context_count,
        "civSourceRecordCount": civ_records.len(),
        "sourceRecordsByRouteDisposition": summarize_dispositions(&all_contexts),
        "independentlyEligibleExactMecContexts": eligible_context_count,
        "blankPreparedMecDestinationGaps": destination_gap_count,
        "mecDuplicateCanonicalIds": duplicates.len(),
        "distinctMecDuplicateContexts": duplicate_context_count,
        "mecDuplicateGroups": duplicates,
        "allMecSourceContexts": all_contexts,
        "civSourceRecordsByDisposition": {"excluded-discipline-CIV-not-routed-to-MEC-signed-target": civ_records.len()},
        "civDuplicateCanonicalIds": civ_groups.values().filter(|rows| rows.len() > 1).count(),
        "civDuplicateGroups": civ_groups.iter().filter(|(_, rows)| rows.len() > 1).map(|(rfi, rows)| json!({
            "rfi": rfi,
            "sourceRowCount": rows.len(),
            "sourceRows": rows,
            "disposition": "excluded-discipline-CIV-not-routed-to-MEC-signed-target",
        })).collect::<Vec<_>>(),
        "allCivSourceContexts": civ_records,
        "mec0767": {
            "sourceRows": source_0767.iter().map(|source_record| json!({
                "row": source_record.row,"rfi": source_record.full_rfi,"itp": source_record.itp,"item": source_record.item,
                "description": source_record.description,"tag": source_record.tag,"area": source_record.area,
                "inspectionLocation": source_record.location,"lifecycle": source_record.lifecycle,"worleyRemarks": source_record.remarks,
            })).collect::<Vec<_>>(),
            "targetLayout": {"worksheet": "Signed By Worley MEC", "itp": "N1", "stage": "N2", "clause": "N3", "rfiHeader": "N4", "statusHeader": "O4", "clauseValue": target_sheet.value(3,14), "rfiHeaderValue": target_sheet.value(4,14), "statusHeaderValue": target_sheet.value(4,15)},
            "targetRows": mec767_targets,
        },
    }))
}
fn short_rfi(full_rfi: &str) -> &str {
    full_rfi
        .strip_prefix("AZE-SUN-INT-QMS-RFI-MEC-")
        .unwrap_or(full_rfi)
}

fn route_columns(
    source: &SourceContext,
    target_sheet: &docflow_lib::ooxml::WorksheetData,
) -> Result<Option<(u32, u32, &'static str)>, String> {
    let description = normalize_header(&source.description);
    if !description.contains("FINALACCEPTANCE") {
        return Ok(None);
    }
    let (category, category_word) = if source.itp.ends_with("0001") && description.contains("PILE")
    {
        ("Pile (3.5)", "PILE")
    } else if source.itp.ends_with("0003") && description.contains("TRACKER") {
        ("Tracker (3.1)", "TRACKER")
    } else if source.itp.ends_with("0004") && description.contains("MODULE") {
        ("Module (3.2)", "MODULE")
    } else {
        return Ok(None);
    };
    let itp = normalize_header(&source.itp);
    let matches = (1..target_sheet.max_column)
        .filter(|column| {
            normalize_header(target_sheet.value(1, *column)).contains(&itp)
                && normalize_header(target_sheet.value(2, *column)) == "FINAL"
                && {
                    let clause = normalize_header(target_sheet.value(3, *column));
                    clause.contains("FINALACCEPTANCE") && clause.contains(category_word)
                }
                && normalize_header(target_sheet.value(4, *column)) == "RFINO"
                && normalize_header(target_sheet.value(4, *column + 1)) == "STATUS"
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "MEC source row {} ITP {:?} has {} unique FINAL {:?} ITP/clause/RFI-STATUS layouts in Signed By Worley MEC; expected one",
            source.row,
            source.itp,
            matches.len(),
            category
        ));
    }
    let column = matches[0];
    Ok(Some((column, column + 1, category)))
}

fn cell_name(column: u32, row: u32) -> String {
    let mut n = column;
    let mut letters = String::new();
    while n > 0 {
        let digit = (n - 1) % 26;
        letters.insert(0, char::from(b'A' + digit as u8));
        n = (n - 1) / 26;
    }
    format!("{letters}{row}")
}

fn assert_required_0767(plan: &Plan, inventory: &Value) -> Result<(), String> {
    let rows = editor_rows(plan)?;
    let list = inventory
        .pointer("/mec0767/sourceRows")
        .and_then(Value::as_array)
        .ok_or_else(|| "inventory lacks MEC-0767 source rows".to_string())?;
    if list.len() != 2 {
        return Err(
            "independent inventory does not contain exactly two MEC-0767 source rows".to_string(),
        );
    }
    let available = rows
        .iter()
        .filter(|row| {
            row.get("rfiNo").and_then(Value::as_str) == Some("MEC-0767")
                || row.get("rfiNo").and_then(Value::as_str) == Some("0767")
        })
        .collect::<Vec<_>>();
    if available.is_empty() {
        return Err("preview editor omitted MEC-0767 entirely".to_string());
    }
    let covered = rows.iter().any(|row| {
        row.get("_sheet").and_then(Value::as_str) == Some("Signed By Worley MEC")
            && row.get("_row").and_then(Value::as_u64) == Some(80)
            && row
                .get("target")
                .and_then(|target| target.get("rfiColumn"))
                .and_then(Value::as_u64)
                == Some(14)
            && row
                .get("target")
                .and_then(|target| target.get("statusColumn"))
                .and_then(Value::as_u64)
                == Some(15)
            && row.get("enabled").and_then(Value::as_bool) == Some(true)
    });
    if !covered {
        let matching = rows
            .iter()
            .filter(|row| {
                row.get("rfiNo")
                    .and_then(Value::as_str)
                    .is_some_and(|rfi| rfi.ends_with("0767"))
            })
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "expected enabled editor row targeting Signed By Worley MEC row80 N/O; observed {matching:?}"
        ));
    }
    Ok(())
}
fn target_contains_rfi(cell_value: &str, expected_rfi: &str) -> bool {
    cell_value
        .split(|character: char| {
            matches!(character, '/' | ',' | ';' | '|' | ':' | '\n' | '\r' | ' ')
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .any(|token| token == expected_rfi || format!("MEC-{token}") == expected_rfi)
}

fn assert_independent_routes_applied(
    inventory: &Value,
    target_sheet: &docflow_lib::ooxml::WorksheetData,
) -> Result<usize, String> {
    let contexts = inventory
        .get("allMecSourceContexts")
        .and_then(Value::as_array)
        .ok_or_else(|| "independent MEC inventory lacks all source contexts".to_string())?;
    let mut verified = 0;
    for context in contexts.iter().filter(|context| {
        context
            .get("sourceRouteDisposition")
            .and_then(Value::as_str)
            == Some("eligible-exact-context")
    }) {
        let source_row = context
            .get("sourceRow")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let candidate = context.pointer("/destinations/0").ok_or_else(|| {
            format!("eligible source row {source_row} has no independently found destination")
        })?;
        let row = candidate
            .get("row")
            .and_then(Value::as_u64)
            .unwrap_or_default() as u32;
        let rfi_column = context
            .get("targetRfiColumn")
            .and_then(Value::as_u64)
            .unwrap_or_default() as u32;
        let status_column = context
            .get("targetStatusColumn")
            .and_then(Value::as_u64)
            .unwrap_or_default() as u32;
        let expected_rfi = candidate
            .get("expectedRfi")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "independent destination for MEC source row {source_row} lacks expected RFI"
                )
            })?;
        let expected_status = candidate
            .get("expectedStatus")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "independent destination for MEC source row {source_row} lacks expected status"
                )
            })?;
        let actual_rfi = target_sheet.value(row, rfi_column);
        let actual_status = target_sheet.value(row, status_column);
        if !target_contains_rfi(actual_rfi, expected_rfi) || actual_status != expected_status {
            return Err(format!(
                "unexplained independent-route gap for MEC source row {source_row}: target row {row} got {actual_rfi:?}/{actual_status:?}, expected code {expected_rfi:?} and status {expected_status:?}"
            ));
        }
        verified += 1;
    }
    Ok(verified)
}
fn summarize_dispositions(contexts: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for context in contexts {
        let disposition = context
            .get("sourceRouteDisposition")
            .and_then(Value::as_str)
            .unwrap_or("missing-disposition")
            .to_string();
        *counts.entry(disposition).or_insert(0) += 1;
    }
    counts
}
