use super::*;

pub(super) fn authenticated_workbook_parts<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<WorkbookPackageParts, OoxmlError> {
    let package_relationships = parse_document(&read_entry(archive, "_rels/.rels")?)?;
    let root = validate_relationship_document_namespace(&package_relationships, "package")?;
    let mut workbook_part = None;
    for relationship in relationship_elements(root) {
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "package relationship type",
        )?;
        let is_office_document = relationship_type.as_deref().is_some_and(|value| {
            matches!(
                value,
                TRANSITIONAL_OFFICE_DOCUMENT_RELATIONSHIP | STRICT_OFFICE_DOCUMENT_RELATIONSHIP
            )
        });
        if !is_office_document {
            continue;
        }
        if workbook_part.is_some() {
            return Err(OoxmlError::invalid(
                "package workbook relationship is ambiguous",
            ));
        }
        bounded_unqualified_attribute(
            relationship,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "package workbook relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("package workbook relationship identity is missing"))?;
        if relationship_target_mode_is_external(relationship, "package workbook")? {
            return Err(OoxmlError::invalid(
                "external package workbook relationship target is not allowed",
            ));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "package workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("package workbook relationship target is missing"))?;
        workbook_part = Some(normalize_package_root_target(&target)?);
    }
    let workbook = workbook_part
        .ok_or_else(|| OoxmlError::invalid("package workbook relationship is missing"))?;
    if !has_entry(archive, &workbook)? {
        return Err(OoxmlError::invalid("workbook package member is missing"));
    }
    let relationships = relationship_part_path(&workbook)?;
    if !has_entry(archive, &relationships)? {
        return Err(OoxmlError::invalid(
            "workbook relationships package member is missing",
        ));
    }
    Ok(WorkbookPackageParts {
        workbook,
        relationships,
    })
}

pub(super) fn relationship_targets(
    document: &XmlDocument,
) -> Result<HashMap<String, String>, OoxmlError> {
    let mut targets = HashMap::new();
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        let id = bounded_unqualified_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship identity is missing"))?;
        let target = bounded_unqualified_attribute(
            element,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        if targets.insert(id, target).is_some() {
            return Err(OoxmlError::invalid(
                "duplicate workbook relationship identity",
            ));
        }
    }
    Ok(targets)
}

pub(super) fn internal_relationship_target_for_class(
    document: &XmlDocument,
    source_part_path: &str,
    expected_class: &str,
) -> Result<Option<String>, OoxmlError> {
    let root = validate_relationship_document_namespace(document, "workbook")?;
    let mut target = None;
    for relationship in relationship_elements(root) {
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship type is missing"))?;
        if relationship_class(&relationship_type) != Some(expected_class) {
            continue;
        }
        if target.is_some() {
            return Err(OoxmlError::invalid(
                "workbook relationship class is ambiguous",
            ));
        }
        if relationship_target_mode_is_external(relationship, "workbook")? {
            return Err(OoxmlError::invalid(
                "external workbook relationship target is not allowed",
            ));
        }
        let raw_target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        target = Some(normalize_workbook_relationship_target(
            source_part_path,
            &raw_target,
        )?);
    }
    Ok(target)
}

pub(super) fn relationship_class(relationship_type: &str) -> Option<&str> {
    ooxml_relationship_class(relationship_type).or_else(|| {
        let rest = relationship_type.strip_prefix("http://schemas.microsoft.com/office/")?;
        let rel_idx = rest.find("/relationships/")?;
        let class = &rest[rel_idx + "/relationships/".len()..];
        let class = class.strip_prefix("ui/").unwrap_or(class);
        (!class.is_empty() && !class.contains('/')).then_some(class)
    })
}

pub(super) fn ooxml_relationship_class(relationship_type: &str) -> Option<&str> {
    let class = relationship_type
        .strip_prefix(TRANSITIONAL_RELATIONSHIP_PREFIX)
        .or_else(|| relationship_type.strip_prefix(STRICT_RELATIONSHIP_PREFIX))
        .or_else(|| {
            relationship_type
                .strip_prefix("http://schemas.openxmlformats.org/package/2006/relationships/")
        })
        .or_else(|| {
            relationship_type.strip_prefix("http://purl.oclc.org/ooxml/package/relationships/")
        })?;
    let class = class.strip_prefix("ui/").unwrap_or(class);
    (!class.is_empty() && !class.contains('/')).then_some(class)
}

pub(super) fn validate_relationship_document_namespace<'a>(
    document: &'a XmlDocument,
    subject: &str,
) -> Result<&'a XmlElement, OoxmlError> {
    let root_error = format!("{subject} relationships root is invalid");
    let root = document_root_local(document, b"Relationships", &root_error)?;
    let field = format!("{subject} relationships namespace");
    if !has_expanded_name(root, b"Relationships", &PACKAGE_RELATIONSHIP_NAMESPACES) {
        return Err(OoxmlError::invalid(format!("{field} is invalid")));
    }
    Ok(root)
}

