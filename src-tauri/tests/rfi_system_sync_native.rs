use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::Path,
    time::Duration,
};

use docflow_lib::{
    fs_ops::FilesystemAuthority,
    ooxml::read_workbook_from_reader,
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
    writer.finish().unwrap().into_inner()
}

fn inline(reference: &str, value: &str) -> String {
    format!(r#"<c r="{reference}" t="inlineStr"><is><t>{value}</t></is></c>"#)
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

    // Setup RFI Log
    let civ_sheet_content = worksheet([
        row(
            10,
            [
                inline("A10", "AZE-SUN-INT-QMS-RFI-CIV-0103"),
                inline("B10", "ITP-0001"),
                inline("C10", "2.3.1"),
                inline("F10", "AREA-42"),
                inline("G10", "CIV-AREA"),
                inline("N10", "LOC-CIV"),
                inline("T10", ""), // Remarks col 20
            ],
        ),
        row(
            11,
            [
                inline("A11", "AZE-SUN-INT-QMS-RFI-CIV-0104"),
                inline("B11", "ITP-0001"),
                inline("C11", "2.3.1"),
                inline("F11", "AREA-43"),
                inline("G11", "CIV-AREA"),
                inline("N11", "LOC-CIV"),
                inline("T11", ""),
            ],
        ),
    ]);
    let mec_sheet_content = worksheet([row(
        10,
        [
            inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0700"),
            inline("B10", "ITP-0002"),
            inline("C10", "1.3.5"),
            inline("F10", "PCS-10"),
            inline("G10", "SEC-A"),
            inline("O10", "LOC-1"), // Location col 15 for MEC
            inline("U10", ""),      // Remarks col 21 for MEC
        ],
    )]);
    let empty_sheet = worksheet(Vec::<String>::new());
    let rfi_log_bytes = package(&[
        ("CIV", civ_sheet_content),
        ("MEC", mec_sheet_content),
        ("ELE", empty_sheet.clone()),
        ("PRC", empty_sheet.clone()),
    ]);
    let rfi_log_path = fixture_path.join("RFI Log.xlsx");
    fs::write(&rfi_log_path, &rfi_log_bytes).unwrap();

    // Setup Follow Up
    let fu_civ_content = worksheet([
        row(1, [inline("A1", "Area"), inline("B1", "3.1")]),
        row(2, [inline("A2", "AREA-42"), inline("B2", "")]),
        row(3, [inline("A3", "AREA-43"), inline("B3", "")]),
    ]);
    let fu_mec_content = worksheet([
        row(
            1,
            [
                inline("A1", "PCS number"),
                inline("B1", "Section"),
                inline("C1", "Area"),
                inline("D1", "3.5"),
                inline("E1", "3.1"),
                inline("F1", "3.2"),
            ],
        ),
        row(
            2,
            [
                inline("A2", "PCS-10"),
                inline("B2", "SEC-A"),
                inline("C2", "LOC-1"),
                inline("D2", ""),
                inline("E2", ""),
                inline("F2", ""),
            ],
        ),
    ]);
    let follow_up_bytes = package(&[
        ("Signed by CIV", fu_civ_content),
        ("Signed by MEC", fu_mec_content),
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

    // Verify RFI Log was updated
    let mut rfi_log_read = File::open(&rfi_log_path).unwrap();
    let updated_rfi_log = read_workbook_from_reader(&mut rfi_log_read).unwrap();
    let civ_sheet = updated_rfi_log.sheet("CIV").unwrap();
    assert_eq!(civ_sheet.value(10, 20), "Signed by Worley");
    assert_eq!(
        civ_sheet.value(11, 20),
        "",
        "Unsigned RFI should remain unchanged"
    );

    let mec_sheet = updated_rfi_log.sheet("MEC").unwrap();
    assert_eq!(mec_sheet.value(10, 21), "Signed by Worley");

    // Verify Follow Up was updated
    let mut follow_up_read = File::open(&follow_up_path).unwrap();
    let updated_fu = read_workbook_from_reader(&mut follow_up_read).unwrap();
    let fu_civ = updated_fu.sheet("Signed by CIV").unwrap();
    assert_eq!(fu_civ.value(2, 2), "CIV-0103");
    assert_eq!(fu_civ.value(3, 2), "", "Unsigned RFI should not be added");

    let fu_mec = updated_fu.sheet("Signed by MEC").unwrap();
    assert_eq!(
        fu_mec.value(2, 4),
        "MEC-0700",
        "MEC-0700 should be placed in col 3.5 (col 4)"
    );

    // UNDO
    let undo_result = authority.undo(&result.journal_id, &caller, None).unwrap();
    assert_eq!(undo_result.applied, 4);
    assert_eq!(undo_result.failed, 0);

    // Verify restored workbooks match original bytes
    let restored_rfi_log = fs::read(&rfi_log_path).unwrap();
    let restored_fu = fs::read(&follow_up_path).unwrap();

    let mut restored_log_read = Cursor::new(&restored_rfi_log);
    let check_log = read_workbook_from_reader(&mut restored_log_read).unwrap();
    assert_eq!(check_log.sheet("CIV").unwrap().value(10, 20), "");
    assert_eq!(check_log.sheet("MEC").unwrap().value(10, 21), "");

    let mut restored_fu_read = Cursor::new(&restored_fu);
    let check_fu = read_workbook_from_reader(&mut restored_fu_read).unwrap();
    assert_eq!(check_fu.sheet("Signed by CIV").unwrap().value(2, 2), "");
    assert_eq!(check_fu.sheet("Signed by MEC").unwrap().value(2, 4), "");
}
