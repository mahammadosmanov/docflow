use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::LazyLock,
    thread,
    time::{Duration, Instant},
};

use regex::Regex;

use crate::{
    operations::pdf_orientation::{OrientationDetection, OrientationDetector},
    operations::process_scans::{OcrAdapter, OcrFailure, OcrRead},
    rfi_code::{PREFIX, RfiMatch, RfiMethod, canonical, exact_codes, find},
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

mod adapter;
#[cfg(test)]
use adapter::*;
mod cover;
mod image;
mod orientation;
mod render;
mod session;
mod tesseract;
pub use cover::take_cover_trace;
use cover::*;
pub use image::*;
use orientation::*;
pub use render::*;
pub(crate) use tesseract::*;

#[cfg(test)]
mod tests;

/// Regression lab for the production fast-cover detector.
#[cfg(test)]
mod cover_detection_tests;

#[cfg(test)]
mod orientation_tests;
