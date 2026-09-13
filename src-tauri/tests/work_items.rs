use std::{fs, path::Path};

use chrono::NaiveDate;
use docflow_lib::{
    contracts::{Step, StepKind},
    journal::{JournalRecord, JournalState, JournalStore},
    work_items::{
        WorkItemExportInput, WorkItemInput, preview_event, preview_event_with_journal,
        preview_export, project, project_with_journals,
    },
};
use serde_json::{Map, json};
use tempfile::tempdir;

fn persist_preview(plan: &docflow_lib::contracts::Plan) {
    let step = plan.steps.first().expect("event write step");
    let path = Path::new(step.dst.as_deref().expect("event path"));
    fs::create_dir_all(path.parent().expect("event parent")).expect("create event parent");
    fs::write(
        path,
        step.meta
            .get("content")
            .and_then(serde_json::Value::as_str)
            .expect("event content"),
    )
    .expect("persist event fixture");
}

#[test]
fn audit_export_is_owner_scoped_bounded_and_excel_safe() {
    let directory = tempdir().expect("temporary store");
    let destination = tempdir().expect("export destination");
    let mut dangerous = input("create", "", "request_captured");
    dangerous.title = "=WEBSERVICE(\"https://example.invalid\")".to_string();
    let owned = preview_event(
        directory.path(),
        "owner-1",
        dangerous,
        "2026-09-13T08:00:00Z",
    )
    .expect("owned event");
    persist_preview(&owned);
    let foreign = preview_event(
        directory.path(),
        "owner-2",
        input("create", "", "request_captured"),
        "2026-09-13T08:01:00Z",
    )
    .expect("foreign event");
    persist_preview(&foreign);
    fs::write(directory.path().join("events/invalid.json"), b"{bad").unwrap();

    let plan = preview_export(
        directory.path(),
        "owner-1",
        WorkItemExportInput {
            destination: destination.path().to_string_lossy().into_owned(),
        },
        "2026-09-13T09:00:00Z",
    )
    .expect("audit export preview");

    assert_eq!(plan.op_id, "rfi.work_item_export");
    assert_eq!(plan.steps.len(), 2);
    assert!(plan.steps.iter().all(|step| step.kind == StepKind::Write));
    assert!(plan.steps.iter().all(|step| {
        Path::new(step.dst.as_deref().unwrap())
            .parent()
            .is_some_and(|parent| parent == destination.path())
    }));
    let json = plan.steps[0].meta["content"].as_str().unwrap();
    assert!(json.contains("=WEBSERVICE"));
    assert!(json.contains("\"ownerId\": \"owner-1\""));
    assert!(!json.contains("owner-2"));
    assert!(json.contains("\"invalidEventCount\": 1"));
    let csv = plan.steps[1].meta["content"].as_str().unwrap();
    assert!(csv.contains("'=WEBSERVICE"));
    assert!(!csv.contains("owner-2"));
    assert_eq!(plan.meta["eventCount"], json!(1));
}

#[test]
fn work_item_projection_reports_non_destructive_store_capacity() {
    let directory = tempdir().expect("temporary store");
    let created = preview_event(
        directory.path(),
        "owner-1",
        input("create", "", "request_captured"),
        "2026-09-13T08:00:00Z",
    )
    .expect("create preview");
    persist_preview(&created);

    let projection = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .unwrap();
    assert_eq!(projection.event_count, 1);
    assert!(projection.event_bytes > 0);
    assert_eq!(projection.event_limit, 10_000);
}

fn input(action: &str, item_id: &str, stage: &str) -> WorkItemInput {
    WorkItemInput {
        action: action.to_string(),
        item_id: item_id.to_string(),
        title: "MEC inspection request".to_string(),
        rfi_codes: "AZE-SUN-INT-QMS-RFI-MEC-0249".to_string(),
        stage: stage.to_string(),
        due_date: "2026-09-13".to_string(),
        needs_review: false,
        reason: if action == "transition" {
            "Prepared documents received".to_string()
        } else {
            String::new()
        },
        source_journal_id: String::new(),
    }
}

