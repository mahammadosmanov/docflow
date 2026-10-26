//! Structural validation of a worksheet part that DocFlow rewrote. A package that fails
//! these checks is never published, because Excel would repair or drop the sheet.

use super::*;

fn column_number(letters: &str) -> Option<u32> {
    letters
        .bytes()
        .try_fold(0_u32, |acc, byte| {
            byte.is_ascii_uppercase()
                .then(|| acc * 26 + u32::from(byte - b'A' + 1))
        })
        .filter(|column| (1..=16_384).contains(column))
}

fn invalid(message: &str) -> OoxmlError {
    OoxmlError::invalid(format!("worksheet structure check failed: {message}"))
}

fn has_duplicate_attributes(element: &XmlElement) -> bool {
    let mut seen = BTreeSet::new();
    element
        .start
        .attributes()
        .with_checks(false)
        .flatten()
        .any(|attribute| !seen.insert(attribute.key.as_ref().to_vec()))
}

/// Cell references in `ref`/`sqref` attributes and `sqref` text must stay inside the
/// worksheet grid, or Excel repairs the sheet.
fn reference_outside_grid(element: &XmlElement) -> bool {
    let outside = |text: &str| {
        text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '$'))
            .filter(|token| !token.is_empty())
            .any(|token| {
                let token = token.replace('$', "");
                let split = token.find(|c: char| c.is_ascii_digit());
                let Some(split) = split else { return false };
                let (letters, digits) = token.split_at(split);
                (column_number(letters).is_none()
                    && letters.bytes().all(|b| b.is_ascii_uppercase()))
                    || digits.parse::<u32>().is_ok_and(|row| row > 1_048_576)
            })
    };
    ["ref", "sqref", "topLeftCell", "activeCell"]
        .iter()
        .filter_map(|name| attribute(element, name.as_bytes()))
        .any(|value| outside(&value))
        || (has_local_name(element, b"sqref") && outside(&element_text(element)))
}

fn has_illegal_character(text: &str) -> bool {
    text.chars().any(|c| {
        (c < ' ' && !matches!(c, '\t' | '\n' | '\r')) || matches!(c, '\u{FFFE}' | '\u{FFFF}')
    })
}

/// Checks the rewritten worksheet XML: parseable, ascending unique rows and cells whose
/// references match their position, no duplicated attributes, no empty formula element,
/// and no characters XML forbids.
pub(super) fn validate_worksheet_structure(raw: &[u8]) -> Result<(), OoxmlError> {
    if has_illegal_character(&String::from_utf8_lossy(raw)) {
        return Err(invalid("illegal character"));
    }
    let document = parse_document(raw)?;
    let mut previous_row = 0_u32;
    let mut problem = None::<&'static str>;
    for element in descendants(&document.nodes) {
        if has_duplicate_attributes(element) {
            problem = Some("duplicate attribute");
        }
        if reference_outside_grid(element) {
            problem = Some("reference outside the worksheet grid");
        }
        if has_local_name(element, b"f") && element_text(element).trim().is_empty() {
            let shared_child = attribute(element, b"t").as_deref() == Some("shared")
                && attribute(element, b"si").is_some()
                && attribute(element, b"ref").is_none();
            if !shared_child {
                problem = Some("empty formula");
            }
        }
        if !has_local_name(element, b"row") {
            continue;
        }
        let Some(row) = attribute(element, b"r").and_then(|value| value.parse::<u32>().ok()) else {
            problem = Some("row number");
            continue;
        };
        if row <= previous_row {
            problem = Some("row order");
        }
        previous_row = row;
        let mut previous_column = 0_u32;
        for cell in child_elements(element, b"c") {
            let reference = attribute(cell, b"r").unwrap_or_default();
            let split = reference.find(|c: char| c.is_ascii_digit()).unwrap_or(0);
            let (letters, digits) = reference.split_at(split);
            let (Some(column), Ok(cell_row)) = (column_number(letters), digits.parse::<u32>())
            else {
                problem = Some("cell reference");
                continue;
            };
            if cell_row != row {
                problem = Some("cell row");
            }
            if column <= previous_column {
                problem = Some("cell order");
            }
            previous_column = column;
            if cell.empty && !cell.children.is_empty() {
                problem = Some("cell content");
            }
        }
    }
    problem.map_or(Ok(()), |message| Err(invalid(message)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(rows: &str) -> Vec<u8> {
        format!(
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#
        )
        .into_bytes()
    }

    #[test]
    fn accepts_a_well_formed_sheet_with_shared_formula_children() {
        let raw = sheet(
            r#"<row r="1"><c r="A1"><f t="shared" ref="A1:A2" si="0">B1</f></c></row><row r="2"><c r="A2"><f t="shared" si="0"/></c><c r="C2"/></row>"#,
        );
        validate_worksheet_structure(&raw).unwrap();
    }

    #[test]
    fn rejects_each_structural_fault() {
        for (rows, why) in [
            (
                r#"<row r="2"><c r="A2"/></row><row r="1"><c r="A1"/></row>"#,
                "row order",
            ),
            (r#"<row r="1"><c r="B1"/><c r="A1"/></row>"#, "cell order"),
            (
                r#"<row r="1"><c r="A1"/><c r="A1"/></row>"#,
                "duplicate cell",
            ),
            (r#"<row r="1"><c r="A2"/></row>"#, "cell row"),
            (r#"<row r="1"><c r="A1"><f/></c></row>"#, "empty formula"),
            (
                r#"<row r="1"><c r="A1" s="1" s="2"/></row>"#,
                "duplicate attribute",
            ),
        ] {
            assert!(
                validate_worksheet_structure(&sheet(rows)).is_err(),
                "{why} must be rejected"
            );
        }
    }

    #[test]
    fn rejects_references_past_the_last_row() {
        let raw = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/><conditionalFormatting sqref="P1:P1048577"/></worksheet>"#;
        assert!(validate_worksheet_structure(raw.as_bytes()).is_err());
        let ok = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/><conditionalFormatting sqref="P1:P1048576 A5"/></worksheet>"#;
        validate_worksheet_structure(ok.as_bytes()).unwrap();
    }

    #[test]
    fn rejects_control_characters() {
        let raw =
            sheet("<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>a\u{1}b</t></is></c></row>");
        assert!(validate_worksheet_structure(&raw).is_err());
    }
}
