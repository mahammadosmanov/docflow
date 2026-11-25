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
    r#"<w:p><w:r><w:t>{{remarks}}</w:t></w:r><w:r><w:tab/><w:t>{{unknown|Mr. A}}{{area|({area}) {tagNo}}}</w:t></w:r></w:p>"#,
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
    assert_eq!(keys, ["dateOfIssue", "rfiNo", "remarks", "unknown", "area"]);
    let (_, defaults) = scan(FileKind::Docx, &docx(SPLIT)).expect("scan");
    assert_eq!(
        defaults,
        BTreeMap::from([
            ("area".to_string(), "({area}) {tagNo}".to_string()),
            ("unknown".to_string(), "Mr. A".to_string()),
        ])
    );
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
    assert_eq!(
        missing,
        BTreeSet::from(["area".to_string(), "unknown".to_string()])
    );
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
    map.merge_placeholders(
        &[
            "remarks".to_string(),
            "rfiNo".to_string(),
            "stage".to_string(),
            "item".to_string(),
        ],
        &BTreeMap::from([
            ("item".to_string(), "Underground LV cable".to_string()),
            (
                "remarks".to_string(),
                "ignored for existing fields".to_string(),
            ),
        ]),
    );
    assert_eq!(map.fields.len(), 4);
    assert_eq!(map.fields[0].value, "keep");
    assert_eq!(map.fields[1].value, "{rfiNo}");
    assert_eq!(map.fields[2].value, "");
    assert_eq!(map.fields[3].value, "Underground LV cable");
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

fn slot_map() -> serde_json::Value {
    json!({
        "schema": 2,
        "fileName": "{{rfiNo}} Pour",
        "slots": [
            {"key": "rfiNo", "label": "RFI No.", "type": "text",
             "anchor": {"kind": "placeholder"},
             "source": {"kind": "log", "token": "rfiNo"}},
            {"key": "note", "label": "Note", "type": "multiline",
             "source": {"kind": "fixed", "value": "({area}) {tagNo}"}},
            {"key": "issuer", "type": "choice", "options": ["A", "B"],
             "source": {"kind": "manual"}}
        ],
        "presets": [{"name": "ITP 5", "values": {"issuer": "A"}}]
    })
}

#[test]
fn schema_two_slots_load_as_the_same_fields_schema_one_has() {
    let map = FieldMap::parse(&slot_map()).expect("schema 2 loads");
    assert_eq!(map.file_name, "{{rfiNo}} Pour");
    let by_key = |key: &str| map.fields.iter().find(|field| field.key == key).expect(key);
    assert_eq!(by_key("rfiNo").value, "{rfiNo}");
    assert_eq!(by_key("rfiNo").label, "RFI No.");
    assert!(by_key("note").multiline);
    assert_eq!(by_key("note").value, "({area}) {tagNo}");
    assert_eq!(by_key("issuer").options, ["A", "B"]);
    assert_eq!(by_key("issuer").value, "");
    assert_eq!(map.presets[0].values["issuer"], "A");
}

#[test]
fn schema_two_and_the_equal_schema_one_map_agree() {
    let two = FieldMap::parse(&slot_map()).expect("schema 2 loads");
    let one = FieldMap::parse(&serde_json::to_value(&two).expect("serialize")).expect("schema 1");
    assert_eq!(one, two);
    assert_eq!(one.schema, 1, "this release still writes schema 1");
}

#[test]
fn schema_two_features_this_release_cannot_apply_are_refused() {
    for slot in [
        json!({"key": "a", "anchor": {"kind": "range", "sheet": "S", "address": "A1:B2"}}),
        json!({"key": "a", "type": "computed"}),
        json!({"key": "a", "type": "checkbox"}),
        json!({"key": "a", "source": {"kind": "other"}}),
    ] {
        let value = json!({"schema": 2, "slots": [slot]});
        let error = FieldMap::parse(&value).expect_err("refused");
        assert!(error.contains("newer DocFlow"), "{error}");
    }
    assert!(FieldMap::parse(&json!({"schema": 3, "slots": []})).is_err());
    let repeated = json!({"schema": 2, "slots": [{"key": "a"}, {"key": "a"}]});
    assert!(FieldMap::parse(&repeated).is_err());
}

