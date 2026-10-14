#[allow(dead_code)]
mod support;

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};

use docflow_lib::{
    contracts::{Plan, StepKind},
    fs_ops::{FilesystemAuthority, file_sha256},
    ocr::NativeOcr,
    operations::pdf_orientation::{
        OPERATION_ID, OrientationDetection, OrientationDetector, PdfOrientationParams,
        orientation_target_rotation, preview, preview_indexed,
    },
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    scan_index::{FileFingerprint, ScanIndex},
};
use support::pdf_fixture::write_rich_archive;

#[derive(Default)]
struct FakeDetector {
    pages: BTreeMap<usize, OrientationDetection>,
}

struct CountingDetector {
    inner: FakeDetector,
    calls: AtomicUsize,
}

impl OrientationDetector for CountingDetector {
    fn detect_page(&self, path: &Path, page_index: usize) -> Result<OrientationDetection, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.detect_page(path, page_index)
    }

    fn diagnostics(&self) -> String {
        self.inner.diagnostics()
    }
}

impl OrientationDetector for FakeDetector {
    fn detect_page(&self, _path: &Path, page_index: usize) -> Result<OrientationDetection, String> {
        self.pages
            .get(&page_index)
            .cloned()
            .ok_or_else(|| "missing fake result".to_string())
    }

    fn diagnostics(&self) -> String {
        "fake orientation detector".to_string()
    }
}

fn replace_pdf_text_preserving_fingerprint(path: &Path) -> Result<(), String> {
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| error.to_string())?;
    let mut bytes = fs::read(path).map_err(|error| error.to_string())?;
    let before = b"ARCHIVE OLD FIRST PAGE";
    let after = b"ARCHIVE NEW FIRST PAGE";
    let start = bytes
        .windows(before.len())
        .position(|window| window == before)
        .ok_or_else(|| "replacement sentinel is missing".to_string())?;
    bytes[start..start + before.len()].copy_from_slice(after);
    fs::write(path, bytes).map_err(|error| error.to_string())?;
    fs::File::open(path)
        .and_then(|file| file.set_times(fs::FileTimes::new().set_modified(modified)))
        .map_err(|error| error.to_string())
}
struct SameFingerprintReplacementDetector {
    replaced: AtomicBool,
}

impl OrientationDetector for SameFingerprintReplacementDetector {
    fn detect_page(&self, path: &Path, _page_index: usize) -> Result<OrientationDetection, String> {
        if !self.replaced.swap(true, Ordering::SeqCst) {
            replace_pdf_text_preserving_fingerprint(path)?;
        }
        Ok(detection(90, 18.0))
    }

    fn diagnostics(&self) -> String {
        "same-fingerprint replacement fixture".to_string()
    }
}

#[test]
#[ignore = "requires an operator-supplied PDF and page numbers"]
fn operator_orientation_pages_match_visual_review() {
    let path = std::env::var_os("DOCFLOW_OPERATOR_ORIENTATION_PDF")
        .map(std::path::PathBuf::from)
        .expect("DOCFLOW_OPERATOR_ORIENTATION_PDF is required");
    let pages = std::env::var("DOCFLOW_OPERATOR_ORIENTATION_PAGES")
        .expect("DOCFLOW_OPERATOR_ORIENTATION_PAGES is required");
    let ocr = std::env::var_os("DOCFLOW_OPERATOR_TESSERACT")
        .map(NativeOcr::with_tesseract)
        .unwrap_or_else(NativeOcr::discover);
    for entry in pages.split(',') {
        let (page, expected) = entry.split_once(':').expect("page:rotation");
        let page_number = page.parse::<usize>().expect("page number");
        let detected = ocr.detect_page(&path, page_number - 1);
        if expected == "review" {
            assert!(detected.is_err(), "page {page_number} must stay in Review");
        } else {
            let expected = expected.parse::<i64>().expect("expected rotation");
            let detected = detected.unwrap_or_else(|error| panic!("page {page_number}: {error}"));
            assert_eq!(detected.rotate_clockwise, expected, "page {page_number}");
        }
    }
}

