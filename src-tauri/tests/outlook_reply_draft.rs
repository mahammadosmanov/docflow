use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use docflow_lib::contracts::StepKind;
use docflow_lib::journal::JournalState;
use docflow_lib::outlook_reply_draft::{
    DraftIdentity, MailItemIdentity, OPERATION_ID, OutlookAdapter, OutlookDraft,
    OutlookDraftService, OutlookFailure, ReplyContent, SelectedMailItem, apply_reply_all,
    build_reply_content,
};
use docflow_lib::paths::CallerIdentity;
use docflow_lib::progress::GlobalMutationLock;
use docflow_lib::registry::{RegistryFacts, operation_registry};

#[derive(Clone)]
struct FakeOutlook {
    selected: Arc<Mutex<MailItemIdentity>>,
    calls: Arc<Mutex<Vec<&'static str>>>,
    durable: Option<DraftIdentity>,
    current_draft: Arc<Mutex<Option<DraftIdentity>>>,
    deleted: Arc<Mutex<Vec<DraftIdentity>>>,
}

impl OutlookAdapter for FakeOutlook {
    fn selected_mail_item(&self) -> Result<SelectedMailItem, OutlookFailure> {
        Ok(SelectedMailItem {
            identity: self.selected.lock().unwrap().clone(),
            subject: "RFI response".to_string(),
        })
    }

    fn reply_all(
        &self,
        expected: &MailItemIdentity,
    ) -> Result<Box<dyn OutlookDraft>, OutlookFailure> {
        if expected != &*self.selected.lock().unwrap() {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookDraft.selectionChanged",
            ));
        }
        self.calls.lock().unwrap().push("ReplyAll");
        Ok(Box::new(FakeDraft {
            calls: Arc::clone(&self.calls),
            durable: self.durable.clone(),
        }))
    }

    fn delete_unchanged_draft(&self, draft: &DraftIdentity) -> Result<(), OutlookFailure> {
        if self.current_draft.lock().unwrap().as_ref() != Some(draft) {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookDraft.draftMissingOrChanged",
            ));
        }
        self.calls.lock().unwrap().push("Delete");
        self.deleted.lock().unwrap().push(draft.clone());
        Ok(())
    }
}

struct FakeDraft {
    calls: Arc<Mutex<Vec<&'static str>>>,
    durable: Option<DraftIdentity>,
}

impl OutlookDraft for FakeDraft {
    fn prepend_body(&mut self, _content: &ReplyContent) -> Result<(), OutlookFailure> {
        self.calls.lock().unwrap().push("prepend_body");
        Ok(())
    }

    fn display(&mut self) -> Result<(), OutlookFailure> {
        self.calls.lock().unwrap().push("Display");
        Ok(())
    }

    fn durable_identity(&mut self) -> Option<DraftIdentity> {
        self.durable.clone()
    }
}

fn mail_identity(entry_id: &str) -> MailItemIdentity {
    MailItemIdentity {
        entry_id: entry_id.to_string(),
        store_id: "store-1".to_string(),
    }
}

#[test]
fn reply_content_is_bounded_and_html_escapes_every_user_character() {
    let content = build_reply_content("Hello <script>alert(\"x\")</script> & goodbye\nnext")
        .expect("bounded reply text should preview");

    assert_eq!(
        content.plain_text,
        "Hello <script>alert(\"x\")</script> & goodbye\r\nnext"
    );
    assert_eq!(
        content.html,
        "<div>Hello &lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; goodbye<br>next</div><br><br>"
    );
    assert!(!content.html.contains("<script>"));

    let oversized = "x".repeat(10_001);
    let error = build_reply_content(&oversized).expect_err("reply text must be bounded");
    assert_eq!(error.key, "error.rfi.outlookDraft.replyTooLong");
}

#[test]
fn apply_uses_only_reply_all_then_body_prepend_then_display_and_returns_unsent_draft() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let durable = DraftIdentity {
        entry_id: "draft-entry".to_string(),
        store_id: "store-1".to_string(),
        fingerprint: "draft-fingerprint".to_string(),
    };
    let outlook = FakeOutlook {
        selected: Arc::new(Mutex::new(mail_identity("source-entry"))),
        calls: Arc::clone(&calls),
        durable: Some(durable.clone()),
        current_draft: Arc::new(Mutex::new(Some(durable.clone()))),
        deleted: Arc::new(Mutex::new(Vec::new())),
    };
    let content = build_reply_content("Approved response").unwrap();

    let outcome = apply_reply_all(&outlook, &mail_identity("source-entry"), &content)
        .expect("the fake Outlook draft should be displayed");

    assert_eq!(outcome.durable_identity, Some(durable));
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        ["ReplyAll", "prepend_body", "Display"]
    );
    assert!(calls.lock().unwrap().iter().all(|call| *call != "Send"));
}

