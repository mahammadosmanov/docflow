use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use docflow_lib::{
    auth::{AuthConfig, AuthService},
    cloud::CloudHistory,
    credentials::{CredentialError, CredentialStore, StoredSession},
    fs_ops::file_sha256,
    http::{HttpError, HttpResponse, JsonHttp},
    settings::SettingsStore,
};
use serde_json::{Value, json};

#[derive(Clone)]
struct FixtureHttp {
    responses: Arc<Mutex<VecDeque<HttpResponse>>>,
    requests: Arc<Mutex<Vec<FixtureRequest>>>,
}

struct FixtureRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl FixtureHttp {
    fn new(responses: Vec<HttpResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl JsonHttp for FixtureHttp {
    fn send_json(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<HttpResponse, HttpError> {
        self.requests
            .lock()
            .expect("request lock")
            .push(FixtureRequest {
                method: method.to_string(),
                url: url.to_string(),
                headers: headers.to_vec(),
                body: body.cloned().unwrap_or(Value::Null),
            });
        self.responses
            .lock()
            .expect("response lock")
            .pop_front()
            .ok_or(HttpError::InvalidResponse)
    }
}

fn restored_auth(
    directory: &tempfile::TempDir,
    http: Arc<FixtureHttp>,
    now: i64,
) -> Arc<AuthService> {
    let settings = Arc::new(SettingsStore::new(directory.path().join("settings.json")).unwrap());
    settings
        .save_session_metadata("user-id", "e@example.com", now + 3600)
        .expect("public session metadata");
    let auth = Arc::new(AuthService::new(
        settings,
        Arc::new(FixtureCredentials(StoredSession::new(
            "access-secret".to_string(),
            "refresh-secret".to_string(),
            "user-id".to_string(),
            "e@example.com".to_string(),
            now + 3600,
        ))),
        http,
        AuthConfig::new("http://supabase.test", "publishable"),
    ));
    auth.restore(now).expect("secure session should restore");
    auth
}

struct FixtureCredentials(StoredSession);

impl CredentialStore for FixtureCredentials {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
        Ok(Some(self.0.clone()))
    }

    fn write(&self, _session: &StoredSession) -> Result<(), CredentialError> {
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        Ok(())
    }
}

#[test]
fn cloud_rows_are_metadata_only_and_only_reversible_local_journals_enable_undo() {
    let now = 1_800_000_000_i64;
    let directory = tempfile::tempdir().expect("temporary directory");
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(
            200,
            &json!([{
                "id": "user-id",
                "email": "e@example.com",
                "allowed": true,
                "is_admin": false
            }]),
        ),
        HttpResponse::from_json(
            200,
            &json!([
                {
                    "id": "cloud-1",
                    "created_at": "2026-08-25T00:00:00Z",
                    "op_id": "rfi.process_scans",
                    "applied": 1,
                    "failed": 0,
                    "journal_id": "local-1",
                    "steps": [{"kind": "move", "src": "/forged", "dst": "/victim"}],
                    "undone": false,
                    "user_email": "e@example.com"
                },
                {
                    "id": "cloud-2",
                    "created_at": "2026-08-24T00:00:00Z",
                    "op_id": "rfi.process_scans",
                    "applied": 2,
                    "failed": 1,
                    "journal_id": "remote-only",
                    "steps": [{"kind": "move", "src": "/other", "dst": "/target"}],
                    "undone": true,
                    "user_email": "other@example.com"
                },
                {
                    "id": "cloud-broken",
                    "created_at": "2026-08-23T12:00:00Z",
                    "op_id": "rfi.process_scans",
                    "applied": 1,
                    "failed": 0,
                    "journal_id": "broken-local",
                    "undone": false,
                    "user_email": "e@example.com"
                },
                {
                    "id": "cloud-print",
                    "created_at": "2026-08-23T00:00:00Z",
                    "op_id": "files.batch_print",
                    "applied": 1,
                    "failed": 0,
                    "journal_id": "print-audit",
                    "undone": false,
                    "user_email": "e@example.com"
                }
            ]),
        ),
    ]));
    let auth = restored_auth(&directory, http.clone(), now);
    let journal_dir = directory.path().join("journal");
    std::fs::create_dir(&journal_dir).expect("journal directory");
    let documents = directory.path().join("documents");
    std::fs::create_dir(&documents).expect("documents directory");
    let documents = std::fs::canonicalize(documents).expect("canonical documents directory");
    let source = documents.join("source.txt");
    let destination = documents.join("copy.txt");
    std::fs::write(&source, b"LOCAL APPROVED").expect("source fixture");
    std::fs::write(&destination, b"LOCAL APPROVED").expect("destination fixture");
    let undo_hash = file_sha256(&destination).expect("fixture hash");
    std::fs::write(
        journal_dir.join("2026-08-25_local.json"),
        serde_json::to_vec(&json!({
            "journalId": "local-1",
            "opId": "rfi.process_scans",
            "ts": "2026-08-25T00:00:00Z",
            "steps": [{
                "kind": "copy",
                "src": source,
                "dst": destination,
                "noteKey": null,
                "meta": {"undoSha256": undo_hash}
            }]
        }))
        .expect("journal fixture should encode"),
    )
    .expect("journal fixture should write");
    std::fs::write(
        journal_dir.join("2026-08-24_broken.json"),
        serde_json::to_vec(&json!({
            "journalId": "broken-local",
            "opId": "rfi.process_scans",
            "ts": "2026-08-24T00:00:00Z",
            "steps": [{
                "kind": "copy",
                "src": documents.join("missing-source.txt"),
                "dst": documents.join("missing-copy.txt"),
                "noteKey": null,
                "meta": {"undoSha256": "missing"}
            }]
        }))
        .expect("broken journal fixture should encode"),
    )
    .expect("broken journal fixture should write");
    std::fs::write(
        journal_dir.join("2026-08-23_print.json"),
        serde_json::to_vec(&json!({
            "journalId": "print-audit",
            "opId": "files.batch_print",
            "ts": "2026-08-23T00:00:00Z",
            "steps": [{"kind": "print", "src": "/real", "dst": "/real"}]
        }))
        .expect("print journal fixture should encode"),
    )
    .expect("print journal fixture should write");
    let cloud = CloudHistory::new(
        auth,
        http.clone(),
        AuthConfig::new("http://supabase.test", "publishable"),
        journal_dir,
    );

    let payload = cloud.list(50, now).expect("cloud history should load");

    let serialized = serde_json::to_value(payload).expect("history should serialize");
    assert_eq!(serialized["online"], true);
    assert_eq!(serialized["rows"][0]["steps"], Value::Null);
    assert_eq!(serialized["rows"][0]["undoAvailable"], true);
    assert_eq!(serialized["rows"][1]["steps"], Value::Null);
    assert_eq!(serialized["rows"][1]["undoAvailable"], false);
    assert_eq!(serialized["rows"][2]["undoAvailable"], false);
    assert_eq!(serialized["rows"][3]["undoAvailable"], false);
    assert!(!serialized.to_string().contains("/victim"));
    assert!(!serialized.to_string().contains("/target"));
    let requests = http.requests.lock().expect("request lock");
    assert!(!requests[1].url.contains("select=*"));
    assert!(!requests[1].url.contains("steps"));
}

