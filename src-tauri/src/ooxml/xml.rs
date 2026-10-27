use super::*;

pub(super) fn patch_calc_chain(
    document: &mut XmlDocument,
    replaced_by_sheet: &[(u32, BTreeSet<String>)],
) -> Result<(), OoxmlError> {
    let root = document
        .nodes
        .iter_mut()
        .find_map(|node| match node {
            XmlNode::Element(element) if has_local_name(element, b"calcChain") => Some(element),
            _ => None,
        })
        .ok_or_else(|| OoxmlError::invalid("calcChain root is missing"))?;
    let removed = replaced_by_sheet
        .iter()
        .map(|(sheet, references)| (*sheet, references))
        .collect::<HashMap<_, _>>();
    let mut current_sheet = None;
    let mut last_kept_sheet = None;
    root.children.retain_mut(|node| {
        let XmlNode::Element(cell) = node else {
            return true;
        };
        if !has_local_name(cell, b"c") {
            return true;
        }
        if let Some(sheet) = attribute(cell, b"i").and_then(|value| value.parse::<u32>().ok()) {
            current_sheet = Some(sheet);
        }
        let reference = attribute(cell, b"r").unwrap_or_default();
        if current_sheet
            .and_then(|sheet| removed.get(&sheet))
            .is_some_and(|references| references.contains(&reference))
        {
            return false;
        }
        if let Some(sheet) = current_sheet
            && current_sheet != last_kept_sheet
            && attribute(cell, b"i").is_none()
        {
            set_attribute(cell, b"i", &sheet.to_string());
        }
        last_kept_sheet = current_sheet;
        true
    });
    Ok(())
}

pub(super) fn patch_duplicate_validation(
    nodes: &mut [XmlNode],
    column_number: u32,
) -> Result<(), OoxmlError> {
    let column = column_name(column_number)?;
    let Some(validations) = find_element_mut(nodes, b"dataValidations") else {
        return Ok(());
    };
    for validation in child_elements_mut(validations, b"dataValidation") {
        if attribute(validation, b"type").as_deref() != Some("custom") {
            continue;
        }
        let reference = attribute(validation, b"sqref");
        let Some(formula) = child_element_mut(validation, b"formula1") else {
            continue;
        };
        if !element_text(formula)
            .to_ascii_uppercase()
            .contains("COUNTIF")
        {
            continue;
        }
        let Some(reference) = reference else {
            continue;
        };
        let start = reference.split_whitespace().find_map(|range| {
            let (left, right) = range.split_once(':')?;
            let (left_column, left_row) = split_reference(left)?;
            let (right_column, _) = split_reference(right)?;
            (left_column.eq_ignore_ascii_case(&column)
                && right_column.eq_ignore_ascii_case(&column))
            .then_some(left_row)
        });
        let Some(start_row) = start else {
            continue;
        };
        let previous = start_row.saturating_sub(1).max(1);
        set_element_text(
            formula,
            &format!(
                "OR({column}{start_row}=\"\",COUNTIF(${column}${start_row}:{column}{start_row},{column}{start_row})=1,{column}{start_row}={column}{previous})"
            ),
        );
        break;
    }
    Ok(())
}

pub(super) fn nearest_style(
    sheet_data: &XmlElement,
    row_number: u32,
    column_number: u32,
) -> Option<String> {
    let mut best = None::<(u32, String)>;
    for row in child_elements(sheet_data, b"row") {
        let Some(current_row) = attribute(row, b"r").and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        for cell in child_elements(row, b"c") {
            let Some(reference) = attribute(cell, b"r") else {
                continue;
            };
            let Some((_, current_column)) = parse_reference(&reference) else {
                continue;
            };
            let Some(style) = attribute(cell, b"s") else {
                continue;
            };
            if current_column == column_number {
                let distance = current_row.abs_diff(row_number);
                if best
                    .as_ref()
                    .is_none_or(|(best_distance, _)| distance < *best_distance)
                {
                    best = Some((distance, style));
                }
            }
        }
    }
    best.map(|(_, style)| style)
}

