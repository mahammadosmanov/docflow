use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrintFailureKind {
    Failed,
    Unsupported,
    UnsupportedOptions,
    SubmittedUnconfirmed,
    Uncertain,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetryDisposition {
    Retryable,
    #[cfg(any(target_os = "windows", test))]
    CleanupQuarantined,
    Never,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CleanupReview {
    pub(crate) warning_key: &'static str,
    pub(crate) locator: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PrintSubmissionOutcome {
    pub(crate) cleanup_review: Option<CleanupReview>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintFailure {
    pub kind: PrintFailureKind,
    pub message: String,
    pub uncertain: bool,
    pub(super) diagnostic_detail: Option<String>,
    retry_disposition: RetryDisposition,
    pub(crate) cleanup_review: Option<CleanupReview>,
}

impl PrintFailure {
    fn new(
        kind: PrintFailureKind,
        message: impl Into<String>,
        uncertain: bool,
        retry_disposition: RetryDisposition,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            uncertain,
            diagnostic_detail: None,
            retry_disposition,
            cleanup_review: None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(
            PrintFailureKind::Failed,
            message,
            false,
            RetryDisposition::Retryable,
        )
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(
            PrintFailureKind::Unsupported,
            message,
            false,
            RetryDisposition::Never,
        )
    }

    pub fn unsupported_options(message: impl Into<String>) -> Self {
        Self::new(
            PrintFailureKind::UnsupportedOptions,
            message,
            false,
            RetryDisposition::Never,
        )
    }

    pub fn uncertain(message: impl Into<String>) -> Self {
        Self::new(
            PrintFailureKind::Uncertain,
            message,
            true,
            RetryDisposition::Never,
        )
    }

    pub fn submitted_unconfirmed(message: impl Into<String>) -> Self {
        Self::new(
            PrintFailureKind::SubmittedUnconfirmed,
            message,
            true,
            RetryDisposition::Never,
        )
    }

    pub fn cancelled(message: impl Into<String>, uncertain: bool) -> Self {
        Self::new(
            PrintFailureKind::Cancelled,
            message,
            uncertain,
            RetryDisposition::Never,
        )
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn with_cleanup_review(mut self, cleanup_review: CleanupReview) -> Self {
        self.cleanup_review = Some(cleanup_review);
        self
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn has_cleanup_review(&self) -> bool {
        self.cleanup_review.is_some()
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn with_retry_disposition(mut self, disposition: RetryDisposition) -> Self {
        self.retry_disposition = disposition;
        self
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn with_diagnostic(mut self, detail: impl Into<String>) -> Self {
        self.diagnostic_detail = Some(redact_error(&detail.into()));
        self
    }

    pub fn is_unsupported(&self) -> bool {
        self.kind == PrintFailureKind::Unsupported
    }

    pub fn is_unsupported_options(&self) -> bool {
        self.kind == PrintFailureKind::UnsupportedOptions
    }

    pub fn into_backend_error(self) -> BackendError {
        match self.kind {
            PrintFailureKind::Unsupported | PrintFailureKind::UnsupportedOptions => {
                BackendError::validation(self.message)
            }
            _ => BackendError::request_failed(self.message),
        }
    }
}

pub(super) fn retry_disposition_allows_retry(failure: &PrintFailure) -> bool {
    failure.retry_disposition == RetryDisposition::Retryable
}

pub(super) fn submission_primary_classification(
    submitted: &Result<PrintSubmissionOutcome, PrintFailure>,
) -> &'static str {
    match submitted {
        Ok(_) => "completed",
        Err(failure) => match failure.kind {
            PrintFailureKind::Failed => "failed",
            PrintFailureKind::Unsupported => "unsupported",
            PrintFailureKind::UnsupportedOptions => "unsupported_options",
            PrintFailureKind::SubmittedUnconfirmed => "submitted_unconfirmed",
            PrintFailureKind::Uncertain => "uncertain",
            PrintFailureKind::Cancelled if failure.uncertain => "cancelled_uncertain",
            PrintFailureKind::Cancelled => "cancelled",
        },
    }
}
