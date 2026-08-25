use std::{
    env, fs,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use serde_json::{Map, Value};
use tempfile::NamedTempFile;

use crate::contracts::{AppPrefs, JsonObject};
use crate::credentials::StoredSession;

const MIN_FONT_SCALE: f64 = 0.8;
const MAX_FONT_SCALE: f64 = 1.6;
const LOCALES: &[&str] = &["tr", "en", "ru", "az"];
const THEMES: &[&str] = &["system", "light", "dark"];
const FONTS: &[&str] = &["system", "sans", "serif", "mono"];

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
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        Self::try_new_with_writer(path, atomic_write)
    }

    fn try_new_with_writer<F>(path: PathBuf, writer: F) -> io::Result<Self>
    where
        F: FnOnce(&Path, &Map<String, Value>) -> io::Result<()>,
    {
        let mut data = load_object(&path);
        if scrub_legacy_operation_secrets(&mut data) {
            writer(&path, &data)?;
        }
        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }

    pub fn at_default_path() -> io::Result<Self> {
        Self::new(default_settings_path())
    }

    pub fn reload(&self) -> io::Result<()> {
        let mut data = load_object(&self.path);
        if scrub_legacy_operation_secrets(&mut data) {
            atomic_write(&self.path, &data)?;
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
        atomic_write(&self.path, &next)?;
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
        atomic_write(&self.path, &next)?;
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
        atomic_write(&self.path, &next)?;
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
        atomic_write(&self.path, &next)?;
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

fn load_object(path: &Path) -> JsonObject {
    fs::read(path)
        .ok()
        .and_then(|encoded| serde_json::from_slice::<Value>(&encoded).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
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
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "settings path has no parent")
    })?;
    fs::create_dir_all(parent)?;

    let mut temporary = NamedTempFile::new_in(parent)?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        serde_json::to_writer(&mut writer, data)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        writer.flush()?;
    }
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;

    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

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
