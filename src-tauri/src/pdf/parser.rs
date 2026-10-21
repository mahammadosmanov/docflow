use super::*;

pub(super) fn contains_signature_marker(
    object: &PdfObject,
    depth: usize,
) -> Result<bool, PdfError> {
    if depth > MAX_DEPTH {
        return Err(PdfError::new("PDF signature object is too deeply nested"));
    }
    Ok(match object {
        PdfObject::Dictionary(dictionary) | PdfObject::Stream { dictionary, .. } => {
            dictionary.contains_key(b"ByteRange".as_slice())
                || dictionary
                    .get(b"Type".as_slice())
                    .and_then(PdfObject::as_name)
                    == Some(b"Sig")
                || dictionary
                    .get(b"FT".as_slice())
                    .and_then(PdfObject::as_name)
                    == Some(b"Sig")
                || dictionary
                    .values()
                    .any(|value| contains_signature_marker(value, depth + 1).unwrap_or(true))
        }
        PdfObject::Array(values) => values
            .iter()
            .any(|value| contains_signature_marker(value, depth + 1).unwrap_or(true)),
        _ => false,
    })
}

pub(super) struct XrefSection {
    pub(super) entries: BTreeMap<u32, XrefEntry>,
    pub(super) defined: BTreeSet<u32>,
    pub(super) trailer: Dictionary,
}

pub(super) fn parse_xref_section(bytes: &[u8], offset: usize) -> Result<XrefSection, PdfError> {
    let mut parser = Parser::at(bytes, offset)?;
    parser.skip_space_and_comments();
    if parser.consume_keyword(b"xref") {
        parse_classic_xref(parser)
    } else {
        let indirect = parse_indirect_object(bytes, offset, |_| {
            Err(PdfError::new(
                "xref stream entries, including /Length, must be direct objects",
            ))
        })?;
        let PdfObject::Stream { dictionary, data } = indirect.object else {
            return Err(PdfError::new("startxref does not point to xref data"));
        };
        if dictionary
            .get(b"Type".as_slice())
            .and_then(PdfObject::as_name)
            != Some(b"XRef")
        {
            return Err(PdfError::new("startxref stream is not /XRef"));
        }
        let widths = integer_array(
            dictionary
                .get(b"W".as_slice())
                .ok_or_else(|| PdfError::new("xref stream has no /W"))?,
        )?;
        if widths.len() != 3 || widths.iter().sum::<usize>() == 0 {
            return Err(PdfError::new("xref stream /W is invalid"));
        }
        let size = dictionary
            .get(b"Size".as_slice())
            .and_then(PdfObject::as_usize)
            .ok_or_else(|| PdfError::new("xref stream has no valid /Size"))?;
        let indexes = match dictionary.get(b"Index".as_slice()) {
            Some(value) => integer_array(value)?,
            None => vec![0, size],
        };
        if !indexes.len().is_multiple_of(2) {
            return Err(PdfError::new("xref stream /Index is invalid"));
        }
        let decoded = decode_stream(&dictionary, &data)?;
        let entry_width = widths.iter().sum::<usize>();
        let expected = indexes
            .as_chunks::<2>()
            .0
            .iter()
            .try_fold(0usize, |total, pair| total.checked_add(pair[1]))
            .and_then(|count| count.checked_mul(entry_width))
            .ok_or_else(|| PdfError::new("xref stream size overflow"))?;
        if decoded.len() < expected {
            return Err(PdfError::new("xref stream data is truncated"));
        }
        let mut entries = BTreeMap::new();
        let mut defined = BTreeSet::new();
        let mut cursor = 0;
        for pair in indexes.as_chunks::<2>().0 {
            for number in pair[0]..pair[0] + pair[1] {
                let kind = if widths[0] == 0 {
                    1
                } else {
                    read_be(&decoded[cursor..cursor + widths[0]])
                };
                cursor += widths[0];
                let first = read_be(&decoded[cursor..cursor + widths[1]]);
                cursor += widths[1];
                let second = read_be(&decoded[cursor..cursor + widths[2]]);
                cursor += widths[2];
                let number = u32::try_from(number)
                    .map_err(|_| PdfError::new("xref object number overflow"))?;
                defined.insert(number);
                match kind {
                    1 => {
                        entries.insert(
                            number,
                            XrefEntry::Offset {
                                offset: first,
                                generation: u16::try_from(second)
                                    .map_err(|_| PdfError::new("xref generation overflow"))?,
                            },
                        );
                    }
                    2 => {
                        entries.insert(
                            number,
                            XrefEntry::Compressed {
                                stream: u32::try_from(first)
                                    .map_err(|_| PdfError::new("object stream number overflow"))?,
                                index: second,
                            },
                        );
                    }
                    0 => {}
                    _ => return Err(PdfError::new("xref stream entry type is invalid")),
                }
            }
        }
        Ok(XrefSection {
            entries,
            defined,
            trailer: dictionary,
        })
    }
}

