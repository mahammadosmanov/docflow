use std::{collections::BTreeMap, fs, path::Path};

pub const CODE: &str = "AZE-SUN-INT-QMS-RFI-CIV-0064";

#[derive(Default)]
struct ClassicPdf {
    objects: BTreeMap<u32, Vec<u8>>,
    root: u32,
    info: Option<u32>,
    trailer_extra: String,
}

impl ClassicPdf {
    fn object(&mut self, number: u32, body: impl Into<Vec<u8>>) {
        self.objects.insert(number, body.into());
    }

    fn finish(self) -> Vec<u8> {
        let max_id = self.objects.keys().copied().max().unwrap_or(0);
        let mut bytes = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = BTreeMap::new();
        for (number, body) in self.objects {
            offsets.insert(number, bytes.len());
            bytes.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
            bytes.extend_from_slice(&body);
            bytes.extend_from_slice(b"\nendobj\n");
        }
        let xref = bytes.len();
        bytes.extend_from_slice(format!("xref\n0 {}\n", max_id + 1).as_bytes());
        bytes.extend_from_slice(b"0000000000 65535 f \n");
        for number in 1..=max_id {
            if let Some(offset) = offsets.get(&number) {
                bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            } else {
                bytes.extend_from_slice(b"0000000000 00000 f \n");
            }
        }
        bytes.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root {} 0 R{}{} >>\nstartxref\n{xref}\n%%EOF\n",
                max_id + 1,
                self.root,
                self.info
                    .map(|number| format!(" /Info {number} 0 R"))
                    .unwrap_or_default(),
                self.trailer_extra,
            )
            .as_bytes(),
        );
        bytes
    }
}

fn literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn text_stream(text: &str) -> Vec<u8> {
    let content = format!("BT /F1 16 Tf 36 700 Td ({}) Tj ET\n", literal(text));
    format!(
        "<< /Length {} >>\nstream\n{content}endstream",
        content.len()
    )
    .into_bytes()
}

fn raw_stream(dictionary: &str, data: &[u8]) -> Vec<u8> {
    let mut body = format!("<< {dictionary} /Length {} >>\nstream\n", data.len()).into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    body
}

pub fn write_rich_archive(path: &Path) {
    let mut pdf = ClassicPdf {
        root: 1,
        info: Some(14),
        ..ClassicPdf::default()
    };
    pdf.object(
        1,
        b"<< /Type /Catalog /Pages 2 0 R /Metadata 15 0 R /Names << /EmbeddedFiles 16 0 R >> /Outlines 18 0 R /PageLabels 20 0 R /AcroForm 22 0 R /OpenAction [3 0 R /Fit] /ViewerPreferences << /DisplayDocTitle true >> >>".to_vec(),
    );
    pdf.object(
        2,
        b"<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>".to_vec(),
    );
    pdf.object(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources 6 0 R /Contents 7 0 R >>"
            .to_vec(),
    );
    pdf.object(
        4,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /CropBox [12 18 580 760] /Rotate 90 /Resources 6 0 R /Contents 8 0 R /Annots [23 0 R 24 0 R 25 0 R] >>".to_vec(),
    );
    pdf.object(
        5,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources 6 0 R /Contents 9 0 R >>"
            .to_vec(),
    );
    pdf.object(6, b"<< /Font << /F1 10 0 R >> >>".to_vec());
    pdf.object(7, text_stream(&format!("ARCHIVE OLD FIRST PAGE {CODE}")));
    pdf.object(8, text_stream("ARCHIVE SECOND PAGE BYTE FIDELITY SENTINEL"));
    pdf.object(9, text_stream("ARCHIVE THIRD PAGE BYTE FIDELITY SENTINEL"));
    pdf.object(
        10,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    );
    pdf.object(
        14,
        b"<< /Title (Original RFI) /Author (AZFEN) /Custom (KEEP-METADATA) >>".to_vec(),
    );
    pdf.object(
        15,
        raw_stream(
            "/Type /Metadata /Subtype /XML",
            b"<x:xmpmeta>KEEP-XMP</x:xmpmeta>",
        ),
    );
    pdf.object(16, b"<< /Names [(archive-note.txt) 17 0 R] >>".to_vec());
    pdf.object(
        17,
        b"<< /Type /Filespec /F (archive-note.txt) /EF << /F 21 0 R >> >>".to_vec(),
    );
    pdf.object(
        18,
        b"<< /Type /Outlines /First 19 0 R /Last 27 0 R /Count 2 >>".to_vec(),
    );
    pdf.object(
        19,
        b"<< /Title (RFI cover) /Parent 18 0 R /Next 27 0 R /Dest [3 0 R /Fit] >>".to_vec(),
    );
    pdf.object(20, b"<< /Nums [0 << /S /D /P (RFI-) /St 1 >>] >>".to_vec());
    pdf.object(
        21,
        raw_stream("/Type /EmbeddedFile", b"keep this embedded archive data"),
    );
    pdf.object(
        22,
        b"<< /Fields [25 0 R] /NeedAppearances false /DA (/F1 10 Tf 0 g) >>".to_vec(),
    );
    pdf.object(23, b"<< /Type /Annot /Subtype /Text /Rect [40 40 60 60] /Contents (KEEP ANNOTATION) /P 4 0 R >>".to_vec());
    pdf.object(24, b"<< /Type /Annot /Subtype /Link /Rect [70 70 160 90] /A << /S /URI /URI (https://example.invalid/keep) >> /P 4 0 R >>".to_vec());
    pdf.object(25, b"<< /Type /Annot /Subtype /Widget /FT /Tx /T (KeepField) /V (keep-value) /Rect [100 100 240 125] /P 4 0 R /AP << /N 26 0 R >> >>".to_vec());
    pdf.object(
        26,
        raw_stream("/Type /XObject /Subtype /Form /BBox [0 0 140 25]", b"q Q"),
    );
    pdf.object(
        27,
        b"<< /Title (RFI continuation) /Parent 18 0 R /Prev 19 0 R /Dest [4 0 R /Fit] >>".to_vec(),
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, pdf.finish()).unwrap();
}

