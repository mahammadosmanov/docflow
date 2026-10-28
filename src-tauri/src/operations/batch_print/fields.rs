use super::*;

pub(super) fn normalized_choice(value: &str, choices: &[&str], default: &str) -> String {
    if choices.contains(&value) {
        value.to_string()
    } else {
        default.to_string()
    }
}

pub(super) fn choice(
    row: &Map<String, Value>,
    key: &str,
    choices: &[&str],
    default: &str,
) -> String {
    normalized_choice(&string_field(row, key), choices, default)
}

pub(super) fn string_field(row: &Map<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn unsigned_field(row: &Map<String, Value>, key: &str) -> Option<u64> {
    row.get(key).and_then(Value::as_u64)
}

pub(super) fn string_meta(plan: &Plan, key: &str) -> String {
    plan.meta
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn copy_or_default(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    key: &str,
    default: Value,
) {
    target.insert(key.to_string(), source.get(key).cloned().unwrap_or(default));
}
