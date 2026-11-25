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
                code: "ELE-RFI-COVER".into(),
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

#[test]
fn archiving_hides_a_template_for_everyone() {
    let remote = Arc::new(Scripted::default());
    remote
        .rows
        .lock()
        .unwrap()
        .insert("GET doc_templates".into(), template_row(json!([])));
    remote
        .rows
        .lock()
        .unwrap()
        .insert("PATCH doc_templates".into(), json!([]));
    let (_dir, store) = store(remote.clone());
    store.archive("RFI-ELE", 1).expect("archived");
    let calls = remote.calls.lock().unwrap();
    let patch = calls
        .iter()
        .find(|(method, _, _)| method == "PATCH")
        .expect("patch");
    assert_eq!(patch.1, format!("doc_templates?id=eq.{TEMPLATE_ID}"));
    assert_eq!(patch.2, Some(json!({"archived": true})));
    assert!(store.archive("bad code", 1).is_err());
}

#[test]
fn marking_text_uploads_a_new_draft_file_with_the_field() {
    let remote = Arc::new(Scripted::default());
    let bytes = docx("Date: 07.11.2026");
    let path = format!("{TEMPLATE_ID}/2/{}.docx", sha256_hex(&bytes));
    remote
        .files
        .lock()
        .unwrap()
        .insert(path.clone(), bytes.clone());
    remote
        .rows
        .lock()
        .unwrap()
        .insert("GET doc_templates".into(), template_row(json!([])));
    remote.rows.lock().unwrap().insert(
        "GET doc_template_versions".into(),
        json!([{"id": VERSION_ID, "template_id": TEMPLATE_ID, "version": 2, "status": "draft",
                "file_sha256": sha256_hex(&bytes), "file_size": bytes.len(), "storage_path": path,
                "field_map": {"schema": 1, "fields": []}}]),
    );
    let (_dir, store) = store(remote.clone());
    let block = &store.blocks(VERSION_ID, 1).expect("blocks")[0];
    assert_eq!(block.text, "Date: 07.11.2026");

    let request = |field: Option<NewField>, replacement: &str| EditText {
        version_id: VERSION_ID.into(),
        part: block.part.clone(),
        index: block.index,
        start: 6,
        end: 16,
        expected: "07.11.2026".into(),
        replacement: replacement.into(),
        field,
    };
    let draft = store
        .edit_text(
            &request(
                Some(NewField {
                    key: "dateOfIssue".into(),
                    label: "Date".into(),
                    value: "{dateOfIssue}".into(),
                }),
                "",
            ),
            1,
        )
        .expect("marked");
    assert_eq!(draft.placeholders, ["dateOfIssue"]);
    assert_eq!(draft.field_map.fields[0].label, "Date");
    assert_eq!(draft.field_map.fields[0].value, "{dateOfIssue}");
    assert_eq!(
        remote.files.lock().unwrap().len(),
        2,
        "edited file uploaded beside the old one"
    );

    assert!(
        store.edit_text(&request(None, "{{x}}"), 1).is_err(),
        "no raw placeholders"
    );
    assert!(
        store
            .edit_text(
                &request(
                    Some(NewField {
                        key: "1bad".into(),
                        label: "x".into(),
                        value: String::new()
                    }),
                    ""
                ),
                1
            )
            .is_err()
    );
}

fn table_docx() -> Vec<u8> {
    table_docx_with("Old")
}

