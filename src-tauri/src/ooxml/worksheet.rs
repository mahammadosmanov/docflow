use super::*;

pub(super) fn shared_strings(raw: &[u8]) -> Result<Vec<String>, OoxmlError> {
    let document = parse_document(raw)?;
    let root = document_root_local(&document, b"sst", "shared strings root is invalid")?;
    let namespace = root
        .namespace
        .as_deref()
        .filter(|namespace| {
            matches!(
                *namespace,
                value
                    if value == TRANSITIONAL_WORKBOOK_NAMESPACE.as_bytes()
                        || value == STRICT_WORKBOOK_NAMESPACE.as_bytes()
            )
        })
        .ok_or_else(|| OoxmlError::invalid("shared strings namespace is invalid"))?;
    let mut strings = Vec::new();
    let mut canonical_items = BTreeSet::new();
    for item in child_elements_in_namespace(root, b"si", namespace) {
        canonical_items.insert(item as *const XmlElement as usize);
        strings.push(canonical_inline_string(item, namespace)?);
    }
    for item in descendants(&document.nodes).filter(|element| has_local_name(element, b"si")) {
        if item.namespace.as_deref() != Some(namespace)
            || !canonical_items.contains(&(item as *const XmlElement as usize))
        {
            return Err(OoxmlError::invalid(
                "shared string is outside the canonical hierarchy",
            ));
        }
    }
    Ok(strings)
}

