//! A typed model of a PowerPoint deck, and the bridge to
//! [PptxGenJS](https://gitbrent.github.io/PptxGenJS/) that writes the file.
//!
//! # Why a wrapper
//!
//! Writing OOXML from Rust would mean re-implementing a large, fiddly format.
//! PptxGenJS already does it and runs in the same webview Cantara draws its
//! presentation in, so the file can be produced on the client with no server
//! and no extra native dependency.
//!
//! What is *not* delegated is the decision of what goes on a slide. That lives
//! here, in plain Rust data that can be unit-tested without a browser:
//!
//! ```text
//! Cantara slides + design  ──►  PptxDeck  ──►  JSON  ──►  PptxGenJS  ──►  .pptx
//!        (Rust)                  (Rust)                     (JS)
//! ```
//!
//! The JavaScript side is deliberately dumb: it walks the JSON and calls
//! PptxGenJS, without knowing anything about songs, slides or fonts. Every
//! decision worth testing is therefore testable here.
//!
//! # Units
//!
//! PptxGenJS positions everything in inches. A 16:9 deck is 13.333 × 7.5 in,
//! which is what [`PptxDeck::widescreen`] sets up. Font sizes are in points, as
//! in PowerPoint itself.

use serde::Serialize;

/// A colour as PptxGenJS wants it: six hex digits, no leading `#`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct PptxColor(String);

impl PptxColor {
    /// Build a colour from RGB components.
    pub fn rgb(red: u8, green: u8, blue: u8) -> PptxColor {
        PptxColor(format!("{red:02X}{green:02X}{blue:02X}"))
    }

    /// Parse a CSS colour of the form `#rgb`, `#rrggbb` or `rgb(r, g, b)`.
    ///
    /// Falls back to black for anything it cannot read, because a deck with an
    /// odd colour is still useful and a failed export is not.
    pub fn from_css(value: &str) -> PptxColor {
        let trimmed = value.trim();

        if let Some(hex) = trimmed.strip_prefix('#') {
            if hex.len() == 6
                && let Ok(parsed) = u32::from_str_radix(hex, 16)
            {
                return PptxColor::rgb(
                    (parsed >> 16) as u8,
                    (parsed >> 8) as u8,
                    parsed as u8,
                );
            }
            if hex.len() == 3 {
                let expand = |c: char| c.to_digit(16).unwrap_or(0) as u8 * 17;
                let mut chars = hex.chars();
                if let (Some(r), Some(g), Some(b)) =
                    (chars.next(), chars.next(), chars.next())
                {
                    return PptxColor::rgb(expand(r), expand(g), expand(b));
                }
            }
        }

        if let Some(inner) = trimmed
            .strip_prefix("rgb(")
            .or_else(|| trimmed.strip_prefix("rgba("))
            .and_then(|rest| rest.strip_suffix(')'))
        {
            let parts: Vec<u8> = inner
                .split(',')
                .take(3)
                .filter_map(|part| part.trim().parse::<f32>().ok())
                .map(|value| value.clamp(0.0, 255.0) as u8)
                .collect();
            if parts.len() == 3 {
                return PptxColor::rgb(parts[0], parts[1], parts[2]);
            }
        }

        PptxColor::rgb(0, 0, 0)
    }

    /// The six hex digits, as PptxGenJS expects them. The value reaches the
    /// browser through `Serialize`, so this is only read back in tests.
    #[cfg(test)]
    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

/// Horizontal alignment of a text box.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PptxAlign {
    Left,
    Center,
    Right,
}

/// Vertical alignment inside a text box.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PptxVAlign {
    /// Not used by the current layout, which centres every band.
    #[allow(dead_code)]
    Top,
    Middle,
    Bottom,
}

/// A rectangle on a slide, in inches from the top left corner.
#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
pub struct PptxRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A block of text on a slide.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct PptxText {
    /// The text. Newlines become line breaks in the shape.
    pub text: String,
    #[serde(flatten)]
    pub rect: PptxRect,
    /// Font size in points.
    pub font_size: f64,
    /// Font family. PowerPoint substitutes if the machine lacks it.
    pub font_face: String,
    pub color: PptxColor,
    /// Whether the run is bold. Taken from the design's weight — see
    /// [`FontRepresentation::is_bold`] — because PowerPoint has a switch where
    /// a stylesheet has a scale.
    pub bold: bool,
    /// Whether the run is slanted.
    pub italic: bool,
    pub align: PptxAlign,
    pub valign: PptxVAlign,
    /// Shrink the text to fit the box rather than letting it overflow.
    pub shrink_to_fit: bool,
}

/// A picture on a slide, as a data URL.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct PptxImage {
    /// `data:` URL holding the image.
    pub data: String,
    #[serde(flatten)]
    pub rect: PptxRect,
}

