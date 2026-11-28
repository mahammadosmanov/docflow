use std::collections::BTreeSet;

use super::*;

const SECTION: &str = "followUpExceptions";
const TABLE: &str = "follow_up_exceptions";
const CLOUD_PAGE_SIZE: usize = 500;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum ExceptionChange {
    AddShared,
    RemoveShared,
    RemoveLocal,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExceptionRequest {
    code: String,
    change: ExceptionChange,
}

#[derive(Deserialize)]
struct CloudException {
    code: String,
}

fn cloud_unavailable() -> BackendError {
    BackendError::validation("error.rfi.followUp.exceptionCloudUnavailable")
}

fn save_unconfirmed() -> BackendError {
    BackendError::validation("error.rfi.followUp.exceptionSaveUnconfirmed")
}

impl NativeBackend {
    fn stored_follow_up_exceptions(&self) -> BTreeSet<String> {
        self.settings
            .section(SECTION)
            .and_then(|mut section| section.remove("codes"))
            .and_then(|codes| serde_json::from_value::<Vec<Value>>(codes).ok())
            .into_iter()
            .flatten()
            .filter_map(|value| rfi_follow_up::parse_exception_code(value.as_str()?).ok())
            .collect()
    }

    fn cloud_exception_rows(&self, path: &str) -> Result<Vec<CloudException>, BackendError> {
        let payload = self
            .templates
            .rest("GET", path, None, now())
            .map_err(|_| cloud_unavailable())?;
        serde_json::from_value(payload).map_err(|_| cloud_unavailable())
    }

    fn cloud_follow_up_exceptions(&self) -> Result<BTreeSet<String>, BackendError> {
        let mut codes = BTreeSet::<String>::new();
        loop {
            let mut path = format!("{TABLE}?select=code&order=code.asc&limit={CLOUD_PAGE_SIZE}");
            if let Some(last) = codes.last() {
                path.push_str(&format!("&code=gt.{last}"));
            }
            let rows = self.cloud_exception_rows(&path)?;
            if rows.is_empty() {
                return Ok(codes);
            }
            for row in rows {
                if codes.last().is_some_and(|last| last >= &row.code)
                    || rfi_follow_up::parse_exception_code(&row.code).ok().as_ref()
                        != Some(&row.code)
                {
                    return Err(cloud_unavailable());
                }
                codes.insert(row.code);
                if codes.len() > rfi_follow_up::MAX_EXCEPTIONS {
                    return Err(BackendError::validation(
                        "error.rfi.followUp.exceptionLimit",
                    ));
                }
            }
        }
    }

    pub(super) fn follow_up_exceptions(&self) -> Result<Value, BackendError> {
        self.caller_identity()?;
        let shared = match self.cloud_follow_up_exceptions() {
            Ok(codes) => Some(short_codes(codes)),
            Err(error) if error == cloud_unavailable() => None,
            Err(error) => return Err(error),
        };
        let local = short_codes(self.stored_follow_up_exceptions());
        Ok(json!({ "shared": shared, "local": local }))
    }

    /// Builds this run's protected set from a read-only snapshot.
    pub(super) fn resolve_follow_up_exceptions(
        &self,
        text: Option<&str>,
    ) -> Result<(BTreeSet<String>, bool), BackendError> {
        let mut codes = self.stored_follow_up_exceptions();
        let shared = self.cloud_follow_up_exceptions();
        let shared_available = match shared {
            Ok(shared) => {
                codes.extend(shared);
                true
            }
            Err(error) if error == cloud_unavailable() => false,
            Err(error) => return Err(error),
        };
        let codes = rfi_follow_up::extend_exception_codes(codes, text.unwrap_or_default())?;
        Ok((codes, shared_available))
    }

    pub(super) fn change_follow_up_exception(&self, params: Value) -> Result<Value, BackendError> {
        self.caller_identity()?;
        let request: ExceptionRequest = decode(params)?;
        let code = rfi_follow_up::parse_exception_code(&request.code)?;
        match request.change {
            ExceptionChange::RemoveLocal => self.remove_local_exception(&code)?,
            ExceptionChange::AddShared => self.set_shared_exception(&code, true)?,
            ExceptionChange::RemoveShared => self.set_shared_exception(&code, false)?,
        }
        Ok(json!({ "ok": true }))
    }

    fn remove_local_exception(&self, code: &str) -> Result<(), BackendError> {
        self.settings
            .update_section(SECTION, |section| {
                let Some(codes) = section.get_mut("codes").and_then(Value::as_array_mut) else {
                    return false;
                };
                let count = codes.len();
                codes.retain(|current| {
                    current
                        .as_str()
                        .and_then(|value| rfi_follow_up::parse_exception_code(value).ok())
                        .as_deref()
                        != Some(code)
                });
                codes.len() != count
            })
            .map_err(|_| BackendError::settings_write())
    }

    fn set_shared_exception(&self, code: &str, add: bool) -> Result<(), BackendError> {
        let filter = format!("{TABLE}?code=eq.{code}");
        let query = format!("{filter}&select=code");
        let exists = || -> Result<bool, BackendError> {
            match self.cloud_exception_rows(&query)?.as_slice() {
                [] => Ok(false),
                [row] if row.code == code => Ok(true),
                _ => Err(cloud_unavailable()),
            }
        };
        if exists()? == add {
            return Ok(());
        }
        if add && self.cloud_follow_up_exceptions()?.len() == rfi_follow_up::MAX_EXCEPTIONS {
            return Err(BackendError::validation(
                "error.rfi.followUp.exceptionLimit",
            ));
        }
        // A timeout or duplicate insert can mean the desired state already exists.
        if add {
            let _ = self
                .templates
                .rest("POST", TABLE, Some(json!({ "code": code })), now());
        } else {
            let _ = self.templates.rest("DELETE", &filter, None, now());
        }
        match exists() {
            Ok(present) if present == add => Ok(()),
            _ => Err(save_unconfirmed()),
        }
    }
}

fn short_codes(codes: BTreeSet<String>) -> Vec<String> {
    codes
        .into_iter()
        .filter_map(|code| rfi_follow_up::canonical_follow_up_rfi(&code).map(|(_, short)| short))
        .collect()
}
