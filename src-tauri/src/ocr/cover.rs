use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CoverSignals {
    pub(super) request_heading: bool,
    pub(super) template_mark: bool,
    pub(super) strict_identity: bool,
    pub(super) identity_hint: bool,
}

pub(super) fn clv_alias_regex() -> &'static Regex {
    static CLV_ALIAS: LazyLock<Regex> = LazyLock::new(|| {
        let separator = r"[\t ]*(?:\r?\n[\t ]*){0,2}-[\t ]*(?:\r?\n[\t ]*){0,2}";
        Regex::new(&format!(
            "AZE{separator}SUN{separator}INT{separator}QMS{separator}RFI{separator}CLV{separator}([0-9]{{1,4}})"
        ))
        .expect("the OCR-only CLV alias pattern is valid")
    });
    &CLV_ALIAS
}

pub(super) fn ocr_alias_matches(text: &str) -> Vec<RfiMatch> {
    let normalized = canonical(text);
    clv_alias_regex()
        .captures_iter(&normalized)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            if normalized[..whole.start()]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
                || normalized[whole.end()..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric)
            {
                return None;
            }
            let mut matched = find(&format!("{PREFIX}-CIV-{}", &captures[1]));
            matched.method = RfiMethod::OcrAlias;
            Some(matched)
        })
        .collect()
}

pub(super) fn cover_signals(text: &str, allow_ocr_alias: bool) -> CoverSignals {
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    let request_heading = normalized.contains("REQUEST FOR INSPECTION");
    let template_mark = [
        "SUNRISE PROJECT",
        "INTEC",
        "LIGHTSOURCEBP",
        "LIGHTSOURCE BP",
    ]
    .iter()
    .any(|mark| normalized.contains(mark));
    let aliases = if allow_ocr_alias {
        ocr_alias_matches(&normalized)
    } else {
        Vec::new()
    };
    let mut codes = exact_codes(&normalized);
    codes.extend(aliases.iter().map(|matched| matched.code.clone()));
    let matched = find(&normalized);
    let strict_identity = codes.len() == 1
        && ((matched.method == RfiMethod::Strict && matched.confidence >= 0.8)
            || aliases.iter().any(|alias| codes.contains(&alias.code)));
    let identity_hint = !codes.is_empty() || matched.ok();
    CoverSignals {
        request_heading,
        template_mark,
        strict_identity,
        identity_hint,
    }
}

pub(super) fn confirmed_cover_read(text: &str, source: &str) -> Option<OcrRead> {
    let signals = cover_signals(text, true);
    if !(signals.request_heading && signals.template_mark && signals.strict_identity) {
        return None;
    }
    let matched = ocr_alias_matches(text)
        .into_iter()
        .next()
        .unwrap_or_else(|| find(text));
    let alias_evidence = if matched.method == RfiMethod::OcrAlias {
        ":alias:CLV->CIV"
    } else {
        ""
    };
    Some(OcrRead {
        matched,
        source: format!("ocr:fast-cover:confirmed:{source}{alias_evidence}"),
    })
}

pub(super) fn uncertain_cover_read(source: String) -> OcrRead {
    OcrRead {
        matched: find(""),
        source: format!("ocr:fast-cover:uncertain:{source}"),
    }
}

pub(super) fn no_cover_signal_read() -> OcrRead {
    OcrRead {
        matched: find(""),
        source: "ocr:fast-cover:no-cover-signal".to_string(),
    }
}

pub(super) fn cover_top_band(image: &GrayImage) -> Result<GrayImage, String> {
    let height = image.height.saturating_mul(FAST_COVER_TOP_PERCENT) / 100;
    let height = height.max(1).min(image.height);
    let pixel_count = image
        .width
        .checked_mul(height)
        .ok_or_else(|| "cover band is too large".to_string())?;
    let pixels = image
        .pixels
        .get(..pixel_count)
        .ok_or_else(|| "cover band pixels are incomplete".to_string())?;
    Ok(GrayImage {
        width: image.width,
        height,
        pixels: pixels.to_vec(),
    })
}
pub(super) fn band_has_text_cues(band: &GrayImage) -> bool {
    // A cover header at 150 DPI contains several lines of black text on white background:
    // "SUNRISE PROJECT", "REQUEST FOR INSPECTION", and the RFI code.
    // If there are fewer than 100 dark pixels in the entire top band, it's effectively blank.
    band.pixels
        .iter()
        .filter(|&&pixel| pixel < 180)
        .take(100)
        .count()
        >= 100
}

