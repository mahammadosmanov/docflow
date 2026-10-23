use super::*;
use crate::watch::{
    self, Decision, ItemStatus, PlanSummary, SETTINGS_SECTION, SETTLE_MS, WatchError,
    WatchSettings, take_snapshot,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WatchUpdate {
    op_id: String,
    enabled: Option<bool>,
    action: Option<WatchAction>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum WatchAction {
    CheckNow,
    Dismiss,
    Reprepare,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WatchOpen {
    op_id: String,
}

impl NativeBackend {
    pub(crate) fn watch_operator_guard(&self) -> watch::OperatorGuard {
        self.watch.operator_guard()
    }

    pub(crate) fn watch_forget_plan(&self, plan_id: &str) {
        self.watch.forget_plan(plan_id);
    }

    pub(crate) fn watch_poll_interval(&self) -> Duration {
        WatchSettings::from_section(self.settings.section(SETTINGS_SECTION)).poll
    }

    fn watch_status(&self) -> Value {
        let settings = WatchSettings::from_section(self.settings.section(SETTINGS_SECTION));
        json!({
            "operations": watch::watchable_ids(),
            "enabled": settings.enabled,
            "configured": watch::watchable_ids()
                .into_iter()
                .filter(|id| watch::roots_from_saved(id, &self.settings.params_for(id)).is_some())
                .collect::<Vec<_>>(),
            "pollSeconds": settings.poll.as_secs(),
            "paused": self.watch.operator_busy(),
            "items": self.watch.items(),
        })
    }

    pub(crate) fn watch_dispatch_status(&self, params: Value) -> Result<Value, BackendError> {
        decode_empty(params)?;
        self.watch.expire(now());
        Ok(self.watch_status())
    }

    pub(crate) fn watch_dispatch_update(&self, params: Value) -> Result<Value, BackendError> {
        let request: WatchUpdate = decode(params)?;
        if !watch::is_watchable(&request.op_id) {
            return Err(BackendError::validation("operation cannot be watched"));
        }
        if let Some(enabled) = request.enabled {
            let mut section = self.settings.section(SETTINGS_SECTION).unwrap_or_default();
            let mut ids = WatchSettings::from_section(Some(section.clone())).enabled;
            ids.retain(|id| *id != request.op_id);
            if enabled {
                ids.push(request.op_id.clone());
            }
            section.insert("enabled".to_string(), json!(ids));
            self.settings
                .save_section(SETTINGS_SECTION, Some(section))
                .map_err(|_| BackendError::settings_write())?;
            if !enabled {
                self.watch.dismiss(&request.op_id);
                self.watch.reset(&request.op_id);
            }
        }
        match request.action {
            Some(WatchAction::Dismiss) => self.watch.dismiss(&request.op_id),
            Some(WatchAction::Reprepare) => self.watch.reset(&request.op_id),
            Some(WatchAction::CheckNow) | None => {}
        }
        Ok(self.watch_status())
    }

    pub(crate) fn watch_dispatch_open(&self, params: Value) -> Result<Value, BackendError> {
        let request: WatchOpen = decode(params)?;
        self.watch.expire(now());
        let item = self
            .watch
            .item(&request.op_id)
            .filter(|item| item.status == ItemStatus::Ready)
            .ok_or_else(|| BackendError::validation("error.watch.notReady"))?;
        item.preview
            .ok_or_else(|| BackendError::validation("error.watch.notReady"))
    }

    /// Read-only background check. It prepares Previews only; nothing here
    /// applies, writes, moves, prints, or sends.
    pub(crate) fn watch_tick(&self) {
        let Some(_tick) = self.watch.try_begin_tick() else {
            return;
        };
        let settings = WatchSettings::from_section(self.settings.section(SETTINGS_SECTION));
        self.watch.expire(now());
        if settings.enabled.is_empty() || self.caller_identity().is_err() {
            return;
        }
        for operation_id in &settings.enabled {
            if self.watch.operator_busy() {
                return;
            }
            self.watch_check_operation(operation_id);
        }
    }

    fn watch_check_operation(&self, operation_id: &str) {
        let saved = self.settings.params_for(operation_id);
        let Some(roots) = watch::roots_from_saved(operation_id, &saved) else {
            return;
        };
        let snapshot = match take_snapshot(&roots, watch::MAX_ENTRIES) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.watch_record_failure(operation_id, &error);
                return;
            }
        };
        self.watch.mark_drift(operation_id, &snapshot.fingerprint);
        let state = self.watch.state(operation_id);
        let now_ms = now().saturating_mul(1000);
        if watch::decide(&state, &snapshot, now_ms, SETTLE_MS) != Decision::Prepare {
            return;
        }
        self.watch
            .begin_prepare(operation_id, &snapshot.fingerprint);
        let outcome = self
            .selected_params(operation_id)
            .and_then(|values| {
                self.watch_preview(
                    operation_id,
                    json!({"opId": operation_id, "values": values}),
                )
            })
            .and_then(|preview| {
                let plan: Plan = decode(preview.clone())?;
                Ok((preview, plan))
            });
        match outcome {
            Ok((preview, plan)) => self.watch.finish_prepared_with(
                operation_id,
                &snapshot.fingerprint,
                PlanSummary {
                    plan_id: plan.plan_id,
                    steps: plan.steps.len(),
                    warnings: plan.warnings.len(),
                },
                now(),
                Some(preview),
            ),
            Err(error) => {
                self.watch
                    .finish_failed(operation_id, &snapshot.fingerprint, error.code);
            }
        }
    }

    fn watch_record_failure(&self, operation_id: &str, error: &WatchError) {
        self.watch.begin_prepare(operation_id, "");
        self.watch.finish_failed(operation_id, "", error.code());
    }

    fn watch_preview(&self, operation_id: &str, params: Value) -> Result<Value, BackendError> {
        let progress = |_event: ProgressEvent| {};
        match operation_id {
            "rfi.process_scans_fast_covers" => self.preview_process_scans(params, progress),
            "rfi.group_files" => self.preview_rfi_file_group(params, progress),
            "rfi.stage_signed_first_pages" => self.preview_stage_signed_pages(params, progress),
            "rfi.complete_status_sync" => self.preview_rfi_complete_status_sync(params, progress),
            _ => Err(BackendError::validation("operation cannot be watched")),
        }
    }
}
