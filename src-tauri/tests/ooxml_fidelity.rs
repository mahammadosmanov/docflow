use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::Path,
};

use docflow_lib::{
    fs_ops::file_sha256,
    ooxml::{
        CellData, CellUpdate, GuardedCellUpdate, WorksheetVisibility, ordered_workbook_discovery,
        package_has_signature, patch_workbook_sheets, patch_workbook_sheets_guarded,
        patch_workbook_sheets_guarded_text, read_workbook_from_reader,
        verify_guarded_workbook_delta, verify_guarded_workbook_delta_text,
    },
};
use tempfile::TempDir;
use zip::{CompressionMethod, DateTime, System, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const STRICT_MAIN: &str = "http://purl.oclc.org/ooxml/spreadsheetml/main";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_OFFICE_REL: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const PACKAGE_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn worksheet_facts_package(
    namespace: &str,
    sheets: &[(&str, Option<&str>, &str)],
    workbook_protected: bool,
    signed: bool,
) -> Cursor<Vec<u8>> {
    worksheet_facts_package_with_shared_strings(namespace, sheets, workbook_protected, signed, None)
}

fn worksheet_facts_package_with_shared_strings(
    namespace: &str,
    sheets: &[(&str, Option<&str>, &str)],
    workbook_protected: bool,
    signed: bool,
    shared_strings: Option<&str>,
) -> Cursor<Vec<u8>> {
    let office_relationship_namespace = if namespace == STRICT_MAIN {
        STRICT_OFFICE_REL
    } else {
        OFFICE_REL
    };
    let mut workbook_sheets = String::new();
    let mut workbook_relationships = String::new();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    for (index, (name, state, worksheet)) in sheets.iter().enumerate() {
        let ordinal = index + 1;
        let state = state
            .map(|value| format!(r#" state="{value}""#))
            .unwrap_or_default();
        workbook_sheets.push_str(&format!(
            r#"<sheet name="{name}" sheetId="{ordinal}" r:id="rId{ordinal}"{state}/>"#
        ));
        workbook_relationships.push_str(&format!(
            r#"<Relationship Id="rId{ordinal}" Type="{office_relationship_namespace}/worksheet" Target="worksheets/sheet{ordinal}.xml"/>"#
        ));
        writer
            .start_file(format!("xl/worksheets/sheet{ordinal}.xml"), options)
            .unwrap();
        writer.write_all(worksheet.as_bytes()).unwrap();
    }
    if shared_strings.is_some() {
        workbook_relationships.push_str(&format!(
            r#"<Relationship Id="rIdSharedStrings" Type="{office_relationship_namespace}/sharedStrings" Target="sharedStrings.xml"/>"#
        ));
    }

    let signature_relationship = if signed {
        r#"<Relationship Id="rIdSignature" Type="http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/origin" Target="_xmlsignatures/origin.sigs"/>"#
    } else {
        ""
    };
    let package_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL}"><Relationship Id="rIdWorkbook" Type="{office_relationship_namespace}/officeDocument" Target="xl/workbook.xml"/>{signature_relationship}</Relationships>"#
    );
    let protection = if workbook_protected {
        "<workbookProtection lockStructure=\"1\"/>"
    } else {
        ""
    };
    let workbook = format!(
        r#"<workbook xmlns="{namespace}" xmlns:r="{office_relationship_namespace}">{protection}<sheets>{workbook_sheets}</sheets></workbook>"#
    );
    let relationships =
        format!(r#"<Relationships xmlns="{PACKAGE_REL}">{workbook_relationships}</Relationships>"#);

    for (name, bytes) in [
        ("[Content_Types].xml", b"content-types".as_slice()),
        ("_rels/.rels", package_relationships.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", relationships.as_bytes()),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    if signed {
        writer
            .start_file("_xmlsignatures/origin.sigs", options)
            .unwrap();
        writer.write_all(b"signature-origin").unwrap();
    }
    if let Some(shared_strings) = shared_strings {
        writer.start_file("xl/sharedStrings.xml", options).unwrap();
        writer.write_all(shared_strings.as_bytes()).unwrap();
    }
    writer.finish().unwrap()
}

fn cursor_member(source: &Cursor<Vec<u8>>, name: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(source.clone()).unwrap();
    let mut member = archive.by_name(name).unwrap();
    let mut bytes = Vec::new();
    member.read_to_end(&mut bytes).unwrap();
    bytes
}

#[test]
fn guarded_text_patch_can_materialize_only_a_missing_cell_in_an_existing_row() {
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>RFI</t></is></c></row></sheetData></worksheet>"#
    );
    let mut source = worksheet_facts_package(MAIN, &[("CIV", None, &sheet)], false, false);
    let updates = BTreeMap::from([(
        "CIV".to_string(),
        BTreeMap::from([(
            (2, 17),
            GuardedCellUpdate {
                expected: CellData::default(),
                replacement: CellUpdate::Text("RFI Status Sync: CIV-1222 Completed".to_string()),
                create_if_absent: true,
            },
        )]),
    )]);
    let mut destination = Cursor::new(Vec::new());

    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).unwrap();
    destination.seek(SeekFrom::Start(0)).unwrap();
    let workbook = read_workbook_from_reader(&mut destination).unwrap();

    assert_eq!(
        workbook.sheet("CIV").unwrap().value(2, 17),
        "RFI Status Sync: CIV-1222 Completed"
    );
}

#[test]
fn guarded_text_patch_rejects_creating_a_missing_row() {
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>RFI</t></is></c></row></sheetData></worksheet>"#
    );
    let mut source = worksheet_facts_package(MAIN, &[("CIV", None, &sheet)], false, false);
    let updates = BTreeMap::from([(
        "CIV".to_string(),
        BTreeMap::from([(
            (3, 17),
            GuardedCellUpdate {
                expected: CellData::default(),
                replacement: CellUpdate::Text("RFI Status Sync: CIV-1222 Completed".to_string()),
                create_if_absent: true,
            },
        )]),
    )]);
    let mut destination = Cursor::new(Vec::new());

    assert!(patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).is_err());
}

