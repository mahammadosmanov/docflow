use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::Duration,
};

use docflow_lib::{
    auth::{AuthConfig, AuthService},
    credentials::{CredentialError, CredentialStore, OsCredentialStore, StoredSession},
    http::{HttpClient, HttpConfig, HttpError, HttpResponse, JsonHttp},
    settings::SettingsStore,
};
use serde_json::json;

fn one_response_server(response: Vec<u8>) -> Option<String> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("fake server should bind: {error}"),
    };
    let address = listener.local_addr().expect("fake server address");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("fake server should accept");
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        let expected = loop {
            let read = stream
                .read(&mut chunk)
                .expect("fake request should be read");
            if read == 0 {
                break request.len();
            }
            request.extend_from_slice(&chunk[..read]);
            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or_default();
            break header_end + 4 + content_length;
        };
        while request.len() < expected {
            let read = stream
                .read(&mut chunk)
                .expect("fake request body should be read");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
        }
        stream
            .write_all(&response)
            .expect("fake response should be written");
        stream.flush().expect("fake response should flush");
    });
    Some(format!("http://{address}"))
}

#[derive(Clone)]
struct FixtureHttp {
    responses: Arc<Mutex<VecDeque<HttpResponse>>>,
    requests: Arc<Mutex<Vec<FixtureRequest>>>,
}

struct FixtureRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: serde_json::Value,
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
        _headers: &[(String, String)],
        body: Option<&serde_json::Value>,
    ) -> Result<HttpResponse, HttpError> {
        self.requests
            .lock()
            .expect("request lock")
            .push(FixtureRequest {
                method: method.to_string(),
                url: url.to_string(),
                headers: _headers.to_vec(),
                body: body.cloned().unwrap_or(serde_json::Value::Null),
            });
        self.responses
            .lock()
            .expect("response lock")
            .pop_front()
            .ok_or(HttpError::InvalidResponse)
    }
}

struct BlockingLogoutHttp {
    responses: Mutex<VecDeque<HttpResponse>>,
    entered: Mutex<Option<mpsc::Sender<()>>>,
    released: (Mutex<bool>, Condvar),
}

impl BlockingLogoutHttp {
    fn new(responses: Vec<HttpResponse>, entered: mpsc::Sender<()>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            entered: Mutex::new(Some(entered)),
            released: (Mutex::new(false), Condvar::new()),
        }
    }

    fn release(&self) {
        *self.released.0.lock().expect("release lock") = true;
        self.released.1.notify_all();
    }
}

impl JsonHttp for BlockingLogoutHttp {
    fn send_json(
        &self,
        _method: &str,
        url: &str,
        _headers: &[(String, String)],
        _body: Option<&serde_json::Value>,
    ) -> Result<HttpResponse, HttpError> {
        if url.ends_with("/auth/v1/logout") {
            if let Some(entered) = self.entered.lock().expect("entered lock").take() {
                entered.send(()).expect("logout observer");
            }
            let mut released = self.released.0.lock().expect("release lock");
            while !*released {
                released = self.released.1.wait(released).expect("release wait");
            }
            return Err(HttpError::Offline);
        }
        self.responses
            .lock()
            .expect("response lock")
            .pop_front()
            .ok_or(HttpError::InvalidResponse)
    }
}

#[derive(Default)]
struct FixtureCredentials(Mutex<Option<StoredSession>>);

impl CredentialStore for FixtureCredentials {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
        Ok(self.0.lock().expect("credential lock").clone())
    }

    fn write(&self, session: &StoredSession) -> Result<(), CredentialError> {
        *self.0.lock().expect("credential lock") = Some(session.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        *self.0.lock().expect("credential lock") = None;
        Ok(())
    }
}

struct UnverifiableCredentials;

impl CredentialStore for UnverifiableCredentials {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
        Ok(None)
    }

    fn write(&self, _session: &StoredSession) -> Result<(), CredentialError> {
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        Ok(())
    }
}

#[derive(Default)]
struct DeleteFailingCredentials(Mutex<Option<StoredSession>>);

impl CredentialStore for DeleteFailingCredentials {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
        Ok(self.0.lock().expect("credential lock").clone())
    }

    fn write(&self, session: &StoredSession) -> Result<(), CredentialError> {
        *self.0.lock().expect("credential lock") = Some(session.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable)
    }
}

fn test_settings_path(name: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join(name);
    (directory, path)
}

