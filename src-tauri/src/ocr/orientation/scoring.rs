use super::*;

#[derive(Clone, Debug)]
pub(in crate::ocr) struct DirectionScore {
    pub(in crate::ocr) rotate_clockwise: i64,
    pub(in crate::ocr) word_count: usize,
    pub(in crate::ocr) confidence_sum: f64,
    pub(in crate::ocr) header_word_count: usize,
    pub(in crate::ocr) header_confidence_sum: f64,
    pub(in crate::ocr) header_rfi_codes: BTreeMap<String, f64>,
}

impl DirectionScore {
    #[cfg(test)]
    pub(in crate::ocr) fn new(
        rotate_clockwise: i64,
        word_count: usize,
        confidence_sum: f64,
    ) -> Self {
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
    pub(in crate::ocr) fn with_header(mut self, word_count: usize, confidence_sum: f64) -> Self {
        self.header_word_count = word_count;
        self.header_confidence_sum = confidence_sum;
        self
    }

    pub(in crate::ocr) fn average_confidence(&self) -> f64 {
        if self.word_count == 0 {
            0.0
        } else {
            self.confidence_sum / self.word_count as f64
        }
    }
}

pub(in crate::ocr) fn select_direction_score(
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

pub(in crate::ocr) fn detect_orientation_by_direction_scores(
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

pub(in crate::ocr) fn direction_summary(scores: &[DirectionScore]) -> String {
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

pub(in crate::ocr) fn osd_summary(detection: Option<&OrientationDetection>) -> String {
    detection.map_or_else(
        || "unavailable".to_string(),
        |value| format!("{}°/{:.1}", value.rotate_clockwise, value.confidence),
    )
}

pub(in crate::ocr) fn select_correlated_direction_score(
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

pub(in crate::ocr) fn rotate_gray_clockwise(
    image: &GrayImage,
    degrees: i64,
) -> Result<GrayImage, String> {
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