fn replace_cursor_member(
    source: &Cursor<Vec<u8>>,
    name: &str,
    replacement: Vec<u8>,
) -> Cursor<Vec<u8>> {
    let mut archive = ZipArchive::new(source.clone()).unwrap();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .set_raw_comment(archive.comment().to_vec().into_boxed_slice())
        .unwrap();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).unwrap();
        if entry.name() == name {
            let entry_name = entry.name().to_string();
            let mut options = entry.options().into_full_options();
            if !entry.comment().is_empty() {
                options = options.with_file_comment(entry.comment());
            }
            drop(entry);
            writer.start_file(entry_name, options).unwrap();
            writer.write_all(&replacement).unwrap();
        } else {
            writer.raw_copy_file(entry).unwrap();
        }
    }
    writer.finish().unwrap()
}

#[test]
fn guarded_follow_up_patch_changes_only_authorized_value_events() {
    for namespace in [MAIN, STRICT_MAIN] {
        let worksheet = format!(
            r#"<worksheet xmlns="{namespace}" xmlns:x="urn:sentinel"><sheetData><row r="2" spans="1:3" customFormat="1"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7" x:keep="yes" x:t="sentinel"><extLst><ext uri="urn:keep"><x:payload>unchanged</x:payload></ext></extLst></c><c r="C2" t="inlineStr"><is><t>do not change</t></is></c></row></sheetData><dataValidations count="1"><dataValidation type="list" sqref="B2"><formula1>Statuses</formula1></dataValidation></dataValidations><hyperlinks><hyperlink ref="C2" location="Sheet1!A1"/></hyperlinks><extLst><ext uri="urn:sheet"><x:payload>sheet sentinel</x:payload></ext></extLst></worksheet>"#
        );
        let mut source = worksheet_facts_package(
            namespace,
            &[("Follow Up", None, worksheet.as_str())],
            false,
            false,
        );
        let before_workbook = cursor_member(&source, "xl/workbook.xml");
        let before_relationships = cursor_member(&source, "xl/_rels/workbook.xml.rels");
        let updates = BTreeMap::from([(
            "Follow Up".to_string(),
            BTreeMap::from([(
                (2, 2),
                GuardedCellUpdate {
                    expected: CellData {
                        value: String::new(),
                        formula: None,
                        style: Some("7".to_string()),
                    },
                    replacement: CellUpdate::Text("Signed".to_string()),
                    create_if_absent: false,
                },
            )]),
        )]);
        let mut destination = Cursor::new(Vec::new());

        patch_workbook_sheets_guarded(&mut source, &mut destination, &updates).unwrap();
        source.seek(SeekFrom::Start(0)).unwrap();
        destination.seek(SeekFrom::Start(0)).unwrap();
        verify_guarded_workbook_delta(&mut source, &mut destination, &updates).unwrap();

        assert_eq!(
            cursor_member(&destination, "xl/workbook.xml"),
            before_workbook
        );
        assert_eq!(
            cursor_member(&destination, "xl/_rels/workbook.xml.rels"),
            before_relationships
        );
        let patched =
            String::from_utf8(cursor_member(&destination, "xl/worksheets/sheet1.xml")).unwrap();
        assert!(patched.contains(
            r#"<c r="B2" s="7" x:keep="yes" x:t="sentinel" t="inlineStr"><is><t>Signed</t></is><extLst>"#
        ));
        assert!(patched.contains("do not change"));
        assert!(patched.contains("sheet sentinel"));
    }
}

