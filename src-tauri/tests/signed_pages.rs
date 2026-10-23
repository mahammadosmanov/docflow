mod support;

use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use docflow_lib::{
    contracts::{Plan, StepKind},
    fs_ops::{FilesystemAuthority, file_sha256},
    journal::{JournalRecord, JournalState, JournalStore, RecoveryDisposition},
    naming::filename_for,
    operations::{
        process_scans::{OcrAdapter, OcrFailure, OcrRead},
        stage_signed_pages::{StageSignedPagesParams, preview, revise},
    },
    paths::{CallerIdentity, GrantAccess, GrantScope},
    pdf::{
        inspect, replace_first_page_incremental, replace_first_page_incremental_from_page,
        rotate_pages_incremental,
    },
    plan_store::{BackendOrigin, PlanBinding},
    rfi_code::{RfiMethod, find, normalize_candidate_number},
};
use serde_json::{Value, json};
use support::pdf_fixture::{
    CODE, write_encrypted, write_object_stream_pdf, write_rich_archive, write_signed,
    write_signed_document, write_signed_with_indirect_stream_length, write_tagged_document,
};

#[derive(Default)]
struct FixtureOcr {
    codes: BTreeMap<String, String>,
}

#[derive(Default)]
struct PageFixtureOcr {
    codes: BTreeMap<(String, usize), String>,
    previews: bool,
}

struct ParallelPageFixtureOcr {
    codes: BTreeMap<(String, usize), String>,
}

impl OcrAdapter for ParallelPageFixtureOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        self.read_page(path, 0)
    }

    fn supports_page_scan(&self) -> bool {
        true
    }

    fn supports_parallel_analysis(&self) -> bool {
        true
    }

    fn read_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let code = self
            .codes
            .get(&(name.to_string(), page_index))
            .cloned()
            .unwrap_or_default();
        Ok(OcrRead {
            matched: find(&code),
            source: format!("parallel-fixture-page-{page_index}"),
        })
    }

    fn diagnostics(&self) -> String {
        "parallel fixture page OCR".to_string()
    }
}

impl OcrAdapter for PageFixtureOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        self.read_page(path, 0)
    }

    fn supports_page_scan(&self) -> bool {
        true
    }

    fn supports_page_previews(&self) -> bool {
        self.previews
    }

    fn read_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let code = self
            .codes
            .get(&(name.to_string(), page_index))
            .cloned()
            .unwrap_or_default();
        Ok(OcrRead {
            matched: find(&code),
            source: format!("fixture-page-{page_index}"),
        })
    }

    fn diagnostics(&self) -> String {
        "fixture page OCR".to_string()
    }
}

struct AliasFixtureOcr {
    code: String,
}

impl OcrAdapter for AliasFixtureOcr {
    fn read(&self, _path: &Path) -> Result<OcrRead, OcrFailure> {
        let mut matched = find(&self.code);
        matched.method = RfiMethod::OcrAlias;
        Ok(OcrRead {
            matched,
            source: "fixture-consensus-alias".to_string(),
        })
    }

    fn diagnostics(&self) -> String {
        "fixture alias OCR".to_string()
    }
}

struct RepairedFixtureOcr;

impl OcrAdapter for RepairedFixtureOcr {
    fn read(&self, _path: &Path) -> Result<OcrRead, OcrFailure> {
        let mut matched = find(CODE);
        matched.method = RfiMethod::Repaired;
        Ok(OcrRead {
            matched,
            source: "fixture-repaired".to_string(),
        })
    }

    fn diagnostics(&self) -> String {
        "fixture repaired OCR".to_string()
    }
}

impl FixtureOcr {
    fn one(names: &[&str]) -> Self {
        Self {
            codes: names
                .iter()
                .map(|name| ((*name).to_string(), CODE.to_string()))
                .collect(),
        }
    }
}

impl OcrAdapter for FixtureOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if name == "malformed.pdf" {
            return Err(OcrFailure::Document("malformed PDF".to_string()));
        }
        let code = self.codes.get(name).cloned().unwrap_or_default();
        Ok(OcrRead {
            matched: find(&code),
            source: "fixture-text-layer".to_string(),
        })
    }

    fn diagnostics(&self) -> String {
        "fixture OCR".to_string()
    }
}

fn roots(
    fixture: &tempfile::TempDir,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let signed = fixture.path().join("signed");
    let archive = fixture.path().join("archive");
    let review = fixture.path().join("review");
    for root in [&signed, &archive, &review] {
        fs::create_dir_all(root).unwrap();
    }
    (signed, archive, review)
}

#[test]
fn incremental_page_rotation_preserves_unrelated_pdf_content() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("source.pdf");
    let output = fixture.path().join("review.pdf");
    write_rich_archive(&source);
    let source_bytes = fs::read(&source).unwrap();
    let before = inspect(&source).unwrap();

    rotate_pages_incremental(&source, &output, &[(0, 90), (1, 270)]).unwrap();

    let output_bytes = fs::read(&output).unwrap();
    let after = inspect(&output).unwrap();
    assert!(output_bytes.starts_with(&source_bytes));
    assert_eq!(after.page_count, before.page_count);
    assert_eq!(
        after.incremental_revisions,
        before.incremental_revisions + 1
    );
    assert_eq!(after.pages[0].rotation, 90);
    assert_eq!(after.pages[1].rotation, 270);
    assert_eq!(after.pages[2], before.pages[2]);
    assert_eq!(after.pages[0].content_hash, before.pages[0].content_hash);
    assert_eq!(after.pages[1].annotations, before.pages[1].annotations);
    assert_eq!(after.catalog_entry_hashes, before.catalog_entry_hashes);
    assert_eq!(after.info_hash, before.info_hash);
    assert_eq!(after.embedded_file_hashes, before.embedded_file_hashes);
}

fn params(signed: &Path, archive: &Path, review: &Path, recursive: bool) -> StageSignedPagesParams {
    StageSignedPagesParams {
        signed_scans: signed.to_path_buf(),
        archive_root: archive.to_path_buf(),
        review_root: review.to_path_buf(),
        scan_subfolders: recursive,
    }
}

