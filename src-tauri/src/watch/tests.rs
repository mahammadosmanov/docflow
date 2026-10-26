use std::{fs, path::Path, time::Duration};

use super::*;

const SETTLE_MS: i64 = 60_000;

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn root(path: &Path, recursive: bool) -> WatchRoot {
    WatchRoot {
        path: path.to_path_buf(),
        recursive,
    }
}

#[test]
fn only_read_only_preview_operations_are_watchable() {
    for id in [
        "rfi.process_scans_fast_covers",
        "rfi.group_files",
        "rfi.stage_signed_first_pages",
        "rfi.complete_status_sync",
    ] {
        assert!(is_watchable(id), "{id}");
    }
    for id in [
        "files.batch_print",
        "files.replica_publish",
        "rfi.log_add",
        "rfi.system_sync",
        "unknown",
    ] {
        assert!(!is_watchable(id), "{id}");
    }
}

#[test]
fn snapshot_changes_when_file_is_added_or_resized_and_ignores_hidden_and_lock_files() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("a.pdf"), b"one");
    let first = take_snapshot(&[root(dir.path(), false)], 100).unwrap();
    assert_eq!(first.entries, 1);

    write(&dir.path().join(".hidden"), b"x");
    write(&dir.path().join("~$book.xlsm"), b"x");
    let ignored = take_snapshot(&[root(dir.path(), false)], 100).unwrap();
    assert_eq!(ignored.fingerprint, first.fingerprint);

    write(&dir.path().join("b.pdf"), b"two");
    let added = take_snapshot(&[root(dir.path(), false)], 100).unwrap();
    assert_eq!(added.entries, 2);
    assert_ne!(added.fingerprint, first.fingerprint);

    write(&dir.path().join("b.pdf"), b"twelve");
    let resized = take_snapshot(&[root(dir.path(), false)], 100).unwrap();
    assert_ne!(resized.fingerprint, added.fingerprint);
}

#[test]
fn non_recursive_root_ignores_subfolders_and_recursive_root_includes_them() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("top.pdf"), b"a");
    write(&dir.path().join("sub/nested.pdf"), b"b");
    let flat = take_snapshot(&[root(dir.path(), false)], 100).unwrap();
    let deep = take_snapshot(&[root(dir.path(), true)], 100).unwrap();
    assert_eq!(flat.entries, 1);
    assert_eq!(deep.entries, 2);
}

#[test]
fn file_root_is_fingerprinted_and_entry_cap_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("log.xlsm");
    write(&book, b"v1");
    let before = take_snapshot(&[root(&book, false)], 100).unwrap();
    write(&book, b"version two");
    let after = take_snapshot(&[root(&book, false)], 100).unwrap();
    assert_eq!(before.entries, 1);
    assert_ne!(before.fingerprint, after.fingerprint);

    for index in 0..5 {
        write(&dir.path().join(format!("f{index}.pdf")), b"x");
    }
    let error = take_snapshot(&[root(dir.path(), false)], 3).unwrap_err();
    assert_eq!(error.code(), "watch.tooManyEntries");
}

#[test]
fn missing_root_is_unreadable_not_empty() {
    let dir = tempfile::tempdir().unwrap();
    let error = take_snapshot(&[root(&dir.path().join("gone"), false)], 10).unwrap_err();
    assert_eq!(error.code(), "watch.unreadable");
}

#[cfg(unix)]
#[test]
fn symlinks_are_never_followed() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("secret.pdf"), b"s");
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    let snapshot = take_snapshot(&[root(dir.path(), true)], 10).unwrap();
    assert_eq!(snapshot.entries, 0);
}

fn snapshot(fingerprint: &str, entries: usize, newest_mtime_ms: i64) -> Snapshot {
    Snapshot {
        fingerprint: fingerprint.to_string(),
        entries,
        newest_mtime_ms,
    }
}

#[test]
fn recent_changes_wait_for_settle_then_prepare_once() {
    let mut state = ItemState::default();
    let recent = snapshot("a", 2, 1_000_000);
    assert_eq!(
        decide(&state, &recent, 1_010_000, SETTLE_MS),
        Decision::Settling
    );
    assert_eq!(
        decide(&state, &recent, 1_070_000, SETTLE_MS),
        Decision::Prepare
    );
    state.prepared_fingerprint = Some("a".to_string());
    assert_eq!(
        decide(&state, &recent, 1_200_000, SETTLE_MS),
        Decision::Nothing
    );
}

#[test]
fn empty_folder_needs_no_preparation() {
    let state = ItemState::default();
    assert_eq!(
        decide(&state, &snapshot("e", 0, 0), 9_999_999, SETTLE_MS),
        Decision::Nothing
    );
}

#[test]
fn failed_fingerprint_is_not_retried_until_it_changes() {
    let state = ItemState {
        failed_fingerprint: Some("a".to_string()),
        ..ItemState::default()
    };
    assert_eq!(
        decide(&state, &snapshot("a", 1, 0), 9_999_999, SETTLE_MS),
        Decision::Nothing
    );
    assert_eq!(
        decide(&state, &snapshot("b", 1, 0), 9_999_999, SETTLE_MS),
        Decision::Prepare
    );
}

