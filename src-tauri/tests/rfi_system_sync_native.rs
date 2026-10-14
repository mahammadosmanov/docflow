#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::fs::OpenOptions;
use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use docflow_lib::{
    fs_ops::FilesystemAuthority,
    ooxml::read_workbook_from_reader,
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    rfi_system_sync::{self, preview_complete_status_sync, preview_system_sync},
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use docflow_lib::{
    ooxml::read_member, rfi_system_sync::scan_system_root_indexed, scan_index::ScanIndex,
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
    if signed {
        write_pdf_at(path, 470, 195, "E.Mammadov Signed Worley");
    } else {
        write_pdf_at(path, 100, 700, "RFI Inspection Details");
    }
}

fn write_pdf_at(path: &Path, x: u32, y: u32, text: &str) {
    let stream = format!("BT /F1 18 Tf {x} {y} Td ({text}) Tj ET\n");
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
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn signature_outside_others_is_not_worley_evidence() {
    let fixture = tempfile::tempdir().unwrap();
    let system = fixture.path().join("RFI_SYSTEM");
    let folder = system.join("CIV/AZE-SUN-INT-QMS-RFI-CIV-0102");
    fs::create_dir_all(&folder).unwrap();
    write_pdf_at(
        &folder.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0102.pdf"),
        460,
        100,
        "Unrelated lower-page signature",
    );

    let (records, _) = scan_system_root_indexed(&system, None, "owner", false, |_| {}).unwrap();
    assert_eq!(records.len(), 1);
    assert!(
        !records[0].signed_by_worley,
        "{}",
        records[0].signature_reason
    );
}

#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn system_scan_requires_the_exact_canonical_rfi_pdf() {
    let fixture = tempfile::tempdir().unwrap();
    let system = fixture.path().join("RFI_SYSTEM");
    let missing = system.join("CIV/AZE-SUN-INT-QMS-RFI-CIV-0101");
    let exact = system.join("MEC/AZE-SUN-INT-QMS-RFI-MEC-0700");
    fs::create_dir_all(&missing).unwrap();
    fs::create_dir_all(&exact).unwrap();
    write_pdf_with_signature(&missing.join("inspection.pdf"), true);
    write_pdf_with_signature(
        &missing.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-9999.pdf"),
        true,
    );
    write_pdf_with_signature(
        &exact.join("rfi no. aze-sun-int-qms-rfi-mec-0700.PDF"),
        true,
    );

    let (records, _) = scan_system_root_indexed(&system, None, "owner", false, |_| {}).unwrap();
    assert_eq!(records.len(), 2);
    let missing = records
        .iter()
        .find(|record| record.canonical_rfi.ends_with("CIV-0101"))
        .unwrap();
    assert!(missing.pdf_path.is_none());
    assert!(!missing.signed_by_worley);
    assert_eq!(
        missing.signature_reason,
        "Exact RFI No. PDF not found in folder"
    );
    let exact = records
        .iter()
        .find(|record| record.canonical_rfi.ends_with("MEC-0700"))
        .unwrap();
    assert!(exact.pdf_path.is_some());
    assert!(exact.signed_by_worley);
}

#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[ignore = "requires operator-supplied signed and unsigned RFI PDFs"]
fn operator_worley_examples_use_only_the_others_signature_cell() {
    let signed = std::env::var("DOCFLOW_SIGNED_WORLEY_PDF").unwrap();
    let unsigned = std::env::var("DOCFLOW_UNSIGNED_WORLEY_PDF").unwrap();
    let expected_unsigned_rfi =
        std::env::var("DOCFLOW_UNSIGNED_WORLEY_RFI").unwrap_or_else(|_| "MEC-0737".to_string());
    let fixture = tempfile::tempdir().unwrap();
    let system = fixture.path().join("RFI_SYSTEM");
    let signed_folder = system.join("CIV/AZE-SUN-INT-QMS-RFI-CIV-1041");
    let unsigned_folder = system
        .join("MEC")
        .join(format!("AZE-SUN-INT-QMS-RFI-{expected_unsigned_rfi}"));
    fs::create_dir_all(&signed_folder).unwrap();
    fs::create_dir_all(&unsigned_folder).unwrap();
    fs::copy(
        signed,
        signed_folder.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1041.pdf"),
    )
    .unwrap();
    fs::copy(
        unsigned,
        unsigned_folder.join(format!(
            "RFI No. AZE-SUN-INT-QMS-RFI-{expected_unsigned_rfi}.pdf"
        )),
    )
    .unwrap();
    let (records, _) = scan_system_root_indexed(&system, None, "owner", false, |_| {}).unwrap();
    let signed = records
        .iter()
        .find(|record| record.canonical_rfi.ends_with("CIV-1041"))
        .unwrap();
    let unsigned = records
        .iter()
        .find(|record| record.canonical_rfi.ends_with(&expected_unsigned_rfi))
        .unwrap();
    assert!(signed.signed_by_worley, "{}", signed.signature_reason);
    assert!(!unsigned.signed_by_worley, "{}", unsigned.signature_reason);
}

#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn unchanged_system_pdf_is_reused_from_durable_index() {
    let fixture = tempfile::tempdir().unwrap();
    let system = fixture.path().join("RFI_SYSTEM");
    let folder = system.join("MEC/AZE-SUN-INT-QMS-RFI-MEC-0700");
    fs::create_dir_all(&folder).unwrap();
    write_pdf_with_signature(
        &folder.join("RFI No. AZE-SUN-INT-QMS-RFI-MEC-0700.pdf"),
        true,
    );
    let index = ScanIndex::new(fixture.path().join("scan-index.sqlite3")).unwrap();
    let mut progress = Vec::new();
    let (first, first_stats) =
        scan_system_root_indexed(&system, Some(&index), "owner", false, |event| {
            progress.push(event)
        })
        .unwrap();
    assert_eq!((first_stats.analyzed, first_stats.reused), (1, 0));
    let (second, second_stats) =
        scan_system_root_indexed(&system, Some(&index), "owner", false, |_| {}).unwrap();
    assert_eq!((second_stats.analyzed, second_stats.reused), (0, 1));
    assert_eq!(first, second);
    assert!(first_stats.parallel_analysis);
    assert!(first_stats.analysis_workers >= 1);
    assert!(
        progress
            .iter()
            .any(|event| event.phase == "system_discovery")
    );
    assert!(
        progress
            .iter()
            .any(|event| event.phase == "system_signature_analysis")
    );
}

