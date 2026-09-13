use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
    sync::OnceLock,
};

use chrono::{DateTime, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    contracts::{Plan, Step, StepKind},
    errors::BackendError,
    journal::{JournalRecord, JournalState, JournalStore},
};

pub const OPERATION_ID: &str = "rfi.work_item_update";
const SCHEMA_VERSION: u64 = 1;
const MAX_EVENTS: usize = 10_000;
const MAX_EVENT_BYTES: u64 = 64 * 1024;
const MAX_TITLE_CHARS: usize = 200;
const MAX_REASON_CHARS: usize = 1_000;
const MAX_RFI_CODES: usize = 500;

const STAGES: &[&str] = &[
    "request_captured",
    "checked",
    "numbers_assigned",
    "prepared_files_received",
    "published_ongoing",
    "publication_email_archived",
    "intec_scan_filed",
    "worley_signed_synced",
    "replicas_synced",
    "complete",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkItemInput {
    pub action: String,
    #[serde(default)]
    pub item_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub rfi_codes: String,
    pub stage: String,
    #[serde(default)]
    pub due_date: String,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub source_journal_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkItemEvent {
    schema_version: u64,
    event_id: String,
    item_id: String,
    owner_id: String,
    occurred_at: String,
    action: String,
    previous_event_id: Option<String>,
    from_stage: Option<String>,
    to_stage: String,
    title: String,
    rfi_codes: Vec<String>,
    due_date: Option<String>,
    needs_review: bool,
    reason: Option<String>,
    #[serde(default)]
    source_journal_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub item_id: String,
    pub title: String,
    pub rfi_codes: Vec<String>,
    pub stage: String,
    pub updated_at: String,
    pub due_date: Option<String>,
    pub needs_review: bool,
    pub reason: Option<String>,
    pub last_event_id: String,
    pub source_journal_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemQueues {
    pub today: Vec<WorkItem>,
    pub resume: Vec<WorkItem>,
    pub needs_review: Vec<WorkItem>,
    pub completed_count: u64,
    pub invalid_event_count: u64,
}

pub fn preview_event(
    root: &Path,
    owner_id: &str,
    input: WorkItemInput,
    occurred_at: &str,
) -> Result<Plan, BackendError> {
    preview_event_with_journal(root, owner_id, input, occurred_at, None)
}

pub fn preview_event_with_journal(
    root: &Path,
    owner_id: &str,
    input: WorkItemInput,
    occurred_at: &str,
    source_journal: Option<&JournalRecord>,
) -> Result<Plan, BackendError> {
    validate_owner(owner_id)?;
    DateTime::parse_from_rfc3339(occurred_at)
        .map_err(|_| validation("error.workItems.timestampInvalid"))?;
    validate_stage(&input.stage)?;
    let due_date = optional_date(&input.due_date)?;
    let reason = bounded_optional(
        &input.reason,
        MAX_REASON_CHARS,
        "error.workItems.reasonInvalid",
    )?;
    let source_journal_id = bounded_optional(
        &input.source_journal_id,
        64,
        "error.workItems.sourceJournalInvalid",
    )?;
    if source_journal_id
        .as_deref()
        .is_some_and(|value| Uuid::parse_str(value).is_err())
    {
        return Err(validation("error.workItems.sourceJournalInvalid"));
    }
    let current = project(root, owner_id, chrono::Local::now().date_naive())?;
    let existing = current
        .today
        .iter()
        .chain(&current.resume)
        .chain(&current.needs_review)
        .find(|item| item.item_id == input.item_id)
        .cloned();

    let event_id = Uuid::new_v4().to_string();
    let (item_id, action, previous_event_id, from_stage, title, mut rfi_codes) =
        match input.action.as_str() {
            "create" => {
                if !input.item_id.trim().is_empty() {
                    return Err(validation("error.workItems.createIdForbidden"));
                }
                let title = bounded_required(
                    &input.title,
                    MAX_TITLE_CHARS,
                    "error.workItems.titleInvalid",
                )?;
                let rfi_codes = parse_rfi_codes(&input.rfi_codes)?;
                (
                    format!("work-{event_id}"),
                    "create".to_string(),
                    None,
                    None,
                    title,
                    rfi_codes,
                )
            }
            "transition" => {
                let existing = existing
                    .as_ref()
                    .ok_or_else(|| validation("error.workItems.notFound"))?;
                if source_journal_id.is_none() {
                    reason
                        .as_ref()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| validation("error.workItems.reasonRequired"))?;
                }
                (
                    existing.item_id.clone(),
                    "transition".to_string(),
                    Some(existing.last_event_id.clone()),
                    Some(existing.stage.clone()),
                    existing.title.clone(),
                    existing.rfi_codes.clone(),
                )
            }
            _ => return Err(validation("error.workItems.actionInvalid")),
        };
    if source_journal_id.is_some() != source_journal.is_some() {
        return Err(validation("error.workItems.sourceJournalInvalid"));
    }
    if let (Some(journal_id), Some(journal)) = (&source_journal_id, source_journal) {
        if journal_id != &journal.journal_id {
            return Err(validation("error.workItems.sourceJournalInvalid"));
        }
        if from_stage
            .as_deref()
            .and_then(stage_index)
            .zip(stage_index(&input.stage))
            .is_none_or(|(from, to)| to <= from)
        {
            return Err(validation("error.workItems.sourceJournalMismatch"));
        }
        rfi_codes = validate_journal_transition(journal, owner_id, &input.stage, &rfi_codes)?;
    }
    let reason = if reason.is_none() {
        source_journal.map(|journal| format!("journal evidence: {}", journal.op_id))
    } else {
        reason
    };
    let event = WorkItemEvent {
        schema_version: SCHEMA_VERSION,
        event_id: event_id.clone(),
        item_id: item_id.clone(),
        owner_id: owner_id.to_string(),
        occurred_at: occurred_at.to_string(),
        action,
        previous_event_id,
        from_stage,
        to_stage: input.stage,
        title,
        rfi_codes,
        due_date: due_date.or_else(|| existing.and_then(|item| item.due_date)),
        needs_review: input.needs_review,
        reason,
        source_journal_id,
    };
    let content = serde_json::to_string_pretty(&event)
        .map(|value| format!("{value}\n"))
        .map_err(|_| BackendError::protocol("work item event could not be encoded"))?;
    if content.len() as u64 > MAX_EVENT_BYTES {
        return Err(validation("error.workItems.eventTooLarge"));
    }
    let destination = root.join("events").join(format!(
        "{}-{}.json",
        compact_timestamp(occurred_at),
        event.event_id
    ));
    let mut meta = Map::new();
    meta.insert("content".to_string(), Value::String(content));
    meta.insert("itemId".to_string(), Value::String(item_id.clone()));
    meta.insert("stage".to_string(), Value::String(event.to_stage.clone()));
    meta.insert("title".to_string(), Value::String(event.title.clone()));
    Ok(Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps: vec![Step {
            kind: StepKind::Write,
            src: None,
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: Some("step.rfi.workItemEvent".to_string()),
            meta,
        }],
        warnings: Vec::new(),
        meta: Map::from_iter([
            ("workItemId".to_string(), Value::String(item_id)),
            ("workItemStage".to_string(), Value::String(event.to_stage)),
        ]),
    })
}

pub fn project(
    root: &Path,
    owner_id: &str,
    today: NaiveDate,
) -> Result<WorkItemQueues, BackendError> {
    validate_owner(owner_id)?;
    let events_root = root.join("events");
    let entries = match fs::read_dir(&events_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkItemQueues::default());
        }
        Err(_) => return Err(validation("error.workItems.storeUnavailable")),
    };
    let mut events = Vec::new();
    let mut invalid_event_count = 0_u64;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_EVENTS {
            return Err(validation("error.workItems.eventLimit"));
        }
        let Ok(entry) = entry else {
            invalid_event_count += 1;
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            invalid_event_count += 1;
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_EVENT_BYTES {
            invalid_event_count += 1;
            continue;
        }
        let Ok(file) = fs::File::open(&path) else {
            invalid_event_count += 1;
            continue;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_EVENT_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_EVENT_BYTES
        {
            invalid_event_count += 1;
            continue;
        }
        let Ok(event) = serde_json::from_slice::<WorkItemEvent>(&bytes) else {
            invalid_event_count += 1;
            continue;
        };
        if validate_event(&event).is_err() {
            invalid_event_count += 1;
            continue;
        }
        if event.owner_id == owner_id {
            events.push(event);
        }
    }
    events.sort_by(|left, right| {
        (&left.occurred_at, &left.event_id).cmp(&(&right.occurred_at, &right.event_id))
    });
    let mut projected = BTreeMap::<String, WorkItem>::new();
    for event in events {
        if event.action == "create" {
            if let Some(existing) = projected.get_mut(&event.item_id) {
                existing.needs_review = true;
                existing.reason = Some("error.workItems.conflict".to_string());
                continue;
            }
            projected.insert(
                event.item_id.clone(),
                WorkItem {
                    item_id: event.item_id,
                    title: event.title,
                    rfi_codes: event.rfi_codes,
                    stage: event.to_stage,
                    updated_at: event.occurred_at,
                    due_date: event.due_date,
                    needs_review: event.needs_review,
                    reason: event.reason,
                    last_event_id: event.event_id,
                    source_journal_id: event.source_journal_id,
                },
            );
            continue;
        }
        let Some(current) = projected.get_mut(&event.item_id) else {
            invalid_event_count += 1;
            continue;
        };
        if event.previous_event_id.as_deref() != Some(&current.last_event_id)
            || event.from_stage.as_deref() != Some(&current.stage)
        {
            current.needs_review = true;
            current.reason = Some("error.workItems.conflict".to_string());
            continue;
        }
        current.stage = event.to_stage;
        current.rfi_codes = event.rfi_codes;
        current.updated_at = event.occurred_at;
        current.due_date = event.due_date;
        current.needs_review = event.needs_review;
        current.reason = event.reason;
        current.last_event_id = event.event_id;
        current.source_journal_id = event.source_journal_id;
    }
    let mut queues = WorkItemQueues {
        invalid_event_count,
        ..WorkItemQueues::default()
    };
    for item in projected.into_values() {
        if item.stage == "complete" {
            queues.completed_count += 1;
        } else if item.needs_review {
            queues.needs_review.push(item);
        } else if item
            .due_date
            .as_deref()
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .is_some_and(|date| date <= today)
        {
            queues.today.push(item);
        } else {
            queues.resume.push(item);
        }
    }
    for queue in [
        &mut queues.today,
        &mut queues.resume,
        &mut queues.needs_review,
    ] {
        queue.sort_by(|left, right| {
            (&left.due_date, &left.updated_at, &left.item_id).cmp(&(
                &right.due_date,
                &right.updated_at,
                &right.item_id,
            ))
        });
    }
    Ok(queues)
}

