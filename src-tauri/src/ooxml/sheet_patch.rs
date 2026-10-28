use super::*;

mod lookup;
mod tokens;
pub(super) use lookup::*;
pub(super) use tokens::*;

pub(super) fn patch_sheet_xml(
    raw: &[u8],
    updates: &BTreeMap<(u32, u32), CellUpdate>,
    duplicate_column: Option<u32>,
) -> Result<(Vec<u8>, BTreeSet<String>), OoxmlError> {
    let mut document = parse_document(raw)?;
    let sheet_data = find_element_mut(&mut document.nodes, b"sheetData")
        .ok_or_else(|| OoxmlError::invalid("worksheet sheetData is missing"))?;
    let mut replaced_formulas = BTreeSet::new();
    for (&(row_number, column_number), update) in updates {
        if row_number == 0 || column_number == 0 || column_number > 16_384 {
            return Err(OoxmlError::invalid("worksheet cell reference is invalid"));
        }
        let style = nearest_style(sheet_data, row_number, column_number);
        let row = ensure_row(sheet_data, row_number)?;
        let cell = ensure_cell(row, row_number, column_number, style.as_deref())?;
        if !matches!(update, CellUpdate::Formula { .. }) && child_element(cell, b"f").is_some() {
            replaced_formulas.insert(cell_reference(row_number, column_number));
        }
        write_update(cell, update);
    }
    if let Some(column) = duplicate_column {
        patch_duplicate_validation(&mut document.nodes, column)?;
    }
    Ok((serialize_document(&document)?, replaced_formulas))
}

pub(super) fn patch_sheet_xml_guarded(
    raw: &[u8],
    updates: &BTreeMap<(u32, u32), GuardedCellUpdate>,
    status_values_only: bool,
    fonts: &FontTable,
) -> Result<Vec<u8>, OoxmlError> {
    let mut document = parse_document(raw)?;
    let root = document_root_mut_local(
        &mut document,
        b"worksheet",
        "guarded worksheet root is invalid",
    )?;
    let namespace = root
        .namespace
        .clone()
        .filter(|namespace| {
            matches!(
                namespace.as_slice(),
                value
                    if value == TRANSITIONAL_WORKBOOK_NAMESPACE.as_bytes()
                        || value == STRICT_WORKBOOK_NAMESPACE.as_bytes()
            )
        })
        .ok_or_else(|| OoxmlError::invalid("guarded worksheet namespace is invalid"))?;
    let prefix = root
        .start
        .name()
        .as_ref()
        .split(|byte| *byte == b':')
        .next()
        .filter(|_| root.start.name().as_ref().contains(&b':'))
        .map(|prefix| String::from_utf8_lossy(prefix).into_owned());
    let sheet_data = single_child_element_in_namespace_mut(
        root,
        b"sheetData",
        &namespace,
        "guarded worksheet sheetData is invalid",
    )?;

    for (&(row_number, column_number), update) in updates {
        let row = unique_direct_child_by_attribute_mut(
            sheet_data,
            b"row",
            &namespace,
            b"r",
            &row_number.to_string(),
            "guarded worksheet row is missing or ambiguous",
        )?;
        let coordinate = cell_reference(row_number, column_number);
        let cell = match optional_unique_direct_child_by_attribute_mut(
            row,
            b"c",
            &namespace,
            b"r",
            &coordinate,
            "guarded worksheet cell is ambiguous",
        )? {
            Some(cell) if !update.create_if_absent => cell,
            None if update.create_if_absent => ensure_guarded_cell(
                row,
                row_number,
                column_number,
                &namespace,
                prefix.as_deref(),
            )?,
            _ => {
                return Err(OoxmlError::invalid(
                    "guarded worksheet cell presence changed",
                ));
            }
        };
        patch_guarded_cell_value(
            cell,
            &namespace,
            prefix.as_deref(),
            &update.replacement,
            status_values_only,
            fonts,
        )?;
    }
    let bytes = serialize_document(&document)?;
    validate_worksheet_structure(&bytes)?;
    Ok(bytes)
}

