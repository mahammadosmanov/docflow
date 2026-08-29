use std::{
    fs::File,
    io::{Cursor, Seek, SeekFrom, Write},
    time::Duration,
};

use docflow_lib::{
    fs_ops::FilesystemAuthority,
    ooxml::{
        OrderedWorkbookDiscovery, WorkbookData, ordered_workbook_discovery,
        read_workbook_from_reader,
    },
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    rfi_follow_up::{
        canonical_rfi, classify_workbooks, guarded_updates, mapped_follow_up_status,
        preview_from_open_files, revise_from_open_files,
    },
};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn package(sheets: &[(&str, Option<&str>, String)]) -> Cursor<Vec<u8>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut workbook_sheets = String::new();
    let mut relationships = String::new();
    for (index, (name, state, worksheet)) in sheets.iter().enumerate() {
        let ordinal = index + 1;
        let state = state
            .map(|value| format!(r#" state="{value}""#))
            .unwrap_or_default();
        workbook_sheets.push_str(&format!(
            r#"<sheet name="{name}" sheetId="{ordinal}" r:id="rId{ordinal}"{state}/>"#
        ));
        relationships.push_str(&format!(
            r#"<Relationship Id="rId{ordinal}" Type="{OFFICE_REL}/worksheet" Target="worksheets/sheet{ordinal}.xml"/>"#
        ));
        writer
            .start_file(format!("xl/worksheets/sheet{ordinal}.xml"), options)
            .unwrap();
        writer.write_all(worksheet.as_bytes()).unwrap();
    }
    let package_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL}"><Relationship Id="rIdWorkbook" Type="{OFFICE_REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="{OFFICE_REL}"><sheets>{workbook_sheets}</sheets></workbook>"#
    );
    let relationships =
        format!(r#"<Relationships xmlns="{PACKAGE_REL}">{relationships}</Relationships>"#);
    for (name, bytes) in [
        ("[Content_Types].xml", b"content-types".as_slice()),
        ("_rels/.rels", package_relationships.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", relationships.as_bytes()),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap()
}

fn parsed(mut source: Cursor<Vec<u8>>) -> (OrderedWorkbookDiscovery, WorkbookData) {
    let discovery = ordered_workbook_discovery(&mut source).unwrap();
    source.seek(SeekFrom::Start(0)).unwrap();
    let workbook = read_workbook_from_reader(&mut source).unwrap();
    (discovery, workbook)
}

fn inline(reference: &str, value: &str) -> String {
    format!(r#"<c r="{reference}" t="inlineStr"><is><t>{value}</t></is></c>"#)
}

fn blank(reference: &str) -> String {
    format!(r#"<c r="{reference}" t="inlineStr"><is><t></t></is></c>"#)
}

fn formula(reference: &str, expression: &str, cached: &str) -> String {
    format!(r#"<c r="{reference}" t="str"><f>{expression}</f><v>{cached}</v></c>"#)
}

fn row(number: u32, cells: impl IntoIterator<Item = String>) -> String {
    format!(
        r#"<row r="{number}">{}</row>"#,
        cells.into_iter().collect::<String>()
    )
}

fn worksheet(rows: impl IntoIterator<Item = String>, merges: &[&str]) -> String {
    let rows = rows.into_iter().collect::<String>();
    let merge_xml = if merges.is_empty() {
        String::new()
    } else {
        format!(
            r#"<mergeCells count="{}">{}</mergeCells>"#,
            merges.len(),
            merges
                .iter()
                .map(|range| format!(r#"<mergeCell ref="{range}"/>"#))
                .collect::<String>()
        )
    };
    format!(r#"<worksheet xmlns="{MAIN}"><sheetData>{rows}</sheetData>{merge_xml}</worksheet>"#)
}

fn source_package(civ: String, mec: String) -> Cursor<Vec<u8>> {
    package(&[
        ("CIV", None, civ),
        ("MEC", None, mec),
        ("ELE", None, worksheet(Vec::<String>::new(), &[])),
        ("PRC", None, worksheet(Vec::<String>::new(), &[])),
    ])
}

fn target_package(sheet: String) -> Cursor<Vec<u8>> {
    package(&[("Follow Up", None, sheet)])
}

fn classify(
    source: Cursor<Vec<u8>>,
    target: Cursor<Vec<u8>>,
) -> Result<Value, docflow_lib::errors::BackendError> {
    let (source_discovery, source_workbook) = parsed(source);
    let (target_discovery, target_workbook) = parsed(target);
    classify_workbooks(
        &source_discovery,
        &source_workbook,
        &target_discovery,
        &target_workbook,
        false,
    )
}

fn target_with_rows(rows: impl IntoIterator<Item = String>, merges: &[&str]) -> Cursor<Vec<u8>> {
    target_package(worksheet(
        std::iter::once(row(1, [inline("A1", "RFI NO."), inline("B1", "STATUS")])).chain(rows),
        merges,
    ))
}

#[test]
fn preview_from_open_files_builds_an_inert_exact_workbook_plan() {
    let fixture = tempfile::tempdir().unwrap();
    let source_path = fixture.path().join("RFI Log.xlsx");
    let target_path = fixture.path().join("Follow Up.xlsx");
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    std::fs::write(
        &source_path,
        source_package(
            worksheet(
                [row(10, [inline("A10", rfi), inline("P10", "Completed")])],
                &[],
            ),
            worksheet(Vec::<String>::new(), &[]),
        )
        .into_inner(),
    )
    .unwrap();
    std::fs::write(
        &target_path,
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]).into_inner(),
    )
    .unwrap();
    let mut source = File::open(&source_path).unwrap();
    let mut target = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();

    let plan =
        preview_from_open_files(&source_path, &target_path, &mut source, &mut target).unwrap();

    assert!(plan.plan_id.is_empty());
    assert_eq!(plan.op_id, "rfi.follow_up_sync");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].src.as_deref(), target_path.to_str());
    assert!(
        plan.steps[0]
            .dst
            .as_deref()
            .unwrap()
            .contains(".docflow-s5-backup-")
    );
    assert!(!fixture.path().join(".docflow-backups").exists());
}

#[test]
fn follow_up_retained_apply_revalidates_open_workbooks_at_every_boundary() {
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let source_path = root.join("RFI Log.xlsx");
    let target_path = root.join("Follow Up.xlsx");
    let journal_path = root.join("journals");
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    let source_bytes = source_package(
        worksheet(
            [row(10, [inline("A10", rfi), inline("P10", "Completed")])],
            &[],
        ),
        worksheet(Vec::<String>::new(), &[]),
    )
    .into_inner();
    let original_target =
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]).into_inner();
    std::fs::write(&source_path, &source_bytes).unwrap();
    std::fs::write(&target_path, &original_target).unwrap();
    let mut source = File::open(&source_path).unwrap();
    let mut target = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();
    let plan =
        preview_from_open_files(&source_path, &target_path, &mut source, &mut target).unwrap();

    let authority = FilesystemAuthority::new(&journal_path, Duration::from_secs(300));
    let caller = CallerIdentity::new("owner", "session");
    let scope = GrantScope::new("owner", "session", "request", "rfi.follow_up_sync");
    let source_grant = authority
        .issue_grant(&scope, &root, GrantAccess::Read)
        .unwrap();
    let target_grant = authority
        .issue_grant(&scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let plan = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner",
                "session",
                "rfi.follow_up_sync",
                vec![source_grant.grant_id, target_grant.grant_id],
            ),
            plan,
        )
        .unwrap();

    let result = authority.apply(&plan.plan_id, &caller).unwrap();

    assert_eq!(result.applied, 1, "{:?}", result.errors);
    assert_eq!(result.failed, 0);
    assert_eq!(std::fs::read(&source_path).unwrap(), source_bytes);
    let (_, applied) = parsed(Cursor::new(std::fs::read(&target_path).unwrap()));
    assert_eq!(
        applied.sheet("Follow Up").unwrap().cells[&(2, 2)].value,
        "Completed"
    );
    let names = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    assert!(
        names
            .iter()
            .any(|name| name.starts_with(".docflow-s5-backup-"))
    );
    assert!(
        names
            .iter()
            .any(|name| name.starts_with(".docflow-s5-retained-original-"))
    );
    assert!(root.join(".docflow-backups").is_dir());
    let retained = result.meta["retainedEvidence"].as_array().unwrap();
    assert_eq!(retained.len(), 3);
    assert!(retained.iter().all(|item| item.get("path").is_none()));
    let restarted = FilesystemAuthority::new(&journal_path, Duration::from_secs(300));
    assert!(restarted.recover_pending(&caller).unwrap().is_empty());
}

#[test]
fn follow_up_retained_undo_restores_exact_backup_and_preserves_all_evidence() {
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let source_path = root.join("RFI Log.xlsx");
    let target_path = root.join("Follow Up.xlsx");
    let journal_path = root.join("journals");
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    let source_bytes = source_package(
        worksheet(
            [row(10, [inline("A10", rfi), inline("P10", "Completed")])],
            &[],
        ),
        worksheet(Vec::<String>::new(), &[]),
    )
    .into_inner();
    let original_target =
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]).into_inner();
    std::fs::write(&source_path, &source_bytes).unwrap();
    std::fs::write(&target_path, &original_target).unwrap();
    let mut source = File::open(&source_path).unwrap();
    let mut target = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();
    let plan =
        preview_from_open_files(&source_path, &target_path, &mut source, &mut target).unwrap();

    let authority = FilesystemAuthority::new(&journal_path, Duration::from_secs(300));
    let caller = CallerIdentity::new("owner", "session");
    let scope = GrantScope::new("owner", "session", "request", "rfi.follow_up_sync");
    let source_grant = authority
        .issue_grant(&scope, &root, GrantAccess::Read)
        .unwrap();
    let target_grant = authority
        .issue_grant(&scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let plan = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner",
                "session",
                "rfi.follow_up_sync",
                vec![source_grant.grant_id, target_grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let applied = authority.apply(&plan.plan_id, &caller).unwrap();
    assert_eq!(applied.applied, 1, "{:?}", applied.errors);
    let applied_target = std::fs::read(&target_path).unwrap();
    assert_ne!(applied_target, original_target);
    let rejected = authority
        .undo(&applied.journal_id, &caller, Some(&root.join("ignored")))
        .unwrap_err();
    assert_eq!(rejected.code, "validation");
    assert_eq!(std::fs::read(&target_path).unwrap(), applied_target);
    std::fs::write(&target_path, b"later user edit").unwrap();
    let rejected = authority
        .undo(&applied.journal_id, &caller, None)
        .unwrap_err();
    assert_eq!(rejected.code, "validation");
    assert_eq!(std::fs::read(&target_path).unwrap(), b"later user edit");
    std::fs::write(&target_path, &applied_target).unwrap();

    let undone = authority.undo(&applied.journal_id, &caller, None).unwrap();

    assert_eq!((undone.applied, undone.failed), (1, 0));
    assert_eq!(std::fs::read(&source_path).unwrap(), source_bytes);
    assert_eq!(std::fs::read(&target_path).unwrap(), original_target);
    assert_eq!(undone.meta["effectState"], "committed");
    let retained = undone.meta["retainedEvidence"].as_array().unwrap();
    assert_eq!(retained.len(), 4);
    assert!(retained.iter().all(|item| item.get("path").is_none()));
    assert!(
        root.read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".docflow-s5-retained-applied-")
            })
    );
    let restarted = FilesystemAuthority::new(&journal_path, Duration::from_secs(300));
    assert!(restarted.recover_pending(&caller).unwrap().is_empty());
}

