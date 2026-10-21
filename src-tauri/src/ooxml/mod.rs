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

pub fn patch_workbook_sheets(
    source: &Path,
    destination: &Path,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), CellUpdate>>,
    allow_consecutive_duplicates_column: Option<u32>,
) -> Result<(), OoxmlError> {
    if updates_by_sheet.is_empty() {
        return Err(OoxmlError::invalid(
            "at least one worksheet update is required",
        ));
    }
    validate_source(source)?;
    if destination.exists() {
        return Err(OoxmlError::invalid("destination already exists"));
    }
    let destination_parent = destination
        .parent()
        .ok_or_else(|| OoxmlError::invalid("destination has no parent"))?;
    if !destination_parent.is_dir() {
        return Err(OoxmlError::invalid("destination parent does not exist"));
    }

    let mut input = File::open(source)?;
    let discovery = ordered_workbook_discovery(&mut input)?;
    let sheet_infos = project_sheet_infos(&discovery);
    let mut archive = open_archive(input)?;
    let mut patched = BTreeMap::<String, Vec<u8>>::new();
    let mut replaced_by_sheet = Vec::<(u32, BTreeSet<String>)>::new();
    for (sheet_name, updates) in updates_by_sheet {
        let info = sheet_infos
            .get(sheet_name)
            .ok_or_else(|| OoxmlError::invalid(format!("worksheet not found: {sheet_name}")))?;
        let raw = read_entry(&mut archive, &info.path)?;
        let (xml, replaced) = patch_sheet_xml(&raw, updates, allow_consecutive_duplicates_column)?;
        patched.insert(info.path.clone(), xml);
        if !replaced.is_empty() {
            replaced_by_sheet.push((info.sheet_id, replaced));
        }
    }
    if !replaced_by_sheet.is_empty() && has_entry(&mut archive, "xl/calcChain.xml")? {
        let mut document = parse_document(&read_entry(&mut archive, "xl/calcChain.xml")?)?;
        patch_calc_chain(&mut document, &replaced_by_sheet)?;
        patched.insert(
            "xl/calcChain.xml".to_string(),
            serialize_document(&document)?,
        );
    }

    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let write_result = write_package(&mut archive, output, &patched);
    if write_result.is_err() {
        let _ = fs::remove_file(destination);
    }
    write_result
}

pub fn patch_workbook_sheets_guarded<R, W>(
    source: &mut R,
    destination: &mut W,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
) -> Result<(), OoxmlError>
where
    R: Read + Seek,
    W: Read + Write + Seek,
{
    patch_workbook_sheets_guarded_inner(source, destination, updates_by_sheet, true)
}

/// Applies arbitrary text replacements while retaining the same exact-cell and
/// closed-package-delta guarantees as the Follow Up status writer. Callers must
/// validate their operation-specific replacement vocabulary before invoking it.
pub fn patch_workbook_sheets_guarded_text<R, W>(
    source: &mut R,
    destination: &mut W,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
) -> Result<(), OoxmlError>
where
    R: Read + Seek,
    W: Read + Write + Seek,
{
    patch_workbook_sheets_guarded_inner(source, destination, updates_by_sheet, false)
}

fn patch_workbook_sheets_guarded_inner<R, W>(
    source: &mut R,
    destination: &mut W,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
    status_values_only: bool,
) -> Result<(), OoxmlError>
where
    R: Read + Seek,
    W: Read + Write + Seek,
{
    validate_guarded_update_set(updates_by_sheet, status_values_only)?;
    source.seek(SeekFrom::Start(0))?;
    let discovery = ordered_workbook_discovery(source)?;
    if discovery.safety.has_workbook_protection {
        return Err(OoxmlError::invalid("guarded workbook is protected"));
    }
    let sheet_infos = project_sheet_infos(&discovery);
    source.seek(SeekFrom::Start(0))?;
    if package_has_signature(source)? {
        return Err(OoxmlError::invalid("guarded workbook is signed"));
    }
    source.seek(SeekFrom::Start(0))?;
    let workbook = read_workbook_from_reader(source)?;
    validate_guarded_source(&workbook, updates_by_sheet)?;

    source.seek(SeekFrom::Start(0))?;
    let mut archive = open_archive(source)?;
    let mut patched = BTreeMap::<String, Vec<u8>>::new();
    for (sheet_name, updates) in updates_by_sheet {
        let info = sheet_infos
            .get(sheet_name)
            .ok_or_else(|| OoxmlError::invalid("guarded worksheet is missing"))?;
        let raw = read_entry(&mut archive, &info.path)?;
        patched.insert(
            info.path.clone(),
            patch_sheet_xml_guarded(&raw, updates, status_values_only)?,
        );
    }

    if destination.seek(SeekFrom::End(0))? != 0 {
        return Err(OoxmlError::invalid(
            "guarded destination must be an empty create-new file",
        ));
    }
    destination.seek(SeekFrom::Start(0))?;
    write_package_guarded(&mut archive, destination, &patched)?;
    destination.seek(SeekFrom::Start(0))?;
    Ok(())
}

pub fn verify_guarded_workbook_delta<R1, R2>(
    source: &mut R1,
    destination: &mut R2,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
) -> Result<(), OoxmlError>
where
    R1: Read + Seek,
    R2: Read + Seek,
{
    verify_guarded_workbook_delta_inner(source, destination, updates_by_sheet, true)
}

pub fn verify_guarded_workbook_delta_text<R1, R2>(
    source: &mut R1,
    destination: &mut R2,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
) -> Result<(), OoxmlError>
where
    R1: Read + Seek,
    R2: Read + Seek,
{
    verify_guarded_workbook_delta_inner(source, destination, updates_by_sheet, false)
}

