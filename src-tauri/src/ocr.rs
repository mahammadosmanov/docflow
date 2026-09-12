use std::{
    collections::HashMap,
    fs,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{
    operations::process_scans::{OcrAdapter, OcrFailure, OcrRead},
    rfi_code::{RfiMethod, find},
};

const TEXT_LAYER_MIN_CHARS: usize = 50;
const ADAPTIVE_C: i32 = 10;

#[derive(Clone, Copy)]
struct OcrAttempt {
    dpi: i32,
    band: f64,
    band_label: &'static str,
    block: usize,
    psm: u8,
}

const OCR_CASCADE: [OcrAttempt; 9] = [
    OcrAttempt {
        dpi: 300,
        band: 0.35,
        band_label: "0.35",
        block: 31,
        psm: 6,
    },
    OcrAttempt {
        dpi: 300,
        band: 0.35,
        band_label: "0.35",
        block: 41,
        psm: 6,
    },
    OcrAttempt {
        dpi: 250,
        band: 0.35,
        band_label: "0.35",
        block: 31,
        psm: 6,
    },
    OcrAttempt {
        dpi: 200,
        band: 0.35,
        band_label: "0.35",
        block: 31,
        psm: 6,
    },
    OcrAttempt {
        dpi: 300,
        band: 0.55,
        band_label: "0.55",
        block: 31,
        psm: 6,
    },
    OcrAttempt {
        dpi: 300,
        band: 0.35,
        band_label: "0.35",
        block: 31,
        psm: 11,
    },
    OcrAttempt {
        dpi: 350,
        band: 0.35,
        band_label: "0.35",
        block: 41,
        psm: 6,
    },
    OcrAttempt {
        dpi: 300,
        band: 0.55,
        band_label: "0.55",
        block: 41,
        psm: 4,
    },
    OcrAttempt {
        dpi: 300,
        band: 1.00,
        band_label: "1.0",
        block: 31,
        psm: 11,
    },
];

#[derive(Clone, Debug)]
pub struct NativeOcr {
    tesseract: Option<PathBuf>,
    tessdata: Option<PathBuf>,
}

impl NativeOcr {
    pub fn with_tesseract(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            tessdata: find_tessdata(&path),
            tesseract: Some(path),
        }
    }

    pub fn discover() -> Self {
        let tesseract = tesseract_candidates()
            .into_iter()
            .find(|path| bundled_regular_file(path));
        Self {
            tessdata: tesseract.as_deref().and_then(find_tessdata),
            tesseract,
        }
    }

    pub(crate) fn support_status(&self) -> (bool, bool) {
        let engine_available = self.tesseract.as_deref().is_some_and(bundled_regular_file);
        let language_data_available = engine_available
            && self
                .tessdata
                .as_deref()
                .is_some_and(|directory| bundled_regular_file(&directory.join("eng.traineddata")));
        (engine_available, language_data_available)
    }
}

