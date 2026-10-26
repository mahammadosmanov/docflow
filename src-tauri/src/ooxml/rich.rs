//! Rich-text cell writing for guarded updates: runs with an emphasized style
//! (bold, underline, theme accent 5) next to plain runs, in the cell's own font.

use super::*;

/// Font facts copied from the cell's style so a rich run looks like its neighbours.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct FontSpec {
    pub(super) size: Option<String>,
    pub(super) name: Option<String>,
    pub(super) family: Option<String>,
    pub(super) color: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct FontTable {
    fonts: Vec<FontSpec>,
    cell_xf_font: Vec<usize>,
}

impl FontTable {
    pub(super) fn parse(styles: &[u8]) -> Result<Self, OoxmlError> {
        let document = parse_document(styles)?;
        let mut table = Self::default();
        let elements = descendants(&document.nodes).collect::<Vec<_>>();
        if let Some(fonts) = elements
            .iter()
            .find(|element| has_local_name(element, b"fonts"))
        {
            for font in child_elements(fonts, b"font") {
                let value = |name: &[u8]| {
                    child_element(font, name).and_then(|child| attribute(child, b"val"))
                };
                let color = child_element(font, b"color")
                    .map(|child| {
                        child
                            .attributes
                            .iter()
                            .filter(|attribute| attribute.namespace.is_none())
                            .map(|attribute| {
                                (
                                    String::from_utf8_lossy(&attribute.local_name).into_owned(),
                                    String::from_utf8_lossy(&attribute.value).into_owned(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                table.fonts.push(FontSpec {
                    size: value(b"sz"),
                    name: value(b"name").or_else(|| value(b"rFont")),
                    family: value(b"family"),
                    color,
                });
            }
        }
        if let Some(xfs) = elements
            .iter()
            .find(|element| has_local_name(element, b"cellXfs"))
        {
            for xf in child_elements(xfs, b"xf") {
                table.cell_xf_font.push(
                    attribute(xf, b"fontId")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(0),
                );
            }
        }
        Ok(table)
    }

    pub(super) fn font_for_cell(&self, cell: &XmlElement) -> FontSpec {
        let index = attribute(cell, b"s")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        self.cell_xf_font
            .get(index)
            .and_then(|font| self.fonts.get(*font))
            .cloned()
            .unwrap_or_default()
    }
}

fn run_properties(
    qualify: &dyn Fn(&str) -> String,
    namespace: &[u8],
    font: &FontSpec,
    emphasized: bool,
) -> XmlElement {
    let mut properties = XmlElement::new(&qualify("rPr"));
    properties.namespace = Some(namespace.to_vec());
    let mut push = |name: &str, attributes: &[(&str, &str)]| {
        let mut element = XmlElement::new(&qualify(name));
        element.namespace = Some(namespace.to_vec());
        element.empty = true;
        for (key, value) in attributes {
            set_unqualified_attribute(&mut element, key.as_bytes(), value);
        }
        properties.children.push(XmlNode::Element(element));
    };
    if emphasized {
        push("b", &[]);
        push("u", &[]);
    }
    if let Some(size) = &font.size {
        push("sz", &[("val", size)]);
    }
    if emphasized {
        // Theme index 8 is Accent 5 (Plum).
        push("color", &[("theme", "8")]);
    } else if !font.color.is_empty() {
        let attributes = font
            .color
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        push("color", &attributes);
    }
    if let Some(name) = &font.name {
        push("rFont", &[("val", name)]);
    }
    if let Some(family) = &font.family {
        push("family", &[("val", family)]);
    }
    properties
}

/// The exact `<is>` element a rich update must produce.
pub(super) fn build_rich_inline(
    namespace: &[u8],
    prefix: Option<&str>,
    runs: &[RichRun],
    font: &FontSpec,
) -> XmlElement {
    let qualify = |local: &str| match prefix {
        Some(prefix) => format!("{prefix}:{local}"),
        None => local.to_string(),
    };
    let mut inline = XmlElement::new(&qualify("is"));
    inline.namespace = Some(namespace.to_vec());
    for run in runs {
        let mut element = XmlElement::new(&qualify("r"));
        element.namespace = Some(namespace.to_vec());
        element.children.push(XmlNode::Element(run_properties(
            &qualify,
            namespace,
            font,
            run.emphasized,
        )));
        let mut text = XmlElement::new(&qualify("t"));
        text.namespace = Some(namespace.to_vec());
        if run.text.trim() != run.text {
            set_attribute(&mut text, b"xml:space", "preserve");
        }
        set_element_text(&mut text, &run.text);
        element.children.push(XmlNode::Element(text));
        inline.children.push(XmlNode::Element(element));
    }
    inline
}

pub(super) fn rich_plain_text(runs: &[RichRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// Stable text used in the delta-verification marker for a rich replacement.
pub(super) fn rich_marker(runs: &[RichRun]) -> String {
    runs.iter()
        .map(|run| format!("{}:{}", if run.emphasized { "E" } else { "N" }, run.text))
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

pub(super) fn validate_exact_rich_output(
    cell: &XmlElement,
    namespace: &[u8],
    prefix: Option<&str>,
    runs: &[RichRun],
    fonts: &FontTable,
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
    let expected = build_rich_inline(namespace, prefix, runs, &fonts.font_for_cell(cell));
    let mut actual_tokens = Vec::new();
    canonical_xml_tokens(&[XmlNode::Element(inline.clone())], &mut actual_tokens)?;
    let expected_tokens = canonical_tokens_of_built(&expected, namespace, prefix)?;
    if actual_tokens == expected_tokens {
        Ok(())
    } else {
        Err(OoxmlError::invalid(
            "guarded output rich text is not the approved text",
        ))
    }
}

/// Built elements only carry raw start tags; serialize and re-parse so they compare
/// exactly like elements read from a package.
fn canonical_tokens_of_built(
    element: &XmlElement,
    namespace: &[u8],
    prefix: Option<&str>,
) -> Result<Vec<CanonicalXmlEvent>, OoxmlError> {
    let mut writer = quick_xml::Writer::new(Vec::new());
    write_node(&mut writer, &XmlNode::Element(element.clone()))?;
    let namespace = String::from_utf8_lossy(namespace);
    let declarations = match prefix {
        Some(prefix) => format!(r#"xmlns:{prefix}="{namespace}""#),
        None => format!(r#"xmlns="{namespace}""#),
    };
    let mut wrapped = format!("<w {declarations}>").into_bytes();
    wrapped.extend(writer.into_inner());
    wrapped.extend(b"</w>");
    let document = parse_document(&wrapped)?;
    let Some(XmlNode::Element(root)) = document.nodes.first() else {
        return Err(OoxmlError::invalid("rich text comparison is invalid"));
    };
    let mut tokens = Vec::new();
    canonical_xml_tokens(&root.children, &mut tokens)?;
    Ok(tokens)
}
