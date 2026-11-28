use std::io::{Cursor, Read, Write};

use quick_xml::{Reader, escape::unescape, events::Event};
use regex::Regex;
use std::sync::LazyLock;
use zip::{ZipArchive, ZipWriter};
pub(crate) struct SourceRecord {
    pub rfi: String,
    pub location: String,
    pub tag: String,
    pub concrete_class: String,
    pub supplier: String,
    pub batch_ticket: String,
    pub truck: String,
    pub quantity: String,
    pub total_quantity: String,
    pub batch_time: String,
    pub pour_date: String,
    pub start_time: String,
    pub end_time: String,
    pub plant_slump: String,
    pub in_situ_slump: String,
    pub plant_temp: String,
    pub in_situ_temp: String,
    pub samples: String,
    pub water_cement_ratio: Option<String>,
    pub measured_ambient: Vec<String>,
    pub drawing: String,
    pub additional_batches: Vec<ConcreteBatch>,
}
#[derive(Clone, Default)]
pub(crate) struct ConcreteBatch {
    pub tag: String,
    pub concrete_class: String,
    pub supplier: String,
    pub batch_ticket: String,
    pub truck: String,
    pub quantity: String,
    pub total_quantity: String,
    pub batch_time: String,
    pub start_time: String,
    pub end_time: String,
    pub plant_slump: String,
    pub in_situ_slump: String,
    pub plant_temp: String,
    pub in_situ_temp: String,
    pub samples: String,
}

pub(crate) struct GeneratedFile {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub measured_ambient: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentType {
    Civ010,
    Civ011,
    Civ013,
    Civ014,
}

impl DocumentType {
    fn expected_extension(self) -> &'static str {
        match self {
            Self::Civ010 | Self::Civ013 => "xlsx",
            Self::Civ011 | Self::Civ014 => "docx",
        }
    }
}

