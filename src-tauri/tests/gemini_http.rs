use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

use docflow_lib::{
    gemini::{GeminiClient, GeminiRequest},
    http::{HttpError, HttpResponse, JsonHttp},
};
use serde_json::{Value, json};

#[derive(Clone)]
struct FixtureHttp {
    responses: Arc<Mutex<VecDeque<HttpResponse>>>,
    requests: Arc<Mutex<Vec<FixtureRequest>>>,
}

struct FixtureRequest {
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl FixtureHttp {
    fn new(responses: Vec<HttpResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl JsonHttp for FixtureHttp {
    fn send_json(
        &self,
        _method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<HttpResponse, HttpError> {
        self.requests
            .lock()
            .expect("request lock")
            .push(FixtureRequest {
                url: url.to_string(),
                headers: headers.to_vec(),
                body: body.cloned().unwrap_or(Value::Null),
            });
        self.responses
            .lock()
            .expect("response lock")
            .pop_front()
            .ok_or(HttpError::InvalidResponse)
    }
}

#[test]
fn gemini_uses_header_secret_and_retries_legacy_structured_schema() {
    let structured = json!({
        "discipline": "MEC",
        "count": 1,
        "rows": [{
            "rfiIndex": 1,
            "itpNo": "AZE-SUN-INT-QMS-ITP-MEC-0003",
            "itemNo": "MEC-3.1",
            "tagNo": "N3_PCS-02_S7/S8",
            "area": "",
            "location": "",
            "subcontractor": ""
        }],
        "confidence": 0.9,
        "notes": []
    });
    let success = json!({
        "candidates": [{"content": {"parts": [{"text": structured.to_string()}]}}]
    });
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(400, &json!({"error": {"message": "invalid"}})),
        HttpResponse::from_json(400, &json!({"error": {"message": "invalid"}})),
        HttpResponse::from_json(200, &success),
    ]));
    let client = GeminiClient::new(
        http.clone(),
        "http://gemini.test/v1beta",
        vec!["gemini-3.1-flash-lite".to_string()],
    );
    let request = GeminiRequest::new(
        "mail body",
        "secret-key",
        "MEC",
        BTreeMap::from([(
            "AZE-SUN-INT-QMS-ITP-MEC-0003".to_string(),
            vec!["MEC-3.1".to_string()],
        )]),
    );

    let result = client
        .extract(&request)
        .expect("legacy schema retry should succeed");

    assert_eq!(result["provider"], "gemini");
    assert_eq!(result["model"], "gemini-3.1-flash-lite");
    assert_eq!(result["rows"][0]["tagNo"], "N3_PCS-02_S7/S8");
    let requests = http.requests.lock().expect("request lock");
    assert_eq!(requests.len(), 3);
    assert!(!requests[0].url.contains("secret-key"));
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(name, value)| name == "x-goog-api-key" && value == "secret-key")
    );
    assert_eq!(
        requests[0].body["generationConfig"]["responseFormat"]["text"]["schema"]["type"],
        "object"
    );
    assert_eq!(
        requests[1].body["generationConfig"]["responseJsonSchema"]["type"],
        "object"
    );
    assert_eq!(
        requests[2].body["generationConfig"]["responseSchema"]["type"],
        "OBJECT"
    );
}

#[test]
fn gemini_retries_the_next_model_after_not_found() {
    let success = json!({
        "candidates": [{"content": {"parts": [{"text": json!({
            "discipline": "CIV",
            "count": 0,
            "rows": [],
            "confidence": 0.5,
            "notes": []
        }).to_string()}]}}]
    });
    let http = Arc::new(FixtureHttp::new(vec![
        HttpResponse::from_json(404, &Value::Null),
        HttpResponse::from_json(200, &success),
    ]));
    let client = GeminiClient::new(
        http.clone(),
        "http://gemini.test/v1beta",
        vec![
            "retired-model".to_string(),
            "gemini-3.1-flash-lite".to_string(),
        ],
    );
    let request = GeminiRequest::new("mail body", "secret-key", "CIV", BTreeMap::new());

    let result = client
        .extract(&request)
        .expect("current model should succeed");

    assert_eq!(result["model"], "gemini-3.1-flash-lite");
    let urls = http
        .requests
        .lock()
        .expect("request lock")
        .iter()
        .map(|request| request.url.clone())
        .collect::<Vec<_>>();
    assert!(urls[0].contains("retired-model"));
    assert!(urls[1].contains("gemini-3.1-flash-lite"));
}
