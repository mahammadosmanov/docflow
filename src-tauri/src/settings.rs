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
        .map_or_else(env::temp_dir, PathBuf::from)
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
            .filter(|(_, value)| is_safe_param_value(value))
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
            start_with_windows: raw
                .get("startWithWindows")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            replica_sync_enabled: raw
                .get("replicaSyncEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
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
    data.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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

fn is_safe_param_value(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
        Value::String(text) => text.chars().count() <= 4096,
        Value::Array(items) => {
            items.len() <= 1000
                && items.iter().all(|item| {
                    item.as_str()
                        .is_some_and(|text| text.chars().count() <= 4096)
                })
        }
        Value::Object(map) => {
            map.len() <= 64
                && map
                    .values()
                    .all(|item| item.as_u64().is_some() || item.as_i64().is_some_and(|n| n >= 0))
        }
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

fn sync_parent_directory(_parent: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(_parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests;
