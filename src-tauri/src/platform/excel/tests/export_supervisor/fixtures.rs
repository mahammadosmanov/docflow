use super::*;

pub(super) fn descriptor(name: &str, workbook_index: u32) -> WorksheetDescriptor {
    WorksheetDescriptor {
        workbook_index,
        sheet_id: workbook_index,
        relationship_id: format!("rId{workbook_index}"),
        relationship_type: "worksheet".to_string(),
        part_path: format!("xl/worksheets/sheet{workbook_index}.xml"),
        name: name.to_string(),
        visibility: WorksheetVisibility::Visible,
    }
}

pub(super) fn safety() -> WorkbookSafetyFacts {
    WorkbookSafetyFacts::default()
}

pub(super) fn worksheet_job(
    source: &std::path::Path,
    signature: FileSignature,
    name: &str,
    workbook_index: u32,
) -> PrintJob {
    PrintJob {
        job_id: format!("worksheet-{workbook_index}"),
        path: source.to_string_lossy().into_owned(),
        printer: "Office Printer".to_string(),
        verb: "docflow".to_string(),
        format: "XLSX".to_string(),
        page_range: "all".to_string(),
        signature,
        worksheet: Some(descriptor(name, workbook_index)),
        workbook_safety: Some(safety()),
        timeout_minutes: 1,
        extra: serde_json::Map::new(),
        ..PrintJob::default()
    }
}

pub(super) fn source_fixture() -> (
    tempfile::TempDir,
    crate::cap_fs::AnchoredPath,
    FileSignature,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("book.xlsx");
    std::fs::write(&path, b"approved workbook bytes").unwrap();
    let root = CapabilityRoot::open(directory.path()).unwrap();
    let source = root.authorize(&path).unwrap().anchor().unwrap();
    let mut file = File::open(path).unwrap();
    let signature = crate::print_queue::file_signature_from_open_file(&mut file, true).unwrap();
    (directory, source, signature)
}

pub(super) fn multi_page_pdf(page_count: usize) -> Vec<u8> {
    super::super::super::fixture_pdf_bytes(page_count)
}

pub(super) fn write_workbook(path: &Path) {
    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const REL: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="MEC" sheetId="17" r:id="rId1"/><sheet name="Hidden" sheetId="18" state="hidden" r:id="rId2"/><sheet name="CIV" sheetId="19" r:id="rId3"/></sheets></workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Type="{REL}" Target="worksheets/sheet3.xml"/></Relationships>"#
    );
    let package_relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let file = File::create(path).unwrap();
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    for (name, bytes) in [
        ("_rels/.rels", package_relationships.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", relationships.as_bytes()),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    for index in 1..=3 {
        writer
            .start_file(format!("xl/worksheets/sheet{index}.xml"), options)
            .unwrap();
        writer
            .write_all(b"<worksheet><sheetData/></worksheet>")
            .unwrap();
    }
    writer.finish().unwrap();
}

pub(super) fn assert_cleanup_review_failure(
    pipeline: &ExcelExportPipeline,
    failure: &PrintFailure,
    expected_kind: crate::print_queue::PrintFailureKind,
    expected_message: &str,
    events: &Arc<Mutex<Vec<&'static str>>>,
    submitted: &SubmittedPdfs,
) {
    let retained = pipeline
        .quarantine
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(failure.kind, expected_kind);
    assert_eq!(failure.message, expected_message);
    assert!(!failure.uncertain);
    let review = failure.cleanup_review.as_ref().expect("cleanup review");
    assert_eq!(
        review.warning_key,
        "error.print.exportCleanupRequiresReview"
    );
    assert!(review.locator.starts_with("docflow-excel-"));
    assert!(pipeline.breaker.load(Ordering::Acquire));
    assert_eq!(retained.len(), 1);
    assert!(retained[0].exists());
    assert_eq!(*events.lock().unwrap(), ["policy", "export"]);
    assert!(submitted.lock().unwrap().is_empty());
    pipeline.clear_test_quarantine();
}
