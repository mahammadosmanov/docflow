use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::NaiveDate;
use docflow_lib::{
    fs_ops::{FilesystemAuthority, file_sha256},
    journal::JournalState,
    ooxml::read_workbook,
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    rfi_log::{copy_payload, preview, preview_table, revise},
};
use serde_json::{Map, Value, json};
use tempfile::TempDir;
use zip::{CompressionMethod, DateTime, System, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

fn inline_cell(reference: &str, value: &str, style: u32) -> String {
    format!(r#"<c r="{reference}" s="{style}" t="inlineStr"><is><t>{value}</t></is></c>"#)
}

fn number_cell(reference: &str, value: &str, style: u32) -> String {
    format!(r#"<c r="{reference}" s="{style}"><v>{value}</v></c>"#)
}

fn workbook_fixture(path: &Path, macros: bool) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    writer.set_comment("RFI corpus fixture").unwrap();
    let options = SimpleFileOptions::default()
        .system(System::Unix)
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::from_date_and_time(2025, 7, 6, 5, 4, 2).unwrap())
        .unix_permissions(0o640);
    let mut add = |name: &str, bytes: &[u8]| {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    };
    add("[Content_Types].xml", b"content-types-sentinel");
    add(
        "_rels/.rels",
        br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
    );
    add(
        "xl/workbook.xml",
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="CIV" sheetId="1" r:id="rId1"/><sheet name="MEC" sheetId="2" r:id="rId2"/><sheet name="Sheet1" sheetId="3" r:id="rId3"/><sheet name="Unrelated" sheetId="4" r:id="rId4"/></sheets><calcPr calcId="191029" calcMode="auto" fullCalcOnLoad="1"/></workbook>"#).as_bytes(),
    );
    add(
        "xl/_rels/workbook.xml.rels",
        b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet2.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet3.xml\"/><Relationship Id=\"rId4\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet4.xml\"/></Relationships>",
    );
    let civ_headers = [
        "RFI No.",
        "ITP No:",
        "Item No.",
        "INSPECTION DESCRIPTION",
        "FORMS",
        "TAG NO",
        "AREA",
        "DATE OF ISSUE",
        "INSPECTION DATE",
        "EXPIRY DATE",
        "INSPECTION START TIME",
        "INSP. TYPE AZFEN",
        "INSP. TYPE INTEC",
        "Inspection Location",
        "SUBCONTRACTOR",
        "Inspection Status",
        "RFI VALIDITY",
    ];
    let civ_header = civ_headers
        .iter()
        .enumerate()
        .map(|(index, value)| inline_cell(&format!("{}9", column_name(index + 1)), value, 1))
        .collect::<String>();
    let civ_row = [
        inline_cell("A10", "AZE-SUN-INT-QMS-RFI-CIV-0007", 4),
        inline_cell("B10", "AZE˗SUN˗INT˗QMS˗ITP˗CIV˗0005", 5),
        inline_cell("C10", "CIV-5.2.1", 6),
        inline_cell("D10", "Civil description", 9),
        inline_cell("E10", "CON-CIV-001", 10),
        inline_cell("F10", "N12", 7),
        inline_cell("G10", "North Site", 7),
        number_cell("H10", "46258", 2),
        number_cell("I10", "46260", 2),
        number_cell("K10", "0.729166666666667", 3),
        inline_cell("L10", "H", 8),
        inline_cell("M10", "W", 8),
        inline_cell("N10", "N3", 7),
        inline_cell("O10", "AZFEN", 7),
    ]
    .join("");
    add(
        "xl/worksheets/sheet1.xml",
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{MAIN}" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:xr2="http://schemas.microsoft.com/office/spreadsheetml/2015/revision2" mc:Ignorable="xr2"><dimension ref="A1:Q30"/><cols><col min="1" max="1" width="25" customWidth="1"/><col min="4" max="4" width="31" customWidth="1"/></cols><sheetData><row r="9">{civ_header}</row><row r="10">{civ_row}</row><row r="11"><c r="D11" s="9" t="str"><f>IFERROR(VLOOKUP(C11,Sheet1!K:L,2,0),\"\")</f><v/></c><c r="E11" s="10"/><c r="J11" s="2" t="inlineStr"><is><t>KEEP EXPIRY</t></is></c><c r="P11" t="inlineStr"><is><t>KEEP STATUS</t></is></c><c r="Q11" t="inlineStr"><is><t>KEEP VALIDITY</t></is></c></row><row r="30"><c r="Q30"/></row></sheetData><dataValidations count="2"><dataValidation type="custom" sqref="A10:A30"><formula1>COUNTIF($A$10:$A$30,A10)&lt;=1</formula1></dataValidation><dataValidation type="list" sqref="C10:C30"><formula1>INDIRECT(B10)</formula1></dataValidation></dataValidations></worksheet>"#).as_bytes(),
    );
    let mec_headers = [
        "RFI No.",
        "ITP No:",
        "Item No.",
        "INSPECTION DESCRIPTION",
        "FORMS",
        "TAG NO",
        "AREA",
        "ROW No.",
        "DATE OF ISSUE",
        "INSPECTION DATE",
        "EXPIRY DATE",
        "INSPECTION START TIME",
        "INSP. TYPE AZFEN",
        "INSP. TYPE INTEC",
        "Inspection Location",
        "SUBCONTRACTOR",
        "Inspection Status",
        "RFI VALIDITY",
    ];
    let mec_header = mec_headers
        .iter()
        .enumerate()
        .map(|(index, value)| inline_cell(&format!("{}9", column_name(index + 1)), value, 1))
        .collect::<String>();
    add(
        "xl/worksheets/sheet2.xml",
        format!(r#"<worksheet xmlns="{MAIN}"><dimension ref="A1:R30"/><sheetData><row r="9">{mec_header}</row><row r="10">{}{}</row><row r="30"><c r="R30"/></row></sheetData></worksheet>"#, inline_cell("A10", "AZE-SUN-INT-QMS-RFI-MEC-0020", 4), inline_cell("C10", "MEC-3.2.1", 6)).as_bytes(),
    );
    let lookup_header = [
        inline_cell("B1", "AZE˗SUN˗INT˗QMS˗ITP˗CIV˗0005", 1),
        inline_cell("C1", "AZE˗SUN˗INT˗QMS˗ITP˗MEC˗0003", 1),
        inline_cell("K1", "Item", 1),
        inline_cell("L1", "Description", 1),
        inline_cell("M1", "Forms", 1),
        inline_cell("N1", "Item", 1),
        inline_cell("O1", "AZFEN", 1),
        inline_cell("P1", "Item", 1),
        inline_cell("Q1", "INTEC", 1),
    ]
    .join("");
    let lookup_civ = [
        inline_cell("K2", "CIV-5.2.1", 1),
        inline_cell("L2", "Civil description", 1),
        inline_cell("M2", "CON-CIV-001", 1),
        inline_cell("N2", "CIV-5.2.1", 1),
        inline_cell("O2", "H", 1),
        inline_cell("P2", "CIV-5.2.1", 1),
        inline_cell("Q2", "W", 1),
    ]
    .join("");
    let lookup_mec = [
        inline_cell("K3", "MEC-3.2.1", 1),
        inline_cell("L3", "Mechanical description", 1),
        inline_cell("M3", "CON-MEC-002", 1),
        inline_cell("N3", "MEC-3.2.1", 1),
        inline_cell("O3", "H", 1),
        inline_cell("P3", "MEC-3.2.1", 1),
        inline_cell("Q3", "S", 1),
    ]
    .join("");
    add(
        "xl/worksheets/sheet3.xml",
        format!(r#"<worksheet xmlns="{MAIN}"><dimension ref="A1:Q3"/><sheetData><row r="1">{lookup_header}</row><row r="2">{lookup_civ}</row><row r="3">{lookup_mec}</row></sheetData></worksheet>"#).as_bytes(),
    );
    add(
        "xl/worksheets/sheet4.xml",
        format!(
            r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1">{}</row></sheetData></worksheet>"#,
            inline_cell("A1", "UNRELATED", 1)
        )
        .as_bytes(),
    );
    add(
        "xl/calcChain.xml",
        format!(r#"<calcChain xmlns="{MAIN}"><c r="D11" i="1"/><c r="D12"/><c r="A1" i="4"/></calcChain>"#).as_bytes(),
    );
    for (name, bytes) in [
        ("xl/styles.xml", b"styles-sentinel".as_slice()),
        ("xl/theme/theme1.xml", b"theme-sentinel".as_slice()),
        (
            "xl/worksheets/_rels/sheet1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdDraw" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
                .as_slice(),
        ),
        ("xl/drawings/drawing1.xml", b"drawing-sentinel".as_slice()),
        ("xl/comments1.xml", b"comments-sentinel".as_slice()),
        (
            "xl/externalLinks/externalLink1.xml",
            b"external-link-sentinel".as_slice(),
        ),
        ("docProps/core.xml", b"metadata-sentinel".as_slice()),
    ] {
        add(name, bytes);
    }
    if macros {
        add("xl/vbaProject.bin", b"VBA\0RFI\xffSENTINEL");
    }
    writer.finish().unwrap();
}

fn column_name(mut number: usize) -> String {
    let mut output = String::new();
    while number > 0 {
        let remainder = (number - 1) % 26;
        output.insert(0, char::from(b'A' + remainder as u8));
        number = (number - 1) / 26;
    }
    output
}

fn values(path: &Path) -> Map<String, Value> {
    json!({
        "workbook": path,
        "email_text": "Create 1 CIV RFI number\nITP No: 5\nItem: CIV-5.2.1\nTag No: N12, S14\nInspection Location: Foundation A",
        "email_provider": "offline",
        "gemini_api_key": "",
        "sheet_counts": {"CIV": 1, "MEC": 0, "ELE": 0, "PRC": 0},
        "overflow_next_day": true
    })
    .as_object()
    .unwrap()
    .clone()
}

fn caller() -> CallerIdentity {
    CallerIdentity::new("owner", "session")
}

fn cache(
    authority: &FilesystemAuthority,
    root: &Path,
    plan: docflow_lib::contracts::Plan,
) -> docflow_lib::contracts::Plan {
    let scope = GrantScope::new("owner", "session", "request", "rfi.log_add");
    let grant = authority
        .issue_grant(&scope, root, GrantAccess::ReadWrite)
        .unwrap();
    authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new("owner", "session", "rfi.log_add", vec![grant.grant_id]),
            plan,
        )
        .unwrap()
}

fn members(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut entry = archive.by_index(index).unwrap();
            let name = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}

#[test]
fn native_preview_revise_and_table_payload_match_the_frozen_compatibility_oracle() {
    let fixture = TempDir::new().unwrap();
    let workbook = fixture.path().join("RFI Log.xlsm");
    workbook_fixture(&workbook, true);
    let today = NaiveDate::from_ymd_opt(2026, 8, 25).unwrap();
    let mut progress = Vec::new();
    let mut params = values(&workbook);
    params.insert("gemini_api_key".to_string(), json!("super-secret-token"));
    let plan = preview(&params, today, None, |event| progress.push(event), || false)
        .expect("native preview");

    assert_eq!(plan.op_id, "rfi.log_add");
    assert!(
        !serde_json::to_string(&plan)
            .unwrap()
            .contains("super-secret-token")
    );
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(
        plan.steps[0].kind,
        docflow_lib::contracts::StepKind::WorkbookUpdate
    );
    assert!(plan.steps[0].dst.as_deref().unwrap().ends_with(".xlsm"));
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["rfiNo"], "AZE-SUN-INT-QMS-RFI-CIV-0008");
    assert_eq!(rows[0]["rfiNo"], rows[1]["rfiNo"]);
    assert_eq!(rows[0]["inspectionDate"], "27-Aug-26");
    assert_eq!(rows[0]["startTime"], "09:00");
    assert_eq!(rows[0]["area"], "North Site");
    assert_eq!(rows[1]["area"], "South Site");
    assert_eq!(progress.first().unwrap().done, 0);
    assert_eq!(progress.last().unwrap().done, 4);

    let batch = &plan.steps[0].meta["batches"][0];
    let copy_source = &plan.meta["editor"]["copySource"];
    let actual = json!({
        "opId": plan.op_id,
        "warnings": plan.warnings,
        "rows": plan.meta["editor"]["rows"],
        "copyFields": plan.meta["editor"]["copyFields"],
        "copyHeaders": plan.meta["editor"]["copyHeaders"],
        "copyColumns": copy_source["columns"],
        "batch": {
            "discipline": batch["discipline"],
            "startRow": batch["startRow"],
            "baseRfis": batch["baseRfis"],
            "lookupColumns": batch["lookupColumns"],
        },
    });
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/slice6_rfi_compatibility_oracle.json"
    ))
    .unwrap();
    assert_eq!(
        actual, oracle,
        "Rust preview diverged from the frozen compatibility oracle"
    );

    let before = preview_table(&plan, Some("CIV")).unwrap();
    assert_eq!(before["count"], 2);
    assert_eq!(before["fromWorkbook"], false);
    assert_eq!(before["columnWidths"][0], 22.0);
    let payload = copy_payload(&plan, Some("CIV")).unwrap();
    assert_eq!(payload.count, 2);
    assert!(payload.plain.contains("\t"));
    assert!(!payload.plain.contains("KEEP EXPIRY"));
    assert!(!payload.html.contains("KEEP STATUS"));

    let mut edited = rows.clone();
    edited[1]["location"] = json!("South foundation");
    edited[1]["_sheet"] = json!("civ");
    edited[1]["itpNo"] = json!("legacy-separator-damage");
    edited[1].as_object_mut().unwrap().remove("rowNo");
    let revised = revise(&plan, &json!({"rows": edited})).unwrap();
    assert_eq!(
        revised.meta["editor"]["rows"][1]["location"],
        "South foundation"
    );
    assert_eq!(
        revised.meta["editor"]["rows"][1]["itpNo"],
        "AZE˗SUN˗INT˗QMS˗ITP˗CIV˗0005"
    );
    let mut invalid = revised.meta["editor"]["rows"].as_array().unwrap().clone();
    invalid[0]["itemNo"] = json!("CIV-DOES-NOT-EXIST");
    assert!(revise(&plan, &json!({"rows": invalid})).is_err());
}

