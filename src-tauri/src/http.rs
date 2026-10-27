use std::{fmt, time::Duration};

use serde_json::Value;
use ureq::{Agent, http};

use crate::errors::BackendError;

#[derive(Clone, Copy)]
pub struct HttpConfig {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(15),
            max_request_bytes: 512 * 1024,
            max_response_bytes: 2 * 1024 * 1024,
        }
    }
}

#[derive(Clone)]
pub struct HttpClient {
    agent: Agent,
    config: HttpConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HttpError {
    InvalidRequest,
    RequestTooLarge,
    ResponseTooLarge,
    Timeout,
    Offline,
    InvalidResponse,
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "network request is invalid",
            Self::RequestTooLarge => "network request exceeded the size limit",
            Self::ResponseTooLarge => "network response exceeded the size limit",
            Self::Timeout => "network request timed out",
            Self::Offline => "network is unavailable",
            Self::InvalidResponse => "network response is invalid",
        })
    }
}

impl std::error::Error for HttpError {}

impl From<HttpError> for BackendError {
    fn from(error: HttpError) -> Self {
        match error {
            HttpError::Timeout => BackendError::network_timeout(),
            HttpError::ResponseTooLarge => BackendError::response_too_large(),
            HttpError::Offline => BackendError::offline(),
            _ => BackendError::request_failed(error.to_string()),
        }
    }
}

pub struct HttpResponse {
    pub status: u16,
    body: Vec<u8>,
}

impl HttpResponse {
    pub fn from_json(status: u16, value: &Value) -> Self {
        Self {
            status,
            body: serde_json::to_vec(value).unwrap_or_default(),
        }
    }

    pub fn json(&self) -> Result<Value, HttpError> {
        if self.body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&self.body).map_err(|_| HttpError::InvalidResponse)
    }
}

pub trait JsonHttp: Send + Sync {
    fn send_json(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<HttpResponse, HttpError>;
}

impl HttpClient {
    pub fn new(config: HttpConfig) -> Result<Self, HttpError> {
        if config.connect_timeout.is_zero()
            || config.request_timeout.is_zero()
            || config.max_request_bytes == 0
            || config.max_response_bytes == 0
        {
            return Err(HttpError::InvalidRequest);
        }
        let agent: Agent = Agent::config_builder()
            .timeout_connect(Some(config.connect_timeout))
            .timeout_global(Some(config.request_timeout))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self { agent, config })
    }

    pub fn send_json(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<HttpResponse, HttpError> {
        let encoded = body
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| HttpError::InvalidRequest)?;
        if encoded
            .as_ref()
            .is_some_and(|payload| payload.len() > self.config.max_request_bytes)
        {
            return Err(HttpError::RequestTooLarge);
        }

        let method =
            http::Method::from_bytes(method.as_bytes()).map_err(|_| HttpError::InvalidRequest)?;
        let mut request = http::Request::builder().method(method).uri(url);
        request = request.header("Accept", "application/json");
        if encoded.is_some() {
            request = request.header("Content-Type", "application/json");
        }
        for (name, value) in headers {
            request = request.header(name, value);
        }
        let request = request
            .body(encoded.unwrap_or_default())
            .map_err(|_| HttpError::InvalidRequest)?;
        let mut response = self.agent.run(request).map_err(map_ureq_error)?;
        if response
            .body()
            .content_length()
            .is_some_and(|length| length > self.config.max_response_bytes as u64)
        {
            return Err(HttpError::ResponseTooLarge);
        }
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(self.config.max_response_bytes as u64)
            .read_to_vec()
            .map_err(map_ureq_error)?;
        Ok(HttpResponse { status, body })
    }
}

impl JsonHttp for HttpClient {
    fn send_json(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<HttpResponse, HttpError> {
        Self::send_json(self, method, url, headers, body)
    }
}

fn map_ureq_error(error: ureq::Error) -> HttpError {
    match error {
        ureq::Error::Timeout(_) => HttpError::Timeout,
        ureq::Error::BodyExceedsLimit(_) => HttpError::ResponseTooLarge,
        ureq::Error::Http(_) | ureq::Error::BadUri(_) => HttpError::InvalidRequest,
        ureq::Error::Protocol(_) | ureq::Error::LargeResponseHeader(_, _) => {
            HttpError::InvalidResponse
        }
        _ => HttpError::Offline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_body_reader_rejects_an_oversized_payload_without_network() {
        let mut body = ureq::Body::builder().data(vec![b'x'; 65]);
        let error = body
            .with_config()
            .limit(64)
            .read_to_vec()
            .expect_err("body above the configured limit must fail");

        assert_eq!(map_ureq_error(error), HttpError::ResponseTooLarge);
    }

    #[test]
    fn request_bound_is_enforced_before_transport() {
        let client = HttpClient::new(HttpConfig {
            max_request_bytes: 8,
            ..HttpConfig::default()
        })
        .expect("HTTP client should build");

        let error = match client.send_json(
            "POST",
            "not a URL",
            &[],
            Some(&serde_json::json!({"payload": "too long"})),
        ) {
            Ok(_) => panic!("oversized request must fail before URL parsing"),
            Err(error) => error,
        };

        assert_eq!(error, HttpError::RequestTooLarge);
    }
}
