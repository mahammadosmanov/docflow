use super::*;

pub(super) fn batch_print_operation(facts: RegistryFacts) -> OperationDef {
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
    apply_int_ranges(
        &mut batch_print,
        &[
            ("copies", 1, i64::from(batch_print::MAX_COPIES)),
            ("retries", 0, i64::from(batch_print::MAX_RETRIES)),
            (
                "timeout_minutes",
                i64::from(batch_print::MIN_TIMEOUT_MINUTES),
                i64::from(batch_print::MAX_TIMEOUT_MINUTES),
            ),
        ],
    );
    batch_print
}
