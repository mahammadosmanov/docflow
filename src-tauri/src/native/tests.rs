use std::collections::VecDeque;

use crate::{
    credentials::{CredentialError, CredentialStore, StoredSession},
    http::{HttpError, HttpResponse},
};
use serde_json::json;

use super::*;

#[test]
fn existing_fast_scan_settings_default_to_multi_rfi() {
    let values: ProcessScansValues = serde_json::from_value(json!({
        "source": "/tmp/source",
        "dest": "/tmp/destination",
        "organize_into_rfi_folders": true,
        "keep_original": false,
        "correct_orientation": false
    }))
    .unwrap();

    assert_eq!(values.scan_mode, ScanMode::MultiRfi);
}
#[test]
fn fast_scan_plan_revision_routes_through_trusted_scan_editor() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let source = temporary.path().join("source");
    let destination = temporary.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let source_file = source.join("scan.pdf");
    fs::write(&source_file, b"source PDF").unwrap();
    let destination = fs::canonicalize(destination).unwrap();
    let original_code = "AZE-SUN-INT-QMS-RFI-CIV-0064";
    let original_target = crate::naming::target_for(&destination.join("CIV"), original_code);
    let step_meta = json!({
        "code": original_code,
        "confidence": 1.0,
        "intoFolder": true,
        "folderCreated": true,
        "source": "ocr:fixture",
        "scanEditRowId": "scan-output-0"
    })
    .as_object()
    .unwrap()
    .clone();
    let rows = json!([{
        "_rowId": "scan-output-0",
        "sourceFile": "scan.pdf",
        "evidenceKey": "rfi.fastScan.evidence.confirmed",
        "detectedCode": original_code,
        "code": original_code,
        "target": original_target.to_string_lossy(),
    }]);
    let plan = crate::contracts::Plan {
        plan_id: String::new(),
        op_id: FAST_COVER_OPERATION_ID.to_string(),
        steps: vec![crate::contracts::Step {
            kind: StepKind::Copy,
            src: Some(source_file.to_string_lossy().into_owned()),
            dst: Some(original_target.to_string_lossy().into_owned()),
            note_key: Some("note.renamed".to_string()),
            meta: step_meta,
        }],
        warnings: Vec::new(),
        meta: json!({
            "editor": {"rows": rows},
            "scanEdit": {
                "destinationRoot": destination.to_string_lossy(),
                "organizeIntoRfiFolders": true
            }
        })
        .as_object()
        .unwrap()
        .clone(),
    };
    let caller = backend.caller_identity().unwrap();
    let scope = GrantScope::new(
        &caller.owner_id,
        &caller.session_id,
        "fast-scan-revision",
        FAST_COVER_OPERATION_ID,
    );
    let source_grant = backend
        .filesystem
        .issue_grant(&scope, &source, GrantAccess::ReadWrite)
        .unwrap();
    let destination_grant = backend
        .filesystem
        .issue_grant(&scope, &destination, GrantAccess::ReadWrite)
        .unwrap();
    let cached = backend
        .filesystem
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                FAST_COVER_OPERATION_ID,
                vec![source_grant.grant_id, destination_grant.grant_id],
            ),
            plan,
        )
        .unwrap();
    let mut rows = rows.as_array().unwrap().clone();
    rows[0]["code"] = json!("AZE-SUN-INT-QMS-RFI-MEC-0192");
    let revised = backend
        .revise_native_plan(json!({"planId": cached.plan_id, "rows": rows}))
        .expect("Fast RFI Scan revisions are registered");
    let output = revised["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["noteKey"] == "note.renamed")
        .unwrap();
    assert_eq!(
        output["dst"],
        crate::naming::target_for(&destination.join("MEC"), "AZE-SUN-INT-QMS-RFI-MEC-0192")
            .to_string_lossy()
            .as_ref()
    );
}

static WORK_ITEM_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn rfi_reply_requires_exact_committed_plan_provenance() {
    let caller = CallerIdentity::new("owner", "session");
    let mut journal = crate::journal::JournalRecord::new(
        "journal",
        "rfi.log_add",
        "2026-09-19T00:00:00Z",
        "owner",
        "session",
        "digest",
    );
    journal.state = Some(crate::journal::JournalState::Committed);

    assert!(committed_rfi_reply_matches(&journal, &caller, "digest"));
    assert!(!committed_rfi_reply_matches(&journal, &caller, "other"));
    journal.state = Some(crate::journal::JournalState::PartialCommitted);
    assert!(!committed_rfi_reply_matches(&journal, &caller, "digest"));
}

struct TestCredentials(Mutex<Option<StoredSession>>);

impl CredentialStore for TestCredentials {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn write(&self, session: &StoredSession) -> Result<(), CredentialError> {
        *self.0.lock().unwrap() = Some(session.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

struct TestHttp(Mutex<VecDeque<Result<HttpResponse, HttpError>>>);

impl JsonHttp for TestHttp {
    fn send_json(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&Value>,
    ) -> Result<HttpResponse, HttpError> {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(HttpError::InvalidResponse))
    }
}

fn auth_responses(owner_id: &str) -> [Result<HttpResponse, HttpError>; 2] {
    auth_responses_with_allowed(owner_id, true)
}

fn auth_responses_with_allowed(
    owner_id: &str,
    allowed: bool,
) -> [Result<HttpResponse, HttpError>; 2] {
    [
        Ok(HttpResponse::from_json(
            200,
            &json!({
                "access_token": "access-secret",
                "refresh_token": "refresh-secret",
                "expires_at": i64::MAX,
                "user": {"id": owner_id, "email": "user@example.com"}
            }),
        )),
        Ok(HttpResponse::from_json(
            200,
            &json!([{
                "id": owner_id,
                "email": "user@example.com",
                "allowed": allowed,
                "is_admin": false
            }]),
        )),
    ]
}

fn test_backend(temporary: &tempfile::TempDir) -> NativeBackend {
    let responses = auth_responses("owner")
        .into_iter()
        .chain(auth_responses("owner"))
        .collect();
    test_backend_with_responses(temporary, responses)
}

#[test]
fn retired_process_scans_rejects_new_preview() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);

