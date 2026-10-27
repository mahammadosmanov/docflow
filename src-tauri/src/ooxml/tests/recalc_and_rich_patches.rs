use super::*;

#[test]
fn recalc_patch_sets_full_calc_on_load_and_nothing_else_in_the_workbook_part() {
    let (mut source, updates) = recalc_fixture(r#"<calcPr calcId="191029"/>"#);
    let before = workbook_xml(&mut source);
    let mut destination = Cursor::new(Vec::new());

    patch_workbook_sheets_guarded_text_recalc(&mut source, &mut destination, &updates).unwrap();
    source.set_position(0);
    destination.set_position(0);
    verify_guarded_workbook_delta_text_recalc(&mut source, &mut destination, &updates).unwrap();

    let after = workbook_xml(&mut destination);
    assert!(after.contains(r#"fullCalcOnLoad="1""#), "{after}");
    assert_eq!(
        after.replace(r#" fullCalcOnLoad="1""#, ""),
        before,
        "only the calcPr attribute may differ"
    );
}

#[test]
fn recalc_verification_rejects_any_other_workbook_part_change() {
    let (mut source, updates) = recalc_fixture(r#"<calcPr calcId="191029"/>"#);
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded_text_recalc(&mut source, &mut destination, &updates).unwrap();
    let patched_sheet = {
        destination.set_position(0);
        let mut archive = zip::ZipArchive::new(&mut destination).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    };
    let tampered = workbook_xml(&mut destination)
        .replace(r#"calcId="191029""#, r#"calcId="191029" iterate="1""#);
    let mut forged = fixture_from_members(
        &tampered,
        &format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        ),
        [("xl/worksheets/sheet1.xml", patched_sheet.as_slice())],
    );
    source.set_position(0);
    let error =
        verify_guarded_workbook_delta_text_recalc(&mut source, &mut forged, &updates).unwrap_err();
    assert!(error.to_string().contains("recalculation flag"), "{error}");
}

#[test]
fn plain_guarded_patch_leaves_the_workbook_part_untouched() {
    let (mut source, updates) = recalc_fixture(r#"<calcPr calcId="191029"/>"#);
    let before = workbook_xml(&mut source);
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    assert_eq!(workbook_xml(&mut destination), before);
}

#[test]
fn recalc_without_a_calc_pr_element_leaves_the_workbook_part_untouched() {
    let (mut source, updates) = recalc_fixture("");
    let before = workbook_xml(&mut source);
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded_text_recalc(&mut source, &mut destination, &updates).unwrap();
    source.set_position(0);
    destination.set_position(0);
    verify_guarded_workbook_delta_text_recalc(&mut source, &mut destination, &updates).unwrap();
    assert_eq!(workbook_xml(&mut destination), before);
}

#[test]
fn rich_patch_writes_emphasized_and_plain_runs_in_the_cell_font() {
    let (mut source, updates) = rich_fixture();
    let mut destination = Cursor::new(Vec::new());

    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    let xml = sheet_xml(&mut destination);

    assert!(
        xml.contains(
            r#"<b/><u/><sz val="9"/><color theme="8"/><rFont val="Arial"/><family val="2"/>"#
        ),
        "{xml}"
    );
    assert!(xml.contains(r#"<sz val="9"/><color theme="1"/><rFont val="Arial"/><family val="2"/></rPr><t xml:space="preserve"> CIV-0043 In Progress</t>"#), "{xml}");
    source.set_position(0);
    destination.set_position(0);
    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).unwrap();
    destination.set_position(0);
    let workbook = read_workbook_from_reader(&mut destination).unwrap();
    assert_eq!(
        workbook.sheet("Follow Up").unwrap().value(1, 1),
        "CIV-0021,0022 Completed CIV-0043 In Progress"
    );
}

#[test]
fn rich_verification_rejects_changed_formatting() {
    let (mut source, updates) = rich_fixture();
    let mut destination = Cursor::new(Vec::new());
    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    let forged_sheet =
        sheet_xml(&mut destination).replace(r#"<color theme="8"/>"#, r#"<color theme="9"/>"#);
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let mut forged = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", forged_sheet.as_bytes()),
            ("xl/styles.xml", STYLES.as_bytes()),
        ],
    );
    source.set_position(0);

    assert!(verify_guarded_workbook_delta_text(&mut source, &mut forged, &updates).is_err());
}

#[test]
fn empty_rich_replacement_is_rejected() {
    let (mut source, mut updates) = rich_fixture();
    updates
        .get_mut("Follow Up")
        .unwrap()
        .get_mut(&(1, 1))
        .unwrap()
        .replacement = CellUpdate::Rich(Vec::new());
    let mut destination = Cursor::new(Vec::new());

    assert!(patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).is_err());
}