impl OcrAdapter for NativeOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        let text = first_page_text(path)?;
        if text.trim().chars().count() >= TEXT_LAYER_MIN_CHARS {
            let matched = find(&text);
            if matches!(matched.method, RfiMethod::Strict | RfiMethod::Repaired) {
                return Ok(OcrRead {
                    matched,
                    source: "text-layer".to_string(),
                });
            }
        }
        if self.tesseract.as_ref().is_none_or(|path| !path.is_file()) {
            return Err(OcrFailure::EngineMissing);
        }
        let temporary = tempfile::tempdir()
            .map_err(|error| OcrFailure::Document(format!("OCR temp directory: {error}")))?;
        let mut gray_cache = HashMap::<i32, GrayImage>::new();
        let mut repaired = None;
        for (index, attempt) in OCR_CASCADE.iter().enumerate() {
            if let std::collections::hash_map::Entry::Vacant(entry) = gray_cache.entry(attempt.dpi)
            {
                let rendered = temporary
                    .path()
                    .join(format!("first-page-{}.png", attempt.dpi));
                render_first_page_png(path, &rendered, attempt.dpi)
                    .map_err(OcrFailure::Document)?;
                entry.insert(read_png_gray(&rendered).map_err(OcrFailure::Document)?);
            }
            let thresholded = adaptive_threshold(
                gray_cache
                    .get(&attempt.dpi)
                    .expect("render cache contains the requested DPI"),
                attempt.band,
                attempt.block,
                ADAPTIVE_C,
            )
            .map_err(OcrFailure::Document)?;
            let image = temporary.path().join(format!("attempt-{index}.pgm"));
            write_pgm(&image, &thresholded).map_err(OcrFailure::Document)?;
            let text = run_tesseract(
                self.tesseract.as_deref().expect("validated Tesseract path"),
                &image,
                temporary.path(),
                self.tessdata.as_deref(),
                attempt.psm,
                index,
            )?;
            let matched = find(&text);
            let source = format!(
                "ocr:dpi{}:b{}:blk{}:psm{}",
                attempt.dpi, attempt.band_label, attempt.block, attempt.psm
            );
            if matched.method == RfiMethod::Strict {
                return Ok(OcrRead { matched, source });
            }
            if matched.method == RfiMethod::Repaired && repaired.is_none() {
                repaired = Some(matched);
            }
        }
        Ok(match repaired {
            Some(matched) => OcrRead {
                matched,
                source: "ocr:repaired".to_string(),
            },
            None => OcrRead {
                matched: find(""),
                source: "ocr:miss".to_string(),
            },
        })
    }

    fn diagnostics(&self) -> String {
        format!(
            "tesseract: {}",
            self.tesseract
                .as_deref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "BULUNAMADI".to_string())
        )
    }
}

fn tesseract_candidates() -> Vec<PathBuf> {
    let executable_name = if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    };
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        candidates.extend([
            directory
                .join("resources")
                .join("tesseract")
                .join(executable_name),
            directory
                .join("../Resources/resources/tesseract")
                .join(executable_name),
            directory.join("tesseract").join(executable_name),
            directory.join(executable_name),
        ]);
    }
    #[cfg(windows)]
    {
        for root in [
            r"C:\Program Files\Tesseract-OCR",
            r"C:\Program Files (x86)\Tesseract-OCR",
        ] {
            candidates.push(PathBuf::from(root).join(executable_name));
        }
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            candidates.push(
                PathBuf::from(program_files)
                    .join("Tesseract-OCR")
                    .join(executable_name),
            );
        }
        if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
            candidates.push(
                PathBuf::from(program_files_x86)
                    .join("Tesseract-OCR")
                    .join(executable_name),
            );
        }
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            candidates.push(
                PathBuf::from(local_appdata)
                    .join("Programs")
                    .join("Tesseract-OCR")
                    .join(executable_name),
            );
        }
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            candidates.push(dir.join(executable_name));
        }
    }
    candidates
}

fn bundled_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && !metadata_is_reparse_point(&metadata)
    })
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn find_tessdata(executable: &Path) -> Option<PathBuf> {
    if let Some(directory) = executable.parent() {
        let candidate = directory.join("tessdata");
        if bundled_regular_file(&candidate.join("eng.traineddata")) {
            return Some(candidate);
        }
        let parent_candidate = directory.join("../tessdata");
        if bundled_regular_file(&parent_candidate.join("eng.traineddata")) {
            return Some(parent_candidate);
        }
    }
    if let Ok(prefix) = std::env::var("TESSDATA_PREFIX") {
        let candidate = PathBuf::from(&prefix);
        if bundled_regular_file(&candidate.join("eng.traineddata")) {
            return Some(candidate);
        }
        let candidate_tessdata = candidate.join("tessdata");
        if bundled_regular_file(&candidate_tessdata.join("eng.traineddata")) {
            return Some(candidate_tessdata);
        }
    }
    #[cfg(windows)]
    {
        for root in [
            r"C:\Program Files\Tesseract-OCR\tessdata",
            r"C:\Program Files (x86)\Tesseract-OCR\tessdata",
        ] {
            let candidate = PathBuf::from(root);
            if bundled_regular_file(&candidate.join("eng.traineddata")) {
                return Some(candidate);
            }
        }
    }
    None
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GrayImage {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) pixels: Vec<u8>,
}