    let result = backend.preview_process_scans(
        json!({
            "opId": "rfi.process_scans",
            "values": {}
        }),
        |_| {},
    );

    assert!(
        result.is_err(),
        "retired operation unexpectedly created a plan"
    );
}

#[test]
fn concrete_document_preview_skips_missing_follow_up_and_applies_available_rfis() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let input = temporary.path().join("inputs");
    let output = temporary.path().join("output");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::create_dir_all(&output).unwrap();

    let rfi_log = input.join("RFI Log.xlsx");
    let log_sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>RFI No.</t></is></c><c r="B1" t="inlineStr"><is><t>FORMS</t></is></c><c r="C1" t="inlineStr"><is><t>TAG NO</t></is></c><c r="D1" t="inlineStr"><is><t>AREA</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0123</t></is></c><c r="B2" t="inlineStr"><is><t>CON-CIV-010</t></is></c><c r="C2" t="inlineStr"><is><t>N1</t></is></c><c r="D2" t="inlineStr"><is><t>North Site</t></is></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0124</t></is></c><c r="B3" t="inlineStr"><is><t>CON-CIV-010</t></is></c><c r="C3" t="inlineStr"><is><t>N2</t></is></c><c r="D3" t="inlineStr"><is><t>North Site</t></is></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0125</t></is></c><c r="B4" t="inlineStr"><is><t>CON-CIV-011</t></is></c><c r="C4" t="inlineStr"><is><t>N3</t></is></c><c r="D4" t="inlineStr"><is><t>North Site</t></is></c></row></sheetData></worksheet>"#;
    std::fs::write(&rfi_log, test_xlsx("CIV", log_sheet)).unwrap();

    let follow_up = input.join("Concrete Follow-Up.xlsx");
    let follow_sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>RFI No</t></is></c><c r="B1" t="inlineStr"><is><t>Concrete Class</t></is></c><c r="C1" t="inlineStr"><is><t>Batch Ticket No</t></is></c><c r="D1" t="inlineStr"><is><t>Unit /Location</t></is></c><c r="E1" t="inlineStr"><is><t>Temperature In Plant (°C)</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>CIV-0123</t></is></c><c r="B2" t="inlineStr"><is><t>C-30/37 XA2</t></is></c><c r="C2" t="inlineStr"><is><t>63840</t></is></c><c r="D2" t="inlineStr"><is><t>N1</t></is></c><c r="E2" t="inlineStr"><is><t>20.3</t></is></c></row></sheetData></worksheet>"#;
    std::fs::write(&follow_up, test_xlsx("Follow Up", follow_sheet)).unwrap();

    let template = input.join("CON-CIV-010.xlsx");
    let template_sheet = concat!(
        r#"<worksheet><dimension ref="A1:CS22"/><sheetData><row r="11"><c r="A11" t="inlineStr"><is><t>DRAWING NO: DRG-1</t></is></c></row><row r="12"><c r="A12" t="inlineStr"><is><t>REPORT (RFI) NUMBER: AZE-SUN-INT-QMS-RFI-CIV-1460</t></is></c></row><row r="13"><c r="A13" t="inlineStr"><is><t>LOCATION: North site area / N-5</t></is></c></row><row r="14"><c r="BB14" t="inlineStr"><is><t>CONCRETE POURING DATE:06.10.2026</t></is></c></row>"#,
        r#"<row r="19"><c r="A19" t="inlineStr"><is><t>N-5</t></is></c><c r="H19" t="inlineStr"><is><t>C-30/37 XA2</t></is></c><c r="O19" t="inlineStr"><is><t>HOLCIM</t></is></c><c r="V19" t="inlineStr"><is><t>66817</t></is></c><c r="AC19" t="inlineStr"><is><t>TRK-1</t></is></c><c r="CS19" t="inlineStr"><is><t>841-846</t></is></c><c r="AI19"><v>1</v></c></row><row r="20"></row>"#,
        r#"<row r="21"><c r="A21" t="inlineStr"><is><t></t></is></c><c r="H21" t="inlineStr"><is><t></t></is></c><c r="O21" t="inlineStr"><is><t></t></is></c><c r="V21" t="inlineStr"><is><t></t></is></c><c r="AC21" t="inlineStr"><is><t></t></is></c><c r="CS21" t="inlineStr"><is><t></t></is></c></row><row r="22"><c r="AI22"><v>3</v></c></row></sheetData></worksheet>"#
    );
    std::fs::write(
        &template,
        test_ooxml_package(&[("xl/worksheets/sheet1.xml", template_sheet)]),
    )
    .unwrap();

    let caller = backend.caller_identity().unwrap();
    let issue = |name: &str, path: &Path, kind| {
        backend
            .selections
            .issue(&caller, "rfi.generate_concrete_documents", name, path, kind)
            .unwrap()
    };
    let result = backend
        .preview_concrete_documents(
            json!({
                "opId": "rfi.generate_concrete_documents",
                "values": {
                    "rfis_010": "123,124,125",
                    "rfi_log_workbook": issue("rfi_log_workbook", &rfi_log, SelectionKind::File),
                    "follow_up_workbook": issue("follow_up_workbook", &follow_up, SelectionKind::File),
                    "template_010": issue("template_010", &template, SelectionKind::File),
                    "output_directory": issue("output_directory", &output, SelectionKind::Folder)
                }
            }),
            |_| {},
        )
        .unwrap();

    assert_eq!(result["steps"].as_array().unwrap().len(), 1);
    let warnings = result["warnings"].as_array().unwrap();
    assert!(warnings.contains(&json!({
        "key": "warn.rfi.documents.followUpMissing",
        "subject": "AZE-SUN-INT-QMS-RFI-CIV-0124"
    })));
    assert!(warnings.contains(&json!({
        "key": "warn.rfi.documents.formNotRequired",
        "subject": "AZE-SUN-INT-QMS-RFI-CIV-0125"
    })));
    assert_eq!(result["meta"]["generatedCount"], 1);

    backend
        .apply_native_plan(
            result["planId"].as_str().unwrap(),
            Some("rfi.generate_concrete_documents"),
            |_| {},
        )
        .unwrap();
    assert!(output.join("CON-CIV-010_CIV-0123.xlsx").is_file());
    assert!(!output.join("CON-CIV-010_CIV-0124.xlsx").exists());
}

