use super::*;
use base64::Engine;

const CHECK_DPI: i32 = 300;
const CROP_MARGIN_X: usize = 14;
const CROP_MARGIN_Y: usize = 10;
const SECOND_READ_BORDER: usize = 24;

/// What a second, independent read of the printed identity cell found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverCheck {
    /// `data:image/png;base64,...` of the cropped identity cell, when it was located.
    pub crop_data_url: Option<String>,
    /// `DISCIPLINE-NNNN` read again from the cropped cell, when the cell was readable.
    pub reread_tail: Option<String>,
}

/// Finds the printed RFI identity cell on a cover page, crops it for the Preview, and reads
/// the crop again as one line. The page is read-only and nothing is written outside a
/// temporary directory.
impl NativeOcr {
    pub fn check_cover_identity(
        &self,
        source: &Path,
        page_index: usize,
    ) -> Result<CoverCheck, OcrFailure> {
        let Some(executable) = self.tesseract.as_deref().filter(|path| path.is_file()) else {
            return Err(OcrFailure::EngineMissing);
        };
        let pdf = PdfOcrSession::open(source)?;
        let temporary = tempfile::tempdir()
            .map_err(|error| OcrFailure::Document(format!("identity check temp: {error}")))?;
        let rendered = temporary.path().join("page.png");
        pdf.render_page_png(source, &rendered, CHECK_DPI, page_index)
            .map_err(OcrFailure::Document)?;
        let page = read_png_gray(&rendered).map_err(OcrFailure::Document)?;
        for (attempt, direction) in [0_i64, 90, 180, 270].into_iter().enumerate() {
            let oriented = rotate_gray_clockwise(&page, direction).map_err(OcrFailure::Document)?;
            let band = cover_top_band(&oriented).map_err(OcrFailure::Document)?;
            if !band_has_text_cues(&band) {
                continue;
            }
            let band_path = temporary.path().join(format!("band-{direction}.pgm"));
            write_pgm(&band_path, &band).map_err(OcrFailure::Document)?;
            let tsv = run_tesseract_tsv(
                executable,
                &band_path,
                temporary.path(),
                self.tessdata.as_deref(),
                attempt,
            )?;
            let Some(boxed) = identity_box(&tsv, &band) else {
                continue;
            };
            let crop = crop_gray(&band, boxed);
            let crop_data_url = encode_png_data_url(&halve(&crop))?;
            let padded = pad_white(&crop, SECOND_READ_BORDER);
            let crop_path = temporary.path().join(format!("crop-{direction}.pgm"));
            write_pgm(&crop_path, &padded).map_err(OcrFailure::Document)?;
            let text = run_tesseract(
                executable,
                &crop_path,
                temporary.path(),
                self.tessdata.as_deref(),
                7,
                attempt + 8,
            )?;
            return Ok(CoverCheck {
                crop_data_url: Some(crop_data_url),
                reread_tail: identity_tail(&text),
            });
        }
        Ok(CoverCheck {
            crop_data_url: None,
            reread_tail: None,
        })
    }
}

/// `DISCIPLINE-NNNN` from text that holds exactly one `-RFI-<discipline>-<number>` tail.
/// Only the tail is compared: it is the part that must be exact.
pub(super) fn identity_tail(text: &str) -> Option<String> {
    static TAIL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"RFI-(CIV|ELE|MEC|PRC)-([0-9]{1,4})(?:[^A-Z0-9]|$)")
            .expect("the identity tail pattern is valid")
    });
    let normalized = canonical(text);
    let tails = TAIL
        .captures_iter(&normalized)
        .map(|captures| format!("{}-{:0>4}", &captures[1], &captures[2]))
        .collect::<BTreeSet<_>>();
    (tails.len() == 1).then(|| tails.into_iter().next())?
}

/// Pixel box (left, top, width, height) of the first word holding `-RFI-` in Tesseract TSV.
pub(super) fn identity_box(tsv: &str, band: &GrayImage) -> Option<(usize, usize, usize, usize)> {
    for line in tsv.lines().skip(1) {
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() < 12 || columns[0] != "5" {
            continue;
        }
        let text = canonical(columns[11]);
        if !text.contains("-RFI-") && !text.contains("QMS-RFI") {
            continue;
        }
        let number = |index: usize| columns[index].parse::<usize>().ok();
        let (left, top, width, height) = (number(6)?, number(7)?, number(8)?, number(9)?);
        if width == 0 || height == 0 || left >= band.width || top >= band.height {
            continue;
        }
        let x0 = left.saturating_sub(CROP_MARGIN_X);
        let y0 = top.saturating_sub(CROP_MARGIN_Y);
        let x1 = (left + width + CROP_MARGIN_X).min(band.width);
        let y1 = (top + height + CROP_MARGIN_Y).min(band.height);
        return Some((x0, y0, x1 - x0, y1 - y0));
    }
    None
}

pub(super) fn crop_gray(
    image: &GrayImage,
    (left, top, width, height): (usize, usize, usize, usize),
) -> GrayImage {
    let mut pixels = Vec::with_capacity(width * height);
    for row in top..top + height {
        let start = row * image.width + left;
        pixels.extend_from_slice(&image.pixels[start..start + width]);
    }
    GrayImage {
        width,
        height,
        pixels,
    }
}

/// Half-size copy (2x2 average) so the Preview payload stays small.
fn halve(image: &GrayImage) -> GrayImage {
    let width = (image.width / 2).max(1);
    let height = (image.height / 2).max(1);
    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        for column in 0..width {
            let mut total = 0_u32;
            let mut count = 0_u32;
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let (y, x) = (row * 2 + dy, column * 2 + dx);
                if y < image.height && x < image.width {
                    total += u32::from(image.pixels[y * image.width + x]);
                    count += 1;
                }
            }
            pixels.push((total / count.max(1)) as u8);
        }
    }
    GrayImage {
        width,
        height,
        pixels,
    }
}

fn pad_white(image: &GrayImage, border: usize) -> GrayImage {
    let width = image.width + 2 * border;
    let height = image.height + 2 * border;
    let mut pixels = vec![255_u8; width * height];
    for row in 0..image.height {
        let start = (row + border) * width + border;
        pixels[start..start + image.width]
            .copy_from_slice(&image.pixels[row * image.width..(row + 1) * image.width]);
    }
    GrayImage {
        width,
        height,
        pixels,
    }
}

fn encode_png_data_url(image: &GrayImage) -> Result<String, OcrFailure> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            &mut bytes,
            u32::try_from(image.width).map_err(|_| OcrFailure::Document("crop width".into()))?,
            u32::try_from(image.height).map_err(|_| OcrFailure::Document("crop height".into()))?,
        );
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| OcrFailure::Document(format!("crop PNG header: {error}")))?;
        writer
            .write_image_data(&image.pixels)
            .map_err(|error| OcrFailure::Document(format!("crop PNG data: {error}")))?;
    }
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
