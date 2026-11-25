//! Template editing for admins and granted editors (ADR 0055).
//!
//! Every write goes to Supabase, where row-level security and the publish
//! function decide who may change what. Published versions never change: an
//! edit always lands in a draft, which becomes visible to everyone only when
//! it is published.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{
    FileKind, MAX_TEMPLATE_BYTES, TemplateStore, anchors,
    copies::{self, CopySuggestion},
    field_map::{FieldDef, FieldMap, TextBlock, replace_in_block, scan, text_blocks},
    remap::{self, AnchorCheck},
    structure::{self, Structure},
    verify_package,
};
use crate::errors::BackendError;

/// One template as an editor sees it, including unpublished drafts.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableTemplate {
    pub template_id: String,
    pub code: String,
    pub title: String,
    pub discipline: String,
    pub file_kind: FileKind,
    pub current_version: Option<u32>,
    pub draft_version: Option<u32>,
}

/// The draft being edited, with the placeholders found in its file.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDraft {
    pub template_id: String,
    pub version_id: String,
    pub version: u32,
    pub code: String,
    pub title: String,
    pub discipline: String,
    pub file_kind: FileKind,
    pub field_map: FieldMap,
    pub placeholders: Vec<String>,
    /// After a file replacement: where each marked cell is in the new file.
    pub checks: Vec<AnchorCheck>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreVersion {
    pub code: String,
    pub version_id: String,
}

/// One version of a template as the version list shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub version_id: String,
    pub version: u32,
    pub status: String,
    pub published_at: Option<String>,
    pub note: String,
}

