pub fn parse_hex_string(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = input
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();

    if cleaned.len() % 2 != 0 {
        return Err("HEX length must be even".into());
    }

    let mut bytes = Vec::new();
    for i in (0..cleaned.len()).step_by(2) {
        let byte = u8::from_str_radix(&cleaned[i..i + 2], 16)
            .map_err(|_| "Invalid HEX")?;
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
