//! Deterministic mutation tests for parsers that read untrusted documents.
//! Every mutated XLSX, PDF, or email text must return a value or an error;
//! a panic fails the test and prints the reproducing seed.

use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
    panic::{AssertUnwindSafe, catch_unwind},
};

use docflow_lib::{
    email_extract::{EmailContext, offline_extract},
    ooxml::read_workbook_from_reader,
    pdf::inspect,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const ITERATIONS: u64 = 2_000;
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }
}

fn mutate(seed: &[u8], round: u64) -> Vec<u8> {
    let mut rng = XorShift(round.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut bytes = seed.to_vec();
    for _ in 0..=rng.below(8) {
        match rng.below(4) {
            0 if !bytes.is_empty() => {
                let index = rng.below(bytes.len());
                bytes[index] ^= 1 << rng.below(8);
            }
            1 if !bytes.is_empty() => {
                let index = rng.below(bytes.len());
                bytes[index] = rng.next() as u8;
            }
            2 => {
                let index = rng.below(bytes.len() + 1);
                bytes.insert(index, rng.next() as u8);
            }
            _ => {
                let length = rng.below(bytes.len() + 1);
                bytes.truncate(length);
            }
        }
    }
    bytes
}

fn assert_no_panic(target: &str, round: u64, run: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(run)).is_err() {
        panic!("{target} parser panicked on mutation round {round}");
    }
}

fn seed_workbook() -> Vec<u8> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let parts = [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#
                .to_string(),
        ),
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
        (
            "xl/worksheets/sheet1.xml",
            format!(
                r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>AZE-SUN-INT-QMS-RFI-CIV-0001</t></is></c><c r="B1"><v>42</v></c></row></sheetData></worksheet>"#
            ),
        ),
    ];
    for (name, body) in parts {
        writer.start_file(name, options).expect("start zip member");
        writer.write_all(body.as_bytes()).expect("write zip member");
    }
    writer.finish().expect("finish zip").into_inner()
}

fn seed_pdf() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Rotate 90 /Contents 4 0 R >>",
        "<< /Length 9 >>\nstream\nBT ET q Q\nendstream",
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    bytes
}

#[test]
fn mutated_workbooks_never_panic() {
    let seed = seed_workbook();
    assert!(read_workbook_from_reader(&mut Cursor::new(seed.clone())).is_ok());
    for round in 0..ITERATIONS {
        let bytes = mutate(&seed, round);
        assert_no_panic("OOXML", round, || {
            let _ = read_workbook_from_reader(&mut Cursor::new(bytes));
        });
    }
}

#[test]
fn mutated_pdfs_never_panic() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("mutated.pdf");
    let seed = seed_pdf();
    std::fs::write(&path, &seed).expect("write seed PDF");
    let inspected = inspect(&path);
    assert!(inspected.is_ok(), "{inspected:?}");
    for round in 0..ITERATIONS {
        std::fs::write(&path, mutate(&seed, round)).expect("write mutated PDF");
        assert_no_panic("PDF", round, || {
            let _ = inspect(&path);
        });
    }
}

#[test]
fn mutated_email_text_never_panics() {
    let seed = "Subject: AZE-SUN-INT-QMS-RFI-MEC-0012 inspection request\n\
        Dear Sir, please inspect ITP-MEC-0005 item MEC-5.1.6 at PCS-19 on 07/11/2026 \
        Şimal Cənub — Tag A12/1 subcontractor ACME.\n"
        .as_bytes();
    let context = EmailContext {
        discipline: "MEC".to_string(),
        itp_to_items: BTreeMap::from([("ITP-MEC-0005".to_string(), vec!["MEC-5.1.6".to_string()])]),
        known_tags: vec!["PCS-19".to_string(), "A12/1".to_string()],
        known_subcontractors: vec!["ACME".to_string()],
    };
    for round in 0..ITERATIONS {
        let text = String::from_utf8_lossy(&mutate(seed, round)).into_owned();
        assert_no_panic("email", round, || {
            let _ = offline_extract(&text, &context);
        });
    }
}
