use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tempfile::TempDir;

use crate::{
    cap_fs::{AnchoredPath, CapabilityRoot},
    print_queue::{FileSignature, PrintFailure},
};

#[cfg(test)]
use super::fixture_pdf_bytes;

pub(crate) struct StagedWorkbook {
    // These are Options because cleanup must close capability handles in this exact order
    // before TempDir attempts directory removal (required by Windows handle semantics).
    pub(super) workbook: Option<AnchoredPath>,
    pub(super) root: Option<CapabilityRoot>,
    pub(super) workspace: Option<TempDir>,
    pub(super) workbook_path: PathBuf,
    pub(super) cleanup_breaker: Arc<AtomicBool>,
    pub(super) cleanup_quarantine: Arc<Mutex<BTreeSet<PathBuf>>>,
    #[cfg(test)]
    pub(super) drop_events: Option<std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>>,
    #[cfg(test)]
    pub(super) cleanup_failure: bool,
}

impl StagedWorkbook {
    pub(super) fn stage(
        source_file: &mut File,
        source_path: &str,
        approved_signature: &FileSignature,
        cleanup_breaker: Arc<AtomicBool>,
        cleanup_quarantine: Arc<Mutex<BTreeSet<PathBuf>>>,
    ) -> Result<Self, PrintFailure> {
        let workspace = tempfile::Builder::new()
            .prefix("docflow-excel-")
            .tempdir()
            .map_err(|_| PrintFailure::failed("error.print.excelStagingUnavailable"))?;
        let mut staged = Self {
            workbook: None,
            root: None,
            workspace: Some(workspace),
            workbook_path: PathBuf::new(),
            cleanup_breaker,
            cleanup_quarantine,
            #[cfg(test)]
            drop_events: None,
            #[cfg(test)]
            cleanup_failure: false,
        };
        let root = CapabilityRoot::open(staged.workspace_path())
            .map_err(|_| PrintFailure::failed("error.print.excelStagingUnavailable"))?;
        staged.root = Some(root);
        let source_name = Path::new(source_path)
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| PrintFailure::failed("error.print.excelSourceNameUnavailable"))?;
        staged.workbook_path = staged.workspace_path().join(source_name);
        let workbook = staged
            .root
            .as_ref()
            .expect("live staged workbook owns its capability root")
            .authorize(&staged.workbook_path)
            .and_then(|path| path.anchor())
            .map_err(|_| PrintFailure::failed("error.print.excelStagingUnavailable"))?;
        staged.workbook = Some(workbook);
        let workbook = staged
            .workbook
            .as_ref()
            .expect("live staged workbook owns its workbook anchor");
        workbook
            .copy_no_clobber_from_file(source_file)
            .map_err(|_| PrintFailure::failed("error.print.excelStagingFailed"))?;
        if workbook
            .sha256()
            .map_err(|_| PrintFailure::failed("error.print.excelStagingVerificationFailed"))?
            != approved_signature.sha256.as_deref().unwrap_or_default()
        {
            return Err(PrintFailure::failed(
                "error.print.excelStagingVerificationFailed",
            ));
        }
        Ok(staged)
    }

    pub(crate) fn workbook_path(&self) -> &Path {
        &self.workbook_path
    }

    pub(super) fn cleanup_locator(&self) -> String {
        self.workspace
            .as_ref()
            .and_then(|workspace| workspace.path().file_name())
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("docflow-excel-cleanup")
            .to_string()
    }

    pub(crate) fn pdf_output_path(&self) -> PathBuf {
        self.workspace_path().join("worksheet.pdf")
    }

    pub(crate) fn quarantine_after_teardown_failure(&self) {
        self.cleanup_breaker.store(true, Ordering::Release);
    }

    pub(crate) fn into_exported_pdf(self) -> Result<ExportedWorksheetPdf, PrintFailure> {
        let pdf_path = self.pdf_output_path();
        let authorized_pdf = self
            .root
            .as_ref()
            .expect("live staged workbook owns its capability root")
            .authorize(&pdf_path)
            .map_err(|_| PrintFailure::failed("error.print.excelPrivatePdfUnavailable"))?;
        let pdf_identity = authorized_pdf
            .anchor()
            .and_then(|pdf| pdf.identity())
            .map_err(|_| PrintFailure::failed("error.print.excelPrivatePdfUnavailable"))?;
        let pdf = authorized_pdf
            .with_expected_identity(pdf_identity)
            .anchor()
            .map_err(|_| PrintFailure::failed("error.print.excelPrivatePdfUnavailable"))?;
        if pdf
            .len()
            .map_err(|_| PrintFailure::failed("error.print.excelPrivatePdfUnavailable"))?
            == 0
        {
            return Err(PrintFailure::failed("error.print.excelPrivatePdfEmpty"));
        }
        Ok(ExportedWorksheetPdf {
            pdf: Some(pdf),
            workspace: Some(self),
            validated_bytes: None,
        })
    }

    pub(super) fn cleanup(&mut self) -> io::Result<()> {
        if let Some(workbook) = self.workbook.take() {
            drop(workbook);
            self.record_drop("workbook");
        }
        if let Some(root) = self.root.take() {
            drop(root);
            self.record_drop("root");
        }
        let Some(workspace) = self.workspace.take() else {
            return Ok(());
        };
        self.record_drop("workspace");
        let workspace_path = workspace.path().to_path_buf();
        self.cleanup_quarantine
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(workspace_path.clone());
        #[cfg(test)]
        let result = if self.cleanup_failure {
            let retained_path = workspace.keep();
            debug_assert_eq!(retained_path, workspace_path);
            Err(io::Error::other(
                "injected private workspace cleanup failure",
            ))
        } else {
            workspace.close()
        };
        #[cfg(not(test))]
        let result = workspace.close();
        match result {
            Ok(()) => {
                self.cleanup_quarantine
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(&workspace_path);
                Ok(())
            }
            Err(error) => {
                self.cleanup_breaker.store(true, Ordering::Release);
                Err(error)
            }
        }
    }

    #[cfg(test)]
    pub(super) fn fixture_with_drop_probe()
    -> (Self, std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>) {
        let workspace = tempfile::tempdir().unwrap();
        let root = CapabilityRoot::open(workspace.path()).unwrap();
        let workbook_path = workspace.path().join("book.xlsx");
        let workbook = root.authorize(&workbook_path).unwrap().anchor().unwrap();
        workbook.write_no_clobber(b"workbook").unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            Self {
                workbook: Some(workbook),
                root: Some(root),
                workspace: Some(workspace),
                workbook_path,
                cleanup_breaker: Arc::new(AtomicBool::new(false)),
                cleanup_quarantine: Arc::new(Mutex::new(BTreeSet::new())),
                drop_events: Some(std::sync::Arc::clone(&events)),
                cleanup_failure: false,
            },
            events,
        )
    }

    #[cfg(test)]
    pub(super) fn fixture_exported_pdf(self) -> ExportedWorksheetPdf {
        let pdf_path = self.pdf_output_path();
        std::fs::write(&pdf_path, fixture_pdf_bytes(2)).unwrap();
        self.into_exported_pdf().unwrap()
    }

    #[cfg(test)]
    pub(super) fn fixture_exported_pdf_with_cleanup_failure(self) -> ExportedWorksheetPdf {
        let mut exported = self.fixture_exported_pdf();
        exported
            .workspace
            .as_mut()
            .expect("fixture PDF owns its staged workbook")
            .cleanup_failure = true;
        exported
    }

    pub(super) fn workspace_path(&self) -> &Path {
        self.workspace
            .as_ref()
            .expect("live staged workbook owns its workspace")
            .path()
    }

    #[cfg(test)]
    pub(super) fn record_drop(&self, event: &'static str) {
        if let Some(events) = self.drop_events.as_ref() {
            events.lock().unwrap().push(event);
        }
    }

    #[cfg(not(test))]
    pub(super) fn record_drop(&self, _event: &'static str) {}
}

