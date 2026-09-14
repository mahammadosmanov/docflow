use std::{
    collections::VecDeque,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

#[cfg(unix)]
use docflow_lib::print_queue::file_signature;
use docflow_lib::{
    ooxml::{WorkbookSafetyFacts, WorksheetDescriptor, WorksheetVisibility},
    operations::batch_print::validate_job,
    page_range::PageRangeCapability,
    paths::CallerIdentity,
    print_queue::{
        BatchPrintParams, FileSignature, PrintAdapter, PrintFailure, PrintJob, PrintJobSupport,
        PrintService, PrintSettings, PrinterInventory, file_signature_from_open_file,
    },
    progress::GlobalMutationLock,
};
use serde_json::json;
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

type SubmitGate = Arc<(Mutex<(bool, bool)>, Condvar)>;

#[derive(Default)]
struct FakeAdapter {
    inventory: PrinterInventory,
    outcomes: Mutex<VecDeque<Result<(), PrintFailure>>>,
    submitted: Mutex<Vec<PrintJob>>,
    inspected_settings: Mutex<Vec<PrintSettings>>,
    inspected: AtomicUsize,
    pdf_page_count: Mutex<Option<u32>>,
    unsupported_paper: Mutex<Option<String>>,
    submit_gate: Mutex<Option<SubmitGate>>,
    inventory_gate: Mutex<Option<SubmitGate>>,
}

impl FakeAdapter {
    fn with_printers(names: &[&str], default: Option<&str>) -> Self {
        Self {
            inventory: PrinterInventory {
                printers: names.iter().map(|name| (*name).to_string()).collect(),
                default_printer: default.map(str::to_string),
            },
            ..Self::default()
        }
    }

    fn push(&self, outcome: Result<(), PrintFailure>) {
        self.outcomes.lock().unwrap().push_back(outcome);
    }

    fn set_pdf_page_count(&self, page_count: u32) {
        *self.pdf_page_count.lock().unwrap() = Some(page_count);
    }

    fn set_inventory_gate(&self, gate: SubmitGate) {
        *self.inventory_gate.lock().unwrap() = Some(gate);
    }
}

impl PrintAdapter for FakeAdapter {
    fn inventory(&self) -> Result<PrinterInventory, PrintFailure> {
        let gate = self.inventory_gate.lock().unwrap().take();
        if let Some(gate) = gate {
            let (state, changed) = &*gate;
            let mut state = state.lock().unwrap();
            state.0 = true;
            changed.notify_all();
            while !state.1 {
                state = changed.wait(state).unwrap();
            }
        }
        Ok(self.inventory.clone())
    }

    fn inspect(
        &self,
        path: &Path,
        _printer: &str,
        settings: &PrintSettings,
    ) -> Result<PrintJobSupport, PrintFailure> {
        self.inspected.fetch_add(1, Ordering::SeqCst);
        self.inspected_settings
            .lock()
            .unwrap()
            .push(settings.clone());
        if self.unsupported_paper.lock().unwrap().as_deref() == Some(settings.paper.as_str()) {
            return Err(PrintFailure::unsupported_options(
                "paper is unsupported by the test printer",
            ));
        }
        match path.extension().and_then(|value| value.to_str()) {
            Some("pdf") => Ok(PrintJobSupport::controlled_pdf(
                *self.pdf_page_count.lock().unwrap(),
            )),
            Some("png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff") => {
                Ok(PrintJobSupport::single_page())
            }
            Some("docx") => Ok(PrintJobSupport::native_application()),
            Some("xlsx" | "xlsm") => Ok(PrintJobSupport::controlled()),
            _ => Err(PrintFailure::unsupported("unsupported test format")),
        }
    }

    fn submit(&self, job: &PrintJob, cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure> {
        if cancelled() {
            return Err(PrintFailure::cancelled("cancelled by test", false));
        }
        self.submitted.lock().unwrap().push(job.clone());
        if let Some(gate) = self.submit_gate.lock().unwrap().clone() {
            let (state, changed) = &*gate;
            let mut state = state.lock().unwrap();
            state.0 = true;
            changed.notify_all();
            while !state.1 {
                state = changed.wait(state).unwrap();
            }
        }
        self.outcomes.lock().unwrap().pop_front().unwrap_or(Ok(()))
    }
}

#[cfg(unix)]
#[derive(Default)]
struct ParentSwapAdapter {
    swap_armed: std::sync::atomic::AtomicBool,
    visible_parent: PathBuf,
    pinned_parent: PathBuf,
    outside_parent: PathBuf,
    submitted_path: Mutex<Option<PathBuf>>,
    submitted_bytes: Mutex<Option<Vec<u8>>>,
}

#[cfg(unix)]
impl PrintAdapter for ParentSwapAdapter {
    fn inventory(&self) -> Result<PrinterInventory, PrintFailure> {
        Ok(PrinterInventory {
            printers: vec!["Office Printer".to_string()],
            default_printer: Some("Office Printer".to_string()),
        })
    }

    fn inspect(
        &self,
        _path: &Path,
        _printer: &str,
        _settings: &PrintSettings,
    ) -> Result<PrintJobSupport, PrintFailure> {
        if self.swap_armed.swap(false, Ordering::SeqCst) {
            use std::os::unix::fs::symlink;

            std::fs::rename(&self.visible_parent, &self.pinned_parent).unwrap();
            symlink(&self.outside_parent, &self.visible_parent).unwrap();
        }
        Ok(PrintJobSupport::controlled_pdf(None))
    }

    fn submit(&self, job: &PrintJob, _cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure> {
        *self.submitted_path.lock().unwrap() = Some(PathBuf::from(&job.path));
        *self.submitted_bytes.lock().unwrap() = Some(std::fs::read(&job.path).unwrap());
        Ok(())
    }
}

fn print_service(root: &TempDir, adapter: Arc<FakeAdapter>) -> PrintService {
    PrintService::new(
        root.path().join("print-queues"),
        root.path().join("journal"),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        adapter,
    )
}

fn caller() -> CallerIdentity {
    CallerIdentity::new("operator", "session")
}

fn params(files: Vec<PathBuf>) -> BatchPrintParams {
    BatchPrintParams {
        files,
        printer: "Office Printer".to_string(),
        paper: "A4".to_string(),
        orientation: "auto".to_string(),
        scaling: "fit".to_string(),
        color: "printer".to_string(),
        duplex: "printer".to_string(),
        copies: 1,
        collate: true,
        retries: 0,
        on_error: "pause".to_string(),
        timeout_minutes: 30,
    }
}

fn write_workbook(path: &Path, sheets: &[(&str, u32, Option<&str>)], extra: &[(&str, &[u8])]) {
    write_workbook_with_relationships(path, sheets, "", "", extra);
}

fn write_workbook_with_relationships(
    path: &Path,
    sheets: &[(&str, u32, Option<&str>)],
    workbook_extra: &str,
    extra_relationships: &str,
    extra: &[(&str, &[u8])],
) {
    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const REL: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
    let sheet_elements = sheets
        .iter()
        .enumerate()
        .map(|(index, (name, sheet_id, visibility))| {
            let state = visibility
                .map(|value| format!(r#" state="{value}""#))
                .unwrap_or_default();
            format!(
                r#"<sheet name="{name}" sheetId="{sheet_id}"{state} r:id="rId{}"/>"#,
                index + 1
            )
        })
        .collect::<String>();
    let relationship_elements = sheets
        .iter()
        .enumerate()
        .map(|(index, _)| {
            format!(
                r#"<Relationship Id="rId{}" Type="{REL}" Target="worksheets/sheet{}.xml"/>"#,
                index + 1,
                index + 1
            )
        })
        .collect::<String>();
    let workbook = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{sheet_elements}</sheets>{workbook_extra}</workbook>"#
    );
    let relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationship_elements}{extra_relationships}</Relationships>"#
    );
    let package_relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let file = std::fs::File::create(path).unwrap();
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
    for (index, _) in sheets.iter().enumerate() {
        writer
            .start_file(format!("xl/worksheets/sheet{}.xml", index + 1), options)
            .unwrap();
        writer
            .write_all(b"<worksheet><sheetData><v>PRIVATE CELL VALUE</v></sheetData></worksheet>")
            .unwrap();
    }
    for (name, bytes) in extra {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

fn descriptor(name: &str, visibility: WorksheetVisibility) -> WorksheetDescriptor {
    WorksheetDescriptor {
        workbook_index: 1,
        sheet_id: 17,
        relationship_id: "rId1".to_string(),
        relationship_type:
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet"
                .to_string(),
        part_path: "xl/worksheets/sheet1.xml".to_string(),
        name: name.to_string(),
        visibility,
    }
}

fn safe_workbook_facts() -> WorkbookSafetyFacts {
    WorkbookSafetyFacts::default()
}

#[test]
fn preview_expands_excel_worksheets() {
    let root = TempDir::new().unwrap();
    let first = root.path().join("first.pdf");
    let workbook = root.path().join("book.xlsx");
    let last = root.path().join("last.png");
    std::fs::write(&first, b"first").unwrap();
    write_workbook(
        &workbook,
        &[
            ("MEC", 17, None),
            ("Hidden", 44, Some("hidden")),
            ("VeryHidden", 45, Some("veryHidden")),
        ],
        &[],
    );
    std::fs::write(&last, b"last").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter);

    let plan = service
        .preview(
            &caller(),
            params(vec![first.clone(), workbook.clone(), last.clone()]),
            |_| {},
        )
        .unwrap();

    let rows = plan.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| {
                (
                    row["file"].as_str().unwrap(),
                    row["worksheet"]["name"].as_str(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("first.pdf", None),
            ("book.xlsx", Some("MEC")),
            ("book.xlsx", Some("Hidden")),
            ("book.xlsx", Some("VeryHidden")),
            ("last.png", None),
        ]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row["enabled"].as_bool().unwrap())
            .collect::<Vec<_>>(),
        vec![true, true, false, false, true]
    );
    assert_eq!(plan.meta["editor"]["reorderable"], true);
    for row in &rows[1..4] {
        assert_eq!(
            row["worksheet"]["relationshipType"],
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet"
        );
        assert_eq!(row["signature"]["sha256"].as_str().unwrap().len(), 64);
        assert!(
            row["signature"]["sha256"]
                .as_str()
                .unwrap()
                .chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
        );
        assert_eq!(row["workbookSafety"]["hasXlmMacroSheet"], false);
        assert_eq!(row["workbookSafety"]["hasWorkbookProtection"], false);
    }
    assert_eq!(rows[1]["signature"], rows[2]["signature"]);
    assert_eq!(rows[2]["signature"], rows[3]["signature"]);
    let encoded = serde_json::to_string(&plan).unwrap();
    assert!(!encoded.contains("PRIVATE CELL VALUE"));
}

#[cfg(unix)]
#[test]
fn workbook_preview_uses_a_capability_before_reading_the_visible_path() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new().unwrap();
    let selected_dir = root.path().join("selected");
    let pinned_dir = root.path().join("pinned");
    let outside_dir = root.path().join("outside");
    std::fs::create_dir_all(&selected_dir).unwrap();
    std::fs::create_dir_all(&outside_dir).unwrap();
    let visible = selected_dir.join("book.xlsx");
    let outside = outside_dir.join("book.xlsx");
    write_workbook(&visible, &[("Selected", 1, None)], &[]);
    write_workbook(&outside, &[("Outside", 2, None)], &[]);
    let service = print_service(
        &root,
        Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        )),
    );
    let mut swapped = false;

    let plan = service
        .preview(&caller(), params(vec![visible.clone()]), |_| {
            if !swapped {
                std::fs::rename(&selected_dir, &pinned_dir).unwrap();
                symlink(&outside_dir, &selected_dir).unwrap();
                swapped = true;
            }
        })
        .unwrap();

    assert_eq!(plan.steps.len(), 1);
    assert!(serde_json::to_string(&plan).unwrap().contains("Selected"));
    assert!(!serde_json::to_string(&plan).unwrap().contains("Outside"));
}

