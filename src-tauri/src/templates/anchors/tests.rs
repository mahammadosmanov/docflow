use std::io::{Cursor, Read, Write};

use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::*;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

pub(crate) fn zip(parts: &[(&str, &str)]) -> Vec<u8> {
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

pub(crate) fn docx() -> Vec<u8> {
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

pub(crate) fn xlsx(extra_sheet_xml: &str, workbook_extra: &str) -> Vec<u8> {
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" s="2" t="inlineStr"><is><t>Title</t></is></c><c r="B1" s="3"/></row><row r="2"><c r="A2"><f>1+1</f><v>2</v></c><c r="B2" s="4"><v>5</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A1:C1"/></mergeCells>{extra_sheet_xml}</worksheet>"#
    );
    workbook_with_sheet(&sheet, workbook_extra)
}

/// A one-sheet workbook ("Form") whose worksheet part is exactly `sheet`.
pub(crate) fn workbook_with_sheet(sheet: &str, workbook_extra: &str) -> Vec<u8> {
    workbook_with_sheet_parts(sheet, workbook_extra, &[])
}

fn workbook_with_sheet_parts(
    sheet: &str,
    workbook_extra: &str,
    extra_parts: &[(&str, &str)],
) -> Vec<u8> {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Form" sheetId="1" r:id="rId1"/></sheets><calcPr calcId="1"/>{workbook_extra}</workbook>"#
    );
    let mut parts = vec![
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
    ];
    parts.extend_from_slice(extra_parts);
    zip(&parts)
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

fn tc(text: &str) -> String {
    format!("<w:tc><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>{text}</w:t></w:r></w:p></w:tc>")
}

fn rows_docx(template_row: &str) -> Vec<u8> {
    let body = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr>{}</w:tr><w:tr>{}</w:tr><w:tr>{}</w:tr></w:tbl></w:body></w:document>"#,
        tc("Header"),
        template_row,
        tc("Footer")
    );
    zip(&[
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", &body),
    ])
}

fn plain_template_row() -> String {
    format!("{}{}{}{}", tc("a"), tc("b"), tc("c"), tc("mm"))
}

fn repeating(row: u32, columns: u32) -> CellAnchor {
    CellAnchor {
        table: Some(0),
        row: Some(row),
        cell: Some(0),
        repeat_columns: Some(columns),
        ..CellAnchor::default()
    }
}

#[test]
fn repeating_a_word_row_cannot_duplicate_a_bookmark_in_an_unmapped_cell() {
    let bookmark = r#"<w:tc><w:p><w:bookmarkStart w:id="1" w:name="Here"/><w:bookmarkEnd w:id="1"/></w:p></w:tc>"#;
    let row = format!("{}{bookmark}", tc("a"));
    let input = rows_docx(&row);
    let anchor = repeating(1, 1);
    assert!(apply(FileKind::Docx, &input, &[(&anchor, "1\n2")]).is_err());
    let single = apply(FileKind::Docx, &input, &[(&anchor, "1")]).expect("no row insertion");
    assert_eq!(
        part(&single, "word/document.xml")
            .matches("<w:bookmarkStart")
            .count(),
        1
    );
}

#[test]
fn a_repeating_row_becomes_one_row_per_line_and_keeps_the_rest_of_the_row() {
    let anchor = repeating(1, 3);
    let out = apply(
        FileKind::Docx,
        &rows_docx(&plain_template_row()),
        &[(&anchor, "1\tx\tp\n2\ty\tq\n3")],
    )
    .expect("apply");
    let xml = part(&out, "word/document.xml");
    assert_eq!(
        xml.matches("<w:tr>").count(),
        5,
        "header, three rows, footer: {xml}"
    );
    assert_eq!(
        xml.matches(">mm<").count(),
        3,
        "the unit cell is cloned unchanged"
    );
    // Header and footer, ten filled cells; the two cells left empty on the last
    // line hold no run, so they carry no run formatting.
    assert_eq!(
        xml.matches("<w:b/>").count(),
        1 + (3 * 4 - 2) + 1,
        "formatting kept in every filled cell"
    );
    let order = [
        "Header", ">1<", ">x<", ">p<", ">2<", ">y<", ">q<", ">3<", "Footer",
    ]
    .map(|needle| xml.find(needle).expect(needle));
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "rows stay in order: {xml}"
    );
    assert!(
        !xml.contains(">a<") && !xml.contains(">b<") && !xml.contains(">c<"),
        "{xml}"
    );
}

#[test]
fn an_empty_value_keeps_one_empty_row_and_other_cells_still_use_their_own_place() {
    let anchor = repeating(1, 3);
    let single = word(0, 0, 0);
    let out = apply(
        FileKind::Docx,
        &rows_docx(&plain_template_row()),
        &[(&anchor, ""), (&single, "Changed header")],
    )
    .expect("apply");
    let xml = part(&out, "word/document.xml");
    assert_eq!(xml.matches("<w:tr>").count(), 3);
    assert!(
        xml.contains("Changed header") && xml.contains("Footer"),
        "{xml}"
    );
}

