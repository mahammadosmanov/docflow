//! Leaving whole sheets out of a generated Excel file (ADR 0058).
//!
//! An operator marks some sheets optional; at generation each document can
//! leave them out. Removal is refused when it would break something that is
//! kept: a formula or a workbook name that uses the sheet. A sheet leaves with
//! its relationships, drawing, comments, and any picture no kept part uses.
//! The calculation chain is dropped; Excel rebuilds it on open.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Write},
    sync::LazyLock,
};

use quick_xml::escape::unescape;
use regex::Regex;
use zip::{ZipWriter, write::SimpleFileOptions};

use super::field_map::read_package;

const WORKBOOK: &str = "xl/workbook.xml";
const WORKBOOK_RELS: &str = "xl/_rels/workbook.xml.rels";
const CONTENT_TYPES: &str = "[Content_Types].xml";
const CALC_CHAIN: &str = "xl/calcChain.xml";

static SHEET_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<sheet\b[^>]*/>").expect("static sheet tag"));
static DEFINED_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<definedName\b[^>]*>.*?</definedName>|<definedName\b[^>]*/>")
        .expect("static defined name")
});
static RELATIONSHIP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<Relationship\b[^>]*/>").expect("static relationship"));
static OVERRIDE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<Override\b[^>]*/>").expect("static override"));
static NAME_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\sname="([^"]*)""#).expect("static name attr"));
static RID_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\s[A-Za-z0-9]+:id="([^"]*)""#).expect("static r:id attr"));
static ID_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\sId="([^"]*)""#).expect("static Id attr"));
static TARGET_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\sTarget="([^"]*)""#).expect("static Target attr"));
static TYPE_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\sType="([^"]*)""#).expect("static Type attr"));
static EXTERNAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\sTargetMode="External""#).expect("static external"));
static PART_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\sPartName="([^"]*)""#).expect("static PartName"));
static LOCAL_SHEET_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"localSheetId="(\d+)""#).expect("static localSheetId"));
static ACTIVE_TAB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b(activeTab|firstSheet)="\d+""#).expect("static activeTab"));

fn attr(pattern: &Regex, tag: &str) -> Option<String> {
    pattern.captures(tag).map(|found| found[1].to_string())
}