pub fn project_with_journals(
    root: &Path,
    owner_id: &str,
    today: NaiveDate,
    journals: &JournalStore,
) -> Result<WorkItemQueues, BackendError> {
    let mut projected = project(root, owner_id, today)?;
    let invalid_evidence = invalid_evidence_items(root, owner_id, journals)?;
    for item in projected
        .today
        .iter_mut()
        .chain(projected.resume.iter_mut())
        .chain(projected.needs_review.iter_mut())
    {
        if invalid_evidence.contains(&item.item_id) {
            item.needs_review = true;
            item.reason = Some("error.workItems.sourceJournalStale".to_string());
        }
    }
    let mut all = projected
        .today
        .drain(..)
        .chain(projected.resume.drain(..))
        .chain(projected.needs_review.drain(..))
        .collect::<Vec<_>>();
    projected.needs_review = all
        .extract_if(.., |item| item.needs_review)
        .collect::<Vec<_>>();
    for item in all {
        if item
            .due_date
            .as_deref()
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .is_some_and(|date| date <= today)
        {
            projected.today.push(item);
        } else {
            projected.resume.push(item);
        }
    }
    for queue in [
        &mut projected.today,
        &mut projected.resume,
        &mut projected.needs_review,
    ] {
        queue.sort_by(|left, right| {
            (&left.due_date, &left.updated_at, &left.item_id).cmp(&(
                &right.due_date,
                &right.updated_at,
                &right.item_id,
            ))
        });
    }
    Ok(projected)
}