#[test]
fn a_repeating_row_that_cannot_be_cloned_safely_is_refused() {
    let wide = apply(
        FileKind::Docx,
        &rows_docx(&plain_template_row()),
        &[(&repeating(1, 3), "1\t2\t3\t4")],
    )
    .expect_err("four values for three columns");
    assert!(wide.contains("columns"), "{wide}");
    let merged = format!(
        "{}{}",
        tc("a"),
        "<w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc>"
    );
    let error = apply(
        FileKind::Docx,
        &rows_docx(&merged),
        &[(&repeating(1, 1), "x")],
    )
    .expect_err("vertical merge");
    assert!(error.contains("vertically merged"), "{error}");
    let drawing = format!("{}<w:tc><w:p><w:r><w:drawing/></w:r></w:p></w:tc>", tc("a"));
    let error = apply(
        FileKind::Docx,
        &rows_docx(&drawing),
        &[(&repeating(1, 2), "x\ty")],
    )
    .expect_err("drawing in a mapped cell");
    assert!(error.contains("cannot be replaced"), "{error}");
    let too_few = apply(
        FileKind::Docx,
        &rows_docx(&tc("a")),
        &[(&repeating(1, 3), "x")],
    )
    .expect_err("row has one cell");
    assert!(too_few.contains("not found"), "{too_few}");
    assert!(
        apply(
            FileKind::Docx,
            &rows_docx(&plain_template_row()),
            &[(&repeating(9, 1), "x")]
        )
        .is_err()
    );
}

#[test]
fn only_word_rows_can_repeat_and_the_column_count_is_bounded() {
    assert!(repeating(1, 3).check_shape().is_ok());
    assert!(repeating(1, 0).check_shape().is_err());
    assert!(repeating(1, 21).check_shape().is_err());
    let excel_rows = CellAnchor {
        repeat_columns: Some(2),
        ..excel("Form", "A1")
    };
    assert!(excel_rows.check_shape().is_ok(), "Excel rows repeat too");
    let too_far = CellAnchor {
        repeat_columns: Some(20),
        ..excel("Form", "XFC1")
    };
    assert!(
        too_far.check_shape().is_err(),
        "the columns must fit on the sheet"
    );
}

fn repeat_sheet(extra_merge: &str, extra_rows: &str) -> Vec<u8> {
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><dimension ref="A1:E3"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Header</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>a</t></is></c><c r="B2" t="inlineStr"><is><t>b</t></is></c><c r="C2" s="1"/><c r="D2" t="inlineStr"><is><t>mm</t></is></c><c r="E2"><f>LEN(D2)</f><v>2</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Footer</t></is></c></row>{extra_rows}</sheetData><mergeCells count="2"><mergeCell ref="B2:C2"/><mergeCell ref="A3:C3"/>{extra_merge}</mergeCells></worksheet>"#
    );
    workbook_with_sheet(&sheet, "")
}

fn excel_rows(address: &str, columns: u32) -> CellAnchor {
    CellAnchor {
        repeat_columns: Some(columns),
        ..excel("Form", address)
    }
}

fn read_form(bytes: &[u8]) -> crate::ooxml::WorksheetData {
    crate::ooxml::read_workbook_from_reader(&mut Cursor::new(bytes))
        .expect("workbook")
        .sheets
        .remove("Form")
        .expect("sheet")
}

#[test]
fn repeating_an_excel_row_refuses_to_shift_or_expand_a_table_below_it() {
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Title</t></is></c></row><row r="2"><c r="A2"/></row><row r="4"><c r="A4"/></row></sheetData><tableParts count="1"><tablePart r:id="table1"/></tableParts></worksheet>"#
    );
    let table = format!(
        r#"<table xmlns="{MAIN}" id="1" name="Results" displayName="Results" ref="A1:A2"><autoFilter ref="A1:A2"/><tableColumns count="1"><tableColumn id="1" name="Title"/></tableColumns></table>"#
    );
    let workbook = workbook_with_sheet_parts(
        &sheet,
        "",
        &[
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="table1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/></Relationships>"#,
            ),
            ("xl/tables/table1.xml", &table),
        ],
    );
    for address in ["A1", "A2"] {
        let anchor = excel_rows(address, 1);
        let error = apply(FileKind::Xlsx, &workbook, &[(&anchor, "1\n2")])
            .expect_err("refuse to shift or expand table");
        assert!(
            error.contains("would shift or expand an Excel table"),
            "{error}"
        );
    }
    assert!(apply(FileKind::Xlsx, &workbook, &[(&excel_rows("A2", 1), "1")]).is_ok());
    let out = apply(FileKind::Xlsx, &workbook, &[(&excel_rows("A4", 1), "1\n2")])
        .expect("insertion below the table");
    assert_eq!(part(&out, "xl/tables/table1.xml"), table);
    assert_eq!(read_form(&out).value(5, 1), "2");
}

