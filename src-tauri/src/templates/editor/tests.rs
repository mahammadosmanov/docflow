use std::{
    collections::HashMap,
    io::{Cursor, Write},
    sync::{Arc, Mutex},
};

use serde_json::json;
use zip::{ZipWriter, write::SimpleFileOptions};

use super::*;
use crate::templates::TemplateRemote;

const TEMPLATE_ID: &str = "11111111-1111-4111-8111-111111111111";
const VERSION_ID: &str = "22222222-2222-4222-8222-222222222222";

fn docx(text: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in [
        ("[Content_Types].xml", "<Types/>".to_string()),
        (
            "word/document.xml",
            format!(
                "<w:document><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"
            ),
        ),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start");
        writer.write_all(body.as_bytes()).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

#[derive(Default)]
struct Scripted {
    calls: Mutex<Vec<(String, String, Option<Value>)>>,
    files: Mutex<HashMap<String, Vec<u8>>>,
    rows: Mutex<HashMap<String, Value>>,
}

impl TemplateRemote for Scripted {
    fn manifest(&self, _now: i64) -> Result<Value, BackendError> {
        Ok(json!([]))
    }

    fn download(&self, path: &str, _now: i64) -> Result<Vec<u8>, BackendError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(BackendError::offline)
    }

    fn rest(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        _now: i64,
    ) -> Result<Value, BackendError> {
        self.calls
            .lock()
            .unwrap()
            .push((method.to_string(), path.to_string(), body.clone()));
        let rows = self.rows.lock().unwrap();
        let key = format!("{method} {}", path.split('?').next().unwrap_or(path));
        if method == "POST" && path == "doc_template_versions" {
            let mut row = body.expect("body");
            row["id"] = json!(VERSION_ID);
            return Ok(json!([row]));
        }
        if method == "PATCH" && path.starts_with("doc_template_versions") {
            let mut row = rows.get("GET doc_template_versions").expect("version")[0].clone();
            for (field, value) in body.expect("body").as_object().expect("object") {
                row[field] = value.clone();
            }
            return Ok(json!([row]));
        }
        rows.get(&key)
            .cloned()
            .ok_or_else(|| BackendError::validation(format!("unscripted {key}")))
    }

    fn upload(
        &self,
        path: &str,
        _kind: FileKind,
        bytes: &[u8],
        _now: i64,
    ) -> Result<(), BackendError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), bytes.to_vec());
        Ok(())
    }
}

fn template_row(versions: Value) -> Value {
    json!([{"id": TEMPLATE_ID, "code": "RFI-ELE", "title": "RFI Cover ELE", "discipline": "ELE",
            "file_kind": "docx", "current_version": 1, "doc_template_versions": versions}])
}

fn store(remote: Arc<Scripted>) -> (tempfile::TempDir, TemplateStore) {
    let directory = tempfile::tempdir().expect("temp");
    let store = TemplateStore::new(directory.path().join("templates"), remote);
    (directory, store)
}

#[test]
fn creating_a_template_uploads_once_and_prefills_known_placeholders() {
    let remote = Arc::new(Scripted::default());
    remote
        .rows
        .lock()
        .unwrap()
        .insert("POST doc_templates".into(), template_row(json!([])));
    let (_dir, store) = store(remote.clone());
    let bytes = docx("{{rfiNo}} {{stage}}");
    let draft = store
        .create(
            &NewTemplate {
                code: "RFI-ELE".into(),
                title: "RFI Cover ELE".into(),
                discipline: "ELE".into(),
            },
            "cover.docx",
            &bytes,
            1,
        )
        .expect("created");
    assert_eq!(draft.version, 1);
    assert_eq!(draft.placeholders, ["rfiNo", "stage"]);
    assert_eq!(draft.field_map.fields[0].value, "{rfiNo}");
    let files = remote.files.lock().unwrap();
    assert_eq!(files.len(), 1);
    let path = files.keys().next().unwrap();
    assert!(path.starts_with(&format!("{TEMPLATE_ID}/1/")) && path.ends_with(".docx"));
}

#[test]
fn opening_a_published_template_makes_a_new_draft_with_the_same_file() {
    let remote = Arc::new(Scripted::default());
    let bytes = docx("{{rfiNo}}");
    let path = format!("{TEMPLATE_ID}/1/{}.docx", sha256_hex(&bytes));
    remote
        .files
        .lock()
        .unwrap()
        .insert(path.clone(), bytes.clone());
    remote.rows.lock().unwrap().insert(
        "GET doc_templates".into(),
        template_row(json!([{"id": VERSION_ID, "version": 1, "status": "published"}])),
    );
    remote.rows.lock().unwrap().insert(
        "GET doc_template_versions".into(),
        json!([{"id": VERSION_ID, "template_id": TEMPLATE_ID, "version": 1, "status": "published",
                "file_sha256": sha256_hex(&bytes), "file_size": bytes.len(), "storage_path": path,
                "field_map": {"schema": 1, "fields": [{"key": "rfiNo", "label": "RFI", "value": "{rfiNo}"}]}}]),
    );
    let (_dir, store) = store(remote.clone());
    let draft = store.open_draft("RFI-ELE", 1).expect("draft");
    assert_eq!(draft.version, 2);
    assert_eq!(draft.field_map.fields[0].label, "RFI");
    let calls = remote.calls.lock().unwrap();
    let created = calls
        .iter()
        .find(|(method, path, _)| method == "POST" && path == "doc_template_versions")
        .expect("insert");
    assert_eq!(created.2.as_ref().unwrap()["status"], "draft");
    assert_eq!(created.2.as_ref().unwrap()["storage_path"], json!(path));
}

#[test]
fn published_versions_and_bad_maps_are_refused() {
    let remote = Arc::new(Scripted::default());
    remote.rows.lock().unwrap().insert(
        "GET doc_template_versions".into(),
        json!([{"id": VERSION_ID, "template_id": TEMPLATE_ID, "version": 1, "status": "published",
                "file_sha256": "0".repeat(64), "file_size": 1, "storage_path": "x", "field_map": {"schema": 1}}]),
    );
    let (_dir, store) = store(remote.clone());
    let error = store.publish(VERSION_ID, 1).expect_err("published");
    assert_eq!(error.message, "error.templates.notDraft");
    let error = store
        .save_draft(
            &SaveDraft {
                version_id: VERSION_ID.into(),
                title: "T".into(),
                discipline: "ELE".into(),
                field_map: json!({"schema": 1, "fields": [{"key": "a"}, {"key": "a"}]}),
            },
            1,
        )
        .expect_err("invalid map");
    assert!(error.message.starts_with("error.templates.invalid"));
    assert!(store.publish("not-a-uuid", 1).is_err());
}

#[test]
fn only_office_files_of_a_known_kind_are_accepted() {
    let bytes = docx("x");
    assert_eq!(
        file_kind_of("A.DOCX", &bytes).expect("docx"),
        FileKind::Docx
    );
    assert!(file_kind_of("a.xlsx", &bytes).is_err());
    assert!(file_kind_of("a.pdf", &bytes).is_err());
    assert!(check_identity(Some("RFI ELE"), "t", "ELE").is_err());
    assert!(check_identity(Some("RFI-ELE"), "t", "ele").is_err());
}