#[test]
fn apply_refuses_identity_drift_before_reply_all_has_any_effect() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let outlook = FakeOutlook {
        selected: Arc::new(Mutex::new(mail_identity("new-selection"))),
        calls: Arc::clone(&calls),
        durable: None,
        current_draft: Arc::new(Mutex::new(None)),
        deleted: Arc::new(Mutex::new(Vec::new())),
    };
    let content = build_reply_content("Approved response").unwrap();

    let error = apply_reply_all(&outlook, &mail_identity("previewed-selection"), &content)
        .expect_err("a changed selection must be refused");

    assert_eq!(error.key, "error.rfi.outlookDraft.selectionChanged");
    assert!(calls.lock().unwrap().is_empty());
}

fn caller() -> CallerIdentity {
    CallerIdentity::new("operator-1", "session-1")
}

#[test]
fn service_journals_a_durable_unsent_draft_and_undo_deletes_that_exact_unchanged_item() {
    let fixture = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let deleted = Arc::new(Mutex::new(Vec::new()));
    let durable = DraftIdentity {
        entry_id: "draft-entry".to_string(),
        store_id: "store-1".to_string(),
        fingerprint: "draft-fingerprint".to_string(),
    };
    let outlook = Arc::new(FakeOutlook {
        selected: Arc::new(Mutex::new(mail_identity("source-entry"))),
        calls: Arc::clone(&calls),
        durable: Some(durable.clone()),
        current_draft: Arc::new(Mutex::new(Some(durable.clone()))),
        deleted: Arc::clone(&deleted),
    });
    let service = OutlookDraftService::new(
        fixture.path(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        outlook,
    );

    let plan = service.preview(&caller(), "Approved response").unwrap();
    assert_eq!(plan.op_id, OPERATION_ID);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].kind, StepKind::OutlookReplyDraft);
    assert_eq!(plan.meta["replyPreview"]["plainText"], "Approved response");

    let result = service
        .apply(&plan.plan_id, Some(OPERATION_ID), &caller(), |_| {})
        .unwrap();
    assert_eq!(result.applied, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.meta["draftDisplayed"], true);
    assert_eq!(result.meta["undoAvailable"], true);

    let journal = service.journals().read(&result.journal_id).unwrap();
    assert_eq!(journal.state, Some(JournalState::Committed));
    assert_eq!(journal.steps.len(), 1);
    assert_eq!(journal.steps[0].meta["draftDurable"], true);
    assert_eq!(journal.steps[0].meta["draftEntryId"], "draft-entry");
    assert_eq!(journal.steps[0].meta["draftStoreId"], "store-1");
    assert_eq!(
        journal.steps[0].meta["draftFingerprint"],
        "draft-fingerprint"
    );

    let undo = service.undo(&result.journal_id, &caller()).unwrap();
    assert_eq!(undo.applied, 1);
    assert_eq!(undo.failed, 0);
    assert_eq!(deleted.lock().unwrap().as_slice(), [durable]);
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        ["ReplyAll", "prepend_body", "Display", "Delete"]
    );
}

#[test]
fn service_refuses_undo_when_outlook_did_not_auto_save_a_durable_draft() {
    let fixture = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let outlook = Arc::new(FakeOutlook {
        selected: Arc::new(Mutex::new(mail_identity("source-entry"))),
        calls: Arc::clone(&calls),
        durable: None,
        current_draft: Arc::new(Mutex::new(None)),
        deleted: Arc::new(Mutex::new(Vec::new())),
    });
    let service = OutlookDraftService::new(
        fixture.path(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        outlook,
    );
    let plan = service.preview(&caller(), "Approved response").unwrap();
    let result = service
        .apply(&plan.plan_id, Some(OPERATION_ID), &caller(), |_| {})
        .unwrap();

    assert_eq!(result.meta["undoAvailable"], false);
    assert_eq!(result.meta["nonUndoable"], true);
    let error = service
        .undo(&result.journal_id, &caller())
        .expect_err("a non-durable displayed item must never be deleted by undo");
    assert_eq!(error.message, "error.rfi.outlookDraft.undoUnavailable");
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        ["ReplyAll", "prepend_body", "Display"]
    );
}