#[derive(Debug, Deserialize)]
struct VersionListRow {
    id: String,
    version: u32,
    status: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTemplate {
    pub code: String,
    pub title: String,
    pub discipline: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDraft {
    pub version_id: String,
    /// The new code; renaming keeps the template's history and versions.
    pub code: String,
    pub title: String,
    pub discipline: String,
    pub field_map: Value,
}

/// A field made from selected text, or plain replacement text.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewField {
    pub key: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditText {
    pub version_id: String,
    pub part: String,
    pub index: usize,
    /// UTF-16 offsets into the block's text, as the webview selected them.
    pub start: usize,
    pub end: usize,
    pub expected: String,
    /// New literal text, used when `field` is absent.
    #[serde(default)]
    pub replacement: String,
    #[serde(default)]
    pub field: Option<NewField>,
}

#[derive(Debug, Deserialize)]
struct TemplateRow {
    id: String,
    code: String,
    title: String,
    discipline: String,
    file_kind: FileKind,
    current_version: Option<u32>,
    #[serde(default)]
    doc_template_versions: Vec<VersionStub>,
}

#[derive(Debug, Deserialize)]
struct VersionStub {
    id: String,
    version: u32,
    status: String,
}

#[derive(Debug, Deserialize)]
struct VersionRow {
    id: String,
    template_id: String,
    version: u32,
    status: String,
    file_sha256: String,
    file_size: u64,
    storage_path: String,
    field_map: Value,
}

fn invalid(detail: impl std::fmt::Display) -> BackendError {
    BackendError::validation(format!("error.templates.invalid: {detail}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn single<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, BackendError> {
    let rows: Vec<T> = serde_json::from_value(value)
        .map_err(|_| BackendError::request_failed("template response is invalid"))?;
    rows.into_iter()
        .next()
        .ok_or_else(|| BackendError::validation("error.templates.notFound"))
}

fn uuid_ok(value: &str) -> Result<&str, BackendError> {
    uuid::Uuid::parse_str(value)
        .map(|_| value)
        .map_err(|_| BackendError::invalid_params())
}

/// Code, title and discipline rules shared with the SQL constraints.
fn check_identity(code: Option<&str>, title: &str, discipline: &str) -> Result<(), BackendError> {
    let code_ok = code.is_none_or(|code| super::TEMPLATE_CODE.is_match(code));
    let title_ok = !title.trim().is_empty() && title.chars().count() <= 200;
    let discipline_ok = (2..=8).contains(&discipline.len())
        && discipline.bytes().all(|byte| byte.is_ascii_uppercase());
    if !code_ok {
        return Err(BackendError::validation("error.templates.codeInvalid"));
    }
    if !title_ok || !discipline_ok {
        return Err(BackendError::validation("error.templates.identityInvalid"));
    }
    Ok(())
}

/// The file kind from a picked file's name, checked against its content.
pub fn file_kind_of(name: &str, bytes: &[u8]) -> Result<FileKind, BackendError> {
    let lower = name.to_ascii_lowercase();
    let kind = if lower.ends_with(".docx") {
        FileKind::Docx
    } else if lower.ends_with(".xlsx") {
        FileKind::Xlsx
    } else {
        return Err(BackendError::validation("error.templates.fileType"));
    };
    if bytes.is_empty() || bytes.len() as u64 > MAX_TEMPLATE_BYTES {
        return Err(BackendError::validation("error.templates.fileSize"));
    }
    verify_package(kind, bytes)?;
    Ok(kind)
}

impl TemplateStore {
    pub fn can_edit(&self, now: i64) -> bool {
        self.remote
            .rest("POST", "rpc/is_template_editor", Some(json!({})), now)
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }

    pub fn editable(&self, now: i64) -> Result<Vec<EditableTemplate>, BackendError> {
        let rows: Vec<TemplateRow> = serde_json::from_value(self.remote.rest(
            "GET",
            "doc_templates?select=id,code,title,discipline,file_kind,current_version,\
             doc_template_versions(id,version,status)&archived=is.false&order=code.asc&limit=500",
            None,
            now,
        )?)
        .map_err(|_| BackendError::request_failed("template list is invalid"))?;
        Ok(rows
            .into_iter()
            .map(|row| EditableTemplate {
                draft_version: row
                    .doc_template_versions
                    .iter()
                    .filter(|version| version.status == "draft")
                    .map(|version| version.version)
                    .max(),
                template_id: row.id,
                code: row.code,
                title: row.title,
                discipline: row.discipline,
                file_kind: row.file_kind,
                current_version: row.current_version,
            })
            .collect())
    }

    fn template_row(&self, filter: &str, now: i64) -> Result<TemplateRow, BackendError> {
        single(self.remote.rest(
            "GET",
            &format!(
                "doc_templates?select=id,code,title,discipline,file_kind,current_version,\
                 doc_template_versions(id,version,status)&{filter}&limit=1"
            ),
            None,
            now,
        )?)
    }

    fn version_row(&self, version_id: &str, now: i64) -> Result<VersionRow, BackendError> {
        single(self.remote.rest(
            "GET",
            &format!(
                "doc_template_versions?select=*&id=eq.{}&limit=1",
                uuid_ok(version_id)?
            ),
            None,
            now,
        )?)
    }

    /// The draft's file, checked against its recorded checksum and size.
    fn version_bytes(&self, version: &VersionRow, now: i64) -> Result<Vec<u8>, BackendError> {
        let bytes = self.remote.download(&version.storage_path, now)?;
        if sha256_hex(&bytes) != version.file_sha256 || bytes.len() as u64 != version.file_size {
            return Err(BackendError::request_failed(
                "template file does not match its checksum",
            ));
        }
        Ok(bytes)
    }

    /// The text of the draft's file, one block per paragraph or string.
    pub fn blocks(&self, version_id: &str, now: i64) -> Result<Vec<TextBlock>, BackendError> {
        let version = self.version_row(version_id, now)?;
        let template =
            self.template_row(&format!("id=eq.{}", uuid_ok(&version.template_id)?), now)?;
        let bytes = self.version_bytes(&version, now)?;
        text_blocks(template.file_kind, &bytes).map_err(invalid)
    }

    /// The draft file's tables or sheets, for marking cells (ADR 0058).
    pub fn cells(&self, version_id: &str, now: i64) -> Result<Structure, BackendError> {
        let version = self.version_row(version_id, now)?;
        let template =
            self.template_row(&format!("id=eq.{}", uuid_ok(&version.template_id)?), now)?;
        let bytes = self.version_bytes(&version, now)?;
        structure::cells(template.file_kind, &bytes).map_err(invalid)
    }

    /// Compares the draft file with other filled copies of the same form and
    /// suggests the cells that change. The copies are only read, never stored.
    pub fn suggest_from_copies(
        &self,
        version_id: &str,
        files: &[(String, Vec<u8>)],
        now: i64,
    ) -> Result<Vec<CopySuggestion>, BackendError> {
        let version = self.version_row(version_id, now)?;
        let template =
            self.template_row(&format!("id=eq.{}", uuid_ok(&version.template_id)?), now)?;
        let mut copies = Vec::with_capacity(files.len());
        for (name, bytes) in files {
            if file_kind_of(name, bytes)? != template.file_kind {
                return Err(BackendError::validation("error.templates.fileType"));
            }
            copies.push(bytes.clone());
        }
        let base = self.version_bytes(&version, now)?;
        copies::compare(template.file_kind, &base, &copies).map_err(invalid)
    }

    /// Turns selected text into a field, or changes it, inside the draft's
    /// file. The edited file is uploaded as the draft's new file.
    pub fn edit_text(&self, request: &EditText, now: i64) -> Result<TemplateDraft, BackendError> {
        let version = self.version_row(&request.version_id, now)?;
        if version.status != "draft" {
            return Err(BackendError::validation("error.templates.notDraft"));
        }
        let template =
            self.template_row(&format!("id=eq.{}", uuid_ok(&version.template_id)?), now)?;
        let mut field_map = FieldMap::parse(&version.field_map).unwrap_or_default();
        let replacement = match &request.field {
            Some(field) => {
                let def = FieldDef {
                    key: field.key.clone(),
                    label: field.label.trim().to_string(),
                    value: field.value.clone(),
                    options: Vec::new(),
                    multiline: false,
                    anchor: None,
                };
                match field_map.fields.iter_mut().find(|item| item.key == def.key) {
                    Some(existing) => {
                        existing.label = def.label;
                        existing.value = def.value;
                    }
                    None => field_map.fields.push(def),
                }
                field_map.validate().map_err(invalid)?;
                format!("{{{{{}}}}}", field.key)
            }
            None => {
                if request.replacement.contains("{{")
                    || request.replacement.contains("}}")
                    || request.replacement.chars().count() > 4_000
                {
                    return Err(invalid("replacement text is not allowed"));
                }
                request.replacement.clone()
            }
        };
        let bytes = self.version_bytes(&version, now)?;
        let edited = replace_in_block(
            template.file_kind,
            &bytes,
            &request.part,
            request.index,
            request.start..request.end,
            &request.expected,
            &replacement,
        )
        .map_err(invalid)?;
        if edited.len() as u64 > MAX_TEMPLATE_BYTES {
            return Err(BackendError::validation("error.templates.fileSize"));
        }
        let (sha, path) = self.upload_file(
            &template.id,
            version.version,
            template.file_kind,
            &edited,
            now,
        )?;
        let (found, defaults) = scan(template.file_kind, &edited).map_err(invalid)?;
        field_map.merge_placeholders(&found, &defaults);
        let updated = single::<VersionRow>(self.remote.rest(
            "PATCH",
            &format!("doc_template_versions?id=eq.{}&status=eq.draft", version.id),
            Some(json!({
                "file_sha256": sha,
                "file_size": edited.len(),
                "storage_path": path,
                "field_map": field_map.to_value(),
            })),
            now,
        )?)?;
        self.draft_of(&template, updated, now)
    }

    fn draft_of(
        &self,
        template: &TemplateRow,
        version: VersionRow,
        now: i64,
    ) -> Result<TemplateDraft, BackendError> {
        let bytes = self.version_bytes(&version, now)?;
        let (found, defaults) = scan(template.file_kind, &bytes).map_err(invalid)?;
        let mut field_map = FieldMap::parse(&version.field_map).unwrap_or_default();
        field_map.merge_placeholders(&found, &defaults);
        Ok(TemplateDraft {
            template_id: template.id.clone(),
            version_id: version.id,
            version: version.version,
            code: template.code.clone(),
            title: template.title.clone(),
            discipline: template.discipline.clone(),
            file_kind: template.file_kind,
            field_map,
            placeholders: found,
            checks: Vec::new(),
        })
    }

    /// Opens the template's draft, creating one from the published version
    /// when none exists. The new draft reuses the published file.
    pub fn open_draft(&self, code: &str, now: i64) -> Result<TemplateDraft, BackendError> {
        check_identity(Some(code), "x", "GEN")?;
        let template = self.template_row(&format!("code=eq.{code}"), now)?;
        let latest = template
            .doc_template_versions
            .iter()
            .max_by_key(|version| version.version)
            .ok_or_else(|| BackendError::validation("error.templates.notFound"))?;
        if latest.status == "draft" {
            let version = self.version_row(&latest.id, now)?;
            return self.draft_of(&template, version, now);
        }
        let published = self.version_row(&latest.id, now)?;
        let created = single::<VersionRow>(self.remote.rest(
            "POST",
            "doc_template_versions",
            Some(json!({
                "template_id": template.id,
                "version": latest.version + 1,
                "status": "draft",
                "file_sha256": published.file_sha256,
                "file_size": published.file_size,
                "storage_path": published.storage_path,
                "field_map": published.field_map,
            })),
            now,
        )?)?;
        self.draft_of(&template, created, now)
    }

    fn upload_file(
        &self,
        template_id: &str,
        version: u32,
        kind: FileKind,
        bytes: &[u8],
        now: i64,
    ) -> Result<(String, String), BackendError> {
        let sha = sha256_hex(bytes);
        let path = format!("{template_id}/{version}/{sha}.{}", kind.extension());
        self.remote.upload(&path, kind, bytes, now)?;
        Ok((sha, path))
    }

    /// Creates a new template with version 1 as a draft.
    pub fn create(
        &self,
        request: &NewTemplate,
        file_name: &str,
        bytes: &[u8],
        now: i64,
    ) -> Result<TemplateDraft, BackendError> {
        check_identity(Some(&request.code), &request.title, &request.discipline)?;
        let kind = file_kind_of(file_name, bytes)?;
        let template = single::<TemplateRow>(self.remote.rest(
            "POST",
            "doc_templates",
            Some(json!({
                "code": request.code,
                "title": request.title.trim(),
                "discipline": request.discipline,
                "file_kind": kind,
            })),
            now,
        )?)?;
        let (sha, path) = self.upload_file(&template.id, 1, kind, bytes, now)?;
        let mut field_map = FieldMap::default();
        let (found, defaults) = scan(kind, bytes).map_err(invalid)?;
        field_map.merge_placeholders(&found, &defaults);
        let version = single::<VersionRow>(self.remote.rest(
            "POST",
            "doc_template_versions",
            Some(json!({
                "template_id": template.id,
                "version": 1,
                "status": "draft",
                "file_sha256": sha,
                "file_size": bytes.len(),
                "storage_path": path,
                "field_map": field_map.to_value(),
            })),
            now,
        )?)?;
        self.draft_of(&template, version, now)
    }

    /// Puts a new file into a draft. Field settings are kept; new
    /// placeholders get fields.
    pub fn replace_file(
        &self,
        version_id: &str,
        file_name: &str,
        bytes: &[u8],
        now: i64,
    ) -> Result<TemplateDraft, BackendError> {
        let version = self.version_row(version_id, now)?;
        if version.status != "draft" {
            return Err(BackendError::validation("error.templates.notDraft"));
        }
        let template =
            self.template_row(&format!("id=eq.{}", uuid_ok(&version.template_id)?), now)?;
        if file_kind_of(file_name, bytes)? != template.file_kind {
            return Err(BackendError::validation("error.templates.fileType"));
        }
        let mut field_map = FieldMap::parse(&version.field_map).unwrap_or_default();
        let anchored = field_map
            .fields
            .iter()
            .filter_map(|field| {
                field
                    .anchor
                    .clone()
                    .map(|anchor| (field.key.clone(), anchor))
            })
            .collect::<Vec<_>>();
        let checks = if anchored.is_empty() {
            Vec::new()
        } else {
            let old = self.version_bytes(&version, now)?;
            remap::check(template.file_kind, &old, bytes, &anchored).map_err(invalid)?
        };
        let (sha, path) = self.upload_file(
            &template.id,
            version.version,
            template.file_kind,
            bytes,
            now,
        )?;
        let (found, defaults) = scan(template.file_kind, bytes).map_err(invalid)?;
        field_map.merge_placeholders(&found, &defaults);
        let updated = single::<VersionRow>(self.remote.rest(
            "PATCH",
            &format!("doc_template_versions?id=eq.{}&status=eq.draft", version.id),
            Some(json!({
                "file_sha256": sha,
                "file_size": bytes.len(),
                "storage_path": path,
                "field_map": field_map.to_value(),
            })),
            now,
        )?)?;
        let mut draft = self.draft_of(&template, updated, now)?;
        draft.checks = checks;
        Ok(draft)
    }

    /// Every version of the template, newest first.
    pub fn versions(&self, code: &str, now: i64) -> Result<Vec<VersionSummary>, BackendError> {
        check_identity(Some(code), "x", "GEN")?;
        let template = self.template_row(&format!("code=eq.{code}"), now)?;
        let rows: Vec<VersionListRow> = serde_json::from_value(self.remote.rest(
            "GET",
            &format!(
                "doc_template_versions?select=id,version,status,published_at,note\
                 &template_id=eq.{}&order=version.desc",
                uuid_ok(&template.id)?
            ),
            None,
            now,
        )?)
        .map_err(|_| BackendError::request_failed("template response is invalid"))?;
        let mut list = rows
            .into_iter()
            .map(|row| VersionSummary {
                version_id: row.id,
                version: row.version,
                status: row.status,
                published_at: row.published_at,
                note: row.note,
            })
            .collect::<Vec<_>>();
        list.sort_by_key(|item| std::cmp::Reverse(item.version));
        Ok(list)
    }

    /// Makes an earlier version the draft again: its file and field map go
    /// into a new draft after the latest version, or replace the open draft.
    /// Earlier versions are never changed.
    pub fn restore_version(
        &self,
        code: &str,
        version_id: &str,
        now: i64,
    ) -> Result<TemplateDraft, BackendError> {
        check_identity(Some(code), "x", "GEN")?;
        let template = self.template_row(&format!("code=eq.{code}"), now)?;
        let old = self.version_row(version_id, now)?;
        if old.template_id != template.id || old.status == "draft" {
            return Err(BackendError::validation("error.templates.notFound"));
        }
        let latest = template
            .doc_template_versions
            .iter()
            .max_by_key(|version| version.version)
            .ok_or_else(|| BackendError::validation("error.templates.notFound"))?;
        let copied = json!({
            "file_sha256": old.file_sha256,
            "file_size": old.file_size,
            "storage_path": old.storage_path,
            "field_map": old.field_map,
        });
        let row = if latest.status == "draft" {
            single::<VersionRow>(self.remote.rest(
                "PATCH",
                &format!("doc_template_versions?id=eq.{}&status=eq.draft", latest.id),
                Some(copied),
                now,
            )?)?
        } else {
            let mut body = copied;
            body["template_id"] = json!(template.id);
            body["version"] = json!(latest.version + 1);
            body["status"] = json!("draft");
            single::<VersionRow>(self.remote.rest(
                "POST",
                "doc_template_versions",
                Some(body),
                now,
            )?)?
        };
        self.draft_of(&template, row, now)
    }

    pub fn save_draft(&self, request: &SaveDraft, now: i64) -> Result<TemplateDraft, BackendError> {
        check_identity(Some(&request.code), &request.title, &request.discipline)?;
        let field_map = FieldMap::parse(&request.field_map).map_err(invalid)?;
        let version = self.version_row(&request.version_id, now)?;
        if version.status != "draft" {
            return Err(BackendError::validation("error.templates.notDraft"));
        }
        let anchored = field_map
            .fields
            .iter()
            .filter_map(|field| field.anchor.as_ref().map(|anchor| (anchor, "")))
            .collect::<Vec<_>>();
        if !anchored.is_empty() {
            // Dry run on the draft's own file: a cell that cannot be written
            // is refused now, not when someone generates a document.
            let template =
                self.template_row(&format!("id=eq.{}", uuid_ok(&version.template_id)?), now)?;
            let bytes = self.version_bytes(&version, now)?;
            anchors::apply(template.file_kind, &bytes, &anchored).map_err(invalid)?;
        }
        self.remote.rest(
            "PATCH",
            &format!("doc_templates?id=eq.{}", uuid_ok(&version.template_id)?),
            Some(json!({
                "code": request.code,
                "title": request.title.trim(),
                "discipline": request.discipline,
            })),
            now,
        )?;
        let updated = single::<VersionRow>(self.remote.rest(
            "PATCH",
            &format!("doc_template_versions?id=eq.{}&status=eq.draft", version.id),
            Some(json!({"field_map": field_map.to_value()})),
            now,
        )?)?;
        let template = self.template_row(&format!("id=eq.{}", version.template_id), now)?;
        self.draft_of(&template, updated, now)
    }

    /// Removes a template from every user's library. Rows and files are kept
    /// (archived, not destroyed), so generated documents stay traceable.
    pub fn archive(&self, code: &str, now: i64) -> Result<super::TemplateStatus, BackendError> {
        check_identity(Some(code), "x", "GEN")?;
        let template = self.template_row(&format!("code=eq.{code}"), now)?;
        self.remote.rest(
            "PATCH",
            &format!("doc_templates?id=eq.{}", uuid_ok(&template.id)?),
            Some(json!({"archived": true})),
            now,
        )?;
        Ok(self.sync(now))
    }

    /// Publishes a valid draft for everyone, then refreshes the local cache.
    pub fn publish(
        &self,
        version_id: &str,
        now: i64,
    ) -> Result<super::TemplateStatus, BackendError> {
        let version = self.version_row(version_id, now)?;
        if version.status != "draft" {
            return Err(BackendError::validation("error.templates.notDraft"));
        }
        let field_map = FieldMap::parse(&version.field_map).map_err(invalid)?;
        let anchored = field_map
            .fields
            .iter()
            .filter_map(|field| field.anchor.as_ref().map(|anchor| (anchor, "")))
            .collect::<Vec<_>>();
        if !anchored.is_empty() {
            // Last check before everyone gets the version: every marked cell
            // must still be writable in the draft's own file.
            let template =
                self.template_row(&format!("id=eq.{}", uuid_ok(&version.template_id)?), now)?;
            let bytes = self.version_bytes(&version, now)?;
            anchors::apply(template.file_kind, &bytes, &anchored).map_err(invalid)?;
        }
        self.remote.rest(
            "POST",
            "rpc/publish_template_version",
            Some(json!({"p_version_id": version.id})),
            now,
        )?;
        Ok(self.sync(now))
    }
}

#[cfg(test)]
mod tests;
