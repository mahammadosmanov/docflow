use super::*;

pub(in crate::ooxml) fn normalize_package_root_target(target: &str) -> Result<String, OoxmlError> {
    let decoded = percent_decode_target(target)?.replace('\\', "/");
    if decoded.split('/').any(|segment| segment == "..") {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    normalize_relationship_target_from_parts(Vec::new(), target)
}

pub(in crate::ooxml) fn normalize_workbook_relationship_target(
    workbook_part_path: &str,
    target: &str,
) -> Result<String, OoxmlError> {
    let decoded = percent_decode_target(target)?.replace('\\', "/");
    if decoded.split('/').any(|segment| segment == "..") {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    normalize_relationship_target(workbook_part_path, target)
}

pub(in crate::ooxml) fn normalize_relationship_target(
    source_part_path: &str,
    target: &str,
) -> Result<String, OoxmlError> {
    let mut source_parts = source_part_path
        .split('/')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if source_parts.is_empty()
        || source_parts
            .iter()
            .any(|part| part.is_empty() || matches!(part.as_str(), "." | ".."))
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    source_parts.pop();
    normalize_relationship_target_from_parts(source_parts, target)
}

pub(in crate::ooxml) fn normalize_relationship_target_from_parts(
    mut source_parts: Vec<String>,
    target: &str,
) -> Result<String, OoxmlError> {
    if target.is_empty()
        || target.starts_with("//")
        || target.contains(['?', '#', '\0'])
        || target.contains(':')
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    let target = percent_decode_target(target)?.replace('\\', "/");
    if target.is_empty()
        || target.starts_with("//")
        || target.contains(['?', '#', '\0'])
        || target.contains(':')
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    let absolute = target.starts_with('/');
    let mut parts = if absolute {
        Vec::new()
    } else {
        std::mem::take(&mut source_parts)
    };

    for component in target.trim_start_matches('/').split('/') {
        match component {
            "" => {
                return Err(OoxmlError::invalid(
                    "worksheet relationship escapes the package",
                ));
            }
            "." => {}
            ".." => {
                if parts.is_empty() {
                    return Err(OoxmlError::invalid(
                        "worksheet relationship escapes the package",
                    ));
                }
                parts.pop();
            }
            part => parts.push(part.to_string()),
        }
    }
    if parts.is_empty() {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    Ok(parts.join("/"))
}

pub(in crate::ooxml) fn relationship_part_path(part_path: &str) -> Result<String, OoxmlError> {
    let part = Path::new(part_path);
    let file_name = part
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| OoxmlError::invalid("relationship source part path is invalid"))?;
    let parent = part.parent().and_then(Path::to_str).unwrap_or_default();
    if parent.is_empty() {
        Ok(format!("_rels/{file_name}.rels"))
    } else {
        Ok(format!("{parent}/_rels/{file_name}.rels"))
    }
}

pub(in crate::ooxml) fn percent_decode_target(target: &str) -> Result<String, OoxmlError> {
    let bytes = target.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let encoded = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| OoxmlError::invalid("worksheet relationship target is invalid"))?;
            let hex = std::str::from_utf8(encoded)
                .ok()
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .ok_or_else(|| OoxmlError::invalid("worksheet relationship target is invalid"))?;
            decoded.push(hex);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|_| OoxmlError::invalid("worksheet relationship target is invalid"))
}
