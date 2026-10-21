use super::*;

impl NativeBackend {
    pub(crate) fn register_selected_paths(
        &self,
        operation_id: &str,
        parameter_name: &str,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<PathSelection>, BackendError> {
        let mode = self.selection_mode(operation_id, parameter_name)?;
        if mode != PathSelectionMode::Files && paths.len() > 1 {
            return Err(BackendError::invalid_params());
        }
        let kind = if mode == PathSelectionMode::Folder {
            SelectionKind::Folder
        } else {
            SelectionKind::File
        };
        let caller = self.caller_identity()?;
        paths
            .into_iter()
            .map(|path| {
                self.selections
                    .issue(&caller, operation_id, parameter_name, &path, kind)
            })
            .collect()
    }

    pub(crate) fn reveal_path(&self, selection_id: &str) -> Result<PathBuf, BackendError> {
        self.selections.resolve_scope(
            selection_id,
            &self.caller_identity()?,
            SelectionScope::RevealReviewItem,
        )
    }

    pub(super) fn selected_params(
        &self,
        operation_id: &str,
    ) -> Result<Map<String, Value>, BackendError> {
        let operation = operation_registry(self.registry_facts())
            .into_iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
        let caller = self.caller_identity()?;
        let saved = self.settings.params_for(operation_id);

        let mut result = Map::new();
        for param in operation.schema.params {
            if !param.remember {
                continue;
            }
            let Some(raw) = saved.get(&param.name).cloned() else {
                continue;
            };
            if raw.is_null() {
                continue;
            }
            match param.kind {
                ParamKind::Secret | ParamKind::LongText => {
                    continue;
                }
                ParamKind::Folder | ParamKind::File => {
                    let selection_kind = if param.kind == ParamKind::Folder {
                        SelectionKind::Folder
                    } else {
                        SelectionKind::File
                    };
                    if let Some(selection) = raw
                        .as_str()
                        .filter(|p| Path::new(p).exists())
                        .and_then(|p| {
                            self.selections
                                .issue(
                                    &caller,
                                    operation_id,
                                    &param.name,
                                    Path::new(p),
                                    selection_kind,
                                )
                                .ok()
                        })
                    {
                        result.insert(param.name, json!(selection));
                    }
                }
                ParamKind::Files => {
                    if let Some(arr) = raw.as_array() {
                        let mut selections = Vec::new();
                        for item in arr {
                            if let Some(selection) = item
                                .as_str()
                                .filter(|p| Path::new(p).exists())
                                .and_then(|p| {
                                    self.selections
                                        .issue(
                                            &caller,
                                            operation_id,
                                            &param.name,
                                            Path::new(p),
                                            SelectionKind::File,
                                        )
                                        .ok()
                                })
                            {
                                selections.push(selection);
                            }
                        }
                        if !selections.is_empty() {
                            result.insert(param.name, json!(selections));
                        }
                    }
                }
                ParamKind::SheetCounts => {
                    if let Some(counts) = raw.as_object() {
                        let is_valid = counts
                            .values()
                            .all(|v| v.as_u64().is_some() || v.as_i64().is_some_and(|n| n >= 0));
                        if is_valid {
                            result.insert(param.name, Value::Object(counts.clone()));
                        }
                    }
                }
                ParamKind::Bool => {
                    if raw.is_boolean() {
                        result.insert(param.name, raw);
                    }
                }
                ParamKind::Choice => {
                    if let Some(_choice_str) = raw.as_str().filter(|c| {
                        param.choices.is_empty() || param.choices.iter().any(|choice| choice == *c)
                    }) {
                        result.insert(param.name, raw);
                    }
                }
                ParamKind::Int => {
                    if raw.is_number() {
                        result.insert(param.name, raw);
                    }
                }
                ParamKind::Text => {
                    if raw.is_string() {
                        result.insert(param.name, raw);
                    }
                }
            }
        }
        Ok(result)
    }

    pub(super) fn sanitize_and_materialize_save_params(
        &self,
        operation_id: &str,
        mut values: Map<String, Value>,
    ) -> Result<Map<String, Value>, BackendError> {
        let operation = operation_registry(self.registry_facts())
            .into_iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
        let caller = self.caller_identity()?;

        let mut sanitized = Map::new();
        for param in operation.schema.params {
            if !param.remember {
                continue;
            }
            let Some(raw) = values.remove(&param.name) else {
                continue;
            };
            if raw.is_null() {
                continue;
            }
            match param.kind {
                ParamKind::Secret | ParamKind::LongText => {
                    continue;
                }
                ParamKind::Folder | ParamKind::File => {
                    let selection_kind = if param.kind == ParamKind::Folder {
                        SelectionKind::Folder
                    } else {
                        SelectionKind::File
                    };
                    if let Ok(selection) = decode::<PathSelection>(raw.clone()) {
                        if let Ok(path) = self.selections.resolve(
                            &selection,
                            &caller,
                            operation_id,
                            &param.name,
                            selection_kind,
                        ) {
                            sanitized.insert(
                                param.name,
                                Value::String(path.to_string_lossy().into_owned()),
                            );
                        } else if let Some(previous_saved) = self
                            .settings
                            .params_for(operation_id)
                            .get(&param.name)
                            .and_then(Value::as_str)
                            .filter(|saved| {
                                *saved == selection.display_path && Path::new(saved).exists()
                            })
                        {
                            sanitized.insert(param.name, Value::String(previous_saved.to_string()));
                        }
                    } else if let Some(canonical) = raw
                        .as_str()
                        .filter(|p| Path::new(p).exists())
                        .and_then(|p| fs::canonicalize(Path::new(p)).ok())
                    {
                        sanitized.insert(
                            param.name,
                            Value::String(canonical.to_string_lossy().into_owned()),
                        );
                    }
                }
                ParamKind::Files => {
                    if let Ok(selections) = decode::<Vec<PathSelection>>(raw.clone()) {
                        let mut paths = Vec::new();
                        for selection in selections {
                            if let Ok(path) = self.selections.resolve(
                                &selection,
                                &caller,
                                operation_id,
                                &param.name,
                                SelectionKind::File,
                            ) {
                                paths.push(Value::String(path.to_string_lossy().into_owned()));
                            } else if let Some(_saved_array) = self
                                .settings
                                .params_for(operation_id)
                                .get(&param.name)
                                .and_then(Value::as_array)
                                .filter(|arr| {
                                    arr.iter().any(|item| {
                                        item.as_str().is_some_and(|p| p == selection.display_path)
                                    }) && Path::new(&selection.display_path).exists()
                                })
                            {
                                paths.push(Value::String(selection.display_path.clone()));
                            }
                        }
                        if !paths.is_empty() {
                            sanitized.insert(param.name, Value::Array(paths));
                        }
                    } else if let Some(arr) = raw.as_array() {
                        let mut paths = Vec::new();
                        for item in arr {
                            if let Some(canonical) = item
                                .as_str()
                                .filter(|p| Path::new(p).exists())
                                .and_then(|p| fs::canonicalize(Path::new(p)).ok())
                            {
                                paths.push(Value::String(canonical.to_string_lossy().into_owned()));
                            }
                        }
                        if !paths.is_empty() {
                            sanitized.insert(param.name, Value::Array(paths));
                        }
                    }
                }
                ParamKind::SheetCounts => {
                    if let Some(counts) = raw.as_object() {
                        let is_valid = counts
                            .values()
                            .all(|v| v.as_u64().is_some() || v.as_i64().is_some_and(|n| n >= 0));
                        if is_valid {
                            sanitized.insert(param.name, Value::Object(counts.clone()));
                        }
                    }
                }
                ParamKind::Bool => {
                    if raw.is_boolean() {
                        sanitized.insert(param.name, raw);
                    }
                }
                ParamKind::Choice => {
                    if let Some(_choice_str) = raw.as_str().filter(|c| {
                        param.choices.is_empty() || param.choices.iter().any(|choice| choice == *c)
                    }) {
                        sanitized.insert(param.name, raw);
                    }
                }
                ParamKind::Int => {
                    if raw.is_number() {
                        sanitized.insert(param.name, raw);
                    }
                }
                ParamKind::Text => {
                    if raw.is_string() {
                        sanitized.insert(param.name, raw);
                    }
                }
            }
        }
        Ok(sanitized)
    }

    pub(super) fn preset_operation(
        &self,
        operation_id: &str,
    ) -> Result<OperationDef, BackendError> {
        let operation = operation_registry(self.registry_facts())
            .into_iter()
            .find(|operation| operation.id == operation_id)
            .ok_or_else(|| BackendError::validation("unknown operation ID"))?;
        if !operation.supports_presets {
            return Err(BackendError::validation(
                "operation does not support presets",
            ));
        }
        Ok(operation)
    }

    pub(super) fn materialize_preview(&self, params: Value) -> Result<Value, BackendError> {
        let mut object = params
            .as_object()
            .cloned()
            .ok_or_else(BackendError::invalid_params)?;
        let operation_id = object
            .get("opId")
            .and_then(Value::as_str)
            .ok_or_else(BackendError::invalid_params)?
            .to_string();
        let values = object
            .remove("values")
            .and_then(|value| value.as_object().cloned())
            .ok_or_else(BackendError::invalid_params)?;
        object.insert(
            "values".to_string(),
            Value::Object(self.materialize_operation_values(&operation_id, values)?),
        );
        Ok(Value::Object(object))
    }

    pub(super) fn materialize_operation_values(
        &self,
        operation_id: &str,
        mut values: Map<String, Value>,
    ) -> Result<Map<String, Value>, BackendError> {
        let caller = self.caller_identity()?;
        for (name, kind) in operation_path_parameters(operation_id)? {
            let Some(raw) = values.get(&name).cloned() else {
                continue;
            };
            if raw.is_null() {
                continue;
            }
            let materialized = match kind {
                ParamKind::Folder | ParamKind::File => {
                    let selection: PathSelection = decode(raw)?;
                    let path = self.selections.resolve(
                        &selection,
                        &caller,
                        operation_id,
                        &name,
                        if kind == ParamKind::Folder {
                            SelectionKind::Folder
                        } else {
                            SelectionKind::File
                        },
                    )?;
                    Value::String(path.to_string_lossy().into_owned())
                }
                ParamKind::Files => {
                    let selections: Vec<PathSelection> = decode(raw)?;
                    Value::Array(
                        selections
                            .into_iter()
                            .map(|selection| {
                                self.selections
                                    .resolve(
                                        &selection,
                                        &caller,
                                        operation_id,
                                        &name,
                                        SelectionKind::File,
                                    )
                                    .map(|path| Value::String(path.to_string_lossy().into_owned()))
                            })
                            .collect::<Result<Vec<_>, BackendError>>()?,
                    )
                }
                _ => continue,
            };
            values.insert(name, materialized);
        }
        Ok(values)
    }

    pub(super) fn recovery(&self) -> Result<RecoveryPayload, BackendError> {
        let caller = self.caller_identity()?;
        Ok(RecoveryPayload {
            journals: self.filesystem.recover_pending(&caller)?,
            print_queue: self.print.latest(&caller)?,
        })
    }
}