#[test]
fn concrete_document_preview_does_not_require_follow_up_for_unlisted_forms() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let input = temporary.path().join("inputs");
    let output = temporary.path().join("output");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::create_dir_all(&output).unwrap();

    let rfi_log = input.join("RFI Log.xlsx");
    let log_sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>RFI No.</t></is></c><c r="B1" t="inlineStr"><is><t>FORMS</t></is></c><c r="C1" t="inlineStr"><is><t>AREA</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0125</t></is></c><c r="B2" t="inlineStr"><is><t>CON-CIV-011</t></is></c><c r="C2" t="inlineStr"><is><t>North Site</t></is></c></row></sheetData></worksheet>"#;
    std::fs::write(&rfi_log, test_xlsx("CIV", log_sheet)).unwrap();
    let template = input.join("CON-CIV-010.xlsx");
    std::fs::write(&template, b"template is not used for an ineligible RFI").unwrap();

    let caller = backend.caller_identity().unwrap();
    let issue = |name: &str, path: &Path, kind| {
        backend
            .selections
            .issue(&caller, "rfi.generate_concrete_documents", name, path, kind)
            .unwrap()
    };
    let result = backend
        .preview_concrete_documents(
            json!({
                "opId": "rfi.generate_concrete_documents",
                "values": {
                    "rfis_010": "125",
                    "rfi_log_workbook": issue("rfi_log_workbook", &rfi_log, SelectionKind::File),
                    "template_010": issue("template_010", &template, SelectionKind::File),
                    "output_directory": issue("output_directory", &output, SelectionKind::Folder)
                }
            }),
            |_| {},
        )
        .unwrap();

    assert!(result["steps"].as_array().unwrap().is_empty());
    assert_eq!(
        result["warnings"],
        json!([{
            "key": "warn.rfi.documents.formNotRequired",
            "subject": "AZE-SUN-INT-QMS-RFI-CIV-0125"
        }])
    );
    assert_eq!(result["meta"]["generatedCount"], 0);
}

fn test_xlsx(sheet_name: &str, sheet: &str) -> Vec<u8> {
    let workbook = format!(
        r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{sheet_name}" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    test_ooxml_package(&[
        ("[Content_Types].xml", "content-types"),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        ("xl/workbook.xml", &workbook),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        ("xl/worksheets/sheet1.xml", sheet),
    ])
}

fn test_ooxml_package(entries: &[(&str, &str)]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, contents) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn orientation_preview_survives_grant_expiry_during_large_scan() {
    let temporary = tempfile::tempdir().unwrap();
    let mut backend = test_backend(&temporary);
    backend.filesystem = FilesystemAuthority::new(
        temporary.path().join("short-grant-journal"),
        Duration::from_millis(100),
    );
    let source = temporary.path().join("RFI System");
    let review = temporary.path().join("Review");
    std::fs::create_dir_all(source.join("PRC")).unwrap();
    std::fs::create_dir_all(&review).unwrap();
    let caller = backend.caller_identity().unwrap();
    let selected_source = backend
        .selections
        .issue(
            &caller,
            "rfi.normalize_pdf_orientation",
            "rfi_system_root",
            &source,
            SelectionKind::Folder,
        )
        .unwrap();
    let selected_review = backend
        .selections
        .issue(
            &caller,
            "rfi.normalize_pdf_orientation",
            "review_root",
            &review,
            SelectionKind::Folder,
        )
        .unwrap();
    let mut delayed = false;
    let result = backend.preview_pdf_orientation(
        json!({
            "opId": "rfi.normalize_pdf_orientation",
            "values": {
                "rfi_system_root": selected_source,
                "review_root": selected_review
            }
        }),
        |_| {
            if !delayed {
                std::thread::sleep(Duration::from_millis(200));
                delayed = true;
            }
        },
    );
    assert!(result.is_ok(), "{result:?}");
}

fn test_backend_with_responses(
    temporary: &tempfile::TempDir,
    responses: VecDeque<Result<HttpResponse, HttpError>>,
) -> NativeBackend {
    let settings = Arc::new(SettingsStore::new(temporary.path().join("settings.json")).unwrap());
    let http: Arc<dyn JsonHttp> = Arc::new(TestHttp(Mutex::new(responses)));
    let config = AuthConfig::new("http://supabase.test", "publishable");
    let auth = Arc::new(AuthService::new(
        settings.clone(),
        Arc::new(TestCredentials(Mutex::new(None))),
        http.clone(),
        config.clone(),
    ));
    auth.sign_in("user@example.com", "password", now()).unwrap();
    let journal_dir = temporary.path().join("journal");
    let mutation_lock = GlobalMutationLock::default();
    NativeBackend {
        settings,
        auth: auth.clone(),
        cloud: Arc::new(CloudHistory::new(
            auth,
            http.clone(),
            config,
            journal_dir.clone(),
        )),
        gemini: Arc::new(GeminiClient::new(
            http,
            "http://gemini.test",
            vec![DEFAULT_MODEL.to_string()],
        )),
        journals: JournalStore::new(&journal_dir),
        filesystem: FilesystemAuthority::with_mutation_lock(
            &journal_dir,
            Duration::from_secs(60),
            mutation_lock.clone(),
        ),
        selections: PathSelectionStore::new(Duration::from_secs(60)),
        print: PrintService::new(
            temporary.path().join("print-queues"),
            &journal_dir,
            Duration::from_secs(60),
            mutation_lock.clone(),
            platform::production_print_adapter(),
        ),
        outlook: OutlookDraftService::new(
            &journal_dir,
            Duration::from_secs(60),
            mutation_lock,
            platform::production_outlook_adapter(),
        ),
        process_session_id: Mutex::new("epoch-initial".to_string()),
        app_data_dir: temporary.path().to_path_buf(),
        scan_index: ScanIndex::new(temporary.path().join("scan-index.sqlite3")).unwrap(),
    }
}