/// A video on a slide, as a data URL.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct PptxMedia {
    /// `data:` URL holding the video.
    pub data: String,
    /// What it is, so PowerPoint knows which decoder to reach for.
    pub mime: String,
    #[serde(flatten)]
    pub rect: PptxRect,
}

/// Anything that can sit on a slide.
#[derive(Clone, PartialEq, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PptxShape {
    Text(PptxText),
    /// A picture slide, and a page of a PDF. Engraved notation is the
    /// remaining candidate, once it can be rasterised.
    Image(PptxImage),
    /// A video, played by PowerPoint itself.
    Media(PptxMedia),
}

/// How big a video may be before it is left out of a deck.
///
/// A `.pptx` carries its video inside itself, and the way one gets there is as
/// base64 inside a JSON document handed to the browser — which makes a
/// hundred-megabyte film something like a hundred and forty megabytes of text
/// to build, hold and parse, twice over. Past this size the deck gets the
/// still frame instead and the export says so.
///
/// 64 MB is roughly a few minutes of ordinary service video, and comfortably
/// more than the clips these decks are usually made of.
///
/// Only where a video can reach a deck at all: the web build has no library on
/// disk to read one from.
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_EMBEDDED_VIDEO_BYTES: u64 = 64 * 1024 * 1024;

/// Whether PowerPoint can play a video of this name at all.
///
/// It is a Microsoft player, not a browser: it reads MP4 and QuickTime and does
/// not read the open formats a web view is happiest with. A `.webm` embedded in
/// a deck is a rectangle with an error in it, so those are exported as their
/// still frame instead — which is what a deck of a video can honestly be
/// without the video.
#[cfg(not(target_arch = "wasm32"))]
pub fn powerpoint_can_play(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    [".mp4", ".m4v", ".mov"].iter().any(|suffix| lower.ends_with(suffix))
}

/// One slide.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct PptxSlide {
    pub background: PptxColor,
    pub shapes: Vec<PptxShape>,
}

impl PptxSlide {
    /// An empty slide on the given background.
    pub fn new(background: PptxColor) -> PptxSlide {
        PptxSlide {
            background,
            shapes: Vec::new(),
        }
    }

    /// Add a shape and return the slide, for building in one expression.
    pub fn with(mut self, shape: PptxShape) -> PptxSlide {
        self.shapes.push(shape);
        self
    }
}

/// A whole deck, ready to be handed to PptxGenJS.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct PptxDeck {
    /// Slide width in inches.
    pub width: f64,
    /// Slide height in inches.
    pub height: f64,
    pub slides: Vec<PptxSlide>,
}

impl PptxDeck {
    /// A 16:9 deck, the shape PowerPoint has defaulted to since 2013 and the
    /// one a projector expects.
    pub fn widescreen() -> PptxDeck {
        PptxDeck {
            width: 13.333,
            height: 7.5,
            slides: Vec::new(),
        }
    }

    pub fn push(&mut self, slide: PptxSlide) {
        self.slides.push(slide);
    }

    pub fn is_empty(&self) -> bool {
        self.slides.is_empty()
    }

    /// The deck as the JSON the JavaScript side consumes.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"slides\":[]}".to_string())
    }

    // --- Convenience for laying a slide out -----------------------------

    /// A text box spanning the full width, inset by `margin` on each side.
    ///
    /// `top` and `height` are fractions of the slide height, which is how the
    /// presentation itself is laid out — that way the deck keeps the same
    /// proportions whatever the slide size.
    pub fn full_width_text(
        &self,
        top: f64,
        height: f64,
        margin: f64,
    ) -> PptxRect {
        PptxRect {
            x: margin,
            y: self.height * top,
            w: self.width - 2.0 * margin,
            h: self.height * height,
        }
    }
}

// ---------------------------------------------------------------------------
// Turning Cantara's slides into a deck
// ---------------------------------------------------------------------------

use cantara_songlib::slides::{Slide, SlideContent};

use crate::logic::settings::{
    CssSize, FontRepresentation, HorizontalAlign, PresentationDesign, PresentationDesignSettings,
    PresentationDesignTemplate,
};