#[test]
fn missing_workbook_error_returns_the_localized_message_key() {
    let error = preview(
        &Map::new(),
        NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(),
        None,
        |_| {},
        || false,
    )
    .unwrap_err();

    assert_eq!(error.message, "error.rfi.workbookInvalid");
}

#[test]
fn native_preview_preserves_multi_sheet_overflow_and_cancellation_contracts() {
    let fixture = TempDir::new().unwrap();
    let workbook = fixture.path().join("RFI Log.xlsx");
    workbook_fixture(&workbook, false);
    let today = NaiveDate::from_ymd_opt(2026, 8, 25).unwrap();

    let mut multi = values(&workbook);
    multi.insert("email_text".to_string(), json!(""));
    multi.insert("sheet_counts".to_string(), json!({"CIV": 1, "MEC": 2}));
    let plan = preview(&multi, today, None, |_| {}, || false).unwrap();
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.iter().filter(|row| row["_sheet"] == "CIV").count(), 1);
    assert_eq!(rows.iter().filter(|row| row["_sheet"] == "MEC").count(), 2);
    assert!(preview_table(&plan, None).is_err());
    assert_eq!(preview_table(&plan, Some("MEC")).unwrap()["count"], 2);

    let mut overflow = values(&workbook);
    overflow.insert("email_text".to_string(), json!(""));
    overflow.insert("sheet_counts".to_string(), json!({"CIV": 1}));
    overflow.insert("overflow_next_day".to_string(), json!(false));
    let plan = preview(&overflow, today, None, |_| {}, || false).unwrap();
    assert_eq!(
        plan.meta["editor"]["rows"][0]["inspectionDate"],
        "26-Aug-26"
    );
    assert_eq!(plan.meta["editor"]["rows"][0]["startTime"], "18:00");
    assert!(
        plan.warnings
            .iter()
            .any(|warning| { warning.key == "warn.rfi.overflowKept" && warning.subject == "1" })
    );

    let cancelled = preview(&multi, today, None, |_| {}, || true).unwrap_err();
    assert_eq!(cancelled.code, "operation_cancelled");

    let mut malformed_count = multi.clone();
    malformed_count.insert("sheet_counts".to_string(), json!({"CIV": "many"}));
    assert_eq!(
        preview(&malformed_count, today, None, |_| {}, || false)
            .unwrap_err()
            .message,
        "CIV RFI adedi gecersiz"
    );
    let mut legacy = multi;
    legacy.remove("sheet_counts");
    legacy.insert("discipline".to_string(), json!("CIV"));
    legacy.insert("count".to_string(), json!(1.9));
    assert_eq!(
        preview(&legacy, today, None, |_| {}, || false)
            .unwrap()
            .meta["editor"]["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn approved_xlsm_apply_preserves_package_and_undo_restores_exact_hash() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    fs::create_dir_all(&root).unwrap();
    let workbook = root.join("RFI Log.xlsm");
    workbook_fixture(&workbook, true);
    let original_hash = file_sha256(&workbook).unwrap();
    assert_eq!(
        original_hash,
        "324c5c2e673184b1a4bcaac43ba5d52c1eb2ebc200391b31c5738e7fb6699dcd"
    );
    let original_members = members(&workbook);
    let plan = preview(
        &values(&workbook),
        NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(),
        None,
        |_| {},
        || false,
    )
    .unwrap();
    let backup = Path::new(plan.steps[0].dst.as_deref().unwrap()).to_path_buf();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache(&authority, &root, plan);
    let result = authority.apply(&cached.plan_id, &caller()).unwrap();

    assert_eq!(
        (result.applied, result.failed),
        (2, 0),
        "{:?}",
        result.errors
    );
    assert_eq!(result.meta["effectState"], "committed");
    assert_eq!(file_sha256(&backup).unwrap(), original_hash);
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.state, Some(JournalState::Committed));
    assert!(
        journal
            .steps
            .iter()
            .any(|step| step.kind == docflow_lib::contracts::StepKind::RestoreBackup)
    );
    let after_members = members(&workbook);
    let changed = BTreeSet::from([
        "xl/worksheets/sheet1.xml".to_string(),
        "xl/calcChain.xml".to_string(),
    ]);
    for (name, bytes) in &original_members {
        if !changed.contains(name) {
            assert_eq!(&after_members[name], bytes, "{name}");
        }
    }
    assert_eq!(after_members["xl/vbaProject.bin"], b"VBA\0RFI\xffSENTINEL");
    let civ_xml = String::from_utf8(after_members["xl/worksheets/sheet1.xml"].clone()).unwrap();
    assert!(civ_xml.contains("IFERROR(VLOOKUP(C11,Sheet1!K:L,2,0),&quot;&quot;)"));
    let workbook_data = read_workbook(&workbook).unwrap();
    let civ = workbook_data.sheet("CIV").unwrap();
    assert_eq!(civ.cell(11, 1).unwrap().style.as_deref(), Some("4"));
    assert_eq!(civ.cell(11, 2).unwrap().style.as_deref(), Some("5"));
    assert_eq!(civ.cell(11, 3).unwrap().style.as_deref(), Some("6"));
    assert_eq!(civ.cell(11, 4).unwrap().style.as_deref(), Some("9"));
    assert_eq!(
        civ.cell(11, 4).unwrap().formula.as_deref(),
        Some("IFERROR(VLOOKUP(C11,Sheet1!K:L,2,0),\"\")")
    );
    assert_eq!(civ.value(11, 10), "KEEP EXPIRY");
    assert_eq!(civ.value(11, 16), "KEEP STATUS");
    assert_eq!(civ.value(11, 17), "KEEP VALIDITY");
    let table = preview_table(&cached, Some("CIV")).unwrap();
    assert_eq!(table["fromWorkbook"], true);
    assert_eq!(table["count"], 2);
    assert_eq!(table["columnWidths"][0], 25.0);

    let undone = authority.undo(&result.journal_id, &caller(), None).unwrap();
    assert_eq!(undone.failed, 0, "{:?}", undone.errors);
    assert_eq!(file_sha256(&workbook).unwrap(), original_hash);
}

