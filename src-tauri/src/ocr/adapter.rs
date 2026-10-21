use super::*;

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
            if index >= 2 && select_rfi_consensus(&observations, text_layer.as_ref()).is_some() {
                break;
            }
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
                None => {
                    let unresolved = OcrRead {
                        matched: find(""),
                        source: unconfirmed_page_source(&observations, text_layer.as_ref()),
                    };
                    read_fast_cover_page(self, path, page_index)
                        .ok()
                        .and_then(|cover| {
                            cover_recovery_read(cover, &observations, text_layer.as_ref())
                        })
                        .unwrap_or(unresolved)
                }
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

pub(super) fn cover_recovery_read(
    cover: OcrRead,
    observations: &[(i32, RfiMatch)],
    text_layer: Option<&BTreeSet<String>>,
) -> Option<OcrRead> {
    let matched = cover.matched;
    if !matches!(matched.method, RfiMethod::Strict | RfiMethod::OcrAlias)
        || matched.confidence < 0.8
    {
        return None;
    }
    if text_layer.is_some_and(|codes| {
        !codes.is_empty() && (codes.len() != 1 || !codes.contains(&matched.code))
    }) {
        return None;
    }

    let mut identities = BTreeMap::<&str, (usize, BTreeSet<i32>)>::new();
    for (dpi, observed) in observations
        .iter()
        .filter(|(_, observed)| matches!(observed.method, RfiMethod::Strict | RfiMethod::OcrAlias))
    {
        let entry = identities
            .entry(observed.code.as_str())
            .or_insert_with(|| (0, BTreeSet::new()));
        entry.0 += 1;
        entry.1.insert(*dpi);
    }
    let repeated = identities
        .iter()
        .filter(|(_, (votes, dpis))| *votes >= 2 && dpis.len() >= 2)
        .map(|(code, _)| *code)
        .collect::<Vec<_>>();
    if repeated.len() > 1
        || repeated
            .first()
            .is_some_and(|code| *code != matched.code.as_str())
    {
        return None;
    }

    let source = if matched.method == RfiMethod::OcrAlias {
        "ocr:cover-recovery:confirmed:alias:CLV->CIV"
    } else {
        "ocr:cover-recovery:confirmed"
    };
    Some(OcrRead {
        matched,
        source: source.to_string(),
    })
}
pub(super) fn consensus_read(
    matched: RfiMatch,
    votes: usize,
    dpi_count: usize,
    text_layer: Option<&BTreeSet<String>>,
    used_fallback: bool,
) -> OcrRead {
    let source = if matched.method == RfiMethod::OcrAlias {
        format!(
            "ocr:consensus{}:{votes}votes:{dpi_count}dpi:alias:CLV->CIV:{}",
            if used_fallback { "-fallback" } else { "" },
            text_layer_evidence(text_layer, &matched.code)
        )
    } else {
        format!(
            "ocr:consensus{}:{votes}votes:{dpi_count}dpi:{}",
            if used_fallback { "-fallback" } else { "" },
            text_layer_evidence(text_layer, &matched.code)
        )
    };
    OcrRead { matched, source }
}

pub(super) fn rfi_matches(text: &str) -> Vec<RfiMatch> {
    let mut matches = exact_codes(text)
        .into_iter()
        .map(|code| find(&code))
        .collect::<Vec<_>>();
    for alias in ocr_alias_matches(text) {
        if !matches.iter().any(|matched| matched.code == alias.code) {
            matches.push(alias);
        }
    }
    if matches.is_empty() {
        matches.push(find(text));
    }
    matches
}

pub(super) fn text_layer_evidence(
    text_layer: Option<&BTreeSet<String>>,
    code: &str,
) -> &'static str {
    match text_layer {
        Some(codes) if codes.len() == 1 && codes.contains(code) => "text-agrees",
        Some(codes) if !codes.is_empty() => "text-conflicts",
        Some(_) => "text-no-exact-code",
        None => "no-text-layer",
    }
}

pub(super) fn text_layer_observation_summary(text_layer: Option<&BTreeSet<String>>) -> String {
    match text_layer {
        None => "no-text-layer".to_string(),
        Some(codes) if codes.is_empty() => "text-no-exact-code".to_string(),
        Some(codes) => format!(
            "text-codes={}",
            codes.iter().cloned().collect::<Vec<_>>().join(",")
        ),
    }
}

pub(super) fn rfi_observation_summary(observations: &[(i32, RfiMatch)]) -> String {
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

pub(super) fn unconfirmed_page_source(
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

pub(super) fn select_rfi_consensus(
    observations: &[(i32, RfiMatch)],
    text_layer: Option<&BTreeSet<String>>,
) -> Option<(RfiMatch, usize, usize)> {
    const MIN_IDENTITY_VOTES: usize = 2;
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
    let mut identity = observations.iter().filter_map(|(_, matched)| {
        ((matched.method == RfiMethod::Strict || matched.method == RfiMethod::OcrAlias)
            && matched.code == code)
            .then_some(matched)
    });
    let mut matched = identity.next()?.clone();
    let identity_votes = 1 + identity.count();
    if observations
        .iter()
        .any(|(_, observed)| observed.code == code && observed.method == RfiMethod::OcrAlias)
    {
        matched.method = RfiMethod::OcrAlias;
    }
    let dpis = observations
        .iter()
        .filter(|(_, matched)| matched.ok() && matched.code == code)
        .map(|(dpi, _)| *dpi)
        .collect::<BTreeSet<_>>();
    if identity_votes < MIN_IDENTITY_VOTES || dpis.len() < MIN_DISTINCT_DPI {
        return None;
    }
    matched.confidence = CONSENSUS_CONFIDENCE;
    Some((matched, identity_votes, dpis.len()))
}
