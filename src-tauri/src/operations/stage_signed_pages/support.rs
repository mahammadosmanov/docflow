use super::*;

#[derive(Clone, Copy)]
pub(super) enum PdfRole {
    Archive,
    SignedScan,
}

pub(super) fn fidelity_supported(facts: &PdfInspection, role: PdfRole) -> bool {
    !facts.encrypted
        && !facts.signed
        && !facts.catalog_entry_hashes.contains_key("StructTreeRoot")
        && (!matches!(role, PdfRole::SignedScan)
            || !facts.catalog_entry_hashes.contains_key("OCProperties"))
}

pub(super) fn fidelity_reason(facts: &PdfInspection, role: PdfRole) -> &'static str {
    if facts.encrypted {
        "encrypted PDF"
    } else if facts.signed {
        "digitally signed PDF"
    } else if facts.catalog_entry_hashes.contains_key("StructTreeRoot") {
        "tagged PDF page structure cannot be safely preserved"
    } else if matches!(role, PdfRole::SignedScan)
        && facts.catalog_entry_hashes.contains_key("OCProperties")
    {
        "signed page depends on document-level optional-content state"
    } else {
        "unsupported PDF fidelity case"
    }
}

pub(super) fn warning(key: &str, subject: &str) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.to_string(),
    }
}

pub(super) fn candidate_display(path: &Path, page_index: usize) -> String {
    format!("{} — #{}", display_name(path), page_index + 1)
}

pub(super) fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// Builds the signed-page review editor metadata: archive choices per code and the row list.
pub(super) fn signed_editor_meta(
    archive_choices: JsonObject,
    archive_choice_labels: JsonObject,
    editor_rows: Vec<Value>,
) -> Value {
    json!({
        "titleKey": "rfi.signed.editor.title",
        "rowCountKey": "rfi.signed.editor.rowCount",
        "groupField": "code",
        "selectionEnabled": false,
        "bulkEnabled": false,
        "duplicateEnabled": false,
        "requiredFields": ["archiveFile"],
        "choicesByValue": {
            "archiveFile": {
                "dependsOn": "code",
                "values": archive_choices,
            }
        },
        "choiceLabels": {
            "archiveFile": archive_choice_labels
        },
        "columns": [
            {"key": "enabled", "labelKey": "rfi.signed.column.enabled", "kind": "bool"},
            {"key": "code", "labelKey": "rfi.signed.column.code", "kind": "readonly"},
            {"key": "confidence", "labelKey": "ocr.confidence", "kind": "readonly", "display": "percent", "width": 112},
            {"key": "signedFile", "labelKey": "rfi.signed.column.scan", "kind": "readonly"},
            {"key": "archiveFile", "labelKey": "rfi.signed.column.archive", "kind": "choice", "bulk": false}
        ],
        "rows": editor_rows,
    })
}
