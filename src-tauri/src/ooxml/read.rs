use super::*;

pub fn read_member(source: &Path, name: &str) -> Result<Vec<u8>, OoxmlError> {
    validate_source(source)?;
    let mut archive = open_archive(File::open(source)?)?;
    read_entry(&mut archive, name)
}

pub fn sheet_infos_from_path(source: &Path) -> Result<BTreeMap<String, SheetInfo>, OoxmlError> {
    validate_source(source)?;
    let mut input = File::open(source)?;
    let discovery = ordered_workbook_discovery(&mut input)?;
    Ok(project_sheet_infos(&discovery))
}

pub(super) fn project_sheet_infos(
    discovery: &OrderedWorkbookDiscovery,
) -> BTreeMap<String, SheetInfo> {
    discovery
        .worksheets
        .iter()
        .map(|worksheet| {
            (
                worksheet.name.clone(),
                SheetInfo {
                    path: worksheet.part_path.clone(),
                    sheet_id: worksheet.sheet_id,
                },
            )
        })
        .collect()
}

pub fn ordered_workbook_discovery<R: Read + Seek>(
    source: &mut R,
) -> Result<OrderedWorkbookDiscovery, OoxmlError> {
    let mut archive = open_archive(source)?;
    let workbook_parts = authenticated_workbook_parts(&mut archive)?;
    let workbook = parse_document(&read_entry(&mut archive, &workbook_parts.workbook)?)?;
    let relationships = parse_document(&read_entry(&mut archive, &workbook_parts.relationships)?)?;
    validate_relationship_document_namespace(&relationships, "workbook")?;
    let relation_targets = relationship_targets(&relationships)?;
    let connection_count = related_collection_count(
        &mut archive,
        &relationships,
        &workbook_parts.workbook,
        "connections",
        b"connections",
        b"connection",
    )?;
    let mut safety = WorkbookSafetyFacts {
        has_vba_project: has_entry(&mut archive, "xl/vbaProject.bin")?,
        has_xlm_macro_sheet: has_relationship_type(&relationships, |relationship_type| {
            matches!(
                relationship_class(relationship_type),
                Some("xlMacrosheet" | "xlIntlMacrosheet")
            )
        })?,
        external_link_count: relationship_type_count(&relationships, "externalLink")?,
        connection_count,
        ..WorkbookSafetyFacts::default()
    };
    let mut worksheets = Vec::new();
    let mut names = BTreeSet::new();
    let mut sheet_ids = BTreeSet::new();
    let mut relationship_ids = BTreeSet::new();
    let mut part_paths = BTreeSet::new();
    let workbook_root = document_root_local(&workbook, b"workbook", "workbook root is invalid")?;
    if !has_expanded_name(workbook_root, b"workbook", &WORKBOOK_NAMESPACES) {
        return Err(OoxmlError::invalid("workbook namespace is invalid"));
    }
    safety.has_workbook_protection =
        child_elements_expanded(workbook_root, b"workbookProtection", &WORKBOOK_NAMESPACES)
            .next()
            .is_some();
    let sheets = single_child_element_expanded(
        workbook_root,
        b"sheets",
        &WORKBOOK_NAMESPACES,
        "workbook sheets container is invalid",
    )?;
    for (index, element) in
        child_elements_expanded(sheets, b"sheet", &WORKBOOK_NAMESPACES).enumerate()
    {
        if index >= MAX_WORKSHEETS {
            return Err(OoxmlError::limit_exceeded(
                "workbook has too many worksheets",
            ));
        }
        let name = bounded_unqualified_attribute(
            element,
            b"name",
            MAX_SHEET_NAME_BYTES,
            "workbook sheet name",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook sheet name is missing"))?;
        if !names.insert(name.clone()) {
            return Err(OoxmlError::invalid("duplicate workbook sheet name"));
        }
        let relationship_id = bounded_office_relationship_attribute(
            element,
            b"id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook sheet relationship identity",
        )?;
        if relationship_id.is_none()
            && bounded_qualified_attribute(
                element,
                b"r:id",
                MAX_RELATIONSHIP_ID_BYTES,
                "workbook sheet relationship identity",
            )?
            .is_some()
        {
            return Err(OoxmlError::invalid(
                "workbook relationship namespace is invalid",
            ));
        }
        let relationship_id = relationship_id
            .ok_or_else(|| OoxmlError::invalid("workbook sheet relationship is missing"))?;
        if !relationship_ids.insert(relationship_id.clone()) {
            return Err(OoxmlError::invalid(
                "duplicate workbook sheet relationship identity",
            ));
        }
        let sheet_id = bounded_unqualified_attribute(
            element,
            b"sheetId",
            MAX_U32_ATTRIBUTE_BYTES,
            "workbook sheet identity",
        )?
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| OoxmlError::invalid("workbook sheet identity is invalid"))?;
        if !sheet_ids.insert(sheet_id) {
            return Err(OoxmlError::invalid("duplicate workbook sheet identity"));
        }
        let target = relation_targets
            .get(&relationship_id)
            .ok_or_else(|| OoxmlError::invalid("workbook sheet target is missing"))?;
        if relationship_target_is_external(&relationships, &relationship_id)? {
            return Err(OoxmlError::invalid(
                "external workbook relationship target is not allowed",
            ));
        }
        let relationship_type = relationship_attribute(&relationships, &relationship_id, b"Type")?
            .ok_or_else(|| OoxmlError::invalid("workbook sheet relationship type is missing"))?;
        if ooxml_relationship_class(&relationship_type) != Some("worksheet") {
            return Err(OoxmlError::invalid(
                "workbook sheet relationship is not a worksheet",
            ));
        }
        let state = bounded_unqualified_attribute(
            element,
            b"state",
            MAX_STATE_BYTES,
            "worksheet visibility state",
        )?;
        let visibility = match state.as_deref() {
            None | Some("visible") => WorksheetVisibility::Visible,
            Some("hidden") => WorksheetVisibility::Hidden,
            Some("veryHidden") => WorksheetVisibility::VeryHidden,
            Some(_) => {
                return Err(OoxmlError::invalid("worksheet visibility state is invalid"));
            }
        };
        let part_path = normalize_workbook_relationship_target(&workbook_parts.workbook, target)?;
        if !part_paths.insert(part_path.clone()) {
            return Err(OoxmlError::invalid("duplicate worksheet part path"));
        }
        if !has_entry(&mut archive, &part_path)? {
            return Err(OoxmlError::invalid("worksheet package member is missing"));
        }
        worksheets.push(WorksheetDescriptor {
            workbook_index: u32::try_from(index + 1)
                .map_err(|_| OoxmlError::invalid("worksheet count is invalid"))?,
            sheet_id,
            relationship_id,
            relationship_type,
            part_path,
            name,
            visibility,
        });
    }
    if worksheets.is_empty() {
        return Err(OoxmlError::invalid("workbook contains no worksheets"));
    }
    safety.has_vba_project |= validate_workbook_relationship_classes(
        &mut archive,
        &relationships,
        &workbook_parts.workbook,
    )?;
    safety.query_table_count =
        worksheet_relationship_type_count(&mut archive, &worksheets, "queryTable")?;
    Ok(OrderedWorkbookDiscovery { worksheets, safety })
}

