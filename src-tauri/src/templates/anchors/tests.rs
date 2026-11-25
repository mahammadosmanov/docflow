use std::io::{Cursor, Read, Write};

use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::*;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

pub(in crate::templates) fn zip(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in parts {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("start");
        writer.write_all(body.as_bytes()).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut text = String::new();
    archive
        .by_name(name)
        .expect("part")
        .read_to_string(&mut text)
        .expect("read");
    text
}

const DOCX_BODY: &str = concat!(
    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
    r#"<w:tbl><w:tblPr/><w:tblGrid/>"#,
    r#"<w:tr><w:trPr/><w:tc><w:tcPr><w:tcW w:w="1" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Label</w:t></w:r></w:p></w:tc>"#,
    r#"<w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:pPr><w:jc w:val="center"/></w:pPr>"#,
    r#"<w:r><w:rPr><w:b/></w:rPr><w:t>Old</w:t></w:r><w:r><w:t> more</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:t>second paragraph</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"<w:tr><w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p/></w:tc>"#,
    r#"<w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>"#,
    r#"<w:tc><w:p><w:r><w:drawing/></w:r></w:p></w:tc></w:tr></w:tbl>"#,
    r#"<w:p><w:r><w:t>After table</w:t></w:r></w:p></w:body></w:document>"#
);

pub(in crate::templates) fn docx() -> Vec<u8> {
    zip(&[
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", DOCX_BODY),
    ])
}

fn word(table: u32, row: u32, cell: u32) -> CellAnchor {
    CellAnchor {
        table: Some(table),
        row: Some(row),
        cell: Some(cell),
        ..CellAnchor::default()
    }
}

fn excel(sheet: &str, address: &str) -> CellAnchor {
    CellAnchor {
        sheet: Some(sheet.to_string()),
        address: Some(address.to_string()),
        ..CellAnchor::default()
    }
}

pub(in crate::templates) fn xlsx(extra_sheet_xml: &str, workbook_extra: &str) -> Vec<u8> {
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" s="2" t="inlineStr"><is><t>Title</t></is></c><c r="B1" s="3"/></row><row r="2"><c r="A2"><f>1+1</f><v>2</v></c><c r="B2" s="4"><v>5</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A1:C1"/></mergeCells>{extra_sheet_xml}</worksheet>"#
    );
    workbook_with_sheet(&sheet, workbook_extra)
}

/// A one-sheet workbook ("Form") whose worksheet part is exactly `sheet`.
pub(in crate::templates) fn workbook_with_sheet(sheet: &str, workbook_extra: &str) -> Vec<u8> {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Form" sheetId="1" r:id="rId1"/></sheets><calcPr calcId="1"/>{workbook_extra}</workbook>"#
    );
    zip(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        ("xl/workbook.xml", &workbook),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        ("xl/worksheets/sheet1.xml", sheet),
    ])
}

#[test]
fn a_word_cell_gets_the_text_and_keeps_its_own_formatting() {
    let anchor = word(0, 0, 1);
    let out = apply(
        FileKind::Docx,
        &docx(),
        &[(&anchor, "New & <value>\nline 2")],
    )
    .expect("apply");
    let xml = part(&out, "word/document.xml");
    assert!(xml.contains(r#"<w:gridSpan w:val="2"/>"#), "{xml}");
    assert!(xml.contains(r#"<w:jc w:val="center"/>"#), "{xml}");
    assert!(xml.contains("<w:b/>"), "first run formatting kept: {xml}");
    assert!(xml.contains("New &amp; &lt;value&gt;"), "{xml}");
    assert!(xml.contains("<w:br/>"), "{xml}");
    assert!(
        !xml.contains("Old") && !xml.contains("second paragraph"),
        "{xml}"
    );
    assert!(
        xml.contains("Label") && xml.contains("After table"),
        "{xml}"
    );
    assert_eq!(part(&out, "[Content_Types].xml"), "<Types/>");
}

#[test]
fn word_cells_that_cannot_be_written_safely_are_refused() {
    for (anchor, reason) in [
        (word(0, 1, 1), "vertically merged"),
        (word(0, 1, 2), "cannot be replaced"),
        (word(1, 0, 0), "not found"),
        (word(0, 9, 0), "not found"),
        (word(0, 0, 9), "not found"),
    ] {
        let error = apply(FileKind::Docx, &docx(), &[(&anchor, "x")]).expect_err(reason);
        assert!(error.contains(reason), "{reason}: {error}");
    }
}

#[test]
fn an_excel_merged_top_left_cell_is_written_with_its_style() {
    let anchor = excel("Form", "A1");
    let out = apply(FileKind::Xlsx, &xlsx("", ""), &[(&anchor, "RFI text")]).expect("apply");
    let sheet = part(&out, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("RFI text"), "{sheet}");
    assert!(sheet.contains(r#"s="2""#), "style kept: {sheet}");
    assert!(
        sheet.contains(r#"s="4""#) && sheet.contains("<v>5</v>"),
        "{sheet}"
    );
    assert!(part(&out, "xl/workbook.xml").contains(r#"fullCalcOnLoad="1""#));
}

#[test]
fn excel_cells_that_cannot_be_written_safely_are_refused() {
    let plain = xlsx("", "");
    for (anchor, reason) in [
        (excel("Form", "B1"), "merge"),
        (excel("Form", "A2"), "formula"),
        (excel("Other", "A1"), "not found"),
        (excel("Form", "a1"), "invalid"),
    ] {
        let error = apply(FileKind::Xlsx, &plain, &[(&anchor, "x")]).expect_err(reason);
        assert!(error.contains(reason), "{reason}: {error}");
    }
    let protected = xlsx(r#"<sheetProtection sheet="1"/>"#, "");
    let error =
        apply(FileKind::Xlsx, &protected, &[(&excel("Form", "B2"), "x")]).expect_err("protected");
    assert!(error.contains("protected"), "{error}");
}

#[test]
fn an_anchor_must_match_the_file_kind() {
    assert!(apply(FileKind::Xlsx, &xlsx("", ""), &[(&word(0, 0, 0), "x")]).is_err());
    assert!(apply(FileKind::Docx, &docx(), &[(&excel("Form", "A1"), "x")]).is_err());
    assert!(word(0, 0, 0).check_shape().is_ok());
    assert!(excel("Form", "A1").check_shape().is_ok());
    assert!(CellAnchor::default().check_shape().is_err());
    let mixed = CellAnchor {
        table: Some(0),
        ..excel("Form", "A1")
    };
    assert!(mixed.check_shape().is_err());
}
