use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

use uuid::Uuid;

use crate::{
    cap_fs::{AuthorizedPath, CapabilityRoot},
    errors::BackendError,
    journal::GrantRootIdentity,
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
    fn allows(self, intent: PathIntent) -> bool {
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

#[derive(Clone)]
struct StoredPathGrant {
    grant: PathGrant,
    capability: CapabilityRoot,
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
        let capability = CapabilityRoot::open(&root)
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
            },
        );
        Ok(grant)
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
        let grant = {
            let mut grants = lock_grants(&self.grants);
            if grants
                .get(grant_id)
                .is_some_and(|stored| expired(stored.grant.created_at, now, self.ttl))
            {
                grants.remove(grant_id);
            }
            grants
                .get(grant_id)
                .map(|stored| stored.grant.clone())
                .ok_or_else(BackendError::path_grant_unavailable)?
        };
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

    pub fn revoke(&self, caller: &CallerIdentity) {
        lock_grants(&self.grants).retain(|_, stored| stored.grant.scope.caller != *caller);
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
                stored.capability.authorize_directory(candidate)
            } else {
                stored.capability.authorize(candidate)
            };
            match authorized {
                Ok(path) => return Ok(path),
                Err(_) => last_error = Some(BackendError::path_escape()),
            }
        }
        Err(last_error.unwrap_or_else(BackendError::path_grant_unavailable))
    }
}

fn reject_lexical_escape(path: &Path) -> Result<(), BackendError> {
    if !path.is_absolute() {
        return Err(BackendError::path_invalid("path must be absolute"));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(BackendError::path_traversal());
    }
    Ok(())
}

fn resolve_existing_ancestor(path: &Path) -> Result<PathBuf, BackendError> {
    let mut probe = path.to_path_buf();
    let mut suffix = Vec::<OsString>::new();
    loop {
        match fs::symlink_metadata(&probe) {
            Ok(_) => {
                let mut resolved = fs::canonicalize(&probe)
                    .map_err(|_| BackendError::path_invalid("path cannot be canonicalized"))?;
                for component in suffix.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = probe
                    .file_name()
                    .ok_or_else(|| BackendError::path_invalid("path has no existing ancestor"))?;
                suffix.push(name.to_os_string());
                if !probe.pop() {
                    return Err(BackendError::path_invalid("path has no existing ancestor"));
                }
            }
            Err(_) => {
                return Err(BackendError::path_invalid(
                    "path metadata could not be inspected",
                ));
            }
        }
    }
}

pub(crate) fn revalidate_under_root(
    root: &Path,
    candidate: &Path,
    intent: PathIntent,
) -> Result<(), BackendError> {
    if !candidate.starts_with(root) {
        return Err(BackendError::path_escape());
    }
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| BackendError::path_escape())?;
    let mut current = root.to_path_buf();
    inspect_ancestor(&current, root, true)?;
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if is_link_or_reparse(&metadata) {
                    return Err(BackendError::path_escape());
                }
                let canonical = fs::canonicalize(&current)
                    .map_err(|_| BackendError::path_invalid("path cannot be canonicalized"))?;
                if !canonical.starts_with(root) {
                    return Err(BackendError::path_escape());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => {
                return Err(BackendError::path_invalid(
                    "path metadata could not be inspected",
                ));
            }
        }
    }
    if intent == PathIntent::Read && !candidate.exists() {
        return Err(BackendError::path_invalid("source path does not exist"));
    }
    Ok(())
}

fn inspect_ancestor(
    current: &Path,
    root: &Path,
    must_be_directory: bool,
) -> Result<(), BackendError> {
    let metadata = fs::symlink_metadata(current)
        .map_err(|_| BackendError::path_invalid("authorized root is unavailable"))?;
    if is_link_or_reparse(&metadata) || must_be_directory && !metadata.is_dir() {
        return Err(BackendError::path_escape());
    }
    let canonical = fs::canonicalize(current)
        .map_err(|_| BackendError::path_invalid("authorized root cannot be canonicalized"))?;
    if canonical != root {
        return Err(BackendError::path_escape());
    }
    Ok(())
}

#[cfg(windows)]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn expired(created_at: SystemTime, now: SystemTime, ttl: Duration) -> bool {
    now.duration_since(created_at).is_ok_and(|age| age >= ttl)
}

fn lock_grants(
    grants: &Mutex<HashMap<String, StoredPathGrant>>,
) -> MutexGuard<'_, HashMap<String, StoredPathGrant>> {
    grants
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