#[test]
fn preview_rejects_unsafe_workbook_facts_and_expanded_row_overflow() {
    const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const MICROSOFT_REL: &str = "http://schemas.microsoft.com/office/2006/relationships";
    let cases = [
        (
            "<workbookProtection lockStructure=\"1\"/>".to_string(),
            String::new(),
            Vec::new(),
            "warn.print.workbookUnsafeProtection",
        ),
        (
            String::new(),
            format!(
                r#"<Relationship Id="unsafe" Type="{MICROSOFT_REL}/xlMacrosheet" Target="macrosheets/sheet1.xml"/>"#
            ),
            vec![("xl/macrosheets/sheet1.xml", b"macrosheet".as_slice())],
            "warn.print.workbookUnsafeXlm",
        ),
        (
            String::new(),
            format!(
                r#"<Relationship Id="unsafe" Type="{OFFICE_REL}/externalLink" Target="externalLinks/externalLink1.xml"/>"#
            ),
            vec![(
                "xl/externalLinks/externalLink1.xml",
                b"external-link".as_slice(),
            )],
            "warn.print.workbookUnsafeExternalLinks",
        ),
        (
            String::new(),
            format!(
                r#"<Relationship Id="unsafe" Type="{OFFICE_REL}/connections" Target="connections.xml"/>"#
            ),
            vec![(
                "xl/connections.xml",
                b"<connections xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><connection/></connections>".as_slice(),
            )],
            "warn.print.workbookUnsafeConnections",
        ),
    ];
    for (index, (workbook_extra, relationships, extra, warning_key)) in
        cases.into_iter().enumerate()
    {
        let root = TempDir::new().unwrap();
        let workbook = root.path().join(format!("unsafe-{index}.xlsx"));
        write_workbook_with_relationships(
            &workbook,
            &[("MEC", 1, None)],
            &workbook_extra,
            &relationships,
            &extra,
        );
        let service = print_service(
            &root,
            Arc::new(FakeAdapter::with_printers(
                &["Office Printer"],
                Some("Office Printer"),
            )),
        );

        let plan = service
            .preview(&caller(), params(vec![workbook]), |_| {})
            .unwrap();

        assert!(plan.steps.is_empty());
        assert_eq!(plan.warnings.len(), 2);
        assert_eq!(plan.warnings[0].key, warning_key);
        assert_eq!(plan.warnings[1].key, "warn.print.noJobsSelected");
    }

    let root = TempDir::new().unwrap();
    let workbook = root.path().join("query.xlsx");
    let query_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="query" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
    write_workbook(
        &workbook,
        &[("MEC", 1, None)],
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", query_relationships),
            ("xl/queryTables/queryTable1.xml", b"query-table"),
        ],
    );
    let service = print_service(
        &root,
        Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        )),
    );
    let plan = service
        .preview(&caller(), params(vec![workbook]), |_| {})
        .unwrap();
    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.print.workbookUnsafeQueryTables");

    let root = TempDir::new().unwrap();
    let first = root.path().join("first.pdf");
    let workbook = root.path().join("too-many.xlsx");
    std::fs::write(&first, b"first").unwrap();
    let names = (1..=250)
        .map(|index| (format!("Sheet{index}"), index as u32))
        .collect::<Vec<_>>();
    let borrowed = names
        .iter()
        .map(|(name, sheet_id)| (name.as_str(), *sheet_id, None))
        .collect::<Vec<_>>();
    write_workbook(&workbook, &borrowed, &[]);
    let service = print_service(
        &root,
        Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        )),
    );
    let plan = service
        .preview(&caller(), params(vec![first, workbook]), |_| {})
        .unwrap();
    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.print.expandedOrderTooLarge");
}

#[test]
fn preview_rejects_workbook_discovery_errors_without_authorizing_rows() {
    let root = TempDir::new().unwrap();
    let malformed = root.path().join("malformed.xlsx");
    std::fs::write(&malformed, b"not an OOXML package").unwrap();
    let service = print_service(
        &root,
        Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        )),
    );

    let plan = service
        .preview(&caller(), params(vec![malformed]), |_| {})
        .unwrap();

    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.print.workbookDiscoveryFailed");

    let too_many = root.path().join("too-many-sheets.xlsx");
    let names = (1..=251)
        .map(|index| (format!("Sheet{index}"), index as u32))
        .collect::<Vec<_>>();
    let borrowed = names
        .iter()
        .map(|(name, sheet_id)| (name.as_str(), *sheet_id, None))
        .collect::<Vec<_>>();
    write_workbook(&too_many, &borrowed, &[]);
    let plan = service
        .preview(&caller(), params(vec![too_many]), |_| {})
        .unwrap();
    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.print.workbookTooManyWorksheets");
}

#[test]
fn expanded_row_limit_applies_after_workbook_first_and_workbook_middle_appends() {
    for (sheet_count, workbook_position) in [(250, 0), (249, 1)] {
        let root = TempDir::new().unwrap();
        let workbook = root.path().join("book.xlsx");
        let first = root.path().join("first.pdf");
        let trailing = root.path().join("trailing.png");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&trailing, b"trailing").unwrap();
        let names = (1..=sheet_count)
            .map(|index| (format!("Sheet{index}"), index as u32))
            .collect::<Vec<_>>();
        let borrowed = names
            .iter()
            .map(|(name, sheet_id)| (name.as_str(), *sheet_id, None))
            .collect::<Vec<_>>();
        write_workbook(&workbook, &borrowed, &[]);
        let files = if workbook_position == 0 {
            vec![workbook, trailing]
        } else {
            vec![first, workbook, trailing]
        };
        let adapter = Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        ));
        let service = print_service(&root, adapter.clone());

        let plan = service.preview(&caller(), params(files), |_| {}).unwrap();

        assert!(plan.steps.is_empty());
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].key, "warn.print.expandedOrderTooLarge");
        assert!(adapter.submitted.lock().unwrap().is_empty());
    }
}

#[test]
fn worksheet_revise_rejects_row_set_metadata_page_range_and_source_drift() {
    let root = TempDir::new().unwrap();
    let workbook = root.path().join("book.xlsx");
    write_workbook(
        &workbook,
        &[
            ("MEC", 17, None),
            ("Hidden", 44, Some("hidden")),
            ("VeryHidden", 45, Some("veryHidden")),
        ],
        &[],
    );
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter);
    let plan = service
        .preview(&caller(), params(vec![workbook.clone()]), |_| {})
        .unwrap();
    let rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();

    let error = service
        .revise(&caller(), &plan.plan_id, &rows[..2])
        .unwrap_err();
    assert_eq!(error.message, "print queue rows cannot be added or removed");

    let mut duplicate = rows.clone();
    duplicate[2] = duplicate[0].clone();
    let error = service
        .revise(&caller(), &plan.plan_id, &duplicate)
        .unwrap_err();
    assert_eq!(error.message, "invalid print row identity");

    let mut forged = rows.clone();
    forged[0]["_rowId"] = json!("forged-row");
    let error = service
        .revise(&caller(), &plan.plan_id, &forged)
        .unwrap_err();
    assert_eq!(error.message, "invalid print row identity");

    for (field, value) in [
        ("worksheet", json!({"name": "Forged"})),
        (
            "signature",
            json!({"size": 1, "mtimeNs": 2, "sha256": "a".repeat(64)}),
        ),
        ("workbookSafety", json!({"connectionCount": 1})),
    ] {
        let mut tampered = rows.clone();
        tampered[0][field] = value;
        let error = service
            .revise(&caller(), &plan.plan_id, &tampered)
            .unwrap_err();
        assert_eq!(error.message, "immutable print row metadata changed");
    }

    for invalid_range in ["1", "ALL"] {
        let mut invalid = rows.clone();
        invalid[0]["pageRange"] = json!(invalid_range);
        let error = service
            .revise(&caller(), &plan.plan_id, &invalid)
            .unwrap_err();
        assert_eq!(error.message, "error.print.worksheetPageRangeAllOnly");
    }

    let mut disabled = rows.clone();
    for row in &mut disabled {
        row["enabled"] = json!(false);
    }
    let error = service
        .revise(&caller(), &plan.plan_id, &disabled)
        .unwrap_err();
    assert_eq!(
        error.message,
        "print queue requires an enabled worksheet row"
    );

    for malformed_enabled in [None, Some(json!("true"))] {
        let mut edited = rows.clone();
        let hidden = edited
            .iter_mut()
            .find(|row| row["worksheetVisibility"] == "hidden")
            .unwrap()
            .as_object_mut()
            .unwrap();
        match malformed_enabled {
            Some(value) => {
                hidden.insert("enabled".to_string(), value);
            }
            None => {
                hidden.remove("enabled");
            }
        }
        let revised = service.revise(&caller(), &plan.plan_id, &edited).unwrap();
        let revised_hidden = revised.meta["editor"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["worksheetVisibility"] == "hidden")
            .unwrap();
        assert_eq!(revised_hidden["enabled"], false);
    }

    write_workbook(&workbook, &[("Changed", 99, None)], &[]);
    let error = service.revise(&caller(), &plan.plan_id, &rows).unwrap_err();
    assert_eq!(error.message, "error.print.workbookChanged");
}