#[test]
fn follow_up_retained_recovery_completes_exact_applied_state_after_restart() {
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let source_path = root.join("RFI Log.xlsx");
    let target_path = root.join("Follow Up.xlsx");
    let journal_path = root.join("journals");
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    std::fs::write(
        &source_path,
        source_package(
            worksheet(
                [row(10, [inline("A10", rfi), inline("P10", "Completed")])],
                &[],
            ),
            worksheet(Vec::<String>::new(), &[]),
        )
        .into_inner(),
    )
    .unwrap();
    std::fs::write(
        &target_path,
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]).into_inner(),
    )
    .unwrap();
    let mut source = File::open(&source_path).unwrap();
    let mut target = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();
    let plan =
        preview_from_open_files(&source_path, &target_path, &mut source, &mut target).unwrap();
    let authority = FilesystemAuthority::new(&journal_path, Duration::from_secs(300));
    let caller = CallerIdentity::new("owner", "session");
    let scope = GrantScope::new("owner", "session", "request", "rfi.follow_up_sync");
    let source_grant = authority
        .issue_grant(&scope, &root, GrantAccess::Read)
        .unwrap();
    let target_grant = authority
        .issue_grant(&scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let plan = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner",
                "session",
                "rfi.follow_up_sync",
                vec![source_grant.grant_id, target_grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let applied = authority.apply(&plan.plan_id, &caller).unwrap();
    assert_eq!(applied.applied, 1, "{:?}", applied.errors);

    let journal_file = std::fs::read_dir(&journal_path)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .unwrap();
    let mut journal: Value =
        serde_json::from_slice(&std::fs::read(&journal_file).unwrap()).unwrap();
    let completed = journal["steps"][0].clone();
    let evidence = journal["retainedEvidence"].as_array().unwrap();
    let backup = evidence
        .iter()
        .find(|item| item["role"] == "backup")
        .unwrap()
        .clone();
    let private = evidence
        .iter()
        .find(|item| item["role"] == "private_output")
        .unwrap()
        .clone();
    let retained = evidence
        .iter()
        .find(|item| item["role"] == "displaced_original")
        .unwrap()
        .clone();
    journal["steps"] = json!([]);
    journal["state"] = json!("applying");
    journal["followUpPhase"] = json!("publication_active");
    journal["pendingRetainedArtifact"] = json!({
        "role": "displaced_original",
        "path": retained["path"],
        "locator": retained["locator"],
        "creationState": "intent"
    });
    journal["pendingPublication"] = json!({
        "purpose": "apply",
        "canonical": target_path,
        "retained": retained["path"],
        "backup": backup["path"],
        "publicationSource": private["path"],
        "expectedCanonicalIdentity": retained["identity"],
        "expectedCanonicalSha256": retained["sha256"],
        "expectedBackupIdentity": backup["identity"],
        "expectedBackupSha256": backup["sha256"],
        "publicationSourceIdentity": private["identity"],
        "publicationSha256": private["sha256"],
        "expectedCanonicalSecuritySha256": retained["securitySha256"],
        "expectedBackupSecuritySha256": backup["securitySha256"],
        "publicationSecuritySha256": private["securitySha256"],
        "expectedCanonicalDefaultStreamBytes": retained["defaultStreamBytes"],
        "expectedCanonicalAuxiliaryChargeBytes": retained["auxiliaryChargeBytes"],
        "expectedCanonicalLogicalChargeBytes": retained["logicalChargeBytes"],
        "destinationAccessSha256": "0".repeat(64),
        "destinationAuxiliarySha256": "0".repeat(64),
        "destinationAuxiliaryChargeBytes": retained["auxiliaryChargeBytes"],
        "retainedIdentity": retained["identity"],
        "retainedSha256": retained["sha256"],
        "retainedSecuritySha256": retained["securitySha256"],
        "createdCanonicalIdentity": completed["meta"]["createdCanonicalIdentity"],
        "phase": "verifying_canonical"
    });
    std::fs::write(&journal_file, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();
    drop(authority);

    let pending_journal = std::fs::read(&journal_file).unwrap();
    let pending_canonical = std::fs::read(&target_path).unwrap();
    let notices = (0_u64..65)
        .map(|index| {
            let path = journal_path.join(format!(".invalid-s5-{index:064x}.json"));
            std::fs::write(&path, b"{}").unwrap();
            path
        })
        .collect::<Vec<_>>();
    let restarted = FilesystemAuthority::new(&journal_path, Duration::from_secs(300));
    let bounded = restarted.recover_pending(&caller).unwrap_err();
    assert_eq!(bounded.code, "journal_invalid");
    assert_eq!(std::fs::read(&journal_file).unwrap(), pending_journal);
    assert_eq!(std::fs::read(&target_path).unwrap(), pending_canonical);
    assert!(
        notices
            .iter()
            .all(|notice| std::fs::read(notice).unwrap() == b"{}")
    );
    std::fs::remove_file(notices.last().unwrap()).unwrap();

    assert!(restarted.recover_pending(&caller).unwrap().is_empty());
    assert!(restarted.recover_pending(&caller).unwrap().is_empty());
    let recovered: Value = serde_json::from_slice(&std::fs::read(&journal_file).unwrap()).unwrap();
    assert_eq!(recovered["state"], "committed");
    assert_eq!(recovered["steps"].as_array().unwrap().len(), 1);
    assert!(recovered.get("pendingPublication").is_none());
    assert_eq!(recovered["retainedEvidence"].as_array().unwrap().len(), 3);
    assert!(
        notices[..64]
            .iter()
            .all(|notice| std::fs::read(notice).unwrap() == b"{}")
    );
}

#[test]
fn revise_from_open_files_changes_only_enabled_and_preserves_plan_identity() {
    let fixture = tempfile::tempdir().unwrap();
    let source_path = fixture.path().join("RFI Log.xlsx");
    let target_path = fixture.path().join("Follow Up.xlsx");
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    std::fs::write(
        &source_path,
        source_package(
            worksheet(
                [row(10, [inline("A10", rfi), inline("P10", "Completed")])],
                &[],
            ),
            worksheet(Vec::<String>::new(), &[]),
        )
        .into_inner(),
    )
    .unwrap();
    std::fs::write(
        &target_path,
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]).into_inner(),
    )
    .unwrap();
    let mut source = File::open(&source_path).unwrap();
    let mut target = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();
    let mut plan =
        preview_from_open_files(&source_path, &target_path, &mut source, &mut target).unwrap();
    plan.plan_id = "preserved-plan".to_string();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["enabled"] = Value::Bool(false);

    let revised = revise_from_open_files(&plan, &rows, &mut source, &mut target).unwrap();

    assert_eq!(revised.plan_id, "preserved-plan");
    assert!(revised.steps.is_empty());
    assert_eq!(revised.meta["editor"]["rows"][0]["enabled"], false);
    assert_eq!(
        revised.warnings.last().unwrap().key,
        "warn.rfi.followUp.noRowsSelected"
    );
}

