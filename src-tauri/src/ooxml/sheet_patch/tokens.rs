use super::*;

pub(in crate::ooxml) fn guarded_sheet_tokens(
    raw: &[u8],
    updates: &BTreeMap<(u32, u32), GuardedCellUpdate>,
    output: bool,
    fonts: &FontTable,
) -> Result<Vec<CanonicalXmlEvent>, OoxmlError> {
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
            Some(cell) if !update.create_if_absent || output => cell,
            None if update.create_if_absent && !output => ensure_guarded_cell(
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
        normalize_guarded_cell_for_tokens(
            cell,
            &namespace,
            prefix.as_deref(),
            &coordinate,
            update,
            output,
            fonts,
        )?;
    }

    let mut tokens = Vec::new();
    canonical_xml_tokens(&document.nodes, &mut tokens)?;
    Ok(tokens)
}

pub(in crate::ooxml) fn normalize_guarded_cell_for_tokens(
    cell: &mut XmlElement,
    namespace: &[u8],
    prefix: Option<&str>,
    coordinate: &str,
    update: &GuardedCellUpdate,
    output: bool,
    fonts: &FontTable,
) -> Result<(), OoxmlError> {
    let replacement = match &update.replacement {
        CellUpdate::Text(replacement) => replacement.clone(),
        CellUpdate::Rich(runs) if !runs.is_empty() => format!("rich:{}", rich_marker(runs)),
        CellUpdate::Formula {
            formula,
            cached: None,
        } if !formula.is_empty() => format!("formula:{formula}"),
        _ => return Err(OoxmlError::invalid("guarded replacement is invalid")),
    };
    let formula_update = matches!(update.replacement, CellUpdate::Formula { .. });
    if output {
        match &update.replacement {
            CellUpdate::Rich(runs) => {
                validate_exact_rich_output(cell, namespace, prefix, runs, fonts)?;
            }
            CellUpdate::Formula { formula, .. } => {
                validate_exact_formula_output(cell, namespace, formula)?;
            }
            _ => validate_exact_guarded_output_value(cell, namespace, &replacement)?,
        }
    } else if child_elements_in_namespace(cell, b"f", namespace)
        .next()
        .is_some()
    {
        return Err(OoxmlError::invalid(
            "guarded source cell contains a formula",
        ));
    }
    if formula_update {
        cell.children.retain(|node| {
            !matches!(node, XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace) && has_local_name(child, b"f"))
        });
    }

    cell.attributes.retain(|attribute| {
        !(attribute.namespace.is_none() && attribute.local_name.as_slice() == b"t")
    });
    let representation_indices = cell
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
            "guarded cell value representation is ambiguous",
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
    for index in representation_indices.into_iter().rev() {
        cell.children.remove(index);
    }
    let marker = format!("docflow-guarded:{coordinate}:{replacement}");
    cell.children.insert(
        insert_at,
        XmlNode::Raw(Event::Text(BytesText::new(&marker).into_owned())),
    );
    cell.empty = false;
    Ok(())
}

