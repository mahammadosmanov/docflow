use super::*;

fn settings() -> PrintSettings {
    PrintSettings {
        paper: "A4".to_string(),
        orientation: "landscape".to_string(),
        scaling: "shrink".to_string(),
        color: "grayscale".to_string(),
        duplex: "short".to_string(),
        page_range: "all".to_string(),
        timeout_minutes: 30,
    }
}

#[test]
fn maps_controlled_job_settings_to_devmode_values() {
    assert_eq!(
        requested_devmode(&settings()).unwrap(),
        RequestedDevMode {
            paper: Some(DMPAPER_A4 as i16),
            orientation: Some(DMORIENT_LANDSCAPE as i16),
            color: Some(DMCOLOR_MONOCHROME),
            duplex: Some(DMDUP_HORIZONTAL),
        }
    );
}

#[test]
fn rejects_unmapped_values_before_any_native_call() {
    let mut unsupported = settings();
    unsupported.paper = "A4;Remove-Item C:\\".to_string();
    assert_eq!(
        requested_devmode(&unsupported).unwrap_err().kind,
        crate::print_queue::PrintFailureKind::UnsupportedOptions
    );
    assert!(!safe_extension("docx;calc"));
    assert!(!safe_extension(""));
    assert!(safe_extension("docx"));
}

#[test]
fn printer_argument_is_data_not_a_command() {
    let argument = format!("\"{}\"", "Office Printer & calc.exe");
    assert_eq!(argument, "\"Office Printer & calc.exe\"");
}

#[test]
fn maps_fit_actual_and_shrink_to_printer_dpi() {
    assert_eq!(
        page_scale(300, 600, 600, 1200, 600, 600, "actual").unwrap(),
        2.0
    );
    assert_eq!(
        page_scale(300, 600, 300, 600, 600, 600, "shrink").unwrap(),
        1.0
    );
    assert_eq!(
        page_scale(300, 600, 150, 300, 600, 600, "fit").unwrap(),
        0.5
    );
    assert_eq!(
        image_destination(300, 600, 300.0, 300.0, 1200, 1200, 600, 600, "actual").unwrap(),
        (600, 1200)
    );
    assert_eq!(
        image_destination(300, 600, 300.0, 300.0, 300, 600, 600, 600, "shrink").unwrap(),
        (300, 600)
    );
    assert!(CONTROLLED_IMAGE_FORMATS.contains(&"tiff"));
}
