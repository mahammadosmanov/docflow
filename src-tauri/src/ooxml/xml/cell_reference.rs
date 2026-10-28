use super::*;

pub(in crate::ooxml) fn cell_reference(row: u32, column: u32) -> String {
    format!("{}{}", column_name(column).unwrap_or_default(), row)
}

pub(in crate::ooxml) fn column_name(mut number: u32) -> Result<String, OoxmlError> {
    if number == 0 || number > 16_384 {
        return Err(OoxmlError::invalid("worksheet column is invalid"));
    }
    let mut output = String::new();
    while number > 0 {
        let remainder = (number - 1) % 26;
        output.insert(0, char::from(b'A' + remainder as u8));
        number = (number - 1) / 26;
    }
    Ok(output)
}

pub(in crate::ooxml) fn split_reference(reference: &str) -> Option<(&str, u32)> {
    let split = reference.find(|character: char| character.is_ascii_digit())?;
    let row = reference[split..].parse::<u32>().ok()?;
    Some((&reference[..split], row))
}

pub(in crate::ooxml) fn parse_reference(reference: &str) -> Option<(u32, u32)> {
    let (column, row) = split_reference(reference)?;
    let mut column_number = 0_u32;
    for byte in column.bytes() {
        if !byte.is_ascii_alphabetic() {
            return None;
        }
        column_number = column_number
            .checked_mul(26)?
            .checked_add(u32::from(byte.to_ascii_uppercase() - b'A' + 1))?;
    }
    (column_number > 0).then_some((row, column_number))
}
