use std::{
    fs::{self, File},
    io::{Cursor, Write},
    time::Duration,
};

use docflow_lib::{
    fs_ops::FilesystemAuthority,
    ooxml::read_workbook_from_reader,
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    rfi_system_sync::{self, preview_publish_status},
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

const MEC: &str = "AZE-SUN-INT-QMS-RFI-MEC";

fn log_workbook() -> Vec<u8> {
    let header = row(
        9,
        [inline("A9", "RFI No."), inline("P9", "Inspection Status")],
    );
    let mec = worksheet([
        header.clone(),
        row(
            10,
            [inline("A10", &format!("{MEC}-0876")), inline("P10", "")],
        ),
        row(
            11,
            [
                inline("A11", &format!("{MEC}-0877")),
                inline("P11", "Ongoing"),
            ],
        ),
        row(
            12,
            [inline("A12", &format!("{MEC}-0878")), inline("P12", "")],
        ),
    ]);
    let empty = worksheet([header]);
    package(&[
        ("CIV", empty.clone()),
        ("MEC", mec),
        ("ELE", empty.clone()),
        ("PRC", empty),
    ])
}

fn dispositions(plan: &docflow_lib::contracts::Plan) -> Vec<(String, String)> {
    plan.meta["editor"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["canonicalRfi"].as_str().unwrap().to_string(),
                row["disposition"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn publish_preview_marks_only_blank_status_with_complete_pair() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(fixture.path()).unwrap();
    let staging = root.join("incoming");
    fs::create_dir_all(staging.join("nested")).unwrap();
    for name in [
        format!("{MEC}-0876.pdf"),
        format!("{MEC}-0876.docx"),
        format!("{MEC}-0877.pdf"),
        format!("{MEC}-0877.docx"),
        format!("{MEC}-0878.pdf"),
        format!("{MEC}-0879.pdf"),
        format!("{MEC}-0879.docx"),
        "image001.png".to_string(),
    ] {
        fs::write(staging.join(name), b"x").unwrap();
    }
    fs::write(staging.join(format!("nested/{MEC}-0880.pdf")), b"x").unwrap();
    let log_path = root.join("RFI Log.xlsx");
    fs::write(&log_path, log_workbook()).unwrap();
    let mut log_file = File::options()
        .read(true)
        .write(true)
        .open(&log_path)
        .unwrap();

    let plan =
        preview_publish_status(&staging, &log_path, &mut log_file, |_| {}, || false).unwrap();

    assert_eq!(plan.op_id, rfi_system_sync::PUBLISH_STATUS_OPERATION_ID);
    assert_eq!(
        dispositions(&plan),
        [
            (
                format!("{MEC}-0876"),
                "rfi.publishStatus.disposition.ready".to_string()
            ),
            (
                format!("{MEC}-0877"),
                "rfi.publishStatus.disposition.statusNotBlank".to_string()
            ),
            (
                format!("{MEC}-0878"),
                "rfi.publishStatus.disposition.incompleteFiles".to_string()
            ),
            (
                format!("{MEC}-0879"),
                "rfi.publishStatus.disposition.notInLog".to_string()
            ),
        ]
    );
    assert_eq!(plan.steps.len(), 1, "only 0876 is authorized");
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["proposedStatus"], "Ongoing");
    assert_eq!(rows[0]["selectable"], true);
    assert!(rows[1..].iter().all(|row| row["selectable"] == false));
}

#[test]
fn publish_preview_without_ready_rows_creates_no_step() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(fixture.path()).unwrap();
    let staging = root.join("incoming");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join(format!("{MEC}-0877.pdf")), b"x").unwrap();
    fs::write(staging.join(format!("{MEC}-0877.docx")), b"x").unwrap();
    let log_path = root.join("RFI Log.xlsx");
    fs::write(&log_path, log_workbook()).unwrap();
    let mut log_file = File::options()
        .read(true)
        .write(true)
        .open(&log_path)
        .unwrap();

    let plan =
        preview_publish_status(&staging, &log_path, &mut log_file, |_| {}, || false).unwrap();

    assert!(plan.steps.is_empty());
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.key == "warn.rfi.publishStatus.noChangesNeeded")
    );
}

