use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub type JsonObject = Map<String, Value>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationPreset {
    pub id: String,
    pub name: String,
    pub values: JsonObject,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ParamKind {
    #[serde(rename = "folder")]
    Folder,
    #[serde(rename = "file")]
    File,
    #[serde(rename = "files")]
    Files,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "choice")]
    Choice,
    #[serde(rename = "int")]
    Int,
    #[serde(rename = "longText")]
    LongText,
    #[serde(rename = "secret")]
    Secret,
    #[serde(rename = "sheetCounts")]
    SheetCounts,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDef {
    pub name: String,
    pub kind: ParamKind,
    pub label_key: String,
    pub default: Value,
    pub required: bool,
    pub choices: Vec<String>,
    pub choice_labels: BTreeMap<String, String>,
    pub visible_when: JsonObject,
    pub help_key: Option<String>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub remember: bool,
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ParamSchema {
    pub params: Vec<ParamDef>,
}

impl ParamSchema {
    pub fn validate(&self, values: &JsonObject) -> Vec<String> {
        self.params
            .iter()
            .filter(|parameter| {
                let value = values.get(&parameter.name).unwrap_or(&Value::Null);
                if parameter.kind == ParamKind::SheetCounts && parameter.required {
                    return !value
                        .as_object()
                        .is_some_and(|counts| counts.values().any(positive_protocol_integer));
                }
                parameter.required && !protocol_truthy(value)
            })
            .map(|parameter| format!("error.param.required.{}", parameter.name))
            .collect()
    }
}

fn positive_protocol_integer(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Number(value) if value.is_i64() => value.as_i64().is_some_and(|value| value > 0),
        Value::Number(value) if value.is_u64() => value.as_u64().is_some_and(|value| value > 0),
        _ => false,
    }
}