pub(super) fn ensure_row(
    sheet_data: &mut XmlElement,
    row_number: u32,
) -> Result<&mut XmlElement, OoxmlError> {
    let mut insert_at = sheet_data.children.len();
    for (index, node) in sheet_data.children.iter().enumerate() {
        let XmlNode::Element(row) = node else {
            continue;
        };
        if !has_local_name(row, b"row") {
            continue;
        }
        let current = attribute(row, b"r")
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| OoxmlError::invalid("worksheet row identity is invalid"))?;
        if current == row_number {
            insert_at = index;
            break;
        }
        if current > row_number {
            insert_at = index;
            break;
        }
    }
    let is_match = sheet_data.children.get(insert_at).is_some_and(|node| {
        matches!(node, XmlNode::Element(row) if has_local_name(row, b"row") && attribute(row, b"r").as_deref() == Some(&row_number.to_string()))
    });
    if !is_match {
        let mut row = XmlElement::new("row");
        set_attribute(&mut row, b"r", &row_number.to_string());
        sheet_data.children.insert(insert_at, XmlNode::Element(row));
    }
    match &mut sheet_data.children[insert_at] {
        XmlNode::Element(row) => Ok(row),
        XmlNode::Raw(_) => Err(OoxmlError::invalid("worksheet row insertion failed")),
    }
}

pub(super) fn ensure_cell<'a>(
    row: &'a mut XmlElement,
    row_number: u32,
    column_number: u32,
    style: Option<&str>,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let reference = cell_reference(row_number, column_number);
    let mut insert_at = row.children.len();
    for (index, node) in row.children.iter().enumerate() {
        let XmlNode::Element(cell) = node else {
            continue;
        };
        if !has_local_name(cell, b"c") {
            continue;
        }
        let current = attribute(cell, b"r")
            .and_then(|value| parse_reference(&value))
            .ok_or_else(|| OoxmlError::invalid("worksheet cell reference is invalid"))?;
        if current.1 == column_number {
            insert_at = index;
            break;
        }
        if current.1 > column_number {
            insert_at = index;
            break;
        }
    }
    let is_match = row.children.get(insert_at).is_some_and(|node| {
        matches!(node, XmlNode::Element(cell) if has_local_name(cell, b"c") && attribute(cell, b"r").as_deref() == Some(reference.as_str()))
    });
    if !is_match {
        let mut cell = XmlElement::new("c");
        set_attribute(&mut cell, b"r", &reference);
        if let Some(style) = style {
            set_attribute(&mut cell, b"s", style);
        }
        row.children.insert(insert_at, XmlNode::Element(cell));
    }
    match &mut row.children[insert_at] {
        XmlNode::Element(cell) => Ok(cell),
        XmlNode::Raw(_) => Err(OoxmlError::invalid("worksheet cell insertion failed")),
    }
}

pub(super) fn ensure_guarded_cell<'a>(
    row: &'a mut XmlElement,
    row_number: u32,
    column_number: u32,
    namespace: &[u8],
    prefix: Option<&str>,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let coordinate = cell_reference(row_number, column_number);
    let cell = ensure_cell(row, row_number, column_number, None)?;
    cell.namespace = Some(namespace.to_vec());
    let qualified_name = prefix.map_or_else(|| "c".to_string(), |prefix| format!("{prefix}:c"));
    cell.start.set_name(qualified_name.as_bytes());
    cell.attributes = vec![ExpandedAttribute {
        namespace: None,
        local_name: b"r".to_vec(),
        value: coordinate.into_bytes(),
    }];
    Ok(cell)
}