fn invalid_evidence_items(
    root: &Path,
    owner_id: &str,
    journals: &JournalStore,
) -> Result<BTreeSet<String>, BackendError> {
    let entries = match fs::read_dir(root.join("events")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(_) => return Err(validation("error.workItems.storeUnavailable")),
    };
    let mut invalid = BTreeSet::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_EVENTS {
            return Err(validation("error.workItems.eventLimit"));
        }
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if path.extension().and_then(|value| value.to_str()) != Some("json")
            || !metadata.is_file()
            || metadata.len() > MAX_EVENT_BYTES
        {
            continue;
        }
        let Ok(file) = fs::File::open(&path) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_EVENT_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_EVENT_BYTES
        {
            continue;
        }
        let Ok(event) = serde_json::from_slice::<WorkItemEvent>(&bytes) else {
            continue;
        };
        if event.owner_id != owner_id || validate_event(&event).is_err() {
            continue;
        }
        let Some(journal_id) = event.source_journal_id.as_deref() else {
            continue;
        };
        let valid = journals.read(journal_id).ok().is_some_and(|journal| {
            validate_journal_transition(&journal, owner_id, &event.to_stage, &event.rfi_codes)
                .is_ok()
        });
        if !valid {
            invalid.insert(event.item_id);
        }
    }
    Ok(invalid)
}

