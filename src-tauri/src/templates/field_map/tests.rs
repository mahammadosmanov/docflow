use serde_json::json;

use super::*;

fn docx(document: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in [
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", document),
        ("word/media/image1.png", "PNG"),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
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

const SPLIT: &str = concat!(
    r#"<w:document><w:body>"#,
    r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Date: </w:t></w:r>"#,
    r#"<w:r><w:t>{{date</w:t></w:r><w:r><w:t>OfIssue}}</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:t>RFI No: {{rfiNo}} / {{ rfiNo }}</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:t>{{remarks}}</w:t></w:r><w:r><w:tab/><w:t>{{unknown}}</w:t></w:r></w:p>"#,
    r#"</w:body></w:document>"#
);

fn values() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("dateOfIssue".to_string(), "07.11.2026".to_string()),
        (
            "rfiNo".to_string(),
            "AZE-SUN-INT-QMS-RFI-ELE-0059".to_string(),
        ),
        ("remarks".to_string(), "A & B\nline 2".to_string()),
    ])
}

#[test]
fn placeholders_are_found_across_runs_once_each() {
    let keys = placeholders(FileKind::Docx, &docx(SPLIT)).expect("placeholders");
    assert_eq!(keys, ["dateOfIssue", "rfiNo", "remarks", "unknown"]);
}

#[test]
fn filling_keeps_run_formatting_and_other_parts() {
    let (bytes, missing) = fill_package(FileKind::Docx, &docx(SPLIT), &values()).expect("fill");
    let xml = part(&bytes, "word/document.xml");
    assert!(xml.contains(r#"<w:b/></w:rPr><w:t xml:space="preserve">Date: </w:t>"#));
    assert!(xml.contains(r#"<w:t xml:space="preserve">07.11.2026</w:t>"#));
    assert!(xml.contains("RFI No: AZE-SUN-INT-QMS-RFI-ELE-0059 / AZE-SUN-INT-QMS-RFI-ELE-0059"));
    assert!(xml.contains("A &amp; B</w:t><w:br/><w:t xml:space=\"preserve\">line 2"));
    assert!(!xml.contains("{{"));
    assert_eq!(missing, BTreeSet::from(["unknown".to_string()]));
    assert_eq!(part(&bytes, "word/media/image1.png"), "PNG");
    assert!(
        placeholders(FileKind::Docx, &bytes)
            .expect("refill")
            .is_empty()
    );
}

#[test]
fn excel_shared_strings_are_filled_without_line_markup() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in [
        ("[Content_Types].xml", "<Types/>"),
        ("xl/workbook.xml", "<workbook/>"),
        (
            "xl/sharedStrings.xml",
            "<sst><si><t>{{rfiNo}}</t></si><si><r><t>{{rem</t></r><r><t>arks}}</t></r></si></sst>",
        ),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start");
        writer.write_all(body.as_bytes()).expect("write");
    }
    let bytes = writer.finish().expect("finish").into_inner();
    assert_eq!(
        placeholders(FileKind::Xlsx, &bytes).expect("keys"),
        ["rfiNo", "remarks"]
    );
    let (filled, _) = fill_package(FileKind::Xlsx, &bytes, &values()).expect("fill");
    let xml = part(&filled, "xl/sharedStrings.xml");
    assert!(xml.contains("AZE-SUN-INT-QMS-RFI-ELE-0059"));
    assert!(xml.contains("A &amp; B\nline 2"));
    assert!(!xml.contains("w:br"));
}

#[test]
fn tokens_resolve_from_the_log_row_and_unknown_ones_stay() {
    let row = BTreeMap::from([
        ("area".to_string(), "North Site".to_string()),
        ("tagNo".to_string(), "PCS-16".to_string()),
    ]);
    assert_eq!(
        resolve_tokens("({area}) {tagNo}", &row),
        "(North Site) PCS-16"
    );
    assert_eq!(resolve_tokens("{nope} x", &row), "{nope} x");
    assert_eq!(
        fill_text("{{rfiNo}}", &values()),
        "AZE-SUN-INT-QMS-RFI-ELE-0059"
    );
}

#[test]
fn new_placeholders_become_fields_and_known_tokens_prefill() {
    let mut map = FieldMap::default();
    map.fields.push(FieldDef {
        key: "remarks".to_string(),
        label: "Remarks".to_string(),
        value: "keep".to_string(),
        ..FieldDef::default()
    });
    map.merge_placeholders(&[
        "remarks".to_string(),
        "rfiNo".to_string(),
        "stage".to_string(),
    ]);
    assert_eq!(map.fields.len(), 3);
    assert_eq!(map.fields[0].value, "keep");
    assert_eq!(map.fields[1].value, "{rfiNo}");
    assert_eq!(map.fields[2].value, "");
    map.validate().expect("valid");
}

#[test]
fn invalid_maps_are_rejected() {
    let base = json!({"schema": 1, "fields": [{"key": "stage"}]});
    assert!(FieldMap::parse(&base).is_ok());
    let mut repeated = base.clone();
    repeated["fields"] = json!([{"key": "a"}, {"key": "a"}]);
    assert!(FieldMap::parse(&repeated).is_err());
    let mut bad_key = base.clone();
    bad_key["fields"] = json!([{"key": "1x"}]);
    assert!(FieldMap::parse(&bad_key).is_err());
    let mut unknown_preset_field = base.clone();
    unknown_preset_field["presets"] = json!([{"name": "ITP 5", "values": {"nope": "x"}}]);
    assert!(FieldMap::parse(&unknown_preset_field).is_err());
    let mut duplicate_preset = base;
    duplicate_preset["presets"] = json!([{"name": "A"}, {"name": "a"}]);
    assert!(FieldMap::parse(&duplicate_preset).is_err());
}