#[test]
#[ignore = "requires an operator-supplied RFI tree and matching fragment"]
fn operator_orientation_directory_preview_reaches_expected_pdf() {
    let root = std::env::var_os("DOCFLOW_OPERATOR_ORIENTATION_ROOT")
        .map(std::path::PathBuf::from)
        .expect("DOCFLOW_OPERATOR_ORIENTATION_ROOT is required");
    let fragment = std::env::var("DOCFLOW_OPERATOR_ORIENTATION_EXPECTED_FRAGMENT")
        .expect("DOCFLOW_OPERATOR_ORIENTATION_EXPECTED_FRAGMENT is required");
    let ocr = std::env::var_os("DOCFLOW_OPERATOR_TESSERACT")
        .map(NativeOcr::with_tesseract)
        .unwrap_or_else(NativeOcr::discover);
    let review = tempfile::tempdir().unwrap();
    let plan = preview(
        &PdfOrientationParams {
            rfi_system_root: root,
            review_root: review.path().to_path_buf(),
        },
        &ocr,
        |_| {},
        || false,
    )
    .unwrap();
    let warnings = plan
        .warnings
        .iter()
        .filter(|warning| warning.subject.contains(&fragment))
        .collect::<Vec<_>>();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        plan.steps.iter().any(|step| {
            step.kind == StepKind::PdfPageRotate
                && step
                    .src
                    .as_deref()
                    .is_some_and(|path| path.contains(&fragment))
        }),
        "expected a review copy plan for {fragment}"
    );
}

#[test]
#[ignore = "requires an operator-supplied RFI tree with a visually verified uncertain page"]
fn operator_uncertain_orientation_page_is_reported_without_rotating() {
    let root = std::env::var_os("DOCFLOW_OPERATOR_ORIENTATION_ROOT")
        .map(std::path::PathBuf::from)
        .expect("DOCFLOW_OPERATOR_ORIENTATION_ROOT is required");
    let fragment = std::env::var("DOCFLOW_OPERATOR_ORIENTATION_EXPECTED_FRAGMENT")
        .expect("DOCFLOW_OPERATOR_ORIENTATION_EXPECTED_FRAGMENT is required");
    let ocr = std::env::var_os("DOCFLOW_OPERATOR_TESSERACT")
        .map(NativeOcr::with_tesseract)
        .unwrap_or_else(NativeOcr::discover);
    let review = tempfile::tempdir().unwrap();
    let plan = preview(
        &PdfOrientationParams {
            rfi_system_root: root,
            review_root: review.path().to_path_buf(),
        },
        &ocr,
        |_| {},
        || false,
    )
    .unwrap();
    assert!(
        plan.warnings.iter().any(|warning| {
            warning.key == "warn.rfi.orientationNeedsReview"
                && warning.subject.contains(&fragment)
                && warning.subject.contains("page 2 orientation uncertain")
        }),
        "expected a page-2 review warning for {fragment}; warnings={:?}",
        plan.warnings
    );
    assert!(
        !plan.steps.iter().any(|step| {
            step.kind == StepKind::PdfPageRotate
                && step
                    .src
                    .as_deref()
                    .is_some_and(|path| path.contains(&fragment))
        }),
        "an uncertain file must not be rotated automatically"
    );
}

fn detection(rotate_clockwise: i64, confidence: f64) -> OrientationDetection {
    OrientationDetection {
        rotate_clockwise,
        confidence,
        method: "fake".to_string(),
    }
}

#[test]
fn orientation_targets_canonicalize_metadata_and_reject_unsafe_inputs() {
    for (current, correction, expected) in [
        (-90, 90, Some(0)),
        (0, 0, None),
        (0, 90, Some(90)),
        (0, 180, Some(180)),
        (0, 270, Some(270)),
        (450, 270, Some(0)),
        (810, 270, Some(0)),
        (90, 90, Some(180)),
        (90, 270, Some(0)),
        (270, 90, Some(0)),
        (180, 90, Some(270)),
    ] {
        assert_eq!(
            orientation_target_rotation(current, &detection(correction, 10.0)),
            Ok(expected)
        );
    }
    assert_eq!(
        orientation_target_rotation(450, &detection(0, 10.0)),
        Ok(None)
    );
    assert!(orientation_target_rotation(45, &detection(90, 20.0)).is_err());
    assert!(orientation_target_rotation(0, &detection(45, 20.0)).is_err());
    assert!(orientation_target_rotation(0, &detection(90, 9.9)).is_err());
    assert!(orientation_target_rotation(0, &detection(90, f64::NAN)).is_err());
}

