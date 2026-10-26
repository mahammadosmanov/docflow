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
        canonical_follow_up_rfi, canonical_rfi, classify_workbooks, guarded_updates,
        mapped_follow_up_status, preview_from_open_files, revise_from_open_files,
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

fn full_schema_source(civ_rows: impl IntoIterator<Item = String>) -> Cursor<Vec<u8>> {
    let header = row(
        9,
        [
            inline("A9", "RFI No."),
            inline("D9", "ITP No:"),
            inline("E9", "Item No."),
            inline("F9", "Tag No"),
            inline("G9", "Area"),
            inline("N9", "Inspection Location"),
            inline("T9", "Inspection Status"),
            inline("V9", "Remarks"),
        ],
    );
    let civ = worksheet(std::iter::once(header.clone()).chain(civ_rows), &[]);
    let mec = worksheet([header], &[]);
    source_package(civ, mec)
}

fn full_schema_record(
    row_number: u32,
    suffix: &str,
    item: &str,
    tag: &str,
    remarks: &str,
) -> String {
    full_schema_record_with_status(
        row_number,
        suffix,
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        item,
        tag,
        "Completed",
        remarks,
    )
}

fn full_schema_record_with_status(
    row_number: u32,
    suffix: &str,
    itp: &str,
    item: &str,
    tag: &str,
    status: &str,
    remarks: &str,
) -> String {
    row(
        row_number,
        [
            inline(
                &format!("A{row_number}"),
                &format!("AZE-SUN-INT-QMS-RFI-CIV-{suffix}"),
            ),
            inline(&format!("D{row_number}"), itp),
            inline(&format!("E{row_number}"), item),
            inline(&format!("F{row_number}"), tag),
            inline(&format!("G{row_number}"), "Zone 1"),
            inline(&format!("N{row_number}"), "N1"),
            inline(&format!("T{row_number}"), status),
            inline(&format!("V{row_number}"), remarks),
        ],
    )
}

fn full_schema_mec_record(
    row_number: u32,
    suffix: &str,
    item: &str,
    tag: &str,
    area: &str,
    location: &str,
    status: &str,
) -> String {
    full_schema_mec_record_with_itp(
        row_number,
        suffix,
        (
            "AZE-SUN-INT-QMS-ITP-MEC-0001",
            item,
            tag,
            area,
            location,
            status,
        ),
    )
}

fn full_schema_mec_record_with_itp(
    row_number: u32,
    suffix: &str,
    fields: (&str, &str, &str, &str, &str, &str),
) -> String {
    let (itp, item, tag, area, location, status) = fields;
    row(
        row_number,
        [
            inline(
                &format!("A{row_number}"),
                &format!("AZE-SUN-INT-QMS-RFI-MEC-{suffix}"),
            ),
            inline(&format!("D{row_number}"), itp),
            inline(&format!("E{row_number}"), item),
            inline(&format!("F{row_number}"), tag),
            inline(&format!("G{row_number}"), area),
            inline(&format!("N{row_number}"), location),
            inline(&format!("T{row_number}"), status),
            blank(&format!("V{row_number}")),
        ],
    )
}

fn full_schema_mec_header() -> String {
    row(
        9,
        [
            inline("A9", "RFI No."),
            inline("D9", "ITP No:"),
            inline("E9", "Item No."),
            inline("F9", "Tag No"),
            inline("G9", "Area"),
            inline("N9", "Inspection Location"),
            inline("T9", "Inspection Status"),
            inline("V9", "Remarks"),
        ],
    )
}

#[test]
fn mixed_completed_groups_update_status_without_writing_remarks() {
    let source = full_schema_source([
        full_schema_record_with_status(
            10,
            "1222",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-1.5",
            "AREA-1",
            "Completed",
            "",
        ),
        full_schema_record_with_status(
            11,
            "1223",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-1.5",
            "AREA-1",
            "Ongoing",
            "",
        ),
        full_schema_record_with_status(
            12,
            "1224",
            "AZE-SUN-INT-QMS-ITP-CIV-0005",
            "CIV-3.6",
            "PCS-01",
            "Completed",
            "",
        ),
        full_schema_record_with_status(
            13,
            "1225",
            "AZE-SUN-INT-QMS-ITP-CIV-0005",
            "CIV-3.6",
            "PCS-01",
            "Ongoing",
            "",
        ),
    ]);
    let target = package(&[
        (
            "CIV",
            None,
            worksheet(
                [
                    row(
                        1,
                        [
                            inline("B1", "Tag"),
                            inline("C1", "RFI No."),
                            inline("D1", "STATUS"),
                            inline("Q1", "Remarks"),
                        ],
                    ),
                    row(
                        2,
                        [
                            inline("B2", "AREA-1"),
                            inline("C2", "CIV-1222/1223"),
                            inline("D2", "Pending"),
                            inline(
                                "Q2",
                                "operator note\nRFI Status Sync: CIV-1222 In Progress, CIV-1223 In Progress",
                            ),
                        ],
                    ),
                ],
                &[],
            ),
        ),
        (
            "PCU",
            None,
            worksheet(
                [
                    row(
                        3,
                        [
                            inline("P3", "Final acceptance of PCU foundation (3.6)"),
                            inline("R3", "Signed by Worley"),
                        ],
                    ),
                    row(
                        4,
                        [
                            inline("P4", "RFI No."),
                            inline("Q4", "STATUS"),
                            inline("U4", "Remarks"),
                        ],
                    ),
                    row(
                        5,
                        [
                            inline("B5", "N1"),
                            inline("C5", "PCS-01"),
                            inline("P5", "CIV-1224/1225"),
                            inline("Q5", "Pending"),
                            inline("R5", "Signed"),
                        ],
                    ),
                ],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [
                    row(
                        1,
                        [
                            inline("B1", "Tag"),
                            inline("C1", "RFI No."),
                            inline("D1", "STATUS"),
                            inline("Q1", "Remarks"),
                        ],
                    ),
                    row(
                        2,
                        [
                            inline("B2", "OTHER"),
                            inline("C2", "CIV-1222/1223"),
                            inline("D2", "Pending"),
                            inline("Q2", "operator note"),
                        ],
                    ),
                ],
                &[],
            ),
        ),
    ]);

    let review = classify(source.clone(), target.clone()).unwrap();
    let rows = review["rows"].as_array().unwrap();
    let civ = rows
        .iter()
        .find(|row| row["_sheet"] == "CIV" && row["_row"] == 2)
        .unwrap();
    let pcu = rows
        .iter()
        .find(|row| row["_sheet"] == "PCU" && row["target"]["statusColumn"] == 17)
        .unwrap();
    assert_eq!(civ["proposedStatus"], "In Progress");
    assert_eq!(pcu["proposedStatus"], "");
    assert_eq!(pcu["proposedRfiNo"], "");
    assert!(rows.iter().all(|row| {
        row.get("currentRemark").is_none()
            && row.get("proposedRemark").is_none()
            && row.get("remarkCellAddress").is_none()
            && row["target"].get("remarkColumn").is_none()
            && row["target"].get("remarkCell").is_none()
            && row["target"].get("remarkCellExists").is_none()
    }));
    let fixture = tempfile::tempdir().unwrap();
    let source_path = fixture.path().join("RFI Log.xlsx");
    let target_path = fixture.path().join("Follow Up.xlsx");
    std::fs::write(&source_path, source.into_inner()).unwrap();
    std::fs::write(&target_path, target.into_inner()).unwrap();
    let mut source_file = File::open(&source_path).unwrap();
    let mut target_file = File::open(&target_path).unwrap();
    let plan = preview_from_open_files(
        &source_path,
        &target_path,
        &mut source_file,
        &mut target_file,
    )
    .unwrap();
    let updates = guarded_updates(&plan.steps[0]).unwrap();

    assert!(!updates["CIV"].contains_key(&(2, 17)));
    assert!(updates["PCU"].contains_key(&(5, 16)));
    assert!(updates["PCU"].contains_key(&(5, 17)));
}