#[test]
fn incremental_first_page_replacement_preserves_the_rich_archive_graph_and_prefix_bytes() {
    let fixture = tempfile::TempDir::new().unwrap();
    let archive = fixture.path().join("archive.pdf");
    let signed = fixture.path().join("signed.pdf");
    let output = fixture.path().join("output.pdf");
    write_rich_archive(&archive);
    write_signed(&signed, true);
    let archive_bytes = fs::read(&archive).unwrap();
    let archive_facts = inspect(&archive).unwrap();
    let signed_facts = inspect(&signed).unwrap();
    for key in [
        "Metadata",
        "Names",
        "Outlines",
        "PageLabels",
        "AcroForm",
        "OpenAction",
        "ViewerPreferences",
    ] {
        assert!(
            archive_facts.catalog_entry_hashes.contains_key(key),
            "fixture must exercise catalog structure {key}"
        );
    }
    assert!(archive_facts.info_hash.is_some());
    assert_eq!(archive_facts.embedded_file_hashes.len(), 1);

    replace_first_page_incremental(&archive, &signed, &output).unwrap();

    let output_bytes = fs::read(&output).unwrap();
    assert!(output_bytes.starts_with(&archive_bytes));
    let output_facts = inspect(&output).unwrap();
    assert_eq!(output_facts.page_count, 3);
    assert_eq!(
        output_facts.incremental_revisions,
        archive_facts.incremental_revisions + 1
    );
    assert_eq!(
        output_facts.catalog_entry_hashes,
        archive_facts.catalog_entry_hashes
    );
    assert_eq!(output_facts.info_hash, archive_facts.info_hash);
    assert_eq!(
        output_facts.embedded_file_hashes,
        archive_facts.embedded_file_hashes
    );
    assert_eq!(output_facts.pages[1..], archive_facts.pages[1..]);
    assert_eq!(
        output_facts.pages[0].media_box,
        signed_facts.pages[0].media_box
    );
    assert_eq!(
        output_facts.pages[0].crop_box,
        signed_facts.pages[0].crop_box
    );
    assert_eq!(
        output_facts.pages[0].rotation,
        signed_facts.pages[0].rotation
    );
    assert_eq!(
        output_facts.pages[0].content_hash,
        signed_facts.pages[0].content_hash
    );
    assert_eq!(output_facts.pages[0].links, 1);
    assert_eq!(output_facts.pages[1].annotations, 3);
    assert_eq!(output_facts.pages[1].links, 1);
    assert_eq!(output_facts.pages[1].widgets, 1);
}

#[test]
fn arbitrary_signed_scan_page_can_replace_archive_page_one_with_fidelity() {
    let fixture = tempfile::TempDir::new().unwrap();
    let archive = fixture.path().join("archive.pdf");
    let signed = fixture.path().join("signed.pdf");
    let output = fixture.path().join("output.pdf");
    write_rich_archive(&archive);
    write_signed(&signed, true);
    let signed_facts = inspect(&signed).unwrap();

    replace_first_page_incremental_from_page(&archive, &signed, 1, &output).unwrap();

    let completed = inspect(&output).unwrap();
    assert_eq!(
        completed.pages[0].content_hash,
        signed_facts.pages[1].content_hash
    );
    assert_eq!(completed.pages[1..], inspect(&archive).unwrap().pages[1..]);
}

#[test]
fn one_multi_page_worley_scan_creates_one_staging_row_per_exact_rfi_page() {
    const SECOND_CODE: &str = "AZE-SUN-INT-QMS-RFI-MEC-0249";
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let signed = signed_root.join("combined.pdf");
    write_signed(&signed, true);
    write_rich_archive(&archive_root.join("CIV").join(filename_for(CODE)));
    write_rich_archive(&archive_root.join("MEC").join(filename_for(SECOND_CODE)));
    let ocr = PageFixtureOcr {
        codes: BTreeMap::from([
            (("combined.pdf".to_string(), 0), CODE.to_string()),
            (("combined.pdf".to_string(), 1), SECOND_CODE.to_string()),
        ]),
        previews: false,
    };

    let mut progress = Vec::new();
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &ocr,
        |event| progress.push(event),
        || false,
    )
    .unwrap();

    let replacement_steps = plan
        .steps
        .iter()
        .filter(|step| step.kind == StepKind::PdfFirstPageReplace)
        .collect::<Vec<_>>();
    assert_eq!(replacement_steps.len(), 2);
    assert_eq!(replacement_steps[0].meta["signedPageIndex"], json!(0));
    assert_eq!(replacement_steps[1].meta["signedPageIndex"], json!(1));
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["confidence"], json!(1.0));
    assert!(
        plan.meta["editor"]["columns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|column| column["key"] == "confidence" && column["display"] == "percent")
    );
    assert_eq!(plan.meta["pageComparisons"], json!({}));
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.key == "warn.rfi.signedMultiPage")
    );

    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache_stage_plan(&authority, plan, &signed_root, &archive_root, &review_root);
    let caller = CallerIdentity::new("owner-1", "session-1");
    let result = authority.apply(&cached.plan_id, &caller).unwrap();
    assert_eq!(result.applied, 4);
    let second_output = review_root.join("MEC").join(filename_for(SECOND_CODE));
    assert_eq!(
        inspect(&second_output).unwrap().pages[0].content_hash,
        inspect(&signed).unwrap().pages[1].content_hash
    );
}

#[test]
fn combined_scan_uses_bounded_parallel_page_ocr_when_supported() {
    const SECOND_CODE: &str = "AZE-SUN-INT-QMS-RFI-MEC-0249";
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("combined.pdf"), true);
    write_rich_archive(&archive_root.join("CIV").join(filename_for(CODE)));
    write_rich_archive(&archive_root.join("MEC").join(filename_for(SECOND_CODE)));
    let ocr = ParallelPageFixtureOcr {
        codes: BTreeMap::from([
            (("combined.pdf".to_string(), 0), CODE.to_string()),
            (("combined.pdf".to_string(), 1), SECOND_CODE.to_string()),
        ]),
    };

    let mut progress = Vec::new();
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &ocr,
        |event| progress.push(event),
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
            .filter(|step| step.kind == StepKind::PdfFirstPageReplace)
            .count(),
        2
    );
    assert!(progress.iter().any(|event| event.phase == "signed_ocr"));
    assert!(progress.iter().any(|event| event.phase == "archive_scan"));
    assert_eq!(
        progress.last().map(|event| event.phase.as_str()),
        Some("preview_ready")
    );
}

