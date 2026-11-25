use serde_json::{Value, json};

use super::*;
use crate::templates::anchors::tests::{docx, xlsx, zip};

fn view(kind: FileKind, bytes: &[u8]) -> Value {
    serde_json::to_value(cells(kind, bytes).expect("structure")).expect("json")
}

#[test]
fn a_word_file_lists_tables_rows_cells_and_why_one_cannot_be_marked() {
    let value = view(FileKind::Docx, &docx());
    assert_eq!(value["kind"], "docx");
    let table = &value["tables"][0];
    let label = &table["rows"][0]["cells"][0];
    assert_eq!(label["text"], "Label");
    assert_eq!(label["problem"], Value::Null);
    let wide = &table["rows"][0]["cells"][1];
    assert_eq!(wide["text"], "Old more\nsecond paragraph");
    assert_eq!(wide["colSpan"], 2);
    assert_eq!(table["rows"][1]["cells"][1]["problem"], "verticalMerge");
    assert_eq!(table["rows"][1]["cells"][2]["problem"], "content");
}

#[test]
fn an_excel_file_lists_sheets_cells_formulas_and_merges() {
    let value = view(FileKind::Xlsx, &xlsx("", ""));
    assert_eq!(value["kind"], "xlsx");
    let sheet = &value["sheets"][0];
    assert_eq!(sheet["name"], "Form");
    assert_eq!(sheet["maxRow"], 2);
    assert_eq!(sheet["truncated"], false);
    let cells = sheet["cells"].as_array().expect("cells");
    let at = |row: u32, column: u32| {
        cells
            .iter()
            .find(|cell| cell["row"] == row && cell["column"] == column)
            .expect("cell")
    };
    assert_eq!(at(1, 1)["text"], "Title");
    assert_eq!(at(2, 1)["formula"], true);
    assert_eq!(at(2, 2)["formula"], false);
    assert_eq!(
        sheet["merges"][0],
        json!({"row": 1, "column": 1, "rowSpan": 1, "colSpan": 3})
    );
}

#[test]
fn a_big_sheet_is_cut_and_says_so() {
    let rows: String = (1..=450)
        .map(|row| format!(r#"<row r="{row}"><c r="A{row}"><v>{row}</v></c></row>"#))
        .collect();
    let sheet = format!(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#
    );
    let package = zip(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Big" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        ("xl/worksheets/sheet1.xml", &sheet),
    ]);
    let value = view(FileKind::Xlsx, &package);
    assert_eq!(value["sheets"][0]["truncated"], true);
    assert_eq!(value["sheets"][0]["maxRow"], 300);
    assert!(value["sheets"][0]["cells"].as_array().expect("cells").len() <= 300);
}

#[test]
fn a_cell_holding_an_rfi_code_is_suggested_with_the_label_kept() {
    let body = concat!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl>"#,
        r#"<w:tr><w:tc><w:p><w:r><w:t>Title</w:t></w:r></w:p></w:tc>"#,
        r#"<w:tc><w:p><w:r><w:t>RFI No: AZE-SUN-INT-QMS-RFI-ELE-0049</w:t></w:r></w:p></w:tc></w:tr>"#,
        r#"</w:tbl></w:body></w:document>"#
    );
    let package = zip(&[
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", body),
    ]);
    let value = view(FileKind::Docx, &package);
    let suggestion = &value["suggestions"][0];
    assert_eq!(suggestion["key"], "rfiNo");
    assert_eq!(suggestion["value"], "RFI No: {rfiNo}");
    assert_eq!(
        suggestion["anchor"],
        json!({"table": 0, "row": 0, "cell": 1})
    );
    assert!(
        view(FileKind::Docx, &docx())["suggestions"]
            .as_array()
            .expect("list")
            .is_empty()
    );
}