pub(super) fn relationship_elements(root: &XmlElement) -> impl Iterator<Item = &XmlElement> {
    child_elements_expanded(root, b"Relationship", &PACKAGE_RELATIONSHIP_NAMESPACES)
}

pub(super) fn validate_workbook_relationship_classes<R: Read + Seek>(
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

pub(super) fn validate_worksheet_relationship_classes<R: Read + Seek>(
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

pub(super) fn relationship_target_is_external(
    document: &XmlDocument,
    relationship_id: &str,
) -> Result<bool, OoxmlError> {
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        let id = bounded_unqualified_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?;
        if id.as_deref() == Some(relationship_id) {
            return relationship_target_mode_is_external(element, "workbook");
        }
    }
    Ok(false)
}

pub(super) fn relationship_target_mode_is_external(
    relationship: &XmlElement,
    subject: &str,
) -> Result<bool, OoxmlError> {
    let context = format!("{subject} relationship target mode");
    match bounded_unqualified_attribute(relationship, b"TargetMode", MAX_STATE_BYTES, &context)?
        .as_deref()
    {
        None | Some("Internal") => Ok(false),
        Some("External") => Ok(true),
        Some(_) => Err(OoxmlError::invalid(format!(
            "{subject} relationship target mode is invalid"
        ))),
    }
}

pub(super) fn relationship_attribute(
    document: &XmlDocument,
    relationship_id: &str,
    name: &[u8],
) -> Result<Option<String>, OoxmlError> {
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        let id = bounded_unqualified_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?;
        if id.as_deref() == Some(relationship_id) {
            return bounded_unqualified_attribute(
                element,
                name,
                MAX_RELATIONSHIP_TYPE_BYTES,
                "workbook relationship type",
            );
        }
    }
    Ok(None)
}