impl Drop for StagedWorkbook {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

pub(crate) struct ExportedWorksheetPdf {
    // The PDF anchor closes before StagedWorkbook closes the workbook/root/workspace.
    pub(super) pdf: Option<AnchoredPath>,

    pub(super) workspace: Option<StagedWorkbook>,
    pub(super) validated_bytes: Option<Arc<[u8]>>,
}

impl ExportedWorksheetPdf {
    pub(super) fn validate_for_submission(&mut self) -> Result<(), PrintFailure> {
        let mut source = self
            .pdf
            .as_ref()
            .ok_or_else(|| PrintFailure::failed("error.print.excelExportMalformedPdf"))?
            .open_read()
            .map_err(|_| PrintFailure::failed("error.print.excelExportMalformedPdf"))?;
        let declared_len = source
            .metadata()
            .map_err(|_| PrintFailure::failed("error.print.excelExportMalformedPdf"))?
            .len();
        let bytes = read_pdf_bytes_bounded(&mut source, declared_len, crate::pdf::MAX_PDF_BYTES)?;
        validate_pdf_envelope(&bytes)?;
        self.validated_bytes = Some(bytes);
        Ok(())
    }

    pub(crate) fn validated_bytes(&self) -> Result<&[u8], PrintFailure> {
        self.validated_bytes
            .as_deref()
            .ok_or_else(|| PrintFailure::failed("error.print.excelExportMalformedPdf"))
    }

    pub(super) fn cleanup_locator(&self) -> String {
        self.workspace.as_ref().map_or_else(
            || "docflow-excel-cleanup".to_string(),
            StagedWorkbook::cleanup_locator,
        )
    }

    pub(super) fn cleanup(&mut self) -> io::Result<()> {
        if let Some(pdf) = self.pdf.take() {
            drop(pdf);
            if let Some(workspace) = self.workspace.as_ref() {
                workspace.record_drop("pdf");
            }
        }
        let Some(mut workspace) = self.workspace.take() else {
            return Ok(());
        };
        workspace.cleanup()
    }
}

pub(super) fn validate_pdf_envelope(bytes: &[u8]) -> Result<(), PrintFailure> {
    let tail = &bytes[bytes.len().saturating_sub(1_024)..];
    if !bytes.starts_with(b"%PDF-")
        || !tail.windows(b"%%EOF".len()).any(|part| part == b"%%EOF")
        || !bytes
            .windows(b"startxref".len())
            .any(|part| part == b"startxref")
    {
        return Err(PrintFailure::failed("error.print.excelExportMalformedPdf"));
    }
    Ok(())
}

pub(super) fn read_pdf_bytes_bounded(
    reader: impl Read,
    declared_len: u64,
    max_bytes: u64,
) -> Result<Arc<[u8]>, PrintFailure> {
    if declared_len == 0 || declared_len > max_bytes {
        return Err(PrintFailure::failed("error.print.excelExportMalformedPdf"));
    }
    let capacity = usize::try_from(declared_len)
        .map_err(|_| PrintFailure::failed("error.print.excelExportMalformedPdf"))?;
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| PrintFailure::failed("error.print.excelExportMalformedPdf"))?;
    if bytes.len() as u64 != declared_len || bytes.len() as u64 > max_bytes {
        return Err(PrintFailure::failed("error.print.excelExportMalformedPdf"));
    }
    Ok(Arc::from(bytes))
}

impl Drop for ExportedWorksheetPdf {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
