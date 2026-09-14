use docflow_lib::naming::{filename_for, flat_target_for, folder_for, target_for};
use docflow_lib::ocr::NativeOcr;
#[cfg(target_os = "macos")]
use docflow_lib::ocr::{image_pdf_from_png, render_first_page_png};
use docflow_lib::operations::process_scans::{
    OcrAdapter, OcrFailure, OcrRead, ProcessScansParams, preview,
};
use docflow_lib::rfi_code::{RfiMethod, canonical, canonical_and_find, find};
use docflow_lib::{
    fs_ops::FilesystemAuthority,
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    review::{ReviewAction, ReviewDecision, preview as review_preview, scan as review_scan},
};

#[test]
fn rfi_matching_preserves_nfc_uppercase_and_the_strict_anchor() {
    let strict = find(concat!(
        "REQUEST FOR INSPECTION 18.02.2026 REI N0 ",
        "AZE-SUN-INT-QMS-ITP-MEC-0001 ... ",
        "AZE-SUN-INT-ENG-STR-DRG-0901 ... ",
        "aze-sun-int-qms-rfi-mec-0107"
    ));
    assert_eq!(strict.code, "AZE-SUN-INT-QMS-RFI-MEC-0107");
    assert_eq!(strict.method, RfiMethod::Strict);
    assert_eq!(strict.confidence, 1.0);

    for damaged in [
        "AZE-SUN-INT-QMS-RFI-MEC-O1O7",
        "AZE-SUN-INT-QMS-RFI-MEC-Ol07",
        "4ZE-SUN-1NT-QMS-RFI-MEC-0107",
    ] {
        let repaired = find(damaged);
        assert_eq!(repaired.code, "AZE-SUN-INT-QMS-RFI-MEC-0107");
        assert_eq!(repaired.method, RfiMethod::Repaired);
    }
    let recovered_discipline = find("AZE-SUN-INT-QMS-RFI-CV-0064");
    assert_eq!(recovered_discipline.code, "AZE-SUN-INT-QMS-RFI-CIV-0064");
    assert_eq!(recovered_discipline.confidence, 0.9);
    let fuzzy = find("AZE-SUN-INT-QMX-RFI-MEC-0107");
    assert_eq!(fuzzy.method, RfiMethod::Fuzzy);
    assert_eq!(fuzzy.confidence, 0.9);
    assert!(!find("AZE-SUN-INT-QMS-ITP-MEC-0001").ok());
    assert!(!find("AZE-SUN-INT-ENG-STR-DRG-0901 S-205").ok());
    assert_eq!(canonical("I\u{0307}NT"), "İNT");
}

#[test]
fn rfi_matching_recovers_a_number_wrapped_after_the_final_anchor_hyphen() {
    let wrapped = find("RFI No: AZE-SUN-INT-QMS-RFI-CIV-\n0064");
    let blank_line = find("RFI No: AZE-SUN-INT-QMS-RFI-CIV-\r\n \r\n0064");

    assert_eq!(wrapped.code, "AZE-SUN-INT-QMS-RFI-CIV-0064");
    assert_eq!(wrapped.method, RfiMethod::Strict);
    assert_eq!(wrapped.confidence, 1.0);
    assert_eq!(blank_line.code, wrapped.code);
    assert!(
        !find("AZE-SUN-INT-QMS-RFI-CIV-\nRemarks 0064").ok(),
        "unrelated text must not be joined into a false RFI label"
    );
    assert!(
        !find("AZE-SUN-INT-QMS-RFI-CIV-\n\n\n0064").ok(),
        "wrapped recovery must not cross more than one blank line"
    );
    for malformed in [
        "AZE-SUN-INT-QMS-RFI-CIV-00645",
        "AZE-SUN-INT-QMS-RFI-CIV-٠٠٦٤",
        "AZE-SUN-INT-QMS-RFI-CIV-0064٥",
        "AZE-SUN-INT-QMS-RFI-CIV-\n00645",
        "AZE-SUN-INT-QMS-RFI-CIV-\n٠٠٦٤",
    ] {
        assert!(
            !find(malformed).ok(),
            "RFI numbers must be exactly four ASCII digits: {malformed}"
        );
    }
    assert_eq!(
        find("AZE-SUN-INT-QMS-RFI-CIV-0064REQUEST").code,
        "AZE-SUN-INT-QMS-RFI-CIV-0064",
        "OCR-concatenated trailing letters remain valid"
    );
    assert_eq!(
        find("AZE-SUN-INT-QMS-RFI-CIV-00645 AZE-SUN-INT-QMS-RFI-MEC-0107").code,
        "AZE-SUN-INT-QMS-RFI-MEC-0107",
        "a malformed candidate must not hide a later valid code"
    );
}