fn verify_guarded_workbook_delta_inner<R1, R2>(
    source: &mut R1,
    destination: &mut R2,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
    status_values_only: bool,
) -> Result<(), OoxmlError>
where
    R1: Read + Seek,
    R2: Read + Seek,
{
    validate_guarded_update_set(updates_by_sheet, status_values_only)?;
    source.seek(SeekFrom::Start(0))?;
    let source_discovery = ordered_workbook_discovery(source)?;
    let source_infos = project_sheet_infos(&source_discovery);
    let target_parts = updates_by_sheet
        .keys()
        .map(|name| {
            source_infos
                .get(name)
                .map(|info| info.path.clone())
                .ok_or_else(|| OoxmlError::invalid("guarded worksheet is missing"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;

    source.seek(SeekFrom::Start(0))?;
    destination.seek(SeekFrom::Start(0))?;
    let mut source_archive = open_archive(&mut *source)?;
    let mut destination_archive = open_archive(&mut *destination)?;
    if source_archive.len() != destination_archive.len() {
        return Err(OoxmlError::invalid("guarded package member roster changed"));
    }
    for index in 0..source_archive.len() {
        let source_name = source_archive.by_index(index)?.name().to_string();
        let destination_name = destination_archive.by_index(index)?.name().to_string();
        if source_name != destination_name {
            return Err(OoxmlError::invalid("guarded package member roster changed"));
        }
        let source_bytes = read_entry(&mut source_archive, &source_name)?;
        let destination_bytes = read_entry(&mut destination_archive, &destination_name)?;
        if target_parts.contains(&source_name) {
            let sheet_name = updates_by_sheet
                .keys()
                .find(|name| {
                    source_infos
                        .get(*name)
                        .is_some_and(|info| info.path == source_name)
                })
                .ok_or_else(|| OoxmlError::invalid("guarded worksheet is missing"))?;
            let updates = updates_by_sheet
                .get(sheet_name)
                .ok_or_else(|| OoxmlError::invalid("guarded updates are missing"))?;
            let source_tokens = guarded_sheet_tokens(&source_bytes, updates, false)?;
            let destination_tokens = guarded_sheet_tokens(&destination_bytes, updates, true)?;
            if destination_tokens != source_tokens {
                return Err(OoxmlError::invalid(
                    "guarded worksheet delta verification failed",
                ));
            }
        } else if destination_bytes != source_bytes {
            return Err(OoxmlError::invalid(
                "guarded non-target package member changed",
            ));
        }
    }

    drop(source_archive);
    drop(destination_archive);

    destination.seek(SeekFrom::Start(0))?;
    let destination_workbook = read_workbook_from_reader(destination)?;
    for (sheet_name, updates) in updates_by_sheet {
        let sheet = destination_workbook
            .sheet(sheet_name)
            .ok_or_else(|| OoxmlError::invalid("guarded worksheet is missing"))?;
        for (&coordinate, update) in updates {
            let cell = sheet
                .cells
                .get(&coordinate)
                .ok_or_else(|| OoxmlError::invalid("guarded target cell is missing"))?;
            let CellUpdate::Text(replacement) = &update.replacement else {
                return Err(OoxmlError::invalid("guarded replacement is invalid"));
            };
            if cell.value != *replacement
                || cell.formula.is_some()
                || cell.style != update.expected.style
            {
                return Err(OoxmlError::invalid(
                    "guarded target cell verification failed",
                ));
            }
        }
    }
    source.seek(SeekFrom::Start(0))?;
    destination.seek(SeekFrom::Start(0))?;
    Ok(())
}

fn validate_guarded_update_set(
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
    status_values_only: bool,
) -> Result<(), OoxmlError> {
    if updates_by_sheet.is_empty() || updates_by_sheet.values().any(|updates| updates.is_empty()) {
        return Err(OoxmlError::invalid(
            "at least one guarded cell update is required",
        ));
    }
    for updates in updates_by_sheet.values() {
        for (&(row, column), update) in updates {
            if row == 0 || column == 0 || column > MAX_WORKSHEET_COLUMNS {
                return Err(OoxmlError::invalid(
                    "guarded worksheet cell reference is invalid",
                ));
            }
            if !matches!(&update.replacement, CellUpdate::Text(_))
                || status_values_only
                    && !matches!(
                        &update.replacement,
                        CellUpdate::Text(value) if matches!(value.as_str(), "Signed" | "Not Signed")
                    )
                || update.create_if_absent
                    && (status_values_only || update.expected != CellData::default())
            {
                return Err(OoxmlError::invalid("guarded replacement is invalid"));
            }
        }
    }
    Ok(())
}

fn validate_guarded_source(
    workbook: &WorkbookData,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
) -> Result<(), OoxmlError> {
    if !workbook.merge_validation_complete() {
        return Err(OoxmlError::invalid(
            "guarded merge validation facts are incomplete",
        ));
    }
    let mut merge_budget =
        MergeComparisonBudget::for_apply(workbook.merge_validation_comparisons())?;
    for (sheet_name, updates) in updates_by_sheet {
        let sheet = workbook
            .sheet(sheet_name)
            .ok_or_else(|| OoxmlError::invalid("guarded worksheet is missing"))?;
        if sheet.has_sheet_protection {
            return Err(OoxmlError::invalid("guarded worksheet is protected"));
        }
        for (&coordinate, update) in updates {
            let current = sheet.cells.get(&coordinate);
            let absent_is_expected = update.create_if_absent
                && update.expected == CellData::default()
                && current.is_none()
                && sheet.cells.keys().any(|(row, _)| *row == coordinate.0);
            let present_is_expected = !update.create_if_absent
                && current.is_some_and(|cell| cell == &update.expected && cell.formula.is_none());
            if !absent_is_expected && !present_is_expected {
                return Err(OoxmlError::invalid(
                    "guarded target cell presence or value no longer matches its approved state",
                ));
            }
            for range in &sheet.merged_ranges {
                merge_budget.consume()?;
                if range.first_row <= coordinate.0
                    && coordinate.0 <= range.last_row
                    && range.first_column <= coordinate.1
                    && coordinate.1 <= range.last_column
                {
                    return Err(OoxmlError::invalid("guarded target cell is merged"));
                }
            }
        }
    }
    Ok(())
}

pub fn read_member(source: &Path, name: &str) -> Result<Vec<u8>, OoxmlError> {
    validate_source(source)?;
    let mut archive = open_archive(File::open(source)?)?;
    read_entry(&mut archive, name)
}

pub fn sheet_infos_from_path(source: &Path) -> Result<BTreeMap<String, SheetInfo>, OoxmlError> {
    validate_source(source)?;
    let mut input = File::open(source)?;
    let discovery = ordered_workbook_discovery(&mut input)?;
    Ok(project_sheet_infos(&discovery))
}

fn project_sheet_infos(discovery: &OrderedWorkbookDiscovery) -> BTreeMap<String, SheetInfo> {
    discovery
        .worksheets
        .iter()
        .map(|worksheet| {
            (
                worksheet.name.clone(),
                SheetInfo {
                    path: worksheet.part_path.clone(),
                    sheet_id: worksheet.sheet_id,
                },
            )
        })
        .collect()
}

pub fn ordered_workbook_discovery<R: Read + Seek>(
    source: &mut R,
) -> Result<OrderedWorkbookDiscovery, OoxmlError> {
    let mut archive = open_archive(source)?;
    let workbook_parts = authenticated_workbook_parts(&mut archive)?;
    let workbook = parse_document(&read_entry(&mut archive, &workbook_parts.workbook)?)?;
    let relationships = parse_document(&read_entry(&mut archive, &workbook_parts.relationships)?)?;
    validate_relationship_document_namespace(&relationships, "workbook")?;
    let relation_targets = relationship_targets(&relationships)?;
    let connection_count = related_collection_count(
        &mut archive,
        &relationships,
        &workbook_parts.workbook,
        "connections",
        b"connections",
        b"connection",
    )?;
    let mut safety = WorkbookSafetyFacts {
        has_vba_project: has_entry(&mut archive, "xl/vbaProject.bin")?,
        has_xlm_macro_sheet: has_relationship_type(&relationships, |relationship_type| {
            matches!(
                relationship_class(relationship_type),
                Some("xlMacrosheet" | "xlIntlMacrosheet")
            )
        })?,
        external_link_count: relationship_type_count(&relationships, "externalLink")?,
        connection_count,
        ..WorkbookSafetyFacts::default()
    };
    let mut worksheets = Vec::new();
    let mut names = BTreeSet::new();
    let mut sheet_ids = BTreeSet::new();
    let mut relationship_ids = BTreeSet::new();
    let mut part_paths = BTreeSet::new();
    let workbook_root = document_root_local(&workbook, b"workbook", "workbook root is invalid")?;
    if !has_expanded_name(workbook_root, b"workbook", &WORKBOOK_NAMESPACES) {
        return Err(OoxmlError::invalid("workbook namespace is invalid"));
    }
    safety.has_workbook_protection =
        child_elements_expanded(workbook_root, b"workbookProtection", &WORKBOOK_NAMESPACES)
            .next()
            .is_some();
    let sheets = single_child_element_expanded(
        workbook_root,
        b"sheets",
        &WORKBOOK_NAMESPACES,
        "workbook sheets container is invalid",
    )?;
    for (index, element) in
        child_elements_expanded(sheets, b"sheet", &WORKBOOK_NAMESPACES).enumerate()
    {
        if index >= MAX_WORKSHEETS {
            return Err(OoxmlError::limit_exceeded(
                "workbook has too many worksheets",
            ));
        }
        let name = bounded_unqualified_attribute(
            element,
            b"name",
            MAX_SHEET_NAME_BYTES,
            "workbook sheet name",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook sheet name is missing"))?;
        if !names.insert(name.clone()) {
            return Err(OoxmlError::invalid("duplicate workbook sheet name"));
        }
        let relationship_id = bounded_office_relationship_attribute(
            element,
            b"id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook sheet relationship identity",
        )?;
        if relationship_id.is_none()
            && bounded_qualified_attribute(
                element,
                b"r:id",
                MAX_RELATIONSHIP_ID_BYTES,
                "workbook sheet relationship identity",
            )?
            .is_some()
        {
            return Err(OoxmlError::invalid(
                "workbook relationship namespace is invalid",
            ));
        }
        let relationship_id = relationship_id
            .ok_or_else(|| OoxmlError::invalid("workbook sheet relationship is missing"))?;
        if !relationship_ids.insert(relationship_id.clone()) {
            return Err(OoxmlError::invalid(
                "duplicate workbook sheet relationship identity",
            ));
        }
        let sheet_id = bounded_unqualified_attribute(
            element,
            b"sheetId",
            MAX_U32_ATTRIBUTE_BYTES,
            "workbook sheet identity",
        )?
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| OoxmlError::invalid("workbook sheet identity is invalid"))?;
        if !sheet_ids.insert(sheet_id) {
            return Err(OoxmlError::invalid("duplicate workbook sheet identity"));
        }
        let target = relation_targets
            .get(&relationship_id)
            .ok_or_else(|| OoxmlError::invalid("workbook sheet target is missing"))?;
        if relationship_target_is_external(&relationships, &relationship_id)? {
            return Err(OoxmlError::invalid(
                "external workbook relationship target is not allowed",
            ));
        }
        let relationship_type = relationship_attribute(&relationships, &relationship_id, b"Type")?
            .ok_or_else(|| OoxmlError::invalid("workbook sheet relationship type is missing"))?;
        if ooxml_relationship_class(&relationship_type) != Some("worksheet") {
            return Err(OoxmlError::invalid(
                "workbook sheet relationship is not a worksheet",
            ));
        }
        let state = bounded_unqualified_attribute(
            element,
            b"state",
            MAX_STATE_BYTES,
            "worksheet visibility state",
        )?;
        let visibility = match state.as_deref() {
            None | Some("visible") => WorksheetVisibility::Visible,
            Some("hidden") => WorksheetVisibility::Hidden,
            Some("veryHidden") => WorksheetVisibility::VeryHidden,
            Some(_) => {
                return Err(OoxmlError::invalid("worksheet visibility state is invalid"));
            }
        };
        let part_path = normalize_workbook_relationship_target(&workbook_parts.workbook, target)?;
        if !part_paths.insert(part_path.clone()) {
            return Err(OoxmlError::invalid("duplicate worksheet part path"));
        }
        if !has_entry(&mut archive, &part_path)? {
            return Err(OoxmlError::invalid("worksheet package member is missing"));
        }
        worksheets.push(WorksheetDescriptor {
            workbook_index: u32::try_from(index + 1)
                .map_err(|_| OoxmlError::invalid("worksheet count is invalid"))?,
            sheet_id,
            relationship_id,
            relationship_type,
            part_path,
            name,
            visibility,
        });
    }
    if worksheets.is_empty() {
        return Err(OoxmlError::invalid("workbook contains no worksheets"));
    }
    safety.has_vba_project |= validate_workbook_relationship_classes(
        &mut archive,
        &relationships,
        &workbook_parts.workbook,
    )?;
    safety.query_table_count =
        worksheet_relationship_type_count(&mut archive, &worksheets, "queryTable")?;
    Ok(OrderedWorkbookDiscovery { worksheets, safety })
}

pub fn read_workbook(source: &Path) -> Result<WorkbookData, OoxmlError> {
    validate_source(source)?;
    let mut input = File::open(source)?;
    read_workbook_from_reader(&mut input)
}

pub fn read_workbook_and_discovery<R: Read + Seek>(
    source: &mut R,
) -> Result<(OrderedWorkbookDiscovery, WorkbookData), OoxmlError> {
    let discovery = ordered_workbook_discovery(source)?;
    let infos = project_sheet_infos(&discovery);
    source.seek(SeekFrom::Start(0))?;
    let mut archive = open_archive(source)?;
    let workbook_parts = authenticated_workbook_parts(&mut archive)?;
    let relationships = parse_document(&read_entry(&mut archive, &workbook_parts.relationships)?)?;
    let shared_strings = match internal_relationship_target_for_class(
        &relationships,
        &workbook_parts.workbook,
        "sharedStrings",
    )? {
        Some(part_path) => shared_strings(&read_entry(&mut archive, &part_path)?)?,
        None => Vec::new(),
    };
    let mut sheets = BTreeMap::new();
    let mut merge_budget = MergeComparisonBudget::for_validation();
    for (name, info) in infos {
        let raw = read_entry(&mut archive, &info.path)?;
        sheets.insert(
            name,
            worksheet_data(&raw, &shared_strings, &mut merge_budget)?,
        );
    }
    Ok((
        discovery,
        WorkbookData {
            sheets,
            merge_validation_comparisons: merge_budget.consumed,
            merge_validation_complete: true,
        },
    ))
}

pub fn read_workbook_from_reader<R: Read + Seek>(
    source: &mut R,
) -> Result<WorkbookData, OoxmlError> {
    read_workbook_and_discovery(source).map(|(_, data)| data)
}
pub fn read_workbook_selective(
    source: &Path,
    wanted_sheets: &[&str],
) -> Result<WorkbookData, OoxmlError> {
    validate_source(source)?;
    let mut input = File::open(source)?;
    read_workbook_selective_from_reader(&mut input, wanted_sheets)
}

pub fn read_workbook_selective_from_reader<R: Read + Seek>(
    source: &mut R,
    wanted_sheets: &[&str],
) -> Result<WorkbookData, OoxmlError> {
    let discovery = ordered_workbook_discovery(source)?;
    let infos = project_sheet_infos(&discovery);
    source.seek(SeekFrom::Start(0))?;
    let mut archive = open_archive(source)?;
    let workbook_parts = authenticated_workbook_parts(&mut archive)?;
    let relationships = parse_document(&read_entry(&mut archive, &workbook_parts.relationships)?)?;
    let shared_strings = match internal_relationship_target_for_class(
        &relationships,
        &workbook_parts.workbook,
        "sharedStrings",
    )? {
        Some(part_path) => shared_strings(&read_entry(&mut archive, &part_path)?)?,
        None => Vec::new(),
    };
    let mut sheets = BTreeMap::new();
    let mut merge_budget = MergeComparisonBudget::for_validation();
    for (name, info) in infos {
        if wanted_sheets
            .iter()
            .any(|wanted| wanted.eq_ignore_ascii_case(&name))
        {
            let raw = read_entry(&mut archive, &info.path)?;
            sheets.insert(
                name,
                worksheet_data(&raw, &shared_strings, &mut merge_budget)?,
            );
        }
    }
    Ok(WorkbookData {
        sheets,
        merge_validation_comparisons: merge_budget.consumed,
        merge_validation_complete: true,
    })
}

pub fn package_has_signature<R: Read + Seek>(source: &mut R) -> Result<bool, OoxmlError> {
    const TRANSITIONAL_SIGNATURE_ORIGIN: &str =
        "http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/origin";
    const STRICT_SIGNATURE_ORIGIN: &str =
        "http://purl.oclc.org/ooxml/package/relationships/digital-signature/origin";

    let mut archive = open_archive(source)?;
    let relationships = parse_document(&read_entry(&mut archive, "_rels/.rels")?)?;
    let root = validate_relationship_document_namespace(&relationships, "package")?;
    let mut signature_target = None;
    for relationship in relationship_elements(root) {
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "package relationship type",
        )?
        .ok_or_else(|| OoxmlError::invalid("package relationship type is missing"))?;
        if !matches!(
            relationship_type.as_str(),
            TRANSITIONAL_SIGNATURE_ORIGIN | STRICT_SIGNATURE_ORIGIN
        ) {
            continue;
        }
        if signature_target.is_some() {
            return Err(OoxmlError::invalid(
                "package signature relationship is ambiguous",
            ));
        }
        bounded_unqualified_attribute(
            relationship,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "package signature relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("package signature relationship identity is missing"))?;
        if relationship_target_mode_is_external(relationship, "package signature")? {
            return Err(OoxmlError::invalid(
                "external package signature relationship target is not allowed",
            ));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "package signature relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("package signature relationship target is missing"))?;
        let target = normalize_package_root_target(&target)?;
        if !is_signature_member(&target) {
            return Err(OoxmlError::invalid(
                "package signature relationship target is invalid",
            ));
        }
        signature_target = Some(target);
    }

    if let Some(target) = signature_target.as_deref()
        && !has_entry(&mut archive, target)?
    {
        return Err(OoxmlError::invalid(
            "package signature relationship target is missing",
        ));
    }
    let mut has_signature_member = false;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        has_signature_member |= is_signature_member(entry.name());
    }
    Ok(signature_target.is_some() || has_signature_member)
}

fn is_signature_member(name: &str) -> bool {
    name.split('/')
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case("_xmlsignatures"))
}

fn validate_source(source: &Path) -> Result<(), OoxmlError> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.file_type().is_file() {
        return Err(OoxmlError::invalid(
            "workbook package is not a regular file",
        ));
    }
    if metadata.len() > MAX_PACKAGE_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "workbook package exceeded the size limit",
        ));
    }
    Ok(())
}

fn open_archive<R: Read + Seek>(mut source: R) -> Result<ZipArchive<R>, OoxmlError> {
    let preflight = preflight_archive(&mut source)?;
    let mut archive = ZipArchive::new(source)?;
    if archive.len() != preflight.entry_count
        || archive.central_directory_start() != preflight.central_directory_start
    {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }
    validate_archive(&mut archive)?;
    Ok(archive)
}

fn reject_zip64_or_malformed_extra<R: Read + Seek>(
    source: &mut R,
    start: u64,
    end: u64,
) -> Result<(), OoxmlError> {
    const EXTRA_HEADER_BYTES: u64 = 4;
    const ZIP64_EXTRA_HEADER_ID: u16 = 0x0001;

    let mut cursor = start;
    while cursor < end {
        let header_end = cursor
            .checked_add(EXTRA_HEADER_BYTES)
            .filter(|header_end| *header_end <= end)
            .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
        source.seek(SeekFrom::Start(cursor))?;
        let mut header = [0_u8; EXTRA_HEADER_BYTES as usize];
        source.read_exact(&mut header)?;
        let header_id = u16::from_le_bytes([header[0], header[1]]);
        let data_len = u64::from(u16::from_le_bytes([header[2], header[3]]));
        if header_id == ZIP64_EXTRA_HEADER_ID {
            return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
        }
        cursor = header_end
            .checked_add(data_len)
            .filter(|data_end| *data_end <= end)
            .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
    }
    Ok(())
}

