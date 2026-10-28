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

pub(super) fn decode_stream(dictionary: &Dictionary, data: &[u8]) -> Result<Vec<u8>, PdfError> {
    let filters = match dictionary.get(b"Filter".as_slice()) {
        None => Vec::new(),
        Some(PdfObject::Name(name)) => vec![name.clone()],
        Some(PdfObject::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_name()
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| PdfError::new("PDF stream filter array is invalid"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(PdfError::new("PDF stream filter is invalid")),
    };
    let mut decoded = data.to_vec();
    for filter in filters {
        match filter.as_slice() {
            b"FlateDecode" | b"Fl" => {
                let decoder = ZlibDecoder::new(decoded.as_slice());
                let mut next = Vec::new();
                decoder
                    .take(MAX_DECOMPRESSED_STREAM as u64 + 1)
                    .read_to_end(&mut next)
                    .map_err(|error| {
                        PdfError::new(format!("PDF Flate stream is invalid: {error}"))
                    })?;
                if next.len() > MAX_DECOMPRESSED_STREAM {
                    return Err(PdfError::new("decompressed PDF stream exceeds the limit"));
                }
                decoded = next;
            }
            _ => {
                return Err(PdfError::new(format!(
                    "unsupported PDF stream filter /{}",
                    String::from_utf8_lossy(&filter)
                )));
            }
        }
    }
    apply_predictor(dictionary, decoded)
}

pub(super) fn apply_predictor(dictionary: &Dictionary, data: Vec<u8>) -> Result<Vec<u8>, PdfError> {
    let Some(parameters) = dictionary.get(b"DecodeParms".as_slice()) else {
        return Ok(data);
    };
    let parameters = match parameters {
        PdfObject::Dictionary(value) => value,
        PdfObject::Array(values) if values.len() == 1 => values[0]
            .as_dictionary()
            .ok_or_else(|| PdfError::new("PDF DecodeParms is invalid"))?,
        PdfObject::Null => return Ok(data),
        _ => return Err(PdfError::new("PDF DecodeParms is invalid")),
    };
    let predictor = parameters
        .get(b"Predictor".as_slice())
        .and_then(PdfObject::as_usize)
        .unwrap_or(1);
    if predictor <= 1 {
        return Ok(data);
    }
    if !(10..=15).contains(&predictor) {
        return Err(PdfError::new("unsupported PDF stream predictor"));
    }
    let columns = parameters
        .get(b"Columns".as_slice())
        .and_then(PdfObject::as_usize)
        .unwrap_or(1);
    let colors = parameters
        .get(b"Colors".as_slice())
        .and_then(PdfObject::as_usize)
        .unwrap_or(1);
    let bits = parameters
        .get(b"BitsPerComponent".as_slice())
        .and_then(PdfObject::as_usize)
        .unwrap_or(8);
    if bits != 8 {
        return Err(PdfError::new("unsupported PDF predictor bit depth"));
    }
    let bytes_per_pixel = colors.max(1);
    let row_bytes = columns
        .checked_mul(colors)
        .ok_or_else(|| PdfError::new("PDF predictor row size overflow"))?;
    let stride = row_bytes + 1;
    if stride == 0 || !data.len().is_multiple_of(stride) {
        return Err(PdfError::new("PDF predictor data is truncated"));
    }
    let mut output = Vec::with_capacity(data.len() / stride * row_bytes);
    let mut previous = vec![0u8; row_bytes];
    for encoded in data.chunks_exact(stride) {
        let filter = encoded[0];
        let mut row = encoded[1..].to_vec();
        for index in 0..row_bytes {
            let left = if index >= bytes_per_pixel {
                row[index - bytes_per_pixel]
            } else {
                0
            };
            let up = previous[index];
            let upper_left = if index >= bytes_per_pixel {
                previous[index - bytes_per_pixel]
            } else {
                0
            };
            row[index] = match filter {
                0 => row[index],
                1 => row[index].wrapping_add(left),
                2 => row[index].wrapping_add(up),
                3 => row[index].wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
                4 => row[index].wrapping_add(paeth(left, up, upper_left)),
                _ => return Err(PdfError::new("PDF predictor filter is invalid")),
            };
        }
        output.extend_from_slice(&row);
        previous = row;
    }
    Ok(output)
}