#[test]
fn canonical_rfi_matching_exposes_the_single_normalized_value() {
    let (normalized, matched) = canonical_and_find("aze-sun-int-qms-rfi-civ-0064");

    assert_eq!(normalized, "AZE-SUN-INT-QMS-RFI-CIV-0064");
    assert_eq!(matched.code, normalized);
    assert_eq!(matched.method, RfiMethod::Strict);
}

#[test]
fn slice5_shared_rfi_and_naming_fixture_matches_the_frozen_compatibility_oracle() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/slice5_rfi_naming.json")).unwrap();
    for case in fixture["rfiCases"].as_array().unwrap() {
        let matched = find(case["input"].as_str().unwrap());
        assert_eq!(
            matched.code,
            case["code"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            matched.discipline,
            case["discipline"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            matched.number,
            case["number"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        let method = match matched.method {
            RfiMethod::Strict => "strict",
            RfiMethod::Repaired => "repaired",
            RfiMethod::Fuzzy => "fuzzy",
            RfiMethod::Miss => "miss",
        };
        assert_eq!(method, case["method"].as_str().unwrap(), "{}", case["name"]);
        assert_eq!(
            matched.confidence,
            case["confidence"].as_f64().unwrap(),
            "{}",
            case["name"]
        );
    }

    let root = std::path::Path::new("fixture-root");
    for case in fixture["namingCases"].as_array().unwrap() {
        let code = case["code"].as_str().unwrap();
        assert_eq!(filename_for(code), case["filename"].as_str().unwrap());
        assert_eq!(
            folder_for(root, code),
            root.join(case["folder"].as_str().unwrap())
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn native_pdf_adapter_uses_the_searchable_text_fast_path() {
    let fixture = tempfile::TempDir::new().unwrap();
    let pdf = fixture.path().join("searchable.pdf");
    write_text_pdf(
        &pdf,
        "REQUEST FOR INSPECTION AZE-SUN-INT-QMS-RFI-CIV-0064 searchable text layer",
    );
    let adapter = NativeOcr::with_tesseract("/path/that/must/not/be-needed");

    let read = adapter
        .read(&pdf)
        .expect("PDFKit should expose searchable text");

    assert_eq!(read.matched.code, "AZE-SUN-INT-QMS-RFI-CIV-0064");
    assert_eq!(read.matched.method, RfiMethod::Strict);
    assert_eq!(read.source, "text-layer");
}

#[cfg(target_os = "macos")]
#[test]
fn searchable_text_decoys_never_become_false_rfi_labels() {
    let fixture = tempfile::TempDir::new().unwrap();
    let pdf = fixture.path().join("decoys.pdf");
    write_text_pdf(
        &pdf,
        "AZE-SUN-INT-QMS-ITP-MEC-0001 AZE-SUN-INT-ENG-STR-DRG-0901 decoy text only",
    );

    let error = NativeOcr::with_tesseract("/missing/tesseract")
        .read(&pdf)
        .expect_err("decoys must not pass the strict -RFI- anchor");

    assert_eq!(error, OcrFailure::EngineMissing);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn native_image_only_pdf_runs_through_platform_renderer_and_real_tesseract() {
    let configured = std::env::var_os("DOCFLOW_TESSERACT")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file());
    #[cfg(target_os = "macos")]
    let default_tesseract = std::path::PathBuf::from("/opt/homebrew/bin/tesseract");
    #[cfg(target_os = "windows")]
    let default_tesseract =
        std::path::PathBuf::from(r"C:\Program Files\Tesseract-OCR\tesseract.exe");
    let tesseract = configured.clone().unwrap_or(default_tesseract);
    if !tesseract.is_file() && std::env::var_os("DOCFLOW_REQUIRE_REAL_OCR").is_some() {
        panic!("required real OCR fixture has no Tesseract executable");
    }
    if !tesseract.is_file() {
        eprintln!("real OCR fixture skipped: local Tesseract is unavailable");
        return;
    }
    #[cfg(target_os = "macos")]
    let image_only = {
        let fixture = tempfile::TempDir::new().unwrap();
        let searchable = fixture.path().join("source.pdf");
        let raster = fixture.path().join("first-page.png");
        let image_only = fixture.path().join("image-only.pdf");
        write_text_pdf(
            &searchable,
            "AZE-SUN-INT-QMS-RFI-CIV-0064 REQUEST FOR INSPECTION NATIVE OCR FIXTURE",
        );
        render_first_page_png(&searchable, &raster, 300).unwrap();
        image_pdf_from_png(&raster, &image_only).unwrap();
        (fixture, image_only)
    };
    #[cfg(target_os = "macos")]
    let image_only_path = image_only.1.as_path();
    #[cfg(target_os = "windows")]
    let image_only = std::env::var_os("DOCFLOW_NATIVE_OCR_SMOKE_PDF")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file());
    #[cfg(target_os = "windows")]
    let Some(image_only_path) = image_only.as_deref() else {
        if std::env::var_os("DOCFLOW_REQUIRE_REAL_OCR").is_some() {
            panic!("required real OCR fixture PDF is unavailable");
        }
        eprintln!("real OCR fixture skipped: image-only PDF is unavailable");
        return;
    };

    let adapter = NativeOcr::with_tesseract(tesseract);
    let read = adapter
        .read(image_only_path)
        .expect("image-only PDF should use the real packaged-shape OCR adapter");

    assert_eq!(read.matched.code, "AZE-SUN-INT-QMS-RFI-CIV-0064");
    assert!(matches!(
        read.matched.method,
        RfiMethod::Strict | RfiMethod::Repaired
    ));
    assert_eq!(read.source, "ocr:dpi300:b0.35:blk31:psm6");
}

#[cfg(target_os = "macos")]
fn write_text_pdf(path: &std::path::Path, text: &str) {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 18 Tf 36 700 Td ({escaped}) Tj ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
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
    std::fs::write(path, bytes).unwrap();
}

struct MissingOcr;

impl OcrAdapter for MissingOcr {
    fn read(&self, _path: &std::path::Path) -> Result<OcrRead, OcrFailure> {
        Err(OcrFailure::EngineMissing)
    }

    fn diagnostics(&self) -> String {
        "tesseract: unavailable".to_string()
    }
}

struct FixtureOcr;

impl OcrAdapter for FixtureOcr {
    fn read(&self, path: &std::path::Path) -> Result<OcrRead, OcrFailure> {
        if path.file_name().and_then(|name| name.to_str()) == Some("broken.pdf") {
            return Err(OcrFailure::Document(
                "cannot open broken document".to_string(),
            ));
        }
        Ok(OcrRead {
            matched: find("AZE-SUN-INT-QMS-RFI-CIV-0064"),
            source: "fixture-text-layer".to_string(),
        })
    }

    fn diagnostics(&self) -> String {
        "fixture OCR available".to_string()
    }
}

struct ParallelFixtureOcr;

impl OcrAdapter for ParallelFixtureOcr {
    fn read(&self, path: &std::path::Path) -> Result<OcrRead, OcrFailure> {
        let number = if path.file_name().and_then(|name| name.to_str()) == Some("second.pdf") {
            "0065"
        } else {
            "0064"
        };
        Ok(OcrRead {
            matched: find(&format!("AZE-SUN-INT-QMS-RFI-CIV-{number}")),
            source: "parallel-fixture".to_string(),
        })
    }

    fn supports_parallel_analysis(&self) -> bool {
        true
    }

    fn diagnostics(&self) -> String {
        "parallel fixture OCR".to_string()
    }
}

#[test]
fn process_scan_preview_uses_bounded_parallel_analysis_when_supported() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("first.pdf"), b"fixture").unwrap();
    std::fs::write(source.join("second.pdf"), b"fixture").unwrap();

    let plan = preview(
        &ProcessScansParams {
            source,
            destination,
            organize_into_rfi_folders: true,
            keep_original: true,
        },
        &ParallelFixtureOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert_eq!(plan.meta["parallelAnalysis"], true);
    assert!(
        plan.meta["workerCount"]
            .as_u64()
            .is_some_and(|value| value >= 1)
    );
    assert_eq!(
        plan.steps
            .iter()
            .filter(|step| step.kind == docflow_lib::contracts::StepKind::Copy)
            .count(),
        2
    );
}

struct BrokenOcr;

impl OcrAdapter for BrokenOcr {
    fn read(&self, _path: &std::path::Path) -> Result<OcrRead, OcrFailure> {
        Err(OcrFailure::Document(
            "Error opening data file eng.traineddata".to_string(),
        ))
    }

    fn diagnostics(&self) -> String {
        "tesseract: fixture-broken".to_string()
    }
}

struct LowConfidenceOcr;

impl OcrAdapter for LowConfidenceOcr {
    fn read(&self, _path: &std::path::Path) -> Result<OcrRead, OcrFailure> {
        Ok(OcrRead {
            matched: docflow_lib::rfi_code::RfiMatch {
                code: "AZE-SUN-INT-QMS-RFI-CIV-0064".to_string(),
                discipline: "CIV".to_string(),
                number: "0064".to_string(),
                method: RfiMethod::Fuzzy,
                confidence: 0.79,
            },
            source: "fixture-low-confidence".to_string(),
        })
    }

    fn diagnostics(&self) -> String {
        "fixture OCR available".to_string()
    }
}

#[test]
fn keep_original_copies_low_confidence_files_into_review() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("low-confidence.pdf"), b"PDF").unwrap();

    let plan = preview(
        &ProcessScansParams {
            source,
            destination,
            organize_into_rfi_folders: true,
            keep_original: true,
        },
        &LowConfidenceOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert_eq!(plan.warnings[0].key, "warn.low_confidence");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].kind, docflow_lib::contracts::StepKind::Copy);
}

#[test]
fn keep_original_copies_ocr_document_errors_into_review() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("broken.pdf"), b"PDF").unwrap();

    let plan = preview(
        &ProcessScansParams {
            source,
            destination,
            organize_into_rfi_folders: true,
            keep_original: true,
        },
        &FixtureOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert_eq!(plan.warnings[0].key, "warn.unreadable");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].kind, docflow_lib::contracts::StepKind::Copy);
}