pub(super) fn write_update(cell: &mut XmlElement, update: &CellUpdate) {
    let existing_formula = child_element(cell, b"f").cloned();
    cell.children.clear();
    remove_attribute(cell, b"t");
    match update {
        CellUpdate::Text(value) if value.is_empty() => {}
        CellUpdate::Rich(runs) => {
            // Unguarded writers only get the plain text; rich text is guarded-only.
            let value = rich_plain_text(runs);
            set_attribute(cell, b"t", "inlineStr");
            let mut inline = XmlElement::new("is");
            let mut text = XmlElement::new("t");
            set_element_text(&mut text, &value);
            inline.children.push(XmlNode::Element(text));
            cell.children.push(XmlNode::Element(inline));
        }
        CellUpdate::Text(value) => {
            set_attribute(cell, b"t", "inlineStr");
            let mut inline = XmlElement::new("is");
            let mut text = XmlElement::new("t");
            if value.trim() != value {
                set_attribute(&mut text, b"xml:space", "preserve");
            }
            set_element_text(&mut text, value);
            inline.children.push(XmlNode::Element(text));
            cell.children.push(XmlNode::Element(inline));
        }
        CellUpdate::Number(value) => {
            cell.children
                .push(XmlNode::Element(text_element("v", value)));
        }
        CellUpdate::Bool(value) => {
            set_attribute(cell, b"t", "b");
            cell.children.push(XmlNode::Element(text_element(
                "v",
                if *value { "1" } else { "0" },
            )));
        }
        CellUpdate::Formula { formula, cached } => {
            let mut formula_element = existing_formula.unwrap_or_else(|| XmlElement::new("f"));
            set_element_text(&mut formula_element, formula.trim_start_matches('='));
            cell.children.push(XmlNode::Element(formula_element));
            if let Some(cached) = cached {
                set_attribute(cell, b"t", "str");
                cell.children
                    .push(XmlNode::Element(text_element("v", cached)));
            }
        }
    }
}

pub(super) fn text_element(name: &str, value: &str) -> XmlElement {
    let mut element = XmlElement::new(name);
    set_element_text(&mut element, value);
    element
}

pub(super) fn parse_document(raw: &[u8]) -> Result<XmlDocument, OoxmlError> {
    parse_document_with_limits(raw, MAX_XML_DEPTH, MAX_XML_NODES)
}

pub(super) fn parse_document_with_limits(
    raw: &[u8],
    max_depth: usize,
    max_nodes: usize,
) -> Result<XmlDocument, OoxmlError> {
    if raw.len() as u64 > MAX_ENTRY_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "XML member exceeded the size limit",
        ));
    }
    let mut reader = NsReader::from_reader(raw);
    reader.config_mut().trim_text(false);
    let mut roots = Vec::<XmlNode>::new();
    let mut stack = Vec::<XmlElement>::new();
    let mut node_count = 0_usize;
    loop {
        match reader.read_event()? {
            Event::Start(start) => {
                record_xml_node(&mut node_count, max_nodes)?;
                if stack.len() >= max_depth {
                    return Err(OoxmlError::limit_exceeded(
                        "XML element depth exceeded the limit",
                    ));
                }
                validate_raw_qualified_attribute_names(&start)?;
                let namespace =
                    expanded_namespace(reader.resolver().resolve_element(start.name()).0)?;
                let attributes = expanded_attributes(reader.resolver(), &start)?;
                stack.push(XmlElement {
                    start: start.into_owned(),
                    namespace,
                    attributes,
                    children: Vec::new(),
                    empty: false,
                });
            }
            Event::Empty(start) => {
                record_xml_node(&mut node_count, max_nodes)?;
                if stack.len() >= max_depth {
                    return Err(OoxmlError::limit_exceeded(
                        "XML element depth exceeded the limit",
                    ));
                }
                validate_raw_qualified_attribute_names(&start)?;
                let namespace =
                    expanded_namespace(reader.resolver().resolve_element(start.name()).0)?;
                let attributes = expanded_attributes(reader.resolver(), &start)?;
                push_node(
                    &mut roots,
                    &mut stack,
                    XmlNode::Element(XmlElement {
                        start: start.into_owned(),
                        namespace,
                        attributes,
                        children: Vec::new(),
                        empty: true,
                    }),
                );
            }
            Event::End(end) => {
                let element = stack
                    .pop()
                    .ok_or_else(|| OoxmlError::invalid("XML element stack is invalid"))?;
                if element.start.name().as_ref() != end.name().as_ref() {
                    return Err(OoxmlError::invalid("XML closing tag is invalid"));
                }
                push_node(&mut roots, &mut stack, XmlNode::Element(element));
            }
            Event::Eof => break,
            event => {
                record_xml_node(&mut node_count, max_nodes)?;
                push_node(&mut roots, &mut stack, XmlNode::Raw(event.into_owned()));
            }
        }
    }
    if !stack.is_empty() || !roots.iter().any(|node| matches!(node, XmlNode::Element(_))) {
        return Err(OoxmlError::invalid("XML document is incomplete"));
    }
    Ok(XmlDocument { nodes: roots })
}