/// Build a deck that looks as close to the running presentation as PowerPoint
/// allows.
///
/// What carries over: the background colour, the fonts and their sizes and
/// colours, the alignment, and where the main text, the spoiler and the meta
/// line sit. What cannot: the notation of a complex slide, because it is drawn
/// by abcjs as SVG in the browser — those slides keep their lyrics, and the
/// caller is told with [`PptxConversion::skipped_notation`].
pub fn deck_from_slides(
    slides: &[Slide],
    design: &PresentationDesign,
    pictures: &std::collections::HashMap<String, String>,
) -> PptxConversion {
    let template = match &design.presentation_design_settings {
        PresentationDesignSettings::Template(template) => template.clone(),
        // A hand-written HTML design cannot be translated into shapes, so the
        // deck falls back to Cantara's defaults rather than failing.
        PresentationDesignSettings::Custom(_) => PresentationDesignTemplate::default(),
    };
    let deck_background = PptxColor::from_css(&template.get_background_color_as_hex_string());

    let mut deck = PptxDeck::widescreen();
    let mut skipped_notation = 0usize;
    let mut missing_pictures = 0usize;
    let mut skipped_videos = 0usize;

    for slide in slides {
        let mut pptx_slide = PptxSlide::new(deck_background.clone());

        match &slide.slide_content {
            // A video reaches the deck the same way a picture does: as a data
            // URL the caller has prepared, looked up here by its path. Which
            // kind it is, is written on it — the caller decides whether the
            // deck gets the film or a still of it, because that decision needs
            // the file's size and PowerPoint's list of formats, and neither is
            // known here.
            SlideContent::Video(video) => match pictures.get(&video.video_path) {
                Some(data) if data.starts_with("data:video/") => {
                    pptx_slide = pptx_slide.with(media_shape(&deck, data));
                }
                // A still of it: everything a deck can honestly show of a
                // video it cannot play.
                Some(data) => {
                    pptx_slide = pptx_slide.with(picture_shape(&deck, data));
                }
                // Nothing was prepared — the file could not be read, or it was
                // too large to carry. Counted rather than passed over in
                // silence, so the export can say what it left out.
                None => {
                    skipped_videos += 1;
                    continue;
                }
            },
            SlideContent::Title(title) => {
                pptx_slide = pptx_slide.with(text_shape(
                    &deck,
                    &title.title_text,
                    &title_font(&template),
                    0.30,
                    0.30,
                ));
                if let Some(meta) = non_empty(&title.meta_text) {
                    pptx_slide = pptx_slide.with(text_shape(
                        &deck,
                        &meta,
                        &template.get_default_meta_font(),
                        0.62,
                        0.12,
                    ));
                }
            }

            SlideContent::SingleLanguageMainContent(main) => {
                let main_text = main.clone().main_text();
                pptx_slide = pptx_slide.with(text_shape(
                    &deck,
                    &main_text,
                    &template.get_default_font(),
                    0.12,
                    0.55,
                ));
                if let Some(spoiler) = non_empty(&main.clone().spoiler_text()) {
                    pptx_slide = pptx_slide.with(text_shape(
                        &deck,
                        &spoiler,
                        &template.get_default_spoiler_font(),
                        0.70,
                        0.18,
                    ));
                }
            }

            SlideContent::MultiLanguageMainContent(multi) => {
                pptx_slide = pptx_slide.with(text_shape(
                    &deck,
                    &multi.main_text_list.join("\n"),
                    &template.get_default_font(),
                    0.12,
                    0.55,
                ));
                if !multi.spoiler_text_vector.is_empty() {
                    pptx_slide = pptx_slide.with(text_shape(
                        &deck,
                        &multi.spoiler_text_vector.join("\n"),
                        &template.get_default_spoiler_font(),
                        0.70,
                        0.18,
                    ));
                }
            }

            SlideContent::Complex(complex) => {
                if complex.rows.iter().any(|row| row.is_notation()) {
                    skipped_notation += 1;
                }

                // The staff cannot go into a PowerPoint shape, so the slide
                // keeps the words it was showing.
                let lyrics: Vec<String> = complex
                    .rows
                    .iter()
                    .filter(|row| !row.is_notation())
                    .map(|row| row.content.clone())
                    .collect();

                if !lyrics.is_empty() {
                    pptx_slide = pptx_slide.with(text_shape(
                        &deck,
                        &lyrics.join("\n"),
                        &template.get_default_font(),
                        0.12,
                        0.55,
                    ));
                }

                let spoiler: Vec<String> = complex
                    .spoiler
                    .iter()
                    .filter(|row| !row.is_notation())
                    .map(|row| row.content.clone())
                    .collect();
                if !spoiler.is_empty() {
                    pptx_slide = pptx_slide.with(text_shape(
                        &deck,
                        &spoiler.join("\n"),
                        &template.get_default_spoiler_font(),
                        0.70,
                        0.18,
                    ));
                }
            }

            // An empty slide is part of the flow — it is what the audience sees
            // between songs — so it becomes an empty slide here too.
            SlideContent::Empty(_) => {}

            // A picture — and a PDF page is one — goes onto the slide as a
            // picture. The caller renders them first and passes them in;
            // getting a PDF page as an image means asking the viewer in the
            // page for it, which is asynchronous and has no place in a pure
            // translation. See [`pictures_needed`].
            SlideContent::SimplePicture(_) | SlideContent::PdfPage(_) => {
                match picture_key(&slide.slide_content).and_then(|key| pictures.get(&key)) {
                    Some(data) => {
                        pptx_slide = pptx_slide.with(picture_shape(&deck, data));
                    }
                    // Nothing came back for it. The slide is still kept, so the
                    // deck has the same number of slides as the presentation
                    // and the moderator's notes still line up.
                    None => missing_pictures += 1,
                }
            }
        }

        // The meta line of a content slide goes in the bottom corner, as it
        // does on screen.
        if !matches!(slide.slide_content, SlideContent::Title(_))
            && let Some(meta) = meta_text_of_slide(&slide.slide_content)
        {
            pptx_slide = pptx_slide.with(meta_corner_shape(
                &deck,
                &meta,
                &template.get_default_meta_font(),
            ));
        }

        deck.push(pptx_slide);
    }

    PptxConversion {
        deck,
        skipped_notation,
        missing_pictures,
        skipped_videos,
    }
}