fn preflight_archive<R: Read + Seek>(source: &mut R) -> Result<ZipPreflightFacts, OoxmlError> {
    const EOCD_MIN_BYTES: usize = 22;
    const EOCD_MAX_COMMENT_BYTES: usize = u16::MAX as usize;
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
    const CENTRAL_DIRECTORY_HEADER_BYTES: u64 = 46;
    const CENTRAL_DIRECTORY_HEADER_SIGNATURE: &[u8; 4] = b"PK\x01\x02";

    let source_len = source.seek(SeekFrom::End(0))?;
    if source_len == 0 {
        return Err(OoxmlError::invalid("workbook source is empty"));
    }
    if source_len > MAX_PACKAGE_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "workbook source exceeded the size limit",
        ));
    }
    let tail_len = usize::try_from(source_len)
        .unwrap_or(usize::MAX)
        .min(EOCD_MIN_BYTES + EOCD_MAX_COMMENT_BYTES);
    source.seek(SeekFrom::Start(source_len - tail_len as u64))?;
    let mut tail = vec![0_u8; tail_len];
    source.read_exact(&mut tail)?;

    let eocd_offset = tail
        .windows(EOCD_SIGNATURE.len())
        .rposition(|window| window == EOCD_SIGNATURE)
        .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
    let eocd = tail
        .get(eocd_offset..)
        .filter(|bytes| bytes.len() >= EOCD_MIN_BYTES)
        .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
    let comment_len = usize::from(u16::from_le_bytes([eocd[20], eocd[21]]));
    if eocd_offset + EOCD_MIN_BYTES + comment_len != tail.len() {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }

    let disk_number = u16::from_le_bytes([eocd[4], eocd[5]]);
    let directory_disk = u16::from_le_bytes([eocd[6], eocd[7]]);
    let entries_on_disk = u16::from_le_bytes([eocd[8], eocd[9]]);
    let entry_count = u16::from_le_bytes([eocd[10], eocd[11]]);
    let directory_size = u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]);
    let directory_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]);
    if disk_number != 0 || directory_disk != 0 || entries_on_disk != entry_count {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }
    if usize::from(entry_count) > MAX_ENTRIES {
        return Err(OoxmlError::limit_exceeded(
            "workbook ZIP entry count is invalid",
        ));
    }
    if entry_count == u16::MAX || directory_size == u32::MAX || directory_offset == u32::MAX {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }
    let absolute_eocd_offset = source_len - tail_len as u64 + eocd_offset as u64;
    let directory_end = u64::from(directory_offset)
        .checked_add(u64::from(directory_size))
        .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
    if directory_end > absolute_eocd_offset {
        return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
    }
    if entry_count > 0 {
        let minimum_directory_size = u64::from(entry_count)
            .checked_mul(CENTRAL_DIRECTORY_HEADER_BYTES)
            .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
        if u64::from(directory_size) < minimum_directory_size
            || directory_end != absolute_eocd_offset
            || u64::from(directory_offset) >= absolute_eocd_offset
        {
            return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
        }
        let mut cursor = u64::from(directory_offset);
        for _ in 0..entry_count {
            let fixed_end = cursor
                .checked_add(CENTRAL_DIRECTORY_HEADER_BYTES)
                .filter(|end| *end <= directory_end)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            source.seek(SeekFrom::Start(cursor))?;
            let mut header = [0_u8; CENTRAL_DIRECTORY_HEADER_BYTES as usize];
            source.read_exact(&mut header)?;
            if &header[..4] != CENTRAL_DIRECTORY_HEADER_SIGNATURE {
                return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
            }
            let compressed_size =
                u32::from_le_bytes([header[20], header[21], header[22], header[23]]);
            let uncompressed_size =
                u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
            let file_name_len = u64::from(u16::from_le_bytes([header[28], header[29]]));
            let extra_field_len = u64::from(u16::from_le_bytes([header[30], header[31]]));
            let comment_len = u64::from(u16::from_le_bytes([header[32], header[33]]));
            let disk_number_start = u16::from_le_bytes([header[34], header[35]]);
            let local_header_offset =
                u32::from_le_bytes([header[42], header[43], header[44], header[45]]);
            if disk_number_start != 0
                || compressed_size == u32::MAX
                || uncompressed_size == u32::MAX
                || local_header_offset == u32::MAX
            {
                return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
            }
            let file_name_end = fixed_end
                .checked_add(file_name_len)
                .filter(|end| *end <= directory_end)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            let extra_field_end = file_name_end
                .checked_add(extra_field_len)
                .filter(|end| *end <= directory_end)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            cursor = extra_field_end
                .checked_add(comment_len)
                .filter(|end| *end <= directory_end)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            reject_zip64_or_malformed_extra(source, file_name_end, extra_field_end)?;

            let local_header_offset = u64::from(local_header_offset);
            let local_fixed_end = local_header_offset
                .checked_add(30)
                .filter(|end| *end <= u64::from(directory_offset) && *end <= source_len)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            source.seek(SeekFrom::Start(local_header_offset))?;
            let mut local_header = [0_u8; 30];
            source.read_exact(&mut local_header)?;
            if &local_header[..4] != b"PK\x03\x04" {
                return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
            }
            let local_compressed_size = u32::from_le_bytes([
                local_header[18],
                local_header[19],
                local_header[20],
                local_header[21],
            ]);
            let local_uncompressed_size = u32::from_le_bytes([
                local_header[22],
                local_header[23],
                local_header[24],
                local_header[25],
            ]);
            let local_file_name_len =
                u64::from(u16::from_le_bytes([local_header[26], local_header[27]]));
            let local_extra_field_len =
                u64::from(u16::from_le_bytes([local_header[28], local_header[29]]));
            if local_compressed_size == u32::MAX
                || local_uncompressed_size == u32::MAX
                || local_file_name_len != file_name_len
            {
                return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
            }
            let local_file_name_end = local_fixed_end
                .checked_add(local_file_name_len)
                .filter(|end| *end <= u64::from(directory_offset) && *end <= source_len)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            let local_extra_field_end = local_file_name_end
                .checked_add(local_extra_field_len)
                .filter(|end| *end <= u64::from(directory_offset) && *end <= source_len)
                .ok_or_else(|| OoxmlError::invalid("workbook ZIP metadata is invalid"))?;
            reject_zip64_or_malformed_extra(source, local_file_name_end, local_extra_field_end)?;
        }
        if cursor != directory_end {
            return Err(OoxmlError::invalid("workbook ZIP metadata is invalid"));
        }
    }
    source.seek(SeekFrom::Start(0))?;
    Ok(ZipPreflightFacts {
        entry_count: usize::from(entry_count),
        central_directory_start: u64::from(directory_offset),
    })
}

fn validate_archive<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<(), OoxmlError> {
    if archive.is_empty() {
        return Err(OoxmlError::invalid("workbook ZIP entry count is invalid"));
    }
    if archive.len() > MAX_ENTRIES {
        return Err(OoxmlError::limit_exceeded(
            "workbook ZIP entry count is invalid",
        ));
    }
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(OoxmlError::limit_exceeded(
                "workbook member exceeded the size limit",
            ));
        }
        if entry.encrypted()
            || entry.enclosed_name().is_none()
            || !names.insert(entry.name().to_string())
        {
            return Err(OoxmlError::invalid("workbook ZIP entry is invalid"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err(OoxmlError::limit_exceeded(
                "workbook uncompressed data exceeded the size limit",
            ));
        }
    }
    Ok(())
}

fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, OoxmlError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| OoxmlError::invalid(format!("workbook member not found: {name}")))?;
    if entry.size() > MAX_ENTRY_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "workbook member exceeded the size limit",
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    entry
        .by_ref()
        .take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "workbook member exceeded the size limit",
        ));
    }
    Ok(bytes)
}

fn has_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<bool, OoxmlError> {
    match archive.by_name(name) {
        Ok(_) => Ok(true),
        Err(zip::result::ZipError::FileNotFound) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn write_package(
    archive: &mut ZipArchive<File>,
    output: File,
    patched: &BTreeMap<String, Vec<u8>>,
) -> Result<(), OoxmlError> {
    let archive_comment = archive.comment().to_vec();
    let mut writer = ZipWriter::new(output);
    writer.set_raw_comment(archive_comment.into_boxed_slice())?;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if let Some(bytes) = patched.get(entry.name()) {
            let name = entry.name().to_string();
            let mut options = entry.options().into_full_options();
            if !entry.comment().is_empty() {
                options = options.with_file_comment(entry.comment());
            }
            writer.start_file(name, options)?;
            writer.write_all(bytes)?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    let output = writer.finish()?;
    output.sync_all()?;
    Ok(())
}

fn write_package_guarded<R, W>(
    archive: &mut ZipArchive<R>,
    output: &mut W,
    patched: &BTreeMap<String, Vec<u8>>,
) -> Result<(), OoxmlError>
where
    R: Read + Seek,
    W: Write + Seek,
{
    let archive_comment = archive.comment().to_vec();
    let limited = LimitedWriter::new(output, MAX_PACKAGE_BYTES)?;
    let mut writer = ZipWriter::new(limited);
    writer.set_raw_comment(archive_comment.into_boxed_slice())?;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if let Some(bytes) = patched.get(entry.name()) {
            let name = entry.name().to_string();
            let mut options = entry.options().into_full_options();
            if !entry.comment().is_empty() {
                options = options.with_file_comment(entry.comment());
            }
            writer.start_file(name, options)?;
            writer.write_all(bytes)?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    let mut output = writer.finish()?;
    output.flush()?;
    Ok(())
}

struct LimitedWriter<'a, W> {
    inner: &'a mut W,
    position: u64,
    limit: u64,
}

impl<'a, W: Seek> LimitedWriter<'a, W> {
    fn new(inner: &'a mut W, limit: u64) -> io::Result<Self> {
        let position = inner.stream_position()?;
        if position > limit {
            return Err(io::Error::other("guarded output exceeded the size limit"));
        }
        Ok(Self {
            inner,
            position,
            limit,
        })
    }
}

impl<W: Write> Write for LimitedWriter<'_, W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let requested = u64::try_from(buffer.len())
            .ok()
            .and_then(|length| self.position.checked_add(length))
            .filter(|end| *end <= self.limit)
            .ok_or_else(|| io::Error::other("guarded output exceeded the size limit"))?;
        let written = self.inner.write(buffer)?;
        self.position = self
            .position
            .checked_add(u64::try_from(written).unwrap_or(u64::MAX))
            .filter(|end| *end <= requested)
            .ok_or_else(|| io::Error::other("guarded output position is invalid"))?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Seek> Seek for LimitedWriter<'_, W> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let position = self.inner.seek(position)?;
        if position > self.limit {
            return Err(io::Error::other("guarded output exceeded the size limit"));
        }
        self.position = position;
        Ok(position)
    }
}

fn authenticated_workbook_parts<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<WorkbookPackageParts, OoxmlError> {
    let package_relationships = parse_document(&read_entry(archive, "_rels/.rels")?)?;
    let root = validate_relationship_document_namespace(&package_relationships, "package")?;
    let mut workbook_part = None;
    for relationship in relationship_elements(root) {
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "package relationship type",
        )?;
        let is_office_document = relationship_type.as_deref().is_some_and(|value| {
            matches!(
                value,
                TRANSITIONAL_OFFICE_DOCUMENT_RELATIONSHIP | STRICT_OFFICE_DOCUMENT_RELATIONSHIP
            )
        });
        if !is_office_document {
            continue;
        }
        if workbook_part.is_some() {
            return Err(OoxmlError::invalid(
                "package workbook relationship is ambiguous",
            ));
        }
        bounded_unqualified_attribute(
            relationship,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "package workbook relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("package workbook relationship identity is missing"))?;
        if relationship_target_mode_is_external(relationship, "package workbook")? {
            return Err(OoxmlError::invalid(
                "external package workbook relationship target is not allowed",
            ));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "package workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("package workbook relationship target is missing"))?;
        workbook_part = Some(normalize_package_root_target(&target)?);
    }
    let workbook = workbook_part
        .ok_or_else(|| OoxmlError::invalid("package workbook relationship is missing"))?;
    if !has_entry(archive, &workbook)? {
        return Err(OoxmlError::invalid("workbook package member is missing"));
    }
    let relationships = relationship_part_path(&workbook)?;
    if !has_entry(archive, &relationships)? {
        return Err(OoxmlError::invalid(
            "workbook relationships package member is missing",
        ));
    }
    Ok(WorkbookPackageParts {
        workbook,
        relationships,
    })
}

fn relationship_targets(document: &XmlDocument) -> Result<HashMap<String, String>, OoxmlError> {
    let mut targets = HashMap::new();
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        let id = bounded_unqualified_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship identity is missing"))?;
        let target = bounded_unqualified_attribute(
            element,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        if targets.insert(id, target).is_some() {
            return Err(OoxmlError::invalid(
                "duplicate workbook relationship identity",
            ));
        }
    }
    Ok(targets)
}

fn internal_relationship_target_for_class(
    document: &XmlDocument,
    source_part_path: &str,
    expected_class: &str,
) -> Result<Option<String>, OoxmlError> {
    let root = validate_relationship_document_namespace(document, "workbook")?;
    let mut target = None;
    for relationship in relationship_elements(root) {
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship type is missing"))?;
        if relationship_class(&relationship_type) != Some(expected_class) {
            continue;
        }
        if target.is_some() {
            return Err(OoxmlError::invalid(
                "workbook relationship class is ambiguous",
            ));
        }
        if relationship_target_mode_is_external(relationship, "workbook")? {
            return Err(OoxmlError::invalid(
                "external workbook relationship target is not allowed",
            ));
        }
        let raw_target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        target = Some(normalize_workbook_relationship_target(
            source_part_path,
            &raw_target,
        )?);
    }
    Ok(target)
}

fn relationship_class(relationship_type: &str) -> Option<&str> {
    ooxml_relationship_class(relationship_type).or_else(|| {
        let rest = relationship_type.strip_prefix("http://schemas.microsoft.com/office/")?;
        let rel_idx = rest.find("/relationships/")?;
        let class = &rest[rel_idx + "/relationships/".len()..];
        let class = class.strip_prefix("ui/").unwrap_or(class);
        (!class.is_empty() && !class.contains('/')).then_some(class)
    })
}

fn ooxml_relationship_class(relationship_type: &str) -> Option<&str> {
    let class = relationship_type
        .strip_prefix(TRANSITIONAL_RELATIONSHIP_PREFIX)
        .or_else(|| relationship_type.strip_prefix(STRICT_RELATIONSHIP_PREFIX))
        .or_else(|| {
            relationship_type
                .strip_prefix("http://schemas.openxmlformats.org/package/2006/relationships/")
        })
        .or_else(|| {
            relationship_type.strip_prefix("http://purl.oclc.org/ooxml/package/relationships/")
        })?;
    let class = class.strip_prefix("ui/").unwrap_or(class);
    (!class.is_empty() && !class.contains('/')).then_some(class)
}

fn validate_relationship_document_namespace<'a>(
    document: &'a XmlDocument,
    subject: &str,
) -> Result<&'a XmlElement, OoxmlError> {
    let root_error = format!("{subject} relationships root is invalid");
    let root = document_root_local(document, b"Relationships", &root_error)?;
    let field = format!("{subject} relationships namespace");
    if !has_expanded_name(root, b"Relationships", &PACKAGE_RELATIONSHIP_NAMESPACES) {
        return Err(OoxmlError::invalid(format!("{field} is invalid")));
    }
    Ok(root)
}

fn relationship_elements(root: &XmlElement) -> impl Iterator<Item = &XmlElement> {
    child_elements_expanded(root, b"Relationship", &PACKAGE_RELATIONSHIP_NAMESPACES)
}