#[test]
fn schema_two_cell_anchors_load_and_bad_ones_are_invalid() {
    let word = json!({"key": "a", "anchor": {"kind": "cell", "table": 0, "row": 1, "cell": 2}});
    let excel = json!({"key": "b", "anchor": {"kind": "cell", "sheet": "Form", "address": "C3"}});
    let map = FieldMap::parse(&json!({"schema": 2, "slots": [word, excel]})).expect("loads");
    let anchor = |key: &str| {
        map.fields
            .iter()
            .find(|field| field.key == key)
            .and_then(|field| field.anchor.clone())
            .expect(key)
    };
    assert_eq!(
        (anchor("a").table, anchor("a").row, anchor("a").cell),
        (Some(0), Some(1), Some(2))
    );
    assert_eq!(anchor("b").address.as_deref(), Some("C3"));
    for bad in [
        json!({"key": "a", "anchor": {"kind": "cell", "sheet": "S"}}),
        json!({"key": "a", "anchor": {"kind": "cell", "sheet": "S", "address": "A0"}}),
        json!({"key": "a", "anchor": {"kind": "cell", "table": 0, "row": 0}}),
    ] {
        let error = FieldMap::parse(&json!({"schema": 2, "slots": [bad]})).expect_err("invalid");
        assert!(!error.contains("newer DocFlow"), "{error}");
    }
    let shared = json!({"schema": 2, "slots": [
        {"key": "a", "anchor": {"kind": "cell", "sheet": "S", "address": "A1"}},
        {"key": "b", "anchor": {"kind": "cell", "sheet": "S", "address": "A1"}}
    ]});
    assert!(
        FieldMap::parse(&shared)
            .expect_err("shared cell")
            .contains("shares its cell")
    );
}

#[test]
fn render_fills_placeholders_and_anchored_cells_together() {
    let document = concat!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
        r#"<w:p><w:r><w:t>RFI {{rfiNo}}</w:t></w:r></w:p>"#,
        r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>old</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        r#"</w:body></w:document>"#
    );
    let map = FieldMap::parse(&json!({"schema": 2, "slots": [
        {"key": "rfiNo"},
        {"key": "cellText", "anchor": {"kind": "cell", "table": 0, "row": 0, "cell": 0}}
    ]}))
    .expect("map");
    let values = BTreeMap::from([
        ("rfiNo".to_string(), "ELE-0049".to_string()),
        ("cellText".to_string(), "Pour 12".to_string()),
    ]);
    let (bytes, missing) =
        render_package(FileKind::Docx, &docx(document), &map, &values).expect("render");
    let xml = part(&bytes, "word/document.xml");
    assert!(
        xml.contains("RFI ELE-0049") && xml.contains("Pour 12"),
        "{xml}"
    );
    assert!(!xml.contains("old") && missing.is_empty(), "{xml}");
}

const PLAIN: &str = concat!(
    r#"<w:document><w:body>"#,
    r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Date: </w:t></w:r>"#,
    r#"<w:r><w:t>0</w:t></w:r><w:r><w:t>7.11.2026</w:t></w:r></w:p>"#,
    r#"<w:p></w:p><w:p><w:r><w:t>Issuer: Ağa Ömer</w:t></w:r></w:p>"#,
    r#"</w:body></w:document>"#
);

#[test]
fn blocks_list_paragraph_text_with_stable_indexes() {
    let blocks = text_blocks(FileKind::Docx, &docx(PLAIN)).expect("blocks");
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text, "Date: 07.11.2026");
    assert_eq!(blocks[0].part, "word/document.xml");
    assert_eq!(blocks[1].text, "Issuer: Ağa Ömer");
}

#[test]
fn marking_a_selection_turns_split_runs_into_one_placeholder() {
    let bytes = docx(PLAIN);
    let block = &text_blocks(FileKind::Docx, &bytes).unwrap()[0];
    let marked = replace_in_block(
        FileKind::Docx,
        &bytes,
        &block.part,
        block.index,
        6..16,
        "07.11.2026",
        "{{dateOfIssue}}",
    )
    .expect("marked");
    let xml = part(&marked, "word/document.xml");
    assert!(xml.contains(r#"<w:b/></w:rPr><w:t xml:space="preserve">Date: </w:t>"#));
    assert_eq!(
        placeholders(FileKind::Docx, &marked).unwrap(),
        ["dateOfIssue"]
    );
    assert_eq!(part(&marked, "word/media/image1.png"), "PNG");

    let other = &text_blocks(FileKind::Docx, &marked).unwrap()[1];
    let renamed = replace_in_block(
        FileKind::Docx,
        &marked,
        &other.part,
        other.index,
        8..16,
        "Ağa Ömer",
        "{{issuer}}",
    )
    .expect("utf-16 offsets map to bytes");
    assert_eq!(
        placeholders(FileKind::Docx, &renamed).unwrap(),
        ["dateOfIssue", "issuer"]
    );
}

#[test]
fn stale_or_invalid_selections_are_refused() {
    let bytes = docx(PLAIN);
    let block = &text_blocks(FileKind::Docx, &bytes).unwrap()[0];
    let edit = |span: std::ops::Range<usize>, expected: &str| {
        replace_in_block(
            FileKind::Docx,
            &bytes,
            &block.part,
            block.index,
            span,
            expected,
            "x",
        )
    };
    assert!(edit(6..16, "08.11.2026").is_err(), "text changed");
    assert!(edit(6..99, "07.11.2026").is_err(), "outside the block");
    assert!(edit(6..6, "").is_err(), "empty selection");
    assert!(
        replace_in_block(FileKind::Docx, &bytes, "word/styles.xml", 0, 0..1, "D", "x").is_err()
    );
    assert!(replace_in_block(FileKind::Docx, &bytes, &block.part, 99, 0..1, "D", "x").is_err());
}