#[test]
fn duplicate_approved_coordinates_rejected_before_guarded_map() {
    let fixture = tempfile::tempdir().unwrap();
    let source_path = fixture.path().join("RFI Log.xlsx");
    let target_path = fixture.path().join("Follow Up.xlsx");
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    std::fs::write(
        &source_path,
        source_package(
            worksheet(
                [row(10, [inline("A10", rfi), inline("P10", "Completed")])],
                &[],
            ),
            worksheet(Vec::<String>::new(), &[]),
        )
        .into_inner(),
    )
    .unwrap();
    std::fs::write(
        &target_path,
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]).into_inner(),
    )
    .unwrap();
    let mut source = File::open(&source_path).unwrap();
    let mut target = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();
    let mut plan =
        preview_from_open_files(&source_path, &target_path, &mut source, &mut target).unwrap();
    let step = &mut plan.steps[0];
    let updates = step.meta["updates"].as_array_mut().unwrap();
    updates.push(updates[0].clone());

    let error = guarded_updates(step).unwrap_err();

    assert_eq!(error.code, "validation");
    assert_eq!(error.message, "error.rfi.followUp.workbookInvalid");
}

#[test]
fn canonical_rfi_uses_the_entire_nfc_trimmed_uppercased_value_without_internal_repair() {
    assert_eq!(
        canonical_rfi("  aze-sun-int-qms-rfi-civ-0001  ").as_deref(),
        Some("AZE-SUN-INT-QMS-RFI-CIV-0001")
    );
    assert_eq!(
        canonical_rfi("AZE-SUN-INT-QMS-RFI-CI\u{56}\u{301}-0001"),
        None
    );
    for rejected in [
        "prefix AZE-SUN-INT-QMS-RFI-CIV-0001",
        "AZE SUN INT QMS RFI CIV 0001",
        "AZE-SUN-INT-QMS-RFI-CIV -0001",
        "AZE-SUN-INT-QMS-RFI-CIV-1",
        "AZE-SUN-INT-QMS-RFI-CIV-0001 suffix",
    ] {
        assert_eq!(canonical_rfi(rejected), None, "{rejected}");
    }
}

