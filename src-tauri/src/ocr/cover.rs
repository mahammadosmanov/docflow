use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CoverSignals {
    pub(super) request_heading: bool,
    cover_fields: bool,
    pile_ramming_checklist: bool,
    pub(super) template_mark: bool,
    pub(super) strict_identity: bool,
    pub(super) identity_hint: bool,
}

impl CoverSignals {
    fn cover_form(self) -> bool {
        !self.pile_ramming_checklist && (self.request_heading || self.cover_fields)
    }
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

/// OCR-only recovery for a damaged identity on the labeled cover line. Two damages are
/// accepted: a damaged constant head (a table rule can swallow a head hyphen, `SUN-INT`
/// read as `SUN4NT`) and the letter `O` read for the digit `0` in the serial (`O069`).
/// The discipline must still follow an exact `-RFI-` anchor, the head must be within two
/// edits of the fixed head, the serial must be exactly four characters with at least one real digit, and exactly one
/// distinct code may follow an `RFI No` label.
pub(super) fn damaged_identity(text: &str) -> Option<RfiMatch> {
    static DAMAGED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"RFI\s*NO\s*[:.]?\s*([A-Z0-9-]{10,18})-RFI-(CIV|ELE|MEC|PRC)-([0-9O]{4})")
            .expect("the damaged identity pattern is valid")
    });
    let normalized = canonical(text);
    let mut found = std::collections::BTreeSet::new();
    for captures in DAMAGED.captures_iter(&normalized) {
        let whole = captures.get(0)?;
        if normalized[whole.end()..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let serial = &captures[3];
        let head_edits = levenshtein(&captures[1], PREFIX_HEAD);
        if head_edits > 2 || !serial.bytes().any(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if head_edits == 0 && !serial.contains('O') {
            continue; // an exact read, handled by the strict path
        }
        found.insert(format!(
            "{PREFIX}-{}-{}",
            &captures[2],
            serial.replace('O', "0")
        ));
    }
    let code = (found.len() == 1).then(|| found.into_iter().next())??;
    let mut matched = find(&code);
    matched.ok().then(|| {
        matched.method = RfiMethod::OcrAlias;
        // Below an exact read, so Preview never shows a repaired number as 100%.
        matched.confidence = 0.9;
        matched
    })
}

/// The cover's own identity line is `RFI No: <exact code>`. When other RFI codes also
/// appear on the page (references, remarks), the code printed right after that label is
/// the cover's identity, but only if exactly one distinct code follows such a label.
pub(super) fn labeled_identity(text: &str) -> Option<String> {
    static LABELED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"RFI\s*NO\s*[:.]?\s*(AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{1,4})")
            .expect("the labeled identity pattern is valid")
    });
    let normalized = canonical(text);
    let mut found = std::collections::BTreeSet::new();
    for captures in LABELED.captures_iter(&normalized) {
        let whole = captures.get(0)?;
        let after = normalized[whole.end()..].chars().next();
        if after.is_some_and(char::is_alphanumeric) {
            continue;
        }
        found.insert(find(&captures[1]).code);
    }
    (found.len() == 1).then(|| found.into_iter().next())?
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
    let labeled = labeled_identity(&normalized);
    let strict_identity = (codes.len() == 1
        && ((matched.method == RfiMethod::Strict && matched.confidence >= 0.8)
            || aliases.iter().any(|alias| codes.contains(&alias.code))))
        || labeled.as_ref().is_some_and(|code| codes.contains(code));
    let damaged = allow_ocr_alias && codes.is_empty() && damaged_identity(&normalized).is_some();
    let strict_identity = strict_identity || damaged;
    let identity_hint = !codes.is_empty() || matched.ok() || damaged;
    CoverSignals {
        request_heading,
        cover_fields: normalized.contains("FOR THE ATTENTION OF")
            && normalized.contains("DISCIPLINE"),
        pile_ramming_checklist: normalized.contains("PILE RAMMING INSTALLATION CHECKLIST"),
        template_mark,
        strict_identity,
        identity_hint,
    }
}