/// What a slide's picture is called, if it has one.
///
/// The same string the caller renders and hands back in `pictures`: for a PDF
/// the document and the page, for anything else the path of the file.
pub fn picture_key(content: &SlideContent) -> Option<String> {
    match content {
        SlideContent::PdfPage(page) => Some(format!("{}#page={}", page.pdf_path, page.page_number)),
        SlideContent::SimplePicture(picture) => {
            Some(crate::logic::presentation::get_picture_path(picture))
        }
        // A video is prepared the same way, keyed by its path. What the caller
        // puts there — the film or a still of it — is its decision; see the
        // `SlideContent::Video` arm of [`deck_from_slides`].
        SlideContent::Video(video) => Some(video.video_path.clone()),
        _ => None,
    }
}

/// Every picture a deck of these slides needs, each named once.
///
/// The caller renders them — a PDF page through
/// [`crate::logic::pdf::page_image`], an ordinary picture through
/// [`crate::logic::images::image_data_url`] — and passes the result to
/// [`deck_from_slides`]. A presentation that shows the same page twice renders
/// it once.
pub fn pictures_needed(slides: &[Slide]) -> Vec<String> {
    let mut needed: Vec<String> = Vec::new();
    for slide in slides {
        if let Some(key) = picture_key(&slide.slide_content)
            && !needed.contains(&key)
        {
            needed.push(key);
        }
    }
    needed
}

/// A picture filling the slide, keeping its proportions.
///
/// PowerPoint has no `object-fit`, so the box is the whole slide and the
/// picture is told to sit inside it — which is what `pptx_export_inline.js`
/// asks PptxGenJS for.
fn media_shape(deck: &PptxDeck, data: &str) -> PptxShape {
    PptxShape::Media(PptxMedia {
        data: data.to_string(),
        // What is in front of the base64 in the data URL, which is what the
        // caller wrote there from the file's name.
        mime: data
            .strip_prefix("data:")
            .and_then(|rest| rest.split(';').next())
            .unwrap_or("video/mp4")
            .to_string(),
        rect: PptxRect {
            x: 0.0,
            y: 0.0,
            w: deck.width,
            h: deck.height,
        },
    })
}

fn picture_shape(deck: &PptxDeck, data: &str) -> PptxShape {
    PptxShape::Image(PptxImage {
        data: data.to_string(),
        rect: PptxRect {
            x: 0.0,
            y: 0.0,
            w: deck.width,
            h: deck.height,
        },
    })
}

/// The result of converting a presentation.
#[derive(Clone, PartialEq, Debug)]
pub struct PptxConversion {
    pub deck: PptxDeck,
    /// How many slides had notation that could not be carried over.
    pub skipped_notation: usize,
    /// How many slides should have carried a picture and could not — a file
    /// that would not open, or a PDF page that would not render.
    pub missing_pictures: usize,
    /// How many video slides the deck could carry neither as a film nor as a
    /// still of one.
    ///
    /// A video is embedded when PowerPoint can play the format and the file is
    /// small enough to travel inside a `.pptx`, and exported as a frame of
    /// itself otherwise — see `video_for_deck` in the export. This counts what
    /// was left of that: a file that could not be read, or one from which no
    /// frame could be taken either.
    pub skipped_videos: usize,
}

/// The headline font as a title slide actually shows it.
///
/// The bold of a title slide lives beside the headline font rather than in it:
/// [`PresentationDesignTemplate::title_bold`] is a switch of its own, and the
/// on-screen title turns the weight up by it (see `TitleSlideComponent`).
/// Taking only the font here would export a design with the switch on in
/// regular type, so the deck and the screen would disagree about the one line
/// everybody reads first.
fn title_font(template: &PresentationDesignTemplate) -> FontRepresentation {
    let mut font = template.get_default_headline_font();
    if template.title_bold {
        font.weight = font.weight.max(crate::logic::settings::BOLD_WEIGHT);
    }
    font
}

