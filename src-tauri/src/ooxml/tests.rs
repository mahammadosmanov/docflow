use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
};

use tempfile::tempdir;
use zip::{ZipWriter, write::SimpleFileOptions};

use super::{
    CellData, CellText, CellUpdate, GuardedCellUpdate, MAX_APPLY_MERGE_COMPARISONS,
    MergeComparisonBudget, OoxmlErrorKind, RichRun, RowOp, WorksheetDescriptor,
    WorksheetVisibility, apply_row_ops, ordered_workbook_discovery, parse_document_with_limits,
    patch_workbook_sheets, patch_workbook_sheets_guarded_text,
    patch_workbook_sheets_guarded_text_recalc, read_workbook, read_workbook_from_reader,
    sheet_infos_from_path, verify_guarded_workbook_delta_text,
    verify_guarded_workbook_delta_text_recalc,
};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const OFFICE_DOCUMENT_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const PACKAGE_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

#[test]
fn guarded_follow_up_apply_merge_budget_exact_boundary_and_one_over() {
    let mut exact = MergeComparisonBudget::for_apply(MAX_APPLY_MERGE_COMPARISONS - 1).unwrap();
    exact.consume().unwrap();
    let one_over = exact.consume().unwrap_err();
    assert_eq!(one_over.kind(), OoxmlErrorKind::LimitExceeded);

    let already_over =
        MergeComparisonBudget::for_apply(MAX_APPLY_MERGE_COMPARISONS + 1).unwrap_err();
    assert_eq!(already_over.kind(), OoxmlErrorKind::LimitExceeded);
}

struct SparseReader {
    len: u64,
    position: u64,
}

impl Read for SparseReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
}

impl Seek for SparseReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => self.len.checked_add_signed(offset).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid sparse seek")
            })?,
            SeekFrom::Current(offset) => {
                self.position.checked_add_signed(offset).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid sparse seek")
                })?
            }
        };
        Ok(self.position)
    }
}

fn fixture(workbook: &str, relationships: &str, members: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
    fixture_from_members(workbook, relationships, members.iter().copied())
}

fn declared_entry_count_fixture(entry_count: u16) -> Cursor<Vec<u8>> {
    let mut bytes = b"PK\x05\x06".to_vec();
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&entry_count.to_le_bytes());
    bytes.extend_from_slice(&entry_count.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    Cursor::new(bytes)
}

fn append_decoy_eocd(mut fixture: Cursor<Vec<u8>>) -> Cursor<Vec<u8>> {
    let offset = u32::try_from(fixture.get_ref().len()).unwrap();
    fixture.set_position(u64::from(offset));
    fixture.write_all(b"PK\x05\x06").unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.write_all(&1_u16.to_le_bytes()).unwrap();
    fixture.write_all(&1_u16.to_le_bytes()).unwrap();
    fixture.write_all(&0_u32.to_le_bytes()).unwrap();
    fixture.write_all(&offset.to_le_bytes()).unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.set_position(0);
    fixture
}

fn append_parser_fallback_eocd(mut fixture: Cursor<Vec<u8>>) -> Cursor<Vec<u8>> {
    let bytes = fixture.get_ref();
    let original_eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .unwrap();
    let original_entries =
        u16::from_le_bytes([bytes[original_eocd + 10], bytes[original_eocd + 11]]);
    let directory_offset = u32::from_le_bytes([
        bytes[original_eocd + 16],
        bytes[original_eocd + 17],
        bytes[original_eocd + 18],
        bytes[original_eocd + 19],
    ]);
    let fake_eocd_offset = u32::try_from(bytes.len()).unwrap();
    let fake_entries = original_entries.checked_add(1).unwrap();
    let fake_directory_size = fake_eocd_offset.checked_sub(directory_offset).unwrap();

    fixture.set_position(u64::from(fake_eocd_offset));
    fixture.write_all(b"PK\x05\x06").unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.write_all(&fake_entries.to_le_bytes()).unwrap();
    fixture.write_all(&fake_entries.to_le_bytes()).unwrap();
    fixture
        .write_all(&fake_directory_size.to_le_bytes())
        .unwrap();
    fixture.write_all(&directory_offset.to_le_bytes()).unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.set_position(0);
    fixture
}

fn append_semantically_invalid_directory(mut fixture: Cursor<Vec<u8>>) -> Cursor<Vec<u8>> {
    let bytes = fixture.get_ref();
    let original_eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .unwrap();
    let entry_count = u16::from_le_bytes([bytes[original_eocd + 10], bytes[original_eocd + 11]]);
    let directory_size = u32::from_le_bytes([
        bytes[original_eocd + 12],
        bytes[original_eocd + 13],
        bytes[original_eocd + 14],
        bytes[original_eocd + 15],
    ]);
    let directory_offset = u32::from_le_bytes([
        bytes[original_eocd + 16],
        bytes[original_eocd + 17],
        bytes[original_eocd + 18],
        bytes[original_eocd + 19],
    ]);
    let mut directory = bytes
        [directory_offset as usize..directory_offset as usize + directory_size as usize]
        .to_vec();
    assert_eq!(&directory[..4], b"PK\x01\x02");
    let name_len = usize::from(u16::from_le_bytes([directory[28], directory[29]]));
    let extra_len = u16::from_le_bytes([directory[30], directory[31]]);
    directory[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    directory[30..32].copy_from_slice(&extra_len.checked_add(4).unwrap().to_le_bytes());
    directory.splice(46 + name_len..46 + name_len, [0x01, 0x00, 0x08, 0x00]);

    let final_directory_offset = u32::try_from(bytes.len()).unwrap();
    let final_eocd_offset = final_directory_offset
        .checked_add(u32::try_from(directory.len()).unwrap())
        .unwrap();
    fixture.set_position(u64::from(final_directory_offset));
    fixture.write_all(&directory).unwrap();
    fixture.write_all(b"PK\x05\x06").unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.write_all(&entry_count.to_le_bytes()).unwrap();
    fixture.write_all(&entry_count.to_le_bytes()).unwrap();
    fixture
        .write_all(&(final_eocd_offset - final_directory_offset).to_le_bytes())
        .unwrap();
    fixture
        .write_all(&final_directory_offset.to_le_bytes())
        .unwrap();
    fixture.write_all(&0_u16.to_le_bytes()).unwrap();
    fixture.set_position(0);
    fixture
}

fn final_eocd_offset(bytes: &[u8]) -> usize {
    bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .unwrap()
}

fn central_directory_offsets(bytes: &[u8]) -> Vec<usize> {
    let eocd_offset = final_eocd_offset(bytes);
    let entry_count = usize::from(u16::from_le_bytes([
        bytes[eocd_offset + 10],
        bytes[eocd_offset + 11],
    ]));
    let mut offset = u32::from_le_bytes([
        bytes[eocd_offset + 16],
        bytes[eocd_offset + 17],
        bytes[eocd_offset + 18],
        bytes[eocd_offset + 19],
    ]) as usize;
    let mut offsets = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        assert_eq!(&bytes[offset..offset + 4], b"PK\x01\x02");
        offsets.push(offset);
        let name_len = usize::from(u16::from_le_bytes([bytes[offset + 28], bytes[offset + 29]]));
        let extra_len = usize::from(u16::from_le_bytes([bytes[offset + 30], bytes[offset + 31]]));
        let comment_len = usize::from(u16::from_le_bytes([bytes[offset + 32], bytes[offset + 33]]));
        offset += 46 + name_len + extra_len + comment_len;
    }
    offsets
}

