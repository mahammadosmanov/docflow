use std::io::{Cursor, Read, Write};

use quick_xml::{Reader, escape::unescape, events::Event};
use regex::Regex;
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
    pub ambient_range: Option<(u8, u8)>,
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
    ambient_range: (u8, u8),
    rng_seed: u64,
) -> Result<Vec<GeneratedFile>, String> {
    if records.is_empty() {
        return Err("No concrete source records were provided".to_string());
    }
    if ambient_range.0 > ambient_range.1 {
        return Err("Ambient temperature range is invalid".to_string());
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
        let name = entry.name().to_string();
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
        DocumentType::Civ010 | DocumentType::Civ013 => parts
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

    let rfi_regex = Regex::new(r"(?i)CIV-\d{3,}").map_err(|error| error.to_string())?;
    let mut rng = rng_seed;
    let mut generated = Vec::with_capacity(records.len());
    for record in records {
        if record.rfi.trim().is_empty() || record.location.trim().is_empty() {
            return Err("A source record is missing its RFI number or location".to_string());
        }
        let mut replacements = if document_type == DocumentType::Civ010 {
            Vec::new()
        } else {
            vec![
                (sample_rfi.clone(), record.rfi.clone()),
                (sample_location.clone(), record.location.clone()),
            ]
        };
        if document_type == DocumentType::Civ011 {
            let ambient_range = record.ambient_range.unwrap_or(ambient_range);
            rng = xorshift64(rng);
            let width = u64::from(ambient_range.1 - ambient_range.0) + 1;
            let ambient = (u64::from(ambient_range.0) + rng % width).to_string();
            let old = ambient_anchor(&parts).ok_or_else(|| {
                "CON-CIV-011 template is missing its ambient-temperature value".to_string()
            })?;
            let suffix = old
                .char_indices()
                .find(|(_, ch)| !ch.is_ascii_digit() && *ch != '.' && *ch != ',' && *ch != '-')
                .map(|(index, _)| old[index..].to_string())
                .unwrap_or_default();
            replacements.push((old, format!("{ambient}{suffix}")));
        }
        let mut civ010_ambients = Vec::new();
        if document_type == DocumentType::Civ010 {
            let temperatures = std::iter::once(record.plant_temp.as_str()).chain(
                record
                    .additional_batches
                    .iter()
                    .map(|batch| batch.plant_temp.as_str()),
            );
            for temperature in temperatures {
                rng = xorshift64(rng);
                let plant_temperature = temperature
                    .trim()
                    .replace(',', ".")
                    .parse::<f64>()
                    .map_err(|_| format!("CON-CIV-010 source has no numeric in-plant fresh-concrete temperature for {}", record.rfi))?;
                let offset = 5 + rng % 3;
                civ010_ambients.push(format!("{:.1}", plant_temperature - offset as f64 / 10.0));
            }
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
                replace_xml_text(&xml, &replacements, &rfi_regex).into_bytes()
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
            bytes = patch_civ010_numeric_cells(&bytes, record, &civ010_ambients)?;
        }
        generated.push(GeneratedFile {
            filename: output_filename(document_type, record),
            bytes,
        });
    }
    Ok(generated)
}
fn patch_civ010_numeric_cells(
    package: &[u8],
    record: &SourceRecord,
    ambients: &[String],
) -> Result<Vec<u8>, String> {
    let mut batches = vec![ConcreteBatch {
        tag: record.tag.clone(),
        concrete_class: record.concrete_class.clone(),
        supplier: record.supplier.clone(),
        batch_ticket: record.batch_ticket.clone(),
        truck: record.truck.clone(),
        quantity: record.quantity.clone(),
        total_quantity: record.total_quantity.clone(),
        batch_time: record.batch_time.clone(),
        start_time: record.start_time.clone(),
        end_time: record.end_time.clone(),
        plant_slump: record.plant_slump.clone(),
        in_situ_slump: record.in_situ_slump.clone(),
        plant_temp: record.plant_temp.clone(),
        in_situ_temp: record.in_situ_temp.clone(),
        samples: record.samples.clone(),
    }];
    batches.extend(record.additional_batches.iter().cloned());
    if batches.len() == 1 {
        batches.push(ConcreteBatch::default());
    }
    let mut archive = ZipArchive::new(Cursor::new(package))
        .map_err(|error| format!("Invalid generated CON-CIV-010 workbook: {error}"))?;
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Invalid generated workbook entry: {error}"))?;
        let name = entry.name().to_string();
        let (options, comment) = (
            entry.options().into_full_options(),
            entry.comment().to_string(),
        );
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read generated workbook entry: {error}"))?;
        if name == "xl/worksheets/sheet1.xml" {
            let mut xml = String::from_utf8(bytes)
                .map_err(|error| format!("Invalid worksheet XML: {error}"))?;
            xml = expand_civ010_batch_rows(&xml, batches.len())?;
            for (index, batch) in batches.iter().enumerate() {
                let row = 19 + index * 2;
                let ambient = ambients.get(index).map(String::as_str).unwrap_or("");
                for (column, value) in [
                    ("AI", batch.quantity.as_str()),
                    ("AP", batch.total_quantity.as_str()),
                    ("AW", batch.batch_time.as_str()),
                    ("BD", ambient),
                    ("BK", batch.start_time.as_str()),
                    ("BR", batch.end_time.as_str()),
                    ("BY", batch.plant_slump.as_str()),
                    ("CC", batch.in_situ_slump.as_str()),
                    ("CF", batch.plant_temp.as_str()),
                    ("CJ", batch.in_situ_temp.as_str()),
                    ("CM", record.water_cement_ratio.as_deref().unwrap_or("")),
                ] {
                    let numeric = if let Some((hour, minute)) = value.split_once(':') {
                        match (hour.parse::<u32>(), minute.parse::<u32>()) {
                            (Ok(hour), Ok(minute)) if hour < 24 && minute < 60 => {
                                format!("{:.15}", (hour * 60 + minute) as f64 / 1440.0)
                            }
                            _ => value.to_string(),
                        }
                    } else {
                        value.to_string()
                    };
                    xml = set_numeric_cell(&xml, &format!("{column}{row}"), &numeric)?;
                }
                for (column, value) in [
                    ("A", &batch.tag),
                    ("H", &batch.concrete_class),
                    ("O", &batch.supplier),
                    ("V", &batch.batch_ticket),
                    ("AC", &batch.truck),
                    ("CS", &batch.samples),
                ] {
                    xml = set_text_cell(&xml, &format!("{column}{row}"), value)?;
                }
            }
            let header_updates = [
                ("A11", format!("DRAWING NO: {}", record.drawing)),
                ("A12", format!("REPORT (RFI) NUMBER: {}", record.rfi)),
                ("A13", format!("LOCATION: {}", record.location)),
                (
                    "BB14",
                    format!("CONCRETE POURING DATE:{}", record.pour_date),
                ),
            ];
            for (reference, value) in header_updates {
                xml = set_text_cell(&xml, reference, &value)?;
            }
            bytes = xml.into_bytes();
        }
        let options = if comment.is_empty() {
            options
        } else {
            options.with_file_comment(comment)
        };
        output
            .start_file(name, options)
            .map_err(|error| format!("Could not write generated workbook: {error}"))?;
        output
            .write_all(&bytes)
            .map_err(|error| format!("Could not write generated workbook: {error}"))?;
    }
    output
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|error| format!("Could not finish generated workbook: {error}"))
}

