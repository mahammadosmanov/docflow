use super::*;

pub(super) fn rfi_log_operation(facts: &RegistryFacts) -> OperationDef {
    let mut provider_visibility = Map::new();
    provider_visibility.insert("email_provider".to_string(), json!("gemini"));
    operation(
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
    )
}