#[test]
fn guarded_follow_up_verifier_compares_expanded_events_not_attribute_quote_bytes() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7"/></row></sheetData></worksheet>"#
    );
    let mut source = worksheet_facts_package(
        MAIN,
        &[("Follow Up", None, worksheet.as_str())],
        false,
        false,
    );
    let updates = BTreeMap::from([(
        "Follow Up".to_string(),
        BTreeMap::from([(
            (2, 2),
            GuardedCellUpdate {
                expected: CellData {
                    value: String::new(),
                    formula: None,
                    style: Some("7".to_string()),
                },
                replacement: CellUpdate::Text("Signed".to_string()),
                create_if_absent: false,
            },
        )]),
    )]);
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded(&mut source, &mut destination, &updates).unwrap();
    let worksheet_bytes = cursor_member(&destination, "xl/worksheets/sheet1.xml");
    let lexical = String::from_utf8(worksheet_bytes)
        .unwrap()
        .replacen(r#"<row r="2">"#, r#"<row r='2'>"#, 1)
        .into_bytes();
    let mut equivalent = replace_cursor_member(&destination, "xl/worksheets/sheet1.xml", lexical);

    source.seek(SeekFrom::Start(0)).unwrap();
    verify_guarded_workbook_delta(&mut source, &mut equivalent, &updates).unwrap();
}

#[test]
fn guarded_follow_up_verifier_rejects_unapproved_event_and_package_deltas() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}" xmlns:x="urn:sentinel"><sheetData><row r="2" spans="1:3"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7"/><c r="C2" t="inlineStr"><is><t>keep</t></is></c></row></sheetData><dataValidations count="1"><dataValidation type="list" sqref="B2"><formula1>Statuses</formula1></dataValidation></dataValidations><extLst><ext uri="urn:sheet"><x:payload>sentinel</x:payload></ext></extLst></worksheet>"#
    );
    let mut source = worksheet_facts_package(
        MAIN,
        &[("Follow Up", None, worksheet.as_str())],
        false,
        false,
    );
    let updates = BTreeMap::from([(
        "Follow Up".to_string(),
        BTreeMap::from([(
            (2, 2),
            GuardedCellUpdate {
                expected: CellData {
                    value: String::new(),
                    formula: None,
                    style: Some("7".to_string()),
                },
                replacement: CellUpdate::Text("Signed".to_string()),
                create_if_absent: false,
            },
        )]),
    )]);
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded(&mut source, &mut destination, &updates).unwrap();
    let sheet = String::from_utf8(cursor_member(&destination, "xl/worksheets/sheet1.xml")).unwrap();
    let workbook = String::from_utf8(cursor_member(&destination, "xl/workbook.xml")).unwrap();
    let tampered = [
        (
            "xl/worksheets/sheet1.xml",
            sheet.replacen(r#"spans="1:3""#, r#"spans="1:4""#, 1),
        ),
        (
            "xl/worksheets/sheet1.xml",
            sheet.replacen(">keep<", ">changed<", 1),
        ),
        (
            "xl/worksheets/sheet1.xml",
            sheet.replacen(">Statuses<", ">Other<", 1),
        ),
        (
            "xl/worksheets/sheet1.xml",
            sheet.replacen(">sentinel<", ">changed<", 1),
        ),
        (
            "xl/workbook.xml",
            workbook.replacen("<workbook ", "<workbook calcId=\"1\" ", 1),
        ),
    ];
    for (name, bytes) in tampered {
        let mut candidate = replace_cursor_member(&destination, name, bytes.into_bytes());
        source.seek(SeekFrom::Start(0)).unwrap();
        let error =
            verify_guarded_workbook_delta(&mut source, &mut candidate, &updates).expect_err(name);
        assert_eq!(error.kind(), docflow_lib::ooxml::OoxmlErrorKind::Invalid);
    }
}