fn expand_civ010_batch_rows(xml: &str, batch_count: usize) -> Result<String, String> {
    if batch_count <= 2 {
        return Ok(xml.to_string());
    }
    let row_pattern = Regex::new(r#"(?s)<row\b[^>]*\br="21"[^>]*>.*?</row>"#)
        .expect("static worksheet row pattern");
    if !row_pattern.is_match(xml) {
        return Err("CON-CIV-010 template has no second concrete-batch row to extend".to_string());
    }
    let extra_rows = (batch_count - 2) * 2;
    let mut shifted = shift_civ010_row_references(xml, extra_rows)?;
    let shifted_row = row_pattern
        .find(&shifted)
        .map(|matched| matched.as_str().to_string())
        .ok_or_else(|| "CON-CIV-010 worksheet row structure is invalid".to_string())?;
    let mut clones = String::new();
    for index in 2..batch_count {
        let target_row = 19 + index * 2;
        clones.push_str(&clone_civ010_row(&shifted_row, target_row)?);
    }
    let replacement = format!("{shifted_row}{clones}");
    let range = row_pattern
        .find(&shifted)
        .expect("row pattern already found")
        .range();
    shifted.replace_range(range, &replacement);
    Ok(shifted)
}

fn shift_civ010_row_references(xml: &str, rows: usize) -> Result<String, String> {
    let attribute = Regex::new(r#"(?P<prefix>\b(?:r|ref|sqref)=")(?P<value>[^"]+)""#)
        .map_err(|error| error.to_string())?;
    let coordinate = Regex::new(r"([A-Z]{1,3})([0-9]+)").map_err(|error| error.to_string())?;
    Ok(attribute
        .replace_all(xml, |captures: &regex::Captures<'_>| {
            let prefix = &captures["prefix"];
            let value = &captures["value"];
            let shifted = if prefix == "r=\"" {
                if let Ok(row) = value.parse::<usize>() {
                    if row >= 22 {
                        (row + rows).to_string()
                    } else {
                        value.to_string()
                    }
                } else {
                    shift_coordinate_rows(&coordinate, value, rows, 22)
                }
            } else {
                shift_coordinate_rows(&coordinate, value, rows, 22)
            };
            format!("{prefix}{shifted}\"")
        })
        .into_owned())
}

fn clone_civ010_row(row: &str, target_row: usize) -> Result<String, String> {
    let attribute = Regex::new(r#"(?P<prefix>\b(?:r|ref|sqref)=")(?P<value>[^"]+)""#)
        .map_err(|error| error.to_string())?;
    let coordinate = Regex::new(r"([A-Z]{1,3})([0-9]+)").map_err(|error| error.to_string())?;
    Ok(attribute
        .replace_all(row, |captures: &regex::Captures<'_>| {
            let prefix = &captures["prefix"];
            let value = &captures["value"];
            let cloned = if prefix == "r=\"" && value == "21" {
                target_row.to_string()
            } else {
                shift_coordinate_rows(&coordinate, value, target_row.saturating_sub(21), 21)
            };
            format!("{prefix}{cloned}\"")
        })
        .into_owned())
}

fn shift_coordinate_rows(
    coordinate: &Regex,
    value: &str,
    delta: usize,
    threshold: usize,
) -> String {
    coordinate
        .replace_all(value, |captures: &regex::Captures<'_>| {
            let Ok(row) = captures[2].parse::<usize>() else {
                return captures[0].to_string();
            };
            if row < threshold {
                captures[0].to_string()
            } else {
                format!("{}{}", &captures[1], row + delta)
            }
        })
        .into_owned()
}