pub fn read_workbook(source: &Path) -> Result<WorkbookData, OoxmlError> {
    validate_source(source)?;
    let mut input = File::open(source)?;
    read_workbook_from_reader(&mut input)
}

pub fn read_workbook_and_discovery<R: Read + Seek>(
    source: &mut R,
) -> Result<(OrderedWorkbookDiscovery, WorkbookData), OoxmlError> {
    let discovery = ordered_workbook_discovery(source)?;
    let infos = project_sheet_infos(&discovery);
    source.seek(SeekFrom::Start(0))?;
    let mut archive = open_archive(source)?;
    let workbook_parts = authenticated_workbook_parts(&mut archive)?;
    let relationships = parse_document(&read_entry(&mut archive, &workbook_parts.relationships)?)?;
    let shared_strings = match internal_relationship_target_for_class(
        &relationships,
        &workbook_parts.workbook,
        "sharedStrings",
    )? {
        Some(part_path) => shared_strings(&read_entry(&mut archive, &part_path)?)?,
        None => Vec::new(),
    };
    let mut sheets = BTreeMap::new();
    let mut merge_budget = MergeComparisonBudget::for_validation();
    for (name, info) in infos {
        let raw = read_entry(&mut archive, &info.path)?;
        sheets.insert(
            name,
            worksheet_data(&raw, &shared_strings, &mut merge_budget)?,
        );
    }
    Ok((
        discovery,
        WorkbookData {
            sheets,
            merge_validation_comparisons: merge_budget.consumed,
            merge_validation_complete: true,
        },
    ))
}

