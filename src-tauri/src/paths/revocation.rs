use super::*;

impl PathGrantStore {
    pub fn revoke(&self, caller: &CallerIdentity) {
        lock_grants(&self.grants).retain(|_, stored| stored.grant.scope.caller != *caller);
    }

    pub(super) fn revoke_grant_ids(&self, grants: &[PathGrant]) {
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
