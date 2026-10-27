use super::*;

#[test]
fn ordered_worksheet_discovery_reports_vba_project() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"not worksheet XML"),
            ("xl/vbaProject.bin", b"synthetic VBA bytes"),
        ],
    ))
    .unwrap();

    assert!(discovery.safety.has_vba_project);
}

#[test]
fn ordered_worksheet_discovery_vba_relationship_reports_noncanonical_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="macros/project.bin"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/macros/project.bin", b"synthetic VBA bytes"),
        ],
    ))
    .unwrap();

    assert!(discovery.safety.has_vba_project);
}

#[test]
fn ordered_worksheet_discovery_vba_relationship_rejects_missing_target_member() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="macros/project.bin"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "VBA project package member is missing");
}

#[test]
fn ordered_worksheet_discovery_vba_relationship_retains_canonical_orphan_signal() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/vbaProject.bin", b"orphan VBA bytes"),
        ],
    ))
    .unwrap();

    assert!(discovery.safety.has_vba_project);
}

#[test]
fn ordered_worksheet_discovery_rejects_non_internal_auxiliary_workbook_target_modes() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    for (target_mode, expected_error) in [
        (
            "External",
            "external workbook relationship target is not allowed",
        ),
        ("Bogus", "workbook relationship target mode is invalid"),
    ] {
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin" TargetMode="{target_mode}"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/vbaProject.bin", b"VBA"),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), expected_error);
    }
}

#[test]
fn ordered_worksheet_discovery_rejects_encoded_forbidden_auxiliary_workbook_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin%3Fignored"/></Relationships>"#
    );

    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/vbaProject.bin", b"VBA"),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship escapes the package"
    );
}

#[test]
fn ordered_worksheet_discovery_reports_workbook_protection() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><workbookProtection lockStructure="1"/><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
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
fn ordered_worksheet_discovery_reports_xlm_macro_sheet_relationship() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdMacro" Type="http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet" Target="macrosheets/sheet2.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet cells are not read"),
            (
                "xl/macrosheets/sheet2.xml",
                b"macro sheet cells are not read",
            ),
        ],
    ))
    .unwrap();

    assert!(discovery.safety.has_xlm_macro_sheet);
}

#[test]
fn ordered_worksheet_discovery_counts_external_links() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdLink1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="externalLinks/externalLink1.xml"/><Relationship Id="rIdLink2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="externalLinks/externalLink2.xml"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/externalLinks/externalLink1.xml", b"link one"),
            ("xl/externalLinks/externalLink2.xml", b"link two"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.safety.external_link_count, 2);
}

#[test]
fn ordered_worksheet_discovery_counts_explicit_internal_connections() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml" TargetMode="Internal"/></Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/connections.xml",
                format!(
                    r#"<connections xmlns="{MAIN}" count="2"><connection id="1"/><connection id="2"/></connections>"#
                )
                .as_bytes(),
            ),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.safety.connection_count, 2);
}

#[test]
fn ordered_worksheet_discovery_uses_canonical_connection_expanded_names() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml"/></Relationships>"#
    );
    let connections = format!(
        r#"<connections xmlns="{MAIN}" xmlns:x="urn:foreign" count="1" x:count="99"><connection id="1"/><x:connection id="2"/></connections>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/connections.xml", connections.as_bytes()),
        ],
    ))
    .unwrap();
    assert_eq!(discovery.safety.connection_count, 1);

    let foreign_connections =
        br#"<x:connections xmlns:x="urn:foreign"><x:connection id="1"/></x:connections>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/connections.xml", foreign_connections),
        ],
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "relationship count is invalid");
}

#[test]
fn ordered_worksheet_discovery_counts_explicit_internal_query_tables_without_reading_cells() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml" TargetMode="Internal"/><Relationship Id="rIdQuery2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable2.xml" TargetMode="Internal"/></Relationships>"#;
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            (
                "xl/worksheets/sheet1.xml",
                b"deliberately malformed worksheet XML with secret cell text",
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
            ("xl/queryTables/queryTable1.xml", b"query one"),
            ("xl/queryTables/queryTable2.xml", b"query two"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.safety.query_table_count, 2);
}

