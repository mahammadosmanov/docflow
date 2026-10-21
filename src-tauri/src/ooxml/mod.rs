use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt, fs,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

use quick_xml::{
    Writer,
    escape::unescape,
    events::{BytesEnd, BytesStart, BytesText, Event},
    name::{NamespaceResolver, ResolveResult},
    reader::NsReader,
};
use serde::{Deserialize, Serialize};
use zip::{ZipArchive, ZipWriter};

pub(crate) const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_MERGE_VALIDATION_COMPARISONS_PER_WORKBOOK: u64 = 1_000_000;
pub(crate) const MAX_APPLY_MERGE_COMPARISONS: u64 = 20_000_000;
const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 4_096;
const MAX_WORKSHEETS: usize = 250;
const MAX_SHEET_NAME_BYTES: usize = 1_024;
const MAX_RELATIONSHIP_ID_BYTES: usize = 1_024;
const MAX_RELATIONSHIP_TYPE_BYTES: usize = 2_048;
const MAX_RELATIONSHIP_TARGET_BYTES: usize = 4_096;
const MAX_STATE_BYTES: usize = 16;
const MAX_U32_ATTRIBUTE_BYTES: usize = 10;
const MAX_XML_DEPTH: usize = 256;
const MAX_XML_NODES: usize = 20_000_000;
const MAX_MERGED_RANGES: usize = 10_000;
const MAX_WORKSHEET_ROWS: u32 = 1_048_576;
const MAX_WORKSHEET_COLUMNS: u32 = 16_384;
const TRANSITIONAL_WORKBOOK_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const STRICT_WORKBOOK_NAMESPACE: &str = "http://purl.oclc.org/ooxml/spreadsheetml/main";
const TRANSITIONAL_RELATIONSHIP_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT_RELATIONSHIP_NAMESPACE: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships";
const TRANSITIONAL_OFFICE_DOCUMENT_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const STRICT_OFFICE_DOCUMENT_RELATIONSHIP: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
const TRANSITIONAL_PACKAGE_RELATIONSHIP_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships";
const STRICT_PACKAGE_RELATIONSHIP_NAMESPACE: &str =
    "http://purl.oclc.org/ooxml/package/relationships";
const WORKBOOK_NAMESPACES: [&str; 2] = [TRANSITIONAL_WORKBOOK_NAMESPACE, STRICT_WORKBOOK_NAMESPACE];
const PACKAGE_RELATIONSHIP_NAMESPACES: [&str; 2] = [
    TRANSITIONAL_PACKAGE_RELATIONSHIP_NAMESPACE,
    STRICT_PACKAGE_RELATIONSHIP_NAMESPACE,
];
const TRANSITIONAL_RELATIONSHIP_PREFIX: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_RELATIONSHIP_PREFIX: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";

