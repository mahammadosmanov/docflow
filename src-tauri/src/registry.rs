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

fn prefixed_labels(prefix: &str, values: &[&str]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|value| ((*value).to_string(), format!("{prefix}.{value}")))
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
        remember: true,
    }
}

fn transient_parameter(
    name: &str,
    kind: ParamKind,
    label_key: &str,
    default: Value,
    help_key: Option<&str>,
) -> ParamDef {
    let mut parameter = parameter(
        name,
        kind,
        label_key,
        default,
        false,
        Vec::new(),
        BTreeMap::new(),
        Map::new(),
        help_key,
    );
    parameter.remember = false;
    parameter
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
    let file_group = operation(
        "rfi.group_files",
        vec![
            parameter(
                "source",
                ParamKind::Folder,
                "param.rfi.fileGroup.source",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.fileGroup.source.help"),
            ),
            parameter(
                "destination",
                ParamKind::Folder,
                "param.rfi.fileGroup.destination",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.fileGroup.destination.help"),
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

    let rfi_follow_up = operation(
        "rfi.follow_up_sync",
        vec![
            parameter(
                "rfi_log_workbook",
                ParamKind::File,
                "param.rfi.followUp.rfiLogWorkbook",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.followUp.rfiLogWorkbook.help"),
            ),
            parameter(
                "follow_up_workbook",
                ParamKind::File,
                "param.rfi.followUp.followUpWorkbook",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.followUp.followUpWorkbook.help"),
            ),
        ],
    );

    let rfi_system_sync = operation(
        "rfi.system_sync",
        vec![
            parameter(
                "rfi_system_root",
                ParamKind::Folder,
                "param.rfi.systemSync.rfiSystemRoot",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.systemSync.rfiSystemRoot.help"),
            ),
            parameter(
                "rfi_log_workbook",
                ParamKind::File,
                "param.rfi.systemSync.rfiLogWorkbook",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.systemSync.rfiLogWorkbook.help"),
            ),
            parameter(
                "follow_up_workbook",
                ParamKind::File,
                "param.rfi.systemSync.followUpWorkbook",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.systemSync.followUpWorkbook.help"),
            ),
            transient_parameter(
                "force_rescan",
                ParamKind::Bool,
                "param.scanIndex.forceRescan",
                json!(false),
                Some("param.scanIndex.forceRescan.help"),
            ),
        ],
    );

    let outlook_reply_draft = operation(
        "rfi.outlook_reply_draft",
        vec![parameter(
            "reply_text",
            ParamKind::LongText,
            "param.rfi.outlookReplyDraft.replyText",
            json!(""),
            true,
            Vec::new(),
            BTreeMap::new(),
            Map::new(),
            Some("param.rfi.outlookReplyDraft.replyText.help"),
        )],
    );

    let default_printer = facts
        .default_printer
        .or_else(|| facts.printers.first().cloned())
        .map_or(Value::Null, Value::String);
    let mut batch_print_files = parameter(
        "files",
        ParamKind::Files,
        "param.print.files",
        json!([]),
        true,
        Vec::new(),
        BTreeMap::new(),
        Map::new(),
        Some("param.print.files.help"),
    );
    // A previous file selection is task identity, not a reusable setting. The
    // operator must explicitly select files for every new print procedure.
    batch_print_files.remember = false;
    let mut batch_print = operation(
        "files.batch_print",
        vec![
            batch_print_files,
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
                json!(true),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.scanSubfolders.help"),
            ),
        ],
    );

    let normalize_pdf_orientation = operation(
        "rfi.normalize_pdf_orientation",
        vec![
            parameter(
                "rfi_system_root",
                ParamKind::Folder,
                "param.rfi.orientation.rfiSystemRoot",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.orientation.rfiSystemRoot.help"),
            ),
            parameter(
                "review_root",
                ParamKind::Folder,
                "param.rfi.orientation.reviewRoot",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.orientation.reviewRoot.help"),
            ),
            transient_parameter(
                "force_rescan",
                ParamKind::Bool,
                "param.scanIndex.forceRescan",
                json!(false),
                Some("param.scanIndex.forceRescan.help"),
            ),
        ],
    );

    vec![
        process_scans,
        file_group,
        rfi_log,
        rfi_follow_up,
        outlook_reply_draft,
        batch_print,
        stage_signed_pages,
        normalize_pdf_orientation,
        rfi_system_sync,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_registry_matches_protocol_one_snapshot() {
        let operations = operation_registry(RegistryFacts::default());
        assert_eq!(
            operations
                .iter()
                .map(|operation| operation.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "rfi.process_scans",
                "rfi.group_files",
                "rfi.log_add",
                "rfi.follow_up_sync",
                "rfi.outlook_reply_draft",
                "files.batch_print",
                "rfi.stage_signed_first_pages",
                "rfi.normalize_pdf_orientation",
                "rfi.system_sync",
            ]
        );
        let follow_up = &operations[3];
        assert_eq!(
            follow_up.schema.validate(&Map::new()),
            vec![
                "error.param.required.rfi_log_workbook".to_string(),
                "error.param.required.follow_up_workbook".to_string(),
            ]
        );
        let system_sync = &operations[8];
        assert_eq!(
            system_sync.schema.validate(&Map::new()),
            vec![
                "error.param.required.rfi_system_root".to_string(),
                "error.param.required.rfi_log_workbook".to_string(),
                "error.param.required.follow_up_workbook".to_string(),
            ]
        );
        assert_eq!(
            operations[4].schema.validate(&Map::new()),
            vec!["error.param.required.reply_text".to_string()]
        );

        let actual = serde_json::to_value(operations).expect("registry should serialize");
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

        let rfi_log = &operations[2];
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
