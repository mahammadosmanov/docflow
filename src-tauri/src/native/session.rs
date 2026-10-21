use super::*;

impl NativeBackend {
    pub(crate) fn try_native_undo(&self, params: &Value) -> Result<Option<Value>, BackendError> {
        let object = params
            .as_object()
            .ok_or_else(BackendError::invalid_params)?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "journalId" | "cloudId" | "destination"))
        {
            return Err(BackendError::invalid_params());
        }
        let journal_id = object
            .get("journalId")
            .and_then(Value::as_str)
            .ok_or_else(BackendError::invalid_params)?;
        let operation_id = self.journals.inspect_operation(journal_id)?;
        if operation_id == outlook_reply_draft::OPERATION_ID && object.contains_key("destination") {
            return Err(BackendError::validation(
                "Outlook draft undo does not accept a destination",
            ));
        }
        if operation_id == outlook_reply_draft::OPERATION_ID {
            let cloud_id = object
                .get("cloudId")
                .and_then(Value::as_str)
                .map(str::to_string);
            let result = self.outlook.undo(journal_id, &self.caller_identity()?)?;
            if result.failed == 0
                && let Some(cloud_id) = cloud_id
            {
                self.mark_undone(&cloud_id);
            }
            return encode(result).map(Some);
        }
        if operation_id == rfi_follow_up::OPERATION_ID && object.contains_key("destination") {
            return Err(BackendError::validation(
                "RFI Follow Up undo does not accept a destination",
            ));
        }
        let caller = self.caller_identity()?;
        if operation_id == rfi_follow_up::OPERATION_ID {
            let cloud_id = object
                .get("cloudId")
                .and_then(Value::as_str)
                .map(str::to_string);
            let result = self.filesystem.undo(journal_id, &caller, None)?;
            if result.failed == 0
                && let Some(cloud_id) = cloud_id
            {
                self.mark_undone(&cloud_id);
            }
            return encode(result).map(Some);
        }
        let record = self.journals.read(journal_id)?;
        if record.steps.iter().any(|step| step.kind == StepKind::Print) {
            return Err(BackendError::validation(
                "print audit journals are not undoable",
            ));
        }
        if record.steps.iter().any(|step| {
            !matches!(
                step.kind,
                StepKind::Mkdir
                    | StepKind::Move
                    | StepKind::Copy
                    | StepKind::Rename
                    | StepKind::Write
                    | StepKind::RestoreBackup
            )
        }) {
            return Ok(None);
        }
        let destination = object
            .get("destination")
            .cloned()
            .map(|value| {
                let selection: PathSelection = decode(value)?;
                self.selections.resolve_scope(
                    &selection.selection_id,
                    &caller,
                    SelectionScope::UndoDestination,
                )
            })
            .transpose()?;
        let cloud_id = object
            .get("cloudId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let result = self
            .filesystem
            .undo(journal_id, &caller, destination.as_deref())?;
        if result.failed == 0
            && let Some(cloud_id) = cloud_id
        {
            self.mark_undone(&cloud_id);
        }
        encode(result).map(Some)
    }

    pub(super) fn caller_identity(&self) -> Result<CallerIdentity, BackendError> {
        let session_id = self
            .process_session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        authenticated_caller_identity(&self.auth.state(), &session_id)
    }

    pub(super) fn signed_in_caller_identity(&self) -> Result<CallerIdentity, BackendError> {
        let session_id = self
            .process_session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        signed_in_caller_identity(&self.auth.state(), &session_id)
    }

    pub(super) fn revoke_session_authority(&self, caller: &CallerIdentity) {
        self.filesystem.revoke(caller);
        self.selections.revoke(caller);
        self.print.revoke(caller);
        self.outlook.revoke(caller);
    }

    pub(super) fn rotate_session_authority(&self) {
        *self
            .process_session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = uuid::Uuid::new_v4().to_string();
    }

    pub(super) fn registry_facts(&self) -> RegistryFacts {
        self.print.inventory().map_or_else(
            |_| RegistryFacts::default(),
            |inventory| RegistryFacts {
                printers: inventory.printers,
                default_printer: inventory.default_printer,
                ..RegistryFacts::default()
            },
        )
    }

    pub(super) fn update_settings(&self, params: Value) -> Result<Value, BackendError> {
        let request: SettingsUpdate = decode(params)?;
        let mut preferences = self.settings.preferences();
        let previous_startup = preferences.start_with_windows;
        if let PatchField::Value(locale) = request.locale {
            preferences.locale = locale;
        }
        if let Some(theme) = request.theme {
            preferences.theme = theme;
        }
        if let Some(font) = request.font {
            preferences.font = font;
        }
        if let Some(font_scale) = request.font_scale {
            preferences.font_scale = font_scale;
        }
        if let Some(compact) = request.compact {
            preferences.compact = compact;
        }
        if let Some(start_with_windows) = request.start_with_windows {
            platform::set_start_with_windows(start_with_windows)
                .map_err(|_| BackendError::settings_write())?;
            preferences.start_with_windows = start_with_windows;
        }
        if let Some(replica_sync_enabled) = request.replica_sync_enabled {
            if replica_sync_enabled
                && self
                    .settings
                    .section("replicaAutomation")
                    .and_then(|section| section.get("profile").cloned())
                    .is_none()
            {
                return Err(BackendError::validation("error.replica.profileMissing"));
            }
            preferences.replica_sync_enabled = replica_sync_enabled;
        }
        if self.settings.save_preferences(preferences).is_err() {
            if request.start_with_windows.is_some() {
                let _ = platform::set_start_with_windows(previous_startup);
            }
            return Err(BackendError::settings_write());
        }
        encode(self.settings.preferences())
    }
}
