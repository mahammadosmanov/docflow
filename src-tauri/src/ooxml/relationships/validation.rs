use super::*;

pub(in crate::ooxml) fn validate_workbook_relationship_classes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    document: &XmlDocument,
    workbook_part_path: &str,
) -> Result<bool, OoxmlError> {
    const KNOWN_CLASSES: &[&str] = &[
        "worksheet",
        "styles",
        "theme",
        "sharedStrings",
        "calcChain",
        "externalLink",
        "connections",
        "vbaProject",
        "vbaProjectSignature",
        "xlMacrosheet",
        "xlIntlMacrosheet",
        "pivotCacheDefinition",
        "customXml",
        "customXmlMappings",
        "xmlMaps",
        "metadata",
        "sheetMetadata",
        "person",
        "richValueRel",
        "slicerCache",
        "timelineCache",
        "chartsheet",
        "dialogsheet",
        "table",
        "queryTable",
        "pivotTable",
        "printerSettings",
        "drawing",
        "vmlDrawing",
        "image",
        "rdRichValueTypes",
        "rdRichValueStructure",
        "rdArray",
        "model",
        "dataModel",
        "webextension",
        "feature",
        "features",
        "calcFeatures",
        "volatileDependencies",
        "customData",
        "customDataProperties",
        "customProperty",
        "customProperties",
        "extendedProperties",
        "package",
        "hyperlink",
        "customUI",
        "attachedToolbars",
        "activeXControlBinary",
        "usernames",
        "revisionLog",
        "revisionHeaders",
    ];

    let root = validate_relationship_document_namespace(document, "workbook")?;
    let mut relationship_targets = BTreeSet::new();
    let mut internal_targets = Vec::new();
    let mut has_vba_project = false;
    for relationship in relationship_elements(root) {
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship type is missing"))?;
        let Some(relationship_class) = relationship_class(&relationship_type) else {
            return Err(OoxmlError::invalid("unknown workbook relationship class"));
        };
        if !KNOWN_CLASSES.contains(&relationship_class) {
            return Err(OoxmlError::invalid("unknown workbook relationship class"));
        }
        if relationship_target_mode_is_external(relationship, "workbook")? {
            if relationship_class == "externalLink" || relationship_class == "hyperlink" {
                continue;
            }
            return Err(OoxmlError::invalid(
                "external workbook relationship target is not allowed",
            ));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        let normalized_target = normalize_relationship_target(workbook_part_path, &target)?;
        if !relationship_targets.insert(normalized_target.clone()) {
            return Err(OoxmlError::invalid(
                "duplicate workbook relationship target",
            ));
        }
        internal_targets.push((normalized_target, relationship_class == "vbaProject"));
        has_vba_project |= relationship_class == "vbaProject";
    }
    for (normalized_target, is_vba_project) in internal_targets {
        if !has_entry(archive, &normalized_target)? {
            let message = if is_vba_project {
                "VBA project package member is missing"
            } else {
                "workbook relationship package member is missing"
            };
            return Err(OoxmlError::invalid(message));
        }
    }
    Ok(has_vba_project)
}

pub(in crate::ooxml) fn validate_worksheet_relationship_classes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    document: &XmlDocument,
    worksheet_part_path: &str,
) -> Result<(), OoxmlError> {
    const KNOWN_CLASSES: &[&str] = &[
        "hyperlink",
        "drawing",
        "vmlDrawing",
        "comments",
        "threadedComment",
        "table",
        "pivotTable",
        "queryTable",
        "printerSettings",
        "oleObject",
        "ctrlProp",
        "image",
        "slicer",
        "timeline",
        "customXml",
        "metadata",
        "chart",
        "chartUserShapes",
        "diagramData",
        "diagramLayout",
        "diagramStyle",
        "diagramColors",
        "package",
        "webextension",
        "customProperty",
        "feature",
    ];

    let root = validate_relationship_document_namespace(document, "worksheet")?;
    let mut relationship_ids = BTreeSet::new();
    let mut relationship_targets = BTreeSet::new();

    for relationship in relationship_elements(root) {
        let relationship_id = bounded_unqualified_attribute(
            relationship,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "worksheet relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("worksheet relationship identity is missing"))?;
        if !relationship_ids.insert(relationship_id) {
            return Err(OoxmlError::invalid(
                "duplicate worksheet relationship identity",
            ));
        }
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "worksheet relationship type",
        )?
        .ok_or_else(|| OoxmlError::invalid("worksheet relationship type is missing"))?;
        let Some(relationship_class) = relationship_class(&relationship_type) else {
            return Err(OoxmlError::invalid("unknown worksheet relationship class"));
        };
        if !KNOWN_CLASSES.contains(&relationship_class) {
            return Err(OoxmlError::invalid("unknown worksheet relationship class"));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "worksheet relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("worksheet relationship target is missing"))?;
        if target.is_empty() {
            return Err(OoxmlError::invalid(
                "worksheet relationship target is missing",
            ));
        }
        if relationship_target_mode_is_external(relationship, "worksheet")? {
            return Err(OoxmlError::invalid(
                "external worksheet relationship target is not allowed",
            ));
        }
        let normalized_target = normalize_relationship_target(worksheet_part_path, &target)?;
        if !relationship_targets.insert(normalized_target.clone()) {
            return Err(OoxmlError::invalid(
                "duplicate worksheet relationship target",
            ));
        }
    }
    for normalized_target in relationship_targets {
        if !has_entry(archive, &normalized_target)? {
            return Err(OoxmlError::invalid(
                "worksheet relationship package member is missing",
            ));
        }
    }
    Ok(())
}
