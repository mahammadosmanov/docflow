use super::*;

use crate::{fs_ops::file_sha256, ocr::render_page_thumbnail_data_url};

const OPERATION_ID: &str = "rfi.stage_signed_first_pages";
static RENDER_LOCK: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComparisonRequest {
    plan_id: String,
    row_id: String,
    archive_choice: String,
}

impl NativeBackend {
    pub(super) fn signed_page_comparison(&self, params: Value) -> Result<Value, BackendError> {
        let request: ComparisonRequest = decode(params)?;
        let _render = RENDER_LOCK
            .try_lock()
            .map_err(|_| comparison_unavailable())?;
        let caller = self.caller_identity()?;
        let claimed = self
            .filesystem
            .resolve_native_plan(&request.plan_id, &caller)?;
        let binding = claimed.binding.ok_or_else(BackendError::plan_unavailable)?;
        if claimed.plan.op_id != OPERATION_ID
            || claimed
                .plan
                .meta
                .get("pageComparisonOnDemand")
                .and_then(Value::as_bool)
                != Some(true)
        {
            return Err(BackendError::invalid_params());
        }
        let candidates = claimed
            .plan
            .meta
            .get("archiveCandidateSteps")
            .and_then(|rows| rows.get(&request.row_id))
            .and_then(Value::as_object)
            .ok_or_else(BackendError::invalid_params)?;
        let steps = if request.archive_choice.is_empty() {
            candidates.values().next()
        } else {
            candidates.get(&request.archive_choice)
        }
        .and_then(Value::as_array)
        .ok_or_else(BackendError::invalid_params)?;
        let step: Step = steps
            .iter()
            .find(|step| step.get("kind").and_then(Value::as_str) == Some("pdf_first_page_replace"))
            .cloned()
            .ok_or_else(BackendError::invalid_params)
            .and_then(decode)?;
        if step.meta.get("rowId").and_then(Value::as_str) != Some(request.row_id.as_str()) {
            return Err(BackendError::invalid_params());
        }
        let text = |key| {
            step.meta
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(comparison_unavailable)
        };
        let signed_page = step
            .meta
            .get("signedPageIndex")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or_else(comparison_unavailable)?;
        let render = |path: &str, hash: &str, page| {
            let anchor = self
                .filesystem
                .anchor_bound_path(&binding, &caller, Path::new(path), PathIntent::Read)
                .map_err(|_| comparison_unavailable())?;
            let source = anchor.open_read().map_err(|_| comparison_unavailable())?;
            if source
                .metadata()
                .map_err(|_| comparison_unavailable())?
                .len()
                > crate::pdf::MAX_PDF_BYTES
            {
                return Err(comparison_unavailable());
            }
            // Native renderers reopen a pathname. Give them only the private, hash-verified copy.
            let directory = tempfile::tempdir().map_err(|_| comparison_unavailable())?;
            let private_pdf = directory.path().join("source.pdf");
            let mut output =
                std::fs::File::create_new(&private_pdf).map_err(|_| comparison_unavailable())?;
            let copied =
                std::io::copy(&mut source.take(crate::pdf::MAX_PDF_BYTES + 1), &mut output)
                    .map_err(|_| comparison_unavailable())?;
            drop(output);
            if copied == 0
                || copied > crate::pdf::MAX_PDF_BYTES
                || file_sha256(&private_pdf).map_err(|_| comparison_unavailable())? != hash
            {
                return Err(comparison_unavailable());
            }
            render_page_thumbnail_data_url(&private_pdf, page).map_err(|_| comparison_unavailable())
        };
        let archive = if request.archive_choice.is_empty() {
            None
        } else {
            Some(render(
                step.src.as_deref().ok_or_else(comparison_unavailable)?,
                text("archiveSha256")?,
                0,
            )?)
        };
        let signed = render(text("signedPdf")?, text("signedSha256")?, signed_page)?;
        Ok(json!({ "archive": archive, "signed": signed }))
    }
}

fn comparison_unavailable() -> BackendError {
    BackendError::validation("rfi.signed.previewUnavailable")
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
mod tests;
