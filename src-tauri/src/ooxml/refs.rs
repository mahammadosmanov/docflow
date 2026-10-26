//! A1 reference handling inside formulas and range lists, used when a worksheet
//! row is inserted: shifting references the way Excel does, and translating a
//! formula to another cell (needed to expand shared formulas and to clone rows).

use std::sync::LazyLock;

use regex::Regex;

/// Inserting `count` rows so the first new row is `at`. `anchor` is the row the new
/// rows are cloned from (`at - 1`), used for the "append at the end of a range" rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RowShift {
    pub sheet: String,
    pub at: u32,
    pub count: u32,
}

/// Excel's last worksheet row; a reference past it makes Excel repair the sheet.
const MAX_ROW: u32 = 1_048_576;

impl RowShift {
    fn shift_start(&self, row: u32) -> u32 {
        if row >= self.at {
            (row + self.count).min(MAX_ROW)
        } else {
            row
        }
    }

    /// A range end moves when it is at or below the insertion, and also when the range
    /// ends exactly on the row before the insertion and spans several rows: new rows
    /// appended under a data range join it, row-local ranges (`C5:P5`) do not.
    fn shift_end(&self, start: u32, end: u32) -> u32 {
        if end >= self.at || (end + 1 == self.at && start < end) {
            (end + self.count).min(MAX_ROW)
        } else {
            end
        }
    }
}

static REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        (?P<sheet>(?:'(?:[^']|'')+'|[A-Za-z_\u{80}-\u{FFFF}][A-Za-z0-9_.\u{80}-\u{FFFF}]*)!)?
        (?P<a>\$?[A-Za-z]{1,3}\$?[0-9]{1,7})
        (?::(?P<b>\$?[A-Za-z]{1,3}\$?[0-9]{1,7}))?",
    )
    .unwrap()
});

#[derive(Clone, Copy, Debug)]
struct Cell {
    col_abs: bool,
    col: u32,
    row_abs: bool,
    row: u32,
}

fn parse_cell(text: &str) -> Option<Cell> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let col_abs = bytes.get(index) == Some(&b'$');
    if col_abs {
        index += 1;
    }
    let col_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_alphabetic) {
        index += 1;
    }
    let letters = &text[col_start..index];
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let row_abs = bytes.get(index) == Some(&b'$');
    if row_abs {
        index += 1;
    }
    let row: u32 = text[index..].parse().ok()?;
    let col = letters.bytes().fold(0_u32, |acc, byte| {
        acc * 26 + u32::from(byte.to_ascii_uppercase() - b'A' + 1)
    });
    (1..=16_384).contains(&col).then_some(())?;
    (1..=1_048_576).contains(&row).then_some(())?;
    Some(Cell {
        col_abs,
        col,
        row_abs,
        row,
    })
}

fn column_letters(mut column: u32) -> String {
    let mut out = Vec::new();
    while column > 0 {
        column -= 1;
        out.push(b'A' + u8::try_from(column % 26).unwrap_or(0));
        column /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn format_cell(cell: Cell) -> String {
    format!(
        "{}{}{}{}",
        if cell.col_abs { "$" } else { "" },
        column_letters(cell.col),
        if cell.row_abs { "$" } else { "" },
        cell.row
    )
}

fn sheet_of(qualifier: Option<&str>, current: &str) -> String {
    match qualifier {
        None => current.to_string(),
        Some(text) => {
            let name = text.trim_end_matches('!');
            name.strip_prefix('\'')
                .and_then(|inner| inner.strip_suffix('\''))
                .map_or_else(|| name.to_string(), |inner| inner.replace("''", "'"))
        }
    }
}

fn boundary_ok(formula: &str, start: usize, end: usize) -> bool {
    let before = formula[..start].chars().next_back();
    let after = formula[end..].chars().next();
    let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '\'' | '$');
    !before.is_some_and(word)
        && !after.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '(' | '!'))
}

