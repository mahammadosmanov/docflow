use std::{fs, path::Path};

use chrono::NaiveDate;
use docflow_lib::work_items::{WorkItemInput, preview_event, project};
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
    }
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