struct IdentityOnlyOcr(ParallelPageFixtureOcr);

impl OcrAdapter for IdentityOnlyOcr {
    fn read(&self, path: &Path) -> Result<OcrRead, OcrFailure> {
        self.0.read(path)
    }

    fn supports_page_scan(&self) -> bool {
        true
    }

    fn supports_parallel_analysis(&self) -> bool {
        true
    }

    fn read_page(&self, _path: &Path, _page_index: usize) -> Result<OcrRead, OcrFailure> {
        panic!("signed first pages must use the cover-first identity reader");
    }

    fn read_identity_page(&self, path: &Path, page_index: usize) -> Result<OcrRead, OcrFailure> {
        self.0.read_page(path, page_index)
    }

    fn diagnostics(&self) -> String {
        "identity only fixture".to_string()
    }
}

#[test]
fn signed_pages_read_identities_through_the_cover_first_reader() {
    const SECOND_CODE: &str = "AZE-SUN-INT-QMS-RFI-MEC-0249";
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("combined.pdf"), true);
    write_rich_archive(&archive_root.join("CIV").join(filename_for(CODE)));
    write_rich_archive(&archive_root.join("MEC").join(filename_for(SECOND_CODE)));
    let ocr = IdentityOnlyOcr(ParallelPageFixtureOcr {
        codes: BTreeMap::from([
            (("combined.pdf".to_string(), 0), CODE.to_string()),
            (("combined.pdf".to_string(), 1), SECOND_CODE.to_string()),
        ]),
    });
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &ocr,
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(
        plan.steps
            .iter()
            .filter(|step| step.kind == StepKind::PdfFirstPageReplace)
            .count(),
        2
    );
}

#[test]
fn archive_discovery_scans_every_depth_with_bounded_parallel_evidence() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("incoming/day-1/signed.pdf"), false);
    for branch in ["CIV", "MEC", "ELE", "PRC"] {
        fs::create_dir_all(archive_root.join(branch).join("Area/Package")).unwrap();
    }
    write_rich_archive(
        &archive_root
            .join("CIV/Area/Package/RFI folder")
            .join(filename_for(CODE)),
    );

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, true),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();

    assert!(
        plan.steps
            .iter()
            .any(|step| step.kind == StepKind::PdfFirstPageReplace)
    );
    assert_eq!(plan.meta["archiveRecursive"], true);
    assert!(plan.meta["archiveDirectoriesScanned"].as_u64().unwrap() >= 10);
    assert!(plan.meta["archiveEntriesScanned"].as_u64().unwrap() >= 10);
    let workers = plan.meta["archiveWorkerCount"].as_u64().unwrap();
    assert!((1..=4).contains(&workers));
}

#[test]
fn unreadable_and_duplicate_combined_scan_pages_fail_closed_with_page_labels() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("combined.pdf"), true);
    write_rich_archive(&archive_root.join("CIV").join(filename_for(CODE)));

    let unreadable = PageFixtureOcr {
        codes: BTreeMap::from([(("combined.pdf".to_string(), 0), CODE.to_string())]),
        previews: false,
    };
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &unreadable,
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(
        plan.steps
            .iter()
            .filter(|step| step.kind == StepKind::PdfFirstPageReplace)
            .count(),
        1
    );
    assert!(plan.warnings.iter().any(|warning| {
        warning.key == "warn.no_code"
            && warning.subject.contains("combined.pdf")
            && warning.subject.contains("#2")
    }));

    let duplicate = PageFixtureOcr {
        codes: BTreeMap::from([
            (("combined.pdf".to_string(), 0), CODE.to_string()),
            (("combined.pdf".to_string(), 1), CODE.to_string()),
        ]),
        previews: false,
    };
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &duplicate,
        |_| {},
        || false,
    )
    .unwrap();
    assert!(
        plan.steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert!(plan.warnings.iter().any(|warning| {
        warning.key == "warn.rfi.signedDuplicate"
            && warning.subject.contains("#1")
            && warning.subject.contains("#2")
    }));
}

#[test]
fn combined_scan_preview_reports_pages_and_cancels_between_them() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("combined.pdf"), true);
    let ocr = PageFixtureOcr {
        codes: BTreeMap::from([
            (("combined.pdf".to_string(), 0), CODE.to_string()),
            (
                ("combined.pdf".to_string(), 1),
                "AZE-SUN-INT-QMS-RFI-MEC-0249".to_string(),
            ),
        ]),
        previews: false,
    };
    let mut cancellation_checks = 0;
    let mut subjects = Vec::new();

    let error = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &ocr,
        |event| subjects.push(event.subject),
        || {
            cancellation_checks += 1;
            cancellation_checks >= 3
        },
    )
    .unwrap_err();

    assert_eq!(error.code, "operation_cancelled");
    assert!(subjects.iter().any(|subject| subject.contains("#1")));
    assert!(!subjects.iter().any(|subject| subject.contains("#2")));
    assert!(fs::read_dir(&review_root).unwrap().next().is_none());
}

#[test]
fn valid_indirect_stream_length_is_inspected_and_staged_without_fidelity_loss() {
    let fixture = tempfile::TempDir::new().unwrap();
    let archive = fixture.path().join("archive.pdf");
    let signed = fixture.path().join("signed.pdf");
    let output = fixture.path().join("output.pdf");
    write_rich_archive(&archive);
    write_signed_with_indirect_stream_length(&signed);

    let signed_facts = inspect(&signed).expect("valid indirect /Length must be resolved");
    assert_eq!(signed_facts.page_count, 1);

    replace_first_page_incremental(&archive, &signed, &output).unwrap();
    let completed = inspect(&output).unwrap();
    assert_eq!(completed.page_count, 3);
    assert_eq!(
        completed.pages[0].content_hash,
        signed_facts.pages[0].content_hash
    );
}

