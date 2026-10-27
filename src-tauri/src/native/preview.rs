use super::*;

impl NativeBackend {
    pub(crate) fn preview_process_scans<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<ProcessScansValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != FAST_COVER_OPERATION_ID {
            return Err(BackendError::validation("unknown operation ID"));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let source_access = process_scans_source_access();
        let source_grant =
            self.filesystem
                .issue_grant(&scope, &request.values.source, source_access)?;
        let destination_grant =
            self.filesystem
                .issue_grant(&scope, &request.values.dest, GrantAccess::ReadWrite)?;
        let trash_grant = if request.values.keep_original {
            None
        } else {
            Some(self.filesystem.issue_grant(
                &scope,
                &self.app_data_dir.join("trash"),
                GrantAccess::ReadWrite,
            )?)
        };
        let scan_params = ProcessScansParams {
            source: request.values.source,
            destination: request.values.dest,
            scan_mode: request.values.scan_mode,
            organize_into_rfi_folders: request.values.organize_into_rfi_folders,
            keep_original: request.values.keep_original,
            correct_orientation: request.values.correct_orientation,
            trash_root: (!request.values.keep_original).then(|| self.app_data_dir.join("trash")),
        };
        let native_ocr = NativeOcr::discover();
        let ocr = CachingOcr::new(
            &native_ocr,
            &self.scan_index,
            &caller.owner_id,
            &scan_params.source,
        );
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let plan = preview_fast_covers(&scan_params, &ocr, &mut progress, || {
            cancellation.is_cancelled()
        })?;
        encode(
            self.filesystem.cache_plan(
                BackendOrigin::Rust,
                PlanBinding::new(
                    &caller.owner_id,
                    &caller.session_id,
                    &request.op_id,
                    [
                        Some(source_grant.grant_id),
                        Some(destination_grant.grant_id),
                        trash_grant.map(|grant| grant.grant_id),
                    ]
                    .into_iter()
                    .flatten()
                    .collect(),
                ),
                plan,
            )?,
        )
    }

