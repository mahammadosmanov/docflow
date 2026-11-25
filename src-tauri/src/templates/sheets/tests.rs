use std::io::{Cursor, Read};

use zip::ZipArchive;

use super::*;
use crate::templates::anchors::tests::zip;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn sheet(body: &str) -> String {
    format!(r#"<worksheet xmlns="{MAIN}"><sheetData>{body}</sheetData></worksheet>"#)
}

/// Form, Photos (drawing and image), Summary. `summary_formula` is Summary!A1.
pub(crate) fn book(summary_formula: &str, global_name: &str, form_shares_image: bool) -> Vec<u8> {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><bookViews><workbookView activeTab="1" firstSheet="1"/></bookViews><sheets><sheet name="Form" sheetId="1" r:id="rId1"/><sheet name="Photos" sheetId="2" r:id="rId2"/><sheet name="Summary" sheetId="3" r:id="rId3"/></sheets><definedNames><definedName name="_xlnm.Print_Area" localSheetId="1">Photos!$A$1:$C$5</definedName><definedName name="_xlnm.Print_Area" localSheetId="2">Summary!$A$1:$B$2</definedName>{global_name}</definedNames><calcPr calcId="1"/></workbook>"#
    );
    let rels = |items: &[(&str, &str, &str)]| {
        let body: String = items
            .iter()
            .map(|(id, kind, target)| {
                format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{target}"/>"#)
            })
            .collect();
        format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#
        )
    };
    let formula = if summary_formula.is_empty() {
        String::new()
    } else {
        format!(r#"<row r="1"><c r="A1"><f>{summary_formula}</f><v>1</v></c></row>"#)
    };
    let parts = vec![
        ("[Content_Types].xml".to_string(), r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet3.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/calcChain.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml"/></Types>"#.to_string()),
        ("_rels/.rels".to_string(), rels(&[("rId1", "officeDocument", "xl/workbook.xml")])),
        ("xl/workbook.xml".to_string(), workbook),
        ("xl/_rels/workbook.xml.rels".to_string(), rels(&[("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "worksheet", "worksheets/sheet2.xml"), ("rId3", "worksheet", "worksheets/sheet3.xml"), ("rId4", "calcChain", "calcChain.xml")])),
        ("xl/worksheets/sheet1.xml".to_string(), sheet(r#"<row r="1"><c r="A1" t="inlineStr"><is><t>Form</t></is></c></row>"#)),
        ("xl/worksheets/sheet2.xml".to_string(), sheet("")),
        ("xl/worksheets/sheet3.xml".to_string(), sheet(&formula)),
        ("xl/worksheets/_rels/sheet2.xml.rels".to_string(), rels(&[("rId1", "drawing", "../drawings/drawing1.xml")])),
        ("xl/drawings/drawing1.xml".to_string(), "<drawing/>".to_string()),
        ("xl/drawings/_rels/drawing1.xml.rels".to_string(), rels(&[("rId1", "image", "../media/image1.png")])),
        ("xl/media/image1.png".to_string(), "PNG".to_string()),
        ("xl/calcChain.xml".to_string(), "<calcChain/>".to_string()),
    ];
    let mut parts = parts;
    if form_shares_image {
        parts.push((
            "xl/worksheets/_rels/sheet1.xml.rels".to_string(),
            rels(&[("rId1", "image", "../media/image1.png")]),
        ));
    }
    let borrowed = parts
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_str()))
        .collect::<Vec<_>>();
    zip(&borrowed)
}

fn names(bytes: &[u8]) -> Vec<String> {
    let archive = ZipArchive::new(Cursor::new(bytes)).expect("zip");
    archive
        .file_names()
        .filter_map(Result::ok)
        .map(|name| name.into_owned())
        .collect()
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

fn sheet_names(bytes: &[u8]) -> Vec<String> {
    crate::ooxml::ordered_workbook_discovery(&mut Cursor::new(bytes))
        .expect("discovery")
        .worksheets
        .into_iter()
        .map(|sheet| sheet.name)
        .collect()
}

fn one(name: &str) -> Vec<String> {
    vec![name.to_string()]
}

#[test]
fn a_sheet_goes_with_its_drawing_image_names_and_calc_chain() {
    let out = remove_sheets(&book("", "", false), &one("Photos")).expect("removed");
    assert_eq!(sheet_names(&out), ["Form", "Summary"]);
    let kept = names(&out);
    for gone in [
        "xl/worksheets/sheet2.xml",
        "xl/worksheets/_rels/sheet2.xml.rels",
        "xl/drawings/drawing1.xml",
        "xl/drawings/_rels/drawing1.xml.rels",
        "xl/media/image1.png",
        "xl/calcChain.xml",
    ] {
        assert!(
            !kept.iter().any(|name| name == gone),
            "{gone} should be gone"
        );
    }
    assert!(kept.iter().any(|name| name == "xl/worksheets/sheet1.xml"));
    assert_eq!(
        part(&out, "xl/worksheets/sheet1.xml"),
        part(&book("", "", false), "xl/worksheets/sheet1.xml")
    );
    let content_types = part(&out, "[Content_Types].xml");
    assert!(
        !content_types.contains("sheet2.xml")
            && !content_types.contains("drawing1")
            && !content_types.contains("calcChain")
    );
    assert!(content_types.contains("sheet3.xml"));
    let rels = part(&out, "xl/_rels/workbook.xml.rels");
    assert!(
        !rels.contains("sheet2.xml") && !rels.contains("calcChain") && rels.contains("sheet3.xml")
    );
    let workbook = part(&out, "xl/workbook.xml");
    assert!(
        !workbook.contains("Photos!"),
        "the removed sheet's print area is dropped: {workbook}"
    );
    assert!(
        workbook.contains(r#"localSheetId="1">Summary!"#),
        "Summary moves from index 2 to 1: {workbook}"
    );
    assert!(
        workbook.contains(r#"activeTab="0""#) && workbook.contains(r#"firstSheet="0""#),
        "{workbook}"
    );
}

#[test]
fn an_image_a_kept_sheet_still_uses_stays() {
    let out = remove_sheets(&book("", "", true), &one("Photos")).expect("removed");
    let kept = names(&out);
    assert!(kept.iter().any(|name| name == "xl/media/image1.png"));
    assert!(!kept.iter().any(|name| name == "xl/drawings/drawing1.xml"));
}

#[test]
fn a_sheet_that_a_kept_formula_or_name_uses_is_not_removed() {
    let error =
        remove_sheets(&book("Photos!A1+1", "", false), &one("Photos")).expect_err("formula");
    assert!(error.contains("used by"), "{error}");
    assert!(
        error.contains("Photos") && error.contains("Summary"),
        "{error}"
    );
    let global = r#"<definedName name="Counts">Photos!$A$1</definedName>"#;
    let error = remove_sheets(&book("", global, false), &one("Photos")).expect_err("name");
    assert!(error.contains("used by"), "{error}");
    assert!(
        remove_sheets(&book("Photos!A1", "", false), &one("Summary")).is_ok(),
        "the formula sits on the removed sheet"
    );
}

#[test]
fn unknown_sheets_and_removing_everything_are_refused() {
    let package = book("", "", false);
    assert!(remove_sheets(&package, &one("Nope")).is_err());
    let all = ["Form", "Photos", "Summary"].map(str::to_string).to_vec();
    assert!(
        remove_sheets(&package, &all).is_err(),
        "one sheet must stay"
    );
    assert_eq!(
        remove_sheets(&package, &[]).expect("nothing to do"),
        package
    );
}
