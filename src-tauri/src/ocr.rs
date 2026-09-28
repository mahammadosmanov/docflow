use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{
    operations::pdf_orientation::{OrientationDetection, OrientationDetector},
    operations::process_scans::{OcrAdapter, OcrFailure, OcrRead},
    rfi_code::{RfiMatch, RfiMethod, exact_codes, find},
};

const TEXT_LAYER_MIN_CHARS: usize = 50;
const ADAPTIVE_C: i32 = 10;

#[derive(Clone, Copy)]
struct OcrAttempt {
    dpi: i32,
    band: f64,
    block: usize,
    psm: u8,
}

const OCR_CASCADE: [OcrAttempt; 4] = [
    OcrAttempt {
        dpi: 300,
        band: 0.35,
        block: 31,
        psm: 6,
    },
    OcrAttempt {
        dpi: 250,
        band: 0.35,
        block: 31,
        psm: 6,
    },
    OcrAttempt {
        dpi: 200,
        band: 0.35,
        block: 31,
        psm: 6,
    },
    OcrAttempt {
        dpi: 350,
        band: 0.35,
        block: 41,
        psm: 6,
    },
];
const RAW_FALLBACK_DPI: [i32; 5] = [250, 300, 350, 400, 450];
const RAW_FALLBACK_PSM: u8 = 11;

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

const FAST_COVER_DPI: i32 = 150;
const FAST_COVER_RESCUE_DPI: i32 = 300;
const FAST_COVER_TOP_PERCENT: usize = 34;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CoverSignals {
    request_heading: bool,
    template_mark: bool,
    strict_identity: bool,
    identity_hint: bool,
}

fn cover_signals(text: &str) -> CoverSignals {
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
    let codes = exact_codes(&normalized);
    let matched = find(&normalized);
    let strict_identity =
        codes.len() == 1 && matched.method == RfiMethod::Strict && matched.confidence >= 0.8;
    let identity_hint = !codes.is_empty() || matched.ok();
    CoverSignals {
        request_heading,
        template_mark,
        strict_identity,
        identity_hint,
    }
}

fn confirmed_cover_read(text: &str, source: &str) -> Option<OcrRead> {
    let signals = cover_signals(text);
    if !(signals.request_heading && signals.template_mark && signals.strict_identity) {
        return None;
    }
    let matched = find(text);
    Some(OcrRead {
        matched,
        source: format!("ocr:fast-cover:confirmed:{source}"),
    })
}

fn uncertain_cover_read(source: String) -> OcrRead {
    OcrRead {
        matched: find(""),
        source: format!("ocr:fast-cover:uncertain:{source}"),
    }
}

fn no_cover_signal_read() -> OcrRead {
    OcrRead {
        matched: find(""),
        source: "ocr:fast-cover:no-cover-signal".to_string(),
    }
}