pub(in crate::ooxml) fn validate_exact_guarded_output_value(
    cell: &XmlElement,
    namespace: &[u8],
    expected: &str,
) -> Result<(), OoxmlError> {
    if bounded_unqualified_attribute(cell, b"t", 32, "guarded cell type")?.as_deref()
        != Some("inlineStr")
    {
        return Err(OoxmlError::invalid("guarded output cell type is invalid"));
    }
    if child_elements_in_namespace(cell, b"f", namespace)
        .next()
        .is_some()
        || child_elements_in_namespace(cell, b"v", namespace)
            .next()
            .is_some()
    {
        return Err(OoxmlError::invalid(
            "guarded output value representation is invalid",
        ));
    }
    let inline = single_child_element_in_namespace(
        cell,
        b"is",
        namespace,
        "guarded output inline string is invalid",
    )?;
    if !inline.attributes.is_empty() || inline.children.len() != 1 {
        return Err(OoxmlError::invalid(
            "guarded output inline string is invalid",
        ));
    }
    let XmlNode::Element(text) = &inline.children[0] else {
        return Err(OoxmlError::invalid(
            "guarded output inline string is invalid",
        ));
    };
    if text.namespace.as_deref() != Some(namespace) || !has_local_name(text, b"t") {
        return Err(OoxmlError::invalid(
            "guarded output inline string is invalid",
        ));
    }
    let allows_space = expected.trim() != expected;
    let attributes_are_exact = if allows_space {
        text.attributes.len() == 1
            && text.attributes[0].namespace.as_deref()
                == Some(b"http://www.w3.org/XML/1998/namespace")
            && text.attributes[0].local_name.as_slice() == b"space"
            && text.attributes[0].value.as_slice() == b"preserve"
    } else {
        text.attributes.is_empty()
    };
    if !attributes_are_exact {
        return Err(OoxmlError::invalid(
            "guarded output inline string attributes are invalid",
        ));
    }
    let decoded = match text.children.as_slice() {
        [] if expected.is_empty() => String::new(),
        [XmlNode::Raw(Event::Text(value))] => value
            .decode()
            .map_err(|_| OoxmlError::invalid("guarded output inline string text is invalid"))?
            .into_owned(),
        _ => {
            return Err(OoxmlError::invalid(
                "guarded output inline string text is invalid",
            ));
        }
    };
    if decode_entities(&decoded) != expected {
        return Err(OoxmlError::invalid(
            "guarded output inline string text is invalid",
        ));
    }
    Ok(())
}

pub(in crate::ooxml) fn canonical_xml_tokens(
    nodes: &[XmlNode],
    output: &mut Vec<CanonicalXmlEvent>,
) -> Result<(), OoxmlError> {
    for node in nodes {
        match node {
            XmlNode::Element(element) => {
                let local_name = element
                    .start
                    .name()
                    .as_ref()
                    .rsplit(|byte| *byte == b':')
                    .next()
                    .unwrap_or_default()
                    .to_vec();
                let attributes = element
                    .attributes
                    .iter()
                    .filter(|attribute| {
                        attribute.local_name.as_slice() != b"xmlns"
                            && attribute.namespace.as_deref()
                                != Some(b"http://www.w3.org/2000/xmlns/")
                    })
                    .map(|attribute| {
                        let raw = std::str::from_utf8(&attribute.value)
                            .map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?;
                        let value = unescape(raw)
                            .map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?
                            .into_owned();
                        Ok((
                            attribute.namespace.clone(),
                            attribute.local_name.clone(),
                            value,
                        ))
                    })
                    .collect::<Result<Vec<_>, OoxmlError>>()?;
                if element.empty && element.children.is_empty() {
                    output.push(CanonicalXmlEvent::Empty {
                        namespace: element.namespace.clone(),
                        local_name,
                        attributes,
                    });
                } else {
                    output.push(CanonicalXmlEvent::Start {
                        namespace: element.namespace.clone(),
                        local_name: local_name.clone(),
                        attributes,
                    });
                    canonical_xml_tokens(&element.children, output)?;
                    output.push(CanonicalXmlEvent::End {
                        namespace: element.namespace.clone(),
                        local_name,
                    });
                }
            }
            XmlNode::Raw(event) => {
                let kind = match event {
                    Event::Text(_) => "text",
                    Event::CData(_) => "cdata",
                    Event::Comment(_) => "comment",
                    Event::Decl(_) => "decl",
                    Event::PI(_) => "pi",
                    Event::DocType(_) => "doctype",
                    Event::GeneralRef(_) => "general_ref",
                    Event::Eof => "eof",
                    Event::Start(_) | Event::End(_) | Event::Empty(_) => {
                        return Err(OoxmlError::invalid("XML event model is invalid"));
                    }
                };
                let value = match event {
                    Event::Text(text) => text
                        .decode()
                        .map(|value| decode_entities(&value).into_bytes())
                        .map_err(|_| OoxmlError::invalid("XML text is invalid"))?,
                    _ => event.as_ref().to_vec(),
                };
                output.push(CanonicalXmlEvent::Raw { kind, value });
            }
        }
    }
    Ok(())
}