/// Generate one template-preserving OOXML package per source record.
/// XML text values are updated in package parts; other entry contents are retained.
pub(crate) fn generate(
    document_type: DocumentType,
    template: &[u8],
    records: &[SourceRecord],
) -> Result<Vec<GeneratedFile>, String> {
    if records.is_empty() {
        return Err("No concrete source records were provided".to_string());
    }

    const MAX_ENTRIES: usize = 4096;
    const MAX_ENTRY_BYTES: u64 = 128 * 1024 * 1024;
    const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
    if template.len() as u64 > crate::ooxml::MAX_PACKAGE_BYTES {
        return Err("Office template exceeds the supported package limit".to_string());
    }
    let mut archive = ZipArchive::new(Cursor::new(template))
        .map_err(|error| format!("Invalid Office package: {error}"))?;
    if archive.is_empty() || archive.len() > MAX_ENTRIES {
        return Err("Office package entry count is invalid".to_string());
    }
    let archive_comment = archive.comment().to_vec();
    let mut parts = Vec::with_capacity(archive.len());
    let mut total_uncompressed = 0_u64;
    let mut has_document_xml = false;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Invalid Office package entry: {error}"))?;
        let name = entry
            .name()
            .map_err(|error| format!("Invalid Office package entry name: {error}"))?
            .into_owned();
        let entry_size = entry.size();
        total_uncompressed = total_uncompressed
            .checked_add(entry_size)
            .ok_or_else(|| "Office package expands beyond its supported limit".to_string())?;
        if entry_size > MAX_ENTRY_BYTES || total_uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err("Office package expands beyond its supported limit".to_string());
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read Office package entry: {error}"))?;
        if name == "word/document.xml" || name.starts_with("xl/worksheets/") {
            has_document_xml |= name == "word/document.xml";
        }
        parts.push((name, bytes));
    }
    let valid_package = match document_type {
        DocumentType::Civ011 | DocumentType::Civ014 => has_document_xml,
        DocumentType::Civ010 => parts
            .iter()
            .any(|(name, _)| name == "xl/worksheets/sheet1.xml"),
        DocumentType::Civ013 => parts
            .iter()
            .any(|(name, _)| name.starts_with("xl/worksheets/") && name.ends_with(".xml")),
    };
    if !valid_package {
        return Err(format!(
            "Template is not a valid .{} Office template",
            document_type.expected_extension()
        ));
    }

    let sample_rfi = parts
        .iter()
        .filter(|(name, _)| match document_type {
            DocumentType::Civ011 | DocumentType::Civ014 => name == "word/document.xml",
            DocumentType::Civ010 | DocumentType::Civ013 => name.ends_with(".xml"),
        })
        .find_map(|(_, bytes)| rfi_anchor(&String::from_utf8_lossy(bytes)))
        .ok_or_else(|| "Template does not contain a recognizable CIV RFI number".to_string())?;
    let sample_location = location_anchor(&parts).ok_or_else(|| {
        "Template does not contain a location value adjacent to a location label".to_string()
    })?;

    let rfi_regex: &Regex = &CIV_RFI;
    let mut generated = Vec::with_capacity(records.len());
    for record in records {
        if record.rfi.trim().is_empty() || record.location.trim().is_empty() {
            return Err("A source record is missing its RFI number or location".to_string());
        }
        let replacements = if document_type == DocumentType::Civ010 {
            Vec::new()
        } else {
            vec![
                (sample_rfi.clone(), record.rfi.clone()),
                (sample_location.clone(), record.location.clone()),
            ]
        };
        let expected_ambient_count = match document_type {
            DocumentType::Civ010 => record.additional_batches.len() + 1,
            DocumentType::Civ011 => 1,
            _ => 0,
        };
        if record.measured_ambient.len() != expected_ambient_count {
            return Err(format!(
                "Measured ambient temperature is missing for {}",
                record.rfi
            ));
        }
        let mut output = ZipWriter::new(Cursor::new(Vec::new()));
        output
            .set_raw_comment(archive_comment.clone().into_boxed_slice())
            .map_err(|error| format!("Could not preserve Office package comment: {error}"))?;
        for (index, (name, original)) in parts.iter().enumerate() {
            let bytes = if document_type == DocumentType::Civ010 {
                original.clone()
            } else if name.ends_with(".xml") {
                let xml = String::from_utf8_lossy(original);
                let rendered = replace_xml_text(&xml, &replacements, rfi_regex);
                if document_type == DocumentType::Civ011 && name == "word/document.xml" {
                    replace_civ011_ambient(rendered, &record.measured_ambient[0])?.into_bytes()
                } else {
                    rendered.into_bytes()
                }
            } else {
                original.clone()
            };
            let (mut options, comment) = {
                let entry = archive
                    .by_index(index)
                    .map_err(|error| format!("Invalid Office package entry: {error}"))?;
                (
                    entry.options().into_full_options(),
                    entry.comment().to_string(),
                )
            };
            if !comment.is_empty() {
                options = options.with_file_comment(comment);
            }
            output
                .start_file(name.clone(), options)
                .map_err(|error| format!("Could not write Office package: {error}"))?;
            output
                .write_all(&bytes)
                .map_err(|error| format!("Could not write Office package: {error}"))?;
        }
        let mut bytes = output
            .finish()
            .map_err(|error| format!("Could not finish Office package: {error}"))?
            .into_inner();
        if document_type == DocumentType::Civ010 {
            bytes = patch_civ010_numeric_cells(&bytes, record)?;
        }
        generated.push(GeneratedFile {
            filename: output_filename(document_type, record),
            bytes,
            measured_ambient: record.measured_ambient.clone(),
        });
    }
    Ok(generated)
}

static CIV_RFI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)CIV-\d{3,}").expect("static CIV RFI regex"));
static CIV_RFI_ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)AZE-SUN-INT-QMS-RFI-CIV-\d{3,}|CIV-\d{3,}")
        .expect("static CIV RFI anchor regex")
});
static CIV_RFI_LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^CIV-\d+$").expect("static RFI label filter"));

fn rfi_anchor(xml: &str) -> Option<String> {
    CIV_RFI_ANCHOR
        .find(&xml_text_nodes(xml).join(""))
        .map(|found| found.as_str().to_string())
}

fn location_anchor(parts: &[(String, Vec<u8>)]) -> Option<String> {
    labeled_anchor(
        parts,
        &["LOCATION/STRUCTURE", "LOCATION", "AREA / LOCATION"],
    )
}

const KNOWN_LABELS: &[&str] = &[
    "REPORT (RFI) NUMBER",
    "REPORT (RFI) NO.",
    "CONCRETE TYPE (CLASS)",
    "SUPPLIER",
    "BATCH TICKET NO.",
    "TRUCK NUMBER",
    "QUANTITY POURED",
    "TOTAL QUANTITY POURED",
    "CONCRETE BATCH TIME",
    "CONCRETE POURING DATE",
    "CONCRETE POURING START TIME",
    "CONCRETE POURING END TIME",
    "IN-PLANT SLUMP",
    "IN-SITU SLUMP",
    "IN-PLANT TEMP",
    "IN-SITU TEMP",
    "WATER/CEMENT RATIO",
    "CONCRETE SAMPLE NO.",
    "DRAWING NO.",
    "TAG NO.",
    "AMBIENT TEMP.",
    "AREA / LOCATION",
];