pub(super) fn validate_raw_qualified_attribute_names(
    start: &BytesStart<'_>,
) -> Result<(), OoxmlError> {
    for attribute in start.attributes() {
        attribute.map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?;
    }
    Ok(())
}

pub(super) fn expanded_namespace(
    namespace: ResolveResult<'_>,
) -> Result<Option<Vec<u8>>, OoxmlError> {
    match namespace {
        ResolveResult::Unbound => Ok(None),
        ResolveResult::Bound(namespace) => Ok(Some(namespace.as_ref().to_vec())),
        ResolveResult::Unknown(prefix) => {
            let mut unresolved = b"\0unresolved:".to_vec();
            unresolved.extend(prefix);
            Ok(Some(unresolved))
        }
    }
}

pub(super) fn expanded_attributes(
    resolver: &NamespaceResolver,
    start: &BytesStart<'_>,
) -> Result<Vec<ExpandedAttribute>, OoxmlError> {
    start
        .attributes()
        .with_checks(false)
        .map(|attribute| {
            let attribute =
                attribute.map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?;
            let (namespace, local_name) = resolver.resolve_attribute(attribute.key);
            Ok(ExpandedAttribute {
                namespace: expanded_namespace(namespace)?,
                local_name: local_name.as_ref().to_vec(),
                value: attribute.value.into_owned(),
            })
        })
        .collect()
}

pub(super) fn record_xml_node(node_count: &mut usize, max_nodes: usize) -> Result<(), OoxmlError> {
    *node_count = node_count
        .checked_add(1)
        .ok_or_else(|| OoxmlError::limit_exceeded("XML node count exceeded the limit"))?;
    if *node_count > max_nodes {
        return Err(OoxmlError::limit_exceeded(
            "XML node count exceeded the limit",
        ));
    }
    Ok(())
}

pub(super) fn push_node(roots: &mut Vec<XmlNode>, stack: &mut [XmlElement], node: XmlNode) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

pub(super) fn serialize_document(document: &XmlDocument) -> Result<Vec<u8>, OoxmlError> {
    let mut writer = Writer::new(Vec::new());
    for node in &document.nodes {
        write_node(&mut writer, node)?;
    }
    Ok(writer.into_inner())
}

pub(super) fn write_node(writer: &mut Writer<Vec<u8>>, node: &XmlNode) -> Result<(), OoxmlError> {
    match node {
        XmlNode::Raw(event) => writer.write_event(event.clone())?,
        XmlNode::Element(element) if element.empty && element.children.is_empty() => {
            writer.write_event(Event::Empty(element.start.clone()))?;
        }
        XmlNode::Element(element) => {
            writer.write_event(Event::Start(element.start.clone()))?;
            for child in &element.children {
                write_node(writer, child)?;
            }
            let name = String::from_utf8_lossy(element.start.name().as_ref()).into_owned();
            writer.write_event(Event::End(BytesEnd::new(name)))?;
        }
    }
    Ok(())
}

impl XmlElement {
    pub(super) fn new(name: &str) -> Self {
        Self {
            start: BytesStart::new(name).into_owned(),
            namespace: None,
            attributes: Vec::new(),
            children: Vec::new(),
            empty: false,
        }
    }
}

pub(super) fn descendants(nodes: &[XmlNode]) -> impl Iterator<Item = &XmlElement> {
    nodes.iter().flat_map(|node| {
        let mut values = Vec::new();
        collect_descendants(node, &mut values);
        values
    })
}

