//! RFIs the RFI Log records wrongly and that cannot be corrected there. Follow Up
//! sync leaves them alone (ADR 0059).

use super::*;

const MAX_EXCEPTION_TEXT_CHARS: usize = 20_000;
pub(crate) const MAX_EXCEPTIONS: usize = 2_000;
pub(crate) const SHARED_EXCEPTIONS_UNAVAILABLE_WARN: &str =
    "warn.rfi.followUp.exceptionCloudUnavailable";

pub(crate) fn parse_exception_code(value: &str) -> Result<String, BackendError> {
    if value.len() <= 64
        && let Some((code, _)) = canonical_follow_up_rfi(value)
    {
        return Ok(code);
    }
    Err(BackendError::validation(
        "error.rfi.followUp.exceptionInvalid",
    ))
}

pub(crate) fn extend_exception_codes(
    mut codes: BTreeSet<String>,
    text: &str,
) -> Result<BTreeSet<String>, BackendError> {
    if text.chars().count() > MAX_EXCEPTION_TEXT_CHARS {
        return Err(BackendError::validation(
            "error.rfi.followUp.exceptionInvalid",
        ));
    }
    for token in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        if !token.is_empty() {
            codes.insert(parse_exception_code(token)?);
        }
    }
    if codes.len() > MAX_EXCEPTIONS {
        return Err(BackendError::validation(
            "error.rfi.followUp.exceptionLimit",
        ));
    }
    Ok(codes)
}