#[test]
fn completion_status_preview_recurses_and_patches_only_exact_safe_log_cells() {
    let fixture = tempfile::tempdir().unwrap();
    let fixture_path = fs::canonicalize(fixture.path()).unwrap();
    let system = fixture_path.join("RFI_SYSTEM");
    let nested = system.join("MEC/Area-1/deeper");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("RFI No. AZE-SUN-INT-QMS-RFI-MEC-0700.pdf"),
        b"exact-name evidence",
    )
    .unwrap();
    fs::write(
        nested.join("RFI No. AZE-SUN-INT-QMS-RFI-MEC-07001.pdf"),
        b"numeric suffix must not match",
    )
    .unwrap();

    let status_sheet = worksheet([
        row(
            9,
            [
                inline("A9", "RFI No."),
                inline("P9", "Inspection Status"),
                inline("Q9", "KEEP"),
            ],
        ),
        row(
            10,
            [
                inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0700"),
                inline("P10", "Ongoing"),
                formula("Q10", "", "1+1"),
            ],
        ),
    ]);
    let empty_status_sheet = worksheet([row(
        9,
        [inline("A9", "RFI No."), inline("P9", "Inspection Status")],
    )]);
    let workbook = package(&[
        ("CIV", empty_status_sheet.clone()),
        ("MEC", status_sheet),
        ("ELE", empty_status_sheet.clone()),
        ("PRC", empty_status_sheet),
    ]);
    let log_path = fixture_path.join("RFI Log.xlsx");
    fs::write(&log_path, workbook).unwrap();
    let mut log_file = File::options()
        .read(true)
        .write(true)
        .open(&log_path)
        .unwrap();
    let mut progress = Vec::new();
    let plan = preview_complete_status_sync(
        &system,
        &log_path,
        &mut log_file,
        |event| progress.push(event),
        || false,
    )
    .unwrap();
    drop(log_file);

    assert_eq!(plan.op_id, rfi_system_sync::COMPLETE_STATUS_OPERATION_ID);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.meta["editor"]["rows"].as_array().unwrap().len(), 1);
    assert_eq!(plan.meta["editor"]["rows"][0]["currentStatus"], "Ongoing");
    assert_eq!(
        plan.meta["editor"]["rows"][0]["proposedStatus"],
        "Completed"
    );
    assert!(
        progress
            .iter()
            .any(|event| event.phase == "completion_pdf_discovery")
    );

    let mut disabled_rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    disabled_rows[0]["enabled"] = serde_json::Value::Bool(false);
    let disabled = rfi_system_sync::revise(&plan, &disabled_rows).unwrap();
    assert!(disabled.steps.is_empty());
    assert!(
        disabled
            .warnings
            .iter()
            .any(|warning| warning.key == "warn.rfi.completeStatus.noRowsSelected")
    );

    let output = fixture.path().join("patched.xlsx");
    rfi_system_sync::execute_sync_step(&plan.steps[0], &log_path, &output).unwrap();
    let mut patched_file = File::open(output).unwrap();
    let patched = read_workbook_from_reader(&mut patched_file).unwrap();
    assert_eq!(patched.sheet("MEC").unwrap().value(10, 16), "Completed");
    assert_eq!(
        patched
            .sheet("MEC")
            .unwrap()
            .cell(10, 17)
            .unwrap()
            .formula
            .as_deref(),
        Some("1+1")
    );

    let authority = FilesystemAuthority::new(
        fixture_path.join("completion-journal"),
        Duration::from_secs(300),
    );
    let caller = CallerIdentity::new("completion-owner", "completion-session");
    let scope = GrantScope::new(
        &caller.owner_id,
        &caller.session_id,
        "completion-request",
        rfi_system_sync::COMPLETE_STATUS_OPERATION_ID,
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
                rfi_system_sync::COMPLETE_STATUS_OPERATION_ID,
                vec![grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let applied = authority
        .apply_with_progress(&cached.plan_id, &caller, |_| {})
        .unwrap();
    assert_eq!((applied.applied, applied.failed), (1, 0));
    let mut applied_file = File::open(&log_path).unwrap();
    assert_eq!(
        read_workbook_from_reader(&mut applied_file)
            .unwrap()
            .sheet("MEC")
            .unwrap()
            .value(10, 16),
        "Completed"
    );
    authority.undo(&applied.journal_id, &caller, None).unwrap();
    let mut restored_file = File::open(&log_path).unwrap();
    assert_eq!(
        read_workbook_from_reader(&mut restored_file)
            .unwrap()
            .sheet("MEC")
            .unwrap()
            .value(10, 16),
        "Ongoing"
    );
}

#[test]
fn completion_status_preview_refuses_terminal_or_formula_controlled_cells() {
    let fixture = tempfile::tempdir().unwrap();
    let system = fixture.path().join("RFI_SYSTEM");
    fs::create_dir_all(system.join("deep")).unwrap();
    for code in [
        "AZE-SUN-INT-QMS-RFI-CIV-0103",
        "AZE-SUN-INT-QMS-RFI-MEC-0700",
    ] {
        fs::write(
            system.join("deep").join(format!("RFI No. {code}.pdf")),
            b"exact-name evidence",
        )
        .unwrap();
    }
    let civ = worksheet([
        row(
            9,
            [inline("A9", "RFI No."), inline("P9", "Inspection Status")],
        ),
        row(
            10,
            [
                inline("A10", "AZE-SUN-INT-QMS-RFI-CIV-0103"),
                formula("P10", "", "1+1"),
            ],
        ),
    ]);
    let mec = worksheet([
        row(
            9,
            [inline("A9", "RFI No."), inline("P9", "Inspection Status")],
        ),
        row(
            10,
            [
                inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0700"),
                inline("P10", "Cancelled"),
            ],
        ),
    ]);
    let empty = worksheet([row(
        9,
        [inline("A9", "RFI No."), inline("P9", "Inspection Status")],
    )]);
    let log_path = fixture.path().join("RFI Log.xlsx");
    fs::write(
        &log_path,
        package(&[
            ("CIV", civ),
            ("MEC", mec),
            ("ELE", empty.clone()),
            ("PRC", empty),
        ]),
    )
    .unwrap();
    let mut file = File::options()
        .read(true)
        .write(true)
        .open(&log_path)
        .unwrap();
    let plan =
        preview_complete_status_sync(&system, &log_path, &mut file, |_| {}, || false).unwrap();
    assert!(plan.steps.is_empty());
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row["enabled"] == false));
    assert!(
        rows.iter()
            .all(|row| { row["disposition"] == "rfi.completeStatus.disposition.manualReview" })
    );
}