#[test]
fn dismissed_fingerprint_stays_hidden_until_the_folder_changes() {
    let state = ItemState {
        dismissed_fingerprint: Some("a".to_string()),
        ..ItemState::default()
    };
    assert_eq!(
        decide(&state, &snapshot("a", 1, 0), 9_999_999, SETTLE_MS),
        Decision::Nothing
    );
    assert_eq!(
        decide(&state, &snapshot("b", 1, 0), 9_999_999, SETTLE_MS),
        Decision::Prepare
    );
}

fn summary(steps: usize, warnings: usize) -> PlanSummary {
    PlanSummary {
        plan_id: "plan-1".to_string(),
        steps,
        warnings,
    }
}

#[test]
fn queue_lists_ready_work_and_hides_nothing_to_do() {
    let queue = WatchQueue::default();
    queue.begin_prepare("rfi.group_files", "fp1");
    assert_eq!(queue.items()[0].status, ItemStatus::Preparing);
    queue.finish_prepared("rfi.group_files", "fp1", summary(4, 1), 100);
    let items = queue.items();
    assert_eq!(items[0].status, ItemStatus::Ready);
    assert_eq!(items[0].plan_id.as_deref(), Some("plan-1"));
    assert_eq!((items[0].steps, items[0].warnings), (4, 1));
    assert_eq!(queue.ready_count(), 1);

    queue.begin_prepare("rfi.complete_status_sync", "fp2");
    queue.finish_prepared("rfi.complete_status_sync", "fp2", summary(0, 0), 100);
    assert_eq!(queue.ready_count(), 1);
    assert!(
        queue
            .items()
            .iter()
            .all(|item| item.op_id != "rfi.complete_status_sync")
    );
}

#[test]
fn dismiss_removes_item_and_remembers_fingerprint() {
    let queue = WatchQueue::default();
    queue.begin_prepare("rfi.group_files", "fp1");
    queue.finish_prepared("rfi.group_files", "fp1", summary(2, 0), 100);
    queue.dismiss("rfi.group_files");
    assert_eq!(queue.ready_count(), 0);
    let state = queue.state("rfi.group_files");
    assert_eq!(
        decide(&state, &snapshot("fp1", 1, 0), 9_999_999, SETTLE_MS),
        Decision::Nothing
    );
}

#[test]
fn ready_items_go_stale_before_the_plan_ttl_and_on_fingerprint_drift() {
    let queue = WatchQueue::default();
    queue.begin_prepare("rfi.group_files", "fp1");
    queue.finish_prepared("rfi.group_files", "fp1", summary(2, 0), 1_000);
    queue.expire(1_000 + PLAN_FRESH_SECS - 1);
    assert_eq!(queue.items()[0].status, ItemStatus::Ready);
    queue.expire(1_000 + PLAN_FRESH_SECS);
    assert_eq!(queue.items()[0].status, ItemStatus::Stale);
    assert!(queue.items()[0].plan_id.is_none());

    queue.begin_prepare("rfi.group_files", "fp2");
    queue.finish_prepared("rfi.group_files", "fp2", summary(2, 0), 5_000);
    queue.mark_drift("rfi.group_files", "fp3");
    assert_eq!(queue.items()[0].status, ItemStatus::Stale);
}

#[test]
fn failures_surface_a_stable_code_and_are_bounded() {
    let queue = WatchQueue::default();
    queue.begin_prepare("rfi.group_files", "fp1");
    queue.finish_failed("rfi.group_files", "fp1", "watch.tooManyEntries");
    let items = queue.items();
    assert_eq!(items[0].status, ItemStatus::Failed);
    assert_eq!(items[0].error_code.as_deref(), Some("watch.tooManyEntries"));
    assert!(items.len() <= MAX_QUEUE);
}

#[test]
fn consumed_plan_leaves_the_queue() {
    let queue = WatchQueue::default();
    queue.begin_prepare("rfi.group_files", "fp1");
    queue.finish_prepared("rfi.group_files", "fp1", summary(2, 0), 100);
    queue.forget_plan("plan-1");
    assert!(queue.items().is_empty());
}

#[test]
fn profile_settings_default_off_and_ignore_unknown_operations() {
    let none = WatchSettings::from_section(None);
    assert!(none.enabled.is_empty());
    assert_eq!(none.poll, Duration::from_secs(DEFAULT_POLL_SECS));

    let section = serde_json::json!({
        "enabled": ["rfi.group_files", "files.batch_print", "rfi.group_files"],
        "pollSeconds": 1
    });
    let settings = WatchSettings::from_section(section.as_object().cloned());
    assert_eq!(settings.enabled, vec!["rfi.group_files".to_string()]);
    assert_eq!(settings.poll, Duration::from_secs(MIN_POLL_SECS));
}

#[test]
fn watcher_sources_have_no_apply_journal_or_mutation_calls() {
    let forbidden = [
        "apply_native_plan",
        "apply_with_progress",
        "claim_for_apply",
        "fs::write",
        "fs::rename",
        "fs::copy",
        "fs::remove",
        "fs::create_dir",
        "File::create",
        "journals",
        "try_native_undo",
        ".print.",
        ".outlook.",
    ];
    for (name, source) in [
        ("watch/mod.rs", include_str!("mod.rs")),
        ("native/watch.rs", include_str!("../native/watch.rs")),
    ] {
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for token in forbidden {
            assert!(!code.contains(token), "{name} must not contain {token}");
        }
    }
}
