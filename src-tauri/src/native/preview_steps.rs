use super::*;
use std::collections::BTreeMap;

/// Builds one extra batch row; blank location, class or supplier falls back to the first follow-up row.
pub(super) fn additional_concrete_batch(
    row: &BTreeMap<String, String>,
    first: Option<&BTreeMap<String, String>>,
) -> ConcreteBatch {
    let field = |key: &str| {
        first
            .and_then(|row| row.get(key))
            .map_or("", String::as_str)
    };
    let value = |key: &str| {
        row.get(key)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if matches!(key, "location" | "concrete_class" | "supplier") {
                    field(key)
                } else {
                    ""
                }
            })
            .to_string()
    };
    ConcreteBatch {
        tag: value("location"),
        concrete_class: value("concrete_class"),
        supplier: value("supplier"),
        batch_ticket: value("batch_ticket"),
        truck: value("truck"),
        quantity: value("quantity"),
        total_quantity: value("total_quantity"),
        batch_time: excel_time_text(&value("batch_time")),
        start_time: excel_time_text(&value("start_time")),
        end_time: excel_time_text(&value("end_time")),
        plant_slump: value("plant_slump"),
        in_situ_slump: value("in_situ_slump"),
        plant_temp: value("plant_temp"),
        in_situ_temp: value("in_situ_temp"),
        samples: value("samples"),
    }
}

pub(super) fn selected_templates(
    values: &Map<String, Value>,
    requested: [(DocumentType, Vec<String>, &str); 4],
) -> Result<Vec<(DocumentType, Vec<String>, PathBuf)>, BackendError> {
    let mut selections = Vec::new();
    for (document_type, numbers, parameter) in requested {
        if numbers.is_empty() {
            continue;
        }
        let template = required_selected_path(values, parameter).map_err(|_| {
            concrete_documents_error("Select an example template for every requested document type")
        })?;
        if !template.is_file() {
            return Err(concrete_documents_error(
                "A selected template file is unavailable",
            ));
        }
        selections.push((document_type, numbers, template));
    }
    Ok(selections)
}

pub(super) fn output_steps(
    generated: Vec<GeneratedFile>,
    output_directory: &Path,
    warnings: &mut Vec<Warning>,
) -> Result<Vec<Step>, BackendError> {
    let mut steps = Vec::new();
    let mut destinations = HashSet::new();
    for GeneratedFile { filename, bytes } in generated {
        if Path::new(&filename)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(filename.as_str())
        {
            return Err(concrete_documents_error("Generated output name is invalid"));
        }
        let destination = output_directory.join(&filename);
        if !destinations.insert(destination.clone()) {
            return Err(concrete_documents_error(
                "Two selected records produce the same output filename",
            ));
        }
        if destination.exists() {
            warnings.push(Warning {
                key: "warn.rfi.documents.outputExists".to_string(),
                subject: filename,
            });
            continue;
        }
        let mut meta = JsonObject::new();
        meta.insert(
            "contentBase64".to_string(),
            Value::String(base64::engine::general_purpose::STANDARD.encode(bytes)),
        );
        steps.push(Step {
            kind: StepKind::Write,
            src: None,
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: Some("step.rfi.generatedConcreteDocument".to_string()),
            meta,
        });
    }
    Ok(steps)
}
