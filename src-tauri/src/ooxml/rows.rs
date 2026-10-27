//! Structural row operations on a workbook package: relabel a cell, and insert a row
//! cloned from a template row the way Excel's Insert Row does (shifting rows,
//! references, ranges, conditional formatting, defined names and other sheets), with
//! shared formulas expanded first so every formula stays correct.

use std::io::Cursor;

use super::refs::{
    RowShift, references_sheet, shift_formula, shift_reference_list, translate_formula,
};
use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RowOp {
    /// Replace the text of one non-formula cell.
    #[serde(rename_all = "camelCase")]
    SetText {
        sheet: String,
        row: u32,
        column: u32,
        text: String,
    },
    /// Insert one row after `after_row`, cloned from `template_row`. Formulas are
    /// translated, values cleared except `copy_columns`, and `texts` set.
    #[serde(rename_all = "camelCase")]
    InsertRow {
        sheet: String,
        after_row: u32,
        template_row: u32,
        copy_columns: Vec<u32>,
        texts: Vec<CellText>,
    },
}

/// Text for one column of an inserted row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellText {
    pub column: u32,
    pub text: String,
}

struct Editor<'a, R: Read + Seek> {
    archive: ZipArchive<&'a mut R>,
    overlay: BTreeMap<String, Vec<u8>>,
    removed: BTreeSet<String>,
}

impl<R: Read + Seek> Editor<'_, R> {
    fn read(&mut self, name: &str) -> Result<Vec<u8>, OoxmlError> {
        if let Some(bytes) = self.overlay.get(name) {
            return Ok(bytes.clone());
        }
        read_entry(&mut self.archive, name)
    }

    fn put(&mut self, name: &str, bytes: Vec<u8>) {
        self.removed.remove(name);
        self.overlay.insert(name.to_string(), bytes);
    }

    fn names(&self) -> Result<Vec<String>, OoxmlError> {
        self.archive
            .file_names()
            .map(|name| Ok(name.map_err(OoxmlError::from)?.into_owned()))
            .collect()
    }
}

/// Applies `ops` in order to the package and returns the new package bytes.
pub fn apply_row_ops<R: Read + Seek>(source: &mut R, ops: &[RowOp]) -> Result<Vec<u8>, OoxmlError> {
    source.seek(SeekFrom::Start(0))?;
    let discovery = ordered_workbook_discovery(source)?;
    let sheets = project_sheet_infos(&discovery);
    source.seek(SeekFrom::Start(0))?;
    let archive = open_archive(&mut *source)?;
    let mut editor = Editor {
        archive,
        overlay: BTreeMap::new(),
        removed: BTreeSet::new(),
    };
    for op in ops {
        match op {
            RowOp::SetText {
                sheet,
                row,
                column,
                text,
            } => set_text(&mut editor, &sheets, sheet, *row, *column, text)?,
            RowOp::InsertRow {
                sheet,
                after_row,
                template_row,
                copy_columns,
                texts,
            } => insert_row(
                &mut editor,
                &sheets,
                sheet,
                *after_row,
                *template_row,
                copy_columns,
                texts,
            )?,
        }
    }
    let mut output = Cursor::new(Vec::new());
    write_package_edited(&mut editor, &mut output)?;
    Ok(output.into_inner())
}