#[test]
fn service_revalidates_the_exact_selected_identity_at_apply() {
    let fixture = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let selected = Arc::new(Mutex::new(mail_identity("source-entry")));
    let outlook = Arc::new(FakeOutlook {
        selected: Arc::clone(&selected),
        calls: Arc::clone(&calls),
        durable: None,
        current_draft: Arc::new(Mutex::new(None)),
        deleted: Arc::new(Mutex::new(Vec::new())),
    });
    let service = OutlookDraftService::new(
        fixture.path(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        outlook,
    );
    let plan = service.preview(&caller(), "Approved response").unwrap();
    *selected.lock().unwrap() = mail_identity("different-entry");

    let result = service
        .apply(&plan.plan_id, Some(OPERATION_ID), &caller(), |_| {})
        .unwrap();

    assert_eq!(result.applied, 0);
    assert_eq!(result.failed, 1);
    assert_eq!(
        result.errors[0].key,
        "error.rfi.outlookDraft.selectionChanged"
    );
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(
        service.journals().read(&result.journal_id).unwrap().state,
        Some(JournalState::NoEffectRetryable)
    );
}

#[test]
fn undo_refuses_a_durable_draft_whose_exact_fingerprint_changed() {
    let fixture = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let durable = DraftIdentity {
        entry_id: "draft-entry".to_string(),
        store_id: "store-1".to_string(),
        fingerprint: "original-fingerprint".to_string(),
    };
    let current_draft = Arc::new(Mutex::new(Some(durable.clone())));
    let outlook = Arc::new(FakeOutlook {
        selected: Arc::new(Mutex::new(mail_identity("source-entry"))),
        calls: Arc::clone(&calls),
        durable: Some(durable),
        current_draft: Arc::clone(&current_draft),
        deleted: Arc::new(Mutex::new(Vec::new())),
    });
    let service = OutlookDraftService::new(
        fixture.path(),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        outlook,
    );
    let plan = service.preview(&caller(), "Approved response").unwrap();
    let result = service
        .apply(&plan.plan_id, Some(OPERATION_ID), &caller(), |_| {})
        .unwrap();
    current_draft.lock().unwrap().as_mut().unwrap().fingerprint =
        "operator-edited-fingerprint".to_string();

    let undo = service.undo(&result.journal_id, &caller()).unwrap();

    assert_eq!(undo.applied, 0);
    assert_eq!(undo.failed, 1);
    assert_eq!(
        undo.errors[0].key,
        "error.rfi.outlookDraft.draftMissingOrChanged"
    );
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        ["ReplyAll", "prepend_body", "Display"]
    );
    assert_eq!(
        service.journals().read(&result.journal_id).unwrap().state,
        Some(JournalState::Committed)
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn production_adapter_is_explicitly_unsupported_off_windows() {
    let adapter = docflow_lib::platform::production_outlook_adapter();
    let error = adapter
        .selected_mail_item()
        .expect_err("Outlook automation must be Windows-only");

    assert_eq!(
        error.kind,
        docflow_lib::outlook_reply_draft::OutlookFailureKind::Unsupported
    );
    assert_eq!(error.key, "error.rfi.outlookDraft.unsupportedPlatform");
}

#[test]
fn windows_adapter_is_closed_direct_com_without_send_recipient_or_process_authority() {
    let source = include_str!("../src/platform/windows/outlook.rs");
    for required in [
        "CoInitializeEx",
        "GetActiveObject",
        "IDispatch",
        "ActiveExplorer",
        "Selection",
        "ReplyAll",
        "HTMLBody",
        "Body",
        "Display",
        "GetItemFromID",
        "Saved",
        "Delete",
    ] {
        assert!(
            source.contains(required),
            "missing closed COM boundary {required}"
        );
    }
    for forbidden in [
        "\"Send\"",
        "Recipients",
        "CreateItem",
        "CoCreateInstance",
        "powershell",
        "PowerShell",
        "vbscript",
        "VBScript",
        "std::process",
        "Command::new",
        "Microsoft Graph",
        "GraphServiceClient",
    ] {
        assert!(
            !source.contains(forbidden),
            "forbidden Outlook authority {forbidden}"
        );
    }
}

#[test]
fn registry_exposes_outlook_draft_as_a_separate_schema_generated_operation() {
    let operations = operation_registry(RegistryFacts::default());
    assert_eq!(
        operations
            .iter()
            .map(|operation| operation.id.as_str())
            .collect::<Vec<_>>(),
        [
            "rfi.process_scans",
            "rfi.group_files",
            "rfi.log_add",
            "rfi.follow_up_sync",
            "rfi.outlook_reply_draft",
            "files.batch_print",
            "rfi.stage_signed_first_pages",
            "rfi.normalize_pdf_orientation",
            "rfi.system_sync",
            "rfi.complete_status_sync",
        ]
    );
    let outlook = &operations[4];
    assert_eq!(outlook.schema.params.len(), 1);
    assert_eq!(outlook.schema.params[0].name, "reply_text");
    assert_eq!(
        outlook.schema.params[0].kind,
        docflow_lib::contracts::ParamKind::LongText
    );
    assert!(outlook.schema.params[0].required);
    assert_eq!(
        outlook.schema.validate(&serde_json::Map::new()),
        ["error.param.required.reply_text"]
    );
}
