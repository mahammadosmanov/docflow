//! Bounded local activity log: one JSON line per backend call, so a support
//! export shows every error, warning and Manual Review reason without
//! screenshots. Paths, tokens and document content are redacted with the same
//! rules as the support bundle. The log never feeds any authority.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
};

use serde_json::{Map, Value, json};

use crate::diagnostics::sanitize_value;

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_EXPORT_BYTES: usize = 768 * 1024;
const MAX_SAMPLES: usize = 400;
const MAX_WARNINGS: usize = 100;

pub(crate) struct ActivityLog {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl ActivityLog {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    pub(crate) fn default_dir() -> PathBuf {
        crate::settings::default_settings_path()
            .parent()
            .map_or_else(std::env::temp_dir, PathBuf::from)
            .join("logs")
    }

    /// Best effort: logging must never fail or slow a user operation.
    pub(crate) fn record(&self, mut entry: Value) {
        sanitize_value(&mut entry, 0);
        let Ok(mut line) = serde_json::to_string(&entry) else {
            return;
        };
        line.push('\n');
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let current = self.dir.join("activity.jsonl");
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        if fs::metadata(&current).is_ok_and(|m| m.len() + line.len() as u64 > MAX_FILE_BYTES) {
            let _ = fs::remove_file(self.dir.join("activity.1.jsonl"));
            let _ = fs::rename(&current, self.dir.join("activity.1.jsonl"));
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&current) {
            let _ = file.write_all(line.as_bytes());
        }
    }

    /// Oldest-to-newest lines, at most `MAX_EXPORT_BYTES`, whole lines only.
    pub(crate) fn export(&self) -> String {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut text = String::new();
        for name in ["activity.1.jsonl", "activity.jsonl"] {
            if let Ok(mut file) = fs::File::open(self.dir.join(name)) {
                let mut part = String::new();
                let _ = file.read_to_string(&mut part);
                text.push_str(&part);
            }
        }
        if text.len() > MAX_EXPORT_BYTES {
            let mut start = text.len() - MAX_EXPORT_BYTES;
            while !text.is_char_boundary(start) {
                start += 1;
            }
            start = text[start..]
                .find('\n')
                .map_or(text.len(), |offset| start + offset + 1);
            text = text[start..].to_string();
        }
        text
    }
}

/// Records every Rust panic as one redacted activity-log line, then runs the
/// previous hook, so a support export shows crashes in background work too.
pub(crate) fn install_panic_hook(log: std::sync::Arc<ActivityLog>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log.record(panic_entry(info));
        previous(info);
    }));
}

fn panic_entry(info: &std::panic::PanicHookInfo<'_>) -> Value {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    let message: String = message.chars().take(300).collect();
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    json!({
        "at": format!("unix:{seconds}"),
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "event": "panic",
        "thread": std::thread::current().name().unwrap_or("unnamed"),
        "location": info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line())),
        "error": message,
    })
}

