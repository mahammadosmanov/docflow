use std::{os::windows::ffi::OsStrExt, path::Path};

use windows::{
    Data::Pdf::{PdfDocument, PdfPageRenderOptions},
    Storage::{
        CreationCollisionOption, FileAccessMode, StorageFolder,
        Streams::{DataReader, IRandomAccessStream, InMemoryRandomAccessStream},
    },
    Win32::{
        System::{
            Com::IStream,
            WinRT::{
                BSOS_DEFAULT, CreateRandomAccessStreamOnFile, CreateRandomAccessStreamOverStream,
                RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
            },
        },
        UI::Shell::SHCreateMemStream,
    },
    core::{HRESULT, HSTRING},
};

const RPC_E_CHANGED_MODE: HRESULT = HRESULT(0x8001_0106_u32 as i32);
const MAX_RENDER_EDGE: u32 = 20_000;
const MAX_RENDER_PIXELS: u64 = 100_000_000;

struct RuntimeGuard(bool);

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { RoUninitialize() };
        }
    }
}

pub(crate) struct InMemoryPdf {
    document: PdfDocument,
    _random_access_stream: IRandomAccessStream,
    _memory_stream: IStream,
    _runtime: RuntimeGuard,
}

impl InMemoryPdf {
    pub(crate) fn load(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > u32::MAX as usize {
            return Err("Windows PDF byte source is empty or oversized".to_string());
        }
        let runtime = initialize_runtime()?;
        let memory_stream = unsafe { SHCreateMemStream(Some(bytes)) }
            .ok_or_else(|| "Windows could not create the PDF memory stream".to_string())?;
        let random_access_stream: IRandomAccessStream =
            unsafe { CreateRandomAccessStreamOverStream(&memory_stream, BSOS_DEFAULT) }.map_err(
                |error| format!("Windows could not adapt the PDF memory stream: {error}"),
            )?;
        let document = PdfDocument::LoadFromStreamAsync(&random_access_stream)
            .and_then(|operation| operation.get())
            .map_err(|error| {
                format!("Windows could not load the PDF from validated bytes: {error}")
            })?;
        Ok(Self {
            document,
            _random_access_stream: random_access_stream,
            _memory_stream: memory_stream,
            _runtime: runtime,
        })
    }

    pub(crate) fn page_count(&self) -> Result<u32, String> {
        document_page_count(&self.document)
    }

    pub(crate) fn render_page_png_bytes(
        &self,
        dpi: i32,
        page_index: u32,
    ) -> Result<Vec<u8>, String> {
        render_document_page_png_bytes(&self.document, dpi, page_index)
    }
}

pub fn render_first_page_png(input: &Path, output: &Path, dpi: i32) -> Result<(), String> {
    render_page_png(input, output, dpi, 0).map(|_| ())
}

pub fn page_count(input: &Path) -> Result<u32, String> {
    let _runtime = initialize_runtime()?;
    let (document, _input_stream) = load_document(input)?;
    document_page_count(&document)
}

fn document_page_count(document: &PdfDocument) -> Result<u32, String> {
    let count = document
        .PageCount()
        .map_err(|error| format!("Windows could not inspect the PDF: {error}"))?;
    if count == 0 {
        return Err("Windows PDF has no pages".to_string());
    }
    Ok(count)
}

