//! Durable per-page OCR identity cache. It is an optimization only: any index
//! problem, corrupt entry, or file outside the cached root falls back to a
//! fresh read, and failures are never stored. Entries are owner-scoped and keyed
//! by the file fingerprint (size, modified time, identity), so a changed file
//! is always read again. Mutating operations still pin and revalidate their own
//! hashes before Apply.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    operations::process_scans::{OcrAdapter, OcrFailure, OcrRead},
    rfi_code::{RfiMatch, RfiMethod},
    scan_index::{FileFingerprint, ScanIndex, ScanIndexStats, ScanRun},
};

#[cfg(test)]
mod tests;

const OPERATION: &str = "ocr.page_cache";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Mode {
    Full,
    Cover,
    Identity,
}

/// The profile includes the app version so any change to OCR logic starts fresh.
fn profile(mode: Mode) -> String {
    let name = match mode {
        Mode::Full => "full",
        Mode::Cover => "cover",
        Mode::Identity => "identity",
    };
    format!("{name}-{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedRead {
    code: String,
    discipline: String,
    number: String,
    method: String,
    confidence: f64,
    source: String,
}

fn method_name(method: RfiMethod) -> &'static str {
    match method {
        RfiMethod::Strict => "strict",
        RfiMethod::Repaired => "repaired",
        RfiMethod::OcrAlias => "ocr_alias",
        RfiMethod::Fuzzy => "fuzzy",
        RfiMethod::Miss => "miss",
    }
}

fn method_from(name: &str) -> Option<RfiMethod> {
    Some(match name {
        "strict" => RfiMethod::Strict,
        "repaired" => RfiMethod::Repaired,
        "ocr_alias" => RfiMethod::OcrAlias,
        "fuzzy" => RfiMethod::Fuzzy,
        "miss" => RfiMethod::Miss,
        _ => return None,
    })
}

impl CachedRead {
    fn from_read(read: &OcrRead) -> Self {
        Self {
            code: read.matched.code.clone(),
            discipline: read.matched.discipline.clone(),
            number: read.matched.number.clone(),
            method: method_name(read.matched.method).to_string(),
            confidence: read.matched.confidence,
            source: read.source.clone(),
        }
    }

    fn into_read(self) -> Option<OcrRead> {
        if !self.confidence.is_finite() || !(0.0..=1.0).contains(&self.confidence) {
            return None;
        }
        Some(OcrRead {
            matched: RfiMatch {
                code: self.code,
                discipline: self.discipline,
                number: self.number,
                method: method_from(&self.method)?,
                confidence: self.confidence,
            },
            source: self.source,
        })
    }
}

