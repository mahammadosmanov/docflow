use super::*;

#[test]
fn guarded_follow_up_apply_merge_budget_exact_boundary_and_one_over() {
    let mut exact = MergeComparisonBudget::for_apply(MAX_APPLY_MERGE_COMPARISONS - 1).unwrap();
    exact.consume().unwrap();
    let one_over = exact.consume().unwrap_err();
    assert_eq!(one_over.kind(), OoxmlErrorKind::LimitExceeded);

    let already_over =
        MergeComparisonBudget::for_apply(MAX_APPLY_MERGE_COMPARISONS + 1).unwrap_err();
    assert_eq!(already_over.kind(), OoxmlErrorKind::LimitExceeded);
}

#[test]
fn package_workbook_authority_authenticates_exact_internal_target() {
    let trusted_workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Trusted" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let trusted_relationships = format!(
        r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let decoy_workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Decoy" sheetId="9" r:id="rId9"/></sheets></workbook>"#
    );
    let decoy_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId9" Type="{REL}" Target="worksheets/decoy.xml"/></Relationships>"#
    );
    let package_relationships = format!(
        r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="custom/book.xml"/></Relationships>"#
    );
    let trusted_members: [(&str, &[u8]); 4] = [
        ("custom/worksheets/sheet1.xml", b"trusted worksheet"),
        ("xl/workbook.xml", decoy_workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", decoy_relationships.as_bytes()),
        ("xl/worksheets/decoy.xml", b"decoy worksheet"),
    ];
    let discovery = ordered_workbook_discovery(&mut fixture_with_workbook_part(
        Some(&package_relationships),
        "custom/book.xml",
        &trusted_workbook,
        &trusted_relationships,
        trusted_members,
    ))
    .unwrap();

    assert_eq!(discovery.worksheets.len(), 1);
    assert_eq!(discovery.worksheets[0].name, "Trusted");
    assert_eq!(
        discovery.worksheets[0].part_path,
        "custom/worksheets/sheet1.xml"
    );

    let strict_office_document =
        "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
    let strict_worksheet = "http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet";
    let strict_workbook = r#"<workbook xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main" xmlns:r="http://purl.oclc.org/ooxml/officeDocument/relationships"><sheets><sheet name="Strict" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let strict_relationships = format!(
        r#"<Relationships xmlns="http://purl.oclc.org/ooxml/package/relationships"><Relationship Id="rId1" Type="{strict_worksheet}" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let strict_package_relationships = format!(
        r#"<Relationships xmlns="http://purl.oclc.org/ooxml/package/relationships"><Relationship Id="rIdWorkbook" Type="{strict_office_document}" Target="strict/book.xml"/></Relationships>"#
    );
    let strict_members: [(&str, &[u8]); 1] =
        [("strict/worksheets/sheet1.xml", b"strict worksheet")];
    let strict_discovery = ordered_workbook_discovery(&mut fixture_with_workbook_part(
        Some(&strict_package_relationships),
        "strict/book.xml",
        strict_workbook,
        &strict_relationships,
        strict_members,
    ))
    .unwrap();
    assert_eq!(strict_discovery.worksheets[0].name, "Strict");

    let invalid_package_relationships = [
        None,
        Some(format!(
            r#"<Relationships xmlns="urn:spoof"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/></Relationships>"#
        )),
        Some(format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdOne" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/><Relationship Id="rIdTwo" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/></Relationships>"#
        )),
        Some(format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml" TargetMode="External"/></Relationships>"#
        )),
        Some(format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="urn:unknown/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
        )),
        Some(format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="missing/book.xml"/></Relationships>"#
        )),
    ];
    for package_relationships in invalid_package_relationships {
        let decoy_members: [(&str, &[u8]); 1] = [("xl/worksheets/decoy.xml", b"decoy worksheet")];
        let mut package = fixture_with_workbook_part(
            package_relationships.as_deref(),
            "xl/workbook.xml",
            &decoy_workbook,
            &decoy_relationships,
            decoy_members,
        );
        assert!(
            ordered_workbook_discovery(&mut package).is_err(),
            "invalid package workbook relationship must fail closed"
        );
    }
}

#[test]
fn xml_parser_rejects_excessive_element_depth() {
    let error = match parse_document_with_limits(b"<a><b><c/></b></a>", 2, 100) {
        Ok(_) => panic!("excessively deep XML must fail closed"),
        Err(error) => error,
    };

    assert_eq!(error.to_string(), "XML element depth exceeded the limit");
    assert_eq!(error.kind(), OoxmlErrorKind::LimitExceeded);
}

#[test]
fn xml_parser_rejects_excessive_retained_nodes() {
    let error = match parse_document_with_limits(b"<a><b/><c/></a>", 10, 2) {
        Ok(_) => panic!("excessive retained XML nodes must fail closed"),
        Err(error) => error,
    };

    assert_eq!(error.to_string(), "XML node count exceeded the limit");
    assert_eq!(error.kind(), OoxmlErrorKind::LimitExceeded);
}
