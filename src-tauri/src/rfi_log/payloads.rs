use super::*;

pub fn preview_table(plan: &Plan, sheet: Option<&str>) -> Result<Value, BackendError> {
    let (fields, headers, rows, from_workbook, widths) = copy_data(plan, sheet)?;
    let table = json!({
        "headers": headers,
        "fields": fields,
        "rows": rows.iter().map(|row| fields.iter().map(|field| row.get(field).cloned().unwrap_or_default()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "columnWidths": widths.into_iter().map(|width| width.clamp(7.0, 42.0)).collect::<Vec<_>>(),
        "fromWorkbook": from_workbook,
        "count": rows.len(),
    });
    if serde_json::to_vec(&table).is_ok_and(|encoded| encoded.len() <= MAX_TABLE_BYTES) {
        Ok(table)
    } else {
        Err(BackendError::validation(
            "table preview exceeded the size limit",
        ))
    }
}

pub fn copy_payload(plan: &Plan, sheet: Option<&str>) -> Result<ClipboardPayload, BackendError> {
    let (fields, headers, rows, _, _) = copy_data(plan, sheet)?;
    let plain_rows = rows
        .iter()
        .map(|row| {
            fields
                .iter()
                .map(|field| row.get(field).cloned().unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>();
    let plain = std::iter::once(headers.join("\t"))
        .chain(plain_rows)
        .collect::<Vec<_>>()
        .join("\r\n");
    let headings = headers
        .iter()
        .map(|header| format!("<th style=\"border:1px solid #8b8b8b;padding:6px 8px;font-weight:700;text-align:center;vertical-align:middle;\">{}</th>", html_escape(header)))
        .collect::<String>();
    let body = rows
        .iter()
        .map(|row| {
            let cells = fields
                .iter()
                .map(|field| {
                    let align = if matches!(field.as_str(), "description" | "forms" | "location") { "left" } else { "center" };
                    let value = html_escape(row.get(field).map_or("", String::as_str)).replace('\n', "<br>");
                    format!("<td style=\"border:1px solid #9a9a9a;padding:6px 8px;text-align:{align};vertical-align:middle;white-space:normal;\">{value}</td>")
                })
                .collect::<String>();
            format!("<tr>{cells}</tr>")
        })
        .collect::<String>();
    let html = format!(
        "<table cellpadding=\"0\" cellspacing=\"0\" style=\"border-collapse:collapse;font-family:Arial,sans-serif;font-size:10pt;color:#202020;\"><thead><tr style=\"background:#dedede;\">{headings}</tr></thead><tbody>{body}</tbody></table>"
    );
    if plain.len() > MAX_TABLE_BYTES || html.len() > MAX_TABLE_BYTES {
        return Err(BackendError::validation(
            "clipboard table exceeded the size limit",
        ));
    }
    Ok(ClipboardPayload {
        html,
        plain,
        count: rows.len(),
    })
}

pub fn reply_payload(plan: &Plan) -> Result<ClipboardPayload, BackendError> {
    let editor = plan
        .meta
        .get("editor")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("bu planda kopyalanabilir tablo yok"))?;
    let sources = editor
        .get("copySources")
        .and_then(Value::as_object)
        .ok_or_else(|| BackendError::validation("bu planda kopyalanabilir tablo yok"))?;
    let sheets = DISCIPLINES
        .into_iter()
        .filter(|sheet| sources.contains_key(*sheet))
        .collect::<Vec<_>>();
    if sheets.is_empty() {
        return Err(BackendError::validation(
            "bu planda kopyalanabilir tablo yok",
        ));
    }

    let multiple = sheets.len() > 1;
    let mut plain_parts = Vec::with_capacity(sheets.len());
    let mut html_parts = Vec::with_capacity(sheets.len());
    let mut count = 0usize;
    for sheet in sheets {
        let payload = copy_payload(plan, Some(sheet))?;
        count += payload.count;
        plain_parts.push(if multiple {
            format!("{sheet}\r\n{}", payload.plain)
        } else {
            payload.plain
        });
        html_parts.push(if multiple {
            format!("<div style=\"font-family:Arial,sans-serif;font-size:10pt;font-weight:700;margin:0 0 6px 0;\">{sheet}</div>{}", payload.html)
        } else {
            payload.html
        });
    }
    let plain = plain_parts.join("\r\n\r\n");
    let html = format!("{}<br><br>", html_parts.join("<br>"));
    if plain.len() > MAX_TABLE_BYTES || html.len() > MAX_TABLE_BYTES {
        return Err(BackendError::validation(
            "clipboard table exceeded the size limit",
        ));
    }
    Ok(ClipboardPayload { plain, html, count })
}