fn committed_journal(op_id: &str, steps: Vec<Step>) -> JournalRecord {
    let mut journal = JournalRecord::new(
        "11111111-1111-4111-8111-111111111111",
        op_id,
        "2026-09-13T09:00:00Z",
        "owner-1",
        "session-1",
        "digest-1",
    );
    journal.state = Some(JournalState::Committed);
    journal.steps = steps;
    journal
}

#[test]
fn append_only_create_and_transition_rebuild_the_current_queue() {
    let directory = tempdir().expect("temporary store");
    let created = preview_event(
        directory.path(),
        "owner-1",
        input("create", "", "request_captured"),
        "2026-09-13T08:00:00Z",
    )
    .expect("create preview");
    persist_preview(&created);

    let initial = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .expect("initial projection");
    assert_eq!(initial.today.len(), 1);
    let item_id = initial.today[0].item_id.clone();
    assert_eq!(initial.today[0].stage, "request_captured");

    let advanced = preview_event(
        directory.path(),
        "owner-1",
        input("transition", &item_id, "checked"),
        "2026-09-13T09:00:00Z",
    )
    .expect("transition preview");
    persist_preview(&advanced);

    let current = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .expect("current projection");
    assert_eq!(current.today.len(), 1);
    assert_eq!(current.today[0].stage, "checked");
    assert_eq!(
        current.today[0].rfi_codes,
        vec!["AZE-SUN-INT-QMS-RFI-MEC-0249"]
    );
}

#[test]
fn concurrent_transitions_fail_closed_into_needs_review() {
    let directory = tempdir().expect("temporary store");
    let create = preview_event(
        directory.path(),
        "owner-1",
        input("create", "", "request_captured"),
        "2026-09-13T08:00:00Z",
    )
    .expect("create preview");
    persist_preview(&create);
    let item_id = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .unwrap()
    .today[0]
        .item_id
        .clone();

    let first = preview_event(
        directory.path(),
        "owner-1",
        input("transition", &item_id, "checked"),
        "2026-09-13T09:00:00Z",
    )
    .expect("first transition");
    let second = preview_event(
        directory.path(),
        "owner-1",
        input("transition", &item_id, "numbers_assigned"),
        "2026-09-13T09:00:01Z",
    )
    .expect("concurrent transition");
    persist_preview(&first);
    persist_preview(&second);

    let projection = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .expect("conflict projection");
    assert!(projection.today.is_empty());
    assert_eq!(projection.needs_review.len(), 1);
    assert_eq!(projection.needs_review[0].stage, "checked");
}

#[test]
fn invalid_or_non_ascii_rfi_codes_are_rejected_without_an_event() {
    let directory = tempdir().expect("temporary store");
    let mut invalid = input("create", "", "request_captured");
    invalid.rfi_codes = "ÂZE-SUN-INT-QMS-RFI-MEC-0249".to_string();
    assert!(preview_event(directory.path(), "owner-1", invalid, "2026-09-13T08:00:00Z").is_err());
    assert!(!directory.path().join("events").exists());
}

#[test]
fn malformed_event_files_are_counted_but_never_projected() {
    let directory = tempdir().expect("temporary store");
    let events = directory.path().join("events");
    fs::create_dir_all(&events).unwrap();
    fs::write(events.join("invalid.json"), b"{not-json").unwrap();

    let projection = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .expect("bounded invalid projection");
    assert_eq!(projection.invalid_event_count, 1);
    assert!(projection.today.is_empty());
    assert!(projection.resume.is_empty());
    assert!(projection.needs_review.is_empty());
}

#[cfg(unix)]
#[test]
fn event_symlinks_are_never_followed() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().expect("temporary store");
    let events = directory.path().join("events");
    fs::create_dir_all(&events).unwrap();
    let outside = directory.path().join("outside.json");
    fs::write(&outside, b"{}").unwrap();
    symlink(&outside, events.join("linked.json")).unwrap();

    let projection = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .expect("symlink-safe projection");
    assert_eq!(projection.invalid_event_count, 1);
}

