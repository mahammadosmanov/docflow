use super::*;

#[path = "../../../tests/support/pdf_fixture.rs"]
#[allow(dead_code)]
mod pdf_fixture;

#[test]
fn signed_comparison_checks_the_exact_row_candidate_and_bound_file_hash() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = super::super::tests::test_backend(&temporary);
    let inputs = temporary.path().join("inputs");
    fs::create_dir(&inputs).unwrap();
    let inputs = fs::canonicalize(inputs).unwrap();
    let archive = inputs.join("archive.pdf");
    let signed = inputs.join("signed.pdf");
    pdf_fixture::write_signed(&archive, false);
    pdf_fixture::write_signed(&signed, false);
    let row_id = "signed-row-21";
    let archive_token = "archive-choice-21";
    let caller = backend.caller_identity().unwrap();
    let scope = GrantScope::new(
        &caller.owner_id,
        &caller.session_id,
        "preview",
        OPERATION_ID,
    );
    let grant = backend
        .filesystem
        .issue_grant(&scope, &inputs, GrantAccess::Read)
        .unwrap();
    let step = Step {
        kind: StepKind::PdfFirstPageReplace,
        src: Some(archive.to_string_lossy().into_owned()),
        dst: None,
        note_key: None,
        meta: serde_json::from_value(json!({
            "rowId": row_id,
            "signedPdf": signed,
            "signedPageIndex": 0,
            "signedSha256": file_sha256(&signed).unwrap(),
            "archiveSha256": file_sha256(&archive).unwrap(),
        }))
        .unwrap(),
    };
    let plan = backend
        .filesystem
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                OPERATION_ID,
                vec![grant.grant_id],
            ),
            Plan {
                plan_id: String::new(),
                op_id: OPERATION_ID.to_string(),
                steps: Vec::new(),
                warnings: Vec::new(),
                meta: serde_json::from_value(json!({
                    "pageComparisonOnDemand": true,
                    "archiveCandidateSteps": { (row_id): { (archive_token): [step] } },
                }))
                .unwrap(),
            },
        )
        .unwrap();
    let mut request =
        json!({ "planId": plan.plan_id, "rowId": row_id, "archiveChoice": archive_token });
    let result = backend.signed_page_comparison(request.clone()).unwrap();
    assert!(
        result["archive"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert!(
        result["signed"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    let mut forged = request.clone();
    forged["path"] = json!("/tmp/ungranted.pdf");
    assert!(backend.signed_page_comparison(forged).is_err());
    request["archiveChoice"] = json!("unoffered-candidate");
    assert!(backend.signed_page_comparison(request.clone()).is_err());
    request["archiveChoice"] = json!(archive_token);
    fs::write(&archive, b"changed after Preview").unwrap();
    assert_eq!(
        backend
            .signed_page_comparison(request.clone())
            .unwrap_err()
            .message,
        "rfi.signed.previewUnavailable"
    );
    backend.rotate_session_authority();
    assert_eq!(
        backend.signed_page_comparison(request).unwrap_err().code,
        BackendError::plan_session_mismatch().code
    );
}
