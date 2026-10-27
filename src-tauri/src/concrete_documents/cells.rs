use super::*;

pub(super) fn patch_civ010_numeric_cells(
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
        let name = entry
            .name()
            .map_err(|error| format!("Invalid generated workbook entry name: {error}"))?
            .into_owned();
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
                let ambient = ambients.get(index).map_or("", String::as_str);
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
        .map(std::io::Cursor::into_inner)
        .map_err(|error| format!("Could not finish generated workbook: {error}"))
}

pub(super) fn expand_civ010_batch_rows(xml: &str, batch_count: usize) -> Result<String, String> {
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

pub(super) fn shift_civ010_row_references(xml: &str, rows: usize) -> Result<String, String> {
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

pub(super) fn clone_civ010_row(row: &str, target_row: usize) -> Result<String, String> {
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

pub(super) fn shift_coordinate_rows(
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

pub(super) fn set_numeric_cell(xml: &str, reference: &str, value: &str) -> Result<String, String> {
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
pub(super) fn set_text_cell(xml: &str, reference: &str, value: &str) -> Result<String, String> {
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
