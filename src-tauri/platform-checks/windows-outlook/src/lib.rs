#![cfg(not(test))]
#![allow(dead_code)]

#[cfg(target_os = "windows")]
pub mod outlook_reply_draft {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct MailItemIdentity {
        pub entry_id: String,
        pub store_id: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SelectedMailItem {
        pub identity: MailItemIdentity,
        pub subject: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ReplyContent {
        pub plain_text: String,
        pub html: String,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DraftIdentity {
        pub entry_id: String,
        pub store_id: String,
        pub fingerprint: String,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum OutlookFailureKind {
        Unsupported,
        NoEffect,
        OutcomeUnknown,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct OutlookFailure {
        pub kind: OutlookFailureKind,
        pub key: &'static str,
    }

    impl OutlookFailure {
        pub const fn unsupported(key: &'static str) -> Self {
            Self {
                kind: OutlookFailureKind::Unsupported,
                key,
            }
        }

        pub const fn no_effect(key: &'static str) -> Self {
            Self {
                kind: OutlookFailureKind::NoEffect,
                key,
            }
        }

        pub const fn outcome_unknown(key: &'static str) -> Self {
            Self {
                kind: OutlookFailureKind::OutcomeUnknown,
                key,
            }
        }
    }

    pub trait OutlookDraft {
        fn prepend_body(&mut self, content: &ReplyContent) -> Result<(), OutlookFailure>;
        fn display(&mut self) -> Result<(), OutlookFailure>;
        fn durable_identity(&mut self) -> Option<DraftIdentity>;
    }

    pub trait OutlookAdapter: Send + Sync {
        fn selected_mail_item(&self) -> Result<SelectedMailItem, OutlookFailure>;
        fn reply_all(
            &self,
            expected: &MailItemIdentity,
        ) -> Result<Box<dyn OutlookDraft>, OutlookFailure>;
        fn delete_unchanged_draft(&self, draft: &DraftIdentity) -> Result<(), OutlookFailure>;
    }
}

#[cfg(target_os = "windows")]
#[path = "../../../src/platform/windows/outlook.rs"]
pub mod outlook;
