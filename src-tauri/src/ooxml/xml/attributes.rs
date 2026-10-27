use super::*;

pub(in crate::ooxml) fn attribute(element: &XmlElement, local: &[u8]) -> Option<String> {
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

pub(in crate::ooxml) fn bounded_unqualified_attribute(
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

pub(in crate::ooxml) fn bounded_office_relationship_attribute(
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

pub(in crate::ooxml) fn bounded_expanded_attribute(
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

pub(in crate::ooxml) fn bounded_qualified_attribute(
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

pub(in crate::ooxml) fn set_attribute(element: &mut XmlElement, local: &[u8], value: &str) {
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

pub(in crate::ooxml) fn set_unqualified_attribute(
    element: &mut XmlElement,
    name: &[u8],
    value: &str,
) {
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

pub(in crate::ooxml) fn remove_attribute(element: &mut XmlElement, local: &[u8]) {
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
