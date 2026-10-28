use super::*;

pub(super) struct CachedPageOcr {
    pub(super) reads:
        BTreeMap<(PathBuf, usize), Result<crate::operations::process_scans::OcrRead, OcrFailure>>,
    pub(super) page_previews: bool,
    pub(super) diagnostics: String,
}

impl OcrAdapter for CachedPageOcr {
    fn read(&self, path: &Path) -> Result<crate::operations::process_scans::OcrRead, OcrFailure> {
        self.read_page(path, 0)
    }

    fn supports_page_scan(&self) -> bool {
        true
    }

    fn supports_page_previews(&self) -> bool {
        self.page_previews
    }

    fn read_page(
        &self,
        path: &Path,
        page_index: usize,
    ) -> Result<crate::operations::process_scans::OcrRead, OcrFailure> {
        self.reads
            .get(&(path.to_path_buf(), page_index))
            .cloned()
            .unwrap_or_else(|| {
                Err(OcrFailure::Document(
                    "parallel page OCR cache entry is missing".to_string(),
                ))
            })
    }

    fn diagnostics(&self) -> String {
        self.diagnostics.clone()
    }
}
