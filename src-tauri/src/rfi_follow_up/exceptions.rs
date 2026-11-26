//! RFIs the RFI Log records wrongly and that cannot be corrected there. Follow Up
//! sync leaves them alone (ADR 0059). The list is a preference kept locally and in
//! the cloud; it never authorizes a mutation or an Undo.

use super::*;

const MAX_EXCEPTION_TEXT_CHARS: usize = 20_000;
const MAX_EXCEPTIONS: usize = 2_000;

/// One edit of the exception list: `CODE` adds, `-CODE` removes.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct ExceptionEdit {
    pub add: BTreeSet<String>,
    pub remove: BTreeSet<String>,
}

fn invalid(token: &str) -> BackendError {
    BackendError::validation(format!("error.rfi.followUp.exceptionInvalid: {token}"))
}

/// Reads the operator's exception text. Codes may be full or short
/// (`CIV-1374`) and are stored in the canonical full form.
pub fn parse_exception_text(text: &str) -> Result<ExceptionEdit, BackendError> {
    if text.chars().count() > MAX_EXCEPTION_TEXT_CHARS {
        return Err(invalid("text too long"));
    }
    let mut edit = ExceptionEdit::default();
    for token in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        if token.is_empty() {
            continue;
        }
        let (target, code) = match token.strip_prefix('-') {
            Some(rest) => (&mut edit.remove, rest),
            None => (&mut edit.add, token),
        };
        let (canonical, _) = canonical_follow_up_rfi(code).ok_or_else(|| invalid(token))?;
        target.insert(canonical);
    }
    if let Some(code) = edit.add.intersection(&edit.remove).next() {
        return Err(invalid(code));
    }
    Ok(edit)
}

/// `(stored ∪ added) − removed`, bounded.
pub fn apply_exception_edit(
    stored: BTreeSet<String>,
    edit: &ExceptionEdit,
) -> Result<BTreeSet<String>, BackendError> {
    let mut merged = stored;
    merged.extend(edit.add.iter().cloned());
    merged.retain(|code| !edit.remove.contains(code));
    if merged.len() > MAX_EXCEPTIONS {
        return Err(invalid("too many exceptions"));
    }
    Ok(merged)
}