#[test]
fn malformed_and_cyclic_indirect_stream_lengths_fail_closed() {
    let fixture = tempfile::TempDir::new().unwrap();
    let malformed = fixture.path().join("malformed-length.pdf");
    let cyclic = fixture.path().join("cyclic-length.pdf");
    write_signed_with_indirect_stream_length(&malformed);
    write_signed_with_indirect_stream_length(&cyclic);

    let mut malformed_bytes = fs::read(&malformed).unwrap();
    let length_object = malformed_bytes
        .windows(b"10 0 obj\n".len())
        .position(|window| window == b"10 0 obj\n")
        .map(|offset| offset + b"10 0 obj\n".len())
        .unwrap();
    let length_end = malformed_bytes[length_object..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|offset| length_object + offset)
        .unwrap();
    assert_eq!(
        length_end - length_object,
        2,
        "fixture length is two digits"
    );
    malformed_bytes[length_object..length_end].copy_from_slice(b"[]");
    fs::write(&malformed, malformed_bytes).unwrap();

    let mut cyclic_bytes = fs::read(&cyclic).unwrap();
    let reference = cyclic_bytes
        .windows(b"/Length 10 0 R".len())
        .position(|window| window == b"/Length 10 0 R")
        .unwrap();
    cyclic_bytes[reference..reference + b"/Length 10 0 R".len()].copy_from_slice(b"/Length 06 0 R");
    fs::write(&cyclic, cyclic_bytes).unwrap();

    assert!(
        inspect(&malformed)
            .unwrap_err()
            .to_string()
            .contains("not a non-negative integer")
    );
    assert!(
        inspect(&cyclic)
            .unwrap_err()
            .to_string()
            .contains("cyclic or too deep")
    );
}

#[test]
fn object_stream_archive_remains_object_stream_based_and_keeps_later_page_objects() {
    let fixture = tempfile::TempDir::new().unwrap();
    let archive = fixture.path().join("archive-object-stream.pdf");
    let signed = fixture.path().join("signed.pdf");
    let output = fixture.path().join("output.pdf");
    write_object_stream_pdf(
        &archive,
        &format!("ARCHIVE {CODE}"),
        "OBJECT STREAM SECOND PAGE SENTINEL",
    );
    write_signed(&signed, false);
    let before = inspect(&archive).unwrap();

    replace_first_page_incremental(&archive, &signed, &output).unwrap();

    let after = inspect(&output).unwrap();
    assert_eq!(before.object_stream_count, 1);
    assert_eq!(after.object_stream_count, 1);
    assert_eq!(after.pages[1], before.pages[1]);
    assert!(
        fs::read(&output)
            .unwrap()
            .starts_with(&fs::read(&archive).unwrap())
    );
}

#[test]
fn signed_encrypted_tagged_and_malformed_pdfs_fail_closed_without_output() {
    let fixture = tempfile::TempDir::new().unwrap();
    let plain_archive = fixture.path().join("plain.pdf");
    let plain_signed = fixture.path().join("signed.pdf");
    let encrypted = fixture.path().join("encrypted.pdf");
    let certified = fixture.path().join("certified.pdf");
    let tagged = fixture.path().join("tagged.pdf");
    let malformed = fixture.path().join("malformed.pdf");
    write_rich_archive(&plain_archive);
    write_signed(&plain_signed, false);
    write_encrypted(&encrypted);
    write_signed_document(&certified);
    write_tagged_document(&tagged);
    fs::write(&malformed, b"%PDF-1.7\nmalformed without xref").unwrap();

    for (archive, signed, label) in [
        (&encrypted, &plain_signed, "encrypted archive"),
        (&plain_archive, &encrypted, "encrypted scan"),
        (&certified, &plain_signed, "signed archive"),
        (&plain_archive, &certified, "signed scan"),
        (&tagged, &plain_signed, "tagged archive"),
        (&plain_archive, &tagged, "tagged scan"),
        (&malformed, &plain_signed, "malformed archive"),
        (&plain_archive, &malformed, "malformed scan"),
    ] {
        let output = fixture
            .path()
            .join(format!("{}.pdf", label.replace(' ', "-")));
        let error = replace_first_page_incremental(archive, signed, &output).expect_err(label);
        assert!(error.reviewable(), "{label}: {error}");
        assert!(!output.exists(), "{label} must have no output effect");
    }
}

#[test]
fn preview_matches_the_frozen_compatibility_oracle_for_duplicates_misses_ambiguity_and_no_clobber()
{
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/slice7_signed_pages_oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let fixture = tempfile::TempDir::new().unwrap();
        let (signed_root, archive_root, review_root) = roots(&fixture);
        let signed_names = case["signed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();
        for name in &signed_names {
            write_signed(&signed_root.join(name), false);
        }
        for relative in case["archives"].as_array().unwrap() {
            let archive = archive_root
                .join(relative.as_str().unwrap())
                .join(filename_for(CODE));
            write_rich_archive(&archive);
        }
        if case["reviewExists"].as_bool() == Some(true) {
            let output = review_root
                .join(case["archives"][0].as_str().unwrap())
                .join(filename_for(CODE));
            fs::create_dir_all(output.parent().unwrap()).unwrap();
            fs::write(&output, b"USER FILE").unwrap();
        }
        let plan = preview(
            &params(&signed_root, &archive_root, &review_root, false),
            &FixtureOcr::one(&signed_names),
            |_| {},
            || false,
        )
        .unwrap();
        let replacements = plan
            .steps
            .iter()
            .filter(|step| step.kind == StepKind::PdfFirstPageReplace)
            .count();
        assert_eq!(
            replacements,
            case["steps"].as_u64().unwrap() as usize,
            "{}",
            case["name"]
        );
        assert_eq!(
            plan.warnings
                .iter()
                .map(|warning| warning.key.as_str())
                .collect::<Vec<_>>(),
            case["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>(),
            "{}",
            case["name"],
        );
        assert!(
            signed_names
                .iter()
                .all(|name| signed_root.join(name).is_file())
        );
    }
}

#[test]
fn archive_lookup_ignores_nonstandard_names_even_when_the_path_contains_the_rfi_code() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("signed.pdf"), false);
    write_rich_archive(
        &archive_root
            .join("Package/Civil")
            .join(format!("Archived {CODE} approved copy.pdf")),
    );
    write_rich_archive(&archive_root.join(CODE).join("approved.pdf"));
    write_rich_archive(&archive_root.join(CODE).join("Sketch.pdf"));
    fs::write(
        archive_root.join(filename_for(CODE)).with_extension("docx"),
        b"not an archive PDF",
    )
    .unwrap();

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();

    assert!(
        plan.steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert_eq!(plan.warnings.len(), 1);
    assert_eq!(plan.warnings[0].key, "warn.rfi.archiveMissing");
    assert_eq!(
        plan.meta["warningReviewItems"][0]["reasons"][0]["code"],
        json!(CODE)
    );
}

