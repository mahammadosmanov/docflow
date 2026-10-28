use super::*;

pub(in crate::ooxml) fn parse_document(raw: &[u8]) -> Result<XmlDocument, OoxmlError> {
    parse_document_with_limits(raw, MAX_XML_DEPTH, MAX_XML_NODES)
}

pub(in crate::ooxml) fn parse_document_with_limits(
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

pub(in crate::ooxml) fn validate_raw_qualified_attribute_names(
    start: &BytesStart<'_>,
) -> Result<(), OoxmlError> {
    for attribute in start.attributes() {
        attribute.map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?;
    }
    Ok(())
}

pub(in crate::ooxml) fn expanded_namespace(
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

pub(in crate::ooxml) fn expanded_attributes(
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

pub(in crate::ooxml) fn record_xml_node(
    node_count: &mut usize,
    max_nodes: usize,
) -> Result<(), OoxmlError> {
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

pub(in crate::ooxml) fn push_node(
    roots: &mut Vec<XmlNode>,
    stack: &mut [XmlElement],
    node: XmlNode,
) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

pub(in crate::ooxml) fn serialize_document(document: &XmlDocument) -> Result<Vec<u8>, OoxmlError> {
    let mut writer = Writer::new(Vec::new());
    for node in &document.nodes {
        write_node(&mut writer, node)?;
    }
    Ok(writer.into_inner())
}

pub(in crate::ooxml) fn write_node(
    writer: &mut Writer<Vec<u8>>,
    node: &XmlNode,
) -> Result<(), OoxmlError> {
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