fn validate_workbook_relationship_classes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    document: &XmlDocument,
    workbook_part_path: &str,
) -> Result<bool, OoxmlError> {
    const KNOWN_CLASSES: &[&str] = &[
        "worksheet",
        "styles",
        "theme",
        "sharedStrings",
        "calcChain",
        "externalLink",
        "connections",
        "vbaProject",
        "vbaProjectSignature",
        "xlMacrosheet",
        "xlIntlMacrosheet",
        "pivotCacheDefinition",
        "customXml",
        "customXmlMappings",
        "xmlMaps",
        "metadata",
        "sheetMetadata",
        "person",
        "richValueRel",
        "slicerCache",
        "timelineCache",
        "chartsheet",
        "dialogsheet",
        "table",
        "queryTable",
        "pivotTable",
        "printerSettings",
        "drawing",
        "vmlDrawing",
        "image",
        "rdRichValueTypes",
        "rdRichValueStructure",
        "rdArray",
        "model",
        "dataModel",
        "webextension",
        "feature",
        "features",
        "calcFeatures",
        "volatileDependencies",
        "customData",
        "customDataProperties",
        "customProperty",
        "customProperties",
        "extendedProperties",
        "package",
        "hyperlink",
        "customUI",
        "attachedToolbars",
        "activeXControlBinary",
        "usernames",
        "revisionLog",
        "revisionHeaders",
    ];

    let root = validate_relationship_document_namespace(document, "workbook")?;
    let mut relationship_targets = BTreeSet::new();
    let mut internal_targets = Vec::new();
    let mut has_vba_project = false;
    for relationship in relationship_elements(root) {
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship type is missing"))?;
        let Some(relationship_class) = relationship_class(&relationship_type) else {
            return Err(OoxmlError::invalid("unknown workbook relationship class"));
        };
        if !KNOWN_CLASSES.contains(&relationship_class) {
            return Err(OoxmlError::invalid("unknown workbook relationship class"));
        }
        if relationship_target_mode_is_external(relationship, "workbook")? {
            if relationship_class == "externalLink" || relationship_class == "hyperlink" {
                continue;
            }
            return Err(OoxmlError::invalid(
                "external workbook relationship target is not allowed",
            ));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        let normalized_target = normalize_relationship_target(workbook_part_path, &target)?;
        if !relationship_targets.insert(normalized_target.clone()) {
            return Err(OoxmlError::invalid(
                "duplicate workbook relationship target",
            ));
        }
        internal_targets.push((normalized_target, relationship_class == "vbaProject"));
        has_vba_project |= relationship_class == "vbaProject";
    }
    for (normalized_target, is_vba_project) in internal_targets {
        if !has_entry(archive, &normalized_target)? {
            let message = if is_vba_project {
                "VBA project package member is missing"
            } else {
                "workbook relationship package member is missing"
            };
            return Err(OoxmlError::invalid(message));
        }
    }
    Ok(has_vba_project)
}

fn validate_worksheet_relationship_classes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    document: &XmlDocument,
    worksheet_part_path: &str,
) -> Result<(), OoxmlError> {
    const KNOWN_CLASSES: &[&str] = &[
        "hyperlink",
        "drawing",
        "vmlDrawing",
        "comments",
        "threadedComment",
        "table",
        "pivotTable",
        "queryTable",
        "printerSettings",
        "oleObject",
        "ctrlProp",
        "image",
        "slicer",
        "timeline",
        "customXml",
        "metadata",
        "chart",
        "chartUserShapes",
        "diagramData",
        "diagramLayout",
        "diagramStyle",
        "diagramColors",
        "package",
        "webextension",
        "customProperty",
        "feature",
    ];

    let root = validate_relationship_document_namespace(document, "worksheet")?;
    let mut relationship_ids = BTreeSet::new();
    let mut relationship_targets = BTreeSet::new();

    for relationship in relationship_elements(root) {
        let relationship_id = bounded_unqualified_attribute(
            relationship,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "worksheet relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("worksheet relationship identity is missing"))?;
        if !relationship_ids.insert(relationship_id) {
            return Err(OoxmlError::invalid(
                "duplicate worksheet relationship identity",
            ));
        }
        let relationship_type = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "worksheet relationship type",
        )?
        .ok_or_else(|| OoxmlError::invalid("worksheet relationship type is missing"))?;
        let Some(relationship_class) = relationship_class(&relationship_type) else {
            return Err(OoxmlError::invalid("unknown worksheet relationship class"));
        };
        if !KNOWN_CLASSES.contains(&relationship_class) {
            return Err(OoxmlError::invalid("unknown worksheet relationship class"));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "worksheet relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("worksheet relationship target is missing"))?;
        if target.is_empty() {
            return Err(OoxmlError::invalid(
                "worksheet relationship target is missing",
            ));
        }
        if relationship_target_mode_is_external(relationship, "worksheet")? {
            return Err(OoxmlError::invalid(
                "external worksheet relationship target is not allowed",
            ));
        }
        let normalized_target = normalize_relationship_target(worksheet_part_path, &target)?;
        if !relationship_targets.insert(normalized_target.clone()) {
            return Err(OoxmlError::invalid(
                "duplicate worksheet relationship target",
            ));
        }
    }
    for normalized_target in relationship_targets {
        if !has_entry(archive, &normalized_target)? {
            return Err(OoxmlError::invalid(
                "worksheet relationship package member is missing",
            ));
        }
    }
    Ok(())
}

fn relationship_target_is_external(
    document: &XmlDocument,
    relationship_id: &str,
) -> Result<bool, OoxmlError> {
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        let id = bounded_unqualified_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?;
        if id.as_deref() == Some(relationship_id) {
            return relationship_target_mode_is_external(element, "workbook");
        }
    }
    Ok(false)
}

fn relationship_target_mode_is_external(
    relationship: &XmlElement,
    subject: &str,
) -> Result<bool, OoxmlError> {
    let context = format!("{subject} relationship target mode");
    match bounded_unqualified_attribute(relationship, b"TargetMode", MAX_STATE_BYTES, &context)?
        .as_deref()
    {
        None | Some("Internal") => Ok(false),
        Some("External") => Ok(true),
        Some(_) => Err(OoxmlError::invalid(format!(
            "{subject} relationship target mode is invalid"
        ))),
    }
}

fn relationship_attribute(
    document: &XmlDocument,
    relationship_id: &str,
    name: &[u8],
) -> Result<Option<String>, OoxmlError> {
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        let id = bounded_unqualified_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?;
        if id.as_deref() == Some(relationship_id) {
            return bounded_unqualified_attribute(
                element,
                name,
                MAX_RELATIONSHIP_TYPE_BYTES,
                "workbook relationship type",
            );
        }
    }
    Ok(None)
}

fn has_relationship_type(
    document: &XmlDocument,
    predicate: impl Fn(&str) -> bool,
) -> Result<bool, OoxmlError> {
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        if bounded_unqualified_attribute(
            element,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        .is_some_and(|relationship_type| predicate(&relationship_type))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn relationship_type_count(
    document: &XmlDocument,
    expected_class: &str,
) -> Result<u32, OoxmlError> {
    let mut count = 0_usize;
    let root = validate_relationship_document_namespace(document, "workbook")?;
    for element in relationship_elements(root) {
        if bounded_unqualified_attribute(
            element,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        .as_deref()
        .and_then(relationship_class)
            == Some(expected_class)
        {
            count += 1;
        }
    }
    u32::try_from(count).map_err(|_| OoxmlError::invalid("relationship count is invalid"))
}

fn related_collection_count<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    relationships: &XmlDocument,
    workbook_part_path: &str,
    expected_class: &str,
    collection_name: &[u8],
    item_name: &[u8],
) -> Result<u32, OoxmlError> {
    let mut count = 0_u32;
    let root = validate_relationship_document_namespace(relationships, "workbook")?;
    for relationship in relationship_elements(root) {
        let Some(relationship_type) = bounded_unqualified_attribute(
            relationship,
            b"Type",
            MAX_RELATIONSHIP_TYPE_BYTES,
            "workbook relationship type",
        )?
        else {
            continue;
        };
        if relationship_class(&relationship_type) != Some(expected_class) {
            continue;
        }
        if relationship_target_mode_is_external(relationship, "workbook")? {
            return Err(OoxmlError::invalid(
                "external workbook relationship target is not allowed",
            ));
        }
        let target = bounded_unqualified_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        let part_path = normalize_workbook_relationship_target(workbook_part_path, &target)?;
        let document = parse_document(&read_entry(archive, &part_path)?)?;
        let collection =
            document_root_local(&document, collection_name, "relationship count is invalid")?;
        if !has_expanded_name(collection, collection_name, &WORKBOOK_NAMESPACES) {
            return Err(OoxmlError::invalid("relationship count is invalid"));
        }
        let part_count =
            child_elements_expanded(collection, item_name, &WORKBOOK_NAMESPACES).count();
        if let Some(declared) = bounded_unqualified_attribute(
            collection,
            b"count",
            MAX_U32_ATTRIBUTE_BYTES,
            "relationship count",
        )? {
            let declared = declared
                .parse::<usize>()
                .map_err(|_| OoxmlError::invalid("relationship count is invalid"))?;
            if declared != part_count {
                return Err(OoxmlError::invalid("relationship count is invalid"));
            }
        }
        count = count
            .checked_add(
                u32::try_from(part_count)
                    .map_err(|_| OoxmlError::invalid("relationship count is invalid"))?,
            )
            .ok_or_else(|| OoxmlError::invalid("relationship count is invalid"))?;
    }
    Ok(count)
}

fn worksheet_relationship_type_count<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheets: &[WorksheetDescriptor],
    expected_class: &str,
) -> Result<u32, OoxmlError> {
    let mut count = 0_u32;
    for worksheet in worksheets {
        let relationships_path = relationship_part_path(&worksheet.part_path)?;
        if !has_entry(archive, &relationships_path)? {
            continue;
        }
        let relationships = parse_document(&read_entry(archive, &relationships_path)?)?;
        validate_worksheet_relationship_classes(archive, &relationships, &worksheet.part_path)?;
        count = count
            .checked_add(relationship_type_count(&relationships, expected_class)?)
            .ok_or_else(|| OoxmlError::invalid("relationship count is invalid"))?;
    }
    Ok(count)
}

fn normalize_package_root_target(target: &str) -> Result<String, OoxmlError> {
    let decoded = percent_decode_target(target)?.replace('\\', "/");
    if decoded.split('/').any(|segment| segment == "..") {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    normalize_relationship_target_from_parts(Vec::new(), target)
}

fn normalize_workbook_relationship_target(
    workbook_part_path: &str,
    target: &str,
) -> Result<String, OoxmlError> {
    let decoded = percent_decode_target(target)?.replace('\\', "/");
    if decoded.split('/').any(|segment| segment == "..") {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    normalize_relationship_target(workbook_part_path, target)
}

fn normalize_relationship_target(
    source_part_path: &str,
    target: &str,
) -> Result<String, OoxmlError> {
    let mut source_parts = source_part_path
        .split('/')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if source_parts.is_empty()
        || source_parts
            .iter()
            .any(|part| part.is_empty() || matches!(part.as_str(), "." | ".."))
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    source_parts.pop();
    normalize_relationship_target_from_parts(source_parts, target)
}

fn normalize_relationship_target_from_parts(
    mut source_parts: Vec<String>,
    target: &str,
) -> Result<String, OoxmlError> {
    if target.is_empty()
        || target.starts_with("//")
        || target.contains(['?', '#', '\0'])
        || target.contains(':')
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    let target = percent_decode_target(target)?.replace('\\', "/");
    if target.is_empty()
        || target.starts_with("//")
        || target.contains(['?', '#', '\0'])
        || target.contains(':')
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    let absolute = target.starts_with('/');
    let mut parts = if absolute {
        Vec::new()
    } else {
        std::mem::take(&mut source_parts)
    };

    for component in target.trim_start_matches('/').split('/') {
        match component {
            "" => {
                return Err(OoxmlError::invalid(
                    "worksheet relationship escapes the package",
                ));
            }
            "." => {}
            ".." => {
                if parts.is_empty() {
                    return Err(OoxmlError::invalid(
                        "worksheet relationship escapes the package",
                    ));
                }
                parts.pop();
            }
            part => parts.push(part.to_string()),
        }
    }
    if parts.is_empty() {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    Ok(parts.join("/"))
}

fn relationship_part_path(part_path: &str) -> Result<String, OoxmlError> {
    let part = Path::new(part_path);
    let file_name = part
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| OoxmlError::invalid("relationship source part path is invalid"))?;
    let parent = part.parent().and_then(Path::to_str).unwrap_or_default();
    if parent.is_empty() {
        Ok(format!("_rels/{file_name}.rels"))
    } else {
        Ok(format!("{parent}/_rels/{file_name}.rels"))
    }
}

fn percent_decode_target(target: &str) -> Result<String, OoxmlError> {
    let bytes = target.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let encoded = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| OoxmlError::invalid("worksheet relationship target is invalid"))?;
            let hex = std::str::from_utf8(encoded)
                .ok()
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .ok_or_else(|| OoxmlError::invalid("worksheet relationship target is invalid"))?;
            decoded.push(hex);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|_| OoxmlError::invalid("worksheet relationship target is invalid"))
}

fn patch_sheet_xml(
    raw: &[u8],
    updates: &BTreeMap<(u32, u32), CellUpdate>,
    duplicate_column: Option<u32>,
) -> Result<(Vec<u8>, BTreeSet<String>), OoxmlError> {
    let mut document = parse_document(raw)?;
    let sheet_data = find_element_mut(&mut document.nodes, b"sheetData")
        .ok_or_else(|| OoxmlError::invalid("worksheet sheetData is missing"))?;
    let mut replaced_formulas = BTreeSet::new();
    for (&(row_number, column_number), update) in updates {
        if row_number == 0 || column_number == 0 || column_number > 16_384 {
            return Err(OoxmlError::invalid("worksheet cell reference is invalid"));
        }
        let style = nearest_style(sheet_data, row_number, column_number);
        let row = ensure_row(sheet_data, row_number)?;
        let cell = ensure_cell(row, row_number, column_number, style.as_deref())?;
        if !matches!(update, CellUpdate::Formula { .. }) && child_element(cell, b"f").is_some() {
            replaced_formulas.insert(cell_reference(row_number, column_number));
        }
        write_update(cell, update);
    }
    if let Some(column) = duplicate_column {
        patch_duplicate_validation(&mut document.nodes, column)?;
    }
    Ok((serialize_document(&document)?, replaced_formulas))
}

fn patch_sheet_xml_guarded(
    raw: &[u8],
    updates: &BTreeMap<(u32, u32), GuardedCellUpdate>,
    status_values_only: bool,
) -> Result<Vec<u8>, OoxmlError> {
    let mut document = parse_document(raw)?;
    let root = document_root_mut_local(
        &mut document,
        b"worksheet",
        "guarded worksheet root is invalid",
    )?;
    let namespace = root
        .namespace
        .clone()
        .filter(|namespace| {
            matches!(
                namespace.as_slice(),
                value
                    if value == TRANSITIONAL_WORKBOOK_NAMESPACE.as_bytes()
                        || value == STRICT_WORKBOOK_NAMESPACE.as_bytes()
            )
        })
        .ok_or_else(|| OoxmlError::invalid("guarded worksheet namespace is invalid"))?;
    let prefix = root
        .start
        .name()
        .as_ref()
        .split(|byte| *byte == b':')
        .next()
        .filter(|_| root.start.name().as_ref().contains(&b':'))
        .map(|prefix| String::from_utf8_lossy(prefix).into_owned());
    let sheet_data = single_child_element_in_namespace_mut(
        root,
        b"sheetData",
        &namespace,
        "guarded worksheet sheetData is invalid",
    )?;

    for (&(row_number, column_number), update) in updates {
        let row = unique_direct_child_by_attribute_mut(
            sheet_data,
            b"row",
            &namespace,
            b"r",
            &row_number.to_string(),
            "guarded worksheet row is missing or ambiguous",
        )?;
        let coordinate = cell_reference(row_number, column_number);
        let cell = match optional_unique_direct_child_by_attribute_mut(
            row,
            b"c",
            &namespace,
            b"r",
            &coordinate,
            "guarded worksheet cell is ambiguous",
        )? {
            Some(cell) if !update.create_if_absent => cell,
            None if update.create_if_absent => ensure_guarded_cell(
                row,
                row_number,
                column_number,
                &namespace,
                prefix.as_deref(),
            )?,
            _ => {
                return Err(OoxmlError::invalid(
                    "guarded worksheet cell presence changed",
                ));
            }
        };
        patch_guarded_cell_value(
            cell,
            &namespace,
            prefix.as_deref(),
            &update.replacement,
            status_values_only,
        )?;
    }
    serialize_document(&document)
}

fn patch_guarded_cell_value(
    cell: &mut XmlElement,
    namespace: &[u8],
    prefix: Option<&str>,
    update: &CellUpdate,
    status_values_only: bool,
) -> Result<(), OoxmlError> {
    let CellUpdate::Text(value) = update else {
        return Err(OoxmlError::invalid("guarded replacement is invalid"));
    };
    if status_values_only && !matches!(value.as_str(), "Signed" | "Not Signed") {
        return Err(OoxmlError::invalid("guarded replacement is invalid"));
    }
    if child_elements_in_namespace(cell, b"f", namespace)
        .next()
        .is_some()
    {
        return Err(OoxmlError::invalid(
            "guarded target cell contains a formula",
        ));
    }

    let mut representation_indices = cell
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace)
                    && (has_local_name(child, b"v") || has_local_name(child, b"is")) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if representation_indices.len() > 1 {
        return Err(OoxmlError::invalid(
            "guarded target cell value representation is ambiguous",
        ));
    }
    let insert_at = representation_indices.first().copied().unwrap_or_else(|| {
        cell.children
            .iter()
            .position(|node| {
                matches!(
                    node,
                    XmlNode::Element(child)
                        if child.namespace.as_deref() == Some(namespace)
                            && has_local_name(child, b"extLst")
                )
            })
            .unwrap_or(cell.children.len())
    });
    for index in representation_indices.drain(..).rev() {
        cell.children.remove(index);
    }

    set_unqualified_attribute(cell, b"t", "inlineStr");
    let qualify = |local: &str| match prefix {
        Some(prefix) => format!("{prefix}:{local}"),
        None => local.to_string(),
    };
    let mut inline = XmlElement::new(&qualify("is"));
    inline.namespace = Some(namespace.to_vec());
    let mut text = XmlElement::new(&qualify("t"));
    text.namespace = Some(namespace.to_vec());
    if value.trim() != value {
        set_attribute(&mut text, b"xml:space", "preserve");
    }
    set_element_text(&mut text, value);
    inline.children.push(XmlNode::Element(text));
    cell.children.insert(insert_at, XmlNode::Element(inline));
    cell.empty = false;
    Ok(())
}