pub fn read_workbook_from_reader<R: Read + Seek>(
    source: &mut R,
) -> Result<WorkbookData, OoxmlError> {
    read_workbook_and_discovery(source).map(|(_, data)| data)
}
pub fn read_workbook_selective(
    source: &Path,
    wanted_sheets: &[&str],
) -> Result<WorkbookData, OoxmlError> {
    validate_source(source)?;
    let mut input = File::open(source)?;
    read_workbook_selective_from_reader(&mut input, wanted_sheets)
}

pub fn read_workbook_selective_from_reader<R: Read + Seek>(
    source: &mut R,
    wanted_sheets: &[&str],
) -> Result<WorkbookData, OoxmlError> {
    let discovery = ordered_workbook_discovery(source)?;
    let infos = project_sheet_infos(&discovery);
    source.seek(SeekFrom::Start(0))?;
    let mut archive = open_archive(source)?;
    let workbook_parts = authenticated_workbook_parts(&mut archive)?;
    let relationships = parse_document(&read_entry(&mut archive, &workbook_parts.relationships)?)?;
    let shared_strings = match internal_relationship_target_for_class(
        &relationships,
        &workbook_parts.workbook,
        "sharedStrings",
    )? {
        Some(part_path) => shared_strings(&read_entry(&mut archive, &part_path)?)?,
        None => Vec::new(),
    };
    let mut sheets = BTreeMap::new();
    let mut merge_budget = MergeComparisonBudget::for_validation();
    for (name, info) in infos {
        if wanted_sheets
            .iter()
            .any(|wanted| wanted.eq_ignore_ascii_case(&name))
        {
            let raw = read_entry(&mut archive, &info.path)?;
            sheets.insert(
                name,
                worksheet_data(&raw, &shared_strings, &mut merge_budget)?,
            );
        }
    }
    Ok(WorkbookData {
        sheets,
        merge_validation_comparisons: merge_budget.consumed,
        merge_validation_complete: true,
    })
}

pub fn package_has_signature<R: Read + Seek>(source: &mut R) -> Result<bool, OoxmlError> {
    const TRANSITIONAL_SIGNATURE_ORIGIN: &str =
        "http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/origin";
    const STRICT_SIGNATURE_ORIGIN: &str =
        "http://purl.oclc.org/ooxml/package/relationships/digital-signature/origin";

    let mut archive = open_archive(source)?;
    let relationships = parse_document(&read_entry(&mut archive, "_rels/.rels")?)?;
    let root = validate_relationship_document_namespace(&relationships, "package")?;
    let mut signature_target = None;
    for relationship in relationship_elements(root) {
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "package relationship type",
        )?
        .ok_or_else(|| OoxmlError::invalid("package relationship type is missing"))?;
        if !matches!(
            relationship_type.as_str(),
            TRANSITIONAL_SIGNATURE_ORIGIN | STRICT_SIGNATURE_ORIGIN
        ) {
            continue;
        }
        if signature_target.is_some() {
            return Err(OoxmlError::invalid(
                "package signature relationship is ambiguous",
            ));
        }
        bounded_unqualified_attribute(
            relationship,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "package signature relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("package signature relationship identity is missing"))?;
        if relationship_target_mode_is_external(relationship, "package signature")? {
            return Err(OoxmlError::invalid(
                "external package signature relationship target is not allowed",
            ));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "package signature relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("package signature relationship target is missing"))?;
        let target = normalize_package_root_target(&target)?;
        if !is_signature_member(&target) {
            return Err(OoxmlError::invalid(
                "package signature relationship target is invalid",
            ));
        }
        signature_target = Some(target);
    }

    if let Some(target) = signature_target.as_deref()
        && !has_entry(&mut archive, target)?
    {
        return Err(OoxmlError::invalid(
            "package signature relationship target is missing",
        ));
    }
    let mut has_signature_member = false;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        has_signature_member |= is_signature_member(&entry.name()?);
    }
    Ok(signature_target.is_some() || has_signature_member)
}

pub(super) fn is_signature_member(name: &str) -> bool {
    name.split('/')
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case("_xmlsignatures"))
}
