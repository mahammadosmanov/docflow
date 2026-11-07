use std::io::{Cursor, Write};

use serde_json::json;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use super::*;

fn cell(reference: &str, value: &str) -> String {
    if value.parse::<f64>().is_ok() {
        format!(r#"<c r="{reference}"><v>{value}</v></c>"#)
    } else {
        format!(r#"<c r="{reference}" t="inlineStr"><is><t>{value}</t></is></c>"#)
    }
}

fn rfi_log(rows: &[&[&str]]) -> Vec<u8> {
    let mut sheet = String::new();
    for (index, values) in rows.iter().enumerate() {
        let row = index + 1;
        sheet.push_str(&format!(r#"<row r="{row}">"#));
        for (column, value) in values.iter().enumerate() {
            let letter = (b'A' + column as u8) as char;
            sheet.push_str(&cell(&format!("{letter}{row}"), value));
        }
        sheet.push_str("</row>");
    }
    let worksheet = format!(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{sheet}</sheetData></worksheet>"#
    );
    let workbook = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="ELE" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let package = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, contents) in [
        ("[Content_Types].xml", "content-types"),
        ("_rels/.rels", package),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", relationships),
        ("xl/worksheets/sheet1.xml", worksheet.as_str()),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

const HEADER: &[&str] = &[
    "RFI No.",
    "ITP No:",
    "Item No.",
    "INSPECTION DESCRIPTION",
    "FORMS",
    "TAG NO",
    "AREA",
    "DATE OF ISSUE",
    "INSPECTION DATE",
    "EXPIRY DATE",
    "INSPECTION START TIME",
    "INSP. TYPE AZFEN",
    "INSP. TYPE INTEC",
];

fn log() -> BTreeMap<String, LogRow> {
    let bytes = rfi_log(&[
        &["SUNRISE PROJECT"],
        HEADER,
        // 46333 = 07.11.2026, 0.395833 = 09:30
        &[
            "AZE-SUN-INT-QMS-RFI-ELE-0059",
            "AZE\u{02D7}SUN\u{02D7}INT\u{02D7}QMS\u{02D7}ITP\u{02D7}ELE\u{02D7}0005",
            "ELE-5.2.6",
            "Verify insulation",
            "CON-ELE-002",
            "PCS-16",
            "North Site",
            "46333",
            "46334",
            "46335",
            "0.3958333333",
            "H",
            "W",
        ],
        &["AZE-SUN-INT-QMS-RFI-ELE-0059", "duplicate row is ignored"],
    ]);
    read_log_rows(
        &mut Cursor::new(bytes),
        &[
            "AZE-SUN-INT-QMS-RFI-ELE-0059".to_string(),
            "AZE-SUN-INT-QMS-RFI-ELE-0099".to_string(),
        ],
    )
    .expect("log rows")
}

fn map() -> FieldMap {
    FieldMap::parse(&json!({
        "schema": 1,
        "fileName": "{{rfiNo}}",
        "fields": [
            {"key": "rfiNo", "label": "RFI No", "value": "{rfiNo}"},
            {"key": "date", "label": "Date", "value": "{dateOfIssue}"},
            {"key": "itp", "label": "ITP", "value": "{itpNo}"},
            {"key": "area", "label": "Area", "value": "({area}) {tagNo}", "options": ["{tagNo}"]},
            {"key": "time", "label": "Time", "value": "{startTime}"},
            {"key": "stage", "label": "Stage", "value": ""},
            {"key": "issuer", "label": "Issuer", "value": "Mahammad Osmanov",
             "options": ["Mahammad Osmanov", "Emin Dashdamirov"]}
        ],
        "presets": [{"name": "ITP 5 · 2.5, 2.6", "values": {"stage": "2.5, 2.6"}}]
    }))
    .expect("map")
}

#[test]
fn log_rows_are_read_from_the_discipline_sheet_with_cover_formats() {
    let rows = log();
    assert_eq!(rows.len(), 1, "missing RFIs are absent");
    let row = &rows["AZE-SUN-INT-QMS-RFI-ELE-0059"];
    assert_eq!(row["itpNo"], "AZE-SUN-INT-QMS-ITP-ELE-0005");
    assert_eq!(row["dateOfIssue"], "07.11.2026");
    assert_eq!(row["inspectionDate"], "08.11.2026");
    assert_eq!(row["startTime"], "09:30");
    assert_eq!(row["azfenType"], "H");
    assert_eq!(row["description"], "Verify insulation");
}

#[test]
fn rows_start_from_the_log_and_the_editor_offers_presets_and_choices() {
    let rfis = vec!["AZE-SUN-INT-QMS-RFI-ELE-0059".to_string()];
    let rows = initial_rows(&map(), &log(), &rfis);
    assert_eq!(rows[0]["area"], "(North Site) PCS-16");
    assert_eq!(rows[0]["date"], "07.11.2026");
    assert_eq!(rows[0]["_preset"], "");
    let editor = editor(&map(), rows);
    assert_eq!(
        editor["choices"]["_preset"],
        json!(["", "ITP 5 · 2.5, 2.6"])
    );
    assert_eq!(
        editor["autoFill"]["values"]["ITP 5 · 2.5, 2.6"]["stage"],
        "2.5, 2.6"
    );
    assert_eq!(
        editor["autocomplete"]["issuer"],
        json!(["Mahammad Osmanov", "Emin Dashdamirov"])
    );
    assert_eq!(editor["columns"][1]["labelKey"], "RFI No");
}

#[test]
fn edited_rows_must_keep_their_rfis_and_fields() {
    let rfis = vec!["AZE-SUN-INT-QMS-RFI-ELE-0059".to_string()];
    let rows = initial_rows(&map(), &log(), &rfis);
    let mut edited = rows.clone();
    edited[0]["stage"] = json!("2.5, 2.6");
    assert_eq!(
        accept_rows(&rows, &edited).expect("accepted")[0]["stage"],
        "2.5, 2.6"
    );
    let mut renamed = rows.clone();
    renamed[0]["_rowId"] = json!("AZE-SUN-INT-QMS-RFI-ELE-0001");
    assert!(accept_rows(&rows, &renamed).is_err());
    let mut extra = rows.clone();
    extra[0]["new"] = json!("x");
    assert!(accept_rows(&rows, &extra).is_err());
    let mut number = rows.clone();
    number[0]["stage"] = json!(5);
    assert!(accept_rows(&rows, &number).is_err());
    assert!(accept_rows(&rows, &[]).is_err());
}

#[test]
fn rendering_fills_the_document_and_names_it_by_rfi() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in [
        ("[Content_Types].xml", "<Types/>"),
        (
            "word/document.xml",
            "<w:document><w:body><w:p><w:r><w:t>RFI No: {{rfiNo}} Stage {{stage}} {{issuer}}</w:t></w:r></w:p></w:body></w:document>",
        ),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    let template = writer.finish().unwrap().into_inner();
    let row = json!({"_rowId": "AZE-SUN-INT-QMS-RFI-ELE-0059", "_preset": "x",
                     "rfiNo": "AZE-SUN-INT-QMS-RFI-ELE-0059", "stage": "2.5, 2.6", "issuer": "Emin Dashdamirov"});
    let (file, missing) = render(FileKind::Docx, &template, &map(), &row).expect("render");
    assert_eq!(file.filename, "AZE-SUN-INT-QMS-RFI-ELE-0059.docx");
    assert!(missing.is_empty());
    let keys = crate::templates::field_map::placeholders(FileKind::Docx, &file.bytes).unwrap();
    assert!(keys.is_empty());
}

#[test]
fn file_names_are_made_safe() {
    assert_eq!(safe_file_stem(" a/b:c*? . "), "a_b_c__");
    assert!(requested_rfis("", Some("ELE")).is_err());
    assert!(requested_rfis("not an rfi", Some("ELE")).is_err());
    assert_eq!(
        requested_rfis(
            "AZE-SUN-INT-QMS-RFI-ELE-0059, AZE-SUN-INT-QMS-RFI-ELE-0059",
            None
        )
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn short_rfi_numbers_use_the_template_discipline() {
    assert_eq!(
        requested_rfis("49\n58; MEC-7, civ 12", Some("ELE")).unwrap(),
        [
            "AZE-SUN-INT-QMS-RFI-ELE-0049",
            "AZE-SUN-INT-QMS-RFI-ELE-0058",
            "AZE-SUN-INT-QMS-RFI-MEC-0007",
            "AZE-SUN-INT-QMS-RFI-CIV-0012",
        ]
    );
    assert!(
        requested_rfis("49", None).is_err(),
        "no discipline to infer"
    );
    assert!(
        requested_rfis("49", Some("GEN")).is_err(),
        "only RFI disciplines"
    );
    assert!(requested_rfis("12345", Some("ELE")).is_err());
}