pub fn render_page_png(
    input: &Path,
    output: &Path,
    dpi: i32,
    page_index: u32,
) -> Result<(u32, u32), String> {
    if !(72..=600).contains(&dpi) {
        return Err("Windows PDF render DPI must be between 72 and 600".to_string());
    }
    let _runtime = initialize_runtime()?;

    let hstring =
        |value: &std::ffi::OsStr| HSTRING::from_wide(&value.encode_wide().collect::<Vec<_>>());
    let (document, _input_stream) = load_document(input)?;
    let page_count = document
        .PageCount()
        .map_err(|error| format!("Windows could not inspect the PDF: {error}"))?;
    if page_index >= page_count {
        return Err("Windows PDF page index is out of range".to_string());
    }
    let page = document
        .GetPage(page_index)
        .map_err(|error| format!("Windows could not open the PDF page: {error}"))?;
    let size = page
        .Size()
        .map_err(|error| format!("Windows could not measure the PDF page: {error}"))?;
    let scale = f64::from(dpi) / 96.0;
    let width = (f64::from(size.Width) * scale).ceil();
    let height = (f64::from(size.Height) * scale).ceil();
    if !width.is_finite()
        || !height.is_finite()
        || width < 1.0
        || height < 1.0
        || width > f64::from(MAX_RENDER_EDGE)
        || height > f64::from(MAX_RENDER_EDGE)
        || (width * height) > MAX_RENDER_PIXELS as f64
    {
        let _ = page.Close();
        return Err("Windows PDF page exceeds the render limit".to_string());
    }

    let options = PdfPageRenderOptions::new()
        .map_err(|error| format!("Windows could not create PDF render options: {error}"))?;
    options
        .SetDestinationWidth(width as u32)
        .and_then(|()| options.SetDestinationHeight(height as u32))
        .map_err(|error| format!("Windows could not size the PDF render: {error}"))?;
    let output_parent = output
        .parent()
        .ok_or_else(|| "Windows PDF image has no parent directory".to_string())?;
    let output_name = output
        .file_name()
        .ok_or_else(|| "Windows PDF image has no file name".to_string())?;
    let output_folder = StorageFolder::GetFolderFromPathAsync(&hstring(output_parent.as_os_str()))
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not open the PDF image directory: {error}"))?;
    let output_file = output_folder
        .CreateFileAsync(&hstring(output_name), CreationCollisionOption::FailIfExists)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not create the PDF image: {error}"))?;
    let stream = output_file
        .OpenAsync(FileAccessMode::ReadWrite)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not open the PDF image stream: {error}"))?;
    let rendered = page
        .RenderWithOptionsToStreamAsync(&stream, &options)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows PDF render failed: {error}"));
    let flushed = stream
        .FlushAsync()
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not flush the PDF image: {error}"));
    let _ = stream.Close();
    let _ = page.Close();
    rendered?;
    if !flushed? {
        return Err("Windows could not flush the PDF image".to_string());
    }
    Ok((width as u32, height as u32))
}