    pub(crate) fn preview_rfi_file_group<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<RfiFileGroupValues> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != rfi_file_group::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI file-group backend",
            ));
        }
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let provisional = self.filesystem.issue_provisional_grants(
            &scope,
            &[
                (request.values.source.clone(), GrantAccess::Read),
                (request.values.destination.clone(), GrantAccess::ReadWrite),
            ],
        )?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let native_ocr = NativeOcr::discover();
        let ocr = CachingOcr::new(
            &native_ocr,
            &self.scan_index,
            &caller.owner_id,
            &request.values.source,
        );
        let plan = preview_rfi_file_group(
            &GroupRfiFilesParams {
                source: request.values.source,
                destination: request.values.destination,
            },
            &ocr,
            progress,
            || cancellation.is_cancelled(),
        )?;
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                grant_ids,
            ),
            plan,
        )?;
        provisional.commit();
        encode(cached)
    }

    pub(crate) fn preview_rfi_log<F>(
        &self,
        params: Value,
        progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<Map<String, Value>> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != "rfi.log_add" {
            return Err(BackendError::validation(
                "operation is not owned by the Rust RFI Log backend",
            ));
        }
        let workbook = request
            .values
            .get("workbook")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(BackendError::invalid_params)?;
        let root = workbook
            .parent()
            .ok_or_else(|| BackendError::path_invalid("workbook has no parent directory"))?;
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            &request.op_id,
        );
        let grant = self
            .filesystem
            .issue_grant(&scope, root, GrantAccess::ReadWrite)?;
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let plan = rfi_log::preview(
            &request.values,
            chrono::Local::now().date_naive(),
            Some(&self.gemini),
            progress,
            || cancellation.is_cancelled(),
        )?;
        encode(self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                &request.op_id,
                vec![grant.grant_id],
            ),
            plan,
        )?)
    }
    pub(crate) fn preview_concrete_documents<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        const OPERATION_ID: &str = "rfi.generate_concrete_documents";
        let request: PreviewEnvelope<Map<String, Value>> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the concrete document backend",
            ));
        }
        let get_text = |name: &str| {
            request
                .values
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
        };
        let rfis_010 = optional_requested_rfis(get_text("rfis_010"))?;
        let rfis_011 = optional_requested_rfis(get_text("rfis_011"))?;
        let rfis_013 = optional_requested_rfis(get_text("rfis_013"))?;
        let rfis_014 = optional_requested_rfis(get_text("rfis_014"))?;
        if rfis_010.is_empty() && rfis_011.is_empty() && rfis_013.is_empty() && rfis_014.is_empty()
        {
            return Err(concrete_documents_error(
                "Enter RFI numbers for at least one CON-CIV type",
            ));
        }
        let ambient_min = request
            .values
            .get("ambient_min")
            .and_then(Value::as_i64)
            .unwrap_or(19);
        let ambient_max = request
            .values
            .get("ambient_max")
            .and_then(Value::as_i64)
            .unwrap_or(24);
        if !(0..=100).contains(&ambient_min)
            || !(0..=100).contains(&ambient_max)
            || ambient_min > ambient_max
        {
            return Err(concrete_documents_error(
                "Ambient temperature range must be between 0 and 100 °C with minimum no greater than maximum",
            ));
        }
        let ambient_groups = parse_ambient_groups(get_text("ambient_groups"), &rfis_011)?;
        let rfi_log = required_selected_path(&request.values, "rfi_log_workbook")?;
        let output_directory = required_selected_path(&request.values, "output_directory")?;
        if !output_directory.is_dir() {
            return Err(concrete_documents_error("Output folder is unavailable"));
        }
        let mut selections = Vec::<(DocumentType, Vec<String>, PathBuf)>::new();
        for (document_type, numbers, parameter) in [
            (DocumentType::Civ010, rfis_010.clone(), "template_010"),
            (DocumentType::Civ011, rfis_011, "template_011"),
            (DocumentType::Civ013, rfis_013, "template_013"),
            (DocumentType::Civ014, rfis_014, "template_014"),
        ] {
            if numbers.is_empty() {
                continue;
            }
            let template = required_selected_path(&request.values, parameter).map_err(|_| {
                concrete_documents_error(
                    "Select an example template for every requested document type",
                )
            })?;
            if !template.is_file() {
                return Err(concrete_documents_error(
                    "A selected template file is unavailable",
                ));
            }
            selections.push((document_type, numbers, template));
        }
        let needs_follow_up = selections
            .iter()
            .any(|(kind, _, _)| *kind == DocumentType::Civ010);
        let follow_up = if needs_follow_up {
            optional_selected_path(&request.values, "follow_up_workbook")
        } else {
            None
        };
        if !rfi_log.is_file() {
            return Err(concrete_documents_error(
                "The selected RFI Log workbook is unavailable",
            ));
        }
        let mut requested = selections
            .iter()
            .flat_map(|(_, numbers, _)| numbers.iter().cloned())
            .collect::<Vec<_>>();
        requested.sort();
        requested.dedup();

        let mut roots = Vec::<(PathBuf, GrantAccess)>::new();
        for path in std::iter::once(&rfi_log)
            .chain(follow_up.iter().filter(|path| path.is_file()))
            .chain(selections.iter().map(|(_, _, path)| path))
        {
            let root = path
                .parent()
                .ok_or_else(|| BackendError::path_invalid("Selected input has no parent folder"))?
                .to_path_buf();
            add_grant_root(&mut roots, root, GrantAccess::Read);
        }
        add_grant_root(&mut roots, output_directory.clone(), GrantAccess::ReadWrite);
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            OPERATION_ID,
        );
        let provisional = self.filesystem.issue_provisional_grants(&scope, &roots)?;
        let read_selected = |path: &Path| -> Result<Vec<u8>, BackendError> {
            let grant = provisional
                .grants()
                .iter()
                .filter(|grant| {
                    path.starts_with(&grant.root) && grant.access.allows(PathIntent::Read)
                })
                .max_by_key(|grant| grant.root.components().count())
                .ok_or_else(|| {
                    BackendError::path_invalid("No read grant covers a selected input")
                })?;
            let anchor = self
                .filesystem
                .anchor_granted_path(grant, path, PathIntent::Read)?;
            let mut file = anchor.open_read().map_err(|error| {
                concrete_documents_error(&format!("Cannot read selected input: {error}"))
            })?;
            let length = file
                .metadata()
                .map_err(|_| concrete_documents_error("Cannot inspect selected input"))?
                .len();
            if length > crate::ooxml::MAX_PACKAGE_BYTES {
                return Err(concrete_documents_error(
                    "A selected Office package is larger than the supported limit",
                ));
            }
            let mut bytes = Vec::with_capacity(length as usize);
            file.read_to_end(&mut bytes)
                .map_err(|_| concrete_documents_error("Could not read a selected input"))?;
            Ok(bytes)
        };

        let log_bytes = read_selected(&rfi_log)?;
        let civil_records = read_civil_records(&mut Cursor::new(log_bytes), &requested)
            .map_err(|error| concrete_documents_error(&error))?;
        let rfis_requiring_follow_up = rfis_010
            .iter()
            .filter(|rfi| {
                civil_records
                    .get(*rfi)
                    .is_some_and(|record| has_con_civ_010_form(&record.forms))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !rfis_requiring_follow_up.is_empty() {
            let path = follow_up.as_ref().ok_or_else(|| {
                BackendError::validation("error.param.required.follow_up_workbook")
            })?;
            if !path.is_file() {
                return Err(concrete_documents_error(
                    "The Concrete Specimen Test Follow-Up workbook is unavailable",
                ));
            }
        }
        let mut warnings = rfis_010
            .iter()
            .filter(|rfi| !rfis_requiring_follow_up.contains(rfi))
            .map(|rfi| Warning {
                key: "warn.rfi.documents.formNotRequired".to_string(),
                subject: rfi.clone(),
            })
            .collect::<Vec<_>>();
        let follow_records = if let Some(path) = follow_up.as_ref() {
            if rfis_requiring_follow_up.is_empty() {
                None
            } else {
                let bytes = read_selected(path)?;
                Some(
                    read_follow_up_records(&mut Cursor::new(bytes), &rfis_requiring_follow_up)
                        .map_err(|error| concrete_documents_error(&error))?,
                )
            }
        } else {
            None
        };
        let ratios = parse_water_ratios(get_text("water_cement_ratios"))?;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let mut generated = Vec::new();
        for (document_type, numbers, template_path) in selections {
            let template_bytes = read_selected(&template_path)?;
            let mut records = Vec::with_capacity(numbers.len());
            for rfi in numbers {
                let civil = civil_records
                    .get(&rfi)
                    .ok_or_else(|| concrete_documents_error("RFI Log row is missing"))?;
                if document_type == DocumentType::Civ010 && !has_con_civ_010_form(&civil.forms) {
                    continue;
                }
                let location = document_location(document_type, &civil.area, &civil.tag);
                if location.is_empty() {
                    return Err(concrete_documents_error(&format!(
                        "The RFI Log has no usable location for {rfi}"
                    )));
                }
                let follow_rows = follow_records.as_ref().and_then(|rows| rows.get(&rfi));
                if document_type == DocumentType::Civ010 && follow_rows.is_none() {
                    warnings.push(Warning {
                        key: "warn.rfi.documents.followUpMissing".to_string(),
                        subject: rfi.clone(),
                    });
                    continue;
                }
                let follow = follow_rows.and_then(|rows| rows.first());
                let field = |key: &str| {
                    follow
                        .and_then(|row| row.get(key))
                        .map_or("", String::as_str)
                };
                let additional_batches = if document_type == DocumentType::Civ010 {
                    follow_rows
                        .into_iter()
                        .flatten()
                        .skip(1)
                        .map(|row| {
                            let value = |key: &str| {
                                row.get(key)
                                    .map(String::as_str)
                                    .filter(|value| !value.trim().is_empty())
                                    .unwrap_or_else(|| {
                                        if matches!(key, "location" | "concrete_class" | "supplier")
                                        {
                                            field(key)
                                        } else {
                                            ""
                                        }
                                    })
                                    .to_string()
                            };
                            ConcreteBatch {
                                tag: value("location"),
                                concrete_class: value("concrete_class"),
                                supplier: value("supplier"),
                                batch_ticket: value("batch_ticket"),
                                truck: value("truck"),
                                quantity: value("quantity"),
                                total_quantity: value("total_quantity"),
                                batch_time: excel_time_text(&value("batch_time")),
                                start_time: excel_time_text(&value("start_time")),
                                end_time: excel_time_text(&value("end_time")),
                                plant_slump: value("plant_slump"),
                                in_situ_slump: value("in_situ_slump"),
                                plant_temp: value("plant_temp"),
                                in_situ_temp: value("in_situ_temp"),
                                samples: value("samples"),
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                records.push(SourceRecord {
                    rfi: rfi.clone(),
                    location,
                    tag: if document_type == DocumentType::Civ010 {
                        field("location").to_string()
                    } else {
                        civil.tag.clone()
                    },
                    concrete_class: field("concrete_class").to_string(),
                    supplier: field("supplier").to_string(),
                    batch_ticket: field("batch_ticket").to_string(),
                    truck: field("truck").to_string(),
                    quantity: field("quantity").to_string(),
                    total_quantity: field("total_quantity").to_string(),
                    batch_time: excel_time_text(field("batch_time")),
                    pour_date: excel_date_text(field("pour_date")),
                    start_time: excel_time_text(field("start_time")),
                    end_time: excel_time_text(field("end_time")),
                    plant_slump: field("plant_slump").to_string(),
                    in_situ_slump: field("in_situ_slump").to_string(),
                    plant_temp: field("plant_temp").to_string(),
                    in_situ_temp: field("in_situ_temp").to_string(),
                    samples: field("samples").to_string(),
                    water_cement_ratio: ratios.get(&rfi).cloned(),
                    ambient_range: ambient_groups.get(&rfi).copied(),
                    drawing: field("drawing").to_string(),
                    additional_batches,
                });
            }
            if records.is_empty() {
                continue;
            }
            generated.extend(
                concrete_documents::generate(
                    document_type,
                    &template_bytes,
                    &records,
                    (ambient_min as u8, ambient_max as u8),
                    seed ^ (generated.len() as u64 + 1),
                )
                .map_err(|error| concrete_documents_error(&error))?,
            );
        }

        let generated_count = generated.len() as u64;
        let mut steps = Vec::new();
        let mut destinations = HashSet::new();
        for GeneratedFile { filename, bytes } in generated {
            if Path::new(&filename)
                .file_name()
                .and_then(|name| name.to_str())
                != Some(filename.as_str())
            {
                return Err(concrete_documents_error("Generated output name is invalid"));
            }
            let destination = output_directory.join(&filename);
            if !destinations.insert(destination.clone()) {
                return Err(concrete_documents_error(
                    "Two selected records produce the same output filename",
                ));
            }
            if destination.exists() {
                warnings.push(Warning {
                    key: "warn.rfi.documents.outputExists".to_string(),
                    subject: filename,
                });
                continue;
            }
            let mut meta = JsonObject::new();
            meta.insert(
                "contentBase64".to_string(),
                Value::String(base64::engine::general_purpose::STANDARD.encode(bytes)),
            );
            steps.push(Step {
                kind: StepKind::Write,
                src: None,
                dst: Some(destination.to_string_lossy().into_owned()),
                note_key: Some("step.rfi.generatedConcreteDocument".to_string()),
                meta,
            });
        }
        progress(ProgressEvent {
            phase: "concrete_document_generation".to_string(),
            done: steps.len() as u64,
            total: steps.len() as u64,
            subject: output_directory.to_string_lossy().into_owned(),
        });
        let plan = Plan {
            plan_id: String::new(),
            op_id: OPERATION_ID.to_string(),
            steps,
            warnings,
            meta: serde_json::from_value(json!({"generatedCount": generated_count})).map_err(
                |_| BackendError::protocol("concrete document plan metadata encoding failed"),
            )?,
        };
        let grant_ids = provisional
            .grants()
            .iter()
            .map(|grant| grant.grant_id.clone())
            .collect();
        let cached = self.filesystem.cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                OPERATION_ID,
                grant_ids,
            ),
            plan,
        )?;
        provisional.commit();
        encode(cached)
    }
}
