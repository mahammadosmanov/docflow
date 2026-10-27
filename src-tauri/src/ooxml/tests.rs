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

mod archive_preflight;
mod namespace_and_relationships;
mod package_and_xml;
mod recalc_and_rich_patches;
mod relationships_and_connections;
mod row_insert_and_formulas;
mod sheet_projection;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const OFFICE_DOCUMENT_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const PACKAGE_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

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

fn formula_fixture(cell_xml: &str) -> (Cursor<Vec<u8>>, GuardedUpdates) {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Road" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="5"><c r="C5" t="inlineStr"><is><t>Completed</t></is></c>{cell_xml}</row></sheetData></worksheet>"#
    );
    let source = fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", worksheet.as_bytes())],
    );
    let updates = BTreeMap::from([(
        "Road".to_string(),
        BTreeMap::from([(
            (5, 25),
            GuardedCellUpdate {
                expected: CellData {
                    value: String::new(),
                    formula: None,
                    style: Some("34".to_string()),
                },
                replacement: CellUpdate::Formula {
                    formula: "COUNTIF(C5:P5,\"Completed\")/6".to_string(),
                    cached: None,
                },
                create_if_absent: false,
            },
        )]),
    )]);
    (source, updates)
}
