use std::collections::BTreeSet;

use super::*;

const SECTION: &str = "followUpExceptions";
const TABLE: &str = "follow_up_exceptions";

impl NativeBackend {
    fn stored_follow_up_exceptions(&self) -> BTreeSet<String> {
        self.settings
            .section(SECTION)
            .and_then(|section| section.get("codes").cloned())
            .and_then(|codes| serde_json::from_value::<Vec<String>>(codes).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|code| rfi_follow_up::canonical_follow_up_rfi(&code).map(|(c, _)| c))
            .collect()
    }

    /// The cloud list, or `None` when offline or not configured.
    fn cloud_follow_up_exceptions(&self) -> Option<BTreeSet<String>> {
        let rows = self
            .templates
            .rest(
                "GET",
                &format!("{TABLE}?select=code&limit=5000"),
                None,
                now(),
            )
            .ok()?;
        Some(
            rows.as_array()?
                .iter()
                .filter_map(|row| row.get("code")?.as_str())
                .filter_map(|code| rfi_follow_up::canonical_follow_up_rfi(code).map(|(c, _)| c))
                .collect(),
        )
    }

    /// Merges the operator's text with the local and cloud lists, saves the result
    /// in both places, and returns the RFIs the sync must leave alone.
    pub(super) fn resolve_follow_up_exceptions(
        &self,
        text: Option<&str>,
    ) -> Result<BTreeSet<String>, BackendError> {
        let edit = rfi_follow_up::parse_exception_text(text.unwrap_or_default())?;
        let local = self.stored_follow_up_exceptions();
        let cloud = self.cloud_follow_up_exceptions();
        // A removal the cloud never saw would come back on the next sync.
        if !edit.remove.is_empty() && cloud.is_none() {
            return Err(BackendError::validation(
                "error.rfi.followUp.exceptionCloudUnavailable",
            ));
        }
        let mut stored = local.clone();
        stored.extend(cloud.iter().flatten().cloned());
        let effective = rfi_follow_up::apply_exception_edit(stored, &edit)?;
        if let Some(cloud) = &cloud {
            let removed = cloud
                .iter()
                .filter(|code| edit.remove.contains(*code))
                .cloned()
                .collect::<Vec<_>>();
            if !removed.is_empty() {
                self.templates.rest(
                    "DELETE",
                    &format!("{TABLE}?code=in.({})", removed.join(",")),
                    None,
                    now(),
                )?;
            }
            let added = effective
                .iter()
                .filter(|code| !cloud.contains(*code))
                .map(|code| json!({ "code": code }))
                .collect::<Vec<_>>();
            if !added.is_empty() {
                // Best effort: the local list still applies if the cloud refuses.
                let _ = self
                    .templates
                    .rest("POST", TABLE, Some(Value::Array(added)), now());
            }
        }
        if effective != local {
            let mut section = serde_json::Map::new();
            section.insert("codes".to_string(), json!(effective));
            self.settings
                .save_section(SECTION, Some(section))
                .map_err(|_| BackendError::settings_write())?;
        }
        Ok(effective)
    }
}