pub(super) fn worksheet_data(
    raw: &[u8],
    shared_strings: &[String],
    merge_budget: &mut MergeComparisonBudget,
) -> Result<WorksheetData, OoxmlError> {
    let document = parse_document(raw)?;
    let root = document_root_local(&document, b"worksheet", "worksheet root is invalid")?;
    let namespace = root
        .namespace
        .as_deref()
        .filter(|namespace| {
            matches!(
                *namespace,
                value
                    if value == TRANSITIONAL_WORKBOOK_NAMESPACE.as_bytes()
                        || value == STRICT_WORKBOOK_NAMESPACE.as_bytes()
            )
        })
        .ok_or_else(|| OoxmlError::invalid("worksheet namespace is invalid"))?;
    let sheet_data = single_child_element_in_namespace(
        root,
        b"sheetData",
        namespace,
        "worksheet sheetData is invalid",
    )?;
    let mut data = WorksheetData {
        has_sheet_protection: optional_single_child_element_in_namespace(
            root,
            b"sheetProtection",
            namespace,
            "worksheet protection is invalid",
        )?
        .is_some(),
        merged_ranges: worksheet_merged_ranges(root, namespace, merge_budget)?,
        ..WorksheetData::default()
    };

    for columns in child_elements_in_namespace(root, b"cols", namespace) {
        for column in child_elements_in_namespace(columns, b"col", namespace) {
            let first = bounded_unqualified_attribute(
                column,
                b"min",
                MAX_U32_ATTRIBUTE_BYTES,
                "worksheet column minimum",
            )?
            .and_then(|value| value.parse::<u32>().ok());
            let last = bounded_unqualified_attribute(
                column,
                b"max",
                MAX_U32_ATTRIBUTE_BYTES,
                "worksheet column maximum",
            )?
            .and_then(|value| value.parse::<u32>().ok());
            let width =
                bounded_unqualified_attribute(column, b"width", 64, "worksheet column width")?
                    .and_then(|value| value.parse::<f64>().ok());
            let (Some(first), Some(last), Some(width)) = (first, last, width) else {
                continue;
            };
            if first == 0
                || first > last
                || last > MAX_WORKSHEET_COLUMNS
                || !width.is_finite()
                || width < 0.0
            {
                return Err(OoxmlError::invalid("worksheet column is invalid"));
            }
            for column in first..=last {
                data.column_widths.insert(column, width);
            }
        }
    }

    let mut canonical_cells = BTreeSet::new();
    let mut row_numbers = BTreeSet::new();
    let mut coordinates = BTreeSet::new();
    for row in child_elements_in_namespace(sheet_data, b"row", namespace) {
        let row_number = bounded_unqualified_attribute(
            row,
            b"r",
            MAX_U32_ATTRIBUTE_BYTES,
            "worksheet row identity",
        )?
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|row| (1..=MAX_WORKSHEET_ROWS).contains(row))
        .ok_or_else(|| OoxmlError::invalid("worksheet row identity is invalid"))?;
        if !row_numbers.insert(row_number) {
            return Err(OoxmlError::invalid("duplicate worksheet row identity"));
        }
        data.max_row = data.max_row.max(row_number);
        for cell in child_elements_in_namespace(row, b"c", namespace) {
            canonical_cells.insert(cell as *const XmlElement as usize);
            reject_noncanonical_cell_value_shadows(cell, namespace)?;
            let reference =
                bounded_unqualified_attribute(cell, b"r", 32, "worksheet cell reference")?
                    .and_then(|value| parse_reference(&value))
                    .filter(|(cell_row, column)| {
                        *cell_row == row_number
                            && (1..=MAX_WORKSHEET_ROWS).contains(cell_row)
                            && (1..=MAX_WORKSHEET_COLUMNS).contains(column)
                    })
                    .ok_or_else(|| OoxmlError::invalid("worksheet cell reference is invalid"))?;
            if !coordinates.insert(reference) {
                return Err(OoxmlError::invalid("duplicate worksheet cell reference"));
            }
            let cell_type = bounded_unqualified_attribute(cell, b"t", 32, "worksheet cell type")?
                .unwrap_or_default();
            let raw_value = optional_single_child_element_in_namespace(
                cell,
                b"v",
                namespace,
                "worksheet cell value is invalid",
            )?
            .map(element_text)
            .unwrap_or_default();
            let inline = optional_single_child_element_in_namespace(
                cell,
                b"is",
                namespace,
                "worksheet inline string is invalid",
            )?;
            let value = match cell_type.as_str() {
                "inlineStr" => inline
                    .map(|inline| canonical_inline_string(inline, namespace))
                    .transpose()?
                    .unwrap_or_default(),
                "s" => {
                    if raw_value.is_empty() {
                        String::new()
                    } else {
                        raw_value
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| shared_strings.get(index))
                            .cloned()
                            .ok_or_else(|| OoxmlError::invalid("shared string index is invalid"))?
                    }
                }
                _ => raw_value,
            };
            let formula = optional_single_child_element_in_namespace(
                cell,
                b"f",
                namespace,
                "worksheet cell formula is invalid",
            )?
            .map(element_text);
            let style = bounded_unqualified_attribute(
                cell,
                b"s",
                MAX_U32_ATTRIBUTE_BYTES,
                "worksheet cell style",
            )?;
            data.max_row = data.max_row.max(reference.0);
            data.max_column = data.max_column.max(reference.1);
            data.cells.insert(
                reference,
                CellData {
                    value,
                    formula,
                    style,
                },
            );
        }
    }

    for element in descendants(&document.nodes).filter(|element| has_local_name(element, b"c")) {
        if element.namespace.as_deref() == Some(namespace) {
            if !canonical_cells.contains(&(element as *const XmlElement as usize)) {
                return Err(OoxmlError::invalid(
                    "worksheet cell is outside the canonical hierarchy",
                ));
            }
        } else if bounded_unqualified_attribute(
            element,
            b"r",
            32,
            "foreign worksheet cell reference",
        )?
        .and_then(|value| parse_reference(&value))
        .is_some()
        {
            return Err(OoxmlError::invalid(
                "foreign namespace worksheet cell shadow is invalid",
            ));
        }
    }
    Ok(data)
}

