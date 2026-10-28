use super::*;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub(crate) enum BackendMethod {
    #[serde(rename = "app/bootstrap")]
    AppBootstrap,
    #[serde(rename = "settings/update")]
    SettingsUpdate,
    #[serde(rename = "settings/reset")]
    SettingsReset,
    #[serde(rename = "settings/replicaStatus")]
    SettingsReplicaStatus,
    #[serde(rename = "settings/params")]
    SettingsParams,
    #[serde(rename = "settings/saveParams")]
    SettingsSaveParams,
    #[serde(rename = "settings/presets")]
    SettingsPresets,
    #[serde(rename = "settings/savePreset")]
    SettingsSavePreset,
    #[serde(rename = "settings/deletePreset")]
    SettingsDeletePreset,
    #[serde(rename = "support/diagnostics")]
    SupportDiagnostics,
    #[serde(rename = "support/copyDiagnostics")]
    SupportCopyDiagnostics,
    #[serde(rename = "support/activityLog")]
    SupportActivityLog,
    #[serde(rename = "auth/signIn")]
    AuthSignIn,
    #[serde(rename = "auth/signUp")]
    AuthSignUp,
    #[serde(rename = "auth/signOut")]
    AuthSignOut,
    #[serde(rename = "auth/usernameAvailable")]
    AuthUsernameAvailable,
    #[serde(rename = "auth/loadMe")]
    AuthLoadMe,
    #[serde(rename = "auth/setUsername")]
    AuthSetUsername,
    #[serde(rename = "auth/listUsers")]
    AuthListUsers,
    #[serde(rename = "auth/setFlags")]
    AuthSetFlags,
    #[serde(rename = "engine/preview")]
    EnginePreview,
    #[serde(rename = "engine/apply")]
    EngineApply,
    #[serde(rename = "engine/cancel")]
    EngineCancel,
    #[serde(rename = "engine/printQueueLatest")]
    EnginePrintQueueLatest,
    #[serde(rename = "engine/printQueueDiscard")]
    EnginePrintQueueDiscard,
    #[serde(rename = "engine/printQueueResume")]
    EnginePrintQueueResume,
    #[serde(rename = "engine/revise")]
    EngineRevise,
    #[serde(rename = "engine/copyTable")]
    EngineCopyTable,
    #[serde(rename = "engine/rfiReplyPreview")]
    EngineRfiReplyPreview,
    #[serde(rename = "engine/reviewScan")]
    EngineReviewScan,
    #[serde(rename = "engine/reviewPreview")]
    EngineReviewPreview,
    #[serde(rename = "engine/undo")]
    EngineUndo,
    #[serde(rename = "history/list")]
    HistoryList,
    #[serde(rename = "paths/select")]
    PathsSelect,
    #[serde(rename = "paths/reveal")]
    PathsReveal,
    #[serde(rename = "recovery/list")]
    RecoveryList,
    #[serde(rename = "rfi/printFind")]
    RfiPrintFind,
    #[serde(rename = "watch/status")]
    WatchStatus,
    #[serde(rename = "watch/update")]
    WatchUpdate,
    #[serde(rename = "watch/open")]
    WatchOpen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MethodOwner {
    Rust,
}

impl BackendMethod {
    pub(crate) const ALL: [Self; 40] = [
        Self::AppBootstrap,
        Self::AuthUsernameAvailable,
        Self::AuthSignIn,
        Self::AuthSignUp,
        Self::AuthSignOut,
        Self::AuthLoadMe,
        Self::AuthSetUsername,
        Self::AuthListUsers,
        Self::AuthSetFlags,
        Self::SettingsUpdate,
        Self::SettingsReset,
        Self::SettingsReplicaStatus,
        Self::SettingsParams,
        Self::SettingsSaveParams,
        Self::SettingsPresets,
        Self::SettingsSavePreset,
        Self::SettingsDeletePreset,
        Self::SupportDiagnostics,
        Self::SupportCopyDiagnostics,
        Self::SupportActivityLog,
        Self::EnginePreview,
        Self::EngineApply,
        Self::EngineCancel,
        Self::EnginePrintQueueLatest,
        Self::EnginePrintQueueDiscard,
        Self::EnginePrintQueueResume,
        Self::EngineRevise,
        Self::EngineCopyTable,
        Self::EngineRfiReplyPreview,
        Self::EngineReviewScan,
        Self::EngineReviewPreview,
        Self::EngineUndo,
        Self::HistoryList,
        Self::PathsSelect,
        Self::PathsReveal,
        Self::RecoveryList,
        Self::RfiPrintFind,
        Self::WatchStatus,
        Self::WatchUpdate,
        Self::WatchOpen,
    ];

    pub(crate) const fn owner(self) -> MethodOwner {
        MethodOwner::Rust
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AppBootstrap => "app/bootstrap",
            Self::SettingsUpdate => "settings/update",
            Self::SettingsReset => "settings/reset",
            Self::SettingsReplicaStatus => "settings/replicaStatus",
            Self::SettingsParams => "settings/params",
            Self::SettingsSaveParams => "settings/saveParams",
            Self::SettingsPresets => "settings/presets",
            Self::SettingsSavePreset => "settings/savePreset",
            Self::SettingsDeletePreset => "settings/deletePreset",
            Self::SupportDiagnostics => "support/diagnostics",
            Self::SupportCopyDiagnostics => "support/copyDiagnostics",
            Self::SupportActivityLog => "support/activityLog",
            Self::AuthSignIn => "auth/signIn",
            Self::AuthSignUp => "auth/signUp",
            Self::AuthSignOut => "auth/signOut",
            Self::AuthUsernameAvailable => "auth/usernameAvailable",
            Self::AuthLoadMe => "auth/loadMe",
            Self::AuthSetUsername => "auth/setUsername",
            Self::AuthListUsers => "auth/listUsers",
            Self::AuthSetFlags => "auth/setFlags",
            Self::EnginePreview => "engine/preview",
            Self::EngineApply => "engine/apply",
            Self::EngineCancel => "engine/cancel",
            Self::EnginePrintQueueLatest => "engine/printQueueLatest",
            Self::EnginePrintQueueDiscard => "engine/printQueueDiscard",
            Self::EnginePrintQueueResume => "engine/printQueueResume",
            Self::EngineRevise => "engine/revise",
            Self::EngineCopyTable => "engine/copyTable",
            Self::EngineRfiReplyPreview => "engine/rfiReplyPreview",
            Self::EngineReviewScan => "engine/reviewScan",
            Self::EngineReviewPreview => "engine/reviewPreview",
            Self::EngineUndo => "engine/undo",
            Self::HistoryList => "history/list",
            Self::PathsSelect => "paths/select",
            Self::PathsReveal => "paths/reveal",
            Self::RfiPrintFind => "rfi/printFind",
            Self::RecoveryList => "recovery/list",
            Self::WatchStatus => "watch/status",
            Self::WatchUpdate => "watch/update",
            Self::WatchOpen => "watch/open",
        }
    }

    pub(super) fn authorize(self, state: &AuthState) -> Result<(), BackendError> {
        if matches!(
            self,
            Self::AppBootstrap
                | Self::SettingsUpdate
                | Self::SettingsReset
                | Self::SettingsReplicaStatus
                | Self::SupportDiagnostics
                | Self::SupportCopyDiagnostics
                | Self::SupportActivityLog
                | Self::AuthSignIn
                | Self::AuthSignUp
                | Self::AuthSignOut
                | Self::AuthUsernameAvailable
                | Self::AuthLoadMe
                | Self::AuthSetUsername
                | Self::AuthListUsers
                | Self::AuthSetFlags
        ) {
            return Ok(());
        }
        if !state.signed_in {
            return Err(BackendError::auth_required());
        }
        if !state.allowed {
            return Err(BackendError::permission_denied());
        }
        Ok(())
    }
}