#[test]
fn source_matrix_allows_only_exact_completed_and_ongoing() {
    assert_eq!(mapped_follow_up_status("Completed"), Some("Completed"));
    assert_eq!(mapped_follow_up_status("Ongoing"), Some("In Progress"));
    for rejected in [
        "completed",
        "ONGOING",
        " Completed ",
        "Cancelled",
        "Rejected",
        "",
    ] {
        assert_eq!(mapped_follow_up_status(rejected), None, "{rejected}");
    }
}

#[test]
fn source_duplicates_require_same_safe_status() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    let civ = worksheet(
        [row(10, [inline("A10", rfi), inline("P10", "Completed")])],
        &[],
    );
    let same = worksheet(
        [row(10, [inline("A10", rfi), inline("Q10", "Completed")])],
        &[],
    );
    let ready = classify(
        source_package(civ.clone(), same),
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]),
    )
    .unwrap();
    assert_eq!(
        ready["rows"][0]["disposition"],
        "rfi.followUp.disposition.ready"
    );
    assert_eq!(ready["rows"][0]["sourceStatuses"], json!(["Completed"]));
    assert_eq!(
        ready["rows"][0]["sourceEvidence"]
            .as_array()
            .unwrap()
            .iter()
            .map(|evidence| evidence["sheet"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["CIV", "MEC"]
    );

    let conflict = worksheet(
        [row(10, [inline("A10", rfi), inline("Q10", "Ongoing")])],
        &[],
    );
    let manual = classify(
        source_package(civ, conflict),
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]),
    )
    .unwrap();
    assert_eq!(
        manual["rows"][0]["reasonKey"],
        "warn.rfi.followUp.reason.sourceConflict"
    );
    assert_eq!(manual["rows"][0]["selectable"], false);
}

