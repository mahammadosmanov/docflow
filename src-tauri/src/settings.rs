use std::{
    env, fs,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use serde_json::{Map, Value};
use tempfile::NamedTempFile;
use unicode_casefold::UnicodeCaseFold;

use crate::contracts::{AppPrefs, JsonObject, OperationDef, OperationPreset, ParamDef, ParamKind};
use crate::credentials::StoredSession;

const MIN_FONT_SCALE: f64 = 0.8;
const MAX_FONT_SCALE: f64 = 1.6;
const LOCALES: &[&str] = &["tr", "en", "ru", "az"];
const THEMES: &[&str] = &["system", "light", "dark"];
const FONTS: &[&str] = &["system", "sans", "serif", "mono"];
const MAX_OPERATION_PRESETS: usize = 20;
const MAX_PRESET_NAME_CHARS: usize = 64;

#[derive(Debug)]
pub enum PresetStoreError {
    InvalidName,
    InvalidValue,
    DuplicateName,
    LimitReached,
    Missing,
    Write(io::Error),
}

impl std::fmt::Display for PresetStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidName => "preset name is invalid",
            Self::InvalidValue => "preset value is invalid",
            Self::DuplicateName => "preset name already exists",
            Self::LimitReached => "preset limit reached",
            Self::Missing => "preset not found",
            Self::Write(error) => return error.fmt(formatter),
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for PresetStoreError {}

impl From<io::Error> for PresetStoreError {
    fn from(error: io::Error) -> Self {
        Self::Write(error)
    }
}

pub fn default_settings_path() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join(".docflow")
        .join("settings.json")
}

pub struct SettingsStore {
    path: PathBuf,
    data: Mutex<JsonObject>,
    writer: Arc<SettingsWriter>,
}

type SettingsWriter = dyn Fn(&Path, &Map<String, Value>) -> io::Result<()> + Send + Sync;

