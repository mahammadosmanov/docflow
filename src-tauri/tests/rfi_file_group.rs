use std::{fs, io::Write, path::Path};

use docflow_lib::{
    operations::{
        process_scans::{OcrAdapter, OcrFailure, OcrRead},
        rfi_file_group::{GroupRfiFilesParams, preview, revise},
    },
    rfi_code::{RfiMatch, RfiMethod},
};
use serde_json::json;
use zip::{ZipWriter, write::SimpleFileOptions};

struct NoOcr;

impl OcrAdapter for NoOcr {
    fn read(&self, _path: &Path) -> Result<OcrRead, OcrFailure> {
        Ok(OcrRead {
            matched: RfiMatch {
                code: String::new(),
                discipline: String::new(),
                number: String::new(),
                method: RfiMethod::Miss,
                confidence: 0.0,
            },
            source: "test".to_string(),
        })
    }

    fn diagnostics(&self) -> String {
        "test OCR".to_string()
    }
}

fn write_docx(path: &Path, text: &str) {
    let file = fs::File::create(path).unwrap();
    let mut archive = ZipWriter::new(file);
    archive
        .start_file("word/document.xml", SimpleFileOptions::default())
        .unwrap();
    write!(
        archive,
        "<w:document xmlns:w=\"urn:test\"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"
    )
    .unwrap();
    archive.finish().unwrap();
}

fn write_docx_xml(path: &Path, xml: &str) {
    let file = fs::File::create(path).unwrap();
    let mut archive = ZipWriter::new(file);
    archive
        .start_file("word/document.xml", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(xml.as_bytes()).unwrap();
    archive.finish().unwrap();
}

#[test]
fn unsupported_exact_discipline_creates_no_grouped_output() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-ARC-1263.pdf"),
        b"unsupported discipline",
    )
    .unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-ARC-1263.eml"),
        b"Subject: AZE-SUN-INT-QMS-RFI-ARC-1263",
    )
    .unwrap();

    let plan = preview(
        &GroupRfiFilesParams {
            source,
            destination: destination.clone(),
        },
        &NoOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert!(plan.meta["editor"]["rows"].as_array().unwrap().is_empty());
    assert!(
        plan.steps.iter().all(|step| step
            .meta
            .get("warningReview")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)),
        "unsupported identifiers may only result in _review handling"
    );
    assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
}

