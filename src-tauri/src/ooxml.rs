use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt, fs,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

use quick_xml::{
    Reader, Writer,
    escape::unescape,
    events::{BytesEnd, BytesStart, BytesText, Event},
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
    pub part_path: String,
    pub name: String,
    pub visibility: WorksheetVisibility,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookSafetyFacts {
    pub has_vba_project: bool,
    pub has_xlm_macro_sheet: bool,
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
    children: Vec<XmlNode>,
    empty: bool,
}

#[derive(Clone)]
struct XmlDocument {
    nodes: Vec<XmlNode>,
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

    let input = File::open(source)?;
    let mut archive = open_archive(input)?;
    let sheet_infos = sheet_infos(&mut archive)?;
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
    let mut archive = open_archive(File::open(source)?)?;
    sheet_infos(&mut archive)
}

pub fn ordered_workbook_discovery<R: Read + Seek>(
    source: &mut R,
) -> Result<OrderedWorkbookDiscovery, OoxmlError> {
    let mut archive = open_archive(source)?;
    let workbook = parse_document(&read_entry(&mut archive, "xl/workbook.xml")?)?;
    let relationships = parse_document(&read_entry(&mut archive, "xl/_rels/workbook.xml.rels")?)?;
    let relation_targets = relationship_targets(&relationships)?;
    let connection_count = related_collection_count(
        &mut archive,
        &relationships,
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
    for (index, element) in descendants(&workbook.nodes)
        .filter(|element| has_local_name(element, b"sheet"))
        .enumerate()
    {
        if index >= MAX_WORKSHEETS {
            return Err(OoxmlError::invalid("workbook has too many worksheets"));
        }
        let name = bounded_attribute(
            element,
            b"name",
            MAX_SHEET_NAME_BYTES,
            "workbook sheet name",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook sheet name is missing"))?;
        if !names.insert(name.clone()) {
            return Err(OoxmlError::invalid("duplicate workbook sheet name"));
        }
        let relationship_id = bounded_attribute(
            element,
            b"id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook sheet relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook sheet relationship is missing"))?;
        if !relationship_ids.insert(relationship_id.clone()) {
            return Err(OoxmlError::invalid(
                "duplicate workbook sheet relationship identity",
            ));
        }
        let sheet_id = bounded_attribute(
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
        if relationship_attribute(&relationships, &relationship_id, b"Type")?
            .as_deref()
            .and_then(ooxml_relationship_class)
            != Some("worksheet")
        {
            return Err(OoxmlError::invalid(
                "workbook sheet relationship is not a worksheet",
            ));
        }
        let state = bounded_attribute(
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
        let part_path = normalize_sheet_target(target)?;
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
            part_path,
            name,
            visibility,
        });
    }
    if worksheets.is_empty() {
        return Err(OoxmlError::invalid("workbook contains no worksheets"));
    }
    validate_workbook_relationship_classes(&relationships)?;
    safety.query_table_count =
        worksheet_relationship_type_count(&mut archive, &worksheets, "queryTable")?;
    Ok(OrderedWorkbookDiscovery { worksheets, safety })
}

pub fn read_workbook(source: &Path) -> Result<WorkbookData, OoxmlError> {
    validate_source(source)?;
    let mut archive = open_archive(File::open(source)?)?;
    let infos = sheet_infos(&mut archive)?;
    let shared_strings = if has_entry(&mut archive, "xl/sharedStrings.xml")? {
        shared_strings(&read_entry(&mut archive, "xl/sharedStrings.xml")?)?
    } else {
        Vec::new()
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
    preflight_archive(&mut source)?;
    let mut archive = ZipArchive::new(source)?;
    validate_archive(&mut archive)?;
    Ok(archive)
}

fn preflight_archive<R: Read + Seek>(source: &mut R) -> Result<(), OoxmlError> {
    const EOCD_MIN_BYTES: usize = 22;
    const EOCD_MAX_COMMENT_BYTES: usize = u16::MAX as usize;
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";

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
    source.seek(SeekFrom::Start(0))?;
    Ok(())
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

fn sheet_infos(archive: &mut ZipArchive<File>) -> Result<BTreeMap<String, SheetInfo>, OoxmlError> {
    let workbook = parse_document(&read_entry(archive, "xl/workbook.xml")?)?;
    let relationships = parse_document(&read_entry(archive, "xl/_rels/workbook.xml.rels")?)?;
    let relation_targets = relationship_targets(&relationships)?;
    let mut sheets = BTreeMap::new();
    for element in descendants(&workbook.nodes).filter(|element| has_local_name(element, b"sheet"))
    {
        let name = attribute(element, b"name")
            .ok_or_else(|| OoxmlError::invalid("workbook sheet name is missing"))?;
        let relation_id = attribute(element, b"id")
            .ok_or_else(|| OoxmlError::invalid("workbook sheet relationship is missing"))?;
        let sheet_id = attribute(element, b"sheetId")
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| OoxmlError::invalid("workbook sheet identity is invalid"))?;
        let target = relation_targets
            .get(&relation_id)
            .ok_or_else(|| OoxmlError::invalid("workbook sheet target is missing"))?;
        let path = normalize_sheet_target(target)?;
        if sheets.insert(name, SheetInfo { path, sheet_id }).is_some() {
            return Err(OoxmlError::invalid("duplicate workbook sheet name"));
        }
    }
    if sheets.is_empty() {
        return Err(OoxmlError::invalid("workbook contains no sheets"));
    }
    Ok(sheets)
}

fn relationship_targets(document: &XmlDocument) -> Result<HashMap<String, String>, OoxmlError> {
    let mut targets = HashMap::new();
    for element in
        descendants(&document.nodes).filter(|element| has_local_name(element, b"Relationship"))
    {
        let id = bounded_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship identity is missing"))?;
        let target = bounded_attribute(
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

fn validate_workbook_relationship_classes(document: &XmlDocument) -> Result<(), OoxmlError> {
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

    for relationship in
        descendants(&document.nodes).filter(|element| has_local_name(element, b"Relationship"))
    {
        let relationship_type = bounded_attribute(
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
    }
    Ok(())
}

fn relationship_target_is_external(
    document: &XmlDocument,
    relationship_id: &str,
) -> Result<bool, OoxmlError> {
    for element in
        descendants(&document.nodes).filter(|element| has_local_name(element, b"Relationship"))
    {
        let id = bounded_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?;
        if id.as_deref() == Some(relationship_id) {
            return Ok(bounded_attribute(
                element,
                b"TargetMode",
                MAX_STATE_BYTES,
                "workbook relationship target mode",
            )?
            .is_some());
        }
    }
    Ok(false)
}

fn relationship_attribute(
    document: &XmlDocument,
    relationship_id: &str,
    name: &[u8],
) -> Result<Option<String>, OoxmlError> {
    for element in
        descendants(&document.nodes).filter(|element| has_local_name(element, b"Relationship"))
    {
        let id = bounded_attribute(
            element,
            b"Id",
            MAX_RELATIONSHIP_ID_BYTES,
            "workbook relationship identity",
        )?;
        if id.as_deref() == Some(relationship_id) {
            return bounded_attribute(
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
    for element in
        descendants(&document.nodes).filter(|element| has_local_name(element, b"Relationship"))
    {
        if bounded_attribute(
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
    for element in
        descendants(&document.nodes).filter(|element| has_local_name(element, b"Relationship"))
    {
        if bounded_attribute(
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
    expected_class: &str,
    collection_name: &[u8],
    item_name: &[u8],
) -> Result<u32, OoxmlError> {
    let mut count = 0_u32;
    for relationship in
        descendants(&relationships.nodes).filter(|element| has_local_name(element, b"Relationship"))
    {
        let Some(relationship_type) = bounded_attribute(
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
        if bounded_attribute(
            relationship,
            b"TargetMode",
            MAX_STATE_BYTES,
            "workbook relationship target mode",
        )?
        .is_some()
        {
            return Err(OoxmlError::invalid(
                "external workbook relationship target is not allowed",
            ));
        }
        let target = bounded_attribute(
            relationship,
            b"Target",
            MAX_RELATIONSHIP_TARGET_BYTES,
            "workbook relationship target",
        )?
        .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        let part_path = normalize_sheet_target(&target)?;
        let document = parse_document(&read_entry(archive, &part_path)?)?;
        let part_count = descendants(&document.nodes)
            .filter(|element| has_local_name(element, item_name))
            .count();
        let mut collections =
            descendants(&document.nodes).filter(|element| has_local_name(element, collection_name));
        let collection = collections
            .next()
            .ok_or_else(|| OoxmlError::invalid("relationship count is invalid"))?;
        if collections.next().is_some() {
            return Err(OoxmlError::invalid("relationship count is invalid"));
        }
        if let Some(declared) = bounded_attribute(
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
        let part = Path::new(&worksheet.part_path);
        let parent = part
            .parent()
            .and_then(Path::to_str)
            .ok_or_else(|| OoxmlError::invalid("worksheet part path is invalid"))?;
        let file_name = part
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| OoxmlError::invalid("worksheet part path is invalid"))?;
        let relationships_path = format!("{parent}/_rels/{file_name}.rels");
        if !has_entry(archive, &relationships_path)? {
            continue;
        }
        let relationships = parse_document(&read_entry(archive, &relationships_path)?)?;
        count = count
            .checked_add(relationship_type_count(&relationships, expected_class)?)
            .ok_or_else(|| OoxmlError::invalid("relationship count is invalid"))?;
    }
    Ok(count)
}

fn normalize_sheet_target(target: &str) -> Result<String, OoxmlError> {
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
    let path = if target.starts_with('/') {
        PathBuf::from(target.trim_start_matches('/'))
    } else {
        PathBuf::from("xl").join(target)
    };
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| {
                        OoxmlError::invalid("worksheet relationship escapes the package")
                    })?
                    .to_string(),
            ),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(OoxmlError::invalid(
                    "worksheet relationship escapes the package",
                ));
            }
        }
    }
    if parts.first().map(String::as_str) != Some("xl") || parts.len() < 2 {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    Ok(parts.join("/"))
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
    if raw.len() as u64 > MAX_ENTRY_BYTES {
        return Err(OoxmlError::invalid("XML member exceeded the size limit"));
    }
    let mut reader = Reader::from_reader(raw);
    reader.config_mut().trim_text(false);
    let mut roots = Vec::<XmlNode>::new();
    let mut stack = Vec::<XmlElement>::new();
    loop {
        match reader.read_event()? {
            Event::Start(start) => stack.push(XmlElement {
                start: start.into_owned(),
                children: Vec::new(),
                empty: false,
            }),
            Event::Empty(start) => push_node(
                &mut roots,
                &mut stack,
                XmlNode::Element(XmlElement {
                    start: start.into_owned(),
                    children: Vec::new(),
                    empty: true,
                }),
            ),
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
            event => push_node(&mut roots, &mut stack, XmlNode::Raw(event.into_owned())),
        }
    }
    if !stack.is_empty() || !roots.iter().any(|node| matches!(node, XmlNode::Element(_))) {
        return Err(OoxmlError::invalid("XML document is incomplete"));
    }
    Ok(XmlDocument { nodes: roots })
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

fn bounded_attribute(
    element: &XmlElement,
    local: &[u8],
    max_bytes: usize,
    field: &str,
) -> Result<Option<String>, OoxmlError> {
    let mut value = None;
    for attribute in element.start.attributes() {
        let attribute =
            attribute.map_err(|_| OoxmlError::invalid(format!("{field} is invalid")))?;
        let is_match = attribute
            .key
            .as_ref()
            .rsplit(|byte| *byte == b':')
            .next()
            .is_some_and(|name| name == local);
        if !is_match {
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
    use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::{WorksheetDescriptor, WorksheetVisibility, ordered_workbook_discovery};

    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const REL: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";

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

    fn fixture_from_members<'a>(
        workbook: &'a str,
        relationships: &'a str,
        members: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Cursor<Vec<u8>> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for (name, bytes) in [
            ("xl/workbook.xml", workbook.as_bytes()),
            ("xl/_rels/workbook.xml.rels", relationships.as_bytes()),
        ]
        .into_iter()
        .chain(members)
        {
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
    fn ordered_worksheet_discovery_preserves_order_identity_and_visibility() {
        let discovery = ordered_workbook_discovery(&mut ordered_fixture()).unwrap();

        assert_eq!(
            discovery.worksheets,
            vec![
                WorksheetDescriptor {
                    workbook_index: 1,
                    sheet_id: 17,
                    relationship_id: "rId7".to_string(),
                    part_path: "xl/worksheets/sheet7.xml".to_string(),
                    name: "MEC".to_string(),
                    visibility: WorksheetVisibility::Visible,
                },
                WorksheetDescriptor {
                    workbook_index: 2,
                    sheet_id: 3,
                    relationship_id: "rId2".to_string(),
                    part_path: "xl/worksheets/sheet2.xml".to_string(),
                    name: "CIV".to_string(),
                    visibility: WorksheetVisibility::Visible,
                },
                WorksheetDescriptor {
                    workbook_index: 3,
                    sheet_id: 44,
                    relationship_id: "rId9".to_string(),
                    part_path: "xl/worksheets/sheet9.xml".to_string(),
                    name: "Hidden".to_string(),
                    visibility: WorksheetVisibility::Hidden,
                },
                WorksheetDescriptor {
                    workbook_index: 4,
                    sheet_id: 45,
                    relationship_id: "rId10".to_string(),
                    part_path: "xl/worksheets/sheet10.xml".to_string(),
                    name: "VeryHidden".to_string(),
                    visibility: WorksheetVisibility::VeryHidden,
                },
            ]
        );
        assert_eq!(discovery.safety, Default::default());
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
        let workbook = format!(r#"<workbook xmlns="{MAIN}"><sheets/></workbook>"#);
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
    fn ordered_worksheet_discovery_counts_connections() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/><Relationship Id="rIdConnections" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections" Target="connections.xml"/></Relationships>"#
        );
        let discovery = ordered_workbook_discovery(&mut fixture(
            &workbook,
            &relationships,
            &[
                ("xl/worksheets/sheet1.xml", b"worksheet"),
                (
                    "xl/connections.xml",
                    br#"<connections count="2"><connection id="1"/><connection id="2"/></connections>"#,
                ),
            ],
        ))
        .unwrap();

        assert_eq!(discovery.safety.connection_count, 2);
    }

    #[test]
    fn ordered_worksheet_discovery_counts_query_tables_without_reading_cells() {
        let workbook = format!(
            r#"<workbook xmlns="{MAIN}" xmlns:r="urn:r"><sheets><sheet name="MEC" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships><Relationship Id="rId1" Type="{REL}" Target="worksheets/sheet1.xml"/></Relationships>"#
        );
        let worksheet_relationships = br#"<Relationships><Relationship Id="rIdQuery1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable1.xml"/><Relationship Id="rIdQuery2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable" Target="../queryTables/queryTable2.xml"/></Relationships>"#;
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
