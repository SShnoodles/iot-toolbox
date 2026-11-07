pub fn parse_hex_string(input: &str) -> Result<Vec<u8>, String> {
    let mut cleaned = String::with_capacity(input.len());
    for character in input.chars() {
        if character.is_ascii_hexdigit() {
            cleaned.push(character);
        } else if !character.is_whitespace() && !matches!(character, ',' | ':' | '-' | ';') {
            return Err(format!("Invalid HEX character: {character:?}"));
        }
    }
    if cleaned.is_empty() {
        return Err("No HEX bytes to send".into());
    }

    if cleaned.len() % 2 != 0 {
        return Err("HEX length must be even".into());
    }

    let mut bytes = Vec::new();
    for i in (0..cleaned.len()).step_by(2) {
        let byte = u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|_| "Invalid HEX")?;
        bytes.push(byte);
    }

    Ok(bytes)
}

pub fn bytes_to_hex_string(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn now_timestamp() -> String {
    use chrono::Local;

    Local::now().format("%H:%M:%S%.3f").to_string()
}

#[cfg(test)]
mod tests {
    use super::parse_hex_string;

    #[test]
    fn rejects_malformed_hex_without_sending_partial_bytes() {
        assert_eq!(parse_hex_string("a5:00, ff").unwrap(), [0xa5, 0, 0xff]);
        assert!(parse_hex_string("A5 GG").is_err());
        assert!(parse_hex_string("A5 0").is_err());
        assert!(parse_hex_string(" , ").is_err());
    }
}