#[test]
fn json_http_client_enforces_response_limit_without_exposing_body() {
    let body = vec![b'x'; 65];
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend(body);
    let Some(base) = one_response_server(response) else {
        // Some hermetic runners prohibit even loopback sockets. The same bound
        // is also covered by transport-fixture tests in this suite.
        return;
    };
    let client = HttpClient::new(HttpConfig {
        connect_timeout: Duration::from_secs(1),
        request_timeout: Duration::from_secs(2),
        max_request_bytes: 1024,
        max_response_bytes: 64,
    })
    .expect("HTTP client should build");

    let error = match client.send_json(
        "POST",
        &format!("{base}/auth/v1/token"),
        &[],
        Some(&json!({"ok": true})),
    ) {
        Ok(_) => panic!("oversized response must fail closed"),
        Err(error) => error,
    };

    assert_eq!(error, HttpError::ResponseTooLarge);
    assert!(
        !error
            .to_string()
            .contains(&String::from_utf8_lossy(&[b'x'; 65]).to_string())
    );
}

#[test]
fn email_sign_in_preserves_auth_dto_and_persists_secrets_only_in_credentials() {
    let now = 1_800_000_000_i64;
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(
            200,
            &json!({
                "access_token": "access-secret",
                "refresh_token": "refresh-secret",
                "expires_at": now + 3600,
                "user": {"id": "user-id", "email": "e@example.com"}
            }),
        ),
        HttpResponse::from_json(
            200,
            &json!([{
                "id": "user-id",
                "email": "e@example.com",
                "username": "operator",
                "allowed": true,
                "is_admin": false,
                "created_at": "2026-08-25T00:00:00Z"
            }]),
        ),
    ]));
    let credentials = Arc::new(FixtureCredentials::default());
    let (_directory, path) = test_settings_path("settings.json");
    let settings = Arc::new(SettingsStore::new(path.clone()).unwrap());
    let auth = AuthService::new(
        settings,
        credentials.clone(),
        http.clone(),
        AuthConfig::new("http://supabase.test", "publishable"),
    );

    let state = auth
        .sign_in("e@example.com", "password", now)
        .expect("sign in should succeed");

    assert_eq!(
        serde_json::to_value(state).expect("auth state should serialize"),
        json!({
            "signedIn": true,
            "allowed": true,
            "isAdmin": false,
            "session": {
                "userId": "user-id",
                "email": "e@example.com",
                "expiresAt": now + 3600
            },
            "user": {
                "id": "user-id",
                "email": "e@example.com",
                "username": "operator",
                "display": "operator",
                "allowed": true,
                "isAdmin": false,
                "createdAt": "2026-08-25T00:00:00Z"
            }
        })
    );
    let persisted = std::fs::read_to_string(path).expect("settings should be readable");
    assert!(!persisted.contains("access-secret"));
    assert!(!persisted.contains("refresh-secret"));
    let stored = credentials
        .read()
        .expect("credential read")
        .expect("stored session");
    assert!(stored.matches_tokens("access-secret", "refresh-secret"));

    let requests = http.requests.lock().expect("request lock");
    assert_eq!(requests[0].method, "POST");
    assert!(
        requests[0]
            .url
            .ends_with("/auth/v1/token?grant_type=password")
    );
    assert_eq!(
        requests[0].body,
        json!({"email": "e@example.com", "password": "password"})
    );
    assert_eq!(requests[1].method, "GET");
    assert!(requests[1].url.contains("/rest/v1/app_users?"));
}

#[test]
fn sign_in_keeps_verified_session_when_profile_fetch_is_offline() {
    let now = 1_800_000_000_i64;
    let http = Arc::new(FixtureHttp::new(vec![HttpResponse::from_json(
        200,
        &json!({
            "access_token": "access-secret",
            "refresh_token": "refresh-secret",
            "expires_at": now + 3600,
            "user": {"id": "user-id", "email": "e@example.com"}
        }),
    )]));
    let credentials = Arc::new(FixtureCredentials::default());
    let (_directory, path) = test_settings_path("settings.json");
    let auth = AuthService::new(
        Arc::new(SettingsStore::new(path).unwrap()),
        credentials,
        http,
        AuthConfig::new("http://supabase.test", "publishable"),
    );

    assert!(auth.sign_in("e@example.com", "password", now).is_err());

    let state = auth.state();
    assert!(state.signed_in);
    assert!(!state.allowed);
    assert!(state.user.is_none());
}

