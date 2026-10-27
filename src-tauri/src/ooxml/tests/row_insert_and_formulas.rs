use super::*;

#[test]
fn insert_row_clones_the_template_and_shifts_everything_it_owns() {
    let mut source = rows_fixture("SUM(Road!D3:D6)");
    let output = apply_row_ops(&mut source, &[insert_after_row_three()]).unwrap();
    let road = member(&output, "xl/worksheets/sheet1.xml");

    // Rows below move down, the new row sits at 4.
    assert!(road.contains(r#"<row r="4" ht="15">"#), "{road}");
    assert!(road.contains(r#"<row r="5"><c r="A5""#), "{road}");
    assert!(road.contains(r#"<row r="7"><c r="D7">"#), "{road}");
    // Template styles and the zone are kept, the label is set, the RFI is cleared.
    assert!(
        road.contains(r#"<c r="A4" s="2" t="inlineStr"><is><t>North</t></is></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="B4" s="2" t="inlineStr"><is><t>Road 01 (Part 2)</t></is></c>"#),
        "{road}"
    );
    assert!(road.contains(r#"<c r="C4" s="3"/>"#), "{road}");
    // Shared formulas are expanded; the clone gets its own translated formula.
    assert!(!road.contains("shared"), "{road}");
    assert!(
        road.contains(r#"<c r="D3" s="4"><f>IF(C3=&quot;&quot;,&quot;empty&quot;,C3)</f>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D4" s="4"><f>IF(C4=&quot;&quot;,&quot;empty&quot;,C4)</f></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D5"><f>IF(C5=&quot;&quot;,&quot;empty&quot;,C5)</f>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D7"><f>IF(C7=&quot;&quot;,&quot;empty&quot;,C7)</f>"#),
        "{road}"
    );
    // Ranges, merges, conditional formatting and the dimension follow Excel's rules.
    assert!(road.contains("AVERAGE(D3:D7)"), "{road}");
    assert!(road.contains(r#"<dimension ref="A1:D7"/>"#), "{road}");
    assert!(road.contains(r#"<mergeCell ref="A3:A5"/>"#), "{road}");
    assert!(
        road.contains(r#"sqref="C3:C7""#) && road.contains(r#"<formula>C3="x"</formula>"#),
        "{road}"
    );
}

#[test]
fn insert_row_updates_other_sheets_defined_names_and_drops_the_calc_chain() {
    let mut source = rows_fixture("SUM(Road!D3:D6)+Road!C9");
    let output = apply_row_ops(&mut source, &[insert_after_row_three()]).unwrap();

    assert!(member(&output, "xl/worksheets/sheet2.xml").contains("SUM(Road!D3:D7)+Road!C10"));
    assert!(member(&output, "xl/workbook.xml").contains("Road!$A$1:$D$7"));
    let mut archive = zip::ZipArchive::new(Cursor::new(output.clone())).unwrap();
    assert!(archive.by_name("xl/calcChain.xml").is_err());
    assert!(!member(&output, "xl/_rels/workbook.xml.rels").contains("calcChain"));
    assert!(!member(&output, "[Content_Types].xml").contains("calcChain"));
    let mut reread = Cursor::new(output);
    let workbook = read_workbook_from_reader(&mut reread).unwrap();
    assert_eq!(
        workbook.sheet("Road").unwrap().value(4, 2),
        "Road 01 (Part 2)"
    );
    assert_eq!(workbook.sheet("Road").unwrap().value(5, 2), "Road 02");
}

#[test]
fn insert_row_refuses_a_shared_formula_on_another_sheet_that_points_at_the_sheet() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Road" sheetId="1" r:id="rId1"/><sheet name="STAT" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let road = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="3"><c r="A3"/></row></sheetData></worksheet>"#
    );
    let stat = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><f t="shared" ref="A1:A2" si="0">Road!A5</f></c></row><row r="2"><c r="A2"><f t="shared" si="0"/></c></row></sheetData></worksheet>"#
    );
    let mut source = fixture_from_members(
        &workbook,
        &relationships,
        [
            ("xl/worksheets/sheet1.xml", road.as_bytes()),
            ("xl/worksheets/sheet2.xml", stat.as_bytes()),
        ],
    );

    let error = apply_row_ops(&mut source, &[insert_after_row_three()]).unwrap_err();

    assert!(error.to_string().contains("shared formula"), "{error}");
}

#[test]
fn set_text_relabels_a_value_cell_but_refuses_a_formula_cell() {
    let mut source = rows_fixture("1");
    let output = apply_row_ops(
        &mut source,
        &[RowOp::SetText {
            sheet: "Road".to_string(),
            row: 3,
            column: 2,
            text: "Road 01 (Part 1)".to_string(),
        }],
    )
    .unwrap();
    assert!(
        member(&output, "xl/worksheets/sheet1.xml")
            .contains(r#"<c r="B3" s="2" t="inlineStr"><is><t>Road 01 (Part 1)</t></is></c>"#)
    );

    let mut source = rows_fixture("1");
    let error = apply_row_ops(
        &mut source,
        &[RowOp::SetText {
            sheet: "Road".to_string(),
            row: 3,
            column: 4,
            text: "x".to_string(),
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("formula"), "{error}");
}

#[test]
fn insert_row_can_precede_the_first_data_row_using_the_row_below_as_template() {
    let mut source = rows_fixture("1");
    let output = apply_row_ops(
        &mut source,
        &[RowOp::InsertRow {
            sheet: "Road".to_string(),
            after_row: 2,
            template_row: 3,
            copy_columns: vec![1],
            texts: vec![CellText {
                column: 2,
                text: "Road 00".to_string(),
            }],
        }],
    )
    .unwrap();
    let road = member(&output, "xl/worksheets/sheet1.xml");

    assert!(road.contains(r#"<row r="3" ht="15">"#), "{road}");
    assert!(
        road.contains(r#"<c r="B3" s="2" t="inlineStr"><is><t>Road 00</t></is></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D3" s="4"><f>IF(C3=&quot;&quot;,&quot;empty&quot;,C3)</f></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="B4" s="2" t="inlineStr"><is><t>Road 01</t></is></c>"#),
        "{road}"
    );
    assert!(
        road.contains(r#"<c r="D4" s="4"><f>IF(C4=&quot;&quot;,&quot;empty&quot;,C4)</f>"#),
        "{road}"
    );
}

#[test]
fn guarded_formula_fills_an_empty_styled_cell_and_verifies() {
    let (mut source, updates) = formula_fixture(r#"<c r="Y5" s="34"/>"#);
    let mut destination = Cursor::new(Vec::new());

    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    source.set_position(0);
    destination.set_position(0);
    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).unwrap();

    let xml = sheet_xml(&mut destination);
    assert!(
        xml.contains(r#"<c r="Y5" s="34"><f>COUNTIF(C5:P5,&quot;Completed&quot;)/6</f></c>"#),
        "{xml}"
    );
    destination.set_position(0);
    let workbook = read_workbook_from_reader(&mut destination).unwrap();
    assert_eq!(
        workbook
            .sheet("Road")
            .unwrap()
            .cell(5, 25)
            .unwrap()
            .formula
            .as_deref(),
        Some("COUNTIF(C5:P5,\"Completed\")/6")
    );
}

#[test]
fn guarded_formula_never_overwrites_a_value_or_an_existing_formula() {
    for cell in [
        r#"<c r="Y5" s="34" t="inlineStr"><is><t>READY</t></is></c>"#,
        r#"<c r="Y5" s="34"><f>1+1</f><v>2</v></c>"#,
    ] {
        let (mut source, mut updates) = formula_fixture(cell);
        // The preview would have recorded the real content; the engine must still refuse.
        updates
            .get_mut("Road")
            .unwrap()
            .get_mut(&(5, 25))
            .unwrap()
            .expected = CellData {
            value: String::new(),
            formula: None,
            style: Some("34".to_string()),
        };
        let mut destination = Cursor::new(Vec::new());
        assert!(
            patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).is_err(),
            "{cell}"
        );
    }
}