pub struct CachingOcr<'a, A: OcrAdapter> {
    inner: &'a A,
    index: &'a ScanIndex,
    owner_id: String,
    root: PathBuf,
    runs: Mutex<HashMap<Mode, Option<ScanRun<'a>>>>,
    fingerprints: Mutex<HashMap<PathBuf, Option<FileFingerprint>>>,
    reused: AtomicU64,
    analyzed: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl<'a, A: OcrAdapter> CachingOcr<'a, A> {
    pub fn new(inner: &'a A, index: &'a ScanIndex, owner_id: &str, root: &Path) -> Self {
        Self {
            inner,
            index,
            owner_id: owner_id.to_string(),
            root: std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()),
            runs: Mutex::new(HashMap::new()),
            fingerprints: Mutex::new(HashMap::new()),
            reused: AtomicU64::new(0),
            analyzed: AtomicU64::new(0),
        }
    }

    pub fn stats(&self) -> ScanIndexStats {
        ScanIndexStats {
            reused: self.reused.load(Ordering::Relaxed),
            analyzed: self.analyzed.load(Ordering::Relaxed),
            ..ScanIndexStats::default()
        }
    }

    fn key(&self, path: &Path, page_index: usize) -> Option<PathBuf> {
        let relative = match path.strip_prefix(&self.root) {
            Ok(relative) => relative.to_path_buf(),
            Err(_) => std::fs::canonicalize(path)
                .ok()?
                .strip_prefix(&self.root)
                .ok()?
                .to_path_buf(),
        };
        Some(PathBuf::from(format!(
            "{}#p{page_index}",
            relative.to_string_lossy()
        )))
    }

    fn fingerprint(&self, path: &Path) -> Option<FileFingerprint> {
        let mut known = lock(&self.fingerprints);
        known
            .entry(path.to_path_buf())
            .or_insert_with(|| FileFingerprint::read(path).ok())
            .clone()
    }

    fn with_run<T>(&self, mode: Mode, action: impl FnOnce(&ScanRun<'a>) -> Option<T>) -> Option<T> {
        let mut runs = lock(&self.runs);
        let run = runs.entry(mode).or_insert_with(|| {
            self.index
                .begin_run(&self.owner_id, OPERATION, &profile(mode), &self.root, false)
                .ok()
                .map(|(run, _)| run)
        });
        run.as_ref().and_then(action)
    }

    fn cached_read(
        &self,
        mode: Mode,
        path: &Path,
        page_index: usize,
        read: impl FnOnce() -> Result<OcrRead, OcrFailure>,
    ) -> Result<OcrRead, OcrFailure> {
        let (Some(key), Some(fingerprint)) = (self.key(path, page_index), self.fingerprint(path))
        else {
            return read();
        };
        let hit = self.with_run(mode, |run| {
            run.lookup::<CachedRead>(&key, &fingerprint)
                .ok()
                .flatten()
                .and_then(CachedRead::into_read)
        });
        if let Some(hit) = hit {
            self.reused.fetch_add(1, Ordering::Relaxed);
            return Ok(hit);
        }
        let fresh = read()?;
        self.analyzed.fetch_add(1, Ordering::Relaxed);
        let _ = self.with_run(mode, |run| {
            run.record_success(&key, &fingerprint, &CachedRead::from_read(&fresh))
                .ok()
        });
        Ok(fresh)
    }
}

impl<A: OcrAdapter> Drop for CachingOcr<'_, A> {
    /// Close the index runs so later previews are not treated as interrupted.
    fn drop(&mut self) {
        let stats = self.stats();
        for (_, run) in lock(&self.runs).drain() {
            if let Some(run) = run {
                let _ = run.complete(&stats);
            }
        }
    }
}

impl<A: OcrAdapter> OcrAdapter for CachingOcr<'_, A> {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        self.read_page(path, 0)
    }

    fn read_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.cached_read(Mode::Full, path, page_index, || {
            self.inner.read_page(path, page_index)
        })
    }

    fn read_cover_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.cached_read(Mode::Cover, path, page_index, || {
            self.inner.read_cover_page(path, page_index)
        })
    }

    fn read_identity_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.cached_read(Mode::Identity, path, page_index, || {
            self.inner.read_identity_page(path, page_index)
        })
    }

    fn supports_page_scan(&self) -> bool {
        self.inner.supports_page_scan()
    }

    fn inspect_pdf(&self, path: &Path) -> Option<Result<crate::pdf::PdfInspection, String>> {
        self.inner.inspect_pdf(path)
    }

    fn supports_page_previews(&self) -> bool {
        self.inner.supports_page_previews()
    }

    fn supports_parallel_analysis(&self) -> bool {
        self.inner.supports_parallel_analysis()
    }

    fn supports_orientation(&self) -> bool {
        self.inner.supports_orientation()
    }

    fn detect_orientation(
        &self,
        path: &Path,
        page_index: usize,
    ) -> Result<crate::operations::pdf_orientation::OrientationDetection, String> {
        self.inner.detect_orientation(path, page_index)
    }

    fn diagnostics(&self) -> String {
        self.inner.diagnostics()
    }
}
