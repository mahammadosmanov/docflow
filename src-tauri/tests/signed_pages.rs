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
    pdf::{inspect, replace_first_page_incremental},
    plan_store::{BackendOrigin, PlanBinding},
    rfi_code::find,
};
use serde_json::{Value, json};
use support::pdf_fixture::{
    CODE, write_encrypted, write_object_stream_pdf, write_rich_archive, write_signed,
    write_signed_document, write_tagged_document,
};

#[derive(Default)]
struct FixtureOcr {
    codes: BTreeMap<String, String>,
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
fn preview_preserves_scan_subfolders_and_surfaces_pdf_fidelity_warnings() {
    let fixture = tempfile::TempDir::new().unwrap();
    let (signed_root, archive_root, review_root) = roots(&fixture);
    let nested_signed = signed_root.join("incoming/day-1/signed.pdf");
    let archive = archive_root.join("Package/Civil").join(filename_for(CODE));
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
    assert!(unsupported.steps.is_empty());
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
    assert!(tagged.steps.is_empty());
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

    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.no_code");
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
    assert!(archive_plan.steps.is_empty());
    assert_eq!(archive_plan.warnings[0].key, "warn.rfi.archiveMissing");
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
    let unrelated = output.with_file_name(format!(
        ".{}.docflow-unrelated.tmp",
        output.file_name().unwrap().to_string_lossy()
    ));
    fs::copy(&output, &unrelated).unwrap();

    let plan = preview(
        &params(&signed_root, &archive_root, &review_root, false),
        &FixtureOcr::one(&["signed.pdf"]),
        |_| {},
        || false,
    )
    .unwrap();

    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.rfi.reviewExists");
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
