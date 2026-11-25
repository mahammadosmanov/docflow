//! Field map schema 2 (ADR 0058): a template's editable parts as `slots`.
//!
//! This release only reads schema 2 and turns it into the schema 1 `FieldMap`
//! every renderer already uses. DocFlow still writes schema 1, so an older
//! install that has not updated yet keeps reading new template versions. A
//! slot that needs a later release (a cell anchor, a computed value) is
//! refused with a clear message instead of being half applied.

use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::LazyLock;

use super::{
    anchors::CellAnchor,
    field_map::{FieldDef, FieldMap, PresetDef},
};

static LOG_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9]{0,31}$").expect("static token regex"));

const NEWER: &str = "The template needs a newer DocFlow version";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SlotMap {
    file_name: Option<String>,
    #[serde(default)]
    slots: Vec<Slot>,
    #[serde(default)]
    presets: Vec<PresetDef>,
    #[serde(default)]
    optional_sheets: Vec<String>,
}

#[derive(Deserialize)]
struct Slot {
    key: String,
    #[serde(default)]
    label: String,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    anchor: Option<Anchor>,
    #[serde(default)]
    source: Option<Source>,
    #[serde(default)]
    options: Vec<String>,
}

#[derive(Deserialize)]
struct Anchor {
    kind: String,
    #[serde(flatten)]
    cell: CellAnchor,
}

#[derive(Deserialize)]
struct Source {
    kind: String,
    value: Option<String>,
    token: Option<String>,
}

/// Loads a schema 2 map as the equivalent schema 1 `FieldMap`.
pub fn parse(value: &Value) -> Result<FieldMap, String> {
    let invalid = || "The template field map is invalid".to_string();
    let map: SlotMap = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    let mut fields = Vec::with_capacity(map.slots.len());
    for slot in map.slots {
        let anchor = match slot.anchor {
            None => None,
            Some(anchor) if anchor.kind == "placeholder" => None,
            Some(anchor) if anchor.kind == "cell" => Some(anchor.cell),
            Some(_) => return Err(NEWER.to_string()),
        };
        let multiline = match slot.kind.as_deref() {
            None | Some("text" | "choice") => false,
            Some("multiline") => true,
            Some(_) => return Err(NEWER.to_string()),
        };
        let start = match slot.source {
            None => String::new(),
            Some(source) => match (source.kind.as_str(), source.value, source.token) {
                ("manual", _, _) => String::new(),
                ("fixed", Some(text), _) => text,
                ("log", _, Some(token)) if LOG_TOKEN.is_match(&token) => format!("{{{token}}}"),
                ("fixed" | "log", _, _) => return Err(invalid()),
                _ => return Err(NEWER.to_string()),
            },
        };
        fields.push(FieldDef {
            key: slot.key,
            label: slot.label,
            value: start,
            options: slot.options,
            multiline,
            anchor,
        });
    }
    let mut result = FieldMap::default();
    if let Some(name) = map.file_name {
        result.file_name = name;
    }
    result.fields = fields;
    result.presets = map.presets;
    result.optional_sheets = map.optional_sheets;
    result.validate()?;
    Ok(result)
}

/// The stored form of a map: schema 1 exactly as before when no field has a
/// cell anchor, otherwise schema 2. An older install reads schema 1 and
/// lists schema 2 as unsupported, so it never silently skips an anchored cell.
pub fn to_value(map: &FieldMap) -> Value {
    if map.fields.iter().all(|field| field.anchor.is_none()) && map.optional_sheets.is_empty() {
        return serde_json::to_value(map).unwrap_or(Value::Null);
    }
    let slots = map
        .fields
        .iter()
        .map(|field| {
            let kind = if field.multiline {
                "multiline"
            } else if field.options.is_empty() {
                "text"
            } else {
                "choice"
            };
            let anchor = field.anchor.as_ref().map_or_else(
                || json!({"kind": "placeholder"}),
                |cell| {
                    let mut value = serde_json::to_value(cell).unwrap_or(Value::Null);
                    value["kind"] = json!("cell");
                    value
                },
            );
            let source = match field.value.as_str() {
                "" => json!({"kind": "manual"}),
                text => match text
                    .strip_prefix('{')
                    .and_then(|rest| rest.strip_suffix('}'))
                    .filter(|token| LOG_TOKEN.is_match(token))
                {
                    Some(token) => json!({"kind": "log", "token": token}),
                    None => json!({"kind": "fixed", "value": text}),
                },
            };
            json!({
                "key": field.key,
                "label": field.label,
                "type": kind,
                "anchor": anchor,
                "source": source,
                "options": field.options,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": 2,
        "fileName": map.file_name,
        "slots": slots,
        "presets": map.presets,
        "optionalSheets": map.optional_sheets,
    })
}