fn cover_top_band(image: &GrayImage) -> Result<GrayImage, String> {
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

fn read_cover_band_text(
    ocr: &NativeOcr,
    image: &GrayImage,
    page_index: usize,
    direction: i64,
    attempt: usize,
    temporary: &Path,
) -> Result<String, OcrFailure> {
    let band = cover_top_band(image).map_err(OcrFailure::Document)?;
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

fn read_fast_cover_page(
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
    let mut possible_cover = cover_signals(&text_layer).request_heading;
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
        let mut signals = cover_signals(&text);
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
                signals = cover_signals(&text);
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
            let rescue_signals = cover_signals(&rescue_text);
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
struct PdfOcrSession {
    #[cfg(target_os = "macos")]
    document: std::ptr::NonNull<std::ffi::c_void>,
    #[cfg(target_os = "windows")]
    document: crate::windows_pdf::InMemoryPdf,
}

impl PdfOcrSession {
    #[cfg(target_os = "macos")]
    fn open(path: &Path) -> Result<Self, OcrFailure> {
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
    fn open(path: &Path) -> Result<Self, OcrFailure> {
        let document =
            crate::windows_pdf::InMemoryPdf::open_path(path).map_err(OcrFailure::Document)?;
        Ok(Self { document })
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn open(_path: &Path) -> Result<Self, OcrFailure> {
        Ok(Self {})
    }

    #[cfg(target_os = "macos")]
    fn page_text(&self, _path: &Path, page_index: usize) -> Result<String, OcrFailure> {
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
    fn page_text(&self, path: &Path, page_index: usize) -> Result<String, OcrFailure> {
        page_text(path, page_index)
    }

    #[cfg(target_os = "macos")]
    fn render_page_png(
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
    fn render_page_png(
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
    fn render_page_png(
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

impl OcrAdapter for NativeOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        self.read_page(path, 0)
    }

    fn supports_page_scan(&self) -> bool {
        true
    }

    fn supports_page_previews(&self) -> bool {
        cfg!(any(target_os = "macos", target_os = "windows"))
    }

    fn supports_parallel_analysis(&self) -> bool {
        true
    }

    fn supports_orientation(&self) -> bool {
        true
    }

    fn detect_orientation(
        &self,
        path: &Path,
        page_index: usize,
    ) -> Result<OrientationDetection, String> {
        OrientationDetector::detect_page(self, path, page_index)
    }

    fn read_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        #[cfg(target_os = "windows")]
        if self.tesseract.as_ref().is_none_or(|path| !path.is_file()) {
            return Err(OcrFailure::EngineMissing);
        }
        let pdf = PdfOcrSession::open(path)?;
        let text = pdf.page_text(path, page_index)?;
        let text_layer =
            (text.trim().chars().count() >= TEXT_LAYER_MIN_CHARS).then(|| exact_codes(&text));
        if self.tesseract.as_ref().is_none_or(|path| !path.is_file()) {
            return Err(OcrFailure::EngineMissing);
        }
        let temporary = tempfile::tempdir()
            .map_err(|error| OcrFailure::Document(format!("OCR temp directory: {error}")))?;
        let mut gray_cache = HashMap::<i32, GrayImage>::new();
        let mut observations = Vec::with_capacity(OCR_CASCADE.len());
        for (index, attempt) in OCR_CASCADE.iter().enumerate() {
            if let std::collections::hash_map::Entry::Vacant(entry) = gray_cache.entry(attempt.dpi)
            {
                let rendered = temporary
                    .path()
                    .join(format!("page-{page_index}-{}.png", attempt.dpi));
                pdf.render_page_png(path, &rendered, attempt.dpi, page_index)
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
            observations.extend(
                rfi_matches(&text)
                    .into_iter()
                    .map(|matched| (attempt.dpi, matched)),
            );
        }
        if let Some((matched, votes, dpi_count)) =
            select_rfi_consensus(&observations, text_layer.as_ref())
        {
            return Ok(consensus_read(
                matched,
                votes,
                dpi_count,
                text_layer.as_ref(),
                false,
            ));
        }

        for (index, dpi) in RAW_FALLBACK_DPI.into_iter().enumerate() {
            if let std::collections::hash_map::Entry::Vacant(entry) = gray_cache.entry(dpi) {
                let rendered = temporary
                    .path()
                    .join(format!("page-{page_index}-{dpi}.png"));
                pdf.render_page_png(path, &rendered, dpi, page_index)
                    .map_err(OcrFailure::Document)?;
                entry.insert(read_png_gray(&rendered).map_err(OcrFailure::Document)?);
            }
            let image = temporary.path().join(format!("raw-{dpi}.pgm"));
            write_pgm(
                &image,
                gray_cache
                    .get(&dpi)
                    .expect("render cache contains every fallback DPI"),
            )
            .map_err(OcrFailure::Document)?;
            let text = run_tesseract(
                self.tesseract.as_deref().expect("validated Tesseract path"),
                &image,
                temporary.path(),
                self.tessdata.as_deref(),
                RAW_FALLBACK_PSM,
                OCR_CASCADE.len() + index,
            )?;
            observations.extend(rfi_matches(&text).into_iter().map(|matched| (dpi, matched)));
        }

        Ok(
            match select_rfi_consensus(&observations, text_layer.as_ref()) {
                Some((matched, votes, dpi_count)) => {
                    consensus_read(matched, votes, dpi_count, text_layer.as_ref(), true)
                }
                None => OcrRead {
                    matched: find(""),
                    source: unconfirmed_page_source(&observations, text_layer.as_ref()),
                },
            },
        )
    }

    fn read_cover_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        read_fast_cover_page(self, path, page_index)
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

fn consensus_read(
    matched: RfiMatch,
    votes: usize,
    dpi_count: usize,
    text_layer: Option<&BTreeSet<String>>,
    used_fallback: bool,
) -> OcrRead {
    OcrRead {
        source: format!(
            "ocr:consensus{}:{votes}votes:{dpi_count}dpi:{}",
            if used_fallback { "-fallback" } else { "" },
            text_layer_evidence(text_layer, &matched.code)
        ),
        matched,
    }
}

fn rfi_matches(text: &str) -> Vec<RfiMatch> {
    let codes = exact_codes(text);
    if codes.is_empty() {
        vec![find(text)]
    } else {
        codes.into_iter().map(|code| find(&code)).collect()
    }
}

fn text_layer_evidence(text_layer: Option<&BTreeSet<String>>, code: &str) -> &'static str {
    match text_layer {
        Some(codes) if codes.len() == 1 && codes.contains(code) => "text-agrees",
        Some(codes) if !codes.is_empty() => "text-conflicts",
        Some(_) => "text-no-exact-code",
        None => "no-text-layer",
    }
}

fn text_layer_observation_summary(text_layer: Option<&BTreeSet<String>>) -> String {
    match text_layer {
        None => "no-text-layer".to_string(),
        Some(codes) if codes.is_empty() => "text-no-exact-code".to_string(),
        Some(codes) => format!(
            "text-codes={}",
            codes.iter().cloned().collect::<Vec<_>>().join(",")
        ),
    }
}

fn rfi_observation_summary(observations: &[(i32, RfiMatch)]) -> String {
    let mut summary = BTreeMap::<&str, (usize, BTreeSet<i32>)>::new();
    for (dpi, matched) in observations.iter().filter(|(_, matched)| matched.ok()) {
        let entry = summary
            .entry(matched.code.as_str())
            .or_insert_with(|| (0, BTreeSet::new()));
        entry.0 += 1;
        entry.1.insert(*dpi);
    }
    if summary.is_empty() {
        return "no-candidate".to_string();
    }
    summary
        .into_iter()
        .map(|(code, (votes, dpis))| format!("{code}={votes}votes/{}dpi", dpis.len()))
        .collect::<Vec<_>>()
        .join(",")
}

fn unconfirmed_page_source(
    observations: &[(i32, RfiMatch)],
    text_layer: Option<&BTreeSet<String>>,
) -> String {
    let has_candidate = observations.iter().any(|(_, matched)| matched.ok())
        || text_layer.is_some_and(|codes| !codes.is_empty());
    format!(
        "ocr:{}:{}:{}",
        if has_candidate {
            "ambiguous-or-unconfirmed"
        } else {
            "no-candidate"
        },
        rfi_observation_summary(observations),
        text_layer_observation_summary(text_layer)
    )
}

fn select_rfi_consensus(
    observations: &[(i32, RfiMatch)],
    text_layer: Option<&BTreeSet<String>>,
) -> Option<(RfiMatch, usize, usize)> {
    const MIN_STRICT_VOTES: usize = 2;
    const MIN_DISTINCT_DPI: usize = 3;
    const CONSENSUS_CONFIDENCE: f64 = 0.95;

    let observed_codes = observations
        .iter()
        .filter(|(_, matched)| matched.ok())
        .map(|(_, matched)| matched.code.as_str())
        .collect::<BTreeSet<_>>();
    if observed_codes.len() != 1 {
        return None;
    }

    let code = *observed_codes.iter().next()?;
    if text_layer
        .is_some_and(|codes| !codes.is_empty() && (codes.len() != 1 || !codes.contains(code)))
    {
        return None;
    }
    let mut strict = observations.iter().filter_map(|(_, matched)| {
        (matched.method == RfiMethod::Strict && matched.code == code).then_some(matched)
    });
    let mut matched = strict.next()?.clone();
    let strict_votes = 1 + strict.count();
    let dpis = observations
        .iter()
        .filter(|(_, matched)| matched.ok() && matched.code == code)
        .map(|(dpi, _)| *dpi)
        .collect::<BTreeSet<_>>();
    if strict_votes < MIN_STRICT_VOTES || dpis.len() < MIN_DISTINCT_DPI {
        return None;
    }
    matched.confidence = CONSENSUS_CONFIDENCE;
    Some((matched, strict_votes, dpis.len()))
}

impl OrientationDetector for NativeOcr {
    fn detect_page(&self, path: &Path, page_index: usize) -> Result<OrientationDetection, String> {
        const PRIMARY_DPI: i32 = 250;
        const SECONDARY_DPI: i32 = 350;
        let tesseract = self
            .tesseract
            .as_deref()
            .filter(|path| path.is_file())
            .ok_or_else(|| "Tesseract is unavailable".to_string())?;
        let tessdata = self
            .tessdata
            .as_deref()
            .filter(|directory| bundled_regular_file(&directory.join("osd.traineddata")))
            .ok_or_else(|| {
                "Tesseract orientation data (osd.traineddata) is unavailable".to_string()
            })?;
        let temporary =
            tempfile::tempdir().map_err(|error| format!("orientation temp: {error}"))?;
        let primary = temporary
            .path()
            .join(format!("page-{page_index}-{PRIMARY_DPI}.png"));
        render_page_png(path, &primary, PRIMARY_DPI, page_index)?;
        let primary_detection =
            run_orientation_tesseract(tesseract, &primary, temporary.path(), tessdata, PRIMARY_DPI)
                .and_then(|text| parse_orientation_output(&text, PRIMARY_DPI))
                .ok();
        if let Some(detection) = select_osd_detection(primary_detection.as_ref(), None) {
            return Ok(detection);
        }

        let secondary = temporary
            .path()
            .join(format!("page-{page_index}-{SECONDARY_DPI}.png"));
        let secondary_detection = render_page_png(path, &secondary, SECONDARY_DPI, page_index)
            .and_then(|()| {
                run_orientation_tesseract(
                    tesseract,
                    &secondary,
                    temporary.path(),
                    tessdata,
                    SECONDARY_DPI,
                )
            })
            .and_then(|text| parse_orientation_output(&text, SECONDARY_DPI))
            .ok();
        if let Some(detection) =
            select_osd_detection(primary_detection.as_ref(), secondary_detection.as_ref())
        {
            return Ok(detection);
        }

        let (first, first_scores) = detect_orientation_by_direction_scores(
            tesseract,
            &primary,
            temporary.path(),
            tessdata,
            PRIMARY_DPI,
        )?;
        if let Some(detection) = first {
            return Ok(detection);
        }
        if !secondary.is_file() {
            render_page_png(path, &secondary, SECONDARY_DPI, page_index)?;
        }
        let (second, second_scores) = detect_orientation_by_direction_scores(
            tesseract,
            &secondary,
            temporary.path(),
            tessdata,
            SECONDARY_DPI,
        )?;
        second
            .or_else(|| {
                select_correlated_header_score(&first_scores, &second_scores)
            })
            .or_else(|| {
                select_correlated_direction_score(
                    &first_scores,
                    &second_scores,
                    primary_detection.as_ref(),
                    secondary_detection.as_ref(),
                )
            })
            .ok_or_else(|| {
            format!(
                "orientation uncertain after 250/350-DPI OSD and four-direction OCR; OSD 250dpi {}, 350dpi {}; OCR 250dpi [{}], 350dpi [{}]",
                osd_summary(primary_detection.as_ref()),
                osd_summary(secondary_detection.as_ref()),
                direction_summary(&first_scores),
                direction_summary(&second_scores),
            )
        })
    }

    fn diagnostics(&self) -> String {
        let engine = self
            .tesseract
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unavailable".to_string());
        let osd = self
            .tessdata
            .as_deref()
            .is_some_and(|directory| bundled_regular_file(&directory.join("osd.traineddata")));
        format!("tesseract={engine}; osd={osd}")
    }
}

fn run_orientation_tesseract(
    executable: &Path,
    image: &Path,
    temporary: &Path,
    tessdata: &Path,
    dpi: i32,
) -> Result<String, String> {
    const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;
    let stdout_path = temporary.join(format!("orientation-{dpi}.stdout.txt"));
    let stderr_path = temporary.join(format!("orientation-{dpi}.stderr.txt"));
    let stdout = fs::File::create(&stdout_path)
        .map_err(|error| format!("orientation stdout create: {error}"))?;
    let stderr = fs::File::create(&stderr_path)
        .map_err(|error| format!("orientation stderr create: {error}"))?;
    let mut command = Command::new(executable);
    configure_tesseract_process(&mut command);
    command
        .arg(image)
        .arg("stdout")
        .arg("--psm")
        .arg("0")
        .arg("-l")
        .arg("osd")
        .env("TESSDATA_PREFIX", tessdata)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    let mut child = command
        .spawn()
        .map_err(|error| format!("orientation detector could not start: {error}"))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                let output_size = fs::metadata(&stdout_path).map_or(0, |value| value.len())
                    + fs::metadata(&stderr_path).map_or(0, |value| value.len());
                if output_size > MAX_OUTPUT_BYTES {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("orientation detector output exceeded the limit".to_string());
                }
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("orientation detector timed out".to_string());
            }
            Err(error) => return Err(format!("orientation detector wait failed: {error}")),
        }
    };
    let stdout = fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
    let combined = format!("{stdout}\n{stderr}");
    if !status.success() {
        return Err(format!(
            "orientation detector failed: {}",
            combined.lines().next().unwrap_or("unknown error")
        ));
    }
    Ok(combined)
}

fn parse_orientation_output(output: &str, dpi: i32) -> Result<OrientationDetection, String> {
    let mut rotate_clockwise = None;
    let mut confidence = None;
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "Rotate" => rotate_clockwise = value.trim().parse::<i64>().ok(),
            "Orientation confidence" => confidence = value.trim().parse::<f64>().ok(),
            _ => {}
        }
    }
    let rotate_clockwise = rotate_clockwise
        .filter(|value| matches!(value, 0 | 90 | 180 | 270))
        .ok_or_else(|| "orientation result has no valid rotation".to_string())?;
    let confidence = confidence
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| "orientation result has no valid confidence".to_string())?;
    Ok(OrientationDetection {
        rotate_clockwise,
        confidence,
        method: format!("tesseract-osd:{dpi}dpi"),
    })
}

fn select_osd_detection(
    primary: Option<&OrientationDetection>,
    secondary: Option<&OrientationDetection>,
) -> Option<OrientationDetection> {
    const STRONG_CONFIDENCE: f64 = 10.0;
    const CONFLICT_CONFIDENCE: f64 = 3.0;
    const CONSENSUS_TOTAL: f64 = 12.0;

    match (primary, secondary) {
        (Some(first), Some(second)) if first.rotate_clockwise == second.rotate_clockwise => {
            let combined = first.confidence + second.confidence;
            if first.confidence >= STRONG_CONFIDENCE
                || second.confidence >= STRONG_CONFIDENCE
                || (first.confidence >= CONFLICT_CONFIDENCE
                    && second.confidence >= CONFLICT_CONFIDENCE
                    && combined >= CONSENSUS_TOTAL)
            {
                Some(OrientationDetection {
                    rotate_clockwise: first.rotate_clockwise,
                    confidence: combined.clamp(STRONG_CONFIDENCE, 100.0),
                    method: "tesseract-osd-consensus:250+350dpi".to_string(),
                })
            } else {
                None
            }
        }
        (Some(first), Some(second)) => {
            let first_strong =
                first.confidence >= STRONG_CONFIDENCE && second.confidence < CONFLICT_CONFIDENCE;
            let second_strong =
                second.confidence >= STRONG_CONFIDENCE && first.confidence < CONFLICT_CONFIDENCE;
            if first_strong {
                Some(first.clone())
            } else if second_strong {
                Some(second.clone())
            } else {
                None
            }
        }
        (Some(detection), None) | (None, Some(detection))
            if detection.confidence >= STRONG_CONFIDENCE =>
        {
            Some(detection.clone())
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct DirectionScore {
    rotate_clockwise: i64,
    word_count: usize,
    confidence_sum: f64,
    header_word_count: usize,
    header_confidence_sum: f64,
}

impl DirectionScore {
    fn new(rotate_clockwise: i64, word_count: usize, confidence_sum: f64) -> Self {
        Self {
            rotate_clockwise,
            word_count,
            confidence_sum,
            header_word_count: 0,
            header_confidence_sum: 0.0,
        }
    }

    fn with_header(mut self, word_count: usize, confidence_sum: f64) -> Self {
        self.header_word_count = word_count;
        self.header_confidence_sum = confidence_sum;
        self
    }

    fn average_confidence(&self) -> f64 {
        if self.word_count == 0 {
            0.0
        } else {
            self.confidence_sum / self.word_count as f64
        }
    }
}

fn select_direction_score(scores: &[DirectionScore], dpi: i32) -> Option<OrientationDetection> {
    const MIN_WORDS: usize = 20;
    const MIN_AVERAGE_CONFIDENCE: f64 = 55.0;
    const MIN_SUM_RATIO: f64 = 1.45;
    const MIN_AVERAGE_MARGIN: f64 = 8.0;

    let mut ranked = scores.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .confidence_sum
            .total_cmp(&left.confidence_sum)
            .then_with(|| right.word_count.cmp(&left.word_count))
    });
    let winner = *ranked.first()?;
    let runner_up = *ranked.get(1)?;
    let winner_average = winner.average_confidence();
    let runner_average = runner_up.average_confidence();
    let sum_ratio = winner.confidence_sum / runner_up.confidence_sum.max(1.0);
    let average_margin = winner_average - runner_average;
    if winner.word_count < MIN_WORDS
        || winner_average < MIN_AVERAGE_CONFIDENCE
        || sum_ratio < MIN_SUM_RATIO
        || average_margin < MIN_AVERAGE_MARGIN
    {
        return None;
    }
    let confidence = (10.0 + (sum_ratio - 1.0) * 35.0 + average_margin).clamp(10.0, 100.0);
    Some(OrientationDetection {
        rotate_clockwise: winner.rotate_clockwise,
        confidence,
        method: format!("tesseract-eng-direction-score:{dpi}dpi"),
    })
}

fn detect_orientation_by_direction_scores(
    executable: &Path,
    image: &Path,
    temporary: &Path,
    tessdata: &Path,
    dpi: i32,
) -> Result<(Option<OrientationDetection>, Vec<DirectionScore>), String> {
    if !bundled_regular_file(&tessdata.join("eng.traineddata")) {
        return Err("Tesseract English data is unavailable for orientation fallback".to_string());
    }
    let max_side = if dpi >= 350 { 3_500 } else { 2_600 };
    let image = resize_gray_to_fit(&read_png_gray(image)?, max_side, max_side)?;
    let (width, height) = (image.width, image.height);
    let mut scores = Vec::with_capacity(4);
    for rotate_clockwise in [0, 90, 180, 270] {
        let rotated = rotate_gray_clockwise(&image, rotate_clockwise)?;
        let path = temporary.join(format!("direction-{dpi}-{rotate_clockwise}.pgm"));
        write_pgm(&path, &rotated)?;
        scores.push(run_direction_tesseract(
            executable,
            &path,
            temporary,
            tessdata,
            rotate_clockwise,
            dpi,
            if matches!(rotate_clockwise, 90 | 270) {
                width
            } else {
                height
            },
        )?);
    }
    Ok((select_direction_score(&scores, dpi), scores))
}

fn direction_summary(scores: &[DirectionScore]) -> String {
    let mut ranked = scores.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.confidence_sum.total_cmp(&left.confidence_sum));
    ranked
        .into_iter()
        .take(2)
        .map(|score| {
            format!(
                "{}°:{}w/{:.0}",
                score.rotate_clockwise, score.word_count, score.confidence_sum
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn osd_summary(detection: Option<&OrientationDetection>) -> String {
    detection.map_or_else(
        || "unavailable".to_string(),
        |value| format!("{}°/{:.1}", value.rotate_clockwise, value.confidence),
    )
}

fn select_correlated_direction_score(
    primary: &[DirectionScore],
    secondary: &[DirectionScore],
    primary_osd: Option<&OrientationDetection>,
    secondary_osd: Option<&OrientationDetection>,
) -> Option<OrientationDetection> {
    let first_osd = primary_osd?;
    let second_osd = secondary_osd?;
    if first_osd.rotate_clockwise != second_osd.rotate_clockwise
        || first_osd.confidence < 0.5
        || second_osd.confidence < 0.5
    {
        return None;
    }
    for scores in [primary, secondary] {
        let mut ranked = scores.iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| right.confidence_sum.total_cmp(&left.confidence_sum));
        let winner = *ranked.first()?;
        let runner = *ranked.get(1)?;
        if winner.rotate_clockwise != first_osd.rotate_clockwise
            || winner.word_count < 20
            || winner.average_confidence() < 45.0
            || winner.confidence_sum / runner.confidence_sum.max(1.0) < 1.04
            || winner.average_confidence() - runner.average_confidence() < 5.0
        {
            return None;
        }
    }
    Some(OrientationDetection {
        rotate_clockwise: first_osd.rotate_clockwise,
        confidence: 10.0,
        method: "tesseract-osd-ocr-consensus:250+350dpi".to_string(),
    })
}

fn rotate_gray_clockwise(image: &GrayImage, degrees: i64) -> Result<GrayImage, String> {
    let (width, height) = match degrees {
        0 | 180 => (image.width, image.height),
        90 | 270 => (image.height, image.width),
        _ => return Err("OCR fallback rotation is not a right angle".to_string()),
    };
    let mut pixels = vec![255; width.saturating_mul(height)];
    for y in 0..image.height {
        for x in 0..image.width {
            let (target_x, target_y) = match degrees {
                0 => (x, y),
                90 => (image.height - 1 - y, x),
                180 => (image.width - 1 - x, image.height - 1 - y),
                270 => (y, image.width - 1 - x),
                _ => unreachable!(),
            };
            pixels[target_y * width + target_x] = image.pixels[y * image.width + x];
        }
    }
    Ok(GrayImage {
        width,
        height,
        pixels,
    })
}

fn run_direction_tesseract(
    executable: &Path,
    image: &Path,
    temporary: &Path,
    tessdata: &Path,
    rotate_clockwise: i64,
    dpi: i32,
    image_height: usize,
) -> Result<DirectionScore, String> {
    const MAX_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;
    let output_path = temporary.join(format!("direction-{dpi}-{rotate_clockwise}.tsv"));
    let error_path = temporary.join(format!("direction-{dpi}-{rotate_clockwise}.stderr.txt"));
    let output = fs::File::create(&output_path)
        .map_err(|error| format!("orientation fallback output create: {error}"))?;
    let error = fs::File::create(&error_path)
        .map_err(|error| format!("orientation fallback stderr create: {error}"))?;
    let mut command = Command::new(executable);
    configure_tesseract_process(&mut command);
    command
        .arg(image)
        .arg("stdout")
        .arg("--oem")
        .arg("3")
        .arg("--psm")
        .arg("11")
        .arg("-l")
        .arg("eng")
        .arg("-c")
        .arg("tessedit_create_tsv=1")
        .env("TESSDATA_PREFIX", tessdata)
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(error);
    let mut child = command
        .spawn()
        .map_err(|error| format!("orientation fallback could not start: {error}"))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                let output_size = fs::metadata(&output_path).map_or(0, |value| value.len())
                    + fs::metadata(&error_path).map_or(0, |value| value.len());
                if output_size > MAX_OUTPUT_BYTES {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("orientation fallback output exceeded the limit".to_string());
                }
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("orientation fallback timed out".to_string());
            }
            Err(error) => return Err(format!("orientation fallback wait failed: {error}")),
        }
    };
    let output_size = fs::metadata(&output_path).map_or(0, |value| value.len())
        + fs::metadata(&error_path).map_or(0, |value| value.len());
    if output_size > MAX_OUTPUT_BYTES {
        return Err("orientation fallback output exceeded the limit".to_string());
    }
    if !status.success() {
        let error = fs::read_to_string(error_path).unwrap_or_default();
        return Err(format!(
            "orientation fallback failed: {}",
            error.lines().next().unwrap_or("unknown error")
        ));
    }
    let output = fs::read_to_string(output_path)
        .map_err(|error| format!("orientation fallback output decode: {error}"))?;
    let mut word_count = 0_usize;
    let mut confidence_sum = 0.0_f64;
    let mut header_word_count = 0_usize;
    let mut header_confidence_sum = 0.0_f64;
    for line in output.lines().skip(1) {
        let columns = line.splitn(12, '\t').collect::<Vec<_>>();
        if columns.len() != 12
            || !columns[11]
                .chars()
                .any(|value| value.is_ascii_alphanumeric())
        {
            continue;
        }
        let Ok(confidence) = columns[10].parse::<f64>() else {
            continue;
        };
        if confidence >= 30.0 && confidence.is_finite() {
            word_count += 1;
            confidence_sum += confidence;
            let top = columns[7].parse::<usize>().unwrap_or(image_height);
            let height = columns[9].parse::<usize>().unwrap_or(0);
            if top.saturating_add(height / 2) <= image_height / 6 {
                header_word_count += 1;
                header_confidence_sum += confidence;
            }
        }
    }
    Ok(
        DirectionScore::new(rotate_clockwise, word_count, confidence_sum)
            .with_header(header_word_count, header_confidence_sum),
    )
}

fn select_correlated_header_score(
    primary: &[DirectionScore],
    secondary: &[DirectionScore],
) -> Option<OrientationDetection> {
    const MIN_HEADER_WORDS: usize = 3;
    const MIN_HEADER_CONFIDENCE: f64 = 120.0;
    const MIN_HEADER_RATIO: f64 = 1.8;
    const MIN_HEADER_MARGIN: f64 = 80.0;

    fn winner(scores: &[DirectionScore]) -> Option<&DirectionScore> {
        let mut ranked = scores
            .iter()
            .filter(|score| {
                score.header_word_count >= MIN_HEADER_WORDS
                    && score.header_confidence_sum >= MIN_HEADER_CONFIDENCE
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .header_confidence_sum
                .total_cmp(&left.header_confidence_sum)
                .then_with(|| right.header_word_count.cmp(&left.header_word_count))
        });
        let best = *ranked.first()?;
        let runner = ranked
            .get(1)
            .map_or(0.0, |score| score.header_confidence_sum);
        (best.header_confidence_sum - runner >= MIN_HEADER_MARGIN
            && best.header_confidence_sum / runner.max(1.0) >= MIN_HEADER_RATIO)
            .then_some(best)
    }

    let first = winner(primary)?;
    let second = winner(secondary)?;
    (first.rotate_clockwise == second.rotate_clockwise).then(|| OrientationDetection {
        rotate_clockwise: first.rotate_clockwise,
        confidence: 10.0,
        method: "tesseract-page-header-consensus:250+350dpi".to_string(),
    })
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

fn run_tesseract_for_cover(
    executable: &Path,
    image: &Path,
    temporary: &Path,
    tessdata: Option<&Path>,
    psm: u8,
    attempt: usize,
) -> Result<String, OcrFailure> {
    run_tesseract_with_whitelist(executable, image, temporary, tessdata, psm, attempt, None)
}

fn run_tesseract_with_whitelist(
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
pub fn render_first_page_png(input: &Path, output: &Path, dpi: i32) -> Result<(), String> {
    render_page_png(input, output, dpi, 0)
}

#[cfg(target_os = "macos")]
fn render_page_png(input: &Path, output: &Path, dpi: i32, page_index: usize) -> Result<(), String> {
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
fn render_page_png(input: &Path, output: &Path, dpi: i32, page_index: usize) -> Result<(), String> {
    let page_index = u32::try_from(page_index)
        .map_err(|_| "Windows PDF page index is out of range".to_string())?;
    crate::windows_pdf::render_page_png(input, output, dpi, page_index).map(|_| ())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn render_first_page_png(_input: &Path, _output: &Path, _dpi: i32) -> Result<(), String> {
    Err("native PDF image rendering is unsupported on this platform".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn render_page_png(
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

#[cfg(test)]
mod orientation_tests {
    use super::*;

    #[test]
    fn parses_tesseract_osd_rotation_and_confidence() {
        let result = parse_orientation_output(
            "Page number: 0\nOrientation in degrees: 270\nRotate: 90\nOrientation confidence: 18.42\nScript: Latin\n",
            250,
        )
        .unwrap();
        assert_eq!(result.rotate_clockwise, 90);
        assert_eq!(result.confidence, 18.42);
    }

    #[test]
    fn rejects_missing_or_non_right_angle_osd_results() {
        assert!(parse_orientation_output("Rotate: 45\nOrientation confidence: 20\n", 250).is_err());
        assert!(parse_orientation_output("Rotate: 90\n", 250).is_err());
    }

    #[test]
    fn accepts_only_clear_four_direction_ocr_winners() {
        let scores = [
            DirectionScore::new(0, 104, 4_999.0),
            DirectionScore::new(90, 288, 20_606.4),
            DirectionScore::new(180, 207, 10_069.5),
            DirectionScore::new(270, 106, 5_492.2),
        ];
        let selected = select_direction_score(&scores, 250).expect("clear 90 degree winner");
        assert_eq!(selected.rotate_clockwise, 90);
        assert!(selected.confidence >= 10.0);
        assert!(selected.confidence <= 100.0);
    }

    #[test]
    fn rejects_sparse_or_near_tied_direction_scores() {
        let sparse = [
            DirectionScore::new(0, 17, 915.2),
            DirectionScore::new(90, 11, 468.6),
            DirectionScore::new(180, 23, 1_146.9),
            DirectionScore::new(270, 4, 182.6),
        ];
        assert!(select_direction_score(&sparse, 250).is_none());

        let near_tie = [
            DirectionScore::new(0, 219, 10_429.9),
            DirectionScore::new(90, 13, 719.5),
            DirectionScore::new(180, 206, 10_181.0),
            DirectionScore::new(270, 13, 678.7),
        ];
        assert!(select_direction_score(&near_tie, 250).is_none());
    }

    #[test]
    fn page_header_consensus_resolves_conflicting_drawing_text_directions() {
        let primary = [
            DirectionScore::new(0, 14, 767.0).with_header(5, 218.0),
            DirectionScore::new(90, 11, 604.0).with_header(1, 45.0),
            DirectionScore::new(180, 4, 100.0),
            DirectionScore::new(270, 3, 90.0),
        ];
        let secondary = [
            DirectionScore::new(0, 8, 434.0).with_header(4, 180.0),
            DirectionScore::new(90, 10, 640.0).with_header(1, 48.0),
            DirectionScore::new(180, 9, 448.0),
            DirectionScore::new(270, 3, 100.0),
        ];
        assert_eq!(
            select_correlated_header_score(&primary, &secondary)
                .expect("both DPIs identify the same page-header direction")
                .rotate_clockwise,
            0
        );

        let conflicting_header = [
            DirectionScore::new(0, 8, 434.0),
            DirectionScore::new(90, 10, 640.0).with_header(4, 190.0),
            DirectionScore::new(180, 9, 448.0),
            DirectionScore::new(270, 3, 100.0),
        ];
        assert!(select_correlated_header_score(&primary, &conflicting_header).is_none());
    }

    #[test]
    fn weak_cover_needs_matching_osd_and_ocr_at_both_resolutions() {
        let primary = [
            DirectionScore::new(0, 21, 1_287.0),
            DirectionScore::new(90, 20, 1_029.0),
            DirectionScore::new(180, 13, 600.0),
            DirectionScore::new(270, 12, 550.0),
        ];
        let secondary = [
            DirectionScore::new(0, 52, 2_586.0),
            DirectionScore::new(90, 20, 900.0),
            DirectionScore::new(180, 56, 2_377.0),
            DirectionScore::new(270, 19, 800.0),
        ];
        let osd = OrientationDetection {
            rotate_clockwise: 0,
            confidence: 1.8,
            method: "osd".to_string(),
        };
        assert!(select_direction_score(&secondary, 350).is_none());
        assert_eq!(
            select_correlated_direction_score(&primary, &secondary, Some(&osd), Some(&osd))
                .unwrap()
                .rotate_clockwise,
            0
        );
        let conflicting_osd = OrientationDetection {
            rotate_clockwise: 180,
            ..osd.clone()
        };
        assert!(
            select_correlated_direction_score(
                &primary,
                &secondary,
                Some(&osd),
                Some(&conflicting_osd)
            )
            .is_none()
        );
        let conflicting_ocr = [
            secondary[0].clone(),
            secondary[1].clone(),
            DirectionScore::new(180, 80, 5_000.0),
            secondary[3].clone(),
        ];
        assert!(
            select_correlated_direction_score(&primary, &conflicting_ocr, Some(&osd), Some(&osd))
                .is_none()
        );
        let sparse = [
            DirectionScore::new(0, 10, 600.0),
            DirectionScore::new(90, 3, 90.0),
            DirectionScore::new(180, 4, 160.0),
            DirectionScore::new(270, 3, 100.0),
        ];
        assert!(
            select_correlated_direction_score(&sparse, &secondary, Some(&osd), Some(&osd))
                .is_none()
        );
        let near_tie = [
            DirectionScore::new(0, 52, 2_400.0),
            secondary[1].clone(),
            DirectionScore::new(180, 56, 2_350.0),
            secondary[3].clone(),
        ];
        assert!(
            select_correlated_direction_score(&primary, &near_tie, Some(&osd), Some(&osd))
                .is_none()
        );
    }

    #[test]
    fn combines_only_agreeing_osd_results() {
        let first = OrientationDetection {
            rotate_clockwise: 0,
            confidence: 7.1,
            method: "first".to_string(),
        };
        let second = OrientationDetection {
            rotate_clockwise: 0,
            confidence: 7.8,
            method: "second".to_string(),
        };
        let selected = select_osd_detection(Some(&first), Some(&second))
            .expect("agreeing OSD passes should combine");
        assert_eq!(selected.rotate_clockwise, 0);
        assert!(selected.confidence >= 10.0);

        let conflict = OrientationDetection {
            rotate_clockwise: 180,
            confidence: 8.0,
            method: "conflict".to_string(),
        };
        assert!(select_osd_detection(Some(&first), Some(&conflict)).is_none());
    }

    #[test]
    fn rotates_grayscale_pixels_clockwise_without_mirroring() {
        let image = GrayImage {
            width: 2,
            height: 3,
            pixels: vec![1, 2, 3, 4, 5, 6],
        };
        let rotated = rotate_gray_clockwise(&image, 90).unwrap();
        assert_eq!((rotated.width, rotated.height), (3, 2));
        assert_eq!(rotated.pixels, vec![5, 3, 1, 6, 4, 2]);
        assert_eq!(
            rotate_gray_clockwise(&image, 180).unwrap().pixels,
            vec![6, 5, 4, 3, 2, 1]
        );
    }
}

fn resize_gray_to_fit(
    image: &GrayImage,
    max_width: usize,
    max_height: usize,
) -> Result<GrayImage, String> {
    if image.width == 0
        || image.height == 0
        || image.pixels.len() != image.width.saturating_mul(image.height)
        || max_width == 0
        || max_height == 0
    {
        return Err("thumbnail dimensions are invalid".to_string());
    }
    let width_scale = max_width as f64 / image.width as f64;
    let height_scale = max_height as f64 / image.height as f64;
    let scale = width_scale.min(height_scale).min(1.0);
    let width = ((image.width as f64 * scale).floor() as usize).max(1);
    let height = ((image.height as f64 * scale).floor() as usize).max(1);
    let mut pixels = Vec::with_capacity(width.saturating_mul(height));
    for y in 0..height {
        let source_y = y.saturating_mul(image.height) / height;
        for x in 0..width {
            let source_x = x.saturating_mul(image.width) / width;
            pixels.push(image.pixels[source_y * image.width + source_x]);
        }
    }
    Ok(GrayImage {
        width,
        height,
        pixels,
    })
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[usize::from(first >> 2)] as char);
        output.push(TABLE[usize::from((first & 0x03) << 4 | second >> 4)] as char);
        output.push(if chunk.len() > 1 {
            TABLE[usize::from((second & 0x0f) << 2 | third >> 6)] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[usize::from(third & 0x3f)] as char
        } else {
            '='
        });
    }
    output
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
fn page_text(_path: &Path, _page_index: usize) -> Result<String, OcrFailure> {
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfirmed_page_source_distinguishes_empty_pages_from_ambiguous_codes() {
        let empty = [(150, find(""))];
        assert!(
            unconfirmed_page_source(&empty, Some(&BTreeSet::new()))
                .starts_with("ocr:no-candidate:")
        );

        let conflicting = [
            (150, find("AZE-SUN-INT-QMS-RFI-CIV-0064")),
            (200, find("AZE-SUN-INT-QMS-RFI-MEC-0064")),
        ];
        assert!(
            unconfirmed_page_source(&conflicting, None)
                .starts_with("ocr:ambiguous-or-unconfirmed:")
        );

        let searchable_code = BTreeSet::from(["AZE-SUN-INT-QMS-RFI-CIV-0064".to_string()]);
        assert!(
            unconfirmed_page_source(&empty, Some(&searchable_code))
                .starts_with("ocr:ambiguous-or-unconfirmed:")
        );
    }

    #[test]
    fn rfi_consensus_rejects_one_conflicting_strict_digit_read() {
        let observations = [
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-4286")),
            (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (200, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (350, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        ];

        assert!(select_rfi_consensus(&observations, None).is_none());
    }

    #[test]
    fn rfi_consensus_requires_three_distinct_render_resolutions() {
        let observations = [
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        ];

        assert!(select_rfi_consensus(&observations, None).is_none());
    }

    #[test]
    fn rfi_consensus_requires_two_exact_reads_not_only_repairs() {
        let observations = [
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (250, find("AZE-SUN-INT-QMS-RFI-CIV-I286")),
            (200, find("AZE-SUN-INT-QMS-RFI-CIV-I286")),
        ];

        assert!(select_rfi_consensus(&observations, None).is_none());
    }

    #[test]
    fn rfi_consensus_accepts_two_exact_fallback_reads_after_consistent_repairs() {
        let observations = [
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-O648")),
            (250, find("AZE-SUN-INT-QMS-RFI-CIV-O648")),
            (200, find("AZE-SUN-INT-QMS-RFI-CIV-O648")),
            (250, find("AZE-SUN-INT-QMS-RFI-CIV-0648")),
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-0648")),
        ];

        let (matched, votes, dpi_count) =
            select_rfi_consensus(&observations, None).expect("exact fallback corroborates repairs");

        assert_eq!(matched.code, "AZE-SUN-INT-QMS-RFI-CIV-0648");
        assert_eq!((votes, dpi_count), (2, 3));
    }

    #[test]
    fn rfi_consensus_accepts_one_exact_code_across_three_resolutions() {
        let observations = [
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (200, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        ];

        let (matched, votes, dpi_count) = select_rfi_consensus(&observations, None).unwrap();

        assert_eq!(matched.code, "AZE-SUN-INT-QMS-RFI-CIV-1286");
        assert_eq!(matched.confidence, 0.95);
        assert_eq!((votes, dpi_count), (3, 3));
    }

    #[test]
    fn rfi_consensus_rejects_a_conflicting_searchable_text_layer() {
        let observations = [
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (200, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        ];
        let stale_text = BTreeSet::from(["AZE-SUN-INT-QMS-RFI-CIV-4286".to_string()]);

        assert!(select_rfi_consensus(&observations, Some(&stale_text)).is_none());
    }

    #[test]
    fn rfi_consensus_rejects_multiple_searchable_text_layer_codes() {
        let observations = [
            (300, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (250, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
            (200, find("AZE-SUN-INT-QMS-RFI-CIV-1286")),
        ];
        let ambiguous_text = BTreeSet::from([
            "AZE-SUN-INT-QMS-RFI-CIV-1286".to_string(),
            "AZE-SUN-INT-QMS-RFI-CIV-4286".to_string(),
        ]);

        assert!(select_rfi_consensus(&observations, Some(&ambiguous_text)).is_none());
    }

    #[test]
    fn ambiguous_ocr_diagnostics_preserve_text_layer_evidence() {
        assert_eq!(
            text_layer_observation_summary(Some(&BTreeSet::from([
                "AZE-SUN-INT-QMS-RFI-CIV-1286".to_string(),
                "AZE-SUN-INT-QMS-RFI-CIV-4286".to_string(),
            ]))),
            "text-codes=AZE-SUN-INT-QMS-RFI-CIV-1286,AZE-SUN-INT-QMS-RFI-CIV-4286"
        );
        assert_eq!(text_layer_observation_summary(None), "no-text-layer");
    }

    #[test]
    fn one_ocr_pass_with_multiple_exact_codes_is_ambiguous() {
        let matches = rfi_matches("AZE-SUN-INT-QMS-RFI-CIV-1286 AZE-SUN-INT-QMS-RFI-CIV-4286");

        assert_eq!(matches.len(), 2);
        assert!(
            matches
                .iter()
                .all(|matched| matched.method == RfiMethod::Strict)
        );
    }

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

    #[test]
    fn thumbnail_resize_and_base64_are_bounded_and_canonical() {
        let image = GrayImage {
            width: 4,
            height: 2,
            pixels: vec![0, 1, 2, 3, 4, 5, 6, 7],
        };
        let resized = resize_gray_to_fit(&image, 2, 2).unwrap();
        assert_eq!((resized.width, resized.height), (2, 1));
        assert_eq!(resized.pixels, vec![0, 2]);
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }

    #[cfg(windows)]
    #[test]
    fn tesseract_launch_suppresses_the_windows_console() {
        assert_eq!(tesseract_windows_creation_flags(), 0x0800_0000);
    }
}

/// Regression lab for the production fast-cover detector.
#[cfg(test)]
mod cover_detection_tests {
    use super::*;
    use std::{env, path::PathBuf, time::Instant};

    fn is_confirmed_cover(read: &OcrRead) -> bool {
        read.source.starts_with("ocr:fast-cover:confirmed:")
            || read.source.starts_with("ocr:fast-cover:fallback:")
    }

    #[test]
    fn fast_cover_requires_heading_template_mark_and_one_strict_identity() {
        let exact =
            "SUNRISE PROJECT\nREQUEST FOR INSPECTION (RFI)\nRFI No: AZE-SUN-INT-QMS-RFI-CIV-1348";
        assert!(confirmed_cover_read(exact, "test").is_some());
        assert!(
            confirmed_cover_read(
                "INTEC ENERGY SOLUTIONS\nREQUEST FOR INSPECTION\nAZE-SUN-INT-QMS-RFI-CIV-1348",
                "test"
            )
            .is_some()
        );
        assert!(confirmed_cover_read(
            "REQUEST FOR INSPECTION\nSUNRISE PROJECT\nAZE-SUN-INT-QMS-RFI-CIV-1348 AZE-SUN-INT-QMS-RFI-CIV-1349",
            "test"
        )
        .is_none());
        assert!(
            confirmed_cover_read("AZE-SUN-INT-QMS-ITP-CIV-0006 inspection stage", "test").is_none()
        );
        assert!(confirmed_cover_read("REQUEST FOR INSPECTION only", "test").is_none());
        assert!(
            confirmed_cover_read(
                "SUNRISE SOLAR PROJECT INTEC ENERGY SOLUTIONS AZE-SUN-INT-QMS-RFI-CIV-0001",
                "test"
            )
            .is_none()
        );
    }

    #[test]
    #[ignore = "set DOCFLOW_COVER_LAB_PDF and DOCFLOW_COVER_LAB_TESSERACT to check a supplied cover page"]
    fn production_fast_cover_detects_the_supplied_first_page() {
        let source = PathBuf::from(
            env::var_os("DOCFLOW_COVER_LAB_PDF").expect("DOCFLOW_COVER_LAB_PDF is required"),
        );
        let tesseract = PathBuf::from(
            env::var_os("DOCFLOW_COVER_LAB_TESSERACT")
                .expect("DOCFLOW_COVER_LAB_TESSERACT is required"),
        );
        let ocr = NativeOcr::with_tesseract(tesseract);
        let started = Instant::now();
        let read = read_fast_cover_page(&ocr, &source, 0)
            .expect("fast detector should process the supplied cover");
        eprintln!(
            "supplied_cover elapsed_ms={} confirmed={} source={}",
            started.elapsed().as_millis(),
            is_confirmed_cover(&read),
            read.source
        );
        assert!(
            is_confirmed_cover(&read),
            "first page must be confirmed cover"
        );
    }

    #[test]
    #[ignore = "set DOCFLOW_COVER_LAB_PDF and DOCFLOW_COVER_LAB_TESSERACT to benchmark a supplied scan"]
    fn benchmark_fast_cover_scan_against_current_page_ocr() {
        let source = PathBuf::from(
            env::var_os("DOCFLOW_COVER_LAB_PDF").expect("DOCFLOW_COVER_LAB_PDF is required"),
        );
        let tesseract = PathBuf::from(
            env::var_os("DOCFLOW_COVER_LAB_TESSERACT")
                .expect("DOCFLOW_COVER_LAB_TESSERACT is required"),
        );
        assert!(tesseract.is_file(), "Tesseract executable is unavailable");
        let ocr = NativeOcr::with_tesseract(tesseract);
        let page_count = crate::pdf::inspect(&source)
            .expect("sample PDF should be inspectable")
            .page_count;
        let current_started = Instant::now();
        let mut current_covers = 0;
        for page_index in 0..page_count {
            let started = Instant::now();
            let read = ocr
                .read_page(&source, page_index)
                .expect("current OCR should process the sample page");
            current_covers += usize::from(
                read.matched.method == RfiMethod::Strict && read.matched.confidence >= 0.8,
            );
            eprintln!(
                "current page={} elapsed_ms={} exact_identity={}",
                page_index + 1,
                started.elapsed().as_millis(),
                read.matched.method == RfiMethod::Strict && read.matched.confidence >= 0.8
            );
        }
        let current_elapsed = current_started.elapsed();

        let fast_started = Instant::now();
        let mut fast_covers = 0;
        for page_index in 0..page_count {
            let started = Instant::now();
            let read = read_fast_cover_page(&ocr, &source, page_index)
                .expect("fast OCR should process the sample page");
            let confirmed = is_confirmed_cover(&read);
            fast_covers += usize::from(confirmed);
            eprintln!(
                "fast page={} elapsed_ms={} confirmed={confirmed} source={}",
                page_index + 1,
                started.elapsed().as_millis(),
                read.source
            );
        }
        let fast_elapsed = fast_started.elapsed();
        eprintln!(
            "pages={page_count} current_ms={} current_exact_pages={current_covers} fast_ms={} fast_cover_candidates={fast_covers}",
            current_elapsed.as_millis(),
            fast_elapsed.as_millis()
        );
        assert!(
            page_count > 0,
            "sample PDF should contain at least one page"
        );
        assert_eq!(fast_covers, 1, "sample must have one cover");
        assert_eq!(current_covers, 1, "current OCR must recognize its cover");
    }

    #[test]
    #[ignore = "set DOCFLOW_COVER_LAB_MANIFEST and DOCFLOW_COVER_LAB_TESSERACT to test a local PDF corpus"]
    fn benchmark_production_fast_cover_scan_corpus_and_rotations() {
        let manifest = PathBuf::from(
            env::var_os("DOCFLOW_COVER_LAB_MANIFEST")
                .expect("DOCFLOW_COVER_LAB_MANIFEST is required"),
        );
        let tesseract = PathBuf::from(
            env::var_os("DOCFLOW_COVER_LAB_TESSERACT")
                .expect("DOCFLOW_COVER_LAB_TESSERACT is required"),
        );
        assert!(tesseract.is_file(), "Tesseract executable is unavailable");
        let sources = fs::read_to_string(&manifest)
            .expect("cover corpus manifest should be readable")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        assert!(!sources.is_empty(), "cover corpus should not be empty");

        let ocr = NativeOcr::with_tesseract(tesseract);
        let temporary = tempfile::tempdir().expect("temporary cover corpus should open");
        let mut source_hits = 0;
        let mut continuation_false_covers = 0;
        let mut continuation_uncertain = 0;
        let mut rotation_hits = 0;
        let mut rotation_misses = 0;
        let mut rotation_variants = 0;

        for (sample_index, source) in sources.iter().enumerate() {
            assert!(source.is_file(), "cover corpus PDF is unavailable");
            let facts = crate::pdf::inspect(source).expect("cover corpus PDF should be valid");
            assert!(facts.page_count > 0, "cover corpus PDF should not be empty");
            let source_label = source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("sample.pdf");
            let source_result = scan_corpus_pages(&ocr, source, 0..facts.page_count);
            source_hits += usize::from(source_result.0);
            continuation_false_covers += source_result.1;
            continuation_uncertain += source_result.2;
            eprintln!(
                "corpus file={source_label} pages={} cover={} continuation_false_covers={} continuation_uncertain={}",
                facts.page_count, source_result.0, source_result.1, source_result.2
            );

            for angle in [90_i64, 180, 270] {
                let rotated = temporary
                    .path()
                    .join(format!("sample-{sample_index}-{angle}.pdf"));
                let rotations = facts
                    .pages
                    .iter()
                    .enumerate()
                    .map(|(page_index, page)| (page_index, (page.rotation + angle).rem_euclid(360)))
                    .collect::<Vec<_>>();
                if let Err(error) =
                    crate::pdf::rotate_pages_incremental(source, &rotated, &rotations)
                {
                    eprintln!(
                        "corpus_rotation=SKIP file={source_label} angle={angle} reason={error}"
                    );
                    continue;
                }
                rotation_variants += 1;
                let rotated_facts = crate::pdf::inspect(&rotated)
                    .expect("rotated test copy should remain inspectable");
                let cover_pages = [0]
                    .into_iter()
                    .chain((facts.page_count >= 20).then_some(facts.page_count - 1))
                    .collect::<Vec<_>>();
                let result = scan_corpus_pages(&ocr, &rotated, cover_pages.iter().copied());
                rotation_hits += usize::from(result.0);
                rotation_misses += usize::from(!result.0);
                continuation_false_covers += result.1;
                continuation_uncertain += result.2;
                assert_eq!(rotated_facts.page_count, facts.page_count);
            }
        }

        eprintln!(
            "corpus_summary sources={} original_cover_hits={source_hits} continuation_false_covers={continuation_false_covers} continuation_uncertain={continuation_uncertain} rotated_variants={rotation_variants} rotated_cover_hits={rotation_hits} rotated_cover_misses={rotation_misses}",
            sources.len()
        );
        assert_eq!(
            source_hits,
            sources.len(),
            "every source cover must be detected"
        );
        assert_eq!(
            continuation_false_covers, 0,
            "continuations must not split groups"
        );
        assert_eq!(
            continuation_uncertain, 0,
            "continuations must not enter Review"
        );
        assert_eq!(
            rotation_hits, rotation_variants,
            "rotated covers must be detected"
        );
        assert_eq!(rotation_misses, 0, "rotated covers must not be missed");
    }

    fn scan_corpus_pages<I>(ocr: &NativeOcr, source: &Path, pages: I) -> (bool, usize, usize)
    where
        I: IntoIterator<Item = usize>,
    {
        let mut first_page_cover = false;
        let mut continuation_false_covers = 0;
        let mut continuation_uncertain = 0;
        for page_index in pages {
            let read = read_fast_cover_page(ocr, source, page_index)
                .expect("corpus page should render and scan");
            let confirmed = is_confirmed_cover(&read);
            if page_index == 0 {
                first_page_cover = confirmed;
            } else {
                continuation_false_covers += usize::from(confirmed);
                continuation_uncertain +=
                    usize::from(!confirmed && read.source.starts_with("ocr:fast-cover:uncertain:"));
            }
        }
        (
            first_page_cover,
            continuation_false_covers,
            continuation_uncertain,
        )
    }
}
