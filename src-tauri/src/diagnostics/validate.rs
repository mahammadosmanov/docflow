use super::*;

pub(crate) fn validate_bundle_content(content: &str) -> Result<(), BackendError> {
    let invalid = || BackendError::validation("diagnostics bundle is invalid or was changed");
    if content.is_empty() || content.len() > MAX_BUNDLE_BYTES || !content.ends_with('\n') {
        return Err(invalid());
    }
    let value = serde_json::from_str::<Value>(content).map_err(|_| invalid())?;
    let mut sanitized = value.clone();
    sanitize_value(&mut sanitized, 0);
    if sanitized != value || !is_bundle_shape(&value) {
        return Err(invalid());
    }
    let canonical = format!(
        "{}\n",
        serde_json::to_string_pretty(&value).map_err(|_| invalid())?
    );
    if canonical != content {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn is_bundle_shape(value: &Value) -> bool {
    let Some(root) = exact_object(
        value,
        &[
            "components",
            "docflow",
            "platform",
            "privacy",
            "recentErrorCodes",
            "schemaVersion",
        ],
    ) else {
        return false;
    };
    if root.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return false;
    }
    let Some(docflow) = root
        .get("docflow")
        .and_then(|value| exact_object(value, &["version"]))
    else {
        return false;
    };
    if !docflow
        .get("version")
        .and_then(Value::as_str)
        .is_some_and(is_bounded_safe_text)
    {
        return false;
    }
    let Some(platform) = root
        .get("platform")
        .and_then(|value| exact_object(value, &["arch", "os"]))
    else {
        return false;
    };
    if ["arch", "os"].into_iter().any(|key| {
        !platform
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(is_bounded_safe_text)
    }) {
        return false;
    }
    let Some(components) = root
        .get("components")
        .and_then(|value| exact_object(value, &["ocr", "printer", "sidecar"]))
    else {
        return false;
    };
    let Some(ocr) = components
        .get("ocr")
        .and_then(|value| exact_object(value, &["engine", "languageDataAvailable", "status"]))
    else {
        return false;
    };
    if ocr.get("engine").and_then(Value::as_str) != Some("tesseract")
        || !ocr
            .get("languageDataAvailable")
            .is_some_and(Value::is_boolean)
        || !matches!(
            ocr.get("status").and_then(Value::as_str),
            Some("available" | "missing")
        )
    {
        return false;
    }
    let Some(sidecar) = components
        .get("sidecar")
        .and_then(|value| exact_object(value, &["required", "status"]))
    else {
        return false;
    };
    if sidecar.get("required").and_then(Value::as_bool) != Some(false)
        || sidecar.get("status").and_then(Value::as_str) != Some("retired")
    {
        return false;
    }
    if !is_printer_shape(components.get("printer")) {
        return false;
    }
    let Some(errors) = root.get("recentErrorCodes").and_then(Value::as_array) else {
        return false;
    };
    if errors.len() > MAX_RECENT_ERROR_CODES
        || errors.iter().any(|value| {
            value
                .as_str()
                .is_none_or(|code| sanitize_error_code(code) != code)
        })
    {
        return false;
    }
    let Some(privacy) = root
        .get("privacy")
        .and_then(|value| exact_object(value, &["delivery", "redaction"]))
    else {
        return false;
    };
    privacy.get("delivery").and_then(Value::as_str) == Some("local-only")
        && privacy.get("redaction").and_then(Value::as_str) == Some("recursive")
}

pub(super) fn exact_object<'a>(
    value: &'a Value,
    expected_keys: &[&str],
) -> Option<&'a serde_json::Map<String, Value>> {
    let object = value.as_object()?;
    (object.len() == expected_keys.len()
        && expected_keys.iter().all(|key| object.contains_key(*key)))
    .then_some(object)
}

pub(super) fn is_bounded_safe_text(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_DIAGNOSTIC_STRING_CHARS
        && !contains_private_path(text)
}

pub(super) fn is_printer_shape(value: Option<&Value>) -> bool {
    let Some(printer) = value.and_then(|value| {
        exact_object(
            value,
            &[
                "defaultPrinterAvailable",
                "inventoryStatus",
                "latestQueue",
                "printerCount",
            ],
        )
    }) else {
        return false;
    };
    if !matches!(
        printer.get("inventoryStatus").and_then(Value::as_str),
        Some("available" | "unavailable")
    ) || printer
        .get("printerCount")
        .and_then(Value::as_u64)
        .is_none()
        || !printer
            .get("defaultPrinterAvailable")
            .is_some_and(Value::is_boolean)
    {
        return false;
    }
    let Some(queue) = printer.get("latestQueue") else {
        return false;
    };
    if queue.is_null() {
        return true;
    }
    let Some(queue) = exact_object(
        queue,
        &["counts", "fileCount", "jobs", "printer", "queueStatus"],
    ) else {
        return false;
    };
    if ["printer", "queueStatus"].into_iter().any(|key| {
        queue
            .get(key)
            .and_then(Value::as_str)
            .is_none_or(|value| !is_bounded_safe_text(value))
    }) || queue.get("fileCount").and_then(Value::as_u64).is_none()
    {
        return false;
    }
    let Some(counts) = queue.get("counts").and_then(Value::as_object) else {
        return false;
    };
    if counts.len() > MAX_DIAGNOSTIC_ITEMS
        || counts
            .iter()
            .any(|(key, value)| !is_bounded_safe_text(key) || value.as_u64().is_none())
    {
        return false;
    }
    let Some(jobs) = queue.get("jobs").and_then(Value::as_array) else {
        return false;
    };
    jobs.len() <= MAX_DIAGNOSTIC_ITEMS
        && jobs.iter().all(|job| {
            let Some(job) = exact_object(
                job,
                &[
                    "attempts",
                    "color",
                    "duplex",
                    "errorDetail",
                    "format",
                    "orientation",
                    "pageRange",
                    "paper",
                    "route",
                    "scaling",
                    "status",
                ],
            ) else {
                return false;
            };
            job.get("attempts").and_then(Value::as_u64).is_some()
                && job.iter().all(|(key, value)| {
                    key == "attempts"
                        || value
                            .as_str()
                            .is_some_and(|text| text.is_empty() || is_bounded_safe_text(text))
                })
        })
}