#[test]
fn username_sign_in_fails_closed_without_any_network_request() {
    let http = Arc::new(FixtureHttp::new(Vec::new()));
    let (_directory, path) = test_settings_path("settings.json");
    let auth = AuthService::new(
        Arc::new(SettingsStore::new(path).unwrap()),
        Arc::new(FixtureCredentials::default()),
        http.clone(),
        AuthConfig::new("http://supabase.test", "publishable"),
    );

    let error = auth
        .sign_in("operator", "wrong-password", 1_800_000_000)
        .expect_err("username sign-in must fail before contacting Supabase");

    assert_eq!(error.code, "auth_failed");
    assert_eq!(error.message, "email_required");
    assert!(http.requests.lock().expect("request lock").is_empty());
}

#[test]
fn sign_out_clears_runtime_before_a_delayed_failing_logout_request() {
    let now = 1_800_000_000_i64;
    let (entered_tx, entered_rx) = mpsc::channel();
    let http = Arc::new(BlockingLogoutHttp::new(
        vec![
            HttpResponse::from_json(
                200,
                &json!({
                    "access_token": "access-secret",
                    "refresh_token": "refresh-secret",
                    "expires_at": now + 3600,
                    "user": {"id": "user-id", "email": "e@example.com"}
                }),
            ),
            HttpResponse::from_json(
                200,
                &json!([{
                    "id": "user-id",
                    "email": "e@example.com",
                    "allowed": true,
                    "is_admin": false
                }]),
            ),
        ],
        entered_tx,
    ));
    let (_directory, path) = test_settings_path("settings.json");
    let auth = Arc::new(AuthService::new(
        Arc::new(SettingsStore::new(path).unwrap()),
        Arc::new(FixtureCredentials::default()),
        http.clone(),
        AuthConfig::new("http://supabase.test", "publishable"),
    ));
    auth.sign_in("e@example.com", "password", now).unwrap();

    let worker_auth = auth.clone();
    let worker = thread::spawn(move || worker_auth.sign_out());
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("logout should reach the delayed transport");

    assert!(!auth.state().signed_in);
    http.release();
    assert!(!worker.join().unwrap().unwrap().signed_in);
}

#[test]
fn sign_out_remains_local_after_credential_delete_failure() {
    let now = 1_800_000_000_i64;
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(
            200,
            &json!({
                "access_token": "access-secret",
                "refresh_token": "refresh-secret",
                "expires_at": now + 3600,
                "user": {"id": "user-id", "email": "e@example.com"}
            }),
        ),
        HttpResponse::from_json(
            200,
            &json!([{
                "id": "user-id",
                "email": "e@example.com",
                "allowed": true,
                "is_admin": false
            }]),
        ),
        HttpResponse::from_json(204, &serde_json::Value::Null),
    ]));
    let (_directory, path) = test_settings_path("settings.json");
    let settings = Arc::new(SettingsStore::new(path).unwrap());
    let auth = AuthService::new(
        settings.clone(),
        Arc::new(DeleteFailingCredentials::default()),
        http,
        AuthConfig::new("http://supabase.test", "publishable"),
    );
    auth.sign_in("e@example.com", "password", now).unwrap();

    assert_eq!(
        auth.sign_out().unwrap_err().code,
        "credential_store_unavailable"
    );
    assert!(!auth.state().signed_in);
    assert!(settings.section("session").is_none());
}

#[test]
fn sign_out_remains_local_after_settings_cleanup_failure() {
    let now = 1_800_000_000_i64;
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(
            200,
            &json!({
                "access_token": "access-secret",
                "refresh_token": "refresh-secret",
                "expires_at": now + 3600,
                "user": {"id": "user-id", "email": "e@example.com"}
            }),
        ),
        HttpResponse::from_json(
            200,
            &json!([{
                "id": "user-id",
                "email": "e@example.com",
                "allowed": true,
                "is_admin": false
            }]),
        ),
        HttpResponse::from_json(204, &serde_json::Value::Null),
    ]));
    let directory = tempfile::tempdir().unwrap();
    let settings_parent = directory.path().join("settings-parent");
    let path = settings_parent.join("settings.json");
    let settings = Arc::new(SettingsStore::new(path.clone()).unwrap());
    let credentials = Arc::new(FixtureCredentials::default());
    let auth = AuthService::new(
        settings,
        credentials.clone(),
        http,
        AuthConfig::new("http://supabase.test", "publishable"),
    );
    auth.sign_in("e@example.com", "password", now).unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(&settings_parent).unwrap();
    std::fs::write(&settings_parent, b"blocks settings directory").unwrap();

    assert_eq!(auth.sign_out().unwrap_err().code, "settings_write_failed");
    assert!(!auth.state().signed_in);
    assert!(credentials.read().unwrap().is_none());
}

