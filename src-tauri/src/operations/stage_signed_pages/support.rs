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
