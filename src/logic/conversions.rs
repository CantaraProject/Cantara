use rgb::{RGB8, RGBA8};

/// Converts a Color to a Hex string
pub trait ToHexString {
    fn to_hex(&self) -> String;
}

impl ToHexString for RGB8 {
    fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl ToHexString for RGBA8 {
    fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

pub trait ToRgb8 {
    fn to_rgb8(&self) -> Option<RGB8>;
}

impl ToRgb8 for String {
    fn to_rgb8(&self) -> Option<RGB8> {
        let hex = self.trim_start_matches('#').to_uppercase();

        // Check if the string is exactly 6 characters long
        if hex.len() != 6 {
            return None;
        }

        // Verify all characters are valid hexadecimal digits
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }

        // Parse each pair of characters as a u8 value
        let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;

        Some(RGB8::new(red, green, blue))
    }
}