fn write_package_edited<R: Read + Seek>(
    editor: &mut Editor<'_, R>,
    output: &mut Cursor<Vec<u8>>,
) -> Result<(), OoxmlError> {
    let comment = editor.archive.comment().to_vec();
    let mut writer = ZipWriter::new(&mut *output);
    writer.set_raw_comment(comment.into_boxed_slice())?;
    for index in 0..editor.archive.len() {
        let entry = editor.archive.by_index(index)?;
        let name = entry.name()?.into_owned();
        if editor.removed.contains(&name) {
            continue;
        }
        if let Some(bytes) = editor.overlay.get(&name) {
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
    writer.finish()?;
    output.set_position(0);
    Ok(())
}

fn sheet_path(sheets: &BTreeMap<String, SheetInfo>, sheet: &str) -> Result<String, OoxmlError> {
    sheets
        .get(sheet)
        .map(|info| info.path.clone())
        .ok_or_else(|| OoxmlError::invalid(format!("worksheet not found: {sheet}")))
}

fn walk_mut(nodes: &mut [XmlNode], visit: &mut impl FnMut(&mut XmlElement)) {
    for node in nodes {
        if let XmlNode::Element(element) = node {
            visit(element);
            walk_mut(&mut element.children, visit);
        }
    }
}

fn row_number(row: &XmlElement) -> Result<u32, OoxmlError> {
    attribute(row, b"r")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| OoxmlError::invalid("worksheet row identity is invalid"))
}

fn set_text(
    editor: &mut Editor<'_, impl Read + Seek>,
    sheets: &BTreeMap<String, SheetInfo>,
    sheet: &str,
    row: u32,
    column: u32,
    text: &str,
) -> Result<(), OoxmlError> {
    let path = sheet_path(sheets, sheet)?;
    let mut document = parse_document(&editor.read(&path)?)?;
    let sheet_data = find_element_mut(&mut document.nodes, b"sheetData")
        .ok_or_else(|| OoxmlError::invalid("worksheet sheetData is missing"))?;
    let reference = cell_reference(row, column);
    let target = child_elements_mut(sheet_data, b"row")
        .find(|candidate| row_number(candidate).ok() == Some(row))
        .and_then(|row_element| {
            child_elements_mut(row_element, b"c")
                .find(|cell| attribute(cell, b"r").as_deref() == Some(reference.as_str()))
        })
        .ok_or_else(|| OoxmlError::invalid("worksheet cell to relabel is missing"))?;
    if child_element(target, b"f").is_some() {
        return Err(OoxmlError::invalid("a formula cell cannot be relabeled"));
    }
    target
        .children
        .retain(|node| !matches!(node, XmlNode::Element(child) if has_local_name(child, b"v") || has_local_name(child, b"is")));
    set_inline_text(target, text);
    let bytes = serialize_document(&document)?;
    validate_worksheet_structure(&bytes)?;
    editor.put(&path, bytes);
    Ok(())
}

fn set_inline_text(cell: &mut XmlElement, text: &str) {
    set_attribute(cell, b"t", "inlineStr");
    let mut inline = XmlElement::new("is");
    let mut element = XmlElement::new("t");
    if text.trim() != text {
        set_attribute(&mut element, b"xml:space", "preserve");
    }
    set_element_text(&mut element, text);
    inline.children.push(XmlNode::Element(element));
    cell.children.push(XmlNode::Element(inline));
    cell.empty = false;
}

/// Expands every shared formula of the sheet into explicit formulas.
fn unshare_formulas(document: &mut XmlDocument) -> Result<(), OoxmlError> {
    let mut masters = BTreeMap::<String, (u32, u32, String)>::new();
    let mut cells = Vec::<(String, u32, u32)>::new();
    let mut failure = None;
    walk_mut(&mut document.nodes, &mut |element| {
        if !has_local_name(element, b"c") {
            return;
        }
        let Some((column, row)) = attribute(element, b"r")
            .and_then(|value| parse_reference(&value))
            .map(|(row, column)| (column, row))
        else {
            return;
        };
        for child in &element.children {
            let XmlNode::Element(f) = child else {
                continue;
            };
            if has_local_name(f, b"f") && attribute(f, b"t").as_deref() == Some("shared") {
                let Some(si) = attribute(f, b"si") else {
                    failure = Some("shared formula without si");
                    continue;
                };
                let text = element_text(f);
                if !text.is_empty() {
                    masters.insert(si.clone(), (row, column, text));
                }
                cells.push((si, row, column));
            }
        }
    });
    if let Some(message) = failure {
        return Err(OoxmlError::invalid(message));
    }
    if cells.is_empty() {
        return Ok(());
    }
    let mut error = None;
    walk_mut(&mut document.nodes, &mut |element| {
        if !has_local_name(element, b"c") {
            return;
        }
        let Some((row, column)) =
            attribute(element, b"r").and_then(|value| parse_reference(&value))
        else {
            return;
        };
        for child in &mut element.children {
            let XmlNode::Element(f) = child else {
                continue;
            };
            if !has_local_name(f, b"f") || attribute(f, b"t").as_deref() != Some("shared") {
                continue;
            }
            let si = attribute(f, b"si").unwrap_or_default();
            let Some((master_row, master_column, master_text)) = masters.get(&si) else {
                error = Some("shared formula master is missing");
                continue;
            };
            let Some(text) = translate_formula(
                master_text,
                i64::from(row) - i64::from(*master_row),
                i64::from(column) - i64::from(*master_column),
            ) else {
                error = Some("shared formula cannot be expanded");
                continue;
            };
            remove_attribute(f, b"t");
            remove_attribute(f, b"si");
            remove_attribute(f, b"ref");
            set_element_text(f, &text);
        }
    });
    error.map_or(Ok(()), |message| Err(OoxmlError::invalid(message)))
}

const FORMULA_ELEMENTS: [&[u8]; 4] = [b"f", b"formula", b"formula1", b"formula2"];
const REFERENCE_ATTRIBUTES: [&[u8]; 4] = [b"ref", b"sqref", b"topLeftCell", b"activeCell"];

fn rewrite_formulas(
    nodes: &mut [XmlNode],
    current_sheet: &str,
    shift: &RowShift,
    reject_shared_references: bool,
) -> Result<bool, OoxmlError> {
    let mut changed = false;
    let mut error = None;
    walk_mut(nodes, &mut |element| {
        let is_formula = FORMULA_ELEMENTS
            .iter()
            .any(|name| has_local_name(element, name));
        if !is_formula
            && !has_local_name(element, b"sqref")
            && !has_local_name(element, b"definedName")
        {
            return;
        }
        let text = element_text(element);
        if text.is_empty() {
            return;
        }
        let rewritten = if has_local_name(element, b"sqref") {
            shift_reference_list(&text, current_sheet, shift)
        } else {
            if reject_shared_references
                && attribute(element, b"t").as_deref() == Some("shared")
                && references_sheet(&text, current_sheet, &shift.sheet)
            {
                error = Some("a shared formula on another sheet references the changed sheet");
                return;
            }
            shift_formula(&text, current_sheet, shift)
        };
        if rewritten != text {
            set_element_text(element, &rewritten);
            changed = true;
        }
    });
    error.map_or(Ok(changed), |message| Err(OoxmlError::invalid(message)))
}

fn shift_sheet_attributes(nodes: &mut [XmlNode], sheet: &str, shift: &RowShift) {
    walk_mut(nodes, &mut |element| {
        if has_local_name(element, b"c") || has_local_name(element, b"row") {
            return;
        }
        for name in REFERENCE_ATTRIBUTES {
            if let Some(value) = attribute(element, name) {
                let rewritten = shift_reference_list(&value, sheet, shift);
                if rewritten != value {
                    set_attribute(element, name, &rewritten);
                }
            }
        }
        if has_local_name(element, b"brk")
            && let Some(id) = attribute(element, b"id").and_then(|value| value.parse::<u32>().ok())
            && id >= shift.at
        {
            set_attribute(element, b"id", &(id + shift.count).to_string());
        }
    });
}

fn insert_row(
    editor: &mut Editor<'_, impl Read + Seek>,
    sheets: &BTreeMap<String, SheetInfo>,
    sheet: &str,
    after_row: u32,
    template_row: u32,
    copy_columns: &[u32],
    texts: &[CellText],
) -> Result<(), OoxmlError> {
    if template_row == 0 || template_row > after_row + 1 || after_row == 0 && template_row != 1 {
        return Err(OoxmlError::invalid("row template is invalid"));
    }
    let at = after_row + 1;
    let shift = RowShift {
        sheet: sheet.to_string(),
        at,
        count: 1,
    };
    let path = sheet_path(sheets, sheet)?;
    let mut document = parse_document(&editor.read(&path)?)?;
    unshare_formulas(&mut document)?;
    rewrite_formulas(&mut document.nodes, sheet, &shift, false)?;
    shift_sheet_attributes(&mut document.nodes, sheet, &shift);

    // Shift the rows below the insertion and clone the template row.
    let sheet_data = find_element_mut(&mut document.nodes, b"sheetData")
        .ok_or_else(|| OoxmlError::invalid("worksheet sheetData is missing"))?;
    let mut template = None::<XmlElement>;
    for node in &mut sheet_data.children {
        let XmlNode::Element(row) = node else {
            continue;
        };
        if !has_local_name(row, b"row") {
            continue;
        }
        let number = row_number(row)?;
        // The template is read before any shifting, from a row at or above the insertion
        // (or the first row below it when inserting before the first data row).
        if number == template_row {
            template = Some(row.clone());
        }
        if number >= at {
            set_attribute(row, b"r", &(number + 1).to_string());
            for cell in child_elements_mut(row, b"c") {
                if let Some((column, _)) = attribute(cell, b"r")
                    .and_then(|value| parse_reference(&value))
                    .map(|(row, column)| (column, row))
                {
                    set_attribute(cell, b"r", &cell_reference(number + 1, column));
                }
            }
        }
    }
    let mut template = template.ok_or_else(|| OoxmlError::invalid("row template is missing"))?;
    // When the template sat at or below the insertion it has been renumbered.
    let template_number = if template_row >= at {
        template_row + 1
    } else {
        template_row
    };
    let new_row = at;
    build_new_row(&mut template, template_number, new_row, copy_columns, texts)?;

    let sheet_data = find_element_mut(&mut document.nodes, b"sheetData")
        .ok_or_else(|| OoxmlError::invalid("worksheet sheetData is missing"))?;
    let position = sheet_data
        .children
        .iter()
        .position(|node| matches!(node, XmlNode::Element(row) if has_local_name(row, b"row") && row_number(row).is_ok_and(|number| number > new_row)))
        .unwrap_or(sheet_data.children.len());
    sheet_data
        .children
        .insert(position, XmlNode::Element(template));
    let bytes = serialize_document(&document)?;
    validate_worksheet_structure(&bytes)?;
    editor.put(&path, bytes);

    // Other worksheets, charts and the workbook's defined names.
    let names = editor.names()?;
    for (other_name, info) in sheets {
        if other_name == sheet {
            continue;
        }
        let mut other = parse_document(&editor.read(&info.path)?)?;
        if rewrite_formulas(&mut other.nodes, other_name, &shift, true)? {
            let bytes = serialize_document(&other)?;
            validate_worksheet_structure(&bytes)?;
            editor.put(&info.path, bytes);
        }
    }
    for name in names {
        if name.starts_with("xl/charts/") && name.ends_with(".xml") {
            let mut chart = parse_document(&editor.read(&name)?)?;
            if rewrite_formulas(&mut chart.nodes, "", &shift, false)? {
                editor.put(&name, serialize_document(&chart)?);
            }
        }
    }
    let mut workbook = parse_document(&editor.read("xl/workbook.xml")?)?;
    if rewrite_formulas(&mut workbook.nodes, "", &shift, false)? {
        editor.put("xl/workbook.xml", serialize_document(&workbook)?);
    }
    drop_calc_chain(editor)?;
    Ok(())
}

fn build_new_row(
    row: &mut XmlElement,
    template_number: u32,
    new_row: u32,
    copy_columns: &[u32],
    texts: &[CellText],
) -> Result<(), OoxmlError> {
    set_attribute(row, b"r", &new_row.to_string());
    let delta = i64::from(new_row) - i64::from(template_number);
    for node in &mut row.children {
        let XmlNode::Element(cell) = node else {
            continue;
        };
        if !has_local_name(cell, b"c") {
            continue;
        }
        let (column, _) = attribute(cell, b"r")
            .and_then(|value| parse_reference(&value))
            .map(|(row, column)| (column, row))
            .ok_or_else(|| OoxmlError::invalid("worksheet cell reference is invalid"))?;
        set_attribute(cell, b"r", &cell_reference(new_row, column));
        let keeps_value = copy_columns.contains(&column);
        for attribute_name in [&b"cm"[..], b"vm", b"ph"] {
            remove_attribute(cell, attribute_name);
        }
        let mut formula_text = None;
        for child in &cell.children {
            if let XmlNode::Element(f) = child
                && has_local_name(f, b"f")
            {
                formula_text = Some(element_text(f));
            }
        }
        if let Some(text) = formula_text {
            let translated = translate_formula(&text, delta, 0)
                .ok_or_else(|| OoxmlError::invalid("row formula cannot be translated"))?;
            cell.children
                .retain(|child| matches!(child, XmlNode::Element(f) if has_local_name(f, b"f")));
            for child in &mut cell.children {
                if let XmlNode::Element(f) = child {
                    remove_attribute(f, b"ref");
                    set_element_text(f, &translated);
                }
            }
            remove_attribute(cell, b"t");
        } else if !keeps_value {
            cell.children.retain(|_| false);
            remove_attribute(cell, b"t");
            cell.empty = true;
        }
        if let Some(entry) = texts.iter().find(|entry| entry.column == column) {
            cell.children.clear();
            set_inline_text(cell, &entry.text);
        }
    }
    Ok(())
}

fn drop_calc_chain<R: Read + Seek>(editor: &mut Editor<'_, R>) -> Result<(), OoxmlError> {
    if !editor
        .names()?
        .iter()
        .any(|name| name == "xl/calcChain.xml")
    {
        return Ok(());
    }
    editor.removed.insert("xl/calcChain.xml".to_string());
    editor.overlay.remove("xl/calcChain.xml");

    let mut relationships = parse_document(&editor.read("xl/_rels/workbook.xml.rels")?)?;
    if let Some(root) = find_element_mut(&mut relationships.nodes, b"Relationships") {
        root.children.retain(|node| {
            !matches!(node, XmlNode::Element(child)
                if has_local_name(child, b"Relationship")
                    && attribute(child, b"Type").is_some_and(|value| value.ends_with("/calcChain")))
        });
    }
    editor.put(
        "xl/_rels/workbook.xml.rels",
        serialize_document(&relationships)?,
    );

    let mut content_types = parse_document(&editor.read("[Content_Types].xml")?)?;
    if let Some(root) = find_element_mut(&mut content_types.nodes, b"Types") {
        root.children.retain(|node| {
            !matches!(node, XmlNode::Element(child)
                if has_local_name(child, b"Override")
                    && attribute(child, b"PartName").as_deref() == Some("/xl/calcChain.xml"))
        });
    }
    editor.put("[Content_Types].xml", serialize_document(&content_types)?);
    Ok(())
}