#[test]
fn full_sync_recognizes_the_mec_inspection_location_header_variant() {
    let civ = worksheet(
        [row(
            9,
            [
                inline("A9", "RFI No."),
                inline("D9", "ITP No:"),
                inline("E9", "Item No."),
                inline("F9", "Tag No"),
                inline("G9", "Area"),
                inline("N9", "Inspection Location"),
                inline("T9", "Inspection Status"),
                inline("V9", "Remarks"),
            ],
        )],
        &[],
    );
    let mec = worksheet(
        [
            row(
                9,
                [
                    inline("A9", "RFI No."),
                    inline("D9", "ITP No:"),
                    inline("E9", "Item No."),
                    inline("F9", "Tag No"),
                    inline("G9", "Area"),
                    inline("O9", "INSP. LOC./AREA"),
                    inline("Q9", "Inspection Status"),
                    inline("W9", "Remarks"),
                ],
            ),
            row(
                10,
                [
                    inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0791"),
                    inline("D10", "AZE-SUN-INT-QMS-ITP-MEC-0003"),
                    inline("E10", "MEC-3.3.1"),
                    inline("F10", "PCS-07"),
                    inline("G10", "S3"),
                    inline("O10", "N10"),
                    inline("Q10", "Ongoing"),
                    blank("W10"),
                ],
            ),
        ],
        &[],
    );
    let source = source_package(civ, mec);
    let target = package(&[(
        "MEC",
        None,
        worksheet(
            [
                row(3, [inline("P3", "FINAL")]),
                row(4, [inline("P4", "RFI No."), inline("Q4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "N10"),
                        inline("C5", "PCS-07"),
                        inline("D5", "S3"),
                        blank("P5"),
                        blank("Q5"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    let row = rows
        .iter()
        .find(|row| row["_row"] == 5 && row["disposition"] == "rfi.followUp.disposition.ready")
        .unwrap();
    assert_eq!(row["rfiNo"], "MEC-0791");
    assert_eq!(row["_sheet"], "MEC");
    assert_eq!(row["_row"], 5);
    assert_eq!(row["proposedStatus"], "In Progress");
    assert_eq!(row["disposition"], "rfi.followUp.disposition.ready");
}

#[test]
fn full_sync_does_not_route_civ_pcs_final_tag_into_pcu_section() {
    let source = full_schema_source([pcu_final_record(10, "Signed by Worley")]);
    let target = package(&[("PCU", None, pcu_target("", "", ""))]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    assert!(rows.is_empty(), "{review}");
}

#[test]
fn full_sync_keeps_literal_pcu_tagged_civ_final_in_scope() {
    let source =
        full_schema_source([pcu_final_record(10, "Signed by Worley").replace("PCS-01", "PCU-01")]);
    let target = package(&[("PCU", None, pcu_target("", "", ""))]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    assert!(!rows.is_empty(), "{review}");
    assert!(rows.iter().any(|row| row["proposedRfiNo"] == "CIV-1001"));
}

#[test]
fn full_sync_excludes_every_civ_pcs_tagged_rfi_from_follow_up() {
    for tag in ["PCS-19", "PCS-19/S1,S2,S3"] {
        let source = full_schema_source([full_schema_record_with_status(
            10,
            "1401",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-2.2.4",
            tag,
            "Ongoing",
            "",
        )]);
        let target = package(&[
            (
                "MEC",
                None,
                worksheet(
                    [row(5, [inline("A5", "Zone 4"), inline("C5", "PCS-19")])],
                    &[],
                ),
            ),
            (
                "CIV",
                None,
                worksheet(
                    [
                        row(1, [inline("I1", "RFI No."), inline("J1", "STATUS")]),
                        row(
                            20,
                            [
                                inline("A20", "Zone 4"),
                                inline("B20", "S1"),
                                inline("I20", ""),
                                inline("J20", ""),
                            ],
                        ),
                    ],
                    &[],
                ),
            ),
            ("PCU", None, pcu_target("", "", "")),
        ]);

        let review = classify(source, target).unwrap();
        assert!(
            review["rows"].as_array().unwrap().is_empty(),
            "tag={tag}: {review}"
        );
    }

    let source = full_schema_source([full_schema_record_with_status(
        10,
        "1401",
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        "CIV-2.2.4",
        "PCS-19/S1,S2,S3",
        "Ongoing",
        "",
    )]);
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(1, [inline("I1", "RFI No."), inline("J1", "STATUS")]),
                row(
                    20,
                    [
                        inline("A20", "Zone 4"),
                        inline("B20", "S1"),
                        inline("I20", "CIV-1401"),
                        inline("J20", "In Progress"),
                    ],
                ),
            ],
            &[],
        ),
    )]);
    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{review}");
    assert_eq!(rows[0]["proposedRfiNo"], "");
    assert_eq!(rows[0]["proposedStatus"], "");
}

#[test]
fn full_sync_maps_mec_multiarea_rfi_to_distinct_rows_without_inspection_location() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            std::iter::once(full_schema_mec_header()).chain([
                full_schema_mec_record_with_itp(
                    10,
                    "0790",
                    (
                        "AZE-SUN-INT-QMS-ITP-MEC-0003",
                        "MEC-3.3.1",
                        "PCS-21",
                        "S1,S2,S4",
                        "N99",
                        "Ongoing",
                    ),
                ),
                full_schema_mec_record_with_itp(
                    11,
                    "0786",
                    (
                        "AZE-SUN-INT-QMS-ITP-MEC-0004",
                        "MEC-4.3.2",
                        "PCS-08",
                        "S4",
                        "N11",
                        "Ongoing",
                    ),
                ),
            ]),
            &[],
        ),
    );
    let target = package(&[(
        "MEC",
        None,
        worksheet(
            [
                row(
                    4,
                    [
                        inline("P4", "RFI No."),
                        inline("Q4", "STATUS"),
                        inline("R4", "RFI No."),
                        inline("S4", "STATUS"),
                    ],
                ),
                row(
                    5,
                    [
                        inline("B5", "S2"),
                        inline("C5", "PCS-21"),
                        inline("D5", "S1"),
                        blank("P5"),
                        blank("Q5"),
                    ],
                ),
                row(
                    6,
                    [
                        inline("B6", "S2"),
                        inline("C6", "PCS-21"),
                        inline("D6", "S2"),
                        blank("P6"),
                        blank("Q6"),
                    ],
                ),
                row(
                    7,
                    [
                        inline("B7", "S2"),
                        inline("C7", "PCS-21"),
                        inline("D7", "S4"),
                        blank("P7"),
                        blank("Q7"),
                    ],
                ),
                row(
                    8,
                    [
                        inline("B8", "N10/N11"),
                        inline("C8", "PCS-08"),
                        inline("D8", "S4"),
                        blank("R8"),
                        blank("S8"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    let mec0790 = rows
        .iter()
        .filter(|row| {
            row["rfiNo"] == "MEC-0790" && row["disposition"] == "rfi.followUp.disposition.ready"
        })
        .collect::<Vec<_>>();
    assert_eq!(mec0790.len(), 3, "{review}");
    assert_eq!(
        mec0790
            .iter()
            .map(|row| row["_row"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![5, 6, 7]
    );
    let mec0786 = rows
        .iter()
        .find(|row| {
            row["rfiNo"] == "MEC-0786" && row["disposition"] == "rfi.followUp.disposition.ready"
        })
        .unwrap();
    assert_eq!(mec0786["_row"], 8);
    assert_eq!(mec0786["target"]["rfiColumn"], 18);
    assert!(
        rows.iter()
            .filter(|row| row["disposition"] == "rfi.followUp.disposition.ready")
            .all(|row| { row["proposedStatus"] == "In Progress" })
    );
}

#[test]
fn full_sync_hides_a_supported_route_without_a_prepared_follow_up_context() {
    let source = full_schema_source([full_schema_record_with_status(
        10,
        "0041",
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        "CIV-3.1.5",
        "AREA-1",
        "Completed",
        "",
    )]);
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [row(1, [inline("C1", "RFI No."), inline("D1", "STATUS")])],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();

    assert!(review["rows"].as_array().unwrap().is_empty(), "{review}");
}

#[test]
fn full_sync_omits_confirmed_scope_exclusions_but_keeps_undecided_routes_visible() {
    let exclusions = [
        ("CIV", "0002", "CIV-2.1.4"),
        ("CIV", "0002", "CIV-2.1.6"),
        ("CIV", "0005", "CIV-5.1.4"),
        ("CIV", "0005", "CIV-5.1.5"),
        ("CIV", "0005", "CIV-5.1.9"),
        ("CIV", "0005", "CIV-5.3.4"),
        ("CIV", "0005", "CIV-5.3.5"),
        ("CIV", "0006", "CIV-6.1.8"),
        ("CIV", "0006", "CIV-6.2.1"),
        ("CIV", "0006", "CIV-6.2.2"),
        ("CIV", "0006", "CIV-6.2.3"),
        ("CIV", "0006", "CIV-6.2.4"),
        ("CIV", "0006", "CIV-6.2.5"),
        ("CIV", "0006", "CIV-6.2.9"),
        ("CIV", "0007", "CIV-7.1.5"),
        ("CIV", "0007", "CIV-7.2.3"),
        ("CIV", "0007", "CIV-7.2.4"),
        ("CIV", "0007", "CIV-7.3.9"),
        ("MEC", "0001", "MEC-1.1.4"),
        ("MEC", "0001", "MEC-1.2.5"),
        ("MEC", "0004", "MEC-4.3.1"),
    ];
    let mut civ_records = Vec::new();
    let mut mec_records = Vec::new();
    for (index, (discipline, itp_number, item)) in exclusions.iter().enumerate() {
        let row_number = 10 + index as u32;
        let suffix = format!("{:04}", 2000 + index);
        let itp = format!("AZE-SUN-INT-QMS-ITP-{discipline}-{itp_number}");
        if *discipline == "CIV" {
            civ_records.push(full_schema_record_with_status(
                row_number,
                &suffix,
                &itp,
                item,
                "AREA-1",
                "Completed",
                "",
            ));
        } else {
            mec_records.push(full_schema_mec_record_with_itp(
                row_number,
                &suffix,
                (&itp, item, "PCS-01", "S1", "N1", "Completed"),
            ));
        }
    }
    civ_records.push(full_schema_record_with_status(
        100,
        "1999",
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        "CIV-2.1.7",
        "AREA-1",
        "Completed",
        "",
    ));
    let civ_header = row(
        9,
        [
            inline("A9", "RFI No."),
            inline("D9", "ITP No:"),
            inline("E9", "Item No."),
            inline("F9", "Tag No"),
            inline("G9", "Area"),
            inline("N9", "Inspection Location"),
            inline("T9", "Inspection Status"),
            inline("V9", "Remarks"),
        ],
    );
    let source = source_package(
        worksheet(std::iter::once(civ_header).chain(civ_records), &[]),
        worksheet(
            std::iter::once(full_schema_mec_header()).chain(mec_records),
            &[],
        ),
    );
    let target = package(&[
        (
            "CIV",
            None,
            worksheet(
                [row(1, [inline("C1", "RFI No."), inline("D1", "STATUS")])],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [row(1, [inline("N1", "RFI No."), inline("O1", "STATUS")])],
                &[],
            ),
        ),
    ]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    assert_eq!(rows.len(), 1, "{review}");
    assert_eq!(rows[0]["rfiNo"], "CIV-1999");
    assert_eq!(
        rows[0]["reasonKey"],
        "warn.rfi.followUp.reason.mappingUnsupported"
    );
}

#[test]
fn full_sync_keeps_multiple_matching_follow_up_rows_in_manual_review() {
    let source = full_schema_source([full_schema_record_with_status(
        10,
        "0041",
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        "CIV-3.1.5",
        "AREA-1",
        "Completed",
        "",
    )]);
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(1, [inline("C1", "RFI No."), inline("D1", "STATUS")]),
                row(2, [inline("B2", "AREA-1"), blank("C2"), blank("D2")]),
                row(3, [inline("B3", "AREA-1"), blank("C3"), blank("D3")]),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    assert_eq!(rows.len(), 1, "{review}");
    assert_eq!(
        rows[0]["disposition"],
        "rfi.followUp.disposition.manualReview"
    );
    assert_eq!(
        rows[0]["reasonKey"],
        "warn.rfi.followUp.reason.mappingAmbiguous"
    );
    assert!(
        rows[0]["mappingDetails"].as_str().is_some_and(
            |details| details.contains("row 2: AREA-1") && details.contains("row 3: AREA-1")
        ),
        "{review}"
    );
    assert_eq!(review["editor"]["detailsDefaultVisible"], true);
}

#[test]
fn full_sync_fans_out_a_record_only_across_distinct_exact_civ_area_tags() {
    let source = full_schema_source([full_schema_record_with_status(
        10,
        "0041",
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        "CIV-2.1.5",
        "S14,S15",
        "Completed",
        "",
    )]);
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(1, [inline("C1", "RFI No."), inline("D1", "STATUS")]),
                row(2, [inline("B2", "S14"), blank("C2"), blank("D2")]),
                row(3, [inline("B3", "S15"), blank("C3"), blank("D3")]),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{review}");
    assert!(rows.iter().all(|row| {
        row["disposition"] == "rfi.followUp.disposition.ready"
            && row["rfiNo"] == "CIV-0041"
            && row["proposedStatus"] == "Completed"
    }));
    assert!(review["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn full_sync_reports_unmatched_civ_tag_when_only_some_tag_rows_are_prepared() {
    let source = full_schema_source([full_schema_record_with_status(
        10,
        "0041",
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        "CIV-2.1.5",
        "S14,S15",
        "Completed",
        "",
    )]);
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(1, [inline("C1", "RFI No."), inline("D1", "STATUS")]),
                row(2, [inline("B2", "S14"), blank("C2"), blank("D2")]),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{review}");
    let ready = rows
        .iter()
        .find(|row| row["disposition"] == "rfi.followUp.disposition.ready")
        .expect("prepared S14 context remains syncable");
    assert_eq!(ready["_row"], 2);
    assert_eq!(ready["rfiNo"], "CIV-0041");
    assert_eq!(ready["proposedStatus"], "Completed");

    let review_row = rows
        .iter()
        .find(|row| row["reasonKey"] == "warn.rfi.followUp.reason.partialContext")
        .expect("missing S15 context remains visible");
    assert_eq!(review_row["_row"], 0);
    assert_eq!(review_row["rfiNo"], "CIV-0041");
    assert_eq!(
        review_row["disposition"],
        "rfi.followUp.disposition.manualReview"
    );
    assert_eq!(review_row["selectable"], false);
    let details = review_row["mappingDetails"].as_str().unwrap();
    assert!(details.contains("matched source context: tag=S14"));
    assert!(details.contains("unmatched source context: tag=S15"));
}

#[test]
fn full_sync_does_not_fan_out_across_duplicate_civ_area_rows() {
    let source = full_schema_source([full_schema_record_with_status(
        10,
        "0041",
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        "CIV-2.1.5",
        "S14,S15",
        "Completed",
        "",
    )]);
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(1, [inline("C1", "RFI No."), inline("D1", "STATUS")]),
                row(2, [inline("B2", "S14"), blank("C2"), blank("D2")]),
                row(3, [inline("B3", "S14"), blank("C3"), blank("D3")]),
                row(4, [inline("B4", "S15"), blank("C4"), blank("D4")]),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{review}");
    assert_eq!(
        rows[0]["reasonKey"],
        "warn.rfi.followUp.reason.mappingAmbiguous"
    );
    assert_eq!(review["warnings"][0]["subject"], "1");
}

#[test]
fn full_sync_fans_out_mec_rfi_only_across_distinct_exact_route_contexts() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            std::iter::once(full_schema_mec_header()).chain([
                full_schema_mec_record(10, "0120", "MEC-1.3.5", "PCS-06", "S3", "N2", "Completed"),
                full_schema_mec_record(11, "0120", "MEC-1.3.5", "PCS-06", "S4", "N2", "Completed"),
            ]),
            &[],
        ),
    );
    let target = package(&[(
        "MEC",
        None,
        worksheet(
            [
                row(1, [inline("N1", "RFI No."), inline("O1", "STATUS")]),
                row(
                    2,
                    [
                        inline("B2", "N2"),
                        inline("C2", "PCS-06"),
                        inline("D2", "S3"),
                        blank("N2"),
                        blank("O2"),
                    ],
                ),
                row(
                    3,
                    [
                        inline("B3", "N2"),
                        inline("C3", "PCS-06"),
                        inline("D3", "S4"),
                        blank("N3"),
                        blank("O3"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    let ready_rows = rows
        .iter()
        .filter(|row| row["disposition"] == "rfi.followUp.disposition.ready")
        .collect::<Vec<_>>();
    assert_eq!(ready_rows.len(), 2, "{review}");
    assert!(
        ready_rows
            .iter()
            .all(|row| { row["rfiNo"] == "MEC-0120" && row["proposedStatus"] == "Completed" })
    );
    assert!(
        review["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["key"] == "warn.rfi.followUp.unmappedRecords")
    );
}

#[test]
fn full_sync_updates_prepared_mec_context_and_reviews_unprepared_context() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            std::iter::once(full_schema_mec_header()).chain([full_schema_mec_record(
                10,
                "0121",
                "MEC-1.3.5",
                "PCS-06",
                "S1,S2",
                "N2",
                "Completed",
            )]),
            &[],
        ),
    );
    let target = package(&[(
        "MEC",
        None,
        worksheet(
            [
                row(1, [inline("N1", "RFI No."), inline("O1", "STATUS")]),
                row(
                    2,
                    [
                        inline("B2", "N2"),
                        inline("C2", "PCS-06"),
                        inline("D2", "S1"),
                        blank("N2"),
                        blank("O2"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    let ready_rows = rows
        .iter()
        .filter(|row| row["disposition"] == "rfi.followUp.disposition.ready")
        .collect::<Vec<_>>();
    assert_eq!(ready_rows.len(), 1, "{review}");
    assert_eq!(ready_rows[0]["_row"], 2);
    assert_eq!(ready_rows[0]["rfiNo"], "MEC-0121");
    assert_eq!(ready_rows[0]["proposedStatus"], "Completed");

    let missing_rows = rows
        .iter()
        .filter(|row| {
            row["_sheet"] == "MEC" && row["reasonKey"] == "warn.rfi.followUp.reason.partialContext"
        })
        .collect::<Vec<_>>();
    assert_eq!(missing_rows.len(), 1, "{review}");
    assert_eq!(missing_rows[0]["_row"], 0);
    assert_eq!(missing_rows[0]["rfiNo"], "MEC-0121");
    assert_eq!(missing_rows[0]["target"]["worksheet"]["name"], "MEC");
    assert_eq!(
        missing_rows[0]["disposition"],
        "rfi.followUp.disposition.manualReview"
    );
    let details = missing_rows[0]["mappingDetails"].as_str().unwrap();
    assert!(details.contains("unmatched source context: tag=PCS-06; area=S2"));
    assert!(details.contains("matched source context: tag=PCS-06; area=S1"));
    assert!(details.contains("row 2: PCS-06 / S1"));
    assert!(rows.iter().all(|row| row["_row"] != 3));
}

#[test]
fn full_sync_explains_unmatched_mec_final_context_for_rfi_log_correction() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            std::iter::once(full_schema_mec_header()).chain([
                full_schema_mec_record(10, "0123", "MEC-1.3.5", "PCS-06", "S1", "N2", "Completed")
                    .replace(&blank("V10"), &inline("V10", "Signed by Worley")),
                full_schema_mec_record(11, "0123", "MEC-1.3.5", "PCS-06", "S2", "N3", "Completed")
                    .replace(&blank("V11"), &inline("V11", "Signed by Worley")),
            ]),
            &[],
        ),
    );
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline("H3", "Final acceptance of pile installation (3.5)")],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "N2"),
                        inline("C5", "PCS-06"),
                        inline("D5", "S1"),
                        blank("H5"),
                        blank("I5"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let warning = review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["_row"] == 0)
        .expect("missing MEC FINAL context remains visible");
    let details = warning["mappingDetails"].as_str().unwrap();

    assert_eq!(
        warning["reasonKey"],
        "warn.rfi.followUp.reason.partialContext"
    );
    assert!(details.contains("unmatched source context: tag=PCS-06; area=S2"));
    assert!(details.contains("matched source context: tag=PCS-06; area=S1"));
    assert!(details.contains("row 5: PCS-06 / S1"));
}

#[test]
fn full_sync_does_not_use_inspection_location_to_distinguish_mec_contexts() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            std::iter::once(full_schema_mec_header()).chain([
                full_schema_mec_record(10, "0122", "MEC-1.3.5", "PCS-06", "S1", "N2", "Completed"),
                full_schema_mec_record(11, "0122", "MEC-1.3.5", "PCS-06", "S1", "N3", "Completed"),
            ]),
            &[],
        ),
    );
    let target = package(&[(
        "MEC",
        None,
        worksheet(
            [
                row(1, [inline("N1", "RFI No."), inline("O1", "STATUS")]),
                row(
                    2,
                    [
                        inline("B2", "N2"),
                        inline("C2", "PCS-06"),
                        inline("D2", "S1"),
                        blank("N2"),
                        blank("O2"),
                    ],
                ),
                row(
                    3,
                    [
                        inline("B3", "N3"),
                        inline("C3", "PCS-06"),
                        inline("D3", "S1"),
                        blank("N3"),
                        blank("O3"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{review}");
    assert_eq!(
        rows[0]["disposition"],
        "rfi.followUp.disposition.manualReview"
    );
    assert_eq!(
        rows[0]["reasonKey"],
        "warn.rfi.followUp.reason.mappingAmbiguous"
    );
    assert!(
        rows[0]["mappingDetails"]
            .as_str()
            .unwrap()
            .contains("candidate target rows: row 2: PCS-06 / S1 | row 3: PCS-06 / S1")
    );
}

#[test]
fn full_sync_keeps_duplicate_mec_route_context_rows_in_manual_review() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            std::iter::once(full_schema_mec_header()).chain([
                full_schema_mec_record(10, "0120", "MEC-1.3.5", "PCS-06", "S3", "N2", "Completed"),
                full_schema_mec_record(11, "0120", "MEC-1.3.5", "PCS-06", "S4", "N2", "Completed"),
            ]),
            &[],
        ),
    );
    let target = package(&[(
        "MEC",
        None,
        worksheet(
            [
                row(1, [inline("N1", "RFI No."), inline("O1", "STATUS")]),
                row(
                    2,
                    [
                        inline("B2", "N2"),
                        inline("C2", "PCS-06"),
                        inline("D2", "S3"),
                        blank("N2"),
                        blank("O2"),
                    ],
                ),
                row(
                    3,
                    [
                        inline("B3", "N2"),
                        inline("C3", "PCS-06"),
                        inline("D3", "S3"),
                        blank("N3"),
                        blank("O3"),
                    ],
                ),
                row(
                    4,
                    [
                        inline("B4", "N2"),
                        inline("C4", "PCS-06"),
                        inline("D4", "S4"),
                        blank("N4"),
                        blank("O4"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{review}");
    assert_eq!(
        rows[0]["reasonKey"],
        "warn.rfi.followUp.reason.mappingAmbiguous"
    );
}

#[test]
fn mixed_group_with_unknown_source_status_stays_in_manual_review() {
    let source = full_schema_source([
        full_schema_record_with_status(
            10,
            "1222",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-1.5",
            "AREA-1",
            "Completed",
            "",
        ),
        full_schema_record_with_status(
            11,
            "1223",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-1.5",
            "AREA-1",
            "Unrecognized status",
            "",
        ),
    ]);
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(
                    1,
                    [
                        inline("B1", "Tag"),
                        inline("C1", "RFI No."),
                        inline("D1", "STATUS"),
                        inline("Q1", "Remarks"),
                    ],
                ),
                row(
                    2,
                    [
                        inline("B2", "AREA-1"),
                        inline("C2", "CIV-1222/1223"),
                        inline("D2", "In Progress"),
                        inline(
                            "Q2",
                            "RFI Status Sync: CIV-1222 Completed, CIV-1223 In Progress",
                        ),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    let row = rows
        .iter()
        .find(|row| row["reasonKey"] == "warn.rfi.followUp.reason.sourceStatusUnknown")
        .unwrap();

    assert_eq!(row["disposition"], "rfi.followUp.disposition.manualReview");
    assert_eq!(
        row["reasonKey"],
        "warn.rfi.followUp.reason.sourceStatusUnknown"
    );
    assert!(row.get("proposedRemark").is_none());
}

fn pcu_final_record(row_number: u32, remarks: &str) -> String {
    row(
        row_number,
        [
            inline(&format!("A{row_number}"), "AZE-SUN-INT-QMS-RFI-CIV-1001"),
            inline(&format!("D{row_number}"), "AZE-SUN-INT-QMS-ITP-CIV-0005"),
            inline(&format!("E{row_number}"), "CIV-2.3.6"),
            inline(&format!("F{row_number}"), "PCS-01"),
            inline(&format!("G{row_number}"), "Zone 1"),
            inline(&format!("N{row_number}"), "N1"),
            inline(&format!("T{row_number}"), "Completed"),
            inline(&format!("V{row_number}"), remarks),
        ],
    )
}

fn pcu_target(final_rfi: &str, lifecycle_status: &str, worley_status: &str) -> String {
    worksheet(
        [
            row(
                3,
                [
                    inline("P3", "Final acceptance of PCU foundation (3.6)"),
                    inline("R3", "Signed by Worley"),
                ],
            ),
            row(4, [inline("P4", "RFI No."), inline("Q4", "STATUS")]),
            row(
                5,
                [
                    inline("B5", "N1"),
                    inline("C5", "PCS-01"),
                    inline("P5", final_rfi),
                    inline("Q5", lifecycle_status),
                    inline("R5", worley_status),
                ],
            ),
        ],
        &[],
    )
}

#[test]
fn full_sync_removes_pcs_tagged_pcu_final_from_existing_follow_up_rows() {
    let source = full_schema_source([pcu_final_record(10, "Signed by Worley")]);
    let target = package(&[
        (
            "PCU",
            None,
            pcu_target("CIV-1001", "Completed", "Not Signed"),
        ),
        (
            "POT test",
            None,
            worksheet(
                [
                    row(3, [inline("D3", "RFI No."), inline("E3", "STATUS")]),
                    row(4, [inline("D4", "MEC-2009"), inline("E4", "Signed")]),
                ],
                &[],
            ),
        ),
    ]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    assert!(!rows.is_empty(), "{review}");
    assert!(rows.iter().all(|row| {
        row["_sheet"] == "PCU" && row["proposedRfiNo"] == "" && row["proposedStatus"] == ""
    }));
    assert!(
        review["layouts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|layout| layout["worksheet"]["name"] != "POT test")
    );
}

#[test]
fn full_sync_removes_pcs_tagged_pcu_final_with_multiline_worley_header() {
    let source = full_schema_source([pcu_final_record(10, "Signed by Worley")]);
    let target_sheet = pcu_target("CIV-1001", "Completed", "Not Signed")
        .replace("Signed by Worley", "Signed \nby \nWorley");
    let target = package(&[("PCU", None, target_sheet)]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    assert!(!rows.is_empty(), "{review}");
    assert!(
        rows.iter()
            .all(|row| row["proposedRfiNo"] == "" && row["proposedStatus"] == "")
    );
}

#[test]
fn full_sync_does_not_add_pcs_tagged_pcu_final_to_empty_follow_up() {
    let source = full_schema_source([pcu_final_record(10, "Signed by Worley")]);
    let target = package(&[("PCU", None, pcu_target("", "", ""))]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    assert!(rows.is_empty(), "{review}");
    assert!(
        review["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|warning| warning["key"] != "warn.rfi.followUp.unmappedRecords")
    );
}

#[test]
fn pcu_worley_plan_clears_rfi_and_status_for_excluded_pcs_final() {
    let fixture = tempfile::tempdir().unwrap();
    let source_path = fixture.path().join("RFI Log.xlsx");
    let target_path = fixture.path().join("Follow Up.xlsx");
    std::fs::write(
        &source_path,
        full_schema_source([pcu_final_record(10, "Signed by Worley")]).into_inner(),
    )
    .unwrap();
    std::fs::write(
        &target_path,
        package(&[(
            "PCU",
            None,
            pcu_target("CIV-1001", "Completed", "Not Signed"),
        )])
        .into_inner(),
    )
    .unwrap();

    let mut source_file = File::open(&source_path).unwrap();
    let mut target_file = File::open(&target_path).unwrap();
    let plan = preview_from_open_files(
        &source_path,
        &target_path,
        &mut source_file,
        &mut target_file,
    )
    .unwrap();
    let updates = guarded_updates(&plan.steps[0]).unwrap();
    let pcu = &updates["PCU"];

    assert!(pcu.len() >= 2);
    assert!(pcu.contains_key(&(5, 16)));
    assert!(pcu.contains_key(&(5, 17)));
    assert!(pcu.contains_key(&(5, 18)));
}

#[test]
fn full_sync_adds_missing_final_rfis_and_exact_worley_statuses_to_prepared_rows() {
    let source = full_schema_source([
        full_schema_record(10, "0001", "CIV-2.3.1", "AREA-1", "Signed by Worley"),
        full_schema_record(11, "0002", "CIV-2.3.1", "AREA-2", ""),
        full_schema_record(12, "0003", "CIV-2.1.5", "AREA-3", "Signed by Worley"),
    ]);
    let target = package(&[(
        "Signed By Worley CIV",
        None,
        worksheet(
            [
                row(4, [inline("C4", "RFI No."), inline("D4", "Status")]),
                row(5, [inline("B5", "AREA-1"), blank("C5"), blank("D5")]),
                row(6, [inline("B6", "AREA-2"), blank("C6"), blank("D6")]),
                row(7, [inline("B7", "AREA-3"), blank("C7"), blank("D7")]),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["proposedRfiNo"], "CIV-0001");
    assert_eq!(rows[0]["sourceStatuses"], json!(["Signed"]));
    assert_eq!(rows[0]["proposedStatus"], "Signed");
    assert_eq!(rows[1]["proposedRfiNo"], "CIV-0002");
    assert_eq!(rows[1]["sourceStatuses"], json!(["Not Signed"]));
    assert_eq!(rows[1]["proposedStatus"], "Not Signed");
    assert!(
        rows.iter()
            .all(|row| row["reasonKey"] == "warn.rfi.followUp.reason.readyWorleySync")
    );
}
#[test]
fn full_sync_fans_out_signed_civ_final_to_each_distinct_exact_area() {
    let source = full_schema_source([
        full_schema_record_with_status(
            10,
            "0001",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-2.3.1",
            "AREA-1",
            "Completed",
            "Signed by Worley",
        ),
        full_schema_record_with_status(
            11,
            "0001",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-2.3.1",
            "AREA-2",
            "Completed",
            "Signed by Worley",
        ),
    ]);
    let target = package(&[(
        "Signed By Worley CIV",
        None,
        worksheet(
            [
                row(4, [inline("C4", "RFI No."), inline("D4", "Status")]),
                row(
                    5,
                    [
                        inline("B5", "AREA-1"),
                        inline("C5", "CIV-0001"),
                        inline("D5", "Signed"),
                    ],
                ),
                row(6, [inline("B6", "AREA-2"), blank("C6"), blank("D6")]),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{review}");
    assert_eq!(rows[0]["_row"], 6);
    assert_eq!(rows[0]["proposedRfiNo"], "CIV-0001");
    assert_eq!(rows[0]["sourceStatuses"], json!(["Signed"]));
    assert_eq!(rows[0]["proposedStatus"], "Signed");
}

#[test]
fn unsigned_duplicate_does_not_downgrade_a_signed_source_rfi() {
    let source = full_schema_source([
        full_schema_record_with_status(
            10,
            "0001",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-1.3.1",
            "AREA-1",
            "Completed",
            "Signed by Worley",
        ),
        full_schema_record_with_status(
            11,
            "0001",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-1.3.1",
            "AREA-1",
            "Completed",
            "",
        ),
    ]);
    let target = package(&[(
        "Signed By Worley CIV",
        None,
        worksheet(
            [
                row(4, [inline("C4", "RFI No."), inline("D4", "Status")]),
                row(5, [inline("B5", "AREA-1"), blank("C5"), blank("D5")]),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let row = review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["proposedRfiNo"] == "CIV-0001")
        .unwrap();

    assert_eq!(row["sourceStatuses"], json!(["Signed"]));
    assert_eq!(row["proposedStatus"], "Signed");
    assert_eq!(row["disposition"], "rfi.followUp.disposition.ready");
}
#[test]
fn full_sync_adds_unsigned_mec_final_rfi_to_a_matching_prepared_row() {
    let mec = worksheet(
        [
            full_schema_mec_header(),
            full_schema_mec_record(10, "0001", "MEC-1.3.5", "PCS-01", "S1", "N1", "Completed"),
        ],
        &[],
    );
    let source = source_package(worksheet([full_schema_mec_header()], &[]), mec);
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline("H3", "Final acceptance of pile installation (3.5)")],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("A5", "Zone 1"),
                        inline("B5", "N1"),
                        inline("C5", "PCS-01"),
                        inline("D5", "S1"),
                        blank("H5"),
                        blank("I5"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let row = review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["proposedRfiNo"] == "MEC-0001")
        .unwrap();

    assert_eq!(row["rfiNo"], "MEC-0001");
    assert_eq!(row["sourceStatuses"], json!(["Not Signed"]));
    assert_eq!(row["proposedRfiNo"], "MEC-0001");
    assert_eq!(row["proposedStatus"], "Not Signed");
    assert_eq!(row["disposition"], "rfi.followUp.disposition.ready");
}

#[test]
fn full_sync_adds_missing_signed_mec_destination_even_when_same_rfi_exists_elsewhere() {
    let source_record = |row_number, area: &str| {
        row(
            row_number,
            [
                inline(&format!("A{row_number}"), "AZE-SUN-INT-QMS-RFI-MEC-0767"),
                inline(&format!("D{row_number}"), "AZE-SUN-INT-QMS-ITP-MEC-0001"),
                inline(&format!("E{row_number}"), "MEC-1.3.5"),
                inline(&format!("F{row_number}"), "PCS-06"),
                inline(&format!("G{row_number}"), area),
                inline(&format!("N{row_number}"), "N2"),
                inline(&format!("T{row_number}"), "Completed"),
                inline(&format!("V{row_number}"), "Signed by Worley"),
            ],
        )
    };
    for source_records in [
        [source_record(10, "S1"), source_record(11, "S2")],
        [source_record(10, "S2"), source_record(11, "S1")],
    ] {
        let source = source_package(
            worksheet([full_schema_mec_header()], &[]),
            worksheet(
                std::iter::once(full_schema_mec_header()).chain(source_records),
                &[],
            ),
        );
        let target = package(&[(
            "Signed By Worley MEC",
            None,
            worksheet(
                [
                    row(
                        3,
                        [inline("H3", "Final acceptance of pile installation (3.5)")],
                    ),
                    row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                    row(
                        5,
                        [
                            inline("B5", "N2"),
                            inline("C5", "PCS-06"),
                            inline("D5", "S1"),
                            inline("H5", "MEC-0767"),
                            inline("I5", "Signed"),
                        ],
                    ),
                    row(
                        6,
                        [
                            inline("B6", "N2"),
                            inline("C6", "PCS-06"),
                            inline("D6", "S2"),
                            blank("H6"),
                            blank("I6"),
                        ],
                    ),
                ],
                &[],
            ),
        )]);

        let review = classify(source, target).unwrap();
        let rows = review["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{review}");
        assert_eq!(rows[0]["_sheet"], "Signed By Worley MEC");
        assert_eq!(rows[0]["_row"], 6);
        assert_eq!(rows[0]["proposedRfiNo"], "MEC-0767");
        assert_eq!(rows[0]["proposedStatus"], "Signed");
        assert_eq!(rows[0]["disposition"], "rfi.followUp.disposition.ready");
    }
}

#[test]
fn full_sync_updates_prepared_mec_context_and_reviews_missing_context() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            [
                full_schema_mec_header(),
                full_schema_mec_record(10, "0123", "MEC-1.3.5", "PCS-06", "S1", "N2", "Completed")
                    .replace(&blank("V10"), &inline("V10", "Signed by Worley")),
                full_schema_mec_record(11, "0123", "MEC-1.3.5", "PCS-06", "S2", "N2", "Completed")
                    .replace(&blank("V11"), &inline("V11", "Signed by Worley")),
            ],
            &[],
        ),
    );
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline("H3", "Final acceptance of pile installation (3.5)")],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "N2"),
                        inline("C5", "PCS-06"),
                        inline("D5", "S1"),
                        blank("H5"),
                        blank("I5"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert!(rows.iter().any(|row| {
        row["_row"] == 5
            && row["proposedRfiNo"] == "MEC-0123"
            && row["proposedStatus"] == "Signed"
            && row["disposition"] == "rfi.followUp.disposition.ready"
    }));
    assert!(rows.iter().any(|row| {
        row["_row"] == 0
            && row["target"]["worksheet"]["name"] == "Signed By Worley MEC"
            && row["disposition"] == "rfi.followUp.disposition.manualReview"
            && row["reasonKey"] == "warn.rfi.followUp.reason.partialContext"
    }));
}

#[test]
fn full_sync_does_not_duplicate_mec_final_entries_when_all_exact_rows_are_present() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            [
                full_schema_mec_header(),
                full_schema_mec_record(10, "0767", "MEC-1.3.5", "PCS-06", "S1", "N2", "Completed"),
                full_schema_mec_record(11, "0767", "MEC-1.3.5", "PCS-06", "S2", "N2", "Completed"),
            ],
            &[],
        ),
    );
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline("H3", "Final acceptance of pile installation (3.5)")],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "N2"),
                        inline("C5", "PCS-06"),
                        inline("D5", "S1"),
                        inline("H5", "MEC-0767"),
                        inline("I5", "Not Signed"),
                    ],
                ),
                row(
                    6,
                    [
                        inline("B6", "N2"),
                        inline("C6", "PCS-06"),
                        inline("D6", "S2"),
                        inline("H6", "MEC-0767"),
                        inline("I6", "Not Signed"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    assert!(review["rows"].as_array().unwrap().is_empty(), "{review}");
}

#[test]
fn full_sync_reports_unrelated_mec_final_placement_in_manual_review() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            [
                full_schema_mec_header(),
                full_schema_mec_record(10, "0767", "MEC-1.3.5", "PCS-06", "S2", "N2", "Completed"),
            ],
            &[],
        ),
    );
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline("H3", "Final acceptance of pile installation (3.5)")],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "N2"),
                        inline("C5", "PCS-06"),
                        inline("D5", "S1"),
                        inline("H5", "MEC-0767"),
                        inline("I5", "Not Signed"),
                    ],
                ),
                row(
                    6,
                    [
                        inline("B6", "N2"),
                        inline("C6", "PCS-06"),
                        inline("D6", "S2"),
                        blank("H6"),
                        blank("I6"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{review}");
    assert_eq!(
        rows[0]["disposition"],
        "rfi.followUp.disposition.manualReview"
    );
    assert_eq!(
        rows[0]["reasonKey"],
        "warn.rfi.followUp.reason.mappingAmbiguous"
    );
}

#[test]
fn full_sync_keeps_ambiguous_mec_final_contexts_in_manual_review_when_one_exists() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            [
                full_schema_mec_header(),
                full_schema_mec_record(10, "0767", "MEC-1.3.5", "PCS-06", "S1", "N2", "Completed")
                    .replace(&blank("V10"), &inline("V10", "Signed by Worley")),
                full_schema_mec_record(11, "0767", "MEC-1.3.5", "PCS-06", "S1", "N2", "Completed")
                    .replace(&blank("V11"), &inline("V11", "Signed by Worley")),
            ],
            &[],
        ),
    );
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline("H3", "Final acceptance of pile installation (3.5)")],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "N2"),
                        inline("C5", "PCS-06"),
                        inline("D5", "S1"),
                        inline("H5", "MEC-0767"),
                        inline("I5", "Not Signed"),
                    ],
                ),
                row(
                    6,
                    [
                        inline("B6", "N2"),
                        inline("C6", "PCS-06"),
                        inline("D6", "S1"),
                        blank("H6"),
                        blank("I6"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert!(rows.iter().any(|row| {
        row["rfiNo"] == "MEC-0767"
            && row["disposition"] == "rfi.followUp.disposition.manualReview"
            && row["reasonKey"] == "warn.rfi.followUp.reason.mappingAmbiguous"
    }));
    assert!(
        !rows.iter().any(|row| {
            row["rfiNo"] == "MEC-0767"
                && (row["disposition"] == "rfi.followUp.disposition.ready"
                    || row["enabled"] == true
                    || row["proposedRfiNo"].is_string()
                    || row["proposedStatus"].is_string())
        }),
        "{review}"
    );
}

#[test]
fn full_sync_does_not_mark_a_shared_mec_worley_group_signed_from_one_rfi() {
    let signed = row(
        10,
        [
            inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0020"),
            inline("D10", "AZE-SUN-INT-QMS-ITP-MEC-0001"),
            inline("E10", "MEC-1.3.5"),
            inline("F10", "PCS-06"),
            inline("G10", "S1"),
            inline("N10", "N2"),
            inline("T10", "Completed"),
            inline("V10", "Signed by Worley"),
        ],
    );
    let unsigned =
        full_schema_mec_record(11, "0021", "MEC-1.3.5", "PCS-06", "S1", "N2", "Completed");
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet([full_schema_mec_header(), signed, unsigned], &[]),
    );
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline("H3", "Final acceptance of pile installation (3.5)")],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "N2"),
                        inline("C5", "PCS-06"),
                        inline("D5", "S1"),
                        inline("H5", "MEC-0020/0021"),
                        inline("I5", "Signed"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    assert_eq!(review["rows"].as_array().unwrap().len(), 1, "{review}");
    assert_eq!(review["rows"][0]["proposedStatus"], "Not Signed");
}

#[test]
fn full_sync_omits_a_required_final_rfi_without_a_prepared_context() {
    let mec = worksheet(
        [
            full_schema_mec_header(),
            full_schema_mec_record(10, "0001", "MEC-1.3.5", "PCS-01", "S1", "N1", "Completed"),
        ],
        &[],
    );
    let source = source_package(worksheet([full_schema_mec_header()], &[]), mec);
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline("H3", "Final acceptance of pile installation (3.5)")],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "OTHER"),
                        inline("C5", "PCS-OTHER"),
                        inline("D5", "S9"),
                        blank("H5"),
                        blank("I5"),
                    ],
                ),
            ],
            &[],
        ),
    )]);
    let review = classify(source, target).unwrap();

    assert!(
        !review["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["rfiNo"] == "MEC-0001"),
        "{review}"
    );
    assert!(
        !review["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["key"] == "warn.rfi.followUp.unmappedRecords"),
        "{review}"
    );
}
#[test]
fn conflicting_mec_final_routes_do_not_authorize_shared_cell_signature_writes() {
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            [
                full_schema_mec_header(),
                row(
                    10,
                    [
                        inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0790"),
                        inline("D10", "AZE-SUN-INT-QMS-ITP-MEC-0003"),
                        inline("E10", "MEC-1.3.1"),
                        inline("F10", "PCS-02"),
                        inline("G10", "S1"),
                        inline("N10", "N1"),
                        inline("T10", "Completed"),
                        inline("V10", "Signed by Worley"),
                    ],
                ),
                full_schema_mec_record_with_itp(
                    11,
                    "0790",
                    (
                        "AZE-SUN-INT-QMS-ITP-MEC-0004",
                        "MEC-1.3.2",
                        "PCS-02",
                        "S1",
                        "N1",
                        "Completed",
                    ),
                ),
                row(
                    12,
                    [
                        inline("A12", "AZE-SUN-INT-QMS-RFI-MEC-0791"),
                        inline("D12", "AZE-SUN-INT-QMS-ITP-MEC-0003"),
                        inline("E12", "MEC-1.3.1"),
                        inline("F12", "PCS-02"),
                        inline("G12", "S1"),
                        inline("N12", "N1"),
                        inline("T12", "Completed"),
                        inline("V12", "Signed by Worley"),
                    ],
                ),
            ],
            &[],
        ),
    );
    let target = package(&[(
        "Signed By Worley MEC",
        None,
        worksheet(
            [
                row(
                    3,
                    [inline(
                        "H3",
                        "Final acceptance of mechanical installation (3.1)",
                    )],
                ),
                row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")]),
                row(
                    5,
                    [
                        inline("B5", "N1"),
                        inline("C5", "PCS-02"),
                        inline("D5", "S1"),
                        inline("H5", "MEC-0790/0791"),
                        inline("I5", "Not Signed"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert!(rows.iter().any(|row| {
        row["rfiNo"] == "MEC-0790"
            && row["disposition"] == "rfi.followUp.disposition.manualReview"
            && row["reasonKey"] == "warn.rfi.followUp.reason.sourceStatusUnknown"
    }));
    assert!(
        !rows.iter().any(|row| {
            row["rfiNo"] == "MEC-0790"
                && (row["disposition"] == "rfi.followUp.disposition.ready"
                    || row["enabled"] == true
                    || row["proposedRfiNo"].is_string()
                    || row["proposedStatus"].is_string())
        }),
        "{review}"
    );
    assert!(
        review["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["key"] == "warn.rfi.followUp.unmappedRecords")
    );
}

#[test]
fn full_sync_keeps_formula_backed_missing_final_rfi_in_manual_review() {
    let source = full_schema_source([full_schema_record(
        10,
        "0001",
        "CIV-2.3.1",
        "AREA-1",
        "Signed by Worley",
    )]);
    let target = package(&[(
        "Signed By Worley CIV",
        None,
        worksheet(
            [
                row(4, [inline("C4", "RFI No."), inline("D4", "Status")]),
                row(
                    5,
                    [
                        inline("B5", "AREA-1"),
                        blank("C5"),
                        formula("D5", "LOOKUP(C5)", "Not Signed"),
                    ],
                ),
            ],
            &[],
        ),
    )]);

    let review = classify(source, target).unwrap();

    assert_eq!(review["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        review["rows"][0]["disposition"],
        "rfi.followUp.disposition.manualReview"
    );
    assert_eq!(
        review["rows"][0]["reasonKey"],
        "warn.rfi.followUp.reason.targetFormula"
    );
    assert_eq!(review["rows"][0]["enabled"], false);
}

#[test]
fn full_sync_ignores_formula_summary_in_lifecycle_comparison_and_mapping() {
    let header = full_schema_mec_header();
    let source_records = [
        full_schema_mec_record(10, "0001", "MEC-1.3.5", "PCS-01", "S1", "N1", "Completed")
            .replace(&blank("V10"), &inline("V10", "Signed by Worley")),
        full_schema_mec_record(11, "0001", "MEC-1.3.5", "PCS-01", "S2", "N1", "Completed")
            .replace(&blank("V11"), &inline("V11", "Signed by Worley")),
    ];
    let source = source_package(
        worksheet([header.clone()], &[]),
        worksheet(std::iter::once(header).chain(source_records), &[]),
    );
    let target = package(&[
        (
            "MEC",
            None,
            worksheet(
                [
                    row(4, [inline("N4", "RFI No."), inline("O4", "STATUS")]),
                    row(
                        5,
                        [
                            inline("A5", "Zone 1"),
                            inline("B5", "N1"),
                            inline("C5", "PCS-01"),
                            inline("D5", "S1"),
                            inline("N5", "MEC-0001"),
                            inline("O5", "Completed"),
                        ],
                    ),
                    row(
                        6,
                        [
                            inline("A6", "Zone 1"),
                            inline("B6", "N1"),
                            inline("C6", "PCS-01"),
                            inline("D6", "S2"),
                            inline("N6", "MEC-0001"),
                            inline("O6", "Completed"),
                        ],
                    ),
                ],
                &[],
            ),
        ),
        (
            "Signed By Worley MEC",
            None,
            worksheet(
                [
                    row(3, [inline("S3", "RFI No."), inline("T3", "Worley Status")]),
                    row(
                        4,
                        [
                            inline("H4", "RFI No."),
                            inline("I4", "STATUS"),
                            inline("S4", "MEC-0001"),
                            inline("T4", "Signed"),
                        ],
                    ),
                    row(
                        5,
                        [
                            inline("A5", "Zone 1"),
                            inline("B5", "N1"),
                            inline("C5", "PCS-01"),
                            inline("D5", "S1"),
                            inline("H5", "MEC-0001"),
                            formula("I5", "T4", "Signed"),
                        ],
                    ),
                    row(
                        6,
                        [
                            inline("A6", "Zone 1"),
                            inline("B6", "N1"),
                            inline("C6", "PCS-01"),
                            inline("D6", "S2"),
                            inline("H6", "MEC-0001"),
                            formula("I6", "T4", "Signed"),
                        ],
                    ),
                ],
                &[],
            ),
        ),
    ]);

    let review = classify(source, target).unwrap();

    assert!(review["rows"].as_array().unwrap().is_empty(), "{review}");
    assert!(
        review["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|warning| { warning["key"] != "warn.rfi.followUp.unmappedRecords" })
    );
}

#[test]
fn full_sync_omits_a_final_rfi_without_a_prepared_context() {
    let source = full_schema_source([full_schema_record(
        10,
        "0001",
        "CIV-2.3.1",
        "AREA-1",
        "Signed by Worley",
    )]);
    let target = package(&[(
        "Signed By Worley CIV",
        None,
        worksheet(
            [
                row(4, [inline("C4", "RFI No."), inline("D4", "Status")]),
                row(5, [inline("B5", "OTHER-AREA"), blank("C5"), blank("D5")]),
            ],
            &[],
        ),
    )]);
    let review = classify(source, target).unwrap();

    assert!(
        !review["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["rfiNo"] == "CIV-0001"),
        "{review}"
    );
    assert!(
        !review["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["key"] == "warn.rfi.followUp.unmappedRecords"),
        "{review}"
    );
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
                [row(10, [inline("A10", rfi), inline("T10", "Completed")])],
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
    guarded_updates(&plan.steps[0]).unwrap();

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
            [row(10, [inline("A10", rfi), inline("T10", "Completed")])],
            &[],
        ),
        worksheet(Vec::<String>::new(), &[]),
    )
    .into_inner();
    let original_target =
        target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]).into_inner();
    std::fs::write(&source_path, &source_bytes).unwrap();
    std::fs::write(&target_path, &original_target).unwrap();
    for index in 0..64 {
        std::fs::write(
            root.join(format!(".docflow-s5-backup-prior-{index}.xlsx")),
            b"retained recovery evidence",
        )
        .unwrap();
    }
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
        "Not Signed"
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
fn follow_up_sync_applies_more_than_five_hundred_guarded_cell_updates() {
    const RFI_COUNT: u32 = 501;

    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let source_path = root.join("RFI Log.xlsx");
    let target_path = root.join("Follow Up.xlsx");
    let journal_path = root.join("journals");
    let source_rows = (0..RFI_COUNT).map(|index| {
        let row_number = 10 + index;
        let rfi = format!("AZE-SUN-INT-QMS-RFI-CIV-{:04}", 1000 + index);
        row(
            row_number,
            [
                inline(&format!("A{row_number}"), &rfi),
                inline(&format!("T{row_number}"), "Completed"),
            ],
        )
    });
    let target_rows = (0..RFI_COUNT).map(|index| {
        let row_number = 2 + index;
        let rfi = format!("AZE-SUN-INT-QMS-RFI-CIV-{:04}", 1000 + index);
        row(
            row_number,
            [
                inline(&format!("A{row_number}"), &rfi),
                blank(&format!("B{row_number}")),
            ],
        )
    });
    let source_bytes = source_package(
        worksheet(source_rows, &[]),
        worksheet(Vec::<String>::new(), &[]),
    )
    .into_inner();
    let original_target = target_with_rows(target_rows, &[]).into_inner();
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
    let preview_rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(preview_rows.len(), RFI_COUNT as usize);
    assert!(preview_rows.iter().all(|row| row["enabled"] == true));

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

    assert_eq!(result.applied, RFI_COUNT as u64, "{:?}", result.errors);
    assert_eq!(result.failed, 0);
    assert_eq!(std::fs::read(&source_path).unwrap(), source_bytes);
    let (_, applied) = parsed(Cursor::new(std::fs::read(&target_path).unwrap()));
    let sheet = applied.sheet("Follow Up").unwrap();
    assert_eq!(sheet.cells[&(2, 2)].value, "Not Signed");
    assert_eq!(sheet.cells[&(RFI_COUNT + 1, 2)].value, "Not Signed");
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
            [row(10, [inline("A10", rfi), inline("T10", "Completed")])],
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
                [row(10, [inline("A10", rfi), inline("T10", "Completed")])],
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
                [row(10, [inline("A10", rfi), inline("T10", "Completed")])],
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
                [row(10, [inline("A10", rfi), inline("T10", "Completed")])],
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
fn canonical_rfi_requires_an_exact_supported_anchor_and_normalizes_numeric_suffixes() {
    assert_eq!(
        canonical_rfi("  aze-sun-int-qms-rfi-civ-0001  ").as_deref(),
        Some("AZE-SUN-INT-QMS-RFI-CIV-0001")
    );
    assert_eq!(
        canonical_rfi("AZE-SUN-INT-QMS-RFI-CIV-1").as_deref(),
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
        "AZE-SUN-INT-QMS-RFI-CIV-00010",
        "AZE-SUN-INT-QMS-RFI-CIV-0001 suffix",
        "ITP-CIV-1",
        "2026-09-01",
    ] {
        assert_eq!(canonical_rfi(rejected), None, "{rejected}");
    }
}

#[test]
fn short_follow_up_rfi_uses_worley_remark_and_preserves_short_display() {
    let full = "AZE-SUN-INT-QMS-RFI-CIV-0170";
    let review = classify(
        source_package(
            worksheet(
                [row(
                    10,
                    [inline("A10", full), inline("T10", "Signed by Worley")],
                )],
                &[],
            ),
            worksheet(Vec::<String>::new(), &[]),
        ),
        target_with_rows(
            [row(
                2,
                [inline("A2", "CIV-170"), inline("B2", "Not Signed")],
            )],
            &[],
        ),
    )
    .unwrap();

    assert_eq!(review["rows"][0]["rfiNo"], "CIV-0170");
    assert_eq!(review["rows"][0]["sourceStatuses"], json!(["Signed"]));
    assert_eq!(review["rows"][0]["currentStatus"], "Not Signed");
    assert_eq!(review["rows"][0]["proposedStatus"], "Signed");
    assert_eq!(review["rows"][0]["selectable"], true);
    assert_eq!(
        review["rows"][0]["reasonKey"],
        "warn.rfi.followUp.reason.readySigned"
    );
}

#[test]
fn follow_up_uses_short_rfi_identity_and_worley_signature_remark_only() {
    assert_eq!(
        canonical_follow_up_rfi("CIV-0170"),
        Some((
            "AZE-SUN-INT-QMS-RFI-CIV-0170".to_string(),
            "CIV-0170".to_string(),
        ))
    );
    assert_eq!(
        canonical_follow_up_rfi("AZE-SUN-INT-QMS-RFI-CIV-0170"),
        Some((
            "AZE-SUN-INT-QMS-RFI-CIV-0170".to_string(),
            "CIV-0170".to_string(),
        ))
    );
    assert_eq!(
        canonical_follow_up_rfi("CIV-170"),
        Some((
            "AZE-SUN-INT-QMS-RFI-CIV-0170".to_string(),
            "CIV-0170".to_string(),
        ))
    );
    for rejected in [
        "CIV-01700",
        "prefix CIV-0170",
        "CIV-0170 suffix",
        "ITP-CIV-170",
        "2026-09-01",
    ] {
        assert_eq!(canonical_follow_up_rfi(rejected), None, "{rejected}");
    }

    assert_eq!(mapped_follow_up_status("Signed by Worley"), "Signed");
    assert_eq!(
        mapped_follow_up_status("survey report completed\r\n\r\nSigned by Worley"),
        "Signed"
    );
    for not_signed in ["", "Completed", "Not Signed by Worley", "signed by worley"] {
        assert_eq!(mapped_follow_up_status(not_signed), "Not Signed");
    }
}

#[test]
fn source_duplicates_require_same_safe_status() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0006";
    let civ = worksheet(
        [row(
            10,
            [inline("A10", rfi), inline("T10", "Signed by Worley")],
        )],
        &[],
    );
    let same = worksheet(
        [row(
            10,
            [inline("A10", rfi), inline("U10", "Signed by Worley")],
        )],
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
    assert_eq!(ready["rows"][0]["sourceStatuses"], json!(["Signed"]));
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
        [row(10, [inline("A10", rfi), inline("U10", "Ongoing")])],
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
                inline(&format!("T{number}"), "Completed"),
            ],
        )
    });
    let mec = worksheet(
        [row(10, [inline("A10", rfi), inline("U10", "Completed")])],
        &[],
    );

    assert_eq!(
        classify(
            source_package(worksheet(civ_rows, &[]), mec),
            target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]),
        )
        .unwrap_err()
        .message
        .split(':')
        .next()
        .unwrap(),
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
                inline(&format!("T{number}"), "Completed"),
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
        .message
        .split(':')
        .next()
        .unwrap(),
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
                    inline(&format!("T{number}"), "Completed"),
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
        .message
        .split(':')
        .next()
        .unwrap(),
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
fn unique_follow_up_table_is_not_poisoned_by_a_duplicate_summary_layout() {
    let source = source_package(
        worksheet(Vec::<String>::new(), &[]),
        worksheet(Vec::<String>::new(), &[]),
    );
    let target = package(&[
        (
            "Summary",
            None,
            worksheet(
                [row(
                    1,
                    [
                        inline("A1", "RFI NO."),
                        inline("B1", "STATUS"),
                        inline("C1", "RFI NO."),
                        inline("D1", "STATUS"),
                    ],
                )],
                &[],
            ),
        ),
        (
            "Signed By Worley CIV",
            None,
            worksheet(
                [row(4, [inline("C4", "RFI NO."), inline("D4", "STATUS")])],
                &[],
            ),
        ),
    ]);

    let review = classify(source, target).unwrap();

    assert_eq!(
        review["layout"]["worksheet"]["name"],
        "Signed By Worley CIV"
    );
}

#[test]
fn production_template_physical_extent_within_twenty_thousand_rows_is_accepted() {
    let source = source_package(
        worksheet([row(18_913, Vec::<String>::new())], &[]),
        worksheet(Vec::<String>::new(), &[]),
    );

    classify(source, target_with_rows(Vec::<String>::new(), &[])).unwrap();
}

#[test]
fn empty_physical_high_row_enforces_source_limit() {
    let source = source_package(
        worksheet([row(20_010, Vec::<String>::new())], &[]),
        worksheet(Vec::<String>::new(), &[]),
    );

    let message = classify(source, target_with_rows(Vec::<String>::new(), &[]))
        .unwrap_err()
        .message;
    assert!(message.starts_with("error.rfi.followUp.limitExceeded: "));
    assert!(message.contains("reaches row 20010"), "{message}");
}

#[test]
fn classification_keeps_unsafe_rows_visible_but_unselectable() {
    let completed = "AZE-SUN-INT-QMS-RFI-CIV-0001";
    let unknown = "AZE-SUN-INT-QMS-RFI-CIV-0002";
    let controlled = "AZE-SUN-INT-QMS-RFI-CIV-0003";
    let source = source_package(
        worksheet(
            [
                row(10, [inline("A10", completed), inline("T10", "Completed")]),
                row(11, [inline("A11", unknown), inline("T11", "Cancelled")]),
                row(12, [inline("A12", controlled), inline("T12", "Completed")]),
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
            "warn.rfi.followUp.reason.readyNotSigned",
            "warn.rfi.followUp.reason.targetStatusUnknown",
            "warn.rfi.followUp.reason.blankRfi",
        ]
    );
    assert_eq!(review["rows"][3]["enabled"], true);
    for index in [1usize, 2, 4, 5] {
        assert_eq!(review["rows"][index]["enabled"], false);
        assert_eq!(review["rows"][index]["selectable"], false);
    }
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
                        formula("T10", "LOOKUP()", "Completed"),
                    ],
                ),
                row(
                    11,
                    [inline("A11", source_merged), inline("T11", "Completed")],
                ),
                row(
                    12,
                    [inline("A12", target_formula), inline("T12", "Completed")],
                ),
                row(
                    13,
                    [inline("A13", target_merged), inline("T13", "Completed")],
                ),
            ],
            &["T11:U11"],
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
                || blank(&format!("T{physical_row}")),
                |status| inline(&format!("T{physical_row}"), status),
            ));
            row(physical_row, cells)
        })
        .collect::<Vec<_>>();
    civ_rows.push(row(
        17,
        [
            inline("A17", "AZE-SUN-INT-QMS-RFI-CIV-0008"),
            formula("T17", "LOOKUP()", "Completed"),
        ],
    ));
    civ_rows.sort();
    source_package(
        worksheet(civ_rows, &["T18:U18"]),
        worksheet(
            [
                row(
                    10,
                    [
                        inline("A10", "AZE-SUN-INT-QMS-RFI-CIV-0006"),
                        inline("U10", "Completed"),
                    ],
                ),
                row(
                    11,
                    [
                        inline("A11", "AZE-SUN-INT-QMS-RFI-CIV-0007"),
                        inline("U11", "Ongoing"),
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
                [inline("A2", "CIV-0003"), inline("B2", "Not Signed")],
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
                [inline("A2", "CIV-0003"), inline("B2", "Not Signed")],
            )],
            &[],
        ),
    )
    .unwrap();
    assert_eq!(no_selection["warnings"], oracle["noSelectionWarnings"]);
    assert_eq!(no_selection["editor"]["requiredFields"], json!(["enabled"]));
}

#[test]
fn partial_discipline_source_package_without_all_four_sheets_is_accepted() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0001";
    let civ_sheet = worksheet(
        [row(
            10,
            [inline("A10", rfi), inline("T10", "Signed by Worley")],
        )],
        &[],
    );
    // Source package only has CIV sheet, no MEC, ELE, or PRC
    let source = package(&[("CIV", None, civ_sheet)]);
    let target = target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]);
    let classification = classify(source, target).unwrap();
    assert_eq!(classification["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        classification["rows"][0]["disposition"],
        "rfi.followUp.disposition.ready"
    );
    assert_eq!(classification["rows"][0]["proposedStatus"], "Signed");
}

#[test]
fn source_package_with_sheet_name_whitespace_and_case_is_accepted() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0001";
    let civ_sheet = worksheet(
        [row(10, [inline("A10", rfi), inline("T10", "Completed")])],
        &[],
    );
    // Source package has "Civ " with trailing space and mixed case
    let source = package(&[("Civ ", None, civ_sheet)]);
    let target = target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]);
    let classification = classify(source, target).unwrap();
    assert_eq!(classification["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        classification["rows"][0]["disposition"],
        "rfi.followUp.disposition.ready"
    );
}

#[test]
fn source_package_with_short_rfi_code_is_matched() {
    // Source sheet uses short code "CIV-0001" instead of full prefix
    let civ_sheet = worksheet(
        [row(
            10,
            [inline("A10", "CIV-0001"), inline("T10", "Signed by Worley")],
        )],
        &[],
    );
    let source = package(&[("CIV", None, civ_sheet)]);
    let target = target_with_rows([row(2, [inline("A2", "CIV-0001"), blank("B2")])], &[]);
    let classification = classify(source, target).unwrap();
    assert_eq!(classification["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        classification["rows"][0]["disposition"],
        "rfi.followUp.disposition.ready"
    );
    assert_eq!(classification["rows"][0]["proposedStatus"], "Signed");
}

#[test]
fn preview_from_open_files_accepts_uppercase_extension() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0001";
    let source_bytes = source_package(
        worksheet(
            [row(10, [inline("A10", rfi), inline("T10", "Completed")])],
            &[],
        ),
        worksheet(Vec::<String>::new(), &[]),
    );
    let target_bytes = target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]);
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("RFI_LOG.XLSX");
    let target_path = temp.path().join("FOLLOW_UP.XLSX");
    std::fs::write(&source_path, source_bytes.into_inner()).unwrap();
    std::fs::write(&target_path, target_bytes.into_inner()).unwrap();
    let mut source = File::options().read(true).open(&source_path).unwrap();
    let mut target = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();
    let plan =
        preview_from_open_files(&source_path, &target_path, &mut source, &mut target).unwrap();
    assert_eq!(plan.op_id, "rfi.follow_up_sync");
    assert!(!plan.steps.is_empty());
}

#[test]
fn source_package_with_single_rfi_log_sheet_is_accepted() {
    let rfi = "AZE-SUN-INT-QMS-RFI-CIV-0001";
    let log_sheet = worksheet(
        [
            row(
                1,
                [
                    inline("A1", "No"),
                    inline("B1", "RFI No"),
                    inline("C1", "Description"),
                    inline("D1", "Remarks"),
                ],
            ),
            row(
                2,
                [
                    inline("A2", "1"),
                    inline("B2", rfi),
                    inline("C2", "Inspection"),
                    inline("D2", "Signed by Worley"),
                ],
            ),
        ],
        &[],
    );
    let source = package(&[("RFI Log", None, log_sheet)]);
    let target = target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]);
    let classification = classify(source, target).unwrap();
    assert_eq!(classification["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        classification["rows"][0]["disposition"],
        "rfi.followUp.disposition.ready"
    );
    assert_eq!(classification["rows"][0]["proposedStatus"], "Signed");
}

#[test]
fn source_package_with_shifted_rfi_column_in_discipline_sheet_is_accepted() {
    let rfi = "CIV-0002";
    let civ_sheet = worksheet(
        [
            row(
                1,
                [
                    inline("A1", "Item"),
                    inline("B1", "RFI Code"),
                    inline("T1", "Remarks"),
                ],
            ),
            row(
                2,
                [
                    inline("A2", "1"),
                    inline("B2", rfi),
                    inline("T2", "Signed by Worley"),
                ],
            ),
        ],
        &[],
    );
    let source = package(&[("CIVIL WORKS", None, civ_sheet)]);
    let target = target_with_rows([row(2, [inline("A2", rfi), blank("B2")])], &[]);
    let classification = classify(source, target).unwrap();
    assert_eq!(classification["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        classification["rows"][0]["disposition"],
        "rfi.followUp.disposition.ready"
    );
    assert_eq!(classification["rows"][0]["proposedStatus"], "Signed");
}

#[test]
fn multi_sheet_follow_up_with_civ_and_mec_is_synchronized() {
    let civ_rfi = "CIV-0001";
    let mec_rfi = "MEC-0005";

    let source = package(&[
        (
            "CIV",
            None,
            worksheet(
                [
                    row(1, [inline("A1", "RFI NO"), inline("T1", "Remarks")]),
                    row(2, [inline("A2", civ_rfi), inline("T2", "Signed by Worley")]),
                ],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [
                    row(1, [inline("A1", "RFI NO"), inline("U1", "Remarks")]),
                    row(2, [inline("A2", mec_rfi), inline("U2", "Under review")]),
                ],
                &[],
            ),
        ),
    ]);

    let target = package(&[
        (
            "Signed by CIV",
            None,
            worksheet(
                [
                    row(1, [inline("A1", "RFI NO."), inline("B1", "STATUS")]),
                    row(2, [inline("A2", civ_rfi), blank("B2")]),
                ],
                &[],
            ),
        ),
        (
            "Signed by MEC",
            None,
            worksheet(
                [
                    row(1, [inline("C1", "RFI NO."), inline("D1", "STATUS")]),
                    row(2, [inline("C2", mec_rfi), blank("D2")]),
                ],
                &[],
            ),
        ),
    ]);

    let classification = classify(source.clone(), target.clone()).unwrap();
    let rows = classification["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);

    assert_eq!(rows[0]["_sheet"], "Signed by CIV");
    assert_eq!(rows[0]["rfiNo"], civ_rfi);
    assert_eq!(rows[0]["proposedStatus"], "Signed");
    assert_eq!(rows[0]["disposition"], "rfi.followUp.disposition.ready");

    assert_eq!(rows[1]["_sheet"], "Signed by MEC");
    assert_eq!(rows[1]["rfiNo"], mec_rfi);
    assert_eq!(rows[1]["proposedStatus"], "Not Signed");
    assert_eq!(rows[1]["disposition"], "rfi.followUp.disposition.ready");

    let fixture = tempfile::tempdir().unwrap();
    let source_path = fixture.path().join("RFI Log.xlsx");
    let target_path = fixture.path().join("Follow Up.xlsx");
    std::fs::write(&source_path, source.into_inner()).unwrap();
    std::fs::write(&target_path, target.into_inner()).unwrap();

    let mut source_file = File::open(&source_path).unwrap();
    let mut target_file = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();

    let plan = preview_from_open_files(
        &source_path,
        &target_path,
        &mut source_file,
        &mut target_file,
    )
    .unwrap();
    assert_eq!(plan.steps.len(), 1);

    let updates = guarded_updates(&plan.steps[0]).unwrap();
    assert!(updates.contains_key("Signed by CIV"));
    assert!(updates.contains_key("Signed by MEC"));
    assert_eq!(updates["Signed by CIV"].len(), 1);
    assert_eq!(updates["Signed by MEC"].len(), 1);
}

#[test]
fn multi_sheet_follow_up_prioritizes_signed_by_sheets_over_arbitrary_sheets() {
    let civ_rfi = "CIV-0001";
    let source = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(1, [inline("A1", "RFI NO"), inline("T1", "Remarks")]),
                row(2, [inline("A2", civ_rfi), inline("T2", "Signed by Worley")]),
            ],
            &[],
        ),
    )]);

    let target = package(&[
        (
            "Arbitrary Notes",
            None,
            worksheet(
                [row(1, [inline("A1", "RFI NO."), inline("B1", "STATUS")])],
                &[],
            ),
        ),
        (
            "Signed by CIV",
            None,
            worksheet(
                [
                    row(4, [inline("C4", "RFI NO."), inline("D4", "STATUS")]),
                    row(5, [inline("C5", civ_rfi), blank("D5")]),
                ],
                &[],
            ),
        ),
    ]);

    let classification = classify(source, target).unwrap();
    let rows = classification["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["_sheet"], "Signed by CIV");
    assert_eq!(rows[0]["rfiNo"], civ_rfi);
    assert_eq!(rows[0]["proposedStatus"], "Signed");
}
#[test]
fn placeholder_2000_series_rfis_are_preserved_without_sourcemissing_warnings() {
    assert!(docflow_lib::rfi_follow_up::is_placeholder_rfi("CIV-2007"));
    assert!(docflow_lib::rfi_follow_up::is_placeholder_rfi("MEC-2183"));
    assert!(!docflow_lib::rfi_follow_up::is_placeholder_rfi("CIV-0170"));
    assert!(!docflow_lib::rfi_follow_up::is_placeholder_rfi("MEC-0330"));
}

#[test]
fn road_name_matches_by_exact_tag_without_requiring_zone_match() {
    let source_full = full_schema_source([full_schema_record_with_status(
        10,
        "0001",
        "AZE-SUN-INT-QMS-ITP-CIV-0004",
        "CIV-4.2.4",
        "Road 14 (Part 1)",
        "Ongoing",
        "",
    )]);
    let target = package(&[
        (
            "CIV",
            None,
            worksheet(
                [row(4, [inline("C4", "RFI No."), inline("D4", "STATUS")])],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")])],
                &[],
            ),
        ),
        (
            "Road",
            None,
            worksheet(
                [
                    row(
                        4,
                        [
                            inline("B4", "ROAD No."),
                            inline("K4", "RFI No."),
                            inline("L4", "STATUS"),
                        ],
                    ),
                    row(
                        5,
                        [
                            inline("A5", "South"),
                            inline("B5", "Road 14 (Part 1)"),
                            inline("K5", ""),
                            inline("L5", ""),
                        ],
                    ),
                ],
                &[],
            ),
        ),
    ]);
    let classification = classify(source_full, target).unwrap();
    let rows = classification["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["_sheet"], "Road");
    assert_eq!(rows[0]["_row"], 5);
    assert_eq!(rows[0]["proposedRfiNo"], "CIV-0001");
    assert_eq!(rows[0]["proposedStatus"], "In Progress");
}
#[test]
fn multi_section_mec_signed_rfi_uses_section_and_block_without_location() {
    let mec_row = row(
        10,
        [
            inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0001"),
            inline("D10", "AZE-SUN-INT-QMS-ITP-MEC-0003"),
            inline("E10", "MEC-3.3.1"),
            inline("F10", "PCS-10"),
            inline("G10", "S1, S3"),
            inline("N10", "N99"),
            inline("T10", "Completed"),
            inline("V10", "Signed by Worley"),
        ],
    );
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            [
                full_schema_mec_header(),
                mec_row,
                // Already numbered in the Log, so Follow Up's MEC-0558 is not a deleted RFI.
                row(11, [inline("A11", "AZE-SUN-INT-QMS-RFI-MEC-0558")]),
            ],
            &[],
        ),
    );
    let target = package(&[
        (
            "CIV",
            None,
            worksheet(
                [row(4, [inline("C4", "RFI No."), inline("D4", "STATUS")])],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")])],
                &[],
            ),
        ),
        (
            "Signed By Worley MEC",
            None,
            worksheet(
                [
                    row(
                        4,
                        [
                            inline("C4", "Block #"),
                            inline("D4", "Section"),
                            inline("J4", "RFI No."),
                            inline("K4", "STATUS"),
                        ],
                    ),
                    row(
                        5,
                        [
                            inline("B5", "N1"),
                            inline("C5", "PCS-10"),
                            inline("D5", "S1"),
                            inline("J5", ""),
                            inline("K5", ""),
                        ],
                    ),
                    row(
                        6,
                        [
                            inline("C6", "PCS-10"),
                            inline("D6", "S2"),
                            inline("J6", "MEC-0558"),
                            inline("K6", "Signed"),
                        ],
                    ),
                    row(
                        7,
                        [
                            inline("B7", "N1"),
                            inline("C7", "PCS-10"),
                            inline("D7", "S3"),
                            inline("J7", ""),
                            inline("K7", ""),
                        ],
                    ),
                ],
                &[],
            ),
        ),
    ]);
    let classification = classify(source, target).unwrap();
    let warnings = classification["warnings"].as_array().unwrap();
    assert!(
        warnings.is_empty(),
        "expected 0 warnings, got {:?}",
        warnings
    );
    let rows = classification["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["_sheet"], "Signed By Worley MEC");
    assert_eq!(rows[0]["_row"], 5);
    assert_eq!(rows[0]["proposedRfiNo"], "MEC-0001");
    assert_eq!(rows[0]["proposedStatus"], "Signed");
    assert_eq!(rows[1]["_sheet"], "Signed By Worley MEC");
    assert_eq!(rows[1]["_row"], 7);
    assert_eq!(rows[1]["proposedRfiNo"], "MEC-0001");
    assert_eq!(rows[1]["proposedStatus"], "Signed");
}

#[test]
fn full_sync_maps_signed_mec_without_inspection_location_columns() {
    let mec_row = row(
        10,
        [
            inline("A10", "AZE-SUN-INT-QMS-RFI-MEC-0001"),
            inline("D10", "AZE-SUN-INT-QMS-ITP-MEC-0003"),
            inline("E10", "MEC-3.3.1"),
            inline("F10", "PCS-10"),
            inline("G10", "S1, S3"),
            inline("T10", "Completed"),
            inline("V10", "Signed by Worley"),
        ],
    );
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet([full_schema_mec_header(), mec_row], &[]),
    );
    let target = package(&[
        (
            "MEC",
            None,
            worksheet(
                [row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")])],
                &[],
            ),
        ),
        (
            "Signed By Worley MEC",
            None,
            worksheet(
                [
                    row(
                        4,
                        [
                            inline("C4", "Block #"),
                            inline("D4", "Section"),
                            inline("J4", "RFI No."),
                            inline("K4", "STATUS"),
                        ],
                    ),
                    row(
                        5,
                        [
                            inline("C5", "PCS-10"),
                            inline("D5", "S1"),
                            inline("J5", ""),
                            inline("K5", ""),
                        ],
                    ),
                    row(
                        6,
                        [
                            inline("C6", "PCS-10"),
                            inline("D6", "S3"),
                            inline("J6", ""),
                            inline("K6", ""),
                        ],
                    ),
                ],
                &[],
            ),
        ),
    ]);
    let review = classify(source, target).unwrap();
    let rows = review["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{review}");
    assert!(rows.iter().any(|row| {
        row["_sheet"] == "Signed By Worley MEC"
            && row["_row"] == 5
            && row["rfiNo"] == "MEC-0001"
            && row["proposedStatus"] == "Signed"
    }));
    assert!(rows.iter().any(|row| {
        row["_sheet"] == "Signed By Worley MEC"
            && row["_row"] == 6
            && row["rfiNo"] == "MEC-0001"
            && row["proposedStatus"] == "Signed"
    }));
}
#[test]
fn rfis_of_different_items_or_itps_are_strictly_isolated_and_never_merge_into_same_slot() {
    let tracker_rfi = "AZE-SUN-INT-QMS-RFI-MEC-0370";
    let module_rfi = "AZE-SUN-INT-QMS-RFI-MEC-0530";
    let source = source_package(
        worksheet([full_schema_mec_header()], &[]),
        worksheet(
            [
                full_schema_mec_header(),
                row(
                    10,
                    [
                        inline("A10", tracker_rfi),
                        inline("D10", "AZE-SUN-INT-QMS-ITP-MEC-0003"),
                        inline("E10", "MEC-3.3.1"),
                        inline("F10", "PCS-02"),
                        inline("G10", "S1"),
                        inline("T10", "Completed"),
                        inline("V10", "Signed by Worley"),
                    ],
                ),
                row(
                    11,
                    [
                        inline("A11", module_rfi),
                        inline("D11", "AZE-SUN-INT-QMS-ITP-MEC-0004"),
                        inline("E11", "MEC-4.3.2"),
                        inline("F11", "PCS-02"),
                        inline("G11", "S1"),
                        inline("T11", "Completed"),
                        inline("V11", "Signed by Worley"),
                    ],
                ),
            ],
            &[],
        ),
    );
    let target = package(&[
        (
            "CIV",
            None,
            worksheet(
                [row(4, [inline("C4", "RFI No."), inline("D4", "STATUS")])],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [
                    row(
                        4,
                        [
                            inline("C4", "Block #"),
                            inline("D4", "Section"),
                            inline("P4", "RFI No."),
                            inline("Q4", "STATUS"),
                            inline("R4", "RFI No."),
                            inline("S4", "STATUS"),
                        ],
                    ),
                    row(
                        5,
                        [
                            inline("C5", "PCS-02"),
                            inline("D5", "S1"),
                            inline("P5", "MEC-0370"),
                            inline("Q5", "Completed"),
                            inline("R5", "MEC-0530"),
                            inline("S5", "Completed"),
                        ],
                    ),
                ],
                &[],
            ),
        ),
    ]);
    let classification = classify(source, target).unwrap();
    let rows = classification["rows"].as_array().unwrap();
    assert!(rows.iter().all(|row| {
        row["disposition"] != "rfi.followUp.disposition.ready"
            && row["proposedRfiNo"].is_null()
            && row["proposedStatus"].is_null()
    }));
    assert!(
        classification["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["key"] == "warn.rfi.followUp.unmappedRecords")
    );
}

#[test]
fn large_follow_up_with_more_than_five_thousand_changes_previews_within_the_byte_budget() {
    let count = 8_406;
    let code = |n: usize| format!("AZE-SUN-INT-QMS-RFI-CIV-{n:04}");
    let source_rows = (1..=count).map(|n| {
        let r = n as u32 + 9;
        row(
            r,
            [
                inline(&format!("A{r}"), &code(n)),
                inline(&format!("T{r}"), "Completed"),
            ],
        )
    });
    let target_rows = (1..=count).map(|n| {
        let r = n as u32 + 1;
        row(
            r,
            [inline(&format!("A{r}"), &code(n)), blank(&format!("B{r}"))],
        )
    });
    let fixture = tempfile::tempdir().unwrap();
    let source_path = fixture.path().join("RFI Log.xlsx");
    let target_path = fixture.path().join("Follow Up.xlsx");
    std::fs::write(
        &source_path,
        source_package(
            worksheet(source_rows, &[]),
            worksheet(Vec::<String>::new(), &[]),
        )
        .into_inner(),
    )
    .unwrap();
    std::fs::write(
        &target_path,
        target_with_rows(target_rows, &[]).into_inner(),
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

    assert_eq!(plan.meta["editor"]["rows"].as_array().unwrap().len(), count);
    assert_eq!(
        guarded_updates(&plan.steps[0])
            .unwrap()
            .values()
            .map(|u| u.len())
            .sum::<usize>(),
        count
    );
    let bytes = serde_json::to_vec(&plan).unwrap().len();
    eprintln!("PLAN_BYTES {bytes}");
    assert!(bytes < 32 * 1024 * 1024);
}

fn number_only_record(row_number: u32, suffix: &str) -> String {
    row(
        row_number,
        [inline(
            &format!("A{row_number}"),
            &format!("AZE-SUN-INT-QMS-RFI-CIV-{suffix}"),
        )],
    )
}

fn civ_template_target() -> Cursor<Vec<u8>> {
    package(&[
        (
            "MEC",
            None,
            worksheet(
                [row(5, [inline("A5", "Zone 4"), inline("C5", "PCS-19")])],
                &[],
            ),
        ),
        (
            "CIV",
            None,
            worksheet(
                [
                    row(1, [inline("I1", "RFI No."), inline("J1", "STATUS")]),
                    row(
                        20,
                        [
                            inline("A20", "Zone 4"),
                            inline("B20", "S1"),
                            inline("I20", ""),
                            inline("J20", ""),
                        ],
                    ),
                ],
                &[],
            ),
        ),
        ("PCU", None, pcu_target("", "", "")),
    ])
}

#[test]
fn number_only_rfi_log_rows_are_not_records_and_raise_no_manual_review() {
    let source = full_schema_source([
        number_only_record(10, "2001"),
        number_only_record(11, "2002"),
        number_only_record(12, "2003"),
    ]);

    let review = classify(source, civ_template_target()).unwrap();

    assert!(review["rows"].as_array().unwrap().is_empty());
}

#[test]
fn rfi_log_row_with_a_status_but_no_itp_stays_visible_for_review() {
    let source = full_schema_source([row(
        10,
        [
            inline("A10", "AZE-SUN-INT-QMS-RFI-CIV-2004"),
            inline("T10", "Completed"),
        ],
    )]);

    let review = classify(source, civ_template_target()).unwrap();

    assert_eq!(review["rows"].as_array().unwrap().len(), 1);
}

fn road_source(rfis: &[(&str, &str, &str)]) -> Cursor<Vec<u8>> {
    // (suffix, tag, status), every record is ITP CIV-0004 item 4.2.4.
    full_schema_source(
        rfis.iter()
            .enumerate()
            .map(|(index, (suffix, tag, status))| {
                full_schema_record_with_status(
                    10 + index as u32,
                    suffix,
                    "AZE-SUN-INT-QMS-ITP-CIV-0004",
                    "CIV-4.2.4",
                    tag,
                    status,
                    "",
                )
            }),
    )
}

fn road_target(road_cells: &[&str]) -> Cursor<Vec<u8>> {
    let rows = road_cells.iter().enumerate().map(|(index, cell)| {
        let r = 5 + index as u32;
        row(
            r,
            [
                inline(&format!("A{r}"), "North"),
                inline(&format!("B{r}"), cell),
                inline(&format!("K{r}"), ""),
                inline(&format!("L{r}"), ""),
            ],
        )
    });
    let header = row(
        4,
        [
            inline("B4", "ROAD No."),
            inline("K4", "RFI No."),
            inline("L4", "STATUS"),
        ],
    );
    package(&[
        (
            "CIV",
            None,
            worksheet(
                [row(4, [inline("C4", "RFI No."), inline("D4", "STATUS")])],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")])],
                &[],
            ),
        ),
        (
            "Road",
            None,
            worksheet(std::iter::once(header).chain(rows), &[]),
        ),
    ])
}

fn road_rows(review: &Value) -> Vec<(u64, String)> {
    review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            row["_sheet"] == "Road" && row["selectable"] == true && row["proposedRfiNo"].is_string()
        })
        .map(|row| {
            (
                row["_row"].as_u64().unwrap(),
                row["proposedRfiNo"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn road_rfi_without_a_part_goes_to_every_part_row_of_that_road() {
    let review = classify(
        road_source(&[("0001", "Road 06", "Ongoing")]),
        road_target(&[
            "Road 06 (Part 1)",
            "Road 06 (Part 2)",
            "Road 07 (Part 1)",
            "Road 06 (Part 3)",
        ]),
    )
    .unwrap();

    assert_eq!(
        road_rows(&review),
        [
            (5, "CIV-0001".to_string()),
            (6, "CIV-0001".to_string()),
            (8, "CIV-0001".to_string())
        ]
    );
}

#[test]
fn road_rfi_with_a_part_goes_only_to_that_part_row() {
    let review = classify(
        road_source(&[("0002", "Road 06 (Part 2)", "Completed")]),
        road_target(&["Road 06 (Part 1)", "Road 06 (Part 2)", "Road 06 (Part 3)"]),
    )
    .unwrap();

    assert_eq!(road_rows(&review), [(6, "CIV-0002".to_string())]);
}

#[test]
fn road_rfi_listing_several_parts_goes_to_each_listed_part() {
    for tag in [
        "Road 15 (Part 1), (Part 2)",
        "Road 15 (Part 1),Road 15 (Part 2)",
    ] {
        let review = classify(
            road_source(&[("0003", tag, "Ongoing")]),
            road_target(&["Road 15 (Part 1)", "Road 15 (Part 2)", "Road 15 (Part 3)"]),
        )
        .unwrap();

        assert_eq!(
            road_rows(&review),
            [(5, "CIV-0003".to_string()), (6, "CIV-0003".to_string())],
            "{tag}"
        );
    }
}

#[test]
fn road_rfi_names_a_part_with_no_row_as_partial_context_and_still_syncs_the_rest() {
    let review = classify(
        road_source(&[("0004", "Road 06 (Part 1), Road 06 (Part 9)", "Ongoing")]),
        road_target(&["Road 06 (Part 1)", "Road 06 (Part 2)"]),
    )
    .unwrap();

    assert_eq!(road_rows(&review), [(5, "CIV-0004".to_string())]);
    assert!(
        review["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| { row["reasonKey"] == "warn.rfi.followUp.reason.partialContext" })
    );
}

#[test]
fn partless_road_row_still_matches_a_partless_tag() {
    let review = classify(
        road_source(&[("0005", "Road 02", "Completed")]),
        road_target(&["Road 02", "Road 03"]),
    )
    .unwrap();

    assert_eq!(road_rows(&review), [(5, "CIV-0005".to_string())]);
}

fn deleted_rfi_review(extra_log_rows: Vec<String>, target_cell: &str) -> Value {
    let mut civ = vec![full_schema_record_with_status(
        10,
        "1222",
        "AZE-SUN-INT-QMS-ITP-CIV-0002",
        "CIV-1.5",
        "AREA-1",
        "Completed",
        "",
    )];
    civ.extend(extra_log_rows);
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(
                    1,
                    [
                        inline("B1", "Tag"),
                        inline("C1", "RFI No."),
                        inline("D1", "STATUS"),
                    ],
                ),
                row(
                    2,
                    [
                        inline("B2", "AREA-1"),
                        inline("C2", target_cell),
                        inline("D2", "In Progress"),
                    ],
                ),
            ],
            &[],
        ),
    )]);
    classify(full_schema_source(civ), target).unwrap()
}

fn proposed_rfis(review: &Value) -> Vec<String> {
    review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["proposedRfiNo"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn rfi_missing_from_the_whole_log_is_removed_from_the_follow_up_cell() {
    let review = deleted_rfi_review(Vec::new(), "CIV-1222/0099");

    assert_eq!(proposed_rfis(&review), ["CIV-1222"]);
    assert_eq!(review["rows"][0]["proposedStatus"], "Completed");
    assert_eq!(review["rows"][0]["selectable"], true);
}

#[test]
fn rfi_that_is_only_numbered_in_the_log_is_not_treated_as_deleted() {
    let review = deleted_rfi_review(
        vec![row(11, [inline("A11", "AZE-SUN-INT-QMS-RFI-CIV-0098")])],
        "CIV-1222/0098",
    );

    assert!(
        !proposed_rfis(&review).contains(&"CIV-1222".to_string()),
        "{review}"
    );
}

#[test]
fn placeholder_rfi_numbers_are_never_removed_as_deleted() {
    let review = deleted_rfi_review(Vec::new(), "CIV-1222/2001");

    assert!(
        !proposed_rfis(&review).contains(&"CIV-1222".to_string()),
        "{review}"
    );
}

fn remark_target(rfi_cell: &str, status: &str, remark: &str) -> Cursor<Vec<u8>> {
    package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(
                    1,
                    [
                        inline("B1", "Tag"),
                        inline("C1", "RFI No."),
                        inline("D1", "STATUS"),
                        inline("Q1", "Remarks"),
                    ],
                ),
                row(
                    2,
                    [
                        inline("B2", "AREA-1"),
                        inline("C2", rfi_cell),
                        inline("D2", status),
                        inline("Q2", remark),
                    ],
                ),
            ],
            &[],
        ),
    )])
}

fn remark_source(statuses: &[(&str, &str)]) -> Cursor<Vec<u8>> {
    full_schema_source(
        statuses
            .iter()
            .enumerate()
            .map(|(index, (suffix, status))| {
                full_schema_record_with_status(
                    10 + index as u32,
                    suffix,
                    "AZE-SUN-INT-QMS-ITP-CIV-0002",
                    "CIV-1.5",
                    "AREA-1",
                    status,
                    "",
                )
            }),
    )
}

fn remark_row(review: &Value) -> Option<&Value> {
    review["rows"].as_array().unwrap().iter().find(|row| {
        row["remark"].is_object()
            || row["reasonKey"]
                .as_str()
                .is_some_and(|key| key.contains("remark"))
    })
}

fn remark_text(row: &Value) -> String {
    row["remark"]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["text"].as_str().unwrap())
        .collect()
}

#[test]
fn mixed_status_cell_gets_a_completed_first_remark() {
    let review = classify(
        remark_source(&[("1223", "Ongoing"), ("1222", "Completed")]),
        remark_target("CIV-1222/1223", "In Progress", ""),
    )
    .unwrap();

    let row = remark_row(&review).expect("remark row");
    assert_eq!(row["disposition"], "rfi.followUp.disposition.ready");
    assert_eq!(row["selectable"], true);
    assert_eq!(remark_text(row), "CIV-1222 Completed CIV-1223 In Progress");
    let runs = row["remark"]["runs"].as_array().unwrap();
    assert_eq!(runs[0]["emphasized"], true);
    assert_eq!(runs[1]["emphasized"], false);
    assert_eq!(row["remark"]["column"], 17);
}

#[test]
fn single_rfi_cell_gets_no_remark() {
    let review = classify(
        remark_source(&[("1222", "Ongoing")]),
        remark_target("CIV-1222", "In Progress", ""),
    )
    .unwrap();

    assert!(remark_row(&review).is_none(), "{review}");
}

#[test]
fn generated_remark_is_cleared_once_every_rfi_in_the_cell_is_completed() {
    let review = classify(
        remark_source(&[("1222", "Completed"), ("1223", "Completed")]),
        remark_target(
            "CIV-1222/1223",
            "Completed",
            "CIV-1222 Completed CIV-1223 In Progress",
        ),
    )
    .unwrap();

    let row = remark_row(&review).expect("clear row");
    assert_eq!(
        row["reasonKey"],
        "warn.rfi.followUp.reason.readyRemarkClear"
    );
    assert_eq!(row["remark"]["runs"].as_array().unwrap().len(), 0);
    assert_eq!(row["selectable"], true);
}

#[test]
fn operator_notes_are_never_replaced_or_cleared() {
    for (statuses, cell) in [
        (
            vec![("1222", "Completed"), ("1223", "Ongoing")],
            "CIV-1222/1223",
        ),
        (
            vec![("1222", "Completed"), ("1223", "Completed")],
            "CIV-1222/1223",
        ),
    ] {
        let review = classify(
            remark_source(&statuses),
            remark_target(cell, "In Progress", "operator note"),
        )
        .unwrap();

        let touched = remark_row(&review);
        assert!(
            touched.is_none_or(|row| row["selectable"] == false && row["remark"].is_null()),
            "{review}"
        );
    }
}

#[test]
fn up_to_date_generated_remark_is_left_alone() {
    let review = classify(
        remark_source(&[("1222", "Completed"), ("1223", "Ongoing")]),
        remark_target(
            "CIV-1222/1223",
            "In Progress",
            "CIV-1222 Completed CIV-1223 In Progress",
        ),
    )
    .unwrap();

    assert!(remark_row(&review).is_none(), "{review}");
}

#[test]
fn remark_is_written_as_rich_text_through_apply_and_undone_exactly() {
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let source_path = root.join("RFI Log.xlsx");
    let target_path = root.join("Follow Up.xlsx");
    let source_bytes = remark_source(&[("1222", "Completed"), ("1223", "Ongoing")]).into_inner();
    let original_target = remark_target("CIV-1222/1223", "Pending", "").into_inner();
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
    assert!(plan.steps[0].meta["remarks"].as_array().unwrap().len() == 1);

    let authority = FilesystemAuthority::new(root.join("journals"), Duration::from_secs(300));
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
    assert_eq!(applied.applied, 2, "{:?}", applied.errors);

    let mut written = File::open(&target_path).unwrap();
    let workbook = read_workbook_from_reader(&mut written).unwrap();
    assert_eq!(
        workbook.sheet("CIV").unwrap().value(2, 17),
        "CIV-1222 Completed CIV-1223 In Progress"
    );
    assert_eq!(workbook.sheet("CIV").unwrap().value(2, 4), "In Progress");

    let undone = authority.undo(&applied.journal_id, &caller, None).unwrap();
    assert_eq!(undone.failed, 0);
    assert_eq!(std::fs::read(&target_path).unwrap(), original_target);
}

fn road_target_with_formulas(road_cells: &[&str]) -> Cursor<Vec<u8>> {
    let rows = road_cells.iter().enumerate().map(|(index, cell)| {
        let r = 5 + index as u32;
        row(
            r,
            [
                inline(&format!("A{r}"), "South"),
                inline(&format!("B{r}"), cell),
                inline(&format!("K{r}"), ""),
                inline(&format!("L{r}"), ""),
                formula(
                    &format!("Y{r}"),
                    &format!("COUNTIF(K{r}:L{r},\"Completed\")/6"),
                    "0",
                ),
            ],
        )
    });
    let last = 4 + road_cells.len();
    let summary = row(3, [formula("Z3", &format!("AVERAGE(Y5:Y{last})"), "0")]);
    let header = row(
        4,
        [
            inline("B4", "ROAD No."),
            inline("K4", "RFI No."),
            inline("L4", "STATUS"),
        ],
    );
    package(&[
        (
            "CIV",
            None,
            worksheet(
                [row(4, [inline("C4", "RFI No."), inline("D4", "STATUS")])],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")])],
                &[],
            ),
        ),
        (
            "Road",
            None,
            worksheet(
                std::iter::once(summary)
                    .chain(std::iter::once(header))
                    .chain(rows),
                &[],
            ),
        ),
    ])
}

fn structure_reasons(review: &Value) -> Vec<(String, String)> {
    review["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            row["reasonKey"]
                .as_str()
                .is_some_and(|key| key.contains("reason.structure"))
        })
        .map(|row| {
            (
                row["reasonKey"].as_str().unwrap().to_string(),
                row["rfiNo"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn road_preview(
    root: &std::path::Path,
    source: Cursor<Vec<u8>>,
    target: Cursor<Vec<u8>>,
) -> (
    docflow_lib::contracts::Plan,
    std::path::PathBuf,
    std::path::PathBuf,
    Vec<u8>,
) {
    let source_path = root.join("RFI Log.xlsx");
    let target_path = root.join("Follow Up.xlsx");
    let target_bytes = target.into_inner();
    std::fs::write(&source_path, source.into_inner()).unwrap();
    std::fs::write(&target_path, &target_bytes).unwrap();
    let mut source_file = File::open(&source_path).unwrap();
    let mut target_file = File::options()
        .read(true)
        .write(true)
        .open(&target_path)
        .unwrap();
    let plan = preview_from_open_files(
        &source_path,
        &target_path,
        &mut source_file,
        &mut target_file,
    )
    .unwrap();
    (plan, source_path, target_path, target_bytes)
}

fn plan_rows(plan: &docflow_lib::contracts::Plan) -> Value {
    json!({"rows": plan.meta["editor"]["rows"].clone()})
}

#[test]
fn partless_road_row_is_split_into_parts_when_the_log_names_a_part() {
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let (plan, ..) = road_preview(
        &root,
        road_source(&[
            ("0001", "Road 18", "Ongoing"),
            ("0002", "Road 18 (Part 2)", "Completed"),
        ]),
        road_target_with_formulas(&["Road 17", "Road 18", "Road 19"]),
    );
    let review = plan_rows(&plan);

    let road_18: Vec<_> = structure_reasons(&review)
        .into_iter()
        .filter(|(_, label)| label.starts_with("Road 18"))
        .collect();
    assert_eq!(
        road_18,
        [
            (
                "warn.rfi.followUp.reason.structureInsert".to_string(),
                "Road 18 (Part 2)".to_string()
            ),
            (
                "warn.rfi.followUp.reason.structureRename".to_string(),
                "Road 18 (Part 1)".to_string()
            ),
        ]
    );
    let mut codes: Vec<String> = road_rows(&review)
        .into_iter()
        .map(|(_, code)| code)
        .collect();
    codes.sort();
    assert_eq!(codes, ["CIV-0001", "CIV-0001/0002"]);
}

#[test]
fn road_structure_is_applied_through_the_guarded_pipeline_and_undone_exactly() {
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let (plan, _, target_path, original) = road_preview(
        &root,
        road_source(&[
            ("0001", "Road 18", "Ongoing"),
            ("0002", "Road 18 (Part 2)", "Completed"),
        ]),
        road_target_with_formulas(&["Road 17", "Road 18", "Road 19"]),
    );
    assert!(plan.steps[0].meta["structure"].as_array().unwrap().len() >= 2);
    eprintln!(
        "GUARDED {:?}",
        guarded_updates(&plan.steps[0]).map(|u| u.len())
    );
    eprintln!(
        "STEP {}",
        serde_json::to_string(&plan.steps[0].meta["structure"]).unwrap()
    );

    let authority = FilesystemAuthority::new(root.join("journals"), Duration::from_secs(300));
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
    assert!(applied.failed == 0, "{:?}", applied.errors);

    let mut written = File::open(&target_path).unwrap();
    let workbook = read_workbook_from_reader(&mut written).unwrap();
    let road = workbook.sheet("Road").unwrap();
    let row_of = |label: &str| {
        (1..=road.max_row)
            .find(|row| road.value(*row, 2) == label)
            .unwrap_or_else(|| panic!("{label} missing"))
    };
    let part1 = row_of("Road 18 (Part 1)");
    assert_eq!(row_of("Road 18 (Part 2)"), part1 + 1);
    assert_eq!(road.value(part1, 11), "CIV-0001");
    assert_eq!(road.value(part1 + 1, 11), "CIV-0001/0002");
    assert_eq!(road.value(part1 + 1, 12), "In Progress");
    assert_eq!(road.value(part1 + 1, 1), "South");
    assert_eq!(
        road.cell(part1 + 1, 25).unwrap().formula.as_deref(),
        Some(format!("COUNTIF(K{0}:L{0},\"Completed\")/6", part1 + 1).as_str())
    );
    for (label, zone) in [
        ("Road 01", "North"),
        ("Road 08", "Center"),
        ("Road 14", "South"),
        ("Road 21", "North"),
    ] {
        assert_eq!(road.value(row_of(label), 1), zone, "{label}");
    }
    for n in 1..=21 {
        let prefix = format!("Road {n:02}");
        assert!(
            (1..=road.max_row).any(|row| road.value(row, 2).starts_with(&prefix)),
            "{prefix} has a row"
        );
    }

    let undone = authority.undo(&applied.journal_id, &caller, None).unwrap();
    assert_eq!(undone.failed, 0);
    assert_eq!(std::fs::read(&target_path).unwrap(), original);
}

#[test]
fn a_log_missing_most_follow_up_rfis_pauses_deletion_and_warns() {
    let cells = (0..120)
        .map(|n| format!("CIV-{:04}", 1000 + n))
        .collect::<Vec<_>>();
    let rows = cells.iter().enumerate().map(|(index, code)| {
        let r = 2 + index as u32;
        row(
            r,
            [
                inline(&format!("B{r}"), "AREA-1"),
                inline(&format!("C{r}"), code),
                inline(&format!("D{r}"), "In Progress"),
            ],
        )
    });
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            std::iter::once(row(
                1,
                [
                    inline("B1", "Tag"),
                    inline("C1", "RFI No."),
                    inline("D1", "STATUS"),
                ],
            ))
            .chain(rows),
            &[],
        ),
    )]);
    let review = classify(
        full_schema_source([full_schema_record_with_status(
            10,
            "1222",
            "AZE-SUN-INT-QMS-ITP-CIV-0002",
            "CIV-1.5",
            "AREA-1",
            "Completed",
            "",
        )]),
        target,
    )
    .unwrap();

    assert!(
        review["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |warning| warning["key"] == "warn.rfi.followUp.massDeletionBrake"
                    && warning["subject"] == "120"
            ),
        "{review}"
    );
    assert!(
        review["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["reasonKey"] != "warn.rfi.followUp.reason.readyDeleted")
    );
}

#[test]
fn remark_lists_only_the_rfis_of_cells_that_need_it() {
    // Row has a mixed two-RFI cell and, in another cell pair, one single Completed RFI.
    let target = package(&[(
        "CIV",
        None,
        worksheet(
            [
                row(
                    1,
                    [
                        inline("B1", "Tag"),
                        inline("C1", "RFI No."),
                        inline("D1", "STATUS"),
                        inline("E1", "RFI No."),
                        inline("F1", "STATUS"),
                        inline("Q1", "Remarks"),
                    ],
                ),
                row(
                    2,
                    [
                        inline("B2", "AREA-1"),
                        inline("C2", "CIV-1222/1223"),
                        inline("D2", "In Progress"),
                        inline("E2", "CIV-1300"),
                        inline("F2", "Completed"),
                    ],
                ),
            ],
            &[],
        ),
    )]);
    let review = classify(
        remark_source(&[
            ("1222", "Completed"),
            ("1223", "Ongoing"),
            ("1300", "Completed"),
        ]),
        target,
    )
    .unwrap();

    let row = remark_row(&review).expect("remark row");
    assert_eq!(remark_text(row), "CIV-1222 Completed CIV-1223 In Progress");
}

#[test]
fn empty_formula_cells_in_a_formula_column_are_repaired_and_values_are_never_overwritten() {
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    // Rows 5-8: Y is formula-driven; row 6 has an empty Y, row 7 holds a typed value.
    let rows = (5..=8).map(|r| {
        let y = match r {
            6 => blank(&format!("Y{r}")),
            7 => inline(&format!("Y{r}"), "READY"),
            _ => formula(
                &format!("Y{r}"),
                &format!("COUNTIF(K{r}:L{r},\"Completed\")/6"),
                "0",
            ),
        };
        row(
            r,
            [
                inline(&format!("A{r}"), "South"),
                inline(&format!("B{r}"), &format!("Road {r:02}")),
                inline(&format!("K{r}"), ""),
                inline(&format!("L{r}"), ""),
                y,
            ],
        )
    });
    let header = row(
        4,
        [
            inline("B4", "ROAD No."),
            inline("K4", "RFI No."),
            inline("L4", "STATUS"),
        ],
    );
    let target = package(&[
        (
            "CIV",
            None,
            worksheet(
                [row(4, [inline("C4", "RFI No."), inline("D4", "STATUS")])],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")])],
                &[],
            ),
        ),
        (
            "Road",
            None,
            worksheet(std::iter::once(header).chain(rows), &[]),
        ),
    ]);
    let (plan, _, target_path, original) = road_preview(
        &root,
        road_source(&[("0001", "Road 99", "Completed")]),
        target,
    );

    let fixes = plan.meta["editor"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["reasonKey"] == "warn.rfi.followUp.reason.readyFormulaRepair")
        .collect::<Vec<_>>();
    assert_eq!(fixes.len(), 1, "{fixes:?}");
    assert!(
        fixes[0]["mappingDetails"]
            .as_str()
            .unwrap()
            .contains("= COUNTIF(K"),
        "{fixes:?}"
    );

    let authority = FilesystemAuthority::new(root.join("journals"), Duration::from_secs(300));
    let caller = CallerIdentity::new("owner", "session");
    let scope = GrantScope::new("owner", "session", "request", "rfi.follow_up_sync");
    let g1 = authority
        .issue_grant(&scope, &root, GrantAccess::Read)
        .unwrap();
    let g2 = authority
        .issue_grant(&scope, &root, GrantAccess::ReadWrite)
        .unwrap();
    let plan = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner",
                "session",
                "rfi.follow_up_sync",
                vec![g1.grant_id, g2.grant_id],
            ),
            plan,
        )
        .unwrap();
    let applied = authority.apply(&plan.plan_id, &caller).unwrap();
    assert_eq!(applied.failed, 0, "{:?}", applied.errors);
    let mut written = File::open(&target_path).unwrap();
    let road = read_workbook_from_reader(&mut written).unwrap();
    let road = road.sheet("Road").unwrap();
    let row_of = |label: &str| {
        (1..=road.max_row)
            .find(|row| road.value(*row, 2) == label)
            .unwrap()
    };
    let repaired = row_of("Road 06");
    let typed = row_of("Road 07");
    assert_eq!(
        road.cell(repaired, 25).unwrap().formula.as_deref(),
        Some(format!("COUNTIF(K{0}:L{0},\"Completed\")/6", repaired).as_str())
    );
    assert_eq!(road.value(typed, 25), "READY");
    assert!(road.cell(typed, 25).unwrap().formula.is_none());
    authority.undo(&applied.journal_id, &caller, None).unwrap();
    assert_eq!(std::fs::read(&target_path).unwrap(), original);
}

#[test]
fn empty_zone_cells_are_filled_from_the_fixed_road_table() {
    let fixture = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(fixture.path()).unwrap();
    let header = row(
        4,
        [
            inline("B4", "ROAD No."),
            inline("K4", "RFI No."),
            inline("L4", "STATUS"),
        ],
    );
    let rows = [(5, "Road 06 (Part 1)"), (6, "Road 14")]
        .into_iter()
        .map(|(r, label)| {
            row(
                r,
                [
                    blank(&format!("A{r}")),
                    inline(&format!("B{r}"), label),
                    inline(&format!("K{r}"), ""),
                    inline(&format!("L{r}"), ""),
                ],
            )
        });
    let target = package(&[
        (
            "CIV",
            None,
            worksheet(
                [row(4, [inline("C4", "RFI No."), inline("D4", "STATUS")])],
                &[],
            ),
        ),
        (
            "MEC",
            None,
            worksheet(
                [row(4, [inline("H4", "RFI No."), inline("I4", "STATUS")])],
                &[],
            ),
        ),
        (
            "Road",
            None,
            worksheet(std::iter::once(header).chain(rows), &[]),
        ),
    ]);
    let (plan, ..) = road_preview(
        &root,
        road_source(&[("0001", "Road 06", "Ongoing")]),
        target,
    );

    let zones: Vec<(String, String)> = plan.meta["editor"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["reasonKey"] == "warn.rfi.followUp.reason.structureZone")
        .map(|row| {
            (
                row["rfiNo"].as_str().unwrap().to_string(),
                row["_row"].to_string(),
            )
        })
        .collect();
    assert_eq!(zones.len(), 2, "{zones:?}");
    let texts: Vec<&str> = zones.iter().map(|(text, _)| text.as_str()).collect();
    assert!(
        texts.contains(&"Center") && texts.contains(&"South"),
        "{texts:?}"
    );
}