pub(super) fn parse_classic_xref(mut parser: Parser<'_>) -> Result<XrefSection, PdfError> {
    let mut entries = BTreeMap::new();
    let mut defined = BTreeSet::new();
    loop {
        parser.skip_space_and_comments();
        if parser.consume_keyword(b"trailer") {
            let trailer = parser.parse_object(0)?;
            let PdfObject::Dictionary(trailer) = trailer else {
                return Err(PdfError::new("classic xref trailer is not a dictionary"));
            };
            if trailer.contains_key(b"XRefStm".as_slice()) {
                return Err(PdfError::new(
                    "hybrid-reference PDF cross-reference sections are unsupported",
                ));
            }
            return Ok(XrefSection {
                entries,
                defined,
                trailer,
            });
        }
        let first = parser.read_unsigned()?;
        let count = parser.read_unsigned()?;
        for index in 0..count {
            let offset = parser.read_unsigned()?;
            let generation = parser.read_unsigned()?;
            let status = parser.read_word()?;
            let number = u32::try_from(first + index)
                .map_err(|_| PdfError::new("xref object number overflow"))?;
            defined.insert(number);
            if status == b"n" {
                entries.insert(
                    number,
                    XrefEntry::Offset {
                        offset,
                        generation: u16::try_from(generation)
                            .map_err(|_| PdfError::new("xref generation overflow"))?,
                    },
                );
            } else if status != b"f" {
                return Err(PdfError::new("classic xref entry status is invalid"));
            }
        }
    }
}

pub(super) struct IndirectObject {
    pub(super) id: ObjectId,
    pub(super) object: PdfObject,
}

pub(super) fn parse_indirect_object<F>(
    bytes: &[u8],
    offset: usize,
    mut resolve_length: F,
) -> Result<IndirectObject, PdfError>
where
    F: FnMut(ObjectId) -> Result<usize, PdfError>,
{
    let mut parser = Parser::at(bytes, offset)?;
    let number = u32::try_from(parser.read_unsigned()?)
        .map_err(|_| PdfError::new("PDF object number overflow"))?;
    let generation = u16::try_from(parser.read_unsigned()?)
        .map_err(|_| PdfError::new("PDF object generation overflow"))?;
    if !parser.consume_keyword(b"obj") {
        return Err(PdfError::new(
            "xref offset does not point to an indirect object",
        ));
    }
    let object = match parser.parse_object(0)? {
        PdfObject::Dictionary(dictionary) => {
            parser.skip_space_and_comments();
            if parser.consume_keyword(b"stream") {
                parser.consume_stream_eol()?;
                let length_object = dictionary
                    .get(b"Length".as_slice())
                    .ok_or_else(|| PdfError::new("PDF stream has no /Length"))?;
                let length = if let Some(length) = length_object.as_usize() {
                    length
                } else if let Some(reference) = length_object.as_reference() {
                    resolve_length(reference)?
                } else {
                    return Err(PdfError::new(
                        "PDF stream /Length is not a non-negative integer or reference",
                    ));
                };
                let end = parser
                    .position
                    .checked_add(length)
                    .filter(|end| *end <= bytes.len())
                    .ok_or_else(|| PdfError::new("PDF stream length exceeds the file"))?;
                let data = bytes[parser.position..end].to_vec();
                parser.position = end;
                parser.skip_space_and_comments();
                if !parser.consume_keyword(b"endstream") {
                    return Err(PdfError::new("PDF stream is missing endstream"));
                }
                PdfObject::Stream { dictionary, data }
            } else {
                PdfObject::Dictionary(dictionary)
            }
        }
        object => object,
    };
    parser.skip_space_and_comments();
    if !parser.consume_keyword(b"endobj") {
        return Err(PdfError::new("PDF indirect object is missing endobj"));
    }
    Ok(IndirectObject {
        id: (number, generation),
        object,
    })
}