fn protocol_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationDef {
    pub id: String,
    pub title_key: String,
    pub description_key: String,
    pub schema: ParamSchema,
    #[serde(default)]
    pub supports_presets: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPrefs {
    pub locale: Option<String>,
    pub theme: String,
    pub font: String,
    pub font_scale: f64,
    pub compact: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            locale: None,
            theme: "system".to_string(),
            font: "system".to_string(),
            font_scale: 1.0,
            compact: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUser {
    pub id: String,
    pub email: String,
    pub username: Option<String>,
    pub display: String,
    pub allowed: bool,
    pub is_admin: bool,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionView {
    pub user_id: String,
    pub email: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub signed_in: bool,
    pub allowed: bool,
    pub is_admin: bool,
    pub session: Option<AuthSessionView>,
    pub user: Option<AppUser>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Bootstrap {
    pub protocol: u64,
    pub version: String,
    pub auth: AuthState,
    pub prefs: AppPrefs,
    pub operations: Vec<OperationDef>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub phase: String,
    pub done: u64,
    pub total: u64,
    #[serde(default)]
    pub subject: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Mkdir,
    Move,
    Copy,
    Rename,
    Write,
    WorkbookUpdate,
    RestoreBackup,
    Print,
    PdfFirstPageReplace,
    PdfPageRotate,
    CleanupTemp,
    OutlookReplyDraft,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub kind: StepKind,
    pub src: Option<String>,
    pub dst: Option<String>,
    pub note_key: Option<String>,
    pub meta: JsonObject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Warning {
    pub key: String,
    pub subject: String,
}

pub type Action = Step;
pub type Issue = Warning;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub plan_id: String,
    pub op_id: String,
    pub steps: Vec<Step>,
    pub warnings: Vec<Warning>,
    pub meta: JsonObject,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub applied: u64,
    pub failed: u64,
    pub journal_id: String,
    pub errors: Vec<Warning>,
    pub meta: JsonObject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_synced: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub journal_id: String,
    pub op_id: String,
    pub ts: String,
    pub steps: Vec<Step>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_step: Option<Step>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub name: String,
    pub path: String,
    pub code: String,
    pub confidence: f64,
    pub target: Option<String>,
    pub target_exists: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub when: String,
    pub op_id: String,
    pub moves: u64,
    pub failed: u64,
    pub journal_id: String,
    pub steps: Option<Vec<Step>>,
    pub cloud_id: Option<String>,
    pub undone: bool,
    pub undo_available: bool,
    pub from_cloud: bool,
    pub user_email: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HistoryPayload {
    pub online: bool,
    pub rows: Vec<HistoryRow>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintQueueSummary {
    pub queue_id: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub counts: BTreeMap<String, u64>,
    pub resumable: u64,
    pub file_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_supports_presets_serializes_enabled_and_defaults_disabled() {
        let enabled = OperationDef {
            id: "files.batch_print".to_string(),
            title_key: "op.files.batch_print.title".to_string(),
            description_key: "op.files.batch_print.desc".to_string(),
            schema: ParamSchema { params: Vec::new() },
            supports_presets: true,
        };
        assert_eq!(
            serde_json::to_value(enabled).expect("enabled operation should serialize")["supportsPresets"],
            serde_json::json!(true)
        );

        let disabled: OperationDef = serde_json::from_value(serde_json::json!({
            "id": "rfi.process_scans",
            "titleKey": "op.rfi.process_scans.title",
            "descriptionKey": "op.rfi.process_scans.desc",
            "schema": []
        }))
        .expect("legacy operation without preset capability should deserialize");
        assert!(!disabled.supports_presets);
    }

    #[test]
    fn protocol_one_plan_and_journal_snapshots_preserve_nulls_and_omissions() {
        let step = Step {
            kind: StepKind::Move,
            src: Some("/in/scan.pdf".to_string()),
            dst: Some("/out/RFI.pdf".to_string()),
            note_key: None,
            meta: JsonObject::new(),
        };
        let plan = Plan {
            plan_id: "plan-1".to_string(),
            op_id: "rfi.process_scans".to_string(),
            steps: vec![step.clone()],
            warnings: vec![Warning {
                key: "warn.exists".to_string(),
                subject: "scan.pdf".to_string(),
            }],
            meta: JsonObject::new(),
        };
        assert_eq!(
            serde_json::to_value(&plan).expect("plan should serialize"),
            serde_json::json!({
                "planId": "plan-1",
                "opId": "rfi.process_scans",
                "steps": [{
                    "kind": "move",
                    "src": "/in/scan.pdf",
                    "dst": "/out/RFI.pdf",
                    "noteKey": null,
                    "meta": {}
                }],
                "warnings": [{"key": "warn.exists", "subject": "scan.pdf"}],
                "meta": {}
            })
        );

        let journal = JournalEntry {
            journal_id: "journal-1".to_string(),
            op_id: plan.op_id,
            ts: "2026-08-25T00:00:00+00:00".to_string(),
            steps: vec![step],
            pending_step: None,
        };
        let encoded = serde_json::to_value(journal).expect("journal should serialize");
        assert_eq!(encoded.get("pendingStep"), None);
        assert_eq!(encoded["steps"][0]["noteKey"], serde_json::Value::Null);
    }

    #[test]
    fn protocol_one_param_and_result_snapshots_match_frozen_contracts() {
        let parameter = ParamDef {
            name: "source".to_string(),
            kind: ParamKind::Folder,
            label_key: "param.source".to_string(),
            default: serde_json::Value::Null,
            required: true,
            choices: Vec::new(),
            choice_labels: std::collections::BTreeMap::new(),
            visible_when: JsonObject::new(),
            help_key: Some("param.source.help".to_string()),
            remember: true,
        };
        assert_eq!(
            serde_json::to_value(parameter).expect("parameter should serialize"),
            serde_json::json!({
                "name": "source",
                "kind": "folder",
                "labelKey": "param.source",
                "default": null,
                "required": true,
                "choices": [],
                "choiceLabels": {},
                "visibleWhen": {},
                "helpKey": "param.source.help"
            })
        );

        let result = OperationResult {
            applied: 0,
            failed: 0,
            journal_id: String::new(),
            errors: Vec::new(),
            meta: JsonObject::new(),
            cloud_synced: None,
        };
        assert_eq!(
            serde_json::to_value(result).expect("result should serialize"),
            serde_json::json!({
                "applied": 0,
                "failed": 0,
                "journalId": "",
                "errors": [],
                "meta": {}
            })
        );
    }

    #[test]
    fn protocol_one_application_response_snapshots_match_frozen_facade() {
        let bootstrap = Bootstrap {
            protocol: 1,
            version: "0.6.0".to_string(),
            auth: AuthState {
                signed_in: false,
                allowed: false,
                is_admin: false,
                session: None,
                user: None,
            },
            prefs: AppPrefs::default(),
            operations: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(bootstrap).expect("bootstrap should serialize"),
            serde_json::json!({
                "protocol": 1,
                "version": "0.6.0",
                "auth": {
                    "signedIn": false,
                    "allowed": false,
                    "isAdmin": false,
                    "session": null,
                    "user": null
                },
                "prefs": {
                    "locale": null,
                    "theme": "system",
                    "font": "system",
                    "fontScale": 1.0,
                    "compact": false
                },
                "operations": []
            })
        );

        let history = HistoryPayload {
            online: false,
            rows: vec![HistoryRow {
                when: "2026-08-25T00:00:00Z".to_string(),
                op_id: "rfi.process_scans".to_string(),
                moves: 1,
                failed: 0,
                journal_id: "local-1".to_string(),
                steps: None,
                cloud_id: None,
                undone: false,
                undo_available: true,
                from_cloud: false,
                user_email: String::new(),
            }],
        };
        let history_json = serde_json::to_value(history).expect("history should serialize");
        assert_eq!(history_json["rows"][0]["steps"], serde_json::Value::Null);
        assert_eq!(history_json["rows"][0]["cloudId"], serde_json::Value::Null);
        assert!(history_json["rows"][0].get("undoAvailable").is_some());

        assert_eq!(
            serde_json::to_value(ProgressEvent {
                phase: "preview".to_string(),
                done: 1,
                total: 2,
                subject: String::new(),
            })
            .expect("progress should serialize"),
            serde_json::json!({"phase": "preview", "done": 1, "total": 2, "subject": ""})
        );

        let review = ReviewItem {
            name: "scan.pdf".to_string(),
            path: "/review/scan.pdf".to_string(),
            code: String::new(),
            confidence: 0.0,
            target: None,
            target_exists: false,
        };
        let review_json = serde_json::to_value(review).expect("review item should serialize");
        assert_eq!(review_json["target"], serde_json::Value::Null);
        assert_eq!(review_json["targetExists"], false);

        let queue = PrintQueueSummary {
            queue_id: "queue-1".to_string(),
            status: "paused".to_string(),
            created_at: "2026-08-25T00:00:00Z".to_string(),
            updated_at: "2026-08-25T00:01:00Z".to_string(),
            counts: BTreeMap::from([("pending".to_string(), 1)]),
            resumable: 1,
            file_count: 1,
        };
        assert_eq!(
            serde_json::to_value(queue).expect("queue summary should serialize")["fileCount"],
            1
        );

        let apply = OperationResult {
            applied: 1,
            failed: 0,
            journal_id: "journal-1".to_string(),
            errors: Vec::new(),
            meta: JsonObject::new(),
            cloud_synced: Some(false),
        };
        assert_eq!(
            serde_json::to_value(apply)
                .expect("apply result should serialize")
                .get("cloudSynced"),
            Some(&serde_json::json!(false))
        );
    }
}
