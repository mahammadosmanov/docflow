use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use percent_encoding::utf8_percent_encode;
use serde_json::{Value, json};

use crate::{
    auth::{AuthConfig, AuthService},
    contracts::{HistoryPayload, HistoryRow},
    credentials::StoredSession,
    errors::BackendError,
    http::{HttpError, HttpResponse, JsonHttp},
};

const MAX_HISTORY_LIMIT: usize = 100;
const MAX_ID_CHARS: usize = 128;
const MAX_SUMMARY_CHARS: usize = 500;

pub struct CloudHistory {
    auth: Arc<AuthService>,
    http: Arc<dyn JsonHttp>,
    config: AuthConfig,
    journal_dir: PathBuf,
}

impl CloudHistory {
    pub fn new(
        auth: Arc<AuthService>,
        http: Arc<dyn JsonHttp>,
        config: AuthConfig,
        journal_dir: PathBuf,
    ) -> Self {
        Self {
            auth,
            http,
            config,
            journal_dir,
        }
    }

    pub fn list(&self, limit: usize, now: i64) -> Result<HistoryPayload, BackendError> {
        if limit == 0 || limit > MAX_HISTORY_LIMIT {
            return Err(BackendError::validation(
                "history limit must be between 1 and 100",
            ));
        }
        let local = read_local_history(&self.journal_dir, limit);
        let local_ids = local
            .iter()
            .filter(|row| row.undo_available)
            .map(|row| row.journal_id.clone())
            .filter(|id| !id.is_empty())
            .collect::<HashSet<_>>();
        let Ok(session) = self.auth.session_for_cloud(now) else {
            return Ok(HistoryPayload {
                online: false,
                rows: local,
            });
        };
        let path = format!(
            "/rest/v1/history?select=id,created_at,op_id,applied,failed,journal_id,undone,user_email&order=created_at.desc&limit={limit}"
        );
        let response = match self.request("GET", &path, &session, &[], None) {
            Ok(response) => response,
            Err(_) => {
                return Ok(HistoryPayload {
                    online: false,
                    rows: local,
                });
            }
        };
        let raw = match cloud_json(response) {
            Ok(value) => value,
            Err(_) => {
                return Ok(HistoryPayload {
                    online: false,
                    rows: local,
                });
            }
        };
        let rows = raw
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .map(|entry| {
                let journal_id = string_value(entry.get("journal_id"));
                HistoryRow {
                    when: string_value(entry.get("created_at")),
                    op_id: string_value(entry.get("op_id")),
                    moves: unsigned_value(entry.get("applied")),
                    failed: unsigned_value(entry.get("failed")),
                    journal_id: journal_id.clone(),
                    // Phase 0: cloud-authored steps and paths are never deserialized.
                    steps: None,
                    cloud_id: Some(string_value(entry.get("id"))),
                    undone: entry.get("undone") == Some(&Value::Bool(true)),
                    undo_available: local_ids.contains(&journal_id),
                    from_cloud: true,
                    user_email: string_value(entry.get("user_email")),
                }
            })
            .collect();
        Ok(HistoryPayload { online: true, rows })
    }

    pub fn push(
        &self,
        op_id: &str,
        applied: u64,
        failed: u64,
        journal_id: &str,
        summary: &str,
        now: i64,
    ) -> bool {
        if !bounded(op_id, MAX_ID_CHARS)
            || !bounded(journal_id, MAX_ID_CHARS)
            || !bounded(summary, MAX_SUMMARY_CHARS)
            || journal_id.is_empty()
        {
            return false;
        }
        let Ok(session) = self.auth.session_for_cloud(now) else {
            return false;
        };
        let device = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_default();
        let body = json!({
            "device": device,
            "op_id": op_id,
            "applied": applied,
            "failed": failed,
            "journal_id": journal_id,
            "summary": summary,
            "steps": [],
            "user_id": session.user_id(),
            "user_email": session.email()
        });
        self.request(
            "POST",
            "/rest/v1/history",
            &session,
            &[("Prefer".to_string(), "return=minimal".to_string())],
            Some(&body),
        )
        .is_ok_and(|response| (200..300).contains(&response.status))
    }

    pub fn mark_undone(&self, entry_id: &str, now: i64) {
        if entry_id.is_empty() || !bounded(entry_id, MAX_ID_CHARS) {
            return;
        }
        let Ok(session) = self.auth.session_for_cloud(now) else {
            return;
        };
        let safe_id = utf8_percent_encode(entry_id, super::auth::QUERY_COMPONENT);
        let path = format!("/rest/v1/history?id=eq.{safe_id}");
        let _ = self.request(
            "PATCH",
            &path,
            &session,
            &[("Prefer".to_string(), "return=minimal".to_string())],
            Some(&json!({"undone": true})),
        );
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        session: &StoredSession,
        extra_headers: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<HttpResponse, BackendError> {
        let mut headers = vec![
            ("apikey".to_string(), self.config.key().to_string()),
            (
                "Authorization".to_string(),
                format!("Bearer {}", session.access_token()),
            ),
        ];
        headers.extend_from_slice(extra_headers);
        self.http
            .send_json(
                method,
                &format!("{}{}", self.config.url(), path),
                &headers,
                body,
            )
            .map_err(map_http_error)
    }
}

fn read_local_history(journal_dir: &Path, limit: usize) -> Vec<HistoryRow> {
    let Ok(entries) = fs::read_dir(journal_dir) else {
        return Vec::new();
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    files
        .into_iter()
        .take(limit)
        .filter_map(|path| fs::read(path).ok())
        .filter_map(|encoded| serde_json::from_slice::<Value>(&encoded).ok())
        .filter_map(|value| value.as_object().cloned())
        .map(|entry| {
            let local_record = serde_json::from_value::<crate::journal::JournalRecord>(
                Value::Object(entry.clone()),
            )
            .ok();
            let moves = entry
                .get("steps")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|step| {
                    matches!(
                        step.get("kind").and_then(Value::as_str),
                        Some("move" | "copy" | "rename" | "restore_backup")
                    )
                })
                .count() as u64;
            let undo_available = local_record
                .as_ref()
                .is_some_and(crate::undo::journal_is_safely_undoable);
            HistoryRow {
                when: string_value(entry.get("ts")),
                op_id: string_value(entry.get("opId")),
                moves,
                failed: 0,
                journal_id: string_value(entry.get("journalId")),
                steps: None,
                cloud_id: None,
                undone: false,
                undo_available,
                from_cloud: false,
                user_email: String::new(),
            }
        })
        .collect()
}

fn cloud_json(response: HttpResponse) -> Result<Value, BackendError> {
    if !(200..300).contains(&response.status) {
        return Err(BackendError::request_failed(format!(
            "cloud_history_http_{}",
            response.status
        )));
    }
    response
        .json()
        .map_err(|_| BackendError::request_failed("cloud_history_invalid_response"))
}

fn bounded(value: &str, maximum: usize) -> bool {
    value.chars().count() <= maximum
}

fn string_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn unsigned_value(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or_default()
}

fn map_http_error(error: HttpError) -> BackendError {
    match error {
        HttpError::Timeout => BackendError::network_timeout(),
        HttpError::ResponseTooLarge => BackendError::response_too_large(),
        HttpError::Offline => BackendError::offline(),
        _ => BackendError::request_failed(error.to_string()),
    }
}
