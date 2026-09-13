use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use docflow_lib::{
    fs_ops::FilesystemAuthority,
    ooxml::{read_member, read_workbook_from_reader},
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    rfi_system_sync::{self, preview_system_sync},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn package(sheets: &[(&str, String)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut workbook_sheets = String::new();
    let mut relationships = String::new();
    for (index, (name, worksheet)) in sheets.iter().enumerate() {
        let ordinal = index + 1;
        workbook_sheets.push_str(&format!(
            r#"<sheet name="{name}" sheetId="{ordinal}" r:id="rId{ordinal}"/>"#
        ));
        relationships.push_str(&format!(
            r#"<Relationship Id="rId{ordinal}" Type="{OFFICE_REL}/worksheet" Target="worksheets/sheet{ordinal}.xml"/>"#
        ));
        writer
            .start_file(format!("xl/worksheets/sheet{ordinal}.xml"), options)
            .unwrap();
        writer.write_all(worksheet.as_bytes()).unwrap();
    }
    relationships.push_str(&format!(
        r#"<Relationship Id="rIdCalc" Type="{OFFICE_REL}/calcChain" Target="calcChain.xml"/>"#
    ));
    let package_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL}"><Relationship Id="rIdWorkbook" Type="{OFFICE_REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="{OFFICE_REL}"><sheets>{workbook_sheets}</sheets></workbook>"#
    );
    let relationships =
        format!(r#"<Relationships xmlns="{PACKAGE_REL}">{relationships}</Relationships>"#);
    let calc_chain =
        format!(r#"<calcChain xmlns="{MAIN}"><c r="T10" i="1"/><c r="U10" i="2"/></calcChain>"#);
    for (name, bytes) in [
        ("[Content_Types].xml", b"content-types".as_slice()),
        ("_rels/.rels", package_relationships.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", relationships.as_bytes()),
        ("xl/calcChain.xml", calc_chain.as_bytes()),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn inline(reference: &str, value: &str) -> String {
    format!(r#"<c r="{reference}" t="inlineStr"><is><t>{value}</t></is></c>"#)
}

fn formula(reference: &str, attributes: &str, value: &str) -> String {
    format!(r#"<c r="{reference}"><f {attributes}>{value}</f><v/></c>"#)
}

fn row(number: u32, cells: impl IntoIterator<Item = String>) -> String {
    format!(
        r#"<row r="{number}">{}</row>"#,
        cells.into_iter().collect::<String>()
    )
}

fn worksheet(rows: impl IntoIterator<Item = String>) -> String {
    let rows = rows.into_iter().collect::<String>();
    format!(r#"<worksheet xmlns="{MAIN}"><sheetData>{rows}</sheetData></worksheet>"#)
}

fn write_pdf_with_signature(path: &Path, signed: bool) {
    let stream = if signed {
        "BT /F1 18 Tf 460 100 Td (E.Mammadov Signed Worley) Tj ET\n"
    } else {
        "BT /F1 18 Tf 100 700 Td (RFI Inspection Details) Tj ET\n"
    };
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    fs::write(path, bytes).unwrap();
}

#[test]
fn test_system_sync_full_preview_apply_and_undo() {
    let fixture = tempfile::tempdir().unwrap();
    let fixture_path = fs::canonicalize(fixture.path()).unwrap();
    let system_root = fixture_path.join("RFI_SYSTEM");
    let civ_dir = system_root.join("CIV");
    let mec_dir = system_root.join("MEC");
    fs::create_dir_all(&civ_dir).unwrap();
    fs::create_dir_all(&mec_dir).unwrap();

    // 1. CIV-0103 signed
    let civ_103_dir = civ_dir.join("AZE-SUN-INT-QMS-RFI-CIV-0103");
    fs::create_dir_all(&civ_103_dir).unwrap();
    let civ_103_pdf = civ_103_dir.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0103.pdf");
    write_pdf_with_signature(&civ_103_pdf, true);

    // 2. CIV-0104 unsigned
    let civ_104_dir = civ_dir.join("AZE-SUN-INT-QMS-RFI-CIV-0104");
    fs::create_dir_all(&civ_104_dir).unwrap();
    let civ_104_pdf = civ_104_dir.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0104.pdf");
    write_pdf_with_signature(&civ_104_pdf, false);

    // 3. MEC-0700 signed
    let mec_700_dir = mec_dir.join("AZE-SUN-INT-QMS-RFI-MEC-0700");
    fs::create_dir_all(&mec_700_dir).unwrap();
    let mec_700_pdf = mec_700_dir.join("RFI No. AZE-SUN-INT-QMS-RFI-MEC-0700.pdf");
    write_pdf_with_signature(&mec_700_pdf, true);

    // Mirror the production workbook schema: REMARKS is V/W, while T/U is a
    // shared-formula Folder Link column. The rc.8 regression wrote into T/U.
    let civ_sheet_content = worksheet([
        row(
            9,
            [
                inline("A9", "RFI No."),
                inline("T9", "Folder Link2"),
                inline("V9", "REMARKS"),
            ],
        ),
        row(
            10,
            [
                inline("A10", "AZE-SUN-INT-QMS-RFI-CIV-0103"),
                formula(
                    "T10",
                    r#"t="shared" si="1" ref="T10:T11""#,
                    "HYPERLINK(A10)",
                ),
                inline("V10", ""),
            ],
        ),
        row(
            11,
            [
                inline("A11", "AZE-SUN-INT-QMS-RFI-CIV-0104"),
                formula("T11", r#"t="shared" si="1""#, ""),
                inline("V11", ""),
            ],
        ),
    ]);
    let mec_sheet_content = worksheet([
        row(
            9,
            [
                inline("A9", "RFI No."),
                inline("U9", "Folder Link"),
                inline("W9", "REMARKS"),
            ],
        ),
        row(
            10,
            [
                inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0700"),
                formula(
                    "U10",
                    r#"t="shared" si="2" ref="U10:U11""#,
                    "HYPERLINK(A10)",
                ),
                inline("W10", "existing note"),
            ],
        ),
        row(
            11,
            [
                formula("U11", r#"t="shared" si="2""#, ""),
                inline("W11", ""),
            ],
        ),
    ]);
    let ele_sheet = worksheet([row(9, [inline("A9", "RFI No."), inline("V9", "REMARKS")])]);
    let prc_sheet = worksheet([row(9, [inline("A9", "RFI No."), inline("W9", "REMARKS")])]);
    let rfi_log_bytes = package(&[
        ("CIV", civ_sheet_content),
        ("MEC", mec_sheet_content),
        ("ELE", ele_sheet),
        ("PRC", prc_sheet),
    ]);
    let rfi_log_path = fixture_path.join("RFI Log.xlsx");
    fs::write(&rfi_log_path, &rfi_log_bytes).unwrap();
    let original_calc_chain = read_member(&rfi_log_path, "xl/calcChain.xml").unwrap();

    // Mirror the current Follow Up control tables, not the retired Final-ITP layout.
    let fu_civ_content = worksheet([
        row(4, [inline("C4", "RFI No."), inline("D4", "STATUS")]),
        row(5, [inline("C5", "CIV-0103"), inline("D5", "Not Signed")]),
        row(6, [inline("C6", "CIV-0104"), inline("D6", "Not Signed")]),
    ]);
    let fu_mec_content = worksheet([
        row(3, [inline("S3", "RFI No."), inline("T3", "Worley Status")]),
        row(4, [inline("S4", "MEC-0700"), inline("T4", "Not Signed")]),
    ]);
    let follow_up_bytes = package(&[
        ("Signed By Worley CIV", fu_civ_content),
        ("Signed By Worley MEC", fu_mec_content),
    ]);
    let follow_up_path = fixture_path.join("Follow Up.xlsx");
    fs::write(&follow_up_path, &follow_up_bytes).unwrap();

    let mut rfi_log_file = File::options()
        .read(true)
        .write(true)
        .open(&rfi_log_path)
        .unwrap();
    let mut follow_up_file = File::options()
        .read(true)
        .write(true)
        .open(&follow_up_path)
        .unwrap();

    let plan = preview_system_sync(
        &system_root,
        &rfi_log_path,
        &follow_up_path,
        &mut rfi_log_file,
        &mut follow_up_file,
    )
    .unwrap();

    assert_eq!(plan.op_id, rfi_system_sync::OPERATION_ID);
    assert_eq!(
        plan.steps.len(),
        2,
        "Expected 2 steps: RFI Log update and Follow Up update"
    );
    let mut revised_rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    revised_rows[2]["enabled"] = serde_json::Value::Bool(false);
    let revised = rfi_system_sync::revise(&plan, &revised_rows).unwrap();
    assert!(revised.steps.iter().all(|step| {
        step.meta["syncStep"]["updates"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|sheet| sheet.as_object().unwrap().values())
            .all(|update| update["rowId"] != "sync-row-3")
    }));

    // Any post-preview change must fail before either workbook or any backup is
    // touched.
    let stale_authority =
        FilesystemAuthority::new(fixture_path.join("stale-journal"), Duration::from_secs(300));
    let stale_caller = CallerIdentity::new("stale-owner", "stale-session");
    let stale_scope = GrantScope::new(
        &stale_caller.owner_id,
        &stale_caller.session_id,
        "stale-request",
        rfi_system_sync::OPERATION_ID,
    );
    let stale_grant = stale_authority
        .issue_grant(&stale_scope, &fixture_path, GrantAccess::ReadWrite)
        .unwrap();
    let stale_cached = stale_authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                stale_caller.owner_id.clone(),
                stale_caller.session_id.clone(),
                rfi_system_sync::OPERATION_ID,
                vec![stale_grant.grant_id],
            ),
            plan.clone(),
        )
        .unwrap();
    OpenOptions::new()
        .append(true)
        .open(&rfi_log_path)
        .unwrap()
        .write_all(b"stale")
        .unwrap();
    let stale_error = stale_authority
        .apply_with_progress(&stale_cached.plan_id, &stale_caller, |_| {})
        .unwrap_err();
    assert_eq!(stale_error.message, "error.rfi.systemSync.workbookChanged");
    for step in &plan.steps {
        assert!(!Path::new(step.dst.as_deref().unwrap()).exists());
    }
    fs::write(&rfi_log_path, &rfi_log_bytes).unwrap();

    let mut rfi_log_file = File::options()
        .read(true)
        .write(true)
        .open(&rfi_log_path)
        .unwrap();
    let mut follow_up_file = File::options()
        .read(true)
        .write(true)
        .open(&follow_up_path)
        .unwrap();
    let plan = preview_system_sync(
        &system_root,
        &rfi_log_path,
        &follow_up_path,
        &mut rfi_log_file,
        &mut follow_up_file,
    )
    .unwrap();

    // Setup FilesystemAuthority for Apply & Undo
    let journal_root = fixture_path.join("journal");
    let authority = FilesystemAuthority::new(&journal_root, Duration::from_secs(300));
    let caller = CallerIdentity::new("test-owner", "test-session");

    let scope = GrantScope::new(
        &caller.owner_id,
        &caller.session_id,
        "test-req-1",
        rfi_system_sync::OPERATION_ID,
    );
    let grant = authority
        .issue_grant(&scope, &fixture_path, GrantAccess::ReadWrite)
        .unwrap();

    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                caller.owner_id.clone(),
                caller.session_id.clone(),
                rfi_system_sync::OPERATION_ID,
                vec![grant.grant_id],
            ),
            plan,
        )
        .unwrap();

    // APPLY
    let result = authority
        .apply_with_progress(&cached.plan_id, &caller, |_| {})
        .unwrap();
    assert_eq!(result.applied, 2);
    assert_eq!(result.failed, 0);

    // Verify only the discovered REMARKS cells changed. Folder Link shared
    // formulas and calcChain must remain intact.
    let mut rfi_log_read = File::open(&rfi_log_path).unwrap();
    let updated_rfi_log = read_workbook_from_reader(&mut rfi_log_read).unwrap();
    let civ_sheet = updated_rfi_log.sheet("CIV").unwrap();
    assert_eq!(civ_sheet.value(10, 22), "Signed by Worley");
    assert_eq!(
        civ_sheet.value(11, 22),
        "",
        "Unsigned RFI should remain unchanged"
    );
    assert!(civ_sheet.cell(10, 20).unwrap().formula.is_some());
    assert!(civ_sheet.cell(11, 20).unwrap().formula.is_some());

    let mec_sheet = updated_rfi_log.sheet("MEC").unwrap();
    assert_eq!(mec_sheet.value(10, 23), "existing note\nSigned by Worley");
    assert!(mec_sheet.cell(10, 21).unwrap().formula.is_some());
    assert!(mec_sheet.cell(11, 21).unwrap().formula.is_some());
    assert_eq!(
        read_member(&rfi_log_path, "xl/calcChain.xml").unwrap(),
        original_calc_chain
    );

    // Verify Follow Up was updated
    let mut follow_up_read = File::open(&follow_up_path).unwrap();
    let updated_fu = read_workbook_from_reader(&mut follow_up_read).unwrap();
    let fu_civ = updated_fu.sheet("Signed By Worley CIV").unwrap();
    assert_eq!(fu_civ.value(5, 4), "Signed");
    assert_eq!(fu_civ.value(6, 4), "Not Signed");

    let fu_mec = updated_fu.sheet("Signed By Worley MEC").unwrap();
    assert_eq!(fu_mec.value(4, 20), "Signed");

    // UNDO
    let undo_result = authority.undo(&result.journal_id, &caller, None).unwrap();
    assert_eq!(undo_result.applied, 4);
    assert_eq!(undo_result.failed, 0);

    // Verify restored workbooks match original bytes
    let restored_rfi_log = fs::read(&rfi_log_path).unwrap();
    let restored_fu = fs::read(&follow_up_path).unwrap();

    let mut restored_log_read = Cursor::new(&restored_rfi_log);
    let check_log = read_workbook_from_reader(&mut restored_log_read).unwrap();
    assert_eq!(check_log.sheet("CIV").unwrap().value(10, 22), "");
    assert_eq!(
        check_log.sheet("MEC").unwrap().value(10, 23),
        "existing note"
    );

    let mut restored_fu_read = Cursor::new(&restored_fu);
    let check_fu = read_workbook_from_reader(&mut restored_fu_read).unwrap();
    assert_eq!(
        check_fu.sheet("Signed By Worley CIV").unwrap().value(5, 4),
        "Not Signed"
    );
    assert_eq!(
        check_fu.sheet("Signed By Worley MEC").unwrap().value(4, 20),
        "Not Signed"
    );
}

#[test]
#[ignore = "requires operator-supplied current production workbook copies"]
fn current_workbook_templates_accept_only_closed_guarded_deltas() {
    let rfi_log_source = PathBuf::from(
        std::env::var("DOCFLOW_CURRENT_RFI_LOG").expect("DOCFLOW_CURRENT_RFI_LOG is required"),
    );
    let follow_up_source = PathBuf::from(
        std::env::var("DOCFLOW_CURRENT_FOLLOW_UP").expect("DOCFLOW_CURRENT_FOLLOW_UP is required"),
    );
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("RFI_SYSTEM");
    for rfi in ["MEC-0001", "MEC-0361"] {
        let folder = root.join("MEC").join(format!("AZE-SUN-INT-QMS-RFI-{rfi}"));
        fs::create_dir_all(&folder).unwrap();
        write_pdf_with_signature(
            &folder.join(format!("RFI No. AZE-SUN-INT-QMS-RFI-{rfi}.pdf")),
            true,
        );
    }
    let rfi_log = fixture.path().join("RFI Log.xlsm");
    let follow_up = fixture.path().join("Inspection Status Follow Up.xlsm");
    fs::copy(rfi_log_source, &rfi_log).unwrap();
    fs::copy(follow_up_source, &follow_up).unwrap();
    let mut rfi_log_file = File::options()
        .read(true)
        .write(true)
        .open(&rfi_log)
        .unwrap();
    let mut follow_up_file = File::options()
        .read(true)
        .write(true)
        .open(&follow_up)
        .unwrap();
    let plan = preview_system_sync(
        &root,
        &rfi_log,
        &follow_up,
        &mut rfi_log_file,
        &mut follow_up_file,
    )
    .unwrap();
    assert!(!plan.steps.is_empty());
    for (index, step) in plan.steps.iter().enumerate() {
        let output = fixture.path().join(format!("guarded-output-{index}.xlsm"));
        rfi_system_sync::execute_sync_step(step, Path::new(step.src.as_deref().unwrap()), &output)
            .unwrap();
    }
}