#[test]
fn worksheet_revise_preserves_complete_reordered_row_set_and_identity() {
    let root = TempDir::new().unwrap();
    let workbook = root.path().join("book.xlsm");
    write_workbook(
        &workbook,
        &[
            ("MEC", 17, None),
            ("Hidden", 44, Some("hidden")),
            ("VeryHidden", 45, Some("veryHidden")),
        ],
        &[("xl/vbaProject.bin", b"macro bytes")],
    );
    let service = print_service(
        &root,
        Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        )),
    );
    let plan = service
        .preview(&caller(), params(vec![workbook]), |_| {})
        .unwrap();
    let original = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    let original_ids = original
        .iter()
        .map(|row| row["_rowId"].clone())
        .collect::<Vec<_>>();
    let mut reordered = vec![
        original[1].clone(),
        original[2].clone(),
        original[0].clone(),
    ];
    reordered[0]["enabled"] = json!(true);

    let revised = service
        .revise(&caller(), &plan.plan_id, &reordered)
        .unwrap();

    let revised_rows = revised.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(
        revised_rows
            .iter()
            .map(|row| row["_rowId"].clone())
            .collect::<Vec<_>>(),
        vec![
            original_ids[1].clone(),
            original_ids[2].clone(),
            original_ids[0].clone(),
        ]
    );
    assert_eq!(
        revised.meta["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["worksheet"]["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["Hidden", "MEC"]
    );
    assert_eq!(revised_rows[0]["worksheet"], original[1]["worksheet"]);
    assert_eq!(revised_rows[0]["signature"], original[1]["signature"]);
    assert_eq!(
        revised_rows[0]["workbookSafety"],
        original[1]["workbookSafety"]
    );
}

#[test]
fn worksheet_print_queue_persists_and_resumes_typed_metadata_without_submission() {
    let root = TempDir::new().unwrap();
    let workbook = root.path().join("book.xlsx");
    write_workbook(&workbook, &[("MEC", 17, None)], &[]);
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![workbook]), |_| {})
        .unwrap();
    let expected_worksheet = plan.meta["jobs"][0]["worksheet"].clone();
    let expected_safety = plan.meta["jobs"][0]["workbookSafety"].clone();
    let expected_signature = plan.meta["jobs"][0]["signature"].clone();

    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();

    assert_eq!((result.applied, result.failed), (0, 1));
    assert!(adapter.submitted.lock().unwrap().is_empty());
    assert_eq!(result.errors[0].key, "error.print.excelUnavailable");
    assert_eq!(result.errors[0].subject, "book.xlsx");
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            root.path()
                .join("print-queues")
                .join(format!("{queue_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(saved["jobs"][0]["worksheet"], expected_worksheet);
    assert_eq!(saved["jobs"][0]["workbookSafety"], expected_safety);
    assert_eq!(saved["jobs"][0]["signature"], expected_signature);

    let restarted = print_service(&root, adapter);
    let resumed = restarted.resume(&caller(), queue_id).unwrap();
    assert_eq!(
        resumed.meta["editor"]["rows"][0]["worksheet"],
        expected_worksheet
    );
    assert_eq!(
        resumed.meta["editor"]["rows"][0]["workbookSafety"],
        expected_safety
    );
    assert_eq!(
        resumed.meta["editor"]["rows"][0]["signature"],
        expected_signature
    );
}

#[test]
fn same_workbook_rows_with_inconsistent_signatures_fail_before_authorization() {
    let root = TempDir::new().unwrap();
    let workbook = root.path().join("book.xlsx");
    write_workbook(&workbook, &[("MEC", 17, None), ("CIV", 18, None)], &[]);
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![workbook]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let queue_path = root
        .path()
        .join("print-queues")
        .join(format!("{queue_id}.json"));
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    saved["jobs"][1]["signature"]["sha256"] = json!("b".repeat(64));
    std::fs::write(&queue_path, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();

    let restarted = print_service(&root, adapter.clone());
    let error = match restarted.resume(&caller(), queue_id) {
        Err(error) => error,
        Ok(resumed) => {
            let rows = resumed.meta["editor"]["rows"].as_array().unwrap().clone();
            restarted
                .revise(&caller(), &resumed.plan_id, &rows)
                .unwrap_err()
        }
    };

    assert_eq!(error.message, "error.print.workbookChanged");
    assert!(adapter.submitted.lock().unwrap().is_empty());
}

#[test]
fn worksheet_resume_revalidates_source_before_caching_a_plan() {
    let root = TempDir::new().unwrap();
    let workbook = root.path().join("book.xlsx");
    write_workbook(&workbook, &[("MEC", 17, None)], &[]);
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![workbook.clone()]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    write_workbook(&workbook, &[("Changed", 99, None)], &[]);

    let restarted = print_service(&root, adapter);
    let error = restarted.resume(&caller(), queue_id).unwrap_err();

    assert_eq!(error.message, "error.print.workbookChanged");
}

#[test]
fn reordered_resumed_worksheet_jobs_persist_across_another_pause() {
    let root = TempDir::new().unwrap();
    let workbook = root.path().join("book.xlsx");
    write_workbook(
        &workbook,
        &[("MEC", 17, None), ("CIV", 18, None), ("ELE", 19, None)],
        &[],
    );
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![workbook]), |_| {})
        .unwrap();
    let first = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = first.meta["queueId"].as_str().unwrap();

    let resumed_service = print_service(&root, adapter.clone());
    let resumed = resumed_service.resume(&caller(), queue_id).unwrap();
    let rows = resumed.meta["editor"]["rows"].as_array().unwrap();
    let reordered = vec![rows[2].clone(), rows[1].clone(), rows[0].clone()];
    let revised = resumed_service
        .revise(&caller(), &resumed.plan_id, &reordered)
        .unwrap();
    resumed_service
        .apply(&revised.plan_id, &caller(), |_| {})
        .unwrap();

    let restarted = print_service(&root, adapter);
    let resumed_again = restarted.resume(&caller(), queue_id).unwrap();
    assert_eq!(
        resumed_again.meta["editor"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["worksheetName"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["ELE", "CIV", "MEC"]
    );
}

#[test]
fn worksheet_apply_rejects_source_drift_before_any_submission() {
    let root = TempDir::new().unwrap();
    let workbook = root.path().join("book.xlsx");
    write_workbook(&workbook, &[("MEC", 17, None)], &[]);
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![workbook.clone()]), |_| {})
        .unwrap();
    write_workbook(&workbook, &[("Changed", 99, None)], &[]);

    let error = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap_err();

    assert!(adapter.submitted.lock().unwrap().is_empty());
    assert_eq!(error.message, "error.print.workbookChanged");
}

#[test]
fn worksheet_job_validation_requires_typed_sha_safety_and_all_only_range() {
    let valid = PrintJob {
        job_id: "worksheet-c1".to_string(),
        path: "/approved/book.xlsx".to_string(),
        printer: "Office Printer".to_string(),
        verb: "docflow".to_string(),
        format: "XLSX".to_string(),
        signature: FileSignature {
            size: 42,
            mtime_ns: 99,
            sha256: Some("a".repeat(64)),
        },
        worksheet: Some(descriptor("MEC", WorksheetVisibility::Visible)),
        workbook_safety: Some(safe_workbook_facts()),
        ..PrintJob::default()
    };
    validate_job(&valid).unwrap();

    let mut missing_sha = valid.clone();
    missing_sha.signature.sha256 = None;
    assert!(validate_job(&missing_sha).is_err());
    let mut uppercase_sha = valid.clone();
    uppercase_sha.signature.sha256 = Some("A".repeat(64));
    assert!(validate_job(&uppercase_sha).is_err());
    let mut missing_safety = valid.clone();
    missing_safety.workbook_safety = None;
    assert!(validate_job(&missing_safety).is_err());
    let mut unsafe_job = valid.clone();
    unsafe_job
        .workbook_safety
        .as_mut()
        .unwrap()
        .external_link_count = 1;
    assert!(validate_job(&unsafe_job).is_err());
    let mut ranged = valid;
    ranged.page_range = "1".to_string();
    assert_eq!(
        validate_job(&ranged).unwrap_err().message,
        "error.print.worksheetPageRangeAllOnly"
    );
}

#[test]
fn legacy_file_job_defaults_optional_worksheet_metadata_and_sha256() {
    let legacy: PrintJob = serde_json::from_value(json!({
        "jobId": "legacy-c1",
        "path": "/approved/legacy.pdf",
        "printer": "Office Printer",
        "verb": "docflow",
        "format": "PDF",
        "signature": {"size": 12, "mtimeNs": 34}
    }))
    .unwrap();

    assert_eq!(legacy.signature.size, 12);
    assert_eq!(legacy.signature.mtime_ns, 34);
    assert_eq!(legacy.signature.sha256, None);
    assert_eq!(legacy.worksheet, None);
    assert_eq!(legacy.workbook_safety, None);
    assert!(validate_job(&legacy).is_ok());
    assert_eq!(
        serde_json::to_value(&legacy).unwrap()["signature"],
        json!({"size": 12, "mtimeNs": 34})
    );
}

