use super::*;

pub(super) fn signature(path: &Path) -> Result<SourceSignature, BackendError> {
    let size = fs::metadata(path)
        .map_err(|_| BackendError::path_invalid("workbook metadata is unavailable"))?
        .len();
    let sha256 = file_sha256(path)
        .map_err(|_| BackendError::path_invalid("workbook hash is unavailable"))?;
    Ok(SourceSignature { size, sha256 })
}

pub(super) fn only_workbook_step(plan: &Plan) -> Result<&Step, BackendError> {
    if plan.op_id != "rfi.log_add"
        || plan.steps.len() != 1
        || plan.steps[0].kind != StepKind::WorkbookUpdate
    {
        return Err(BackendError::validation("gecersiz RFI Log plani"));
    }
    Ok(&plan.steps[0])
}

pub(super) fn decode_batches(step: &Step) -> Result<Vec<Batch>, BackendError> {
    serde_json::from_value(
        step.meta
            .get("batches")
            .cloned()
            .ok_or_else(|| BackendError::validation("gecersiz coklu-sheet RFI plani"))?,
    )
    .map_err(|_| BackendError::validation("gecersiz coklu-sheet RFI plani"))
}

pub(super) fn formula_or_value(value: &str, default: &str, formula: String) -> CellUpdate {
    if normalize_lines(value) == normalize_lines(default) {
        CellUpdate::Formula {
            formula,
            cached: Some(default.to_string()),
        }
    } else {
        CellUpdate::Text(value.to_string())
    }
}

pub(super) fn normalize_lines(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .nfc()
        .collect()
}

pub(super) fn resolve_itp(
    value: &str,
    item: &str,
    mapping: &BTreeMap<String, Vec<String>>,
) -> Option<String> {
    if mapping.contains_key(value) {
        return Some(value.to_string());
    }
    let signature = itp_signature(value);
    let matches = mapping
        .keys()
        .filter(|label| !signature.is_empty() && itp_signature(label) == signature)
        .cloned()
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        return matches.into_iter().next();
    }
    unique_itp_for_item(item, mapping)
}

pub(super) fn unique_itp_for_item(
    item: &str,
    mapping: &BTreeMap<String, Vec<String>>,
) -> Option<String> {
    let matches = mapping
        .iter()
        .filter(|(_, items)| items.iter().any(|candidate| candidate == item))
        .map(|(label, _)| label.clone())
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0].clone())
}

pub(super) fn itp_signature(value: &str) -> Vec<String> {
    token_pattern()
        .find_iter(&value.nfkc().collect::<String>().to_uppercase())
        .map(|token| token.as_str().to_string())
        .collect()
}

pub(super) fn infer_area(tag: &str) -> String {
    let upper = tag.to_ascii_uppercase();
    if let Some(site) = leading_site_pattern()
        .captures(&upper)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str())
    {
        return if site == "N" {
            "North Site"
        } else {
            "South Site"
        }
        .to_string();
    }
    let north = north_pattern().is_match(&upper);
    let south = south_pattern().is_match(&upper);
    match (north, south) {
        (true, false) => "North Site".to_string(),
        (false, true) => "South Site".to_string(),
        _ => String::new(),
    }
}

pub(super) fn clean_text(value: &str) -> String {
    let mut text = value.trim().nfc().collect::<String>();
    for _ in 0..2 {
        if mojibake_score(&text) == 0 {
            break;
        }
        let Some(bytes) = text.chars().map(cp1252_byte).collect::<Option<Vec<_>>>() else {
            break;
        };
        let Ok(repaired) = String::from_utf8(bytes) else {
            break;
        };
        if mojibake_score(&repaired) >= mojibake_score(&text) {
            break;
        }
        text = repaired.nfc().collect();
    }
    text
}

