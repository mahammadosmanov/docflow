use super::*;

#[test]
fn ordered_worksheet_discovery_rejects_sheet_id_from_foreign_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:evil="urn:evil"><sheets><sheet name="MEC" sheetId="1" evil:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook sheet relationship is missing");
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_sheet_attribute_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:spoof"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook relationship namespace is invalid"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_workbook_namespace() {
    let workbook = r#"<workbook xmlns="urn:spoof" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook namespace is invalid");
}

#[test]
fn ordered_worksheet_discovery_expanded_names_rejects_evil_sheets_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets xmlns="urn:evil"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook sheets container is invalid");
}

#[test]
fn ordered_worksheet_discovery_expanded_names_rejects_shadowed_relationship_child() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship xmlns="urn:evil" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook sheet target is missing");
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_raw_default_namespace_declarations() {
    let canonical_workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let canonical_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let cases = [
        (
            format!(
                r#"<workbook xmlns="urn:foreign" xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets xmlns="urn:foreign" xmlns="{MAIN}"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet xmlns="urn:foreign" xmlns="{MAIN}" name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            canonical_workbook.clone(),
            format!(
                r#"<Relationships xmlns="urn:foreign" xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
        (
            canonical_workbook,
            format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship xmlns="urn:foreign" xmlns="{PACKAGE_REL_NS}" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
    ];

    for (workbook, relationships) in cases {
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "XML attribute is invalid");
    }
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_raw_prefixed_namespace_declarations() {
    const OFFICE_REL_NS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let canonical_workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="{OFFICE_REL_NS}"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let canonical_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let cases = [
        (
            format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            format!(
                r#"<workbook xmlns="{MAIN}"><sheets xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            format!(
                r#"<workbook xmlns="{MAIN}"><sheets><sheet xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}" name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
            canonical_relationships.clone(),
        ),
        (
            canonical_workbook.clone(),
            format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}" xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
        (
            canonical_workbook,
            format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
    ];

    for (workbook, relationships) in cases {
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "XML attribute is invalid");
    }
}

#[test]
fn ordered_worksheet_discovery_expanded_names_require_unqualified_attributes() {
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    for (attribute, expected_error) in [
        (
            r#"evil:name="MEC" sheetId="1""#,
            "workbook sheet name is missing",
        ),
        (
            r#"name="MEC" evil:sheetId="1""#,
            "workbook sheet identity is invalid",
        ),
    ] {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:evil="urn:evil"><sheets><sheet {attribute} r:id="rId1"/></sheets></workbook>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), expected_error);
    }

    for (attribute, expected_error) in [
        (
            format!(r#"evil:Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml""#),
            "workbook relationship identity is missing",
        ),
        (
            r#"Id="rId1" evil:Type="urn:evil" Target="worksheets/sheet1.xml""#.to_string(),
            "workbook sheet relationship type is missing",
        ),
        (
            format!(r#"Id="rId1" Type="{REL}" evil:Target="worksheets/sheet1.xml""#),
            "workbook relationship target is missing",
        ),
    ] {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns:evil="urn:evil"><Relationship {attribute}/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), expected_error);
    }

    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:evil="urn:evil"><sheets><sheet name="MEC" sheetId="1" evil:state="hidden" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns:evil="urn:evil"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml" evil:TargetMode="External"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap();

    assert_eq!(
        discovery.worksheets[0].visibility,
        WorksheetVisibility::Visible
    );
}

#[test]
fn ordered_worksheet_discovery_expanded_names_accepts_alternate_valid_prefixes() {
    for (workbook_namespace, office_namespace, package_namespace, relationship_type) in [
        (
            MAIN,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
            "http://schemas.openxmlformats.org/package/2006/relationships",
            REL,
        ),
        (
            "http://purl.oclc.org/ooxml/spreadsheetml/main",
            "http://purl.oclc.org/ooxml/officeDocument/relationships",
            "http://purl.oclc.org/ooxml/package/relationships",
            "http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet",
        ),
    ] {
        let workbook = format!(
            r#"<ss:workbook xmlns:ss="{workbook_namespace}" xmlns:od="{office_namespace}"><ss:sheets><ss:sheet name="MEC" sheetId="1" od:id="rId1"/></ss:sheets></ss:workbook>"#
        );
        let relationships = format!(
            r#"<pkg:Relationships xmlns:pkg="{package_namespace}"><pkg:Relationship Id="rId1" Type="{relationship_type}" Target="worksheets/sheet1.xml"/></pkg:Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap();

        assert_eq!(discovery.worksheets[0].relationship_id, "rId1");
        assert_eq!(discovery.worksheets[0].relationship_type, relationship_type);
    }
}

#[test]
fn ordered_worksheet_discovery_expanded_names_reports_qualified_workbook_protection() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:ss="{MAIN}"><ss:workbookProtection lockStructure="1"/><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap();

    assert!(discovery.safety.has_workbook_protection);
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_name() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="MEC" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"one"),
            ("xl/worksheets/sheet2.xml", b"two"),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "duplicate workbook sheet name");
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_sheet_id() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="7" r:id="rId1"/><sheet name="CIV" sheetId="7" r:id="rId2"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"one"),
            ("xl/worksheets/sheet2.xml", b"two"),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "duplicate workbook sheet identity");
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_relationship_id() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="CIV" sheetId="2" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"one")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "duplicate workbook sheet relationship identity"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_relationship_declaration() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"one"),
            ("xl/worksheets/sheet2.xml", b"two"),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "duplicate workbook relationship identity"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_resolved_part() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="CIV" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="/xl/worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"one")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "duplicate worksheet part path");
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_normalized_workbook_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    for alias in [
        "./worksheets/sheet1.xml",
        "/xl/worksheets/%73heet1.xml",
        r"worksheets\sheet1.xml",
    ] {
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="{alias}"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "duplicate workbook relationship target");
    }
}