pub(super) fn patch_guarded_cell_value(
    cell: &mut XmlElement,
    namespace: &[u8],
    prefix: Option<&str>,
    update: &CellUpdate,
    status_values_only: bool,
    fonts: &FontTable,
) -> Result<(), OoxmlError> {
    if let CellUpdate::Formula {
        formula,
        cached: None,
    } = update
        && !status_values_only
        && !formula.is_empty()
    {
        return patch_guarded_formula(cell, namespace, prefix, formula);
    }
    let (value, runs) = match update {
        CellUpdate::Text(value) => (value.as_str(), None),
        CellUpdate::Rich(runs) if !status_values_only && !runs.is_empty() => ("", Some(runs)),
        _ => return Err(OoxmlError::invalid("guarded replacement is invalid")),
    };
    if status_values_only && !matches!(value, "Signed" | "Not Signed") {
        return Err(OoxmlError::invalid("guarded replacement is invalid"));
    }
    if child_elements_in_namespace(cell, b"f", namespace)
        .next()
        .is_some()
    {
        return Err(OoxmlError::invalid(
            "guarded target cell contains a formula",
        ));
    }

    let mut representation_indices = cell
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace)
                    && (has_local_name(child, b"v") || has_local_name(child, b"is")) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if representation_indices.len() > 1 {
        return Err(OoxmlError::invalid(
            "guarded target cell value representation is ambiguous",
        ));
    }
    let insert_at = representation_indices.first().copied().unwrap_or_else(|| {
        cell.children
            .iter()
            .position(|node| {
                matches!(
                    node,
                    XmlNode::Element(child)
                        if child.namespace.as_deref() == Some(namespace)
                            && has_local_name(child, b"extLst")
                )
            })
            .unwrap_or(cell.children.len())
    });
    for index in representation_indices.drain(..).rev() {
        cell.children.remove(index);
    }

    set_unqualified_attribute(cell, b"t", "inlineStr");
    let qualify = |local: &str| match prefix {
        Some(prefix) => format!("{prefix}:{local}"),
        None => local.to_string(),
    };
    let inline = if let Some(runs) = runs {
        build_rich_inline(namespace, prefix, runs, &fonts.font_for_cell(cell))
    } else {
        let mut inline = XmlElement::new(&qualify("is"));
        inline.namespace = Some(namespace.to_vec());
        let mut text = XmlElement::new(&qualify("t"));
        text.namespace = Some(namespace.to_vec());
        if value.trim() != value {
            set_attribute(&mut text, b"xml:space", "preserve");
        }
        set_element_text(&mut text, value);
        inline.children.push(XmlNode::Element(text));
        inline
    };
    cell.children.insert(insert_at, XmlNode::Element(inline));
    cell.empty = false;
    Ok(())
}

/// Writes a plain formula (no cached value, so Excel computes it on open) into a cell
/// that currently holds neither a formula nor a value.
fn patch_guarded_formula(
    cell: &mut XmlElement,
    namespace: &[u8],
    prefix: Option<&str>,
    formula: &str,
) -> Result<(), OoxmlError> {
    // Only a truly empty cell qualifies: no formula, and a value representation that is
    // absent or an empty string.
    let holds_content = cell.children.iter().any(|node| {
        matches!(node, XmlNode::Element(child)
            if child.namespace.as_deref() == Some(namespace)
                && (has_local_name(child, b"f")
                    || ((has_local_name(child, b"v") || has_local_name(child, b"is"))
                        && !element_text(child).trim().is_empty())))
    });
    if holds_content {
        return Err(OoxmlError::invalid(
            "guarded formula target cell is not empty",
        ));
    }
    cell.children.retain(|node| {
        !matches!(node, XmlNode::Element(child)
            if child.namespace.as_deref() == Some(namespace)
                && (has_local_name(child, b"v") || has_local_name(child, b"is")))
    });
    remove_attribute(cell, b"t");
    let qualify = |local: &str| match prefix {
        Some(prefix) => format!("{prefix}:{local}"),
        None => local.to_string(),
    };
    let mut element = XmlElement::new(&qualify("f"));
    element.namespace = Some(namespace.to_vec());
    set_element_text(&mut element, formula);
    let insert_at = cell
        .children
        .iter()
        .position(|node| {
            matches!(node, XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace)
                    && has_local_name(child, b"extLst"))
        })
        .unwrap_or(cell.children.len());
    cell.children.insert(insert_at, XmlNode::Element(element));
    cell.empty = false;
    Ok(())
}

fn validate_exact_formula_output(
    cell: &XmlElement,
    namespace: &[u8],
    expected: &str,
) -> Result<(), OoxmlError> {
    let invalid = || OoxmlError::invalid("guarded output formula is not the approved formula");
    if bounded_unqualified_attribute(cell, b"t", 32, "guarded cell type")?.is_some()
        || child_elements_in_namespace(cell, b"v", namespace)
            .next()
            .is_some()
        || child_elements_in_namespace(cell, b"is", namespace)
            .next()
            .is_some()
    {
        return Err(invalid());
    }
    let formula = single_child_element_in_namespace(
        cell,
        b"f",
        namespace,
        "guarded output formula is invalid",
    )?;
    if !formula.attributes.is_empty() || element_text(formula) != expected {
        return Err(invalid());
    }
    Ok(())
}