fn mutate_first_central_field(
    mut fixture: Cursor<Vec<u8>>,
    field_offset: usize,
    replacement: &[u8],
) -> Cursor<Vec<u8>> {
    let central_offset = central_directory_offsets(fixture.get_ref())[0];
    fixture.get_mut()
        [central_offset + field_offset..central_offset + field_offset + replacement.len()]
        .copy_from_slice(replacement);
    fixture.set_position(0);
    fixture
}

fn insert_first_central_extra(mut fixture: Cursor<Vec<u8>>, extra: &[u8]) -> Cursor<Vec<u8>> {
    let bytes = fixture.get_ref();
    let eocd_offset = final_eocd_offset(bytes);
    let central_offset = central_directory_offsets(bytes)[0];
    let directory_size = u32::from_le_bytes([
        bytes[eocd_offset + 12],
        bytes[eocd_offset + 13],
        bytes[eocd_offset + 14],
        bytes[eocd_offset + 15],
    ]);
    let name_len = usize::from(u16::from_le_bytes([
        bytes[central_offset + 28],
        bytes[central_offset + 29],
    ]));
    let extra_len = u16::from_le_bytes([bytes[central_offset + 30], bytes[central_offset + 31]]);
    let insertion_offset = central_offset + 46 + name_len + usize::from(extra_len);
    let new_extra_len = extra_len
        .checked_add(u16::try_from(extra.len()).unwrap())
        .unwrap();
    fixture.get_mut()[central_offset + 30..central_offset + 32]
        .copy_from_slice(&new_extra_len.to_le_bytes());
    fixture
        .get_mut()
        .splice(insertion_offset..insertion_offset, extra.iter().copied());
    let new_eocd_offset = eocd_offset + extra.len();
    let new_directory_size = directory_size
        .checked_add(u32::try_from(extra.len()).unwrap())
        .unwrap();
    fixture.get_mut()[new_eocd_offset + 12..new_eocd_offset + 16]
        .copy_from_slice(&new_directory_size.to_le_bytes());
    fixture.set_position(0);
    fixture
}

fn mutate_first_local_field(
    mut fixture: Cursor<Vec<u8>>,
    field_offset: usize,
    replacement: &[u8],
) -> Cursor<Vec<u8>> {
    let bytes = fixture.get_ref();
    let central_offset = central_directory_offsets(bytes)[0];
    let local_offset = u32::from_le_bytes([
        bytes[central_offset + 42],
        bytes[central_offset + 43],
        bytes[central_offset + 44],
        bytes[central_offset + 45],
    ]) as usize;
    assert_eq!(&bytes[local_offset..local_offset + 4], b"PK\x03\x04");
    fixture.get_mut()[local_offset + field_offset..local_offset + field_offset + replacement.len()]
        .copy_from_slice(replacement);
    fixture.set_position(0);
    fixture
}

fn insert_last_local_extra(mut fixture: Cursor<Vec<u8>>, extra: &[u8]) -> Cursor<Vec<u8>> {
    let bytes = fixture.get_ref();
    let eocd_offset = final_eocd_offset(bytes);
    let central_offsets = central_directory_offsets(bytes);
    let central_offset = *central_offsets
        .iter()
        .max_by_key(|offset| {
            u32::from_le_bytes([
                bytes[**offset + 42],
                bytes[**offset + 43],
                bytes[**offset + 44],
                bytes[**offset + 45],
            ])
        })
        .unwrap();
    let local_offset = u32::from_le_bytes([
        bytes[central_offset + 42],
        bytes[central_offset + 43],
        bytes[central_offset + 44],
        bytes[central_offset + 45],
    ]) as usize;
    assert_eq!(&bytes[local_offset..local_offset + 4], b"PK\x03\x04");
    let directory_offset = u32::from_le_bytes([
        bytes[eocd_offset + 16],
        bytes[eocd_offset + 17],
        bytes[eocd_offset + 18],
        bytes[eocd_offset + 19],
    ]);
    let name_len = usize::from(u16::from_le_bytes([
        bytes[local_offset + 26],
        bytes[local_offset + 27],
    ]));
    let extra_len = u16::from_le_bytes([bytes[local_offset + 28], bytes[local_offset + 29]]);
    let insertion_offset = local_offset + 30 + name_len + usize::from(extra_len);
    let new_extra_len = extra_len
        .checked_add(u16::try_from(extra.len()).unwrap())
        .unwrap();
    fixture.get_mut()[local_offset + 28..local_offset + 30]
        .copy_from_slice(&new_extra_len.to_le_bytes());
    fixture
        .get_mut()
        .splice(insertion_offset..insertion_offset, extra.iter().copied());
    let new_eocd_offset = eocd_offset + extra.len();
    let new_directory_offset = directory_offset
        .checked_add(u32::try_from(extra.len()).unwrap())
        .unwrap();
    fixture.get_mut()[new_eocd_offset + 16..new_eocd_offset + 20]
        .copy_from_slice(&new_directory_offset.to_le_bytes());
    fixture.set_position(0);
    fixture
}

fn assert_archive_preflight_rejects(fixture: Cursor<Vec<u8>>) {
    let mut preflight_source = fixture.clone();
    let preflight_error = match super::preflight_archive(&mut preflight_source) {
        Ok(_) => {
            let discovery = ordered_workbook_discovery(&mut fixture.clone());
            panic!(
                "archive preflight accepted entry-level ZIP metadata; discovery returned {discovery:?}"
            );
        }
        Err(error) => error,
    };
    assert_eq!(
        preflight_error.to_string(),
        "workbook ZIP metadata is invalid"
    );

    let discovery_error = ordered_workbook_discovery(&mut fixture.clone()).unwrap_err();
    assert_eq!(
        discovery_error.to_string(),
        "workbook ZIP metadata is invalid"
    );
}

fn fixture_from_members<'a>(
    workbook: &'a str,
    relationships: &'a str,
    members: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Cursor<Vec<u8>> {
    let package_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/></Relationships>"#
    );
    fixture_with_workbook_part(
        Some(&package_relationships),
        "xl/workbook.xml",
        workbook,
        relationships,
        members,
    )
}

fn fixture_with_workbook_part<'a>(
    package_relationships: Option<&str>,
    workbook_part: &str,
    workbook: &'a str,
    relationships: &'a str,
    members: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Cursor<Vec<u8>> {
    let workbook = workbook.replace(
        r#"xmlns:r="urn:r""#,
        r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#,
    );
    let relationships = if relationships.contains("xmlns=") {
        relationships.to_string()
    } else {
        relationships.replacen(
            "<Relationships",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships""#,
            1,
        )
    };
    let (workbook_parent, workbook_name) = workbook_part.rsplit_once('/').unwrap();
    let workbook_relationships_part = format!("{workbook_parent}/_rels/{workbook_name}.rels");
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    if let Some(package_relationships) = package_relationships {
        writer.start_file("_rels/.rels", options).unwrap();
        writer.write_all(package_relationships.as_bytes()).unwrap();
    }
    for (name, bytes) in [
        (workbook_part, workbook.as_bytes()),
        (
            workbook_relationships_part.as_str(),
            relationships.as_bytes(),
        ),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    for (name, bytes) in members {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    let mut fixture = writer.finish().unwrap();
    fixture.set_position(0);
    fixture
}

fn ordered_fixture() -> Cursor<Vec<u8>> {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="MEC" sheetId="17" r:id="rId7"/><sheet name="CIV" sheetId="3" r:id="rId2"/><sheet name="Hidden" sheetId="44" state="hidden" r:id="rId9"/><sheet name="VeryHidden" sheetId="45" state="veryHidden" r:id="rId10"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/><Relationship Id="rId10" Type="{REL}" Target="worksheets/sheet10.xml"/><Relationship Id="rId7" Type="{REL}" Target="/xl/worksheets/sheet7.xml"/><Relationship Id="rId9" Type="{REL}" Target="worksheets/sheet9.xml"/></Relationships>"#
    );
    fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet2.xml", b"CIV"),
            ("xl/worksheets/sheet7.xml", b"MEC"),
            ("xl/worksheets/sheet9.xml", b"Hidden"),
            ("xl/worksheets/sheet10.xml", b"VeryHidden"),
            ("docProps/unrelated.bin", b"unrelated"),
        ],
    )
}

