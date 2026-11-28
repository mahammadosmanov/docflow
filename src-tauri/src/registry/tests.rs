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
    assert_eq!(operations.len(), 9);
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

#[test]
fn batch_print_numeric_fields_carry_the_backend_bounds() {
    let operations = operation_registry(RegistryFacts::default());
    let batch_print = operations
        .iter()
        .find(|operation| operation.id == "files.batch_print")
        .expect("batch print is registered");
    let range = |name: &str| {
        let param = batch_print
            .schema
            .params
            .iter()
            .find(|param| param.name == name)
            .expect("parameter exists");
        (param.min, param.max)
    };
    assert_eq!(range("copies"), (Some(1), Some(99)));
    assert_eq!(range("retries"), (Some(0), Some(3)));
    assert_eq!(range("timeout_minutes"), (Some(1), Some(240)));
    let collate = batch_print
        .schema
        .params
        .iter()
        .find(|param| param.name == "collate")
        .expect("collate exists");
    assert_eq!((collate.min, collate.max), (None, None));
}

#[test]
fn concrete_documents_offer_measured_ambient_inputs_without_defaults() {
    let operations = operation_registry(RegistryFacts::default());
    let concrete = operations
        .iter()
        .find(|operation| operation.id == "rfi.generate_concrete_documents")
        .expect("concrete documents are registered");
    for name in ["measured_ambient_010", "measured_ambient_011"] {
        let param = concrete
            .schema
            .params
            .iter()
            .find(|param| param.name == name)
            .expect("measured ambient input is present");
        assert_eq!(param.kind, ParamKind::LongText);
        assert_eq!(param.default, json!(""));
        assert!(!param.remember);
    }
    assert!(
        concrete
            .schema
            .params
            .iter()
            .all(|param| !param.name.starts_with("ambient_"))
    );
}

#[test]
fn template_generation_has_a_mode_and_only_shows_what_the_mode_needs() {
    let operation = operation_registry(RegistryFacts::default())
        .into_iter()
        .find(|operation| operation.id == "templates.generate_document")
        .expect("registered");
    let param = |name: &str| {
        operation
            .schema
            .params
            .iter()
            .find(|param| param.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    assert_eq!(param("mode").choices, ["rfi", "free"]);
    assert_eq!(param("mode").default, json!("rfi"));
    assert_eq!(param("rfi_log_workbook").visible_when["mode"], "rfi");
    assert_eq!(param("rfi_numbers").visible_when["mode"], "rfi");
    assert_eq!(param("document_count").visible_when["mode"], "free");
    assert_eq!(param("follow_up_workbook").visible_when["mode"], "rfi");
    assert!(
        !param("follow_up_workbook").required,
        "the Follow-Up workbook is optional"
    );
    assert_eq!(
        (param("document_count").min, param("document_count").max),
        (Some(1), Some(100))
    );
    // Nothing hidden is required, so free mode is not blocked by the log.
    let mut values = Map::new();
    values.insert("template".into(), json!("X"));
    values.insert("mode".into(), json!("free"));
    values.insert("output_directory".into(), json!("/tmp/out"));
    assert_eq!(
        first_missing_required("templates.generate_document", &values),
        None
    );
}

#[test]
fn registry_excludes_retired_workflows() {
    let ids = operation_registry(RegistryFacts::default())
        .into_iter()
        .map(|operation| operation.id)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 9);
    for id in [
        "rfi.system_sync",
        "rfi.complete_status_sync",
        "rfi.publish_status",
        "rfi.outlook_reply_draft",
        "files.replica_publish",
    ] {
        assert!(!ids.iter().any(|registered| registered == id), "{id}");
    }
    assert!(ids.iter().any(|id| id == "files.batch_print"));
}
