use std::sync::{Arc, Mutex, MutexGuard};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::{Value, json};

use crate::{
    contracts::{AppUser, AuthSessionView, AuthState},
    credentials::{CredentialStore, StoredSession},
    errors::BackendError,
    http::{HttpResponse, JsonHttp},
    settings::SettingsStore,
};

pub(crate) const QUERY_COMPONENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

const MAX_IDENTIFIER_CHARS: usize = 320;
const MAX_PASSWORD_CHARS: usize = 1024;
const MAX_USERNAME_CHARS: usize = 24;
const MAX_USER_ID_CHARS: usize = 128;

#[derive(Clone)]
pub struct AuthConfig {
    url: String,
    key: String,
}

impl AuthConfig {
    pub fn new(url: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            url: url.into().trim_end_matches('/').to_string(),
            key: key.into(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

#[derive(Default)]
struct AuthRuntime {
    session: Option<StoredSession>,
    me: Option<AppUser>,
}

pub struct AuthService {
    settings: Arc<SettingsStore>,
    credentials: Arc<dyn CredentialStore>,
    http: Arc<dyn JsonHttp>,
    config: AuthConfig,
    runtime: Mutex<AuthRuntime>,
}

impl AuthService {
    pub fn new(
        settings: Arc<SettingsStore>,
        credentials: Arc<dyn CredentialStore>,
        http: Arc<dyn JsonHttp>,
        config: AuthConfig,
    ) -> Self {
        Self {
            settings,
            credentials,
            http,
            config,
            runtime: Mutex::new(AuthRuntime::default()),
        }
    }

    pub fn state(&self) -> AuthState {
        state_from_runtime(&lock_runtime(&self.runtime))
    }

    pub fn restore(&self, now: i64) -> Result<AuthState, BackendError> {
        let legacy = self
            .settings
            .section("session")
            .as_ref()
            .and_then(stored_session_from_legacy);
        let mut session = if let Some(legacy) = legacy {
            if !self.import_legacy(&legacy) {
                return Ok(self.clear_runtime());
            }
            legacy
        } else {
            match self.credentials.read() {
                Ok(Some(session)) => session,
                Ok(None) | Err(_) => return Ok(self.clear_runtime()),
            }
        };

        if session.expired_at(now) {
            if let Ok(refreshed) = self.refresh_session(&session, now) {
                session = refreshed
            } else {
                let _ = self.credentials.delete();
                let _ = self.settings.save_section("session", None);
                return Ok(self.clear_runtime());
            }
        }
        let me = self.load_user(&session).ok().flatten();
        let mut runtime = lock_runtime(&self.runtime);
        runtime.session = Some(session);
        runtime.me = me;
        Ok(state_from_runtime(&runtime))
    }

    pub fn sign_in(
        &self,
        identifier: &str,
        password: &str,
        now: i64,
    ) -> Result<AuthState, BackendError> {
        let login = identifier.trim();
        validate_bounded("identifier", login, MAX_IDENTIFIER_CHARS)?;
        validate_bounded("password", password, MAX_PASSWORD_CHARS)?;

        if !login.contains('@') {
            return Err(BackendError::auth_failed("email_required"));
        }
        let email = login.to_string();

        let response = self.request(
            "POST",
            "/auth/v1/token?grant_type=password",
            false,
            Some(&json!({"email": email, "password": password})),
        )?;
        let raw = require_auth_json(response)?;
        let session = parse_token_response(&raw, now)?;
        self.persist_session(&session)?;
        {
            let mut runtime = lock_runtime(&self.runtime);
            runtime.session = Some(session.clone());
            runtime.me = None;
        }
        let me = self.load_user(&session)?;

        let mut runtime = lock_runtime(&self.runtime);
        runtime.session = Some(session);
        runtime.me = me;
        Ok(state_from_runtime(&runtime))
    }

    pub fn sign_up(&self, email: &str, password: &str, username: &str) -> Result<(), BackendError> {
        validate_bounded("email", email, MAX_IDENTIFIER_CHARS)?;
        validate_bounded("password", password, MAX_PASSWORD_CHARS)?;
        validate_bounded("username", username, MAX_USERNAME_CHARS)?;
        let response = self.request(
            "POST",
            "/auth/v1/signup",
            false,
            Some(&json!({
                "email": email,
                "password": password,
                "data": {"username": username.trim()}
            })),
        )?;
        require_auth_json(response).map(|_| ())
    }

    pub fn username_available(&self, username: &str) -> bool {
        if !username_rule_ok(username) {
            return false;
        }
        match self.request(
            "POST",
            "/rest/v1/rpc/username_available",
            false,
            Some(&json!({"p_username": username.trim()})),
        ) {
            Ok(response) => match require_auth_json(response) {
                Ok(value) => value == Value::Bool(true),
                Err(_) => true,
            },
            // Compatibility: a transient availability failure must not block signup.
            Err(_) => true,
        }
    }

    pub fn load_me(&self, now: i64) -> Result<AuthState, BackendError> {
        let session = self.valid_session(now)?;
        let me = self.load_user(&session)?;
        let mut runtime = lock_runtime(&self.runtime);
        runtime.session = Some(session);
        runtime.me = me;
        Ok(state_from_runtime(&runtime))
    }

    pub fn set_username(&self, username: &str, now: i64) -> Result<AuthState, BackendError> {
        validate_bounded("username", username, MAX_USERNAME_CHARS)?;
        let session = self.valid_session(now)?;
        let response = self.request_with_session(
            "POST",
            "/rest/v1/rpc/set_username",
            &session,
            Some(&json!({"p_username": username.trim()})),
        )?;
        require_auth_json(response)?;
        self.load_me(now)
    }

    pub fn list_users(&self, now: i64) -> Result<Vec<AppUser>, BackendError> {
        self.require_admin()?;
        let session = self.valid_session(now)?;
        let response = self.request_with_session(
            "GET",
            "/rest/v1/app_users?select=*&order=created_at.desc",
            &session,
            None,
        )?;
        let raw = require_auth_json(response)?;
        Ok(raw
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .map(app_user_from_json)
            .collect())
    }

    pub fn set_flags(
        &self,
        user_id: &str,
        allowed: Option<bool>,
        is_admin: Option<bool>,
        now: i64,
    ) -> Result<(), BackendError> {
        self.require_admin()?;
        validate_bounded("userId", user_id, MAX_USER_ID_CHARS)?;
        let mut patch = serde_json::Map::new();
        if let Some(allowed) = allowed {
            patch.insert("allowed".to_string(), Value::Bool(allowed));
        }
        if let Some(is_admin) = is_admin {
            patch.insert("is_admin".to_string(), Value::Bool(is_admin));
        }
        if patch.is_empty() {
            return Ok(());
        }
        let session = self.valid_session(now)?;
        let safe_id = utf8_percent_encode(user_id, QUERY_COMPONENT);
        let path = format!("/rest/v1/app_users?id=eq.{safe_id}");
        let response = self.request_with_session_headers(
            "PATCH",
            &path,
            &session,
            &[("Prefer".to_string(), "return=minimal".to_string())],
            Some(&Value::Object(patch)),
        )?;
        require_auth_json(response).map(|_| ())
    }

    pub fn sign_out(&self) -> Result<AuthState, BackendError> {
        let (session, signed_out) = {
            let mut runtime = lock_runtime(&self.runtime);
            let session = runtime.session.take();
            runtime.me = None;
            (session, state_from_runtime(&runtime))
        };
        if let Some(session) = session.as_ref() {
            let _ = self.request_with_session("POST", "/auth/v1/logout", session, None);
        }
        let credential_result = self.credentials.delete();
        let settings_result = self.settings.save_section("session", None);
        if credential_result.is_err() {
            return Err(BackendError::credential_store());
        }
        if settings_result.is_err() {
            return Err(BackendError::settings_write());
        }
        Ok(signed_out)
    }

    pub fn session_for_cloud(&self, now: i64) -> Result<StoredSession, BackendError> {
        let state = self.state();
        if !state.allowed {
            return Err(BackendError::permission_denied());
        }
        self.valid_session(now)
    }

    fn load_user(&self, session: &StoredSession) -> Result<Option<AppUser>, BackendError> {
        let user_id = utf8_percent_encode(session.user_id(), QUERY_COMPONENT);
        let path = format!("/rest/v1/app_users?select=*&id=eq.{user_id}");
        let response = self.request_with_session("GET", &path, session, None)?;
        let raw = require_auth_json(response)?;
        Ok(raw
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(Value::as_object)
            .map(app_user_from_json))
    }

    fn persist_session(&self, session: &StoredSession) -> Result<(), BackendError> {
        self.credentials
            .write(session)
            .map_err(|_| BackendError::credential_store())?;
        let verified = self
            .credentials
            .read()
            .map_err(|_| BackendError::credential_store())?
            .is_some_and(|stored| stored == *session);
        if !verified {
            let _ = self.credentials.delete();
            return Err(BackendError::credential_store());
        }
        if self
            .settings
            .save_session_metadata(session.user_id(), session.email(), session.expires_at())
            .is_err()
        {
            let _ = self.credentials.delete();
            return Err(BackendError::settings_write());
        }
        Ok(())
    }

    fn import_legacy(&self, session: &StoredSession) -> bool {
        if self.credentials.write(session).is_err() {
            return false;
        }
        let verified = self
            .credentials
            .read()
            .ok()
            .flatten()
            .is_some_and(|stored| stored == *session);
        if !verified {
            let _ = self.credentials.delete();
            return false;
        }
        if let Ok(true) = self.settings.strip_legacy_session_tokens(session) {
            true
        } else {
            let _ = self.credentials.delete();
            false
        }
    }

    fn clear_runtime(&self) -> AuthState {
        let mut runtime = lock_runtime(&self.runtime);
        runtime.session = None;
        runtime.me = None;
        state_from_runtime(&runtime)
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        authed: bool,
        body: Option<&Value>,
    ) -> Result<HttpResponse, BackendError> {
        let session = if authed {
            lock_runtime(&self.runtime).session.clone()
        } else {
            None
        };
        if let Some(session) = session.as_ref() {
            return self.request_with_session(method, path, session, body);
        }
        let headers = vec![("apikey".to_string(), self.config.key.clone())];
        self.http
            .send_json(
                method,
                &format!("{}{}", self.config.url, path),
                &headers,
                body,
            )
            .map_err(BackendError::from)
    }

    fn request_with_session(
        &self,
        method: &str,
        path: &str,
        session: &StoredSession,
        body: Option<&Value>,
    ) -> Result<HttpResponse, BackendError> {
        self.request_with_session_headers(method, path, session, &[], body)
    }

    fn request_with_session_headers(
        &self,
        method: &str,
        path: &str,
        session: &StoredSession,
        extra_headers: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<HttpResponse, BackendError> {
        let mut headers = vec![
            ("apikey".to_string(), self.config.key.clone()),
            (
                "Authorization".to_string(),
                format!("Bearer {}", session.access_token()),
            ),
        ];
        headers.extend_from_slice(extra_headers);
        self.http
            .send_json(
                method,
                &format!("{}{}", self.config.url, path),
                &headers,
                body,
            )
            .map_err(BackendError::from)
    }

    fn valid_session(&self, now: i64) -> Result<StoredSession, BackendError> {
        let session = lock_runtime(&self.runtime)
            .session
            .clone()
            .ok_or_else(BackendError::auth_required)?;
        if !session.expired_at(now) {
            return Ok(session);
        }
        let refreshed = self.refresh_session(&session, now)?;
        let mut runtime = lock_runtime(&self.runtime);
        runtime.session = Some(refreshed.clone());
        Ok(refreshed)
    }

    fn refresh_session(
        &self,
        session: &StoredSession,
        now: i64,
    ) -> Result<StoredSession, BackendError> {
        let response = self.request(
            "POST",
            "/auth/v1/token?grant_type=refresh_token",
            false,
            Some(&json!({"refresh_token": session.refresh_token()})),
        )?;
        let raw = require_auth_json(response)?;
        let refreshed = parse_token_response(&raw, now)?;
        self.persist_session(&refreshed)?;
        Ok(refreshed)
    }

    fn require_admin(&self) -> Result<(), BackendError> {
        let runtime = lock_runtime(&self.runtime);
        if runtime.session.is_none() {
            return Err(BackendError::auth_required());
        }
        if !runtime.me.as_ref().is_some_and(|user| user.is_admin) {
            return Err(BackendError::permission_denied());
        }
        Ok(())
    }
}

fn validate_bounded(name: &str, value: &str, maximum: usize) -> Result<(), BackendError> {
    if value.chars().count() > maximum {
        return Err(BackendError::validation(format!(
            "{name} exceeded the size limit"
        )));
    }
    Ok(())
}

fn username_rule_ok(value: &str) -> bool {
    let cleaned = value.trim();
    (3..=MAX_USERNAME_CHARS).contains(&cleaned.len())
        && cleaned
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn parse_token_response(raw: &Value, now: i64) -> Result<StoredSession, BackendError> {
    let access_token = raw
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BackendError::auth_failed("invalid_token_response"))?;
    let refresh_token = raw
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BackendError::auth_failed("invalid_token_response"))?;
    let user = raw.get("user").and_then(Value::as_object);
    let user_id = user
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let email = user
        .and_then(|value| value.get("email"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expires_in = raw
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600);
    let expires_at = raw
        .get("expires_at")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| now.saturating_add(expires_in));
    Ok(StoredSession::new(
        access_token.to_string(),
        refresh_token.to_string(),
        user_id.to_string(),
        email.to_string(),
        expires_at,
    ))
}

fn stored_session_from_legacy(raw: &serde_json::Map<String, Value>) -> Option<StoredSession> {
    let access_token = raw.get("accessToken")?.as_str()?;
    let refresh_token = raw.get("refreshToken")?.as_str()?;
    Some(StoredSession::new(
        access_token.to_string(),
        refresh_token.to_string(),
        raw.get("userId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        raw.get("email")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        raw.get("expiresAt").and_then(Value::as_i64).unwrap_or(0),
    ))
}

fn app_user_from_json(raw: &serde_json::Map<String, Value>) -> AppUser {
    let email = string_value(raw.get("email"));
    let username = raw
        .get("username")
        .and_then(Value::as_str)
        .map(str::to_string);
    let display = username
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(&email)
        .to_string();
    AppUser {
        id: string_value(raw.get("id")),
        email,
        username,
        display,
        allowed: raw
            .get("allowed")
            .is_some_and(|value| value == &Value::Bool(true)),
        is_admin: raw
            .get("is_admin")
            .is_some_and(|value| value == &Value::Bool(true)),
        created_at: string_value(raw.get("created_at")),
    }
}

fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn state_from_runtime(runtime: &AuthRuntime) -> AuthState {
    let session = runtime.session.as_ref();
    AuthState {
        signed_in: session.is_some(),
        allowed: runtime.me.as_ref().is_some_and(|user| user.allowed),
        is_admin: runtime.me.as_ref().is_some_and(|user| user.is_admin),
        session: session.map(|session| AuthSessionView {
            user_id: session.user_id().to_string(),
            email: session.email().to_string(),
            expires_at: session.expires_at(),
        }),
        user: runtime.me.clone(),
    }
}

fn require_auth_json(response: HttpResponse) -> Result<Value, BackendError> {
    if (200..300).contains(&response.status) {
        return response.json().map_err(BackendError::from);
    }
    let raw = response.json().unwrap_or(Value::Null);
    let message = ["error_description", "msg", "message", "error"]
        .into_iter()
        .find_map(|key| raw.get(key).and_then(Value::as_str))
        .map_or_else(
            || format!("auth_http_{}", response.status),
            stable_auth_message,
        );
    Err(BackendError::auth_failed(message))
}

fn stable_auth_message(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("invalid login") || lower.contains("invalid_credentials") {
        "invalid_credentials".to_string()
    } else if lower.contains("email not confirmed") || lower.contains("email_not_confirmed") {
        "email_not_confirmed".to_string()
    } else if lower.contains("already registered") || lower.contains("user_already_exists") {
        "user_already_exists".to_string()
    } else if lower.contains("weak_password") || lower.contains("password should be") {
        "weak_password".to_string()
    } else if lower.contains("rate limit") || lower.contains("too many") {
        "rate_limit".to_string()
    } else if lower.contains("username_taken") {
        "username_taken".to_string()
    } else if lower.contains("username_invalid") {
        "username_invalid".to_string()
    } else {
        "auth_request_failed".to_string()
    }
}

fn lock_runtime(runtime: &Mutex<AuthRuntime>) -> MutexGuard<'_, AuthRuntime> {
    runtime
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
