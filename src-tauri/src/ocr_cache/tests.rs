use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::rfi_code::find;

struct CountingOcr {
    full: AtomicUsize,
    identity: AtomicUsize,
    fail: bool,
}

impl CountingOcr {
    fn new(fail: bool) -> Self {
        Self {
            full: AtomicUsize::new(0),
            identity: AtomicUsize::new(0),
            fail,
        }
    }
}

impl OcrAdapter for CountingOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        self.read_page(path, 0)
    }

    fn read_page(&self, _path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.full.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(OcrFailure::Document("unreadable".to_string()));
        }
        Ok(OcrRead {
            matched: find(&format!("AZE-SUN-INT-QMS-RFI-CIV-{:04}", page_index + 1)),
            source: "ocr:full".to_string(),
        })
    }

    fn read_identity_page(&self, _path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.identity.fetch_add(1, Ordering::SeqCst);
        Ok(OcrRead {
            matched: find(&format!("AZE-SUN-INT-QMS-RFI-MEC-{:04}", page_index + 1)),
            source: "ocr:fast-cover:confirmed:150dpi".to_string(),
        })
    }

    fn diagnostics(&self) -> String {
        "counting".to_string()
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    index: ScanIndex,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("scans");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.pdf"), b"one").unwrap();
    let index = ScanIndex::new(dir.path().join("index.sqlite3")).unwrap();
    Fixture {
        _dir: dir,
        root,
        index,
    }
}

#[test]
fn unchanged_files_are_answered_from_the_cache_with_identical_reads() {
    let f = fixture();
    let inner = CountingOcr::new(false);
    let cache = CachingOcr::new(&inner, &f.index, "owner", &f.root);
    let path = f.root.join("a.pdf");
    let first = cache.read_identity_page(&path, 2).unwrap();
    let second = cache.read_identity_page(&path, 2).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.matched.code, "AZE-SUN-INT-QMS-RFI-MEC-0003");
    assert_eq!(inner.identity.load(Ordering::SeqCst), 1);
    assert_eq!(cache.stats().reused, 1);
    assert_eq!(cache.stats().analyzed, 1);
    drop(cache);

    // A new cache object (a later preview) reuses the durable entry.
    let later = CachingOcr::new(&inner, &f.index, "owner", &f.root);
    assert_eq!(later.read_identity_page(&path, 2).unwrap(), first);
    assert_eq!(inner.identity.load(Ordering::SeqCst), 1);
}

#[test]
fn a_changed_file_is_read_again() {
    let f = fixture();
    let inner = CountingOcr::new(false);
    let path = f.root.join("a.pdf");
    CachingOcr::new(&inner, &f.index, "owner", &f.root)
        .read_identity_page(&path, 0)
        .unwrap();
    fs::write(&path, b"longer content now").unwrap();
    CachingOcr::new(&inner, &f.index, "owner", &f.root)
        .read_identity_page(&path, 0)
        .unwrap();
    assert_eq!(inner.identity.load(Ordering::SeqCst), 2);
}

#[test]
fn modes_pages_and_owners_never_share_entries() {
    let f = fixture();
    let inner = CountingOcr::new(false);
    let path = f.root.join("a.pdf");
    let cache = CachingOcr::new(&inner, &f.index, "owner", &f.root);
    let identity = cache.read_identity_page(&path, 0).unwrap();
    let full = cache.read_page(&path, 0).unwrap();
    assert_ne!(identity.source, full.source);
    cache.read_identity_page(&path, 1).unwrap();
    assert_eq!(inner.identity.load(Ordering::SeqCst), 2);
    assert_eq!(inner.full.load(Ordering::SeqCst), 1);

    CachingOcr::new(&inner, &f.index, "someone-else", &f.root)
        .read_identity_page(&path, 0)
        .unwrap();
    assert_eq!(inner.identity.load(Ordering::SeqCst), 3);
}

#[test]
fn failures_are_never_cached() {
    let f = fixture();
    let inner = CountingOcr::new(true);
    let path = f.root.join("a.pdf");
    let cache = CachingOcr::new(&inner, &f.index, "owner", &f.root);
    assert!(cache.read_page(&path, 0).is_err());
    assert!(cache.read_page(&path, 0).is_err());
    assert_eq!(inner.full.load(Ordering::SeqCst), 2);
}

#[test]
fn files_outside_the_root_bypass_the_cache() {
    let f = fixture();
    let inner = CountingOcr::new(false);
    let outside = f._dir.path().join("elsewhere.pdf");
    fs::write(&outside, b"x").unwrap();
    let cache = CachingOcr::new(&inner, &f.index, "owner", &f.root);
    cache.read_identity_page(&outside, 0).unwrap();
    cache.read_identity_page(&outside, 0).unwrap();
    assert_eq!(inner.identity.load(Ordering::SeqCst), 2);
}

#[test]
fn a_corrupt_entry_falls_back_to_a_fresh_read() {
    let f = fixture();
    let inner = CountingOcr::new(false);
    let path = f.root.join("a.pdf");
    let cache = CachingOcr::new(&inner, &f.index, "owner", &f.root);
    cache.read_identity_page(&path, 0).unwrap();
    drop(cache);
    // Same key, wrong shape: it must be ignored, not trusted.
    let fingerprint = FileFingerprint::read(&path).unwrap();
    let (run, _) = f
        .index
        .begin_run(
            "owner",
            OPERATION,
            &profile(Mode::Identity),
            &fs::canonicalize(&f.root).unwrap(),
            false,
        )
        .unwrap();
    run.record_success(Path::new("a.pdf#p0"), &fingerprint, &"garbage")
        .unwrap();
    drop(run);
    let again = CachingOcr::new(&inner, &f.index, "owner", &f.root);
    assert!(again.read_identity_page(&path, 0).is_ok());
    assert_eq!(inner.identity.load(Ordering::SeqCst), 2);
}

#[test]
fn an_unreadable_index_never_blocks_reading() {
    let f = fixture();
    let inner = CountingOcr::new(false);
    let cache = CachingOcr::new(&inner, &f.index, "", &f.root); // invalid owner key
    assert!(cache.read_identity_page(&f.root.join("a.pdf"), 0).is_ok());
    assert!(cache.read_identity_page(&f.root.join("a.pdf"), 0).is_ok());
    assert_eq!(inner.identity.load(Ordering::SeqCst), 2);
}