fn guarded_sheet_tokens(
    raw: &[u8],
    updates: &BTreeMap<(u32, u32), GuardedCellUpdate>,
    output: bool,
) -> Result<Vec<CanonicalXmlEvent>, OoxmlError> {
    let mut document = parse_document(raw)?;
    let root = document_root_mut_local(
        &mut document,
        b"worksheet",
        "guarded worksheet root is invalid",
    )?;
    let namespace = root
        .namespace
        .clone()
        .filter(|namespace| {
            matches!(
                namespace.as_slice(),
                value
                    if value == TRANSITIONAL_WORKBOOK_NAMESPACE.as_bytes()
                        || value == STRICT_WORKBOOK_NAMESPACE.as_bytes()
            )
        })
        .ok_or_else(|| OoxmlError::invalid("guarded worksheet namespace is invalid"))?;
    let prefix = root
        .start
        .name()
        .as_ref()
        .split(|byte| *byte == b':')
        .next()
        .filter(|_| root.start.name().as_ref().contains(&b':'))
        .map(|prefix| String::from_utf8_lossy(prefix).into_owned());
    let sheet_data = single_child_element_in_namespace_mut(
        root,
        b"sheetData",
        &namespace,
        "guarded worksheet sheetData is invalid",
    )?;
    for (&(row_number, column_number), update) in updates {
        let row = unique_direct_child_by_attribute_mut(
            sheet_data,
            b"row",
            &namespace,
            b"r",
            &row_number.to_string(),
            "guarded worksheet row is missing or ambiguous",
        )?;
        let coordinate = cell_reference(row_number, column_number);
        let cell = match optional_unique_direct_child_by_attribute_mut(
            row,
            b"c",
            &namespace,
            b"r",
            &coordinate,
            "guarded worksheet cell is ambiguous",
        )? {
            Some(cell) if !update.create_if_absent || output => cell,
            None if update.create_if_absent && !output => ensure_guarded_cell(
                row,
                row_number,
                column_number,
                &namespace,
                prefix.as_deref(),
            )?,
            _ => {
                return Err(OoxmlError::invalid(
                    "guarded worksheet cell presence changed",
                ));
            }
        };
        normalize_guarded_cell_for_tokens(cell, &namespace, &coordinate, update, output)?;
    }

    let mut tokens = Vec::new();
    canonical_xml_tokens(&document.nodes, &mut tokens)?;
    Ok(tokens)
}

fn normalize_guarded_cell_for_tokens(
    cell: &mut XmlElement,
    namespace: &[u8],
    coordinate: &str,
    update: &GuardedCellUpdate,
    output: bool,
) -> Result<(), OoxmlError> {
    let CellUpdate::Text(replacement) = &update.replacement else {
        return Err(OoxmlError::invalid("guarded replacement is invalid"));
    };
    if output {
        validate_exact_guarded_output_value(cell, namespace, replacement)?;
    } else if child_elements_in_namespace(cell, b"f", namespace)
        .next()
        .is_some()
    {
        return Err(OoxmlError::invalid(
            "guarded source cell contains a formula",
        ));
    }

    cell.attributes.retain(|attribute| {
        !(attribute.namespace.is_none() && attribute.local_name.as_slice() == b"t")
    });
    let representation_indices = cell
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace)
                    && (has_local_name(child, b"v") || has_local_name(child, b"is")) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if representation_indices.len() > 1 {
        return Err(OoxmlError::invalid(
            "guarded cell value representation is ambiguous",
        ));
    }
    let insert_at = representation_indices.first().copied().unwrap_or_else(|| {
        cell.children
            .iter()
            .position(|node| {
                matches!(
                    node,
                    XmlNode::Element(child)
                        if child.namespace.as_deref() == Some(namespace)
                            && has_local_name(child, b"extLst")
                )
            })
            .unwrap_or(cell.children.len())
    });
    for index in representation_indices.into_iter().rev() {
        cell.children.remove(index);
    }
    let marker = format!("docflow-guarded:{coordinate}:{replacement}");
    cell.children.insert(
        insert_at,
        XmlNode::Raw(Event::Text(BytesText::new(&marker).into_owned())),
    );
    cell.empty = false;
    Ok(())
}

fn validate_exact_guarded_output_value(
    cell: &XmlElement,
    namespace: &[u8],
    expected: &str,
) -> Result<(), OoxmlError> {
    if bounded_unqualified_attribute(cell, b"t", 32, "guarded cell type")?.as_deref()
        != Some("inlineStr")
    {
        return Err(OoxmlError::invalid("guarded output cell type is invalid"));
    }
    if child_elements_in_namespace(cell, b"f", namespace)
        .next()
        .is_some()
        || child_elements_in_namespace(cell, b"v", namespace)
            .next()
            .is_some()
    {
        return Err(OoxmlError::invalid(
            "guarded output value representation is invalid",
        ));
    }
    let inline = single_child_element_in_namespace(
        cell,
        b"is",
        namespace,
        "guarded output inline string is invalid",
    )?;
    if !inline.attributes.is_empty() || inline.children.len() != 1 {
        return Err(OoxmlError::invalid(
            "guarded output inline string is invalid",
        ));
    }
    let XmlNode::Element(text) = &inline.children[0] else {
        return Err(OoxmlError::invalid(
            "guarded output inline string is invalid",
        ));
    };
    if text.namespace.as_deref() != Some(namespace) || !has_local_name(text, b"t") {
        return Err(OoxmlError::invalid(
            "guarded output inline string is invalid",
        ));
    }
    let allows_space = expected.trim() != expected;
    let attributes_are_exact = if allows_space {
        text.attributes.len() == 1
            && text.attributes[0].namespace.as_deref()
                == Some(b"http://www.w3.org/XML/1998/namespace")
            && text.attributes[0].local_name.as_slice() == b"space"
            && text.attributes[0].value.as_slice() == b"preserve"
    } else {
        text.attributes.is_empty()
    };
    if !attributes_are_exact {
        return Err(OoxmlError::invalid(
            "guarded output inline string attributes are invalid",
        ));
    }
    let decoded = match text.children.as_slice() {
        [] if expected.is_empty() => String::new(),
        [XmlNode::Raw(Event::Text(value))] => value
            .decode()
            .map_err(|_| OoxmlError::invalid("guarded output inline string text is invalid"))?
            .into_owned(),
        _ => {
            return Err(OoxmlError::invalid(
                "guarded output inline string text is invalid",
            ));
        }
    };
    if decode_entities(&decoded) != expected {
        return Err(OoxmlError::invalid(
            "guarded output inline string text is invalid",
        ));
    }
    Ok(())
}

fn canonical_xml_tokens(
    nodes: &[XmlNode],
    output: &mut Vec<CanonicalXmlEvent>,
) -> Result<(), OoxmlError> {
    for node in nodes {
        match node {
            XmlNode::Element(element) => {
                let local_name = element
                    .start
                    .name()
                    .as_ref()
                    .rsplit(|byte| *byte == b':')
                    .next()
                    .unwrap_or_default()
                    .to_vec();
                let attributes = element
                    .attributes
                    .iter()
                    .filter(|attribute| {
                        attribute.local_name.as_slice() != b"xmlns"
                            && attribute.namespace.as_deref()
                                != Some(b"http://www.w3.org/2000/xmlns/")
                    })
                    .map(|attribute| {
                        let raw = std::str::from_utf8(&attribute.value)
                            .map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?;
                        let value = unescape(raw)
                            .map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?
                            .into_owned();
                        Ok((
                            attribute.namespace.clone(),
                            attribute.local_name.clone(),
                            value,
                        ))
                    })
                    .collect::<Result<Vec<_>, OoxmlError>>()?;
                if element.empty && element.children.is_empty() {
                    output.push(CanonicalXmlEvent::Empty {
                        namespace: element.namespace.clone(),
                        local_name,
                        attributes,
                    });
                } else {
                    output.push(CanonicalXmlEvent::Start {
                        namespace: element.namespace.clone(),
                        local_name: local_name.clone(),
                        attributes,
                    });
                    canonical_xml_tokens(&element.children, output)?;
                    output.push(CanonicalXmlEvent::End {
                        namespace: element.namespace.clone(),
                        local_name,
                    });
                }
            }
            XmlNode::Raw(event) => {
                let kind = match event {
                    Event::Text(_) => "text",
                    Event::CData(_) => "cdata",
                    Event::Comment(_) => "comment",
                    Event::Decl(_) => "decl",
                    Event::PI(_) => "pi",
                    Event::DocType(_) => "doctype",
                    Event::GeneralRef(_) => "general_ref",
                    Event::Eof => "eof",
                    Event::Start(_) | Event::End(_) | Event::Empty(_) => {
                        return Err(OoxmlError::invalid("XML event model is invalid"));
                    }
                };
                let value = match event {
                    Event::Text(text) => text
                        .decode()
                        .map(|value| decode_entities(&value).into_bytes())
                        .map_err(|_| OoxmlError::invalid("XML text is invalid"))?,
                    _ => event.as_ref().to_vec(),
                };
                output.push(CanonicalXmlEvent::Raw { kind, value });
            }
        }
    }
    Ok(())
}

fn unique_direct_child_by_attribute_mut<'a>(
    parent: &'a mut XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    attribute_name: &[u8],
    attribute_value: &str,
    error: &str,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let matches = parent
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace)
                    && has_local_name(child, local_name) =>
            {
                Some((index, attribute(child, attribute_name)))
            }
            _ => None,
        })
        .filter(|(_, value)| value.as_deref() == Some(attribute_value))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(OoxmlError::invalid(error));
    }
    match &mut parent.children[matches[0]] {
        XmlNode::Element(element) => Ok(element),
        XmlNode::Raw(_) => Err(OoxmlError::invalid(error)),
    }
}