#[test]
fn package_workbook_authority_authenticates_exact_internal_target() {
    let trusted_workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Trusted" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let trusted_relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let decoy_workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Decoy" sheetId="9" r:id="rId9"/></sheets></workbook>"#
    );
    let decoy_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId9" Type="{REL}" Target="worksheets/decoy.xml"/></Relationships>"#
    );
    let package_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="custom/book.xml"/></Relationships>"#
    );
    let trusted_members: [(&str, &[u8]); 4] = [
        ("custom/worksheets/sheet1.xml", b"trusted worksheet"),
        ("xl/workbook.xml", decoy_workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", decoy_relationships.as_bytes()),
        ("xl/worksheets/decoy.xml", b"decoy worksheet"),
    ];
    let discovery = ordered_workbook_discovery(&mut fixture_with_workbook_part(
        Some(&package_relationships),
        "custom/book.xml",
        &trusted_workbook,
        &trusted_relationships,
        trusted_members,
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "Trusted");
    assert_eq!(
        discovery.worksheets[0].part_path,
        "custom/worksheets/sheet1.xml"
    );

    let strict_office_document =
        "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
    let strict_worksheet = "http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet";
    let strict_workbook = r#"<workbook xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main" xmlns:r="http://purl.oclc.org/ooxml/officeDocument/relationships"><sheets><sheet name="Strict" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let strict_relationships = format!(
        r#"<Relationships xmlns="http://purl.oclc.org/ooxml/package/relationships"><Relationship Id="rId1" Type="{strict_worksheet}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let strict_package_relationships = format!(
        r#"<Relationships xmlns="http://purl.oclc.org/ooxml/package/relationships"><Relationship Id="rIdWorkbook" Type="{strict_office_document}" Target="strict/book.xml"/></Relationships>"#
    );
    let strict_members: [(&str, &[u8]); 1] =
        [("strict/worksheets/sheet1.xml", b"strict worksheet")];
    let strict_discovery = ordered_workbook_discovery(&mut fixture_with_workbook_part(
        Some(&strict_package_relationships),
        "strict/book.xml",
        strict_workbook,
        &strict_relationships,
        strict_members,
    ))
    .unwrap();
    assert_eq!(strict_discovery.worksheets[0].name, "Strict");

    let invalid_package_relationships = [
        None,
        Some(format!(
            r#"<Relationships xmlns="urn:spoof"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/></Relationships>"#
        )),
        Some(format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdOne" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/><Relationship Id="rIdTwo" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/></Relationships>"#
        )),
        Some(format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml" TargetMode="External"/></Relationships>"#
        )),
        Some(format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="urn:unknown/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
        )),
        Some(format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="missing/book.xml"/></Relationships>"#
        )),
    ];
    for package_relationships in invalid_package_relationships {
        let decoy_members: [(&str, &[u8]); 1] = [("xl/worksheets/decoy.xml", b"decoy worksheet")];
        let mut package = fixture_with_workbook_part(
            package_relationships.as_deref(),
            "xl/workbook.xml",
            &decoy_workbook,
            &decoy_relationships,
            decoy_members,
        );
        assert!(
            ordered_workbook_discovery(&mut package).is_err(),
            "invalid package workbook relationship must fail closed"
        );
    }
}

#[test]
fn xml_parser_rejects_excessive_element_depth() {
    let error = match parse_document_with_limits(b"<a><b><c/></b></a>", 2, 100) {
        Ok(_) => panic!("excessively deep XML must fail closed"),
        Err(error) => error,
    };

    assert_eq!(error.to_string(), "XML element depth exceeded the limit");
    assert_eq!(error.kind(), OoxmlErrorKind::LimitExceeded);
}

#[test]
fn xml_parser_rejects_excessive_retained_nodes() {
    let error = match parse_document_with_limits(b"<a><b/><c/></a>", 10, 2) {
        Ok(_) => panic!("excessive retained XML nodes must fail closed"),
        Err(error) => error,
    };

    assert_eq!(error.to_string(), "XML node count exceeded the limit");
    assert_eq!(error.kind(), OoxmlErrorKind::LimitExceeded);
}

#[test]
fn ordered_worksheet_discovery_preserves_order_identity_and_visibility() {
    let discovery = ordered_workbook_discovery(&mut ordered_fixture()).unwrap();

    assert_eq!(
        discovery.worksheets,
        vec![
            WorksheetDescriptor {
                workbook_index: 1,
                sheet_id: 17,
                relationship_id: "rId7".to_string(),
                relationship_type: REL.to_string(),
                part_path: "xl/worksheets/sheet7.xml".to_string(),
                name: "MEC".to_string(),
                visibility: WorksheetVisibility::Visible,
            },
            WorksheetDescriptor {
                workbook_index: 2,
                sheet_id: 3,
                relationship_id: "rId2".to_string(),
                relationship_type: REL.to_string(),
                part_path: "xl/worksheets/sheet2.xml".to_string(),
                name: "CIV".to_string(),
                visibility: WorksheetVisibility::Visible,
            },
            WorksheetDescriptor {
                workbook_index: 3,
                sheet_id: 44,
                relationship_id: "rId9".to_string(),
                relationship_type: REL.to_string(),
                part_path: "xl/worksheets/sheet9.xml".to_string(),
                name: "Hidden".to_string(),
                visibility: WorksheetVisibility::Hidden,
            },
            WorksheetDescriptor {
                workbook_index: 4,
                sheet_id: 45,
                relationship_id: "rId10".to_string(),
                relationship_type: REL.to_string(),
                part_path: "xl/worksheets/sheet10.xml".to_string(),
                name: "VeryHidden".to_string(),
                visibility: WorksheetVisibility::VeryHidden,
            },
        ]
    );
    assert_eq!(discovery.safety, Default::default());
}

#[test]
fn ordered_worksheet_discovery_ignores_sheet_elements_outside_sheets_container() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><ext:data><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/></ext:data><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", b"worksheet"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "MEC");
    assert_eq!(discovery.worksheets[0].workbook_index, 1);
}

#[test]
fn ordered_worksheet_discovery_ignores_foreign_sheet_elements_in_sheets_container() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", b"worksheet"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "MEC");
}

#[test]
fn legacy_sheet_info_projection_ignores_foreign_sheet_elements() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let bytes = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", b"worksheet"),
        ],
    )
    .into_inner();
    let directory = tempdir().unwrap();
    let path = directory.path().join("workbook.xlsx");
    fs::write(&path, bytes).unwrap();

    let infos = sheet_infos_from_path(&path).unwrap();

    assert_eq!(infos.len(), 1);
    assert_eq!(infos["MEC"].sheet_id, 1);
    assert_eq!(infos["MEC"].path, "xl/worksheets/sheet1.xml");
}

#[test]
fn read_workbook_uses_canonical_sheet_projection() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(r#"<worksheet xmlns="{MAIN}"><sheetData/></worksheet>"#);
    let bytes = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
        ],
    )
    .into_inner();
    let directory = tempdir().unwrap();
    let path = directory.path().join("workbook.xlsx");
    fs::write(&path, bytes).unwrap();

    let data = read_workbook(&path).unwrap();

    assert!(data.sheet("MEC").is_some());
    assert!(data.sheet("Decoy").is_none());
}

