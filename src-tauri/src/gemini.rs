use std::{collections::BTreeMap, sync::Arc};

use serde_json::{Value, json};
use zeroize::Zeroize;

use crate::{
    errors::BackendError,
    http::{HttpError, JsonHttp},
};

pub const DEFAULT_MODEL: &str = "gemini-3.1-flash-lite";
pub const FALLBACK_MODELS: &[&str] = &["gemini-3.5-flash-lite", "gemini-3.5-flash"];
const MAX_EMAIL_CHARS: usize = 30_000;
const MAX_API_KEY_CHARS: usize = 512;
const MAX_MODEL_CHARS: usize = 128;

pub struct GeminiRequest {
    text: String,
    api_key: String,
    discipline: String,
    itp_to_items: BTreeMap<String, Vec<String>>,
}

impl GeminiRequest {
    pub fn new(
        text: impl Into<String>,
        api_key: impl Into<String>,
        discipline: impl Into<String>,
        itp_to_items: BTreeMap<String, Vec<String>>,
    ) -> Self {
        Self {
            text: text.into(),
            api_key: api_key.into(),
            discipline: discipline.into(),
            itp_to_items,
        }
    }
}

impl Drop for GeminiRequest {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

pub struct GeminiClient {
    http: Arc<dyn JsonHttp>,
    base_url: String,
    models: Vec<String>,
}

impl GeminiClient {
    pub fn new(http: Arc<dyn JsonHttp>, base_url: impl Into<String>, models: Vec<String>) -> Self {
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            models,
        }
    }

    pub fn extract(&self, request: &GeminiRequest) -> Result<Value, BackendError> {
        let key = request.api_key.trim();
        if key.is_empty() {
            return Err(BackendError::validation("Gemini API key is missing"));
        }
        if key.chars().count() > MAX_API_KEY_CHARS {
            return Err(BackendError::validation(
                "Gemini API key exceeded the size limit",
            ));
        }
        if !matches!(request.discipline.as_str(), "CIV" | "MEC" | "ELE" | "PRC") {
            return Err(BackendError::validation("Gemini discipline is invalid"));
        }
        if self.models.is_empty() || self.models.iter().any(|model| !valid_model(model)) {
            return Err(BackendError::validation("Gemini model name is invalid"));
        }

        let prompt = prompt(&request.text, &request.discipline, &request.itp_to_items)?;
        let base_body = json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": prompt}]
            }]
        });
        let formats = request_formats();
        let headers = vec![("x-goog-api-key".to_string(), key.to_string())];
        let mut saw_invalid_format = false;
        let mut saw_missing_model = false;

        for model in &self.models {
            for generation_config in &formats {
                let mut body = base_body.clone();
                body.as_object_mut()
                    .expect("Gemini request body is an object")
                    .insert("generationConfig".to_string(), generation_config.clone());
                let url = format!("{}/models/{}:generateContent", self.base_url, model);
                let response = self
                    .http
                    .send_json("POST", &url, &headers, Some(&body))
                    .map_err(map_http_error)?;
                match response.status {
                    200..=299 => {
                        let payload = response.json().map_err(|_| {
                            BackendError::request_failed(
                                "Gemini returned an invalid structured response",
                            )
                        })?;
                        return decode_result(payload, model);
                    }
                    404 => {
                        saw_missing_model = true;
                        break;
                    }
                    400 => saw_invalid_format = true,
                    status => {
                        return Err(BackendError::request_failed(format!(
                            "gemini_http_{status}"
                        )));
                    }
                }
            }
        }
        if saw_invalid_format {
            Err(BackendError::request_failed(
                "Gemini rejected all supported request formats",
            ))
        } else if saw_missing_model {
            Err(BackendError::request_failed(
                "No supported Gemini model was available",
            ))
        } else {
            Err(BackendError::request_failed("Gemini request failed"))
        }
    }
}

fn prompt(
    text: &str,
    discipline: &str,
    itp_to_items: &BTreeMap<String, Vec<String>>,
) -> Result<String, BackendError> {
    let mapping = serde_json::to_string(itp_to_items)
        .map_err(|_| BackendError::validation("Gemini lookup mapping is invalid"))?;
    let email = text.chars().take(MAX_EMAIL_CHARS).collect::<String>();
    Ok(format!(
        "Extract an RFI inspection request from the email below. \
Use only facts explicitly present in the email; never invent values. \
Discipline must be CIV, MEC, ELE, or PRC; use the currently selected discipline \
when the email does not state one. Other unknown strings must be empty. \
Count means unique new RFI numbers, not Excel rows. When several tags/areas are requested \
for the same RFI, return multiple rows with the same rfiIndex, but keep the unique RFI \
count stated by the sender. rfiIndex is 1-based and identifies which new RFI number each \
Excel row belongs to. ITP and Item values must exactly match this allowed mapping: \
{mapping}. The currently selected discipline is {discipline}. \
Return short review notes for ambiguity.\n\nEMAIL:\n{email}"
    ))
}

fn request_formats() -> [Value; 3] {
    let current_schema = response_schema(false);
    let legacy_schema = response_schema(true);
    [
        json!({
            "responseFormat": {
                "text": {
                    "mimeType": "APPLICATION_JSON",
                    "schema": current_schema
                }
            }
        }),
        json!({
            "responseMimeType": "application/json",
            "responseJsonSchema": current_schema
        }),
        json!({
            "responseMimeType": "application/json",
            "responseSchema": legacy_schema
        }),
    ]
}

fn response_schema(legacy: bool) -> Value {
    let (object, string, integer, number, array) = if legacy {
        ("OBJECT", "STRING", "INTEGER", "NUMBER", "ARRAY")
    } else {
        ("object", "string", "integer", "number", "array")
    };
    let row = json!({
        "type": object,
        "properties": {
            "rfiIndex": {"type": integer, "minimum": 1, "maximum": 100},
            "itpNo": {"type": string},
            "itemNo": {"type": string},
            "tagNo": {"type": string},
            "area": {"type": string},
            "location": {"type": string},
            "subcontractor": {"type": string}
        },
        "required": [
            "rfiIndex", "itpNo", "itemNo", "tagNo", "area", "location", "subcontractor"
        ]
    });
    json!({
        "type": object,
        "properties": {
            "discipline": {"type": string, "enum": ["CIV", "MEC", "ELE", "PRC"]},
            "count": {"type": integer, "minimum": 0, "maximum": 100},
            "rows": {"type": array, "items": row, "maxItems": 100},
            "confidence": {"type": number, "minimum": 0, "maximum": 1},
            "notes": {"type": array, "items": {"type": string}}
        },
        "required": ["discipline", "count", "rows", "confidence", "notes"]
    })
}

fn decode_result(payload: Value, model: &str) -> Result<Value, BackendError> {
    let encoded = payload
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
        .and_then(|parts| parts.first())
        .and_then(|part| part.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            BackendError::request_failed("Gemini returned an invalid structured response")
        })?;
    let mut result = serde_json::from_str::<Value>(encoded)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or_else(|| BackendError::request_failed("Gemini response is not an object"))?;
    result.insert("provider".to_string(), Value::String("gemini".to_string()));
    result.insert("model".to_string(), Value::String(model.to_string()));
    Ok(Value::Object(result))
}

fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.chars().count() <= MAX_MODEL_CHARS
        && model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn map_http_error(error: HttpError) -> BackendError {
    match error {
        HttpError::Timeout => BackendError::network_timeout(),
        HttpError::ResponseTooLarge => BackendError::response_too_large(),
        HttpError::Offline => BackendError::offline(),
        _ => BackendError::request_failed(error.to_string()),
    }
}
