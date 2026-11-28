use super::preview_steps::*;
use super::*;

/// Reads a selected input through the read grant that covers it, bounded by the package size limit.
fn read_granted_input(
    filesystem: &FilesystemAuthority,
    provisional: &crate::paths::ProvisionalPathGrants<'_>,
    path: &Path,
) -> Result<Vec<u8>, BackendError> {
    let grant = provisional
        .grants()
        .iter()
        .filter(|grant| path.starts_with(&grant.root) && grant.access.allows(PathIntent::Read))
        .max_by_key(|grant| grant.root.components().count())
        .ok_or_else(|| BackendError::path_invalid("No read grant covers a selected input"))?;
    let anchor = filesystem.anchor_granted_path(grant, path, PathIntent::Read)?;
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
}

fn measured_ambient_by_rfi(
    value: Option<&Value>,
    selected_rfis: &[String],
) -> Result<HashMap<String, Vec<String>>, BackendError> {
    let invalid = || BackendError::validation("error.rfi.documents.measuredAmbient");
    let raw = value.map_or(Some(""), Value::as_str).ok_or_else(invalid)?;
    let mut measurements = HashMap::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let (rfi, readings) = line.split_once('=').ok_or_else(invalid)?;
        let rfis = requested_rfis(rfi).map_err(|_| invalid())?;
        if rfis.len() != 1 || !selected_rfis.contains(&rfis[0]) {
            return Err(invalid());
        }
        let readings = readings
            .split_whitespace()
            .map(|reading| {
                let reading = reading.replace(',', ".");
                let unsigned = reading.strip_prefix('-').unwrap_or(&reading);
                let (whole, decimal) = unsigned.split_once('.').unwrap_or((unsigned, "0"));
                if whole.is_empty()
                    || whole.len() > 3
                    || decimal.is_empty()
                    || decimal.len() > 6
                    || !whole
                        .bytes()
                        .chain(decimal.bytes())
                        .all(|byte| byte.is_ascii_digit())
                    || !reading
                        .parse::<f64>()
                        .is_ok_and(|value| (-100.0..=100.0).contains(&value))
                {
                    return Err(invalid());
                }
                Ok(reading)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if readings.is_empty() || measurements.insert(rfis[0].clone(), readings).is_some() {
            return Err(invalid());
        }
    }
    Ok(measurements)
}

fn input_grant_roots(
    rfi_log: &Path,
    follow_up: Option<&Path>,
    selections: &[(DocumentType, Vec<String>, PathBuf)],
    output_directory: &Path,
) -> Result<Vec<(PathBuf, GrantAccess)>, BackendError> {
    let mut roots = Vec::<(PathBuf, GrantAccess)>::new();
    let follow_up = follow_up.filter(|path| path.is_file());
    for path in std::iter::once(rfi_log)
        .chain(follow_up)
        .chain(selections.iter().map(|(_, _, path)| path.as_path()))
    {
        let root = path
            .parent()
            .ok_or_else(|| BackendError::path_invalid("Selected input has no parent folder"))?
            .to_path_buf();
        add_grant_root(&mut roots, root, GrantAccess::Read);
    }
    add_grant_root(
        &mut roots,
        output_directory.to_path_buf(),
        GrantAccess::ReadWrite,
    );
    Ok(roots)
}

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
        let log_statuses = match request.values.rfi_log_workbook.as_deref() {
            None => None,
            Some(log) => {
                if !log.is_file() {
                    return Err(BackendError::path_invalid(
                        "The selected RFI Log workbook is unavailable",
                    ));
                }
                let mut roots = Vec::new();
                add_grant_root(
                    &mut roots,
                    log.parent()
                        .ok_or_else(|| {
                            BackendError::path_invalid("Selected input has no parent folder")
                        })?
                        .to_path_buf(),
                    GrantAccess::Read,
                );
                let provisional = self.filesystem.issue_provisional_grants(&scope, &roots)?;
                let bytes = read_granted_input(&self.filesystem, &provisional, log)?;
                Some(crate::rfi_system_sync::read_log_statuses(
                    &mut Cursor::new(bytes),
                )?)
            }
        };
        let (cancellation, _registration) = self.filesystem.register_cancellation(&request.op_id);
        let mut plan = preview_fast_covers(&scan_params, &ocr, &mut progress, || {
            cancellation.is_cancelled()
        })?;
        ocr.attach_stats(&mut plan);
        if let Some(statuses) = &log_statuses {
            crate::operations::process_scans::apply_log_check(&mut plan, statuses);
        }
        crate::operations::process_scans::attach_cover_checks(&mut plan, |source, page| {
            if cancellation.is_cancelled() {
                return None;
            }
            // A failed second read only leaves that row unverified; the scan itself stands.
            native_ocr
                .check_cover_identity(source, page)
                .ok()
                .map(|check| crate::operations::process_scans::CoverEvidence {
                    crop_data_url: check.crop_data_url,
                    reread_tail: check.reread_tail,
                })
        });
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
        let mut plan = preview_rfi_file_group(
            &GroupRfiFilesParams {
                source: request.values.source,
                destination: request.values.destination,
            },
            &ocr,
            progress,
            || cancellation.is_cancelled(),
        )?;
        ocr.attach_stats(&mut plan);
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
            jiff::Zoned::now().date(),
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
        let measured_ambient_010 =
            measured_ambient_by_rfi(request.values.get("measured_ambient_010"), &rfis_010)?;
        let measured_ambient_011 =
            measured_ambient_by_rfi(request.values.get("measured_ambient_011"), &rfis_011)?;
        let rfi_log = required_selected_path(&request.values, "rfi_log_workbook")?;
        let output_directory = required_selected_path(&request.values, "output_directory")?;
        if !output_directory.is_dir() {
            return Err(concrete_documents_error("Output folder is unavailable"));
        }
        let selections = selected_templates(
            &request.values,
            [
                (DocumentType::Civ010, rfis_010.clone(), "template_010"),
                (DocumentType::Civ011, rfis_011, "template_011"),
                (DocumentType::Civ013, rfis_013, "template_013"),
                (DocumentType::Civ014, rfis_014, "template_014"),
            ],
        )?;
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

        let roots = input_grant_roots(
            &rfi_log,
            follow_up.as_deref(),
            &selections,
            &output_directory,
        )?;
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            OPERATION_ID,
        );
        let provisional = self.filesystem.issue_provisional_grants(&scope, &roots)?;
        let read_selected = |path: &Path| read_granted_input(&self.filesystem, &provisional, path);

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
                        .map(|row| additional_concrete_batch(row, follow))
                        .collect()
                } else {
                    Vec::new()
                };
                let ambient_input = match document_type {
                    DocumentType::Civ010 => Some((
                        &measured_ambient_010,
                        additional_batches.len() + 1,
                        "CON-CIV-010",
                    )),
                    DocumentType::Civ011 => Some((&measured_ambient_011, 1, "CON-CIV-011")),
                    _ => None,
                };
                let measured_ambient = if let Some((ambient, expected, form)) = ambient_input {
                    let Some(readings) =
                        ambient.get(&rfi).filter(|values| values.len() == expected)
                    else {
                        warnings.push(Warning {
                            key: "warn.rfi.documents.measuredAmbientMissing".to_string(),
                            subject: format!("{rfi} ({form})"),
                        });
                        continue;
                    };
                    readings.clone()
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
                    measured_ambient,
                    drawing: field("drawing").to_string(),
                    additional_batches,
                });
            }
            if records.is_empty() {
                continue;
            }
            generated.extend(
                concrete_documents::generate(document_type, &template_bytes, &records)
                    .map_err(|error| concrete_documents_error(&error))?,
            );
        }

        let generated_count = generated.len() as u64;
        let steps = output_steps(
            generated,
            &output_directory,
            &mut warnings,
            "step.rfi.generatedConcreteDocument",
        )?;
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

