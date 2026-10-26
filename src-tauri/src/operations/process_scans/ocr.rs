use std::path::Path;

use crate::operations::process_scans::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OcrFailure {
    EngineMissing,
    Document(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct OcrRead {
    pub matched: RfiMatch,
    pub source: String,
}

pub trait OcrAdapter: Sync {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure>;
    fn read_cover_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.read_page(path, page_index)
    }
    /// Reads the RFI identity of a page that is expected to be a form cover.
    /// Adapters may answer from a cheap cover read; the default is the full reader.
    fn read_identity_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.read_page(path, page_index)
    }
    fn supports_page_scan(&self) -> bool {
        false
    }
    fn inspect_pdf(&self, _path: &Path) -> Option<Result<crate::pdf::PdfInspection, String>> {
        None
    }
    fn supports_page_previews(&self) -> bool {
        false
    }
    fn supports_parallel_analysis(&self) -> bool {
        false
    }
    fn supports_orientation(&self) -> bool {
        false
    }
    fn detect_orientation(
        &self,
        _path: &Path,
        _page_index: usize,
    ) -> Result<crate::operations::pdf_orientation::OrientationDetection, String> {
        Err("page orientation detection is unavailable".to_string())
    }
    fn read_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        if page_index == 0 {
            self.read(path)
        } else {
            Err(OcrFailure::Document(
                "page-level OCR is unavailable".to_string(),
            ))
        }
    }
    fn diagnostics(&self) -> String;
}