#[cfg(unix)]
#[test]
fn event_store_directory_symlinks_are_never_followed() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().expect("temporary store");
    let outside = tempdir().expect("outside event store");
    symlink(outside.path(), directory.path().join("events")).unwrap();

    assert!(
        project(
            directory.path(),
            "owner-1",
            NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
        )
        .is_err()
    );
}

#[test]
fn committed_rfi_log_journal_assigns_exact_numbers_to_an_empty_item() {
    let directory = tempdir().expect("temporary store");
    let mut create_input = input("create", "", "checked");
    create_input.rfi_codes.clear();
    let create = preview_event(
        directory.path(),
        "owner-1",
        create_input,
        "2026-09-13T08:00:00Z",
    )
    .unwrap();
    persist_preview(&create);
    let item_id = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .unwrap()
    .today[0]
        .item_id
        .clone();
    let journal = committed_journal(
        "rfi.log_add",
        vec![Step {
            kind: StepKind::WorkbookUpdate,
            src: None,
            dst: None,
            note_key: None,
            meta: Map::from_iter([(
                "workItemEvidence".to_string(),
                json!({"canonicalRfiCodes": ["AZE-SUN-INT-QMS-RFI-MEC-0249"]}),
            )]),
        }],
    );
    let mut transition = input("transition", &item_id, "numbers_assigned");
    transition.source_journal_id = journal.journal_id.clone();
    transition.reason.clear();
    let plan = preview_event_with_journal(
        directory.path(),
        "owner-1",
        transition,
        "2026-09-13T09:01:00Z",
        Some(&journal),
    )
    .unwrap();
    persist_preview(&plan);
    let item = &project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .unwrap()
    .today[0];
    assert_eq!(item.stage, "numbers_assigned");
    assert_eq!(item.rfi_codes, ["AZE-SUN-INT-QMS-RFI-MEC-0249"]);
    assert_eq!(
        item.source_journal_id.as_deref(),
        Some("11111111-1111-4111-8111-111111111111")
    );
}

#[test]
fn partial_wrong_owner_or_outlook_journals_never_advance_work() {
    let directory = tempdir().expect("temporary store");
    let create = preview_event(
        directory.path(),
        "owner-1",
        input("create", "", "checked"),
        "2026-09-13T08:00:00Z",
    )
    .unwrap();
    persist_preview(&create);
    let item_id = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .unwrap()
    .today[0]
        .item_id
        .clone();
    for (op_id, owner, state) in [
        ("rfi.log_add", "owner-1", JournalState::PartialCommitted),
        ("rfi.log_add", "owner-2", JournalState::Committed),
        (
            "rfi.outlook_reply_draft",
            "owner-1",
            JournalState::Committed,
        ),
    ] {
        let mut journal = committed_journal(op_id, Vec::new());
        journal.owner_id = Some(owner.to_string());
        journal.state = Some(state);
        let mut transition = input("transition", &item_id, "numbers_assigned");
        transition.source_journal_id = journal.journal_id.clone();
        assert!(
            preview_event_with_journal(
                directory.path(),
                "owner-1",
                transition,
                "2026-09-13T09:01:00Z",
                Some(&journal),
            )
            .is_err()
        );
    }
}

#[test]
fn process_scan_evidence_counts_only_successfully_filed_rfi_documents() {
    let directory = tempdir().unwrap();
    let create = preview_event(
        directory.path(),
        "owner-1",
        input("create", "", "publication_email_archived"),
        "2026-09-13T08:00:00Z",
    )
    .unwrap();
    persist_preview(&create);
    let item_id = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .unwrap()
    .today[0]
        .item_id
        .clone();
    let journal = committed_journal(
        "rfi.process_scans",
        vec![
            Step {
                kind: StepKind::Copy,
                src: None,
                dst: None,
                note_key: Some("note.renamed".to_string()),
                meta: Map::from_iter([("code".to_string(), json!("AZE-SUN-INT-QMS-RFI-MEC-0249"))]),
            },
            Step {
                kind: StepKind::Copy,
                src: None,
                dst: None,
                note_key: Some("note.needs_review".to_string()),
                meta: Map::from_iter([("code".to_string(), json!("AZE-SUN-INT-QMS-RFI-MEC-0250"))]),
            },
        ],
    );
    let mut transition = input("transition", &item_id, "intec_scan_filed");
    transition.source_journal_id = journal.journal_id.clone();
    assert!(
        preview_event_with_journal(
            directory.path(),
            "owner-1",
            transition,
            "2026-09-13T09:01:00Z",
            Some(&journal),
        )
        .is_ok()
    );
}