pub fn write_signed(path: &Path, extra_page: bool) {
    let mut pdf = ClassicPdf {
        root: 1,
        ..ClassicPdf::default()
    };
    let kids = if extra_page {
        "[3 0 R 4 0 R]"
    } else {
        "[3 0 R]"
    };
    let count = if extra_page { 2 } else { 1 };
    pdf.object(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    pdf.object(2, format!("<< /Type /Pages /Kids {kids} /Count {count} /Resources 5 0 R /MediaBox [0 0 612 792] /Rotate 270 >>").into_bytes());
    pdf.object(
        3,
        b"<< /Type /Page /Parent 2 0 R /CropBox [10 20 600 770] /Contents 6 0 R /Annots [8 0 R] >>"
            .to_vec(),
    );
    if extra_page {
        pdf.object(
            4,
            b"<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>".to_vec(),
        );
    }
    pdf.object(5, b"<< /Font << /F1 9 0 R >> >>".to_vec());
    pdf.object(6, text_stream(&format!("SIGNED FIRST PAGE {CODE}")));
    if extra_page {
        pdf.object(7, text_stream("SIGNED SECOND PAGE MUST BE IGNORED"));
    }
    pdf.object(8, b"<< /Type /Annot /Subtype /Link /Rect [30 30 180 50] /A << /S /URI /URI (https://example.invalid/signed) >> /P 3 0 R >>".to_vec());
    pdf.object(
        9,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, pdf.finish()).unwrap();
}

pub fn write_signed_with_indirect_stream_length(path: &Path) {
    let mut pdf = ClassicPdf {
        root: 1,
        ..ClassicPdf::default()
    };
    let content = format!("BT /F1 16 Tf 36 700 Td (SIGNED FIRST PAGE {CODE}) Tj ET\n");
    pdf.object(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    pdf.object(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    pdf.object(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources 5 0 R /Contents 6 0 R >>"
            .to_vec(),
    );
    pdf.object(5, b"<< /Font << /F1 9 0 R >> >>".to_vec());
    pdf.object(
        6,
        format!("<< /Length 10 0 R >>\nstream\n{content}endstream").into_bytes(),
    );
    pdf.object(
        9,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    );
    pdf.object(10, content.len().to_string().into_bytes());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, pdf.finish()).unwrap();
}

pub fn write_encrypted(path: &Path) {
    let mut pdf = ClassicPdf {
        root: 1,
        trailer_extra: " /Encrypt 6 0 R /ID [<00112233><00112233>]".to_string(),
        ..ClassicPdf::default()
    };
    pdf.object(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    pdf.object(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    pdf.object(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>".to_vec(),
    );
    pdf.object(4, text_stream(&format!("ENCRYPTED {CODE}")));
    pdf.object(
        6,
        b"<< /Filter /Standard /V 2 /R 3 /Length 128 /O <00> /U <00> /P -4 >>".to_vec(),
    );
    fs::write(path, pdf.finish()).unwrap();
}

pub fn write_signed_document(path: &Path) {
    let mut pdf = ClassicPdf {
        root: 1,
        ..ClassicPdf::default()
    };
    pdf.object(
        1,
        b"<< /Type /Catalog /Pages 2 0 R /AcroForm 5 0 R /Perms << /DocMDP 7 0 R >> >>".to_vec(),
    );
    pdf.object(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    pdf.object(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Annots [6 0 R] >>"
            .to_vec(),
    );
    pdf.object(4, text_stream(&format!("SIGNED-DOCUMENT {CODE}")));
    pdf.object(5, b"<< /Fields [6 0 R] /SigFlags 3 >>".to_vec());
    pdf.object(6, b"<< /Type /Annot /Subtype /Widget /FT /Sig /T (Signature1) /V 7 0 R /Rect [0 0 100 40] /P 3 0 R >>".to_vec());
    pdf.object(7, b"<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange [0 100 200 300] /Contents <00112233> >>".to_vec());
    fs::write(path, pdf.finish()).unwrap();
}

pub fn write_tagged_document(path: &Path) {
    let mut pdf = ClassicPdf {
        root: 1,
        ..ClassicPdf::default()
    };
    pdf.object(
        1,
        b"<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 5 0 R >>"
            .to_vec(),
    );
    pdf.object(2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    pdf.object(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /StructParents 0 >>"
            .to_vec(),
    );
    pdf.object(4, text_stream(&format!("TAGGED {CODE}")));
    pdf.object(
        5,
        b"<< /Type /StructTreeRoot /K [6 0 R] /ParentTree << /Nums [0 [6 0 R]] >> >>".to_vec(),
    );
    pdf.object(
        6,
        b"<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R /K 0 >>".to_vec(),
    );
    fs::write(path, pdf.finish()).unwrap();
}

pub fn write_object_stream_pdf(path: &Path, first_text: &str, second_text: &str) {
    let page_two = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources 5 0 R /Contents 8 0 R /Rotate 90 >>";
    let header = b"7 0 ";
    let mut object_stream_data = header.to_vec();
    object_stream_data.extend_from_slice(page_two);

    let mut objects = BTreeMap::<u32, Vec<u8>>::new();
    objects.insert(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.insert(
        2,
        b"<< /Type /Pages /Kids [3 0 R 7 0 R] /Count 2 >>".to_vec(),
    );
    objects.insert(
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources 5 0 R /Contents 4 0 R >>"
            .to_vec(),
    );
    objects.insert(4, text_stream(first_text));
    objects.insert(5, b"<< /Font << /F1 10 0 R >> >>".to_vec());
    objects.insert(
        6,
        raw_stream(
            &format!("/Type /ObjStm /N 1 /First {}", header.len()),
            &object_stream_data,
        ),
    );
    objects.insert(8, text_stream(second_text));
    objects.insert(
        10,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    );

    let mut bytes = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = BTreeMap::new();
    for (number, body) in &objects {
        offsets.insert(*number, bytes.len());
        bytes.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        bytes.extend_from_slice(body);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref_id = 11_u32;
    let xref_offset = bytes.len();
    let size = 12_u32;
    let mut entries = Vec::new();
    for number in 0..size {
        let (kind, first, second) = if number == 0 {
            (0_u8, 0_u32, 65_535_u16)
        } else if number == 7 {
            (2, 6, 0)
        } else if number == xref_id {
            (1, u32::try_from(xref_offset).unwrap(), 0)
        } else if let Some(offset) = offsets.get(&number) {
            (1, u32::try_from(*offset).unwrap(), 0)
        } else {
            (0, 0, 0)
        };
        entries.push(kind);
        entries.extend_from_slice(&first.to_be_bytes());
        entries.extend_from_slice(&second.to_be_bytes());
    }
    let xref_body = raw_stream(
        &format!("/Type /XRef /Size {size} /Root 1 0 R /W [1 4 2] /Index [0 {size}]"),
        &entries,
    );
    bytes.extend_from_slice(format!("{xref_id} 0 obj\n").as_bytes());
    bytes.extend_from_slice(&xref_body);
    bytes.extend_from_slice(b"\nendobj\n");
    bytes.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