#[test]
fn persisted_job_metadata_distinguishes_legacy_absence_from_null_malformed_and_orphans() {
    let legacy = json!({
        "jobId": "legacy-c1",
        "path": "/approved/legacy.pdf",
        "printer": "Office Printer",
        "verb": "docflow",
        "format": "PDF",
        "signature": {"size": 12, "mtimeNs": 34}
    });
    let parsed_legacy: PrintJob = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(parsed_legacy.worksheet, None);
    assert_eq!(parsed_legacy.workbook_safety, None);
    assert_eq!(parsed_legacy.signature.sha256, None);

    let worksheet = serde_json::to_value(descriptor("MEC", WorksheetVisibility::Visible)).unwrap();
    let safety = serde_json::to_value(safe_workbook_facts()).unwrap();
    let valid_worksheet = json!({
        "jobId": "worksheet-c1",
        "path": "/approved/book.xlsx",
        "printer": "Office Printer",
        "verb": "docflow",
        "format": "XLSX",
        "signature": {"size": 12, "mtimeNs": 34, "sha256": "a".repeat(64)},
        "worksheet": worksheet,
        "workbookSafety": safety
    });
    let parsed_worksheet: PrintJob = serde_json::from_value(valid_worksheet.clone()).unwrap();
    assert!(parsed_worksheet.worksheet.is_some());

    let mut explicit_null = legacy.clone();
    explicit_null["worksheet"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<PrintJob>(explicit_null).is_err());

    let mut malformed = valid_worksheet.clone();
    malformed["worksheet"] = json!({"name": "MEC"});
    assert!(serde_json::from_value::<PrintJob>(malformed).is_err());

    let mut null_safety = legacy.clone();
    null_safety["workbookSafety"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<PrintJob>(null_safety).is_err());

    let mut worksheet_without_safety_or_sha = legacy.clone();
    worksheet_without_safety_or_sha["worksheet"] = valid_worksheet["worksheet"].clone();
    assert!(serde_json::from_value::<PrintJob>(worksheet_without_safety_or_sha).is_err());

    let mut safety_without_worksheet = legacy.clone();
    safety_without_worksheet["workbookSafety"] = valid_worksheet["workbookSafety"].clone();
    assert!(serde_json::from_value::<PrintJob>(safety_without_worksheet).is_err());

    let mut sha_without_worksheet = legacy;
    sha_without_worksheet["signature"]["sha256"] = json!("a".repeat(64));
    assert!(serde_json::from_value::<PrintJob>(sha_without_worksheet).is_err());

    for missing in ["signature", "workbookSafety"] {
        let mut orphan = valid_worksheet.clone();
        orphan.as_object_mut().unwrap().remove(missing);
        assert!(
            serde_json::from_value::<PrintJob>(orphan).is_err(),
            "worksheet metadata without {missing} must fail"
        );
    }
}

#[cfg(unix)]
#[test]
fn resume_authority_requires_original_source_and_root_capability_identity() {
    for replace_parent in [false, true] {
        let root = TempDir::new().unwrap();
        let selected_parent = root.path().join("selected");
        std::fs::create_dir_all(&selected_parent).unwrap();
        let approved = selected_parent.join("approved.pdf");
        std::fs::write(&approved, b"SIGNATURE COMPATIBLE BYTES").unwrap();
        let adapter = Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        ));
        adapter.push(Err(PrintFailure::failed("pause queue")));
        let service = print_service(&root, adapter.clone());
        let plan = service
            .preview(&caller(), params(vec![approved.clone()]), |_| {})
            .unwrap();
        let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
        let queue_id = result.meta["queueId"].as_str().unwrap();
        adapter.submitted.lock().unwrap().clear();
        let queue_path = root
            .path()
            .join("print-queues")
            .join(format!("{queue_id}.json"));
        let mut saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();

        let replacement = if replace_parent {
            let pinned_parent = root.path().join("pinned-selected");
            std::fs::rename(&selected_parent, &pinned_parent).unwrap();
            std::fs::create_dir_all(&selected_parent).unwrap();
            let replacement = selected_parent.join("approved.pdf");
            std::fs::write(&replacement, b"SIGNATURE COMPATIBLE BYTES").unwrap();
            replacement
        } else {
            let replacement = selected_parent.join("tampered.pdf");
            std::fs::write(&replacement, b"SIGNATURE COMPATIBLE BYTES").unwrap();
            saved["jobs"][0]["path"] = json!(replacement);
            replacement
        };
        saved["jobs"][0]["signature"] =
            serde_json::to_value(file_signature(&replacement).unwrap()).unwrap();
        std::fs::write(&queue_path, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();

        let restarted = print_service(&root, adapter.clone());
        let error = restarted.resume(&caller(), queue_id).unwrap_err();

        assert_eq!(error.message, "error.print.queueSourceReselectionRequired");
        assert!(adapter.submitted.lock().unwrap().is_empty());
        assert!(!saved["grantRootIdentities"].as_array().unwrap().is_empty());
        assert!(!saved["sourceIdentities"].as_array().unwrap().is_empty());
    }
}

#[test]
fn resume_rejects_coherently_rewritten_job_and_source_identity_paths() {
    let root = TempDir::new().unwrap();
    let selected_parent = root.path().join("selected");
    std::fs::create_dir_all(&selected_parent).unwrap();
    let approved = selected_parent.join("approved.pdf");
    let replacement = selected_parent.join("replacement.pdf");
    std::fs::write(&approved, b"ORIGINAL APPROVED PDF").unwrap();
    std::fs::write(&replacement, b"UNAPPROVED REPLACEMENT").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("pause queue")));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![approved]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    adapter.submitted.lock().unwrap().clear();
    let inspected_before = adapter.inspected.load(Ordering::SeqCst);
    let queue_path = root
        .path()
        .join("print-queues")
        .join(format!("{queue_id}.json"));
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    let original_canonical = saved["sourceIdentities"][0]["canonicalPath"].clone();
    let original_identity = saved["sourceIdentities"][0]["identity"].clone();
    saved["jobs"][0]["path"] = json!(replacement);
    saved["sourceIdentities"][0]["path"] = saved["jobs"][0]["path"].clone();
    assert_eq!(
        saved["sourceIdentities"][0]["canonicalPath"],
        original_canonical
    );
    assert_eq!(saved["sourceIdentities"][0]["identity"], original_identity);
    std::fs::write(&queue_path, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();
    let queue_before = std::fs::read(&queue_path).unwrap();
    drop(service);
    let restarted = print_service(&root, adapter.clone());

    let error = restarted.resume(&caller(), queue_id).unwrap_err();

    assert_eq!(error.message, "error.print.queueSourceReselectionRequired");
    assert_eq!(std::fs::read(&queue_path).unwrap(), queue_before);
    assert_eq!(adapter.inspected.load(Ordering::SeqCst), inspected_before);
    assert!(adapter.submitted.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn resumed_plan_rejects_same_signature_source_identity_swap_before_submission() {
    let root = TempDir::new().unwrap();
    let selected_parent = root.path().join("selected");
    std::fs::create_dir_all(&selected_parent).unwrap();
    let approved = selected_parent.join("approved.pdf");
    let original_backup = selected_parent.join("original.pdf");
    std::fs::write(&approved, b"SIGNATURE COMPATIBLE BYTES").unwrap();
    let original_modified = std::fs::metadata(&approved).unwrap().modified().unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("pause queue")));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![approved.clone()]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    adapter.submitted.lock().unwrap().clear();
    drop(service);

    let restarted = print_service(&root, adapter.clone());
    let resumed = restarted.resume(&caller(), queue_id).unwrap();
    std::fs::rename(&approved, &original_backup).unwrap();
    std::fs::write(&approved, b"SIGNATURE COMPATIBLE BYTES").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&approved)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();
    assert_eq!(
        file_signature(&approved).unwrap(),
        file_signature(&original_backup).unwrap()
    );

    let error = restarted
        .apply(&resumed.plan_id, &caller(), |_| {})
        .unwrap_err();

    assert_eq!(error.message, "error.print.queueSourceReselectionRequired");
    assert!(adapter.submitted.lock().unwrap().is_empty());
}

#[test]
fn concurrent_resume_reserves_queue_before_loading_and_caching() {
    let root = TempDir::new().unwrap();
    let approved = root.path().join("approved.pdf");
    std::fs::write(&approved, b"approved").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("pause queue")));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![approved]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap().to_string();
    drop(service);

    let gate: SubmitGate = Arc::new((Mutex::new((false, false)), Condvar::new()));
    adapter.set_inventory_gate(gate.clone());
    let restarted = Arc::new(print_service(&root, adapter));
    let worker_service = restarted.clone();
    let worker_queue_id = queue_id.clone();
    let first = thread::spawn(move || worker_service.resume(&caller(), &worker_queue_id));
    let (state, changed) = &*gate;
    let mut state = state.lock().unwrap();
    while !state.0 {
        state = changed.wait(state).unwrap();
    }

    let second = restarted.resume(&caller(), &queue_id);
    state.1 = true;
    changed.notify_all();
    drop(state);
    first.join().unwrap().unwrap();
    let second_error = second.unwrap_err();
    assert_eq!(second_error.code, "operation_in_progress");
}

#[cfg(unix)]
#[test]
fn resume_save_failure_does_not_publish_plan_or_retain_grants() {
    use std::os::unix::fs::PermissionsExt;

    let root = TempDir::new().unwrap();
    let approved = root.path().join("approved.pdf");
    std::fs::write(&approved, b"approved").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("pause queue")));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![approved]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap().to_string();
    drop(service);

    let restarted = print_service(&root, adapter);
    let queue_root = root.path().join("print-queues");
    let original_permissions = std::fs::metadata(&queue_root).unwrap().permissions();
    std::fs::set_permissions(&queue_root, std::fs::Permissions::from_mode(0o500)).unwrap();
    let error = restarted.resume(&caller(), &queue_id).unwrap_err();
    std::fs::set_permissions(&queue_root, original_permissions).unwrap();

    assert_eq!(error.message, "print queue could not be saved");
    assert_eq!(restarted.active_grant_count_for_testing(), 0);
    assert_eq!(restarted.active_plan_count_for_testing(), 0);
    restarted.discard(&caller(), &queue_id).unwrap();
}