pub(super) fn has_relationship_type(
    document: &XmlDocument,
    predicate: impl Fn(&str) -> bool,
) -> Result<bool, OoxmlError> {
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        if bounded_unqualified_attribute(
            element,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        .is_some_and(|relationship_type| predicate(&relationship_type))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn relationship_type_count(
    document: &XmlDocument,
    expected_class: &str,
) -> Result<u32, OoxmlError> {
    let mut count = 0_usize;
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        if bounded_unqualified_attribute(
            element,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        .as_deref()
        .and_then(relationship_class)
            == Some(expected_class)
        {
            count += 1;
        }
    }
    u32::try_from(count).map_err(|_| OoxmlError::invalid("relationship count is invalid"))
}

pub(super) fn related_collection_count<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    relationships: &XmlDocument,
    workbook_part_path: &str,
    expected_class: &str,
    collection_name: &[u8],
    item_name: &[u8],
) -> Result<u32, OoxmlError> {
    let mut count = 0_u32;
    let root = validate_relationship_document_namespace(relationships, "workbook")?;
    for relationship in relationship_elements(root) {
        let Some(relationship_type) = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        else {
            continue;
        };
        if relationship_class(&relationship_type) != Some(expected_class) {
            continue;
        }
        if relationship_target_mode_is_external(relationship, "workbook")? {
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
        let part_path = normalize_workbook_relationship_target(workbook_part_path, &target)?;
        let document = parse_document(&read_entry(archive, &part_path)?)?;
        let collection =
            document_root_local(&document, collection_name, "relationship count is invalid")?;
        if !has_expanded_name(collection, collection_name, &WORKBOOK_NAMESPACES) {
            return Err(OoxmlError::invalid("relationship count is invalid"));
        }
        let part_count =
            child_elements_expanded(collection, item_name, &WORKBOOK_NAMESPACES).count();
        if let Some(declared) = bounded_unqualified_attribute(
            collection,
            b"count",
            MAX_U32_ATTRIBUTE_BYTES,
            "relationship count",
        )? {
            let declared = declared
                .parse::<usize>()
                .map_err(|_| OoxmlError::invalid("relationship count is invalid"))?;
            if declared != part_count {
                return Err(OoxmlError::invalid("relationship count is invalid"));
            }
        }
        count = count
            .checked_add(
                u32::try_from(part_count)
                    .map_err(|_| OoxmlError::invalid("relationship count is invalid"))?,
            )
            .ok_or_else(|| OoxmlError::invalid("relationship count is invalid"))?;
    }
    Ok(count)
}

pub(super) fn worksheet_relationship_type_count<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheets: &[WorksheetDescriptor],
    expected_class: &str,
) -> Result<u32, OoxmlError> {
    let mut count = 0_u32;
    for worksheet in worksheets {
        let relationships_path = relationship_part_path(&worksheet.part_path)?;
        if !has_entry(archive, &relationships_path)? {
            continue;
        }
        let relationships = parse_document(&read_entry(archive, &relationships_path)?)?;
        validate_worksheet_relationship_classes(archive, &relationships, &worksheet.part_path)?;
        count = count
            .checked_add(relationship_type_count(&relationships, expected_class)?)
            .ok_or_else(|| OoxmlError::invalid("relationship count is invalid"))?;
    }
    Ok(count)
}

pub(super) fn normalize_package_root_target(target: &str) -> Result<String, OoxmlError> {
    let decoded = percent_decode_target(target)?.replace('\\', "/");
    if decoded.split('/').any(|segment| segment == "..") {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    normalize_relationship_target_from_parts(Vec::new(), target)
}

pub(super) fn normalize_workbook_relationship_target(
    workbook_part_path: &str,
    target: &str,
) -> Result<String, OoxmlError> {
    let decoded = percent_decode_target(target)?.replace('\\', "/");
    if decoded.split('/').any(|segment| segment == "..") {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    normalize_relationship_target(workbook_part_path, target)
}

pub(super) fn normalize_relationship_target(
    source_part_path: &str,
    target: &str,
) -> Result<String, OoxmlError> {
    let mut source_parts = source_part_path
        .split('/')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if source_parts.is_empty()
        || source_parts
            .iter()
            .any(|part| part.is_empty() || matches!(part.as_str(), "." | ".."))
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    source_parts.pop();
    normalize_relationship_target_from_parts(source_parts, target)
}

pub(super) fn normalize_relationship_target_from_parts(
    mut source_parts: Vec<String>,
    target: &str,
) -> Result<String, OoxmlError> {
    if target.is_empty()
        || target.starts_with("//")
        || target.contains(['?', '#', '\0'])
        || target.contains(':')
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    let target = percent_decode_target(target)?.replace('\\', "/");
    if target.is_empty()
        || target.starts_with("//")
        || target.contains(['?', '#', '\0'])
        || target.contains(':')
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    let absolute = target.starts_with('/');
    let mut parts = if absolute {
        Vec::new()
    } else {
        std::mem::take(&mut source_parts)
    };

    for component in target.trim_start_matches('/').split('/') {
        match component {
            "" => {
                return Err(OoxmlError::invalid(
                    "worksheet relationship escapes the package",
                ));
            }
            "." => {}
            ".." => {
                if parts.is_empty() {
                    return Err(OoxmlError::invalid(
                        "worksheet relationship escapes the package",
                    ));
                }
                parts.pop();
            }
            part => parts.push(part.to_string()),
        }
    }
    if parts.is_empty() {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    Ok(parts.join("/"))
}

pub(super) fn relationship_part_path(part_path: &str) -> Result<String, OoxmlError> {
    let part = Path::new(part_path);
    let file_name = part
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| OoxmlError::invalid("relationship source part path is invalid"))?;
    let parent = part.parent().and_then(Path::to_str).unwrap_or_default();
    if parent.is_empty() {
        Ok(format!("_rels/{file_name}.rels"))
    } else {
        Ok(format!("{parent}/_rels/{file_name}.rels"))
    }
}

pub(super) fn percent_decode_target(target: &str) -> Result<String, OoxmlError> {
    let bytes = target.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let encoded = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| OoxmlError::invalid("worksheet relationship target is invalid"))?;
            let hex = std::str::from_utf8(encoded)
                .ok()
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .ok_or_else(|| OoxmlError::invalid("worksheet relationship target is invalid"))?;
            decoded.push(hex);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|_| OoxmlError::invalid("worksheet relationship target is invalid"))
}