#[derive(Clone, Debug, PartialEq)]
pub enum CellUpdate {
    Text(String),
    Number(String),
    Bool(bool),
    Formula {
        formula: String,
        cached: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct GuardedCellUpdate {
    pub expected: CellData,
    pub replacement: CellUpdate,
    pub create_if_absent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SheetInfo {
    pub path: String,
    pub sheet_id: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorksheetVisibility {
    Visible,
    Hidden,
    VeryHidden,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetDescriptor {
    pub workbook_index: u32,
    pub sheet_id: u32,
    pub relationship_id: String,
    pub relationship_type: String,
    pub part_path: String,
    pub name: String,
    pub visibility: WorksheetVisibility,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookSafetyFacts {
    pub has_vba_project: bool,
    pub has_xlm_macro_sheet: bool,
    pub has_workbook_protection: bool,
    pub external_link_count: u32,
    pub connection_count: u32,
    pub query_table_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedWorkbookDiscovery {
    pub worksheets: Vec<WorksheetDescriptor>,
    pub safety: WorkbookSafetyFacts,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CellData {
    pub value: String,
    pub formula: Option<String>,
    pub style: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellRange {
    pub first_row: u32,
    pub first_column: u32,
    pub last_row: u32,
    pub last_column: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorksheetData {
    pub max_row: u32,
    pub max_column: u32,
    pub cells: BTreeMap<(u32, u32), CellData>,
    pub column_widths: BTreeMap<u32, f64>,
    pub has_sheet_protection: bool,
    pub merged_ranges: Vec<CellRange>,
}

impl WorksheetData {
    pub fn value(&self, row: u32, column: u32) -> &str {
        self.cells
            .get(&(row, column))
            .map(|cell| cell.value.as_str())
            .unwrap_or("")
    }

    pub fn cell(&self, row: u32, column: u32) -> Option<&CellData> {
        self.cells.get(&(row, column))
    }

    pub fn merged_range_containing(&self, row: u32, column: u32) -> Option<&CellRange> {
        self.merged_ranges.iter().find(|range| {
            range.first_row <= row
                && row <= range.last_row
                && range.first_column <= column
                && column <= range.last_column
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbookData {
    pub sheets: BTreeMap<String, WorksheetData>,
    merge_validation_comparisons: u64,
    merge_validation_complete: bool,
}

impl WorkbookData {
    pub fn sheet(&self, name: &str) -> Option<&WorksheetData> {
        self.sheets.get(name)
    }

    pub(crate) fn merge_validation_comparisons(&self) -> u64 {
        self.merge_validation_comparisons
    }

    pub(crate) fn merge_validation_complete(&self) -> bool {
        self.merge_validation_complete
    }
}

#[derive(Debug)]
pub(crate) struct MergeComparisonBudget {
    remaining: u64,
    consumed: u64,
}

impl MergeComparisonBudget {
    fn for_validation() -> Self {
        Self {
            remaining: MAX_MERGE_VALIDATION_COMPARISONS_PER_WORKBOOK,
            consumed: 0,
        }
    }

    fn for_apply(parsed_comparisons: u64) -> Result<Self, OoxmlError> {
        let remaining = MAX_APPLY_MERGE_COMPARISONS
            .checked_sub(parsed_comparisons)
            .ok_or_else(|| {
                OoxmlError::limit_exceeded("guarded merge comparison count exceeded the limit")
            })?;
        Ok(Self {
            remaining,
            consumed: parsed_comparisons,
        })
    }

    fn consume(&mut self) -> Result<(), OoxmlError> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            OoxmlError::limit_exceeded("guarded merge comparison count exceeded the limit")
        })?;
        self.consumed = self.consumed.checked_add(1).ok_or_else(|| {
            OoxmlError::limit_exceeded("guarded merge comparison count exceeded the limit")
        })?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OoxmlErrorKind {
    Invalid,
    LimitExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OoxmlError {
    kind: OoxmlErrorKind,
    message: String,
}

impl OoxmlError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: OoxmlErrorKind::Invalid,
            message: message.into(),
        }
    }

    fn limit_exceeded(message: impl Into<String>) -> Self {
        Self {
            kind: OoxmlErrorKind::LimitExceeded,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> OoxmlErrorKind {
        self.kind
    }
}

impl fmt::Display for OoxmlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for OoxmlError {}

impl From<std::io::Error> for OoxmlError {
    fn from(error: std::io::Error) -> Self {
        Self::invalid(error.to_string())
    }
}

impl From<zip::result::ZipError> for OoxmlError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::invalid(error.to_string())
    }
}

impl From<quick_xml::Error> for OoxmlError {
    fn from(error: quick_xml::Error) -> Self {
        Self::invalid(error.to_string())
    }
}

#[derive(Clone)]
enum XmlNode {
    Element(XmlElement),
    Raw(Event<'static>),
}

#[derive(Clone)]
struct XmlElement {
    start: BytesStart<'static>,
    namespace: Option<Vec<u8>>,
    attributes: Vec<ExpandedAttribute>,
    children: Vec<XmlNode>,
    empty: bool,
}

#[derive(Clone)]
struct ExpandedAttribute {
    namespace: Option<Vec<u8>>,
    local_name: Vec<u8>,
    value: Vec<u8>,
}

#[derive(Clone)]
struct XmlDocument {
    nodes: Vec<XmlNode>,
}

#[derive(Debug, Eq, PartialEq)]
enum CanonicalXmlEvent {
    Start {
        namespace: Option<Vec<u8>>,
        local_name: Vec<u8>,
        attributes: Vec<(Option<Vec<u8>>, Vec<u8>, String)>,
    },
    End {
        namespace: Option<Vec<u8>>,
        local_name: Vec<u8>,
    },
    Empty {
        namespace: Option<Vec<u8>>,
        local_name: Vec<u8>,
        attributes: Vec<(Option<Vec<u8>>, Vec<u8>, String)>,
    },
    Raw {
        kind: &'static str,
        value: Vec<u8>,
    },
}

struct ZipPreflightFacts {
    entry_count: usize,
    central_directory_start: u64,
}

struct WorkbookPackageParts {
    workbook: String,
    relationships: String,
}

mod archive;
mod patch;
mod read;
mod relationships;
mod sheet_patch;
mod worksheet;
mod xml;
use archive::*;
pub use patch::*;
pub use read::*;
use relationships::*;
use sheet_patch::*;
use worksheet::*;
use xml::*;

#[cfg(test)]
mod tests;
