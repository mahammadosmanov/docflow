use super::*;

impl PathGrantStore {
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
}