fn signed_in_state(owner_id: &str) -> crate::contracts::AuthState {
    crate::contracts::AuthState {
        signed_in: true,
        allowed: true,
        is_admin: false,
        session: Some(crate::contracts::AuthSessionView {
            user_id: owner_id.to_string(),
            email: "user@example.com".to_string(),
            expires_at: i64::MAX,
        }),
        user: None,
    }
}

#[test]
fn rfi_file_group_apply_copies_without_touching_source_and_undoes_created_output() {
    let _guard = WORK_ITEM_TEST_LOCK.lock().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let source_file = source.join("AZE-SUN-INT-QMS-RFI-CIV-1263.pdf");
    fs::write(&source_file, b"source stays unchanged").unwrap();
    let source_mail = source.join("AZE-SUN-INT-QMS-RFI-CIV-1263.eml");
    fs::write(&source_mail, b"Subject: RFI package").unwrap();
    let source_selection = backend
        .register_selected_paths(rfi_file_group::OPERATION_ID, "source", vec![source.clone()])
        .unwrap()
        .remove(0);
    let destination_selection = backend
        .register_selected_paths(
            rfi_file_group::OPERATION_ID,
            "destination",
            vec![destination.clone()],
        )
        .unwrap()
        .remove(0);

    let plan = backend
        .preview_rfi_file_group(
            json!({
                "opId": rfi_file_group::OPERATION_ID,
                "values": {
                    "source": source_selection,
                    "destination": destination_selection
                }
            }),
            |_| {},
        )
        .unwrap();
    let (result, operation_id) = backend
        .apply_native_plan(
            plan["planId"].as_str().unwrap(),
            Some(rfi_file_group::OPERATION_ID),
            |_| {},
        )
        .unwrap();
    assert_eq!(operation_id, rfi_file_group::OPERATION_ID);
    let copied_directory = destination.join("CIV").join("AZE-SUN-INT-QMS-RFI-CIV-1263");
    let copied = copied_directory.join(source_file.file_name().unwrap());
    let copied_mail = copied_directory.join(source_mail.file_name().unwrap());
    assert_eq!(fs::read(&source_file).unwrap(), b"source stays unchanged");
    assert_eq!(fs::read(&copied).unwrap(), b"source stays unchanged");
    assert_eq!(fs::read(&copied_mail).unwrap(), b"Subject: RFI package");

    backend
        .try_native_undo(&json!({"journalId": result["journalId"]}))
        .unwrap()
        .expect("RFI file grouping should be undoable");
    assert!(source_file.exists());
    assert!(source_mail.exists());
    assert!(!copied.exists());
    assert!(!copied_mail.exists());
    assert!(!copied_directory.exists());
}

#[test]
fn caller_identity_never_falls_back_to_an_authorized_anonymous_identity() {
    let signed_out = crate::contracts::AuthState {
        signed_in: false,
        allowed: false,
        is_admin: false,
        session: None,
        user: None,
    };
    assert_eq!(
        authenticated_caller_identity(&signed_out, "session")
            .unwrap_err()
            .code,
        "auth_required"
    );
    assert_eq!(
        authenticated_caller_identity(&signed_in_state("owner"), "session").unwrap(),
        CallerIdentity::new("owner", "session")
    );
}

#[test]
fn follow_up_preview_values_accept_only_exact_snake_case_keys() {
    let exact: RfiFollowUpPreviewValues = decode(json!({
        "rfi_log_workbook": "/source.xlsx",
        "follow_up_workbook": "/target.xlsx"
    }))
    .unwrap();
    assert_eq!(exact.rfi_log_workbook, PathBuf::from("/source.xlsx"));
    assert_eq!(exact.follow_up_workbook, PathBuf::from("/target.xlsx"));
    assert!(
        decode::<RfiFollowUpPreviewValues>(json!({
            "rfiLogWorkbook": "/source.xlsx",
            "followUpWorkbook": "/target.xlsx"
        }))
        .is_err()
    );
    assert!(
        decode::<RfiFollowUpPreviewValues>(json!({
            "rfi_log_workbook": "/source.xlsx",
            "follow_up_workbook": "/target.xlsx",
            "unknown": true
        }))
        .is_err()
    );
}