fn template_error(detail: impl std::fmt::Display) -> BackendError {
    BackendError::validation(format!("error.templates.generate: {detail}"))
}

/// Renders every row and turns the files into no-overwrite write steps.
fn template_steps(
    kind: crate::templates::FileKind,
    template: &[u8],
    map: &crate::templates::field_map::FieldMap,
    rows: &[Value],
    output_directory: &Path,
    warnings: &mut Vec<Warning>,
) -> Result<Vec<Step>, BackendError> {
    let mut generated = Vec::with_capacity(rows.len());
    let mut empty = std::collections::BTreeSet::new();
    for row in rows {
        let (file, missing) =
            template_documents::render(kind, template, map, row).map_err(template_error)?;
        empty.extend(missing);
        generated.push(file);
    }
    if !empty.is_empty() {
        warnings.push(Warning {
            key: "warn.templates.placeholderEmpty".to_string(),
            subject: empty.into_iter().collect::<Vec<_>>().join(", "),
        });
    }
    output_steps(
        generated,
        output_directory,
        warnings,
        template_documents::STEP_NOTE,
    )
}

impl NativeBackend {
    fn cached_template(
        &self,
        code: &str,
    ) -> Result<
        (
            crate::templates::TemplateEntry,
            Vec<u8>,
            crate::templates::field_map::FieldMap,
        ),
        BackendError,
    > {
        let (entry, bytes) = self
            .templates
            .read_cached(code)
            .ok_or_else(|| BackendError::validation("error.templates.notCached"))?;
        let map =
            crate::templates::field_map::FieldMap::parse(&entry.field_map).map_err(|error| {
                BackendError::validation(format!("error.templates.invalid: {error}"))
            })?;
        Ok((entry, bytes, map))
    }