#[test]
fn preview_rejects_same_fingerprint_content_change_during_detection() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("system");
    let review = fixture.path().join("review");
    let pdf = source.join("CIV/RFI-0001/document.pdf");
    fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    fs::create_dir_all(&review).unwrap();
    write_rich_archive(&pdf);
    let original_fingerprint = FileFingerprint::read(&pdf).unwrap();
    let original_hash = file_sha256(&pdf).unwrap();

    let result = preview(
        &PdfOrientationParams {
            rfi_system_root: source,
            review_root: review,
        },
        &SameFingerprintReplacementDetector {
            replaced: AtomicBool::new(false),
        },
        |_| {},
        || false,
    );

    assert!(result.is_err());
    assert_eq!(FileFingerprint::read(&pdf).unwrap(), original_fingerprint);
    assert_ne!(file_sha256(&pdf).unwrap(), original_hash);
    assert_eq!(docflow_lib::pdf::inspect(&pdf).unwrap().page_count, 3);
}
#[test]
fn preview_scans_only_discipline_trees_and_proposes_confident_rotations() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("system");
    let review = fixture.path().join("review");
    let pdf = source.join("01_RFI/MEC/RFI-0001/document.pdf");
    let ignored = source.join("Certificates/not-an-rfi.pdf");
    fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    fs::create_dir_all(ignored.parent().unwrap()).unwrap();
    fs::create_dir_all(&review).unwrap();
    write_rich_archive(&pdf);
    write_rich_archive(&ignored);
    let detector = FakeDetector {
        pages: BTreeMap::from([
            (0, detection(90, 18.0)),
            (1, detection(0, 22.0)),
            (2, detection(180, 15.0)),
        ]),
    };

    let plan = preview(
        &PdfOrientationParams {
            rfi_system_root: source.clone(),
            review_root: review.clone(),
        },
        &detector,
        |_| {},
        || false,
    )
    .unwrap();

    let rotate = plan
        .steps
        .iter()
        .find(|step| step.kind == StepKind::PdfPageRotate)
        .expect("one PDF rotation step");
    assert_eq!(
        rotate.src.as_deref(),
        Some(fs::canonicalize(&pdf).unwrap().to_string_lossy().as_ref())
    );
    assert_eq!(
        rotate.dst.as_deref(),
        Some(
            fs::canonicalize(&review)
                .unwrap()
                .join("01_RFI/MEC/RFI-0001/document.pdf")
                .to_string_lossy()
                .as_ref()
        )
    );
    let rotations = rotate.meta["rotations"].as_array().unwrap();
    assert_eq!(rotations.len(), 2);
    assert_eq!(rotations[0]["pageIndex"], 0);
    assert_eq!(rotations[0]["targetRotation"], 90);
    assert_eq!(rotations[1]["pageIndex"], 2);
    assert_eq!(rotations[1]["targetRotation"], 180);
    assert_eq!(plan.meta["verifiedPages"], 3);
    assert_eq!(plan.meta["rotatedPages"], 2);
    assert!(plan.warnings.is_empty());
}

#[test]
fn unchanged_orientation_pdf_is_reused_from_durable_index() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("system");
    let review = fixture.path().join("review");
    let pdf = source.join("MEC/RFI-0001/document.pdf");
    fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    fs::create_dir_all(&review).unwrap();
    write_rich_archive(&pdf);
    let second_pdf = source.join("PRC/RFI-0002/document.pdf");
    fs::create_dir_all(second_pdf.parent().unwrap()).unwrap();
    write_rich_archive(&second_pdf);
    let detector = CountingDetector {
        inner: FakeDetector {
            pages: BTreeMap::from([
                (0, detection(90, 18.0)),
                (1, detection(0, 22.0)),
                (2, detection(180, 15.0)),
            ]),
        },
        calls: AtomicUsize::new(0),
    };
    let scan_index = ScanIndex::new(fixture.path().join("scan-index.sqlite3")).unwrap();
    let params = PdfOrientationParams {
        rfi_system_root: source,
        review_root: review,
    };
    let first = preview_indexed(
        &params,
        &detector,
        |_| {},
        || false,
        Some(&scan_index),
        "owner",
        false,
    )
    .unwrap();
    assert_eq!(first.meta["scanIndex"]["analyzed"], 2);
    let expected_workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .saturating_sub(1)
        .clamp(1, 2);
    assert_eq!(first.meta["inspectionWorkers"], expected_workers as u64);
    assert_eq!(detector.calls.load(Ordering::SeqCst), 6);

    let mut second_progress = Vec::new();
    let second = preview_indexed(
        &params,
        &detector,
        |event| second_progress.push(event),
        || false,
        Some(&scan_index),
        "owner",
        false,
    )
    .unwrap();
    assert_eq!(second.meta["scanIndex"]["reused"], 2);
    assert_eq!(second.meta["scanIndex"]["analyzed"], 0);
    assert_eq!(detector.calls.load(Ordering::SeqCst), 6);
    let inspection_events = second_progress
        .iter()
        .filter(|event| event.phase == "orientation_inspection")
        .collect::<Vec<_>>();
    assert_eq!(inspection_events.first().unwrap().done, 0);
    assert_eq!(inspection_events.last().unwrap().done, 2);
    assert!(
        second_progress
            .iter()
            .filter(|event| event.phase == "orientation_analysis")
            .count()
            <= 2
    );
    assert_eq!(first.steps, second.steps);
    let original_fingerprint = FileFingerprint::read(&pdf).unwrap();
    let original_hash = file_sha256(&pdf).unwrap();
    replace_pdf_text_preserving_fingerprint(&pdf).unwrap();
    assert_eq!(FileFingerprint::read(&pdf).unwrap(), original_fingerprint);
    assert_ne!(file_sha256(&pdf).unwrap(), original_hash);
    let after_replacement = preview_indexed(
        &params,
        &detector,
        |_| {},
        || false,
        Some(&scan_index),
        "owner",
        false,
    )
    .unwrap();
    assert_eq!(after_replacement.meta["scanIndex"]["reused"], 1);
    assert_eq!(after_replacement.meta["scanIndex"]["analyzed"], 1);
    assert_eq!(detector.calls.load(Ordering::SeqCst), 9);

    let forced = preview_indexed(
        &params,
        &detector,
        |_| {},
        || false,
        Some(&scan_index),
        "owner",
        true,
    )
    .unwrap();
    assert_eq!(forced.meta["scanIndex"]["reused"], 0);
    assert_eq!(forced.meta["scanIndex"]["analyzed"], 2);
    assert_eq!(detector.calls.load(Ordering::SeqCst), 15);
}