pub(super) fn document_root_local<'a>(
    document: &'a XmlDocument,
    local_name: &[u8],
    error: &str,
) -> Result<&'a XmlElement, OoxmlError> {
    let mut roots = document.nodes.iter().filter_map(|node| match node {
        XmlNode::Element(element) => Some(element),
        XmlNode::Raw(_) => None,
    });
    let root = roots
        .next()
        .filter(|element| has_local_name(element, local_name))
        .ok_or_else(|| OoxmlError::invalid(error))?;
    if roots.next().is_some() {
        return Err(OoxmlError::invalid(error));
    }
    Ok(root)
}

pub(super) fn document_root_mut_local<'a>(
    document: &'a mut XmlDocument,
    local_name: &[u8],
    error: &str,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let roots = document
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(element) if has_local_name(element, local_name) => Some(index),
            XmlNode::Element(_) => Some(usize::MAX),
            XmlNode::Raw(_) => None,
        })
        .collect::<Vec<_>>();
    if roots.len() != 1 || roots[0] == usize::MAX {
        return Err(OoxmlError::invalid(error));
    }
    match &mut document.nodes[roots[0]] {
        XmlNode::Element(root) => Ok(root),
        XmlNode::Raw(_) => Err(OoxmlError::invalid(error)),
    }
}

pub(super) fn single_child_element_expanded<'a>(
    parent: &'a XmlElement,
    local_name: &'a [u8],
    namespaces: &'a [&'a str],
    error: &str,
) -> Result<&'a XmlElement, OoxmlError> {
    let mut matches = child_elements_expanded(parent, local_name, namespaces);
    let child = matches.next().ok_or_else(|| OoxmlError::invalid(error))?;
    if matches.next().is_some() {
        return Err(OoxmlError::invalid(error));
    }
    Ok(child)
}

pub(super) fn collect_descendants<'a>(node: &'a XmlNode, output: &mut Vec<&'a XmlElement>) {
    if let XmlNode::Element(element) = node {
        output.push(element);
        for child in &element.children {
            collect_descendants(child, output);
        }
    }
}

pub(super) fn find_element_mut<'a>(
    nodes: &'a mut [XmlNode],
    name: &[u8],
) -> Option<&'a mut XmlElement> {
    for node in nodes {
        let XmlNode::Element(element) = node else {
            continue;
        };
        if has_local_name(element, name) {
            return Some(element);
        }
        if let Some(found) = find_element_mut(&mut element.children, name) {
            return Some(found);
        }
    }
    None
}

pub(super) fn child_elements<'a>(
    element: &'a XmlElement,
    name: &'a [u8],
) -> impl Iterator<Item = &'a XmlElement> {
    element.children.iter().filter_map(move |node| match node {
        XmlNode::Element(child) if has_local_name(child, name) => Some(child),
        _ => None,
    })
}

pub(super) fn child_elements_expanded<'a>(
    element: &'a XmlElement,
    local_name: &'a [u8],
    namespaces: &'a [&'a str],
) -> impl Iterator<Item = &'a XmlElement> {
    element.children.iter().filter_map(move |node| match node {
        XmlNode::Element(child) if has_expanded_name(child, local_name, namespaces) => Some(child),
        _ => None,
    })
}

pub(super) fn child_elements_mut<'a>(
    element: &'a mut XmlElement,
    name: &'a [u8],
) -> impl Iterator<Item = &'a mut XmlElement> {
    element
        .children
        .iter_mut()
        .filter_map(move |node| match node {
            XmlNode::Element(child) if has_local_name(child, name) => Some(child),
            _ => None,
        })
}

pub(super) fn child_element<'a>(element: &'a XmlElement, name: &'a [u8]) -> Option<&'a XmlElement> {
    child_elements(element, name).next()
}

pub(super) fn child_element_mut<'a>(
    element: &'a mut XmlElement,
    name: &'a [u8],
) -> Option<&'a mut XmlElement> {
    child_elements_mut(element, name).next()
}

pub(super) fn has_local_name(element: &XmlElement, expected: &[u8]) -> bool {
    let qualified = element.start.name();
    qualified
        .as_ref()
        .rsplit(|byte| *byte == b':')
        .next()
        .is_some_and(|name| name == expected)
}

