pub fn hex_to_bytes(hex_str: &str) -> Vec<u8> {
    let cleaned = hex_str.replace(" ", "");
    assert!(
        cleaned.len() % 2 == 0,
        "Length of the input string in characters must be even"
    );

    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).unwrap())
        .collect()
}

pub fn bytes_to_hex_string(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<String>>()
        .join(" ")
}