fn render_document_page_png_bytes(
    document: &PdfDocument,
    dpi: i32,
    page_index: u32,
) -> Result<Vec<u8>, String> {
    if !(72..=600).contains(&dpi) {
        return Err("Windows PDF render DPI must be between 72 and 600".to_string());
    }
    let page_count = document
        .PageCount()
        .map_err(|error| format!("Windows could not inspect the PDF: {error}"))?;
    if page_index >= page_count {
        return Err("Windows PDF page index is out of range".to_string());
    }
    let page = document
        .GetPage(page_index)
        .map_err(|error| format!("Windows could not open the PDF page: {error}"))?;
    let result = (|| {
        let size = page
            .Size()
            .map_err(|error| format!("Windows could not measure the PDF page: {error}"))?;
        let scale = f64::from(dpi) / 96.0;
        let width = (f64::from(size.Width) * scale).ceil();
        let height = (f64::from(size.Height) * scale).ceil();
        if !width.is_finite()
            || !height.is_finite()
            || width < 1.0
            || height < 1.0
            || width > f64::from(MAX_RENDER_EDGE)
            || height > f64::from(MAX_RENDER_EDGE)
            || (width * height) > MAX_RENDER_PIXELS as f64
        {
            return Err("Windows PDF page exceeds the render limit".to_string());
        }
        let options = PdfPageRenderOptions::new()
            .map_err(|error| format!("Windows could not create PDF render options: {error}"))?;
        options
            .SetDestinationWidth(width as u32)
            .and_then(|()| options.SetDestinationHeight(height as u32))
            .map_err(|error| format!("Windows could not size the PDF render: {error}"))?;
        let stream = InMemoryRandomAccessStream::new()
            .map_err(|error| format!("Windows could not create the PDF image stream: {error}"))?;
        let streamed = (|| {
            page.RenderWithOptionsToStreamAsync(&stream, &options)
                .and_then(|operation| operation.get())
                .map_err(|error| format!("Windows PDF render failed: {error}"))?;
            let flushed = stream
                .FlushAsync()
                .and_then(|operation| operation.get())
                .map_err(|error| format!("Windows could not flush the PDF image: {error}"))?;
            if !flushed {
                return Err("Windows could not flush the PDF image".to_string());
            }
            let byte_count = stream
                .Size()
                .map_err(|error| format!("Windows could not size the PDF image: {error}"))?;
            let byte_count = u32::try_from(byte_count)
                .map_err(|_| "Windows rendered PDF image is too large".to_string())?;
            if byte_count == 0 {
                return Err("Windows rendered an empty PDF image".to_string());
            }
            let input = stream
                .GetInputStreamAt(0)
                .map_err(|error| format!("Windows could not read the PDF image: {error}"))?;
            let reader = DataReader::CreateDataReader(&input).map_err(|error| {
                format!("Windows could not decode the PDF image stream: {error}")
            })?;
            let read_result = (|| {
                let loaded = reader
                    .LoadAsync(byte_count)
                    .and_then(|operation| operation.get())
                    .map_err(|error| {
                        format!("Windows could not load the PDF image bytes: {error}")
                    })?;
                if loaded != byte_count {
                    return Err("Windows returned an incomplete PDF image".to_string());
                }
                let mut bytes = vec![0; byte_count as usize];
                reader.ReadBytes(&mut bytes).map_err(|error| {
                    format!("Windows could not copy the PDF image bytes: {error}")
                })?;
                Ok(bytes)
            })();
            let _ = reader.Close();
            read_result
        })();
        let _ = stream.Close();
        streamed
    })();
    let _ = page.Close();
    result
}

fn load_document(input: &Path) -> Result<(PdfDocument, IRandomAccessStream), String> {
    const STGM_READ: u32 = 0;
    let input = HSTRING::from_wide(&input.as_os_str().encode_wide().collect::<Vec<_>>());
    let stream: IRandomAccessStream = unsafe { CreateRandomAccessStreamOnFile(&input, STGM_READ) }
        .map_err(|error| format!("Windows could not open the PDF stream: {error}"))?;
    let document = PdfDocument::LoadFromStreamAsync(&stream)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not load the PDF: {error}"))?;
    Ok((document, stream))
}

fn initialize_runtime() -> Result<RuntimeGuard, String> {
    let initialized = match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        Ok(()) => true,
        Err(error) if error.code() == RPC_E_CHANGED_MODE => false,
        Err(error) => return Err(format!("Windows Runtime initialization failed: {error}")),
    };
    Ok(RuntimeGuard(initialized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::windows::ffi::OsStrExt};

    #[test]
    fn opens_a_pdf_through_an_extended_length_canonical_path() {
        let fixture = tempfile::tempdir().unwrap();
        let mut directory = fs::canonicalize(fixture.path()).unwrap();
        while directory.as_os_str().encode_wide().count() < 300 {
            directory.push("docflow-extended-path-segment");
        }
        fs::create_dir_all(&directory).unwrap();
        let pdf = directory.join("scan.pdf");
        fs::write(&pdf, minimal_pdf()).unwrap();

        assert!(pdf.as_os_str().encode_wide().count() > 260);
        assert_eq!(page_count(&pdf).unwrap(), 1);
    }

    fn minimal_pdf() -> Vec<u8> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>",
        ];
        let mut bytes = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(bytes.len());
            bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = bytes.len();
        bytes.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in offsets {
            bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(
            format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );
        bytes
    }
}
