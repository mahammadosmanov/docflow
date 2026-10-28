use std::{collections::VecDeque, sync::Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::BackendError;

mod sanitize;
mod validate;
pub(crate) use sanitize::sanitize_value;
use sanitize::*;
pub(crate) use validate::validate_bundle_content;

pub(crate) const MAX_BUNDLE_BYTES: usize = 32 * 1024;
pub(crate) const MAX_RECENT_ERROR_CODES: usize = 20;
const MAX_DIAGNOSTIC_DEPTH: usize = 8;
const MAX_DIAGNOSTIC_ITEMS: usize = 64;
const MAX_DIAGNOSTIC_STRING_CHARS: usize = 512;
const MAX_ERROR_CODE_BYTES: usize = 64;
const REDACTED_SECRET: &str = "[REDACTED_SECRET]";
const REDACTED_CONTENT: &str = "[REDACTED_CONTENT]";
const REDACTED_PATH: &str = "[REDACTED_PATH]";
const REDACTED_DEPTH: &str = "[REDACTED_DEPTH]";
const REDACTED_ERROR_CODE: &str = "redacted_error_code";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiagnosticsInput {
    pub version: String,
    pub platform_os: String,
    pub platform_arch: String,
    pub ocr_available: bool,
    pub ocr_language_data_available: bool,
    pub recent_error_codes: Vec<String>,
    pub printer: PrinterDiagnostic,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrinterDiagnostic {
    pub inventory_status: String,
    pub printer_count: u64,
    pub default_printer_available: bool,
    pub latest_queue: Option<PrintQueueDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrintQueueDiagnostic {
    pub queue_status: String,
    pub printer: String,
    pub file_count: u64,
    pub counts: std::collections::BTreeMap<String, u64>,
    pub jobs: Vec<PrintJobDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrintJobDiagnostic {
    pub format: String,
    pub route: String,
    pub status: String,
    pub attempts: u32,
    pub paper: String,
    pub orientation: String,
    pub scaling: String,
    pub color: String,
    pub duplex: String,
    pub page_range: String,
    pub error_detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SupportBundlePreview {
    pub file_name: String,
    pub media_type: String,
    pub byte_length: usize,
    pub content: String,
}

#[derive(Default)]
pub(crate) struct RecentErrorCodes {
    codes: Mutex<VecDeque<String>>,
}

impl RecentErrorCodes {
    pub(crate) fn record(&self, code: &str) {
        let code = sanitize_error_code(code);
        let mut codes = self
            .codes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if codes.len() == MAX_RECENT_ERROR_CODES {
            codes.pop_front();
        }
        codes.push_back(code);
    }

    pub(crate) fn snapshot(&self) -> Vec<String> {
        self.codes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .rev()
            .cloned()
            .collect()
    }
}

pub(crate) fn build_bundle(input: DiagnosticsInput) -> Result<SupportBundlePreview, BackendError> {
    let document = BundleDocument {
        schema_version: 1,
        docflow: DocFlowDiagnostic {
            version: input.version,
        },
        platform: PlatformDiagnostic {
            arch: input.platform_arch,
            os: input.platform_os,
        },
        components: ComponentDiagnostics {
            ocr: OcrDiagnostic {
                engine: "tesseract",
                language_data_available: input.ocr_language_data_available,
                status: if input.ocr_available {
                    "available"
                } else {
                    "missing"
                },
            },
            sidecar: SidecarDiagnostic {
                required: false,
                status: "retired",
            },
            printer: input.printer,
        },
        recent_error_codes: input
            .recent_error_codes
            .into_iter()
            .take(MAX_RECENT_ERROR_CODES)
            .map(|code| sanitize_error_code(&code))
            .collect(),
        privacy: PrivacyDiagnostic {
            delivery: "local-only",
            redaction: "recursive",
        },
    };
    let mut value = serde_json::to_value(document)
        .map_err(|_| BackendError::protocol("diagnostics bundle encoding failed"))?;
    sanitize_value(&mut value, 0);
    let mut content = serde_json::to_string_pretty(&value)
        .map_err(|_| BackendError::protocol("diagnostics bundle encoding failed"))?;
    content.push('\n');
    if content.len() > MAX_BUNDLE_BYTES {
        return Err(BackendError::protocol(
            "diagnostics bundle exceeded the byte limit",
        ));
    }
    Ok(SupportBundlePreview {
        file_name: "docflow-support-diagnostics-v1.json".to_string(),
        media_type: "application/json".to_string(),
        byte_length: content.len(),
        content,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleDocument {
    schema_version: u8,
    docflow: DocFlowDiagnostic,
    platform: PlatformDiagnostic,
    components: ComponentDiagnostics,
    recent_error_codes: Vec<String>,
    privacy: PrivacyDiagnostic,
}

#[derive(Serialize)]
struct DocFlowDiagnostic {
    version: String,
}

#[derive(Serialize)]
struct PlatformDiagnostic {
    arch: String,
    os: String,
}

#[derive(Serialize)]
struct ComponentDiagnostics {
    ocr: OcrDiagnostic,
    sidecar: SidecarDiagnostic,
    printer: PrinterDiagnostic,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OcrDiagnostic {
    engine: &'static str,
    language_data_available: bool,
    status: &'static str,
}

#[derive(Serialize)]
struct SidecarDiagnostic {
    required: bool,
    status: &'static str,
}

#[derive(Serialize)]
struct PrivacyDiagnostic {
    delivery: &'static str,
    redaction: &'static str,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn input(recent_error_codes: Vec<String>) -> DiagnosticsInput {
        DiagnosticsInput {
            version: "0.6.0".to_string(),
            platform_os: "macos".to_string(),
            platform_arch: "aarch64".to_string(),
            ocr_available: true,
            ocr_language_data_available: true,
            recent_error_codes,
            printer: PrinterDiagnostic {
                inventory_status: "available".to_string(),
                printer_count: 1,
                default_printer_available: true,
                latest_queue: None,
            },
        }
    }

    #[test]
    fn bundle_shape_is_deterministic_bounded_and_contains_only_pathless_diagnostics() {
        let errors = (0..MAX_RECENT_ERROR_CODES + 5)
            .rev()
            .map(|index| format!("error_{index:02}"))
            .collect::<Vec<_>>();

        let first = build_bundle(input(errors.clone())).unwrap();
        let second = build_bundle(input(errors)).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.file_name, "docflow-support-diagnostics-v1.json");
        assert_eq!(first.media_type, "application/json");
        assert_eq!(first.byte_length, first.content.len());
        assert!(first.byte_length <= MAX_BUNDLE_BYTES);
        assert!(first.content.ends_with('\n'));
        assert_eq!(
            serde_json::from_str::<Value>(&first.content).unwrap(),
            json!({
                "schemaVersion": 1,
                "docflow": {"version": "0.6.0"},
                "platform": {"arch": "aarch64", "os": "macos"},
                "components": {
                    "ocr": {
                        "engine": "tesseract",
                        "languageDataAvailable": true,
                        "status": "available"
                    },
                    "sidecar": {
                        "required": false,
                        "status": "retired"
                    },
                    "printer": {
                        "inventoryStatus": "available",
                        "printerCount": 1,
                        "defaultPrinterAvailable": true,
                        "latestQueue": null
                    }
                },
                "recentErrorCodes": (5..25)
                    .rev()
                    .map(|index| format!("error_{index:02}"))
                    .collect::<Vec<_>>(),
                "privacy": {
                    "delivery": "local-only",
                    "redaction": "recursive"
                }
            })
        );
        for rejected in ["/Users/", "\\Users\\", "accessToken", "refreshToken"] {
            assert!(!first.content.contains(rejected), "leaked {rejected}");
        }
    }

    #[test]
    fn printer_queue_diagnostics_are_copy_valid_and_redact_private_paths() {
        let mut value = input(Vec::new());
        value.printer.latest_queue = Some(PrintQueueDiagnostic {
            queue_status: "completed".to_string(),
            printer: "EPSON L15160 Series".to_string(),
            file_count: 1,
            counts: [("completed_unconfirmed".to_string(), 1)]
                .into_iter()
                .collect(),
            jobs: vec![PrintJobDiagnostic {
                format: "pdf".to_string(),
                route: "docflow".to_string(),
                status: "completed_unconfirmed".to_string(),
                attempts: 1,
                paper: "A4".to_string(),
                orientation: "auto".to_string(),
                scaling: "fit".to_string(),
                color: "printer".to_string(),
                duplex: "printer".to_string(),
                page_range: "all".to_string(),
                error_detail: "failed at C:\\Users\\Operator\\private.pdf".to_string(),
            }],
        });

        let bundle = build_bundle(value).unwrap();

        assert!(bundle.content.contains("EPSON L15160 Series"));
        assert!(bundle.content.contains("completed_unconfirmed"));
        assert!(bundle.content.contains(REDACTED_PATH));
        assert!(!bundle.content.contains("private.pdf"));
        validate_bundle_content(&bundle.content).unwrap();
    }

    #[test]
    fn recursive_redaction_replaces_secrets_contents_paths_and_excess_depth() {
        let mut raw = json!({
            "code": "validation",
            "accessToken": "access-value",
            "credentials": {"password": "password-value"},
            "documentContents": "private document text",
            "nested": [{
                "refresh_token": "refresh-value",
                "message": "failed at /Users/alice/Private/report.pdf",
                "filePath": "C:\\Users\\Alice\\Documents\\report.pdf",
                "unc": "\\\\server\\private\\report.pdf",
                "uri": "file:///home/alice/report.pdf"
            }]
        });
        raw["deep"] = (0..12).fold(json!("hidden"), |value, _| json!({"next": value}));

        sanitize_value(&mut raw, 0);
        let encoded = serde_json::to_string(&raw).unwrap();

        assert_eq!(raw["code"], "validation");
        assert_eq!(raw["accessToken"], "[REDACTED_SECRET]");
        assert_eq!(raw["credentials"], "[REDACTED_SECRET]");
        assert_eq!(raw["documentContents"], "[REDACTED_CONTENT]");
        assert_eq!(raw["nested"][0]["message"], "[REDACTED_PATH]");
        assert_eq!(raw["nested"][0]["filePath"], "[REDACTED_PATH]");
        assert_eq!(raw["nested"][0]["unc"], "[REDACTED_PATH]");
        assert_eq!(raw["nested"][0]["uri"], "[REDACTED_PATH]");
        assert!(encoded.contains("[REDACTED_DEPTH]"));
        for rejected in [
            "access-value",
            "password-value",
            "private document text",
            "/Users/alice",
            "C:\\\\Users",
            "server",
            "/home/alice",
            "hidden",
        ] {
            assert!(!encoded.contains(rejected), "leaked {rejected}");
        }
    }

    #[test]
    fn recent_error_codes_are_newest_first_bounded_and_sanitized() {
        let log = RecentErrorCodes::default();
        log.record("validation");
        log.record("contains /Users/alice/report.pdf");
        for index in 0..MAX_RECENT_ERROR_CODES + 2 {
            log.record(&format!("safe_error_{index:02}"));
        }

        let snapshot = log.snapshot();

        assert_eq!(snapshot.len(), MAX_RECENT_ERROR_CODES);
        assert_eq!(snapshot[0], "safe_error_21");
        assert_eq!(snapshot.last().unwrap(), "safe_error_02");
        assert!(snapshot.iter().all(|code| {
            code.len() <= 64
                && code.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
                })
        }));
        assert!(!snapshot.iter().any(|code| code.contains("alice")));
    }

    #[test]
    fn recorded_error_snapshot_stays_newest_first_in_the_built_bundle() {
        let log = RecentErrorCodes::default();
        log.record("older_error");
        log.record("newest_error");

        let bundle = build_bundle(input(log.snapshot())).unwrap();
        let value = serde_json::from_str::<Value>(&bundle.content).unwrap();

        assert_eq!(
            value["recentErrorCodes"],
            json!(["newest_error", "older_error"])
        );
    }

    #[test]
    fn copy_validation_accepts_only_canonical_redacted_bundle_content() {
        let bundle = build_bundle(input(vec!["validation".to_string()])).unwrap();
        validate_bundle_content(&bundle.content).unwrap();

        let mut with_secret = serde_json::from_str::<Value>(&bundle.content).unwrap();
        with_secret["accessToken"] = json!("secret-value");
        let with_secret = format!("{}\n", serde_json::to_string_pretty(&with_secret).unwrap());
        assert_eq!(
            validate_bundle_content(&with_secret).unwrap_err().code,
            "validation"
        );

        let with_path = bundle.content.replace("0.6.0", "/Users/alice/report.pdf");
        assert_eq!(
            validate_bundle_content(&with_path).unwrap_err().code,
            "validation"
        );
        assert_eq!(
            validate_bundle_content(&"x".repeat(MAX_BUNDLE_BYTES + 1))
                .unwrap_err()
                .code,
            "validation"
        );
    }
}
