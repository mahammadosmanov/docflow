use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{Read, Seek},
    sync::LazyLock,
};

use regex::Regex;

use crate::ooxml::{WorksheetData, read_workbook_from_reader};

static CIV_SERIAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)CIV[^0-9]{0,4}([0-9]{1,4})").expect("static CIV RFI regex"));
static CON_CIV_010_FORM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^A-Z0-9])CON[\s_-]*CIV[\s_-]*010(?:$|[^0-9])")
        .expect("static concrete form regex")
});
#[derive(Clone, Debug, Default)]
pub(crate) struct CivilRecord {
    pub(crate) tag: String,
    pub(crate) area: String,
    pub(crate) forms: String,
}

pub(crate) fn requested_rfis(raw: &str) -> Result<Vec<String>, String> {
    let mut values = Vec::new();
    for item in raw.split([',', ';', '\n', '\r', '\t']) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let digits = item
            .strip_prefix("CIV-")
            .or_else(|| item.strip_prefix("CIV"))
            .unwrap_or(item);
        if digits.bytes().all(|byte| byte.is_ascii_digit()) && (1..=4).contains(&digits.len()) {
            values.push(format!("AZE-SUN-INT-QMS-RFI-CIV-{:0>4}", digits));
            continue;
        }
        let matched = crate::rfi_code::find(item);
        if matched.ok() && matched.discipline == "CIV" {
            values.push(matched.code);
        } else {
            return Err(format!("Invalid CIV RFI number: {item}"));
        }
    }
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
    if values.is_empty() {
        return Err("Enter at least one RFI number for a document type".to_string());
    }
    Ok(values)
}

pub(crate) fn read_civil_records<R: Read + Seek>(
    reader: &mut R,
    requested: &[String],
) -> Result<HashMap<String, CivilRecord>, String> {
    let workbook = read_workbook_from_reader(reader)
        .map_err(|error| format!("Cannot read RFI Log workbook: {error}"))?;
    let sheet = workbook
        .sheets
        .iter()
        .find(|(name, _)| normalize(name) == "civ")
        .map(|(_, sheet)| sheet)
        .ok_or_else(|| "RFI Log does not contain a Civil (CIV) sheet".to_string())?;
    let (header_row, columns) = find_header(
        sheet,
        &["rfino"],
        &["area"],
        &[
            ("tag", &["tagno", "tag"]),
            ("area", &["area"]),
            ("location", &["inspectionlocation", "location"]),
            ("forms", &["forms", "form"]),
        ],
    )?;
    let rfi_column = *columns.get("rfino").expect("required header mapped");
    let mut records = HashMap::new();
    for row in (header_row + 1)..=sheet.max_row {
        if sheet
            .cell(row, rfi_column)
            .is_some_and(|cell| cell.formula.is_some())
        {
            return Err(format!("RFI Log identifier at row {row} is a formula"));
        }
        let raw_rfi = cell_value(sheet, row, rfi_column)?;
        if raw_rfi.trim().is_empty() {
            continue;
        }
        let Some(rfi) = canonical_log_rfi(&raw_rfi) else {
            continue;
        };
        if !requested.contains(&rfi) {
            continue;
        }
        let tag = value_for(sheet, row, &columns, "tag")?;
        let mut area = value_for(sheet, row, &columns, "area")?;
        if area.is_empty() {
            area = value_for(sheet, row, &columns, "location")?;
        }
        if records
            .insert(
                rfi.clone(),
                CivilRecord {
                    tag,
                    forms: value_for(sheet, row, &columns, "forms")?,
                    area,
                },
            )
            .is_some()
        {
            return Err(format!("RFI Log has more than one Civil row for {rfi}"));
        }
    }
    for rfi in requested {
        if !records.contains_key(rfi) {
            return Err(format!("RFI Log Civil sheet has no exact row for {rfi}"));
        }
    }
    Ok(records)
}

pub(crate) fn has_con_civ_010_form(forms: &str) -> bool {
    CON_CIV_010_FORM.is_match(forms)
}

