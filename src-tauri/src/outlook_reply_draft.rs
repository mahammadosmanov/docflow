use std::{
    collections::HashMap,
    fmt,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use serde_json::{Value, json};

use crate::{
    contracts::{JsonObject, OperationResult, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    journal::{JournalRecord, JournalState, JournalStore},
    paths::CallerIdentity,
    plan_store::{BackendOrigin, PlanBinding, PlanStore},
    progress::{CancellationRegistry, GlobalMutationLock},
};

pub const OPERATION_ID: &str = "rfi.outlook_reply_draft";
pub const ARCHIVE_OPERATION_ID: &str = "rfi.outlook_archive_selected";
pub const MAX_REPLY_TEXT_CHARS: usize = 10_000;
const MAX_OUTLOOK_ID_CHARS: usize = 4096;
const MAX_SUBJECT_CHARS: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailItemIdentity {
    pub entry_id: String,
    pub store_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedMailItem {
    pub identity: MailItemIdentity,
    pub subject: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplyContent {
    pub plain_text: String,
    pub html: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftIdentity {
    pub entry_id: String,
    pub store_id: String,
    pub fingerprint: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutlookFailureKind {
    Unsupported,
    NoEffect,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookFailure {
    pub kind: OutlookFailureKind,
    pub key: &'static str,
}

impl OutlookFailure {
    pub const fn unsupported(key: &'static str) -> Self {
        Self {
            kind: OutlookFailureKind::Unsupported,
            key,
        }
    }

    pub const fn no_effect(key: &'static str) -> Self {
        Self {
            kind: OutlookFailureKind::NoEffect,
            key,
        }
    }

    pub const fn outcome_unknown(key: &'static str) -> Self {
        Self {
            kind: OutlookFailureKind::OutcomeUnknown,
            key,
        }
    }
}

impl fmt::Display for OutlookFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key)
    }
}

impl std::error::Error for OutlookFailure {}

pub trait OutlookDraft {
    fn prepend_body(&mut self, content: &ReplyContent) -> Result<(), OutlookFailure>;
    fn display(&mut self) -> Result<(), OutlookFailure>;
    fn durable_identity(&mut self) -> Option<DraftIdentity>;
}

pub trait OutlookAdapter: Send + Sync {
    fn selected_mail_item(&self) -> Result<SelectedMailItem, OutlookFailure>;

    /// Re-resolves the active selection, verifies `expected`, and only then calls ReplyAll.
    fn reply_all(
        &self,
        expected: &MailItemIdentity,
    ) -> Result<Box<dyn OutlookDraft>, OutlookFailure>;

    /// Deletes only the exact durable draft when its current fingerprint still matches.
    fn delete_unchanged_draft(&self, draft: &DraftIdentity) -> Result<(), OutlookFailure>;

    fn export_selected_msg(
        &self,
        expected: &MailItemIdentity,
        expected_fingerprint: &str,
    ) -> Result<Vec<u8>, OutlookFailure>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplyApplyOutcome {
    pub durable_identity: Option<DraftIdentity>,
}

pub fn build_reply_content(reply_text: &str) -> Result<ReplyContent, OutlookFailure> {
    if reply_text.trim().is_empty() {
        return Err(OutlookFailure::no_effect(
            "error.rfi.outlookDraft.replyRequired",
        ));
    }
    if reply_text.chars().count() > MAX_REPLY_TEXT_CHARS {
        return Err(OutlookFailure::no_effect(
            "error.rfi.outlookDraft.replyTooLong",
        ));
    }
    if reply_text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
    {
        return Err(OutlookFailure::no_effect(
            "error.rfi.outlookDraft.replyInvalid",
        ));
    }

    let plain_text = reply_text.replace("\r\n", "\n").replace('\r', "\n");
    let plain_text = plain_text.replace('\n', "\r\n");
    let mut escaped = String::with_capacity(plain_text.len());
    for character in plain_text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            '\r' => {}
            '\n' => escaped.push_str("<br>"),
            _ => escaped.push(character),
        }
    }

    Ok(ReplyContent {
        plain_text,
        html: format!("<div>{escaped}</div><br><br>"),
    })
}

pub fn apply_reply_all(
    adapter: &dyn OutlookAdapter,
    expected: &MailItemIdentity,
    content: &ReplyContent,
) -> Result<ReplyApplyOutcome, OutlookFailure> {
    let mut draft = adapter.reply_all(expected)?;
    draft.prepend_body(content)?;
    draft.display()?;
    Ok(ReplyApplyOutcome {
        durable_identity: draft.durable_identity(),
    })
}

#[derive(Clone)]
struct OutlookPlanAuthority {
    source: MailItemIdentity,
    content: ReplyContent,
}

#[derive(Clone)]
struct OutlookArchiveAuthority {
    owner_id: String,
    session_id: String,
    source: MailItemIdentity,
    fingerprint: String,
}

pub(crate) struct PreparedArchive {
    pub plan: Plan,
    authority: OutlookArchiveAuthority,
}

pub struct OutlookDraftService {
    plans: PlanStore,
    authorities: Mutex<HashMap<String, OutlookPlanAuthority>>,
    archive_authorities: Mutex<HashMap<String, OutlookArchiveAuthority>>,
    journals: JournalStore,
    mutation_lock: GlobalMutationLock,
    cancellations: CancellationRegistry,
    adapter: Arc<dyn OutlookAdapter>,
}

impl OutlookDraftService {
    pub fn new(
        journal_dir: impl AsRef<Path>,
        ttl: Duration,
        mutation_lock: GlobalMutationLock,
        adapter: Arc<dyn OutlookAdapter>,
    ) -> Self {
        Self {
            plans: PlanStore::new(ttl),
            authorities: Mutex::new(HashMap::new()),
            archive_authorities: Mutex::new(HashMap::new()),
            journals: JournalStore::new(journal_dir),
            mutation_lock,
            cancellations: CancellationRegistry::default(),
            adapter,
        }
    }

    pub(crate) fn prepare_archive(
        &self,
        caller: &CallerIdentity,
        destination_root: &Path,
    ) -> Result<PreparedArchive, BackendError> {
        let selected = self
            .adapter
            .selected_mail_item()
            .map_err(outlook_validation)?;
        validate_selected_mail_item(&selected)?;
        let destination = destination_root.join(archive_filename(&selected.subject));
        if destination.exists() {
            return Err(BackendError::validation(
                "error.rfi.outlookArchive.destinationExists",
            ));
        }
        let step = Step {
            kind: StepKind::Write,
            src: Some(selected.subject.clone()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: Some("step.rfi.outlookArchive".to_string()),
            meta: json_object(json!({
                "generatedBy": "outlook_msg",
                "format": "msg",
                "mailboxMutationAllowed": false,
                "sendAllowed": false
            }))?,
        };
        Ok(PreparedArchive {
            plan: Plan {
                plan_id: String::new(),
                op_id: ARCHIVE_OPERATION_ID.to_string(),
                steps: vec![step],
                warnings: vec![Warning {
                    key: "warn.rfi.outlookArchive.localCopy".to_string(),
                    subject: selected.subject,
                }],
                meta: JsonObject::new(),
            },
            authority: OutlookArchiveAuthority {
                owner_id: caller.owner_id.clone(),
                session_id: caller.session_id.clone(),
                source: selected.identity,
                fingerprint: selected.fingerprint,
            },
        })
    }

    pub(crate) fn bind_archive(&self, plan_id: &str, prepared: PreparedArchive) {
        lock_archive_authorities(&self.archive_authorities)
            .insert(plan_id.to_string(), prepared.authority);
    }

    pub(crate) fn export_archive(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
    ) -> Result<Vec<u8>, OutlookFailure> {
        let authority = lock_archive_authorities(&self.archive_authorities)
            .remove(plan_id)
            .ok_or_else(|| OutlookFailure::no_effect("error.rfi.outlookArchive.planUnavailable"))?;
        if authority.owner_id != caller.owner_id || authority.session_id != caller.session_id {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookArchive.planUnavailable",
            ));
        }
        self.adapter
            .export_selected_msg(&authority.source, &authority.fingerprint)
    }

    pub fn preview(&self, caller: &CallerIdentity, reply_text: &str) -> Result<Plan, BackendError> {
        let content = build_reply_content(reply_text).map_err(outlook_validation)?;
        let selected = self
            .adapter
            .selected_mail_item()
            .map_err(outlook_validation)?;
        validate_selected_mail_item(&selected)?;

        let step = Step {
            kind: StepKind::OutlookReplyDraft,
            src: Some(selected.subject.clone()),
            dst: None,
            note_key: Some("step.rfi.outlookReplyDraft".to_string()),
            meta: json_object(json!({
                "displayOnly": true,
                "sendAllowed": false
            }))?,
        };
        let plan = Plan {
            plan_id: String::new(),
            op_id: OPERATION_ID.to_string(),
            steps: vec![step],
            warnings: vec![
                Warning {
                    key: "warn.rfi.outlookDraft.unsent".to_string(),
                    subject: selected.subject.clone(),
                },
                Warning {
                    key: "warn.rfi.outlookDraft.undoConditional".to_string(),
                    subject: String::new(),
                },
            ],
            meta: json_object(json!({
                "replyPreview": {
                    "subject": selected.subject,
                    "plainText": content.plain_text,
                    "html": content.html
                }
            }))?,
        };
        let cached = self.plans.insert_bound(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                OPERATION_ID,
                Vec::new(),
            ),
            plan,
        )?;
        lock_authorities(&self.authorities).insert(
            cached.plan_id.clone(),
            OutlookPlanAuthority {
                source: selected.identity,
                content,
            },
        );
        Ok(cached)
    }

    pub fn contains_plan(&self, plan_id: &str) -> bool {
        self.plans.snapshot(plan_id).is_some()
    }

    pub fn apply<F>(
        &self,
        plan_id: &str,
        operation_id: Option<&str>,
        caller: &CallerIdentity,
        mut progress: F,
    ) -> Result<OperationResult, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let approved = self
            .plans
            .inspect_available_bound_operation(plan_id, caller)?;
        if approved != OPERATION_ID
            || operation_id.is_some_and(|operation_id| operation_id != approved)
        {
            return Err(BackendError::validation(
                "apply operation does not match the approved plan",
            ));
        }
        let claim = self
            .plans
            .claim_for_apply_bound(plan_id, caller, &self.mutation_lock)?;
        let authority = lock_authorities(&self.authorities)
            .remove(plan_id)
            .ok_or_else(BackendError::plan_unavailable)?;
        let token = self.cancellations.register(plan_id);
        let apply = (|| {
            token.checkpoint()?;
            let binding = claim
                .claimed
                .binding
                .as_ref()
                .ok_or_else(BackendError::plan_unavailable)?;
            let step = claim
                .claimed
                .plan
                .steps
                .first()
                .filter(|_| claim.claimed.plan.steps.len() == 1)
                .cloned()
                .ok_or_else(BackendError::plan_unavailable)?;
            if step.kind != StepKind::OutlookReplyDraft {
                return Err(BackendError::plan_unavailable());
            }
            let mut record = JournalRecord::fresh(
                OPERATION_ID,
                &binding.owner_id,
                &binding.session_id,
                &claim.claimed.digest,
            );
            record.state = Some(JournalState::Applying);
            record.pending_step = Some(step.clone());
            self.journals.write(&record)?;
            progress(ProgressEvent {
                phase: "apply".to_string(),
                done: 0,
                total: 1,
                subject: step.src.clone().unwrap_or_default(),
            });

            let result =
                match apply_reply_all(self.adapter.as_ref(), &authority.source, &authority.content)
                {
                    Ok(outcome) => {
                        let durable = outcome.durable_identity;
                        let mut committed_step = step;
                        committed_step
                            .meta
                            .insert("draftDurable".to_string(), Value::Bool(durable.is_some()));
                        if let Some(draft) = durable.as_ref() {
                            committed_step.meta.insert(
                                "draftEntryId".to_string(),
                                Value::String(draft.entry_id.clone()),
                            );
                            committed_step.meta.insert(
                                "draftStoreId".to_string(),
                                Value::String(draft.store_id.clone()),
                            );
                            committed_step.meta.insert(
                                "draftFingerprint".to_string(),
                                Value::String(draft.fingerprint.clone()),
                            );
                        }
                        record.pending_step = None;
                        record.steps.push(committed_step);
                        record.state = Some(JournalState::Committed);
                        self.journals.write(&record)?;
                        let mut result = operation_result(&record.journal_id, 1, 0);
                        result
                            .meta
                            .insert("draftDisplayed".to_string(), Value::Bool(true));
                        result
                            .meta
                            .insert("undoAvailable".to_string(), Value::Bool(durable.is_some()));
                        if durable.is_none() {
                            result
                                .meta
                                .insert("nonUndoable".to_string(), Value::Bool(true));
                            result.errors.push(Warning {
                                key: "warn.rfi.outlookDraft.undoUnavailable".to_string(),
                                subject: String::new(),
                            });
                        }
                        result
                    }
                    Err(failure) => {
                        let outcome_unknown = failure.kind == OutlookFailureKind::OutcomeUnknown;
                        if !outcome_unknown {
                            record.pending_step = None;
                        }
                        record.state = Some(if outcome_unknown {
                            JournalState::RecoveryRequired
                        } else {
                            JournalState::NoEffectRetryable
                        });
                        self.journals.write(&record)?;
                        let mut result = operation_result(&record.journal_id, 0, 1);
                        result.errors.push(Warning {
                            key: failure.key.to_string(),
                            subject: String::new(),
                        });
                        result.meta.insert(
                            "effectState".to_string(),
                            Value::String(if outcome_unknown {
                                "outcome_unknown".to_string()
                            } else {
                                "no_effect_retryable".to_string()
                            }),
                        );
                        result
                    }
                };
            progress(ProgressEvent {
                phase: "apply".to_string(),
                done: 1,
                total: 1,
                subject: String::new(),
            });
            Ok(result)
        })();
        self.cancellations.finish(plan_id);
        apply
    }

    pub fn undo(
        &self,
        journal_id: &str,
        caller: &CallerIdentity,
    ) -> Result<OperationResult, BackendError> {
        let mut record = self.journals.read(journal_id)?;
        validate_undo_record(&record, caller)?;
        let draft = durable_identity_from_step(&record.steps[0])?;
        let _process = self.mutation_lock.try_acquire()?;

        record.pending_undo = Some(0);
        self.journals.write(&record)?;
        match self.adapter.delete_unchanged_draft(&draft) {
            Ok(()) => {
                record.pending_undo = None;
                record.undone_steps.push(0);
                record.state = Some(JournalState::Undone);
                self.journals.write(&record)?;
                Ok(operation_result(journal_id, 1, 0))
            }
            Err(failure) => {
                let outcome_unknown = failure.kind == OutlookFailureKind::OutcomeUnknown;
                if !outcome_unknown {
                    record.pending_undo = None;
                }
                record.state = Some(if outcome_unknown {
                    JournalState::RecoveryRequired
                } else {
                    JournalState::Committed
                });
                self.journals.write(&record)?;
                let mut result = operation_result(journal_id, 0, 1);
                result.errors.push(Warning {
                    key: failure.key.to_string(),
                    subject: String::new(),
                });
                result.meta.insert(
                    "effectState".to_string(),
                    Value::String(if outcome_unknown {
                        "outcome_unknown".to_string()
                    } else {
                        "no_effect_retryable".to_string()
                    }),
                );
                Ok(result)
            }
        }
    }

    pub fn cancel(&self, plan_id: &str) -> bool {
        self.cancellations.cancel(plan_id)
    }

    pub fn revoke(&self, caller: &CallerIdentity) {
        self.plans.revoke(caller);
        lock_authorities(&self.authorities)
            .retain(|plan_id, _| self.plans.snapshot(plan_id).is_some());
        lock_archive_authorities(&self.archive_authorities).retain(|_, authority| {
            authority.owner_id != caller.owner_id || authority.session_id != caller.session_id
        });
        self.cancellations.revoke_all();
    }

    pub fn journals(&self) -> &JournalStore {
        &self.journals
    }
}