pub(super) fn has_expanded_name(
    element: &XmlElement,
    local_name: &[u8],
    namespaces: &[&str],
) -> bool {
    has_local_name(element, local_name)
        && element.namespace.as_deref().is_some_and(|namespace| {
            namespaces
                .iter()
                .any(|expected| namespace == expected.as_bytes())
        })
}

pub(super) fn attribute(element: &XmlElement, local: &[u8]) -> Option<String> {
    element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .find(|attribute| {
            attribute
                .key
                .as_ref()
                .rsplit(|byte| *byte == b':')
                .next()
                .is_some_and(|name| name == local)
        })
        .map(|attribute| String::from_utf8_lossy(attribute.value.as_ref()).into_owned())
}

pub(super) fn bounded_unqualified_attribute(
    element: &XmlElement,
    local_name: &[u8],
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    bounded_expanded_attribute(
        element,
        local_name,
        |namespace| namespace.is_none(),
        max_bytes,
        field,
    )
}

pub(super) fn bounded_office_relationship_attribute(
    element: &XmlElement,
    local_name: &[u8],
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    bounded_expanded_attribute(
        element,
        local_name,
        |namespace| {
            namespace.is_some_and(|namespace| {
                matches!(
                    namespace,
                    value
                        if value == TRANSITIONAL_RELATIONSHIP_NAMESPACE.as_bytes()
                            || value == STRICT_RELATIONSHIP_NAMESPACE.as_bytes()
                )
            })
        },
        max_bytes,
        field,
    )
}

/// Checks the raw and unescaped byte lengths of an attribute value against `max_bytes`.
fn decode_bounded_value(raw: &[u8], max_bytes: usize, field: &str) -> Result<String, OoxmlError> {
    if raw.len() > max_bytes {
        return Err(OoxmlError::limit_exceeded(format!(
            "{field} exceeded the size limit"
        )));
    }
    let encoded =
        std::str::from_utf8(raw).map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
    let decoded =
        unescape(encoded).map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
    if decoded.len() > max_bytes {
        return Err(OoxmlError::limit_exceeded(format!(
            "{field} exceeded the size limit"
        )));
    }
    Ok(decoded.into_owned())
}

pub(super) fn bounded_expanded_attribute(
    element: &XmlElement,
    local_name: &[u8],
    namespace_matches: impl Fn(Option<&[u8]>) -> bool,
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    let mut value = None;
    for attribute in &element.attributes {
        if attribute.local_name != local_name || !namespace_matches(attribute.namespace.as_deref())
        {
            continue;
        }
        if value.is_some() {
            return Err(OoxmlError::invalid(format!("{field} is ambiguous")));
        }
        value = Some(decode_bounded_value(
            attribute.value.as_slice(),
            max_bytes,
            field,
        )?);
    }
    Ok(value)
}

pub(super) fn bounded_qualified_attribute(
    element: &XmlElement,
    qualified_name: &[u8],
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    let mut value = None;
    for attribute in element.start.attributes() {
        let attribute =
            attribute.map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        if attribute.key.as_ref() != qualified_name {
            continue;
        }
        if value.is_some() {
            return Err(OoxmlError::invalid(format!("{field} is ambiguous")));
        }
        value = Some(decode_bounded_value(
            attribute.value.as_ref(),
            max_bytes,
            field,
        )?);
    }
    Ok(value)
}