pub(crate) fn read_follow_up_records<R: Read + Seek>(
    reader: &mut R,
    requested: &[String],
) -> Result<HashMap<String, Vec<BTreeMap<String, String>>>, String> {
    let workbook = read_workbook_from_reader(reader).map_err(|error| {
        format!("Cannot read Concrete Specimen Test Follow-Up workbook: {error}")
    })?;
    let sheet = workbook
        .sheets
        .iter()
        .find(|(name, _)| normalize(name).contains("followup"))
        .map(|(_, sheet)| sheet)
        .ok_or_else(|| "Follow Up workbook does not contain a Follow Up sheet".to_string())?;
    let (header_row, columns) = find_header(
        sheet,
        &["rfino"],
        &["concreteclass"],
        &[
            ("rfi", &["rfino", "rfi"]),
            ("drawing", &["drawingno", "drawingnumber"]),
            ("pour_date", &["pouringdate", "pourdate"]),
            ("concrete_class", &["concreteclass", "class"]),
            ("location", &["unitlocation", "location"]),
            ("structure", &["structure", "structue"]),
            ("supplier", &["supplier"]),
            ("batch_ticket", &["batchticketno", "batchticketnumber"]),
            ("truck", &["truckno", "trucknumber"]),
            ("batch_time", &["timeofloading", "loadingtime", "batchtime"]),
            ("start_time", &["starttime", "pouringstarttime"]),
            ("end_time", &["completiontime", "endtime", "pouringendtime"]),
            ("quantity", &["quantity", "quantitym", "quantitypoured"]),
            ("total_quantity", &["totalquantity", "totalquantitypoured"]),
            (
                "plant_slump",
                &[
                    "slumpingplantmm",
                    "slumpinplantmm",
                    "slumpplant",
                    "slumptestplant",
                    "inplantslump",
                ],
            ),
            (
                "in_situ_slump",
                &[
                    "slumpinsitumm",
                    "slumpinginsitumm",
                    "insituslump",
                    "slumpinsitu",
                    "slumptestinsitu",
                ],
            ),
            (
                "plant_temp",
                &[
                    "tempratureinplantc",
                    "temperatureinplantc",
                    "planttemperature",
                    "planttemp",
                    "inplanttemperature",
                ],
            ),
            (
                "in_situ_temp",
                &[
                    "tempratureinsituc",
                    "temperatureinsituc",
                    "insitutemperature",
                    "insitutemp",
                ],
            ),
            (
                "samples",
                &[
                    "specimenno",
                    "specimennumber",
                    "concretesamplesno",
                    "samplesno",
                    "samplenumbers",
                ],
            ),
            ("water_cement_ratio", &["watercementratio", "wcratio"]),
        ],
    )?;
    let rfi_column = *columns.get("rfi").expect("Follow Up RFI header mapped");
    let mut records: HashMap<String, Vec<BTreeMap<String, String>>> = HashMap::new();
    let mut preceding_rfis: Vec<String> = Vec::new();
    for row in (header_row + 1)..=sheet.max_row {
        if sheet
            .cell(row, rfi_column)
            .is_some_and(|cell| cell.formula.is_some())
        {
            return Err(format!("Follow Up identifier at row {row} is a formula"));
        }
        let raw_rfis = value_for(sheet, row, &columns, "rfi")?;
        let fields = columns
            .keys()
            .map(|name| Ok((name.clone(), value_for(sheet, row, &columns, name)?)))
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        if raw_rfis.trim().is_empty() {
            let has_follow_up_data = fields
                .iter()
                .any(|(name, value)| name != "rfi" && !value.trim().is_empty());
            if has_follow_up_data {
                for rfi in &preceding_rfis {
                    records.entry(rfi.clone()).or_default().push(fields.clone());
                }
            }
            continue;
        }

        preceding_rfis = requested
            .iter()
            .filter(|rfi| rfi_matches(&raw_rfis, rfi))
            .cloned()
            .collect();
        for rfi in &preceding_rfis {
            records.entry(rfi.clone()).or_default().push(fields.clone());
        }
    }
    Ok(records)
}

fn find_header(
    sheet: &WorksheetData,
    required: &[&str],
    one_of: &[&str],
    mapped: &[(&str, &[&str])],
) -> Result<(u32, HashMap<String, u32>), String> {
    for row in 1..=sheet.max_row.min(32) {
        let mut headers = HashMap::new();
        for column in 1..=sheet.max_column {
            let name = normalize(sheet.value(row, column));
            if !name.is_empty() {
                headers.insert(name, column);
            }
        }
        if required.iter().all(|key| headers.contains_key(*key))
            && one_of.iter().any(|key| headers.contains_key(*key))
        {
            let mut columns = HashMap::new();
            for (target, candidates) in mapped {
                if let Some(column) = candidates
                    .iter()
                    .find_map(|candidate| headers.get(*candidate))
                {
                    columns.insert((*target).to_string(), *column);
                }
            }
            for key in required {
                if let Some(column) = headers.get(*key) {
                    columns.insert((*key).to_string(), *column);
                }
            }
            return Ok((row, columns));
        }
    }
    Err("Could not identify an unambiguous workbook header row".to_string())
}

fn value_for(
    sheet: &WorksheetData,
    row: u32,
    columns: &HashMap<String, u32>,
    name: &str,
) -> Result<String, String> {
    let Some(column) = columns.get(name) else {
        return Ok(String::new());
    };
    cell_value(sheet, row, *column)
}

