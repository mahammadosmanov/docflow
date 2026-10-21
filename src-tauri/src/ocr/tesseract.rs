use super::*;

#[cfg(windows)]
pub(super) fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(super) fn metadata_is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

pub(super) fn find_tessdata(executable: &Path) -> Option<PathBuf> {
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
            for sample in input.as_chunks::<2>().0 {
                pixels.push(composite_white(sample[0], sample[1]));
            }
        }
        png::ColorType::Rgb => {
            for sample in input.as_chunks::<3>().0 {
                pixels.push(rgb_to_gray(sample[0], sample[1], sample[2]));
            }
        }
        png::ColorType::Rgba => {
            for sample in input.as_chunks::<4>().0 {
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

pub(super) fn rgb_to_gray(red: u8, green: u8, blue: u8) -> u8 {
    let weighted = u32::from(red) * 77 + u32::from(green) * 150 + u32::from(blue) * 29;
    ((weighted + 128) >> 8) as u8
}

pub(super) fn composite_white(value: u8, alpha: u8) -> u8 {
    let alpha = u32::from(alpha);
    let result = u32::from(value) * alpha + 255 * (255 - alpha);
    ((result + 127) / 255) as u8
}

pub(super) fn adaptive_threshold(
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

pub(super) fn gaussian_kernel(block: usize) -> Vec<f32> {
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

pub(super) fn reflect_101(mut index: isize, length: usize) -> usize {
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

pub(super) fn write_pgm(path: &Path, image: &GrayImage) -> Result<(), String> {
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

pub(super) fn run_tesseract(
    executable: &Path,
    image: &Path,
    temporary: &Path,
    tessdata: Option<&Path>,
    psm: u8,
    attempt: usize,
) -> Result<String, OcrFailure> {
    run_tesseract_with_whitelist(
        executable,
        image,
        temporary,
        tessdata,
        psm,
        attempt,
        Some("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-:."),
    )
}

pub(super) fn run_tesseract_for_cover(
    executable: &Path,
    image: &Path,
    temporary: &Path,
    tessdata: Option<&Path>,
    psm: u8,
    attempt: usize,
) -> Result<String, OcrFailure> {
    run_tesseract_with_whitelist(executable, image, temporary, tessdata, psm, attempt, None)
}

pub(super) fn run_tesseract_with_whitelist(
    executable: &Path,
    image: &Path,
    temporary: &Path,
    tessdata: Option<&Path>,
    psm: u8,
    attempt: usize,
    character_whitelist: Option<&str>,
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
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::null());
    if let Some(whitelist) = character_whitelist {
        command
            .arg("-c")
            .arg(format!("tessedit_char_whitelist={whitelist}"));
    }
    if let Some(tessdata) = tessdata {
        command.env("TESSDATA_PREFIX", tessdata);
    }
    let mut child = command.spawn().map_err(|_| OcrFailure::EngineMissing)?;
    let started = Instant::now();
    let mut poll_count = 0_usize;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                poll_count = poll_count.wrapping_add(1);
                if poll_count.is_multiple_of(32)
                    && fs::metadata(&output_path)
                        .is_ok_and(|metadata| metadata.len() > MAX_OUTPUT_BYTES)
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(OcrFailure::Document(
                        "Tesseract output exceeded the size limit".to_string(),
                    ));
                }
                thread::sleep(Duration::from_millis(1));
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
pub(super) fn tesseract_windows_creation_flags() -> u32 {
    windows_sys::Win32::System::Threading::CREATE_NO_WINDOW
}

#[cfg(windows)]
pub(super) fn configure_tesseract_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(tesseract_windows_creation_flags());
}

#[cfg(not(windows))]
pub(super) fn configure_tesseract_process(_command: &mut Command) {}
