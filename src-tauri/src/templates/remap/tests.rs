use super::*;
use crate::templates::anchors::tests::{workbook_with_sheet, zip};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

fn cell(text: &str) -> String {
    format!("<w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>")
}

fn table(rows: &[(&str, &str)]) -> Vec<u8> {
    let body: String = rows
        .iter()
        .map(|(label, value)| format!("<w:tr>{}{}</w:tr>", cell(label), cell(value)))
        .collect();
    let xml =
        format!(r#"<w:document xmlns:w="{W}"><w:body><w:tbl>{body}</w:tbl></w:body></w:document>"#);
    zip(&[
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", &xml),
    ])
}

fn word(row: u32) -> (String, CellAnchor) {
    (
        "poured".to_string(),
        CellAnchor {
            table: Some(0),
            row: Some(row),
            cell: Some(1),
            ..CellAnchor::default()
        },
    )
}

fn run(old: &[u8], new: &[u8], field: (String, CellAnchor)) -> AnchorCheck {
    check(FileKind::Docx, old, new, &[field])
        .expect("check")
        .remove(0)
}

#[test]
fn a_cell_beside_the_same_label_is_found() {
    let old = table(&[("Date:", "01.01"), ("Pour no:", "P1")]);
    let new = table(&[("Date:", "05.05"), ("Pour no:", "P9")]);
    let result = run(&old, &new, word(1));
    assert_eq!(
        result.status,
        CheckStatus::Found,
        "values change, labels do not"
    );
    assert_eq!(result.key, "poured");
    assert!(result.moved_to.is_none());
}

#[test]
fn a_cell_whose_label_moved_follows_it() {
    let old = table(&[("Date:", ""), ("Pour no:", "P1")]);
    let new = table(&[
        ("Header", ""),
        ("Date:", ""),
        ("Other:", ""),
        ("Pour no:", "P2"),
    ]);
    let mut field = word(1);
    field.1.repeat_columns = Some(1);
    let result = run(&old, &new, field);
    assert_eq!(result.status, CheckStatus::Moved);
    let to = result.moved_to.expect("new place");
    assert_eq!((to.table, to.row, to.cell), (Some(0), Some(3), Some(1)));
    assert_eq!(to.repeat_columns, Some(1));
}

#[test]
fn a_label_that_changed_or_repeats_is_missing() {
    let old = table(&[("Pour no:", "P1")]);
    assert_eq!(
        run(&old, &table(&[("Pour code:", "P1")]), word(0)).status,
        CheckStatus::Missing
    );
    let twice = table(&[("Other:", ""), ("Pour no:", "a"), ("Pour no:", "b")]);
    assert_eq!(
        run(&old, &twice, word(0)).status,
        CheckStatus::Missing,
        "two candidates is no answer"
    );
    assert_eq!(run(&old, &table(&[]), word(0)).status, CheckStatus::Missing);
}

fn sheet(rows: &[(u32, &str, &str)]) -> Vec<u8> {
    let body: String = rows
        .iter()
        .map(|(row, label, value)| {
            format!(
                r#"<row r="{row}"><c r="A{row}" t="inlineStr"><is><t>{label}</t></is></c><c r="B{row}" t="inlineStr"><is><t>{value}</t></is></c></row>"#
            )
        })
        .collect();
    workbook_with_sheet(
        &format!(r#"<worksheet xmlns="{X}"><sheetData>{body}</sheetData></worksheet>"#),
        "",
    )
}

#[test]
fn an_excel_cell_follows_its_label_down_the_sheet() {
    let old = sheet(&[(1, "Name:", "X")]);
    let new = sheet(&[(1, "Title", ""), (2, "Other:", ""), (3, "Name:", "Y")]);
    let field = (
        "name".to_string(),
        CellAnchor {
            sheet: Some("Form".to_string()),
            address: Some("B1".to_string()),
            ..CellAnchor::default()
        },
    );
    let result = check(FileKind::Xlsx, &old, &new, std::slice::from_ref(&field))
        .expect("check")
        .remove(0);
    assert_eq!(result.status, CheckStatus::Moved);
    assert_eq!(
        result.moved_to.expect("moved").address.as_deref(),
        Some("B3")
    );
    let same = check(FileKind::Xlsx, &old, &old, &[field])
        .expect("check")
        .remove(0);
    assert_eq!(same.status, CheckStatus::Found);
    let broken = check(FileKind::Xlsx, &old, b"nope", &[]);
    assert!(broken.is_err());
}