#[test]
fn read_workbook_uses_authenticated_shared_strings_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStrings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="/custom/strings.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#
    );
    let trusted_strings =
        format!(r#"<sst xmlns="{MAIN}" count="1" uniqueCount="1"><si><t>Trusted</t></si></sst>"#);
    let decoy_strings =
        format!(r#"<sst xmlns="{MAIN}" count="1" uniqueCount="1"><si><t>Decoy</t></si></sst>"#);
    let bytes = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
            ("custom/strings.xml", trusted_strings.as_bytes()),
            ("xl/sharedStrings.xml", decoy_strings.as_bytes()),
        ],
    )
    .into_inner();
    let directory = tempdir().unwrap();
    let path = directory.path().join("workbook.xlsx");
    fs::write(&path, bytes).unwrap();

    let data = read_workbook(&path).unwrap();

    assert_eq!(data.sheet("MEC").unwrap().value(1, 1), "Trusted");
}

#[test]
fn patch_workbook_sheets_uses_canonical_sheet_projection() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="MEC" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><v>old</v></c></row></sheetData></worksheet>"#
    );
    let bytes = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
        ],
    )
    .into_inner();
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.xlsx");
    let destination = directory.path().join("destination.xlsx");
    fs::write(&source, bytes).unwrap();
    let updates = BTreeMap::from([(
        "MEC".to_string(),
        BTreeMap::from([((1, 1), CellUpdate::Text("new".to_string()))]),
    )]);

    patch_workbook_sheets(&source, &destination, &updates, None).unwrap();

    let data = read_workbook(&destination).unwrap();
    assert_eq!(data.sheet("MEC").unwrap().value(1, 1), "new");
}

#[test]
fn guarded_text_patch_can_clear_a_literal_cell() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Pending</t></is></c></row></sheetData></worksheet>"#
    );
    let mut source = fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", worksheet.as_bytes())],
    );
    let updates = BTreeMap::from([(
        "Follow Up".to_string(),
        BTreeMap::from([(
            (1, 1),
            GuardedCellUpdate {
                expected: CellData {
                    value: "Pending".to_string(),
                    formula: None,
                    style: None,
                },
                replacement: CellUpdate::Text(String::new()),
                create_if_absent: false,
            },
        )]),
    )]);
    let mut destination = Cursor::new(Vec::new());

    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    source.set_position(0);
    destination.set_position(0);

    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).unwrap();
}

#[test]
fn ordered_worksheet_discovery_rejects_sheet_id_from_foreign_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:evil="urn:evil"><sheets><sheet name="MEC" sheetId="1" evil:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook sheet relationship is missing");
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_sheet_attribute_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:spoof"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook relationship namespace is invalid"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_workbook_namespace() {
    let workbook = r#"<workbook xmlns="urn:spoof" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook namespace is invalid");
}

#[test]
fn ordered_worksheet_discovery_expanded_names_rejects_evil_sheets_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets xmlns="urn:evil"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook sheets container is invalid");
}

#[test]
fn ordered_worksheet_discovery_expanded_names_rejects_shadowed_relationship_child() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship xmlns="urn:evil" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook sheet target is missing");
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_raw_default_namespace_declarations() {
    let canonical_workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let canonical_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let cases = [
        (
            format!(
                r#"<workbook xmlns="urn:foreign" xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets xmlns="urn:foreign" xmlns="{MAIN}"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet xmlns="urn:foreign" xmlns="{MAIN}" name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            canonical_workbook.clone(),
            format!(
                r#"<Relationships xmlns="urn:foreign" xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
        (
            canonical_workbook,
            format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship xmlns="urn:foreign" xmlns="{PACKAGE_REL_NS}" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
    ];

    for (workbook, relationships) in cases {
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "XML attribute is invalid");
    }
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_raw_prefixed_namespace_declarations() {
    const OFFICE_REL_NS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let canonical_workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="{OFFICE_REL_NS}"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let canonical_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let cases = [
        (
            format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            format!(
                r#"<workbook xmlns="{MAIN}"><sheets xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            format!(
                r#"<workbook xmlns="{MAIN}"><sheets><sheet xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}" name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            canonical_workbook.clone(),
            format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}" xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
        (
            canonical_workbook,
            format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
    ];

    for (workbook, relationships) in cases {
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "XML attribute is invalid");
    }
}

#[test]
fn ordered_worksheet_discovery_expanded_names_require_unqualified_attributes() {
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    for (attribute, expected_error) in [
        (
            r#"evil:name="MEC" sheetId="1""#,
            "workbook sheet name is missing",
        ),
        (
            r#"name="MEC" evil:sheetId="1""#,
            "workbook sheet identity is invalid",
        ),
    ] {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:evil="urn:evil"><sheets><sheet {attribute} r:id="rId1"/></sheets></workbook>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), expected_error);
    }

    for (attribute, expected_error) in [
        (
            format!(r#"evil:Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml""#),
            "workbook relationship identity is missing",
        ),
        (
            r#"Id="rId1" evil:Type="urn:evil" Target="worksheets/sheet1.xml""#.to_string(),
            "workbook sheet relationship type is missing",
        ),
        (
            format!(r#"Id="rId1" Type="{REL}" evil:Target="worksheets/sheet1.xml""#),
            "workbook relationship target is missing",
        ),
    ] {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns:evil="urn:evil"><Relationship {attribute}/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), expected_error);
    }

    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:evil="urn:evil"><sheets><sheet name="MEC" sheetId="1" evil:state="hidden" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns:evil="urn:evil"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml" evil:TargetMode="External"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap();

    assert_eq!(
        discovery.worksheets[0].visibility,
        WorksheetVisibility::Visible
    );
}

#[test]
fn ordered_worksheet_discovery_expanded_names_accepts_alternate_valid_prefixes() {
    for (workbook_namespace, office_namespace, package_namespace, relationship_type) in [
        (
            MAIN,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
            "http://schemas.openxmlformats.org/package/2006/relationships",
            REL,
        ),
        (
            "http://purl.oclc.org/ooxml/spreadsheetml/main",
            "http://purl.oclc.org/ooxml/officeDocument/relationships",
            "http://purl.oclc.org/ooxml/package/relationships",
            "http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet",
        ),
    ] {
        let workbook = format!(
            r#"<ss:workbook xmlns:ss="{workbook_namespace}" xmlns:od="{office_namespace}"><ss:sheets><ss:sheet name="MEC" sheetId="1" od:id="rId1"/></ss:sheets></ss:workbook>"#
        );
        let relationships = format!(
            r#"<pkg:Relationships xmlns:pkg="{package_namespace}"><pkg:Relationship Id="rId1" Type="{relationship_type}" Target="worksheets/sheet1.xml"/></pkg:Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap();

        assert_eq!(discovery.worksheets[0].relationship_id, "rId1");
        assert_eq!(discovery.worksheets[0].relationship_type, relationship_type);
    }
}

#[test]
fn ordered_worksheet_discovery_expanded_names_reports_qualified_workbook_protection() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:ss="{MAIN}"><ss:workbookProtection lockStructure="1"/><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap();

    assert!(discovery.safety.has_workbook_protection);
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_name() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="MEC" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"one"),
            ("xl/worksheets/sheet2.xml", b"two"),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "duplicate workbook sheet name");
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_sheet_id() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="7" r:id="rId1"/><sheet name="CIV" sheetId="7" r:id="rId2"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"one"),
            ("xl/worksheets/sheet2.xml", b"two"),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "duplicate workbook sheet identity");
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_relationship_id() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="CIV" sheetId="2" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"one")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "duplicate workbook sheet relationship identity"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_relationship_declaration() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"one"),
            ("xl/worksheets/sheet2.xml", b"two"),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "duplicate workbook relationship identity"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_resolved_part() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="CIV" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="/xl/worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"one")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "duplicate worksheet part path");
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_normalized_workbook_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    for alias in [
        "./worksheets/sheet1.xml",
        "/xl/worksheets/%73heet1.xml",
        r"worksheets\sheet1.xml",
    ] {
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="{alias}"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "duplicate workbook relationship target");
    }
}

