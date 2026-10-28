use super::*;

impl FilesystemAuthority {
    pub(super) fn execute_pdf_first_page_replace(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        let archive = required_path(step.src.as_deref(), "archive PDF")?;
        let archive = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &archive,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?;
        let signed = step
            .meta
            .get("signedPdf")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| StepFailure::NoEffect("signed PDF path is missing".to_string()))?;
        let signed = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &signed,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?;
        let destination = required_path(step.dst.as_deref(), "review PDF destination")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let expected_archive_hash = step
            .meta
            .get("archiveSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("archive PDF hash is missing".to_string()))?;
        let expected_signed_hash = step
            .meta
            .get("signedSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("signed PDF hash is missing".to_string()))?;
        let signed_page_index = step
            .meta
            .get("signedPageIndex")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        StepFailure::NoEffect("signed PDF page index is invalid".to_string())
                    })
            })
            .transpose()?
            .unwrap_or(0);
        let archive_anchor = self.anchor_plan_path(binding, caller, &archive, PathIntent::Read)?;
        let signed_anchor = self.anchor_plan_path(binding, caller, &signed, PathIntent::Read)?;
        let destination_anchor =
            self.anchor_plan_path(binding, caller, &destination, PathIntent::Write)?;
        if archive_anchor.sha256().map_err(io_no_effect)? != expected_archive_hash {
            return Err(StepFailure::NoEffect(
                "archive PDF changed after preview".to_string(),
            ));
        }
        if signed_anchor.sha256().map_err(io_no_effect)? != expected_signed_hash {
            return Err(StepFailure::NoEffect(
                "signed PDF changed after preview".to_string(),
            ));
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        record.pending_step = Some(step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;

        let workspace = tempfile::tempdir().map_err(io_no_effect)?;
        let workspace_capability =
            crate::cap_fs::CapabilityRoot::open(workspace.path()).map_err(io_no_effect)?;
        let archive_copy = workspace.path().join("archive.pdf");
        let signed_copy = workspace.path().join("signed.pdf");
        let completed = workspace.path().join("completed.pdf");
        let archive_copy_anchor = workspace_capability
            .authorize(&archive_copy)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let signed_copy_anchor = workspace_capability
            .authorize(&signed_copy)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        archive_copy_anchor
            .copy_no_clobber_from(&archive_anchor)
            .map_err(io_no_effect)?;
        signed_copy_anchor
            .copy_no_clobber_from(&signed_anchor)
            .map_err(io_no_effect)?;
        let generated = replace_first_page_incremental_from_page(
            &archive_copy,
            &signed_copy,
            signed_page_index,
            &completed,
        )
        .map_err(|error| StepFailure::NoEffect(error.to_string()));
        if let Err(error) = generated {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(error);
        }
        let completed_anchor = workspace_capability
            .authorize(&completed)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let output_hash = completed_anchor.sha256().map_err(io_no_effect)?;
        if token.checkpoint().is_err() {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::Cancelled);
        }
        if archive_anchor.sha256().map_err(io_no_effect)? != expected_archive_hash
            || signed_anchor.sha256().map_err(io_no_effect)? != expected_signed_hash
        {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::NoEffect(
                "approved PDF input changed during apply".to_string(),
            ));
        }
        if let Err(error) = destination_anchor.copy_no_clobber_from(&completed_anchor) {
            if error.kind() != io::ErrorKind::AlreadyExists
                && destination_anchor.open_read().is_ok()
            {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    destination.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(io_no_effect(error));
        }
        if destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })? != output_hash
        {
            return Err(StepFailure::OutcomeUnknown(
                "published review PDF failed hash validation".to_string(),
            ));
        }
        let mut actual = Step {
            kind: StepKind::Copy,
            src: Some(archive.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: step.note_key.clone(),
            meta: step.meta.clone(),
        };
        actual
            .meta
            .insert("undoSha256".to_string(), Value::String(output_hash));
        record.steps.push(actual);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }

    pub(super) fn execute_pdf_page_rotate(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        if binding.operation_id != "rfi.normalize_pdf_orientation" {
            return Err(StepFailure::NoEffect(
                "PDF rotation is not authorized for this operation".to_string(),
            ));
        }
        let source = required_path(step.src.as_deref(), "source PDF")?;
        let source = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &source,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?;
        let destination = required_path(step.dst.as_deref(), "orientation review PDF")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let expected_hash = step
            .meta
            .get("sourceSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("source PDF hash is missing".to_string()))?;
        let rotations = step
            .meta
            .get("rotations")
            .and_then(Value::as_array)
            .ok_or_else(|| StepFailure::NoEffect("approved PDF rotations are missing".to_string()))?
            .iter()
            .map(|rotation| {
                let page_index = rotation
                    .get("pageIndex")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        StepFailure::NoEffect("approved page index is invalid".to_string())
                    })?;
                let target = rotation
                    .get("targetRotation")
                    .and_then(Value::as_i64)
                    .filter(|value| matches!(value, 0 | 90 | 180 | 270))
                    .ok_or_else(|| {
                        StepFailure::NoEffect("approved page rotation is invalid".to_string())
                    })?;
                Ok((page_index, target))
            })
            .collect::<Result<Vec<_>, StepFailure>>()?;
        if rotations.is_empty() {
            return Err(StepFailure::NoEffect(
                "approved PDF rotations are empty".to_string(),
            ));
        }
        let source_anchor = self.anchor_plan_path(binding, caller, &source, PathIntent::Read)?;
        if source_anchor.sha256().map_err(io_no_effect)? != expected_hash {
            return Err(StepFailure::NoEffect(
                "source PDF changed after preview".to_string(),
            ));
        }
        let destination_anchor =
            self.anchor_plan_path(binding, caller, &destination, PathIntent::Write)?;
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        record.pending_step = Some(step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;

        let workspace = tempfile::tempdir().map_err(io_no_effect)?;
        let workspace_capability =
            crate::cap_fs::CapabilityRoot::open(workspace.path()).map_err(io_no_effect)?;
        let source_copy = workspace.path().join("source.pdf");
        let completed = workspace.path().join("completed.pdf");
        let source_copy_anchor = workspace_capability
            .authorize(&source_copy)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        source_copy_anchor
            .copy_no_clobber_from(&source_anchor)
            .map_err(io_no_effect)?;
        if let Err(error) = rotate_pages_incremental(&source_copy, &completed, &rotations) {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::NoEffect(error.to_string()));
        }
        let completed_anchor = workspace_capability
            .authorize(&completed)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let output_hash = completed_anchor.sha256().map_err(io_no_effect)?;
        if token.checkpoint().is_err() {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::Cancelled);
        }
        if source_anchor.sha256().map_err(io_no_effect)? != expected_hash {
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(StepFailure::NoEffect(
                "approved PDF changed during apply".to_string(),
            ));
        }
        if let Err(error) = destination_anchor.copy_no_clobber_from(&completed_anchor) {
            if error.kind() != io::ErrorKind::AlreadyExists
                && destination_anchor.open_read().is_ok()
            {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    destination.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(io_no_effect(error));
        }
        if destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })? != output_hash
        {
            return Err(StepFailure::OutcomeUnknown(
                "published orientation PDF failed hash validation".to_string(),
            ));
        }
        let mut actual = Step {
            kind: StepKind::Copy,
            src: Some(source.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: step.note_key.clone(),
            meta: step.meta.clone(),
        };
        actual
            .meta
            .insert("undoSha256".to_string(), Value::String(output_hash));
        record.steps.push(actual);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }

    pub(super) fn execute_pdf_pages_extract(
        &self,
        step: &Step,
        binding: &PlanBinding,
        caller: &CallerIdentity,
        token: &CancellationToken,
        record: &mut JournalRecord,
    ) -> Result<u64, StepFailure> {
        if !matches!(
            binding.operation_id.as_str(),
            "rfi.process_scans" | "rfi.process_scans_fast_covers"
        ) {
            return Err(StepFailure::NoEffect(
                "PDF page extraction is not authorized for this operation".to_string(),
            ));
        }
        let source = required_path(step.src.as_deref(), "batch scan PDF")?;
        let source = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &source,
                PathIntent::Read,
            )
            .map_err(step_no_effect)?;
        let destination = required_path(step.dst.as_deref(), "extracted RFI PDF")?;
        let destination = self
            .grants
            .authorize_for_plan(
                &binding.grant_ids,
                caller,
                &binding.operation_id,
                &destination,
                PathIntent::Write,
            )
            .map_err(step_no_effect)?;
        let expected_hash = step
            .meta
            .get("sourceSha256")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("batch scan hash is missing".to_string()))?;
        let page_indexes = step
            .meta
            .get("pageIndexes")
            .and_then(Value::as_array)
            .filter(|indexes| !indexes.is_empty())
            .ok_or_else(|| StepFailure::NoEffect("approved PDF pages are missing".to_string()))?
            .iter()
            .map(|index| {
                index
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        StepFailure::NoEffect("approved PDF page is invalid".to_string())
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if page_indexes.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(StepFailure::NoEffect(
                "approved PDF page order is invalid".to_string(),
            ));
        }
        let source_anchor = self.anchor_plan_path(binding, caller, &source, PathIntent::Read)?;
        if source_anchor.sha256().map_err(io_no_effect)? != expected_hash {
            return Err(StepFailure::NoEffect(
                "batch scan changed after Preview".to_string(),
            ));
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        let workspace = tempfile::tempdir().map_err(io_no_effect)?;
        let workspace_capability =
            crate::cap_fs::CapabilityRoot::open(workspace.path()).map_err(io_no_effect)?;
        let source_copy = workspace.path().join("batch.pdf");
        let extracted = workspace.path().join("extracted.pdf");
        let rotated = workspace.path().join("rotated.pdf");
        let source_copy_anchor = workspace_capability
            .authorize(&source_copy)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        source_copy_anchor
            .copy_no_clobber_from(&source_anchor)
            .map_err(io_no_effect)?;
        crate::pdf::extract_pages(&source_copy, &page_indexes, &extracted)
            .map_err(|error| StepFailure::NoEffect(error.to_string()))?;
        let rotations = step
            .meta
            .get("rotations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|rotation| {
                let page_index = rotation
                    .get("pageIndex")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        StepFailure::NoEffect("approved output page is invalid".to_string())
                    })?;
                let target = rotation
                    .get("targetRotation")
                    .and_then(Value::as_i64)
                    .filter(|value| matches!(value, 0 | 90 | 180 | 270))
                    .ok_or_else(|| {
                        StepFailure::NoEffect("approved output rotation is invalid".to_string())
                    })?;
                Ok((page_index, target))
            })
            .collect::<Result<Vec<_>, StepFailure>>()?;
        let completed = if rotations.is_empty() {
            extracted
        } else {
            crate::pdf::rotate_pages_incremental(&extracted, &rotated, &rotations)
                .map_err(|error| StepFailure::NoEffect(error.to_string()))?;
            rotated
        };
        let completed_anchor = workspace_capability
            .authorize(&completed)
            .map_err(io_no_effect)?
            .anchor()
            .map_err(io_no_effect)?;
        let output_hash = completed_anchor.sha256().map_err(io_no_effect)?;
        if source_anchor.sha256().map_err(io_no_effect)? != expected_hash {
            return Err(StepFailure::NoEffect(
                "batch scan changed during Apply".to_string(),
            ));
        }
        token.checkpoint().map_err(|_| StepFailure::Cancelled)?;
        let destination_parent = destination.parent().ok_or_else(|| {
            StepFailure::NoEffect("extracted PDF destination has no parent".to_string())
        })?;
        self.create_missing_directories(destination_parent, step, binding, caller, token, record)?;
        let destination_anchor =
            self.anchor_plan_path(binding, caller, &destination, PathIntent::Write)?;
        record.pending_step = Some(step.clone());
        record.state = Some(JournalState::Applying);
        self.journals.write(record).map_err(step_unknown)?;
        if let Err(error) = destination_anchor.copy_no_clobber_from(&completed_anchor) {
            if error.kind() != io::ErrorKind::AlreadyExists
                && destination_anchor.open_read().is_ok()
            {
                return Err(StepFailure::OutcomeUnknown(format!(
                    "{}: {error}",
                    destination.display()
                )));
            }
            record.pending_step = None;
            self.journals.write(record).map_err(step_unknown)?;
            return Err(io_no_effect(error));
        }
        if destination_anchor.sha256().map_err(|error| {
            StepFailure::OutcomeUnknown(format!("{}: {error}", destination.display()))
        })? != output_hash
        {
            return Err(StepFailure::OutcomeUnknown(
                "extracted PDF failed hash validation".to_string(),
            ));
        }
        let mut actual = Step {
            kind: StepKind::Copy,
            src: Some(source.to_string_lossy().into_owned()),
            dst: Some(destination.to_string_lossy().into_owned()),
            note_key: step.note_key.clone(),
            meta: step.meta.clone(),
        };
        actual
            .meta
            .insert("undoSha256".to_string(), Value::String(output_hash));
        record.steps.push(actual);
        record.pending_step = None;
        self.journals.write(record).map_err(step_unknown)?;
        Ok(1)
    }
}