#[test]
fn sign_out_and_re_sign_in_revoke_old_plan_and_selection_ids() {
    let temporary = tempfile::tempdir().unwrap();
    let old = CallerIdentity::new("owner", "session-old");
    let fresh = CallerIdentity::new("owner", "session-fresh");
    let selections = PathSelectionStore::new(Duration::from_secs(60));
    let selected = selections
        .issue(
            &old,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();
    let plans = crate::plan_store::PlanStore::new(Duration::from_secs(60));
    let plan = plans
        .insert_bound(
            BackendOrigin::Rust,
            PlanBinding::new("owner", "session-old", "rfi.review", Vec::new()),
            crate::contracts::Plan {
                plan_id: "backend-plan".to_string(),
                op_id: "rfi.review".to_string(),
                steps: Vec::new(),
                warnings: Vec::new(),
                meta: Map::new(),
            },
        )
        .unwrap();

    selections.revoke(&old);
    plans.revoke(&old);

    assert_eq!(
        selections
            .resolve_scope(
                &selected.selection_id,
                &fresh,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
    assert_eq!(
        plans.resolve_bound(&plan.plan_id, &fresh).unwrap_err().code,
        "plan_not_found_or_expired"
    );
}

#[test]
fn attempted_and_same_uid_sign_in_replacements_rotate_and_revoke_capabilities() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let initial = backend.caller_identity().unwrap();
    let selected = backend
        .selections
        .issue(
            &initial,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();
    let grant = backend
        .filesystem
        .issue_grant(
            &GrantScope::new(&initial.owner_id, &initial.session_id, "request", "op.test"),
            temporary.path(),
            GrantAccess::ReadWrite,
        )
        .unwrap();
    let plan = backend
        .filesystem
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &initial.owner_id,
                &initial.session_id,
                "op.test",
                vec![grant.grant_id],
            ),
            crate::contracts::Plan {
                plan_id: "ignored".to_string(),
                op_id: "op.test".to_string(),
                steps: Vec::new(),
                warnings: Vec::new(),
                meta: Map::new(),
            },
        )
        .unwrap();

    assert_eq!(
        backend
            .dispatch(
                BackendMethod::AuthSignIn,
                json!({"identifier": "not-an-email", "password": "password"}),
            )
            .unwrap_err()
            .code,
        "auth_failed"
    );
    let after_attempt = backend.caller_identity().unwrap();
    assert_ne!(after_attempt.session_id, initial.session_id);
    assert_eq!(
        backend
            .selections
            .resolve_scope(
                &selected.selection_id,
                &after_attempt,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
    assert_eq!(
        backend
            .filesystem
            .apply(&plan.plan_id, &after_attempt)
            .unwrap_err()
            .code,
        "plan_not_found_or_expired"
    );

    let selected_after_attempt = backend
        .selections
        .issue(
            &after_attempt,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();
    let grant_after_attempt = backend
        .filesystem
        .issue_grant(
            &GrantScope::new(
                &after_attempt.owner_id,
                &after_attempt.session_id,
                "request-after-attempt",
                "op.test",
            ),
            temporary.path(),
            GrantAccess::ReadWrite,
        )
        .unwrap();
    let plan_after_attempt = backend
        .filesystem
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &after_attempt.owner_id,
                &after_attempt.session_id,
                "op.test",
                vec![grant_after_attempt.grant_id],
            ),
            crate::contracts::Plan {
                plan_id: "ignored-again".to_string(),
                op_id: "op.test".to_string(),
                steps: Vec::new(),
                warnings: Vec::new(),
                meta: Map::new(),
            },
        )
        .unwrap();

    backend
        .dispatch(
            BackendMethod::AuthSignIn,
            json!({"identifier": "user@example.com", "password": "password"}),
        )
        .unwrap();
    let after_success = backend.caller_identity().unwrap();
    assert_eq!(after_success.owner_id, initial.owner_id);
    assert_ne!(after_success.session_id, after_attempt.session_id);
    assert_eq!(
        backend
            .selections
            .resolve_scope(
                &selected_after_attempt.selection_id,
                &after_success,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
    assert_eq!(
        backend
            .filesystem
            .apply(&plan_after_attempt.plan_id, &after_success)
            .unwrap_err()
            .code,
        "plan_not_found_or_expired"
    );
}

#[test]
fn disallowed_same_uid_reauthentication_cannot_revive_old_capabilities() {
    let temporary = tempfile::tempdir().unwrap();
    let responses = auth_responses("owner")
        .into_iter()
        .chain([Ok(HttpResponse::from_json(
            200,
            &json!([{
                "id": "owner",
                "email": "user@example.com",
                "allowed": false,
                "is_admin": false
            }]),
        ))])
        .chain(auth_responses("owner"))
        .collect();
    let backend = test_backend_with_responses(&temporary, responses);
    let initial = backend.caller_identity().unwrap();
    let selected = backend
        .selections
        .issue(
            &initial,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();

    let denied = backend.auth.load_me(now()).unwrap();
    assert!(denied.signed_in);
    assert!(!denied.allowed);
    backend
        .dispatch(
            BackendMethod::AuthSignIn,
            json!({"identifier": "user@example.com", "password": "password"}),
        )
        .unwrap();

    let replacement = backend.caller_identity().unwrap();
    assert_ne!(replacement.session_id, initial.session_id);
    assert_eq!(
        backend
            .selections
            .resolve_scope(
                &selected.selection_id,
                &replacement,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
}

#[test]
fn dispatch_sign_out_stays_revoked_when_settings_cleanup_fails() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let initial = backend.caller_identity().unwrap();
    let initial_epoch = initial.session_id.clone();
    let selected = backend
        .selections
        .issue(
            &initial,
            "rfi.review",
            "destination",
            temporary.path(),
            SelectionKind::Folder,
        )
        .unwrap();

    let settings_path = temporary.path().join("settings.json");
    std::fs::remove_file(&settings_path).unwrap();
    std::fs::create_dir(&settings_path).unwrap();

    assert_eq!(
        backend
            .dispatch(BackendMethod::AuthSignOut, json!({}))
            .unwrap_err()
            .code,
        "settings_write_failed"
    );
    assert!(!backend.auth_state().signed_in);
    assert_ne!(
        *backend
            .process_session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        initial_epoch
    );
    assert_eq!(
        backend
            .selections
            .resolve_scope(
                &selected.selection_id,
                &initial,
                SelectionScope::ReviewDestination,
            )
            .unwrap_err()
            .code,
        "path_grant_not_found_or_expired"
    );
}

#[test]
fn settings_locale_distinguishes_omission_from_explicit_null() {
    let omitted: SettingsUpdate = decode(json!({})).expect("omitted locale should decode");
    let cleared: SettingsUpdate =
        decode(json!({"locale": null})).expect("null locale should decode");

    assert_eq!(omitted.locale, PatchField::Missing);
    assert_eq!(cleared.locale, PatchField::Value(None));
}

#[test]
fn saved_operation_params_are_schema_filtered_and_never_include_secrets() {
    let filtered = filter_saved_operation_params(
        "rfi.log_add",
        serde_json::from_value(json!({
            "email_provider": "gemini",
            "gemini_api_key": "must-not-persist",
            "unknown": "must-not-persist"
        }))
        .unwrap(),
    )
    .expect("known operation should filter saved params");

    assert_eq!(filtered.get("email_provider"), Some(&json!("gemini")));
    assert!(!filtered.contains_key("gemini_api_key"));
    assert!(!filtered.contains_key("unknown"));

    let print = filter_saved_operation_params(
        "files.batch_print",
        serde_json::from_value(json!({
            "files": ["C:/stale/old-job.pdf"],
            "printer": "EPSON L15160 Series",
            "paper": "A3",
            "copies": 2
        }))
        .unwrap(),
    )
    .expect("print settings should exclude task-local file selections");
    assert!(!print.contains_key("files"));
    assert_eq!(print.get("printer"), Some(&json!("EPSON L15160 Series")));
    assert_eq!(print.get("paper"), Some(&json!("A3")));
    assert_eq!(print.get("copies"), Some(&json!(2)));
}

#[test]
fn settings_presets_commands_are_capability_checked_filtered_and_typed() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);

    let saved = backend
        .dispatch(
            BackendMethod::SettingsSavePreset,
            json!({
                "opId": "files.batch_print",
                "name": "Office",
                "values": {
                    "paper": "A4",
                    "copies": 2,
                    "collate": true,
                    "files": [{"selectionId": "forged"}],
                    "email_text": "must not persist",
                    "nested": {"bad": true}
                }
            }),
        )
        .unwrap();
    assert_eq!(saved["name"], "Office");
    assert_eq!(
        saved["values"],
        json!({
            "paper": "A4",
            "copies": 2,
            "collate": true
        })
    );

    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsSavePreset,
                json!({
                    "opId": "files.batch_print",
                    "name": "office",
                    "values": {"copies": 3}
                }),
            )
            .unwrap_err()
            .message,
        "error.preset.duplicate"
    );
    let replaced = backend
        .dispatch(
            BackendMethod::SettingsSavePreset,
            json!({
                "opId": "files.batch_print",
                "name": "office",
                "values": {"copies": 3},
                "replace": true
            }),
        )
        .unwrap();
    assert_eq!(replaced["id"], saved["id"]);
    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsPresets,
                json!({"opId": "files.batch_print"}),
            )
            .unwrap(),
        json!([replaced])
    );

    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsSavePreset,
                json!({
                    "opId": "files.batch_print",
                    "name": "Bad choice",
                    "values": {"paper": "Tabloid"}
                }),
            )
            .unwrap_err()
            .code,
        "validation"
    );
    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsPresets,
                json!({"opId": "rfi.process_scans"}),
            )
            .unwrap_err()
            .code,
        "validation"
    );

    backend
        .dispatch(
            BackendMethod::SettingsDeletePreset,
            json!({
                "opId": "files.batch_print",
                "presetId": saved["id"]
            }),
        )
        .unwrap();
    assert_eq!(
        backend
            .dispatch(
                BackendMethod::SettingsPresets,
                json!({"opId": "files.batch_print"}),
            )
            .unwrap(),
        json!([])
    );
}

