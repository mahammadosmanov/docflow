use super::*;

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
    patch_workbook_sheets_guarded_inner(source, destination, updates_by_sheet, true, false)
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
    patch_workbook_sheets_guarded_inner(source, destination, updates_by_sheet, false, false)
}

/// Same as [`patch_workbook_sheets_guarded_text`] and additionally sets
/// `fullCalcOnLoad="1"` on the existing `calcPr` of `xl/workbook.xml`. Only that
/// one attribute may change in the workbook part. Cached formula values (for
/// example percentage indicators) are otherwise left stale because Excel does
/// not mark XML-patched cells dirty, so neither opening nor F9 refreshes them.
pub fn patch_workbook_sheets_guarded_text_recalc<R, W>(
    source: &mut R,
    destination: &mut W,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
) -> Result<(), OoxmlError>
where
    R: Read + Seek,
    W: Read + Write + Seek,
{
    patch_workbook_sheets_guarded_inner(source, destination, updates_by_sheet, false, true)
}

pub(super) fn patch_workbook_sheets_guarded_inner<R, W>(
    source: &mut R,
    destination: &mut W,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
    status_values_only: bool,
    recalc_on_load: bool,
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
    let fonts = load_font_table(&mut archive)?;
    let mut patched = BTreeMap::<String, Vec<u8>>::new();
    for (sheet_name, updates) in updates_by_sheet {
        let info = sheet_infos
            .get(sheet_name)
            .ok_or_else(|| OoxmlError::invalid("guarded worksheet is missing"))?;
        let raw = read_entry(&mut archive, &info.path)?;
        patched.insert(
            info.path.clone(),
            patch_sheet_xml_guarded(&raw, updates, status_values_only, &fonts)?,
        );
    }

    if recalc_on_load {
        let raw = read_entry(&mut archive, WORKBOOK_PART)?;
        patched.insert(WORKBOOK_PART.to_string(), with_full_calc_on_load(&raw)?);
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
    verify_guarded_workbook_delta_inner(source, destination, updates_by_sheet, true, false)
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
    verify_guarded_workbook_delta_inner(source, destination, updates_by_sheet, false, false)
}

pub fn verify_guarded_workbook_delta_text_recalc<R1, R2>(
    source: &mut R1,
    destination: &mut R2,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
) -> Result<(), OoxmlError>
where
    R1: Read + Seek,
    R2: Read + Seek,
{
    verify_guarded_workbook_delta_inner(source, destination, updates_by_sheet, false, true)
}

const WORKBOOK_PART: &str = "xl/workbook.xml";

/// Returns the workbook XML with `fullCalcOnLoad="1"` on its existing `calcPr`.
/// A workbook without `calcPr` is returned byte-identical.
pub(super) fn with_full_calc_on_load(raw: &[u8]) -> Result<Vec<u8>, OoxmlError> {
    let mut document = parse_document(raw)?;
    let Some(calc) = find_element_mut(&mut document.nodes, b"calcPr") else {
        return Ok(raw.to_vec());
    };
    set_attribute(calc, b"fullCalcOnLoad", "1");
    serialize_document(&document)
}

pub(super) fn verify_guarded_workbook_delta_inner<R1, R2>(
    source: &mut R1,
    destination: &mut R2,
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
    status_values_only: bool,
    recalc_on_load: bool,
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
    let fonts = load_font_table(&mut source_archive)?;
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
            let source_tokens = guarded_sheet_tokens(&source_bytes, updates, false, &fonts)?;
            let destination_tokens =
                guarded_sheet_tokens(&destination_bytes, updates, true, &fonts)?;
            if destination_tokens != source_tokens {
                return Err(OoxmlError::invalid(
                    "guarded worksheet delta verification failed",
                ));
            }
        } else if recalc_on_load && source_name == WORKBOOK_PART {
            if destination_bytes != with_full_calc_on_load(&source_bytes)? {
                return Err(OoxmlError::invalid(
                    "guarded workbook part changed beyond the recalculation flag",
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
            let replacement = match &update.replacement {
                CellUpdate::Text(replacement) => replacement.clone(),
                CellUpdate::Rich(runs) => rich_plain_text(runs),
                _ => return Err(OoxmlError::invalid("guarded replacement is invalid")),
            };
            if cell.value != replacement
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

pub(super) fn validate_guarded_update_set(
    updates_by_sheet: &BTreeMap<String, BTreeMap<(u32, u32), GuardedCellUpdate>>,
    status_values_only: bool,
) -> Result<(), OoxmlError> {
    if updates_by_sheet.is_empty()
        || updates_by_sheet
            .values()
            .any(std::collections::BTreeMap::is_empty)
    {
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
            if !matches!(
                &update.replacement,
                CellUpdate::Text(_) | CellUpdate::Rich(_)
            ) || matches!(&update.replacement, CellUpdate::Rich(runs) if runs.is_empty())
                || status_values_only && matches!(&update.replacement, CellUpdate::Rich(_))
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

pub(super) fn validate_guarded_source(
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

/// Fonts for rich-text runs; a workbook without `xl/styles.xml` yields no font facts.
pub(super) fn load_font_table<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<FontTable, OoxmlError> {
    if !has_entry(archive, "xl/styles.xml")? {
        return Ok(FontTable::default());
    }
    FontTable::parse(&read_entry(archive, "xl/styles.xml")?)
}