pub(super) fn set_attribute(element: &mut XmlElement, local: &[u8], value: &str) {
    let mut attributes = element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .map(|attribute| {
            (
                attribute.key.as_ref().to_vec(),
                attribute.value.as_ref().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    if let Some(attribute) = attributes.iter_mut().find(|attribute| {
        attribute
            .0
            .rsplit(|byte| *byte == b':')
            .next()
            .is_some_and(|name| name == local)
    }) {
        attribute.1 = value.as_bytes().to_vec();
    } else {
        attributes.push((local.to_vec(), value.as_bytes().to_vec()));
    }
    element.start.clear_attributes();
    for (key, value) in attributes {
        element
            .start
            .push_attribute((key.as_slice(), value.as_slice()));
    }
}

pub(super) fn set_unqualified_attribute(element: &mut XmlElement, name: &[u8], value: &str) {
    let mut attributes = element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .map(|attribute| {
            (
                attribute.key.as_ref().to_vec(),
                attribute.value.as_ref().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    if let Some(attribute) = attributes
        .iter_mut()
        .find(|attribute| attribute.0.as_slice() == name)
    {
        attribute.1 = value.as_bytes().to_vec();
    } else {
        attributes.push((name.to_vec(), value.as_bytes().to_vec()));
    }
    element.start.clear_attributes();
    for (key, value) in attributes {
        element
            .start
            .push_attribute((key.as_slice(), value.as_slice()));
    }
}

pub(super) fn remove_attribute(element: &mut XmlElement, local: &[u8]) {
    let attributes = element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .map(|attribute| {
            (
                attribute.key.as_ref().to_vec(),
                attribute.value.as_ref().to_vec(),
            )
        })
        .filter(|attribute| {
            attribute
                .0
                .rsplit(|byte| *byte == b':')
                .next()
                .is_none_or(|name| name != local)
        })
        .collect::<Vec<_>>();
    element.start.clear_attributes();
    for (key, value) in attributes {
        element
            .start
            .push_attribute((key.as_slice(), value.as_slice()));
    }
}

pub(super) fn element_text(element: &XmlElement) -> String {
    let mut output = String::new();
    collect_text(element, &mut output);
    output
}

pub(super) fn collect_text(element: &XmlElement, output: &mut String) {
    for node in &element.children {
        match node {
            XmlNode::Raw(Event::Text(text)) => {
                if let Ok(value) = text.decode() {
                    output.push_str(&decode_entities(&value));
                }
            }
            XmlNode::Raw(Event::GeneralRef(reference)) => {
                if let Ok(value) = reference.decode() {
                    output.push_str(&decode_entities(&format!("&{value};")));
                }
            }
            XmlNode::Raw(Event::CData(data)) => {
                if let Ok(value) = data.decode() {
                    output.push_str(&value);
                }
            }
            XmlNode::Element(child) => collect_text(child, output),
            XmlNode::Raw(_) => {}
        }
    }
}

pub(super) fn decode_entities(value: &str) -> String {
    unescape(value).map_or_else(|_| value.to_string(), std::borrow::Cow::into_owned)
}

pub(super) fn set_element_text(element: &mut XmlElement, value: &str) {
    element.children.clear();
    if !value.is_empty() {
        element.children.push(XmlNode::Raw(Event::Text(
            BytesText::new(value).into_owned(),
        )));
    }
    element.empty = value.is_empty();
}

pub(super) fn cell_reference(row: u32, column: u32) -> String {
    format!("{}{}", column_name(column).unwrap_or_default(), row)
}

pub(super) fn column_name(mut number: u32) -> Result<String, OoxmlError> {
    if number == 0 || number > 16_384 {
        return Err(OoxmlError::invalid("worksheet column is invalid"));
    }
    let mut output = String::new();
    while number > 0 {
        let remainder = (number - 1) % 26;
        output.insert(0, char::from(b'A' + remainder as u8));
        number = (number - 1) / 26;
    }
    Ok(output)
}

pub(super) fn split_reference(reference: &str) -> Option<(&str, u32)> {
    let split = reference.find(|character: char| character.is_ascii_digit())?;
    let row = reference[split..].parse::<u32>().ok()?;
    Some((&reference[..split], row))
}

pub(super) fn parse_reference(reference: &str) -> Option<(u32, u32)> {
    let (column, row) = split_reference(reference)?;
    let mut column_number = 0_u32;
    for byte in column.bytes() {
        if !byte.is_ascii_alphabetic() {
            return None;
        }
        column_number = column_number
            .checked_mul(26)?
            .checked_add(u32::from(byte.to_ascii_uppercase() - b'A' + 1))?;
    }
    (column_number > 0).then_some((row, column_number))
}