#[test]
fn approved_ocr_alias_is_accepted_and_short_archive_numbers_match_canonically() {
    assert_eq!(normalize_candidate_number("64").as_deref(), Some("0064"));
    assert_eq!(normalize_candidate_number("0064").as_deref(), Some("0064"));
    assert_eq!(normalize_candidate_number("10064"), None);
    for archive_number in ["64", "0064"] {
        let fixture = tempfile::tempdir().unwrap();
        let (signed_root, archive_root, review_root) = roots(&fixture);
        write_signed(&signed_root.join("signed.pdf"), false);
        let archive = archive_root.join("Package/Civil").join(format!(
            "RFI No. AZE-SUN-INT-QMS-RFI-CIV-{archive_number}.pdf"
        ));
        write_rich_archive(&archive);

        let plan = preview(
            &params(&signed_root, &archive_root, &review_root, false),
            &AliasFixtureOcr {
                code: CODE.to_string(),
            },
            |_| {},
            || false,
        )
        .unwrap();

        assert!(
            plan.steps
                .iter()
                .any(|step| step.kind == StepKind::PdfFirstPageReplace),
            "archive number {archive_number} should resolve the canonical RFI"
        );
        assert_eq!(
            plan.meta["editor"]["rows"][0]["code"],
            json!(CODE),
            "persisted plan code must be canonical"
        );
    }
}

#[test]
fn repaired_rfi_identity_remains_review_only() {
    let fixture = tempfile::tempdir().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("signed.pdf"), false);
    write_rich_archive(&archive_root.join("Package/Civil").join(filename_for(CODE)));

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &RepairedFixtureOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert!(
        plan.steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.key == "warn.no_code")
    );
    assert_eq!(
        plan.meta["warningReviewItems"][0]["reasons"][0]["key"],
        json!("warn.no_code")
    );
}
#[test]
fn archive_ambiguity_requires_an_allowlisted_rfi_no_pdf_selection() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("signed.pdf"), false);
    let first = Path::new("Other").join(filename_for(CODE));
    let second = Path::new("Package").join("Civil").join(filename_for(CODE));
    write_rich_archive(&archive_root.join(&first));
    write_rich_archive(&archive_root.join(&second));

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();

    assert!(
        plan.steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert_eq!(plan.warnings.len(), 1);
    assert_eq!(plan.warnings[0].key, "warn.rfi.archiveAmbiguous");
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["enabled"], true);
    assert_eq!(rows[0]["code"], CODE);
    assert_eq!(rows[0]["archiveFile"], "");
    assert_eq!(
        plan.meta["editor"]["requiredFields"],
        json!(["archiveFile"])
    );
    assert_eq!(
        plan.meta["editor"]["choicesByValue"]["archiveFile"]["values"][CODE],
        json!([
            "",
            format!("signed-{CODE}-candidate-1"),
            format!("signed-{CODE}-candidate-2")
        ])
    );
    assert_eq!(
        plan.meta["editor"]["choiceLabels"]["archiveFile"][&format!("signed-{CODE}-candidate-1")],
        json!(first.to_string_lossy())
    );
    assert_eq!(
        plan.meta["editor"]["choiceLabels"]["archiveFile"][&format!("signed-{CODE}-candidate-2")],
        json!(second.to_string_lossy())
    );

    let mut selected = rows.clone();
    selected[0]["archiveFile"] = json!(format!("signed-{CODE}-candidate-2"));
    let revised = revise(&plan, &json!({"rows": selected})).unwrap();
    let replacements = revised
        .steps
        .iter()
        .filter(|step| step.kind == StepKind::PdfFirstPageReplace)
        .collect::<Vec<_>>();
    assert_eq!(replacements.len(), 1);
    let canonical_archive_root = fs::canonicalize(&archive_root).unwrap();
    let canonical_review_root = fs::canonicalize(&review_root).unwrap();
    assert_eq!(
        replacements[0].src.as_deref(),
        Some(
            canonical_archive_root
                .join(&second)
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(
        replacements[0].dst.as_deref(),
        Some(
            canonical_review_root
                .join(&second)
                .to_string_lossy()
                .as_ref()
        )
    );

    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache_stage_plan(
        &authority,
        revised,
        &signed_root,
        &archive_root,
        &review_root,
    );
    fs::write(archive_root.join(&second), b"changed after selection").unwrap();
    let result = authority
        .apply(
            &cached.plan_id,
            &CallerIdentity::new("owner-1", "session-1"),
        )
        .unwrap();
    assert_eq!(result.failed, 1);
    assert!(!review_root.join(&second).exists());

    let mut missing = rows.clone();
    missing[0]["archiveFile"] = json!("");
    assert!(revise(&plan, &json!({"rows": missing})).is_err());

    let mut forged = rows.clone();
    forged[0]["archiveFile"] = json!("Forged/RFI No. AZE-SUN-INT-QMS-RFI-CIV-0064.pdf");
    assert!(revise(&plan, &json!({"rows": forged})).is_err());

    let mut disabled = rows.clone();
    disabled[0]["enabled"] = json!(false);
    let skipped = revise(&plan, &json!({"rows": disabled})).unwrap();
    assert!(
        skipped
            .steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
}

#[test]
fn archive_lookup_rejects_numeric_tails_suffixes_and_non_pdf_files() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("signed.pdf"), false);
    write_rich_archive(
        &archive_root
            .join("Package/Civil")
            .join(format!("RFI No. {CODE}5.pdf")),
    );
    write_rich_archive(
        &archive_root
            .join("Package/Civil")
            .join(format!("RFI No. {CODE} approved.pdf")),
    );
    write_rich_archive(
        &archive_root
            .join("Package/Civil")
            .join(format!("RFI No. {CODE}٥.pdf")),
    );
    write_rich_archive(
        &archive_root
            .join("Package/Civil")
            .join(format!("RFI No. {CODE}.txt")),
    );

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();

    assert!(
        plan.steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert_eq!(plan.warnings.len(), 1);
    assert_eq!(plan.warnings[0].key, "warn.rfi.archiveMissing");
}

#[test]
fn preview_preserves_scan_subfolders_and_surfaces_pdf_fidelity_warnings() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let nested_signed = signed_root.join("incoming/day-1/signed.pdf");
    let archive = archive_root
        .join("Package/Civil")
        .join(format!("rfi no. {}.PdF", CODE.to_ascii_lowercase()));
    write_signed(&nested_signed, true);
    write_rich_archive(&archive);

    let shallow = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    assert!(shallow.steps.is_empty());
    assert_eq!(shallow.warnings[0].key, "warn.rfi.noSignedPdfs");

    let recursive = preview(
        &params(&signed_root, &archive_root, &review_root, true),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(
        recursive
            .steps
            .iter()
            .filter(|step| step.kind == StepKind::PdfFirstPageReplace)
            .count(),
        1
    );
    assert!(
        recursive
            .warnings
            .iter()
            .any(|warning| warning.key == "warn.rfi.signedExtraPages")
    );

    write_signed_document(&archive);
    let unsupported = preview(
        &params(&signed_root, &archive_root, &review_root, true),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    assert!(
        unsupported
            .steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert!(
        unsupported
            .warnings
            .iter()
            .any(|warning| warning.key == "warn.rfi.pdfFidelityUnsupported")
    );

    write_tagged_document(&archive);
    let tagged = preview(
        &params(&signed_root, &archive_root, &review_root, true),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    assert!(
        tagged
            .steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert!(
        tagged
            .warnings
            .iter()
            .any(|warning| warning.key == "warn.rfi.pdfFidelityUnsupported")
    );
}

#[test]
fn fuzzy_prefixes_never_authorize_signed_page_staging() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("fuzzy.pdf"), false);
    write_rich_archive(&archive_root.join("Package/Civil").join(filename_for(CODE)));
    let ocr = FixtureOcr {
        codes: BTreeMap::from([(
            "fuzzy.pdf".to_string(),
            "AZE-SUN-INT-QMX-RFI-CIV-0064".to_string(),
        )]),
    };

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &ocr,
        |_| {},
        || false,
    )
    .unwrap();

    let review_copy = plan
        .steps
        .iter()
        .find(|step| step.kind == StepKind::Copy)
        .expect("warning source is routed to review");
    let canonical_review_root = fs::canonicalize(&review_root).unwrap();
    assert_eq!(
        review_copy.dst.as_deref().map(Path::new),
        Some(canonical_review_root.join("_review/fuzzy.pdf").as_path())
    );
    assert_eq!(review_copy.meta["warningReview"], true);
    assert!(review_copy.meta["expectedSourceSha256"].is_string());
    assert_eq!(plan.warnings[0].key, "warn.no_code");
    assert!(plan.warnings[0].subject.contains("fuzzy.pdf"));
    assert!(plan.warnings[0].subject.contains("page 1"));
    assert_eq!(plan.meta["warningReviewItems"][0]["status"], "planned");
    assert_eq!(
        plan.meta["warningReviewItems"][0]["reasons"][0]["key"],
        "warn.no_code"
    );
    assert_eq!(plan.meta["archiveWorkerCount"], 0);
    assert_eq!(plan.meta["archiveDirectoriesScanned"], 0);
    assert_eq!(plan.meta["archiveEntriesScanned"], 0);
}

#[test]
fn warning_review_copy_revalidates_the_previewed_source_hash() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let source = signed_root.join("unmatched.pdf");
    write_signed(&source, false);
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr {
            codes: BTreeMap::new(),
        },
        |_| {},
        || false,
    )
    .unwrap();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache_stage_plan(&authority, plan, &signed_root, &archive_root, &review_root);
    fs::write(&source, b"changed after preview").unwrap();

    let result = authority
        .apply(
            &cached.plan_id,
            &CallerIdentity::new("owner-1", "session-1"),
        )
        .unwrap();

    assert_eq!(result.failed, 1);
    assert!(!review_root.join("_review/unmatched.pdf").exists());
}

