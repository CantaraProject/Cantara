//! This module contains structures for building CSS rules which can be used to build a CSS string.

use crate::logic::settings::{CssSize, FontRepresentation, HorizontalAlign};
use rgb::{RGB8, RGBA8};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

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

    pub fn background_image_none(&mut self) {
        self.push(
            "background-image".to_string(),
            CssValue::String("none".to_string()),
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

    pub fn margin_top(&mut self, size: CssSize) {
        self.push("margin-top".to_string(), CssValue::CssSize(size))
    }

    pub fn min_height(&mut self, min_height: CssSize) {
        self.push("min-height".to_string(), CssValue::CssSize(min_height))
    }

    pub fn hyphens(&mut self, value: &str) {
        self.push(
            "hyphens".to_string(),
            CssValue::String(value.to_string()),
        )
    }

    /// How heavy the type is drawn, as a CSS font weight.
    pub fn font_weight(&mut self, weight: u16) {
        self.push(
            "font-weight".to_string(),
            CssValue::String(weight.to_string()),
        )
    }

    /// An outline around the glyphs.
    ///
    /// `paint-order` goes with it: without it the stroke is drawn on top of the
    /// fill and eats into the letterforms from the inside, which thins the text
    /// instead of outlining it.
    pub fn text_stroke(&mut self, width: f64, color: RGBA8) {
        self.push(
            "-webkit-text-stroke".to_string(),
            CssValue::String(format!(
                "{}px rgba({}, {}, {}, {})",
                width,
                color.r,
                color.g,
                color.b,
                color.a as f32 / 255.0
            )),
        );
        self.push(
            "paint-order".to_string(),
            CssValue::String("stroke fill".to_string()),
        );
    }

    /// A drop shadow behind the text.
    pub fn text_shadow(&mut self, offset_x: f64, offset_y: f64, blur: f64, color: RGBA8) {
        self.push(
            "text-shadow".to_string(),
            CssValue::String(format!(
                "{}px {}px {}px rgba({}, {}, {}, {})",
                offset_x,
                offset_y,
                blur,
                color.r,
                color.g,
                color.b,
                color.a as f32 / 255.0
            )),
        );
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
    FontFamily(CssFontFamily),
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

impl CssHandler {
    /// The declarations of a font, optionally without its colour.
    ///
    /// Leaving the colour out lets the text follow whatever surrounds it. That
    /// matters outside a presentation: a slide font is normally white, which is
    /// invisible on an ordinary page.
    pub fn from_font(font: FontRepresentation, include_color: bool) -> CssHandler {
        let mut css_handler = CssHandler::from(font.clone());
        if !include_color {
            css_handler.declarations.retain(|entry| entry.key != "color");
        }
        css_handler
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
        css_handler.font_weight(font.weight);
        if font.horizontal_alignment == HorizontalAlign::JustifyWithHyphenation {
            css_handler.hyphens("auto");
        }

        if let Some(outline) = font.outline {
            css_handler.text_stroke(outline.width, outline.color);
        }

        // The flag used to be carried in the settings without ever reaching the
        // stylesheet, so turning the shadow on did nothing.
        if font.shadow {
            let shadow = font.shadow_style;
            css_handler.text_shadow(shadow.offset_x, shadow.offset_y, shadow.blur, shadow.color);
        }

        css_handler
    }
}

/// The `place-items` values the presentation uses.
///
/// The shared `Stretch` is not a naming accident: these are the CSS values
/// themselves, and the second word of `place-items` is `stretch` in each of
/// them.
#[allow(clippy::enum_variant_names)]
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
            genereric_family: GenericFontFamily::default(),
        }
    }

    /// Create the [CssFontFamily] with the builder pattern
    pub fn without_family() -> Self {
        CssFontFamily {
            family: None,
            genereric_family: GenericFontFamily::SansSerif,
        }
    }

}

impl CssString for CssFontFamily {
    fn to_css_string(&self) -> String {
        match &self.family {
            Some(family_name) => {
                format!("{}, {}", family_name, self.genereric_family.to_css_string())
            }
            None => self.genereric_family.to_css_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default, Debug)]
pub enum GenericFontFamily {
    Serif,
    #[default]
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
    Inherit,
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

#[cfg(test)]
mod font_color_tests {
    use super::*;
    use crate::logic::settings::FontRepresentation;

    /// A presentation font is white, because slides are dark. Drawn on an
    /// ordinary page that is invisible, so the detail view asks for the colour
    /// to be left out and lets `currentColor` decide.
    #[test]
    fn test_the_colour_can_be_left_out() {
        let font = FontRepresentation::default();

        let with_color = CssHandler::from_font(font.clone(), true).to_string();
        let without_color = CssHandler::from_font(font, false).to_string();

        assert!(with_color.contains("color:"), "got {with_color:?}");
        assert!(
            !without_color.contains("color:"),
            "the colour still forces itself on the page: {without_color:?}"
        );
    }

    /// Only the colour goes; everything else about the font has to survive.
    #[test]
    fn test_nothing_else_is_lost() {
        let font = FontRepresentation {
            font_size: CssSize::Pt(19.0),
            weight: 700,
            ..FontRepresentation::default()
        };

        let style = CssHandler::from_font(font, false).to_string();

        assert!(style.contains("font-size"), "got {style:?}");
        assert!(style.contains("font-weight"), "got {style:?}");
        assert!(style.contains("line-height"), "got {style:?}");
    }

    /// Transparent would have been the obvious shortcut and the wrong one: the
    /// staff is drawn in `currentColor`, so a transparent colour hides it.
    #[test]
    fn test_leaving_it_out_is_not_the_same_as_transparent() {
        let font = FontRepresentation::default();

        let omitted = CssHandler::from_font(font.clone(), false).to_string();

        assert!(!omitted.contains("rgba"), "got {omitted:?}");
        assert!(!omitted.contains("transparent"), "got {omitted:?}");
    }
}