fn optional_unique_direct_child_by_attribute_mut<'a>(
    parent: &'a mut XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    attribute_name: &[u8],
    attribute_value: &str,
    error: &str,
) -> Result<Option<&'a mut XmlElement>, OoxmlError> {
    let matches = parent
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child)
                if child.namespace.as_deref() == Some(namespace)
                    && has_local_name(child, local_name)
                    && attribute(child, attribute_name).as_deref() == Some(attribute_value) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [index] => match &mut parent.children[*index] {
            XmlNode::Element(element) => Ok(Some(element)),
            XmlNode::Raw(_) => Err(OoxmlError::invalid(error)),
        },
        _ => Err(OoxmlError::invalid(error)),
    }
}

fn shared_strings(raw: &[u8]) -> Result<Vec<String>, OoxmlError> {
    let document = parse_document(raw)?;
    let root = document_root_local(&document, b"sst", "shared strings root is invalid")?;
    let namespace = root
        .namespace
        .as_deref()
        .filter(|namespace| {
            matches!(
                *namespace,
                value
                    if value == TRANSITIONAL_WORKBOOK_NAMESPACE.as_bytes()
                        || value == STRICT_WORKBOOK_NAMESPACE.as_bytes()
            )
        })
        .ok_or_else(|| OoxmlError::invalid("shared strings namespace is invalid"))?;
    let mut strings = Vec::new();
    let mut canonical_items = BTreeSet::new();
    for item in child_elements_in_namespace(root, b"si", namespace) {
        canonical_items.insert(item as *const XmlElement as usize);
        strings.push(canonical_inline_string(item, namespace)?);
    }
    for item in descendants(&document.nodes).filter(|element| has_local_name(element, b"si")) {
        if item.namespace.as_deref() != Some(namespace)
            || !canonical_items.contains(&(item as *const XmlElement as usize))
        {
            return Err(OoxmlError::invalid(
                "shared string is outside the canonical hierarchy",
            ));
        }
    }
    Ok(strings)
}

fn worksheet_data(
    raw: &[u8],
    shared_strings: &[String],
    merge_budget: &mut MergeComparisonBudget,
) -> Result<WorksheetData, OoxmlError> {
    let document = parse_document(raw)?;
    let root = document_root_local(&document, b"worksheet", "worksheet root is invalid")?;
    let namespace = root
        .namespace
        .as_deref()
        .filter(|namespace| {
            matches!(
                *namespace,
                value
                    if value == TRANSITIONAL_WORKBOOK_NAMESPACE.as_bytes()
                        || value == STRICT_WORKBOOK_NAMESPACE.as_bytes()
            )
        })
        .ok_or_else(|| OoxmlError::invalid("worksheet namespace is invalid"))?;
    let sheet_data = single_child_element_in_namespace(
        root,
        b"sheetData",
        namespace,
        "worksheet sheetData is invalid",
    )?;
    let mut data = WorksheetData {
        has_sheet_protection: optional_single_child_element_in_namespace(
            root,
            b"sheetProtection",
            namespace,
            "worksheet protection is invalid",
        )?
        .is_some(),
        merged_ranges: worksheet_merged_ranges(root, namespace, merge_budget)?,
        ..WorksheetData::default()
    };

    for columns in child_elements_in_namespace(root, b"cols", namespace) {
        for column in child_elements_in_namespace(columns, b"col", namespace) {
            let first = bounded_unqualified_attribute(
                column,
                b"min",
                MAX_U32_ATTRIBUTE_BYTES,
                "worksheet column minimum",
            )?
            .and_then(|value| value.parse::<u32>().ok());
            let last = bounded_unqualified_attribute(
                column,
                b"max",
                MAX_U32_ATTRIBUTE_BYTES,
                "worksheet column maximum",
            )?
            .and_then(|value| value.parse::<u32>().ok());
            let width =
                bounded_unqualified_attribute(column, b"width", 64, "worksheet column width")?
                    .and_then(|value| value.parse::<f64>().ok());
            let (Some(first), Some(last), Some(width)) = (first, last, width) else {
                continue;
            };
            if first == 0
                || first > last
                || last > MAX_WORKSHEET_COLUMNS
                || !width.is_finite()
                || width < 0.0
            {
                return Err(OoxmlError::invalid("worksheet column is invalid"));
            }
            for column in first..=last {
                data.column_widths.insert(column, width);
            }
        }
    }

    let mut canonical_cells = BTreeSet::new();
    let mut row_numbers = BTreeSet::new();
    let mut coordinates = BTreeSet::new();
    for row in child_elements_in_namespace(sheet_data, b"row", namespace) {
        let row_number = bounded_unqualified_attribute(
            row,
            b"r",
            MAX_U32_ATTRIBUTE_BYTES,
            "worksheet row identity",
        )?
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|row| (1..=MAX_WORKSHEET_ROWS).contains(row))
        .ok_or_else(|| OoxmlError::invalid("worksheet row identity is invalid"))?;
        if !row_numbers.insert(row_number) {
            return Err(OoxmlError::invalid("duplicate worksheet row identity"));
        }
        data.max_row = data.max_row.max(row_number);
        for cell in child_elements_in_namespace(row, b"c", namespace) {
            canonical_cells.insert(cell as *const XmlElement as usize);
            reject_noncanonical_cell_value_shadows(cell, namespace)?;
            let reference =
                bounded_unqualified_attribute(cell, b"r", 32, "worksheet cell reference")?
                    .and_then(|value| parse_reference(&value))
                    .filter(|(cell_row, column)| {
                        *cell_row == row_number
                            && (1..=MAX_WORKSHEET_ROWS).contains(cell_row)
                            && (1..=MAX_WORKSHEET_COLUMNS).contains(column)
                    })
                    .ok_or_else(|| OoxmlError::invalid("worksheet cell reference is invalid"))?;
            if !coordinates.insert(reference) {
                return Err(OoxmlError::invalid("duplicate worksheet cell reference"));
            }
            let cell_type = bounded_unqualified_attribute(cell, b"t", 32, "worksheet cell type")?
                .unwrap_or_default();
            let raw_value = optional_single_child_element_in_namespace(
                cell,
                b"v",
                namespace,
                "worksheet cell value is invalid",
            )?
            .map(element_text)
            .unwrap_or_default();
            let inline = optional_single_child_element_in_namespace(
                cell,
                b"is",
                namespace,
                "worksheet inline string is invalid",
            )?;
            let value = match cell_type.as_str() {
                "inlineStr" => inline
                    .map(|inline| canonical_inline_string(inline, namespace))
                    .transpose()?
                    .unwrap_or_default(),
                "s" => {
                    if raw_value.is_empty() {
                        String::new()
                    } else {
                        raw_value
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| shared_strings.get(index))
                            .cloned()
                            .ok_or_else(|| OoxmlError::invalid("shared string index is invalid"))?
                    }
                }
                _ => raw_value,
            };
            let formula = optional_single_child_element_in_namespace(
                cell,
                b"f",
                namespace,
                "worksheet cell formula is invalid",
            )?
            .map(element_text);
            let style = bounded_unqualified_attribute(
                cell,
                b"s",
                MAX_U32_ATTRIBUTE_BYTES,
                "worksheet cell style",
            )?;
            data.max_row = data.max_row.max(reference.0);
            data.max_column = data.max_column.max(reference.1);
            data.cells.insert(
                reference,
                CellData {
                    value,
                    formula,
                    style,
                },
            );
        }
    }

    for element in descendants(&document.nodes).filter(|element| has_local_name(element, b"c")) {
        if element.namespace.as_deref() == Some(namespace) {
            if !canonical_cells.contains(&(element as *const XmlElement as usize)) {
                return Err(OoxmlError::invalid(
                    "worksheet cell is outside the canonical hierarchy",
                ));
            }
        } else if bounded_unqualified_attribute(
            element,
            b"r",
            32,
            "foreign worksheet cell reference",
        )?
        .and_then(|value| parse_reference(&value))
        .is_some()
        {
            return Err(OoxmlError::invalid(
                "foreign namespace worksheet cell shadow is invalid",
            ));
        }
    }
    Ok(data)
}

fn child_elements_in_namespace<'a>(
    element: &'a XmlElement,
    local_name: &'a [u8],
    namespace: &'a [u8],
) -> impl Iterator<Item = &'a XmlElement> {
    element.children.iter().filter_map(move |node| match node {
        XmlNode::Element(child)
            if has_local_name(child, local_name)
                && child.namespace.as_deref() == Some(namespace) =>
        {
            Some(child)
        }
        _ => None,
    })
}

fn optional_single_child_element_in_namespace<'a>(
    parent: &'a XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    error: &str,
) -> Result<Option<&'a XmlElement>, OoxmlError> {
    let mut found = None;
    for node in &parent.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        if !has_local_name(child, local_name) {
            continue;
        }
        if child.namespace.as_deref() != Some(namespace) || found.is_some() {
            return Err(OoxmlError::invalid(error));
        }
        found = Some(child);
    }
    Ok(found)
}

fn single_child_element_in_namespace<'a>(
    parent: &'a XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    error: &str,
) -> Result<&'a XmlElement, OoxmlError> {
    optional_single_child_element_in_namespace(parent, local_name, namespace, error)?
        .ok_or_else(|| OoxmlError::invalid(error))
}

fn single_child_element_in_namespace_mut<'a>(
    parent: &'a mut XmlElement,
    local_name: &[u8],
    namespace: &[u8],
    error: &str,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let matches = parent
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(child) if has_local_name(child, local_name) => {
                if child.namespace.as_deref() == Some(namespace) {
                    Some(Ok(index))
                } else {
                    Some(Err(OoxmlError::invalid(error)))
                }
            }
            _ => None,
        })
        .collect::<Result<Vec<_>, _>>()?;
    if matches.len() != 1 {
        return Err(OoxmlError::invalid(error));
    }
    match &mut parent.children[matches[0]] {
        XmlNode::Element(element) => Ok(element),
        XmlNode::Raw(_) => Err(OoxmlError::invalid(error)),
    }
}

fn canonical_inline_string(element: &XmlElement, namespace: &[u8]) -> Result<String, OoxmlError> {
    let mut text = String::new();
    for node in &element.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        require_namespace(
            child,
            namespace,
            "worksheet inline string namespace is invalid",
        )?;
        if has_local_name(child, b"t") {
            text.push_str(&element_text(child));
        } else if has_local_name(child, b"r") {
            append_canonical_rich_text(child, namespace, &mut text)?;
        } else if has_local_name(child, b"rPh") || has_local_name(child, b"phoneticPr") {
            validate_descendant_namespace(child, namespace)?;
        } else {
            return Err(OoxmlError::invalid(
                "worksheet inline string hierarchy is invalid",
            ));
        }
    }
    Ok(text)
}

fn append_canonical_rich_text(
    element: &XmlElement,
    namespace: &[u8],
    output: &mut String,
) -> Result<(), OoxmlError> {
    for node in &element.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        require_namespace(
            child,
            namespace,
            "worksheet inline string namespace is invalid",
        )?;
        if has_local_name(child, b"t") {
            output.push_str(&element_text(child));
        } else if has_local_name(child, b"rPr") {
            validate_descendant_namespace(child, namespace)?;
        } else {
            return Err(OoxmlError::invalid(
                "worksheet rich text hierarchy is invalid",
            ));
        }
    }
    Ok(())
}

fn require_namespace(
    element: &XmlElement,
    namespace: &[u8],
    error: &str,
) -> Result<(), OoxmlError> {
    if element.namespace.as_deref() == Some(namespace) {
        Ok(())
    } else {
        Err(OoxmlError::invalid(error))
    }
}

fn validate_descendant_namespace(element: &XmlElement, namespace: &[u8]) -> Result<(), OoxmlError> {
    for node in &element.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        require_namespace(child, namespace, "worksheet rich text namespace is invalid")?;
        validate_descendant_namespace(child, namespace)?;
    }
    Ok(())
}

fn reject_noncanonical_cell_value_shadows(
    cell: &XmlElement,
    namespace: &[u8],
) -> Result<(), OoxmlError> {
    for node in &cell.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        if [b"f".as_slice(), b"v".as_slice(), b"is".as_slice()]
            .iter()
            .any(|name| has_local_name(child, name))
        {
            require_namespace(
                child,
                namespace,
                "worksheet cell value namespace is invalid",
            )?;
        }
        reject_nested_cell_value_shadow(child)?;
    }
    Ok(())
}

fn reject_nested_cell_value_shadow(element: &XmlElement) -> Result<(), OoxmlError> {
    for node in &element.children {
        let XmlNode::Element(child) = node else {
            continue;
        };
        if [b"f".as_slice(), b"v".as_slice(), b"is".as_slice()]
            .iter()
            .any(|name| has_local_name(child, name))
        {
            return Err(OoxmlError::invalid(
                "worksheet cell value is outside the canonical hierarchy",
            ));
        }
        reject_nested_cell_value_shadow(child)?;
    }
    Ok(())
}

fn worksheet_merged_ranges(
    root: &XmlElement,
    namespace: &[u8],
    merge_budget: &mut MergeComparisonBudget,
) -> Result<Vec<CellRange>, OoxmlError> {
    let Some(merge_cells) = optional_single_child_element_in_namespace(
        root,
        b"mergeCells",
        namespace,
        "worksheet merge collection is invalid",
    )?
    else {
        return Ok(Vec::new());
    };
    let mut ranges = Vec::<CellRange>::new();
    for node in &merge_cells.children {
        let XmlNode::Element(element) = node else {
            continue;
        };
        if !has_local_name(element, b"mergeCell") {
            continue;
        }
        if element.namespace.as_deref() != Some(namespace) {
            return Err(OoxmlError::invalid("worksheet merge range is invalid"));
        }
        if ranges.len() >= MAX_MERGED_RANGES {
            return Err(OoxmlError::limit_exceeded(
                "worksheet merge range count exceeded the limit",
            ));
        }
        let reference =
            bounded_unqualified_attribute(element, b"ref", 64, "worksheet merge range")?
                .ok_or_else(|| OoxmlError::invalid("worksheet merge range is invalid"))?;
        let (first, last) = reference
            .split_once(':')
            .and_then(|(first, last)| Some((parse_reference(first)?, parse_reference(last)?)))
            .ok_or_else(|| OoxmlError::invalid("worksheet merge range is invalid"))?;
        let range = CellRange {
            first_row: first.0,
            first_column: first.1,
            last_row: last.0,
            last_column: last.1,
        };
        if range.first_row == 0
            || range.first_column == 0
            || range.first_row > range.last_row
            || range.first_column > range.last_column
            || range.last_row > MAX_WORKSHEET_ROWS
            || range.last_column > MAX_WORKSHEET_COLUMNS
        {
            return Err(OoxmlError::invalid("worksheet merge range is invalid"));
        }
        if ranges.contains(&range) {
            continue;
        }
        for existing in &ranges {
            merge_budget.consume()?;
            if existing.first_row <= range.last_row
                && range.first_row <= existing.last_row
                && existing.first_column <= range.last_column
                && range.first_column <= existing.last_column
            {
                return Err(OoxmlError::invalid("worksheet merge range is invalid"));
            }
        }
        ranges.push(range);
    }
    if let Some(declared) = bounded_unqualified_attribute(
        merge_cells,
        b"count",
        MAX_U32_ATTRIBUTE_BYTES,
        "worksheet merge count",
    )? {
        let declared = declared
            .parse::<usize>()
            .map_err(|_| OoxmlError::invalid("worksheet merge count is invalid"))?;
        if declared > MAX_MERGED_RANGES {
            return Err(OoxmlError::limit_exceeded(
                "worksheet merge range count exceeded the limit",
            ));
        }
    }
    Ok(ranges)
}