pub(crate) fn read_png_gray(path: &Path) -> Result<GrayImage, String> {
    const MAX_PIXELS: usize = 100_000_000;
    let file = fs::File::open(path).map_err(|error| format!("OCR image open: {error}"))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("OCR PNG header: {error}"))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| format!("OCR PNG pixels: {error}"))?;
    let width = usize::try_from(info.width).map_err(|_| "OCR PNG width is invalid")?;
    let height = usize::try_from(info.height).map_err(|_| "OCR PNG height is invalid")?;
    let pixel_count = width
        .checked_mul(height)
        .filter(|count| *count > 0 && *count <= MAX_PIXELS)
        .ok_or_else(|| "OCR PNG exceeds the pixel limit".to_string())?;
    let input = &buffer[..info.buffer_size()];
    let mut pixels = Vec::with_capacity(pixel_count);
    match info.color_type {
        png::ColorType::Grayscale => pixels.extend_from_slice(input),
        png::ColorType::GrayscaleAlpha => {
            for sample in input.chunks_exact(2) {
                pixels.push(composite_white(sample[0], sample[1]));
            }
        }
        png::ColorType::Rgb => {
            for sample in input.chunks_exact(3) {
                pixels.push(rgb_to_gray(sample[0], sample[1], sample[2]));
            }
        }
        png::ColorType::Rgba => {
            for sample in input.chunks_exact(4) {
                pixels.push(composite_white(
                    rgb_to_gray(sample[0], sample[1], sample[2]),
                    sample[3],
                ));
            }
        }
        png::ColorType::Indexed => {
            return Err("OCR PNG palette was not expanded".to_string());
        }
    }
    if pixels.len() != pixel_count {
        return Err("OCR PNG pixel count is inconsistent".to_string());
    }
    Ok(GrayImage {
        width,
        height,
        pixels,
    })
}

fn rgb_to_gray(red: u8, green: u8, blue: u8) -> u8 {
    let weighted = u32::from(red) * 77 + u32::from(green) * 150 + u32::from(blue) * 29;
    ((weighted + 128) >> 8) as u8
}

fn composite_white(value: u8, alpha: u8) -> u8 {
    let alpha = u32::from(alpha);
    let result = u32::from(value) * alpha + 255 * (255 - alpha);
    ((result + 127) / 255) as u8
}

fn adaptive_threshold(
    image: &GrayImage,
    band: f64,
    block: usize,
    constant: i32,
) -> Result<GrayImage, String> {
    if image.width == 0
        || image.height == 0
        || image.pixels.len() != image.width.saturating_mul(image.height)
        || !(0.0 < band && band <= 1.0)
        || block < 3
        || block.is_multiple_of(2)
    {
        return Err("adaptive OCR preprocessing parameters are invalid".to_string());
    }
    let height = ((image.height as f64 * band) as usize).max(1);
    let pixel_count = image
        .width
        .checked_mul(height)
        .ok_or_else(|| "adaptive OCR band is too large".to_string())?;
    let kernel = gaussian_kernel(block);
    let radius = (block / 2) as isize;
    let mut horizontal = vec![0.0_f32; pixel_count];
    for row in 0..height {
        for column in 0..image.width {
            let mut total = 0.0_f32;
            for (offset, weight) in kernel.iter().enumerate() {
                let source_column =
                    reflect_101(column as isize + offset as isize - radius, image.width);
                total += f32::from(image.pixels[row * image.width + source_column]) * weight;
            }
            horizontal[row * image.width + column] = total;
        }
    }
    let mut pixels = vec![0_u8; pixel_count];
    for row in 0..height {
        for column in 0..image.width {
            let mut mean = 0.0_f32;
            for (offset, weight) in kernel.iter().enumerate() {
                let source_row = reflect_101(row as isize + offset as isize - radius, height);
                mean += horizontal[source_row * image.width + column] * weight;
            }
            let source = image.pixels[row * image.width + column];
            pixels[row * image.width + column] = if f32::from(source) > mean - constant as f32 {
                255
            } else {
                0
            };
        }
    }
    Ok(GrayImage {
        width: image.width,
        height,
        pixels,
    })
}

fn gaussian_kernel(block: usize) -> Vec<f32> {
    let radius = (block / 2) as isize;
    let sigma = 0.3 * ((block as f32 - 1.0) * 0.5 - 1.0) + 0.8;
    let mut kernel = (-radius..=radius)
        .map(|offset| (-(offset * offset) as f32 / (2.0 * sigma * sigma)).exp())
        .collect::<Vec<_>>();
    let total = kernel.iter().sum::<f32>();
    for value in &mut kernel {
        *value /= total;
    }
    kernel
}

