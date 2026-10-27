use super::*;

#[cfg(target_os = "macos")]
pub fn render_first_page_png(input: &Path, output: &Path, dpi: i32) -> Result<(), String> {
    render_page_png(input, output, dpi, 0)
}

#[cfg(target_os = "macos")]
pub(super) fn render_page_png(
    input: &Path,
    output: &Path,
    dpi: i32,
    page_index: usize,
) -> Result<(), String> {
    use std::ffi::{CString, c_char};
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn docflow_pdf_render_page(
            input: *const c_char,
            output: *const c_char,
            dpi: i32,
            page_index: usize,
        ) -> i32;
    }
    let input = CString::new(input.as_os_str().as_bytes())
        .map_err(|_| "PDF path contains NUL".to_string())?;
    let output = CString::new(output.as_os_str().as_bytes())
        .map_err(|_| "image path contains NUL".to_string())?;
    // SAFETY: input and output are NUL-terminated C strings that live across the call; the bridge reads them only during the call.
    let status =
        unsafe { docflow_pdf_render_page(input.as_ptr(), output.as_ptr(), dpi, page_index) };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("PDFKit render failed ({status})"))
    }
}

#[cfg(target_os = "windows")]
pub fn render_first_page_png(input: &Path, output: &Path, dpi: i32) -> Result<(), String> {
    render_page_png(input, output, dpi, 0)
}

#[cfg(target_os = "windows")]
pub(super) fn render_page_png(
    input: &Path,
    output: &Path,
    dpi: i32,
    page_index: usize,
) -> Result<(), String> {
    let page_index = u32::try_from(page_index)
        .map_err(|_| "Windows PDF page index is out of range".to_string())?;
    crate::windows_pdf::render_page_png(input, output, dpi, page_index).map(|_| ())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn render_first_page_png(_input: &Path, _output: &Path, _dpi: i32) -> Result<(), String> {
    Err("native PDF image rendering is unsupported on this platform".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn render_page_png(
    _input: &Path,
    _output: &Path,
    _dpi: i32,
    _page_index: usize,
) -> Result<(), String> {
    Err("native PDF image rendering is unsupported on this platform".to_string())
}

pub fn render_page_thumbnail_data_url(input: &Path, page_index: usize) -> Result<String, String> {
    const MAX_WIDTH: usize = 540;
    const MAX_HEIGHT: usize = 760;
    const MAX_PNG_BYTES: usize = 768 * 1024;
    let temporary = tempfile::tempdir().map_err(|error| format!("thumbnail temp: {error}"))?;
    let rendered = temporary.path().join("page.png");
    render_page_png(input, &rendered, 144, page_index)?;
    let thumbnail = resize_gray_to_fit(&read_png_gray(&rendered)?, MAX_WIDTH, MAX_HEIGHT)?;
    let mut png_bytes = Vec::new();
    {
        let width =
            u32::try_from(thumbnail.width).map_err(|_| "thumbnail width is invalid".to_string())?;
        let height = u32::try_from(thumbnail.height)
            .map_err(|_| "thumbnail height is invalid".to_string())?;
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("thumbnail PNG header: {error}"))?;
        writer
            .write_image_data(&thumbnail.pixels)
            .map_err(|error| format!("thumbnail PNG pixels: {error}"))?;
    }
    if png_bytes.len() > MAX_PNG_BYTES {
        return Err("thumbnail exceeds the encoded size limit".to_string());
    }
    Ok(format!(
        "data:image/png;base64,{}",
        base64_encode(&png_bytes)
    ))
}