#[test]
fn source_evidence_per_rfi_limit_fails_before_projection_amplification() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    let civ_rows = (10..510).map(|number| {
        row(
            number,
            [
                inline(&format!("A{number}"), rfi),
                inline(&format!("P{number}"), "Completed"),
            ],
        )
    });
    let mec = worksheet(
        [row(10, [inline("A10", rfi), inline("Q10", "Completed")])],
        &[],
    );

    assert_eq!(
        classify(
            source_package(worksheet(civ_rows, &[]), mec),
            target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]),
        )
        .unwrap_err()
        .message,
        "error.rfi.followUp.limitExceeded"
    );
}

#[test]
fn projection_budget_rejects_500_by_500_before_owned_materialization() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    let source_rows = (10..510).map(|number| {
        row(
            number,
            [
                inline(&format!("A{number}"), rfi),
                inline(&format!("P{number}"), "Completed"),
            ],
        )
    });
    let target_rows = (2..502).map(|number| {
        row(
            number,
            [
                inline(&format!("A{number}"), rfi),
                blank(&format!("B{number}")),
            ],
        )
    });

    assert_eq!(
        classify(
            source_package(
                worksheet(source_rows, &[]),
                worksheet(Vec::<String>::new(), &[]),
            ),
            target_with_rows(target_rows, &[]),
        )
        .unwrap_err()
        .message,
        "error.rfi.followUp.limitExceeded"
    );
}

#[test]
fn merge_query_budget_bounds_duplicate_target_range_amplification() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    let source_rows = (10..310)
        .map(|number| {
            row(
                number,
                [
                    inline(&format!("A{number}"), rfi),
                    inline(&format!("P{number}"), "Completed"),
                ],
            )
        })
        .collect::<String>();
    let target_rows = std::iter::once(row(1, [inline("A1", "RFI NO."), inline("B1", "STATUS")]))
        .chain((2..502).map(|number| {
            row(
                number,
                [
                    inline(&format!("A{number}"), rfi),
                    blank(&format!("B{number}")),
                ],
            )
        }))
        .collect::<String>();
    let merges = (1_000..2_414)
        .map(|number| format!(r#"<mergeCell ref="Z{number}:Z{number}"/>"#))
        .collect::<String>();
    let source = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData>{source_rows}</sheetData><mergeCells count="1414">{merges}</mergeCells></worksheet>"#
    );
    let target = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData>{target_rows}</sheetData><mergeCells count="1414">{merges}</mergeCells></worksheet>"#
    );

    assert_eq!(
        classify(
            source_package(source, worksheet(Vec::<String>::new(), &[]),),
            target_package(target),
        )
        .unwrap_err()
        .message,
        "error.rfi.followUp.limitExceeded"
    );
}

