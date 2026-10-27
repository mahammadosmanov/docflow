use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

use uuid::Uuid;

use crate::{
    cap_fs::{AuthorizedPath, CapabilityRoot, DirectoryAccess},
    errors::BackendError,
    journal::{
        FilesystemObjectIdentity, GrantRootAccessRecord, GrantRootIdentity, PersistedRootAccess,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallerIdentity {
    pub owner_id: String,
    pub session_id: String,
}

impl CallerIdentity {
    pub fn new(owner_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            owner_id: owner_id.into(),
            session_id: session_id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrantScope {
    pub caller: CallerIdentity,
    pub request_id: String,
    pub operation_id: String,
}

impl GrantScope {
    pub fn new(
        owner_id: impl Into<String>,
        session_id: impl Into<String>,
        request_id: impl Into<String>,
        operation_id: impl Into<String>,
    ) -> Self {
        Self {
            caller: CallerIdentity::new(owner_id, session_id),
            request_id: request_id.into(),
            operation_id: operation_id.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrantAccess {
    Read,
    Write,
    ReadWrite,
}

impl GrantAccess {
    pub(crate) fn allows(self, intent: PathIntent) -> bool {
        matches!(
            (self, intent),
            (Self::ReadWrite, _)
                | (Self::Read, PathIntent::Read)
                | (Self::Write, PathIntent::Write)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathIntent {
    Read,
    Write,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathGrant {
    pub grant_id: String,
    pub root: PathBuf,
    pub access: GrantAccess,
    pub scope: GrantScope,
    pub created_at: SystemTime,
}

pub struct PathGrantStore {
    ttl: Duration,
    grants: Mutex<HashMap<String, StoredPathGrant>>,
}

pub(crate) struct ValidatedGrantRootRoster {
    records: Vec<GrantRootAccessRecord>,
    roots: Vec<CapabilityRoot>,
    grant_ids: Vec<String>,
}

impl ValidatedGrantRootRoster {
    pub(crate) fn records(&self) -> &[GrantRootAccessRecord] {
        &self.records
    }

    pub(crate) fn roots(&self) -> &[CapabilityRoot] {
        &self.roots
    }

    pub(crate) fn matches_grants(&self, grant_ids: &[String]) -> bool {
        self.grant_ids == grant_ids
    }
}

#[derive(Clone)]
struct StoredPathGrant {
    grant: PathGrant,
    capability: CapabilityRoot,
    expected_read_identities: HashMap<PathBuf, FilesystemObjectIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedPersistedSource {
    pub(crate) path: PathBuf,
}

#[must_use = "provisional path grants are revoked unless committed"]
pub(crate) struct ProvisionalPathGrants<'a> {
    store: &'a PathGrantStore,
    grants: Vec<PathGrant>,
    verified_sources: Vec<VerifiedPersistedSource>,
    committed: bool,
}

impl ProvisionalPathGrants<'_> {
    pub(crate) fn grants(&self) -> &[PathGrant] {
        &self.grants
    }

    pub(crate) fn verified_sources(&self) -> &[VerifiedPersistedSource] {
        &self.verified_sources
    }

    pub(crate) fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for ProvisionalPathGrants<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.store.revoke_grant_ids(&self.grants);
        }
    }
}

mod authorization;
mod issuing;
mod revocation;

fn lock_grants(
    grants: &Mutex<HashMap<String, StoredPathGrant>>,
) -> MutexGuard<'_, HashMap<String, StoredPathGrant>> {
    grants
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

mod filesystem;
pub(crate) use filesystem::expired;
use filesystem::*;

#[cfg(test)]
mod lexical_guard_tests;
#[cfg(test)]
mod provisional_tests;