#[test]
fn collisions_cancellation_and_later_edits_fail_without_silent_overwrite() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("documents");
    fs::create_dir_all(&root).unwrap();
    let workbook = root.join("RFI Log.xlsx");
    workbook_fixture(&workbook, false);
    let original_hash = file_sha256(&workbook).unwrap();
    assert_eq!(
        original_hash,
        "3a1a2c22d39b632528a63525507cb1f9b89a6b3f1bad6a265aac87f77e030ef5"
    );
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));

    let collision_plan = preview(
        &values(&workbook),
        NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(),
        None,
        |_| {},
        || false,
    )
    .unwrap();
    let collision_backup = PathBuf::from(collision_plan.steps[0].dst.as_deref().unwrap());
    fs::create_dir_all(collision_backup.parent().unwrap()).unwrap();
    fs::write(&collision_backup, b"KEEP").unwrap();
    let collision = cache(&authority, &root, collision_plan);
    let collision_result = authority.apply(&collision.plan_id, &caller()).unwrap();
    assert_eq!((collision_result.applied, collision_result.failed), (0, 1));
    assert_eq!(file_sha256(&workbook).unwrap(), original_hash);
    assert_eq!(fs::read(&collision_backup).unwrap(), b"KEEP");

    let cancelled_plan = preview(
        &values(&workbook),
        NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(),
        None,
        |_| {},
        || false,
    )
    .unwrap();
    let cancelled_backup = Path::new(cancelled_plan.steps[0].dst.as_deref().unwrap()).to_path_buf();
    let cancelled = cache(&authority, &root, cancelled_plan);
    let cancelled_id = cancelled.plan_id.clone();
    let cancelled_result = authority
        .apply_with_progress(&cancelled_id, &caller(), |event| {
            if event.done == 0 {
                assert!(authority.cancel(&cancelled_id));
            }
        })
        .unwrap();
    assert_eq!(cancelled_result.meta["effectState"], "cancelled_clean");
    assert_eq!(file_sha256(&workbook).unwrap(), original_hash);
    assert!(!cancelled_backup.exists());

    let applied_plan = preview(
        &values(&workbook),
        NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(),
        None,
        |_| {},
        || false,
    )
    .unwrap();
    let applied_backup = PathBuf::from(applied_plan.steps[0].dst.as_deref().unwrap());
    let applied = cache(&authority, &root, applied_plan);
    let applied_result = authority.apply(&applied.plan_id, &caller()).unwrap();
    assert_eq!(applied_result.failed, 0, "{:?}", applied_result.errors);
    let mut changed = fs::read(&workbook).unwrap();
    changed.extend_from_slice(b"later-user-edit");
    fs::write(&workbook, changed).unwrap();
    let changed_hash = file_sha256(&workbook).unwrap();
    let refused = authority
        .undo(&applied_result.journal_id, &caller(), None)
        .unwrap();
    assert!(refused.failed > 0);
    assert_eq!(file_sha256(&workbook).unwrap(), changed_hash);
    assert_eq!(file_sha256(&applied_backup).unwrap(), original_hash);
}
