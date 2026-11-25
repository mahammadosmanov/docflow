use super::*;

fn field(kind: FieldKind, format: &str) -> FieldDef {
    FieldDef {
        key: "f".into(),
        label: "Field".into(),
        kind,
        format: format.into(),
        ..FieldDef::default()
    }
}

#[test]
fn text_is_written_as_typed() {
    let text = field(FieldKind::Text, "");
    assert_eq!(convert(&text, " a  b ").unwrap(), " a  b ");
}

#[test]
fn numbers_accept_digits_with_a_dot_or_comma_and_nothing_else() {
    let number = field(FieldKind::Number, "");
    assert_eq!(convert(&number, "").unwrap(), "");
    assert_eq!(convert(&number, " 12 ").unwrap(), "12");
    assert_eq!(convert(&number, "-3,5").unwrap(), "-3,5");
    assert_eq!(convert(&number, "0.25").unwrap(), "0.25");
    for bad in ["abc", "1,2,3", "1.", ".5", "1e5", "12 m", "--1", "+1"] {
        let error = convert(&number, bad).expect_err(bad);
        assert!(error.contains("Field"), "{error}");
    }
}

#[test]
fn dates_are_read_in_common_forms_and_written_in_the_slot_format() {
    let date = field(FieldKind::Date, "");
    assert_eq!(convert(&date, "").unwrap(), "");
    assert_eq!(convert(&date, "07.11.2026").unwrap(), "07.11.2026");
    assert_eq!(convert(&date, "2026-11-07").unwrap(), "07.11.2026");
    assert_eq!(convert(&date, "7/11/2026").unwrap(), "07.11.2026");
    let custom = field(FieldKind::Date, "yyyy-MM-dd");
    assert_eq!(convert(&custom, "07.11.2026").unwrap(), "2026-11-07");
    let short = field(FieldKind::Date, "dd/MM/yy");
    assert_eq!(convert(&short, "07.11.2026").unwrap(), "07/11/26");
    for bad in [
        "31.02.2026",
        "2026-13-01",
        "tomorrow",
        "07.11",
        "00.01.2026",
    ] {
        assert!(convert(&date, bad).is_err(), "{bad}");
    }
    assert!(convert(&date, "29.02.2028").is_ok(), "leap day");
    assert!(convert(&date, "29.02.2027").is_err());
}

#[test]
fn checkboxes_write_the_checked_or_unchecked_text() {
    let default = field(FieldKind::Checkbox, "");
    assert_eq!(convert(&default, "true").unwrap(), "\u{2612}");
    assert_eq!(convert(&default, "false").unwrap(), "\u{2610}");
    let words = field(FieldKind::Checkbox, "Yes|No");
    assert_eq!(convert(&words, "true").unwrap(), "Yes");
    assert_eq!(convert(&words, "false").unwrap(), "No");
}

#[test]
fn starting_values_for_a_checkbox_are_read_leniently() {
    for yes in ["true", "TRUE", " yes ", "1", "x", "X"] {
        assert!(truthy(yes), "{yes}");
    }
    for no in ["", "false", "no", "0", "maybe"] {
        assert!(!truthy(no), "{no}");
    }
}

#[test]
fn formats_are_checked_when_the_map_is_saved() {
    assert!(check_format(FieldKind::Date, "").is_ok());
    assert!(check_format(FieldKind::Date, "dd.MM.yyyy").is_ok());
    assert!(check_format(FieldKind::Date, "yyyy").is_ok());
    assert!(check_format(FieldKind::Date, "hello").is_err());
    assert!(check_format(FieldKind::Date, "dd{MM").is_err());
    assert!(check_format(FieldKind::Date, &"d".repeat(41)).is_err());
    assert!(check_format(FieldKind::Checkbox, "").is_ok());
    assert!(check_format(FieldKind::Checkbox, "Yes|No").is_ok());
    for bad in ["Yes", "|No", "Yes|", "a|b|c", "{x}|y"] {
        assert!(check_format(FieldKind::Checkbox, bad).is_err(), "{bad}");
    }
    assert!(check_format(FieldKind::Text, "x").is_err());
    assert!(check_format(FieldKind::Number, "x").is_err());
    assert!(check_format(FieldKind::Number, "").is_ok());
}

#[test]
fn times_are_read_as_hours_and_minutes_and_written_as_hh_mm() {
    let time = field(FieldKind::Time, "");
    assert_eq!(convert(&time, "").unwrap(), "");
    assert_eq!(convert(&time, "9:30").unwrap(), "09:30");
    assert_eq!(convert(&time, " 23:05 ").unwrap(), "23:05");
    for bad in ["24:00", "12:60", "9", "9:5", "ab:cd", "9.30", "09:30:00"] {
        assert!(convert(&time, bad).is_err(), "{bad}");
    }
    assert!(check_format(FieldKind::Time, "").is_ok());
    assert!(check_format(FieldKind::Time, "hh:mm").is_err());
}

#[test]
fn an_excel_cell_gets_a_real_number_or_a_day_fraction() {
    assert_eq!(
        cell_number(&field(FieldKind::Number, ""), "3,5"),
        Some("3.5".into())
    );
    assert_eq!(
        cell_number(&field(FieldKind::Number, ""), "-12"),
        Some("-12".into())
    );
    assert_eq!(cell_number(&field(FieldKind::Number, ""), ""), None);
    let time = field(FieldKind::Time, "");
    assert_eq!(
        cell_number(&time, "09:30"),
        Some("0.395833333333333".into())
    );
    assert_eq!(
        cell_number(&time, "00:00"),
        Some("0.000000000000000".into())
    );
    assert_eq!(
        cell_number(&time, "12:00"),
        Some("0.500000000000000".into())
    );
    assert_eq!(cell_number(&field(FieldKind::Date, ""), "07.11.2026"), None);
    assert_eq!(cell_number(&field(FieldKind::Text, ""), "5"), None);
}