/// Visits every cell reference outside string literals. The callback receives the
/// referenced sheet and the optional start/end cells and returns the replacement
/// text for the range part (without the sheet qualifier) or `None` to keep it.
fn map_references(
    formula: &str,
    current_sheet: &str,
    mut replace: impl FnMut(&str, Cell, Option<Cell>) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(formula.len() + 8);
    let mut rest = formula;
    // Split on string literals so quoted text is never rewritten.
    let mut offset = 0;
    while !rest.is_empty() {
        let (code, after_code) = match rest.find('"') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };
        out.push_str(&map_code_segment(
            formula,
            offset,
            code,
            current_sheet,
            &mut replace,
        ));
        offset += code.len();
        if after_code.is_empty() {
            break;
        }
        // Copy the string literal, honoring "" escapes.
        let bytes = after_code.as_bytes();
        let mut index = 1;
        loop {
            match bytes.get(index) {
                None => break,
                Some(b'"') if bytes.get(index + 1) == Some(&b'"') => index += 2,
                Some(b'"') => {
                    index += 1;
                    break;
                }
                Some(_) => index += 1,
            }
        }
        out.push_str(&after_code[..index]);
        offset += index;
        rest = &after_code[index..];
    }
    out
}

fn map_code_segment(
    whole: &str,
    segment_start: usize,
    code: &str,
    current_sheet: &str,
    replace: &mut impl FnMut(&str, Cell, Option<Cell>) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(code.len());
    let mut last = 0;
    for captures in REFERENCE.captures_iter(code) {
        let whole_match = captures.get(0).expect("match exists");
        let start = segment_start + whole_match.start();
        let end = segment_start + whole_match.end();
        if !boundary_ok(whole, start, end) {
            continue;
        }
        let Some(first) = parse_cell(&captures["a"]) else {
            continue;
        };
        let second = match captures.name("b") {
            Some(text) => match parse_cell(text.as_str()) {
                Some(cell) => Some(cell),
                None => continue,
            },
            None => None,
        };
        let qualifier = captures.name("sheet").map(|m| m.as_str());
        let sheet = sheet_of(qualifier, current_sheet);
        if let Some(text) = replace(&sheet, first, second) {
            out.push_str(&code[last..whole_match.start()]);
            if let Some(qualifier) = qualifier {
                out.push_str(qualifier);
            }
            out.push_str(&text);
            last = whole_match.end();
        }
    }
    out.push_str(&code[last..]);
    out
}

/// Rewrites references in `formula` (which lives on `current_sheet`) for a row insertion.
pub fn shift_formula(formula: &str, current_sheet: &str, shift: &RowShift) -> String {
    map_references(formula, current_sheet, |sheet, first, second| {
        if !sheet.eq_ignore_ascii_case(&shift.sheet) {
            return None;
        }
        Some(match second {
            None => format_cell(Cell {
                row: shift.shift_start(first.row),
                ..first
            }),
            Some(second) => {
                let (low, high) = if first.row <= second.row {
                    (first, second)
                } else {
                    (second, first)
                };
                let low_row = shift.shift_start(low.row);
                let high_row = shift.shift_end(low.row, high.row);
                format!(
                    "{}:{}",
                    format_cell(Cell {
                        row: low_row,
                        ..low
                    }),
                    format_cell(Cell {
                        row: high_row,
                        ..high
                    })
                )
            }
        })
    })
}

