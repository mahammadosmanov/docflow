//! Opt-in timing probes: `cargo test --release --test perf_ooxml -- --ignored --nocapture`.
use std::{
    collections::BTreeMap,
    io::{Cursor, Seek, SeekFrom, Write},
    time::Instant,
};

use docflow_lib::ooxml::{
    CellData, CellUpdate, GuardedCellUpdate, patch_workbook_sheets_guarded_text,
    read_workbook_from_reader, verify_guarded_workbook_delta_text,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const ROWS: u32 = 20_000;
const COLUMNS: u32 = 12;

fn column_name(column: u32) -> char {
    (b'A' + (column - 1) as u8) as char
}

fn big_workbook() -> Cursor<Vec<u8>> {
    let mut sheet = format!(r#"<worksheet xmlns="{MAIN}"><sheetData>"#);
    for row in 1..=ROWS {
        sheet.push_str(&format!(r#"<row r="{row}">"#));
        for column in 1..=COLUMNS {
            let name = column_name(column);
            sheet.push_str(&format!(
                r#"<c r="{name}{row}" t="inlineStr"><is><t>RFI-{row}-{column} some cell text</t></is></c>"#
            ));
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let parts = [
        ("[Content_Types].xml", "content-types".to_string()),
        (
            "_rels/.rels",
            format!(
                r#"<Relationships xmlns="{PKG}"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
            ),
        ),
        (
            "xl/workbook.xml",
            format!(
                r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="CIV" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            format!(
                r#"<Relationships xmlns="{PKG}"><Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
            ),
        ),
        ("xl/worksheets/sheet1.xml", sheet),
    ];
    for (name, body) in parts {
        writer.start_file(name, options).unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    writer.finish().unwrap()
}

#[test]
#[ignore = "timing probe"]
fn ooxml_read_and_guarded_patch_timing() {
    let mut source = big_workbook();
    println!("package bytes: {}", source.get_ref().len());

    let started = Instant::now();
    let workbook = read_workbook_from_reader(&mut source).unwrap();
    println!(
        "read {} cells: {:?}",
        workbook.sheet("CIV").unwrap().cells.len(),
        started.elapsed()
    );

    let updates = BTreeMap::from([(
        "CIV".to_string(),
        (1..=200)
            .map(|row| {
                (
                    (row * 90, 13),
                    GuardedCellUpdate {
                        expected: CellData::default(),
                        replacement: CellUpdate::Text("RFI Status Sync: Completed".to_string()),
                        create_if_absent: true,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )]);
    source.seek(SeekFrom::Start(0)).unwrap();
    let mut destination = Cursor::new(Vec::new());
    let started = Instant::now();
    patch_workbook_sheets_guarded_text(&mut source, &mut destination, &updates).unwrap();
    println!("guarded patch of 200 cells: {:?}", started.elapsed());
    let started = Instant::now();
    verify_guarded_workbook_delta_text(&mut source, &mut destination, &updates).unwrap();
    println!("verify delta: {:?}", started.elapsed());
}