#[test]
fn settings_save_preset_sanitizes_existing_stored_values_before_rewrite() {
    let temporary = tempfile::tempdir().unwrap();
    let settings_path = temporary.path().join("settings.json");
    let existing_id = uuid::Uuid::new_v4().to_string();
    std::fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "operationPresets": {
                "files.batch_print": [{
                    "id": existing_id.clone(),
                    "name": "Stored",
                    "values": {
                        "paper": "A4",
                        "copies": 2,
                        "collate": true,
                        "files": [{"selectionId": "forged"}],
                        "email_text": "must not persist",
                        "gemini_api_key": "must not persist",
                        "retries": "3",
                        "duplex": "sideways",
                        "unknownArray": [1, 2, 3],
                        "unknownObject": {"bad": true}
                    }
                }]
            },
            "custom": {"keep": true}
        }))
        .unwrap(),
    )
    .unwrap();
    let backend = test_backend(&temporary);

    backend
        .dispatch(
            BackendMethod::SettingsSavePreset,
            json!({
                "opId": "files.batch_print",
                "name": "New",
                "values": {"copies": 1}
            }),
        )
        .unwrap();

    let persisted: Value = serde_json::from_slice(&std::fs::read(settings_path).unwrap()).unwrap();
    let stored = persisted["operationPresets"]["files.batch_print"]
        .as_array()
        .unwrap()
        .iter()
        .find(|preset| preset["id"] == existing_id)
        .unwrap();
    assert_eq!(
        stored["values"],
        json!({
            "paper": "A4",
            "copies": 2,
            "collate": true
        })
    );
    assert_eq!(persisted["custom"], json!({"keep": true}));
}

#[test]
fn saved_operation_params_preserve_paths_and_reissue_active_selections_for_caller() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);

    let source_dir = temporary.path().join("source");
    fs::create_dir(&source_dir).unwrap();
    let dest_dir = temporary.path().join("dest");
    fs::create_dir(&dest_dir).unwrap();

    let params: Map<String, Value> = serde_json::from_value(json!({
        "scan_mode": "multi_rfi",
        "source": source_dir.to_str().unwrap(),
        "dest": dest_dir.to_str().unwrap(),
        "organize_into_rfi_folders": false,
        "keep_original": true,
        "secret_key": "forbidden",
        "unknown": "drop-me"
    }))
    .unwrap();

    let filtered = filter_saved_operation_params("rfi.process_scans_fast_covers", params)
        .expect("known operation should filter saved params");
    assert_eq!(
        filtered.get("source"),
        Some(&json!(source_dir.to_str().unwrap()))
    );
    assert_eq!(
        filtered.get("dest"),
        Some(&json!(dest_dir.to_str().unwrap()))
    );
    assert_eq!(filtered.get("keep_original"), Some(&json!(true)));
    assert_eq!(
        filtered.get("organize_into_rfi_folders"),
        Some(&json!(false))
    );
    assert!(!filtered.contains_key("secret_key"));
    assert!(!filtered.contains_key("unknown"));

    backend
        .settings
        .save_params("rfi.process_scans_fast_covers", filtered)
        .unwrap();

    let restored = backend
        .selected_params("rfi.process_scans_fast_covers")
        .unwrap();
    let source_selection: PathSelection =
        serde_json::from_value(restored.get("source").unwrap().clone()).unwrap();
    let dest_selection: PathSelection =
        serde_json::from_value(restored.get("dest").unwrap().clone()).unwrap();

    assert_eq!(
        source_selection.display_path,
        fs::canonicalize(&source_dir)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(
        dest_selection.display_path,
        fs::canonicalize(&dest_dir)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    );

    backend
        .settings
        .save_params(
            "rfi.process_scans_fast_covers",
            serde_json::from_value(json!({
                "source": "/non/existent/path/never/created",
                "dest": dest_dir.to_str().unwrap(),
                "keep_original": true
            }))
            .unwrap(),
        )
        .unwrap();
    let restored2 = backend
        .selected_params("rfi.process_scans_fast_covers")
        .unwrap();
    assert!(!restored2.contains_key("source"));
    assert!(restored2.contains_key("dest"));
    assert_eq!(restored2.get("keep_original"), Some(&json!(true)));
}

