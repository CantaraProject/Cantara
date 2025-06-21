//! This module contains structures for building CSS rules which can be used to build a CSS string.

use crate::logic::settings::{CssSize, FontRepresentation, HorizontalAlign};
use rgb::{RGB8, RGBA8};
use std::fmt::Display;
use serde::{Deserialize, Serialize};

/// A handler representing a CSS declaration of an object
#[derive(Debug, Clone, PartialEq)]
pub struct CssHandler {
    declarations: Vec<CssEntry>,
    important: bool,
}

impl CssHandler {
    pub fn new() -> Self {
        CssHandler {
            declarations: vec![],
            important: false,
        }
    }

    fn push(&mut self, key: String, value: CssValue) {
        self.declarations.push(CssEntry {
            key,
            value,
            important_flag: self.important,
        })
    }

    /// Extends the CssHandler with the CSS Value from another one.
    pub fn extend(&mut self, other: &CssHandler) {
        for entry in other.declarations.clone() {
            self.push(entry.key, entry.value);
        }
    }

    /// Sets the important flag of the handler.
    /// If set to true, any following CSS entries will have the important flag
    pub fn set_important(&mut self, important: bool) {
        self.important = important;
    }

    pub fn background_color(&mut self, color: RGB8) {
        self.push("background-color".to_string(), CssValue::Rgb(color))
    }

    pub fn padding_left(&mut self, size: CssSize) {
        self.push("padding-left".to_string(), CssValue::CssSize(size))
    }

    pub fn padding_right(&mut self, size: CssSize) {
        self.push("padding-right".to_string(), CssValue::CssSize(size))
    }

    pub fn padding_top(&mut self, size: CssSize) {
        self.push("padding-top".to_string(), CssValue::CssSize(size))
    }

    pub fn padding_bottom(&mut self, size: CssSize) {
        self.push("padding-bottom".to_string(), CssValue::CssSize(size))
    }

    pub fn color(&mut self, color: RGBA8) {
        self.push("color".to_string(), CssValue::Rgba(color))
    }

    pub fn font_size(&mut self, size: CssSize) {
        self.push("font-size".to_string(), CssValue::CssSize(size))
    }

    pub fn text_align(&mut self, align: HorizontalAlign) {
        self.push("text-align".to_string(), CssValue::HorizontalAlign(align))
    }

    pub fn background_image(&mut self, url: &str) {
        self.push(
            "background-image".to_string(),
            CssValue::Url(url.to_string()),
        )
    }

    pub fn background_size(&mut self, content: &str) {
        self.push(
            "background-size".to_string(),
            CssValue::String(content.to_string()),
        )
    }

    pub fn background_position(&mut self, content: &str) {
        self.push(
            "background-position".to_string(),
            CssValue::String(content.to_string()),
        )
    }

    pub fn background_repeat(&mut self, content: &str) {
        self.push(
            "background-repeat".to_string(),
            CssValue::String(content.to_string()),
        )
    }

    pub fn opacity(&mut self, opacity: f32) {
        let opacity = opacity.clamp(0.0, 1.0);

        self.push("opacity".to_string(), CssValue::Float(opacity))
    }

    pub fn z_index(&mut self, index: i32) {
        self.push("z-index".to_string(), CssValue::Int(index))
    }

    pub fn place_items(&mut self, place_items: PlaceItems) {
        self.push("place-items".to_string(), CssValue::PlaceItems(place_items))
    }

    pub fn font_family(&mut self, font_family: CssFontFamily) {
        self.push("font-family".to_string(), CssValue::FontFamily(font_family))
    }

    pub fn line_height(&mut self, line_height: f32) {
        self.push("line-height".to_string(), CssValue::Float(line_height))
    }
}

impl Display for CssHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for entry in &self.declarations {
            write!(f, "{}", entry)?;
        }
        Ok(())
    }
}

/// Represents a single CssEntry
#[derive(Debug, Clone, PartialEq)]
pub struct CssEntry {
    pub key: String,
    pub value: CssValue,
    pub important_flag: bool,
}

impl Display for CssEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}{};",
            self.key,
            self.value,
            match self.important_flag {
                true => "!important",
                false => "",
            }
        )
    }
}

/// Any Css Value
#[derive(Debug, Clone, PartialEq)]
pub enum CssValue {
    String(String),
    Rgb(RGB8),
    Rgba(RGBA8),
    Url(String),
    Int(i32),
    Float(f32),
    CssSize(CssSize),
    HorizontalAlign(HorizontalAlign),
    PlaceItems(PlaceItems),
    FontFamily(CssFontFamily)
}