#[test]
fn preview_groups_exact_filename_office_email_and_utf16_codes_without_mutating_sources() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::create_dir(source.join("Old Yerlesdirilmis")).unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-CIV-1260 SKETCH.pdf"),
        b"filename is sufficient",
    )
    .unwrap();
    write_docx(
        &source.join("2.AF110-CON-MEC-002 Checklist.docx"),
        "AZE-SUN-INT-QMS-RFI-\nCIV-1263",
    );
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-CIV-1260_1261.eml"),
        b"Subject: RFIs\r\n\r\nAZE-SUN-INT-QMS-RFI-CIV-1262",
    )
    .unwrap();
    let message = "AZE-SUN-INT-QMS-RFI-CIV-1263 and AZE-SUN-INT-QMS-RFI-CIV-1264"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    fs::write(source.join("Outlook item.msg"), message).unwrap();
    fs::write(source.join("unmatched.bin"), b"no code").unwrap();
    fs::write(
        source
            .join("Old Yerlesdirilmis")
            .join("AZE-SUN-INT-QMS-RFI-CIV-9999.pdf"),
        b"ignored subfolder",
    )
    .unwrap();

    let mut progress = Vec::new();
    let plan = preview(
        &GroupRfiFilesParams {
            source: source.clone(),
            destination: destination.clone(),
        },
        &NoOcr,
        |event| progress.push(event),
        || false,
    )
    .unwrap();

    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 7);
    let codes = rows
        .iter()
        .map(|row| row["rfiCode"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"AZE-SUN-INT-QMS-RFI-CIV-1260"));
    assert!(codes.contains(&"AZE-SUN-INT-QMS-RFI-CIV-1261"));
    assert!(codes.contains(&"AZE-SUN-INT-QMS-RFI-CIV-1262"));
    assert!(codes.contains(&"AZE-SUN-INT-QMS-RFI-CIV-1263"));
    assert!(codes.contains(&"AZE-SUN-INT-QMS-RFI-CIV-1264"));
    assert!(plan.warnings.iter().any(|warning| {
        warning.key == "warn.rfi.fileGroup.noCode" && warning.subject == "unmatched.bin"
    }));
    assert!(!serde_json::to_string(&plan).unwrap().contains("9999"));
    assert!(
        plan.steps.iter().any(|step| {
            step.src
                .as_deref()
                .and_then(|value| Path::new(value).file_name())
                == Some(std::ffi::OsStr::new("Outlook item.msg"))
                && step
                    .dst
                    .as_deref()
                    .is_some_and(|value| value.contains("CIV-1264"))
        }),
        "{:#?}",
        plan.steps
    );
    assert!(plan.steps.iter().any(|step| {
        step.src
            .as_deref()
            .and_then(|value| Path::new(value).file_name())
            == Some(std::ffi::OsStr::new("unmatched.bin"))
            && step
                .dst
                .as_deref()
                .is_some_and(|value| value.contains("_review"))
    }));
    assert!(
        progress
            .iter()
            .any(|event| event.phase == "file_group_discovery")
    );
    assert!(
        progress
            .iter()
            .any(|event| event.phase == "file_group_analysis")
    );
    assert_eq!(progress.last().unwrap().phase, "preview_ready");

    let revised_rows = rows
        .iter()
        .cloned()
        .map(|mut row| {
            if row["rfiCode"] == "AZE-SUN-INT-QMS-RFI-CIV-1264" {
                row["enabled"] = json!(false);
            }
            row
        })
        .collect::<Vec<_>>();
    let revised = revise(&plan, &json!({"rows": revised_rows})).unwrap();
    assert!(!revised.steps.iter().any(|step| {
        step.dst
            .as_deref()
            .is_some_and(|value| value.contains("CIV-1264"))
    }));
    assert!(revised.steps.iter().any(|step| {
        step.dst
            .as_deref()
            .is_some_and(|value| value.contains("_review"))
    }));
}

#[test]
fn preview_finds_a_wrapped_rfi_in_a_word_table_without_joining_the_next_cell() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    write_docx_xml(
        &source.join("Tracker Installation Checklist.docx"),
        concat!(
            r#"<w:document xmlns:w="urn:test"><w:body><w:tbl><w:tr>"#,
            r#"<w:tc><w:p><w:r><w:t>Report/RFI No.</w:t></w:r></w:p></w:tc>"#,
            r#"<w:tc><w:p><w:r><w:t>AZE-SUN-INT-QMS-RFI-</w:t><w:br/><w:t>MEC-0765</w:t></w:r></w:p></w:tc>"#,
            r#"</w:tr><w:tr><w:tc><w:p><w:r><w:t>MOS No.</w:t></w:r></w:p></w:tc></w:tr>"#,
            r#"</w:tbl></w:body></w:document>"#,
        ),
    );
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-MEC-0765.eml"),
        b"Subject: AZE-SUN-INT-QMS-RFI-MEC-0765",
    )
    .unwrap();

    let plan = preview(
        &GroupRfiFilesParams {
            source,
            destination,
        },
        &NoOcr,
        |_| {},
        || false,
    )
    .unwrap();

    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert!(rows.iter().any(|row| {
        row["sourceFile"] == "Tracker Installation Checklist.docx"
            && row["rfiCode"] == "AZE-SUN-INT-QMS-RFI-MEC-0765"
    }));
    assert!(!plan.warnings.iter().any(|warning| {
        warning.key == "warn.rfi.fileGroup.noCode"
            && warning
                .subject
                .contains("Tracker Installation Checklist.docx")
    }));
}

#[test]
fn preview_rejects_overlapping_source_and_destination() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("incoming");
    fs::create_dir(&source).unwrap();
    let error = preview(
        &GroupRfiFilesParams {
            source: source.clone(),
            destination: source,
        },
        &NoOcr,
        |_| {},
        || false,
    )
    .unwrap_err();
    assert_eq!(error.code, "path_invalid");
}

