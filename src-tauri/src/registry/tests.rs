use super::*;

#[test]
fn registry_exposes_only_fast_rfi_scan() {
    let operations = operation_registry(RegistryFacts::default());
    let scan_operations = operations
        .iter()
        .filter(|operation| operation.id.starts_with("rfi.process_scans"))
        .map(|operation| operation.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(scan_operations, vec!["rfi.process_scans_fast_covers"]);
}

#[test]
fn fast_scan_mode_defaults_to_multi_rfi_and_offers_single_rfi() {
    let operations = operation_registry(RegistryFacts::default());
    let scan = operations
        .iter()
        .find(|operation| operation.id == FAST_COVER_OPERATION_ID)
        .unwrap();
    let mode = scan
        .schema
        .params
        .iter()
        .find(|parameter| parameter.name == "scan_mode")
        .expect("Fast RFI Scan exposes scan mode");

    assert_eq!(mode.kind, ParamKind::Choice);
    assert_eq!(mode.default, json!("multi_rfi"));
    assert_eq!(mode.choices, vec!["multi_rfi", "single_rfi"]);
    let orientation = scan
        .schema
        .params
        .iter()
        .find(|parameter| parameter.name == "correct_orientation")
        .expect("orientation correction is present in Multi RFI mode");
    assert_eq!(
        orientation.visible_when.get("scan_mode"),
        Some(&json!("multi_rfi"))
    );
}

#[test]
fn param_schema_validation_matches_frozen_required_value_rules() {
    let operations = operation_registry(RegistryFacts::default());
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
#[test]
fn concrete_documents_require_log_and_output_but_not_follow_up_by_default() {
    let operations = operation_registry(RegistryFacts::default());
    assert_eq!(operations.len(), 11);
    let operation = operations
        .iter()
        .find(|operation| operation.id == "rfi.generate_concrete_documents")
        .expect("concrete document generator is registered");
    assert_eq!(
        operation.schema.validate(&Map::new()),
        vec![
            "error.param.required.rfi_log_workbook".to_string(),
            "error.param.required.output_directory".to_string(),
        ]
    );
    let follow_up = operation
        .schema
        .params
        .iter()
        .find(|parameter| parameter.name == "follow_up_workbook")
        .expect("optional Follow-Up source is registered");
    assert!(!follow_up.required);
}

#[test]
fn publish_status_requires_staging_folder_and_log_workbook() {
    let operations = operation_registry(RegistryFacts::default());
    let operation = operations
        .iter()
        .find(|operation| operation.id == "rfi.publish_status")
        .expect("publish status is registered");
    assert_eq!(
        operation.schema.validate(&Map::new()),
        vec![
            "error.param.required.staging_folder".to_string(),
            "error.param.required.rfi_log_workbook".to_string(),
        ]
    );
}

#[test]
fn preview_reports_the_first_missing_required_value() {
    let operation = operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == "files.batch_print")
        .expect("batch print is registered");
    let required: Vec<&str> = operation
        .schema
        .params
        .iter()
        .filter(|param| param.required)
        .map(|param| param.name.as_str())
        .collect();
    assert!(
        required.len() >= 2,
        "batch print needs several required values"
    );

    let mut values = Map::new();
    assert_eq!(
        first_missing_required("files.batch_print", &values),
        Some(format!("error.param.required.{}", required[0]))
    );
    for name in &required {
        values.insert((*name).to_string(), json!("present"));
    }
    assert_eq!(first_missing_required("files.batch_print", &values), None);
    assert_eq!(
        first_missing_required("no.such.operation", &Map::new()),
        None
    );
}