pub fn archive_filename(subject: &str) -> String {
    let mut stem = String::with_capacity(subject.len().min(120));
    let mut previous_space = false;
    for character in subject.trim().chars() {
        let character = if character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) {
            ' '
        } else {
            character
        };
        if character.is_whitespace() {
            if !previous_space && !stem.is_empty() {
                stem.push(' ');
            }
            previous_space = true;
        } else {
            if stem.chars().count() >= 100 {
                break;
            }
            stem.push(character);
            previous_space = false;
        }
    }
    let trimmed = stem.trim_end_matches([' ', '.']);
    let mut stem = if trimmed.is_empty() {
        "Outlook message".to_string()
    } else {
        trimmed.to_string()
    };
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0')
    {
        stem.insert(0, '_');
    }
    format!("{stem}.msg")
}

pub(crate) fn journal_is_safely_undoable(record: &JournalRecord) -> bool {
    let caller = CallerIdentity::new(
        record.owner_id.clone().unwrap_or_default(),
        record.session_id.clone().unwrap_or_default(),
    );
    validate_undo_record(record, &caller).is_ok()
        && durable_identity_from_step(&record.steps[0]).is_ok()
}

fn validate_selected_mail_item(selected: &SelectedMailItem) -> Result<(), BackendError> {
    if !bounded_identifier(&selected.identity.entry_id)
        || !bounded_identifier(&selected.identity.store_id)
        || selected.fingerprint.len() != 64
        || !selected
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(BackendError::validation(
            "error.rfi.outlookDraft.selectionInvalid",
        ));
    }
    if selected.subject.chars().count() > MAX_SUBJECT_CHARS
        || selected.subject.chars().any(char::is_control)
    {
        return Err(BackendError::validation(
            "error.rfi.outlookDraft.subjectTooLong",
        ));
    }
    Ok(())
}

