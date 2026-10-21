use super::*;

pub(super) fn paeth(left: u8, up: u8, upper_left: u8) -> u8 {
    let estimate = i32::from(left) + i32::from(up) - i32::from(upper_left);
    let left_distance = (estimate - i32::from(left)).abs();
    let up_distance = (estimate - i32::from(up)).abs();
    let upper_left_distance = (estimate - i32::from(upper_left)).abs();
    if left_distance <= up_distance && left_distance <= upper_left_distance {
        left
    } else if up_distance <= upper_left_distance {
        up
    } else {
        upper_left
    }
}

pub(super) fn find_startxref(bytes: &[u8]) -> Result<usize, PdfError> {
    let marker = b"startxref";
    let position = bytes
        .windows(marker.len())
        .rposition(|window| window == marker)
        .ok_or_else(|| PdfError::new("PDF startxref is missing"))?;
    let mut parser = Parser::at(bytes, position + marker.len())?;
    parser.read_unsigned()
}

pub(super) fn integer_array(object: &PdfObject) -> Result<Vec<usize>, PdfError> {
    object
        .as_array()
        .ok_or_else(|| PdfError::new("PDF integer array is invalid"))?
        .iter()
        .map(|value| {
            value
                .as_usize()
                .ok_or_else(|| PdfError::new("PDF integer array contains a non-integer"))
        })
        .collect()
}

pub(super) fn box_values(object: &PdfObject) -> Option<Vec<String>> {
    let values = object.as_array()?;
    (values.len() == 4).then(|| {
        values
            .iter()
            .map(|value| match value {
                PdfObject::Number(number) => Some(number.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
    })?
}

pub(super) fn object_text(object: &PdfObject) -> String {
    match object {
        PdfObject::String(value) | PdfObject::Name(value) => {
            String::from_utf8_lossy(value).into_owned()
        }
        _ => String::new(),
    }
}

pub(super) fn read_bounded(path: &Path) -> Result<Vec<u8>, PdfError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| PdfError::new(format!("PDF metadata failed: {error}")))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_PDF_BYTES {
        return Err(PdfError::new("PDF file type or size is unsupported"));
    }
    fs::read(path).map_err(|error| PdfError::new(format!("PDF read failed: {error}")))
}

pub(super) fn read_be(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .fold(0usize, |value, byte| (value << 8) | usize::from(*byte))
}

pub(super) fn hex_byte(bytes: &[u8]) -> Result<u8, PdfError> {
    if bytes.len() != 2 {
        return Err(PdfError::new("PDF hex byte is truncated"));
    }
    let value = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    value(bytes[0])
        .and_then(|high| value(bytes[1]).map(|low| high * 16 + low))
        .ok_or_else(|| PdfError::new("PDF hex byte is invalid"))
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | b'\t' | b'\n' | 12 | b'\r' | b' ')
}

pub(super) fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}
