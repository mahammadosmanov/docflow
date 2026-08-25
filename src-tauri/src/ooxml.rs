use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt, fs,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use quick_xml::{
    Reader, Writer,
    escape::unescape,
    events::{BytesEnd, BytesStart, BytesText, Event},
};
use zip::{ZipArchive, ZipWriter};

const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 4_096;

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
    let mut archive = ZipArchive::new(input)?;
    validate_archive(&mut archive)?;
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
    let mut archive = ZipArchive::new(File::open(source)?)?;
    validate_archive(&mut archive)?;
    read_entry(&mut archive, name)
}

pub fn sheet_infos_from_path(source: &Path) -> Result<BTreeMap<String, SheetInfo>, OoxmlError> {
    validate_source(source)?;
    let mut archive = ZipArchive::new(File::open(source)?)?;
    validate_archive(&mut archive)?;
    sheet_infos(&mut archive)
}

pub fn read_workbook(source: &Path) -> Result<WorkbookData, OoxmlError> {
    validate_source(source)?;
    let mut archive = ZipArchive::new(File::open(source)?)?;
    validate_archive(&mut archive)?;
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

fn validate_archive(archive: &mut ZipArchive<File>) -> Result<(), OoxmlError> {
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

fn read_entry(archive: &mut ZipArchive<File>, name: &str) -> Result<Vec<u8>, OoxmlError> {
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

fn has_entry(archive: &mut ZipArchive<File>, name: &str) -> Result<bool, OoxmlError> {
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
        let id = attribute(element, b"Id")
            .ok_or_else(|| OoxmlError::invalid("workbook relationship identity is missing"))?;
        let target = attribute(element, b"Target")
            .ok_or_else(|| OoxmlError::invalid("workbook relationship target is missing"))?;
        targets.insert(id, target);
    }
    Ok(targets)
}

fn normalize_sheet_target(target: &str) -> Result<String, OoxmlError> {
    let target = target.replace('\\', "/");
    let path = if target.starts_with('/') {
        PathBuf::from(target.trim_start_matches('/'))
    } else {
        PathBuf::from("xl").join(target)
    };
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || !path.starts_with("xl")
    {
        return Err(OoxmlError::invalid(
            "worksheet relationship escapes the package",
        ));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
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