#[test]
fn cloud_mutations_send_metadata_only_as_the_authenticated_owner() {
    let now = 1_800_000_000_i64;
    let directory = tempfile::tempdir().expect("temporary directory");
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(
            200,
            &json!([{
                "id": "user-id",
                "email": "e@example.com",
                "allowed": true,
                "is_admin": false
            }]),
        ),
        HttpResponse::from_json(201, &Value::Null),
        HttpResponse::from_json(204, &Value::Null),
    ]));
    let auth = restored_auth(&directory, http.clone(), now);
    let cloud = CloudHistory::new(
        auth,
        http.clone(),
        AuthConfig::new("http://supabase.test", "publishable"),
        directory.path().join("journal"),
    );

    assert!(cloud.push("rfi.process_scans", 2, 0, "journal-1", "2/2", now,));
    cloud.mark_undone("cloud-1", now);

    let requests = http.requests.lock().expect("request lock");
    let push = &requests[1];
    assert_eq!(push.method, "POST");
    assert!(push.url.ends_with("/rest/v1/history"));
    assert_eq!(push.body["steps"], json!([]));
    assert_eq!(push.body["user_id"], "user-id");
    assert_eq!(push.body["user_email"], "e@example.com");
    assert!(
        push.headers
            .iter()
            .any(|(name, value)| name == "Authorization" && value == "Bearer access-secret")
    );
    assert!(
        !serde_json::to_string(&push.body)
            .expect("body should serialize")
            .contains("src")
    );
    let patch = &requests[2];
    assert_eq!(patch.method, "PATCH");
    assert!(patch.url.contains("history?id=eq.cloud-1"));
    assert_eq!(patch.body, json!({"undone": true}));
    assert!(
        !patch
            .body
            .as_object()
            .expect("patch object")
            .contains_key("user_id")
    );
}