    pub(crate) fn preview_template_document<F>(
        &self,
        params: Value,
        mut progress: F,
    ) -> Result<Value, BackendError>
    where
        F: FnMut(ProgressEvent),
    {
        let request: PreviewEnvelope<Map<String, Value>> =
            decode(self.materialize_preview(params)?)?;
        if request.op_id != template_documents::OPERATION_ID {
            return Err(BackendError::validation(
                "operation is not owned by the template document backend",
            ));
        }
        let text = |name: &str| {
            request
                .values
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let code = text("template");
        let (entry, template, map) = self.cached_template(&code)?;
        if map.fields.is_empty() {
            return Err(BackendError::validation("error.templates.noFields"));
        }
        let free = text("mode") == "free";
        let rfis = if free {
            Vec::new()
        } else {
            template_documents::requested_rfis(&text("rfi_numbers"), Some(&entry.discipline))
                .map_err(template_error)?
        };
        let count = request
            .values
            .get("document_count")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        if free && !(1..=template_documents::MAX_RFIS as i64).contains(&count) {
            return Err(template_error("The document count must be 1 to 100"));
        }
        let rfi_log = if free {
            None
        } else {
            Some(
                required_selected_path(&request.values, "rfi_log_workbook").map_err(|_| {
                    BackendError::validation("error.param.required.rfi_log_workbook")
                })?,
            )
        };
        let output_directory = required_selected_path(&request.values, "output_directory")?;
        if rfi_log.as_ref().is_some_and(|path| !path.is_file()) {
            return Err(template_error(
                "The selected RFI Log workbook is unavailable",
            ));
        }
        if !output_directory.is_dir() {
            return Err(template_error("The output folder is unavailable"));
        }
        let follow_up = if free {
            None
        } else {
            optional_selected_path(&request.values, "follow_up_workbook")
        };
        if follow_up.as_ref().is_some_and(|path| !path.is_file()) {
            return Err(template_error(
                "The selected Follow-Up workbook is unavailable",
            ));
        }
        let mut roots = Vec::new();
        for input in rfi_log.iter().chain(follow_up.iter()) {
            add_grant_root(
                &mut roots,
                input
                    .parent()
                    .ok_or_else(|| {
                        BackendError::path_invalid("Selected input has no parent folder")
                    })?
                    .to_path_buf(),
                GrantAccess::Read,
            );
        }
        add_grant_root(&mut roots, output_directory.clone(), GrantAccess::ReadWrite);
        let caller = self.caller_identity()?;
        let scope = GrantScope::new(
            &caller.owner_id,
            &caller.session_id,
            uuid::Uuid::new_v4().to_string(),
            template_documents::OPERATION_ID,
        );
        let provisional = self.filesystem.issue_provisional_grants(&scope, &roots)?;
        let mut warnings = Vec::new();
        let rows = if let Some(rfi_log) = &rfi_log {
            let log_bytes = read_granted_input(&self.filesystem, &provisional, rfi_log)?;
            let log = template_documents::read_log_rows(&mut Cursor::new(log_bytes), &rfis)
                .map_err(template_error)?;
            warnings = rfis
                .iter()
                .filter(|rfi| !log.contains_key(*rfi))
                .map(|rfi| Warning {
                    key: "warn.templates.rfiNotInLog".to_string(),
                    subject: rfi.clone(),
                })
                .collect();
            let batches = match &follow_up {
                Some(path) => {
                    let bytes = read_granted_input(&self.filesystem, &provisional, path)?;
                    let batches =
                        template_documents::read_follow_up_batches(&mut Cursor::new(bytes), &rfis)
                            .map_err(template_error)?;
                    warnings.extend(
                        rfis.iter()
                            .filter(|rfi| log.contains_key(*rfi) && !batches.contains_key(*rfi))
                            .map(|rfi| Warning {
                                key: "warn.templates.followUpMissing".to_string(),
                                subject: rfi.clone(),
                            }),
                    );
                    batches
                }
                None => std::collections::BTreeMap::new(),
            };
            template_documents::initial_rows_with(&map, &log, &batches, &rfis)
        } else {
            template_documents::free_rows(&map, count as usize)
        };
        if rows.is_empty() {
            return Err(template_error("None of the RFI numbers are in the RFI Log"));
        }
        let steps = template_steps(
            entry.file_kind,
            &template,
            &map,
            &rows,
            &output_directory,
            &mut warnings,
        )?;
        progress(ProgressEvent {
            phase: "template_document_generation".to_string(),
            done: steps.len() as u64,
            total: steps.len() as u64,
            subject: output_directory.to_string_lossy().into_owned(),
        });
        let meta = serde_json::from_value(json!({
            "editor": template_documents::editor(&map, rows),
            "templateCode": entry.code,
            "templateVersion": entry.version,
            "templateSha256": entry.file_sha256,
            "outputDirectory": output_directory.to_string_lossy(),
        }))
        .map_err(|_| BackendError::protocol("template plan metadata encoding failed"))?;
        let plan = Plan {
            plan_id: String::new(),
            op_id: template_documents::OPERATION_ID.to_string(),
            steps,
            warnings,
            meta,
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
                template_documents::OPERATION_ID,
                grant_ids,
            ),
            plan,
        )?;
        provisional.commit();
        encode(cached)
    }