fn patch_calc_chain(
    document: &mut XmlDocument,
    replaced_by_sheet: &[(u32, BTreeSet<String>)],
) -> Result<(), OoxmlError> {
    let root = document
        .nodes
        .iter_mut()
        .find_map(|node| match node {
            XmlNode::Element(element) if has_local_name(element, b"calcChain") => Some(element),
            _ => None,
        })
        .ok_or_else(|| OoxmlError::invalid("calcChain root is missing"))?;
    let removed = replaced_by_sheet
        .iter()
        .map(|(sheet, references)| (*sheet, references))
        .collect::<HashMap<_, _>>();
    let mut current_sheet = None;
    let mut last_kept_sheet = None;
    root.children.retain_mut(|node| {
        let XmlNode::Element(cell) = node else {
            return true;
        };
        if !has_local_name(cell, b"c") {
            return true;
        }
        if let Some(sheet) = attribute(cell, b"i").and_then(|value| value.parse::<u32>().ok()) {
            current_sheet = Some(sheet);
        }
        let reference = attribute(cell, b"r").unwrap_or_default();
        if current_sheet
            .and_then(|sheet| removed.get(&sheet))
            .is_some_and(|references| references.contains(&reference))
        {
            return false;
        }
        if let Some(sheet) = current_sheet
            && current_sheet != last_kept_sheet
            && attribute(cell, b"i").is_none()
        {
            set_attribute(cell, b"i", &sheet.to_string());
        }
        last_kept_sheet = current_sheet;
        true
    });
    Ok(())
}

fn patch_duplicate_validation(nodes: &mut [XmlNode], column_number: u32) -> Result<(), OoxmlError> {
    let column = column_name(column_number)?;
    let Some(validations) = find_element_mut(nodes, b"dataValidations") else {
        return Ok(());
    };
    for validation in child_elements_mut(validations, b"dataValidation") {
        if attribute(validation, b"type").as_deref() != Some("custom") {
            continue;
        }
        let reference = attribute(validation, b"sqref");
        let Some(formula) = child_element_mut(validation, b"formula1") else {
            continue;
        };
        if !element_text(formula)
            .to_ascii_uppercase()
            .contains("COUNTIF")
        {
            continue;
        }
        let Some(reference) = reference else {
            continue;
        };
        let start = reference.split_whitespace().find_map(|range| {
            let (left, right) = range.split_once(':')?;
            let (left_column, left_row) = split_reference(left)?;
            let (right_column, _) = split_reference(right)?;
            (left_column.eq_ignore_ascii_case(&column)
                && right_column.eq_ignore_ascii_case(&column))
            .then_some(left_row)
        });
        let Some(start_row) = start else {
            continue;
        };
        let previous = start_row.saturating_sub(1).max(1);
        set_element_text(
            formula,
            &format!(
                "OR({column}{start_row}=\"\",COUNTIF(${column}${start_row}:{column}{start_row},{column}{start_row})=1,{column}{start_row}={column}{previous})"
            ),
        );
        break;
    }
    Ok(())
}

fn nearest_style(sheet_data: &XmlElement, row_number: u32, column_number: u32) -> Option<String> {
    let mut best = None::<(u32, String)>;
    for row in child_elements(sheet_data, b"row") {
        let Some(current_row) = attribute(row, b"r").and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        for cell in child_elements(row, b"c") {
            let Some(reference) = attribute(cell, b"r") else {
                continue;
            };
            let Some((_, current_column)) = parse_reference(&reference) else {
                continue;
            };
            let Some(style) = attribute(cell, b"s") else {
                continue;
            };
            if current_column == column_number {
                let distance = current_row.abs_diff(row_number);
                if best
                    .as_ref()
                    .is_none_or(|(best_distance, _)| distance < *best_distance)
                {
                    best = Some((distance, style));
                }
            }
        }
    }
    best.map(|(_, style)| style)
}

fn ensure_row(sheet_data: &mut XmlElement, row_number: u32) -> Result<&mut XmlElement, OoxmlError> {
    let mut insert_at = sheet_data.children.len();
    for (index, node) in sheet_data.children.iter().enumerate() {
        let XmlNode::Element(row) = node else {
            continue;
        };
        if !has_local_name(row, b"row") {
            continue;
        }
        let current = attribute(row, b"r")
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| OoxmlError::invalid("worksheet row identity is invalid"))?;
        if current == row_number {
            insert_at = index;
            break;
        }
        if current > row_number {
            insert_at = index;
            break;
        }
    }
    let is_match = sheet_data.children.get(insert_at).is_some_and(|node| {
        matches!(node, XmlNode::Element(row) if has_local_name(row, b"row") && attribute(row, b"r").as_deref() == Some(&row_number.to_string()))
    });
    if !is_match {
        let mut row = XmlElement::new("row");
        set_attribute(&mut row, b"r", &row_number.to_string());
        sheet_data.children.insert(insert_at, XmlNode::Element(row));
    }
    match &mut sheet_data.children[insert_at] {
        XmlNode::Element(row) => Ok(row),
        XmlNode::Raw(_) => Err(OoxmlError::invalid("worksheet row insertion failed")),
    }
}

fn ensure_cell<'a>(
    row: &'a mut XmlElement,
    row_number: u32,
    column_number: u32,
    style: Option<&str>,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let reference = cell_reference(row_number, column_number);
    let mut insert_at = row.children.len();
    for (index, node) in row.children.iter().enumerate() {
        let XmlNode::Element(cell) = node else {
            continue;
        };
        if !has_local_name(cell, b"c") {
            continue;
        }
        let current = attribute(cell, b"r")
            .and_then(|value| parse_reference(&value))
            .ok_or_else(|| OoxmlError::invalid("worksheet cell reference is invalid"))?;
        if current.1 == column_number {
            insert_at = index;
            break;
        }
        if current.1 > column_number {
            insert_at = index;
            break;
        }
    }
    let is_match = row.children.get(insert_at).is_some_and(|node| {
        matches!(node, XmlNode::Element(cell) if has_local_name(cell, b"c") && attribute(cell, b"r").as_deref() == Some(reference.as_str()))
    });
    if !is_match {
        let mut cell = XmlElement::new("c");
        set_attribute(&mut cell, b"r", &reference);
        if let Some(style) = style {
            set_attribute(&mut cell, b"s", style);
        }
        row.children.insert(insert_at, XmlNode::Element(cell));
    }
    match &mut row.children[insert_at] {
        XmlNode::Element(cell) => Ok(cell),
        XmlNode::Raw(_) => Err(OoxmlError::invalid("worksheet cell insertion failed")),
    }
}

fn ensure_guarded_cell<'a>(
    row: &'a mut XmlElement,
    row_number: u32,
    column_number: u32,
    namespace: &[u8],
    prefix: Option<&str>,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let coordinate = cell_reference(row_number, column_number);
    let cell = ensure_cell(row, row_number, column_number, None)?;
    cell.namespace = Some(namespace.to_vec());
    let qualified_name = prefix
        .map(|prefix| format!("{prefix}:c"))
        .unwrap_or_else(|| "c".to_string());
    cell.start.set_name(qualified_name.as_bytes());
    cell.attributes = vec![ExpandedAttribute {
        namespace: None,
        local_name: b"r".to_vec(),
        value: coordinate.into_bytes(),
    }];
    Ok(cell)
}

fn write_update(cell: &mut XmlElement, update: &CellUpdate) {
    let existing_formula = child_element(cell, b"f").cloned();
    cell.children.clear();
    remove_attribute(cell, b"t");
    match update {
        CellUpdate::Text(value) if value.is_empty() => {}
        CellUpdate::Text(value) => {
            set_attribute(cell, b"t", "inlineStr");
            let mut inline = XmlElement::new("is");
            let mut text = XmlElement::new("t");
            if value.trim() != value {
                set_attribute(&mut text, b"xml:space", "preserve");
            }
            set_element_text(&mut text, value);
            inline.children.push(XmlNode::Element(text));
            cell.children.push(XmlNode::Element(inline));
        }
        CellUpdate::Number(value) => {
            cell.children
                .push(XmlNode::Element(text_element("v", value)));
        }
        CellUpdate::Bool(value) => {
            set_attribute(cell, b"t", "b");
            cell.children.push(XmlNode::Element(text_element(
                "v",
                if *value { "1" } else { "0" },
            )));
        }
        CellUpdate::Formula { formula, cached } => {
            let mut formula_element = existing_formula.unwrap_or_else(|| XmlElement::new("f"));
            set_element_text(&mut formula_element, formula.trim_start_matches('='));
            cell.children.push(XmlNode::Element(formula_element));
            if let Some(cached) = cached {
                set_attribute(cell, b"t", "str");
                cell.children
                    .push(XmlNode::Element(text_element("v", cached)));
            }
        }
    }
}

fn text_element(name: &str, value: &str) -> XmlElement {
    let mut element = XmlElement::new(name);
    set_element_text(&mut element, value);
    element
}

fn parse_document(raw: &[u8]) -> Result<XmlDocument, OoxmlError> {
    parse_document_with_limits(raw, MAX_XML_DEPTH, MAX_XML_NODES)
}

fn parse_document_with_limits(
    raw: &[u8],
    max_depth: usize,
    max_nodes: usize,
) -> Result<XmlDocument, OoxmlError> {
    if raw.len() as u64 > MAX_ENTRY_BYTES {
        return Err(OoxmlError::limit_exceeded(
            "XML member exceeded the size limit",
        ));
    }
    let mut reader = NsReader::from_reader(raw);
    reader.config_mut().trim_text(false);
    let mut roots = Vec::<XmlNode>::new();
    let mut stack = Vec::<XmlElement>::new();
    let mut node_count = 0_usize;
    loop {
        match reader.read_event()? {
            Event::Start(start) => {
                record_xml_node(&mut node_count, max_nodes)?;
                if stack.len() >= max_depth {
                    return Err(OoxmlError::limit_exceeded(
                        "XML element depth exceeded the limit",
                    ));
                }
                validate_raw_qualified_attribute_names(&start)?;
                let namespace =
                    expanded_namespace(reader.resolver().resolve_element(start.name()).0)?;
                let attributes = expanded_attributes(reader.resolver(), &start)?;
                stack.push(XmlElement {
                    start: start.into_owned(),
                    namespace,
                    attributes,
                    children: Vec::new(),
                    empty: false,
                });
            }
            Event::Empty(start) => {
                record_xml_node(&mut node_count, max_nodes)?;
                if stack.len() >= max_depth {
                    return Err(OoxmlError::limit_exceeded(
                        "XML element depth exceeded the limit",
                    ));
                }
                validate_raw_qualified_attribute_names(&start)?;
                let namespace =
                    expanded_namespace(reader.resolver().resolve_element(start.name()).0)?;
                let attributes = expanded_attributes(reader.resolver(), &start)?;
                push_node(
                    &mut roots,
                    &mut stack,
                    XmlNode::Element(XmlElement {
                        start: start.into_owned(),
                        namespace,
                        attributes,
                        children: Vec::new(),
                        empty: true,
                    }),
                );
            }
            Event::End(end) => {
                let element = stack
                    .pop()
                    .ok_or_else(|| OoxmlError::invalid("XML element stack is invalid"))?;
                if element.start.name().as_ref() != end.name().as_ref() {
                    return Err(OoxmlError::invalid("XML closing tag is invalid"));
                }
                push_node(&mut roots, &mut stack, XmlNode::Element(element));
            }
            Event::Eof => break,
            event => {
                record_xml_node(&mut node_count, max_nodes)?;
                push_node(&mut roots, &mut stack, XmlNode::Raw(event.into_owned()));
            }
        }
    }
    if !stack.is_empty() || !roots.iter().any(|node| matches!(node, XmlNode::Element(_))) {
        return Err(OoxmlError::invalid("XML document is incomplete"));
    }
    Ok(XmlDocument { nodes: roots })
}

fn validate_raw_qualified_attribute_names(start: &BytesStart<'_>) -> Result<(), OoxmlError> {
    for attribute in start.attributes() {
        attribute.map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?;
    }
    Ok(())
}

fn expanded_namespace(namespace: ResolveResult<'_>) -> Result<Option<Vec<u8>>, OoxmlError> {
    match namespace {
        ResolveResult::Unbound => Ok(None),
        ResolveResult::Bound(namespace) => Ok(Some(namespace.as_ref().to_vec())),
        ResolveResult::Unknown(prefix) => {
            let mut unresolved = b"\0unresolved:".to_vec();
            unresolved.extend(prefix);
            Ok(Some(unresolved))
        }
    }
}