#[cfg(unix)]
#[test]
fn recursive_discovery_never_follows_signed_or_archive_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let signed_fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&signed_fixture);
    let outside_signed = signed_fixture.path().join("outside-signed");
    write_signed(&outside_signed.join("signed.pdf"), false);
    symlink(&outside_signed, signed_root.join("linked-scans")).unwrap();
    write_rich_archive(&archive_root.join("Package/Civil").join(filename_for(CODE)));

    let signed_plan = preview(
        &params(&signed_root, &archive_root, &review_root, true),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    assert!(signed_plan.steps.is_empty());
    assert_eq!(signed_plan.warnings[0].key, "warn.rfi.noSignedPdfs");

    let archive_fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&archive_fixture);
    write_signed(&signed_root.join("signed.pdf"), false);
    let outside_archive = archive_fixture.path().join("outside-archive");
    write_rich_archive(&outside_archive.join(filename_for(CODE)));
    symlink(&outside_archive, archive_root.join("linked-package")).unwrap();

    let archive_plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    assert!(
        archive_plan
            .steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert_eq!(archive_plan.warnings[0].key, "warn.rfi.archiveMissing");
}

#[cfg(unix)]
#[test]
fn archive_lookup_never_follows_direct_pdf_symlinks() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("signed.pdf"), false);
    let outside_archive = fixture.path().join("outside-archive.pdf");
    write_rich_archive(&outside_archive);
    symlink(&outside_archive, archive_root.join(filename_for(CODE))).unwrap();

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();

    assert!(
        plan.steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert_eq!(plan.warnings.len(), 1);
    assert_eq!(plan.warnings[0].key, "warn.rfi.archiveMissing");
}

#[test]
fn revise_can_only_disable_existing_staging_rows_and_never_recalculates_paths() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    write_signed(&signed_root.join("signed.pdf"), false);
    write_rich_archive(&archive_root.join("Package/Civil").join(filename_for(CODE)));
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["enabled"] = json!(false);

    let revised = revise(&plan, &json!({"rows": rows})).unwrap();
    assert!(
        revised
            .steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert!(
        plan.steps
            .iter()
            .any(|step| step.kind == StepKind::PdfFirstPageReplace)
    );

    let forged = json!([{"_rowId": "forged", "enabled": true}]);
    assert!(revise(&plan, &json!({"rows": forged})).is_err());
}