fn validate_undo_record(
    record: &JournalRecord,
    caller: &CallerIdentity,
) -> Result<(), BackendError> {
    if record.op_id != OPERATION_ID
        || record.state != Some(JournalState::Committed)
        || record.pending_step.is_some()
        || record.pending_undo.is_some()
        || record.undo_recovery.is_some()
        || record.steps.len() != 1
        || record.steps[0].kind != StepKind::OutlookReplyDraft
        || !record.undone_steps.is_empty()
    {
        return Err(BackendError::journal_invalid(
            "Outlook draft journal is not eligible for exact undo",
        ));
    }
    if record.owner_id.as_deref() != Some(caller.owner_id.as_str()) {
        return Err(BackendError::journal_owner_mismatch());
    }
    Ok(())
}

fn durable_identity_from_step(step: &Step) -> Result<DraftIdentity, BackendError> {
    if step.meta.get("draftDurable") != Some(&Value::Bool(true)) {
        return Err(BackendError::validation(
            "error.rfi.outlookDraft.undoUnavailable",
        ));
    }
    let field = |name: &str| {
        step.meta
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| bounded_identifier(value))
            .map(str::to_string)
            .ok_or_else(|| BackendError::journal_invalid("Outlook draft identity is invalid"))
    };
    Ok(DraftIdentity {
        entry_id: field("draftEntryId")?,
        store_id: field("draftStoreId")?,
        fingerprint: field("draftFingerprint")?,
    })
}