fn table_docx_with(value: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let body = format!(
        "<w:document><w:body><w:tbl><w:tr>\
         <w:tc><w:p><w:r><w:t>Label</w:t></w:r></w:p></w:tc>\
         <w:tc><w:p><w:r><w:t>{value}</w:t></w:r></w:p></w:tc>\
         </w:tr></w:tbl></w:body></w:document>"
    );
    for (name, text) in [
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", body.as_str()),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start");
        writer.write_all(text.as_bytes()).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

fn draft_store(bytes: &[u8]) -> (Arc<Scripted>, tempfile::TempDir, TemplateStore) {
    let remote = Arc::new(Scripted::default());
    let path = format!("{TEMPLATE_ID}/2/{}.docx", sha256_hex(bytes));
    remote
        .files
        .lock()
        .unwrap()
        .insert(path.clone(), bytes.to_vec());
    remote
        .rows
        .lock()
        .unwrap()
        .insert("GET doc_templates".into(), template_row(json!([])));
    remote.rows.lock().unwrap().insert(
        "GET doc_template_versions".into(),
        json!([{"id": VERSION_ID, "template_id": TEMPLATE_ID, "version": 2, "status": "draft",
                "file_sha256": sha256_hex(bytes), "file_size": bytes.len(), "storage_path": path,
                "field_map": {"schema": 1, "fields": []}}]),
    );
    remote
        .rows
        .lock()
        .unwrap()
        .insert("PATCH doc_templates".into(), json!([]));
    let (directory, store) = store(remote.clone());
    (remote, directory, store)
}

fn save(field_map: Value) -> SaveDraft {
    SaveDraft {
        version_id: VERSION_ID.into(),
        code: "ELE-RFI-COVER".into(),
        title: "T".into(),
        discipline: "ELE".into(),
        field_map,
    }
}

#[test]
fn the_cell_view_of_a_draft_file_comes_from_rust() {
    let (_remote, _dir, store) = draft_store(&table_docx());
    let value = serde_json::to_value(store.cells(VERSION_ID, 1).expect("cells")).expect("json");
    assert_eq!(value["tables"][0]["rows"][0]["cells"][1]["text"], "Old");
}

#[test]
fn a_marked_cell_is_saved_as_schema_two_after_a_dry_run_on_the_file() {
    let (remote, _dir, store) = draft_store(&table_docx());
    let map = json!({"schema": 1, "fileName": "{{rfiNo}}", "presets": [], "fields": [
        {"key": "rfiNo", "label": "RFI", "value": "{rfiNo}", "options": [], "multiline": false,
         "anchor": {"table": 0, "row": 0, "cell": 1}}
    ]});
    let draft = store.save_draft(&save(map), 1).expect("saved");
    assert_eq!(
        draft.field_map.fields[0]
            .anchor
            .as_ref()
            .and_then(|a| a.cell),
        Some(1)
    );
    let calls = remote.calls.lock().unwrap();
    let patch = calls
        .iter()
        .find(|(method, path, _)| method == "PATCH" && path.starts_with("doc_template_versions"))
        .and_then(|(_, _, body)| body.clone())
        .expect("version patch");
    assert_eq!(patch["field_map"]["schema"], 2);
    assert_eq!(patch["field_map"]["slots"][0]["anchor"]["kind"], "cell");
}

#[test]
fn a_cell_that_cannot_be_written_is_refused_before_anything_is_saved() {
    let (remote, _dir, store) = draft_store(&table_docx());
    let map = json!({"schema": 1, "fields": [
        {"key": "a", "anchor": {"table": 0, "row": 0, "cell": 7}}
    ]});
    let error = store.save_draft(&save(map), 1).expect_err("missing cell");
    assert!(
        error.message.starts_with("error.templates.invalid"),
        "{}",
        error.message
    );
    assert!(
        remote
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|(method, _, _)| method != "PATCH"),
        "nothing was written"
    );
}

#[test]
fn other_filled_copies_suggest_the_cells_that_change() {
    let (_remote, _dir, store) = draft_store(&table_docx());
    let copies = vec![("copy.docx".to_string(), table_docx_with("New"))];
    let found = store
        .suggest_from_copies(VERSION_ID, &copies, 1)
        .expect("suggestions");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].anchor.cell, Some(1));
    assert_eq!(found[0].label, "Label");
    assert_eq!(found[0].samples, ["Old", "New"]);

    let wrong_kind = vec![("copy.xlsx".to_string(), table_docx_with("New"))];
    assert!(
        store
            .suggest_from_copies(VERSION_ID, &wrong_kind, 1)
            .is_err()
    );
}