#[test]
fn follow_up_authority_and_layout_require_exact_safe_bounded_inputs() {
    let source = source_package(
        worksheet(Vec::<String>::new(), &[]),
        worksheet(Vec::<String>::new(), &[]),
    );
    let valid = classify(source.clone(), target_with_rows(Vec::<String>::new(), &[])).unwrap();
    assert_eq!(valid["layout"]["headerRow"], 1);
    assert_eq!(valid["layout"]["rfiColumn"], 1);
    assert_eq!(valid["layout"]["statusColumn"], 2);

    let missing = target_package(worksheet(
        [row(1, [inline("A1", "RFI NO"), inline("B1", "STATUS")])],
        &[],
    ));
    assert_eq!(
        classify(source.clone(), missing).unwrap_err().message,
        "error.rfi.followUp.layoutMissing"
    );

    let out_of_bounds = target_package(worksheet(
        [row(33, [inline("A33", "RFI NO."), inline("B33", "STATUS")])],
        &[],
    ));
    assert_eq!(
        classify(source.clone(), out_of_bounds).unwrap_err().message,
        "error.rfi.followUp.layoutMissing"
    );

    let duplicate_formula_header = target_package(worksheet(
        [row(
            1,
            [
                formula("A1", "LOOKUP()", "RFI NO."),
                inline("B1", "STATUS"),
                inline("C1", "RFI NO."),
            ],
        )],
        &[],
    ));
    assert_eq!(
        classify(source.clone(), duplicate_formula_header)
            .unwrap_err()
            .message,
        "error.rfi.followUp.layoutAmbiguous"
    );

    let hidden = package(&[(
        "Hidden Follow Up",
        Some("hidden"),
        worksheet(
            [row(1, [inline("A1", "RFI NO."), inline("B1", "STATUS")])],
            &[],
        ),
    )]);
    assert_eq!(
        classify(source.clone(), hidden).unwrap_err().message,
        "error.rfi.followUp.protected"
    );

    let protected_sheet = worksheet(
        [row(1, [inline("A1", "RFI NO."), inline("B1", "STATUS")])],
        &[],
    )
    .replace("</worksheet>", "<sheetProtection sheet=\"1\"/></worksheet>");
    assert_eq!(
        classify(source.clone(), target_package(protected_sheet))
            .unwrap_err()
            .message,
        "error.rfi.followUp.protected"
    );

    let (source_discovery, source_workbook) = parsed(source.clone());
    let (signed_discovery, signed_workbook) = parsed(target_with_rows(Vec::<String>::new(), &[]));
    assert_eq!(
        classify_workbooks(
            &source_discovery,
            &source_workbook,
            &signed_discovery,
            &signed_workbook,
            true,
        )
        .unwrap_err()
        .message,
        "error.rfi.followUp.signedPackage"
    );

    let ambiguous = package(&[
        (
            "First",
            None,
            worksheet(
                [row(1, [inline("A1", "RFI NO."), inline("B1", "STATUS")])],
                &[],
            ),
        ),
        (
            "Second",
            None,
            worksheet(
                [row(1, [inline("D1", "RFI NO."), inline("F1", "STATUS")])],
                &[],
            ),
        ),
    ]);
    assert_eq!(
        classify(source, ambiguous).unwrap_err().message,
        "error.rfi.followUp.layoutAmbiguous"
    );
}

#[test]
fn empty_physical_high_row_enforces_source_limit() {
    let source = source_package(
        worksheet([row(10_010, Vec::<String>::new())], &[]),
        worksheet(Vec::<String>::new(), &[]),
    );

    assert_eq!(
        classify(source, target_with_rows(Vec::<String>::new(), &[]))
            .unwrap_err()
            .message,
        "error.rfi.followUp.limitExceeded"
    );
}

#[test]
fn classification_keeps_unsafe_rows_visible_but_unselectable() {
    let completed = "AZE-SUN-INT-QMS-RFI-CIV-0001";
    let unknown = "AZE-SUN-INT-QMS-RFI-CIV-0002";
    let controlled = "AZE-SUN-INT-QMS-RFI-CIV-0003";
    let source = source_package(
        worksheet(
            [
                row(10, [inline("A10", completed), inline("P10", "Completed")]),
                row(11, [inline("A11", unknown), inline("P11", "Cancelled")]),
                row(12, [inline("A12", controlled), inline("P12", "Completed")]),
            ],
            &[],
        ),
        worksheet(Vec::<String>::new(), &[]),
    );
    let review = classify(
        source,
        target_with_rows(
            [
                row(2, [inline("A2", completed), blank("B2")]),
                row(3, [inline("A3", "not-an-rfi"), blank("B3")]),
                row(
                    4,
                    [inline("A4", "AZE-SUN-INT-QMS-RFI-CIV-9999"), blank("B4")],
                ),
                row(5, [inline("A5", unknown), blank("B5")]),
                row(6, [inline("A6", controlled), inline("B6", "Pending")]),
                row(7, [blank("A7"), inline("B7", "Completed")]),
            ],
            &[],
        ),
    )
    .unwrap();
    assert_eq!(review["rows"].as_array().unwrap().len(), 6);
    assert_eq!(review["rows"][0]["enabled"], true);
    let reasons = review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|row| row["reasonKey"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        vec![
            "warn.rfi.followUp.reason.invalidRfi",
            "warn.rfi.followUp.reason.sourceMissing",
            "warn.rfi.followUp.reason.sourceStatusUnknown",
            "warn.rfi.followUp.reason.targetStatusUnknown",
            "warn.rfi.followUp.reason.blankRfi",
        ]
    );
    assert!(
        review["rows"]
            .as_array()
            .unwrap()
            .iter()
            .skip(1)
            .all(|row| row["enabled"] == false && row["selectable"] == false)
    );
}