fn cell_value(sheet: &WorksheetData, row: u32, column: u32) -> Result<String, String> {
    let Some(cell) = sheet.cell(row, column) else {
        return Ok(String::new());
    };
    Ok(cell.value.trim().to_string())
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn canonical_log_rfi(value: &str) -> Option<String> {
    crate::rfi_code::exact_codes(value)
        .into_iter()
        .find(|code| code.contains("-CIV-"))
        .or_else(|| {
            let captures = CIV_SERIAL.captures(value)?;
            let matched = captures.get(0)?;
            if value
                .as_bytes()
                .get(matched.end())
                .is_some_and(u8::is_ascii_digit)
            {
                return None;
            }
            let serial = captures.get(1)?.as_str();
            Some(format!("AZE-SUN-INT-QMS-RFI-CIV-{:0>4}", serial))
        })
}

fn rfi_matches(value: &str, expected: &str) -> bool {
    if crate::rfi_code::exact_codes(value).contains(expected)
        || canonical_log_rfi(value).as_deref() == Some(expected)
    {
        return true;
    }
    let Some(position) = value.to_ascii_uppercase().find("CIV") else {
        return false;
    };
    let expected_serial = expected.rsplit('-').next().unwrap_or_default();
    value[position + 3..]
        .split(|character: char| !character.is_ascii_digit())
        .any(|digits| digits == expected_serial)
}
#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::{has_con_civ_010_form, read_follow_up_records};

    #[test]
    fn concrete_specimen_follow_up_is_only_expected_for_exact_form_code() {
        assert!(has_con_civ_010_form("CON-CIV-010"));
        assert!(has_con_civ_010_form("CON-CIV-013 / con civ 010"));
        assert!(!has_con_civ_010_form("CON-CIV-011"));
        assert!(!has_con_civ_010_form("CON-CIV-0105"));
        assert!(!has_con_civ_010_form("CON-CIV-1010"));
    }

    fn follow_up_workbook(rows: &[(&str, &str, &str)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let mut worksheet = String::new();
        for (index, (rfi, class, ticket)) in rows.iter().enumerate() {
            let row = index + 1;
            worksheet.push_str(&format!(
                r#"<row r="{row}"><c r="A{row}" t="inlineStr"><is><t>{rfi}</t></is></c><c r="B{row}" t="inlineStr"><is><t>{class}</t></is></c><c r="C{row}" t="inlineStr"><is><t>{ticket}</t></is></c></row>"#
            ));
        }
        let sheet = format!(
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{worksheet}</sheetData></worksheet>"#
        );
        let workbook = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Follow Up" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
        let relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
        let package_relationships = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
        for (name, contents) in [
            ("[Content_Types].xml", "content-types"),
            ("_rels/.rels", package_relationships),
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", relationships),
            ("xl/worksheets/sheet1.xml", sheet.as_str()),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(contents.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn read(
        rows: &[(&str, &str, &str)],
        requested: &[&str],
    ) -> HashMap<String, Vec<BTreeMap<String, String>>> {
        let mut workbook = Cursor::new(follow_up_workbook(rows));
        read_follow_up_records(
            &mut workbook,
            &requested
                .iter()
                .map(|rfi| (*rfi).to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn retains_multiple_explicit_batches_for_each_rfi() {
        let records = read(
            &[
                ("RFI No", "Concrete Class", "Batch Ticket No"),
                ("CIV-0123", "C40", "A-1"),
                ("CIV-0123", "C45", "A-2"),
            ],
            &["AZE-SUN-INT-QMS-RFI-CIV-0123"],
        );
        let batches = &records["AZE-SUN-INT-QMS-RFI-CIV-0123"];
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0]["batch_ticket"], "A-1");
        assert_eq!(batches[1]["batch_ticket"], "A-2");
    }

    #[test]
    fn ignores_blank_spacers_and_attaches_continuation_rows() {
        let records = read(
            &[
                ("RFI No", "Concrete Class", "Batch Ticket No"),
                ("CIV-0123", "C40", "A-1"),
                ("", "", ""),
                ("", "C40", "A-2"),
            ],
            &["AZE-SUN-INT-QMS-RFI-CIV-0123"],
        );
        let batches = &records["AZE-SUN-INT-QMS-RFI-CIV-0123"];
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[1]["batch_ticket"], "A-2");
    }

    #[test]
    fn composite_rfi_rows_and_continuations_are_kept_for_each_requested_rfi() {
        let records = read(
            &[
                ("RFI No", "Concrete Class", "Batch Ticket No"),
                ("CIV-0123, CIV-0124", "C40", "A-1"),
                ("", "C40", "A-2"),
            ],
            &[
                "AZE-SUN-INT-QMS-RFI-CIV-0123",
                "AZE-SUN-INT-QMS-RFI-CIV-0124",
            ],
        );
        for rfi in [
            "AZE-SUN-INT-QMS-RFI-CIV-0123",
            "AZE-SUN-INT-QMS-RFI-CIV-0124",
        ] {
            assert_eq!(records[rfi].len(), 2);
            assert_eq!(records[rfi][1]["batch_ticket"], "A-2");
        }
    }

    use std::collections::{BTreeMap, HashMap};
}