#[test]
fn cached_and_new_pdfs_keep_deterministic_review_order() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("system");
    let review = fixture.path().join("review");
    let mec_pdf = source.join("MEC/RFI-0001/document.pdf");
    let prc_pdf = source.join("PRC/RFI-0002/document.pdf");
    fs::create_dir_all(mec_pdf.parent().unwrap()).unwrap();
    fs::create_dir_all(&review).unwrap();
    write_rich_archive(&mec_pdf);
    let detector = CountingDetector {
        inner: FakeDetector {
            pages: BTreeMap::from([
                (0, detection(90, 18.0)),
                (1, detection(0, 22.0)),
                (2, detection(0, 15.0)),
            ]),
        },
        calls: AtomicUsize::new(0),
    };
    let scan_index = ScanIndex::new(fixture.path().join("scan-index.sqlite3")).unwrap();
    let params = PdfOrientationParams {
        rfi_system_root: source,
        review_root: review,
    };
    preview_indexed(
        &params,
        &detector,
        |_| {},
        || false,
        Some(&scan_index),
        "owner",
        false,
    )
    .unwrap();

    fs::create_dir_all(prc_pdf.parent().unwrap()).unwrap();
    write_rich_archive(&prc_pdf);
    let mixed = preview_indexed(
        &params,
        &detector,
        |_| {},
        || false,
        Some(&scan_index),
        "owner",
        false,
    )
    .unwrap();
    assert_eq!(mixed.meta["scanIndex"]["reused"], 1);
    assert_eq!(mixed.meta["scanIndex"]["analyzed"], 1);
    assert_eq!(detector.calls.load(Ordering::SeqCst), 6);
    let sources = mixed
        .steps
        .iter()
        .filter(|step| step.kind == StepKind::PdfPageRotate)
        .map(|step| step.src.as_deref().unwrap())
        .collect::<Vec<_>>();
    let expected = [
        fs::canonicalize(mec_pdf)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        fs::canonicalize(prc_pdf)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    ];
    assert_eq!(sources, expected);
}

#[test]
fn one_uncertain_page_fails_the_whole_pdf_closed() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("system");
    let review = fixture.path().join("review");
    let pdf = source.join("CIV/RFI-0002/document.pdf");
    fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    fs::create_dir_all(&review).unwrap();
    write_rich_archive(&pdf);
    let detector = FakeDetector {
        pages: BTreeMap::from([
            (0, detection(90, 18.0)),
            (1, detection(0, 4.0)),
            (2, detection(0, 15.0)),
        ]),
    };

    let plan = preview(
        &PdfOrientationParams {
            rfi_system_root: source,
            review_root: review,
        },
        &detector,
        |_| {},
        || false,
    )
    .unwrap();

    assert!(
        plan.steps
            .iter()
            .all(|step| step.kind != StepKind::PdfPageRotate)
    );
    assert_eq!(plan.meta["manualReviewFiles"], 1);
    assert_eq!(plan.warnings[0].key, "warn.rfi.orientationNeedsReview");
}

