use super::*;

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
}