#[test]
fn guarded_follow_up_rejects_invalid_update_authority_before_output() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7"/></row></sheetData></worksheet>"#
    );
    let mut source = worksheet_facts_package(
        MAIN,
        &[("Follow Up", None, worksheet.as_str())],
        false,
        false,
    );
    let approved = GuardedCellUpdate {
        expected: CellData {
            value: String::new(),
            formula: None,
            style: Some("7".to_string()),
        },
        replacement: CellUpdate::Text("Signed".to_string()),
        create_if_absent: false,
    };
    let invalid_sets = [
        BTreeMap::new(),
        BTreeMap::from([(
            "Follow Up".to_string(),
            BTreeMap::from([(
                (2, 2),
                GuardedCellUpdate {
                    expected: CellData {
                        style: Some("8".to_string()),
                        ..approved.expected.clone()
                    },
                    replacement: approved.replacement.clone(),
                    create_if_absent: false,
                },
            )]),
        )]),
        BTreeMap::from([(
            "Follow Up".to_string(),
            BTreeMap::from([(
                (2, 2),
                GuardedCellUpdate {
                    expected: approved.expected.clone(),
                    replacement: CellUpdate::Text("Rejected".to_string()),
                    create_if_absent: false,
                },
            )]),
        )]),
        BTreeMap::from([(
            "Follow Up".to_string(),
            BTreeMap::from([((3, 2), approved.clone())]),
        )]),
    ];
    for updates in invalid_sets {
        let mut destination = Cursor::new(Vec::new());
        source.seek(SeekFrom::Start(0)).unwrap();
        assert!(patch_workbook_sheets_guarded(&mut source, &mut destination, &updates).is_err());
        assert!(destination.into_inner().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn guarded_writer_stays_on_pinned_handles_after_visible_parent_swap() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::tempdir().unwrap();
    let source_path = fixture.path().join("Follow Up.xlsx");
    let visible_parent = fixture.path().join(".docflow-backups");
    let pinned_parent = fixture.path().join("pinned-backups");
    let replacement_parent = fixture.path().join("replacement-backups");
    fs::create_dir(&visible_parent).unwrap();
    fs::create_dir(&replacement_parent).unwrap();
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0006</t></is></c><c r="B2" s="7"/></row></sheetData></worksheet>"#
    );
    let package = worksheet_facts_package(
        MAIN,
        &[("Follow Up", None, worksheet.as_str())],
        false,
        false,
    );
    fs::write(&source_path, package.into_inner()).unwrap();
    let output_path = visible_parent.join("private-output.xlsx");
    let mut source = File::open(&source_path).unwrap();
    let mut destination = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&output_path)
        .unwrap();
    fs::rename(&visible_parent, &pinned_parent).unwrap();
    symlink(&replacement_parent, &visible_parent).unwrap();
    let updates = BTreeMap::from([(
        "Follow Up".to_string(),
        BTreeMap::from([(
            (2, 2),
            GuardedCellUpdate {
                expected: CellData {
                    value: String::new(),
                    formula: None,
                    style: Some("7".to_string()),
                },
                replacement: CellUpdate::Text("Signed".to_string()),
                create_if_absent: false,
            },
        )]),
    )]);

    patch_workbook_sheets_guarded(&mut source, &mut destination, &updates).unwrap();
    source.seek(SeekFrom::Start(0)).unwrap();
    destination.seek(SeekFrom::Start(0)).unwrap();
    verify_guarded_workbook_delta(&mut source, &mut destination, &updates).unwrap();

    assert!(pinned_parent.join("private-output.xlsx").is_file());
    assert!(!replacement_parent.join("private-output.xlsx").exists());
}