#[test]
fn process_scans_preview_apply_journal_and_undo_preserve_duplicates_and_failures() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    let journal = fixture.path().join("journal");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("older.pdf"), b"OLDER").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(source.join("newest.pdf"), b"NEWEST").unwrap();
    std::fs::write(source.join("broken.pdf"), b"BROKEN").unwrap();

    let plan = preview(
        &ProcessScansParams {
            source: source.clone(),
            destination: destination.clone(),
            organize_into_rfi_folders: true,
            keep_original: false,
        },
        &FixtureOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert_eq!(
        plan.warnings
            .iter()
            .map(|warning| warning.key.as_str())
            .collect::<Vec<_>>(),
        vec!["warn.unreadable", "warn.duplicate"]
    );
    assert!(source.join("newest.pdf").exists(), "preview is inert");
    assert!(!destination.join("_review").exists(), "preview is inert");

    let authority = FilesystemAuthority::new(&journal, std::time::Duration::from_secs(300));
    let scope = GrantScope::new("owner-1", "session-1", "preview-1", "rfi.process_scans");
    let source_grant = authority
        .issue_grant(&scope, &source, GrantAccess::ReadWrite)
        .unwrap();
    let destination_grant = authority
        .issue_grant(&scope, &destination, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                "rfi.process_scans",
                vec![source_grant.grant_id, destination_grant.grant_id],
            ),
            plan.clone(),
        )
        .unwrap();
    let caller = CallerIdentity::new("owner-1", "session-1");
    let applied = authority.apply(&cached.plan_id, &caller).unwrap();

    assert_eq!(
        (applied.applied, applied.failed),
        (plan.steps.len() as u64, 0)
    );
    assert_eq!(
        std::fs::read(
            destination
                .join("AZE-SUN-INT-QMS-RFI-CIV-0064")
                .join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf")
        )
        .unwrap(),
        b"NEWEST"
    );
    assert_eq!(
        std::fs::read(destination.join("_review/older.pdf")).unwrap(),
        b"OLDER"
    );
    assert_eq!(
        std::fs::read(destination.join("_review/broken.pdf")).unwrap(),
        b"BROKEN"
    );

    let undone = authority.undo(&applied.journal_id, &caller, None).unwrap();
    assert_eq!(undone.failed, 0, "{:?}", undone.errors);
    assert_eq!(std::fs::read(source.join("older.pdf")).unwrap(), b"OLDER");
    assert_eq!(std::fs::read(source.join("newest.pdf")).unwrap(), b"NEWEST");
    assert_eq!(std::fs::read(source.join("broken.pdf")).unwrap(), b"BROKEN");
    assert!(std::fs::read_dir(&destination).unwrap().next().is_none());
}