pub(super) struct Parser<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) position: usize,
}

impl<'a> Parser<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(super) fn at(bytes: &'a [u8], position: usize) -> Result<Self, PdfError> {
        if position > bytes.len() {
            return Err(PdfError::new("PDF parser offset exceeds the file"));
        }
        Ok(Self { bytes, position })
    }

    pub(super) fn parse_object(&mut self, depth: usize) -> Result<PdfObject, PdfError> {
        if depth > MAX_DEPTH {
            return Err(PdfError::new("PDF object nesting is too deep"));
        }
        self.skip_space_and_comments();
        let Some(byte) = self.peek() else {
            return Err(PdfError::new("unexpected end of PDF object"));
        };
        match byte {
            b'/' => self.parse_name().map(PdfObject::Name),
            b'(' => self.parse_literal_string().map(PdfObject::String),
            b'[' => self.parse_array(depth + 1),
            b'<' if self.bytes.get(self.position + 1) == Some(&b'<') => {
                self.parse_dictionary(depth + 1)
            }
            b'<' => self.parse_hex_string().map(PdfObject::String),
            b'n' if self.consume_keyword(b"null") => Ok(PdfObject::Null),
            b't' if self.consume_keyword(b"true") => Ok(PdfObject::Bool(true)),
            b'f' if self.consume_keyword(b"false") => Ok(PdfObject::Bool(false)),
            b'+' | b'-' | b'.' | b'0'..=b'9' => self.parse_number_or_reference(),
            _ => Err(PdfError::new("unsupported or malformed PDF object token")),
        }
    }

    pub(super) fn parse_number_or_reference(&mut self) -> Result<PdfObject, PdfError> {
        let first = self.read_number_token()?;
        let saved = self.position;
        if !first.contains(['.', '+', '-']) {
            self.skip_space_and_comments();
            if self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                let second = self.read_number_token()?;
                self.skip_space_and_comments();
                if self.consume_keyword(b"R") {
                    return Ok(PdfObject::Reference((
                        first
                            .parse()
                            .map_err(|_| PdfError::new("PDF reference number is invalid"))?,
                        second
                            .parse()
                            .map_err(|_| PdfError::new("PDF reference generation is invalid"))?,
                    )));
                }
            }
        }
        self.position = saved;
        Ok(PdfObject::Number(first))
    }

    pub(super) fn parse_array(&mut self, depth: usize) -> Result<PdfObject, PdfError> {
        self.position += 1;
        let mut values = Vec::new();
        loop {
            self.skip_space_and_comments();
            if self.peek() == Some(b']') {
                self.position += 1;
                return Ok(PdfObject::Array(values));
            }
            values.push(self.parse_object(depth)?);
            if values.len() > MAX_OBJECTS {
                return Err(PdfError::new("PDF array is too large"));
            }
        }
    }

    pub(super) fn parse_dictionary(&mut self, depth: usize) -> Result<PdfObject, PdfError> {
        self.position += 2;
        let mut values = Dictionary::new();
        loop {
            self.skip_space_and_comments();
            if self.bytes.get(self.position..self.position + 2) == Some(b">>") {
                self.position += 2;
                return Ok(PdfObject::Dictionary(values));
            }
            let key = self.parse_name()?;
            let value = self.parse_object(depth)?;
            values.insert(key, value);
            if values.len() > MAX_OBJECTS {
                return Err(PdfError::new("PDF dictionary is too large"));
            }
        }
    }

    pub(super) fn parse_name(&mut self) -> Result<Vec<u8>, PdfError> {
        if self.peek() != Some(b'/') {
            return Err(PdfError::new("PDF dictionary key is not a name"));
        }
        self.position += 1;
        let mut value = Vec::new();
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) || is_delimiter(byte) {
                break;
            }
            if byte == b'#' {
                let hex = self
                    .bytes
                    .get(self.position + 1..self.position + 3)
                    .ok_or_else(|| PdfError::new("PDF name escape is truncated"))?;
                value.push(hex_byte(hex)?);
                self.position += 3;
            } else {
                value.push(byte);
                self.position += 1;
            }
        }
        Ok(value)
    }

    pub(super) fn parse_literal_string(&mut self) -> Result<Vec<u8>, PdfError> {
        self.position += 1;
        let mut depth = 1usize;
        let mut value = Vec::new();
        while let Some(byte) = self.next() {
            match byte {
                b'(' => {
                    depth += 1;
                    value.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(value);
                    }
                    value.push(byte);
                }
                b'\\' => {
                    let Some(escaped) = self.next() else {
                        return Err(PdfError::new("PDF literal string escape is truncated"));
                    };
                    match escaped {
                        b'n' => value.push(b'\n'),
                        b'r' => value.push(b'\r'),
                        b't' => value.push(b'\t'),
                        b'b' => value.push(8),
                        b'f' => value.push(12),
                        b'(' | b')' | b'\\' => value.push(escaped),
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.position += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut octal = u16::from(escaped - b'0');
                            for _ in 0..2 {
                                if let Some(next @ b'0'..=b'7') = self.peek() {
                                    self.position += 1;
                                    octal = octal * 8 + u16::from(next - b'0');
                                } else {
                                    break;
                                }
                            }
                            value.push(octal as u8);
                        }
                        other => value.push(other),
                    }
                }
                other => value.push(other),
            }
        }
        Err(PdfError::new("PDF literal string is unterminated"))
    }

    pub(super) fn parse_hex_string(&mut self) -> Result<Vec<u8>, PdfError> {
        self.position += 1;
        let mut digits = Vec::new();
        loop {
            let byte = self
                .next()
                .ok_or_else(|| PdfError::new("PDF hex string is unterminated"))?;
            if byte == b'>' {
                break;
            }
            if !is_whitespace(byte) {
                if !byte.is_ascii_hexdigit() {
                    return Err(PdfError::new("PDF hex string contains invalid data"));
                }
                digits.push(byte);
            }
        }
        if !digits.len().is_multiple_of(2) {
            digits.push(b'0');
        }
        digits
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| hex_byte(pair))
            .collect()
    }

    pub(super) fn read_unsigned(&mut self) -> Result<usize, PdfError> {
        self.skip_space_and_comments();
        let start = self.position;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        if start == self.position {
            return Err(PdfError::new("expected unsigned PDF integer"));
        }
        std::str::from_utf8(&self.bytes[start..self.position])
            .ok()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| PdfError::new("PDF unsigned integer is invalid"))
    }

    pub(super) fn read_number_token(&mut self) -> Result<String, PdfError> {
        self.skip_space_and_comments();
        let start = self.position;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.position += 1;
        }
        let mut digits = 0;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            digits += 1;
            self.position += 1;
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                digits += 1;
                self.position += 1;
            }
        }
        if digits == 0 {
            return Err(PdfError::new("PDF number is invalid"));
        }
        String::from_utf8(self.bytes[start..self.position].to_vec())
            .map_err(|_| PdfError::new("PDF number is not ASCII"))
    }

    pub(super) fn read_word(&mut self) -> Result<Vec<u8>, PdfError> {
        self.skip_space_and_comments();
        let start = self.position;
        while self
            .peek()
            .is_some_and(|byte| !is_whitespace(byte) && !is_delimiter(byte))
        {
            self.position += 1;
        }
        if start == self.position {
            return Err(PdfError::new("expected PDF keyword"));
        }
        Ok(self.bytes[start..self.position].to_vec())
    }

    pub(super) fn consume_keyword(&mut self, keyword: &[u8]) -> bool {
        self.skip_space_and_comments();
        if self.bytes.get(self.position..self.position + keyword.len()) != Some(keyword) {
            return false;
        }
        let end = self.position + keyword.len();
        if self
            .bytes
            .get(end)
            .is_some_and(|byte| !is_whitespace(*byte) && !is_delimiter(*byte))
        {
            return false;
        }
        self.position = end;
        true
    }

    pub(super) fn consume_stream_eol(&mut self) -> Result<(), PdfError> {
        match (self.next(), self.peek()) {
            (Some(b'\r'), Some(b'\n')) => {
                self.position += 1;
                Ok(())
            }
            (Some(b'\r' | b'\n'), _) => Ok(()),
            _ => Err(PdfError::new("PDF stream keyword has no line ending")),
        }
    }

    pub(super) fn skip_space_and_comments(&mut self) {
        loop {
            while self.peek().is_some_and(is_whitespace) {
                self.position += 1;
            }
            if self.peek() != Some(b'%') {
                break;
            }
            while self
                .peek()
                .is_some_and(|byte| !matches!(byte, b'\r' | b'\n'))
            {
                self.position += 1;
            }
        }
    }

    pub(super) fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    pub(super) fn next(&mut self) -> Option<u8> {
        let value = self.peek()?;
        self.position += 1;
        Some(value)
    }
}