pub(super) fn child_elements_in_namespace<'a>(
    element: &'a XmlElement,
    local_name: &'a [u8],
    namespace: &'a [u8],
) -> impl Iterator<Item = &'a XmlElement> {
    element.children.iter().filter_map(move |node| match node {
        XmlNode::Element(child)
            if has_local_name(child, local_name)
                && child.namespace.as_deref() == Some(namespace) =>
        {
            Some(child)
        }
        _ => None,
    })
}

pub(super) fn optional_single_child_element_in_namespace<'a>(
    parent: &'a XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    error: &str,
) -> Result<Option<&'a XmlElement>, OoxmlError> {
    let mut found = None;
    for node in &parent.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        if !has_local_name(child, local_name) {
            continue;
        }
        if child.namespace.as_deref() != Some(namespace) || found.is_some() {
            return Err(OoxmlError::invalid(error));
        }
        found = Some(child);
    }
    Ok(found)
}

pub(super) fn single_child_element_in_namespace<'a>(
    parent: &'a XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    error: &str,
) -> Result<&'a XmlElement, OoxmlError> {
    optional_single_child_element_in_namespace(parent, local_name, namespace, error)?
        .ok_or_else(|| OoxmlError::invalid(error))
}

pub(super) fn single_child_element_in_namespace_mut<'a>(
    parent: &'a mut XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    error: &str,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let matches = parent
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child) if has_local_name(child, local_name) => {
                if child.namespace.as_deref() == Some(namespace) {
                    Some(Ok(index))
                } else {
                    Some(Err(OoxmlError::invalid(error)))
                }
            }
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;
    if matches.len() != 1 {
        return Err(OoxmlError::invalid(error));
    }
    match &mut parent.children[matches[0]] {
        XmlNode::Element(element) => Ok(element),
        XmlNode::Raw(_) => Err(OoxmlError::invalid(error)),
    }
}

pub(super) fn canonical_inline_string(
    element: &XmlElement,
    namespace: &[u8],
) -> Result<String, OoxmlError> {
    let mut text = String::new();
    for node in &element.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        require_namespace(
            child,
            namespace,
            "worksheet inline string namespace is invalid",
        )?;
        if has_local_name(child, b"t") {
            text.push_str(&element_text(child));
        } else if has_local_name(child, b"r") {
            append_canonical_rich_text(child, namespace, &mut text)?;
        } else if has_local_name(child, b"rPh") || has_local_name(child, b"phoneticPr") {
            validate_descendant_namespace(child, namespace)?;
        } else {
            return Err(OoxmlError::invalid(
                "worksheet inline string hierarchy is invalid",
            ));
        }
    }
    Ok(text)
}

pub(super) fn append_canonical_rich_text(
    element: &XmlElement,
    namespace: &[u8],
    output: &mut String,
) -> Result<(), OoxmlError> {
    for node in &element.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        require_namespace(
            child,
            namespace,
            "worksheet inline string namespace is invalid",
        )?;
        if has_local_name(child, b"t") {
            output.push_str(&element_text(child));
        } else if has_local_name(child, b"rPr") {
            validate_descendant_namespace(child, namespace)?;
        } else {
            return Err(OoxmlError::invalid(
                "worksheet rich text hierarchy is invalid",
            ));
        }
    }
    Ok(())
}

pub(super) fn require_namespace(
    element: &XmlElement,
    namespace: &[u8],
    error: &str,
) -> Result<(), OoxmlError> {
    if element.namespace.as_deref() == Some(namespace) {
        Ok(())
    } else {
        Err(OoxmlError::invalid(error))
    }
}

pub(super) fn validate_descendant_namespace(
    element: &XmlElement,
    namespace: &[u8],
) -> Result<(), OoxmlError> {
    for node in &element.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        require_namespace(child, namespace, "worksheet rich text namespace is invalid")?;
        validate_descendant_namespace(child, namespace)?;
    }
    Ok(())
}

