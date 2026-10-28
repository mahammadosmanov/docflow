use std::sync::Arc;

#[cfg(any(target_os = "windows", test))]
use std::collections::HashSet;

use crate::{outlook_reply_draft::OutlookAdapter, print_queue::PrintAdapter};

mod startup;
pub(crate) use startup::set_start_with_windows;

#[cfg(not(target_os = "windows"))]
use crate::outlook_reply_draft::{
    DraftIdentity, MailItemIdentity, OutlookDraft, OutlookFailure, SelectedMailItem,
};

#[cfg(any(target_os = "windows", test))]
pub(crate) mod controlled_submit;
#[cfg(any(target_os = "windows", test))]
pub(crate) mod excel;
#[cfg(any(target_os = "windows", test))]
pub(crate) mod excel_windows_contract;
#[cfg(any(target_os = "windows", test))]
mod windows_submission;
#[cfg(any(target_os = "windows", test))]
pub(crate) use windows_submission::*;

#[cfg(any(target_os = "windows", test))]
pub(crate) fn consume_controlled_page_bytes<T>(
    render: impl FnOnce() -> Result<Vec<u8>, crate::print_queue::PrintFailure>,
    consume: impl FnOnce(&[u8]) -> Result<T, crate::print_queue::PrintFailure>,
) -> Result<T, crate::print_queue::PrintFailure> {
    let rendered = render()?;
    consume(&rendered)
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn map_controlled_print_failure(
    mut failure: crate::print_queue::PrintFailure,
) -> crate::print_queue::PrintFailure {
    if failure.message.starts_with("error.print.") {
        return failure;
    }
    failure.message = match failure.kind {
        crate::print_queue::PrintFailureKind::Failed => "error.print.excelPrintPreparationFailed",
        crate::print_queue::PrintFailureKind::Unsupported => {
            "error.print.excelControlledPdfUnavailable"
        }
        crate::print_queue::PrintFailureKind::UnsupportedOptions => {
            "error.print.excelPrintOptionsRejected"
        }
        crate::print_queue::PrintFailureKind::SubmittedUnconfirmed => {
            "error.print.excelPrintCompletionUnconfirmed"
        }
        crate::print_queue::PrintFailureKind::Uncertain => {
            "error.print.excelPrintCompletionUnconfirmed"
        }
        crate::print_queue::PrintFailureKind::Cancelled if failure.uncertain => {
            "error.print.excelPrintCancelledUnconfirmed"
        }
        crate::print_queue::PrintFailureKind::Cancelled => "error.print.excelExportCancelled",
    }
    .to_string();
    failure
}

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub fn production_print_adapter() -> Arc<dyn PrintAdapter> {
    Arc::new(macos::printing::MacOsPrintAdapter)
}

#[cfg(target_os = "windows")]
pub fn production_print_adapter() -> Arc<dyn PrintAdapter> {
    Arc::new(windows::printing::WindowsPrintAdapter)
}

#[cfg(target_os = "windows")]
pub fn production_outlook_adapter() -> Arc<dyn OutlookAdapter> {
    Arc::new(windows::outlook::WindowsOutlookAdapter)
}

#[cfg(not(target_os = "windows"))]
pub fn production_outlook_adapter() -> Arc<dyn OutlookAdapter> {
    Arc::new(UnsupportedOutlookAdapter)
}

#[cfg(not(target_os = "windows"))]
struct UnsupportedOutlookAdapter;

#[cfg(not(target_os = "windows"))]
impl OutlookAdapter for UnsupportedOutlookAdapter {
    fn selected_mail_item(&self) -> Result<SelectedMailItem, OutlookFailure> {
        Err(outlook_unsupported())
    }

    fn reply_all(
        &self,
        _expected: &MailItemIdentity,
    ) -> Result<Box<dyn OutlookDraft>, OutlookFailure> {
        Err(outlook_unsupported())
    }

    fn delete_unchanged_draft(&self, _draft: &DraftIdentity) -> Result<(), OutlookFailure> {
        Err(outlook_unsupported())
    }

    fn export_selected_msg(
        &self,
        _expected: &MailItemIdentity,
        _expected_fingerprint: &str,
    ) -> Result<Vec<u8>, OutlookFailure> {
        Err(outlook_unsupported())
    }
}

#[cfg(not(target_os = "windows"))]
fn outlook_unsupported() -> OutlookFailure {
    OutlookFailure::unsupported("error.rfi.outlookDraft.unsupportedPlatform")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn production_print_adapter() -> Arc<dyn PrintAdapter> {
    Arc::new(UnsupportedPrintAdapter)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct UnsupportedPrintAdapter;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl PrintAdapter for UnsupportedPrintAdapter {
    fn inventory(
        &self,
    ) -> Result<crate::print_queue::PrinterInventory, crate::print_queue::PrintFailure> {
        Ok(crate::print_queue::PrinterInventory::default())
    }

    fn inspect(
        &self,
        _path: &std::path::Path,
        _printer: &str,
        _settings: &crate::print_queue::PrintSettings,
    ) -> Result<crate::print_queue::PrintJobSupport, crate::print_queue::PrintFailure> {
        Err(crate::print_queue::PrintFailure::unsupported(
            "printing is unsupported on this platform",
        ))
    }

    fn submit(
        &self,
        _job: &crate::print_queue::PrintJob,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<(), crate::print_queue::PrintFailure> {
        Err(crate::print_queue::PrintFailure::failed(
            "printing is unsupported on this platform",
        ))
    }
}

#[cfg(test)]
mod tests;
