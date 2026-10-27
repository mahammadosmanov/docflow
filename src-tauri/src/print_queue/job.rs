use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::{Map, Value};

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    pub job_id: String,
    pub path: String,
    pub printer: String,
    pub verb: String,
    pub format: String,
    pub paper: String,
    pub orientation: String,
    pub scaling: String,
    pub color: String,
    pub duplex: String,
    #[serde(default = "default_page_range")]
    pub page_range: String,
    pub timeout_minutes: u32,
    pub retries: u32,
    pub on_error: String,
    pub copy: u32,
    #[serde(default)]
    pub signature: FileSignature,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worksheet: Option<WorksheetDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workbook_safety: Option<WorkbookSafetyFacts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_review: Option<CleanupReviewEvidence>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PrintJobUnchecked {
    job_id: String,
    path: String,
    printer: String,
    verb: String,
    format: String,
    paper: String,
    orientation: String,
    scaling: String,
    color: String,
    duplex: String,
    page_range: String,
    timeout_minutes: u32,
    retries: u32,
    on_error: String,
    copy: u32,
    signature: FileSignature,
    worksheet: Option<WorksheetDescriptor>,
    workbook_safety: Option<WorkbookSafetyFacts>,
    status: Option<String>,
    attempts: Option<u32>,
    error: Option<String>,
    completed_at: Option<String>,
    cleanup_review: Option<CleanupReviewEvidence>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl Default for PrintJob {
    fn default() -> Self {
        Self {
            job_id: String::new(),
            path: String::new(),
            printer: String::new(),
            verb: String::new(),
            format: String::new(),
            paper: "A4".to_string(),
            orientation: "auto".to_string(),
            scaling: "fit".to_string(),
            color: "printer".to_string(),
            duplex: "printer".to_string(),
            page_range: default_page_range(),
            timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
            retries: 0,
            on_error: "pause".to_string(),
            copy: 1,
            signature: FileSignature::default(),
            worksheet: None,
            workbook_safety: None,
            status: None,
            attempts: None,
            error: None,
            completed_at: None,
            cleanup_review: None,
            extra: Map::new(),
        }
    }
}

impl Default for PrintJobUnchecked {
    fn default() -> Self {
        let job = PrintJob::default();
        Self {
            job_id: job.job_id,
            path: job.path,
            printer: job.printer,
            verb: job.verb,
            format: job.format,
            paper: job.paper,
            orientation: job.orientation,
            scaling: job.scaling,
            color: job.color,
            duplex: job.duplex,
            page_range: job.page_range,
            timeout_minutes: job.timeout_minutes,
            retries: job.retries,
            on_error: job.on_error,
            copy: job.copy,
            signature: job.signature,
            worksheet: job.worksheet,
            workbook_safety: job.workbook_safety,
            status: job.status,
            attempts: job.attempts,
            error: job.error,
            completed_at: job.completed_at,
            cleanup_review: job.cleanup_review,
            extra: job.extra,
        }
    }
}

impl<'de> Deserialize<'de> for PrintJob {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("print job must be an object"))?;
        let worksheet_present = object.contains_key("worksheet");
        let safety_present = object.contains_key("workbookSafety");
        let signature_present = object.contains_key("signature");
        for field in ["worksheet", "workbookSafety", "signature"] {
            if object.get(field).is_some_and(Value::is_null) {
                return Err(D::Error::custom(format!(
                    "print job field {field} cannot be null"
                )));
            }
        }
        let unchecked: PrintJobUnchecked =
            serde_json::from_value(value).map_err(D::Error::custom)?;
        if worksheet_present {
            if !signature_present
                || !safety_present
                || unchecked.worksheet.is_none()
                || unchecked.workbook_safety.is_none()
                || !unchecked.signature.has_valid_sha256()
            {
                return Err(D::Error::custom("incomplete worksheet print metadata"));
            }
        } else if safety_present
            || unchecked.workbook_safety.is_some()
            || unchecked.signature.sha256.is_some()
        {
            return Err(D::Error::custom("orphan worksheet print metadata"));
        }
        Ok(Self {
            job_id: unchecked.job_id,
            path: unchecked.path,
            printer: unchecked.printer,
            verb: unchecked.verb,
            format: unchecked.format,
            paper: unchecked.paper,
            orientation: unchecked.orientation,
            scaling: unchecked.scaling,
            color: unchecked.color,
            duplex: unchecked.duplex,
            page_range: unchecked.page_range,
            timeout_minutes: unchecked.timeout_minutes,
            retries: unchecked.retries,
            on_error: unchecked.on_error,
            copy: unchecked.copy,
            signature: unchecked.signature,
            worksheet: unchecked.worksheet,
            workbook_safety: unchecked.workbook_safety,
            status: unchecked.status,
            attempts: unchecked.attempts,
            error: unchecked.error,
            completed_at: unchecked.completed_at,
            cleanup_review: unchecked.cleanup_review,
            extra: unchecked.extra,
        })
    }
}

impl PrintJob {
    pub fn settings(&self) -> PrintSettings {
        PrintSettings {
            paper: self.paper.clone(),
            orientation: self.orientation.clone(),
            scaling: self.scaling.clone(),
            color: self.color.clone(),
            duplex: self.duplex.clone(),
            page_range: self.page_range.clone(),
            timeout_minutes: self.timeout_minutes,
        }
    }

    pub(super) fn normalize_legacy(&mut self, index: usize) {
        if self.job_id.is_empty() {
            self.job_id = format!("legacy-{}", index + 1);
        }
        if self.paper.is_empty() {
            self.paper = "A4".to_string();
        }
        if self.orientation.is_empty() {
            self.orientation = "auto".to_string();
        }
        if self.scaling.is_empty() {
            self.scaling = "fit".to_string();
        }
        if self.color.is_empty() {
            self.color = "printer".to_string();
        }
        if self.duplex.is_empty() {
            self.duplex = "printer".to_string();
        }
        if self.timeout_minutes == 0 {
            self.timeout_minutes = DEFAULT_TIMEOUT_MINUTES;
        }
        if self.on_error.is_empty() {
            self.on_error = "pause".to_string();
        }
        if self.copy == 0 {
            self.copy = 1;
        }
    }
}
