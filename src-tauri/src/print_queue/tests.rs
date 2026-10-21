use super::*;

#[test]
fn diagnostic_detail_is_persisted_without_replacing_the_localized_error() {
    let failure = PrintFailure::failed("error.print.excelComFailed")
        .with_diagnostic("excel_com stage=open hresult=0x800A03EC");

    assert_eq!(failure.message, "error.print.excelComFailed");
    assert_eq!(
        failure_evidence(&failure),
        "excel_com stage=open hresult=0x800A03EC"
    );
}

#[test]
fn printer_diagnostic_reports_routes_and_outcomes_without_file_paths() {
    let mut job = PrintJob {
        path: "C:\\Private\\RFI No. secret.pdf".to_string(),
        printer: "EPSON L15160 Series".to_string(),
        format: "pdf".to_string(),
        verb: "docflow".to_string(),
        status: Some("completed_unconfirmed".to_string()),
        attempts: Some(1),
        error: Some("driver released the spool record".to_string()),
        ..PrintJob::default()
    };
    job.job_id = "job-1".to_string();
    let report = queue_diagnostic(QueueState {
        version: 1,
        queue_id: "queue-1".to_string(),
        owner_id: "owner".to_string(),
        session_id: "session".to_string(),
        created_at: utc_now(),
        updated_at: utc_now(),
        status: "completed".to_string(),
        current_job_id: None,
        jobs: vec![job],
        grant_root_identities: Vec::new(),
        source_identities: Vec::new(),
        extra: Map::new(),
    });

    let encoded = serde_json::to_string(&report).unwrap();
    assert_eq!(report.printer, "EPSON L15160 Series");
    assert_eq!(report.jobs[0].route, "docflow");
    assert_eq!(report.jobs[0].status, "completed_unconfirmed");
    assert!(!encoded.contains("Private"));
    assert!(!encoded.contains("secret.pdf"));
}

#[test]
fn cleanup_review_wire_collection_deduplicates_exact_locator() {
    let mut job = serde_json::to_value(PrintJob::default()).unwrap();
    let evidence = json!({
        "warningKey": "error.print.exportCleanupRequiresReview",
        "locator": "docflow-excel-retained"
    });
    job["cleanupReview"] = json!([evidence.clone(), evidence.clone()]);

    let deduplicated: PrintJob = serde_json::from_value(job).unwrap();

    assert_eq!(
        serde_json::to_value(deduplicated).unwrap()["cleanupReview"],
        evidence
    );
}

#[test]
fn cleanup_review_wire_collection_enforces_its_distinct_locator_bound() {
    let evidence = |index| {
        json!({
            "warningKey": "error.print.exportCleanupRequiresReview",
            "locator": format!("docflow-excel-{index}")
        })
    };
    let mut job = serde_json::to_value(PrintJob::default()).unwrap();
    job["cleanupReview"] = json!((0..16).map(evidence).collect::<Vec<_>>());
    let accepted: PrintJob = serde_json::from_value(job.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(accepted).unwrap()["cleanupReview"]
            .as_array()
            .unwrap()
            .len(),
        16
    );

    job["cleanupReview"] = json!((0..17).map(evidence).collect::<Vec<_>>());
    let error = serde_json::from_value::<PrintJob>(job).unwrap_err();
    assert!(error.to_string().contains("cleanup review evidence limit"));
}

#[test]
fn cleanup_evidence_is_not_retry_authority_but_quarantine_disposition_is() {
    let review = CleanupReview {
        warning_key: "error.print.exportCleanupRequiresReview",
        locator: "docflow-excel-test".to_string(),
    };
    let retryable = PrintFailure::failed("pre-effect failure").with_cleanup_review(review);

    assert!(retry_disposition_allows_retry(&retryable));
    assert!(retryable.cleanup_review.is_some());

    let quarantined = retryable.with_retry_disposition(RetryDisposition::CleanupQuarantined);
    assert!(!retry_disposition_allows_retry(&quarantined));
    assert!(quarantined.cleanup_review.is_some());
}

#[test]
fn protected_workbook_safety_is_unsafe_for_resume_and_apply() {
    let safety = WorkbookSafetyFacts {
        has_workbook_protection: true,
        ..WorkbookSafetyFacts::default()
    };

    assert!(workbook_safety_is_unsafe(&safety));
}