#[test]
fn ordered_worksheet_discovery_rejects_unknown_worksheet_relationship_class() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdMystery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/mysteryData" Target="../mystery.bin"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
            ("xl/mystery.bin", b"unknown data"),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "unknown worksheet relationship class");
}

#[test]
fn ordered_worksheet_discovery_rejects_external_worksheet_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="https://example.invalid/query" TargetMode="External"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "external worksheet relationship target is not allowed"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_worksheet_relationship_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="urn:spoof"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationships namespace is invalid"
    );
}

#[test]
fn ordered_worksheet_discovery_ignores_foreign_worksheet_relationship_elements() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships" xmlns:ext="urn:extension"><ext:Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.safety.query_table_count, 0);
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_worksheet_relationship_identity() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable2.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "duplicate worksheet relationship identity"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_duplicate_worksheet_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "duplicate worksheet relationship target");
}

#[test]
fn ordered_worksheet_discovery_rejects_missing_worksheet_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship target is missing"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_encoded_forbidden_worksheet_relationship_target() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml%3Fignored"/></Relationships>"#;
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "worksheet relationship escapes the package"
    );
}

#[test]
fn ordered_worksheet_discovery_rejects_unknown_workbook_relationship_class() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdMystery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/mysteryData" Target="mystery.bin"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/mystery.bin", b"unknown data"),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "unknown workbook relationship class");
}

#[test]
fn ordered_worksheet_discovery_accepts_volatile_dependencies_and_modern_office_relationships() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships>
            <Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/>
            <Relationship Id="rIdVolatile" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/volatileDependencies" Target="volatileDependencies.xml"/>
            <Relationship Id="rIdCalc" Type="http://schemas.microsoft.com/office/2020/07/relationships/calcFeatures" Target="calcFeatures.xml"/>
            <Relationship Id="rIdCustomUI" Type="http://schemas.microsoft.com/office/2006/relationships/ui/customUI" Target="customUI/customUI.xml"/>
        </Relationships>"#
    );
    let discovery = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            ("xl/volatileDependencies.xml", b"<volTypes/>"),
            ("xl/calcFeatures.xml", b"<calcFeatures/>"),
            ("xl/customUI/customUI.xml", b"<customUI/>"),
        ],
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "MEC");
}

#[test]
fn ordered_worksheet_discovery_rejects_spoofed_workbook_relationship_document_namespace() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="urn:spoof"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[("xl/worksheets/sheet1.xml", b"worksheet")],
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "workbook relationships namespace is invalid"
    );
}

#[test]
fn ordered_worksheet_discovery_ignores_foreign_workbook_relationship_elements() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships" xmlns:ext="urn:extension"><ext:Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet" Target="worksheets/decoy.xml" TargetMode="External"/><ext:Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="missing.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
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
    assert_eq!(
        discovery.worksheets[0].part_path,
        "xl/worksheets/sheet1.xml"
    );
    assert!(!discovery.safety.has_xlm_macro_sheet);
}

#[test]
fn ordered_worksheet_discovery_rejects_malformed_connection_count() {
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml"/></Relationships>"#
    );
    let error = ordered_workbook_discovery(&mut fixture(
        &workbook,
        &relationships,
        &[
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/connections.xml",
                br#"<connections count="many"><connection id="1"/></connections>"#,
            ),
        ],
    ))
    .unwrap_err();

    assert_eq!(error.to_string(), "relationship count is invalid");
}

#[test]
fn ordered_worksheet_discovery_rejects_oversized_sheet_attribute() {
    let name = "N".repeat(super::super::MAX_SHEET_NAME_BYTES + 1);
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="{name}" sheetId="1" r:id="rId1"/></sheets></workbook>"#
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

    assert_eq!(
        error.to_string(),
        "workbook sheet name exceeded the size limit"
    );
}
