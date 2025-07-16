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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb8_to_hex() {
        let rgb = RGB8::new(255, 0, 128);
        assert_eq!(rgb.to_hex(), "#FF0080");

        let rgb = RGB8::new(0, 0, 0);
        assert_eq!(rgb.to_hex(), "#000000");

        let rgb = RGB8::new(255, 255, 255);
        assert_eq!(rgb.to_hex(), "#FFFFFF");
    }

    #[test]
    fn test_rgba8_to_hex() {
        let rgba = RGBA8::new(255, 0, 128, 255);
        assert_eq!(rgba.to_hex(), "#FF0080");

        let rgba = RGBA8::new(0, 0, 0, 128);
        assert_eq!(rgba.to_hex(), "#000000");
    }

    #[test]
    fn test_string_to_rgb8_valid() {
        let hex = "#FF0080".to_string();
        let rgb = hex.to_rgb8().unwrap();
        assert_eq!(rgb.r, 255);
        assert_eq!(rgb.g, 0);
        assert_eq!(rgb.b, 128);

        // Test without # prefix
        let hex = "FF0080".to_string();
        let rgb = hex.to_rgb8().unwrap();
        assert_eq!(rgb.r, 255);
        assert_eq!(rgb.g, 0);
        assert_eq!(rgb.b, 128);

        // Test lowercase
        let hex = "#ff0080".to_string();
        let rgb = hex.to_rgb8().unwrap();
        assert_eq!(rgb.r, 255);
        assert_eq!(rgb.g, 0);
        assert_eq!(rgb.b, 128);
    }

    #[test]
    fn test_string_to_rgb8_invalid() {
        // Invalid length
        let hex = "#FF00".to_string();
        assert!(hex.to_rgb8().is_none());

        // Invalid characters
        let hex = "#FF00ZZ".to_string();
        assert!(hex.to_rgb8().is_none());

        // Empty string
        let hex = "".to_string();
        assert!(hex.to_rgb8().is_none());
    }

    #[test]
    fn test_roundtrip_conversion() {
        // RGB8 -> hex -> RGB8
        let original = RGB8::new(255, 0, 128);
        let hex = original.to_hex();
        let converted = hex.to_rgb8().unwrap();
        assert_eq!(original, converted);

        // String -> RGB8 -> String
        let original_hex = "#00FF00".to_string();
        let rgb = original_hex.to_rgb8().unwrap();
        let converted_hex = rgb.to_hex();
        assert_eq!(original_hex.to_uppercase(), converted_hex);
    }
}
