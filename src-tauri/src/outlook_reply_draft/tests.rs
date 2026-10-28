use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

struct ArchiveAdapter {
    exported: AtomicBool,
}

impl OutlookAdapter for ArchiveAdapter {
    fn selected_mail_item(&self) -> Result<SelectedMailItem, OutlookFailure> {
        Ok(SelectedMailItem {
            identity: MailItemIdentity {
                entry_id: "entry".to_string(),
                store_id: "store".to_string(),
            },
            subject: "RFI response".to_string(),
            fingerprint: "a".repeat(64),
        })
    }

    fn reply_all(
        &self,
        _expected: &MailItemIdentity,
    ) -> Result<Box<dyn OutlookDraft>, OutlookFailure> {
        Err(OutlookFailure::no_effect("unused"))
    }

    fn delete_unchanged_draft(&self, _draft: &DraftIdentity) -> Result<(), OutlookFailure> {
        Err(OutlookFailure::no_effect("unused"))
    }

    fn export_selected_msg(
        &self,
        expected: &MailItemIdentity,
        expected_fingerprint: &str,
    ) -> Result<Vec<u8>, OutlookFailure> {
        if expected.entry_id != "entry"
            || expected.store_id != "store"
            || expected_fingerprint != "a".repeat(64)
        {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookArchive.selectionChanged",
            ));
        }
        self.exported.store(true, Ordering::SeqCst);
        Ok(b"msg".to_vec())
    }
}

#[test]
fn archive_authority_is_owner_bound_and_consumed_once() {
    let fixture = tempfile::TempDir::new().unwrap();
    let adapter = Arc::new(ArchiveAdapter {
        exported: AtomicBool::new(false),
    });
    let service = OutlookDraftService::new(
        fixture.path().join("journal"),
        Duration::from_secs(300),
        GlobalMutationLock::default(),
        adapter.clone(),
    );
    let caller = CallerIdentity::new("owner", "session");
    let prepared = service.prepare_archive(&caller, fixture.path()).unwrap();
    assert_eq!(prepared.plan.steps[0].kind, StepKind::Write);
    assert_eq!(
        prepared.plan.steps[0].dst.as_deref().unwrap(),
        fixture.path().join("RFI response.msg").to_string_lossy()
    );
    service.bind_archive("plan", prepared);

    assert_eq!(service.export_archive("plan", &caller).unwrap(), b"msg");
    assert!(adapter.exported.load(Ordering::SeqCst));
    assert_eq!(
        service.export_archive("plan", &caller).unwrap_err().key,
        "error.rfi.outlookArchive.planUnavailable"
    );
}