pub(super) fn serialize_object(
    object: &PdfObject,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), PdfError> {
    if depth > MAX_DEPTH {
        return Err(PdfError::new(
            "PDF object is too deeply nested to serialize",
        ));
    }
    match object {
        PdfObject::Null => output.extend_from_slice(b"null"),
        PdfObject::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        PdfObject::Number(value) => output.extend_from_slice(value.as_bytes()),
        PdfObject::Name(value) => serialize_name(value, output),
        PdfObject::String(value) => {
            output.push(b'<');
            for byte in value {
                output.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            output.push(b'>');
        }
        PdfObject::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b' ');
                }
                serialize_object(value, output, depth + 1)?;
            }
            output.push(b']');
        }
        PdfObject::Dictionary(dictionary) => serialize_dictionary(dictionary, output, depth + 1)?,
        PdfObject::Reference((number, generation)) => {
            output.extend_from_slice(format!("{number} {generation} R").as_bytes());
        }
        PdfObject::Stream { dictionary, data } => {
            let mut dictionary = dictionary.clone();
            dictionary.insert(
                b"Length".to_vec(),
                PdfObject::Number(data.len().to_string()),
            );
            serialize_dictionary(&dictionary, output, depth + 1)?;
            output.extend_from_slice(b"\nstream\n");
            output.extend_from_slice(data);
            output.extend_from_slice(b"\nendstream");
        }
    }
    Ok(())
}

pub(super) fn serialize_dictionary(
    dictionary: &Dictionary,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), PdfError> {
    output.extend_from_slice(b"<<");
    for (key, value) in dictionary {
        output.push(b' ');
        serialize_name(key, output);
        output.push(b' ');
        serialize_object(value, output, depth + 1)?;
    }
    output.extend_from_slice(b" >>");
    Ok(())
}

pub(super) fn serialize_name(name: &[u8], output: &mut Vec<u8>) {
    output.push(b'/');
    for byte in name {
        if *byte < 33 || *byte > 126 || is_delimiter(*byte) || *byte == b'#' {
            output.extend_from_slice(format!("#{byte:02X}").as_bytes());
        } else {
            output.push(*byte);
        }
    }
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
