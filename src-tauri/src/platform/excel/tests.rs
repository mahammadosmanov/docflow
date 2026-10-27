#[test]
fn production_excel_failures_are_localization_keys() {
    let source = include_str!("../excel.rs").replace("\r\n", "\n");
    let production = source
        .split_once("\n#[cfg(test)]\nmod tests")
        .map(|(production, _)| production)
        .expect("Excel production source precedes its tests");
    let failure_literal = regex::Regex::new(
        r#"PrintFailure::(?:failed|unsupported|uncertain|cancelled)\(\s*\"([^\"]+)\""#,
    )
    .unwrap();
    let raw = failure_literal
        .captures_iter(production)
        .map(|capture| capture[1].to_string())
        .filter(|message| !message.starts_with("error."))
        .collect::<Vec<_>>();
    assert!(raw.is_empty(), "unlocalized Excel failures: {raw:?}");
}

mod export_supervisor;
