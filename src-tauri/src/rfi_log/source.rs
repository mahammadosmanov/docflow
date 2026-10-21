use super::*;

pub(super) fn read_source(
    workbook: &WorkbookData,
    discipline: &str,
) -> Result<Source, BackendError> {
    let layout =
        layout(discipline).ok_or_else(|| BackendError::validation("gecersiz RFI sheet'i"))?;
    let sheet = workbook.sheet(layout.sheet).ok_or_else(|| {
        BackendError::validation(format!(
            "RFI Log icin {}/Sheet1 sayfalari bulunamadi",
            layout.sheet
        ))
    })?;
    let lookup = workbook.sheet("Sheet1").ok_or_else(|| {
        BackendError::validation(format!(
            "RFI Log icin {}/Sheet1 sayfalari bulunamadi",
            layout.sheet
        ))
    })?;
    let lookup_columns = lookup_columns(lookup)?;
    let mut presets = BTreeMap::new();
    for row in 1..=lookup.max_row {
        let item = clean_text(lookup.value(row, lookup_columns["item"]));
        let Some((item_discipline, _)) = item_parts(&item) else {
            continue;
        };
        if item_discipline != discipline {
            continue;
        }
        let azfen_item = clean_text(lookup.value(row, lookup_columns["azfenItem"]));
        let intec_item = clean_text(lookup.value(row, lookup_columns["intecItem"]));
        if azfen_item != item || intec_item != item {
            return Err(BackendError::validation(format!(
                "{discipline} Sheet1 Item eslemesi tutarsiz: {item}"
            )));
        }
        let preset = Preset {
            description: clean_text(lookup.value(row, lookup_columns["description"])),
            forms: clean_text(lookup.value(row, lookup_columns["forms"])),
            azfen_type: clean_text(lookup.value(row, lookup_columns["azfenType"])),
            intec_type: clean_text(lookup.value(row, lookup_columns["intecType"])),
        };
        let missing = [
            ("description", &preset.description),
            ("forms", &preset.forms),
            ("azfenType", &preset.azfen_type),
            ("intecType", &preset.intec_type),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.is_empty().then_some(name))
        .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(BackendError::validation(format!(
                "{discipline} Sheet1 esleme verisi eksik: {item} / {}",
                missing.join(", ")
            )));
        }
        if presets
            .get(&item)
            .is_some_and(|existing| existing != &preset)
        {
            return Err(BackendError::validation(format!(
                "{discipline} Sheet1 cakisan Item eslemesi: {item}"
            )));
        }
        presets.insert(item, preset);
    }
    if presets.is_empty() {
        return Err(BackendError::validation(format!(
            "{discipline} icin Sheet1 esleme verisi bulunamadi"
        )));
    }
    let mut max_number = 0_u32;
    let mut last_row = 9_u32;
    let mut latest_item = String::new();
    let mut tags = Vec::new();
    let mut areas = Vec::new();
    let mut locations = Vec::new();
    let mut subcontractors = Vec::new();
    let mut occupied = BTreeMap::<NaiveDate, BTreeMap<String, NaiveTime>>::new();
    for row in 10..=sheet.max_row {
        let rfi = sheet.value(row, 1).trim();
        let Some((rfi_discipline, number)) = rfi_parts(rfi) else {
            continue;
        };
        if rfi_discipline != discipline {
            continue;
        }
        max_number = max_number.max(number);
        last_row = row;
        let item = clean_text(sheet.value(row, 3));
        if !item.is_empty() {
            latest_item = item;
        }
        tags.push(clean_text(sheet.value(row, 6)));
        areas.push(clean_text(sheet.value(row, 7)));
        locations.push(clean_text(sheet.value(row, layout.location_col)));
        subcontractors.push(clean_text(sheet.value(row, layout.subcontractor_col)));
        if let (Some(day), Some(time)) = (
            cell_date(sheet.value(row, layout.inspection_col)),
            cell_time(sheet.value(row, layout.time_col)),
        ) {
            occupied
                .entry(day)
                .or_default()
                .insert(rfi.to_string(), time);
        }
    }
    let mut workbook_itps = BTreeMap::new();
    for column in 1..=lookup.max_column {
        let label = clean_text(lookup.value(1, column));
        if label.to_ascii_uppercase().contains("ITP")
            && label.to_ascii_uppercase().contains(discipline)
            && let Some(number) = trailing_four_digits(&label)
        {
            workbook_itps.insert(number, label);
        }
    }
    let mut itp_to_items = BTreeMap::<String, Vec<String>>::new();
    for item in presets.keys() {
        let (_, number) = item_parts(item).expect("validated item");
        let itp = workbook_itps
            .get(&number)
            .cloned()
            .unwrap_or_else(|| canonical_itp(discipline, number));
        itp_to_items.entry(itp).or_default().push(item.clone());
    }
    for items in itp_to_items.values_mut() {
        items.sort_by_key(|item| natural_key(item));
    }
    let default_item = if presets.contains_key(&latest_item) {
        latest_item
    } else {
        presets.keys().next().cloned().expect("nonempty presets")
    };
    let (_, default_number) = item_parts(&default_item).expect("validated default item");
    let default_itp = workbook_itps
        .get(&default_number)
        .cloned()
        .unwrap_or_else(|| canonical_itp(discipline, default_number));
    Ok(Source {
        data: SourceData {
            presets,
            itp_to_items,
            lookup_columns,
            tags: recent_unique(tags),
            areas: recent_unique(areas),
            locations: recent_unique(locations),
            subcontractors: recent_unique(subcontractors),
            max_row: sheet.max_row,
        },
        default_item,
        default_itp,
        max_number,
        last_row,
        occupied,
    })
}