/// A text box spanning the slide width at the given vertical band.
fn text_shape(
    deck: &PptxDeck,
    text: &str,
    font: &FontRepresentation,
    top: f64,
    height: f64,
) -> PptxShape {
    PptxShape::Text(PptxText {
        text: text.to_string(),
        rect: deck.full_width_text(top, height, 0.5),
        font_size: font_size_in_points(&font.font_size),
        font_face: font_face(font),
        color: PptxColor::rgb(font.color.r, font.color.g, font.color.b),
        bold: font.is_bold(),
        italic: font.italic,
        align: match font.horizontal_alignment {
            HorizontalAlign::Left => PptxAlign::Left,
            HorizontalAlign::Right => PptxAlign::Right,
            // Centred and justified both come out centred: PowerPoint has no
            // justification that matches how the slides are laid out.
            _ => PptxAlign::Center,
        },
        valign: PptxVAlign::Middle,
        // Long verses are common; shrinking beats spilling off the slide.
        shrink_to_fit: true,
    })
}

/// The meta line, in the bottom right corner.
fn meta_corner_shape(deck: &PptxDeck, text: &str, font: &FontRepresentation) -> PptxShape {
    PptxShape::Text(PptxText {
        text: text.to_string(),
        rect: PptxRect {
            x: deck.width * 0.5,
            y: deck.height - 0.7,
            w: deck.width * 0.5 - 0.4,
            h: 0.5,
        },
        font_size: font_size_in_points(&font.font_size),
        font_face: font_face(font),
        color: PptxColor::rgb(font.color.r, font.color.g, font.color.b),
        bold: font.is_bold(),
        italic: font.italic,
        align: PptxAlign::Right,
        valign: PptxVAlign::Bottom,
        shrink_to_fit: true,
    })
}

/// The font family name, or a sensible default when the design leaves it open.
fn font_face(font: &FontRepresentation) -> String {
    font.font_family
        .as_ref()
        .and_then(|family| family.family.clone())
        .filter(|family| !family.trim().is_empty())
        .unwrap_or_else(|| "Arial".to_string())
}

