use std::{
    collections::HashMap,
    io::{Cursor, Write},
    sync::Mutex,
};

use serde_json::json;
use zip::{ZipWriter, write::SimpleFileOptions};

use super::*;

const TEMPLATE_ID: &str = "11111111-1111-4111-8111-111111111111";

fn package(kind: FileKind, marker: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for name in ["[Content_Types].xml", kind.main_part()] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start member");
        writer.write_all(marker.as_bytes()).expect("write member");
    }
    writer.finish().expect("finish zip").into_inner()
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn row(code: &str, version: u32, bytes: &[u8]) -> Value {
    let hash = sha(bytes);
    json!({
        "template_id": TEMPLATE_ID,
        "code": code,
        "title": "RFI Form",
        "discipline": "GEN",
        "file_kind": "xlsx",
        "version_id": format!("v{version}"),
        "version": version,
        "file_sha256": hash,
        "file_size": bytes.len(),
        "storage_path": format!("{TEMPLATE_ID}/{version}/{hash}.xlsx"),
        "field_map": {"schema": 1, "fields": [{"key": "rfi_no"}]},
        "published_at": "2026-11-07T00:00:00Z",
        "updated_at": "2026-11-07T00:00:00Z"
    })
}

#[derive(Default)]
struct FakeRemote {
    manifest: Mutex<Option<Result<Value, BackendError>>>,
    files: Mutex<HashMap<String, Vec<u8>>>,
    downloads: Mutex<usize>,
}

impl FakeRemote {
    fn serve(&self, rows: Vec<Value>, files: &[(&Value, &[u8])]) {
        *self.manifest.lock().expect("lock") = Some(Ok(Value::Array(rows)));
        let mut stored = self.files.lock().expect("lock");
        stored.clear();
        for (row, bytes) in files {
            stored.insert(
                row["storage_path"].as_str().expect("path").to_string(),
                bytes.to_vec(),
            );
        }
    }

    fn fail(&self, error: BackendError) {
        *self.manifest.lock().expect("lock") = Some(Err(error));
    }
}

impl TemplateRemote for FakeRemote {
    fn manifest(&self, _now: i64) -> Result<Value, BackendError> {
        self.manifest
            .lock()
            .expect("lock")
            .clone()
            .unwrap_or_else(|| Err(BackendError::offline()))
    }

    fn download(&self, storage_path: &str, _now: i64) -> Result<Vec<u8>, BackendError> {
        *self.downloads.lock().expect("lock") += 1;
        self.files
            .lock()
            .expect("lock")
            .get(storage_path)
            .cloned()
            .ok_or_else(BackendError::offline)
    }
}

fn store() -> (tempfile::TempDir, Arc<FakeRemote>, TemplateStore) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let remote = Arc::new(FakeRemote::default());
    let store = TemplateStore::new(directory.path().join("templates"), remote.clone());
    (directory, remote, store)
}

#[test]
fn sync_downloads_verifies_and_caches_published_templates() {
    let (_directory, remote, store) = store();
    let bytes = package(FileKind::Xlsx, "v1");
    let first = row("QM-FRM-001", 1, &bytes);
    remote.serve(vec![first.clone()], &[(&first, &bytes)]);

    let status = store.sync(100);
    assert!(status.online);
    assert_eq!(status.last_synced_at, Some(100));
    assert_eq!(status.templates.len(), 1);
    assert!(status.templates[0].cached);
    assert_eq!(status.templates[0].field_count, 1);
    let (entry, path) = store.resolve("qm-frm-001").expect("cached template");
    assert_eq!(entry.version, 1);
    assert_eq!(fs::read(path).expect("read cache"), bytes);

    store.sync(200);
    assert_eq!(
        *remote.downloads.lock().expect("lock"),
        1,
        "unchanged file re-downloaded"
    );
}