fn cache_stage_plan(
    authority: &FilesystemAuthority,
    plan: Plan,
    signed: &Path,
    archive: &Path,
    review: &Path,
) -> Plan {
    let scope = GrantScope::new(
        "owner-1",
        "session-1",
        "preview-1",
        "rfi.stage_signed_first_pages",
    );
    let signed_grant = authority
        .issue_grant(&scope, signed, GrantAccess::Read)
        .unwrap();
    let archive_grant = authority
        .issue_grant(&scope, archive, GrantAccess::Read)
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
                "rfi.stage_signed_first_pages",
                vec![
                    signed_grant.grant_id,
                    archive_grant.grant_id,
                    review_grant.grant_id,
                ],
            ),
            plan,
        )
        .unwrap()
}

#[test]
fn completed_publish_stale_hardlink_is_reviewed_and_cleaned_without_touching_output() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let signed = signed_root.join("signed.pdf");
    let archive = archive_root.join("Package/Civil").join(filename_for(CODE));
    let output = review_root.join("Package/Civil").join(filename_for(CODE));
    write_signed(&signed, false);
    write_rich_archive(&archive);
    write_rich_archive(&output);
    let output_hash = file_sha256(&output).unwrap();
    let stale = output.with_file_name(format!(
        ".{}.docflow-leftover.tmp",
        output.file_name().unwrap().to_string_lossy()
    ));
    fs::hard_link(&output, &stale).unwrap();

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();

    assert_eq!(
        plan.steps.iter().map(|step| step.kind).collect::<Vec<_>>(),
        [StepKind::CleanupTemp]
    );
    assert_eq!(
        plan.warnings
            .iter()
            .map(|warning| warning.key.as_str())
            .collect::<Vec<_>>(),
        ["warn.rfi.staleTempFound"]
    );
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache_stage_plan(&authority, plan, &signed_root, &archive_root, &review_root);
    let result = authority
        .apply(
            &cached.plan_id,
            &CallerIdentity::new("owner-1", "session-1"),
        )
        .unwrap();

    assert_eq!((result.applied, result.failed), (1, 0));
    assert!(!stale.exists());
    assert_eq!(file_sha256(&output).unwrap(), output_hash);
    let journal = authority.journals().read(&result.journal_id).unwrap();
    assert!(journal.steps.is_empty(), "cleanup is not an undo authority");
}

#[test]
fn same_bytes_in_an_unrelated_temp_copy_never_authorize_deletion() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let signed = signed_root.join("signed.pdf");
    let archive = archive_root.join("Package/Civil").join(filename_for(CODE));
    let output = review_root.join("Package/Civil").join(filename_for(CODE));
    write_signed(&signed, false);
    write_rich_archive(&archive);
    write_rich_archive(&output);
    let output_bytes = fs::read(&output).unwrap();
    let unrelated = output.with_file_name(format!(
        ".{}.docflow-unrelated.tmp",
        output.file_name().unwrap().to_string_lossy()
    ));
    fs::copy(&output, &unrelated).unwrap();

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &AliasFixtureOcr {
            code: CODE.to_string(),
        },
        |_| {},
        || false,
    )
    .unwrap();

    assert!(
        plan.steps
            .iter()
            .all(|step| step.kind != StepKind::PdfFirstPageReplace)
    );
    assert_eq!(plan.warnings[0].key, "warn.rfi.reviewExists");
    assert_eq!(fs::read(&output).unwrap(), output_bytes);
    assert!(unrelated.is_file());
}

#[test]
fn approved_stage_apply_revalidates_hashes_never_clobbers_and_undoes_created_output() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let signed = signed_root.join("signed.pdf");
    let archive = archive_root.join("Package/Civil").join(filename_for(CODE));
    write_signed(&signed, false);
    write_rich_archive(&archive);
    let archive_hash = file_sha256(&archive).unwrap();
    let signed_hash = file_sha256(&signed).unwrap();
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache_stage_plan(&authority, plan, &signed_root, &archive_root, &review_root);
    let caller = CallerIdentity::new("owner-1", "session-1");

    let result = authority.apply(&cached.plan_id, &caller).unwrap();
    let output = review_root.join("Package/Civil").join(filename_for(CODE));
    assert_eq!(
        (result.failed, result.meta["effectState"].as_str()),
        (0, Some("committed"))
    );
    assert!(output.is_file());
    assert_eq!(file_sha256(&archive).unwrap(), archive_hash);
    assert_eq!(file_sha256(&signed).unwrap(), signed_hash);
    let journal = authority.journals().read(&result.journal_id).unwrap();
    let canonical_output = fs::canonicalize(&output).unwrap();
    assert!(
        journal.steps.iter().any(|step| {
            step.kind == StepKind::Copy
                && step.dst.as_deref().map(Path::new) == Some(canonical_output.as_path())
        }),
        "journal steps: {:?}",
        journal.steps,
    );

    let undone = authority.undo(&result.journal_id, &caller, None).unwrap();
    assert_eq!(undone.failed, 0);
    assert!(!output.exists());
    assert!(!review_root.join("Package").exists());

    let second_plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    let second = cache_stage_plan(
        &authority,
        second_plan,
        &signed_root,
        &archive_root,
        &review_root,
    );
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, b"USER RACE WINNER").unwrap();
    let collision = authority.apply(&second.plan_id, &caller).unwrap();
    assert_eq!(collision.failed, 1);
    assert_eq!(fs::read(&output).unwrap(), b"USER RACE WINNER");
}