pub(super) fn source_from_data(batch: &Batch) -> Source {
    Source {
        data: batch.source_data.clone(),
        default_item: batch
            .records
            .first()
            .map(|record| record.item_no.clone())
            .unwrap_or_default(),
        default_itp: batch
            .records
            .first()
            .map(|record| record.itp_no.clone())
            .unwrap_or_default(),
        max_number: 0,
        last_row: batch.start_row.saturating_sub(1),
        occupied: BTreeMap::new(),
    }
}

pub(super) fn lookup_columns(sheet: &WorksheetData) -> Result<BTreeMap<String, u32>, BackendError> {
    for start in 1..=sheet.max_column.saturating_sub(6) {
        if !matches!(
            header_key(sheet.value(1, start)).as_str(),
            "item" | "itemno" | "itemnumber"
        ) {
            continue;
        }
        if !matches!(
            header_key(sheet.value(1, start + 3)).as_str(),
            "item" | "itemno" | "itemnumber"
        ) || !matches!(
            header_key(sheet.value(1, start + 5)).as_str(),
            "item" | "itemno" | "itemnumber"
        ) {
            continue;
        }
        return Ok(BTreeMap::from([
            ("item".to_string(), start),
            ("description".to_string(), start + 1),
            ("forms".to_string(), start + 2),
            ("azfenItem".to_string(), start + 3),
            ("azfenType".to_string(), start + 4),
            ("intecItem".to_string(), start + 5),
            ("intecType".to_string(), start + 6),
        ]));
    }
    Err(BackendError::validation(
        "Sheet1 Item/description/forms/type esleme kolonlari bulunamadi",
    ))
}

pub(super) fn allocate(
    occupied: &BTreeMap<NaiveDate, BTreeMap<String, NaiveTime>>,
    start_day: NaiveDate,
    count: usize,
    wrap: bool,
) -> Result<(Vec<(NaiveDate, NaiveTime)>, usize), BackendError> {
    let mut working = occupied.clone();
    let mut slots = Vec::new();
    let mut overflowed = 0;
    let mut day = start_day;
    for index in 0..count {
        let mut minutes = next_slot_minutes(&working, day);
        if minutes > DAY_END_MINUTES {
            overflowed += 1;
            if wrap {
                while minutes > DAY_END_MINUTES {
                    day = day.checked_add_days(Days::new(1)).ok_or_else(|| {
                        BackendError::validation("RFI schedule date is out of range")
                    })?;
                    minutes = next_slot_minutes(&working, day);
                }
            }
        }
        let time = NaiveTime::from_num_seconds_from_midnight_opt((minutes % (24 * 60)) * 60, 0)
            .ok_or_else(|| BackendError::validation("RFI schedule time is invalid"))?;
        working
            .entry(day)
            .or_default()
            .insert(format!("__planned_{index}"), time);
        slots.push((day, time));
    }
    Ok((slots, overflowed))
}

pub(super) fn next_slot_minutes(
    occupied: &BTreeMap<NaiveDate, BTreeMap<String, NaiveTime>>,
    day: NaiveDate,
) -> u32 {
    occupied
        .get(&day)
        .and_then(|values| values.values().max())
        .map(|time| time.hour() * 60 + time.minute() + SLOT_MINUTES)
        .unwrap_or(DAY_START_MINUTES)
}

pub(super) fn record(
    index: usize,
    discipline: &str,
    rfi: &str,
    source: &Source,
    issue_day: NaiveDate,
    slot: (NaiveDate, NaiveTime),
) -> Record {
    let preset = &source.data.presets[&source.default_item];
    Record {
        row_id: format!("{discipline}-g{}-r1", index + 1),
        sheet: discipline.to_string(),
        rfi_no: rfi.to_string(),
        itp_no: source.default_itp.clone(),
        item_no: source.default_item.clone(),
        description: preset.description.clone(),
        forms: preset.forms.clone(),
        tag_no: String::new(),
        area: String::new(),
        row_no: String::new(),
        date_of_issue: date_text(issue_day),
        inspection_date: date_text(slot.0),
        start_time: time_text(slot.1),
        azfen_type: preset.azfen_type.clone(),
        intec_type: preset.intec_type.clone(),
        location: String::new(),
        subcontractor: "AZFEN".to_string(),
    }
}
