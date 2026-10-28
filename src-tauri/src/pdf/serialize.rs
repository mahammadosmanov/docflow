use super::*;

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