#[test]
fn formula_and_merge_facts_drive_manual_review() {
    let source_formula = "AZE-SUN-INT-QMS-RFI-CIV-0008";
    let source_merged = "AZE-SUN-INT-QMS-RFI-CIV-0009";
    let target_formula = "AZE-SUN-INT-QMS-RFI-CIV-0010";
    let target_merged = "AZE-SUN-INT-QMS-RFI-CIV-0011";
    let source = source_package(
        worksheet(
            [
                row(
                    10,
                    [
                        inline("A10", source_formula),
                        formula("P10", "LOOKUP()", "Completed"),
                    ],
                ),
                row(
                    11,
                    [inline("A11", source_merged), inline("P11", "Completed")],
                ),
                row(
                    12,
                    [inline("A12", target_formula), inline("P12", "Completed")],
                ),
                row(
                    13,
                    [inline("A13", target_merged), inline("P13", "Completed")],
                ),
            ],
            &["P11:Q11"],
        ),
        worksheet(Vec::<String>::new(), &[]),
    );
    let review = classify(
        source,
        target_with_rows(
            [
                row(2, [inline("A2", source_formula), blank("B2")]),
                row(3, [inline("A3", source_merged), blank("B3")]),
                row(4, [formula("A4", "LOOKUP()", target_formula), blank("B4")]),
                row(5, [inline("A5", target_merged), blank("B5")]),
            ],
            &["B5:C5"],
        ),
    )
    .unwrap();
    assert_eq!(
        review["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["reasonKey"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "warn.rfi.followUp.reason.sourceFormula",
            "warn.rfi.followUp.reason.sourceMerged",
            "warn.rfi.followUp.reason.targetRfiFormula",
            "warn.rfi.followUp.reason.targetMerged",
        ]
    );
}

fn compatibility_source() -> Cursor<Vec<u8>> {
    let entries = [
        (10, "0001", Some("Completed")),
        (11, "0002", Some("Ongoing")),
        (12, "0003", Some("Completed")),
        (13, "0004", None),
        (14, "0005", Some("Cancelled")),
        (15, "0006", Some("Completed")),
        (16, "0007", Some("Completed")),
        (18, "0009", Some("Completed")),
        (19, "0010", Some("Completed")),
        (20, "0011", Some("Completed")),
        (21, "0012", Some("Completed")),
        (22, "0013", Some("Completed")),
        (23, "0014", Some("Completed")),
        (24, "0015", Some("Completed")),
        (25, "0016", Some("Completed")),
        (26, "0017", Some("Completed")),
    ];
    let mut civ_rows = entries
        .into_iter()
        .map(|(physical_row, suffix, status)| {
            let mut cells = vec![inline(
                &format!("A{physical_row}"),
                &format!("AZE-SUN-INT-QMS-RFI-CIV-{suffix}"),
            )];
            cells.push(status.map_or_else(
                || blank(&format!("P{physical_row}")),
                |status| inline(&format!("P{physical_row}"), status),
            ));
            row(physical_row, cells)
        })
        .collect::<Vec<_>>();
    civ_rows.push(row(
        17,
        [
            inline("A17", "AZE-SUN-INT-QMS-RFI-CIV-0008"),
            formula("P17", "LOOKUP()", "Completed"),
        ],
    ));
    civ_rows.sort();
    source_package(
        worksheet(civ_rows, &["P18:Q18"]),
        worksheet(
            [
                row(
                    10,
                    [
                        inline("A10", "AZE-SUN-INT-QMS-RFI-CIV-0006"),
                        inline("Q10", "Completed"),
                    ],
                ),
                row(
                    11,
                    [
                        inline("A11", "AZE-SUN-INT-QMS-RFI-CIV-0007"),
                        inline("Q11", "Ongoing"),
                    ],
                ),
            ],
            &[],
        ),
    )
}

fn compatibility_target() -> Cursor<Vec<u8>> {
    let rfi = |suffix: &str| format!("AZE-SUN-INT-QMS-RFI-CIV-{suffix}");
    target_with_rows(
        [
            row(2, [inline("A2", &rfi("0001")), blank("B2")]),
            row(3, [inline("A3", &rfi("0002")), blank("B3")]),
            row(4, [inline("A4", &rfi("0003")), inline("B4", "Completed")]),
            row(5, [inline("A5", &rfi("0004")), blank("B5")]),
            row(6, [inline("A6", &rfi("0005")), blank("B6")]),
            row(7, [inline("A7", &rfi("0006")), blank("B7")]),
            row(8, [inline("A8", &rfi("0007")), blank("B8")]),
            row(9, [inline("A9", &rfi("0008")), blank("B9")]),
            row(10, [inline("A10", &rfi("0009")), blank("B10")]),
            row(11, [inline("A11", &rfi("9999")), blank("B11")]),
            row(
                12,
                [
                    inline("A12", "prefix AZE-SUN-INT-QMS-RFI-CIV-0001"),
                    blank("B12"),
                ],
            ),
            row(13, [formula("A13", "LOOKUP()", &rfi("0017")), blank("B13")]),
            row(14, [inline("A14", &rfi("0011")), blank("B14")]),
            row(15, [inline("A15", &rfi("0014")), blank("B15")]),
            row(16, [inline("A16", &rfi("0014")), blank("B16")]),
            row(17, [inline("A17", &rfi("0012"))]),
            row(
                18,
                [
                    inline("A18", &rfi("0013")),
                    formula("B18", "LOOKUP()", "Completed"),
                ],
            ),
            row(19, [inline("A19", &rfi("0016")), blank("B19")]),
            row(20, [inline("A20", &rfi("0010")), inline("B20", "Pending")]),
            row(21, [blank("A21"), inline("B21", "Completed")]),
            row(
                22,
                [inline("A22", &rfi("0015")), inline("B22", "In Progress")],
            ),
        ],
        &["A14:C14", "B19:C19"],
    )
}