#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
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

    // 3. CIV-0105 unsigned must not revoke a prior signed status.
    let civ_105_dir = civ_dir.join("AZE-SUN-INT-QMS-RFI-CIV-0105");
    fs::create_dir_all(&civ_105_dir).unwrap();
    let civ_105_pdf = civ_105_dir.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0105.pdf");
    write_pdf_with_signature(&civ_105_pdf, false);
    // 4. CIV-0106 unsigned has a source log row but no prepared signature row.
    let civ_106_dir = civ_dir.join("AZE-SUN-INT-QMS-RFI-CIV-0106");
    fs::create_dir_all(&civ_106_dir).unwrap();
    let civ_106_pdf = civ_106_dir.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0106.pdf");
    write_pdf_with_signature(&civ_106_pdf, false);

    // 5. MEC-0700 signed
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
        row(
            12,
            [
                inline("A12", "AZE-SUN-INT-QMS-RFI-CIV-0105"),
                inline("V12", ""),
            ],
        ),
        row(
            13,
            [
                inline("A13", "AZE-SUN-INT-QMS-RFI-CIV-0106"),
                inline("V13", ""),
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
        row(6, [inline("C6", "CIV-0104"), inline("D6", "")]),
        row(7, [inline("C7", "CIV-0105"), inline("D7", "Signed")]),
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
    let missing_target = plan.meta["editor"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["rfiNo"] == "CIV-0106")
        .unwrap();
    assert_eq!(
        missing_target["disposition"],
        "rfi.systemSync.disposition.manualReview"
    );
    assert_eq!(
        missing_target["followUpAction"],
        "Manual review: no existing Follow Up control row"
    );
    assert_eq!(missing_target["selectable"], false);

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
    drop(rfi_log_file);
    drop(follow_up_file);

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
    assert_eq!(civ_sheet.value(12, 22), "");
    assert_eq!(civ_sheet.value(13, 22), "");
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
    assert_eq!(fu_civ.value(7, 4), "Signed");

    let fu_mec = updated_fu.sheet("Signed By Worley MEC").unwrap();
    assert_eq!(fu_mec.value(4, 20), "Signed");

    // UNDO
    let undo_result = authority.undo(&result.journal_id, &caller, None).unwrap();
    assert_eq!(undo_result.failed, 0);

    // Undo owns restoration of both workbook files; verify their original bytes.
    let restored_rfi_log = fs::read(&rfi_log_path).unwrap();
    let restored_fu = fs::read(&follow_up_path).unwrap();
    assert_eq!(restored_rfi_log, rfi_log_bytes);
    assert_eq!(restored_fu, follow_up_bytes);
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
        check_fu.sheet("Signed By Worley CIV").unwrap().value(6, 4),
        ""
    );
    assert_eq!(
        check_fu.sheet("Signed By Worley CIV").unwrap().value(7, 4),
        "Signed"
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
    let original_rfi_log = fs::read(&rfi_log_source).unwrap();
    let original_follow_up = fs::read(&follow_up_source).unwrap();
    fs::copy(&rfi_log_source, &rfi_log).unwrap();
    fs::copy(&follow_up_source, &follow_up).unwrap();
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
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "both discovered RFIs must remain visible");
    if plan.steps.is_empty() {
        assert!(
            rows.iter().all(|row| {
                row["disposition"] == "rfi.systemSync.disposition.alreadySynced"
                    || (row["disposition"] == "rfi.systemSync.disposition.manualReview"
                        && row["selectable"] == false
                        && row["enabled"] == false
                        && row["followUpAction"]
                            == "Manual review: no existing Follow Up control row")
            }),
            "a zero-step plan must represent every RFI as synchronized or as a concrete non-selectable review"
        );
        assert!(
            rows.iter()
                .any(|row| row["disposition"] == "rfi.systemSync.disposition.manualReview")
                || rows
                    .iter()
                    .all(|row| row["disposition"] == "rfi.systemSync.disposition.alreadySynced"),
            "a zero-step plan must not disguise unresolved RFIs as already synchronized"
        );
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.key == "warn.rfi.systemSync.noChangesNeeded")
        );
    } else {
        assert!(
            rows.iter()
                .any(|row| row["disposition"] == "rfi.systemSync.disposition.ready"),
            "a non-empty plan requires at least one ready RFI"
        );
        assert!(plan.warnings.is_empty());
        for (index, step) in plan.steps.iter().enumerate() {
            let output = fixture.path().join(format!("guarded-output-{index}.xlsm"));
            rfi_system_sync::execute_sync_step(
                step,
                Path::new(step.src.as_deref().unwrap()),
                &output,
            )
            .unwrap();
        }
    }
    assert_eq!(fs::read(&rfi_log_source).unwrap(), original_rfi_log);
    assert_eq!(fs::read(&follow_up_source).unwrap(), original_follow_up);
}