fn reflect_101(mut index: isize, length: usize) -> usize {
    if length == 1 {
        return 0;
    }
    let length = length as isize;
    while index < 0 || index >= length {
        index = if index < 0 {
            -index
        } else {
            2 * length - index - 2
        };
    }
    index as usize
}

fn write_pgm(path: &Path, image: &GrayImage) -> Result<(), String> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("OCR PGM create: {error}"))?;
    let mut writer = BufWriter::new(file);
    write!(writer, "P5\n{} {}\n255\n", image.width, image.height)
        .and_then(|()| writer.write_all(&image.pixels))
        .and_then(|()| writer.flush())
        .map_err(|error| format!("OCR PGM write: {error}"))?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| format!("OCR PGM sync: {error}"))
}

fn run_tesseract(
    executable: &Path,
    image: &Path,
    temporary: &Path,
    tessdata: Option<&Path>,
    psm: u8,
    attempt: usize,
) -> Result<String, OcrFailure> {
    const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;
    let output_path = temporary.join(format!("tesseract-{attempt}-{psm}.txt"));
    let output = fs::File::create(&output_path)
        .map_err(|error| OcrFailure::Document(format!("OCR output: {error}")))?;
    let mut command = Command::new(executable);
    configure_tesseract_process(&mut command);
    command
        .arg(image)
        .arg("stdout")
        .arg("--oem")
        .arg("3")
        .arg("--psm")
        .arg(psm.to_string())
        .arg("-l")
        .arg("eng")
        .arg("-c")
        .arg("tessedit_char_whitelist=ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-:.")
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::null());
    if let Some(tessdata) = tessdata {
        command.env("TESSDATA_PREFIX", tessdata);
    }
    let mut child = command.spawn().map_err(|_| OcrFailure::EngineMissing)?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                if fs::metadata(&output_path)
                    .is_ok_and(|metadata| metadata.len() > MAX_OUTPUT_BYTES)
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(OcrFailure::Document(
                        "Tesseract output exceeded the size limit".to_string(),
                    ));
                }
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OcrFailure::Document("Tesseract timed out".to_string()));
            }
            Err(error) => {
                return Err(OcrFailure::Document(format!(
                    "Tesseract status failed: {error}"
                )));
            }
        }
    };
    if !status.success() {
        return Err(OcrFailure::Document(format!(
            "Tesseract exited with {status}"
        )));
    }
    let metadata = fs::metadata(&output_path)
        .map_err(|error| OcrFailure::Document(format!("OCR output metadata: {error}")))?;
    if metadata.len() > MAX_OUTPUT_BYTES {
        return Err(OcrFailure::Document(
            "Tesseract output exceeded the size limit".to_string(),
        ));
    }
    fs::read_to_string(output_path)
        .map_err(|error| OcrFailure::Document(format!("OCR output decode: {error}")))
}

#[cfg(windows)]
fn tesseract_windows_creation_flags() -> u32 {
    windows_sys::Win32::System::Threading::CREATE_NO_WINDOW
}

#[cfg(windows)]
fn configure_tesseract_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(tesseract_windows_creation_flags());
}

#[cfg(not(windows))]
fn configure_tesseract_process(_command: &mut Command) {}

#[cfg(target_os = "macos")]
fn first_page_text(path: &Path) -> Result<String, OcrFailure> {
    use std::ffi::{CStr, CString, c_char};
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn docflow_pdf_first_page_text(path: *const c_char, output: *mut *mut c_char) -> i32;
        fn docflow_pdfkit_free(value: *mut c_char);
    }

    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| OcrFailure::Document("PDF path contains NUL".to_string()))?;
    let mut output = std::ptr::null_mut();
    let status = unsafe { docflow_pdf_first_page_text(path.as_ptr(), &mut output) };
    if status != 0 || output.is_null() {
        return Err(OcrFailure::Document(format!(
            "PDFKit could not read the first page ({status})"
        )));
    }
    let text = unsafe { CStr::from_ptr(output) }
        .to_string_lossy()
        .into_owned();
    unsafe { docflow_pdfkit_free(output) };
    Ok(text)
}