fn rows_docx(rows: &[(&str, &str)]) -> Vec<u8> {
    let body: String = rows
        .iter()
        .map(|(label, value)| {
            format!(
                "<w:tr><w:tc><w:p><w:r><w:t>{label}</w:t></w:r></w:p></w:tc>\
                 <w:tc><w:p><w:r><w:t>{value}</w:t></w:r></w:p></w:tc></w:tr>"
            )
        })
        .collect();
    let xml = format!("<w:document><w:body><w:tbl>{body}</w:tbl></w:body></w:document>");
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in [
        ("[Content_Types].xml", "<Types/>"),
        ("word/document.xml", xml.as_str()),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start");
        writer.write_all(text.as_bytes()).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

fn anchored_draft_store(bytes: &[u8]) -> (Arc<Scripted>, tempfile::TempDir, TemplateStore) {
    let (remote, dir, store) = draft_store(bytes);
    let mut rows = remote.rows.lock().unwrap();
    let row = rows.get_mut("GET doc_template_versions").expect("row");
    row[0]["field_map"] = json!({"schema": 2, "slots": [
        {"key": "poured", "anchor": {"kind": "cell", "table": 0, "row": 0, "cell": 1}}
    ]});
    drop(rows);
    (remote, dir, store)
}

#[test]
fn replacing_the_file_reports_where_each_marked_cell_went() {
    let (_remote, _dir, store) = anchored_draft_store(&rows_docx(&[("Pour no:", "P1")]));
    let new = rows_docx(&[("Header", ""), ("Pour no:", "P2")]);
    let draft = store
        .replace_file(VERSION_ID, "new.docx", &new, 1)
        .expect("replaced");
    assert_eq!(draft.checks.len(), 1);
    assert_eq!(draft.checks[0].key, "poured");
    assert_eq!(
        draft.checks[0].status,
        crate::templates::remap::CheckStatus::Moved
    );
    assert_eq!(
        draft.checks[0].moved_to.as_ref().and_then(|a| a.row),
        Some(1)
    );

    let gone = rows_docx(&[("Something else:", "x")]);
    let draft = store
        .replace_file(VERSION_ID, "new.docx", &gone, 1)
        .expect("replaced");
    assert_eq!(
        draft.checks[0].status,
        crate::templates::remap::CheckStatus::Missing
    );
}

#[test]
fn publishing_refuses_a_marked_cell_the_file_cannot_write() {
    let (remote, _dir, store) = anchored_draft_store(&rows_docx(&[("Pour no:", "P1")]));
    remote
        .rows
        .lock()
        .unwrap()
        .get_mut("GET doc_template_versions")
        .unwrap()[0]["field_map"] = json!({"schema": 2, "slots": [
        {"key": "poured", "anchor": {"kind": "cell", "table": 0, "row": 5, "cell": 1}}
    ]});
    let error = store.publish(VERSION_ID, 1).expect_err("unwritable");
    assert!(
        error.message.starts_with("error.templates.invalid"),
        "{}",
        error.message
    );
    assert!(
        remote
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|(_, path, _)| !path.contains("publish_template_version")),
        "nothing was published"
    );
}

#[test]
fn versions_are_listed_newest_first_and_an_old_one_can_become_the_draft() {
    let (remote, _dir, store) = draft_store(&rows_docx(&[("A:", "1")]));
    {
        let mut rows = remote.rows.lock().unwrap();
        let mut old = rows.get("GET doc_template_versions").unwrap()[0].clone();
        old["status"] = json!("published");
        old["version"] = json!(1);
        old["published_at"] = json!("2026-11-07T00:00:00Z");
        rows.insert("GET doc_template_versions".into(), json!([old]));
        rows.insert(
            "GET doc_templates".into(),
            template_row(json!([{"id": VERSION_ID, "version": 1, "status": "published"}])),
        );
    }
    let list = store.versions("RFI-ELE", 1).expect("versions");
    assert_eq!(list[0].version, 1);
    assert_eq!(list[0].status, "published");

    let draft = store
        .restore_version("RFI-ELE", VERSION_ID, 1)
        .expect("restored");
    assert_eq!(draft.version, 2, "a new draft after the latest version");
    let calls = remote.calls.lock().unwrap();
    let post = calls
        .iter()
        .find(|(method, path, _)| method == "POST" && path == "doc_template_versions")
        .and_then(|(_, _, body)| body.clone())
        .expect("draft created");
    assert_eq!(post["status"], "draft");
    assert_eq!(post["version"], 2);
}

#[test]
fn optional_sheets_are_refused_for_a_word_template_before_anything_is_saved() {
    let (remote, _dir, store) = draft_store(&table_docx());
    let map = json!({"schema": 2, "slots": [{"key": "a"}], "optionalSheets": ["Photos"]});
    let error = store
        .save_draft(&save(map), 1)
        .expect_err("word has no sheets");
    assert!(
        error.message.starts_with("error.templates.invalid"),
        "{}",
        error.message
    );
    assert!(
        remote
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|(method, _, _)| method != "PATCH")
    );
}
