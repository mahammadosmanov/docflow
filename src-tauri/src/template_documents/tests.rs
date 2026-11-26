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
    book_named("ELE", rows)
}

fn book_named(sheet_name: &str, rows: &[&[&str]]) -> Vec<u8> {
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
    let workbook = format!(
        r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{sheet_name}" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let package = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, contents) in [
        ("[Content_Types].xml", "content-types"),
        ("_rels/.rels", package),
        ("xl/workbook.xml", workbook.as_str()),
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

fn sheet_map() -> FieldMap {
    FieldMap::parse(&json!({
        "schema": 2,
        "slots": [{"key": "name"}],
        "optionalSheets": ["Photos"]
    }))
    .expect("map")
}

#[test]
fn an_optional_sheet_is_a_checkbox_per_document_and_can_be_left_out() {
    let map = sheet_map();
    let log = BTreeMap::from([("RFI-1".to_string(), LogRow::new())]);
    let rows = initial_rows(&map, &log, &["RFI-1".to_string()]);
    assert_eq!(rows[0]["_sheet0"], json!(true));
    let editor = editor(&map, rows.clone());
    let column = editor["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .find(|column| column["key"] == "_sheet0")
        .expect("sheet column");
    assert_eq!(column["kind"], "bool");
    assert_eq!(column["labelKey"], "Photos");

    let mut edited = rows.clone();
    edited[0]["_sheet0"] = json!(false);
    assert!(accept_rows(&rows, &edited).is_ok(), "a checkbox may change");
    edited[0]["name"] = json!(true);
    assert!(
        accept_rows(&rows, &edited).is_err(),
        "other cells stay text"
    );

    let book = crate::templates::sheets::tests::book("", "", false);
    let keep = render(FileKind::Xlsx, &book, &map, &rows[0])
        .expect("render")
        .0;
    let leave_out = render(FileKind::Xlsx, &book, &map, &edited_with(&rows[0], false))
        .expect("render")
        .0;
    let count = |bytes: &[u8]| {
        crate::ooxml::ordered_workbook_discovery(&mut Cursor::new(bytes))
            .expect("discovery")
            .worksheets
            .len()
    };
    assert_eq!(count(&keep.bytes), 3);
    assert_eq!(count(&leave_out.bytes), 2);
}

fn edited_with(row: &Value, include: bool) -> Value {
    let mut row = row.clone();
    row["_sheet0"] = json!(include);
    row
}

fn docx_with(text: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in [
        ("[Content_Types].xml", "<Types/>".to_string()),
        (
            "word/document.xml",
            format!(
                "<w:document><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"
            ),
        ),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn typed_map() -> FieldMap {
    FieldMap::parse(&json!({
        "schema": 2,
        "fileName": "{{no}}",
        "slots": [
            {"key": "no", "type": "number", "source": {"kind": "fixed", "value": "7"}},
            {"key": "day", "type": "date", "format": "yyyy-MM-dd",
             "source": {"kind": "log", "token": "dateOfIssue"}},
            {"key": "ok", "type": "checkbox", "format": "Yes|No",
             "source": {"kind": "fixed", "value": "yes"}},
            {"key": "note"}
        ]
    }))
    .expect("typed map")
}

#[test]
fn typed_slots_start_typed_and_the_table_edits_them() {
    let rfis = vec!["AZE-SUN-INT-QMS-RFI-ELE-0059".to_string()];
    let rows = initial_rows(&typed_map(), &log(), &rfis);
    assert_eq!(rows[0]["no"], "7");
    assert_eq!(rows[0]["day"], "07.11.2026");
    assert_eq!(rows[0]["ok"], json!(true));
    assert_eq!(rows[0]["note"], "");
    let editor = editor(&typed_map(), rows.clone());
    let kinds = editor["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|column| column["kind"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["text", "text", "bool", "text"]);

    let mut edited = rows.clone();
    edited[0]["ok"] = json!(false);
    assert!(accept_rows(&rows, &edited).is_ok());
    edited[0]["ok"] = json!("no");
    assert!(
        accept_rows(&rows, &edited).is_err(),
        "a checkbox stays a boolean"
    );
}

#[test]
fn typed_values_are_checked_and_written_in_their_format() {
    let template = docx_with("{{no}} {{day}} {{ok}} {{note}}");
    let row = json!({"_rowId": "d1", "no": "3,5", "day": "07.11.2026", "ok": false, "note": "x"});
    let (file, _) = render(FileKind::Docx, &template, &typed_map(), &row).expect("render");
    let mut archive = zip::ZipArchive::new(Cursor::new(file.bytes.clone())).unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("word/document.xml").unwrap(),
        &mut text,
    )
    .unwrap();
    assert!(text.contains("3,5 2026-11-07 No x"), "{text}");
    assert_eq!(file.filename, "3,5.docx");

    let bad = json!({"_rowId": "d1", "no": "abc", "day": "07.11.2026", "ok": true, "note": ""});
    let error = render(FileKind::Docx, &template, &typed_map(), &bad)
        .err()
        .expect("number");
    assert!(error.contains("must be a number"), "{error}");
    let bad = json!({"_rowId": "d1", "no": "1", "day": "31.02.2026", "ok": true, "note": ""});
    assert!(render(FileKind::Docx, &template, &typed_map(), &bad).is_err());
}

#[test]
fn free_mode_makes_numbered_empty_documents_with_only_today_known() {
    let map = map();
    let rows = free_rows(&map, 3);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["_rowId"], "document-1");
    assert_eq!(rows[2]["_rowId"], "document-3");
    assert_eq!(rows[0]["rfiNo"], "");
    assert_eq!(rows[0]["date"], "");
    assert_eq!(
        rows[0]["issuer"], "Mahammad Osmanov",
        "fixed starting text stays"
    );
    assert!(free_rows(&map, 0).is_empty());
}

#[test]
fn a_free_document_without_a_name_part_is_named_by_its_row() {
    let template = docx_with("{{rfiNo}}");
    let rows = free_rows(&map(), 2);
    let (first, _) = render(FileKind::Docx, &template, &map(), &rows[0]).expect("render");
    let (second, _) = render(FileKind::Docx, &template, &map(), &rows[1]).expect("render");
    assert_eq!(first.filename, "document-1.docx");
    assert_eq!(second.filename, "document-2.docx");
}

fn computed_doc_map() -> FieldMap {
    FieldMap::parse(&json!({
        "schema": 2,
        "fileName": "{{label}}",
        "slots": [
            {"key": "plant", "type": "number", "source": {"kind": "fixed", "value": "24.0"}},
            {"key": "ambient", "type": "computed",
             "compute": {"op": "offset", "inputs": ["plant"], "by": "-0.6"}},
            {"key": "label", "type": "computed",
             "compute": {"op": "concat", "inputs": ["plant"], "separator": ""}}
        ]
    }))
    .expect("map")
}

#[test]
fn computed_values_show_in_preview_as_read_only_and_follow_edits() {
    let map = computed_doc_map();
    let rows = free_rows(&map, 1);
    assert_eq!(rows[0]["ambient"], "23.4");
    let editor = editor(&map, rows.clone());
    let kinds = editor["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|column| column["kind"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["text", "readonly", "readonly"]);

    let mut edited = rows.clone();
    edited[0]["plant"] = json!("30");
    let updated = with_computed(&map, edited);
    assert_eq!(updated[0]["ambient"], "29.4");
    let mut broken = rows;
    broken[0]["plant"] = json!("abc");
    assert_eq!(
        with_computed(&map, broken)[0]["ambient"],
        "",
        "errors surface at render"
    );
}

#[test]
fn rendering_recomputes_and_ignores_a_submitted_computed_value() {
    let template = docx_with("{{plant}} {{ambient}}");
    let row = json!({"_rowId": "document-1", "plant": "30", "ambient": "999", "label": "x"});
    let (file, _) = render(FileKind::Docx, &template, &computed_doc_map(), &row).expect("render");
    let mut archive = zip::ZipArchive::new(Cursor::new(file.bytes.clone())).unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("word/document.xml").unwrap(),
        &mut text,
    )
    .unwrap();
    assert!(text.contains("30 29.4"), "{text}");
    assert_eq!(
        file.filename, "30.docx",
        "a computed value can be in the file name"
    );
    let bad = json!({"_rowId": "document-1", "plant": "abc", "ambient": "", "label": ""});
    assert!(render(FileKind::Docx, &template, &computed_doc_map(), &bad).is_err());
}

#[test]
fn a_time_field_is_text_in_preview_and_checked_when_rendered() {
    let map =
        FieldMap::parse(&json!({"schema": 2, "slots": [{"key": "at", "type": "time"}]})).unwrap();
    let rows = free_rows(&map, 1);
    let bad = json!({"_rowId": "document-1", "at": "25:00"});
    assert!(render(FileKind::Docx, &docx_with("{{at}}"), &map, &bad).is_err());
    let good = json!({"_rowId": "document-1", "at": "9:05"});
    assert!(render(FileKind::Docx, &docx_with("{{at}}"), &map, &good).is_ok());
    assert_eq!(rows[0]["at"], "");
}

const CIV_RFI: &str = "AZE-SUN-INT-QMS-RFI-CIV-0123";

fn follow_up_book() -> Vec<u8> {
    book_named(
        "Follow Up",
        &[
            &[
                "RFI No",
                "Concrete Class",
                "Batch Ticket No",
                "Truck No",
                "Time of Loading",
                "Pouring Date",
                "Temperature in plant (C)",
            ],
            // 46333 = 07.11.2026, 0.3958333333 = 09:30
            &[
                "CIV-0123",
                "C40",
                "A-1",
                "T1",
                "0.3958333333",
                "46333",
                "24.5",
            ],
            &["", "C40", "A-2", "T2", "0.4166666667", "46333", "24,0"],
            &["CIV-0999", "C25", "Z-1", "T9", "0.5", "46334", "20"],
        ],
    )
}

fn follow_up() -> BTreeMap<String, Vec<LogRow>> {
    read_follow_up_batches(
        &mut Cursor::new(follow_up_book()),
        &[
            CIV_RFI.to_string(),
            "AZE-SUN-INT-QMS-RFI-CIV-0200".to_string(),
        ],
    )
    .expect("follow up")
}

#[test]
fn follow_up_rows_become_batches_with_log_style_dates_and_times() {
    let batches = follow_up();
    let rows = &batches[CIV_RFI];
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["fuBatchTicket"], "A-1");
    assert_eq!(rows[1]["fuBatchTicket"], "A-2");
    assert_eq!(rows[0]["fuBatchTime"], "09:30");
    assert_eq!(rows[0]["fuPourDate"], "07.11.2026");
    assert_eq!(rows[1]["fuPlantTemp"], "24,0");
    assert_eq!(
        (
            rows[0]["fuBatchNo"].as_str(),
            rows[0]["fuBatchCount"].as_str()
        ),
        ("1", "2")
    );
    assert_eq!(rows[1]["fuBatchNo"], "2");
    assert_eq!(
        rows[0]["fuStructure"], "",
        "a column the sheet lacks is empty"
    );
    assert!(
        !batches.contains_key("AZE-SUN-INT-QMS-RFI-CIV-0200"),
        "no rows, no entry"
    );
    assert!(
        !batches.contains_key("AZE-SUN-INT-QMS-RFI-CIV-0999"),
        "not requested"
    );
}

fn follow_up_map() -> FieldMap {
    FieldMap::parse(&json!({"schema": 2, "slots": [
        {"key": "ticket", "source": {"kind": "fixed", "value": "{fuBatchTicket}"}},
        {"key": "count", "source": {"kind": "fixed", "value": "{fuBatchCount}"}},
        {"key": "batches",
         "anchor": {"kind": "rows", "sheet": "Form", "address": "A19", "repeatColumns": 3},
         "source": {"kind": "fixed", "value": "{fuBatchNo}\t{fuBatchTicket}\t{fuTruck}"}}
    ]}))
    .expect("map")
}

fn civ_log() -> BTreeMap<String, LogRow> {
    BTreeMap::from([(CIV_RFI.to_string(), LogRow::new())])
}

#[test]
fn a_field_starts_from_the_first_batch_and_a_repeating_row_gets_one_line_per_batch() {
    let rows = initial_rows_with(
        &follow_up_map(),
        &civ_log(),
        &follow_up(),
        &[CIV_RFI.to_string()],
    );
    assert_eq!(rows[0]["ticket"], "A-1");
    assert_eq!(rows[0]["count"], "2");
    assert_eq!(rows[0]["batches"], "1\tA-1\tT1\n2\tA-2\tT2");
}

#[test]
fn without_follow_up_rows_the_batch_tokens_are_empty_and_nothing_stays_literal() {
    let rows = initial_rows_with(
        &follow_up_map(),
        &civ_log(),
        &BTreeMap::new(),
        &[CIV_RFI.to_string()],
    );
    assert_eq!(rows[0]["ticket"], "");
    assert_eq!(rows[0]["count"], "0");
    assert_eq!(rows[0]["batches"], "\t\t");
    // The plain entry point (no Follow-Up workbook) behaves the same.
    let plain = initial_rows(&follow_up_map(), &civ_log(), &[CIV_RFI.to_string()]);
    assert_eq!(plain[0]["ticket"], "");
    let free = free_rows(&follow_up_map(), 1);
    assert_eq!(free[0]["ticket"], "");
    assert_eq!(free[0]["batches"], "\t\t");
}
