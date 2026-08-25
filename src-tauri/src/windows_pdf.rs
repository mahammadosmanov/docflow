use std::{os::windows::ffi::OsStrExt, path::Path};

use windows::{
    Data::Pdf::{PdfDocument, PdfPageRenderOptions},
    Storage::{CreationCollisionOption, FileAccessMode, StorageFile, StorageFolder},
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
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

pub fn render_first_page_png(input: &Path, output: &Path, dpi: i32) -> Result<(), String> {
    render_page_png(input, output, dpi, 0).map(|_| ())
}

pub fn page_count(input: &Path) -> Result<u32, String> {
    let _runtime = initialize_runtime()?;
    let hstring =
        |value: &std::ffi::OsStr| HSTRING::from_wide(&value.encode_wide().collect::<Vec<_>>());
    let input_file = StorageFile::GetFileFromPathAsync(&hstring(input.as_os_str()))
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not open the PDF: {error}"))?;
    let document = PdfDocument::LoadFromFileAsync(&input_file)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not load the PDF: {error}"))?;
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
    let input_file = StorageFile::GetFileFromPathAsync(&hstring(input.as_os_str()))
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not open the PDF: {error}"))?;
    let document = PdfDocument::LoadFromFileAsync(&input_file)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("Windows could not load the PDF: {error}"))?;
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

fn initialize_runtime() -> Result<RuntimeGuard, String> {
    let initialized = match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        Ok(()) => true,
        Err(error) if error.code() == RPC_E_CHANGED_MODE => false,
        Err(error) => return Err(format!("Windows Runtime initialization failed: {error}")),
    };
    Ok(RuntimeGuard(initialized))
}