pub(super) fn cp1252_byte(character: char) -> Option<u8> {
    if (character as u32) <= 0x7f || (0xa0..=0xff).contains(&(character as u32)) {
        return Some(character as u8);
    }
    Some(match character {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8a,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        'Ž' => 0x8e,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9a,
        '›' => 0x9b,
        'œ' => 0x9c,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        _ => return None,
    })
}

pub(super) fn mojibake_score(value: &str) -> usize {
    ["Ã", "Â", "â", "ð", "Ë"]
        .iter()
        .map(|marker| value.matches(marker).count())
        .sum()
}

pub(super) fn header_key(value: &str) -> String {
    value
        .nfkd()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

pub(super) fn item_parts(value: &str) -> Option<(String, u32)> {
    let captures = item_pattern().captures(value)?;
    Some((
        captures.get(1)?.as_str().to_ascii_uppercase(),
        captures.get(2)?.as_str().parse().ok()?,
    ))
}

pub(super) fn rfi_parts(value: &str) -> Option<(String, u32)> {
    let captures = rfi_pattern().captures(value)?;
    Some((
        captures.get(1)?.as_str().to_ascii_uppercase(),
        normalize_candidate_number(captures.get(2)?.as_str())?
            .parse()
            .ok()?,
    ))
}

pub(super) fn trailing_four_digits(value: &str) -> Option<u32> {
    trailing_digits_pattern()
        .captures(value)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse().ok())
}

pub(super) fn canonical_itp(discipline: &str, number: u32) -> String {
    format!("AZE-SUN-INT-QMS-ITP-{discipline}-{number:04}")
}

pub(super) fn natural_key(value: &str) -> Vec<String> {
    natural_pattern()
        .find_iter(value)
        .map(|part| {
            part.as_str().parse::<u64>().map_or_else(
                |_| format!("1{}", part.as_str()),
                |number| format!("0{number:020}"),
            )
        })
        .collect()
}

pub(super) fn recent_unique(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .rev()
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .take(500)
        .collect()
}

pub(super) fn cell_date(value: &str) -> Option<NaiveDate> {
    if let Ok(serial) = value.parse::<f64>() {
        return NaiveDate::from_ymd_opt(1899, 12, 30)?
            .checked_add_days(Days::new(serial.floor().max(0.0) as u64));
    }
    parse_date(value).ok()
}

pub(super) fn cell_time(value: &str) -> Option<NaiveTime> {
    if let Ok(serial) = value.parse::<f64>() {
        let seconds = (serial * 24.0 * 60.0 * 60.0).round() as u32 % (24 * 60 * 60);
        return NaiveTime::from_num_seconds_from_midnight_opt(seconds, 0)
            .map(|time| time.with_second(0).unwrap_or(time));
    }
    parse_time(value).ok()
}

pub(super) fn parse_date(value: &str) -> Result<NaiveDate, BackendError> {
    for format in ["%d-%b-%y", "%Y-%m-%d", "%d.%m.%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(value.trim(), format) {
            return Ok(date);
        }
    }
    Err(BackendError::validation(format!("gecersiz tarih: {value}")))
}

pub(super) fn parse_time(value: &str) -> Result<NaiveTime, BackendError> {
    NaiveTime::parse_from_str(value.trim(), "%H:%M")
        .map_err(|_| BackendError::validation(format!("gecersiz saat: {value}")))
}

pub(super) fn date_text(value: NaiveDate) -> String {
    value.format("%d-%b-%y").to_string()
}

pub(super) fn time_text(value: NaiveTime) -> String {
    value.format("%H:%M").to_string()
}

pub(super) fn excel_date(value: NaiveDate) -> i64 {
    value
        .signed_duration_since(NaiveDate::from_ymd_opt(1899, 12, 30).expect("Excel epoch"))
        .num_days()
}

pub(super) fn excel_time(value: NaiveTime) -> String {
    let seconds = value.num_seconds_from_midnight() as f64;
    format!("{:.15}", seconds / (24.0 * 60.0 * 60.0))
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

pub(super) fn record_map(record: Record) -> BTreeMap<String, String> {
    serde_json::to_value(record)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| (key, value.as_str().unwrap_or_default().to_string()))
        .collect()
}