fn oracle_projection(review: &Value) -> Value {
    let rows = review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            json!({
                "_rowId": row["_rowId"],
                "_sheet": row["_sheet"],
                "_row": row["_row"],
                "rfiNo": row["rfiNo"],
                "sourceStatuses": row["sourceStatuses"],
                "currentStatus": row["currentStatus"],
                "proposedStatus": row["proposedStatus"],
                "disposition": row["disposition"],
                "reasonKey": row["reasonKey"],
                "selectable": row["selectable"],
                "enabled": row["enabled"],
                "sourceProvenance": row["sourceEvidence"].as_array().unwrap().iter().map(|evidence| json!({
                    "sheet": evidence["sheet"],
                    "row": evidence["row"],
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "rows": rows,
        "warnings": review["warnings"],
    })
}

fn compatibility_plan(
    source: Cursor<Vec<u8>>,
    target: Cursor<Vec<u8>>,
) -> docflow_lib::contracts::Plan {
    let fixture = tempfile::tempdir().unwrap();
    let source_file = fixture.path().join("RFI Log.xlsx");
    let target_file = fixture.path().join("Follow Up.xlsx");
    std::fs::write(&source_file, source.into_inner()).unwrap();
    std::fs::write(&target_file, target.into_inner()).unwrap();
    let mut source = File::open(source_file).unwrap();
    let mut target = File::options()
        .read(true)
        .write(true)
        .open(target_file)
        .unwrap();
    preview_from_open_files(
        std::path::Path::new("RFI Log.xlsx"),
        std::path::Path::new("Follow Up.xlsx"),
        &mut source,
        &mut target,
    )
    .unwrap()
}

fn public_oracle_projection(plan: &docflow_lib::contracts::Plan) -> Value {
    let editor = &plan.meta["editor"];
    let rows = editor["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            json!({
                "_rowId": row["_rowId"],
                "_sheet": row["_sheet"],
                "_row": row["_row"],
                "rfiNo": row["rfiNo"],
                "sourceStatuses": row["sourceStatuses"],
                "currentStatus": row["currentStatus"],
                "proposedStatus": row["proposedStatus"],
                "disposition": row["disposition"],
                "reasonKey": row["reasonKey"],
                "selectable": row["selectable"],
                "enabled": row["enabled"],
                "sourceProvenance": row["sourceEvidence"].as_array().unwrap().iter().map(|evidence| json!({
                    "sheet": evidence["sheet"],
                    "row": evidence["row"],
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let updates = plan
        .steps
        .first()
        .and_then(|step| step.meta.get("updates"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|update| {
            json!({
                "rowId": update["rowId"],
                "canonicalRfi": update["canonicalRfi"],
                "proposedStatus": update["proposedStatus"],
                "target": {
                    "sheet": update["target"]["worksheet"]["name"],
                    "row": update["target"]["row"],
                    "rfiColumn": update["target"]["rfiColumn"],
                    "statusColumn": update["target"]["statusColumn"],
                },
            })
        })
        .collect::<Vec<_>>();
    json!({
        "opId": plan.op_id,
        "columns": editor["columns"],
        "rows": rows,
        "updates": updates,
        "warnings": plan.warnings,
    })
}

#[test]
fn preview_matches_the_frozen_follow_up_compatibility_oracle() {
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/rfi_follow_up_compatibility_oracle.json"
    ))
    .unwrap();
    let plan = compatibility_plan(compatibility_source(), compatibility_target());

    assert_eq!(
        public_oracle_projection(&plan),
        oracle["defaultReview"],
        "public Follow Up preview diverged from the frozen compatibility oracle"
    );

    let no_selection = compatibility_plan(
        compatibility_source(),
        target_with_rows(
            [row(
                2,
                [
                    inline("A2", "AZE-SUN-INT-QMS-RFI-CIV-0003"),
                    inline("B2", "Completed"),
                ],
            )],
            &[],
        ),
    );
    assert!(no_selection.steps.is_empty());
    assert_eq!(
        serde_json::to_value(&no_selection.warnings).unwrap(),
        oracle["noSelectionWarnings"]
    );
}

#[test]
fn compatibility_oracle_matches_exact_rows() {
    let actual =
        oracle_projection(&classify(compatibility_source(), compatibility_target()).unwrap());
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/rfi_follow_up_compatibility_oracle.json"
    ))
    .unwrap();
    assert_eq!(actual["rows"], oracle["defaultReview"]["rows"]);
    assert_eq!(actual["warnings"], oracle["defaultReview"]["warnings"]);

    let no_selection = classify(
        compatibility_source(),
        target_with_rows(
            [row(
                2,
                [
                    inline("A2", "AZE-SUN-INT-QMS-RFI-CIV-0003"),
                    inline("B2", "Completed"),
                ],
            )],
            &[],
        ),
    )
    .unwrap();
    assert_eq!(no_selection["warnings"], oracle["noSelectionWarnings"]);
    assert_eq!(no_selection["editor"]["requiredFields"], json!(["enabled"]));
}
