#[allow(dead_code)]
mod support;

use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use docflow_lib::{
    contracts::{Plan, StepKind},
    fs_ops::{FilesystemAuthority, file_sha256},
    operations::pdf_orientation::{
        OPERATION_ID, OrientationDetection, OrientationDetector, PdfOrientationParams, preview,
    },
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
};
use support::pdf_fixture::write_rich_archive;

#[derive(Default)]
struct FakeDetector {
    pages: BTreeMap<usize, OrientationDetection>,
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

fn detection(rotate_clockwise: i64, confidence: f64) -> OrientationDetection {
    OrientationDetection {
        rotate_clockwise,
        confidence,
        method: "fake".to_string(),
    }
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