#[test]
fn ordered_worksheet_discovery_rejects_external_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="https://example.invalid/sheet.xml" TargetMode="External"/></Relationships>"#
    );
    let error =
        ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

    assert_eq!(
        error.to_string(),
        "external workbook relationship target is not allowed"
    );
}

#[test]
fn ordered_worksheet_discovery_accepts_package_internal_parent_relative_custom_xml() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../customXml/item1.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.xml"/></Relationships>"#
    );

    let discovery = ordered_workbook_discovery(&mut fixture_from_members(
        &workbook,
        &relationships,
        [
            ("xl/worksheets/sheet1.xml", b"worksheet".as_slice()),
            ("customXml/item1.xml", b"custom xml".as_slice()),
            ("xl/metadata.xml", b"sheet metadata".as_slice()),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(
        discovery.worksheets[0].part_path,
        "xl/worksheets/sheet1.xml"
    );
}

#[test]
fn ordered_worksheet_discovery_accepts_explicit_internal_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml" TargetMode="Internal"/></Relationships>"#
    );

    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "MEC");
}

#[test]
fn ordered_worksheet_discovery_rejects_encoded_traversal_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/%2E%2E/evil.xml"/></Relationships>"#
    );
    let error =
        ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship escapes the package"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_encoded_forbidden_target_character() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml%3Fignored"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml?ignored", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship escapes the package"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_non_worksheet_relationship() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Chart" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet" Target="chartsheets/sheet1.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        relationships,
        &[("xl/chartsheets/sheet1.xml", b"chart")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook sheet relationship is not a worksheet"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_relationship_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = r#"<Relationships><Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook sheet relationship is not a worksheet"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_missing_worksheet_member() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/missing.xml"/></Relationships>"#
    );
    let error =
        ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

    assert_eq!(error.to_string(), "worksheet package member is missing");
}

#[test]
fn internal_relationship_targets_require_exact_package_members() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let missing_workbook_auxiliary = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles/missing.xml"/></Relationships>"#
    );
    let workbook_error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &missing_workbook_auxiliary,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();
    assert_eq!(
        workbook_error.to_string(),
        "workbook relationship package member is missing"
    );

    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdDrawing" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/missing.xml"/></Relationships>"#;
    let worksheet_members: [(&str, &[u8]); 2] = [
        ("xl/worksheets/sheet1.xml", b"worksheet"),
        (
            "xl/worksheets/_rels/sheet1.xml.rels",
            worksheet_relationships,
        ),
    ];
    let worksheet_relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &worksheet_relationships,
        worksheet_members.as_slice(),
    ))
    .unwrap_err();
    assert_eq!(
        worksheet_error.to_string(),
        "worksheet relationship package member is missing"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_unknown_visibility_state() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" state="mystery" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"one")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "worksheet visibility state is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_zero_worksheets() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets/></workbook>"#
    );
    let error =
        ordered_workbook_discovery(&mut fixture(&workbook, "<Relationships/>", &[])).unwrap_err();

    assert_eq!(error.to_string(), "workbook contains no worksheets");
}

#[test]
fn ordered_worksheet_discovery_rejects_251_worksheets() {
    let mut sheets = String::new();
    let mut relationship_elements = String::new();
    let mut members = Vec::new();
    for index in 1..=251 {
        sheets.push_str(&format!(
            r#"<sheet name="Sheet{index}" sheetId="{index}" r:id="rId{index}"/>"#
        ));
        relationship_elements.push_str(&format!(
            r#"<Relationship Id="rId{index}" Type="{REL}" Target="worksheets/sheet{index}.xml"/>"#
        ));
        members.push((
            format!("xl/worksheets/sheet{index}.xml"),
            Vec::from(b"sheet".as_slice()),
        ));
    }
    let workbook =
        format!(r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets>{sheets}</sheets></workbook>"#);
    let relationships = format!(r#"<Relationships>{relationship_elements}</Relationships>"#);
    let error = ordered_workbook_discovery(&mut fixture_from_members(
        &workbook,
        &relationships,
        members
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook has too many worksheets");
}

#[test]
fn ordered_worksheet_discovery_rejects_oversized_source_before_zip_parse() {
    let error = ordered_workbook_discovery(&mut SparseReader {
        len: super::MAX_PACKAGE_BYTES + 1,
        position: 0,
    })
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook source exceeded the size limit");
}

#[test]
fn ordered_worksheet_discovery_rejects_declared_entry_count_before_zip_parse() {
    let error = ordered_workbook_discovery(&mut declared_entry_count_fixture(
        u16::try_from(super::MAX_ENTRIES + 1).unwrap(),
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP entry count is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_decoy_eocd_before_parser_fallback() {
    let error = ordered_workbook_discovery(&mut append_decoy_eocd(ordered_fixture())).unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_inflated_final_eocd_before_parser_fallback() {
    let error = ordered_workbook_discovery(&mut append_parser_fallback_eocd(ordered_fixture()))
        .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_semantically_invalid_final_directory() {
    let error =
        ordered_workbook_discovery(&mut append_semantically_invalid_directory(ordered_fixture()))
            .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}

#[test]
fn archive_preflight_rejects_entry_nonzero_disk_number_start() {
    assert_archive_preflight_rejects(mutate_first_central_field(
        ordered_fixture(),
        34,
        &1_u16.to_le_bytes(),
    ));
}

#[test]
fn archive_preflight_rejects_entry_zip64_central_sentinels() {
    for field_offset in [20, 24, 42] {
        assert_archive_preflight_rejects(mutate_first_central_field(
            ordered_fixture(),
            field_offset,
            &u32::MAX.to_le_bytes(),
        ));
    }
    assert_archive_preflight_rejects(mutate_first_central_field(
        ordered_fixture(),
        34,
        &u16::MAX.to_le_bytes(),
    ));
}

#[test]
fn archive_preflight_rejects_entry_zip64_central_extra_without_sentinel() {
    assert_archive_preflight_rejects(insert_first_central_extra(
        ordered_fixture(),
        &[0x01, 0x00, 0x00, 0x00],
    ));
}

#[test]
fn archive_preflight_rejects_entry_malformed_central_extra() {
    assert_archive_preflight_rejects(insert_first_central_extra(
        ordered_fixture(),
        &[0xfe, 0xca, 0x04, 0x00],
    ));
}

#[test]
fn archive_preflight_rejects_entry_zip64_local_size_sentinels() {
    for field_offset in [18, 22] {
        assert_archive_preflight_rejects(mutate_first_local_field(
            ordered_fixture(),
            field_offset,
            &u32::MAX.to_le_bytes(),
        ));
    }
}

#[test]
fn archive_preflight_rejects_entry_zip64_local_extra() {
    assert_archive_preflight_rejects(insert_last_local_extra(
        ordered_fixture(),
        &[0x01, 0x00, 0x00, 0x00],
    ));
}

#[test]
fn archive_preflight_rejects_entry_malformed_local_extra() {
    assert_archive_preflight_rejects(insert_last_local_extra(
        ordered_fixture(),
        &[0xfe, 0xca, 0x04, 0x00],
    ));
}

#[test]
fn ordered_worksheet_discovery_reports_vba_project() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"not worksheet XML"),
            ("xl/vbaProject.bin", b"synthetic VBA bytes"),
        ],
    ))
    .unwrap();

    assert!(discovery.safety.has_vba_project);
}

#[test]
fn ordered_worksheet_discovery_vba_relationship_reports_noncanonical_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="macros/project.bin"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/macros/project.bin", b"synthetic VBA bytes"),
        ],
    ))
    .unwrap();

    assert!(discovery.safety.has_vba_project);
}