#[test]
fn an_excel_row_repeats_with_its_merges_formulas_and_the_rows_below() {
    let anchor = excel_rows("A2", 2);
    let out = apply(
        FileKind::Xlsx,
        &repeat_sheet("", ""),
        &[(&anchor, "1\tx\n2\ty\n3\tz")],
    )
    .expect("apply");
    let sheet = read_form(&out);
    let text = |row: u32, column: u32| sheet.value(row, column).to_string();
    for (row, a, b) in [(2, "1", "x"), (3, "2", "y"), (4, "3", "z")] {
        assert_eq!(
            (text(row, 1), text(row, 2)),
            (a.to_string(), b.to_string()),
            "row {row}"
        );
        assert_eq!(text(row, 4), "mm", "the unit column is cloned, row {row}");
        assert_eq!(
            sheet
                .cell(row, 5)
                .and_then(|cell| cell.formula.clone())
                .as_deref(),
            Some(format!("LEN(D{row})").as_str()),
            "formula translated, row {row}"
        );
    }
    assert_eq!(
        (text(1, 1), text(5, 1)),
        ("Header".to_string(), "Footer".to_string())
    );
    let merges = sheet
        .merged_ranges
        .iter()
        .map(|range| {
            (
                range.first_row,
                range.first_column,
                range.last_row,
                range.last_column,
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    let expected = [(2, 2, 2, 3), (3, 2, 3, 3), (4, 2, 4, 3), (5, 1, 5, 3)]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        merges, expected,
        "each cloned row has its merge, the footer merge moved down"
    );
}

#[test]
fn an_empty_value_keeps_the_excel_row_once_and_two_repeats_do_not_disturb_each_other() {
    let one = apply(
        FileKind::Xlsx,
        &repeat_sheet("", ""),
        &[(&excel_rows("A2", 2), "")],
    )
    .expect("apply");
    let sheet = read_form(&one);
    assert_eq!((sheet.value(2, 1), sheet.value(3, 1)), ("", "Footer"));

    let second = r#"<row r="4"><c r="A4" t="inlineStr"><is><t>q</t></is></c></row>"#;
    let first = excel_rows("A2", 1);
    let lower = excel_rows("A4", 1);
    let out = apply(
        FileKind::Xlsx,
        &repeat_sheet("", second),
        &[(&first, "1\n2"), (&lower, "7\n8\n9")],
    )
    .expect("apply");
    let sheet = read_form(&out);
    let column_a = (1..=8)
        .map(|row| sheet.value(row, 1).to_string())
        .collect::<Vec<_>>();
    assert_eq!(column_a, ["Header", "1", "2", "Footer", "7", "8", "9", ""]);
}

#[test]
fn excel_rows_that_cannot_be_cloned_safely_are_refused() {
    let package = repeat_sheet("", "");
    for (anchor, reason) in [
        (excel_rows("D2", 2), "formula"),
        (excel_rows("A2", 3), "merge"),
        (excel_rows("A9", 1), "not found"),
    ] {
        let error = apply(FileKind::Xlsx, &package, &[(&anchor, "x\ny")]).expect_err(reason);
        assert!(error.contains(reason), "{reason}: {error}");
    }
    let vertical = repeat_sheet(r#"<mergeCell ref="D2:D3"/>"#, "");
    let error =
        apply(FileKind::Xlsx, &vertical, &[(&excel_rows("A2", 1), "x\ny")]).expect_err("vertical");
    assert!(error.contains("vertically merged"), "{error}");
    let wide = apply(
        FileKind::Xlsx,
        &package,
        &[(&excel_rows("A2", 2), "1\t2\t3")],
    )
    .expect_err("wide");
    assert!(wide.contains("columns"), "{wide}");
}

#[test]
fn an_excel_cell_can_get_a_real_number_instead_of_text() {
    let book = xlsx("", "");
    let out = apply_cells(
        FileKind::Xlsx,
        &book,
        &[
            (&excel("Form", "B2"), "3.5", true),
            (&excel("Form", "D5"), "x", false),
        ],
    )
    .expect("apply");
    let xml = part(&out, "xl/worksheets/sheet1.xml");
    assert!(
        xml.contains(r#"r="B2""#) && xml.contains("<v>3.5</v>"),
        "{xml}"
    );
    let cell = xml
        .split(r#"r="B2""#)
        .nth(1)
        .unwrap()
        .split("</c>")
        .next()
        .unwrap();
    assert!(!cell.contains("inlineStr"), "{cell}");
    assert!(xml.contains("inlineStr"), "the text cell stays text: {xml}");
    // The plain entry point still writes text.
    let text = apply(FileKind::Xlsx, &book, &[(&excel("Form", "B2"), "3.5")]).expect("apply");
    assert!(part(&text, "xl/worksheets/sheet1.xml").contains("<is><t>3.5</t></is>"));
}

#[test]
fn a_number_flag_is_ignored_in_word() {
    let anchor = word(0, 0, 1);
    let out = apply_cells(FileKind::Docx, &docx(), &[(&anchor, "7", true)]).expect("apply");
    assert!(part(&out, "word/document.xml").contains(">7</w:t>"));
}
