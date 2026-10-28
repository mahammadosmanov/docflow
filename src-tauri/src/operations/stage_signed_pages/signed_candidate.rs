use super::*;

/// Inspects one candidate's signed scan and returns its facts when the page can be used.
/// Otherwise records a review reason for the candidate and returns `None`.
pub(super) fn usable_signed_facts(
    candidate: &Candidate,
    code: &str,
    signed_root: &Path,
    plan: &mut Plan,
    warning_reviews: &mut BTreeMap<PathBuf, Vec<WarningReviewReason>>,
) -> Option<PdfInspection> {
    let facts = match inspect(&candidate.path) {
        Ok(facts) if fidelity_supported(&facts, PdfRole::SignedScan) => facts,
        Ok(facts) => {
            push_warning_review(
                plan,
                warning_reviews,
                signed_root,
                "warn.rfi.pdfFidelityUnsupported",
                &candidate.path,
                Some(candidate.page_index),
                Some(code),
                fidelity_reason(&facts, PdfRole::SignedScan),
            );
            return None;
        }
        Err(error) => {
            push_warning_review(
                plan,
                warning_reviews,
                signed_root,
                "warn.rfi.pdfFidelityUnsupported",
                &candidate.path,
                Some(candidate.page_index),
                Some(code),
                &error.to_string(),
            );
            return None;
        }
    };
    if facts.page_count == 0 {
        push_warning_review(
            plan,
            warning_reviews,
            signed_root,
            "warn.rfi.pdfFidelityUnsupported",
            &candidate.path,
            Some(candidate.page_index),
            Some(code),
            "PDF has no pages",
        );
        return None;
    }
    if facts
        .pages
        .get(candidate.page_index)
        .is_none_or(|page| page.widgets > 0)
    {
        push_warning_review(
            plan,
            warning_reviews,
            signed_root,
            "warn.rfi.pdfFidelityUnsupported",
            &candidate.path,
            Some(candidate.page_index),
            Some(code),
            "The signed page contains form widgets and requires manual review",
        );
        return None;
    }
    Some(facts)
}