fn expanded_attributes(
    resolver: &NamespaceResolver,
    start: &BytesStart<'_>,
) -> Result<Vec<ExpandedAttribute>, OoxmlError> {
    start
        .attributes()
        .with_checks(false)
        .map(|attribute| {
            let attribute =
                attribute.map_err(|_| OoxmlError::invalid("XML attribute is invalid"))?;
            let (namespace, local_name) = resolver.resolve_attribute(attribute.key);
            Ok(ExpandedAttribute {
                namespace: expanded_namespace(namespace)?,
                local_name: local_name.as_ref().to_vec(),
                value: attribute.value.into_owned(),
            })
        })
        .collect()
}

fn record_xml_node(node_count: &mut usize, max_nodes: usize) -> Result<(), OoxmlError> {
    *node_count = node_count
        .checked_add(1)
        .ok_or_else(|| OoxmlError::limit_exceeded("XML node count exceeded the limit"))?;
    if *node_count > max_nodes {
        return Err(OoxmlError::limit_exceeded(
            "XML node count exceeded the limit",
        ));
    }
    Ok(())
}

fn push_node(roots: &mut Vec<XmlNode>, stack: &mut [XmlElement], node: XmlNode) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

fn serialize_document(document: &XmlDocument) -> Result<Vec<u8>, OoxmlError> {
    let mut writer = Writer::new(Vec::new());
    for node in &document.nodes {
        write_node(&mut writer, node)?;
    }
    Ok(writer.into_inner())
}

fn write_node(writer: &mut Writer<Vec<u8>>, node: &XmlNode) -> Result<(), OoxmlError> {
    match node {
        XmlNode::Raw(event) => writer.write_event(event.clone())?,
        XmlNode::Element(element) if element.empty && element.children.is_empty() => {
            writer.write_event(Event::Empty(element.start.clone()))?;
        }
        XmlNode::Element(element) => {
            writer.write_event(Event::Start(element.start.clone()))?;
            for child in &element.children {
                write_node(writer, child)?;
            }
            let name = String::from_utf8_lossy(element.start.name().as_ref()).into_owned();
            writer.write_event(Event::End(BytesEnd::new(name)))?;
        }
    }
    Ok(())
}

impl XmlElement {
    fn new(name: &str) -> Self {
        Self {
            start: BytesStart::new(name).into_owned(),
            namespace: None,
            attributes: Vec::new(),
            children: Vec::new(),
            empty: false,
        }
    }
}

fn descendants(nodes: &[XmlNode]) -> impl Iterator<Item = &XmlElement> {
    nodes.iter().flat_map(|node| {
        let mut values = Vec::new();
        collect_descendants(node, &mut values);
        values
    })
}

fn document_root_local<'a>(
    document: &'a XmlDocument,
    local_name: &[u8],
    error: &str,
) -> Result<&'a XmlElement, OoxmlError> {
    let mut roots = document.nodes.iter().filter_map(|node| match node {
        XmlNode::Element(element) => Some(element),
        XmlNode::Raw(_) => None,
    });
    let root = roots
        .next()
        .filter(|element| has_local_name(element, local_name))
        .ok_or_else(|| OoxmlError::invalid(error))?;
    if roots.next().is_some() {
        return Err(OoxmlError::invalid(error));
    }
    Ok(root)
}

fn document_root_mut_local<'a>(
    document: &'a mut XmlDocument,
    local_name: &[u8],
    error: &str,
) -> Result<&'a mut XmlElement, OoxmlError> {
    let roots = document
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            XmlNode::Element(element) if has_local_name(element, local_name) => Some(index),
            XmlNode::Element(_) => Some(usize::MAX),
            XmlNode::Raw(_) => None,
        })
        .collect::<Vec<_>>();
    if roots.len() != 1 || roots[0] == usize::MAX {
        return Err(OoxmlError::invalid(error));
    }
    match &mut document.nodes[roots[0]] {
        XmlNode::Element(root) => Ok(root),
        XmlNode::Raw(_) => Err(OoxmlError::invalid(error)),
    }
}

fn single_child_element_expanded<'a>(
    parent: &'a XmlElement,
    local_name: &'a [u8],
    namespaces: &'a [&'a str],
    error: &str,
) -> Result<&'a XmlElement, OoxmlError> {
    let mut matches = child_elements_expanded(parent, local_name, namespaces);
    let child = matches.next().ok_or_else(|| OoxmlError::invalid(error))?;
    if matches.next().is_some() {
        return Err(OoxmlError::invalid(error));
    }
    Ok(child)
}

fn collect_descendants<'a>(node: &'a XmlNode, output: &mut Vec<&'a XmlElement>) {
    if let XmlNode::Element(element) = node {
        output.push(element);
        for child in &element.children {
            collect_descendants(child, output);
        }
    }
}

fn find_element_mut<'a>(nodes: &'a mut [XmlNode], name: &[u8]) -> Option<&'a mut XmlElement> {
    for node in nodes {
        let XmlNode::Element(element) = node else {
            continue;
        };
        if has_local_name(element, name) {
            return Some(element);
        }
        if let Some(found) = find_element_mut(&mut element.children, name) {
            return Some(found);
        }
    }
    None
}

fn child_elements<'a>(
    element: &'a XmlElement,
    name: &'a [u8],
) -> impl Iterator<Item = &'a XmlElement> {
    element.children.iter().filter_map(move |node| match node {
        XmlNode::Element(child) if has_local_name(child, name) => Some(child),
        _ => None,
    })
}

fn child_elements_expanded<'a>(
    element: &'a XmlElement,
    local_name: &'a [u8],
    namespaces: &'a [&'a str],
) -> impl Iterator<Item = &'a XmlElement> {
    element.children.iter().filter_map(move |node| match node {
        XmlNode::Element(child) if has_expanded_name(child, local_name, namespaces) => Some(child),
        _ => None,
    })
}

fn child_elements_mut<'a>(
    element: &'a mut XmlElement,
    name: &'a [u8],
) -> impl Iterator<Item = &'a mut XmlElement> {
    element
        .children
        .iter_mut()
        .filter_map(move |node| match node {
            XmlNode::Element(child) if has_local_name(child, name) => Some(child),
            _ => None,
        })
}

fn child_element<'a>(element: &'a XmlElement, name: &'a [u8]) -> Option<&'a XmlElement> {
    child_elements(element, name).next()
}

fn child_element_mut<'a>(
    element: &'a mut XmlElement,
    name: &'a [u8],
) -> Option<&'a mut XmlElement> {
    child_elements_mut(element, name).next()
}

fn has_local_name(element: &XmlElement, expected: &[u8]) -> bool {
    let qualified = element.start.name();
    qualified
        .as_ref()
        .rsplit(|byte| *byte == b':')
        .next()
        .is_some_and(|name| name == expected)
}

fn has_expanded_name(element: &XmlElement, local_name: &[u8], namespaces: &[&str]) -> bool {
    has_local_name(element, local_name)
        && element.namespace.as_deref().is_some_and(|namespace| {
            namespaces
                .iter()
                .any(|expected| namespace == expected.as_bytes())
        })
}

fn attribute(element: &XmlElement, local: &[u8]) -> Option<String> {
    element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .find(|attribute| {
            attribute
                .key
                .as_ref()
                .rsplit(|byte| *byte == b':')
                .next()
                .is_some_and(|name| name == local)
        })
        .map(|attribute| String::from_utf8_lossy(attribute.value.as_ref()).into_owned())
}

fn bounded_unqualified_attribute(
    element: &XmlElement,
    local_name: &[u8],
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    bounded_expanded_attribute(
        element,
        local_name,
        |namespace| namespace.is_none(),
        max_bytes,
        field,
    )
}

fn bounded_office_relationship_attribute(
    element: &XmlElement,
    local_name: &[u8],
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    bounded_expanded_attribute(
        element,
        local_name,
        |namespace| {
            namespace.is_some_and(|namespace| {
                matches!(
                    namespace,
                    value
                        if value == TRANSITIONAL_RELATIONSHIP_NAMESPACE.as_bytes()
                            || value == STRICT_RELATIONSHIP_NAMESPACE.as_bytes()
                )
            })
        },
        max_bytes,
        field,
    )
}

fn bounded_expanded_attribute(
    element: &XmlElement,
    local_name: &[u8],
    namespace_matches: impl Fn(Option<&[u8]>) -> bool,
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    let mut value = None;
    for attribute in &element.attributes {
        if attribute.local_name != local_name || !namespace_matches(attribute.namespace.as_deref())
        {
            continue;
        }
        if value.is_some() {
            return Err(OoxmlError::invalid(format!("{field} is ambiguous")));
        }
        let raw = attribute.value.as_slice();
        if raw.len() > max_bytes {
            return Err(OoxmlError::limit_exceeded(format!(
                "{field} exceeded the size limit"
            )));
        }
        let encoded = std::str::from_utf8(raw)
            .map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        let decoded =
            unescape(encoded).map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        if decoded.len() > max_bytes {
            return Err(OoxmlError::limit_exceeded(format!(
                "{field} exceeded the size limit"
            )));
        }
        value = Some(decoded.into_owned());
    }
    Ok(value)
}

fn bounded_qualified_attribute(
    element: &XmlElement,
    qualified_name: &[u8],
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    let mut value = None;
    for attribute in element.start.attributes() {
        let attribute =
            attribute.map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        if attribute.key.as_ref() != qualified_name {
            continue;
        }
        if value.is_some() {
            return Err(OoxmlError::invalid(format!("{field} is ambiguous")));
        }
        let raw = attribute.value.as_ref();
        if raw.len() > max_bytes {
            return Err(OoxmlError::limit_exceeded(format!(
                "{field} exceeded the size limit"
            )));
        }
        let encoded = std::str::from_utf8(raw)
            .map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        let decoded =
            unescape(encoded).map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        if decoded.len() > max_bytes {
            return Err(OoxmlError::limit_exceeded(format!(
                "{field} exceeded the size limit"
            )));
        }
        value = Some(decoded.into_owned());
    }
    Ok(value)
}

fn set_attribute(element: &mut XmlElement, local: &[u8], value: &str) {
    let mut attributes = element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .map(|attribute| {
            (
                attribute.key.as_ref().to_vec(),
                attribute.value.as_ref().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    if let Some(attribute) = attributes.iter_mut().find(|attribute| {
        attribute
            .0
            .rsplit(|byte| *byte == b':')
            .next()
            .is_some_and(|name| name == local)
    }) {
        attribute.1 = value.as_bytes().to_vec();
    } else {
        attributes.push((local.to_vec(), value.as_bytes().to_vec()));
    }
    element.start.clear_attributes();
    for (key, value) in attributes {
        element
            .start
            .push_attribute((key.as_slice(), value.as_slice()));
    }
}

fn set_unqualified_attribute(element: &mut XmlElement, name: &[u8], value: &str) {
    let mut attributes = element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .map(|attribute| {
            (
                attribute.key.as_ref().to_vec(),
                attribute.value.as_ref().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    if let Some(attribute) = attributes
        .iter_mut()
        .find(|attribute| attribute.0.as_slice() == name)
    {
        attribute.1 = value.as_bytes().to_vec();
    } else {
        attributes.push((name.to_vec(), value.as_bytes().to_vec()));
    }
    element.start.clear_attributes();
    for (key, value) in attributes {
        element
            .start
            .push_attribute((key.as_slice(), value.as_slice()));
    }
}

fn remove_attribute(element: &mut XmlElement, local: &[u8]) {
    let attributes = element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .map(|attribute| {
            (
                attribute.key.as_ref().to_vec(),
                attribute.value.as_ref().to_vec(),
            )
        })
        .filter(|attribute| {
            attribute
                .0
                .rsplit(|byte| *byte == b':')
                .next()
                .is_none_or(|name| name != local)
        })
        .collect::<Vec<_>>();
    element.start.clear_attributes();
    for (key, value) in attributes {
        element
            .start
            .push_attribute((key.as_slice(), value.as_slice()));
    }
}

fn element_text(element: &XmlElement) -> String {
    let mut output = String::new();
    collect_text(element, &mut output);
    output
}

fn collect_text(element: &XmlElement, output: &mut String) {
    for node in &element.children {
        match node {
            XmlNode::Raw(Event::Text(text)) => {
                if let Ok(value) = text.decode() {
                    output.push_str(&decode_entities(&value));
                }
            }
            XmlNode::Raw(Event::GeneralRef(reference)) => {
                if let Ok(value) = reference.decode() {
                    output.push_str(&decode_entities(&format!("&{value};")));
                }
            }
            XmlNode::Raw(Event::CData(data)) => {
                if let Ok(value) = data.decode() {
                    output.push_str(&value);
                }
            }
            XmlNode::Element(child) => collect_text(child, output),
            XmlNode::Raw(_) => {}
        }
    }
}

fn decode_entities(value: &str) -> String {
    unescape(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn set_element_text(element: &mut XmlElement, value: &str) {
    element.children.clear();
    if !value.is_empty() {
        element.children.push(XmlNode::Raw(Event::Text(
            BytesText::new(value).into_owned(),
        )));
    }
    element.empty = value.is_empty();
}

fn cell_reference(row: u32, column: u32) -> String {
    format!("{}{}", column_name(column).unwrap_or_default(), row)
}

fn column_name(mut number: u32) -> Result<String, OoxmlError> {
    if number == 0 || number > 16_384 {
        return Err(OoxmlError::invalid("worksheet column is invalid"));
    }
    let mut output = String::new();
    while number > 0 {
        let remainder = (number - 1) % 26;
        output.insert(0, char::from(b'A' + remainder as u8));
        number = (number - 1) / 26;
    }
    Ok(output)
}

fn split_reference(reference: &str) -> Option<(&str, u32)> {
    let split = reference.find(|character: char| character.is_ascii_digit())?;
    let row = reference[split..].parse::<u32>().ok()?;
    Some((&reference[..split], row))
}

fn parse_reference(reference: &str) -> Option<(u32, u32)> {
    let (column, row) = split_reference(reference)?;
    let mut column_number = 0_u32;
    for byte in column.bytes() {
        if !byte.is_ascii_alphabetic() {
            return None;
        }
        column_number = column_number
            .checked_mul(26)?
            .checked_add(u32::from(byte.to_ascii_uppercase() - b'A' + 1))?;
    }
    (column_number > 0).then_some((row, column_number))
}

#[cfg(test)]
mod tests;