#[test]
fn worksheet_facts_accept_only_canonical_strict_or_transitional_hierarchy() {
    for namespace in [MAIN, STRICT_MAIN] {
        let worksheet = format!(
            r#"<worksheet xmlns="{namespace}"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>canonical</t></is></c></row></sheetData></worksheet>"#
        );
        let mut source = worksheet_facts_package(
            namespace,
            &[("Facts", None, worksheet.as_str())],
            false,
            false,
        );
        let workbook = read_workbook_from_reader(&mut source).unwrap();
        assert_eq!(workbook.sheet("Facts").unwrap().value(2, 1), "canonical");
    }

    for worksheet in [
        format!(r#"<worksheet xmlns="{MAIN}"><cols/></worksheet>"#),
        format!(
            r#"<worksheet xmlns="{MAIN}"><sheetData/><sheetData><row r="2"><c r="A2"/></row></sheetData></worksheet>"#
        ),
        format!(r#"<worksheet xmlns="{MAIN}"><sheetData><c r="A2"/></sheetData></worksheet>"#),
        format!(
            r#"<worksheet xmlns="{MAIN}"><sheetData><row r="3"><c r="A2"/></row></sheetData></worksheet>"#
        ),
    ] {
        let mut source =
            worksheet_facts_package(MAIN, &[("Facts", None, worksheet.as_str())], false, false);
        assert!(
            read_workbook_from_reader(&mut source).is_err(),
            "{worksheet}"
        );
    }
}

#[test]
fn worksheet_facts_reject_duplicate_coordinates_before_indexing() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>first</t></is></c><c r="A2" t="inlineStr"><is><t>second</t></is></c></row></sheetData></worksheet>"#
    );
    let mut source =
        worksheet_facts_package(MAIN, &[("Facts", None, worksheet.as_str())], false, false);

    assert!(read_workbook_from_reader(&mut source).is_err());
}

#[test]
fn worksheet_facts_reject_foreign_namespace_cell_shadowing() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}" xmlns:foreign="urn:foreign"><sheetData><row r="2"><foreign:c r="A2" t="inlineStr"><foreign:is><foreign:t>shadow</foreign:t></foreign:is></foreign:c></row></sheetData></worksheet>"#
    );
    let mut source =
        worksheet_facts_package(MAIN, &[("Facts", None, worksheet.as_str())], false, false);

    assert!(read_workbook_from_reader(&mut source).is_err());
}

#[test]
fn worksheet_facts_reject_foreign_inline_wrapper_shadowing() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}" xmlns:evil="urn:evil"><sheetData><row r="2"><c r="A2" t="inlineStr"><is><evil:r><t>shadow</t></evil:r></is></c></row></sheetData></worksheet>"#
    );
    let mut source =
        worksheet_facts_package(MAIN, &[("Facts", None, worksheet.as_str())], false, false);

    assert!(read_workbook_from_reader(&mut source).is_err());
}

#[test]
fn worksheet_facts_reject_formula_outside_canonical_cell_child_chain() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}" xmlns:evil="urn:evil"><sheetData><row r="2"><c r="A2"><evil:wrapper><f>LOOKUP()</f></evil:wrapper><v>Completed</v></c></row></sheetData></worksheet>"#
    );
    let mut source =
        worksheet_facts_package(MAIN, &[("Facts", None, worksheet.as_str())], false, false);

    assert!(read_workbook_from_reader(&mut source).is_err());
}

#[test]
fn worksheet_facts_reject_duplicate_physical_row_identities() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="2"><c r="A2"/></row><row r="2"><c r="B2"/></row></sheetData></worksheet>"#
    );
    let mut source =
        worksheet_facts_package(MAIN, &[("Facts", None, worksheet.as_str())], false, false);

    assert!(read_workbook_from_reader(&mut source).is_err());
}