pub(super) fn confirmed_cover_read(text: &str, source: &str) -> Option<OcrRead> {
    let signals = cover_signals(text, true);
    // Attachments also print an RFI No. With an unreadable title, require
    // cover-specific fields and an identity from the labeled cell.
    if !(signals.cover_form()
        && signals.template_mark
        && signals.strict_identity
        && (signals.request_heading
            || labeled_identity(text).is_some()
            || damaged_identity(text).is_some()))
    {
        return None;
    }
    let matched = labeled_identity(text)
        .map(|code| find(&code))
        .filter(|labeled| labeled.ok())
        .or_else(|| ocr_alias_matches(text).into_iter().next())
        .or_else(|| damaged_identity(text))
        .unwrap_or_else(|| find(text));
    let alias_evidence = if matched.method != RfiMethod::OcrAlias {
        ""
    } else if damaged_identity(text).as_ref() == Some(&matched) {
        ":alias:repair"
    } else {
        ":alias:CLV->CIV"
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

fn read_cover_at_resolution(
    ocr: &NativeOcr,
    image: &GrayImage,
    page_index: usize,
    direction: i64,
    dpi: i32,
    temporary: &Path,
) -> Result<(CoverSignals, Option<OcrRead>), OcrFailure> {
    let oriented = rotate_gray_clockwise(image, direction).map_err(OcrFailure::Document)?;
    let band = cover_top_band(&oriented).map_err(OcrFailure::Document)?;
    if !band_has_text_cues(&band) {
        return Ok((CoverSignals::default(), None));
    }
    let band_path = temporary.join(format!("cover-{dpi}-{direction}.pgm"));
    write_pgm(&band_path, &band).map_err(OcrFailure::Document)?;
    let mut evidence = CoverSignals::default();
    for psm in [11, 6, 12] {
        let text = run_tesseract_for_cover(
            ocr.tesseract
                .as_deref()
                .expect("cover OCR engine is validated"),
            &band_path,
            temporary,
            ocr.tessdata.as_deref(),
            psm,
            0,
        )?;
        let signals = cover_signals(&text, true);
        if psm == 11 {
            trace_cover(page_index, direction, &text, signals);
        }
        if signals.pile_ramming_checklist {
            return Ok((signals, None));
        }
        if let Some(read) = confirmed_cover_read(&text, &format!("{dpi}dpi")) {
            return Ok((signals, Some(read)));
        }
        evidence.request_heading |= signals.request_heading;
        evidence.cover_fields |= signals.cover_fields;
        evidence.template_mark |= signals.template_mark;
        evidence.identity_hint |= signals.identity_hint;
        if !evidence.cover_form() {
            break;
        }
    }
    Ok((evidence, None))
}

pub(super) fn verify_cover_reads(
    first: OcrRead,
    second: OcrRead,
    text_codes: &BTreeSet<String>,
) -> OcrRead {
    if first.matched.code != second.matched.code
        || (!text_codes.is_empty()
            && (text_codes.len() != 1 || !text_codes.contains(&first.matched.code)))
    {
        return uncertain_cover_read("conflicting-identities".to_string());
    }
    // Preserve alias/repair evidence even when only one read needed it.
    let mut read =
        if first.source.ends_with(":alias:repair") || second.matched.method == RfiMethod::Strict {
            first
        } else {
            second
        };
    read.source = read.source.replacen("confirmed:", "confirmed:verified:", 1);
    read
}

static COVER_TRACE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
const MAX_COVER_TRACE: usize = 40;
const FAST_COVER_FALLBACK_DPI: i32 = 225;

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
    if ocr.tesseract.as_ref().is_none_or(|path| !path.is_file()) {
        return Err(OcrFailure::EngineMissing);
    }
    let pdf = PdfOcrSession::open(source)?;
    let text_layer = pdf.page_text(source, page_index)?;
    let mut possible_cover = cover_signals(&text_layer, false).cover_form();
    let temporary = tempfile::tempdir()
        .map_err(|error| OcrFailure::Document(format!("cover OCR temp directory: {error}")))?;
    let render = |dpi| {
        let path = temporary.path().join(format!("cover-{dpi}.png"));
        pdf.render_page_png(source, &path, dpi, page_index)
            .map_err(OcrFailure::Document)?;
        read_png_gray(&path).map_err(OcrFailure::Document)
    };
    let image = render(FAST_COVER_DPI)?;
    let mut verification_images = HashMap::new();

    for direction in [0_i64, 90, 180, 270] {
        let (signals, first) = read_cover_at_resolution(
            ocr,
            &image,
            page_index,
            direction,
            FAST_COVER_DPI,
            temporary.path(),
        )?;
        if signals.pile_ramming_checklist {
            return Ok(OcrRead {
                matched: find(""),
                source: crate::operations::process_scans::PILE_RAMMING_ATTACHMENT.to_string(),
            });
        }
        if !signals.cover_form() {
            continue;
        }
        possible_cover = true;
        let mut confirm = |dpi| {
            if let std::collections::hash_map::Entry::Vacant(entry) = verification_images.entry(dpi)
            {
                entry.insert(render(dpi)?);
            }
            read_cover_at_resolution(
                ocr,
                &verification_images[&dpi],
                page_index,
                direction,
                dpi,
                temporary.path(),
            )
            .map(|(_, read)| read)
        };
        let verify = || {
            let second = confirm(FAST_COVER_RESCUE_DPI)?;
            let pair = match (first, second) {
                (Some(first), Some(second)) => Some((first, second)),
                (Some(read), None) | (None, Some(read)) => {
                    confirm(FAST_COVER_FALLBACK_DPI)?.map(|other| (read, other))
                }
                (None, None) => None,
            };
            Ok(pair.map(|(first, second)| {
                let text_codes = labeled_identity(&text_layer)
                    .map(|code| BTreeSet::from([code]))
                    .unwrap_or_else(|| exact_codes(&text_layer));
                verify_cover_reads(first, second, &text_codes)
            }))
        };
        match verify() {
            Ok(Some(read)) => return Ok(read),
            Ok(None) => {}
            Err(OcrFailure::Document(_)) => {
                return Ok(uncertain_cover_read("verification-failed".to_string()));
            }
            Err(error) => return Err(error),
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

#[cfg(test)]
mod labeled_identity_tests {
    use super::*;

    const HEAD: &str = "SUNRISE PROJECT\nREQUEST FOR INSPECTION\n";

    #[test]
    fn labeled_identity_wins_over_other_codes_on_the_cover() {
        let text = format!(
            "{HEAD}RFI No: AZE-SUN-INT-QMS-RFI-CIV-1625\nRelated: AZE-SUN-INT-QMS-RFI-CIV-1500"
        );
        let read = confirmed_cover_read(&text, "test").expect("confirmed");
        assert_eq!(read.matched.code, "AZE-SUN-INT-QMS-RFI-CIV-1625");
    }

    #[test]
    fn several_codes_without_a_single_label_stay_unconfirmed() {
        let unlabeled = format!("{HEAD}AZE-SUN-INT-QMS-RFI-CIV-1625 AZE-SUN-INT-QMS-RFI-CIV-1500");
        assert!(confirmed_cover_read(&unlabeled, "test").is_none());
        let two_labels = format!(
            "{HEAD}RFI No: AZE-SUN-INT-QMS-RFI-CIV-1625\nRFI No: AZE-SUN-INT-QMS-RFI-CIV-1500"
        );
        assert!(confirmed_cover_read(&two_labels, "test").is_none());
    }

    #[test]
    fn a_label_needs_the_cover_heading_and_template_mark() {
        let text = "RFI No: AZE-SUN-INT-QMS-RFI-CIV-1625\nAZE-SUN-INT-QMS-RFI-CIV-1500";
        assert!(confirmed_cover_read(text, "test").is_none());
    }

    #[test]
    fn the_single_code_path_is_unchanged() {
        let text = format!("{HEAD}RFI No: AZE-SUN-INT-QMS-RFI-CIV-1625");
        assert_eq!(
            confirmed_cover_read(&text, "test").unwrap().matched.code,
            "AZE-SUN-INT-QMS-RFI-CIV-1625"
        );
    }
}
