use super::*;

pub(in crate::ooxml) fn unique_direct_child_by_attribute_mut<'a>(
    parent: &'a mut XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    attribute_name: &[u8],
    attribute_value: &str,
    error: &str,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let matches = parent
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace)
                    && has_local_name(child, local_name) =>
            {
                Some((index, attribute(child, attribute_name)))
            }
            _ => None,
        })
        .filter(|(_, value)| value.as_deref() == Some(attribute_value))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(OoxmlError::invalid(error));
    }
    match &mut parent.children[matches[0]] {
        XmlNode::Element(element) => Ok(element),
        XmlNode::Raw(_) => Err(OoxmlError::invalid(error)),
    }
}

pub(in crate::ooxml) fn optional_unique_direct_child_by_attribute_mut<'a>(
    parent: &'a mut XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    attribute_name: &[u8],
    attribute_value: &str,
    error: &str,
) -> Result<Option<&'a mut XmlElement>, OoxmlError> {
    let matches = parent
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace)
                    && has_local_name(child, local_name)
                    && attribute(child, attribute_name).as_deref() == Some(attribute_value) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [index] => match &mut parent.children[*index] {
            XmlNode::Element(element) => Ok(Some(element)),
            XmlNode::Raw(_) => Err(OoxmlError::invalid(error)),
        },
        _ => Err(OoxmlError::invalid(error)),
    }
}