#[test]
fn worksheet_facts_reject_noncanonical_shared_string_namespace_or_hierarchy() {
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="2"><c r="A2" t="s"><v>0</v></c></row></sheetData></worksheet>"#
    );
    let shared_strings = [
        r#"<sst xmlns="urn:evil"><si><t>shadow</t></si></sst>"#.to_string(),
        format!(
            r#"<sst xmlns="{MAIN}" xmlns:evil="urn:evil"><evil:si><t>shadow</t></evil:si></sst>"#
        ),
        format!(
            r#"<sst xmlns="{MAIN}" xmlns:evil="urn:evil"><si><evil:wrapper><t>shadow</t></evil:wrapper></si></sst>"#
        ),
    ];

    for shared in shared_strings {
        let mut source = worksheet_facts_package_with_shared_strings(
            MAIN,
            &[("Facts", None, worksheet.as_str())],
            false,
            false,
            Some(&shared),
        );
        assert!(read_workbook_from_reader(&mut source).is_err(), "{shared}");
    }
}

#[test]
fn worksheet_facts_capture_formula_merge_protection_signature_and_visibility() {
    let hidden = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetProtection sheet="1"/><sheetData><row r="2"><c r="A2" t="inlineStr"><is><t>merged</t></is></c></row><row r="3"><c r="C3"><f>1+1</f><v>2</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A2:B2"/></mergeCells></worksheet>"#
    );
    let very_hidden = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"/></row></sheetData></worksheet>"#
    );
    let mut source = worksheet_facts_package(
        MAIN,
        &[
            ("Hidden", Some("hidden"), hidden.as_str()),
            ("VeryHidden", Some("veryHidden"), very_hidden.as_str()),
        ],
        true,
        true,
    );

    let discovery = ordered_workbook_discovery(&mut source).unwrap();
    assert!(discovery.safety.has_workbook_protection);
    assert_eq!(
        discovery
            .worksheets
            .iter()
            .map(|sheet| sheet.visibility)
            .collect::<Vec<_>>(),
        vec![WorksheetVisibility::Hidden, WorksheetVisibility::VeryHidden]
    );
    source.seek(SeekFrom::Start(0)).unwrap();
    assert!(package_has_signature(&mut source).unwrap());
    source.seek(SeekFrom::Start(0)).unwrap();
    let workbook = read_workbook_from_reader(&mut source).unwrap();
    let sheet = workbook.sheet("Hidden").unwrap();
    assert!(sheet.has_sheet_protection);
    assert_eq!(sheet.cell(3, 3).unwrap().formula.as_deref(), Some("1+1"));
    let merged = sheet.merged_range_containing(2, 2).unwrap();
    assert_eq!(
        (
            merged.first_row,
            merged.first_column,
            merged.last_row,
            merged.last_column,
        ),
        (2, 1, 2, 2)
    );
}