fn validate_journal_transition(
    journal: &JournalRecord,
    owner_id: &str,
    target_stage: &str,
    current_codes: &[String],
) -> Result<Vec<String>, BackendError> {
    if journal.owner_id.as_deref() != Some(owner_id)
        || journal.state != Some(JournalState::Committed)
        || journal.pending_step.is_some()
        || journal.pending_undo.is_some()
        || !journal.undone_steps.is_empty()
    {
        return Err(validation("error.workItems.sourceJournalInvalid"));
    }
    let expected_stage = match journal.op_id.as_str() {
        "rfi.log_add" => "numbers_assigned",
        "rfi.process_scans" => "intec_scan_filed",
        "rfi.system_sync" => "worley_signed_synced",
        _ => return Err(validation("error.workItems.sourceJournalUnsupported")),
    };
    if target_stage != expected_stage {
        return Err(validation("error.workItems.sourceJournalMismatch"));
    }
    let evidence = journal_rfi_codes(journal)?;
    if journal.op_id == "rfi.log_add" && current_codes.is_empty() {
        return Ok(evidence);
    }
    if current_codes.is_empty()
        || !current_codes
            .iter()
            .all(|code| evidence.binary_search(code).is_ok())
    {
        return Err(validation("error.workItems.sourceJournalMismatch"));
    }
    Ok(current_codes.to_vec())
}