/// A size in points, which is what PowerPoint measures type in.
fn font_size_in_points(size: &CssSize) -> f64 {
    let points = match size {
        CssSize::Px(value) => *value as f64 * 0.75,
        CssSize::Pt(value) => *value as f64,
        CssSize::Em(value) => *value as f64 * 12.0,
        CssSize::Percentage(value) => *value as f64 / 100.0 * 12.0,
        CssSize::Null => 0.0,
    };
    points.clamp(8.0, 200.0)
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// The meta line of a slide. `SingleLanguageMainContentSlide` keeps the field
/// private, so it is read through serde — the same workaround the presentation
/// renderer uses.
fn meta_text_of_slide(content: &SlideContent) -> Option<String> {
    let text = match content {
        SlideContent::Title(title) => title.meta_text.clone(),
        SlideContent::MultiLanguageMainContent(multi) => multi.meta_text.clone(),
        SlideContent::Complex(complex) => complex.meta_text.clone(),
        SlideContent::SingleLanguageMainContent(_) => serde_json::to_value(content)
            .ok()
            .and_then(|value| {
                value
                    .as_object()
                    .and_then(|map| map.values().next())
                    .and_then(|inner| inner.get("meta_text"))
                    .and_then(|meta| meta.as_str())
                    .map(String::from)
            }),
        _ => None,
    };
    text.filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use cantara_songlib::slides::{Slide, SlideSettings};

    fn slides_of(path: &str, settings: &SlideSettings) -> Vec<Slide> {
        let content = std::fs::read_to_string(path).unwrap();
        let file_name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        crate::logic::presentation::slides_from_song_content(
            &content,
            file_name,
            settings,
            "fallback",
            &[],
        )
        .unwrap()
    }

    /// The deck has to keep one PowerPoint slide per presentation slide, so
    /// the exported file runs the same way the presentation does.
    #[test]
    fn test_one_pptx_slide_per_presentation_slide() {
        let slides = slides_of(
            "testfiles/Amazing Grace.song.yml",
            &SlideSettings::default(),
        );
        let conversion = deck_from_slides(&slides, &PresentationDesign::default(), &HashMap::new());

        assert_eq!(conversion.deck.slides.len(), slides.len());
        assert_eq!(conversion.skipped_notation, 0);
    }

    /// The lyrics have to make it into a text shape, otherwise the deck is
    /// empty boxes.
    #[test]
    fn test_lyrics_reach_the_deck() {
        let slides = slides_of(
            "testfiles/Amazing Grace.song.yml",
            &SlideSettings::default(),
        );
        let conversion = deck_from_slides(&slides, &PresentationDesign::default(), &HashMap::new());
        let json = conversion.deck.to_json();

        assert!(json.contains("Amazing Grace"), "the title is missing");
        assert!(
            json.contains("Amazing grace, How sweet the sound"),
            "the lyrics are missing"
        );
    }

    /// Every shape has to sit inside the slide; PowerPoint would otherwise
    /// place it off the canvas.
    #[test]
    fn test_every_shape_is_inside_the_slide() {
        let slides = slides_of(
            "testfiles/Amazing Grace.song.yml",
            &SlideSettings::default(),
        );
        let deck = deck_from_slides(&slides, &PresentationDesign::default(), &HashMap::new()).deck;

        for (index, slide) in deck.slides.iter().enumerate() {
            for shape in &slide.shapes {
                let rect = match shape {
                    PptxShape::Text(text) => text.rect,
                    PptxShape::Image(image) => image.rect,
                    PptxShape::Media(media) => media.rect,
                };
                assert!(rect.x >= 0.0, "slide {index}: x {} < 0", rect.x);
                assert!(rect.y >= 0.0, "slide {index}: y {} < 0", rect.y);
                assert!(
                    rect.x + rect.w <= deck.width + 0.001,
                    "slide {index}: runs past the right edge"
                );
                assert!(
                    rect.y + rect.h <= deck.height + 0.001,
                    "slide {index}: runs past the bottom edge"
                );
            }
        }
    }

    /// The design's colours and fonts have to carry over.
    #[test]
    fn test_design_carries_over() {
        let slides = slides_of(
            "testfiles/Amazing Grace.song.yml",
            &SlideSettings::default(),
        );
        let deck = deck_from_slides(&slides, &PresentationDesign::default(), &HashMap::new()).deck;

        let first_text = deck
            .slides
            .iter()
            .flat_map(|slide| &slide.shapes)
            .find_map(|shape| match shape {
                PptxShape::Text(text) => Some(text),
                _ => None,
            })
            .expect("a text shape");

        // Cantara's default design is white type on black.
        assert_eq!(first_text.color.as_hex(), "FFFFFF");
        assert_eq!(deck.slides[0].background.as_hex(), "000000");
        assert!(first_text.font_size >= 8.0 && first_text.font_size <= 200.0);
        assert!(!first_text.font_face.is_empty());
        // The default design sets neither.
        assert!(!first_text.bold);
        assert!(!first_text.italic);
    }

    /// A design set in bold italic has to export as bold italic. Both used to
    /// be dropped on the way out: every run was written `bold: false` and
    /// there was no italic at all, so a deck exported from a heavy design came
    /// back in regular type.
    #[test]
    fn test_the_designs_type_style_survives_the_export() {
        let mut design = PresentationDesign::default();
        let PresentationDesignSettings::Template(template) =
            &mut design.presentation_design_settings
        else {
            panic!("the default design is a template");
        };
        for font in &mut template.fonts {
            font.set_bold(true);
            font.italic = true;
        }

        let slides = slides_of(
            "testfiles/Amazing Grace.song.yml",
            &SlideSettings::default(),
        );
        let deck = deck_from_slides(&slides, &design, &HashMap::new()).deck;

        let texts: Vec<&PptxText> = deck
            .slides
            .iter()
            .flat_map(|slide| &slide.shapes)
            .filter_map(|shape| match shape {
                PptxShape::Text(text) => Some(text),
                _ => None,
            })
            .collect();

        assert!(!texts.is_empty(), "the deck has no text at all");
        for text in texts {
            assert!(text.bold, "{:?} came out in regular weight", text.text);
            assert!(text.italic, "{:?} came out upright", text.text);
        }
    }

    /// The title slide's bold is a switch beside the headline font, not a
    /// weight in it. It used to be read off the font alone on the way out, so
    /// a design with the switch on and a regular headline font exported a
    /// title in regular type while the screen showed it heavy.
    #[test]
    fn test_the_title_bold_switch_survives_the_export() {
        let mut design = PresentationDesign::default();
        let PresentationDesignSettings::Template(template) =
            &mut design.presentation_design_settings
        else {
            panic!("the default design is a template");
        };
        template.title_bold = true;
        for font in &mut template.fonts {
            font.set_bold(false);
        }

        let slides = vec![Slide::new_title_slide("Amazing Grace".to_string(), None)];
        let deck = deck_from_slides(&slides, &design, &HashMap::new()).deck;

        let PptxShape::Text(title) = &deck.slides[0].shapes[0] else {
            panic!("the title is a text shape");
        };
        assert_eq!(title.text, "Amazing Grace");
        assert!(
            title.bold,
            "the title slide's bold switch was dropped on the way out"
        );
    }

    /// Notation cannot become a PowerPoint shape. The slide keeps its words and
    /// the caller is told how many staves were left out.
    #[test]
    fn test_notation_is_reported_and_lyrics_kept() {
        use cantara_songlib::slides::{LanguageConfiguration, SlideElement};

        let settings = SlideSettings {
            language: LanguageConfiguration::Complex(vec![
                SlideElement::Notation,
                SlideElement::Lyrics("en".to_string()),
            ]),
            title_slide: false,
            empty_last_slide: false,
            ..SlideSettings::default()
        };
        let slides = slides_of("testfiles/Amazing Grace.song.yml", &settings);
        let conversion = deck_from_slides(&slides, &PresentationDesign::default(), &HashMap::new());

        assert!(
            conversion.skipped_notation > 0,
            "the skipped staves were not reported"
        );
        assert!(
            conversion.deck.to_json().contains("Amazing grace"),
            "the lyrics were dropped along with the notation"
        );
        // And no staff leaked in as raw ABC.
        assert!(!conversion.deck.to_json().contains("X:1"));
    }

    #[test]
    fn test_colour_from_css() {
        assert_eq!(PptxColor::from_css("#ffffff").as_hex(), "FFFFFF");
        assert_eq!(PptxColor::from_css("#000000").as_hex(), "000000");
        assert_eq!(PptxColor::from_css("#F0A").as_hex(), "FF00AA");
        assert_eq!(PptxColor::from_css("rgb(255, 128, 0)").as_hex(), "FF8000");
        assert_eq!(PptxColor::from_css("rgba(255, 128, 0, 0.5)").as_hex(), "FF8000");
        assert_eq!(PptxColor::from_css("  #12ab34  ").as_hex(), "12AB34");
    }

    /// An unreadable colour must not fail the export.
    #[test]
    fn test_unknown_colour_falls_back_to_black() {
        assert_eq!(PptxColor::from_css("chartreuse").as_hex(), "000000");
        assert_eq!(PptxColor::from_css("").as_hex(), "000000");
        assert_eq!(PptxColor::from_css("#12").as_hex(), "000000");
    }

    #[test]
    fn test_widescreen_is_16_by_9() {
        let deck = PptxDeck::widescreen();
        let ratio = deck.width / deck.height;
        assert!((ratio - 16.0 / 9.0).abs() < 0.01, "ratio was {ratio}");
    }

    #[test]
    fn test_full_width_text_stays_inside_the_slide() {
        let deck = PptxDeck::widescreen();
        let rect = deck.full_width_text(0.1, 0.5, 0.5);

        assert!(rect.x >= 0.0);
        assert!(rect.x + rect.w <= deck.width + f64::EPSILON);
        assert!(rect.y >= 0.0);
        assert!(rect.y + rect.h <= deck.height + f64::EPSILON);
        assert_eq!(rect.w, deck.width - 1.0);
    }

    /// The JSON is the contract with the JavaScript side, so its shape is
    /// pinned here.
    #[test]
    fn test_json_shape() {
        let mut deck = PptxDeck::widescreen();
        deck.push(PptxSlide::new(PptxColor::rgb(0, 0, 0)).with(PptxShape::Text(PptxText {
            text: "Amazing Grace".to_string(),
            rect: PptxRect { x: 0.5, y: 1.0, w: 12.0, h: 2.0 },
            font_size: 40.0,
            font_face: "Arial".to_string(),
            color: PptxColor::rgb(255, 255, 255),
            bold: true,
            italic: false,
            align: PptxAlign::Center,
            valign: PptxVAlign::Middle,
            shrink_to_fit: true,
        })));

        let json: serde_json::Value = serde_json::from_str(&deck.to_json()).unwrap();

        assert_eq!(json["width"], 13.333);
        let shape = &json["slides"][0]["shapes"][0];
        assert_eq!(shape["kind"], "text");
        assert_eq!(shape["text"], "Amazing Grace");
        // The rectangle is flattened, so PptxGenJS can take the fields directly.
        assert_eq!(shape["x"], 0.5);
        assert_eq!(shape["w"], 12.0);
        assert_eq!(shape["font_size"], 40.0);
        assert_eq!(shape["color"], "FFFFFF");
        assert_eq!(shape["align"], "center");
        assert_eq!(shape["valign"], "middle");
        assert_eq!(json["slides"][0]["background"], "000000");
    }

    #[test]
    fn test_empty_deck() {
        let deck = PptxDeck::widescreen();
        assert!(deck.is_empty());
        let json: serde_json::Value = serde_json::from_str(&deck.to_json()).unwrap();
        assert_eq!(json["slides"].as_array().unwrap().len(), 0);
    }

    /// A picture slide used to come out blank. It carries its picture now, and
    /// a page of a PDF is a picture like any other.
    #[test]
    fn a_picture_slide_carries_its_picture() {
        let slides = vec![
            Slide::new_pdf_page_slide("handout.pdf".to_string(), 2),
        ];
        let mut pictures = HashMap::new();
        pictures.insert(
            "handout.pdf#page=2".to_string(),
            "data:image/png;base64,AAAA".to_string(),
        );

        let conversion = deck_from_slides(&slides, &PresentationDesign::default(), &pictures);

        assert_eq!(conversion.missing_pictures, 0);
        let shapes = &conversion.deck.slides[0].shapes;
        let picture = shapes
            .iter()
            .find_map(|shape| match shape {
                PptxShape::Image(image) => Some(image),
                _ => None,
            })
            .expect("the slide carries its picture");
        assert_eq!(picture.data, "data:image/png;base64,AAAA");
        // The whole slide, so PowerPoint fits the page into it.
        assert_eq!(picture.rect.x, 0.0);
        assert_eq!(picture.rect.w, conversion.deck.width);
    }

    /// A picture that could not be rendered leaves the slide in place — the
    /// deck has to have as many slides as the presentation — and is counted, so
    /// the user is told rather than finding a blank slide later.
    #[test]
    fn a_picture_that_is_not_there_is_counted() {
        let slides = vec![Slide::new_pdf_page_slide("handout.pdf".to_string(), 2)];

        let conversion =
            deck_from_slides(&slides, &PresentationDesign::default(), &HashMap::new());

        assert_eq!(conversion.deck.slides.len(), 1, "the slide is still there");
        assert_eq!(conversion.missing_pictures, 1);
    }

    /// The caller has to know what to render, and a page shown twice is
    /// rendered once.
    #[test]
    fn the_pictures_a_deck_needs_are_named_once_each() {
        let slides = vec![
            Slide::new_pdf_page_slide("handout.pdf".to_string(), 1),
            Slide::new_content_slide("Amazing grace".to_string(), None, None),
            Slide::new_pdf_page_slide("handout.pdf".to_string(), 2),
            Slide::new_pdf_page_slide("handout.pdf".to_string(), 1),
        ];

        assert_eq!(
            pictures_needed(&slides),
            vec!["handout.pdf#page=1", "handout.pdf#page=2"]
        );
    }

    /// A slide made of text needs no picture.
    #[test]
    fn a_text_slide_needs_no_picture() {
        let slides = vec![
            Slide::new_content_slide("Amazing grace".to_string(), None, None),
            Slide::new_title_slide("Amazing Grace".to_string(), None),
        ];

        assert!(pictures_needed(&slides).is_empty());
    }

    /// PowerPoint is a Microsoft player, not a browser: it reads MP4 and
    /// QuickTime and does not read the formats a web view is happiest with.
    /// Embedding a `.webm` would put a rectangle with an error in it on the
    /// slide, which is worse than a still of the video.
    #[test]
    fn test_which_formats_powerpoint_can_play() {
        for playable in ["intro.mp4", "Intro.MP4", "clip.m4v", "scene.mov"] {
            assert!(powerpoint_can_play(playable), "{playable}");
        }
        for not in ["intro.webm", "intro.ogv", "intro.mkv", "intro.avi", "intro"] {
            assert!(!powerpoint_can_play(not), "{not}");
        }
    }

    /// A video the caller prepared as a film is played by the deck; one it
    /// prepared as a still is shown as a picture. Which of the two it is, is
    /// written on the data URL, so the two cannot be confused.
    #[test]
    fn test_a_video_becomes_a_film_or_a_still_depending_on_what_was_prepared() {
        use cantara_songlib::slides::Slide;
        use std::collections::HashMap;

        let design = PresentationDesign::default();
        let slides = vec![Slide::new_video_slide("/library/intro.mp4".to_string(), true, false)];

        // Prepared as a film.
        let mut prepared = HashMap::new();
        prepared.insert(
            "/library/intro.mp4".to_string(),
            "data:video/mp4;base64,AAAA".to_string(),
        );
        let conversion = deck_from_slides(&slides, &design, &prepared);
        assert!(matches!(
            conversion.deck.slides[0].shapes[0],
            PptxShape::Media(_)
        ));
        assert_eq!(conversion.skipped_videos, 0);

        // Prepared as a still.
        let mut prepared = HashMap::new();
        prepared.insert(
            "/library/intro.mp4".to_string(),
            "data:image/png;base64,AAAA".to_string(),
        );
        let conversion = deck_from_slides(&slides, &design, &prepared);
        assert!(matches!(
            conversion.deck.slides[0].shapes[0],
            PptxShape::Image(_)
        ));
    }

    /// Nothing prepared at all — the file could not be read, or was too large
    /// to carry. The deck is still written, and says what it left out.
    #[test]
    fn test_a_video_that_could_not_be_prepared_is_counted_not_dropped_silently() {
        use cantara_songlib::slides::Slide;
        use std::collections::HashMap;

        let slides = vec![Slide::new_video_slide("/library/intro.mp4".to_string(), true, false)];

        let conversion =
            deck_from_slides(&slides, &PresentationDesign::default(), &HashMap::new());

        assert_eq!(conversion.skipped_videos, 1);
        assert!(conversion.deck.is_empty(), "no slide was written for it");
    }

    /// The MIME type travels with the film so PowerPoint knows what it has.
    #[test]
    fn test_the_media_shape_carries_the_type_from_the_data_url() {
        let deck = PptxDeck::widescreen();

        match media_shape(&deck, "data:video/quicktime;base64,AAAA") {
            PptxShape::Media(media) => assert_eq!(media.mime, "video/quicktime"),
            other => panic!("expected media, got {other:?}"),
        }
    }

}
