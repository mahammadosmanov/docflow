use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSession {
    access_token: String,
    refresh_token: String,
    user_id: String,
    email: String,
    expires_at: i64,
}

impl StoredSession {
    pub fn new(
        access_token: String,
        refresh_token: String,
        user_id: String,
        email: String,
        expires_at: i64,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            user_id,
            email,
            expires_at,
        }
    }

    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }

    pub fn expired_at(&self, now: i64) -> bool {
        now >= self.expires_at.saturating_sub(60)
    }

    pub fn matches_tokens(&self, access_token: &str, refresh_token: &str) -> bool {
        self.access_token == access_token && self.refresh_token == refresh_token
    }
}

impl Drop for StoredSession {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialError {
    Unavailable,
    InvalidData,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "secure credential storage is unavailable",
            Self::InvalidData => "secure credential data is invalid",
        })
    }
}

impl std::error::Error for CredentialError {}

pub trait CredentialStore: Send + Sync {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError>;
    fn write(&self, session: &StoredSession) -> Result<(), CredentialError>;
    fn delete(&self) -> Result<(), CredentialError>;
}

pub struct OsCredentialStore;

impl OsCredentialStore {
    #[cfg(target_os = "macos")]
    pub const BACKEND: &'static str = "macos-keychain";
    #[cfg(target_os = "windows")]
    pub const BACKEND: &'static str = "windows-credential-manager";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub const BACKEND: &'static str = "unsupported";

    pub const fn new() -> Self {
        Self
    }
}

impl Default for OsCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
const SERVICE: &str = "com.docflow.desktop.supabase";
#[cfg(any(target_os = "macos", target_os = "windows"))]
const ACCOUNT: &str = "session-v1";

#[cfg(target_os = "macos")]
impl CredentialStore for OsCredentialStore {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
        use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
        use security_framework::os::macos::keychain::SecKeychain;

        const ERR_SEC_ITEM_NOT_FOUND: i32 = -25_300;
        let _interaction_lock =
            SecKeychain::disable_user_interaction().map_err(|_| CredentialError::Unavailable)?;
        let mut query = ItemSearchOptions::new();
        query
            .class(ItemClass::generic_password())
            .service(SERVICE)
            .account(ACCOUNT)
            .load_data(true)
            .limit(1)
            .skip_authenticated_items(true);
        let mut results = match query.search() {
            Ok(results) => results,
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => return Ok(None),
            Err(_) => return Err(CredentialError::Unavailable),
        };
        let mut encoded = match results.pop() {
            Some(SearchResult::Data(encoded)) if results.is_empty() => encoded,
            None => return Ok(None),
            Some(_) => return Err(CredentialError::InvalidData),
        };
        let result = serde_json::from_slice(&encoded).map_err(|_| CredentialError::InvalidData);
        encoded.zeroize();
        result.map(Some)
    }

    fn write(&self, session: &StoredSession) -> Result<(), CredentialError> {
        let mut encoded = serde_json::to_vec(session).map_err(|_| CredentialError::InvalidData)?;
        let result =
            security_framework::passwords::set_generic_password(SERVICE, ACCOUNT, &encoded)
                .map_err(|_| CredentialError::Unavailable);
        encoded.zeroize();
        result
    }

    fn delete(&self) -> Result<(), CredentialError> {
        const ERR_SEC_ITEM_NOT_FOUND: i32 = -25_300;
        match security_framework::passwords::delete_generic_password(SERVICE, ACCOUNT) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(_) => Err(CredentialError::Unavailable),
        }
    }
}

#[cfg(target_os = "windows")]
impl CredentialStore for OsCredentialStore {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
        use std::{ptr, slice};
        use windows_sys::Win32::{
            Foundation::{ERROR_NOT_FOUND, GetLastError},
            Security::Credentials::{CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW},
        };

        struct CredentialBuffer(*mut CREDENTIALW);
        impl Drop for CredentialBuffer {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    // SAFETY: CredReadW allocated this buffer and transfers its
                    // ownership to the caller, which must release it with CredFree.
                    unsafe { CredFree(self.0.cast()) };
                }
            }
        }

        let target = wide(SERVICE);
        let mut raw = ptr::null_mut();
        // SAFETY: target is null-terminated and raw is a valid output pointer.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0 {
            // SAFETY: CredReadW just failed on this thread, so GetLastError
            // returns the corresponding Win32 error code.
            if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err(CredentialError::Unavailable);
        }
        let buffer = CredentialBuffer(raw);
        if buffer.0.is_null() {
            return Err(CredentialError::InvalidData);
        }
        // SAFETY: CredReadW returned a valid CREDENTIALW whose blob remains
        // allocated until buffer is dropped below.
        let credential = unsafe { &*buffer.0 };
        // SAFETY: CredentialBlob and CredentialBlobSize describe the allocated blob.
        let blob = unsafe {
            slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        };
        let mut encoded = blob.to_vec();
        let result = serde_json::from_slice(&encoded).map_err(|_| CredentialError::InvalidData);
        encoded.zeroize();
        result.map(Some)
    }

    fn write(&self, session: &StoredSession) -> Result<(), CredentialError> {
        use windows_sys::Win32::Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
        };

        let mut target = wide(SERVICE);
        let mut account = wide(ACCOUNT);
        let mut encoded = serde_json::to_vec(session).map_err(|_| CredentialError::InvalidData)?;
        let blob_size = u32::try_from(encoded.len()).map_err(|_| CredentialError::InvalidData)?;
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: blob_size,
            CredentialBlob: encoded.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: account.as_mut_ptr(),
            ..Default::default()
        };
        // SAFETY: all pointers in credential remain valid for the duration of
        // CredWriteW, and their lengths are described by the structure fields.
        let result = if unsafe { CredWriteW(&credential, 0) } == 0 {
            Err(CredentialError::Unavailable)
        } else {
            Ok(())
        };
        encoded.zeroize();
        result
    }

    fn delete(&self) -> Result<(), CredentialError> {
        use windows_sys::Win32::{
            Foundation::{ERROR_NOT_FOUND, GetLastError},
            Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW},
        };

        let target = wide(SERVICE);
        // SAFETY: target is a valid null-terminated UTF-16 string.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
            return Ok(());
        }
        // SAFETY: CredDeleteW just failed on this thread, so GetLastError
        // returns the corresponding Win32 error code.
        if unsafe { GetLastError() } == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err(CredentialError::Unavailable)
        }
    }
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl CredentialStore for OsCredentialStore {
    fn read(&self) -> Result<Option<StoredSession>, CredentialError> {
        Err(CredentialError::Unavailable)
    }

    fn write(&self, _session: &StoredSession) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable)
    }

    fn delete(&self) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable)
    }
}