#[test]
#[ignore = "requires an operator-supplied current production RFI Log copy"]
fn current_rfi_log_accepts_only_closed_completion_status_deltas() {
    let source = PathBuf::from(
        std::env::var("DOCFLOW_CURRENT_RFI_LOG").expect("DOCFLOW_CURRENT_RFI_LOG is required"),
    );
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("RFI_SYSTEM/deep/archive");
    fs::create_dir_all(&root).unwrap();
    for rfi in ["MEC-0001", "MEC-0361"] {
        fs::write(
            root.join(format!("RFI No. AZE-SUN-INT-QMS-RFI-{rfi}.pdf")),
            b"exact-name evidence",
        )
        .unwrap();
    }
    let log = fixture.path().join("RFI Log.xlsm");
    fs::copy(source, &log).unwrap();
    let mut file = File::options().read(true).write(true).open(&log).unwrap();
    let plan = preview_complete_status_sync(
        &fixture.path().join("RFI_SYSTEM"),
        &log,
        &mut file,
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(plan.meta["editor"]["rows"].as_array().unwrap().len(), 2);
    for (index, step) in plan.steps.iter().enumerate() {
        let output = fixture
            .path()
            .join(format!("completion-output-{index}.xlsm"));
        rfi_system_sync::execute_sync_step(step, &log, &output).unwrap();
    }
}

#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[ignore = "set DOCFLOW_VENTOY_RFI_SYSTEM_ROOT, DOCFLOW_CURRENT_RFI_LOG, and DOCFLOW_CURRENT_FOLLOW_UP to private Ventoy copies"]
fn operator_ventoy_system_sync_uses_real_signatures_and_workbooks() {
    let system_root = PathBuf::from(
        std::env::var("DOCFLOW_VENTOY_RFI_SYSTEM_ROOT")
            .expect("DOCFLOW_VENTOY_RFI_SYSTEM_ROOT is required"),
    );
    let fixture = tempfile::tempdir().unwrap();
    let rfi_log = fixture.path().join("RFI Log.xlsm");
    let follow_up = fixture.path().join("Inspection Status Follow Up.xlsm");
    fs::copy(
        std::env::var("DOCFLOW_CURRENT_RFI_LOG").expect("DOCFLOW_CURRENT_RFI_LOG is required"),
        &rfi_log,
    )
    .unwrap();
    fs::copy(
        std::env::var("DOCFLOW_CURRENT_FOLLOW_UP").expect("DOCFLOW_CURRENT_FOLLOW_UP is required"),
        &follow_up,
    )
    .unwrap();
    let original_log = fs::read(&rfi_log).unwrap();
    let original_follow_up = fs::read(&follow_up).unwrap();
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
        &system_root,
        &rfi_log,
        &follow_up,
        &mut rfi_log_file,
        &mut follow_up_file,
    )
    .unwrap();
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    eprintln!(
        "Ventoy System Sync rows={rows:#?} warnings={:?} steps={}",
        plan.warnings,
        plan.steps.len()
    );
    assert_eq!(rows.len(), 4);
    assert!(plan.steps.is_empty());
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.key == "warn.rfi.systemSync.noChangesNeeded")
    );
    let row_for = |suffix: &str| {
        rows.iter()
            .find(|row| {
                row["canonicalRfi"]
                    .as_str()
                    .is_some_and(|code| code.ends_with(suffix))
            })
            .unwrap_or_else(|| panic!("missing {suffix}; rows={rows:#?}"))
    };
    assert_eq!(row_for("ELE-0032")["worleyStatus"], "Signed by Worley");
    for suffix in ["CIV-1316", "MEC-0767", "PRC-0074"] {
        assert_eq!(
            row_for(suffix)["worleyStatus"],
            "Unsigned",
            "{suffix} has a visually blank OTHERS signature cell"
        );
    }
    for (index, step) in plan.steps.iter().enumerate() {
        let output = fixture.path().join(format!("Ventoy-sync-{index}.xlsm"));
        rfi_system_sync::execute_sync_step(step, Path::new(step.src.as_deref().unwrap()), &output)
            .unwrap();
        assert!(output.is_file());
    }
    assert_eq!(fs::read(&rfi_log).unwrap(), original_log);
    assert_eq!(fs::read(&follow_up).unwrap(), original_follow_up);
}