#[test]
fn supabase_setup_exposes_no_anonymous_password_checking_oracle() {
    let sql = include_str!("../../docs/supabase_auth.sql");

    assert!(sql.contains("drop function if exists public.email_for_login(text, text)"));
    assert!(!sql.contains("create or replace function public.email_for_login"));
    assert!(!sql.contains("encrypted_password"));
    assert!(!sql.contains("grant execute on function public.email_for_login"));
}

#[test]
fn restore_imports_legacy_tokens_once_then_atomically_removes_only_plaintext_tokens() {
    let now = 1_800_000_000_i64;
    let http = Arc::new(FixtureHttp::new(vec![HttpResponse::from_json(
        200,
        &json!([{
            "id": "user-id",
            "email": "e@example.com",
            "allowed": true,
            "is_admin": false
        }]),
    )]));
    let credentials = Arc::new(FixtureCredentials::default());
    let (_directory, path) = test_settings_path("settings.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "prefs": {"locale": "az"},
            "session": {
                "accessToken": "legacy-access",
                "refreshToken": "legacy-refresh",
                "userId": "user-id",
                "email": "e@example.com",
                "expiresAt": now + 3600,
                "compatibilityMarker": "keep"
            },
            "custom": [1, 2, 3]
        }))
        .expect("legacy settings should encode"),
    )
    .expect("legacy settings should be written");
    let auth = AuthService::new(
        Arc::new(SettingsStore::new(path.clone()).unwrap()),
        credentials.clone(),
        http,
        AuthConfig::new("http://supabase.test", "publishable"),
    );

    let state = auth.restore(now).expect("legacy import should succeed");

    assert!(state.signed_in);
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("migrated settings should be readable"))
            .expect("migrated settings should be valid JSON");
    assert!(persisted["session"].get("accessToken").is_none());
    assert!(persisted["session"].get("refreshToken").is_none());
    assert_eq!(persisted["session"]["compatibilityMarker"], "keep");
    assert_eq!(persisted["prefs"]["locale"], "az");
    assert_eq!(persisted["custom"], json!([1, 2, 3]));
    let stored = credentials
        .read()
        .expect("credential read")
        .expect("imported session");
    assert!(stored.matches_tokens("legacy-access", "legacy-refresh"));
}

#[test]
fn failed_secure_readback_leaves_legacy_source_intact_and_restores_signed_out() {
    let now = 1_800_000_000_i64;
    let (_directory, path) = test_settings_path("settings.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "prefs": {"locale": "tr"},
            "session": {
                "accessToken": "legacy-access",
                "refreshToken": "legacy-refresh",
                "userId": "user-id",
                "email": "e@example.com",
                "expiresAt": now + 3600
            }
        }))
        .expect("legacy settings should encode"),
    )
    .expect("legacy settings should be written");
    let auth = AuthService::new(
        Arc::new(SettingsStore::new(path.clone()).unwrap()),
        Arc::new(UnverifiableCredentials),
        Arc::new(FixtureHttp::new(Vec::new())),
        AuthConfig::new("http://supabase.test", "publishable"),
    );

    let state = auth.restore(now).expect("restore should fail closed");

    assert!(!state.signed_in);
    let persisted = std::fs::read_to_string(path).expect("legacy settings should remain");
    assert!(persisted.contains("legacy-access"));
    assert!(persisted.contains("legacy-refresh"));
}

#[test]
fn platform_credential_backend_is_native_for_supported_targets() {
    #[cfg(target_os = "macos")]
    assert_eq!(OsCredentialStore::BACKEND, "macos-keychain");
    #[cfg(target_os = "windows")]
    assert_eq!(OsCredentialStore::BACKEND, "windows-credential-manager");
}

