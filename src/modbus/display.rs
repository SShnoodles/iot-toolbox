#[derive(PartialEq, Clone, Copy, Debug)]
pub enum DisplayFormat {
    Signed,
    Unsigned,
    Hex,
    Binary,
    Long,
    LongInverse,
    Float,
    FloatInverse,
    Double,
    DoubleInverse,
}

impl DisplayFormat {
    pub fn label(&self) -> &'static str {
        match self {
            DisplayFormat::Signed => "Signed",
            DisplayFormat::Unsigned => "Unsigned",
            DisplayFormat::Hex => "Hex",
            DisplayFormat::Binary => "Binary",
            DisplayFormat::Long => "Long",
            DisplayFormat::LongInverse => "Long Inverse",
            DisplayFormat::Float => "Float",
            DisplayFormat::FloatInverse => "Float Inverse",
            DisplayFormat::Double => "Double",
            DisplayFormat::DoubleInverse => "Double Inverse",
        }
    }

    pub const ALL: [DisplayFormat; 10] = [
        DisplayFormat::Signed,
        DisplayFormat::Unsigned,
        DisplayFormat::Hex,
        DisplayFormat::Binary,
        DisplayFormat::Long,
        DisplayFormat::LongInverse,
        DisplayFormat::Float,
        DisplayFormat::FloatInverse,
        DisplayFormat::Double,
        DisplayFormat::DoubleInverse,
    ];

    pub fn register_count(&self) -> usize {
        match self {
            DisplayFormat::Signed
            | DisplayFormat::Unsigned
            | DisplayFormat::Hex
            | DisplayFormat::Binary => 1,

            DisplayFormat::Long
            | DisplayFormat::LongInverse
            | DisplayFormat::Float
            | DisplayFormat::FloatInverse => 2,

            DisplayFormat::Double | DisplayFormat::DoubleInverse => 4,
        }
    }

    pub fn format(&self, raw: &[u16]) -> String {
        match self {
            DisplayFormat::Signed => raw
                .get(0)
                .map(|v| (*v as i16).to_string())
                .unwrap_or("-".into()),
            DisplayFormat::Unsigned => raw.get(0).map(|v| v.to_string()).unwrap_or("-".into()),
            DisplayFormat::Hex => raw
                .get(0)
                .map(|v| format!("0x{:04X}", v))
                .unwrap_or("-".into()),
            DisplayFormat::Binary => raw
                .get(0)
                .map(|v| format!("{:016b}", v))
                .unwrap_or("-".into()),
            DisplayFormat::Long => {
                if raw.len() >= 2 {
                    let v = ((raw[0] as u32) << 16) | raw[1] as u32;
                    (v as i32).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::LongInverse => {
                if raw.len() >= 2 {
                    let v = ((raw[1] as u32) << 16) | raw[0] as u32;
                    (v as i32).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::Float => {
                if raw.len() >= 2 {
                    let bits = ((raw[0] as u32) << 16) | raw[1] as u32;
                    f32::from_bits(bits).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::FloatInverse => {
                if raw.len() >= 2 {
                    let bits = ((raw[1] as u32) << 16) | raw[0] as u32;
                    f32::from_bits(bits).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::Double => {
                if raw.len() >= 4 {
                    let bits = ((raw[0] as u64) << 48)
                        | ((raw[1] as u64) << 32)
                        | ((raw[2] as u64) << 16)
                        | (raw[3] as u64);
                    f64::from_bits(bits).to_string()
                } else {
                    "-".into()
                }
            }
            DisplayFormat::DoubleInverse => {
                if raw.len() >= 4 {
                    let bits = ((raw[3] as u64) << 48)
                        | ((raw[2] as u64) << 32)
                        | ((raw[1] as u64) << 16)
                        | (raw[0] as u64);
                    f64::from_bits(bits).to_string()
                } else {
                    "-".into()
                }
            }
        }
    }
}