#[test]
fn review_scan_and_put_delete_decisions_are_inert_canonical_rust_plans() {
    let fixture = tempfile::TempDir::new().unwrap();
    let destination = fixture.path().join("destination");
    let review = destination.join("_review");
    let trash = fixture.path().join("app-data/trash");
    std::fs::create_dir_all(&review).unwrap();
    std::fs::create_dir_all(&trash).unwrap();
    let put_source = review.join("put.pdf");
    let delete_source = review.join("delete.pdf");
    std::fs::write(&put_source, b"PUT").unwrap();
    std::fs::write(&delete_source, b"DELETE").unwrap();

    let rows = review_scan(&destination, &FixtureOcr).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| row.code == "AZE-SUN-INT-QMS-RFI-CIV-0064")
    );
    let plan = review_preview(
        &destination,
        &trash,
        &[
            ReviewDecision {
                path: put_source.clone(),
                action: ReviewAction::Put,
                code: "aze-sun-int-qms-rfi-civ-0064".to_string(),
            },
            ReviewDecision {
                path: delete_source.clone(),
                action: ReviewAction::Delete,
                code: String::new(),
            },
        ],
        "20260825T020304000005",
    )
    .unwrap();

    assert_eq!(plan.op_id, "rfi.review");
    assert_eq!(plan.steps[0].kind, docflow_lib::contracts::StepKind::Mkdir);
    assert!(plan.steps.iter().any(|step| {
        step.dst
            .as_deref()
            .is_some_and(|path| path.ends_with("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf"))
    }));
    assert!(plan.steps.iter().any(|step| {
        step.meta.get("action").and_then(serde_json::Value::as_str) == Some("delete")
    }));
    assert!(
        put_source.exists() && delete_source.exists(),
        "preview is inert"
    );

    let authority = FilesystemAuthority::new(
        fixture.path().join("journal"),
        std::time::Duration::from_secs(300),
    );
    let scope = GrantScope::new("owner-1", "session-1", "review-1", "rfi.review");
    let destination_grant = authority
        .issue_grant(&scope, &destination, GrantAccess::ReadWrite)
        .unwrap();
    let app_data_grant = authority
        .issue_grant(
            &scope,
            &fixture.path().join("app-data"),
            GrantAccess::ReadWrite,
        )
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                "rfi.review",
                vec![destination_grant.grant_id, app_data_grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let caller = CallerIdentity::new("owner-1", "session-1");
    let applied = authority.apply(&cached.plan_id, &caller).unwrap();

    assert_eq!((applied.applied, applied.failed), (3, 0));
    assert!(!put_source.exists() && !delete_source.exists());
    assert!(
        destination
            .join("AZE-SUN-INT-QMS-RFI-CIV-0064/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf")
            .is_file()
    );
    assert_eq!(std::fs::read_dir(&trash).unwrap().count(), 1);

    let undone = authority.undo(&applied.journal_id, &caller, None).unwrap();
    assert_eq!(undone.failed, 0, "{:?}", undone.errors);
    assert!(put_source.is_file() && delete_source.is_file());
    assert!(std::fs::read_dir(&trash).unwrap().next().is_none());
    assert!(!destination.join("AZE-SUN-INT-QMS-RFI-CIV-0064").exists());
}

