use super::*;

mod paths;
mod validation;
pub(super) use paths::*;
pub(super) use validation::*;

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
