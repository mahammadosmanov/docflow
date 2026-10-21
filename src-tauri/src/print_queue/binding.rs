use super::*;

impl PrintService {
    pub(super) fn resolve(
        &self,
        plan_id: &str,
        caller: &CallerIdentity,
    ) -> Result<crate::plan_store::ClaimedPlan, BackendError> {
        let claimed = self.plans.resolve_bound(plan_id, caller)?;
        if claimed.origin != BackendOrigin::Rust || claimed.plan.op_id != "files.batch_print" {
            return Err(BackendError::validation(
                "plan is not owned by the Rust print backend",
            ));
        }
        Ok(claimed)
    }

    pub(super) fn prepare_resume_file_binding<'a>(
        &'a self,
        caller: &CallerIdentity,
        state: &QueueState,
        plan: &Plan,
    ) -> Result<(PlanBinding, ProvisionalPathGrants<'a>), BackendError> {
        let (binding, provisional_grants) = self.issue_resume_file_binding(caller, state)?;
        validate_verified_resume_plan_sources(plan, provisional_grants.verified_sources())?;
        self.revalidate_worksheet_plan(&binding, caller, plan)?;
        Ok((binding, provisional_grants))
    }

    pub(super) fn issue_resume_file_binding<'a>(
        &'a self,
        caller: &CallerIdentity,
        state: &QueueState,
    ) -> Result<(PlanBinding, ProvisionalPathGrants<'a>), BackendError> {
        let queue_paths = state
            .jobs
            .iter()
            .map(|job| job.path.clone())
            .collect::<HashSet<_>>();
        let source_paths = state
            .source_identities
            .iter()
            .map(|source| source.path.clone())
            .collect::<HashSet<_>>();
        if queue_paths.is_empty()
            || queue_paths != source_paths
            || source_paths.len() != state.source_identities.len()
        {
            return Err(persisted_source_reselection_required());
        }
        let canonical_parents = state
            .source_identities
            .iter()
            .map(|source| {
                Path::new(&source.canonical_path)
                    .parent()
                    .map(|parent| parent.to_string_lossy().into_owned())
                    .ok_or_else(persisted_source_reselection_required)
            })
            .collect::<Result<HashSet<_>, _>>()?;
        let persisted_roots = state
            .grant_root_identities
            .iter()
            .map(|root| root.root.clone())
            .collect::<HashSet<_>>();
        if canonical_parents != persisted_roots
            || persisted_roots.len() != state.grant_root_identities.len()
        {
            return Err(persisted_source_reselection_required());
        }
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            Uuid::new_v4().to_string(),
            "files.batch_print",
        );
        let expected_sources = state
            .source_identities
            .iter()
            .map(|source| {
                (
                    PathBuf::from(&source.path),
                    PathBuf::from(&source.canonical_path),
                    source.identity.clone(),
                )
            })
            .collect::<Vec<_>>();
        let provisional_grants = self.grants.issue_persisted_read_capabilities(
            &scope,
            &state.grant_root_identities,
            &expected_sources,
        )?;
        let grant_ids = provisional_grants
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        Ok((
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                "files.batch_print",
                grant_ids,
            ),
            provisional_grants,
        ))
    }

    pub(super) fn issue_file_binding(
        &self,
        caller: &CallerIdentity,
        candidate_paths: &[PathBuf],
    ) -> Result<PlanBinding, BackendError> {
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            Uuid::new_v4().to_string(),
            "files.batch_print",
        );
        let mut parents = Vec::new();
        let mut seen = HashSet::new();
        for path in candidate_paths {
            let Some(parent) = path.parent() else {
                return Err(BackendError::path_invalid(
                    "print file has no parent directory",
                ));
            };
            let canonical = fs::canonicalize(parent)
                .map_err(|_| BackendError::path_invalid("print file directory is unavailable"))?;
            if seen.insert(canonical.clone()) {
                parents.push(canonical);
            }
        }
        let grant_ids = parents
            .iter()
            .map(|parent| self.grants.issue(&scope, parent, GrantAccess::Read))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|grant| grant.grant_id)
            .collect::<Vec<_>>();
        Ok(PlanBinding::new(
            &caller.owner_id,
            &caller.session_id,
            "files.batch_print",
            grant_ids,
        ))
    }

    pub(super) fn state_for_plan(
        &self,
        plan: &Plan,
        jobs: &[PrintJob],
        caller: &CallerIdentity,
        grant_root_identities: &[GrantRootIdentity],
        source_identities: &[QueueSourceIdentity],
    ) -> Result<QueueState, BackendError> {
        let queue_id = plan
            .meta
            .get("queueId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let resume_queue_id = plan
            .meta
            .get("resumeQueueId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !resume_queue_id.is_empty() {
            let mut state = self.queues.load(resume_queue_id)?;
            state.authorize_session(caller)?;
            let rows = plan
                .meta
                .get("editor")
                .and_then(Value::as_object)
                .and_then(|editor| editor.get("rows"))
                .and_then(Value::as_array)
                .ok_or_else(|| BackendError::validation("print plan has no rows"))?;
            let mut saved_by_id = HashMap::new();
            for saved in state.jobs.drain(..) {
                if saved_by_id.insert(saved.job_id.clone(), saved).is_some() {
                    return Err(BackendError::validation(
                        "saved print job identity is duplicated",
                    ));
                }
            }
            let mut planned_by_id = HashMap::new();
            for planned in jobs {
                if planned_by_id
                    .insert(planned.job_id.clone(), planned)
                    .is_some()
                {
                    return Err(BackendError::validation(
                        "planned print job identity is duplicated",
                    ));
                }
            }
            let mut reordered = Vec::with_capacity(rows.len());
            let mut displayed = HashSet::new();
            for row in rows {
                let job_id = row
                    .get("_resumeJobId")
                    .and_then(Value::as_str)
                    .filter(|job_id| !job_id.is_empty())
                    .ok_or_else(|| {
                        BackendError::validation("saved print job identity is missing")
                    })?;
                if !displayed.insert(job_id.to_string()) {
                    return Err(BackendError::validation(
                        "saved print job identity is duplicated",
                    ));
                }
                let saved = saved_by_id
                    .remove(job_id)
                    .ok_or_else(|| BackendError::validation("saved print job no longer exists"))?;
                let enabled = row.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                if enabled {
                    if !matches!(
                        saved.status.as_deref(),
                        Some("pending" | "failed" | "cancelled" | "uncertain")
                    ) {
                        return Err(BackendError::validation("saved print job is not resumable"));
                    }
                    let planned = planned_by_id.remove(job_id).ok_or_else(|| {
                        BackendError::validation("enabled saved print job is missing from the plan")
                    })?;
                    let mut next = planned.clone();
                    next.status = Some("pending".to_string());
                    next.attempts = saved.attempts;
                    next.error = Some(String::new());
                    next.completed_at = None;
                    next.cleanup_review = saved.cleanup_review.clone();
                    reordered.push(next);
                } else {
                    if planned_by_id.contains_key(job_id) {
                        return Err(BackendError::validation(
                            "disabled saved print job is present in the plan",
                        ));
                    }
                    reordered.push(saved);
                }
            }
            if !saved_by_id.is_empty() || !planned_by_id.is_empty() {
                return Err(BackendError::validation("saved print job set changed"));
            }
            state.jobs = reordered;
            state.status = "ready".to_string();
            state.current_job_id = None;
            return Ok(state);
        }
        let created = utc_now();
        Ok(QueueState {
            version: 1,
            queue_id,
            owner_id: caller.owner_id.clone(),
            session_id: caller.session_id.clone(),
            created_at: created.clone(),
            updated_at: created,
            status: "ready".to_string(),
            current_job_id: None,
            jobs: jobs
                .iter()
                .cloned()
                .map(|mut job| {
                    job.status = Some("pending".to_string());
                    job.attempts = Some(0);
                    job.error = Some(String::new());
                    job
                })
                .collect(),
            grant_root_identities: grant_root_identities.to_vec(),
            source_identities: source_identities.to_vec(),
            extra: Map::new(),
        })
    }
}