/// Rewrites a reference list such as `sqref="A5:A10 C5"` on `current_sheet`.
pub fn shift_reference_list(list: &str, current_sheet: &str, shift: &RowShift) -> String {
    list.split_whitespace()
        .map(|item| shift_formula(item, current_sheet, shift))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Moves every relative reference by `(rows, columns)` the way a copied formula does.
/// Returns `None` if a reference would leave the sheet.
pub fn translate_formula(formula: &str, rows: i64, columns: i64) -> Option<String> {
    let mut failed = false;
    let translated = map_references(formula, "", |_, first, second| {
        let mut move_cell = |cell: Cell| -> Cell {
            let row = if cell.row_abs {
                i64::from(cell.row)
            } else {
                i64::from(cell.row) + rows
            };
            let col = if cell.col_abs {
                i64::from(cell.col)
            } else {
                i64::from(cell.col) + columns
            };
            if !(1..=1_048_576).contains(&row) || !(1..=16_384).contains(&col) {
                failed = true;
                return cell;
            }
            Cell {
                row: u32::try_from(row).unwrap_or(1),
                col: u32::try_from(col).unwrap_or(1),
                ..cell
            }
        };
        let first = move_cell(first);
        Some(match second {
            None => format_cell(first),
            Some(second) => format!("{}:{}", format_cell(first), format_cell(move_cell(second))),
        })
    });
    (!failed).then_some(translated)
}

/// True if the formula mentions any cell of `sheet` (qualified, or unqualified when
/// `current_sheet` is `sheet`).
pub fn references_sheet(formula: &str, current_sheet: &str, sheet: &str) -> bool {
    let mut found = false;
    map_references(formula, current_sheet, |target, _, _| {
        found |= target.eq_ignore_ascii_case(sheet);
        None
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shift() -> RowShift {
        RowShift {
            sheet: "Road".into(),
            at: 10,
            count: 1,
        }
    }

    #[test]
    fn references_never_pass_the_last_worksheet_row() {
        let s = shift();
        assert_eq!(
            shift_reference_list("P1:P1048576", "Road", &s),
            "P1:P1048576"
        );
        assert_eq!(
            shift_formula("SUM(A5:A1048576)", "Road", &s),
            "SUM(A5:A1048576)"
        );
        assert_eq!(
            shift_reference_list("A10:A1048575", "Road", &s),
            "A11:A1048576"
        );
    }

    #[test]
    fn single_cells_at_or_below_the_insertion_move() {
        assert_eq!(
            shift_formula("A9+A10+$B$11", "Road", &shift()),
            "A9+A11+$B$12"
        );
    }

    #[test]
    fn ranges_extend_when_the_insertion_is_inside_or_appends_under_them() {
        let s = shift();
        assert_eq!(
            shift_formula("AVERAGE(Y5:Y48)", "Road", &s),
            "AVERAGE(Y5:Y49)"
        );
        assert_eq!(shift_formula("SUM(Y5:Y9)", "Road", &s), "SUM(Y5:Y10)");
        assert_eq!(shift_formula("SUM(Y5:Y8)", "Road", &s), "SUM(Y5:Y8)");
        assert_eq!(shift_formula("SUM(Y10:Y20)", "Road", &s), "SUM(Y11:Y21)");
    }

    #[test]
    fn row_local_ranges_ending_on_the_anchor_row_do_not_extend() {
        let s = shift();
        assert_eq!(
            shift_formula("COUNTIF(C9:P9,\"Completed\")", "Road", &s),
            "COUNTIF(C9:P9,\"Completed\")"
        );
    }

    #[test]
    fn other_sheets_and_strings_are_untouched() {
        let s = shift();
        assert_eq!(
            shift_formula("Other!A20+Road!A20+IF(A20=\"A20\",1,0)", "Other", &s),
            "Other!A20+Road!A21+IF(A20=\"A20\",1,0)"
        );
        assert_eq!(
            shift_formula("'Signed By Worley'!A20", "Road", &s),
            "'Signed By Worley'!A20"
        );
        assert_eq!(
            shift_formula("COUNTIFS(Road!$D$5:$D$47,\"x\")", "STAT", &s),
            "COUNTIFS(Road!$D$5:$D$48,\"x\")"
        );
    }

    #[test]
    fn function_names_that_look_like_cells_are_not_references() {
        assert_eq!(
            shift_formula("LOG10(A20)+ATAN2(1,2)", "Road", &shift()),
            "LOG10(A21)+ATAN2(1,2)"
        );
    }

    #[test]
    fn reference_lists_shift_each_range() {
        assert_eq!(
            shift_reference_list("A5:A48 C12", "Road", &shift()),
            "A5:A49 C13"
        );
    }

    #[test]
    fn translation_moves_relative_parts_only() {
        assert_eq!(
            translate_formula(
                "(COUNTIF(C5:P5,\"Completed\")+COUNTIF(C5:P5,\"NA\"))/6",
                1,
                0
            )
            .unwrap(),
            "(COUNTIF(C6:P6,\"Completed\")+COUNTIF(C6:P6,\"NA\"))/6"
        );
        assert_eq!(
            translate_formula("$A$1+A$1+$A1", 2, 1).unwrap(),
            "$A$1+B$1+$A3"
        );
        assert_eq!(translate_formula("A1", -1, 0), None);
    }

    #[test]
    fn detects_references_to_a_sheet() {
        assert!(references_sheet("SUM(Road!A1:A9)", "STAT", "Road"));
        assert!(!references_sheet("SUM(A1:A9)", "STAT", "Road"));
        assert!(references_sheet("SUM(A1:A9)", "Road", "Road"));
        assert!(!references_sheet("\"Road!A1\"", "STAT", "Road"));
    }
}
