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

impl PathGrantStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            grants: Mutex::new(HashMap::new()),
        }
    }

    pub fn issue(
        &self,
        scope: &GrantScope,
        selected_root: &Path,
        access: GrantAccess,
    ) -> Result<PathGrant, BackendError> {
        reject_lexical_escape(selected_root)?;
        let root = fs::canonicalize(selected_root)
            .map_err(|_| BackendError::path_invalid("selected path does not exist"))?;
        if !root.is_dir() {
            return Err(BackendError::path_invalid(
                "selected path grant root is not a directory",
            ));
        }
        let directory_access = match access {
            GrantAccess::Read => DirectoryAccess::ReadOnly,
            GrantAccess::Write | GrantAccess::ReadWrite => DirectoryAccess::ReadWrite,
        };
        let capability = CapabilityRoot::open_with_access(&root, directory_access)
            .map_err(|_| BackendError::path_invalid("selected path cannot be opened safely"))?;
        let now = SystemTime::now();
        let mut grants = lock_grants(&self.grants);
        grants.retain(|_, stored| !expired(stored.grant.created_at, now, self.ttl));
        let grant_id = loop {
            let candidate = Uuid::new_v4().to_string();
            if !grants.contains_key(&candidate) {
                break candidate;
            }
        };
        let grant = PathGrant {
            grant_id: grant_id.clone(),
            root,
            access,
            scope: scope.clone(),
            created_at: now,
        };
        grants.insert(
            grant_id,
            StoredPathGrant {
                grant: grant.clone(),
                capability,
                expected_read_identities: HashMap::new(),
            },
        );
        Ok(grant)
    }

    pub(crate) fn issue_provisional<'a>(
        &'a self,
        scope: &GrantScope,
        requests: &[(PathBuf, GrantAccess)],
    ) -> Result<ProvisionalPathGrants<'a>, BackendError> {
        if requests.is_empty() || requests.len() > 8 {
            return Err(BackendError::validation(
                "provisional path grant count is invalid",
            ));
        }
        let mut issued = Vec::with_capacity(requests.len());
        for (root, access) in requests {
            match self.issue(scope, root, *access) {
                Ok(grant) => issued.push(grant),
                Err(error) => {
                    self.revoke_grant_ids(&issued);
                    return Err(error);
                }
            }
        }
        Ok(ProvisionalPathGrants {
            store: self,
            grants: issued,
            verified_sources: Vec::new(),
            committed: false,
        })
    }

    pub(crate) fn issue_persisted_read_capabilities<'a>(
        &'a self,
        scope: &GrantScope,
        expected_roots: &[GrantRootIdentity],
        expected_sources: &[(PathBuf, PathBuf, FilesystemObjectIdentity)],
    ) -> Result<ProvisionalPathGrants<'a>, BackendError> {
        if expected_roots.is_empty() || expected_sources.is_empty() {
            return Err(persisted_source_reselection_required());
        }
        let mut prepared = Vec::with_capacity(expected_roots.len());
        let mut seen_roots = HashSet::new();
        for expected in expected_roots {
            let root = PathBuf::from(&expected.root);
            reject_lexical_escape(&root)?;
            let canonical =
                fs::canonicalize(&root).map_err(|_| persisted_source_reselection_required())?;
            if canonical != root || !seen_roots.insert(canonical.clone()) || !canonical.is_dir() {
                return Err(persisted_source_reselection_required());
            }
            let capability =
                CapabilityRoot::open_with_access(&canonical, DirectoryAccess::ReadOnly)
                    .map_err(|_| persisted_source_reselection_required())?;
            if capability
                .identity()
                .map_err(|_| persisted_source_reselection_required())?
                != expected.identity
            {
                return Err(persisted_source_reselection_required());
            }
            prepared.push((canonical, capability, HashMap::new()));
        }

        let mut seen_sources = HashSet::new();
        let mut verified_sources = Vec::with_capacity(expected_sources.len());
        for (source, expected_canonical, expected_identity) in expected_sources {
            reject_lexical_escape(source)
                .and_then(|()| reject_lexical_escape(expected_canonical))
                .map_err(|_| persisted_source_reselection_required())?;
            if !seen_sources.insert(source.clone()) {
                return Err(persisted_source_reselection_required());
            }
            let resolved_source =
                fs::canonicalize(source).map_err(|_| persisted_source_reselection_required())?;
            if resolved_source != *expected_canonical {
                return Err(persisted_source_reselection_required());
            }
            let matched_index = prepared
                .iter()
                .enumerate()
                .filter(|(_, (root, _, _))| expected_canonical.starts_with(root))
                .max_by_key(|(_, (root, _, _))| root.components().count())
                .map(|(index, _)| index)
                .ok_or_else(persisted_source_reselection_required)?;
            let (_, capability, expected_read_identities) = &mut prepared[matched_index];
            let anchored = capability
                .authorize(&resolved_source)
                .and_then(|authorized| authorized.anchor())
                .map_err(|_| persisted_source_reselection_required())?;
            if anchored
                .identity()
                .map_err(|_| persisted_source_reselection_required())?
                != *expected_identity
            {
                return Err(persisted_source_reselection_required());
            }
            if expected_read_identities
                .insert(expected_canonical.clone(), expected_identity.clone())
                .is_some()
            {
                return Err(persisted_source_reselection_required());
            }
            verified_sources.push(VerifiedPersistedSource {
                path: source.clone(),
            });
        }

        let now = SystemTime::now();
        let mut grants = lock_grants(&self.grants);
        grants.retain(|_, stored| !expired(stored.grant.created_at, now, self.ttl));
        let mut issued = Vec::with_capacity(prepared.len());
        for (root, capability, expected_read_identities) in prepared {
            let grant_id = loop {
                let candidate = Uuid::new_v4().to_string();
                if !grants.contains_key(&candidate) {
                    break candidate;
                }
            };
            let grant = PathGrant {
                grant_id: grant_id.clone(),
                root,
                access: GrantAccess::Read,
                scope: scope.clone(),
                created_at: now,
            };
            grants.insert(
                grant_id,
                StoredPathGrant {
                    grant: grant.clone(),
                    capability,
                    expected_read_identities,
                },
            );
            issued.push(grant);
        }
        Ok(ProvisionalPathGrants {
            store: self,
            grants: issued,
            verified_sources,
            committed: false,
        })
    }

    pub fn authorize(
        &self,
        grant_id: &str,
        scope: &GrantScope,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<PathBuf, BackendError> {
        reject_lexical_escape(candidate)?;
        let now = SystemTime::now();
        let stored = {
            let mut grants = lock_grants(&self.grants);
            if grants
                .get(grant_id)
                .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
            {
                grants.remove(grant_id);
            }
            grants
                .get(grant_id)
                .cloned()
                .ok_or_else(BackendError::path_grant_unavailable)?
        };
        let grant = &stored.grant;
        if grant.scope != *scope {
            return Err(BackendError::path_grant_scope_mismatch());
        }
        if !grant.access.allows(intent) {
            return Err(BackendError::path_access_denied());
        }
        let resolved = resolve_existing_ancestor(candidate)?;
        if !resolved.starts_with(&grant.root) {
            return Err(BackendError::path_escape());
        }
        if intent == PathIntent::Read && !resolved.exists() {
            return Err(BackendError::path_invalid("source path does not exist"));
        }
        if !stored.expected_read_identities.is_empty() {
            let expected_identity = stored
                .expected_read_identities
                .get(&resolved)
                .ok_or_else(persisted_source_reselection_required)?;
            stored
                .capability
                .authorize(&resolved)
                .map(|authorized| authorized.with_expected_identity(expected_identity.clone()))
                .and_then(|authorized| authorized.anchor())
                .and_then(|anchored| anchored.open_read())
                .map_err(|_| persisted_source_reselection_required())?;
        }
        Ok(resolved)
    }

    pub fn authorize_any(
        &self,
        grant_ids: &[String],
        scope: &GrantScope,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<PathBuf, BackendError> {
        let mut last_error = None;
        for grant_id in grant_ids {
            match self.authorize(grant_id, scope, candidate, intent) {
                Ok(path) => return Ok(path),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(BackendError::path_grant_unavailable))
    }

    pub fn authorize_for_plan(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<PathBuf, BackendError> {
        let now = SystemTime::now();
        let mut last_error = None;
        for grant_id in grant_ids {
            let grant = {
                let mut grants = lock_grants(&self.grants);
                if grants
                    .get(grant_id)
                    .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
                {
                    grants.remove(grant_id);
                }
                grants.get(grant_id).map(|stored| stored.grant.clone())
            };
            let Some(grant) = grant else {
                last_error = Some(BackendError::path_grant_unavailable());
                continue;
            };
            if grant.scope.caller != *caller || grant.scope.operation_id != operation_id {
                last_error = Some(BackendError::path_grant_scope_mismatch());
                continue;
            }
            match self.authorize(grant_id, &grant.scope, candidate, intent) {
                Ok(path) => match revalidate_under_root(&grant.root, &path, intent) {
                    Ok(()) => return Ok(path),
                    Err(error) => last_error = Some(error),
                },
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(BackendError::path_grant_unavailable))
    }

    pub fn revalidate_for_plan(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<PathBuf, BackendError> {
        reject_lexical_escape(candidate)?;
        let now = SystemTime::now();
        let mut last_error = None;
        for grant_id in grant_ids {
            let grant = {
                let mut grants = lock_grants(&self.grants);
                if grants
                    .get(grant_id)
                    .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
                {
                    grants.remove(grant_id);
                }
                grants.get(grant_id).map(|stored| stored.grant.clone())
            };
            let Some(grant) = grant else {
                last_error = Some(BackendError::path_grant_unavailable());
                continue;
            };
            if grant.scope.caller != *caller || grant.scope.operation_id != operation_id {
                last_error = Some(BackendError::path_grant_scope_mismatch());
                continue;
            }
            if !grant.access.allows(intent) {
                last_error = Some(BackendError::path_access_denied());
                continue;
            }
            match revalidate_under_root(&grant.root, candidate, intent) {
                Ok(()) => return Ok(candidate.to_path_buf()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(BackendError::path_grant_unavailable))
    }

    pub fn roots_for_plan(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
    ) -> Result<Vec<PathBuf>, BackendError> {
        let now = SystemTime::now();
        let mut roots = Vec::new();
        let mut grants = lock_grants(&self.grants);
        for grant_id in grant_ids {
            if grants
                .get(grant_id)
                .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
            {
                grants.remove(grant_id);
            }
            let grant = grants
                .get(grant_id)
                .ok_or_else(BackendError::path_grant_unavailable)?;
            if grant.grant.scope.caller != *caller || grant.grant.scope.operation_id != operation_id
            {
                return Err(BackendError::path_grant_scope_mismatch());
            }
            if !roots.contains(&grant.grant.root) {
                roots.push(grant.grant.root.clone());
            }
        }
        if roots.is_empty() {
            return Err(BackendError::path_grant_unavailable());
        }
        Ok(roots)
    }

    pub(crate) fn root_identities_for_plan(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
    ) -> Result<Vec<GrantRootIdentity>, BackendError> {
        let now = SystemTime::now();
        let mut roots = Vec::new();
        let mut grants = lock_grants(&self.grants);
        for grant_id in grant_ids {
            if grants
                .get(grant_id)
                .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
            {
                grants.remove(grant_id);
            }
            let stored = grants
                .get(grant_id)
                .ok_or_else(BackendError::path_grant_unavailable)?;
            if stored.grant.scope.caller != *caller
                || stored.grant.scope.operation_id != operation_id
            {
                return Err(BackendError::path_grant_scope_mismatch());
            }
            let root = stored.grant.root.to_string_lossy().into_owned();
            if roots
                .iter()
                .any(|known: &GrantRootIdentity| known.root == root)
            {
                continue;
            }
            roots.push(GrantRootIdentity {
                root,
                identity: stored.capability.identity().map_err(|_| {
                    BackendError::path_invalid("granted root identity is unavailable")
                })?,
            });
        }
        if roots.is_empty() {
            return Err(BackendError::path_grant_unavailable());
        }
        Ok(roots)
    }

    pub(crate) fn root_accesses_for_plan(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
    ) -> Result<ValidatedGrantRootRoster, BackendError> {
        if grant_ids.is_empty() || grant_ids.len() > 8 {
            return Err(BackendError::path_grant_unavailable());
        }
        let now = SystemTime::now();
        let mut seen_grants = HashSet::new();
        let mut entries = Vec::<(GrantRootAccessRecord, CapabilityRoot)>::new();
        let mut grants = lock_grants(&self.grants);
        for grant_id in grant_ids {
            if !seen_grants.insert(grant_id.clone()) {
                return Err(BackendError::path_grant_unavailable());
            }
            if grants
                .get(grant_id)
                .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
            {
                grants.remove(grant_id);
            }
            let stored = grants
                .get(grant_id)
                .ok_or_else(BackendError::path_grant_unavailable)?;
            if stored.grant.scope.caller != *caller
                || stored.grant.scope.operation_id != operation_id
            {
                return Err(BackendError::path_grant_scope_mismatch());
            }
            let access = match stored.grant.access {
                GrantAccess::Read => PersistedRootAccess::ReadOnly,
                GrantAccess::Write | GrantAccess::ReadWrite => PersistedRootAccess::ReadWrite,
            };
            let root = stored.grant.root.to_string_lossy().into_owned();
            let identity = stored
                .capability
                .identity()
                .map_err(|_| BackendError::path_invalid("granted root identity is unavailable"))?;
            if let Some((record, capability)) =
                entries.iter_mut().find(|(record, _)| record.root == root)
            {
                if record.identity != identity {
                    return Err(BackendError::path_invalid(
                        "granted root identity changed during admission",
                    ));
                }
                if access == PersistedRootAccess::ReadWrite {
                    record.access = PersistedRootAccess::ReadWrite;
                    *capability = stored.capability.clone();
                }
            } else {
                entries.push((
                    GrantRootAccessRecord {
                        root,
                        identity,
                        access,
                    },
                    stored.capability.clone(),
                ));
            }
        }
        let (records, roots): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
        Ok(ValidatedGrantRootRoster {
            records,
            roots,
            grant_ids: grant_ids.to_vec(),
        })
    }

    pub(crate) fn reopen_recorded_roots(
        &self,
        records: &[GrantRootAccessRecord],
        _caller: &CallerIdentity,
        _operation_id: &str,
    ) -> Result<ValidatedGrantRootRoster, BackendError> {
        if records.is_empty() || records.len() > 8 {
            return Err(BackendError::path_grant_unavailable());
        }
        let mut seen = HashSet::new();
        let mut roots = Vec::with_capacity(records.len());
        for record in records {
            let root = PathBuf::from(&record.root);
            reject_lexical_escape(&root)?;
            if !seen.insert(record.root.clone())
                || fs::canonicalize(&root).map_err(|_| BackendError::path_grant_unavailable())?
                    != root
            {
                return Err(BackendError::path_grant_unavailable());
            }
            let access = match record.access {
                PersistedRootAccess::ReadOnly => DirectoryAccess::ReadOnly,
                PersistedRootAccess::ReadWrite => DirectoryAccess::ReadWrite,
            };
            let capability = CapabilityRoot::open_with_access(&root, access)
                .map_err(|_| BackendError::path_grant_unavailable())?;
            if capability
                .identity()
                .map_err(|_| BackendError::path_grant_unavailable())?
                != record.identity
            {
                return Err(BackendError::path_grant_unavailable());
            }
            roots.push(capability);
        }
        Ok(ValidatedGrantRootRoster {
            records: records.to_vec(),
            roots,
            grant_ids: Vec::new(),
        })
    }

    pub fn revoke(&self, caller: &CallerIdentity) {
        lock_grants(&self.grants).retain(|_, stored| stored.grant.scope.caller != *caller);
    }

    fn revoke_grant_ids(&self, grants: &[PathGrant]) {
        let grant_ids = grants
            .iter()
            .map(|grant| grant.grant_id.as_str())
            .collect::<HashSet<_>>();
        lock_grants(&self.grants).retain(|grant_id, _| !grant_ids.contains(grant_id.as_str()));
    }

    #[cfg(debug_assertions)]
    pub(crate) fn active_grant_count_for_testing(&self) -> usize {
        lock_grants(&self.grants).len()
    }

    pub(crate) fn authorize_capability_for_plan(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<AuthorizedPath, BackendError> {
        self.authorize_capability_for_plan_inner(
            grant_ids,
            caller,
            operation_id,
            candidate,
            intent,
            false,
        )
    }

    pub(crate) fn authorize_directory_capability_for_plan(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<AuthorizedPath, BackendError> {
        self.authorize_capability_for_plan_inner(
            grant_ids,
            caller,
            operation_id,
            candidate,
            intent,
            true,
        )
    }

    pub(crate) fn authorize_capability_for_grant(
        &self,
        grant: &PathGrant,
        candidate: &Path,
        intent: PathIntent,
    ) -> Result<AuthorizedPath, BackendError> {
        reject_lexical_escape(candidate)?;
        let now = SystemTime::now();
        let stored = {
            let mut grants = lock_grants(&self.grants);
            if grants
                .get(&grant.grant_id)
                .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
            {
                grants.remove(&grant.grant_id);
            }
            grants
                .get(&grant.grant_id)
                .cloned()
                .ok_or_else(BackendError::path_grant_unavailable)?
        };
        if stored.grant != *grant {
            return Err(BackendError::path_grant_scope_mismatch());
        }
        if !grant.access.allows(intent) {
            return Err(BackendError::path_access_denied());
        }
        let resolved = resolve_existing_ancestor(candidate)?;
        stored
            .capability
            .authorize(&resolved)
            .map_err(|_| BackendError::path_escape())
    }

    fn authorize_capability_for_plan_inner(
        &self,
        grant_ids: &[String],
        caller: &CallerIdentity,
        operation_id: &str,
        candidate: &Path,
        intent: PathIntent,
        allow_root: bool,
    ) -> Result<AuthorizedPath, BackendError> {
        reject_lexical_escape(candidate)?;
        let resolved = resolve_existing_ancestor(candidate)?;
        let now = SystemTime::now();
        let mut last_error = None;
        for grant_id in grant_ids {
            let stored = {
                let mut grants = lock_grants(&self.grants);
                if grants
                    .get(grant_id)
                    .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
                {
                    grants.remove(grant_id);
                }
                grants.get(grant_id).cloned()
            };
            let Some(stored) = stored else {
                last_error = Some(BackendError::path_grant_unavailable());
                continue;
            };
            let grant = &stored.grant;
            if grant.scope.caller != *caller || grant.scope.operation_id != operation_id {
                last_error = Some(BackendError::path_grant_scope_mismatch());
                continue;
            }
            if !grant.access.allows(intent) {
                last_error = Some(BackendError::path_access_denied());
                continue;
            }
            let authorized = if allow_root {
                stored.capability.authorize_directory(&resolved)
            } else {
                stored.capability.authorize(&resolved)
            };
            match authorized {
                Ok(path) if stored.expected_read_identities.is_empty() => return Ok(path),
                Ok(path) => {
                    let Some(expected_identity) = stored.expected_read_identities.get(&resolved)
                    else {
                        last_error = Some(persisted_source_reselection_required());
                        continue;
                    };
                    return Ok(path.with_expected_identity(expected_identity.clone()));
                }
                Err(_) => last_error = Some(BackendError::path_escape()),
            }
        }
        Err(last_error.unwrap_or_else(BackendError::path_grant_unavailable))
    }
}

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