#[test]
fn stage_cancellation_and_pending_journal_recovery_are_explicit_and_non_mutating() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let signed = signed_root.join("signed.pdf");
    let archive = archive_root.join("Package/Civil").join(filename_for(CODE));
    write_signed(&signed, false);
    write_rich_archive(&archive);
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    let journal_root = fixture.path().join("journal");
    let authority = FilesystemAuthority::new(&journal_root, Duration::from_secs(300));
    let cached = cache_stage_plan(
        &authority,
        plan.clone(),
        &signed_root,
        &archive_root,
        &review_root,
    );
    let plan_id = cached.plan_id.clone();
    let caller = CallerIdentity::new("owner-1", "session-1");
    let cancelled = authority
        .apply_with_progress(&plan_id, &caller, |event| {
            if event.done == 0 {
                assert!(authority.cancel(&plan_id));
            }
        })
        .unwrap();
    assert_eq!(cancelled.meta["effectState"], json!("cancelled_clean"));
    assert!(fs::read_dir(&review_root).unwrap().next().is_none());

    let output = review_root.join("Package/Civil").join(filename_for(CODE));
    let replacement = plan
        .steps
        .iter()
        .find(|step| step.kind == StepKind::PdfFirstPageReplace)
        .unwrap()
        .clone();
    let store = JournalStore::new(&journal_root);
    let mut pending = JournalRecord::new(
        "slice7-pending",
        "rfi.stage_signed_first_pages",
        "2026-08-25T00:00:00+00:00",
        "owner-1",
        "session-1",
        "digest",
    );
    pending.pending_step = Some(replacement);
    pending.state = Some(JournalState::Applying);
    store.write(&pending).unwrap();
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, b"UNCERTAIN PUBLISHED OUTPUT").unwrap();
    let recovery = store.recover_pending().unwrap();
    assert!(
        recovery
            .iter()
            .any(|entry| entry.journal_id == "slice7-pending"
                && entry.disposition == RecoveryDisposition::OutcomeUnknown)
    );
    assert_eq!(fs::read(&output).unwrap(), b"UNCERTAIN PUBLISHED OUTPUT");
}

#[test]
fn changed_approved_inputs_fail_without_publishing_or_recalculating() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let signed = signed_root.join("signed.pdf");
    let archive = archive_root.join("Package/Civil").join(filename_for(CODE));
    write_signed(&signed, false);
    write_rich_archive(&archive);
    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    let authority =
        FilesystemAuthority::new(fixture.path().join("journal"), Duration::from_secs(300));
    let cached = cache_stage_plan(&authority, plan, &signed_root, &archive_root, &review_root);
    fs::write(&signed, b"changed after approval").unwrap();

    let result = authority
        .apply(
            &cached.plan_id,
            &CallerIdentity::new("owner-1", "session-1"),
        )
        .unwrap();
    assert_eq!(result.failed, 1);
    assert!(
        !review_root
            .join("Package/Civil")
            .join(filename_for(CODE))
            .exists()
    );
    assert!(!review_root.join("Package").exists());
}

#[test]
fn review_root_overlap_is_rejected_without_any_plan_authority() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, _review_root) = roots(&fixture);
    write_signed(&signed_root.join("signed.pdf"), false);
    write_rich_archive(&archive_root.join("Package/Civil").join(filename_for(CODE)));
    let overlapping = archive_root.join("review");
    fs::create_dir(&overlapping).unwrap();

    let plan = preview(
        &params(&signed_root, &archive_root, &overlapping, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();
    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "error.rfi.reviewRootMustBeSeparate");
}

#[test]
#[ignore = "set DOCFLOW_VENTOY_SIGNED_SCAN_ROOT, DOCFLOW_VENTOY_SIGNED_ARCHIVE_ROOT, and DOCFLOW_VENTOY_TESSERACT to private copies; archive root may be empty"]
fn operator_ventoy_signed_pages_preview_matches_nine_real_rfi_files() {
    let signed_root = std::path::PathBuf::from(
        std::env::var("DOCFLOW_VENTOY_SIGNED_SCAN_ROOT")
            .expect("DOCFLOW_VENTOY_SIGNED_SCAN_ROOT is required"),
    );
    let archive_root = std::path::PathBuf::from(
        std::env::var("DOCFLOW_VENTOY_SIGNED_ARCHIVE_ROOT")
            .expect("DOCFLOW_VENTOY_SIGNED_ARCHIVE_ROOT is required"),
    );
    let tesseract = std::path::PathBuf::from(
        std::env::var("DOCFLOW_VENTOY_TESSERACT").expect("DOCFLOW_VENTOY_TESSERACT is required"),
    );
    let review = tempfile::tempdir().unwrap();
    let source_files = [
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0901.pdf", "0901"),
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1131.pdf", "1131"),
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1199.pdf", "1199"),
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1261.pdf", "1261"),
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1296.pdf", "1296"),
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1067.pdf", "1067"),
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1132.pdf", "1132"),
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1252.pdf", "1252"),
        ("RFI No. AZE-SUN-INT-QMS-RFI-CIV-1253.pdf", "1253"),
    ];
    let source_hashes = source_files.map(|(source_name, _)| {
        let source = signed_root.join(source_name);
        assert!(
            source.is_file(),
            "missing private signed scan {source_name}"
        );
        (source_name, file_sha256(&source).unwrap())
    });
    let plan = preview(
        &params(&signed_root, &archive_root, review.path(), false),
        &docflow_lib::ocr::NativeOcr::with_tesseract(tesseract),
        |_| {},
        || false,
    )
    .unwrap();
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    let warning_items = plan.meta["warningReviewItems"].as_array().unwrap();
    let recognized = rows
        .iter()
        .filter_map(|row| row["code"].as_str())
        .chain(
            warning_items
                .iter()
                .flat_map(|item| item["reasons"].as_array().into_iter().flatten())
                .filter_map(|reason| reason["code"].as_str()),
        )
        .collect::<std::collections::BTreeSet<_>>();
    for (_, number) in source_files {
        let code = format!("AZE-SUN-INT-QMS-RFI-CIV-{number}");
        assert!(
            recognized.contains(code.as_str()),
            "missing {code}; rows={rows:#?}, review={warning_items:#?}"
        );
    }
    assert!(
        recognized.contains("AZE-SUN-INT-QMS-RFI-CIV-0901"),
        "short RFI number 901 must be normalized to 0901"
    );
    assert!(
        recognized.contains("AZE-SUN-INT-QMS-RFI-CIV-1296"),
        "independently confirmed 1296 must be recognized"
    );
    for item in warning_items {
        for reason in item["reasons"].as_array().into_iter().flatten() {
            if reason["key"] == "warn.rfi.archiveMissing" {
                assert!(
                    reason["code"]
                        .as_str()
                        .is_some_and(|code| recognized.contains(code)),
                    "archive-missing warning must retain the canonical per-page code"
                );
            }
        }
    }
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.key == "warn.rfi.archiveMissing"),
        "private archive copy should retain warning coverage for genuine missing originals"
    );
    for (source_name, expected_hash) in source_hashes {
        assert_eq!(
            file_sha256(&signed_root.join(source_name)).unwrap(),
            expected_hash,
            "preview must not change source scans"
        );
    }
    assert_eq!(fs::read_dir(review.path()).unwrap().count(), 0);
}
