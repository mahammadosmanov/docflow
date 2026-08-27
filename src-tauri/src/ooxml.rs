use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt, fs,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
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

const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 4_096;
const MAX_WORKSHEETS: usize = 250;
const MAX_SHEET_NAME_BYTES: usize = 1_024;
const MAX_RELATIONSHIP_ID_BYTES: usize = 1_024;
const MAX_RELATIONSHIP_TYPE_BYTES: usize = 2_048;
const MAX_RELATIONSHIP_TARGET_BYTES: usize = 4_096;
const MAX_STATE_BYTES: usize = 16;
const MAX_U32_ATTRIBUTE_BYTES: usize = 10;
const MAX_XML_DEPTH: usize = 256;
const MAX_XML_NODES: usize = 1_000_000;
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
const MICROSOFT_OFFICE_RELATIONSHIP_PREFIX: &str =
    "http://schemas.microsoft.com/office/2006/relationships/";

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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorksheetData {
    pub max_row: u32,
    pub max_column: u32,
    pub cells: BTreeMap<(u32, u32), CellData>,
    pub column_widths: BTreeMap<u32, f64>,
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
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbookData {
    pub sheets: BTreeMap<String, WorksheetData>,
}

impl WorkbookData {
    pub fn sheet(&self, name: &str) -> Option<&WorksheetData> {
        self.sheets.get(name)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OoxmlError(String);

impl OoxmlError {
    fn invalid(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for OoxmlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
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
    let sheet_infos = project_sheet_infos(ordered_workbook_discovery(&mut input)?);
    input.seek(SeekFrom::Start(0))?;
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

pub fn read_member(source: &Path, name: &str) -> Result<Vec<u8>, OoxmlError> {
    validate_source(source)?;
    let mut archive = open_archive(File::open(source)?)?;
    read_entry(&mut archive, name)
}

pub fn sheet_infos_from_path(source: &Path) -> Result<BTreeMap<String, SheetInfo>, OoxmlError> {
    validate_source(source)?;
    let mut input = File::open(source)?;
    let discovery = ordered_workbook_discovery(&mut input)?;
    Ok(project_sheet_infos(discovery))
}

fn project_sheet_infos(discovery: OrderedWorkbookDiscovery) -> BTreeMap<String, SheetInfo> {
    discovery
        .worksheets
        .into_iter()
        .map(|worksheet| {
            (
                worksheet.name,
                SheetInfo {
                    path: worksheet.part_path,
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
            return Err(OoxmlError::invalid("workbook has too many worksheets"));
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
    let infos = project_sheet_infos(ordered_workbook_discovery(&mut input)?);
    input.seek(SeekFrom::Start(0))?;
    let mut archive = open_archive(input)?;
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
    for (name, info) in infos {
        let raw = read_entry(&mut archive, &info.path)?;
        sheets.insert(name, worksheet_data(&raw, &shared_strings)?);
    }
    Ok(WorkbookData { sheets })
}

fn validate_source(source: &Path) -> Result<(), OoxmlError> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_PACKAGE_BYTES {
        return Err(OoxmlError::invalid(
            "workbook package is invalid or too large",
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
    if source_len == 0 || source_len > MAX_PACKAGE_BYTES {
        return Err(OoxmlError::invalid(
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
        return Err(OoxmlError::invalid("workbook ZIP entry count is invalid"));
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
    if archive.is_empty() || archive.len() > MAX_ENTRIES {
        return Err(OoxmlError::invalid("workbook ZIP entry count is invalid"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.encrypted()
            || entry.size() > MAX_ENTRY_BYTES
            || entry.enclosed_name().is_none()
            || !names.insert(entry.name().to_string())
        {
            return Err(OoxmlError::invalid("workbook ZIP entry is invalid"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err(OoxmlError::invalid(
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
        return Err(OoxmlError::invalid(
            "workbook member exceeded the size limit",
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    entry
        .by_ref()
        .take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(OoxmlError::invalid(
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
    let class = ooxml_relationship_class(relationship_type)
        .or_else(|| relationship_type.strip_prefix(MICROSOFT_OFFICE_RELATIONSHIP_PREFIX))?;
    (!class.is_empty() && !class.contains('/')).then_some(class)
}

fn ooxml_relationship_class(relationship_type: &str) -> Option<&str> {
    let class = relationship_type
        .strip_prefix(TRANSITIONAL_RELATIONSHIP_PREFIX)
        .or_else(|| relationship_type.strip_prefix(STRICT_RELATIONSHIP_PREFIX))?;
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
        "xlMacrosheet",
        "xlIntlMacrosheet",
        "pivotCacheDefinition",
        "customXml",
        "metadata",
        "person",
        "richValueRel",
        "slicerCache",
        "timelineCache",
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
        let normalized_target =
            normalize_workbook_relationship_target(workbook_part_path, &target)?;
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
        let Some(relationship_class) = ooxml_relationship_class(&relationship_type) else {
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

fn shared_strings(raw: &[u8]) -> Result<Vec<String>, OoxmlError> {
    let document = parse_document(raw)?;
    Ok(descendants(&document.nodes)
        .filter(|element| has_local_name(element, b"si"))
        .map(descendant_text)
        .collect())
}

fn worksheet_data(raw: &[u8], shared_strings: &[String]) -> Result<WorksheetData, OoxmlError> {
    let document = parse_document(raw)?;
    let mut data = WorksheetData::default();
    for element in descendants(&document.nodes) {
        if has_local_name(element, b"col") {
            let Some(first) =
                attribute(element, b"min").and_then(|value| value.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(last) = attribute(element, b"max").and_then(|value| value.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(width) =
                attribute(element, b"width").and_then(|value| value.parse::<f64>().ok())
            else {
                continue;
            };
            for column in first..=last.min(16_384) {
                data.column_widths.insert(column, width);
            }
            continue;
        }
        if !has_local_name(element, b"c") {
            continue;
        }
        let reference = attribute(element, b"r")
            .and_then(|value| parse_reference(&value))
            .ok_or_else(|| OoxmlError::invalid("worksheet cell reference is invalid"))?;
        let cell_type = attribute(element, b"t").unwrap_or_default();
        let raw_value = child_element(element, b"v")
            .map(element_text)
            .unwrap_or_default();
        let value = match cell_type.as_str() {
            "inlineStr" => child_element(element, b"is")
                .map(descendant_text)
                .unwrap_or_default(),
            "s" => raw_value
                .parse::<usize>()
                .ok()
                .and_then(|index| shared_strings.get(index))
                .cloned()
                .ok_or_else(|| OoxmlError::invalid("shared string index is invalid"))?,
            _ => raw_value,
        };
        let formula = child_element(element, b"f").map(element_text);
        data.max_row = data.max_row.max(reference.0);
        data.max_column = data.max_column.max(reference.1);
        data.cells.insert(
            reference,
            CellData {
                value,
                formula,
                style: attribute(element, b"s"),
            },
        );
    }
    Ok(data)
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
        return Err(OoxmlError::invalid("XML member exceeded the size limit"));
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
                    return Err(OoxmlError::invalid("XML element depth exceeded the limit"));
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
                    return Err(OoxmlError::invalid("XML element depth exceeded the limit"));
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
        .ok_or_else(|| OoxmlError::invalid("XML node count exceeded the limit"))?;
    if *node_count > max_nodes {
        return Err(OoxmlError::invalid("XML node count exceeded the limit"));
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
            return Err(OoxmlError::invalid(format!(
                "{field} exceeded the size limit"
            )));
        }
        let encoded = std::str::from_utf8(raw)
            .map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        let decoded =
            unescape(encoded).map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        if decoded.len() > max_bytes {
            return Err(OoxmlError::invalid(format!(
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
            return Err(OoxmlError::invalid(format!(
                "{field} exceeded the size limit"
            )));
        }
        let encoded = std::str::from_utf8(raw)
            .map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        let decoded =
            unescape(encoded).map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        if decoded.len() > max_bytes {
            return Err(OoxmlError::invalid(format!(
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

fn descendant_text(element: &XmlElement) -> String {
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
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        io::{self, Cursor, Read, Seek, SeekFrom, Write},
    };

    use tempfile::tempdir;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::{
        CellUpdate, WorksheetDescriptor, WorksheetVisibility, ordered_workbook_discovery,
        parse_document_with_limits, patch_workbook_sheets, read_workbook, sheet_infos_from_path,
    };

    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const REL: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
    const OFFICE_DOCUMENT_REL: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
    const PACKAGE_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

    struct SparseReader {
        len: u64,
        position: u64,
    }

    impl Read for SparseReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }

    impl Seek for SparseReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.position = match position {
                SeekFrom::Start(offset) => offset,
                SeekFrom::End(offset) => self.len.checked_add_signed(offset).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid sparse seek")
                })?,
                SeekFrom::Current(offset) => {
                    self.position.checked_add_signed(offset).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "invalid sparse seek")
                    })?
                }
            };
            Ok(self.position)
        }
    }

    fn fixture(workbook: &str, relationships: &str, members: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
        fixture_from_members(workbook, relationships, members.iter().copied())
    }

    fn declared_entry_count_fixture(entry_count: u16) -> Cursor<Vec<u8>> {
        let mut bytes = b"PK\x05\x06".to_vec();
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&entry_count.to_le_bytes());
        bytes.extend_from_slice(&entry_count.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        Cursor::new(bytes)
    }

    fn append_decoy_eocd(mut fixture: Cursor<Vec<u8>>) -> Cursor<Vec<u8>> {
        let offset = u32::try_from(fixture.get_ref().len()).unwrap();
        fixture.set_position(u64::from(offset));
        fixture.write_all(b"PK\x05\x06").unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.write_all(&1_u16.to_le_bytes()).unwrap();
        fixture.write_all(&1_u16.to_le_bytes()).unwrap();
        fixture.write_all(&0_u32.to_le_bytes()).unwrap();
        fixture.write_all(&offset.to_le_bytes()).unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.set_position(0);
        fixture
    }

    fn append_parser_fallback_eocd(mut fixture: Cursor<Vec<u8>>) -> Cursor<Vec<u8>> {
        let bytes = fixture.get_ref();
        let original_eocd = bytes
            .windows(4)
            .rposition(|window| window == b"PK\x05\x06")
            .unwrap();
        let original_entries =
            u16::from_le_bytes([bytes[original_eocd + 10], bytes[original_eocd + 11]]);
        let directory_offset = u32::from_le_bytes([
            bytes[original_eocd + 16],
            bytes[original_eocd + 17],
            bytes[original_eocd + 18],
            bytes[original_eocd + 19],
        ]);
        let fake_eocd_offset = u32::try_from(bytes.len()).unwrap();
        let fake_entries = original_entries.checked_add(1).unwrap();
        let fake_directory_size = fake_eocd_offset.checked_sub(directory_offset).unwrap();

        fixture.set_position(u64::from(fake_eocd_offset));
        fixture.write_all(b"PK\x05\x06").unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.write_all(&fake_entries.to_le_bytes()).unwrap();
        fixture.write_all(&fake_entries.to_le_bytes()).unwrap();
        fixture
            .write_all(&fake_directory_size.to_le_bytes())
            .unwrap();
        fixture.write_all(&directory_offset.to_le_bytes()).unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.set_position(0);
        fixture
    }

    fn append_semantically_invalid_directory(mut fixture: Cursor<Vec<u8>>) -> Cursor<Vec<u8>> {
        let bytes = fixture.get_ref();
        let original_eocd = bytes
            .windows(4)
            .rposition(|window| window == b"PK\x05\x06")
            .unwrap();
        let entry_count =
            u16::from_le_bytes([bytes[original_eocd + 10], bytes[original_eocd + 11]]);
        let directory_size = u32::from_le_bytes([
            bytes[original_eocd + 12],
            bytes[original_eocd + 13],
            bytes[original_eocd + 14],
            bytes[original_eocd + 15],
        ]);
        let directory_offset = u32::from_le_bytes([
            bytes[original_eocd + 16],
            bytes[original_eocd + 17],
            bytes[original_eocd + 18],
            bytes[original_eocd + 19],
        ]);
        let mut directory = bytes
            [directory_offset as usize..directory_offset as usize + directory_size as usize]
            .to_vec();
        assert_eq!(&directory[..4], b"PK\x01\x02");
        let name_len = usize::from(u16::from_le_bytes([directory[28], directory[29]]));
        let extra_len = u16::from_le_bytes([directory[30], directory[31]]);
        directory[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
        directory[30..32].copy_from_slice(&extra_len.checked_add(4).unwrap().to_le_bytes());
        directory.splice(46 + name_len..46 + name_len, [0x01, 0x00, 0x08, 0x00]);

        let final_directory_offset = u32::try_from(bytes.len()).unwrap();
        let final_eocd_offset = final_directory_offset
            .checked_add(u32::try_from(directory.len()).unwrap())
            .unwrap();
        fixture.set_position(u64::from(final_directory_offset));
        fixture.write_all(&directory).unwrap();
        fixture.write_all(b"PK\x05\x06").unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.write_all(&entry_count.to_le_bytes()).unwrap();
        fixture.write_all(&entry_count.to_le_bytes()).unwrap();
        fixture
            .write_all(&(final_eocd_offset - final_directory_offset).to_le_bytes())
            .unwrap();
        fixture
            .write_all(&final_directory_offset.to_le_bytes())
            .unwrap();
        fixture.write_all(&0_u16.to_le_bytes()).unwrap();
        fixture.set_position(0);
        fixture
    }

    fn final_eocd_offset(bytes: &[u8]) -> usize {
        bytes
            .windows(4)
            .rposition(|window| window == b"PK\x05\x06")
            .unwrap()
    }

    fn central_directory_offsets(bytes: &[u8]) -> Vec<usize> {
        let eocd_offset = final_eocd_offset(bytes);
        let entry_count = usize::from(u16::from_le_bytes([
            bytes[eocd_offset + 10],
            bytes[eocd_offset + 11],
        ]));
        let mut offset = u32::from_le_bytes([
            bytes[eocd_offset + 16],
            bytes[eocd_offset + 17],
            bytes[eocd_offset + 18],
            bytes[eocd_offset + 19],
        ]) as usize;
        let mut offsets = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            assert_eq!(&bytes[offset..offset + 4], b"PK\x01\x02");
            offsets.push(offset);
            let name_len =
                usize::from(u16::from_le_bytes([bytes[offset + 28], bytes[offset + 29]]));
            let extra_len =
                usize::from(u16::from_le_bytes([bytes[offset + 30], bytes[offset + 31]]));
            let comment_len =
                usize::from(u16::from_le_bytes([bytes[offset + 32], bytes[offset + 33]]));
            offset += 46 + name_len + extra_len + comment_len;
        }
        offsets
    }

    fn mutate_first_central_field(
        mut fixture: Cursor<Vec<u8>>,
        field_offset: usize,
        replacement: &[u8],
    ) -> Cursor<Vec<u8>> {
        let central_offset = central_directory_offsets(fixture.get_ref())[0];
        fixture.get_mut()
            [central_offset + field_offset..central_offset + field_offset + replacement.len()]
            .copy_from_slice(replacement);
        fixture.set_position(0);
        fixture
    }

    fn insert_first_central_extra(mut fixture: Cursor<Vec<u8>>, extra: &[u8]) -> Cursor<Vec<u8>> {
        let bytes = fixture.get_ref();
        let eocd_offset = final_eocd_offset(bytes);
        let central_offset = central_directory_offsets(bytes)[0];
        let directory_size = u32::from_le_bytes([
            bytes[eocd_offset + 12],
            bytes[eocd_offset + 13],
            bytes[eocd_offset + 14],
            bytes[eocd_offset + 15],
        ]);
        let name_len = usize::from(u16::from_le_bytes([
            bytes[central_offset + 28],
            bytes[central_offset + 29],
        ]));
        let extra_len =
            u16::from_le_bytes([bytes[central_offset + 30], bytes[central_offset + 31]]);
        let insertion_offset = central_offset + 46 + name_len + usize::from(extra_len);
        let new_extra_len = extra_len
            .checked_add(u16::try_from(extra.len()).unwrap())
            .unwrap();
        fixture.get_mut()[central_offset + 30..central_offset + 32]
            .copy_from_slice(&new_extra_len.to_le_bytes());
        fixture
            .get_mut()
            .splice(insertion_offset..insertion_offset, extra.iter().copied());
        let new_eocd_offset = eocd_offset + extra.len();
        let new_directory_size = directory_size
            .checked_add(u32::try_from(extra.len()).unwrap())
            .unwrap();
        fixture.get_mut()[new_eocd_offset + 12..new_eocd_offset + 16]
            .copy_from_slice(&new_directory_size.to_le_bytes());
        fixture.set_position(0);
        fixture
    }

    fn mutate_first_local_field(
        mut fixture: Cursor<Vec<u8>>,
        field_offset: usize,
        replacement: &[u8],
    ) -> Cursor<Vec<u8>> {
        let bytes = fixture.get_ref();
        let central_offset = central_directory_offsets(bytes)[0];
        let local_offset = u32::from_le_bytes([
            bytes[central_offset + 42],
            bytes[central_offset + 43],
            bytes[central_offset + 44],
            bytes[central_offset + 45],
        ]) as usize;
        assert_eq!(&bytes[local_offset..local_offset + 4], b"PK\x03\x04");
        fixture.get_mut()
            [local_offset + field_offset..local_offset + field_offset + replacement.len()]
            .copy_from_slice(replacement);
        fixture.set_position(0);
        fixture
    }

    fn insert_last_local_extra(mut fixture: Cursor<Vec<u8>>, extra: &[u8]) -> Cursor<Vec<u8>> {
        let bytes = fixture.get_ref();
        let eocd_offset = final_eocd_offset(bytes);
        let central_offsets = central_directory_offsets(bytes);
        let central_offset = *central_offsets
            .iter()
            .max_by_key(|offset| {
                u32::from_le_bytes([
                    bytes[**offset + 42],
                    bytes[**offset + 43],
                    bytes[**offset + 44],
                    bytes[**offset + 45],
                ])
            })
            .unwrap();
        let local_offset = u32::from_le_bytes([
            bytes[central_offset + 42],
            bytes[central_offset + 43],
            bytes[central_offset + 44],
            bytes[central_offset + 45],
        ]) as usize;
        assert_eq!(&bytes[local_offset..local_offset + 4], b"PK\x03\x04");
        let directory_offset = u32::from_le_bytes([
            bytes[eocd_offset + 16],
            bytes[eocd_offset + 17],
            bytes[eocd_offset + 18],
            bytes[eocd_offset + 19],
        ]);
        let name_len = usize::from(u16::from_le_bytes([
            bytes[local_offset + 26],
            bytes[local_offset + 27],
        ]));
        let extra_len = u16::from_le_bytes([bytes[local_offset + 28], bytes[local_offset + 29]]);
        let insertion_offset = local_offset + 30 + name_len + usize::from(extra_len);
        let new_extra_len = extra_len
            .checked_add(u16::try_from(extra.len()).unwrap())
            .unwrap();
        fixture.get_mut()[local_offset + 28..local_offset + 30]
            .copy_from_slice(&new_extra_len.to_le_bytes());
        fixture
            .get_mut()
            .splice(insertion_offset..insertion_offset, extra.iter().copied());
        let new_eocd_offset = eocd_offset + extra.len();
        let new_directory_offset = directory_offset
            .checked_add(u32::try_from(extra.len()).unwrap())
            .unwrap();
        fixture.get_mut()[new_eocd_offset + 16..new_eocd_offset + 20]
            .copy_from_slice(&new_directory_offset.to_le_bytes());
        fixture.set_position(0);
        fixture
    }

    fn assert_archive_preflight_rejects(fixture: Cursor<Vec<u8>>) {
        let mut preflight_source = fixture.clone();
        let preflight_error = match super::preflight_archive(&mut preflight_source) {
            Ok(_) => {
                let discovery = ordered_workbook_discovery(&mut fixture.clone());
                panic!(
                    "archive preflight accepted entry-level ZIP metadata; discovery returned {discovery:?}"
                );
            }
            Err(error) => error,
        };
        assert_eq!(
            preflight_error.to_string(),
            "workbook ZIP metadata is invalid"
        );

        let discovery_error = ordered_workbook_discovery(&mut fixture.clone()).unwrap_err();
        assert_eq!(
            discovery_error.to_string(),
            "workbook ZIP metadata is invalid"
        );
    }

    fn fixture_from_members<'a>(
        workbook: &'a str,
        relationships: &'a str,
        members: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Cursor<Vec<u8>> {
        let package_relationships = format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/></Relationships>"#
        );
        fixture_with_workbook_part(
            Some(&package_relationships),
            "xl/workbook.xml",
            workbook,
            relationships,
            members,
        )
    }

    fn fixture_with_workbook_part<'a>(
        package_relationships: Option<&str>,
        workbook_part: &str,
        workbook: &'a str,
        relationships: &'a str,
        members: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Cursor<Vec<u8>> {
        let workbook = workbook.replace(
            r#"xmlns:r="urn:r""#,
            r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#,
        );
        let relationships = if relationships.contains("xmlns=") {
            relationships.to_string()
        } else {
            relationships.replacen(
                "<Relationships",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships""#,
                1,
            )
        };
        let (workbook_parent, workbook_name) = workbook_part.rsplit_once('/').unwrap();
        let workbook_relationships_part = format!("{workbook_parent}/_rels/{workbook_name}.rels");
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        if let Some(package_relationships) = package_relationships {
            writer.start_file("_rels/.rels", options).unwrap();
            writer.write_all(package_relationships.as_bytes()).unwrap();
        }
        for (name, bytes) in [
            (workbook_part, workbook.as_bytes()),
            (
                workbook_relationships_part.as_str(),
                relationships.as_bytes(),
            ),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        for (name, bytes) in members {
            writer.start_file(name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        let mut fixture = writer.finish().unwrap();
        fixture.set_position(0);
        fixture
    }

    fn ordered_fixture() -> Cursor<Vec<u8>> {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="MEC" sheetId="17" r:id="rId7"/><sheet name="CIV" sheetId="3" r:id="rId2"/><sheet name="Hidden" sheetId="44" state="hidden" r:id="rId9"/><sheet name="VeryHidden" sheetId="45" state="veryHidden" r:id="rId10"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/><Relationship Id="rId10" Type="{REL}" Target="worksheets/sheet10.xml"/><Relationship Id="rId7" Type="{REL}" Target="/xl/worksheets/sheet7.xml"/><Relationship Id="rId9" Type="{REL}" Target="worksheets/sheet9.xml"/></Relationships>"#
        );
        fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet2.xml", b"CIV"),
                ("xl/worksheets/sheet7.xml", b"MEC"),
                ("xl/worksheets/sheet9.xml", b"Hidden"),
                ("xl/worksheets/sheet10.xml", b"VeryHidden"),
                ("docProps/unrelated.bin", b"unrelated"),
            ],
        )
    }

    #[test]
    fn package_workbook_authority_authenticates_exact_internal_target() {
        let trusted_workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Trusted" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let trusted_relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let decoy_workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Decoy" sheetId="9" r:id="rId9"/></sheets></workbook>"#
        );
        let decoy_relationships = format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId9" Type="{REL}" Target="worksheets/decoy.xml"/></Relationships>"#
        );
        let package_relationships = format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="custom/book.xml"/></Relationships>"#
        );
        let trusted_members: [(&str, &[u8]); 4] = [
            ("custom/worksheets/sheet1.xml", b"trusted worksheet"),
            ("xl/workbook.xml", decoy_workbook.as_bytes()),
            ("xl/_rels/workbook.xml.rels", decoy_relationships.as_bytes()),
            ("xl/worksheets/decoy.xml", b"decoy worksheet"),
        ];
        let discovery = ordered_workbook_discovery(&mut fixture_with_workbook_part(
            Some(&package_relationships),
            "custom/book.xml",
            &trusted_workbook,
            &trusted_relationships,
            trusted_members,
        ))
        .unwrap();

        assert_eq!(discovery.worksheets.len(), 1);
        assert_eq!(discovery.worksheets[0].name, "Trusted");
        assert_eq!(
            discovery.worksheets[0].part_path,
            "custom/worksheets/sheet1.xml"
        );

        let strict_office_document =
            "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
        let strict_worksheet = "http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet";
        let strict_workbook = r#"<workbook xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main" xmlns:r="http://purl.oclc.org/ooxml/officeDocument/relationships"><sheets><sheet name="Strict" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
        let strict_relationships = format!(
            r#"<Relationships xmlns="http://purl.oclc.org/ooxml/package/relationships"><Relationship Id="rId1" Type="{strict_worksheet}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let strict_package_relationships = format!(
            r#"<Relationships xmlns="http://purl.oclc.org/ooxml/package/relationships"><Relationship Id="rIdWorkbook" Type="{strict_office_document}" Target="strict/book.xml"/></Relationships>"#
        );
        let strict_members: [(&str, &[u8]); 1] =
            [("strict/worksheets/sheet1.xml", b"strict worksheet")];
        let strict_discovery = ordered_workbook_discovery(&mut fixture_with_workbook_part(
            Some(&strict_package_relationships),
            "strict/book.xml",
            strict_workbook,
            &strict_relationships,
            strict_members,
        ))
        .unwrap();
        assert_eq!(strict_discovery.worksheets[0].name, "Strict");

        let invalid_package_relationships = [
            None,
            Some(format!(
                r#"<Relationships xmlns="urn:spoof"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/></Relationships>"#
            )),
            Some(format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdOne" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/><Relationship Id="rIdTwo" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml"/></Relationships>"#
            )),
            Some(format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="xl/workbook.xml" TargetMode="External"/></Relationships>"#
            )),
            Some(format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="urn:unknown/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
            )),
            Some(format!(
                r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rIdWorkbook" Type="{OFFICE_DOCUMENT_REL}" Target="missing/book.xml"/></Relationships>"#
            )),
        ];
        for package_relationships in invalid_package_relationships {
            let decoy_members: [(&str, &[u8]); 1] =
                [("xl/worksheets/decoy.xml", b"decoy worksheet")];
            let mut package = fixture_with_workbook_part(
                package_relationships.as_deref(),
                "xl/workbook.xml",
                &decoy_workbook,
                &decoy_relationships,
                decoy_members,
            );
            assert!(
                ordered_workbook_discovery(&mut package).is_err(),
                "invalid package workbook relationship must fail closed"
            );
        }
    }

    #[test]
    fn xml_parser_rejects_excessive_element_depth() {
        let error = match parse_document_with_limits(b"<a><b><c/></b></a>", 2, 100) {
            Ok(_) => panic!("excessively deep XML must fail closed"),
            Err(error) => error,
        };

        assert_eq!(error.to_string(), "XML element depth exceeded the limit");
    }

    #[test]
    fn xml_parser_rejects_excessive_retained_nodes() {
        let error = match parse_document_with_limits(b"<a><b/><c/></a>", 10, 2) {
            Ok(_) => panic!("excessive retained XML nodes must fail closed"),
            Err(error) => error,
        };

        assert_eq!(error.to_string(), "XML node count exceeded the limit");
    }

    #[test]
    fn ordered_worksheet_discovery_preserves_order_identity_and_visibility() {
        let discovery = ordered_workbook_discovery(&mut ordered_fixture()).unwrap();

        assert_eq!(
            discovery.worksheets,
            vec![
                WorksheetDescriptor {
                    workbook_index: 1,
                    sheet_id: 17,
                    relationship_id: "rId7".to_string(),
                    relationship_type: REL.to_string(),
                    part_path: "xl/worksheets/sheet7.xml".to_string(),
                    name: "MEC".to_string(),
                    visibility: WorksheetVisibility::Visible,
                },
                WorksheetDescriptor {
                    workbook_index: 2,
                    sheet_id: 3,
                    relationship_id: "rId2".to_string(),
                    relationship_type: REL.to_string(),
                    part_path: "xl/worksheets/sheet2.xml".to_string(),
                    name: "CIV".to_string(),
                    visibility: WorksheetVisibility::Visible,
                },
                WorksheetDescriptor {
                    workbook_index: 3,
                    sheet_id: 44,
                    relationship_id: "rId9".to_string(),
                    relationship_type: REL.to_string(),
                    part_path: "xl/worksheets/sheet9.xml".to_string(),
                    name: "Hidden".to_string(),
                    visibility: WorksheetVisibility::Hidden,
                },
                WorksheetDescriptor {
                    workbook_index: 4,
                    sheet_id: 45,
                    relationship_id: "rId10".to_string(),
                    relationship_type: REL.to_string(),
                    part_path: "xl/worksheets/sheet10.xml".to_string(),
                    name: "VeryHidden".to_string(),
                    visibility: WorksheetVisibility::VeryHidden,
                },
            ]
        );
        assert_eq!(discovery.safety, Default::default());
    }

    #[test]
    fn ordered_worksheet_discovery_ignores_sheet_elements_outside_sheets_container() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><ext:data><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/></ext:data><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/decoy.xml", b"decoy"),
                ("xl/worksheets/sheet1.xml", b"worksheet"),
            ],
        ))
        .unwrap();

        assert_eq!(discovery.worksheets.len(), 1);
        assert_eq!(discovery.worksheets[0].name, "MEC");
        assert_eq!(discovery.worksheets[0].workbook_index, 1);
    }

    #[test]
    fn ordered_worksheet_discovery_ignores_foreign_sheet_elements_in_sheets_container() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/decoy.xml", b"decoy"),
                ("xl/worksheets/sheet1.xml", b"worksheet"),
            ],
        ))
        .unwrap();

        assert_eq!(discovery.worksheets.len(), 1);
        assert_eq!(discovery.worksheets[0].name, "MEC");
    }

    #[test]
    fn legacy_sheet_info_projection_ignores_foreign_sheet_elements() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let bytes = fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/decoy.xml", b"decoy"),
                ("xl/worksheets/sheet1.xml", b"worksheet"),
            ],
        )
        .into_inner();
        let directory = tempdir().unwrap();
        let path = directory.path().join("workbook.xlsx");
        fs::write(&path, bytes).unwrap();

        let infos = sheet_infos_from_path(&path).unwrap();

        assert_eq!(infos.len(), 1);
        assert_eq!(infos["MEC"].sheet_id, 1);
        assert_eq!(infos["MEC"].path, "xl/worksheets/sheet1.xml");
    }

    #[test]
    fn read_workbook_uses_canonical_sheet_projection() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="Decoy" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet = format!(r#"<worksheet xmlns="{MAIN}"><sheetData/></worksheet>"#);
        let bytes = fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/decoy.xml", b"decoy"),
                ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
            ],
        )
        .into_inner();
        let directory = tempdir().unwrap();
        let path = directory.path().join("workbook.xlsx");
        fs::write(&path, bytes).unwrap();

        let data = read_workbook(&path).unwrap();

        assert!(data.sheet("MEC").is_some());
        assert!(data.sheet("Decoy").is_none());
    }

    #[test]
    fn read_workbook_uses_authenticated_shared_strings_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStrings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="/custom/strings.xml"/></Relationships>"#
        );
        let worksheet = format!(
            r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#
        );
        let trusted_strings = format!(
            r#"<sst xmlns="{MAIN}" count="1" uniqueCount="1"><si><t>Trusted</t></si></sst>"#
        );
        let decoy_strings =
            format!(r#"<sst xmlns="{MAIN}" count="1" uniqueCount="1"><si><t>Decoy</t></si></sst>"#);
        let bytes = fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
                ("custom/strings.xml", trusted_strings.as_bytes()),
                ("xl/sharedStrings.xml", decoy_strings.as_bytes()),
            ],
        )
        .into_inner();
        let directory = tempdir().unwrap();
        let path = directory.path().join("workbook.xlsx");
        fs::write(&path, bytes).unwrap();

        let data = read_workbook(&path).unwrap();

        assert_eq!(data.sheet("MEC").unwrap().value(1, 1), "Trusted");
    }

    #[test]
    fn patch_workbook_sheets_uses_canonical_sheet_projection() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:ext="urn:extension"><sheets><ext:sheet name="MEC" sheetId="99" r:id="rId99"/><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId99" Type="{REL}" Target="worksheets/decoy.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet = format!(
            r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><v>old</v></c></row></sheetData></worksheet>"#
        );
        let bytes = fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/decoy.xml", b"decoy"),
                ("xl/worksheets/sheet1.xml", worksheet.as_bytes()),
            ],
        )
        .into_inner();
        let directory = tempdir().unwrap();
        let source = directory.path().join("source.xlsx");
        let destination = directory.path().join("destination.xlsx");
        fs::write(&source, bytes).unwrap();
        let updates = BTreeMap::from([(
            "MEC".to_string(),
            BTreeMap::from([((1, 1), CellUpdate::Text("new".to_string()))]),
        )]);

        patch_workbook_sheets(&source, &destination, &updates, None).unwrap();

        let data = read_workbook(&destination).unwrap();
        assert_eq!(data.sheet("MEC").unwrap().value(1, 1), "new");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_sheet_id_from_foreign_namespace() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:evil="urn:evil"><sheets><sheet name="MEC" sheetId="1" evil:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "workbook sheet relationship is missing");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_spoofed_sheet_attribute_namespace() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:spoof"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "workbook relationship namespace is invalid"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_spoofed_workbook_namespace() {
        let workbook = r#"<workbook xmlns="urn:spoof" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "workbook namespace is invalid");
    }

    #[test]
    fn ordered_worksheet_discovery_expanded_names_rejects_evil_sheets_namespace() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets xmlns="urn:evil"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "workbook sheets container is invalid");
    }

    #[test]
    fn ordered_worksheet_discovery_expanded_names_rejects_shadowed_relationship_child() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship xmlns="urn:evil" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "workbook sheet target is missing");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_raw_default_namespace_declarations() {
        let canonical_workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let canonical_relationships = format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let cases = [
            (
                format!(
                    r#"<workbook xmlns="urn:foreign" xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
                ),
                canonical_relationships.clone(),
            ),
            (
                format!(
                    r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets xmlns="urn:foreign" xmlns="{MAIN}"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
                ),
                canonical_relationships.clone(),
            ),
            (
                format!(
                    r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet xmlns="urn:foreign" xmlns="{MAIN}" name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
                ),
                canonical_relationships.clone(),
            ),
            (
                canonical_workbook.clone(),
                format!(
                    r#"<Relationships xmlns="urn:foreign" xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
                ),
            ),
            (
                canonical_workbook,
                format!(
                    r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship xmlns="urn:foreign" xmlns="{PACKAGE_REL_NS}" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
                ),
            ),
        ];

        for (workbook, relationships) in cases {
            let error = ordered_workbook_discovery(&mut fixture(
                &workbook,
                &relationships,
                &[("xl/worksheets/sheet1.xml", b"worksheet")],
            ))
            .unwrap_err();

            assert_eq!(error.to_string(), "XML attribute is invalid");
        }
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_raw_prefixed_namespace_declarations() {
        const OFFICE_REL_NS: &str =
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let canonical_workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="{OFFICE_REL_NS}"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let canonical_relationships = format!(
            r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let cases = [
            (
                format!(
                    r#"<workbook xmlns="{MAIN}" xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
                ),
                canonical_relationships.clone(),
            ),
            (
                format!(
                    r#"<workbook xmlns="{MAIN}"><sheets xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
                ),
                canonical_relationships.clone(),
            ),
            (
                format!(
                    r#"<workbook xmlns="{MAIN}"><sheets><sheet xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}" name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
                ),
                canonical_relationships.clone(),
            ),
            (
                canonical_workbook.clone(),
                format!(
                    r#"<Relationships xmlns="{PACKAGE_REL_NS}" xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
                ),
            ),
            (
                canonical_workbook,
                format!(
                    r#"<Relationships xmlns="{PACKAGE_REL_NS}"><Relationship xmlns:r="urn:foreign" xmlns:r="{OFFICE_REL_NS}" Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
                ),
            ),
        ];

        for (workbook, relationships) in cases {
            let error = ordered_workbook_discovery(&mut fixture(
                &workbook,
                &relationships,
                &[("xl/worksheets/sheet1.xml", b"worksheet")],
            ))
            .unwrap_err();

            assert_eq!(error.to_string(), "XML attribute is invalid");
        }
    }

    #[test]
    fn ordered_worksheet_discovery_expanded_names_require_unqualified_attributes() {
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        for (attribute, expected_error) in [
            (
                r#"evil:name="MEC" sheetId="1""#,
                "workbook sheet name is missing",
            ),
            (
                r#"name="MEC" evil:sheetId="1""#,
                "workbook sheet identity is invalid",
            ),
        ] {
            let workbook = format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:evil="urn:evil"><sheets><sheet {attribute} r:id="rId1"/></sheets></workbook>"#
            );
            let error = ordered_workbook_discovery(&mut fixture(
                &workbook,
                &relationships,
                &[("xl/worksheets/sheet1.xml", b"worksheet")],
            ))
            .unwrap_err();

            assert_eq!(error.to_string(), expected_error);
        }

        for (attribute, expected_error) in [
            (
                format!(r#"evil:Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml""#),
                "workbook relationship identity is missing",
            ),
            (
                r#"Id="rId1" evil:Type="urn:evil" Target="worksheets/sheet1.xml""#.to_string(),
                "workbook sheet relationship type is missing",
            ),
            (
                format!(r#"Id="rId1" Type="{REL}" evil:Target="worksheets/sheet1.xml""#),
                "workbook relationship target is missing",
            ),
        ] {
            let workbook = format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            );
            let relationships = format!(
                r#"<Relationships xmlns:evil="urn:evil"><Relationship {attribute}/></Relationships>"#
            );
            let error = ordered_workbook_discovery(&mut fixture(
                &workbook,
                &relationships,
                &[("xl/worksheets/sheet1.xml", b"worksheet")],
            ))
            .unwrap_err();

            assert_eq!(error.to_string(), expected_error);
        }

        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:evil="urn:evil"><sheets><sheet name="MEC" sheetId="1" evil:state="hidden" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns:evil="urn:evil"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml" evil:TargetMode="External"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap();

        assert_eq!(
            discovery.worksheets[0].visibility,
            WorksheetVisibility::Visible
        );
    }

    #[test]
    fn ordered_worksheet_discovery_expanded_names_accepts_alternate_valid_prefixes() {
        for (workbook_namespace, office_namespace, package_namespace, relationship_type) in [
            (
                MAIN,
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                "http://schemas.openxmlformats.org/package/2006/relationships",
                REL,
            ),
            (
                "http://purl.oclc.org/ooxml/spreadsheetml/main",
                "http://purl.oclc.org/ooxml/officeDocument/relationships",
                "http://purl.oclc.org/ooxml/package/relationships",
                "http://purl.oclc.org/ooxml/officeDocument/relationships/worksheet",
            ),
        ] {
            let workbook = format!(
                r#"<ss:workbook xmlns:ss="{workbook_namespace}" xmlns:od="{office_namespace}"><ss:sheets><ss:sheet name="MEC" sheetId="1" od:id="rId1"/></ss:sheets></ss:workbook>"#
            );
            let relationships = format!(
                r#"<pkg:Relationships xmlns:pkg="{package_namespace}"><pkg:Relationship Id="rId1" Type="{relationship_type}" Target="worksheets/sheet1.xml"/></pkg:Relationships>"#
            );
            let discovery = ordered_workbook_discovery(&mut fixture(
                &workbook,
                &relationships,
                &[("xl/worksheets/sheet1.xml", b"worksheet")],
            ))
            .unwrap();

            assert_eq!(discovery.worksheets[0].relationship_id, "rId1");
            assert_eq!(discovery.worksheets[0].relationship_type, relationship_type);
        }
    }

    #[test]
    fn ordered_worksheet_discovery_expanded_names_reports_qualified_workbook_protection() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r" xmlns:ss="{MAIN}"><ss:workbookProtection lockStructure="1"/><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap();

        assert!(discovery.safety.has_workbook_protection);
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_name() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="MEC" sheetId="2" r:id="rId2"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"one"),
                ("xl/worksheets/sheet2.xml", b"two"),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "duplicate workbook sheet name");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_sheet_id() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="7" r:id="rId1"/><sheet name="CIV" sheetId="7" r:id="rId2"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"one"),
                ("xl/worksheets/sheet2.xml", b"two"),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "duplicate workbook sheet identity");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_relationship_id() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="CIV" sheetId="2" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"one")],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "duplicate workbook sheet relationship identity"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_relationship_declaration() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet2.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"one"),
                ("xl/worksheets/sheet2.xml", b"two"),
            ],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "duplicate workbook relationship identity"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_resolved_part() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/><sheet name="CIV" sheetId="2" r:id="rId2"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="{REL}" Target="/xl/worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"one")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "duplicate worksheet part path");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_normalized_workbook_relationship_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        for alias in [
            "./worksheets/sheet1.xml",
            "/xl/worksheets/%73heet1.xml",
            r"worksheets\sheet1.xml",
        ] {
            let relationships = format!(
                r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="{alias}"/></Relationships>"#
            );
            let error = ordered_workbook_discovery(&mut fixture(
                &workbook,
                &relationships,
                &[("xl/worksheets/sheet1.xml", b"worksheet")],
            ))
            .unwrap_err();

            assert_eq!(error.to_string(), "duplicate workbook relationship target");
        }
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_external_relationship_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="https://example.invalid/sheet.xml" TargetMode="External"/></Relationships>"#
        );
        let error =
            ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

        assert_eq!(
            error.to_string(),
            "external workbook relationship target is not allowed"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_accepts_explicit_internal_relationship_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml" TargetMode="Internal"/></Relationships>"#
        );

        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap();

        assert_eq!(discovery.worksheets.len(), 1);
        assert_eq!(discovery.worksheets[0].name, "MEC");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_encoded_traversal_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/%2E%2E/evil.xml"/></Relationships>"#
        );
        let error =
            ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

        assert_eq!(
            error.to_string(),
            "worksheet relationship escapes the package"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_encoded_forbidden_target_character() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml%3Fignored"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml?ignored", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "worksheet relationship escapes the package"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_non_worksheet_relationship() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="Chart" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet" Target="chartsheets/sheet1.xml"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            relationships,
            &[("xl/chartsheets/sheet1.xml", b"chart")],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "workbook sheet relationship is not a worksheet"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_spoofed_relationship_namespace() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = r#"<Relationships><Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "workbook sheet relationship is not a worksheet"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_missing_worksheet_member() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/missing.xml"/></Relationships>"#
        );
        let error =
            ordered_workbook_discovery(&mut fixture(&workbook, &relationships, &[])).unwrap_err();

        assert_eq!(error.to_string(), "worksheet package member is missing");
    }

    #[test]
    fn internal_relationship_targets_require_exact_package_members() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let missing_workbook_auxiliary = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles/missing.xml"/></Relationships>"#
        );
        let workbook_error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &missing_workbook_auxiliary,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();
        assert_eq!(
            workbook_error.to_string(),
            "workbook relationship package member is missing"
        );

        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdDrawing" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/missing.xml"/></Relationships>"#;
        let worksheet_members: [(&str, &[u8]); 2] = [
            ("xl/worksheets/sheet1.xml", b"worksheet"),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                worksheet_relationships,
            ),
        ];
        let worksheet_relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &worksheet_relationships,
            worksheet_members.as_slice(),
        ))
        .unwrap_err();
        assert_eq!(
            worksheet_error.to_string(),
            "worksheet relationship package member is missing"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_unknown_visibility_state() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" state="mystery" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"one")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "worksheet visibility state is invalid");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_zero_worksheets() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets/></workbook>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(&workbook, "<Relationships/>", &[]))
            .unwrap_err();

        assert_eq!(error.to_string(), "workbook contains no worksheets");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_251_worksheets() {
        let mut sheets = String::new();
        let mut relationship_elements = String::new();
        let mut members = Vec::new();
        for index in 1..=251 {
            sheets.push_str(&format!(
                r#"<sheet name="Sheet{index}" sheetId="{index}" r:id="rId{index}"/>"#
            ));
            relationship_elements.push_str(&format!(
                r#"<Relationship Id="rId{index}" Type="{REL}" Target="worksheets/sheet{index}.xml"/>"#
            ));
            members.push((
                format!("xl/worksheets/sheet{index}.xml"),
                Vec::from(b"sheet".as_slice()),
            ));
        }
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets>{sheets}</sheets></workbook>"#
        );
        let relationships = format!(r#"<Relationships>{relationship_elements}</Relationships>"#);
        let error = ordered_workbook_discovery(&mut fixture_from_members(
            &workbook,
            &relationships,
            members
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "workbook has too many worksheets");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_oversized_source_before_zip_parse() {
        let error = ordered_workbook_discovery(&mut SparseReader {
            len: super::MAX_PACKAGE_BYTES + 1,
            position: 0,
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "workbook source exceeded the size limit");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_declared_entry_count_before_zip_parse() {
        let error = ordered_workbook_discovery(&mut declared_entry_count_fixture(
            u16::try_from(super::MAX_ENTRIES + 1).unwrap(),
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "workbook ZIP entry count is invalid");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_decoy_eocd_before_parser_fallback() {
        let error =
            ordered_workbook_discovery(&mut append_decoy_eocd(ordered_fixture())).unwrap_err();

        assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_inflated_final_eocd_before_parser_fallback() {
        let error = ordered_workbook_discovery(&mut append_parser_fallback_eocd(ordered_fixture()))
            .unwrap_err();

        assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_semantically_invalid_final_directory() {
        let error = ordered_workbook_discovery(&mut append_semantically_invalid_directory(
            ordered_fixture(),
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "workbook ZIP metadata is invalid");
    }

    #[test]
    fn archive_preflight_rejects_entry_nonzero_disk_number_start() {
        assert_archive_preflight_rejects(mutate_first_central_field(
            ordered_fixture(),
            34,
            &1_u16.to_le_bytes(),
        ));
    }

    #[test]
    fn archive_preflight_rejects_entry_zip64_central_sentinels() {
        for field_offset in [20, 24, 42] {
            assert_archive_preflight_rejects(mutate_first_central_field(
                ordered_fixture(),
                field_offset,
                &u32::MAX.to_le_bytes(),
            ));
        }
        assert_archive_preflight_rejects(mutate_first_central_field(
            ordered_fixture(),
            34,
            &u16::MAX.to_le_bytes(),
        ));
    }

    #[test]
    fn archive_preflight_rejects_entry_zip64_central_extra_without_sentinel() {
        assert_archive_preflight_rejects(insert_first_central_extra(
            ordered_fixture(),
            &[0x01, 0x00, 0x00, 0x00],
        ));
    }

    #[test]
    fn archive_preflight_rejects_entry_malformed_central_extra() {
        assert_archive_preflight_rejects(insert_first_central_extra(
            ordered_fixture(),
            &[0xfe, 0xca, 0x04, 0x00],
        ));
    }

    #[test]
    fn archive_preflight_rejects_entry_zip64_local_size_sentinels() {
        for field_offset in [18, 22] {
            assert_archive_preflight_rejects(mutate_first_local_field(
                ordered_fixture(),
                field_offset,
                &u32::MAX.to_le_bytes(),
            ));
        }
    }

    #[test]
    fn archive_preflight_rejects_entry_zip64_local_extra() {
        assert_archive_preflight_rejects(insert_last_local_extra(
            ordered_fixture(),
            &[0x01, 0x00, 0x00, 0x00],
        ));
    }

    #[test]
    fn archive_preflight_rejects_entry_malformed_local_extra() {
        assert_archive_preflight_rejects(insert_last_local_extra(
            ordered_fixture(),
            &[0xfe, 0xca, 0x04, 0x00],
        ));
    }

    #[test]
    fn ordered_worksheet_discovery_reports_vba_project() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"not worksheet XML"),
                ("xl/vbaProject.bin", b"synthetic VBA bytes"),
            ],
        ))
        .unwrap();

        assert!(discovery.safety.has_vba_project);
    }

    #[test]
    fn ordered_worksheet_discovery_vba_relationship_reports_noncanonical_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="macros/project.bin"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/macros/project.bin", b"synthetic VBA bytes"),
            ],
        ))
        .unwrap();

        assert!(discovery.safety.has_vba_project);
    }

    #[test]
    fn ordered_worksheet_discovery_vba_relationship_rejects_missing_target_member() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="macros/project.bin"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "VBA project package member is missing");
    }

    #[test]
    fn ordered_worksheet_discovery_vba_relationship_retains_canonical_orphan_signal() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/vbaProject.bin", b"orphan VBA bytes"),
            ],
        ))
        .unwrap();

        assert!(discovery.safety.has_vba_project);
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_non_internal_auxiliary_workbook_target_modes() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        for (target_mode, expected_error) in [
            (
                "External",
                "external workbook relationship target is not allowed",
            ),
            ("Bogus", "workbook relationship target mode is invalid"),
        ] {
            let relationships = format!(
                r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin" TargetMode="{target_mode}"/></Relationships>"#
            );
            let error = ordered_workbook_discovery(&mut fixture(
                &workbook,
                &relationships,
                &[
                    ("xl/worksheets/sheet1.xml", b"worksheet"),
                    ("xl/vbaProject.bin", b"VBA"),
                ],
            ))
            .unwrap_err();

            assert_eq!(error.to_string(), expected_error);
        }
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_encoded_forbidden_auxiliary_workbook_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdVba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin%3Fignored"/></Relationships>"#
        );

        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/vbaProject.bin", b"VBA"),
            ],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "worksheet relationship escapes the package"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_reports_workbook_protection() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><workbookProtection lockStructure="1"/><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap();

        assert!(discovery.safety.has_workbook_protection);
    }

    #[test]
    fn ordered_worksheet_discovery_reports_xlm_macro_sheet_relationship() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdMacro" Type="http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet" Target="macrosheets/sheet2.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet cells are not read"),
                (
                    "xl/macrosheets/sheet2.xml",
                    b"macro sheet cells are not read",
                ),
            ],
        ))
        .unwrap();

        assert!(discovery.safety.has_xlm_macro_sheet);
    }

    #[test]
    fn ordered_worksheet_discovery_counts_external_links() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdLink1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="externalLinks/externalLink1.xml"/><Relationship Id="rIdLink2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="externalLinks/externalLink2.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/externalLinks/externalLink1.xml", b"link one"),
                ("xl/externalLinks/externalLink2.xml", b"link two"),
            ],
        ))
        .unwrap();

        assert_eq!(discovery.safety.external_link_count, 2);
    }

    #[test]
    fn ordered_worksheet_discovery_counts_explicit_internal_connections() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml" TargetMode="Internal"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/connections.xml",
                    format!(
                        r#"<connections xmlns="{MAIN}" count="2"><connection id="1"/><connection id="2"/></connections>"#
                    )
                    .as_bytes(),
                ),
            ],
        ))
        .unwrap();

        assert_eq!(discovery.safety.connection_count, 2);
    }

    #[test]
    fn ordered_worksheet_discovery_uses_canonical_connection_expanded_names() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml"/></Relationships>"#
        );
        let connections = format!(
            r#"<connections xmlns="{MAIN}" xmlns:x="urn:foreign" count="1" x:count="99"><connection id="1"/><x:connection id="2"/></connections>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/connections.xml", connections.as_bytes()),
            ],
        ))
        .unwrap();
        assert_eq!(discovery.safety.connection_count, 1);

        let foreign_connections =
            br#"<x:connections xmlns:x="urn:foreign"><x:connection id="1"/></x:connections>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/connections.xml", foreign_connections),
            ],
        ))
        .unwrap_err();
        assert_eq!(error.to_string(), "relationship count is invalid");
    }

    #[test]
    fn ordered_worksheet_discovery_counts_explicit_internal_query_tables_without_reading_cells() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml" TargetMode="Internal"/><Relationship Id="rIdQuery2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable2.xml" TargetMode="Internal"/></Relationships>"#;
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                (
                    "xl/worksheets/sheet1.xml",
                    b"deliberately malformed worksheet XML with secret cell text",
                ),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
                ("xl/queryTables/queryTable1.xml", b"query one"),
                ("xl/queryTables/queryTable2.xml", b"query two"),
            ],
        ))
        .unwrap();

        assert_eq!(discovery.safety.query_table_count, 2);
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_unknown_worksheet_relationship_class() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdMystery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/mysteryData" Target="../mystery.bin"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
                ("xl/mystery.bin", b"unknown data"),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "unknown worksheet relationship class");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_external_worksheet_relationship_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="https://example.invalid/query" TargetMode="External"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
            ],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "external worksheet relationship target is not allowed"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_spoofed_worksheet_relationship_namespace() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="urn:spoof"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
            ],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "worksheet relationships namespace is invalid"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_ignores_foreign_worksheet_relationship_elements() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships" xmlns:ext="urn:extension"><ext:Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
            ],
        ))
        .unwrap();

        assert_eq!(discovery.safety.query_table_count, 0);
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_worksheet_relationship_identity() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable2.xml"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
            ],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "duplicate worksheet relationship identity"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_duplicate_worksheet_relationship_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "duplicate worksheet relationship target");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_missing_worksheet_relationship_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
            ],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "worksheet relationship target is missing"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_encoded_forbidden_worksheet_relationship_target() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdQuery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml%3Fignored"/></Relationships>"#;
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    worksheet_relationships,
                ),
            ],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "worksheet relationship escapes the package"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_unknown_workbook_relationship_class() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdMystery" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/mysteryData" Target="mystery.bin"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                ("xl/mystery.bin", b"unknown data"),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "unknown workbook relationship class");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_spoofed_workbook_relationship_document_namespace() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="urn:spoof"><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "workbook relationships namespace is invalid"
        );
    }

    #[test]
    fn ordered_worksheet_discovery_ignores_foreign_workbook_relationship_elements() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships" xmlns:ext="urn:extension"><ext:Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet" Target="worksheets/decoy.xml" TargetMode="External"/><ext:Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="missing.xml"/><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/decoy.xml", b"decoy"),
                ("xl/worksheets/sheet1.xml", b"worksheet"),
            ],
        ))
        .unwrap();

        assert_eq!(discovery.worksheets.len(), 1);
        assert_eq!(
            discovery.worksheets[0].part_path,
            "xl/worksheets/sheet1.xml"
        );
        assert!(!discovery.safety.has_xlm_macro_sheet);
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_malformed_connection_count() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/connections.xml",
                    br#"<connections count="many"><connection id="1"/></connections>"#,
                ),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "relationship count is invalid");
    }

    #[test]
    fn ordered_worksheet_discovery_rejects_oversized_sheet_attribute() {
        let name = "N".repeat(super::MAX_SHEET_NAME_BYTES + 1);
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="{name}" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let error = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[("xl/worksheets/sheet1.xml", b"worksheet")],
        ))
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "workbook sheet name exceeded the size limit"
        );
    }
}