fn set_numeric_cell(xml: &str, reference: &str, value: &str) -> Result<String, String> {
    let pattern = Regex::new(&format!(r#"(?s)<c\b[^>]*\br="{reference}"[^>]*>.*?</c>"#))
        .map_err(|error| error.to_string())?;
    let Some(cell) = pattern.find(xml) else {
        return Ok(xml.to_string());
    };
    let mut replacement = cell.as_str().replace(" t=\"s\"", "");
    let value_pattern = Regex::new(r"(?s)<v>.*?</v>").expect("static cell-value expression");
    if let Some(value_node) = value_pattern.find(&replacement) {
        if value.is_empty() {
            replacement.replace_range(value_node.range(), "");
        } else {
            replacement.replace_range(value_node.range(), &format!("<v>{value}</v>"));
        }
    } else if !value.is_empty() {
        let index = replacement
            .rfind("</c>")
            .ok_or_else(|| format!("Malformed workbook cell {reference}"))?;
        replacement.insert_str(index, &format!("<v>{value}</v>"));
    }
    let mut output = xml.to_string();
    output.replace_range(cell.range(), &replacement);
    Ok(output)
}
fn set_text_cell(xml: &str, reference: &str, value: &str) -> Result<String, String> {
    let pattern = Regex::new(&format!(r#"(?s)<c\b[^>]*\br="{reference}"[^>]*>.*?</c>"#))
        .map_err(|error| error.to_string())?;
    let Some(cell) = pattern.find(xml) else {
        return Err(format!("CON-CIV-010 template is missing cell {reference}"));
    };
    let matched = cell.as_str();
    let opening_end = matched
        .find('>')
        .ok_or_else(|| format!("Malformed cell {reference}"))?;
    let mut opening = matched[..=opening_end].to_string();
    let type_attribute = Regex::new(r#"\s+t="[^"]*""#).expect("static cell type attribute");
    opening = type_attribute.replace_all(&opening, "").into_owned();
    opening.pop();
    opening.push_str(" t=\"inlineStr\">");
    let body = if value.is_empty() {
        "<is><t></t></is>".to_string()
    } else {
        format!(
            "<is><t xml:space=\"preserve\">{}</t></is>",
            xml_escape(value)
        )
    };
    let replacement = format!("{opening}{body}</c>");
    let mut output = xml.to_string();
    output.replace_range(cell.range(), &replacement);
    Ok(output)
}

fn rfi_anchor(xml: &str) -> Option<String> {
    let regex = Regex::new(r"(?i)AZE-SUN-INT-QMS-RFI-CIV-\d{3,}|CIV-\d{3,}").ok()?;
    regex
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
    let rfi_label = Regex::new(r"(?i)^CIV-\d+$").expect("static RFI label filter");
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

fn ambient_anchor(parts: &[(String, Vec<u8>)]) -> Option<String> {
    for (name, bytes) in parts {
        if name != "word/document.xml" && !name.starts_with("xl/worksheets/") {
            continue;
        }
        let xml = String::from_utf8_lossy(bytes);
        let texts = xml_text_nodes(&xml);
        for start in 0..texts.len() {
            let mut label = String::new();
            for end in start..texts.len().min(start + 5) {
                label.push_str(&normalize(&texts[end]));
                if label.contains("AMBIENTTEMPERATURE") || label.contains("AMBIENTTEMP") {
                    if let Some(value) = texts.iter().skip(end + 1).take(5).find(|value| {
                        let number = value.trim().trim_end_matches([' ', 'C', 'c', '°']);
                        number.parse::<f64>().is_ok()
                    }) {
                        return Some(value.trim().to_string());
                    }
                    break;
                }
            }
        }
    }
    None
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
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

fn replace_xml_text(xml: &str, replacements: &[(String, String)], rfi_regex: &Regex) -> String {
    use std::sync::LazyLock;
    static TEXT_NODE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)<([A-Za-z_][\w.-]*:)?t\b[^>]*>.*?</([A-Za-z_][\w.-]*:)?t\s*>")
            .expect("static OOXML text-node regex")
    });
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
                    .map(|value| value.into_owned())
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
                .map(|value| value.into_owned())
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
    use std::sync::LazyLock;
    static TEXT_NODE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)<([A-Za-z_][\w.-]*:)?t\b[^>]*>.*?</([A-Za-z_][\w.-]*:)?t\s*>")
            .expect("static OOXML text-node regex")
    });
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
                        .map(|value| value.into_owned())
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

fn xorshift64(state: u64) -> u64 {
    let mut value = if state == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        state
    };
    value ^= value << 13;
    value ^= value >> 7;
    value ^ (value << 17)
}

fn output_filename(document_type: DocumentType, record: &SourceRecord) -> String {
    let serial = record.rfi.rsplit('-').next().unwrap_or(record.rfi.as_str());
    let serial = serial
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    let kind = match document_type {
        DocumentType::Civ010 => "CON-CIV-010",
        DocumentType::Civ011 => "CON-CIV-011",
        DocumentType::Civ013 => "CON-CIV-013",
        DocumentType::Civ014 => "CON-CIV-014",
    };
    format!("{kind}_CIV-{serial}.{}", document_type.expected_extension())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{CompressionMethod, write::SimpleFileOptions};

    #[test]
    fn extends_civ010_template_rows_without_overwriting_following_rows() {
        let worksheet = r#"<worksheet><dimension ref="A1:CS22"/><sheetData><row r="19"><c r="AI19"><v>1</v></c></row><row r="20"></row><row r="21"><c r="AI21"><v>2</v></c></row><row r="22"><c r="AI22"><v>3</v></c></row></sheetData></worksheet>"#;
        let expanded = expand_civ010_batch_rows(worksheet, 4).unwrap();
        assert!(expanded.contains(r#"<dimension ref="A1:CS26"/>"#));
        assert!(expanded.contains(r#"<row r="23"><c r="AI23"><v>2</v></c></row>"#));
        assert!(expanded.contains(r#"<row r="25"><c r="AI25"><v>2</v></c></row>"#));
        assert!(expanded.contains(r#"<row r="26"><c r="AI26"><v>3</v></c></row>"#));
    }

    fn record(rfi: &str, location: &str) -> SourceRecord {
        SourceRecord {
            rfi: rfi.to_string(),
            location: location.to_string(),
            tag: String::new(),
            concrete_class: String::new(),
            supplier: String::new(),
            batch_ticket: String::new(),
            truck: String::new(),
            quantity: String::new(),
            total_quantity: String::new(),
            batch_time: String::new(),
            pour_date: String::new(),
            start_time: String::new(),
            end_time: String::new(),
            plant_slump: String::new(),
            in_situ_slump: String::new(),
            plant_temp: String::new(),
            in_situ_temp: String::new(),
            samples: String::new(),
            water_cement_ratio: None,
            ambient_range: None,
            drawing: String::new(),
            additional_batches: Vec::new(),
        }
    }

    fn package(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in entries {
            zip.start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn read_member(bytes: &[u8], name: &str) -> String {
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut member = zip.by_name(name).unwrap();
        let mut text = String::new();
        member.read_to_string(&mut text).unwrap();
        text
    }

    #[test]
    fn generates_docx_per_record_and_preserves_unrelated_parts() {
        let template = package(&[
            (
                "word/document.xml",
                "<w:document><w:p><w:t>AZE-SUN-INT-QMS-</w:t><w:t>RFI-CIV-1498</w:t></w:p><w:p><w:t>LOCATION</w:t><w:t>SOUTH SITE PCU-18</w:t></w:p><w:p><w:t>AMBIENT TEMP.</w:t><w:t>27 °C</w:t></w:p></w:document>",
            ),
            (
                "word/styles.xml",
                "<w:styles><w:style w:val=\"unchanged\"/></w:styles>",
            ),
        ]);
        let mut first = record("AZE-SUN-INT-QMS-RFI-CIV-2001", "PCU-1");
        first.ambient_range = Some((31, 31));
        let mut second = record("AZE-SUN-INT-QMS-RFI-CIV-2002", "PCU-2");
        second.ambient_range = Some((42, 42));
        let result = generate(
            DocumentType::Civ011,
            &template,
            &[first, second],
            (20, 30),
            4,
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert!(read_member(&result[0].bytes, "word/document.xml").contains("31 °C"));
        assert!(read_member(&result[0].bytes, "word/document.xml").contains("PCU-1"));
        assert!(read_member(&result[1].bytes, "word/document.xml").contains("42 °C"));
        assert!(read_member(&result[1].bytes, "word/document.xml").contains("PCU-2"));
        assert_eq!(
            read_member(&result[0].bytes, "word/styles.xml"),
            "<w:styles><w:style w:val=\"unchanged\"/></w:styles>"
        );
    }

    #[test]
    fn generates_xlsx_and_rejects_missing_anchor() {
        let template = package(&[
            (
                "xl/worksheets/sheet1.xml",
                "<worksheet><row><c><is><t>AZE-SUN-INT-QMS-RFI-CIV-1500</t></is></c><c><is><t>LOCATION/STRUCTURE</t></is></c><c><is><t>PCU-18 (South Site)</t></is></c></row></worksheet>",
            ),
            ("xl/styles.xml", "<styleSheet/>"),
        ]);
        let result = generate(
            DocumentType::Civ013,
            &template,
            &[record("AZE-SUN-INT-QMS-RFI-CIV-2003", "PCU-9")],
            (0, 0),
            1,
        )
        .unwrap();
        assert!(read_member(&result[0].bytes, "xl/worksheets/sheet1.xml").contains("PCU-9"));
        let invalid = package(&[("xl/worksheets/sheet1.xml", "<worksheet/>")]);
        assert!(
            generate(
                DocumentType::Civ013,
                &invalid,
                &[record("CIV-1", "x")],
                (0, 0),
                0
            )
            .is_err()
        );
    }

    #[test]
    fn generates_civ010_data_and_civ014_identity() {
        let mut source = record("AZE-SUN-INT-QMS-RFI-CIV-2010", "N-5");
        source.tag = "N-5".to_string();
        source.concrete_class = "C-30/37 XA2".to_string();
        source.supplier = "HOLCIM".to_string();
        source.batch_ticket = "66817".to_string();
        source.truck = "TRK-1".to_string();
        source.quantity = "5".to_string();
        source.total_quantity = "15".to_string();
        source.batch_time = "09:30".to_string();
        source.pour_date = "06.10.2026".to_string();
        source.start_time = "10:00".to_string();
        source.end_time = "11:00".to_string();
        source.plant_slump = "175".to_string();
        source.in_situ_slump = "170".to_string();
        source.plant_temp = "21.3".to_string();
        source.in_situ_temp = "21.4".to_string();
        source.samples = "841-846".to_string();
        let mut cells = vec![
            ("A11".to_string(), "DRAWING NO: DRG-1".to_string(), true),
            (
                "A12".to_string(),
                "REPORT (RFI) NUMBER: AZE-SUN-INT-QMS-RFI-CIV-1460".to_string(),
                true,
            ),
            (
                "A13".to_string(),
                "LOCATION: North site area / N-5".to_string(),
                true,
            ),
            (
                "BB14".to_string(),
                "CONCRETE POURING DATE:06.10.2026".to_string(),
                true,
            ),
            ("A19".to_string(), "N-5".to_string(), true),
            ("H19".to_string(), "C-30/37 XA2".to_string(), true),
            ("O19".to_string(), "HOLCIM".to_string(), true),
            ("V19".to_string(), "66817".to_string(), true),
            ("AC19".to_string(), "TRK-1".to_string(), true),
            ("CS19".to_string(), "841-846".to_string(), true),
        ];
        for reference in [
            "AI", "AP", "AW", "BD", "BK", "BR", "BY", "CC", "CF", "CJ", "CM",
        ] {
            cells.push((format!("{reference}19"), "1".to_string(), false));
            cells.push((format!("{reference}21"), "2".to_string(), false));
        }
        for reference in ["A", "H", "O", "V", "AC", "CS"] {
            cells.push((format!("{reference}21"), String::new(), true));
        }
        let render_row = |row: usize| {
            cells
                .iter()
                .filter(|(reference, _, _)| {
                    reference
                        .rfind(|character: char| !character.is_ascii_digit())
                        .and_then(|index| reference[index + 1..].parse::<usize>().ok())
                        == Some(row)
                })
                .map(|(reference, value, text)| {
                    if *text {
                        format!("<c r=\"{reference}\" t=\"inlineStr\"><is><t>{value}</t></is></c>")
                    } else {
                        format!("<c r=\"{reference}\"><v>{value}</v></c>")
                    }
                })
                .collect::<String>()
        };
        let worksheet = format!(
            "<worksheet><dimension ref=\"A1:CS22\"/><sheetData><row r=\"11\">{}</row><row r=\"12\">{}</row><row r=\"13\">{}</row><row r=\"14\">{}</row><row r=\"19\">{}</row><row r=\"20\"></row><row r=\"21\">{}</row><row r=\"22\"><c r=\"AI22\"><v>3</v></c></row></sheetData></worksheet>",
            render_row(11),
            render_row(12),
            render_row(13),
            render_row(14),
            render_row(19),
            render_row(21)
        );
        let xlsx = package(&[("xl/worksheets/sheet1.xml", &worksheet)]);
        let output = generate(DocumentType::Civ010, &xlsx, &[source], (0, 0), 2).unwrap();
        let sheet = read_member(&output[0].bytes, "xl/worksheets/sheet1.xml");
        assert!(sheet.contains("CIV-2010"), "{sheet}");
        assert!(sheet.contains("N-5"));
        assert!(sheet.contains("06.10.2026"));
        let cell = |reference: &str| {
            Regex::new(&format!(r#"(?s)<c\b[^>]*\br="{reference}"[^>]*>(.*?)</c>"#))
                .unwrap()
                .captures(&sheet)
                .map(|captures| captures[1].to_string())
                .unwrap()
        };
        assert!(cell("A12").contains("CIV-2010"));
        assert!(cell("H19").contains("C-30/37 XA2"));
        assert!(cell("AI19").contains("<v>5</v>"));
        assert!(cell("AP19").contains("<v>15</v>"));
        assert!(cell("CF19").contains("<v>21.3</v>"));
        let ambient_value = Regex::new(r"<v>([^<]+)</v>")
            .unwrap()
            .captures(&cell("BD19"))
            .unwrap()[1]
            .parse::<f64>()
            .unwrap();
        assert!((20.6..=20.8).contains(&ambient_value));
        assert!(cell("AI21").is_empty());
        let mut multi = record("AZE-SUN-INT-QMS-RFI-CIV-2010", "N-5");
        multi.plant_temp = "21.3".to_string();
        multi.additional_batches.push(ConcreteBatch {
            tag: "N-5".to_string(),
            concrete_class: "C-30/37 XA2".to_string(),
            supplier: "HOLCIM".to_string(),
            batch_ticket: "66818".to_string(),
            truck: "TRK-2".to_string(),
            quantity: "6".to_string(),
            total_quantity: "15".to_string(),
            batch_time: "10:00".to_string(),
            start_time: "10:00".to_string(),
            end_time: "11:00".to_string(),
            plant_slump: "175".to_string(),
            in_situ_slump: "170".to_string(),
            plant_temp: "21.3".to_string(),
            in_situ_temp: "21.4".to_string(),
            samples: "847-852".to_string(),
        });
        let mut third = multi.additional_batches[0].clone();
        third.batch_ticket = "66819".to_string();
        third.quantity = "7".to_string();
        third.samples = "853-858".to_string();
        multi.additional_batches.push(third);
        let multi_output = generate(DocumentType::Civ010, &xlsx, &[multi], (0, 0), 2).unwrap();
        let multi_sheet = read_member(&multi_output[0].bytes, "xl/worksheets/sheet1.xml");
        let second_cell = |reference: &str| {
            Regex::new(&format!(r#"(?s)<c\b[^>]*\br="{reference}"[^>]*>(.*?)</c>"#))
                .unwrap()
                .captures(&multi_sheet)
                .map(|captures| captures[1].to_string())
                .unwrap()
        };
        assert!(second_cell("AI21").contains("<v>6</v>"));
        assert!(second_cell("V21").contains("66818"));
        assert!(second_cell("CS21").contains("847-852"));
        assert!(second_cell("AI23").contains("<v>7</v>"));
        assert!(second_cell("V23").contains("66819"));
        assert!(second_cell("CS23").contains("853-858"));
        assert!(second_cell("AI24").contains("<v>3</v>"));
        assert!(multi_sheet.contains(r#"<dimension ref="A1:CS24"/>"#));

        let docx = package(&[(
            "word/document.xml",
            "<w:document><w:p><w:t>REPORT NO.</w:t><w:t>AZE-SUN-INT-QMS-RFI-CIV-1404</w:t></w:p><w:p><w:t>LOCATION</w:t><w:t>(North Site) N5</w:t></w:p></w:document>",
        )]);
        let output = generate(
            DocumentType::Civ014,
            &docx,
            &[record("AZE-SUN-INT-QMS-RFI-CIV-2014", "South Site")],
            (0, 0),
            2,
        )
        .unwrap();
        let document = read_member(&output[0].bytes, "word/document.xml");
        assert!(document.contains("CIV-2014"));
        assert!(document.contains("South Site"));
    }
}