/// `base` is the folder of the part that owns the relationship.
fn resolve(base: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        base.split('/').filter(|part| !part.is_empty()).collect()
    };
    for piece in target.split('/') {
        match piece {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn folder_of(part: &str) -> &str {
    part.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The part a `.rels` file belongs to (`xl/worksheets/_rels/a.xml.rels`).
fn owner_of(rels: &str) -> Option<String> {
    let (folder, file) = rels.rsplit_once('/')?;
    let folder = folder.strip_suffix("_rels")?.trim_end_matches('/');
    let name = file.strip_suffix(".rels")?;
    (!name.is_empty()).then(|| {
        if folder.is_empty() {
            name.to_string()
        } else {
            format!("{folder}/{name}")
        }
    })
}

fn rels_of(part: &str) -> String {
    let folder = folder_of(part);
    let name = part.rsplit('/').next().unwrap_or(part);
    if folder.is_empty() {
        format!("_rels/{name}.rels")
    } else {
        format!("{folder}/_rels/{name}.rels")
    }
}

/// Internal targets of a relationships file, resolved to part names.
fn targets_of(rels_xml: &str, owner: &str) -> Vec<String> {
    RELATIONSHIP
        .find_iter(rels_xml)
        .filter(|tag| !EXTERNAL.is_match(tag.as_str()))
        .filter_map(|tag| attr(&TARGET_ATTR, tag.as_str()))
        .map(|target| resolve(folder_of(owner), &target))
        .collect()
}

pub fn remove_sheets(package: &[u8], names: &[String]) -> Result<Vec<u8>, String> {
    if names.is_empty() {
        return Ok(package.to_vec());
    }
    let invalid = |error: crate::ooxml::OoxmlError| format!("Cannot read the workbook: {error}");
    let discovery =
        crate::ooxml::ordered_workbook_discovery(&mut Cursor::new(package)).map_err(invalid)?;
    if discovery.safety.has_workbook_protection {
        return Err("The workbook is protected".to_string());
    }
    if crate::ooxml::package_has_signature(&mut Cursor::new(package)).map_err(invalid)? {
        return Err("The workbook is signed".to_string());
    }
    let order = discovery
        .worksheets
        .iter()
        .map(|sheet| sheet.name.as_str())
        .collect::<Vec<_>>();
    let removed = names.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if let Some(unknown) = removed.iter().find(|name| !order.contains(name)) {
        return Err(format!("The workbook has no sheet named {unknown}"));
    }
    let keeps_visible = discovery.worksheets.iter().any(|sheet| {
        !removed.contains(sheet.name.as_str())
            && sheet.visibility == crate::ooxml::WorksheetVisibility::Visible
    });
    if !keeps_visible {
        return Err("At least one sheet must stay".to_string());
    }

    // Nothing kept may use a removed sheet.
    let workbook =
        crate::ooxml::read_workbook_from_reader(&mut Cursor::new(package)).map_err(invalid)?;
    for kept in order.iter().filter(|name| !removed.contains(*name)) {
        let Some(sheet) = workbook.sheet(kept) else {
            continue;
        };
        for gone in &removed {
            if sheet.cells.values().any(|cell| {
                cell.formula
                    .as_deref()
                    .is_some_and(|formula| crate::ooxml::references_sheet(formula, kept, gone))
            }) {
                return Err(format!("Sheet {gone} is used by a formula on sheet {kept}"));
            }
        }
    }

    let package_parts = read_package(package)?;
    let mut entries = package_parts
        .entries
        .iter()
        .map(|(name, data)| (name.clone(), data.clone()))
        .collect::<BTreeMap<_, _>>();
    let text_of = |entries: &BTreeMap<String, Vec<u8>>, name: &str| {
        entries
            .get(name)
            .map(|data| String::from_utf8_lossy(data).into_owned())
            .ok_or_else(|| format!("The workbook is missing {name}"))
    };

    // workbook.xml: drop the sheets, fix names and the open tab.
    let mut workbook_xml = text_of(&entries, WORKBOOK)?;
    let sheet_tags = SHEET_TAG
        .find_iter(&workbook_xml)
        .map(|tag| tag.as_str().to_string())
        .collect::<Vec<_>>();
    let mut removed_indices = BTreeSet::new();
    let mut removed_ids = BTreeSet::new();
    for (index, tag) in sheet_tags.iter().enumerate() {
        let name = attr(&NAME_ATTR, tag).unwrap_or_default();
        let name = unescape(&name).map_or(name.clone(), |text| text.into_owned());
        if removed.contains(name.as_str()) {
            removed_indices.insert(index as u32);
            removed_ids.extend(attr(&RID_ATTR, tag));
            workbook_xml = workbook_xml.replacen(tag, "", 1);
        }
    }
    if removed_indices.len() != removed.len() {
        return Err("The workbook's sheet list does not match its sheets".to_string());
    }
    let mut names_error = None;
    workbook_xml = DEFINED_NAME
        .replace_all(&workbook_xml, |found: &regex::Captures<'_>| {
            let element = found[0].to_string();
            match attr(&LOCAL_SHEET_ID, &element).and_then(|id| id.parse::<u32>().ok()) {
                Some(id) if removed_indices.contains(&id) => String::new(),
                Some(id) => {
                    let shift = removed_indices.iter().filter(|gone| **gone < id).count() as u32;
                    LOCAL_SHEET_ID
                        .replace(&element, format!(r#"localSheetId="{}""#, id - shift))
                        .into_owned()
                }
                None => {
                    let inner = element
                        .split_once('>')
                        .map_or("", |(_, rest)| rest.trim_end_matches("</definedName>"));
                    let text = unescape(inner).map_or(inner.to_string(), |text| text.into_owned());
                    if removed
                        .iter()
                        .any(|gone| crate::ooxml::references_sheet(&text, "", gone))
                    {
                        names_error = Some(attr(&NAME_ATTR, &element).unwrap_or_default());
                    }
                    element
                }
            }
        })
        .into_owned();
    if let Some(name) = names_error {
        return Err(format!("Sheet is used by the workbook name {name}"));
    }
    workbook_xml = ACTIVE_TAB
        .replace_all(&workbook_xml, |found: &regex::Captures<'_>| {
            format!(r#"{}="0""#, &found[1])
        })
        .into_owned();
    entries.insert(WORKBOOK.to_string(), workbook_xml.into_bytes());

    // workbook.xml.rels: drop the sheets' (and the calc chain's) relationships.
    let rels_xml = text_of(&entries, WORKBOOK_RELS)?;
    let mut deleted = BTreeSet::<String>::new();
    let mut kept_rels = rels_xml.clone();
    for tag in RELATIONSHIP.find_iter(&rels_xml) {
        let tag = tag.as_str();
        let id = attr(&ID_ATTR, tag).unwrap_or_default();
        let is_chain = attr(&TYPE_ATTR, tag).is_some_and(|kind| kind.ends_with("/calcChain"));
        if removed_ids.contains(&id) || is_chain {
            if let Some(target) = attr(&TARGET_ATTR, tag) {
                deleted.insert(resolve(folder_of(WORKBOOK), &target));
            }
            kept_rels = kept_rels.replacen(tag, "", 1);
        }
    }
    if entries.contains_key(CALC_CHAIN) {
        deleted.insert(CALC_CHAIN.to_string());
    }
    entries.insert(WORKBOOK_RELS.to_string(), kept_rels.into_bytes());

    // Parts only the removed sheets used go with them.
    loop {
        let referenced = entries
            .iter()
            .filter(|(name, _)| name.ends_with(".rels"))
            .filter_map(|(name, data)| {
                let owner = owner_of(name).unwrap_or_default();
                (!deleted.contains(&owner))
                    .then(|| targets_of(&String::from_utf8_lossy(data), &owner))
            })
            .flatten()
            .collect::<BTreeSet<_>>();
        let mut grown = false;
        for part in deleted.clone() {
            let Some(data) = entries.get(&rels_of(&part)) else {
                continue;
            };
            for target in targets_of(&String::from_utf8_lossy(data), &part) {
                if !referenced.contains(&target) && entries.contains_key(&target) {
                    grown |= deleted.insert(target);
                }
            }
        }
        if !grown {
            break;
        }
    }
    let gone_entries = deleted
        .iter()
        .flat_map(|part| [part.clone(), rels_of(part)])
        .collect::<BTreeSet<_>>();

    let content_types = text_of(&entries, CONTENT_TYPES)?;
    let mut kept_types = content_types.clone();
    for tag in OVERRIDE.find_iter(&content_types) {
        let part = attr(&PART_NAME, tag.as_str()).unwrap_or_default();
        if deleted.contains(part.trim_start_matches('/')) {
            kept_types = kept_types.replacen(tag.as_str(), "", 1);
        }
    }
    entries.insert(CONTENT_TYPES.to_string(), kept_types.into_bytes());

    // Rebuild in the original order.
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, _) in &package_parts.entries {
        if gone_entries.contains(name) {
            continue;
        }
        if name.ends_with('/') {
            writer
                .add_directory(name.as_str(), options)
                .map_err(|_| "Could not build the workbook".to_string())?;
            continue;
        }
        let data = entries.get(name).ok_or("Could not build the workbook")?;
        writer
            .start_file(name.as_str(), options)
            .and_then(|()| writer.write_all(data).map_err(Into::into))
            .map_err(|_| "Could not build the workbook".to_string())?;
    }
    writer
        .set_raw_comment(package_parts.comment.into())
        .map_err(|_| "Could not build the workbook".to_string())?;
    let output = writer
        .finish()
        .map_err(|_| "Could not build the workbook".to_string())?
        .into_inner();

    // The result must still open with exactly the sheets that should stay.
    let after = crate::ooxml::ordered_workbook_discovery(&mut Cursor::new(output.as_slice()))
        .map_err(invalid)?;
    if after.worksheets.len() + removed.len() != order.len() {
        return Err("The workbook did not keep the right sheets".to_string());
    }
    Ok(output)
}

#[cfg(test)]
pub(crate) mod tests;