#[test]
fn offline_sync_keeps_the_cached_manifest() {
    let (_directory, remote, store) = store();
    let bytes = package(FileKind::Xlsx, "v1");
    let first = row("QM-FRM-001", 1, &bytes);
    remote.serve(vec![first.clone()], &[(&first, &bytes)]);
    store.sync(100);

    remote.fail(BackendError::offline());
    let status = store.sync(200);
    assert!(!status.online);
    assert_eq!(status.last_error.as_deref(), Some("offline"));
    assert_eq!(status.last_synced_at, Some(100));
    assert!(store.resolve("QM-FRM-001").is_some());
}

#[test]
fn a_failed_download_keeps_the_previous_version() {
    let (_directory, remote, store) = store();
    let old = package(FileKind::Xlsx, "v1");
    let first = row("QM-FRM-001", 1, &old);
    remote.serve(vec![first.clone()], &[(&first, &old)]);
    store.sync(100);

    let new = package(FileKind::Xlsx, "v2");
    let second = row("QM-FRM-001", 2, &new);
    remote.serve(vec![second], &[]);
    let status = store.sync(200);
    assert_eq!(status.failed_codes, vec!["QM-FRM-001".to_string()]);
    assert_eq!(
        store.resolve("QM-FRM-001").expect("old version").0.version,
        1
    );
}

#[test]
fn a_file_that_does_not_match_its_checksum_is_rejected() {
    let (_directory, remote, store) = store();
    let bytes = package(FileKind::Xlsx, "v1");
    let first = row("QM-FRM-001", 1, &bytes);
    let tampered = package(FileKind::Xlsx, "v1-tampered");
    remote.serve(vec![first.clone()], &[(&first, &tampered)]);

    let status = store.sync(100);
    assert_eq!(status.failed_codes, vec!["QM-FRM-001".to_string()]);
    assert!(status.templates.is_empty());
    assert!(store.resolve("QM-FRM-001").is_none());
}

#[test]
fn a_damaged_cache_file_is_not_used() {
    let (_directory, remote, store) = store();
    let bytes = package(FileKind::Xlsx, "v1");
    let first = row("QM-FRM-001", 1, &bytes);
    remote.serve(vec![first.clone()], &[(&first, &bytes)]);
    store.sync(100);
    let (_, path) = store.resolve("QM-FRM-001").expect("cached");
    fs::write(&path, b"damaged").expect("damage cache");

    assert!(store.resolve("QM-FRM-001").is_none());
    assert!(!store.status().templates[0].cached);
}

#[test]
fn unsafe_manifest_entries_are_skipped() {
    let (_directory, remote, store) = store();
    let bytes = package(FileKind::Xlsx, "v1");
    let mut escape = row("QM-FRM-001", 1, &bytes);
    escape["storage_path"] = json!(format!("{TEMPLATE_ID}/../{}.xlsx", sha(&bytes)));
    let mut bad_code = row("../x", 1, &bytes);
    bad_code["template_id"] = json!("other");
    remote.serve(vec![escape, bad_code], &[]);

    let status = store.sync(100);
    assert!(status.online);
    assert_eq!(status.failed_codes.len(), 2);
    assert!(status.templates.is_empty());
}

#[test]
fn a_file_of_the_wrong_office_kind_is_rejected() {
    let docx = package(FileKind::Docx, "word");
    assert!(verify_package(FileKind::Xlsx, &docx).is_err());
    assert!(verify_package(FileKind::Docx, &docx).is_ok());
    assert!(verify_package(FileKind::Docx, b"not a zip").is_err());
}

#[test]
fn missing_supabase_setup_is_reported_as_not_configured() {
    let (_directory, remote, store) = store();
    remote.fail(BackendError::request_failed("templates_not_configured"));
    assert_eq!(store.sync(1).last_error.as_deref(), Some("notConfigured"));
    remote.fail(BackendError::auth_required());
    assert_eq!(store.sync(2).last_error.as_deref(), Some("signedOut"));
}