pub(super) fn read_cover_band_text(
    ocr: &NativeOcr,
    image: &GrayImage,
    page_index: usize,
    direction: i64,
    attempt: usize,
    temporary: &Path,
) -> Result<String, OcrFailure> {
    let band = cover_top_band(image).map_err(OcrFailure::Document)?;
    if !band_has_text_cues(&band) {
        return Ok(String::new());
    }
    let image_path = temporary.join(format!("cover-{page_index}-{direction}.pgm"));
    write_pgm(&image_path, &band).map_err(OcrFailure::Document)?;
    run_tesseract_for_cover(
        ocr.tesseract
            .as_deref()
            .expect("cover OCR engine is validated"),
        &image_path,
        temporary,
        ocr.tessdata.as_deref(),
        11,
        attempt,
    )
}

static COVER_TRACE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
const MAX_COVER_TRACE: usize = 40;

/// Records why a cover page was not confirmed (cue flags and text length only, never the
/// text) for the activity log, so an unreadable scan can be diagnosed from one export.
fn trace_cover(page_index: usize, direction: i64, text: &str, signals: CoverSignals) {
    let mut trace = COVER_TRACE.lock().unwrap_or_else(|e| e.into_inner());
    if trace.len() < MAX_COVER_TRACE {
        // Only the identity-looking line is kept: it is the thing being diagnosed.
        let identity_line = text
            .lines()
            .find(|line| line.to_ascii_uppercase().contains("RFI-"))
            .map(|line| line.trim().chars().take(70).collect::<String>())
            .unwrap_or_default();
        trace.push(format!(
            "page={page_index} dir={direction} chars={} heading={} mark={} strict={} hint={} identity=[{identity_line}]",
            text.chars().count(),
            signals.request_heading,
            signals.template_mark,
            signals.strict_identity,
            signals.identity_hint
        ));
    }
}

pub fn take_cover_trace() -> Vec<String> {
    std::mem::take(&mut *COVER_TRACE.lock().unwrap_or_else(|e| e.into_inner()))
}

