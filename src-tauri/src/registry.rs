use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::contracts::{OperationDef, ParamDef, ParamKind, ParamSchema};

const PAPER_CHOICES: &[&str] = &["auto", "A4", "A3", "A5", "Letter", "Legal"];
const ORIENTATION_CHOICES: &[&str] = &["auto", "portrait", "landscape"];
const SCALING_CHOICES: &[&str] = &["fit", "actual", "shrink"];
const COLOR_CHOICES: &[&str] = &["printer", "color", "grayscale"];
const DUPLEX_CHOICES: &[&str] = &["printer", "simplex", "long", "short"];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegistryFacts {
    pub default_workbook: Option<String>,
    pub printers: Vec<String>,
    pub default_printer: Option<String>,
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn labels(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(value, label)| ((*value).to_string(), (*label).to_string()))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn parameter(
    name: &str,
    kind: ParamKind,
    label_key: &str,
    default: Value,
    required: bool,
    choices: Vec<String>,
    choice_labels: BTreeMap<String, String>,
    visible_when: Map<String, Value>,
    help_key: Option<&str>,
) -> ParamDef {
    ParamDef {
        name: name.to_string(),
        kind,
        label_key: label_key.to_string(),
        default,
        required,
        choices,
        choice_labels,
        visible_when,
        help_key: help_key.map(str::to_string),
    }
}

fn operation(id: &str, schema: Vec<ParamDef>) -> OperationDef {
    OperationDef {
        id: id.to_string(),
        title_key: format!("op.{id}.title"),
        description_key: format!("op.{id}.desc"),
        schema: ParamSchema { params: schema },
        supports_presets: false,
    }
}

pub fn operation_registry(facts: RegistryFacts) -> Vec<OperationDef> {
    let process_scans = operation(
        "rfi.process_scans",
        vec![
            parameter(
                "source",
                ParamKind::Folder,
                "param.source",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.source.help"),
            ),
            parameter(
                "dest",
                ParamKind::Folder,
                "param.dest",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.dest.help"),
            ),
            parameter(
                "organize_into_rfi_folders",
                ParamKind::Bool,
                "param.organize_into_rfi_folders",
                json!(true),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.organize_into_rfi_folders.help"),
            ),
            parameter(
                "keep_original",
                ParamKind::Bool,
                "param.keep_original",
                json!(false),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                None,
            ),
        ],
    );

    let mut provider_visibility = Map::new();
    provider_visibility.insert("email_provider".to_string(), json!("gemini"));
    let rfi_log = operation(
        "rfi.log_add",
        vec![
            parameter(
                "workbook",
                ParamKind::File,
                "param.rfi.workbook",
                facts
                    .default_workbook
                    .clone()
                    .map_or(Value::Null, Value::String),
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.workbook.help"),
            ),
            parameter(
                "email_text",
                ParamKind::LongText,
                "param.rfi.emailText",
                json!(""),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.emailText.help"),
            ),
            parameter(
                "email_provider",
                ParamKind::Choice,
                "param.rfi.emailProvider",
                json!("offline"),
                true,
                strings(&["offline", "gemini"]),
                labels(&[
                    ("offline", "choice.rfi.emailProvider.offline"),
                    ("gemini", "choice.rfi.emailProvider.gemini"),
                ]),
                Map::new(),
                Some("param.rfi.emailProvider.help"),
            ),
            parameter(
                "gemini_api_key",
                ParamKind::Secret,
                "param.rfi.geminiApiKey",
                json!(""),
                false,
                Vec::new(),
                BTreeMap::new(),
                provider_visibility,
                Some("param.rfi.geminiApiKey.help"),
            ),
            parameter(
                "sheet_counts",
                ParamKind::SheetCounts,
                "param.rfi.sheetCounts",
                json!({"CIV": 3, "MEC": 0, "ELE": 0, "PRC": 0}),
                true,
                strings(&["CIV", "MEC", "ELE", "PRC"]),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.sheetCounts.help"),
            ),
            parameter(
                "overflow_next_day",
                ParamKind::Bool,
                "param.rfi.overflowNextDay",
                json!(true),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.overflowNextDay.help"),
            ),
        ],
    );

    let default_printer = facts
        .default_printer
        .or_else(|| facts.printers.first().cloned())
        .map_or(Value::Null, Value::String);
    let prefixed_labels = |prefix: &str, values: &[&str]| {
        values
            .iter()
            .map(|value| ((*value).to_string(), format!("{prefix}.{value}")))
            .collect()
    };
    let mut batch_print = operation(
        "files.batch_print",
        vec![
            parameter(
                "files",
                ParamKind::Files,
                "param.print.files",
                json!([]),
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.print.files.help"),
            ),
            parameter(
                "printer",
                ParamKind::Choice,
                "param.print.printer",
                default_printer,
                true,
                facts.printers,
                BTreeMap::new(),
                Map::new(),
                Some("param.print.printer.help"),
            ),
            parameter(
                "paper",
                ParamKind::Choice,
                "param.print.paper",
                json!("A4"),
                true,
                strings(PAPER_CHOICES),
                prefixed_labels("choice.print.paper", PAPER_CHOICES),
                Map::new(),
                Some("param.print.paper.help"),
            ),
            parameter(
                "orientation",
                ParamKind::Choice,
                "param.print.orientation",
                json!("auto"),
                true,
                strings(ORIENTATION_CHOICES),
                prefixed_labels("choice.print.orientation", ORIENTATION_CHOICES),
                Map::new(),
                None,
            ),
            parameter(
                "scaling",
                ParamKind::Choice,
                "param.print.scaling",
                json!("fit"),
                true,
                strings(SCALING_CHOICES),
                prefixed_labels("choice.print.scaling", SCALING_CHOICES),
                Map::new(),
                None,
            ),
            parameter(
                "color",
                ParamKind::Choice,
                "param.print.color",
                json!("printer"),
                true,
                strings(COLOR_CHOICES),
                prefixed_labels("choice.print.color", COLOR_CHOICES),
                Map::new(),
                None,
            ),
            parameter(
                "duplex",
                ParamKind::Choice,
                "param.print.duplex",
                json!("printer"),
                true,
                strings(DUPLEX_CHOICES),
                prefixed_labels("choice.print.duplex", DUPLEX_CHOICES),
                Map::new(),
                None,
            ),
            parameter(
                "copies",
                ParamKind::Int,
                "param.print.copies",
                json!(1),
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.print.copies.help"),
            ),
            parameter(
                "collate",
                ParamKind::Bool,
                "param.print.collate",
                json!(true),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.print.collate.help"),
            ),
            parameter(
                "retries",
                ParamKind::Int,
                "param.print.retries",
                json!(0),
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.print.retries.help"),
            ),
            parameter(
                "on_error",
                ParamKind::Choice,
                "param.print.onError",
                json!("pause"),
                true,
                strings(&["pause", "skip"]),
                labels(&[
                    ("pause", "choice.print.onError.pause"),
                    ("skip", "choice.print.onError.skip"),
                ]),
                Map::new(),
                Some("param.print.onError.help"),
            ),
            parameter(
                "timeout_minutes",
                ParamKind::Int,
                "param.print.timeout",
                json!(30),
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.print.timeout.help"),
            ),
        ],
    );
    batch_print.supports_presets = true;

    let stage_signed_pages = operation(
        "rfi.stage_signed_first_pages",
        vec![
            parameter(
                "signed_scans",
                ParamKind::Folder,
                "param.rfi.signedScans",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.signedScans.help"),
            ),
            parameter(
                "archive_root",
                ParamKind::Folder,
                "param.rfi.archiveRoot",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.archiveRoot.help"),
            ),
            parameter(
                "review_root",
                ParamKind::Folder,
                "param.rfi.reviewRoot",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.reviewRoot.help"),
            ),
            parameter(
                "scan_subfolders",
                ParamKind::Bool,
                "param.rfi.scanSubfolders",
                json!(false),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.scanSubfolders.help"),
            ),
        ],
    );

    vec![process_scans, rfi_log, batch_print, stage_signed_pages]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_registry_matches_protocol_one_snapshot() {
        let actual = serde_json::to_value(operation_registry(RegistryFacts::default()))
            .expect("registry should serialize");
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/protocol1_operations.json"))
                .expect("frozen protocol fixture should be valid JSON");

        assert_eq!(actual, expected);
    }

    #[test]
    fn param_schema_validation_matches_frozen_required_value_rules() {
        let operations = operation_registry(RegistryFacts::default());
        let process_scans = &operations[0];
        assert_eq!(
            process_scans.schema.validate(&Map::new()),
            vec![
                "error.param.required.source".to_string(),
                "error.param.required.dest".to_string()
            ]
        );

        let rfi_log = &operations[1];
        let values = serde_json::from_value(json!({
            "workbook": "/tmp/RFI Log.xlsx",
            "email_provider": "offline",
            "sheet_counts": {"CIV": 0, "MEC": 0, "ELE": 0, "PRC": 0}
        }))
        .expect("validation values should be an object");
        assert_eq!(
            rfi_log.schema.validate(&values),
            vec!["error.param.required.sheet_counts".to_string()]
        );
    }
}