#[test]
fn legacy_queue_without_persisted_source_identity_requires_explicit_reselection() {
    let root = TempDir::new().unwrap();
    let approved = root.path().join("approved.pdf");
    std::fs::write(&approved, b"approved").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("pause queue")));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![approved]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let queue_path = root
        .path()
        .join("print-queues")
        .join(format!("{queue_id}.json"));
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("grantRootIdentities");
    legacy.as_object_mut().unwrap().remove("sourceIdentities");
    std::fs::write(&queue_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
    drop(service);

    let restarted = print_service(&root, adapter);
    let error = restarted.resume(&caller(), queue_id).unwrap_err();

    assert_eq!(error.message, "error.print.queueSourceReselectionRequired");
    assert_eq!(restarted.active_grant_count_for_testing(), 0);
    assert_eq!(restarted.active_plan_count_for_testing(), 0);
}

#[test]
fn failed_resume_revalidation_releases_provisional_grants_and_root_handles() {
    for corrupt_descriptor in [false, true] {
        let root = TempDir::new().unwrap();
        let selected_parent = TempDir::new().unwrap();
        let workbook = selected_parent.path().join("book.pdf");
        write_workbook(&workbook, &[("MEC", 17, None)], &[]);
        let adapter = Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        ));
        adapter.push(Err(PrintFailure::failed("pause queue")));
        let service = print_service(&root, adapter.clone());
        let plan = service
            .preview(&caller(), params(vec![workbook.clone()]), |_| {})
            .unwrap();
        let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
        let queue_id = result.meta["queueId"].as_str().unwrap();
        let queue_path = root
            .path()
            .join("print-queues")
            .join(format!("{queue_id}.json"));
        let mut saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
        let mut source_file = std::fs::File::open(&workbook).unwrap();
        saved["jobs"][0]["format"] = json!("XLSX");
        saved["jobs"][0]["signature"] =
            serde_json::to_value(file_signature_from_open_file(&mut source_file, true).unwrap())
                .unwrap();
        saved["jobs"][0]["worksheet"] =
            serde_json::to_value(descriptor("MEC", WorksheetVisibility::Visible)).unwrap();
        saved["jobs"][0]["workbookSafety"] = serde_json::to_value(safe_workbook_facts()).unwrap();
        if corrupt_descriptor {
            saved["jobs"][0]["worksheet"]["name"] = json!("Forged");
        }
        std::fs::write(&queue_path, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();
        if !corrupt_descriptor {
            write_workbook(&workbook, &[("Changed", 99, None)], &[]);
        }
        drop(service);
        let restarted = print_service(&root, adapter);
        let grants_before = restarted.active_grant_count_for_testing();

        let error = restarted.resume(&caller(), queue_id).unwrap_err();

        assert_eq!(
            error.message,
            if corrupt_descriptor {
                "error.print.worksheetChanged"
            } else {
                "error.print.workbookChanged"
            }
        );
        assert_eq!(
            restarted.active_grant_count_for_testing(),
            grants_before,
            "failed resume retained a capability grant"
        );
        #[cfg(not(windows))]
        {
            let renamed_parent = selected_parent.path().with_extension("renamed-selected");
            std::fs::rename(selected_parent.path(), &renamed_parent)
                .expect("failed resume retained an open root capability handle");
            std::fs::rename(renamed_parent, selected_parent.path()).unwrap();
        }
    }
}

#[cfg(unix)]
#[test]
fn failed_resume_drift_grant_and_cache_revalidation_leave_queue_bytes_unchanged() {
    let arm_interrupted = |queue_path: &Path| {
        let mut saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(queue_path).unwrap()).unwrap();
        saved["sessionId"] = json!("session");
        saved["status"] = json!("running");
        saved["currentJobId"] = saved["jobs"][0]["jobId"].clone();
        saved["jobs"][0]["status"] = json!("printing");
        std::fs::write(queue_path, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();
        std::fs::read(queue_path).unwrap()
    };
    let fresh_session = CallerIdentity::new("operator", "fresh-session");

    let drift_root = TempDir::new().unwrap();
    let drift_workbook = drift_root.path().join("drift.xlsx");
    write_workbook(&drift_workbook, &[("MEC", 17, None)], &[]);
    let drift_adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let drift_service = print_service(&drift_root, drift_adapter.clone());
    let drift_plan = drift_service
        .preview(&caller(), params(vec![drift_workbook.clone()]), |_| {})
        .unwrap();
    let drift_result = drift_service
        .apply(&drift_plan.plan_id, &caller(), |_| {})
        .unwrap();
    let drift_queue_id = drift_result.meta["queueId"].as_str().unwrap();
    let drift_queue_path = drift_root
        .path()
        .join("print-queues")
        .join(format!("{drift_queue_id}.json"));
    let drift_before = arm_interrupted(&drift_queue_path);
    write_workbook(&drift_workbook, &[("Changed", 99, None)], &[]);

    let drift_error = print_service(&drift_root, drift_adapter)
        .resume(&fresh_session, drift_queue_id)
        .unwrap_err();

    assert_eq!(drift_error.message, "error.print.workbookChanged");
    assert_eq!(std::fs::read(&drift_queue_path).unwrap(), drift_before);

    let cache_root = TempDir::new().unwrap();
    let cache_workbook = cache_root.path().join("cache.xlsx");
    write_workbook(&cache_workbook, &[("MEC", 17, None)], &[]);
    let cache_adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let cache_service = print_service(&cache_root, cache_adapter.clone());
    let cache_plan = cache_service
        .preview(&caller(), params(vec![cache_workbook]), |_| {})
        .unwrap();
    let cache_result = cache_service
        .apply(&cache_plan.plan_id, &caller(), |_| {})
        .unwrap();
    let cache_queue_id = cache_result.meta["queueId"].as_str().unwrap();
    let cache_queue_path = cache_root
        .path()
        .join("print-queues")
        .join(format!("{cache_queue_id}.json"));
    arm_interrupted(&cache_queue_path);
    let mut cache_saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&cache_queue_path).unwrap()).unwrap();
    cache_saved["jobs"][0]["worksheet"]["name"] = json!("Forged");
    std::fs::write(
        &cache_queue_path,
        serde_json::to_vec_pretty(&cache_saved).unwrap(),
    )
    .unwrap();
    let cache_before = std::fs::read(&cache_queue_path).unwrap();

    let cache_error = print_service(&cache_root, cache_adapter)
        .resume(&fresh_session, cache_queue_id)
        .unwrap_err();

    assert_eq!(cache_error.message, "error.print.worksheetChanged");
    assert_eq!(std::fs::read(&cache_queue_path).unwrap(), cache_before);

    let grant_root = TempDir::new().unwrap();
    let selected_parent = grant_root.path().join("selected");
    let pinned_parent = grant_root.path().join("pinned-selected");
    std::fs::create_dir_all(&selected_parent).unwrap();
    let granted_file = selected_parent.join("one.pdf");
    std::fs::write(&granted_file, b"one").unwrap();
    let grant_adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    grant_adapter.push(Err(PrintFailure::failed("pause queue")));
    let grant_service = print_service(&grant_root, grant_adapter.clone());
    let grant_plan = grant_service
        .preview(&caller(), params(vec![granted_file.clone()]), |_| {})
        .unwrap();
    let grant_result = grant_service
        .apply(&grant_plan.plan_id, &caller(), |_| {})
        .unwrap();
    grant_adapter.submitted.lock().unwrap().clear();
    let grant_queue_id = grant_result.meta["queueId"].as_str().unwrap();
    let grant_queue_path = grant_root
        .path()
        .join("print-queues")
        .join(format!("{grant_queue_id}.json"));
    let grant_before = arm_interrupted(&grant_queue_path);
    std::fs::rename(&selected_parent, &pinned_parent).unwrap();
    std::fs::create_dir_all(&selected_parent).unwrap();
    std::fs::write(&granted_file, b"one").unwrap();

    let grant_error = print_service(&grant_root, grant_adapter.clone())
        .resume(&fresh_session, grant_queue_id)
        .unwrap_err();

    assert_eq!(
        grant_error.message,
        "error.print.queueSourceReselectionRequired"
    );
    assert_eq!(std::fs::read(&grant_queue_path).unwrap(), grant_before);
    assert!(grant_adapter.submitted.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn apply_prints_the_pinned_file_bytes_after_the_visible_parent_is_swapped() {
    let root = TempDir::new().unwrap();
    let visible_parent = root.path().join("approved");
    let pinned_parent = root.path().join("pinned-approved");
    let outside_parent = root.path().join("outside");
    std::fs::create_dir_all(&visible_parent).unwrap();
    std::fs::create_dir_all(&outside_parent).unwrap();
    let approved = visible_parent.join("approved.pdf");
    std::fs::write(&approved, b"ORIGINAL APPROVED PDF").unwrap();
    std::fs::write(
        outside_parent.join("approved.pdf"),
        b"OUTSIDE REPLACEMENT PDF",
    )
    .unwrap();
    let adapter = Arc::new(ParentSwapAdapter {
        visible_parent: visible_parent.clone(),
        pinned_parent: pinned_parent.clone(),
        outside_parent: outside_parent.clone(),
        ..ParentSwapAdapter::default()
    });
    let service = PrintService::new(
        root.path().join("print-queues"),
        root.path().join("journal"),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        adapter.clone(),
    );
    let plan = service
        .preview(&caller(), params(vec![approved.clone()]), |_| {})
        .unwrap();
    adapter.swap_armed.store(true, Ordering::SeqCst);

    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();

    assert_eq!(
        (result.applied, result.failed),
        (1, 0),
        "{:?}",
        result.errors
    );
    assert_eq!(
        adapter.submitted_bytes.lock().unwrap().as_deref(),
        Some(b"ORIGINAL APPROVED PDF".as_slice())
    );
    let submitted_path = adapter.submitted_path.lock().unwrap().clone().unwrap();
    assert_ne!(submitted_path, approved);
    assert_eq!(
        submitted_path.extension().and_then(|value| value.to_str()),
        Some("pdf")
    );
    assert_eq!(
        std::fs::read(outside_parent.join("approved.pdf")).unwrap(),
        b"OUTSIDE REPLACEMENT PDF"
    );
    assert_eq!(
        std::fs::read(pinned_parent.join("approved.pdf")).unwrap(),
        b"ORIGINAL APPROVED PDF"
    );
}

#[test]
fn preview_preserves_order_deduplicates_collates_and_maps_per_file_settings() {
    let root = TempDir::new().unwrap();
    let first = root.path().join("first.pdf");
    let second = root.path().join("second.docx");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer", "Backup Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter);
    let mut values = params(vec![first.clone(), second.clone(), first.clone()]);
    values.copies = 2;

    let plan = service.preview(&caller(), values, |_| {}).unwrap();

    assert_eq!(
        plan.steps
            .iter()
            .filter_map(|step| step.src.as_deref())
            .collect::<Vec<_>>(),
        vec![first.to_str().unwrap(), second.to_str().unwrap()]
    );
    let jobs = plan.meta["jobs"].as_array().unwrap();
    assert_eq!(
        jobs.iter()
            .map(|job| job["path"].clone())
            .collect::<Vec<_>>(),
        vec![json!(first), json!(second), json!(first), json!(second)]
    );
    assert_eq!(plan.meta["fileCount"], 2);
    assert_eq!(plan.meta["jobCount"], 4);
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.key == "warn.print.settingsLimited")
    );
}

