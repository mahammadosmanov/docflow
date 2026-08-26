use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use docflow_lib::{
    operations::batch_print::validate_job,
    page_range::PageRangeCapability,
    paths::CallerIdentity,
    print_queue::{
        BatchPrintParams, PrintAdapter, PrintFailure, PrintJob, PrintJobSupport, PrintService,
        PrintSettings, PrinterInventory,
    },
    progress::GlobalMutationLock,
};
use serde_json::json;
use tempfile::TempDir;

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
}

impl PrintAdapter for FakeAdapter {
    fn inventory(&self) -> Result<PrinterInventory, PrintFailure> {
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
        .next()
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
    let queue_id = "33333333-3333-4333-8333-333333333333";
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

    let resumed = service.resume(&caller(), queue_id).unwrap();

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
    let adopted: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            root.path()
                .join("print-queues")
                .join(format!("{queue_id}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(adopted["ownerId"], "operator-a");
    assert_eq!(adopted["sessionId"], "session-b");
    assert_eq!(
        restarted.resume(&fresh_session, queue_id).unwrap().op_id,
        "files.batch_print"
    );
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
fn restart_recovery_marks_an_active_job_uncertain_and_requires_explicit_selection() {
    let root = TempDir::new().unwrap();
    let queue_id = "11111111-1111-4111-8111-111111111111";
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
            "status": "running",
            "currentJobId": "file-1-c1",
            "jobs": [{
                "jobId": "file-1-c1",
                "path": root.path().join("one.pdf"),
                "printer": "Office Printer",
                "verb": "docflow",
                "format": "PDF",
                "signature": {},
                "status": "printing",
                "attempts": 1,
                "error": ""
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(root.path().join("one.pdf"), b"one").unwrap();
    let adapter = Arc::new(FakeAdapter::with_printers(
        &["Office Printer"],
        Some("Office Printer"),
    ));
    let service = print_service(&root, adapter);

    let summary = service.latest(&caller()).unwrap().unwrap();
    assert_eq!(summary.status, "paused");
    assert_eq!(summary.counts["uncertain"], 1);
    let resumed = service.resume(&caller(), queue_id).unwrap();
    assert_eq!(resumed.meta["editor"]["rows"][0]["status"], "uncertain");
    assert_eq!(resumed.meta["editor"]["rows"][0]["enabled"], false);
    assert_eq!(resumed.meta["jobs"].as_array().unwrap().len(), 0);
    let mut rows = resumed.meta["editor"]["rows"].as_array().unwrap().clone();
    rows[0]["enabled"] = json!(true);
    let approved = service.revise(&caller(), &resumed.plan_id, &rows).unwrap();
    let result = service.apply(&approved.plan_id, &caller(), |_| {}).unwrap();
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
