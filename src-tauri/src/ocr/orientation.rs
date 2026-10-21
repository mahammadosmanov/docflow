use super::*;

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
        let header = select_correlated_header_score_checked(&first_scores, &second_scores);
        let header_conflict = header.is_err();
        let header = header.ok().flatten();
        let second_conflict = header.as_ref().is_some_and(|header| {
            second
                .as_ref()
                .is_some_and(|direction| direction.rotate_clockwise != header.rotate_clockwise)
        });
        let detection = if header_conflict || second_conflict {
            None
        } else {
            let detection = match (second, header) {
                (Some(direction), _) => Some(direction),
                (None, header) => header,
            };
            detection.or_else(|| {
                select_correlated_direction_score(
                    &first_scores,
                    &second_scores,
                    primary_detection.as_ref(),
                    secondary_detection.as_ref(),
                )
            })
        };
        detection.ok_or_else(|| {
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
        let engine = self.tesseract.as_deref().map_or_else(
            || "unavailable".to_string(),
            |path| path.to_string_lossy().into_owned(),
        );
        let osd = self
            .tessdata
            .as_deref()
            .is_some_and(|directory| bundled_regular_file(&directory.join("osd.traineddata")));
        format!("tesseract={engine}; osd={osd}")
    }
}

pub(super) fn run_orientation_tesseract(
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
    let mut poll_count = 0_usize;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                poll_count = poll_count.wrapping_add(1);
                if poll_count.is_multiple_of(32) {
                    let output_size = fs::metadata(&stdout_path).map_or(0, |value| value.len())
                        + fs::metadata(&stderr_path).map_or(0, |value| value.len());
                    if output_size > MAX_OUTPUT_BYTES {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("orientation detector output exceeded the limit".to_string());
                    }
                }
                thread::sleep(Duration::from_millis(1));
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

pub(super) fn parse_orientation_output(
    output: &str,
    dpi: i32,
) -> Result<OrientationDetection, String> {
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

pub(super) fn select_osd_detection(
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
pub(super) struct DirectionScore {
    pub(super) rotate_clockwise: i64,
    pub(super) word_count: usize,
    pub(super) confidence_sum: f64,
    pub(super) header_word_count: usize,
    pub(super) header_confidence_sum: f64,
    pub(super) header_rfi_codes: BTreeMap<String, f64>,
}

impl DirectionScore {
    #[cfg(test)]
    pub(super) fn new(rotate_clockwise: i64, word_count: usize, confidence_sum: f64) -> Self {
        Self {
            rotate_clockwise,
            word_count,
            confidence_sum,
            header_word_count: 0,
            header_confidence_sum: 0.0,
            header_rfi_codes: BTreeMap::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_header(mut self, word_count: usize, confidence_sum: f64) -> Self {
        self.header_word_count = word_count;
        self.header_confidence_sum = confidence_sum;
        self
    }

    pub(super) fn average_confidence(&self) -> f64 {
        if self.word_count == 0 {
            0.0
        } else {
            self.confidence_sum / self.word_count as f64
        }
    }
}

pub(super) fn select_direction_score(
    scores: &[DirectionScore],
    dpi: i32,
) -> Option<OrientationDetection> {
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

pub(super) fn detect_orientation_by_direction_scores(
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

pub(super) fn direction_summary(scores: &[DirectionScore]) -> String {
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

pub(super) fn osd_summary(detection: Option<&OrientationDetection>) -> String {
    detection.map_or_else(
        || "unavailable".to_string(),
        |value| format!("{}°/{:.1}", value.rotate_clockwise, value.confidence),
    )
}

pub(super) fn select_correlated_direction_score(
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

pub(super) fn rotate_gray_clockwise(image: &GrayImage, degrees: i64) -> Result<GrayImage, String> {
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

pub(super) fn run_direction_tesseract(
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
    let mut poll_count = 0_usize;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                poll_count = poll_count.wrapping_add(1);
                if poll_count.is_multiple_of(32) {
                    let output_size = fs::metadata(&output_path).map_or(0, |value| value.len())
                        + fs::metadata(&error_path).map_or(0, |value| value.len());
                    if output_size > MAX_OUTPUT_BYTES {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("orientation fallback output exceeded the limit".to_string());
                    }
                }
                thread::sleep(Duration::from_millis(1));
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
    Ok(parse_direction_score_tsv(
        &output,
        rotate_clockwise,
        image_height,
    ))
}

pub(super) fn parse_direction_score_tsv(
    output: &str,
    rotate_clockwise: i64,
    image_height: usize,
) -> DirectionScore {
    const MIN_RFI_HEADER_CONFIDENCE: f64 = 60.0;
    let mut word_count = 0_usize;
    let mut confidence_sum = 0.0_f64;
    let mut header_word_count = 0_usize;
    let mut header_confidence_sum = 0.0_f64;
    let mut header_rfi_codes = BTreeMap::<String, f64>::new();
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
        if !confidence.is_finite() {
            continue;
        }
        let top = columns[7].parse::<usize>().unwrap_or(image_height);
        let height = columns[9].parse::<usize>().unwrap_or(0);
        let in_header = top.saturating_add(height / 2) <= image_height / 6;
        if in_header && confidence >= MIN_RFI_HEADER_CONFIDENCE {
            for code in exact_codes(columns[11]) {
                header_rfi_codes
                    .entry(code)
                    .and_modify(|best| *best = best.max(confidence))
                    .or_insert(confidence);
            }
        }
        if confidence >= 30.0 {
            word_count += 1;
            confidence_sum += confidence;
            if in_header {
                header_word_count += 1;
                header_confidence_sum += confidence;
            }
        }
    }
    DirectionScore {
        rotate_clockwise,
        word_count,
        confidence_sum,
        header_word_count,
        header_confidence_sum,
        header_rfi_codes,
    }
}

pub(super) fn select_correlated_rfi_header_score(
    primary: &[DirectionScore],
    secondary: &[DirectionScore],
) -> Result<Option<OrientationDetection>, ()> {
    fn unique_candidate(scores: &[DirectionScore]) -> Result<Option<(&DirectionScore, &str)>, ()> {
        const MIN_CODE_CONFIDENCE: f64 = 60.0;
        if scores.iter().any(|score| score.header_rfi_codes.len() > 1) {
            return Err(());
        }
        let mut candidates = scores.iter().filter_map(|score| {
            let (code, confidence) = score.header_rfi_codes.iter().next()?;
            (*confidence >= MIN_CODE_CONFIDENCE).then_some((score, code.as_str()))
        });
        let candidate = candidates.next();
        if candidates.next().is_some() {
            return Err(());
        }
        Ok(candidate)
    }

    match (unique_candidate(primary)?, unique_candidate(secondary)?) {
        (None, None) => Ok(None),
        (Some((first, first_code)), Some((second, second_code)))
            if first.rotate_clockwise == second.rotate_clockwise && first_code == second_code =>
        {
            Ok(Some(OrientationDetection {
                rotate_clockwise: first.rotate_clockwise,
                confidence: 10.0,
                method: "tesseract-rfi-header-consensus:250+350dpi".to_string(),
            }))
        }
        _ => Err(()),
    }
}

#[cfg(test)]
pub(super) fn select_correlated_header_score(
    primary: &[DirectionScore],
    secondary: &[DirectionScore],
) -> Option<OrientationDetection> {
    select_correlated_header_score_checked(primary, secondary)
        .ok()
        .flatten()
}

pub(super) fn select_correlated_header_score_checked(
    primary: &[DirectionScore],
    secondary: &[DirectionScore],
) -> Result<Option<OrientationDetection>, ()> {
    let rfi_header = select_correlated_rfi_header_score(primary, secondary)?;
    let text_header = select_correlated_text_header_score(primary, secondary);
    match (rfi_header, text_header) {
        (Some(code), Some(text)) if code.rotate_clockwise != text.rotate_clockwise => Err(()),
        (Some(code), _) => Ok(Some(code)),
        (None, text) => Ok(text),
    }
}

pub(super) fn select_correlated_text_header_score(
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

pub(super) fn tesseract_candidates() -> Vec<PathBuf> {
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

pub(super) fn bundled_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && !metadata_is_reparse_point(&metadata)
    })
}
