use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use docflow_lib::{
    fs_ops::file_sha256,
    ooxml::{CellUpdate, patch_workbook_sheets},
};
use tempfile::TempDir;
use zip::{CompressionMethod, DateTime, System, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

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