#[test]
fn revise_preserves_row_identity_and_applies_per_file_selection_and_settings() {
    let root = TempDir::new().unwrap();
    let first = root.path().join("first.pdf");
    let second = root.path().join("second.pdf");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer", "Backup Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter);
    let mut values = params(vec![first.clone(), second.clone()]);
    values.copies = 2;
    let plan = service.preview(&caller(), values, |_| {}).unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["enabled"] = json!(false);
    rows[1]["printer"] = json!("Backup Printer");
    rows[1]["paper"] = json!("A3");
    rows[1]["orientation"] = json!("landscape");
    rows[1]["color"] = json!("grayscale");

    let revised = service.revise(&caller(), &plan.plan_id, &rows).unwrap();

    assert_eq!(revised.steps.len(), 1);
    assert_eq!(revised.steps[0].src.as_deref(), second.to_str());
    assert_eq!(revised.meta["jobs"].as_array().unwrap().len(), 2);
    for job in revised.meta["jobs"].as_array().unwrap() {
        assert_eq!(job["printer"], "Backup Printer");
        assert_eq!(job["paper"], "A3");
        assert_eq!(job["orientation"], "landscape");
        assert_eq!(job["color"], "grayscale");
    }
}

#[test]
fn preview_defaults_page_range_to_all() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.set_pdf_page_count(3);
    let service = print_service(&root, adapter);

    let plan = service
        .preview(&caller(), params(vec![file]), |_| {})
        .unwrap();

    assert_eq!(plan.meta["editor"]["rows"][0]["pageRange"], "all");
    assert_eq!(plan.meta["jobs"][0]["pageRange"], "all");
    assert_eq!(plan.steps[0].meta["pageRange"], "all");
    assert!(
        plan.meta["editor"]["columns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|column| column["key"] == "pageRange" && column["kind"] == "text")
    );
}

#[test]
fn revise_canonicalizes_per_file_page_ranges() {
    let root = TempDir::new().unwrap();
    let first = root.path().join("first.pdf");
    let second = root.path().join("second.pdf");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.set_pdf_page_count(10);
    let service = print_service(&root, adapter);
    let plan = service
        .preview(
            &caller(),
            params(vec![first.clone(), second.clone()]),
            |_| {},
        )
        .unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["pageRange"] = json!(" 3, 1-2,2 ");
    rows[1]["pageRange"] = json!("7, 5-6");

    let revised = service.revise(&caller(), &plan.plan_id, &rows).unwrap();

    assert_eq!(
        revised
            .steps
            .iter()
            .map(|step| (
                step.src.as_deref().unwrap(),
                step.meta["pageRange"].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        vec![
            (first.to_str().unwrap(), "1-3"),
            (second.to_str().unwrap(), "5-7"),
        ]
    );
    assert_eq!(revised.meta["editor"]["rows"][0]["pageRange"], "1-3");
    assert_eq!(revised.meta["editor"]["rows"][1]["pageRange"], "5-7");
    assert_eq!(revised.meta["jobs"][0]["pageRange"], "1-3");
    assert_eq!(revised.meta["jobs"][1]["pageRange"], "5-7");
}

#[test]
fn out_of_bounds_pdf_revision_leaves_the_prior_plan_usable() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.set_pdf_page_count(2);
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![file]), |_| {})
        .unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["pageRange"] = json!("3");

    let error = service.revise(&caller(), &plan.plan_id, &rows).unwrap_err();

    assert_eq!(error.message, "error.print.pageRangeBounds");
    assert_eq!(
        service
            .apply(&plan.plan_id, &caller(), |_| {})
            .unwrap()
            .applied,
        1
    );
    assert_eq!(adapter.submitted.lock().unwrap()[0].page_range, "all");
}

#[test]
fn approved_pdf_range_is_rechecked_against_page_count_before_submission() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.set_pdf_page_count(3);
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![file]), |_| {})
        .unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["pageRange"] = json!("3");
    let revised = service.revise(&caller(), &plan.plan_id, &rows).unwrap();
    adapter.set_pdf_page_count(2);

    let result = service.apply(&revised.plan_id, &caller(), |_| {}).unwrap();

    assert_eq!((result.applied, result.failed), (0, 1));
    assert!(adapter.submitted.lock().unwrap().is_empty());
}

#[test]
fn image_page_one_canonicalizes_to_all_and_other_pages_fail() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.png");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![file]), |_| {})
        .unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["pageRange"] = json!("1");

    let revised = service.revise(&caller(), &plan.plan_id, &rows).unwrap();

    assert_eq!(revised.meta["editor"]["rows"][0]["pageRange"], "all");
    assert_eq!(revised.meta["jobs"][0]["pageRange"], "all");
    let mut invalid = revised.meta["editor"]["rows"].as_array().unwrap().clone();
    invalid[0]["pageRange"] = json!("2");
    let error = service
        .revise(&caller(), &revised.plan_id, &invalid)
        .unwrap_err();
    assert_eq!(error.message, "error.print.pageRangeImageUnsupported");
    assert!(adapter.submitted.lock().unwrap().is_empty());
}

#[test]
fn native_application_numeric_page_range_fails_before_apply() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.docx");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![file]), |_| {})
        .unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["pageRange"] = json!("1");

    let error = service.revise(&caller(), &plan.plan_id, &rows).unwrap_err();

    assert_eq!(error.message, "error.print.pageRangeNativeUnsupported");
    assert!(adapter.submitted.lock().unwrap().is_empty());
}

