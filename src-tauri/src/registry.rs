use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::contracts::{OperationDef, ParamDef, ParamKind, ParamSchema};
use crate::operations::{batch_print, process_scans::FAST_COVER_OPERATION_ID};

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
        min: None,
        max: None,
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
            parameter(
                "backup_directory",
                ParamKind::Folder,
                "param.rfi.followUp.backupDirectory",
                Value::Null,
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.followUp.backupDirectory.help"),
            ),
        ],
    );
    let concrete_documents = operation(
        "rfi.generate_concrete_documents",
        vec![
            parameter(
                "rfi_log_workbook",
                ParamKind::File,
                "param.rfi.documents.rfiLog",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.rfiLog.help"),
            ),
            parameter(
                "follow_up_workbook",
                ParamKind::File,
                "param.rfi.documents.followUp",
                Value::Null,
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.followUp.help"),
            ),
            parameter(
                "template_010",
                ParamKind::File,
                "param.rfi.documents.template010",
                Value::Null,
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                None,
            ),
            parameter(
                "template_011",
                ParamKind::File,
                "param.rfi.documents.template011",
                Value::Null,
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                None,
            ),
            parameter(
                "template_013",
                ParamKind::File,
                "param.rfi.documents.template013",
                Value::Null,
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                None,
            ),
            parameter(
                "template_014",
                ParamKind::File,
                "param.rfi.documents.template014",
                Value::Null,
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                None,
            ),
            parameter(
                "output_directory",
                ParamKind::Folder,
                "param.rfi.documents.output",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.output.help"),
            ),
            parameter(
                "rfis_010",
                ParamKind::LongText,
                "param.rfi.documents.rfis010",
                json!(""),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.rfis.help"),
            ),
            parameter(
                "rfis_011",
                ParamKind::LongText,
                "param.rfi.documents.rfis011",
                json!(""),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.rfis.help"),
            ),
            parameter(
                "rfis_013",
                ParamKind::LongText,
                "param.rfi.documents.rfis013",
                json!(""),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.rfis.help"),
            ),
            parameter(
                "rfis_014",
                ParamKind::LongText,
                "param.rfi.documents.rfis014",
                json!(""),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.rfis.help"),
            ),
            parameter(
                "water_cement_ratios",
                ParamKind::LongText,
                "param.rfi.documents.waterRatios",
                json!(""),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.waterRatios.help"),
            ),
            parameter(
                "ambient_min",
                ParamKind::Int,
                "param.rfi.documents.ambientMin",
                json!(19),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                None,
            ),
            parameter(
                "ambient_groups",
                ParamKind::LongText,
                "param.rfi.documents.ambientGroups",
                json!(""),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.documents.ambientGroups.help"),
            ),
            parameter(
                "ambient_max",
                ParamKind::Int,
                "param.rfi.documents.ambientMax",
                json!(24),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                None,
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

    let rfi_complete_status_sync = operation(
        "rfi.complete_status_sync",
        vec![
            parameter(
                "rfi_system_root",
                ParamKind::Folder,
                "param.rfi.completeStatus.rfiSystemRoot",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.completeStatus.rfiSystemRoot.help"),
            ),
            parameter(
                "rfi_log_workbook",
                ParamKind::File,
                "param.rfi.completeStatus.rfiLogWorkbook",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.completeStatus.rfiLogWorkbook.help"),
            ),
        ],
    );

    let rfi_publish_status = operation(
        "rfi.publish_status",
        vec![
            parameter(
                "staging_folder",
                ParamKind::Folder,
                "param.rfi.publishStatus.stagingFolder",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.publishStatus.stagingFolder.help"),
            ),
            parameter(
                "rfi_log_workbook",
                ParamKind::File,
                "param.rfi.publishStatus.rfiLogWorkbook",
                Value::Null,
                true,
                Vec::new(),
                BTreeMap::new(),
                Map::new(),
                Some("param.rfi.publishStatus.rfiLogWorkbook.help"),
            ),
        ],
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
            transient_parameter(
                "rfi_system_root",
                ParamKind::Folder,
                "param.print.rfiRoot",
                Value::Null,
                Some("param.print.rfiRoot.help"),
            ),
            transient_parameter(
                "rfi_numbers",
                ParamKind::LongText,
                "param.print.rfiNumbers",
                json!(""),
                Some("param.print.rfiNumbers.help"),
            ),
            transient_parameter(
                "include_first_page",
                ParamKind::Bool,
                "param.print.includeFirstPage",
                json!(true),
                None,
            ),
            transient_parameter(
                "include_sketch",
                ParamKind::Bool,
                "param.print.includeSketch",
                json!(true),
                None,
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
    for param in &mut batch_print.schema.params {
        let range = match param.name.as_str() {
            "copies" => Some((1, i64::from(batch_print::MAX_COPIES))),
            "retries" => Some((0, i64::from(batch_print::MAX_RETRIES))),
            "timeout_minutes" => Some((
                i64::from(batch_print::MIN_TIMEOUT_MINUTES),
                i64::from(batch_print::MAX_TIMEOUT_MINUTES),
            )),
            _ => None,
        };
        if let Some((min, max)) = range {
            param.min = Some(min);
            param.max = Some(max);
        }
    }

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

    let fast_cover_scans = operation(
        FAST_COVER_OPERATION_ID,
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
                "scan_mode",
                ParamKind::Choice,
                "param.rfi.processScans.scanMode",
                json!("multi_rfi"),
                true,
                strings(&["multi_rfi", "single_rfi"]),
                labels(&[
                    ("multi_rfi", "choice.rfi.processScans.scanMode.multiRfi"),
                    ("single_rfi", "choice.rfi.processScans.scanMode.singleRfi"),
                ]),
                Map::new(),
                Some("param.rfi.processScans.scanMode.help"),
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
            parameter(
                "correct_orientation",
                ParamKind::Bool,
                "param.rfi.processScans.correctOrientation",
                json!(false),
                false,
                Vec::new(),
                BTreeMap::new(),
                Map::from_iter([("scan_mode".to_string(), json!("multi_rfi"))]),
                Some("param.rfi.processScans.correctOrientation.help"),
            ),
        ],
    );

    vec![
        file_group,
        rfi_log,
        rfi_follow_up,
        concrete_documents,
        batch_print,
        stage_signed_pages,
        normalize_pdf_orientation,
        rfi_system_sync,
        rfi_complete_status_sync,
        rfi_publish_status,
        fast_cover_scans,
    ]
}

/// Returns the first required-parameter error key for an operation's values, or
/// `None` when the operation is unknown or every required value is present.
pub fn first_missing_required(operation_id: &str, values: &Map<String, Value>) -> Option<String> {
    operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == operation_id)?
        .schema
        .validate(values)
        .into_iter()
        .next()
}

#[cfg(test)]
mod tests;