#[test]
fn missing_ocr_engine_returns_one_diagnostic_and_a_zero_step_plan() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("a.pdf"), b"%PDF stub").unwrap();
    std::fs::write(source.join("b.PDF"), b"%PDF stub").unwrap();

    let plan = preview(
        &ProcessScansParams {
            source: source.clone(),
            destination: destination.clone(),
            organize_into_rfi_folders: true,
            keep_original: false,
        },
        &MissingOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings.len(), 1);
    assert_eq!(plan.warnings[0].key, "warn.no_ocr");
    assert!(plan.warnings[0].subject.contains("tesseract: unavailable"));
    assert!(source.join("a.pdf").exists());
    assert!(source.join("b.PDF").exists());
    assert!(std::fs::read_dir(destination).unwrap().next().is_none());
}

#[test]
fn a_broken_ocr_install_fails_closed_and_preview_cancellation_is_inert() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    for name in ["a.pdf", "b.pdf", "c.pdf"] {
        std::fs::write(source.join(name), b"PDF").unwrap();
    }
    let params = ProcessScansParams {
        source: source.clone(),
        destination: destination.clone(),
        organize_into_rfi_folders: true,
        keep_original: false,
    };

    let broken = preview(&params, &BrokenOcr, |_| {}, || false).unwrap();
    assert!(broken.steps.is_empty());
    assert_eq!(broken.warnings[0].key, "warn.ocr_broken");
    assert!(broken.warnings[0].subject.contains("eng.traineddata"));
    assert!(broken.warnings[0].subject.contains("fixture-broken"));

    let cancelled = preview(&params, &FixtureOcr, |_| {}, || true).unwrap_err();
    assert_eq!(cancelled.code, "operation_cancelled");
    assert_eq!(std::fs::read_dir(&source).unwrap().count(), 3);
    assert!(std::fs::read_dir(&destination).unwrap().next().is_none());
}