#[test]
fn ordered_worksheet_discovery_vba_relationship_rejects_missing_target_member() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="macros/project.bin"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "VBA project package member is missing");
}

#[test]
fn ordered_worksheet_discovery_vba_relationship_retains_canonical_orphan_signal() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/vbaProject.bin", b"orphan VBA bytes"),
        ],
    ))
    .unwrap();

    assert!(discovery.safety.has_vba_project);
}

#[test]
fn ordered_worksheet_discovery_rejects_non_internal_auxiliary_workbook_target_modes() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    for (target_mode, expected_error) in [
        (
            "External",
            "external workbook relationship target is not allowed",
        ),
        ("Bogus", "workbook relationship target mode is invalid"),
    ] {
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin" TargetMode="{target_mode}"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/vbaProject.bin", b"VBA"),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), expected_error);
    }
}

#[test]
fn ordered_worksheet_discovery_rejects_encoded_forbidden_auxiliary_workbook_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin%3Fignored"/></Relationships>"#
    );

    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/vbaProject.bin", b"VBA"),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship escapes the package"
    );
}

#[test]
fn ordered_worksheet_discovery_reports_workbook_protection() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><workbookProtection lockStructure="1"/><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap();

    assert!(discovery.safety.has_workbook_protection);
}

#[test]
fn ordered_worksheet_discovery_reports_xlm_macro_sheet_relationship() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdMacro" Type="http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet" Target="macrosheets/sheet2.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet cells are not read"),
            (
                "xl/macrosheets/sheet2.xml",
                b"macro sheet cells are not read",
            ),
        ],
    ))
    .unwrap();

    assert!(discovery.safety.has_xlm_macro_sheet);
}

#[test]
fn ordered_worksheet_discovery_counts_external_links() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdLink1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="externalLinks/externalLink1.xml"/><Relationship Id="rIdLink2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="externalLinks/externalLink2.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/externalLinks/externalLink1.xml", b"link one"),
            ("xl/externalLinks/externalLink2.xml", b"link two"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.safety.external_link_count, 2);
}

#[test]
fn ordered_worksheet_discovery_counts_explicit_internal_connections() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml" TargetMode="Internal"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/connections.xml",
                format!(
                    r#"<connections xmlns="{MAIN}" count="2"><connection id="1"/><connection id="2"/></connections>"#
                )
                .as_bytes(),
            ),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.safety.connection_count, 2);
}

#[test]
fn ordered_worksheet_discovery_uses_canonical_connection_expanded_names() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml"/></Relationships>"#
    );
    let connections = format!(
        r#"<connections xmlns="{MAIN}" xmlns:x="urn:foreign" count="1" x:count="99"><connection id="1"/><x:connection id="2"/></connections>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/connections.xml", connections.as_bytes()),
        ],
    ))
    .unwrap();
    assert_eq!(discovery.safety.connection_count, 1);

    let foreign_connections =
        br#"<x:connections xmlns:x="urn:foreign"><x:connection id="1"/></x:connections>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/connections.xml", foreign_connections),
        ],
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "relationship count is invalid");
}

#[test]
fn ordered_worksheet_discovery_counts_explicit_internal_query_tables_without_reading_cells() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml" TargetMode="Internal"/><Relationship Id="rIdQuery2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable2.xml" TargetMode="Internal"/></Relationships>"#;
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            (
                "xl/worksheets/sheet1.xml",
                b"deliberately malformed worksheet XML with secret cell text",
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
            ("xl/queryTables/queryTable1.xml", b"query one"),
            ("xl/queryTables/queryTable2.xml", b"query two"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.safety.query_table_count, 2);
}

#[test]
fn ordered_worksheet_discovery_rejects_unknown_worksheet_relationship_class() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdMystery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/mysteryData" Target="../mystery.bin"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
            ("xl/mystery.bin", b"unknown data"),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "unknown worksheet relationship class");
}

#[test]
fn ordered_worksheet_discovery_rejects_external_worksheet_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="https://example.invalid/query" TargetMode="External"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "external worksheet relationship target is not allowed"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_worksheet_relationship_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="urn:spoof"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationships namespace is invalid"
    );
}

#[test]
fn ordered_worksheet_discovery_ignores_foreign_worksheet_relationship_elements() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships" xmlns:ext="urn:extension"><ext:Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.safety.query_table_count, 0);
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_worksheet_relationship_identity() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable2.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "duplicate worksheet relationship identity"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_worksheet_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "duplicate worksheet relationship target");
}

#[test]
fn ordered_worksheet_discovery_rejects_missing_worksheet_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship target is missing"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_encoded_forbidden_worksheet_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml%3Fignored"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship escapes the package"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_unknown_workbook_relationship_class() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdMystery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/mysteryData" Target="mystery.bin"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/mystery.bin", b"unknown data"),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "unknown workbook relationship class");
}

#[test]
fn ordered_worksheet_discovery_accepts_volatile_dependencies_and_modern_office_relationships() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships>
            <Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/>
            <Relationship Id="rIdVolatile" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/volatileDependencies" Target="volatileDependencies.xml"/>
            <Relationship Id="rIdCalc" Type="http://schemas.microsoft.com/office/2020/07/relationships/calcFeatures" Target="calcFeatures.xml"/>
            <Relationship Id="rIdCustomUI" Type="http://schemas.microsoft.com/office/2006/relationships/ui/customUI" Target="customUI/customUI.xml"/>
        </Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/volatileDependencies.xml", b"<volTypes/>"),
            ("xl/calcFeatures.xml", b"<calcFeatures/>"),
            ("xl/customUI/customUI.xml", b"<customUI/>"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "MEC");
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_workbook_relationship_document_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="urn:spoof"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook relationships namespace is invalid"
    );
}

#[test]
fn ordered_worksheet_discovery_ignores_foreign_workbook_relationship_elements() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships" xmlns:ext="urn:extension"><ext:Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet" Target="worksheets/decoy.xml" TargetMode="External"/><ext:Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="missing.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", b"worksheet"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(
        discovery.worksheets[0].part_path,
        "xl/worksheets/sheet1.xml"
    );
    assert!(!discovery.safety.has_xlm_macro_sheet);
}

#[test]
fn ordered_worksheet_discovery_rejects_malformed_connection_count() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/connections.xml",
                br#"<connections count="many"><connection id="1"/></connections>"#,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "relationship count is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_oversized_sheet_attribute() {
    let name = "N".repeat(super::MAX_SHEET_NAME_BYTES + 1);
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="{name}" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook sheet name exceeded the size limit"
    );
}

type GuardedUpdates = BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>;