/// Compact record of an engine result: warnings, and for review tables the
/// count per decision/reason plus samples of every non-ready row.
pub(crate) fn summarize_plan(plan: &Value) -> Value {
    let mut out = Map::new();
    out.insert(
        "opId".into(),
        plan.get("opId").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "steps".into(),
        json!(
            plan.get("steps")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        ),
    );
    let warnings: Vec<Value> = plan
        .get("warnings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_WARNINGS)
        .map(|w| json!({"key": w.get("key"), "subject": w.get("subject")}))
        .collect();
    out.insert("warnings".into(), Value::Array(warnings));
    let rows = plan.pointer("/meta/editor/rows").and_then(Value::as_array);
    if let Some(rows) = rows {
        let mut counts = std::collections::BTreeMap::<String, u64>::new();
        let mut samples = Vec::new();
        for row in rows {
            let decision = row.get("disposition").and_then(Value::as_str).unwrap_or("");
            let reason = row.get("reasonKey").and_then(Value::as_str).unwrap_or("");
            *counts.entry(format!("{decision}|{reason}")).or_default() += 1;
            let ready = row.get("selectable").and_then(Value::as_bool) == Some(true);
            if !ready && samples.len() < MAX_SAMPLES {
                samples.push(json!({
                    "rfi": row.get("rfiNo").or_else(|| row.get("canonicalRfi")),
                    "sheet": row.get("_sheet").or_else(|| row.get("discipline")),
                    "decision": decision,
                    "reason": reason,
                    "details": row
                        .get("mappingDetails")
                        .and_then(Value::as_str)
                        // Slashes in tags would be mistaken for paths by the redactor.
                        .map(|text| text.replace(['/', '\\'], "|")),
                }));
            }
        }
        out.insert("rowCount".into(), json!(rows.len()));
        out.insert("rowCounts".into(), json!(counts));
        out.insert("rowSamples".into(), Value::Array(samples));
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (tempfile::TempDir, ActivityLog) {
        let dir = tempfile::tempdir().unwrap();
        let log = ActivityLog::new(dir.path().join("logs"));
        (dir, log)
    }

    #[test]
    fn records_lines_and_redacts_paths_tokens_and_content() {
        let (_dir, log) = temp();
        log.record(json!({
            "method": "engine/preview",
            "error": "error.rfi.followUp.workbookLocked: os error 5",
            "path": "/Users/x/RFI Log.xlsx",
            "detail": "C:\\Users\\x\\Follow Up.xlsx",
            "accessToken": "secret",
            "content": "document body",
        }));
        let text = log.export();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("os error 5"));
        for leaked in ["RFI Log.xlsx", "Follow Up.xlsx", "secret", "document body"] {
            assert!(!text.contains(leaked), "{leaked} leaked: {text}");
        }
    }

    #[test]
    fn panic_hook_records_a_redacted_panic_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = std::sync::Arc::new(ActivityLog::new(dir.path().join("logs")));
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        install_panic_hook(log.clone());
        let result = std::panic::catch_unwind(|| {
            panic!("failed to open /Users/x/RFI Log.xlsx");
        });
        let _ = std::panic::take_hook();
        std::panic::set_hook(previous);
        assert!(result.is_err());
        let text = log.export();
        assert!(text.lines().any(|line| {
            let line: Value = serde_json::from_str(line).unwrap();
            line["event"] == "panic"
                && line["location"]
                    .as_str()
                    .is_some_and(|location| location.contains("activity_log.rs"))
        }));
        assert!(!text.contains("RFI Log.xlsx"), "{text}");
    }

    #[test]
    fn rotates_and_stays_bounded() {
        let (_dir, log) = temp();
        let big = "x".repeat(60_000);
        for _ in 0..80 {
            log.record(json!({"note": big}));
        }
        let text = log.export();
        assert!(text.len() <= MAX_EXPORT_BYTES);
        assert!(
            text.lines()
                .all(|l| serde_json::from_str::<Value>(l).is_ok())
        );
        assert!(fs::read_dir(&log.dir).unwrap().count() <= 2);
    }

    #[test]
    fn summary_counts_reasons_and_samples_only_unready_rows() {
        let plan = json!({
            "opId": "rfi.follow_up_sync",
            "steps": [{}],
            "warnings": [{"key": "warn.x", "subject": "s"}],
            "meta": {"editor": {"rows": [
                {"rfiNo": "CIV-0001", "disposition": "ready", "reasonKey": "r1", "selectable": true},
                {"rfiNo": "CIV-0002", "_sheet": "CIV", "disposition": "manualReview",
                 "reasonKey": "warn.rfi.followUp.reason.mappingMissing", "selectable": false,
                 "mappingDetails": "ITP: CIV-0002; item: 2.1.4"},
            ]}}
        });
        let s = summarize_plan(&plan);
        assert_eq!(s["rowCount"], 2);
        assert_eq!(s["rowSamples"].as_array().unwrap().len(), 1);
        assert_eq!(s["rowSamples"][0]["rfi"], "CIV-0002");
        assert!(
            s["rowSamples"][0]["details"]
                .as_str()
                .unwrap()
                .contains("2.1.4")
        );
        assert_eq!(
            s["rowCounts"]["manualReview|warn.rfi.followUp.reason.mappingMissing"],
            1
        );
    }
}