    /// Re-renders edited rows from the exact template version previewed.
    pub(crate) fn revise_template_document(
        &self,
        plan: &Plan,
        payload: &Value,
    ) -> Result<Plan, BackendError> {
        let invalid = || BackendError::validation("error.templates.rowsInvalid");
        let meta_text = |key: &str| {
            plan.meta
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        let (entry, template, map) = self.cached_template(meta_text("templateCode"))?;
        if entry.file_sha256 != meta_text("templateSha256") {
            return Err(BackendError::validation("error.templates.changed"));
        }
        let original = plan
            .meta
            .get("editor")
            .and_then(|editor| editor.get("rows"))
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        let submitted = payload["rows"].as_array().ok_or_else(invalid)?;
        let rows = template_documents::accept_rows(original, submitted).map_err(|_| invalid())?;
        let rows = template_documents::with_computed(&map, rows);
        let output_directory = PathBuf::from(meta_text("outputDirectory"));
        let mut warnings = plan
            .warnings
            .iter()
            .filter(|warning| {
                matches!(
                    warning.key.as_str(),
                    "warn.templates.rfiNotInLog" | "warn.templates.followUpMissing"
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let steps = template_steps(
            entry.file_kind,
            &template,
            &map,
            &rows,
            &output_directory,
            &mut warnings,
        )?;
        let mut revised = plan.clone();
        revised.steps = steps;
        revised.warnings = warnings;
        revised
            .meta
            .insert("editor".to_string(), template_documents::editor(&map, rows));
        Ok(revised)
    }
}
