use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::contracts::{OperationDef, ParamDef, ParamKind, ParamSchema};
use crate::operations::{batch_print, process_scans::FAST_COVER_OPERATION_ID};

mod concrete_documents;
mod print_batch;
mod rfi_log;
mod rfi_sync;
mod signed_scans;
use concrete_documents::concrete_documents_operation;
use print_batch::batch_print_operation;
use rfi_log::rfi_log_operation;
use rfi_sync::*;
use signed_scans::*;

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

/// Sets the integer bounds the backend enforces on the named parameters.
fn apply_int_ranges(operation: &mut OperationDef, ranges: &[(&str, i64, i64)]) {
    for param in &mut operation.schema.params {
        if let Some(&(_, min, max)) = ranges.iter().find(|(name, _, _)| *name == param.name) {
            param.min = Some(min);
            param.max = Some(max);
        }
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

    let rfi_log = rfi_log_operation(&facts);
    let rfi_follow_up = rfi_follow_up_operation();
    let concrete_documents = concrete_documents_operation();

    let rfi_system_sync = rfi_system_sync_operation();

    let rfi_complete_status_sync = rfi_complete_status_sync_operation();

    let rfi_publish_status = rfi_publish_status_operation();

    let batch_print = batch_print_operation(facts);

    let stage_signed_pages = stage_signed_pages_operation();

    let normalize_pdf_orientation = normalize_pdf_orientation_operation();

    let fast_cover_scans = fast_cover_scans_operation();

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
