use super::*;

#[test]
fn ordered_worksheet_discovery_preserves_order_identity_and_visibility() {
    let discovery = ordered_workbook_discovery(&mut ordered_fixture()).unwrap();

    assert_eq!(
        discovery.worksheets,
        vec![
            WorksheetDescriptor {
                workbook_index: 1,
                sheet_id: 17,
                relationship_id: "rId7".to_string(),
                relationship_type: REL.to_string(),
                part_path: "xl/worksheets/sheet7.xml".to_string(),
                name: "MEC".to_string(),
                visibility: WorksheetVisibility::Visible,
            },
            WorksheetDescriptor {
                workbook_index: 2,
                sheet_id: 3,
                relationship_id: "rId2".to_string(),
                relationship_type: REL.to_string(),
                part_path: "xl/worksheets/sheet2.xml".to_string(),
                name: "CIV".to_string(),
                visibility: WorksheetVisibility::Visible,
            },
            WorksheetDescriptor {
                workbook_index: 3,
                sheet_id: 44,
                relationship_id: "rId9".to_string(),
                relationship_type: REL.to_string(),
                part_path: "xl/worksheets/sheet9.xml".to_string(),
                name: "Hidden".to_string(),
                visibility: WorksheetVisibility::Hidden,
            },
            WorksheetDescriptor {
                workbook_index: 4,
                sheet_id: 45,
                relationship_id: "rId10".to_string(),
                relationship_type: REL.to_string(),
                part_path: "xl/worksheets/sheet10.xml".to_string(),
                name: "VeryHidden".to_string(),
                visibility: WorksheetVisibility::VeryHidden,
            },
        ]
    );
    assert_eq!(discovery.safety, Default::default());
}

#[test]
fn ordered_worksheet_discovery_ignores_sheet_elements_outside_sheets_container() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><ext:data><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/></ext:data><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", b"worksheet"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "MEC");
    assert_eq!(discovery.worksheets[0].workbook_index, 1);
}

#[test]
fn ordered_worksheet_discovery_ignores_foreign_sheet_elements_in_sheets_container() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", b"worksheet"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "MEC");
}

#[test]
fn legacy_sheet_info_projection_ignores_foreign_sheet_elements() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let bytes = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", b"worksheet"),
        ],
    )
    .into_inner();
    let directory = tempdir().unwrap();
    let path = directory.path().join("workbook.xlsx");
    fs::write(&path, bytes).unwrap();

    let infos = sheet_infos_from_path(&path).unwrap();

    assert_eq!(infos.len(), 1);
    assert_eq!(infos["MEC"].sheet_id, 1);
    assert_eq!(infos["MEC"].path, "xl/worksheets/sheet1.xml");
}

#[test]
fn read_workbook_uses_canonical_sheet_projection() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(r#"<worksheet xmlns="{MAIN}"><sheetData/></worksheet>"#);
    let bytes = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
        ],
    )
    .into_inner();
    let directory = tempdir().unwrap();
    let path = directory.path().join("workbook.xlsx");
    fs::write(&path, bytes).unwrap();

    let data = read_workbook(&path).unwrap();

    assert!(data.sheet("MEC").is_some());
    assert!(data.sheet("Decoy").is_none());
}

#[test]
fn read_workbook_uses_authenticated_shared_strings_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStrings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="/custom/strings.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#
    );
    let trusted_strings =
        format!(r#"<sst xmlns="{MAIN}" count="1" uniqueCount="1"><si><t>Trusted</t></si></sst>"#);
    let decoy_strings =
        format!(r#"<sst xmlns="{MAIN}" count="1" uniqueCount="1"><si><t>Decoy</t></si></sst>"#);
    let bytes = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
            ("custom/strings.xml", trusted_strings.as_bytes()),
            ("xl/sharedStrings.xml", decoy_strings.as_bytes()),
        ],
    )
    .into_inner();
    let directory = tempdir().unwrap();
    let path = directory.path().join("workbook.xlsx");
    fs::write(&path, bytes).unwrap();

    let data = read_workbook(&path).unwrap();

    assert_eq!(data.sheet("MEC").unwrap().value(1, 1), "Trusted");
}

#[test]
fn patch_workbook_sheets_uses_canonical_sheet_projection() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="MEC" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><v>old</v></c></row></sheetData></worksheet>"#
    );
    let bytes = fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/decoy.xml", b"decoy"),
            ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
        ],
    )
    .into_inner();
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.xlsx");
    let destination = directory.path().join("destination.xlsx");
    fs::write(&source, bytes).unwrap();
    let updates = BTreeMap::from([(
        "MEC".to_string(),
        BTreeMap::from([((1, 1), CellUpdate::Text("new".to_string()))]),
    )]);

    patch_workbook_sheets(&source, &destination, &updates, None).unwrap();

    let data = read_workbook(&destination).unwrap();
    assert_eq!(data.sheet("MEC").unwrap().value(1, 1), "new");
}

#[test]
fn guarded_text_patch_can_clear_a_literal_cell() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Pending</t></is></c></row></sheetData></worksheet>"#
    );
    let mut source = fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", worksheet.as_bytes())],
    );
    let updates = BTreeMap::from([(
        "Follow Up".to_string(),
        BTreeMap::from([(
            (1, 1),
            GuardedCellUpdate {
                expected: CellData {
                    value: "Pending".to_string(),
                    formula: None,
                    style: None,
                },
                replacement: CellUpdate::Text(String::new()),
                create_if_absent: false,
            },
        )]),
    )]);
    let mut destination = Cursor::new(Vec::new());

    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    source.set_position(0);
    destination.set_position(0);

    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).unwrap();
}
