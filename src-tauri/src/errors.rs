use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendError {
    pub code: &'static str,
    pub message: String,
}

impl BackendError {
    pub fn protocol(message: impl Into<String>) -> Self {
        Self {
            code: "backend_protocol_error",
            message: message.into(),
        }
    }

    pub fn stopped() -> Self {
        Self {
            code: "backend_stopped",
            message: "DocFlow backend stopped unexpectedly".to_string(),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            code: "backend_unavailable",
            message: "DocFlow backend could not be started".to_string(),
        }
    }

    pub fn invalid_params() -> Self {
        Self {
            code: "backend_invalid_params",
            message: "Backend parameters must be an object".to_string(),
        }
    }

    pub fn request_failed(message: impl Into<String>) -> Self {
        Self {
            code: "backend_request_failed",
            message: message.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            code: "validation",
            message: message.into(),
        }
    }

    pub fn offline() -> Self {
        Self {
            code: "offline",
            message: "network is unavailable".to_string(),
        }
    }

    pub fn network_timeout() -> Self {
        Self {
            code: "network_timeout",
            message: "network request timed out".to_string(),
        }
    }

    pub fn response_too_large() -> Self {
        Self {
            code: "network_response_too_large",
            message: "network response exceeded the size limit".to_string(),
        }
    }

    pub fn auth_failed(message: impl Into<String>) -> Self {
        Self {
            code: "auth_failed",
            message: message.into(),
        }
    }

    pub fn auth_required() -> Self {
        Self {
            code: "auth_required",
            message: "not_authenticated".to_string(),
        }
    }

    pub fn permission_denied() -> Self {
        Self {
            code: "permission_denied",
            message: "permission_denied".to_string(),
        }
    }

    pub fn credential_store() -> Self {
        Self {
            code: "credential_store_unavailable",
            message: "secure credential storage is unavailable".to_string(),
        }
    }

    pub fn settings_write() -> Self {
        Self {
            code: "settings_write_failed",
            message: "settings could not be saved".to_string(),
        }
    }

    pub fn plan_unavailable() -> Self {
        Self {
            code: "plan_not_found_or_expired",
            message: "plan not found or expired; preview again".to_string(),
        }
    }

    pub fn operation_in_progress() -> Self {
        Self {
            code: "operation_in_progress",
            message: "another operation is already running".to_string(),
        }
    }

    pub fn cancelled() -> Self {
        Self {
            code: "operation_cancelled",
            message: "operation cancelled".to_string(),
        }
    }

    pub fn plan_owner_mismatch() -> Self {
        Self {
            code: "plan_owner_mismatch",
            message: "plan belongs to another owner".to_string(),
        }
    }

    pub fn plan_session_mismatch() -> Self {
        Self {
            code: "plan_session_mismatch",
            message: "plan belongs to another session".to_string(),
        }
    }

    pub fn path_grant_unavailable() -> Self {
        Self {
            code: "path_grant_not_found_or_expired",
            message: "path grant was not found or expired".to_string(),
        }
    }

    pub fn path_grant_scope_mismatch() -> Self {
        Self {
            code: "path_grant_scope_mismatch",
            message: "path grant belongs to another request or operation".to_string(),
        }
    }

    pub fn path_traversal() -> Self {
        Self {
            code: "path_traversal",
            message: "path traversal is not allowed".to_string(),
        }
    }

    pub fn path_escape() -> Self {
        Self {
            code: "path_escape",
            message: "path escapes the granted root".to_string(),
        }
    }

    pub fn path_access_denied() -> Self {
        Self {
            code: "path_access_denied",
            message: "path grant does not allow this access".to_string(),
        }
    }

    pub fn path_invalid(message: impl Into<String>) -> Self {
        Self {
            code: "path_invalid",
            message: message.into(),
        }
    }

    pub fn journal_owner_mismatch() -> Self {
        Self {
            code: "journal_owner_mismatch",
            message: "journal belongs to another owner".to_string(),
        }
    }

    pub fn journal_invalid(message: impl Into<String>) -> Self {
        Self {
            code: "journal_invalid",
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_error_snapshots_keep_stable_codes_and_protocol_shape() {
        assert_eq!(
            serde_json::to_value(BackendError::protocol("invalid frame"))
                .expect("error should serialize"),
            serde_json::json!({
                "code": "backend_protocol_error",
                "message": "invalid frame"
            })
        );
        assert_eq!(
            serde_json::to_value(BackendError::plan_unavailable()).expect("error should serialize"),
            serde_json::json!({
                "code": "plan_not_found_or_expired",
                "message": "plan not found or expired; preview again"
            })
        );
        assert_eq!(
            BackendError::operation_in_progress().code,
            "operation_in_progress"
        );
    }
}