#[cfg(unix)]
#[test]
fn orientation_discovery_never_follows_pdf_or_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("system");
    let review = fixture.path().join("review");
    let outside = fixture.path().join("outside");
    fs::create_dir_all(source.join("CIV")).unwrap();
    fs::create_dir_all(&review).unwrap();
    fs::create_dir_all(&outside).unwrap();
    write_rich_archive(&outside.join("outside.pdf"));
    symlink(&outside, source.join("CIV/linked-directory")).unwrap();
    symlink(
        outside.join("outside.pdf"),
        source.join("CIV/linked-file.pdf"),
    )
    .unwrap();

    let plan = preview(
        &PdfOrientationParams {
            rfi_system_root: source,
            review_root: review,
        },
        &FakeDetector::default(),
        |_| {},
        || false,
    )
    .unwrap();

    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.rfi.orientationNoPdfs");
}

#[test]
fn apply_preserves_source_publishes_no_clobber_and_undoes_exact_copy() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("system");
    let review = fixture.path().join("review");
    let pdf = source.join("MEC/RFI-0003/document.pdf");
    fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    fs::create_dir_all(&review).unwrap();
    write_rich_archive(&pdf);
    let source_hash = file_sha256(&pdf).unwrap();
    let detector = FakeDetector {
        pages: BTreeMap::from([
            (0, detection(90, 18.0)),
            (1, detection(0, 22.0)),
            (2, detection(0, 15.0)),
        ]),
    };
    let plan = preview(
        &PdfOrientationParams {
            rfi_system_root: source.clone(),
            review_root: review.clone(),
        },
        &detector,
        |_| {},
        || false,
    )
    .unwrap();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache_plan(&authority, plan, &source, &review);
    let caller = CallerIdentity::new("owner-1", "session-1");

    let result = authority.apply(&cached.plan_id, &caller).unwrap();
    let output = review.join("MEC/RFI-0003/document.pdf");
    assert_eq!((result.applied, result.failed), (3, 0));
    assert_eq!(file_sha256(&pdf).unwrap(), source_hash);
    assert_eq!(
        docflow_lib::pdf::inspect(&output).unwrap().pages[0].rotation,
        90
    );
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert!(journal.steps.iter().any(|step| step.kind == StepKind::Copy));

    let undone = authority.undo(&result.journal_id, &caller, None).unwrap();
    assert_eq!(undone.failed, 0);
    assert!(!output.exists());
}

#[test]
fn apply_rejects_source_drift_without_creating_a_review_pdf() {
    use std::io::Write;

    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("system");
    let review = fixture.path().join("review");
    let pdf = source.join("ELE/RFI-0004/document.pdf");
    fs::create_dir_all(pdf.parent().unwrap()).unwrap();
    fs::create_dir_all(&review).unwrap();
    write_rich_archive(&pdf);
    let detector = FakeDetector {
        pages: BTreeMap::from([
            (0, detection(90, 18.0)),
            (1, detection(0, 22.0)),
            (2, detection(0, 15.0)),
        ]),
    };
    let plan = preview(
        &PdfOrientationParams {
            rfi_system_root: source.clone(),
            review_root: review.clone(),
        },
        &detector,
        |_| {},
        || false,
    )
    .unwrap();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache_plan(&authority, plan, &source, &review);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&pdf)
        .unwrap()
        .write_all(b"\n% operator edit")
        .unwrap();

    let result = authority
        .apply(
            &cached.plan_id,
            &CallerIdentity::new("owner-1", "session-1"),
        )
        .unwrap();

    assert_eq!(result.failed, 1);
    assert!(!review.join("ELE/RFI-0004/document.pdf").exists());
    assert!(!review.join("ELE").exists());
}

fn cache_plan(authority: &FilesystemAuthority, plan: Plan, source: &Path, review: &Path) -> Plan {
    let scope = GrantScope::new("owner-1", "session-1", "preview-1", OPERATION_ID);
    let source_grant = authority
        .issue_grant(&scope, source, GrantAccess::Read)
        .unwrap();
    let review_grant = authority
        .issue_grant(&scope, review, GrantAccess::ReadWrite)
        .unwrap();
    authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                OPERATION_ID,
                vec![source_grant.grant_id, review_grant.grant_id],
            ),
            plan,
        )
        .unwrap()
}
