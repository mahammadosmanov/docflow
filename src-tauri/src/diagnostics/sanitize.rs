use super::*;

pub(super) fn sanitize_error_code(code: &str) -> String {
    if !code.is_empty()
        && code.len() <= MAX_ERROR_CODE_BYTES
        && code.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        })
    {
        code.to_string()
    } else {
        REDACTED_ERROR_CODE.to_string()
    }
}

pub(crate) fn sanitize_value(value: &mut Value, depth: usize) {
    if depth >= MAX_DIAGNOSTIC_DEPTH {
        *value = Value::String(REDACTED_DEPTH.to_string());
        return;
    }
    match value {
        Value::Object(object) => {
            if object.len() > MAX_DIAGNOSTIC_ITEMS {
                let bounded = std::mem::take(object)
                    .into_iter()
                    .take(MAX_DIAGNOSTIC_ITEMS)
                    .collect();
                *object = bounded;
            }
            for (key, nested) in object {
                let normalized = normalize_key(key);
                if is_secret_key(&normalized) {
                    *nested = Value::String(REDACTED_SECRET.to_string());
                } else if is_content_key(&normalized) {
                    *nested = Value::String(REDACTED_CONTENT.to_string());
                } else if is_path_key(&normalized) {
                    *nested = Value::String(REDACTED_PATH.to_string());
                } else {
                    sanitize_value(nested, depth + 1);
                }
            }
        }
        Value::Array(items) => {
            items.truncate(MAX_DIAGNOSTIC_ITEMS);
            for item in items {
                sanitize_value(item, depth + 1);
            }
        }
        Value::String(text) => {
            if contains_private_path(text) {
                *text = REDACTED_PATH.to_string();
            } else if text.chars().count() > MAX_DIAGNOSTIC_STRING_CHARS {
                *text = text.chars().take(MAX_DIAGNOSTIC_STRING_CHARS).collect();
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

pub(super) fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn is_secret_key(key: &str) -> bool {
    matches!(
        key,
        "accesstoken"
            | "refreshtoken"
            | "sessiontoken"
            | "token"
            | "password"
            | "passwd"
            | "credential"
            | "credentials"
            | "secret"
            | "apikey"
            | "authorization"
            | "privatekey"
            | "cookie"
    ) || key.ends_with("token")
        || key.ends_with("password")
        || key.ends_with("secret")
        || key.ends_with("credential")
}

pub(super) fn is_content_key(key: &str) -> bool {
    matches!(
        key,
        "content"
            | "contents"
            | "document"
            | "documents"
            | "documentdata"
            | "filedata"
            | "body"
            | "payload"
            | "bytes"
            | "raw"
    ) || key.ends_with("contents")
}

pub(super) fn is_path_key(key: &str) -> bool {
    key.contains("path")
        || matches!(
            key,
            "file" | "files" | "directory" | "folder" | "home" | "cwd" | "workingdirectory"
        )
}

pub(super) fn contains_private_path(text: &str) -> bool {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("file://")
        || trimmed.starts_with('/')
        || trimmed.starts_with("\\\\")
        || trimmed.starts_with("//")
        || contains_windows_drive_path(trimmed.as_bytes())
    {
        return true;
    }
    trimmed.split_ascii_whitespace().any(|part| {
        let part = part.trim_matches(|character: char| {
            matches!(character, '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';')
        });
        part.starts_with('/')
            || part.starts_with("\\\\")
            || part.to_ascii_lowercase().starts_with("file://")
            || contains_windows_drive_path(part.as_bytes())
    })
}

pub(super) fn contains_windows_drive_path(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|window| {
        window[0].is_ascii_alphabetic() && window[1] == b':' && matches!(window[2], b'\\' | b'/')
    })
}