fn recalc_fixture(calc_pr: &str) -> (Cursor<Vec<u8>>, GuardedUpdates) {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets>{calc_pr}</workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Pending</t></is></c><c r="B1"><f>COUNTIF(A1:A1,"Completed")</f><v>0</v></c></row></sheetData></worksheet>"#
    );
    let source = fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", worksheet.as_bytes())],
    );
    let updates = BTreeMap::from([(
        "Follow Up".to_string(),
        BTreeMap::from([(
            (1, 1),
            GuardedCellUpdate {
                expected: CellData {
                    value: "Pending".to_string(),
                    formula: None,
                    style: None,
                },
                replacement: CellUpdate::Text("Completed".to_string()),
                create_if_absent: false,
            },
        )]),
    )]);
    (source, updates)
}

fn workbook_xml(package: &mut Cursor<Vec<u8>>) -> String {
    package.set_position(0);
    let mut archive = zip::ZipArchive::new(&mut *package).unwrap();
    let mut text = String::new();
    archive
        .by_name("xl/workbook.xml")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn recalc_patch_sets_full_calc_on_load_and_nothing_else_in_the_workbook_part() {
    let (mut source, updates) = recalc_fixture(r#"<calcPr calcId="191029"/>"#);
    let before = workbook_xml(&mut source);
    let mut destination = Cursor::new(Vec::new());

    patch_workbook_sheets_guarded_text_recalc(&mut source, &mut destination, &updates).unwrap();
    source.set_position(0);
    destination.set_position(0);
    verify_guarded_workbook_delta_text_recalc(&mut source, &mut destination, &updates).unwrap();

    let after = workbook_xml(&mut destination);
    assert!(after.contains(r#"fullCalcOnLoad="1""#), "{after}");
    assert_eq!(
        after.replace(r#" fullCalcOnLoad="1""#, ""),
        before,
        "only the calcPr attribute may differ"
    );
}

#[test]
fn recalc_verification_rejects_any_other_workbook_part_change() {
    let (mut source, updates) = recalc_fixture(r#"<calcPr calcId="191029"/>"#);
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded_text_recalc(&mut source, &mut destination, &updates).unwrap();
    let patched_sheet = {
        destination.set_position(0);
        let mut archive = zip::ZipArchive::new(&mut destination).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    };
    let tampered = workbook_xml(&mut destination)
        .replace(r#"calcId="191029""#, r#"calcId="191029" iterate="1""#);
    let mut forged = fixture_from_members(
        &tampered,
        &format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        ),
        [("xl/worksheets/sheet1.xml", patched_sheet.as_slice())],
    );
    source.set_position(0);
    let error =
        verify_guarded_workbook_delta_text_recalc(&mut source, &mut forged, &updates).unwrap_err();
    assert!(error.to_string().contains("recalculation flag"), "{error}");
}

#[test]
fn plain_guarded_patch_leaves_the_workbook_part_untouched() {
    let (mut source, updates) = recalc_fixture(r#"<calcPr calcId="191029"/>"#);
    let before = workbook_xml(&mut source);
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    assert_eq!(workbook_xml(&mut destination), before);
}

#[test]
fn recalc_without_a_calc_pr_element_leaves_the_workbook_part_untouched() {
    let (mut source, updates) = recalc_fixture("");
    let before = workbook_xml(&mut source);
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded_text_recalc(&mut source, &mut destination, &updates).unwrap();
    source.set_position(0);
    destination.set_position(0);
    verify_guarded_workbook_delta_text_recalc(&mut source, &mut destination, &updates).unwrap();
    assert_eq!(workbook_xml(&mut destination), before);
}

const STYLES: &str = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/><family val="2"/></font><font><sz val="9"/><color theme="1"/><name val="Arial"/><family val="2"/></font></fonts><cellXfs count="2"><xf fontId="0"/><xf fontId="1"/></cellXfs></styleSheet>"#;

fn rich_runs() -> Vec<RichRun> {
    vec![
        RichRun {
            text: "CIV-0021,0022 Completed".to_string(),
            emphasized: true,
        },
        RichRun {
            text: " CIV-0043 In Progress".to_string(),
            emphasized: false,
        },
    ]
}

fn rich_fixture() -> (Cursor<Vec<u8>>, GuardedUpdates) {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" s="1"/></row></sheetData></worksheet>"#
    );
    let source = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
            ("xl/styles.xml", STYLES.as_bytes()),
        ],
    );
    let updates = BTreeMap::from([(
        "Follow Up".to_string(),
        BTreeMap::from([(
            (1, 1),
            GuardedCellUpdate {
                expected: CellData {
                    value: String::new(),
                    formula: None,
                    style: Some("1".to_string()),
                },
                replacement: CellUpdate::Rich(rich_runs()),
                create_if_absent: false,
            },
        )]),
    )]);
    (source, updates)
}

fn sheet_xml(package: &mut Cursor<Vec<u8>>) -> String {
    package.set_position(0);
    let mut archive = zip::ZipArchive::new(&mut *package).unwrap();
    let mut text = String::new();
    archive
        .by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn rich_patch_writes_emphasized_and_plain_runs_in_the_cell_font() {
    let (mut source, updates) = rich_fixture();
    let mut destination = Cursor::new(Vec::new());

    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    let xml = sheet_xml(&mut destination);

    assert!(
        xml.contains(
            r#"<b/><u/><sz val="9"/><color theme="8"/><rFont val="Arial"/><family val="2"/>"#
        ),
        "{xml}"
    );
    assert!(xml.contains(r#"<sz val="9"/><color theme="1"/><rFont val="Arial"/><family val="2"/></rPr><t xml:space="preserve"> CIV-0043 In Progress</t>"#), "{xml}");
    source.set_position(0);
    destination.set_position(0);
    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).unwrap();
    destination.set_position(0);
    let workbook = read_workbook_from_reader(&mut destination).unwrap();
    assert_eq!(
        workbook.sheet("Follow Up").unwrap().value(1, 1),
        "CIV-0021,0022 Completed CIV-0043 In Progress"
    );
}

#[test]
fn rich_verification_rejects_changed_formatting() {
    let (mut source, updates) = rich_fixture();
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    let forged_sheet =
        sheet_xml(&mut destination).replace(r#"<color theme="8"/>"#, r#"<color theme="9"/>"#);
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let mut forged = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", forged_sheet.as_bytes()),
            ("xl/styles.xml", STYLES.as_bytes()),
        ],
    );
    source.set_position(0);

    assert!(verify_guarded_workbook_delta_text(&mut source, &mut forged, &updates).is_err());
}

#[test]
fn empty_rich_replacement_is_rejected() {
    let (mut source, mut updates) = rich_fixture();
    updates
        .get_mut("Follow Up")
        .unwrap()
        .get_mut(&(1, 1))
        .unwrap()
        .replacement = CellUpdate::Rich(Vec::new());
    let mut destination = Cursor::new(Vec::new());

    assert!(patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).is_err());
}

const CONTENT_TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.xml" ContentType="x"/><Override PartName="/xl/calcChain.xml" ContentType="y"/></Types>"#;

fn rows_fixture(stat_formula: &str) -> Cursor<Vec<u8>> {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Road" sheetId="1" r:id="rId1"/><sheet name="STAT" sheetId="2" r:id="rId2"/></sheets><definedNames><definedName name="_xlnm.Print_Area" localSheetId="0">Road!$A$1:$D$6</definedName></definedNames></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain" Target="calcChain.xml"/></Relationships>"#
    );
    let road = format!(
        r#"<worksheet xmlns="{MAIN}"><dimension ref="A1:D6"/><sheetData><row r="1"><c r="D1"><f>AVERAGE(D3:D6)</f><v>0</v></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>ZONE</t></is></c></row><row r="3" ht="15"><c r="A3" s="2" t="inlineStr"><is><t>North</t></is></c><c r="B3" s="2" t="inlineStr"><is><t>Road 01</t></is></c><c r="C3" s="3" t="inlineStr"><is><t>CIV-0001</t></is></c><c r="D3" s="4"><f t="shared" ref="D3:D6" si="0">IF(C3="","empty",C3)</f><v>0</v></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>North</t></is></c><c r="B4" t="inlineStr"><is><t>Road 02</t></is></c><c r="D4"><f t="shared" si="0"/><v>0</v></c></row><row r="6"><c r="D6"><f t="shared" si="0"/><v>0</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A3:A4"/></mergeCells><conditionalFormatting sqref="C3:C6"><cfRule type="expression" priority="1"><formula>C3="x"</formula></cfRule></conditionalFormatting></worksheet>"#
    );
    let stat = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><f>{stat_formula}</f></c></row></sheetData></worksheet>"#
    );
    fixture_from_members(
        &workbook,
        &relationships,
        [
            ("xl/worksheets/sheet1.xml", road.as_bytes()),
            ("xl/worksheets/sheet2.xml", stat.as_bytes()),
            ("xl/calcChain.xml", b"<calcChain/>".as_slice()),
            ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ],
    )
}

fn member(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn insert_after_row_three() -> RowOp {
    RowOp::InsertRow {
        sheet: "Road".to_string(),
        after_row: 3,
        template_row: 3,
        copy_columns: vec![1],
        texts: vec![CellText {
            column: 2,
            text: "Road 01 (Part 2)".to_string(),
        }],
    }
}

#[test]
fn insert_row_clones_the_template_and_shifts_everything_it_owns() {
    let mut source = rows_fixture("SUM(Road!D3:D6)");
    let output = apply_row_ops(&mut source, &[insert_after_row_three()]).unwrap();
    let road = member(&output, "xl/worksheets/sheet1.xml");

    // Rows below move down, the new row sits at 4.
    assert!(road.contains(r#"<row r="4" ht="15">"#), "{road}");
    assert!(road.contains(r#"<row r="5"><c r="A5""#), "{road}");
    assert!(road.contains(r#"<row r="7"><c r="D7">"#), "{road}");
    // Template styles and the zone are kept, the label is set, the RFI is cleared.
    assert!(
        road.contains(r#"<c r="A4" s="2" t="inlineStr"><is><t>North</t></is></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="B4" s="2" t="inlineStr"><is><t>Road 01 (Part 2)</t></is></c>"#),
        "{road}"
    );
    assert!(road.contains(r#"<c r="C4" s="3"/>"#), "{road}");
    // Shared formulas are expanded; the clone gets its own translated formula.
    assert!(!road.contains("shared"), "{road}");
    assert!(
        road.contains(r#"<c r="D3" s="4"><f>IF(C3=&quot;&quot;,&quot;empty&quot;,C3)</f>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D4" s="4"><f>IF(C4=&quot;&quot;,&quot;empty&quot;,C4)</f></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D5"><f>IF(C5=&quot;&quot;,&quot;empty&quot;,C5)</f>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D7"><f>IF(C7=&quot;&quot;,&quot;empty&quot;,C7)</f>"#),
        "{road}"
    );
    // Ranges, merges, conditional formatting and the dimension follow Excel's rules.
    assert!(road.contains("AVERAGE(D3:D7)"), "{road}");
    assert!(road.contains(r#"<dimension ref="A1:D7"/>"#), "{road}");
    assert!(road.contains(r#"<mergeCell ref="A3:A5"/>"#), "{road}");
    assert!(
        road.contains(r#"sqref="C3:C7""#) && road.contains(r#"<formula>C3="x"</formula>"#),
        "{road}"
    );
}

#[test]
fn insert_row_updates_other_sheets_defined_names_and_drops_the_calc_chain() {
    let mut source = rows_fixture("SUM(Road!D3:D6)+Road!C9");
    let output = apply_row_ops(&mut source, &[insert_after_row_three()]).unwrap();

    assert!(member(&output, "xl/worksheets/sheet2.xml").contains("SUM(Road!D3:D7)+Road!C10"));
    assert!(member(&output, "xl/workbook.xml").contains("Road!$A$1:$D$7"));
    let mut archive = zip::ZipArchive::new(Cursor::new(output.clone())).unwrap();
    assert!(archive.by_name("xl/calcChain.xml").is_err());
    assert!(!member(&output, "xl/_rels/workbook.xml.rels").contains("calcChain"));
    assert!(!member(&output, "[Content_Types].xml").contains("calcChain"));
    let mut reread = Cursor::new(output);
    let workbook = read_workbook_from_reader(&mut reread).unwrap();
    assert_eq!(
        workbook.sheet("Road").unwrap().value(4, 2),
        "Road 01 (Part 2)"
    );
    assert_eq!(workbook.sheet("Road").unwrap().value(5, 2), "Road 02");
}

#[test]
fn insert_row_refuses_a_shared_formula_on_another_sheet_that_points_at_the_sheet() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Road" sheetId="1" r:id="rId1"/><sheet name="STAT" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let road = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="3"><c r="A3"/></row></sheetData></worksheet>"#
    );
    let stat = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><f t="shared" ref="A1:A2" si="0">Road!A5</f></c></row><row r="2"><c r="A2"><f t="shared" si="0"/></c></row></sheetData></worksheet>"#
    );
    let mut source = fixture_from_members(
        &workbook,
        &relationships,
        [
            ("xl/worksheets/sheet1.xml", road.as_bytes()),
            ("xl/worksheets/sheet2.xml", stat.as_bytes()),
        ],
    );

    let error = apply_row_ops(&mut source, &[insert_after_row_three()]).unwrap_err();

    assert!(error.to_string().contains("shared formula"), "{error}");
}

#[test]
fn set_text_relabels_a_value_cell_but_refuses_a_formula_cell() {
    let mut source = rows_fixture("1");
    let output = apply_row_ops(
        &mut source,
        &[RowOp::SetText {
            sheet: "Road".to_string(),
            row: 3,
            column: 2,
            text: "Road 01 (Part 1)".to_string(),
        }],
    )
    .unwrap();
    assert!(
        member(&output, "xl/worksheets/sheet1.xml")
            .contains(r#"<c r="B3" s="2" t="inlineStr"><is><t>Road 01 (Part 1)</t></is></c>"#)
    );

    let mut source = rows_fixture("1");
    let error = apply_row_ops(
        &mut source,
        &[RowOp::SetText {
            sheet: "Road".to_string(),
            row: 3,
            column: 4,
            text: "x".to_string(),
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("formula"), "{error}");
}

#[test]
fn insert_row_can_precede_the_first_data_row_using_the_row_below_as_template() {
    let mut source = rows_fixture("1");
    let output = apply_row_ops(
        &mut source,
        &[RowOp::InsertRow {
            sheet: "Road".to_string(),
            after_row: 2,
            template_row: 3,
            copy_columns: vec![1],
            texts: vec![CellText {
                column: 2,
                text: "Road 00".to_string(),
            }],
        }],
    )
    .unwrap();
    let road = member(&output, "xl/worksheets/sheet1.xml");

    assert!(road.contains(r#"<row r="3" ht="15">"#), "{road}");
    assert!(
        road.contains(r#"<c r="B3" s="2" t="inlineStr"><is><t>Road 00</t></is></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D3" s="4"><f>IF(C3=&quot;&quot;,&quot;empty&quot;,C3)</f></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="B4" s="2" t="inlineStr"><is><t>Road 01</t></is></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D4" s="4"><f>IF(C4=&quot;&quot;,&quot;empty&quot;,C4)</f>"#),
        "{road}"
    );
}