impl Display for CssValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CssValue::String(s) => write!(f, "{}", s),
            CssValue::Rgb(rgb) => write!(f, "rgb({}, {}, {})", rgb.r, rgb.g, rgb.b),
            CssValue::Rgba(rgba) => {
                write!(f, "rgba({}, {}, {}, {})", rgba.r, rgba.g, rgba.b, rgba.a)
            }
            CssValue::Url(s) => write!(f, "url('{}')", s),
            CssValue::Int(i) => write!(f, "{}", i),
            CssValue::Float(float) => write!(f, "{}", float),
            CssValue::CssSize(css_size) => write!(f, "{}", css_size.to_css_string()),
            CssValue::HorizontalAlign(align) => write!(f, "{}", align.to_css_string()),
            CssValue::PlaceItems(place_items) => write!(f, "{}", place_items),
            CssValue::FontFamily(font_family) => write!(f, "{}", font_family.to_css_string()),
        }
    }
}

impl From<FontRepresentation> for CssHandler {
    fn from(font: FontRepresentation) -> CssHandler {
        let mut css_handler = CssHandler::new();

        css_handler.font_family(font.font_family.unwrap_or_default());
        css_handler.font_size(font.font_size);
        css_handler.line_height(font.line_height as f32);
        css_handler.color(font.color);
        css_handler.text_align(font.horizontal_alignment);

        css_handler
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaceItems {
    StartStretch,
    CenterStretch,
    EndStretch,
}

impl Display for PlaceItems {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PlaceItems::StartStretch => "start stretch",
                PlaceItems::CenterStretch => "center stretch",
                PlaceItems::EndStretch => "end stretch",
            }
        )
    }
}

/// A trait which allows the conversion of an object to a CSS string
pub trait CssString {
    fn to_css_string(&self) -> String;
}

/// An item representing a CSS font family entry
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CssFontFamily {
    pub family: Option<String>,
    pub genereric_family: GenericFontFamily,
}

impl Default for CssFontFamily {
    fn default() -> Self {
        CssFontFamily::without_family()
    }
}

impl CssFontFamily {

    /// Create the [CssFontFamily] with the builder pattern
    pub fn with_family(family: String) -> Self {
        Self {
            family: Some(family),
            genereric_family: GenericFontFamily::default()
        }
    }

    /// Create the [CssFontFamily] with the builder pattern
    pub fn without_family() -> Self {
        CssFontFamily {
            family: None,
            genereric_family: GenericFontFamily::SansSerif
        }
    }

    pub fn generic_family(self) -> Self {
        CssFontFamily {
            family: self.family,
            genereric_family: self.genereric_family,
        }
    }
}

impl CssString for CssFontFamily {
    fn to_css_string(&self) -> String {
        match &self.family {
            Some(family_name) => format!("{}, {}", family_name, self.genereric_family.to_css_string()),
            None => self.genereric_family.to_css_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default, Debug)]
pub enum GenericFontFamily {
    Serif,
    #[default] SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
    Inherit
}

impl CssString for GenericFontFamily {
    fn to_css_string(&self) -> String {
        match self {
            GenericFontFamily::Serif => "serif".to_string(),
            GenericFontFamily::SansSerif => "sans-serif".to_string(),
            GenericFontFamily::Monospace => "monospace".to_string(),
            GenericFontFamily::Cursive => "cursive".to_string(),
            GenericFontFamily::Fantasy => "fantasy".to_string(),
            GenericFontFamily::SystemUi => "system-ui".to_string(),
            GenericFontFamily::Inherit => "inherit".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_css_handler() {
        let mut handler = CssHandler::new();
        handler.background_color(RGB8::new(100, 100, 100));
        handler.set_important(true);
        handler.color(RGBA8::new(255, 255, 255, 255));
        handler.set_important(false);
        handler.padding_left(CssSize::Px(20.0));
        handler.padding_right(CssSize::Px(20.0));
        handler.padding_top(CssSize::Px(20.0));
        handler.padding_bottom(CssSize::Px(20.0));

        assert_eq!(
            handler.to_string().as_str(),
            "background-color:rgb(100, 100, 100);color:rgba(255, 255, 255, 255)!important;padding-left:20px;padding-right:20px;padding-top:20px;padding-bottom:20px;"
        );
    }

    #[test]
    fn test_empty_handler_css() {
        let handler = CssHandler::new();
        assert_eq!(handler.to_string().as_str(), "");
    }
}