#[test]
fn process_scans_source_grant_allows_review_moves_even_in_copy_mode() {
    assert_eq!(process_scans_source_access(), GrantAccess::ReadWrite);
}

#[test]
fn cloud_summary_preserves_legacy_journal_step_count() {
    assert_eq!(cloud_result_summary(1, 3), "1/3");
}

#[test]
fn batch_print_audit_result_finds_and_pushes_the_nested_journal_id() {
    let mut result = json!({
        "applied": 2,
        "failed": 1,
        "meta": {"auditJournalId": "print-audit-1"}
    });
    let mut pushed = None;

    sync_result_metadata(
        &mut result,
        |journal_id| (journal_id == "print-audit-1").then_some(3),
        |applied, failed, journal_id, summary| {
            pushed = Some((applied, failed, journal_id.to_string(), summary.to_string()));
            true
        },
    );

    assert_eq!(
        pushed,
        Some((2, 1, "print-audit-1".to_string(), "2/3".to_string()))
    );
    assert_eq!(result["cloudSynced"], true);
}

// This path needs the native PDF page renderer; Linux intentionally lacks it.
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn preview_rfi_system_sync_with_nested_workbooks_succeeds_and_applies() {
    use std::io::{Cursor, Write};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn create_xlsx(sheets: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let mut workbook_sheets = String::new();
        let mut relationships = String::new();
        for (index, (name, content)) in sheets.iter().enumerate() {
            let ordinal = index + 1;
            workbook_sheets.push_str(&format!(
                r#"<sheet name="{name}" sheetId="{ordinal}" r:id="rId{ordinal}"/>"#
            ));
            relationships.push_str(&format!(
                r#"<Relationship Id="rId{ordinal}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{ordinal}.xml"/>"#
            ));
            writer
                .start_file(format!("xl/worksheets/sheet{ordinal}.xml"), options)
                .unwrap();
            let ws = format!(
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{content}</sheetData></worksheet>"#
            );
            writer.write_all(ws.as_bytes()).unwrap();
        }
        let pkg_rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
        let wb = format!(
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{workbook_sheets}</sheets></workbook>"#
        );
        let rels = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}</Relationships>"#
        );
        for (name, bytes) in [
            ("[Content_Types].xml", b"content-types".as_slice()),
            ("_rels/.rels", pkg_rels.as_bytes()),
            ("xl/workbook.xml", wb.as_bytes()),
            ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let temp_path = fs::canonicalize(temporary.path()).unwrap();
    let system_root = temp_path.join("RFI_SYSTEM");
    let civ_dir = system_root.join("CIV");
    fs::create_dir_all(&civ_dir).unwrap();

    let civ_103_dir = civ_dir.join("AZE-SUN-INT-QMS-RFI-CIV-0103");
    fs::create_dir_all(&civ_103_dir).unwrap();
    let civ_103_pdf = civ_103_dir.join("RFI No. AZE-SUN-INT-QMS-RFI-CIV-0103.pdf");
    {
        let stream = "BT /F1 18 Tf 470 195 Td (E.Mammadov Signed Worley) Tj ET\n";
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
            bytes
                .extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
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
        fs::write(&civ_103_pdf, bytes).unwrap();
    }

    // Place workbooks INSIDE system_root to test nested grant handling
    let rfi_log_path = system_root.join("RFI_Log.xlsx");
    let follow_up_path = system_root.join("Follow_Up.xlsx");

    let civ_log_row = r#"<row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="V9" t="inlineStr"><is><t>REMARKS</t></is></c></row><row r="10"><c r="A10" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0103</t></is></c><c r="V10" t="inlineStr"><is><t></t></is></c></row>"#;
    let mec_log_header = r#"<row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="W9" t="inlineStr"><is><t>REMARKS</t></is></c></row>"#;
    let ele_log_header = r#"<row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="V9" t="inlineStr"><is><t>REMARKS</t></is></c></row>"#;
    let prc_log_header = r#"<row r="9"><c r="A9" t="inlineStr"><is><t>RFI No.</t></is></c><c r="W9" t="inlineStr"><is><t>REMARKS</t></is></c></row>"#;
    let rfi_log_bytes = create_xlsx(&[
        ("CIV", civ_log_row),
        ("MEC", mec_log_header),
        ("ELE", ele_log_header),
        ("PRC", prc_log_header),
    ]);
    fs::write(&rfi_log_path, &rfi_log_bytes).unwrap();

    let fu_civ_row = r#"<row r="4"><c r="C4" t="inlineStr"><is><t>RFI No.</t></is></c><c r="D4" t="inlineStr"><is><t>STATUS</t></is></c></row><row r="5"><c r="C5" t="inlineStr"><is><t>CIV-0103</t></is></c><c r="D5" t="inlineStr"><is><t>Not Signed</t></is></c></row>"#;
    let fu_mec_header = r#"<row r="3"><c r="S3" t="inlineStr"><is><t>RFI No.</t></is></c><c r="T3" t="inlineStr"><is><t>Worley Status</t></is></c></row>"#;
    let fu_bytes = create_xlsx(&[
        ("Signed By Worley CIV", fu_civ_row),
        ("Signed By Worley MEC", fu_mec_header),
    ]);
    fs::write(&follow_up_path, &fu_bytes).unwrap();

    let caller = backend.caller_identity().unwrap();
    let system_root_sel = backend
        .selections
        .issue(
            &caller,
            "rfi.system_sync",
            "rfi_system_root",
            &system_root,
            SelectionKind::Folder,
        )
        .unwrap();
    let rfi_log_sel = backend
        .selections
        .issue(
            &caller,
            "rfi.system_sync",
            "rfi_log_workbook",
            &rfi_log_path,
            SelectionKind::File,
        )
        .unwrap();
    let fu_sel = backend
        .selections
        .issue(
            &caller,
            "rfi.system_sync",
            "follow_up_workbook",
            &follow_up_path,
            SelectionKind::File,
        )
        .unwrap();

    let params = json!({
        "opId": "rfi.system_sync",
        "values": {
            "rfi_system_root": system_root_sel,
            "rfi_log_workbook": rfi_log_sel,
            "follow_up_workbook": fu_sel,
        }
    });

    let preview_result = backend
        .preview_rfi_system_sync(params, |_| {})
        .expect("preview must not fail with path grant error");
    assert_eq!(preview_result["opId"], "rfi.system_sync");
    assert!(
        preview_result["steps"]
            .as_array()
            .unwrap()
            .iter()
            .all(|step| {
                step["meta"]["syncStep"]["canonicalRfiCodes"]
                    == json!(["AZE-SUN-INT-QMS-RFI-CIV-0103"])
            })
    );
    let plan_id = preview_result["planId"].as_str().unwrap();

    // Apply
    let apply_result = backend
        .filesystem
        .apply_with_progress(plan_id, &caller, |_| {})
        .expect("apply must succeed");
    assert_eq!(apply_result.applied, 2);
    assert_eq!(apply_result.failed, 0);
    // Undo
    let undo_result = backend
        .filesystem
        .undo(&apply_result.journal_id, &caller, None)
        .expect("undo must succeed");
    assert_eq!(undo_result.applied, 4);
    assert_eq!(undo_result.failed, 0);
}