fn journal_rfi_codes(journal: &JournalRecord) -> Result<Vec<String>, BackendError> {
    let mut codes = Vec::new();
    match journal.op_id.as_str() {
        "rfi.log_add" => {
            let evidence = journal
                .steps
                .iter()
                .filter_map(|step| step.meta.get("workItemEvidence"))
                .collect::<Vec<_>>();
            if evidence.len() != 1 {
                return Err(validation("error.workItems.sourceJournalMismatch"));
            }
            codes.extend(
                evidence[0]
                    .get("canonicalRfiCodes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string),
            );
        }
        "rfi.process_scans" => {
            for step in &journal.steps {
                if matches!(
                    step.kind,
                    StepKind::Copy | StepKind::Move | StepKind::Rename
                ) && step.note_key.as_deref() == Some("note.renamed")
                    && let Some(code) = step.meta.get("code").and_then(Value::as_str)
                {
                    codes.push(code.to_string());
                }
            }
        }
        "rfi.system_sync" => {
            let mut kinds = Vec::new();
            let mut per_workbook = Vec::<std::collections::BTreeSet<String>>::new();
            for sync in journal
                .steps
                .iter()
                .filter_map(|step| step.meta.get("workItemEvidence"))
                .filter_map(Value::as_object)
            {
                kinds.push(
                    sync.get("workbookKind")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                );
                let step_codes = sync
                    .get("canonicalRfiCodes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<std::collections::BTreeSet<_>>();
                per_workbook.push(step_codes);
            }
            kinds.sort();
            if kinds != ["followUp", "rfiLog"] || per_workbook.len() != 2 {
                return Err(validation("error.workItems.sourceJournalMismatch"));
            }
            let first = per_workbook.pop().unwrap_or_default();
            let second = per_workbook.pop().unwrap_or_default();
            codes = first.intersection(&second).cloned().collect();
        }
        _ => return Err(validation("error.workItems.sourceJournalUnsupported")),
    }
    for code in &codes {
        validate_rfi_code(code)?;
    }
    codes.sort();
    codes.dedup();
    if codes.is_empty() {
        return Err(validation("error.workItems.sourceJournalMismatch"));
    }
    Ok(codes)
}

fn validate_event(event: &WorkItemEvent) -> Result<(), BackendError> {
    let item_uuid = event.item_id.strip_prefix("work-");
    if event.schema_version != SCHEMA_VERSION
        || Uuid::parse_str(&event.event_id).is_err()
        || item_uuid.is_none_or(|value| Uuid::parse_str(value).is_err())
        || DateTime::parse_from_rfc3339(&event.occurred_at).is_err()
        || !matches!(event.action.as_str(), "create" | "transition")
    {
        return Err(validation("error.workItems.eventInvalid"));
    }
    validate_owner(&event.owner_id)?;
    validate_stage(&event.to_stage)?;
    match event.action.as_str() {
        "create"
            if event.previous_event_id.is_some()
                || event.from_stage.is_some()
                || event.item_id != format!("work-{}", event.event_id) =>
        {
            return Err(validation("error.workItems.eventInvalid"));
        }
        "transition" => {
            if event
                .previous_event_id
                .as_deref()
                .is_none_or(|value| Uuid::parse_str(value).is_err())
            {
                return Err(validation("error.workItems.eventInvalid"));
            }
            validate_stage(
                event
                    .from_stage
                    .as_deref()
                    .ok_or_else(|| validation("error.workItems.eventInvalid"))?,
            )?;
            if event.reason.as_deref().is_none_or(str::is_empty) {
                return Err(validation("error.workItems.reasonRequired"));
            }
        }
        _ => {}
    }
    bounded_required(
        &event.title,
        MAX_TITLE_CHARS,
        "error.workItems.titleInvalid",
    )?;
    for code in &event.rfi_codes {
        validate_rfi_code(code)?;
    }
    if event.rfi_codes.len() > MAX_RFI_CODES {
        return Err(validation("error.workItems.rfiInvalid"));
    }
    if let Some(value) = &event.due_date {
        optional_date(value)?;
    }
    if let Some(value) = &event.reason {
        bounded_optional(value, MAX_REASON_CHARS, "error.workItems.reasonInvalid")?;
    }
    if event
        .source_journal_id
        .as_deref()
        .is_some_and(|value| Uuid::parse_str(value).is_err())
    {
        return Err(validation("error.workItems.sourceJournalInvalid"));
    }
    Ok(())
}

fn parse_rfi_codes(value: &str) -> Result<Vec<String>, BackendError> {
    let mut codes = Vec::new();
    for token in value
        .split([',', ';', '\n', '\r'])
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        validate_rfi_code(token)?;
        if !codes.iter().any(|existing| existing == token) {
            codes.push(token.to_string());
        }
    }
    if codes.len() > MAX_RFI_CODES {
        return Err(validation("error.workItems.rfiInvalid"));
    }
    Ok(codes)
}

fn validate_rfi_code(value: &str) -> Result<(), BackendError> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"^AZE-SUN-INT-QMS-RFI-[A-Z]{3}-[0-9]{4}$").expect("canonical RFI pattern")
    });
    if !value.is_ascii() || !pattern.is_match(value) {
        return Err(validation("error.workItems.rfiInvalid"));
    }
    Ok(())
}

fn validate_stage(value: &str) -> Result<(), BackendError> {
    if STAGES.contains(&value) {
        Ok(())
    } else {
        Err(validation("error.workItems.stageInvalid"))
    }
}

fn stage_index(value: &str) -> Option<usize> {
    STAGES.iter().position(|stage| *stage == value)
}

fn validate_owner(value: &str) -> Result<(), BackendError> {
    if !value.is_empty() && value.chars().count() <= 128 && !value.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(validation("error.workItems.ownerInvalid"))
    }
}

fn bounded_required(value: &str, max: usize, key: &str) -> Result<String, BackendError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max || trimmed.chars().any(char::is_control)
    {
        return Err(validation(key));
    }
    Ok(trimmed.to_string())
}

fn bounded_optional(value: &str, max: usize, key: &str) -> Result<Option<String>, BackendError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    bounded_required(value, max, key).map(Some)
}

fn optional_date(value: &str) -> Result<Option<String>, BackendError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| Some(date.format("%Y-%m-%d").to_string()))
        .map_err(|_| validation("error.workItems.dateInvalid"))
}

fn compact_timestamp(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(20)
        .collect()
}

fn validation(key: &str) -> BackendError {
    BackendError::validation(key)
}
