use super::*;
use crate::templates::anchors::tests::{workbook_with_sheet, zip};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

fn cell(text: &str) -> String {
    format!("<w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>")
}

/// One filled copy of a small form. Row 4 holds a continued vertical merge.
fn form(contractor: &str, pour: &str, rfi: &str, locked: &str) -> Vec<u8> {
    let rows = [
        [cell("Contractor:"), cell(contractor)],
        [cell("Pour no:"), cell(pour)],
        [cell("RFI:"), cell(&format!("RFI No: {rfi}"))],
        [cell("Fixed:"), cell("same")],
        [
            cell("Merged:"),
            format!(
                "<w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p><w:r><w:t>{locked}</w:t></w:r></w:p></w:tc>"
            ),
        ],
    ]
    .iter()
    .map(|row| format!("<w:tr>{}{}</w:tr>", row[0], row[1]))
    .collect::<String>();
    let body =
        format!(r#"<w:document xmlns:w="{W}"><w:body><w:tbl>{rows}</w:tbl></w:body></w:document>"#);
    zip(&[
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", &body),
    ])
}

fn anchor_cell(suggestion: &CopySuggestion) -> Option<(u32, u32, u32)> {
    let anchor = &suggestion.anchor;
    Some((anchor.table?, anchor.row?, anchor.cell?))
}

#[test]
fn cells_that_change_between_copies_are_suggested_with_a_label_and_samples() {
    let base = form("A Ltd", "P1", "AZE-SUN-INT-QMS-RFI-ELE-0049", "x");
    let copies = vec![
        form("B Ltd", "P2", "AZE-SUN-INT-QMS-RFI-ELE-0050", "y"),
        form("A Ltd", "P3", "AZE-SUN-INT-QMS-RFI-ELE-0051", "z"),
    ];
    let found = compare(FileKind::Docx, &base, &copies).expect("compare");
    let places = found.iter().filter_map(anchor_cell).collect::<Vec<_>>();
    assert_eq!(
        places,
        [(0, 0, 1), (0, 1, 1), (0, 2, 1)],
        "same, locked cells are left out"
    );
    let contractor = &found[0];
    assert_eq!(contractor.label, "Contractor");
    assert_eq!(contractor.samples, ["A Ltd", "B Ltd"]);
    assert_eq!(contractor.value, "A Ltd");
    assert!(!contractor.rfi);
    let rfi = &found[2];
    assert!(rfi.rfi);
    assert_eq!(rfi.value, "RFI No: {rfiNo}");
    assert_eq!(rfi.label, "RFI");
}

#[test]
fn identical_copies_and_wrong_files_give_nothing_or_an_error() {
    let base = form("A", "P", "AZE-SUN-INT-QMS-RFI-ELE-0049", "x");
    assert!(
        compare(FileKind::Docx, &base, std::slice::from_ref(&base))
            .expect("same")
            .is_empty()
    );
    assert!(
        compare(FileKind::Docx, &base, &[])
            .expect("none")
            .is_empty()
    );
    let sheet = format!(r#"<worksheet xmlns="{X}"><sheetData/></worksheet>"#);
    let book = workbook_with_sheet(&sheet, "");
    assert!(compare(FileKind::Docx, &base, &[book]).is_err());
    assert!(compare(FileKind::Docx, &base, &[b"not a zip".to_vec()]).is_err());
}

fn sheet(b1: &str, c1: &str, b2: &str, a3: &str) -> Vec<u8> {
    let a3_cell = if a3.is_empty() {
        String::new()
    } else {
        format!(r#"<row r="3"><c r="A3" t="inlineStr"><is><t>{a3}</t></is></c></row>"#)
    };
    let xml = format!(
        r#"<worksheet xmlns="{X}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name:</t></is></c><c r="B1" t="inlineStr"><is><t>{b1}</t></is></c><c r="C1" t="inlineStr"><is><t>{c1}</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Sum:</t></is></c><c r="B2"><f>1+1</f><v>{b2}</v></c></row>{a3_cell}</sheetData><mergeCells count="1"><mergeCell ref="B1:C1"/></mergeCells></worksheet>"#
    );
    workbook_with_sheet(&xml, "")
}

#[test]
fn excel_suggests_changed_and_newly_filled_cells_but_not_formulas_or_merge_tails() {
    let base = sheet("X", "", "2", "");
    let copies = vec![sheet("Y", "tail", "3", "note")];
    let found = compare(FileKind::Xlsx, &base, &copies).expect("compare");
    let addresses = found
        .iter()
        .filter_map(|item| item.anchor.address.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        addresses,
        ["B1", "A3"],
        "formula B2 and merge tail C1 are skipped"
    );
    assert_eq!(found[0].label, "Name");
    assert_eq!(found[0].samples, ["X", "Y"]);
    assert_eq!(found[1].value, "", "empty in the draft file");
    assert_eq!(found[1].samples, ["note"]);
    assert_eq!(found[0].anchor.sheet.as_deref(), Some("Form"));
}