#[test]
fn ordered_worksheet_discovery_rejects_external_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="https://example.invalid/sheet.xml" TargetMode="External"/></Relationships>"#
    );
    let error =
        ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

    assert_eq!(
        error.to_string(),
        "external workbook relationship target is not allowed"
    );
}

#[test]
fn ordered_worksheet_discovery_accepts_package_internal_parent_relative_custom_xml() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../customXml/item1.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.xml"/></Relationships>"#
    );

    let discovery = ordered_workbook_discovery(&mut fixture_from_members(
        &workbook,
        &relationships,
        [
            ("xl/worksheets/sheet1.xml", b"worksheet".as_slice()),
            ("customXml/item1.xml", b"custom xml".as_slice()),
            ("xl/metadata.xml", b"sheet metadata".as_slice()),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(
        discovery.worksheets[0].part_path,
        "xl/worksheets/sheet1.xml"
    );
}

#[test]
fn ordered_worksheet_discovery_accepts_explicit_internal_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml" TargetMode="Internal"/></Relationships>"#
    );

    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "MEC");
}

#[test]
fn ordered_worksheet_discovery_rejects_encoded_traversal_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/%2E%2E/evil.xml"/></Relationships>"#
    );
    let error =
        ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship escapes the package"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_encoded_forbidden_target_character() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml%3Fignored"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml?ignored", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship escapes the package"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_non_worksheet_relationship() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Chart" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet" Target="chartsheets/sheet1.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        relationships,
        &[("xl/chartsheets/sheet1.xml", b"chart")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook sheet relationship is not a worksheet"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_relationship_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = r#"<Relationships><Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook sheet relationship is not a worksheet"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_missing_worksheet_member() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/missing.xml"/></Relationships>"#
    );
    let error =
        ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

    assert_eq!(error.to_string(), "worksheet package member is missing");
}

#[test]
fn internal_relationship_targets_require_exact_package_members() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let missing_workbook_auxiliary = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles/missing.xml"/></Relationships>"#
    );
    let workbook_error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &missing_workbook_auxiliary,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();
    assert_eq!(
        workbook_error.to_string(),
        "workbook relationship package member is missing"
    );

    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdDrawing" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/missing.xml"/></Relationships>"#;
    let worksheet_members: [(&str, &[u8]); 2] = [
        ("xl/worksheets/sheet1.xml", b"worksheet"),
        (
            "xl/worksheets/_rels/sheet1.xml.rels",
            worksheet_relationships,
        ),
    ];
    let worksheet_relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &worksheet_relationships,
        worksheet_members.as_slice(),
    ))
    .unwrap_err();
    assert_eq!(
        worksheet_error.to_string(),
        "worksheet relationship package member is missing"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_unknown_visibility_state() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" state="mystery" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"one")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "worksheet visibility state is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_zero_worksheets() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets/></workbook>"#
    );
    let error =
        ordered_workbook_discovery(&mut fixture(&workbook, "<Relationships/>", &[])).unwrap_err();

    assert_eq!(error.to_string(), "workbook contains no worksheets");
}

#[test]
fn ordered_worksheet_discovery_rejects_251_worksheets() {
    let mut sheets = String::new();
    let mut relationship_elements = String::new();
    let mut members = Vec::new();
    for index in 1..=251 {
        sheets.push_str(&format!(
            r#"<sheet name="Sheet{index}" sheetId="{index}" r:id="rId{index}"/>"#
        ));
        relationship_elements.push_str(&format!(
            r#"<Relationship Id="rId{index}" Type="{REL}" Target="worksheets/sheet{index}.xml"/>"#
        ));
        members.push((
            format!("xl/worksheets/sheet{index}.xml"),
            Vec::from(b"sheet".as_slice()),
        ));
    }
    let workbook =
        format!(r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets>{sheets}</sheets></workbook>"#);
    let relationships = format!(r#"<Relationships>{relationship_elements}</Relationships>"#);
    let error = ordered_workbook_discovery(&mut fixture_from_members(
        &workbook,
        &relationships,
        members
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook has too many worksheets");
}

#[test]
fn ordered_worksheet_discovery_rejects_oversized_source_before_zip_parse() {
    let error = ordered_workbook_discovery(&mut SparseReader {
        len: super::super::MAX_PACKAGE_BYTES + 1,
        position: 0,
    })
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook source exceeded the size limit");
}

#[test]
fn ordered_worksheet_discovery_rejects_declared_entry_count_before_zip_parse() {
    let error = ordered_workbook_discovery(&mut declared_entry_count_fixture(
        u16::try_from(super::super::MAX_ENTRIES + 1).unwrap(),
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP entry count is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_decoy_eocd_before_parser_fallback() {
    let error = ordered_workbook_discovery(&mut append_decoy_eocd(ordered_fixture())).unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_inflated_final_eocd_before_parser_fallback() {
    let error = ordered_workbook_discovery(&mut append_parser_fallback_eocd(ordered_fixture()))
        .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_semantically_invalid_final_directory() {
    let error =
        ordered_workbook_discovery(&mut append_semantically_invalid_directory(ordered_fixture()))
            .unwrap_err();

    assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
}