fn package(path: &Path, macros: bool) {
    let file = File::create(path).expect("fixture package");
    let mut writer = ZipWriter::new(file);
    writer.set_comment("DocFlow fixture").unwrap();
    let options = SimpleFileOptions::default()
        .system(System::Unix)
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::from_date_and_time(2024, 5, 4, 3, 2, 0).unwrap())
        .unix_permissions(0o640);
    let stored = options.compression_method(CompressionMethod::Stored);
    let mut add = |name: &str, bytes: &[u8], entry_options: SimpleFileOptions| {
        writer
            .start_file(name, entry_options)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        writer.write_all(bytes).unwrap();
    };
    add(
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
 <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
 <Default Extension="xml" ContentType="application/xml"/>
 <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
</Types>"#,
        options,
    );
    add(
        "_rels/.rels",
        br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        options,
    );
    add(
        "xl/workbook.xml",
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="CIV" sheetId="1" r:id="rId1"/><sheet name="MEC" sheetId="2" r:id="rId2"/><sheet name="Sheet1" sheetId="3" r:id="rId3"/><sheet name="Untouched" sheetId="4" r:id="rId4"/></sheets><definedNames><definedName name="Print_Area" localSheetId="0">CIV!$A$1:$Q$30</definedName><definedName name="ExternalName">'[linked.xlsx]Data'!$A$1</definedName></definedNames><calcPr calcId="191029" calcMode="manual"/></workbook>"#
        )
        .as_bytes(),
        options,
    );
    add(
        "xl/_rels/workbook.xml.rels",
        br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet3.xml"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet4.xml"/></Relationships>"#,
        options,
    );
    add(
        "xl/worksheets/sheet1.xml",
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="{MAIN}" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:x14ac="http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac" xmlns:xr2="http://schemas.microsoft.com/office/spreadsheetml/2015/revision2" mc:Ignorable="x14ac xr2"><dimension ref="A1:Q30"/><cols><col min="1" max="1" width="25.25" customWidth="1"/></cols><sheetData><row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="D9" t="inlineStr"><is><t>DESCRIPTION</t></is></c></row><row r="10"><c r="A10" s="4" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0007</t></is></c></row><row r="11"><c r="A11" s="4"><f>1+1</f><v>2</v></c><c r="D11" s="9" t="str"><f t="shared" si="7">C11*2</f><v/></c><c r="J11" s="12" t="inlineStr"><is><t>KEEP EXPIRY</t></is></c></row><row r="30"><c r="Q30"/></row></sheetData><dataValidations count="2"><dataValidation type="custom" sqref="A10:A30"><formula1>COUNTIF($A$10:$A$30,A10)&lt;=1</formula1></dataValidation><dataValidation type="list" sqref="C10:C30"><formula1>INDIRECT(B10)</formula1></dataValidation></dataValidations><drawing r:id="rIdDraw"/></worksheet>"#
        )
        .as_bytes(),
        options,
    );
    for (name, marker) in [
        ("xl/worksheets/sheet2.xml", "MEC SHEET"),
        ("xl/worksheets/sheet3.xml", "LOOKUP SHEET"),
        ("xl/worksheets/sheet4.xml", "UNTOUCHED SHEET"),
    ] {
        add(
            name,
            format!(r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>{marker}</t></is></c></row><row r="2"><c r="A2"><f>SUM(1,2)</f><v>3</v></c></row></sheetData></worksheet>"#).as_bytes(),
            options,
        );
    }
    add(
        "xl/calcChain.xml",
        format!(r#"<calcChain xmlns="{MAIN}"><c r="A11" i="1"/><c r="D11"/><c r="A11" i="4"/></calcChain>"#).as_bytes(),
        options,
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
        ("customXml/item1.xml", b"custom-xml-sentinel".as_slice()),
    ] {
        add(name, bytes, options);
    }
    if macros {
        add("xl/vbaProject.bin", b"VBA\0PROJECT\xffSENTINEL", stored);
    }
    writer.finish().unwrap();
}

#[derive(Debug, Eq, PartialEq)]
struct EntryEvidence {
    content: Vec<u8>,
    compressed: Vec<u8>,
    compression: CompressionMethod,
    modified: Option<DateTime>,
    mode: Option<u32>,
    comment: String,
}

fn evidence(path: &Path) -> (Vec<String>, Vec<u8>, BTreeMap<String, EntryEvidence>) {
    let file = File::open(path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let comment = archive.comment().to_vec();
    let names = (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().to_string())
        .collect::<Vec<_>>();
    let mut entries = BTreeMap::new();
    for name in &names {
        let mut entry = archive.by_name(name).unwrap();
        let start = entry.data_start().unwrap();
        let compressed_size = entry.compressed_size();
        let compression = entry.compression();
        let modified = entry.last_modified();
        let mode = entry.unix_mode();
        let entry_comment = entry.comment().to_string();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        drop(entry);
        let mut raw = File::open(path).unwrap();
        raw.seek(SeekFrom::Start(start)).unwrap();
        let mut compressed = vec![0; compressed_size as usize];
        raw.read_exact(&mut compressed).unwrap();
        entries.insert(
            name.clone(),
            EntryEvidence {
                content,
                compressed,
                compression,
                modified,
                mode,
                comment: entry_comment,
            },
        );
    }
    (names, comment, entries)
}

#[test]
fn targeted_patch_preserves_all_untouched_members_and_zip_properties() {
    let fixture = TempDir::new().unwrap();
    let source = fixture.path().join("fidelity.xlsm");
    let destination = fixture.path().join("patched.xlsm");
    package(&source, true);
    assert_eq!(
        file_sha256(&source).unwrap(),
        "2c3fb5effe531f10ec62ed690c12d2caa517aa1607fe06085a0f51245b6abd9f"
    );
    let (before_names, before_comment, before) = evidence(&source);

    let updates = BTreeMap::from([(
        "CIV".to_string(),
        BTreeMap::from([
            (
                (11, 1),
                CellUpdate::Text("AZE-SUN-INT-QMS-RFI-CIV-0008".to_string()),
            ),
            (
                (11, 4),
                CellUpdate::Formula {
                    formula: "C11*2".to_string(),
                    cached: Some("calculated".to_string()),
                },
            ),
            ((11, 8), CellUpdate::Number("45500".to_string())),
            ((11, 6), CellUpdate::Text(" padded tag ".to_string())),
        ]),
    )]);
    patch_workbook_sheets(&source, &destination, &updates, Some(1)).unwrap();

    let (after_names, after_comment, after) = evidence(&destination);
    assert_eq!(after_names, before_names);
    assert_eq!(after_comment, before_comment);
    let changed = BTreeSet::from([
        "xl/worksheets/sheet1.xml".to_string(),
        "xl/calcChain.xml".to_string(),
    ]);
    for name in before_names {
        if !changed.contains(&name) {
            assert_eq!(after[&name], before[&name], "{name}");
        }
    }
    for name in changed {
        assert_eq!(
            after[&name].compression, before[&name].compression,
            "{name}"
        );
        assert_eq!(after[&name].modified, before[&name].modified, "{name}");
        assert_eq!(after[&name].mode, before[&name].mode, "{name}");
        assert_eq!(after[&name].comment, before[&name].comment, "{name}");
    }
    assert_eq!(
        after["xl/vbaProject.bin"].content,
        b"VBA\0PROJECT\xffSENTINEL"
    );

    let sheet = String::from_utf8(after["xl/worksheets/sheet1.xml"].content.clone()).unwrap();
    assert!(sheet.contains("AZE-SUN-INT-QMS-RFI-CIV-0008"));
    assert!(sheet.contains("<f t=\"shared\" si=\"7\">C11*2</f>"));
    assert!(sheet.contains("<v>calculated</v>"));
    assert!(sheet.contains("s=\"4\""));
    assert!(sheet.contains("s=\"9\""));
    assert!(sheet.contains("xml:space=\"preserve\"> padded tag </t>"));
    assert!(sheet.contains("KEEP EXPIRY"));
    assert!(sheet.contains("xmlns:xr2="));
    assert!(sheet.contains("mc:Ignorable=\"x14ac xr2\""));
    assert!(sheet.contains("OR(A10=&quot;&quot;,COUNTIF($A$10:A10,A10)=1,A10=A9)"));
    assert!(sheet.contains("INDIRECT(B10)"));
    let chain = String::from_utf8(after["xl/calcChain.xml"].content.clone()).unwrap();
    assert!(!chain.contains("<c r=\"A11\" i=\"1\""));
    assert!(chain.contains("<c r=\"D11\" i=\"1\""));
    assert!(chain.contains("<c r=\"A11\" i=\"4\""));
}

#[test]
fn patch_rejects_malformed_input_and_existing_destination_without_clobber() {
    let fixture = TempDir::new().unwrap();
    let malformed = fixture.path().join("malformed.xlsx");
    let output = fixture.path().join("output.xlsx");
    fs::write(&malformed, b"not a zip").unwrap();
    let updates = BTreeMap::from([(
        "CIV".to_string(),
        BTreeMap::from([((1, 1), CellUpdate::Text("value".to_string()))]),
    )]);

    assert!(patch_workbook_sheets(&malformed, &output, &updates, None).is_err());
    assert!(!output.exists());

    let source = fixture.path().join("source.xlsx");
    package(&source, false);
    fs::write(&output, b"KEEP").unwrap();
    assert!(patch_workbook_sheets(&source, &output, &updates, None).is_err());
    assert_eq!(fs::read(output).unwrap(), b"KEEP");
}