pub(super) fn reject_noncanonical_cell_value_shadows(
    cell: &XmlElement,
    namespace: &[u8],
) -> Result<(), OoxmlError> {
    for node in &cell.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        if [b"f".as_slice(), b"v".as_slice(), b"is".as_slice()]
            .iter()
            .any(|name| has_local_name(child, name))
        {
            require_namespace(
                child,
                namespace,
                "worksheet cell value namespace is invalid",
            )?;
        }
        reject_nested_cell_value_shadow(child)?;
    }
    Ok(())
}

pub(super) fn reject_nested_cell_value_shadow(element: &XmlElement) -> Result<(), OoxmlError> {
    for node in &element.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        if [b"f".as_slice(), b"v".as_slice(), b"is".as_slice()]
            .iter()
            .any(|name| has_local_name(child, name))
        {
            return Err(OoxmlError::invalid(
                "worksheet cell value is outside the canonical hierarchy",
            ));
        }
        reject_nested_cell_value_shadow(child)?;
    }
    Ok(())
}

pub(super) fn worksheet_merged_ranges(
    root: &XmlElement,
    namespace: &[u8],
    merge_budget: &mut MergeComparisonBudget,
) -> Result<Vec<CellRange>, OoxmlError> {
    let Some(merge_cells) = optional_single_child_element_in_namespace(
        root,
        b"mergeCells",
        namespace,
        "worksheet merge collection is invalid",
    )?
    else {
        return Ok(Vec::new());
    };
    let mut ranges = Vec::<CellRange>::new();
    for node in &merge_cells.children {
        let XmlNode::Element(element) = node else {
            continue;
        };
        if !has_local_name(element, b"mergeCell") {
            continue;
        }
        if element.namespace.as_deref() != Some(namespace) {
            return Err(OoxmlError::invalid("worksheet merge range is invalid"));
        }
        if ranges.len() >= MAX_MERGED_RANGES {
            return Err(OoxmlError::limit_exceeded(
                "worksheet merge range count exceeded the limit",
            ));
        }
        let reference =
            bounded_unqualified_attribute(element, b"ref", 64, "worksheet merge range")?
                .ok_or_else(|| OoxmlError::invalid("worksheet merge range is invalid"))?;
        let (first, last) = reference
            .split_once(':')
            .and_then(|(first, last)| Some((parse_reference(first)?, parse_reference(last)?)))
            .ok_or_else(|| OoxmlError::invalid("worksheet merge range is invalid"))?;
        let range = CellRange {
            first_row: first.0,
            first_column: first.1,
            last_row: last.0,
            last_column: last.1,
        };
        if range.first_row == 0
            || range.first_column == 0
            || range.first_row > range.last_row
            || range.first_column > range.last_column
            || range.last_row > MAX_WORKSHEET_ROWS
            || range.last_column > MAX_WORKSHEET_COLUMNS
        {
            return Err(OoxmlError::invalid("worksheet merge range is invalid"));
        }
        if ranges.contains(&range) {
            continue;
        }
        for existing in &ranges {
            merge_budget.consume()?;
            if existing.first_row <= range.last_row
                && range.first_row <= existing.last_row
                && existing.first_column <= range.last_column
                && range.first_column <= existing.last_column
            {
                return Err(OoxmlError::invalid("worksheet merge range is invalid"));
            }
        }
        ranges.push(range);
    }
    if let Some(declared) = bounded_unqualified_attribute(
        merge_cells,
        b"count",
        MAX_U32_ATTRIBUTE_BYTES,
        "worksheet merge count",
    )? {
        let declared = declared
            .parse::<usize>()
            .map_err(|_| OoxmlError::invalid("worksheet merge count is invalid"))?;
        if declared > MAX_MERGED_RANGES {
            return Err(OoxmlError::limit_exceeded(
                "worksheet merge range count exceeded the limit",
            ));
        }
    }
    Ok(ranges)
}