#[cfg(target_os = "macos")]
pub fn render_first_page_png(input: &Path, output: &Path, dpi: i32) -> Result<(), String> {
    use std::ffi::{CString, c_char};
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn docflow_pdf_render_first_page(
            input: *const c_char,
            output: *const c_char,
            dpi: i32,
        ) -> i32;
    }
    let input = CString::new(input.as_os_str().as_bytes())
        .map_err(|_| "PDF path contains NUL".to_string())?;
    let output = CString::new(output.as_os_str().as_bytes())
        .map_err(|_| "image path contains NUL".to_string())?;
    let status = unsafe { docflow_pdf_render_first_page(input.as_ptr(), output.as_ptr(), dpi) };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("PDFKit render failed ({status})"))
    }
}

#[cfg(target_os = "windows")]
pub fn render_first_page_png(input: &Path, output: &Path, dpi: i32) -> Result<(), String> {
    crate::windows_pdf::render_first_page_png(input, output, dpi)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn render_first_page_png(_input: &Path, _output: &Path, _dpi: i32) -> Result<(), String> {
    Err("native PDF image rendering is unsupported on this platform".to_string())
}

pub fn image_pdf_from_png(input: &Path, output: &Path) -> Result<(), String> {
    use flate2::{Compression, write::ZlibEncoder};

    let image = read_png_gray(input)?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&image.pixels)
        .map_err(|error| format!("image-only PDF compression: {error}"))?;
    let compressed = encoder
        .finish()
        .map_err(|error| format!("image-only PDF compression finish: {error}"))?;
    let content = b"q 612 0 0 792 0 0 cm /Im0 Do Q\n";
    let mut image_object = format!(
        "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
        image.width,
        image.height,
        compressed.len()
    )
    .into_bytes();
    image_object.extend_from_slice(&compressed);
    image_object.extend_from_slice(b"\nendstream");
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        image_object,
        {
            let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
            stream.extend_from_slice(content);
            stream.extend_from_slice(b"endstream");
            stream
        },
    ];
    let mut bytes = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        bytes.extend_from_slice(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    fs::write(output, bytes).map_err(|error| format!("image-only PDF write: {error}"))
}

#[cfg(not(target_os = "macos"))]
fn first_page_text(_path: &Path) -> Result<String, OcrFailure> {
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_discovery_ignores_environment_tesseract_overrides() {
        static ENVIRONMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENVIRONMENT.lock().unwrap();
        let fixture = tempfile::tempdir().unwrap();
        let override_binary = fixture.path().join(if cfg!(windows) {
            "attacker-tesseract.exe"
        } else {
            "attacker-tesseract"
        });
        fs::write(&override_binary, b"not a bundled executable").unwrap();
        let previous = std::env::var_os("DOCFLOW_TESSERACT");
        // SAFETY: this focused test serializes its environment mutation and restores it below.
        unsafe { std::env::set_var("DOCFLOW_TESSERACT", &override_binary) };

        let discovered = NativeOcr::discover();

        match previous {
            Some(value) => {
                // SAFETY: the serialized test restores the prior process value.
                unsafe { std::env::set_var("DOCFLOW_TESSERACT", value) };
            }
            None => {
                // SAFETY: the serialized test restores the prior absence.
                unsafe { std::env::remove_var("DOCFLOW_TESSERACT") };
            }
        }
        assert_ne!(
            discovered.tesseract.as_deref(),
            Some(override_binary.as_path())
        );
    }

    #[test]
    fn adaptive_preprocessing_crops_the_upper_band_and_emits_only_binary_pixels() {
        let image = GrayImage {
            width: 5,
            height: 10,
            pixels: (0_u8..50).map(|value| value.saturating_mul(5)).collect(),
        };

        let thresholded = adaptive_threshold(&image, 0.35, 3, 10).unwrap();

        assert_eq!((thresholded.width, thresholded.height), (5, 3));
        assert_eq!(thresholded.pixels.len(), 15);
        assert!(
            thresholded
                .pixels
                .iter()
                .all(|value| matches!(value, 0 | 255))
        );
    }

    #[cfg(windows)]
    #[test]
    fn tesseract_launch_suppresses_the_windows_console() {
        assert_eq!(tesseract_windows_creation_flags(), 0x0800_0000);
    }
}