fn bounded_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_OUTLOOK_ID_CHARS
        && value.chars().all(|character| !character.is_control())
}

fn operation_result(journal_id: &str, applied: u64, failed: u64) -> OperationResult {
    let mut meta = JsonObject::new();
    meta.insert(
        "effectState".to_string(),
        Value::String(if failed == 0 {
            "committed".to_string()
        } else {
            "no_effect_retryable".to_string()
        }),
    );
    OperationResult {
        applied,
        failed,
        journal_id: journal_id.to_string(),
        errors: Vec::new(),
        meta,
        cloud_synced: None,
    }
}

fn outlook_validation(failure: OutlookFailure) -> BackendError {
    BackendError::validation(failure.key)
}

fn json_object(value: Value) -> Result<JsonObject, BackendError> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| BackendError::protocol("Outlook plan metadata is invalid"))
}

fn lock_authorities(
    authorities: &Mutex<HashMap<String, OutlookPlanAuthority>>,
) -> MutexGuard<'_, HashMap<String, OutlookPlanAuthority>> {
    authorities
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_archive_authorities(
    authorities: &Mutex<HashMap<String, OutlookArchiveAuthority>>,
) -> MutexGuard<'_, HashMap<String, OutlookArchiveAuthority>> {
    authorities
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
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
}
