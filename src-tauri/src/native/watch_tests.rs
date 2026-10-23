use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde_json::json;

use super::tests::{WORK_ITEM_TEST_LOCK, test_backend};
use super::*;
use crate::operations::rfi_file_group;

const OP: &str = rfi_file_group::OPERATION_ID;

fn put(source: &Path, number: u32, bytes: &[u8]) -> PathBuf {
    let stem = format!("AZE-SUN-INT-QMS-RFI-CIV-{number}");
    fs::write(source.join(format!("{stem}.eml")), b"Subject: RFI package").unwrap();
    let pdf = source.join(format!("{stem}.pdf"));
    fs::write(&pdf, bytes).unwrap();
    pdf
}

fn age_all(source: &Path) {
    for entry in fs::read_dir(source).unwrap() {
        age(&entry.unwrap().path());
    }
}

fn age(path: &Path) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(600))
        .unwrap();
}

fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                out.push((path.clone(), fs::read(path).unwrap()));
            }
        }
    }
    out.sort();
    out
}

struct Fixture {
    _temporary: tempfile::TempDir,
    backend: NativeBackend,
    source: PathBuf,
    destination: PathBuf,
}

fn fixture() -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    let backend = test_backend(&temporary);
    let source = temporary.path().join("incoming");
    let destination = temporary.path().join("grouped");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let mut values = JsonObject::new();
    values.insert("source".into(), json!(source));
    values.insert("destination".into(), json!(destination));
    backend.settings.save_params(OP, values).unwrap();
    Fixture {
        _temporary: temporary,
        backend,
        source,
        destination,
    }
}

fn enable(backend: &NativeBackend) {
    backend
        .watch_dispatch_update(json!({"opId": OP, "enabled": true}))
        .unwrap();
}

#[test]
fn disabled_profiles_prepare_nothing() {
    let _guard = WORK_ITEM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let f = fixture();
    put(&f.source, 1263, b"scan");
    age_all(&f.source);
    f.backend.watch_tick();
    assert!(f.backend.watch.items().is_empty());
}

#[test]
fn fresh_files_settle_before_a_preview_is_prepared() {
    let _guard = WORK_ITEM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let f = fixture();
    enable(&f.backend);
    put(&f.source, 1263, b"scan");
    f.backend.watch_tick();
    assert!(f.backend.watch.items().is_empty());
}

#[test]
fn settled_folder_yields_a_ready_preview_without_touching_any_file() {
    let _guard = WORK_ITEM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let f = fixture();
    enable(&f.backend);
    put(&f.source, 1263, b"source stays unchanged");
    age_all(&f.source);
    let before = (tree(&f.source), tree(&f.destination));

    f.backend.watch_tick();

    let items = f.backend.watch.items();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0].op_id, OP);
    assert_eq!(items[0].status, crate::watch::ItemStatus::Ready);
    assert!(items[0].steps > 0);
    assert_eq!(before, (tree(&f.source), tree(&f.destination)));
    assert!(
        fs::read_dir(f.backend.journals.root()).map_or(true, |mut dir| dir.next().is_none()),
        "watcher must not write a journal"
    );

    // Unchanged folder is not prepared again.
    let plan_id = items[0].plan_id.clone();
    f.backend.watch_tick();
    assert_eq!(f.backend.watch.items()[0].plan_id, plan_id);

    // The queued preview is a normal plan that can be opened and applied by the operator.
    let preview = f.backend.watch_dispatch_open(json!({"opId": OP})).unwrap();
    let plan_id = preview["planId"].as_str().unwrap().to_string();
    let (result, _) = f
        .backend
        .apply_native_plan(&plan_id, Some(OP), |_| {})
        .unwrap();
    assert_eq!(result["failed"], 0);
    f.backend.watch_forget_plan(&plan_id);
    assert!(f.backend.watch.items().is_empty());
}

#[test]
fn changed_source_marks_the_queued_preview_stale() {
    let _guard = WORK_ITEM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let f = fixture();
    enable(&f.backend);
    put(&f.source, 1263, b"one");
    age_all(&f.source);
    f.backend.watch_tick();
    assert_eq!(
        f.backend.watch.items()[0].status,
        crate::watch::ItemStatus::Ready
    );

    // A new file appears; it is still settling, so the old preview is stale.
    put(&f.source, 1264, b"two");
    f.backend.watch_tick();
    let items = f.backend.watch.items();
    assert_eq!(items[0].status, crate::watch::ItemStatus::Stale);
    assert!(f.backend.watch_dispatch_open(json!({"opId": OP})).is_err());
}

#[test]
fn operator_activity_pauses_the_watcher() {
    let _guard = WORK_ITEM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let f = fixture();
    enable(&f.backend);
    put(&f.source, 1263, b"scan");
    age_all(&f.source);
    let busy = f.backend.watch_operator_guard();
    f.backend.watch_tick();
    assert!(f.backend.watch.items().is_empty());
    drop(busy);
    f.backend.watch_tick();
    assert_eq!(f.backend.watch.items().len(), 1);
}

#[test]
fn update_rejects_operations_that_cannot_be_watched() {
    let _guard = WORK_ITEM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let f = fixture();
    for id in [
        "files.batch_print",
        "rfi.system_sync",
        "files.replica_publish",
    ] {
        let error = f
            .backend
            .watch_dispatch_update(json!({"opId": id, "enabled": true}))
            .unwrap_err();
        assert_eq!(error.code, "validation", "{id}");
    }
}

#[test]
fn status_reports_which_operations_have_remembered_folders() {
    let _guard = WORK_ITEM_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let f = fixture();
    let status = f.backend.watch_dispatch_status(json!({})).unwrap();
    assert_eq!(status["configured"], json!([OP]));
    fs::remove_dir_all(&f.source).unwrap();
    let status = f.backend.watch_dispatch_status(json!({})).unwrap();
    assert_eq!(status["configured"], json!([]));
}