#[test]
fn rfi_print_finder_issues_exact_batch_print_file_selections() {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let root = temporary.path().join("RFI System");
    let code = "AZE-SUN-INT-QMS-RFI-CIV-1319";
    let duplicate = root.join("CIV/old").join(format!("{code}.docx"));
    let first = root.join("CIV").join(format!("{code}.docx"));
    let sketch = root.join("CIV").join(format!("{code} Sketch.pdf"));
    for path in [&duplicate, &first, &sketch] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }
    let caller = backend.caller_identity().unwrap();
    let root_selection = backend
        .selections
        .issue(
            &caller,
            "files.batch_print",
            "rfi_system_root",
            &root,
            SelectionKind::Folder,
        )
        .unwrap();
    let result = backend
        .find_rfi_print_files(json!({
            "root": root_selection,
            "numbers": ["CIV-1319"],
            "firstPage": true,
            "sketch": true,
        }))
        .unwrap();
    let candidates = result["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 3);
    let first_pages = candidates
        .iter()
        .filter(|candidate| candidate["documentType"] == "firstPage")
        .collect::<Vec<_>>();
    let sketches = candidates
        .iter()
        .filter(|candidate| candidate["documentType"] == "sketch")
        .collect::<Vec<_>>();
    assert_eq!(first_pages.len(), 2);
    assert!(
        first_pages
            .iter()
            .all(|candidate| candidate["selected"] == false)
    );
    assert_eq!(sketches.len(), 1);
    assert_eq!(sketches[0]["selected"], true);
    for candidate in candidates {
        let selection: PathSelection =
            serde_json::from_value(candidate["selection"].clone()).unwrap();
        let path = backend
            .selections
            .resolve(
                &selection,
                &caller,
                "files.batch_print",
                "files",
                SelectionKind::File,
            )
            .unwrap();
        assert!(path.starts_with(fs::canonicalize(&root).unwrap()));
    }
}

#[test]
fn finder_revokes_an_issued_selection_outside_the_selected_root() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("selected-root");
    fs::create_dir(&root).unwrap();
    let outside = temporary.path().join("outside.pdf");
    fs::write(&outside, b"outside").unwrap();
    let caller = CallerIdentity::new("owner", "session");
    let selections = PathSelectionStore::new(Duration::from_secs(60));
    let selection = selections
        .issue(
            &caller,
            "files.batch_print",
            "files",
            &outside,
            SelectionKind::File,
        )
        .unwrap();

    let error = resolve_scoped_rfi_print_file(
        &selections,
        &selection,
        &caller,
        &fs::canonicalize(root).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.code, "validation");
    assert_eq!(error.message, "error.print.rfi.rootEscape");
    assert!(
        selections
            .resolve(
                &selection,
                &caller,
                "files.batch_print",
                "files",
                SelectionKind::File,
            )
            .is_err()
    );
}

#[test]
#[ignore = "set DOCFLOW_VENTOY_RFI_PRINT_FIND_ROOT to a private Ventoy RFI System copy"]
fn operator_ventoy_rfi_print_finder_hands_off_scoped_files_without_source_changes() {
    let root = PathBuf::from(
        std::env::var_os("DOCFLOW_VENTOY_RFI_PRINT_FIND_ROOT")
            .expect("DOCFLOW_VENTOY_RFI_PRINT_FIND_ROOT is required"),
    );
    let number = std::env::var("DOCFLOW_VENTOY_RFI_PRINT_FIND_NUMBER")
        .unwrap_or_else(|_| "CIV-1316".to_string());
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let caller = backend.caller_identity().unwrap();
    let canonical_root = fs::canonicalize(&root).expect("private RFI System copy");
    let root_selection = backend
        .selections
        .issue(
            &caller,
            "files.batch_print",
            "rfi_system_root",
            &root,
            SelectionKind::Folder,
        )
        .unwrap();
    let result = backend
        .find_rfi_print_files(json!({
            "root": root_selection,
            "numbers": [number],
            "firstPage": true,
            "sketch": true,
        }))
        .expect("native finder should discover real Ventoy files");
    let candidates = result["candidates"].as_array().unwrap();
    assert!(
        !candidates.is_empty(),
        "selected private copy must contain the requested RFI"
    );
    let mut source_hashes = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let selection: PathSelection =
            serde_json::from_value(candidate["selection"].clone()).unwrap();
        let path = backend
            .selections
            .resolve(
                &selection,
                &caller,
                "files.batch_print",
                "files",
                SelectionKind::File,
            )
            .unwrap();
        assert_eq!(
            path,
            resolve_scoped_rfi_print_file(
                &backend.selections,
                &selection,
                &caller,
                &canonical_root,
            )
            .unwrap(),
        );
        assert!(path.starts_with(&canonical_root));
        source_hashes.push((path.clone(), crate::fs_ops::file_sha256(&path).unwrap()));
    }
    for (path, hash) in source_hashes {
        assert_eq!(crate::fs_ops::file_sha256(&path).unwrap(), hash);
    }
}