#[test]
fn publish_preview_refuses_formula_controlled_status_cell() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(fixture.path()).unwrap();
    let staging = root.join("incoming");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join(format!("{MEC}-0876.pdf")), b"x").unwrap();
    fs::write(staging.join(format!("{MEC}-0876.docx")), b"x").unwrap();
    let header = row(
        9,
        [inline("A9", "RFI No."), inline("P9", "Inspection Status")],
    );
    let mec = worksheet([
        header.clone(),
        row(
            10,
            [
                inline("A10", &format!("{MEC}-0876")),
                formula("P10", "", "\"\""),
            ],
        ),
    ]);
    let empty = worksheet([header]);
    let workbook = package(&[
        ("CIV", empty.clone()),
        ("MEC", mec),
        ("ELE", empty.clone()),
        ("PRC", empty),
    ]);
    let log_path = root.join("RFI Log.xlsx");
    fs::write(&log_path, workbook).unwrap();
    let mut log_file = File::options()
        .read(true)
        .write(true)
        .open(&log_path)
        .unwrap();

    let plan =
        preview_publish_status(&staging, &log_path, &mut log_file, |_| {}, || false).unwrap();

    assert!(plan.steps.is_empty());
    assert_eq!(
        plan.meta["editor"]["rows"][0]["disposition"],
        "rfi.publishStatus.disposition.unsafeCell"
    );
}

fn status_cell(path: &std::path::Path) -> String {
    let mut file = File::open(path).unwrap();
    read_workbook_from_reader(&mut file)
        .unwrap()
        .sheet("MEC")
        .unwrap()
        .value(10, 16)
        .to_string()
}

#[test]
fn publish_status_applies_revises_and_undoes_through_the_guarded_pipeline() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(fixture.path()).unwrap();
    let staging = root.join("incoming");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join(format!("{MEC}-0876.pdf")), b"x").unwrap();
    fs::write(staging.join(format!("{MEC}-0876.docx")), b"x").unwrap();
    let log_path = root.join("RFI Log.xlsx");
    fs::write(&log_path, log_workbook()).unwrap();
    let mut log_file = File::options()
        .read(true)
        .write(true)
        .open(&log_path)
        .unwrap();
    let plan =
        preview_publish_status(&staging, &log_path, &mut log_file, |_| {}, || false).unwrap();
    drop(log_file);
    assert_eq!(plan.steps.len(), 1);

    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["enabled"] = serde_json::Value::Bool(false);
    let disabled = rfi_system_sync::revise(&plan, &rows).unwrap();
    assert!(disabled.steps.is_empty());
    assert!(
        disabled
            .warnings
            .iter()
            .any(|w| w.key == "warn.rfi.publishStatus.noRowsSelected")
    );

    let authority = FilesystemAuthority::new(root.join("journal"), Duration::from_secs(300));
    let caller = CallerIdentity::new("publish-owner", "publish-session");
    let op = rfi_system_sync::PUBLISH_STATUS_OPERATION_ID;
    let scope = GrantScope::new(&caller.owner_id, &caller.session_id, "publish-request", op);
    let grant = authority
        .issue_grant(&scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                caller.owner_id.clone(),
                caller.session_id.clone(),
                op,
                vec![grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let applied = authority
        .apply_with_progress(&cached.plan_id, &caller, |_| {})
        .unwrap();
    assert_eq!((applied.applied, applied.failed), (1, 0));
    assert_eq!(status_cell(&log_path), "Ongoing");
    authority.undo(&applied.journal_id, &caller, None).unwrap();
    assert_eq!(status_cell(&log_path), "");
}
