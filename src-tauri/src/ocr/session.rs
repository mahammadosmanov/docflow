use super::*;

impl PdfOcrSession {
    #[cfg(target_os = "macos")]
    pub(super) fn open(path: &Path) -> Result<Self, OcrFailure> {
        use std::{os::unix::ffi::OsStrExt, ptr::NonNull};

        unsafe extern "C" {
            fn docflow_pdf_session_open(path: *const std::ffi::c_char) -> *mut std::ffi::c_void;
        }
        let path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| OcrFailure::Document("PDF path contains NUL".to_string()))?;
        let document = unsafe { docflow_pdf_session_open(path.as_ptr()) };
        Ok(Self {
            document: NonNull::new(document)
                .ok_or_else(|| OcrFailure::Document("PDFKit could not open the PDF".into()))?,
        })
    }

    #[cfg(target_os = "windows")]
    pub(super) fn open(path: &Path) -> Result<Self, OcrFailure> {
        let document =
            crate::windows_pdf::InMemoryPdf::open_path(path).map_err(OcrFailure::Document)?;
        Ok(Self { document })
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(super) fn open(_path: &Path) -> Result<Self, OcrFailure> {
        Ok(Self {})
    }

    #[cfg(target_os = "macos")]
    pub(super) fn page_text(&self, _path: &Path, page_index: usize) -> Result<String, OcrFailure> {
        use std::ffi::{CStr, c_char};

        unsafe extern "C" {
            fn docflow_pdf_session_page_text(
                document: *mut std::ffi::c_void,
                page_index: usize,
                output: *mut *mut c_char,
            ) -> i32;
            fn docflow_pdfkit_free(value: *mut c_char);
        }
        let mut output = std::ptr::null_mut();
        let status = unsafe {
            docflow_pdf_session_page_text(self.document.as_ptr(), page_index, &mut output)
        };
        if status != 0 || output.is_null() {
            return Err(OcrFailure::Document(format!(
                "PDFKit could not read page {} ({status})",
                page_index + 1
            )));
        }
        let text = unsafe { CStr::from_ptr(output) }
            .to_string_lossy()
            .into_owned();
        unsafe { docflow_pdfkit_free(output) };
        Ok(text)
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn page_text(&self, path: &Path, page_index: usize) -> Result<String, OcrFailure> {
        page_text(path, page_index)
    }

    #[cfg(target_os = "macos")]
    pub(super) fn render_page_png(
        &self,
        _path: &Path,
        output: &Path,
        dpi: i32,
        page_index: usize,
    ) -> Result<(), String> {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};

        unsafe extern "C" {
            fn docflow_pdf_session_render_page(
                document: *mut std::ffi::c_void,
                output: *const std::ffi::c_char,
                dpi: i32,
                page_index: usize,
            ) -> i32;
        }
        let output = CString::new(output.as_os_str().as_bytes())
            .map_err(|_| "image path contains NUL".to_string())?;
        let status = unsafe {
            docflow_pdf_session_render_page(
                self.document.as_ptr(),
                output.as_ptr(),
                dpi,
                page_index,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(format!("PDFKit render failed ({status})"))
        }
    }

    #[cfg(target_os = "windows")]
    pub(super) fn render_page_png(
        &self,
        _path: &Path,
        output: &Path,
        dpi: i32,
        page_index: usize,
    ) -> Result<(), String> {
        let page_index = u32::try_from(page_index)
            .map_err(|_| "Windows PDF page index is out of range".to_string())?;
        let bytes = self.document.render_page_png_bytes(dpi, page_index)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|error| format!("OCR PNG create: {error}"))?;
        use std::io::Write;
        file.write_all(&bytes)
            .and_then(|()| file.flush())
            .map_err(|error| format!("OCR PNG write: {error}"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(super) fn render_page_png(
        &self,
        path: &Path,
        output: &Path,
        dpi: i32,
        page_index: usize,
    ) -> Result<(), String> {
        render_page_png(path, output, dpi, page_index)
    }
}

#[cfg(target_os = "macos")]
impl Drop for PdfOcrSession {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn docflow_pdf_session_close(document: *mut std::ffi::c_void);
        }
        unsafe { docflow_pdf_session_close(self.document.as_ptr()) };
    }
}