pub(super) fn copy_cell_text(sheet: &WorksheetData, row: u32, column: u32, field: &str) -> String {
    let raw = sheet.value(row, column);
    match field {
        "dateOfIssue" | "inspectionDate" => {
            cell_date(raw).map_or_else(|| raw.to_string(), date_text)
        }
        "startTime" => cell_time(raw).map_or_else(|| raw.to_string(), time_text),
        _ => raw.to_string(),
    }
}

pub(super) fn field_width(field: &str) -> f64 {
    match field {
        "rfiNo" => 22.0,
        "itpNo" => 24.0,
        "itemNo" | "rowNo" | "azfenType" | "intecType" => 10.0,
        "description" => 28.0,
        "forms" => 20.0,
        "tagNo" => 15.0,
        "area" | "dateOfIssue" | "inspectionDate" | "startTime" => 12.0,
        "location" => 18.0,
        "subcontractor" => 14.0,
        _ => 14.0,
    }
}

pub(super) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

pub(super) fn column_name(mut number: u32) -> String {
    let mut output = String::new();
    while number > 0 {
        let remainder = (number - 1) % 26;
        output.insert(0, char::from(b'A' + remainder as u8));
        number = (number - 1) / 26;
    }
    output
}

pub(super) fn value_string(values: &Map<String, Value>, key: &str) -> String {
    values
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn is_discipline(value: &str) -> bool {
    DISCIPLINES.contains(&value)
}

pub(super) fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

pub(super) fn deduplicate(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

pub(super) fn insert_json<T: Serialize>(
    object: &mut JsonObject,
    key: &str,
    value: &T,
) -> Result<(), BackendError> {
    object.insert(
        key.to_string(),
        serde_json::to_value(value)
            .map_err(|_| BackendError::protocol("RFI plan could not be encoded"))?,
    );
    Ok(())
}

pub(super) fn insert_value(object: &mut JsonObject, key: &str, value: impl Into<Value>) {
    object.insert(key.to_string(), value.into());
}

pub(super) fn checkpoint(cancelled: &impl Fn() -> bool) -> Result<(), BackendError> {
    if cancelled() {
        Err(BackendError::cancelled())
    } else {
        Ok(())
    }
}

pub(super) fn progress_event(
    progress: &mut impl FnMut(ProgressEvent),
    phase: &str,
    done: u64,
    total: u64,
) {
    progress(ProgressEvent {
        phase: phase.to_string(),
        done,
        total,
        subject: String::new(),
    });
}

pub(super) fn document_error(error: impl std::fmt::Display) -> BackendError {
    BackendError::validation(error.to_string())
}

pub(super) fn rfi_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r"(?i)-RFI-(CIV|MEC|ELE|PRC)-([0-9]{1,4})$")
            .expect("static RFI log regex is valid")
    })
}

pub(super) fn item_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r"(?i)^(CIV|MEC|ELE|PRC)-(\d+)\.").expect("static RFI log regex is valid")
    })
}

pub(super) fn trailing_digits_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"(\d{4})$").expect("static RFI log regex is valid"))
}

pub(super) fn token_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"[A-Z]+|\d+").expect("static RFI log regex is valid"))
}

pub(super) fn natural_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| Regex::new(r"\d+|\D+").expect("static RFI log regex is valid"))
}

pub(super) fn leading_site_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r"(?i)^\s*([NS])\d{1,3}(?:[^A-Z0-9]|$)").expect("static RFI log regex is valid")
    })
}

pub(super) fn north_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^A-Z0-9])N\d{1,3}(?:[^A-Z0-9]|$)")
            .expect("static RFI log regex is valid")
    })
}

pub(super) fn south_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^A-Z0-9])S\d{1,3}(?:[^A-Z0-9]|$)")
            .expect("static RFI log regex is valid")
    })
}
