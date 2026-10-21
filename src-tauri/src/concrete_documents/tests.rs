use super::*;
use zip::{CompressionMethod, write::SimpleFileOptions};

#[test]
fn extends_civ010_template_rows_without_overwriting_following_rows() {
    let worksheet = r#"<worksheet><dimension ref="A1:CS22"/><sheetData><row r="19"><c r="AI19"><v>1</v></c></row><row r="20"></row><row r="21"><c r="AI21"><v>2</v></c></row><row r="22"><c r="AI22"><v>3</v></c></row></sheetData></worksheet>"#;
    let expanded = expand_civ010_batch_rows(worksheet, 4).unwrap();
    assert!(expanded.contains(r#"<dimension ref="A1:CS26"/>"#));
    assert!(expanded.contains(r#"<row r="23"><c r="AI23"><v>2</v></c></row>"#));
    assert!(expanded.contains(r#"<row r="25"><c r="AI25"><v>2</v></c></row>"#));
    assert!(expanded.contains(r#"<row r="26"><c r="AI26"><v>3</v></c></row>"#));
}

fn record(rfi: &str, location: &str) -> SourceRecord {
    SourceRecord {
        rfi: rfi.to_string(),
        location: location.to_string(),
        tag: String::new(),
        concrete_class: String::new(),
        supplier: String::new(),
        batch_ticket: String::new(),
        truck: String::new(),
        quantity: String::new(),
        total_quantity: String::new(),
        batch_time: String::new(),
        pour_date: String::new(),
        start_time: String::new(),
        end_time: String::new(),
        plant_slump: String::new(),
        in_situ_slump: String::new(),
        plant_temp: String::new(),
        in_situ_temp: String::new(),
        samples: String::new(),
        water_cement_ratio: None,
        ambient_range: None,
        drawing: String::new(),
        additional_batches: Vec::new(),
    }
}

fn package(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in entries {
        zip.start_file(
            *name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn read_member(bytes: &[u8], name: &str) -> String {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut member = zip.by_name(name).unwrap();
    let mut text = String::new();
    member.read_to_string(&mut text).unwrap();
    text
}

#[test]
fn generates_docx_per_record_and_preserves_unrelated_parts() {
    let template = package(&[
        (
            "word/document.xml",
            "<w:document><w:p><w:t>AZE-SUN-INT-QMS-</w:t><w:t>RFI-CIV-1498</w:t></w:p><w:p><w:t>LOCATION</w:t><w:t>SOUTH SITE PCU-18</w:t></w:p><w:p><w:t>AMBIENT TEMP.</w:t><w:t>27 °C</w:t></w:p></w:document>",
        ),
        (
            "word/styles.xml",
            "<w:styles><w:style w:val=\"unchanged\"/></w:styles>",
        ),
    ]);
    let mut first = record("AZE-SUN-INT-QMS-RFI-CIV-2001", "PCU-1");
    first.ambient_range = Some((31, 31));
    let mut second = record("AZE-SUN-INT-QMS-RFI-CIV-2002", "PCU-2");
    second.ambient_range = Some((42, 42));
    let result = generate(
        DocumentType::Civ011,
        &template,
        &[first, second],
        (20, 30),
        4,
    )
    .unwrap();
    assert_eq!(result.len(), 2);
    assert!(read_member(&result[0].bytes, "word/document.xml").contains("31 °C"));
    assert!(read_member(&result[0].bytes, "word/document.xml").contains("PCU-1"));
    assert!(read_member(&result[1].bytes, "word/document.xml").contains("42 °C"));
    assert!(read_member(&result[1].bytes, "word/document.xml").contains("PCU-2"));
    assert_eq!(
        read_member(&result[0].bytes, "word/styles.xml"),
        "<w:styles><w:style w:val=\"unchanged\"/></w:styles>"
    );
}

#[test]
fn generates_xlsx_and_rejects_missing_anchor() {
    let template = package(&[
        (
            "xl/worksheets/sheet1.xml",
            "<worksheet><row><c><is><t>AZE-SUN-INT-QMS-RFI-CIV-1500</t></is></c><c><is><t>LOCATION/STRUCTURE</t></is></c><c><is><t>PCU-18 (South Site)</t></is></c></row></worksheet>",
        ),
        ("xl/styles.xml", "<styleSheet/>"),
    ]);
    let result = generate(
        DocumentType::Civ013,
        &template,
        &[record("AZE-SUN-INT-QMS-RFI-CIV-2003", "PCU-9")],
        (0, 0),
        1,
    )
    .unwrap();
    assert!(read_member(&result[0].bytes, "xl/worksheets/sheet1.xml").contains("PCU-9"));
    let invalid = package(&[("xl/worksheets/sheet1.xml", "<worksheet/>")]);
    assert!(
        generate(
            DocumentType::Civ013,
            &invalid,
            &[record("CIV-1", "x")],
            (0, 0),
            0
        )
        .is_err()
    );
}

#[test]
fn generates_civ010_data_and_civ014_identity() {
    let mut source = record("AZE-SUN-INT-QMS-RFI-CIV-2010", "N-5");
    source.tag = "N-5".to_string();
    source.concrete_class = "C-30/37 XA2".to_string();
    source.supplier = "HOLCIM".to_string();
    source.batch_ticket = "66817".to_string();
    source.truck = "TRK-1".to_string();
    source.quantity = "5".to_string();
    source.total_quantity = "15".to_string();
    source.batch_time = "09:30".to_string();
    source.pour_date = "06.10.2026".to_string();
    source.start_time = "10:00".to_string();
    source.end_time = "11:00".to_string();
    source.plant_slump = "175".to_string();
    source.in_situ_slump = "170".to_string();
    source.plant_temp = "21.3".to_string();
    source.in_situ_temp = "21.4".to_string();
    source.samples = "841-846".to_string();
    let mut cells = vec![
        ("A11".to_string(), "DRAWING NO: DRG-1".to_string(), true),
        (
            "A12".to_string(),
            "REPORT (RFI) NUMBER: AZE-SUN-INT-QMS-RFI-CIV-1460".to_string(),
            true,
        ),
        (
            "A13".to_string(),
            "LOCATION: North site area / N-5".to_string(),
            true,
        ),
        (
            "BB14".to_string(),
            "CONCRETE POURING DATE:06.10.2026".to_string(),
            true,
        ),
        ("A19".to_string(), "N-5".to_string(), true),
        ("H19".to_string(), "C-30/37 XA2".to_string(), true),
        ("O19".to_string(), "HOLCIM".to_string(), true),
        ("V19".to_string(), "66817".to_string(), true),
        ("AC19".to_string(), "TRK-1".to_string(), true),
        ("CS19".to_string(), "841-846".to_string(), true),
    ];
    for reference in [
        "AI", "AP", "AW", "BD", "BK", "BR", "BY", "CC", "CF", "CJ", "CM",
    ] {
        cells.push((format!("{reference}19"), "1".to_string(), false));
        cells.push((format!("{reference}21"), "2".to_string(), false));
    }
    for reference in ["A", "H", "O", "V", "AC", "CS"] {
        cells.push((format!("{reference}21"), String::new(), true));
    }
    let render_row = |row: usize| {
        cells
            .iter()
            .filter(|(reference, _, _)| {
                reference
                    .rfind(|character: char| !character.is_ascii_digit())
                    .and_then(|index| reference[index + 1..].parse::<usize>().ok())
                    == Some(row)
            })
            .map(|(reference, value, text)| {
                if *text {
                    format!("<c r=\"{reference}\" t=\"inlineStr\"><is><t>{value}</t></is></c>")
                } else {
                    format!("<c r=\"{reference}\"><v>{value}</v></c>")
                }
            })
            .collect::<String>()
    };
    let worksheet = format!(
        "<worksheet><dimension ref=\"A1:CS22\"/><sheetData><row r=\"11\">{}</row><row r=\"12\">{}</row><row r=\"13\">{}</row><row r=\"14\">{}</row><row r=\"19\">{}</row><row r=\"20\"></row><row r=\"21\">{}</row><row r=\"22\"><c r=\"AI22\"><v>3</v></c></row></sheetData></worksheet>",
        render_row(11),
        render_row(12),
        render_row(13),
        render_row(14),
        render_row(19),
        render_row(21)
    );
    let xlsx = package(&[("xl/worksheets/sheet1.xml", &worksheet)]);
    let output = generate(DocumentType::Civ010, &xlsx, &[source], (0, 0), 2).unwrap();
    let sheet = read_member(&output[0].bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("CIV-2010"), "{sheet}");
    assert!(sheet.contains("N-5"));
    assert!(sheet.contains("06.10.2026"));
    let cell = |reference: &str| {
        Regex::new(&format!(r#"(?s)<c\b[^>]*\br="{reference}"[^>]*>(.*?)</c>"#))
            .unwrap()
            .captures(&sheet)
            .map(|captures| captures[1].to_string())
            .unwrap()
    };
    assert!(cell("A12").contains("CIV-2010"));
    assert!(cell("H19").contains("C-30/37 XA2"));
    assert!(cell("AI19").contains("<v>5</v>"));
    assert!(cell("AP19").contains("<v>15</v>"));
    assert!(cell("CF19").contains("<v>21.3</v>"));
    let ambient_value = Regex::new(r"<v>([^<]+)</v>")
        .unwrap()
        .captures(&cell("BD19"))
        .unwrap()[1]
        .parse::<f64>()
        .unwrap();
    assert!((20.6..=20.8).contains(&ambient_value));
    assert!(cell("AI21").is_empty());
    let mut multi = record("AZE-SUN-INT-QMS-RFI-CIV-2010", "N-5");
    multi.plant_temp = "21.3".to_string();
    multi.additional_batches.push(ConcreteBatch {
        tag: "N-5".to_string(),
        concrete_class: "C-30/37 XA2".to_string(),
        supplier: "HOLCIM".to_string(),
        batch_ticket: "66818".to_string(),
        truck: "TRK-2".to_string(),
        quantity: "6".to_string(),
        total_quantity: "15".to_string(),
        batch_time: "10:00".to_string(),
        start_time: "10:00".to_string(),
        end_time: "11:00".to_string(),
        plant_slump: "175".to_string(),
        in_situ_slump: "170".to_string(),
        plant_temp: "21.3".to_string(),
        in_situ_temp: "21.4".to_string(),
        samples: "847-852".to_string(),
    });
    let mut third = multi.additional_batches[0].clone();
    third.batch_ticket = "66819".to_string();
    third.quantity = "7".to_string();
    third.samples = "853-858".to_string();
    multi.additional_batches.push(third);
    let multi_output = generate(DocumentType::Civ010, &xlsx, &[multi], (0, 0), 2).unwrap();
    let multi_sheet = read_member(&multi_output[0].bytes, "xl/worksheets/sheet1.xml");
    let second_cell = |reference: &str| {
        Regex::new(&format!(r#"(?s)<c\b[^>]*\br="{reference}"[^>]*>(.*?)</c>"#))
            .unwrap()
            .captures(&multi_sheet)
            .map(|captures| captures[1].to_string())
            .unwrap()
    };
    assert!(second_cell("AI21").contains("<v>6</v>"));
    assert!(second_cell("V21").contains("66818"));
    assert!(second_cell("CS21").contains("847-852"));
    assert!(second_cell("AI23").contains("<v>7</v>"));
    assert!(second_cell("V23").contains("66819"));
    assert!(second_cell("CS23").contains("853-858"));
    assert!(second_cell("AI24").contains("<v>3</v>"));
    assert!(multi_sheet.contains(r#"<dimension ref="A1:CS24"/>"#));

    let docx = package(&[(
        "word/document.xml",
        "<w:document><w:p><w:t>REPORT NO.</w:t><w:t>AZE-SUN-INT-QMS-RFI-CIV-1404</w:t></w:p><w:p><w:t>LOCATION</w:t><w:t>(North Site) N5</w:t></w:p></w:document>",
    )]);
    let output = generate(
        DocumentType::Civ014,
        &docx,
        &[record("AZE-SUN-INT-QMS-RFI-CIV-2014", "South Site")],
        (0, 0),
        2,
    )
    .unwrap();
    let document = read_member(&output[0].bytes, "word/document.xml");
    assert!(document.contains("CIV-2014"));
    assert!(document.contains("South Site"));
}