fn labeled_anchor(parts: &[(String, Vec<u8>)], labels: &[&str]) -> Option<String> {
    let rfi_label: &Regex = &CIV_RFI_LABEL;
    for (name, bytes) in parts {
        if !name.ends_with(".xml") {
            continue;
        }
        let xml = String::from_utf8_lossy(bytes);
        let texts = xml_text_nodes(&xml);
        for (index, text) in texts.iter().enumerate() {
            let normalized = normalize(text);
            if labels
                .iter()
                .any(|label| normalized.contains(&normalize(label)))
            {
                if let Some(value) = inline_value_after_label(text, labels) {
                    return Some(value);
                }
                if let Some(value) = texts.iter().skip(index + 1).take(5).find(|value| {
                    !value.trim().is_empty()
                        && !labels
                            .iter()
                            .any(|label| normalize(value).contains(&normalize(label)))
                        && !KNOWN_LABELS
                            .iter()
                            .any(|label| normalize(value) == normalize(label))
                        && !value.trim().starts_with("AZE-SUN-INT-QMS-RFI-")
                        && !rfi_label.is_match(value.trim())
                }) {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

fn inline_value_after_label(text: &str, labels: &[&str]) -> Option<String> {
    let upper = text.to_uppercase();
    for label in labels {
        let Some(index) = upper.find(&label.to_uppercase()) else {
            continue;
        };
        let tail = &text[index + label.len()..];
        if let Some((_, value)) = tail.rsplit_once(':') {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

static AMBIENT_TEMPLATE_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*[+-]?[0-9]+(?:[.,][0-9]+)?(\s*(?:°\s*)?c?\s*)$")
        .expect("static ambient template value regex")
});

fn replace_civ011_ambient(mut xml: String, ambient: &str) -> Result<String, String> {
    let nodes = TEXT_NODE
        .find_iter(&xml)
        .filter_map(|node| {
            let raw = node.as_str();
            let range = node.start() + raw.find('>')? + 1..node.start() + raw.rfind("</")?;
            let text = unescape(&xml[range.clone()]).ok()?.into_owned();
            Some((range, text))
        })
        .collect::<Vec<_>>();
    let mut target = None;
    for start in 0..nodes.len() {
        let mut label = String::new();
        for end in start..nodes.len().min(start + 5) {
            label.push_str(&normalize(&nodes[end].1));
            if !label.contains("AMBIENTTEMP") {
                continue;
            }
            if let Some((range, suffix)) = nodes
                .iter()
                .skip(end + 1)
                .take(5)
                .take_while(|(_, text)| {
                    let normalized = normalize(text);
                    !KNOWN_LABELS
                        .iter()
                        .any(|label| normalized == normalize(label))
                })
                .find_map(|(range, text)| {
                    let suffix = AMBIENT_TEMPLATE_VALUE.captures(text)?.get(1)?.as_str();
                    Some((range, suffix))
                })
            {
                if target
                    .as_ref()
                    .is_some_and(|(previous, _)| previous != range)
                {
                    return Err(
                        "CON-CIV-011 template has ambiguous ambient-temperature values".to_string(),
                    );
                }
                target = Some((range.clone(), suffix));
            }
            break;
        }
    }
    let (range, suffix) = target.ok_or_else(|| {
        "CON-CIV-011 template is missing its ambient-temperature value".to_string()
    })?;
    let replacement = format!("{ambient}{suffix}");
    xml.replace_range(range, &xml_escape(&replacement));
    Ok(xml)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_uppercase)
        .collect()
}

fn xml_text_nodes(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut result = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(text)) => {
                if let Ok(decoded) = text.decode()
                    && let Ok(unescaped) = unescape(&decoded)
                {
                    result.push(unescaped.into_owned());
                }
            }
            Ok(Event::CData(text)) => {
                result.push(String::from_utf8_lossy(text.as_ref()).into_owned())
            }
            Ok(Event::Eof) => break,
            Err(_) => return Vec::new(),
            _ => {}
        }
    }
    result
}

static TEXT_NODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<([A-Za-z_][\w.-]*:)?t\b[^>]*>.*?</([A-Za-z_][\w.-]*:)?t\s*>")
        .expect("static OOXML text-node regex")
});