impl SettingsStore {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        Self::try_new_with_writer(path, atomic_write)
    }

    fn try_new_with_writer<F>(path: PathBuf, writer: F) -> io::Result<Self>
    where
        F: Fn(&Path, &Map<String, Value>) -> io::Result<()> + Send + Sync + 'static,
    {
        let mut data = load_object(&path);
        if scrub_legacy_operation_secrets(&mut data) {
            writer(&path, &data)?;
        }
        Ok(Self {
            path,
            data: Mutex::new(data),
            writer: Arc::new(writer),
        })
    }

    pub fn at_default_path() -> io::Result<Self> {
        Self::new(default_settings_path())
    }

    pub fn reload(&self) -> io::Result<()> {
        let mut data = load_object(&self.path);
        if scrub_legacy_operation_secrets(&mut data) {
            (self.writer)(&self.path, &data)?;
        }
        *lock_data(&self.data) = data;
        Ok(())
    }

    pub fn params_for(&self, op_id: &str) -> JsonObject {
        lock_data(&self.data)
            .get("params")
            .and_then(Value::as_object)
            .and_then(|params| params.get(op_id))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    pub fn save_params(&self, op_id: &str, values: JsonObject) -> io::Result<()> {
        let clean = values
            .into_iter()
            .filter(|(_, value)| {
                matches!(
                    value,
                    Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
                )
            })
            .collect();

        let mut data = lock_data(&self.data);
        let mut next = data.clone();
        let mut params = next
            .get("params")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        params.insert(op_id.to_string(), Value::Object(clean));
        next.insert("params".to_string(), Value::Object(params));
        (self.writer)(&self.path, &next)?;
        *data = next;
        Ok(())
    }

    pub fn presets_for(
        &self,
        operation: &OperationDef,
    ) -> Result<Vec<OperationPreset>, PresetStoreError> {
        let mut data = lock_data(&self.data);
        let mut presets = sanitized_operation_presets(&data, operation);
        let encoded = serde_json::to_value(&presets)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let existing = data
            .get("operationPresets")
            .and_then(Value::as_object)
            .and_then(|sections| sections.get(&operation.id));
        if existing.is_some_and(|value| value != &encoded) {
            let mut next = data.clone();
            let mut sections = next
                .get("operationPresets")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            sections.insert(operation.id.clone(), encoded);
            next.insert("operationPresets".to_string(), Value::Object(sections));
            (self.writer)(&self.path, &next)?;
            *data = next;
        }
        presets.sort_by(|left, right| {
            preset_name_key(&left.name)
                .cmp(&preset_name_key(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(presets)
    }

    pub fn save_preset(
        &self,
        operation: &OperationDef,
        name: &str,
        values: JsonObject,
        replace: bool,
    ) -> Result<OperationPreset, PresetStoreError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_PRESET_NAME_CHARS {
            return Err(PresetStoreError::InvalidName);
        }
        let values = validate_new_preset_values(operation, values)?;
        let mut data = lock_data(&self.data);
        let mut next = data.clone();
        let mut preset_sections = next
            .get("operationPresets")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut presets = sanitized_operation_presets(&next, operation);
        let name_key = preset_name_key(name);
        let duplicate = presets
            .iter()
            .position(|preset| preset_name_key(&preset.name) == name_key);
        let preset = if let Some(index) = duplicate {
            if !replace {
                return Err(PresetStoreError::DuplicateName);
            }
            let preset = OperationPreset {
                id: presets[index].id.clone(),
                name: name.to_string(),
                values,
            };
            presets[index] = preset.clone();
            preset
        } else {
            if presets.len() >= MAX_OPERATION_PRESETS {
                return Err(PresetStoreError::LimitReached);
            }
            let preset = OperationPreset {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.to_string(),
                values,
            };
            presets.push(preset.clone());
            preset
        };
        preset_sections.insert(
            operation.id.clone(),
            serde_json::to_value(presets)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        );
        next.insert(
            "operationPresets".to_string(),
            Value::Object(preset_sections),
        );
        (self.writer)(&self.path, &next)?;
        *data = next;
        Ok(preset)
    }

    pub fn delete_preset(
        &self,
        operation: &OperationDef,
        preset_id: &str,
    ) -> Result<(), PresetStoreError> {
        let mut data = lock_data(&self.data);
        let mut next = data.clone();
        let mut preset_sections = next
            .get("operationPresets")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut presets = sanitized_operation_presets(&next, operation);
        let Some(index) = presets.iter().position(|preset| preset.id == preset_id) else {
            return Err(PresetStoreError::Missing);
        };
        presets.remove(index);
        preset_sections.insert(
            operation.id.clone(),
            serde_json::to_value(presets)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        );
        next.insert(
            "operationPresets".to_string(),
            Value::Object(preset_sections),
        );
        (self.writer)(&self.path, &next)?;
        *data = next;
        Ok(())
    }

    pub fn section(&self, name: &str) -> Option<JsonObject> {
        lock_data(&self.data)
            .get(name)
            .and_then(Value::as_object)
            .cloned()
    }

    pub fn save_section(&self, name: &str, value: Option<JsonObject>) -> io::Result<()> {
        let mut data = lock_data(&self.data);
        let mut next = data.clone();
        if let Some(value) = value {
            next.insert(name.to_string(), Value::Object(value));
        } else {
            next.remove(name);
        }
        (self.writer)(&self.path, &next)?;
        *data = next;
        Ok(())
    }

    pub fn save_session_metadata(
        &self,
        user_id: &str,
        email: &str,
        expires_at: i64,
    ) -> io::Result<()> {
        let mut data = lock_data(&self.data);
        let mut next = data.clone();
        let mut session = next
            .get("session")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        session.remove("accessToken");
        session.remove("refreshToken");
        session.insert("userId".to_string(), Value::String(user_id.to_string()));
        session.insert("email".to_string(), Value::String(email.to_string()));
        session.insert("expiresAt".to_string(), Value::from(expires_at));
        next.insert("session".to_string(), Value::Object(session));
        (self.writer)(&self.path, &next)?;
        *data = next;
        Ok(())
    }

    pub fn strip_legacy_session_tokens(&self, expected: &StoredSession) -> io::Result<bool> {
        let mut data = lock_data(&self.data);
        let Some(current_session) = data.get("session").and_then(Value::as_object) else {
            return Ok(false);
        };
        let tokens_match = current_session
            .get("accessToken")
            .and_then(Value::as_str)
            .is_some_and(|token| token == expected.access_token())
            && current_session
                .get("refreshToken")
                .and_then(Value::as_str)
                .is_some_and(|token| token == expected.refresh_token());
        if !tokens_match {
            return Ok(false);
        }

        let mut next = data.clone();
        let mut session = current_session.clone();
        session.remove("accessToken");
        session.remove("refreshToken");
        next.insert("session".to_string(), Value::Object(session));
        (self.writer)(&self.path, &next)?;
        *data = next;
        Ok(true)
    }

    pub fn preferences(&self) -> AppPrefs {
        let raw = self.section("prefs").unwrap_or_default();
        normalize_preferences(AppPrefs {
            locale: raw
                .get("locale")
                .and_then(Value::as_str)
                .map(str::to_string),
            theme: raw
                .get("theme")
                .and_then(Value::as_str)
                .unwrap_or("system")
                .to_string(),
            font: raw
                .get("font")
                .and_then(Value::as_str)
                .unwrap_or("system")
                .to_string(),
            font_scale: raw.get("fontScale").and_then(Value::as_f64).unwrap_or(1.0),
            compact: raw.get("compact").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    pub fn save_preferences(&self, preferences: AppPrefs) -> io::Result<()> {
        let normalized = normalize_preferences(preferences);
        let encoded = serde_json::to_value(normalized)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.save_section("prefs", encoded.as_object().cloned())
    }
}

fn normalize_preferences(mut preferences: AppPrefs) -> AppPrefs {
    if !preferences
        .locale
        .as_deref()
        .is_some_and(|locale| LOCALES.contains(&locale))
    {
        preferences.locale = None;
    }
    if !THEMES.contains(&preferences.theme.as_str()) {
        preferences.theme = "system".to_string();
    }
    if !FONTS.contains(&preferences.font.as_str()) {
        preferences.font = "system".to_string();
    }
    preferences.font_scale = if preferences.font_scale.is_finite() {
        preferences.font_scale.clamp(MIN_FONT_SCALE, MAX_FONT_SCALE)
    } else {
        1.0
    };
    preferences
}

fn lock_data(data: &Mutex<JsonObject>) -> MutexGuard<'_, JsonObject> {
    data.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn preset_name_key(name: &str) -> String {
    name.case_fold().collect()
}

fn load_object(path: &Path) -> JsonObject {
    fs::read(path)
        .ok()
        .and_then(|encoded| serde_json::from_slice::<Value>(&encoded).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn operation_presets(data: &JsonObject, op_id: &str) -> Vec<OperationPreset> {
    data.get("operationPresets")
        .and_then(Value::as_object)
        .and_then(|presets| presets.get(op_id))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|preset| serde_json::from_value(preset.clone()).ok())
        .filter(|preset: &OperationPreset| {
            let trimmed = preset.name.trim();
            preset.name == trimmed
                && !trimmed.is_empty()
                && trimmed.chars().count() <= MAX_PRESET_NAME_CHARS
                && uuid::Uuid::parse_str(&preset.id).is_ok()
        })
        .collect()
}

fn sanitized_operation_presets(
    data: &JsonObject,
    operation: &OperationDef,
) -> Vec<OperationPreset> {
    operation_presets(data, &operation.id)
        .into_iter()
        .map(|mut preset| {
            preset.values = sanitize_stored_preset_values(operation, preset.values);
            preset
        })
        .collect()
}

fn sanitize_stored_preset_values(operation: &OperationDef, values: JsonObject) -> JsonObject {
    values
        .into_iter()
        .filter(|(name, value)| {
            operation
                .schema
                .params
                .iter()
                .find(|parameter| parameter.name == *name)
                .is_some_and(|parameter| stored_preset_value_matches(operation, parameter, value))
        })
        .collect()
}

fn stored_preset_value_matches(
    operation: &OperationDef,
    parameter: &ParamDef,
    value: &Value,
) -> bool {
    if operation.id == "files.batch_print" && parameter.name == "printer" {
        return value.as_str().is_some_and(|printer| {
            !printer.trim().is_empty()
                && printer.chars().count() <= 512
                && !printer.chars().any(char::is_control)
        });
    }
    preset_value_matches(parameter, value, true)
}

fn validate_new_preset_values(
    operation: &OperationDef,
    values: JsonObject,
) -> Result<JsonObject, PresetStoreError> {
    let mut filtered = JsonObject::new();
    for (name, value) in values {
        let Some(parameter) = operation
            .schema
            .params
            .iter()
            .find(|parameter| parameter.name == name)
        else {
            continue;
        };
        if !matches!(
            parameter.kind,
            ParamKind::Bool | ParamKind::Choice | ParamKind::Int
        ) {
            continue;
        }
        if !preset_value_matches(parameter, &value, true) {
            return Err(PresetStoreError::InvalidValue);
        }
        filtered.insert(name, value);
    }
    Ok(filtered)
}

fn preset_value_matches(parameter: &ParamDef, value: &Value, validate_choices: bool) -> bool {
    match parameter.kind {
        ParamKind::Bool => value.is_boolean(),
        ParamKind::Choice => value.as_str().is_some_and(|choice| {
            !validate_choices || parameter.choices.iter().any(|item| item == choice)
        }),
        ParamKind::Int => value.as_i64().is_some() || value.as_u64().is_some(),
        _ => false,
    }
}

fn scrub_legacy_operation_secrets(data: &mut JsonObject) -> bool {
    let Some(params) = data.get_mut("params").and_then(Value::as_object_mut) else {
        return false;
    };
    let mut changed = false;
    for values in params.values_mut().filter_map(Value::as_object_mut) {
        changed |= values.remove("gemini_api_key").is_some();
    }
    changed
}

fn atomic_write(path: &Path, data: &Map<String, Value>) -> io::Result<()> {
    atomic_write_with_parent_sync(path, data, sync_parent_directory)
}

fn atomic_write_with_parent_sync<F>(
    path: &Path,
    data: &Map<String, Value>,
    sync_parent: F,
) -> io::Result<()>
where
    F: FnOnce(&Path) -> io::Result<()>,
{
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "settings path has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let serialized = serde_json::to_vec(data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    let mut temporary = NamedTempFile::new_in(parent)?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        writer.write_all(&serialized)?;
        writer.flush()?;
    }
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;

    match sync_parent(parent) {
        Ok(()) => Ok(()),
        Err(_) if fs::read(path).is_ok_and(|persisted| persisted == serialized) => Ok(()),
        Err(error) => Err(error),
    }
}

fn sync_parent_directory(parent: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn operation(id: &str) -> OperationDef {
        crate::registry::operation_registry(crate::registry::RegistryFacts {
            printers: vec!["Office Printer".to_string()],
            default_printer: Some("Office Printer".to_string()),
            ..crate::registry::RegistryFacts::default()
        })
        .into_iter()
        .find(|operation| operation.id == id)
        .expect("test operation should exist")
    }

    #[test]
    fn operation_presets_round_trip_without_losing_unknown_settings() {
        let directory = tempfile::tempdir().expect("temporary settings directory");
        let path = directory.path().join("settings.json");
        fs::write(&path, br#"{"custom":{"keep":true}}"#).unwrap();
        let store = SettingsStore::new(path.clone()).unwrap();

        let saved = store
            .save_preset(
                &operation("files.batch_print"),
                "  A4 duplex  ",
                serde_json::from_value(json!({
                    "printer": "Office Printer",
                    "paper": "A4",
                    "duplex": "long",
                    "copies": 2
                }))
                .unwrap(),
                false,
            )
            .unwrap();

        assert_eq!(saved.name, "A4 duplex");
        assert!(uuid::Uuid::parse_str(&saved.id).is_ok());
        assert_eq!(
            store.presets_for(&operation("files.batch_print")).unwrap(),
            vec![saved.clone()]
        );
        assert_eq!(
            SettingsStore::new(path.clone())
                .unwrap()
                .presets_for(&operation("files.batch_print"))
                .unwrap(),
            vec![saved]
        );
        let persisted: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(persisted["custom"], json!({"keep": true}));
    }

    #[test]
    fn operation_presets_reject_duplicates_and_replace_preserves_id() {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(directory.path().join("settings.json")).unwrap();
        let first = store
            .save_preset(
                &operation("files.batch_print"),
                "Office",
                serde_json::from_value(json!({"copies": 1})).unwrap(),
                false,
            )
            .unwrap();

        assert!(matches!(
            store.save_preset(
                &operation("files.batch_print"),
                "office",
                serde_json::from_value(json!({"copies": 2})).unwrap(),
                false,
            ),
            Err(PresetStoreError::DuplicateName)
        ));

        let replaced = store
            .save_preset(
                &operation("files.batch_print"),
                "office",
                serde_json::from_value(json!({"copies": 2})).unwrap(),
                true,
            )
            .unwrap();
        assert_eq!(replaced.id, first.id);
        assert_eq!(replaced.values["copies"], 2);
        assert_eq!(
            store.presets_for(&operation("files.batch_print")).unwrap(),
            vec![replaced]
        );
    }

    #[test]
    fn operation_presets_use_unicode_default_case_folding_for_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(directory.path().join("settings.json")).unwrap();
        let batch_print = operation("files.batch_print");
        store
            .save_preset(
                &batch_print,
                "Straße",
                serde_json::from_value(json!({"copies": 1})).unwrap(),
                false,
            )
            .unwrap();

        assert!(matches!(
            store.save_preset(
                &batch_print,
                "STRASSE",
                serde_json::from_value(json!({"copies": 2})).unwrap(),
                false,
            ),
            Err(PresetStoreError::DuplicateName)
        ));
    }

    #[test]
    fn operation_presets_enforce_name_and_capacity_limits() {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(directory.path().join("settings.json")).unwrap();
        let values = || serde_json::from_value(json!({"copies": 1})).unwrap();

        for invalid in ["   ".to_string(), "x".repeat(65)] {
            assert!(matches!(
                store.save_preset(&operation("files.batch_print"), &invalid, values(), false),
                Err(PresetStoreError::InvalidName)
            ));
        }
        store
            .save_preset(
                &operation("files.batch_print"),
                &"🖨".repeat(64),
                values(),
                false,
            )
            .unwrap();
        for index in 1..20 {
            store
                .save_preset(
                    &operation("files.batch_print"),
                    &format!("Preset {index:02}"),
                    values(),
                    false,
                )
                .unwrap();
        }
        assert!(matches!(
            store.save_preset(&operation("files.batch_print"), "Overflow", values(), false,),
            Err(PresetStoreError::LimitReached)
        ));
    }

    #[test]
    fn operation_presets_list_deterministically_and_delete_exact_ids() {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(directory.path().join("settings.json")).unwrap();
        let values = || serde_json::from_value(json!({"copies": 1})).unwrap();
        let zulu = store
            .save_preset(&operation("files.batch_print"), "zulu", values(), false)
            .unwrap();
        let alpha = store
            .save_preset(&operation("files.batch_print"), "Alpha", values(), false)
            .unwrap();
        store
            .save_preset(&operation("files.batch_print"), "bravo", values(), false)
            .unwrap();

        assert_eq!(
            store
                .presets_for(&operation("files.batch_print"))
                .unwrap()
                .iter()
                .map(|preset| preset.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "bravo", "zulu"]
        );
        assert!(matches!(
            store.delete_preset(&operation("rfi.process_scans"), &alpha.id),
            Err(PresetStoreError::Missing)
        ));
        assert!(matches!(
            store.delete_preset(&operation("files.batch_print"), "stale-id"),
            Err(PresetStoreError::Missing)
        ));
        store
            .delete_preset(&operation("files.batch_print"), &zulu.id)
            .unwrap();
        assert_eq!(
            store
                .presets_for(&operation("files.batch_print"))
                .unwrap()
                .iter()
                .map(|preset| preset.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "bravo"]
        );
    }

    #[test]
    fn operation_presets_drop_only_malformed_stored_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let valid_id = uuid::Uuid::new_v4().to_string();
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "operationPresets": {
                    "files.batch_print": [
                        {
                            "id": valid_id,
                            "name": "Valid",
                            "values": {
                                "copies": 2,
                                "files": {"selectionId": "forbidden"},
                                "orientation": "diagonal",
                                "printer": "bad\nprinter"
                            }
                        },
                        {"id": uuid::Uuid::new_v4().to_string(), "name": "Missing values"},
                        {"id": "not-a-uuid", "name": "Bad ID", "values": {}},
                        {"id": uuid::Uuid::new_v4().to_string(), "name": "", "values": {}},
                        42
                    ]
                },
                "custom": {"keep": true}
            }))
            .unwrap(),
        )
        .unwrap();

        let presets = SettingsStore::new(path.clone())
            .unwrap()
            .presets_for(&operation("files.batch_print"))
            .unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, valid_id);
        assert_eq!(presets[0].name, "Valid");
        assert_eq!(
            presets[0].values,
            json!({"copies": 2}).as_object().unwrap().clone()
        );

        let persisted: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(
            persisted["operationPresets"]["files.batch_print"],
            json!([{"id": valid_id, "name": "Valid", "values": {"copies": 2}}])
        );
        assert_eq!(persisted["custom"], json!({"keep": true}));
    }

    #[test]
    fn operation_presets_scrub_write_failure_preserves_memory_and_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let original = serde_json::to_vec(&json!({
            "operationPresets": {
                "files.batch_print": [{
                    "id": uuid::Uuid::new_v4().to_string(),
                    "name": "Unsafe",
                    "values": {"copies": 2, "files": {"selectionId": "forbidden"}}
                }]
            },
            "custom": {"keep": true}
        }))
        .unwrap();
        fs::write(&path, &original).unwrap();
        let store = SettingsStore::try_new_with_writer(path.clone(), |_path, _data| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected scrub write failure",
            ))
        })
        .unwrap();

        assert!(matches!(
            store.presets_for(&operation("files.batch_print")),
            Err(PresetStoreError::Write(error))
                if error.kind() == io::ErrorKind::PermissionDenied
        ));
        assert_eq!(fs::read(path).unwrap(), original);
        assert_eq!(
            store.section("operationPresets").unwrap()["files.batch_print"][0]["values"]["files"],
            json!({"selectionId": "forbidden"})
        );
    }

    #[test]
    fn operation_presets_preserve_a_temporarily_unavailable_printer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let preset_id = uuid::Uuid::new_v4().to_string();
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "operationPresets": {
                    "files.batch_print": [{
                        "id": preset_id,
                        "name": "Offline queue",
                        "values": {
                            "printer": "Temporarily Disconnected Printer",
                            "orientation": "landscape"
                        }
                    }]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let presets = SettingsStore::new(path.clone())
            .unwrap()
            .presets_for(&operation("files.batch_print"))
            .unwrap();

        assert_eq!(
            presets[0].values["printer"],
            "Temporarily Disconnected Printer"
        );
        let persisted: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(
            persisted["operationPresets"]["files.batch_print"][0]["values"]["printer"],
            "Temporarily Disconnected Printer"
        );
    }

    #[test]
    fn operation_presets_failed_write_rolls_back_memory_and_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let original = br#"{"custom":{"keep":true}}"#.to_vec();
        fs::write(&path, &original).unwrap();
        let store = SettingsStore::try_new_with_writer(path.clone(), |_path, _data| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected atomic write failure",
            ))
        })
        .unwrap();

        assert!(matches!(
            store.save_preset(
                &operation("files.batch_print"),
                "Office",
                serde_json::from_value(json!({"copies": 2})).unwrap(),
                false,
            ),
            Err(PresetStoreError::Write(error))
                if error.kind() == io::ErrorKind::PermissionDenied
        ));
        assert!(
            store
                .presets_for(&operation("files.batch_print"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn atomic_write_accepts_verified_parent_sync_failure_after_persist() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, br#"{"before":true}"#).unwrap();
        let next: JsonObject = serde_json::from_value(json!({
            "after": true,
            "nested": {"value": 2}
        }))
        .unwrap();
        let serialized = serde_json::to_vec(&next).unwrap();

        let result = atomic_write_with_parent_sync(&path, &next, |_parent| {
            Err(io::Error::other("injected parent sync failure"))
        });

        assert!(result.is_ok());
        assert_eq!(fs::read(path).unwrap(), serialized);
    }

    #[test]
    fn atomic_write_fails_closed_when_parent_sync_failure_readback_mismatches() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, br#"{"before":true}"#).unwrap();
        let next: JsonObject = serde_json::from_value(json!({"after": true})).unwrap();
        let serialized = serde_json::to_vec(&next).unwrap();

        let result = atomic_write_with_parent_sync(&path, &next, |_parent| {
            fs::write(&path, br#"{"unexpected":true}"#)?;
            Err(io::Error::other("injected parent sync failure"))
        });

        assert!(result.is_err());
        assert_ne!(fs::read(path).unwrap(), serialized);
    }

    #[test]
    fn settings_round_trip_is_atomic_compatible_and_filters_operation_params() {
        let directory = tempfile::tempdir().expect("temporary settings directory");
        let path = directory.path().join("settings.json");
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "prefs": {"locale": "az", "fontScale": 1.25},
                "session": {"accessToken": "legacy-access", "refreshToken": "legacy-refresh"},
                "custom": [1, 2, 3],
                "params": {"other.operation": {"keep": "value"}}
            }))
            .expect("legacy settings should encode"),
        )
        .expect("legacy settings should be written");

        let store = SettingsStore::new(path.clone()).unwrap();
        store
            .save_params(
                "rfi.process_scans",
                serde_json::from_value(json!({
                    "source": "/in",
                    "dest": "/out",
                    "keep_original": true,
                    "nullable": null,
                    "unsafe_list": [1, 2, 3],
                    "unsafe_object": {"path": "/forged"}
                }))
                .expect("parameters should be an object"),
            )
            .expect("settings save should succeed");

        let restored = SettingsStore::new(path.clone()).unwrap();
        assert_eq!(
            restored.params_for("rfi.process_scans"),
            serde_json::from_value(json!({
                "source": "/in",
                "dest": "/out",
                "keep_original": true,
                "nullable": null
            }))
            .expect("expected parameters should be an object")
        );
        assert_eq!(
            restored.params_for("other.operation"),
            serde_json::from_value(json!({"keep": "value"}))
                .expect("expected parameters should be an object")
        );

        let encoded = fs::read_to_string(&path).expect("settings should be readable");
        let persisted: serde_json::Value =
            serde_json::from_str(&encoded).expect("settings should remain valid JSON");
        assert_eq!(persisted["prefs"]["locale"], "az");
        assert_eq!(persisted["session"]["accessToken"], "legacy-access");
        assert_eq!(persisted["custom"], json!([1, 2, 3]));
        assert!(!encoded.contains("unsafe_list"));
        assert!(!encoded.contains("unsafe_object"));
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("directory listing")
                .count(),
            1
        );
    }

    #[test]
    fn preferences_validate_clamp_round_trip_and_reset_compatibly() {
        let directory = tempfile::tempdir().expect("temporary settings directory");
        let path = directory.path().join("settings.json");
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "prefs": {
                    "locale": 42,
                    "theme": "neon",
                    "font": "comic",
                    "fontScale": "large",
                    "compact": "yes"
                }
            }))
            .expect("legacy settings should encode"),
        )
        .expect("legacy settings should be written");
        let store = SettingsStore::new(path.clone()).unwrap();
        assert_eq!(store.preferences(), AppPrefs::default());

        store
            .save_preferences(AppPrefs {
                locale: Some("az".to_string()),
                theme: "dark".to_string(),
                font: "mono".to_string(),
                font_scale: 9.0,
                compact: true,
            })
            .expect("preferences should save");
        assert_eq!(
            SettingsStore::new(path.clone()).unwrap().preferences(),
            AppPrefs {
                locale: Some("az".to_string()),
                theme: "dark".to_string(),
                font: "mono".to_string(),
                font_scale: 1.6,
                compact: true,
            }
        );

        store
            .save_preferences(AppPrefs::default())
            .expect("default preferences should save");
        let reset: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("reset settings should be readable"))
                .expect("reset settings should be valid JSON");
        assert_eq!(reset["prefs"]["locale"], serde_json::Value::Null);
    }

    #[test]
    fn legacy_operation_secrets_are_scrubbed_from_memory_and_disk_on_load() {
        let directory = tempfile::tempdir().expect("temporary settings directory");
        let path = directory.path().join("settings.json");
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "params": {
                    "rfi.log_add": {
                        "email_provider": "gemini",
                        "gemini_api_key": "legacy-secret"
                    }
                },
                "custom": {"keep": true}
            }))
            .unwrap(),
        )
        .unwrap();

        let store = SettingsStore::new(path.clone()).unwrap();
        assert_eq!(
            store.params_for("rfi.log_add"),
            serde_json::from_value(json!({"email_provider": "gemini"})).unwrap()
        );
        let encoded = fs::read_to_string(path).unwrap();
        assert!(!encoded.contains("gemini_api_key"));
        assert!(!encoded.contains("legacy-secret"));
        assert!(encoded.contains("\"custom\""));
    }

    #[test]
    fn startup_surfaces_injected_legacy_secret_scrub_write_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let original = serde_json::to_vec(&json!({
            "params": {
                "rfi.log_add": {
                    "email_provider": "gemini",
                    "gemini_api_key": "legacy-plaintext-secret"
                }
            },
            "custom": {"keep": true}
        }))
        .unwrap();
        fs::write(&path, &original).unwrap();

        let result = SettingsStore::try_new_with_writer(path.clone(), |_path, _data| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected atomic write failure",
            ))
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains("legacy-plaintext-secret")
        );
    }
}