#[test]
fn decorated_or_extended_identifiers_are_not_silently_accepted() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(
        source.join("ÂAZE-SUN-INT-QMS-RFI-CIV-1263.pdf"),
        b"invalid prefix",
    )
    .unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-CIV-1264X.pdf"),
        b"invalid suffix",
    )
    .unwrap();

    let plan = preview(
        &GroupRfiFilesParams {
            source,
            destination,
        },
        &NoOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert!(plan.meta["editor"]["rows"].as_array().unwrap().is_empty());
    assert_eq!(
        plan.warnings
            .iter()
            .filter(|warning| warning.key == "warn.rfi.fileGroup.unreadable")
            .count(),
        2
    );
}

#[test]
fn preview_skips_existing_targets_and_unsafe_folder_collisions() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let existing_name = "AZE-SUN-INT-QMS-RFI-CIV-1263.pdf";
    fs::write(source.join(existing_name), b"new source").unwrap();
    let existing_directory = destination.join("AZE-SUN-INT-QMS-RFI-CIV-1263");
    fs::create_dir(&existing_directory).unwrap();
    fs::write(existing_directory.join(existing_name), b"existing target").unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-CIV-1264.pdf"),
        b"other source",
    )
    .unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-CIV-1263_1264.eml"),
        b"Subject: collision fixtures",
    )
    .unwrap();
    fs::write(
        existing_directory.join("AZE-SUN-INT-QMS-RFI-CIV-1263_1264.eml"),
        b"existing mail target",
    )
    .unwrap();
    fs::write(
        destination.join("AZE-SUN-INT-QMS-RFI-CIV-1264"),
        b"folder-name collision",
    )
    .unwrap();

    let plan = preview(
        &GroupRfiFilesParams {
            source,
            destination,
        },
        &NoOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert!(plan.steps.is_empty());
    assert_eq!(
        plan.warnings
            .iter()
            .filter(|warning| warning.key == "warn.rfi.fileGroup.exists")
            .count(),
        4
    );
}

#[test]
fn preview_refuses_every_rfi_output_when_that_rfi_has_no_email_file() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-MEC-0765.pdf"),
        b"known RFI without mail",
    )
    .unwrap();
    write_docx(
        &source.join("Tracker Installation Checklist.docx"),
        "AZE-SUN-INT-QMS-RFI-\nMEC-0765",
    );

    let plan = preview(
        &GroupRfiFilesParams {
            source,
            destination,
        },
        &NoOcr,
        |_| {},
        || false,
    )
    .unwrap();

    assert!(plan.meta["editor"]["rows"].as_array().unwrap().is_empty());
    assert!(plan.steps.is_empty());
    assert!(plan.warnings.iter().any(|warning| {
        warning.key == "warn.rfi.fileGroup.missingEmail"
            && warning.subject.contains("AZE-SUN-INT-QMS-RFI-MEC-0765")
            && warning
                .subject
                .contains("Tracker Installation Checklist.docx")
    }));
}

#[test]
fn email_requirement_is_enforced_independently_for_each_rfi() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-MEC-0765_0766.pdf"),
        b"two related RFIs",
    )
    .unwrap();
    fs::write(
        source.join("AZE-SUN-INT-QMS-RFI-MEC-0765.eml"),
        b"Subject: RFI package",
    )
    .unwrap();

    let plan = preview(
        &GroupRfiFilesParams {
            source,
            destination,
        },
        &NoOcr,
        |_| {},
        || false,
    )
    .unwrap();

    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert!(
        rows.iter()
            .all(|row| row["rfiCode"] == "AZE-SUN-INT-QMS-RFI-MEC-0765")
    );
    assert!(
        rows.iter()
            .any(|row| { row["sourceFile"] == "AZE-SUN-INT-QMS-RFI-MEC-0765_0766.pdf" })
    );
    assert!(plan.warnings.iter().any(|warning| {
        warning.key == "warn.rfi.fileGroup.missingEmail"
            && warning.subject.contains("AZE-SUN-INT-QMS-RFI-MEC-0766")
    }));
    assert!(!plan.steps.iter().any(|step| {
        step.dst
            .as_deref()
            .is_some_and(|destination| destination.contains("MEC-0766"))
    }));
}