fn replace_xml_text(xml: &str, replacements: &[(String, String)], rfi_regex: &Regex) -> String {
    let mut output = xml.to_string();
    for (old, new) in replacements {
        if old.is_empty() {
            continue;
        }
        let mut replaced = false;
        output = TEXT_NODE
            .replace_all(&output, |captures: &regex::Captures<'_>| {
                if replaced {
                    return captures[0].to_string();
                }
                let node = &captures[0];
                let open_end = node.find('>').unwrap_or(0) + 1;
                let close_start = node.rfind("</").unwrap_or(node.len());
                let content = &node[open_end..close_start];
                let decoded = unescape(content)
                    .map(std::borrow::Cow::into_owned)
                    .unwrap_or_default();
                let replacement = if decoded == *old || (old.len() > 8 && decoded.contains(old)) {
                    replaced = true;
                    decoded.replacen(old, new, 1)
                } else {
                    node.to_string()
                };
                if replacement == node {
                    replacement
                } else {
                    format!(
                        "{}{}{}",
                        &node[..open_end],
                        xml_escape(&replacement),
                        &node[close_start..]
                    )
                }
            })
            .into_owned();
        if !replaced && old.len() > 8 {
            output = replace_split_text_nodes(&output, old, new);
        }
    }
    let mut replaced_rfi = false;
    TEXT_NODE
        .replace_all(&output, |captures: &regex::Captures<'_>| {
            if replaced_rfi {
                return captures[0].to_string();
            }
            let node = &captures[0];
            let open_end = node.find('>').unwrap_or(0) + 1;
            let close_start = node.rfind("</").unwrap_or(node.len());
            let content = &node[open_end..close_start];
            let decoded = unescape(content)
                .map(std::borrow::Cow::into_owned)
                .unwrap_or_default();
            if let Some(found) = rfi_regex.find(&decoded) {
                replaced_rfi = true;
                format!(
                    "{}{}{}{}{}",
                    &node[..open_end],
                    xml_escape(&decoded[..found.start()]),
                    xml_escape(&replacements[0].1),
                    xml_escape(&decoded[found.end()..]),
                    &node[close_start..]
                )
            } else {
                node.to_string()
            }
        })
        .into_owned()
}

fn replace_split_text_nodes(xml: &str, old: &str, new: &str) -> String {
    static TAG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").expect("static XML tag regex"));

    let nodes = TEXT_NODE.find_iter(xml).collect::<Vec<_>>();
    if nodes.len() < 2 {
        return xml.to_string();
    }
    let mut edits = Vec::new();
    let mut index = 0;
    while index < nodes.len() {
        let mut end = index + 1;
        while end < nodes.len() {
            let gap = &xml[nodes[end - 1].end()..nodes[end].start()];
            if gap.contains("</w:p")
                || gap.contains("</p")
                || gap.contains("</w:tc")
                || gap.contains("</tc")
                || !TAG.replace_all(gap, "").trim().is_empty()
            {
                break;
            }
            end += 1;
        }
        if end - index > 1 {
            let values = nodes[index..end]
                .iter()
                .map(|matched| {
                    let raw = &xml[matched.start()..matched.end()];
                    let open_end = raw.find('>').unwrap_or(0) + 1;
                    let close_start = raw.rfind("</").unwrap_or(raw.len());
                    unescape(&raw[open_end..close_start])
                        .ok()
                        .map(std::borrow::Cow::into_owned)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>();
            let joined = values.concat();
            if let Some(start) = joined.find(old) {
                let finish = start + old.len();
                let mut offset = 0;
                let mut inserted = false;
                for (node_index, value) in values.iter().enumerate() {
                    let node_end = offset + value.len();
                    if start < node_end && finish > offset {
                        let left = start.saturating_sub(offset);
                        let right = finish.min(node_end) - offset;
                        let replacement = format!(
                            "{}{}{}",
                            &value[..left],
                            if inserted { "" } else { new },
                            &value[right..],
                        );
                        inserted = true;
                        let matched = nodes[index + node_index];
                        let raw = &xml[matched.start()..matched.end()];
                        let open_end = matched.start() + raw.find('>').unwrap_or(0) + 1;
                        let close_start = matched.start() + raw.rfind("</").unwrap_or(raw.len());
                        edits.push((open_end..close_start, xml_escape(&replacement)));
                    }
                    offset = node_end;
                }
            }
        }
        index = end;
    }
    let mut result = xml.to_string();
    for (range, replacement) in edits.into_iter().rev() {
        result.replace_range(range, &replacement);
    }
    result
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn output_filename(document_type: DocumentType, record: &SourceRecord) -> String {
    let serial = record.rfi.rsplit('-').next().unwrap_or(record.rfi.as_str());
    let serial = serial
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>();
    let kind = match document_type {
        DocumentType::Civ010 => "CON-CIV-010",
        DocumentType::Civ011 => "CON-CIV-011",
        DocumentType::Civ013 => "CON-CIV-013",
        DocumentType::Civ014 => "CON-CIV-014",
    };
    format!("{kind}_CIV-{serial}.{}", document_type.expected_extension())
}

mod cells;
use cells::*;

#[cfg(test)]
mod tests;