#[test]
fn admin_user_methods_preserve_profile_dtos_and_owner_authorization_headers() {
    let now = 1_800_000_000_i64;
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(
            200,
            &json!({
                "access_token": "admin-access",
                "refresh_token": "admin-refresh",
                "expires_at": now + 3600,
                "user": {"id": "admin-id", "email": "admin@example.com"}
            }),
        ),
        HttpResponse::from_json(
            200,
            &json!([{
                "id": "admin-id",
                "email": "admin@example.com",
                "username": null,
                "allowed": true,
                "is_admin": true,
                "created_at": "2026-08-24T00:00:00Z"
            }]),
        ),
        HttpResponse::from_json(
            200,
            &json!([{
                "id": "user-id",
                "email": "user@example.com",
                "username": "operator",
                "allowed": false,
                "is_admin": false,
                "created_at": "2026-08-25T00:00:00Z"
            }]),
        ),
        HttpResponse::from_json(204, &serde_json::Value::Null),
    ]));
    let (_directory, path) = test_settings_path("settings.json");
    let auth = AuthService::new(
        Arc::new(SettingsStore::new(path).unwrap()),
        Arc::new(FixtureCredentials::default()),
        http.clone(),
        AuthConfig::new("http://supabase.test", "publishable"),
    );
    auth.sign_in("admin@example.com", "password", now)
        .expect("admin sign in");

    let users = auth.list_users(now).expect("admin should list users");
    assert_eq!(
        serde_json::to_value(users).expect("users should serialize"),
        json!([{
            "id": "user-id",
            "email": "user@example.com",
            "username": "operator",
            "display": "operator",
            "allowed": false,
            "isAdmin": false,
            "createdAt": "2026-08-25T00:00:00Z"
        }])
    );
    auth.set_flags("user-id", Some(true), None, now)
        .expect("admin should update flags");

    let requests = http.requests.lock().expect("request lock");
    let list_request = &requests[2];
    assert_eq!(list_request.method, "GET");
    assert!(
        list_request
            .url
            .ends_with("app_users?select=*&order=created_at.desc")
    );
    assert!(
        list_request
            .headers
            .iter()
            .any(|(name, value)| name == "Authorization" && value == "Bearer admin-access")
    );
    let patch_request = &requests[3];
    assert_eq!(patch_request.method, "PATCH");
    assert_eq!(patch_request.body, json!({"allowed": true}));
    assert!(
        patch_request
            .headers
            .iter()
            .any(|(name, value)| name == "Prefer" && value == "return=minimal")
    );
}

#[test]
fn restore_refreshes_an_expired_secure_session_without_returning_tokens() {
    let now = 1_800_000_000_i64;
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(
            200,
            &json!({
                "access_token": "new-access",
                "refresh_token": "new-refresh",
                "expires_at": now + 3600,
                "user": {"id": "user-id", "email": "e@example.com"}
            }),
        ),
        HttpResponse::from_json(
            200,
            &json!([{
                "id": "user-id",
                "email": "e@example.com",
                "allowed": true,
                "is_admin": false
            }]),
        ),
    ]));
    let credentials = Arc::new(FixtureCredentials(Mutex::new(Some(StoredSession::new(
        "old-access".to_string(),
        "old-refresh".to_string(),
        "user-id".to_string(),
        "e@example.com".to_string(),
        now - 1,
    )))));
    let (_directory, path) = test_settings_path("settings.json");
    let settings = Arc::new(SettingsStore::new(path.clone()).unwrap());
    settings
        .save_session_metadata("user-id", "e@example.com", now - 1)
        .expect("public session metadata");
    let auth = AuthService::new(
        settings,
        credentials.clone(),
        http.clone(),
        AuthConfig::new("http://supabase.test", "publishable"),
    );

    let state = auth.restore(now).expect("refresh should succeed");

    assert!(state.signed_in);
    assert_eq!(state.session.expect("session view").expires_at, now + 3600);
    let stored = credentials
        .read()
        .expect("credential read")
        .expect("refreshed credential");
    assert!(stored.matches_tokens("new-access", "new-refresh"));
    let serialized = serde_json::to_string(&auth.state()).expect("state should serialize");
    assert!(!serialized.contains("new-access"));
    assert!(!serialized.contains("new-refresh"));
    let requests = http.requests.lock().expect("request lock");
    assert!(
        requests[0]
            .url
            .ends_with("/auth/v1/token?grant_type=refresh_token")
    );
    assert_eq!(requests[0].body, json!({"refresh_token": "old-refresh"}));
}