pub(super) fn read_fast_cover_page(
    ocr: &NativeOcr,
    source: &Path,
    page_index: usize,
) -> Result<OcrRead, OcrFailure> {
    #[cfg(windows)]
    if ocr.tesseract.as_ref().is_none_or(|path| !path.is_file()) {
        return Err(OcrFailure::EngineMissing);
    }
    let pdf = PdfOcrSession::open(source)?;
    let text_layer = pdf.page_text(source, page_index)?;
    // Searchable text is only a hint. A cover still needs visible-page OCR evidence.
    let mut possible_cover = cover_signals(&text_layer, false).request_heading;
    if ocr.tesseract.as_ref().is_none_or(|path| !path.is_file()) {
        return Err(OcrFailure::EngineMissing);
    }

    let temporary = tempfile::tempdir()
        .map_err(|error| OcrFailure::Document(format!("cover OCR temp directory: {error}")))?;
    let rendered = temporary.path().join(format!("cover-{page_index}-150.png"));
    pdf.render_page_png(source, &rendered, FAST_COVER_DPI, page_index)
        .map_err(OcrFailure::Document)?;
    let image = read_png_gray(&rendered).map_err(OcrFailure::Document)?;
    let mut rescue_used = false;
    let mut attempt = 0;

    for direction in [0_i64, 90, 180, 270] {
        let oriented = rotate_gray_clockwise(&image, direction).map_err(OcrFailure::Document)?;
        let mut text = read_cover_band_text(
            ocr,
            &oriented,
            page_index,
            direction,
            attempt,
            temporary.path(),
        )?;
        attempt += 1;
        if let Some(read) = confirmed_cover_read(&text, "150dpi") {
            return Ok(read);
        }
        let mut signals = cover_signals(&text, true);
        trace_cover(page_index, direction, &text, signals);
        possible_cover |= signals.request_heading;
        let promising = u8::from(signals.request_heading)
            + u8::from(signals.template_mark)
            + u8::from(signals.identity_hint)
            >= 2;
        if promising {
            for psm in [6_u8, 12] {
                let band = temporary
                    .path()
                    .join(format!("cover-{page_index}-{direction}.pgm"));
                let retry = run_tesseract_for_cover(
                    ocr.tesseract
                        .as_deref()
                        .expect("cover OCR engine is validated"),
                    &band,
                    temporary.path(),
                    ocr.tessdata.as_deref(),
                    psm,
                    attempt,
                )?;
                attempt += 1;
                text.push('\n');
                text.push_str(&retry);
                if let Some(read) = confirmed_cover_read(&text, "150dpi-retry") {
                    return Ok(read);
                }
                signals = cover_signals(&text, true);
                possible_cover |= signals.request_heading;
            }
        }

        if !rescue_used && signals.request_heading && signals.template_mark {
            rescue_used = true;
            let rescue = temporary.path().join(format!("cover-{page_index}-300.png"));
            pdf.render_page_png(source, &rescue, FAST_COVER_RESCUE_DPI, page_index)
                .map_err(OcrFailure::Document)?;
            let high_resolution = read_png_gray(&rescue).map_err(OcrFailure::Document)?;
            let oriented =
                rotate_gray_clockwise(&high_resolution, direction).map_err(OcrFailure::Document)?;
            let rescue_text = read_cover_band_text(
                ocr,
                &oriented,
                page_index,
                direction + 360,
                attempt,
                temporary.path(),
            )?;
            attempt += 1;
            if let Some(read) = confirmed_cover_read(&rescue_text, "300dpi-rescue") {
                return Ok(read);
            }
            let rescue_signals = cover_signals(&rescue_text, true);
            if rescue_signals.request_heading && rescue_signals.template_mark {
                return Ok(uncertain_cover_read("300dpi-rescue".to_string()));
            }
        }
    }

    if possible_cover {
        Ok(uncertain_cover_read(
            "partial-visible-cover-cues".to_string(),
        ))
    } else {
        Ok(no_cover_signal_read())
    }
}

/// Keeps one platform PDF decoder open across all OCR resolutions for a page.
/// Reopening the same multi-page source for every render was repeated work in
/// the stacked-scan path.
pub(super) struct PdfOcrSession {
    #[cfg(target_os = "macos")]
    pub(super) document: std::ptr::NonNull<std::ffi::c_void>,
    #[cfg(target_os = "windows")]
    pub(super) document: crate::windows_pdf::InMemoryPdf,
}

/// Accept the cheap cover-band read only when it is a confirmed cover with an
/// exact (or approved OCR-alias) identity. Everything else, including a failed
/// render, falls back to the full-page reader, so accuracy never drops below
/// the full reader; only the engine-missing condition is propagated as is.
pub(super) fn cover_first_identity(
    fast: Result<OcrRead, OcrFailure>,
    full: impl FnOnce() -> Result<OcrRead, OcrFailure>,
) -> Result<OcrRead, OcrFailure> {
    match fast {
        Ok(read) if is_trusted_cover_read(&read) => Ok(read),
        Err(OcrFailure::EngineMissing) => Err(OcrFailure::EngineMissing),
        _ => full(),
    }
}

fn is_trusted_cover_read(read: &OcrRead) -> bool {
    read.source.starts_with("ocr:fast-cover:confirmed:")
        && matches!(read.matched.method, RfiMethod::Strict | RfiMethod::OcrAlias)
        && read.matched.confidence >= 0.8
}