#[test]
fn flat_copy_mode_preserves_the_source_and_uses_the_exact_legacy_filename() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    let journal = fixture.path().join("journal");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("scan.pdf"), b"PDF").unwrap();
    let plan = preview(
        &ProcessScansParams {
            source: source.clone(),
            destination: destination.clone(),
            organize_into_rfi_folders: false,
            keep_original: true,
        },
        &FixtureOcr,
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].kind, docflow_lib::contracts::StepKind::Copy);
    assert_eq!(
        std::path::Path::new(plan.steps[0].dst.as_deref().unwrap()).file_name(),
        Some(std::ffi::OsStr::new(
            "RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf"
        ))
    );

    let authority = FilesystemAuthority::new(&journal, std::time::Duration::from_secs(300));
    let scope = GrantScope::new("owner-1", "session-1", "preview-flat", "rfi.process_scans");
    let source_grant = authority
        .issue_grant(&scope, &source, GrantAccess::ReadWrite)
        .unwrap();
    let destination_grant = authority
        .issue_grant(&scope, &destination, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                "rfi.process_scans",
                vec![source_grant.grant_id, destination_grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let applied = authority
        .apply(
            &cached.plan_id,
            &CallerIdentity::new("owner-1", "session-1"),
        )
        .unwrap();
    assert_eq!((applied.applied, applied.failed), (1, 0));
    assert!(source.join("scan.pdf").is_file());
    assert!(
        destination
            .join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf")
            .is_file()
    );
}

#[test]
fn process_scans_apply_cancellation_is_observed_before_the_first_mutation() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("scan.pdf"), b"SOURCE").unwrap();
    let plan = preview(
        &ProcessScansParams {
            source: source.clone(),
            destination: destination.clone(),
            organize_into_rfi_folders: true,
            keep_original: false,
        },
        &FixtureOcr,
        |_| {},
        || false,
    )
    .unwrap();
    let authority = FilesystemAuthority::new(
        fixture.path().join("journal"),
        std::time::Duration::from_secs(300),
    );
    let scope = GrantScope::new("owner-1", "session-1", "cancel-1", "rfi.process_scans");
    let source_grant = authority
        .issue_grant(&scope, &source, GrantAccess::ReadWrite)
        .unwrap();
    let destination_grant = authority
        .issue_grant(&scope, &destination, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                "rfi.process_scans",
                vec![source_grant.grant_id, destination_grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let plan_id = cached.plan_id.clone();
    let result = authority
        .apply_with_progress(
            &plan_id,
            &CallerIdentity::new("owner-1", "session-1"),
            |event| {
                if event.phase == "apply" && event.done == 0 {
                    assert!(authority.cancel(&plan_id));
                }
            },
        )
        .unwrap();

    assert_eq!((result.applied, result.failed), (0, 0));
    assert_eq!(result.meta["effectState"], "cancelled_clean");
    assert!(source.join("scan.pdf").is_file());
    assert!(std::fs::read_dir(destination).unwrap().next().is_none());
}

#[test]
fn process_scans_apply_never_clobbers_a_destination_created_after_preview() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(source.join("scan.pdf"), b"SOURCE").unwrap();
    let plan = preview(
        &ProcessScansParams {
            source: source.clone(),
            destination: destination.clone(),
            organize_into_rfi_folders: false,
            keep_original: false,
        },
        &FixtureOcr,
        |_| {},
        || false,
    )
    .unwrap();
    let target = destination.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf");
    std::fs::write(&target, b"USER DATA").unwrap();

    let authority = FilesystemAuthority::new(
        fixture.path().join("journal"),
        std::time::Duration::from_secs(300),
    );
    let scope = GrantScope::new("owner-1", "session-1", "race-1", "rfi.process_scans");
    let source_grant = authority
        .issue_grant(&scope, &source, GrantAccess::ReadWrite)
        .unwrap();
    let destination_grant = authority
        .issue_grant(&scope, &destination, GrantAccess::ReadWrite)
        .unwrap();
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                "owner-1",
                "session-1",
                "rfi.process_scans",
                vec![source_grant.grant_id, destination_grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let result = authority
        .apply(
            &cached.plan_id,
            &CallerIdentity::new("owner-1", "session-1"),
        )
        .unwrap();

    assert_eq!((result.applied, result.failed), (0, 1));
    assert_eq!(std::fs::read(&target).unwrap(), b"USER DATA");
    assert_eq!(std::fs::read(source.join("scan.pdf")).unwrap(), b"SOURCE");
}

#[test]
fn naming_keeps_the_canonical_code_untouched_and_preview_only() {
    let fixture = tempfile::TempDir::new().unwrap();
    let code = "AZE-SUN-INT-QMS-RFI-CIV-0064";

    assert_eq!(filename_for(code), format!("RFI No. {code}.pdf"));
    assert_eq!(folder_for(fixture.path(), code), fixture.path().join(code));
    assert_eq!(
        target_for(fixture.path(), code),
        fixture
            .path()
            .join(code)
            .join(format!("RFI No. {code}.pdf"))
    );
    assert_eq!(
        flat_target_for(fixture.path(), code),
        fixture.path().join(format!("RFI No. {code}.pdf"))
    );
    assert!(!fixture.path().join(code).exists());
}

#[test]
fn process_scans_rejects_an_unbounded_source_before_ocr() {
    let fixture = tempfile::TempDir::new().unwrap();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    for index in 0..=10_000 {
        std::fs::write(source.join(format!("{index:05}.pdf")), b"").unwrap();
    }

    let error = preview(
        &ProcessScansParams {
            source,
            destination,
            organize_into_rfi_folders: true,
            keep_original: false,
        },
        &MissingOcr,
        |_| {},
        || false,
    )
    .expect_err("an oversized source must fail before OCR or plan construction");

    assert_eq!(error.code, "validation");
}

#[test]
fn review_rejects_an_unbounded_decision_payload_before_planning() {
    let fixture = tempfile::TempDir::new().unwrap();
    let destination = fixture.path().join("destination");
    let review = destination.join("_review");
    let trash = fixture.path().join("trash");
    std::fs::create_dir_all(&review).unwrap();
    std::fs::create_dir_all(&trash).unwrap();
    let source = review.join("scan.pdf");
    std::fs::write(&source, b"PDF").unwrap();
    let decisions = (0..=10_000)
        .map(|_| ReviewDecision {
            path: source.clone(),
            action: ReviewAction::Delete,
            code: String::new(),
        })
        .collect::<Vec<_>>();

    let error = review_preview(&destination, &trash, &decisions, "20260825T020304000005")
        .expect_err("an oversized decision payload must fail before plan construction");

    assert_eq!(error.code, "validation");
}