#[test]
fn canonical_page_ranges_persist_through_copies_retries_queue_journal_and_resume() {
    let root = TempDir::new().unwrap();
    let first = root.path().join("first.pdf");
    let second = root.path().join("second.pdf");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.set_pdf_page_count(10);
    adapter.push(Ok(()));
    adapter.push(Err(PrintFailure::failed("first failure")));
    adapter.push(Err(PrintFailure::failed("retry failure")));
    let service = print_service(&root, adapter.clone());
    let mut values = params(vec![first, second]);
    values.copies = 2;
    values.retries = 1;
    let plan = service.preview(&caller(), values, |_| {}).unwrap();
    let mut rows = plan.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["pageRange"] = json!("3,1-2");
    rows[1]["pageRange"] = json!("5");
    let revised = service.revise(&caller(), &plan.plan_id, &rows).unwrap();

    assert_eq!(revised.steps[0].meta["pageRange"], "1-3");
    assert_eq!(revised.steps[1].meta["pageRange"], "5");
    assert_eq!(
        revised.meta["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["pageRange"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["1-3", "5", "1-3", "5"]
    );
    let planned_job: PrintJob = serde_json::from_value(revised.meta["jobs"][0].clone()).unwrap();
    assert_eq!(planned_job.settings().page_range, "1-3");

    let result = service.apply(&revised.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    assert_eq!(
        adapter
            .submitted
            .lock()
            .unwrap()
            .iter()
            .map(|job| job.page_range.as_str())
            .collect::<Vec<_>>(),
        vec!["1-3", "5", "5"]
    );
    assert!(
        adapter
            .inspected_settings
            .lock()
            .unwrap()
            .iter()
            .any(|settings| settings.page_range == "1-3")
    );
    let saved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            root.path()
                .join("print-queues")
                .join(format!("{queue_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["pageRange"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["1-3", "5", "1-3", "5"]
    );
    let journal_file = std::fs::read_dir(root.path().join("journal"))
        .unwrap()
        .find(|entry| {
            entry.as_ref().is_ok_and(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
            })
        })
        .unwrap()
        .unwrap()
        .path();
    let journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(journal_file).unwrap()).unwrap();
    assert_eq!(journal["steps"][0]["meta"]["pageRange"], "1-3");

    let restarted = print_service(&root, adapter);
    let resumed = restarted.resume(&caller(), queue_id).unwrap();
    assert_eq!(
        resumed.meta["editor"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["pageRange"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["1-3", "5", "1-3", "5"]
    );
}

#[test]
fn legacy_queue_without_page_range_resumes_as_all() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("pause queue")));
    adapter.set_pdf_page_count(3);
    let service = print_service(&root, adapter);
    let plan = service
        .preview(&caller(), params(vec![file]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let queue_path = root
        .path()
        .join("print-queues")
        .join(format!("{queue_id}.json"));
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    saved["jobs"][0]
        .as_object_mut()
        .unwrap()
        .remove("pageRange");
    std::fs::write(&queue_path, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();
    let restarted = print_service(
        &root,
        Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        )),
    );

    let resumed = restarted.resume(&caller(), queue_id).unwrap();

    assert_eq!(resumed.meta["editor"]["rows"][0]["pageRange"], "all");
    assert_eq!(resumed.meta["jobs"][0]["pageRange"], "all");
}

#[test]
fn owned_persisted_queue_with_empty_page_range_is_rejected_on_resume() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let queue_id = "44444444-4444-4444-8444-444444444444";
    let queue_root = root.path().join("print-queues");
    std::fs::create_dir_all(&queue_root).unwrap();
    std::fs::write(
        queue_root.join(format!("{queue_id}.json")),
        serde_json::to_vec(&json!({
            "version": 1,
            "queueId": queue_id,
            "ownerId": "operator",
            "sessionId": "session",
            "createdAt": "2026-08-25T00:00:00+00:00",
            "updatedAt": "2026-08-25T00:00:01+00:00",
            "status": "paused",
            "currentJobId": null,
            "jobs": [{
                "jobId": "file-1-c1",
                "path": file,
                "printer": "Office Printer",
                "verb": "docflow",
                "format": "PDF",
                "pageRange": "",
                "signature": {},
                "status": "pending",
                "attempts": 0,
                "error": ""
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.set_pdf_page_count(3);
    let service = print_service(&root, adapter);

    let error = service.resume(&caller(), queue_id).unwrap_err();

    assert_eq!(error.message, "error.print.pageRangeInvalid");
}

#[test]
fn forged_job_with_invalid_page_range_fails_validation_without_submission() {
    let adapter = FakeAdapter::default();
    let job = PrintJob {
        job_id: "forged-c1".to_string(),
        path: "/approved/forged.pdf".to_string(),
        printer: "Office Printer".to_string(),
        verb: "docflow".to_string(),
        format: "PDF".to_string(),
        page_range: "0".to_string(),
        ..PrintJob::default()
    };

    let error = validate_job(&job).unwrap_err();

    assert_eq!(error.message, "error.print.pageRangeInvalid");
    assert!(adapter.submitted.lock().unwrap().is_empty());
    assert_eq!(
        PrintJobSupport::controlled_pdf(Some(4)).page_range,
        PageRangeCapability::Pdf {
            page_count: Some(4)
        }
    );
}

#[test]
fn queue_is_persisted_and_journaled_before_failure_then_resumes_and_discards() {
    let root = TempDir::new().unwrap();
    let first = root.path().join("one.pdf");
    let second = root.path().join("two.pdf");
    let third = root.path().join("three.pdf");
    for path in [&first, &second, &third] {
        std::fs::write(path, path.file_name().unwrap().as_encoded_bytes()).unwrap();
    }
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Ok(()));
    adapter.push(Err(PrintFailure::failed("paper jam\nprivate path omitted")));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(
            &caller(),
            params(vec![first.clone(), second.clone(), third.clone()]),
            |_| {},
        )
        .unwrap();

    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();

    assert_eq!(result.applied, 1);
    assert_eq!(result.failed, 1);
    assert!(result.journal_id.is_empty(), "printing is not undoable");
    assert_eq!(result.meta["nonUndoable"], true);
    assert_eq!(result.meta["resumeAvailable"], true);
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            root.path()
                .join("print-queues")
                .join(format!("{queue_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["ownerId"], "operator");
    assert_eq!(saved["sessionId"], "session");
    assert_eq!(saved["status"], "paused");
    assert_eq!(saved["currentJobId"], serde_json::Value::Null);
    assert_eq!(
        saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["completed", "failed", "pending"]
    );
    assert!(!saved["jobs"][1]["error"].as_str().unwrap().contains('\n'));
    let journal_files = std::fs::read_dir(root.path().join("journal"))
        .unwrap()
        .filter(|entry| {
            entry.as_ref().is_ok_and(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(journal_files.len(), 1);
    let journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(journal_files[0].path()).unwrap()).unwrap();
    assert_eq!(journal["opId"], "files.batch_print");
    assert_eq!(journal["steps"].as_array().unwrap().len(), 1);
    assert_eq!(journal["steps"][0]["kind"], "print");
    assert!(journal.get("pendingStep").is_none());

    let restarted = print_service(&root, adapter);
    let resumed = restarted.resume(&caller(), queue_id).unwrap();
    let rows = resumed.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["enabled"].as_bool().unwrap())
            .collect::<Vec<_>>(),
        vec![false, true, true]
    );
    assert_eq!(resumed.meta["jobs"].as_array().unwrap().len(), 2);
    let discarded = restarted.discard(&caller(), queue_id).unwrap();
    assert_eq!(discarded.resumable, 0);
    assert!(restarted.latest(&caller()).unwrap().is_none());
}

#[test]
fn partial_resume_rebuilds_the_complete_displayed_order_for_legacy_and_worksheet_queues() {
    let legacy_root = TempDir::new().unwrap();
    let legacy_files =
        ["one.pdf", "two.pdf", "three.pdf"].map(|name| legacy_root.path().join(name));
    for path in &legacy_files {
        std::fs::write(path, path.file_name().unwrap().as_encoded_bytes()).unwrap();
    }
    let legacy_adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    legacy_adapter.push(Ok(()));
    legacy_adapter.push(Err(PrintFailure::failed("pause queue")));
    let legacy_service = print_service(&legacy_root, legacy_adapter.clone());
    let legacy_plan = legacy_service
        .preview(&caller(), params(legacy_files.to_vec()), |_| {})
        .unwrap();
    let legacy_result = legacy_service
        .apply(&legacy_plan.plan_id, &caller(), |_| {})
        .unwrap();
    let legacy_queue_id = legacy_result.meta["queueId"].as_str().unwrap();
    let legacy_restarted = print_service(&legacy_root, legacy_adapter);
    let legacy_resumed = legacy_restarted.resume(&caller(), legacy_queue_id).unwrap();
    let legacy_rows = legacy_resumed.meta["editor"]["rows"].as_array().unwrap();
    assert_eq!(
        legacy_rows
            .iter()
            .map(|row| row["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["completed", "failed", "pending"]
    );
    let legacy_reordered = vec![
        legacy_rows[2].clone(),
        legacy_rows[0].clone(),
        legacy_rows[1].clone(),
    ];
    let legacy_revised = legacy_restarted
        .revise(&caller(), &legacy_resumed.plan_id, &legacy_reordered)
        .unwrap();

    legacy_restarted
        .apply(&legacy_revised.plan_id, &caller(), |_| {})
        .unwrap();

    let legacy_saved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            legacy_root
                .path()
                .join("print-queues")
                .join(format!("{legacy_queue_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        legacy_saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| Path::new(job["path"].as_str().unwrap())
                .file_name()
                .unwrap())
            .collect::<Vec<_>>(),
        vec![
            legacy_files[2].file_name().unwrap(),
            legacy_files[0].file_name().unwrap(),
            legacy_files[1].file_name().unwrap(),
        ]
    );
    assert_eq!(
        legacy_saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["completed", "completed", "completed"]
    );
    assert_eq!(
        legacy_saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["attempts"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 1, 2]
    );

    let worksheet_root = TempDir::new().unwrap();
    let workbook = worksheet_root.path().join("book.xlsx");
    write_workbook(
        &workbook,
        &[("MEC", 17, None), ("CIV", 18, None), ("ELE", 19, None)],
        &[],
    );
    let worksheet_adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let worksheet_service = print_service(&worksheet_root, worksheet_adapter.clone());
    let worksheet_plan = worksheet_service
        .preview(&caller(), params(vec![workbook]), |_| {})
        .unwrap();
    let worksheet_result = worksheet_service
        .apply(&worksheet_plan.plan_id, &caller(), |_| {})
        .unwrap();
    let worksheet_queue_id = worksheet_result.meta["queueId"].as_str().unwrap();
    let worksheet_queue_path = worksheet_root
        .path()
        .join("print-queues")
        .join(format!("{worksheet_queue_id}.json"));
    let mut worksheet_saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&worksheet_queue_path).unwrap()).unwrap();
    for (job, (status, attempts)) in worksheet_saved["jobs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip([("completed", 1), ("failed", 2), ("pending", 0)])
    {
        job["status"] = json!(status);
        job["attempts"] = json!(attempts);
    }
    std::fs::write(
        &worksheet_queue_path,
        serde_json::to_vec_pretty(&worksheet_saved).unwrap(),
    )
    .unwrap();
    let worksheet_restarted = print_service(&worksheet_root, worksheet_adapter.clone());
    let worksheet_resumed = worksheet_restarted
        .resume(&caller(), worksheet_queue_id)
        .unwrap();
    let worksheet_rows = worksheet_resumed.meta["editor"]["rows"].as_array().unwrap();
    let worksheet_reordered = vec![
        worksheet_rows[2].clone(),
        worksheet_rows[0].clone(),
        worksheet_rows[1].clone(),
    ];
    let worksheet_revised = worksheet_restarted
        .revise(&caller(), &worksheet_resumed.plan_id, &worksheet_reordered)
        .unwrap();

    worksheet_restarted
        .apply(&worksheet_revised.plan_id, &caller(), |_| {})
        .unwrap();

    let worksheet_saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&worksheet_queue_path).unwrap()).unwrap();
    assert_eq!(
        worksheet_saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["worksheet"]["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["ELE", "MEC", "CIV"]
    );
    assert_eq!(
        worksheet_saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["failed", "completed", "pending"]
    );
    assert_eq!(
        worksheet_saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|job| job["attempts"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 1, 2]
    );
    assert!(worksheet_adapter.submitted.lock().unwrap().is_empty());
}

#[test]
fn persisted_queues_are_adopted_only_by_the_exact_owner_after_restart() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("pause queue")));
    let service = print_service(&root, adapter.clone());
    let original = CallerIdentity::new("operator-a", "session-a");
    let plan = service
        .preview(&original, params(vec![file]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &original, |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    let restarted = print_service(&root, adapter);
    let other_owner = CallerIdentity::new("operator-b", "session-a");
    let fresh_session = CallerIdentity::new("operator-a", "session-b");

    assert!(restarted.latest(&other_owner).unwrap().is_none());
    assert_eq!(
        restarted.resume(&other_owner, queue_id).unwrap_err().code,
        "plan_not_found_or_expired"
    );
    assert_eq!(
        restarted.discard(&other_owner, queue_id).unwrap_err().code,
        "plan_not_found_or_expired"
    );
    assert_eq!(
        restarted.latest(&fresh_session).unwrap().unwrap().queue_id,
        queue_id
    );
    let observed: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            root.path()
                .join("print-queues")
                .join(format!("{queue_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(observed["ownerId"], "operator-a");
    assert_eq!(observed["sessionId"], "session-a");
    assert_eq!(
        restarted.resume(&fresh_session, queue_id).unwrap().op_id,
        "files.batch_print"
    );
    let adopted: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            root.path()
                .join("print-queues")
                .join(format!("{queue_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(adopted["sessionId"], "session-b");
    assert_eq!(
        restarted.discard(&fresh_session, queue_id).unwrap().status,
        "abandoned"
    );
    assert!(restarted.latest(&original).unwrap().is_none());
}

#[test]
fn retry_timeout_and_skip_semantics_produce_partial_results_without_reordering() {
    let root = TempDir::new().unwrap();
    let first = root.path().join("one.pdf");
    let second = root.path().join("two.pdf");
    std::fs::write(&first, b"one").unwrap();
    std::fs::write(&second, b"two").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("temporary spool failure")));
    adapter.push(Err(PrintFailure::failed("retry exhausted")));
    adapter.push(Ok(()));
    let service = print_service(&root, adapter.clone());
    let mut values = params(vec![first.clone(), second.clone()]);
    values.retries = 1;
    values.on_error = "skip".to_string();
    values.timeout_minutes = 7;
    let plan = service.preview(&caller(), values, |_| {}).unwrap();

    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();

    assert_eq!((result.applied, result.failed), (1, 1));
    assert_eq!(
        adapter
            .submitted
            .lock()
            .unwrap()
            .iter()
            .map(|job| (
                Path::new(&job.path).file_name().unwrap().to_owned(),
                job.timeout_minutes
            ))
            .collect::<Vec<_>>(),
        vec![
            (first.file_name().unwrap().to_owned(), 7),
            (first.file_name().unwrap().to_owned(), 7),
            (second.file_name().unwrap().to_owned(), 7),
        ]
    );
    assert_eq!(result.meta["status"], "completed_with_errors");
    assert_eq!(result.meta["counts"]["failed"], 1);
    assert_eq!(result.meta["counts"]["completed"], 1);
}

#[test]
fn uncertain_and_cancelled_jobs_are_never_retried_automatically() {
    for (failure, expected_status, expected_warning) in [
        (
            PrintFailure::uncertain("spool outcome unknown"),
            "uncertain",
            "warn.print.stateUncertain",
        ),
        (
            PrintFailure::cancelled("cancelled while active", false),
            "cancelled",
            "warn.print.cancelled",
        ),
        (
            PrintFailure::cancelled("cancelled before acknowledgement", true),
            "uncertain",
            "warn.print.cancelledUncertain",
        ),
    ] {
        let root = TempDir::new().unwrap();
        let file = root.path().join("one.pdf");
        std::fs::write(&file, b"one").unwrap();
        let adapter = Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        ));
        adapter.push(Err(failure));
        adapter.push(Ok(()));
        let service = print_service(&root, adapter.clone());
        let mut values = params(vec![file]);
        values.retries = 3;
        let plan = service.preview(&caller(), values, |_| {}).unwrap();

        let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();

        assert_eq!(adapter.submitted.lock().unwrap().len(), 1);
        assert_eq!(result.meta["counts"][expected_status], 1);
        assert_eq!(result.errors[0].key, expected_warning);
    }
}

#[test]
fn submitted_unconfirmed_job_is_not_retried_and_queue_continues() {
    let root = TempDir::new().unwrap();
    let first = root.path().join("one.pdf");
    let second = root.path().join("two.pdf");
    std::fs::write(&first, b"one").unwrap();
    std::fs::write(&second, b"two").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::submitted_unconfirmed(
        "Windows released the receipt after accepting the job",
    )));
    adapter.push(Ok(()));
    let service = print_service(&root, adapter.clone());
    let mut values = params(vec![first, second]);
    values.retries = 3;
    values.on_error = "pause".to_string();
    let plan = service.preview(&caller(), values, |_| {}).unwrap();

    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();

    assert_eq!(adapter.submitted.lock().unwrap().len(), 2);
    assert_eq!((result.applied, result.failed), (2, 0));
    assert_eq!(result.meta["status"], "completed");
    assert_eq!(result.meta["resumable"], 0);
    assert_eq!(result.meta["counts"]["completed_unconfirmed"], 1);
    assert_eq!(result.meta["counts"]["completed"], 1);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].key, "warn.print.submittedUnconfirmed");

    let journal = std::fs::read_dir(root.path().join("journal"))
        .unwrap()
        .find_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension().and_then(|value| value.to_str()) == Some("json"))
                .then(|| std::fs::read(path).ok())
                .flatten()
        })
        .expect("print journal");
    let journal: serde_json::Value = serde_json::from_slice(&journal).unwrap();
    assert_eq!(
        journal["steps"][0]["meta"]["outcome"],
        "submitted_unconfirmed"
    );
    assert_eq!(journal["state"], "committed");
}