#[test]
fn system_sync_requires_the_same_rfi_in_both_committed_workbook_steps() {
    let directory = tempdir().unwrap();
    let create = preview_event(
        directory.path(),
        "owner-1",
        input("create", "", "intec_scan_filed"),
        "2026-09-13T08:00:00Z",
    )
    .unwrap();
    persist_preview(&create);
    let item_id = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .unwrap()
    .today[0]
        .item_id
        .clone();
    let sync_step = |kind: &str, codes: serde_json::Value| Step {
        kind: StepKind::WorkbookUpdate,
        src: None,
        dst: None,
        note_key: None,
        meta: Map::from_iter([(
            "workItemEvidence".to_string(),
            json!({"workbookKind": kind, "canonicalRfiCodes": codes}),
        )]),
    };
    let journal = committed_journal(
        "rfi.system_sync",
        vec![
            sync_step("rfiLog", json!(["AZE-SUN-INT-QMS-RFI-MEC-0249"])),
            sync_step("followUp", json!([])),
        ],
    );
    let mut transition = input("transition", &item_id, "worley_signed_synced");
    transition.source_journal_id = journal.journal_id.clone();
    assert!(
        preview_event_with_journal(
            directory.path(),
            "owner-1",
            transition,
            "2026-09-13T09:01:00Z",
            Some(&journal),
        )
        .is_err()
    );
    let journal = committed_journal(
        "rfi.system_sync",
        vec![
            sync_step("rfiLog", json!(["AZE-SUN-INT-QMS-RFI-MEC-0249"])),
            sync_step("followUp", json!(["AZE-SUN-INT-QMS-RFI-MEC-0249"])),
        ],
    );
    let mut transition = input("transition", &item_id, "worley_signed_synced");
    transition.source_journal_id = journal.journal_id.clone();
    assert!(
        preview_event_with_journal(
            directory.path(),
            "owner-1",
            transition,
            "2026-09-13T09:02:00Z",
            Some(&journal),
        )
        .is_ok()
    );
}

#[test]
fn missing_source_journal_moves_a_previously_advanced_item_to_review() {
    let directory = tempdir().unwrap();
    let mut create_input = input("create", "", "checked");
    create_input.rfi_codes.clear();
    let create = preview_event(
        directory.path(),
        "owner-1",
        create_input,
        "2026-09-13T08:00:00Z",
    )
    .unwrap();
    persist_preview(&create);
    let item_id = project(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
    )
    .unwrap()
    .today[0]
        .item_id
        .clone();
    let journal = committed_journal(
        "rfi.log_add",
        vec![Step {
            kind: StepKind::WorkbookUpdate,
            src: None,
            dst: None,
            note_key: None,
            meta: Map::from_iter([(
                "workItemEvidence".to_string(),
                json!({"canonicalRfiCodes": ["AZE-SUN-INT-QMS-RFI-MEC-0249"]}),
            )]),
        }],
    );
    let mut transition = input("transition", &item_id, "numbers_assigned");
    transition.source_journal_id = journal.journal_id.clone();
    let advanced = preview_event_with_journal(
        directory.path(),
        "owner-1",
        transition,
        "2026-09-13T09:01:00Z",
        Some(&journal),
    )
    .unwrap();
    persist_preview(&advanced);

    let projection = project_with_journals(
        directory.path(),
        "owner-1",
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
        &JournalStore::new(directory.path().join("missing-journals")),
    )
    .unwrap();
    assert_eq!(projection.needs_review.len(), 1);
    assert_eq!(
        projection.needs_review[0].reason.as_deref(),
        Some("error.workItems.sourceJournalStale")
    );
}