#[test]
fn restart_recovery_marks_an_active_job_uncertain_and_requires_explicit_selection() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    adapter.push(Err(PrintFailure::failed("pause queue")));
    let service = print_service(&root, adapter.clone());
    let plan = service
        .preview(&caller(), params(vec![file]), |_| {})
        .unwrap();
    let result = service.apply(&plan.plan_id, &caller(), |_| {}).unwrap();
    let queue_id = result.meta["queueId"].as_str().unwrap();
    adapter.submitted.lock().unwrap().clear();
    let queue_path = root
        .path()
        .join("print-queues")
        .join(format!("{queue_id}.json"));
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    saved["status"] = json!("running");
    saved["currentJobId"] = saved["jobs"][0]["jobId"].clone();
    saved["jobs"][0]["status"] = json!("printing");
    saved["jobs"][0]["attempts"] = json!(1);
    saved["jobs"][0]["error"] = json!("");
    std::fs::write(&queue_path, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();
    let restarted = print_service(&root, adapter);

    let summary = restarted.latest(&caller()).unwrap().unwrap();
    assert_eq!(summary.status, "paused");
    assert_eq!(summary.counts["uncertain"], 1);
    let persisted_before_resume: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&queue_path).unwrap()).unwrap();
    assert_eq!(persisted_before_resume["status"], "running");
    assert_eq!(persisted_before_resume["jobs"][0]["status"], "printing");
    let resumed = restarted.resume(&caller(), queue_id).unwrap();
    assert_eq!(resumed.meta["editor"]["rows"][0]["status"], "uncertain");
    assert_eq!(resumed.meta["editor"]["rows"][0]["enabled"], false);
    assert_eq!(resumed.meta["jobs"].as_array().unwrap().len(), 0);
    let mut rows = resumed.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["enabled"] = json!(true);
    let approved = restarted
        .revise(&caller(), &resumed.plan_id, &rows)
        .unwrap();
    let result = restarted
        .apply(&approved.plan_id, &caller(), |_| {})
        .unwrap();
    assert_eq!((result.applied, result.failed), (1, 0));
    assert_eq!(result.meta["counts"]["completed"], 1);
}

#[test]
fn legacy_unowned_queues_remain_untrusted_and_unavailable() {
    let root = TempDir::new().unwrap();
    let queue_id = "22222222-2222-4222-8222-222222222222";
    let queue_root = root.path().join("print-queues");
    std::fs::create_dir_all(&queue_root).unwrap();
    std::fs::write(
        queue_root.join(format!("{queue_id}.json")),
        serde_json::to_vec(&json!({
            "version": 1,
            "queueId": queue_id,
            "createdAt": "2026-08-25T00:00:00+00:00",
            "updatedAt": "2026-08-25T00:00:01+00:00",
            "status": "paused",
            "currentJobId": null,
            "jobs": [{
                "jobId": "file-1-c1",
                "path": root.path().join("one.pdf"),
                "printer": "Office Printer",
                "verb": "docflow",
                "format": "PDF",
                "signature": {},
                "status": "pending",
                "attempts": 0,
                "error": ""
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(root.path().join("one.pdf"), b"one").unwrap();
    let service = print_service(
        &root,
        Arc::new(FakeAdapter::with_printers(
            &["Office Printer"],
            Some("Office Printer"),
        )),
    );

    assert!(service.latest(&caller()).unwrap().is_none());
    assert_eq!(
        service.resume(&caller(), queue_id).unwrap_err().code,
        "plan_not_found_or_expired"
    );
    assert_eq!(
        service.discard(&caller(), queue_id).unwrap_err().code,
        "plan_not_found_or_expired"
    );
}

#[test]
fn unknown_printers_unsupported_options_and_injection_strings_fail_before_submit() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter.clone());
    let mut malicious = params(vec![file.clone()]);
    malicious.printer = "Office Printer\"; Remove-Item C:\\\\*".to_string();

    let plan = service.preview(&caller(), malicious, |_| {}).unwrap();

    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.print.printerUnavailable");
    assert_eq!(adapter.inspected.load(Ordering::SeqCst), 0);
    assert!(adapter.submitted.lock().unwrap().is_empty());

    *adapter.unsupported_paper.lock().unwrap() = Some("A3".to_string());
    let mut unsupported = params(vec![file]);
    unsupported.paper = "A3".to_string();
    let plan = service.preview(&caller(), unsupported, |_| {}).unwrap();
    assert!(plan.steps.is_empty());
    assert_eq!(plan.warnings[0].key, "warn.print.unsupportedOptions");
    assert!(adapter.submitted.lock().unwrap().is_empty());
}

#[test]
fn active_queue_is_not_recovered_or_discarded_while_submission_is_running() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("one.pdf");
    std::fs::write(&file, b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let gate = Arc::new((Mutex::new((false, false)), Condvar::new()));
    *adapter.submit_gate.lock().unwrap() = Some(gate.clone());
    let service = Arc::new(print_service(&root, adapter));
    let plan = service
        .preview(&caller(), params(vec![file]), |_| {})
        .unwrap();
    let queue_id = plan.meta["queueId"].as_str().unwrap().to_string();
    let plan_id = plan.plan_id;
    let applying = {
        let service = service.clone();
        thread::spawn(move || service.apply(&plan_id, &caller(), |_| {}))
    };
    let (state, changed) = &*gate;
    let mut state = state.lock().unwrap();
    while !state.0 {
        state = changed.wait(state).unwrap();
    }
    assert!(service.latest(&caller()).unwrap().is_none());
    assert_eq!(
        service.discard(&caller(), &queue_id).unwrap_err().code,
        "operation_in_progress"
    );
    state.1 = true;
    changed.notify_all();
    drop(state);
    assert_eq!(applying.join().unwrap().unwrap().applied, 1);
}
