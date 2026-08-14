//! What a browser on the network is told about the presentation.
//!
//! This is the whole agreement between Cantara and the page it serves, and it
//! is deliberately *data* rather than a rendering. A phone is not a projector:
//! it is held upright, it is a fifth of the size, and a picture of a 16:9 slide
//! is unreadable on it. Sending what the slide *says* lets the page set it in
//! the design's colours and fonts at a size that suits the screen it is on.
//!
//! It also carries the **whole running order**, not only the slide that is up.
//! Two views are planned that need exactly that — a stage view, which shows
//! what is coming as well as what is showing, and a running-order sheet, which
//! is the entire service scrolled through in one page — and neither should
//! need a protocol of its own. What is up now is [`StreamState::position`]; the
//! rest is already there.
//!
//! Everything here is pure: it is built from a [`RunningPresentation`] and
//! turned into JSON, and both halves can be tested without a browser or a
//! socket in sight.

use serde::{Deserialize, Serialize};

use crate::components::presentation_components::{
    abcjs_vocal_font, meta_text_of, notation_block_style, staff_separation,
};
use crate::logic::presentation::{get_markdown_html, get_picture_path, html_to_plain_text};
use crate::logic::css::{CssFontFamily, CssString};
use crate::logic::settings::{CssSize, PresentationDesign, PresentationDesignSettings};
use crate::logic::states::RunningPresentation;
use cantara_songlib::slides::{Slide, SlideContent, SlideRow};

/// Everything a viewer needs, as of one moment.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct StreamState {
    /// Counts up with every change. A viewer that reconnects can tell at a
    /// glance whether it missed anything, and a page that is told the same
    /// thing twice can ignore the second one.
    pub revision: u64,

    /// Whether a presentation is running at all. When it is not, the page says
    /// so and waits — the address stays open so it can be tried in advance.
    pub running: bool,

    /// How the presentation looks, so the page can look like it.
    pub design: StreamDesign,

    /// The whole service, in order.
    pub chapters: Vec<StreamChapter>,

    /// Which slide is up, if one is.
    pub position: Option<StreamPosition>,

    /// Whether the presentation is showing a blank screen. The page follows,
    /// so that a moderator who blanks the projection blanks every phone too
    /// rather than leaving the words up in the room.
    pub blacked_out: bool,

    /// Where the video on the current slide has got to, when the slide is a
    /// video. `None` otherwise.
    ///
    /// Sent with every update so that a phone shows the same moment of the
    /// video as the room does: the page holds its own copy of the file and is
    /// pulled onto this position rather than being sent pictures. That is what
    /// makes it the same video at the same moment without anything being
    /// re-encoded — and what makes it work at all on a phone, which would drop
    /// a stream of frames long before it drops a file it is playing itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<StreamVideoState>,
}

/// Where the video on the current slide stands.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct StreamVideoState {
    /// Whether it is running.
    pub playing: bool,
    /// How far into it the presentation is, in seconds.
    pub position: f64,
    /// How long it is, in seconds; `0.0` before that is known.
    pub duration: f64,
}

/// Where the presentation stands, as indices into [`StreamState::chapters`].
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StreamPosition {
    pub chapter: usize,
    pub slide: usize,
}

/// One element of the service — a song, a reading, a document.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct StreamChapter {
    /// What it is called, as it reads in the running order.
    pub title: String,
    pub slides: Vec<StreamSlide>,
}

/// One slide, as much of it as is worth sending.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StreamSlide {
    /// The title of an element, shown before it begins.
    Title { text: String, meta: Option<String> },
    /// Words to read or sing, and the staves that stand among them.
    Text {
        /// The blocks of the slide, in the order the design puts them — a song
        /// set up as "english, notation, german" puts the staff between the two
        /// languages, and a viewer has to see it there too.
        rows: Vec<StreamRow>,
        /// The next lines, where the design shows them ahead. Text only: a
        /// spoiler is meant to be small, and the projection does not repeat the
        /// notation in it either.
        spoiler: Vec<String>,
        meta: Option<String>,
    },
    /// A picture, or a page of a PDF — anything that has to be *seen* rather
    /// than read. `media` names it in [`crate::logic::stream`]'s store; the
    /// page fetches it from the server rather than having it pushed through
    /// every update.
    Picture { media: String },
    /// A video. `media` names it the way [`StreamSlide::Picture`] does, and the
    /// page fetches it from the server, which serves it in the parts the
    /// browser asks for so that it can be played and seeked without the whole
    /// file arriving first.
    ///
    /// How far into it the presentation is does *not* belong here. This is the
    /// running order, which changes when the service is rebuilt; the playback
    /// position changes many times a second and is sent separately — see
    /// [`StreamState::position`] for the same distinction one level up.
    Video {
        media: String,
        autostart: bool,
        looping: bool,
    },
    /// Deliberately nothing: the gap between two elements of a service.
    Empty,
}

/// One block of a text slide.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "row", rename_all = "lowercase")]
pub enum StreamRow {
    /// Words. One entry per line, so the page can break them where the slide
    /// does rather than guessing.
    Lyrics { lines: Vec<String> },

    /// A staff, as the ABC source the projection engraves.
    ///
    /// Sent as source rather than as a picture on purpose. A staff drawn to fit
    /// a projector is a thin grey smear on a phone; engraved in the browser it
    /// is drawn as SVG at whatever width the screen has, stays sharp, and costs
    /// one line of text per slide change instead of an image. The words under
    /// the notes are part of the engraving, which is why they are not repeated
    /// as lyrics — see [`StreamRow::of`].
    Notation { abc: String },
}

/// The parts of a presentation design a page can honour.
///
/// Not the whole design — a browser has no business with padding in
/// millimetres or which monitor it is on. Colours and a font family are what
/// make the page recognisably the same presentation.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct StreamDesign {
    /// The slide background, as CSS.
    pub background: String,
    /// The text colour, as CSS.
    pub foreground: String,
    /// The font family, as CSS. A viewer's browser may not have it; the page
    /// falls back to something sensible, which is why this is a whole family
    /// list and not one name.
    pub font_family: String,

    /// Everything else the design says about its text, as CSS declarations:
    /// the family, the weight, the alignment, the shadow, the outline.
    ///
    /// Generated by the same code that dresses the projection, so a design
    /// cannot look one way on the wall and another on a phone — and so that a
    /// setting added to a design reaches a viewer without anything here having
    /// to learn about it.
    ///
    /// The size is deliberately *not* in it: a phone is a fifth of the size of
    /// a projection, so that one is the page's to decide. The family is, ending
    /// in something every device has — a design that gives the spoiler a face
    /// of its own is not showing itself if every block on the phone wears the
    /// main text's. See [`without_properties`].
    #[serde(default)]
    pub text_css: String,

    /// The same, for the lines a design shows ahead. A design gives the
    /// spoiler a font of its own — usually the same face, smaller and quieter
    /// — and a page that invents its own instead is not showing the design.
    #[serde(default)]
    pub spoiler_css: String,

    /// The same, for the meta line in the corner.
    #[serde(default)]
    pub meta_css: String,

    /// The same, for the headline of a title slide.
    #[serde(default)]
    pub title_css: String,

    /// How much smaller the spoiler is than the words being sung, as a
    /// fraction of them.
    ///
    /// A ratio rather than a size, because the size itself is the page's to
    /// choose: what has to survive the trip is the *relationship* the design
    /// sets up between the two, so that a spoiler stays quieter than the line
    /// above it on a phone exactly as it does on the wall.
    #[serde(default = "one")]
    pub spoiler_scale: f32,

    /// The same, for the meta line.
    #[serde(default = "one")]
    pub meta_scale: f32,

    /// The same, for a headline.
    #[serde(default = "one")]
    pub title_scale: f32,

    /// The gap a design puts between the words and the lines shown ahead, as
    /// CSS. The design states one distance for both blocks of a slide, and
    /// this is it.
    #[serde(default)]
    pub block_gap: String,

    /// How the design engraves a staff, for the slides that have one.
    #[serde(default)]
    pub notation: StreamNotation,

    /// The design's background picture, named the way slide pictures are —
    /// the page fetches it once rather than having it pushed with every
    /// update. `None` when the design is a plain colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_image: Option<String>,

    /// How far the background picture is faded towards the background colour,
    /// from 0 (the picture as it is) to 100 (not visible at all). The same
    /// number the projection uses, so the two look alike.
    #[serde(default)]
    pub background_transparency: u8,
}

/// What a page needs in order to engrave a staff the way the projection does.
///
/// Not a rendering of the staff — the numbers the engraver is given, so that
/// abcjs in the viewer's browser is driven with exactly the arguments abcjs in
/// the presentation window was. The two then differ only in how wide they are
/// drawn, which is the one thing that *should* differ.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct StreamNotation {
    /// The `vocalfont` abcjs is given for the words under the notes. A
    /// `"family size"` string, because that is the only form abcjs honours.
    pub vocal_font: String,

    /// The `%%staffsep` the design asks for, or `None` where it asks for the
    /// engraver's own spacing. The page prepends it to the tune — the directive
    /// is only honoured in the source, not as a render option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staff_separation: Option<f64>,

    /// The box the staff is drawn in — how wide, and where — as the same CSS
    /// the projection puts on its notation row.
    pub block_css: String,
}

impl Default for StreamDesign {
    fn default() -> Self {
        StreamDesign {
            background: "#000000".to_string(),
            foreground: "#ffffff".to_string(),
            font_family: "system-ui, sans-serif".to_string(),
            text_css: String::new(),
            spoiler_css: String::new(),
            meta_css: String::new(),
            title_css: String::new(),
            spoiler_scale: 1.0,
            meta_scale: 1.0,
            title_scale: 1.0,
            block_gap: "2em".to_string(),
            notation: StreamNotation::default(),
            background_image: None,
            background_transparency: 0,
        }
    }
}

/// Names the picture of a slide, so the same picture is asked for once.
///
/// Derived from what the picture *is* rather than from where it sits in the
/// running order: a document that appears twice in a service is one file, and
/// a viewer should not fetch it twice.
pub fn media_id(source: &str) -> String {
    format!("{:x}", md5::compute(source.as_bytes()))
}

impl StreamState {
    /// What to tell a viewer when nothing is running.
    ///
    /// The address stays open between presentations so it can be handed out
    /// and tried in advance, and this is what answers it.
    pub fn waiting(revision: u64) -> Self {
        StreamState {
            revision,
            running: false,
            ..StreamState::default()
        }
    }

    /// What to tell a viewer about a presentation that is running.
    ///
    /// Built throughout from what the *stream* is set up to show, which is the
    /// projection's own slides and design unless the service asked for
    /// something else. Where it did, the second division of the song is what
    /// travels and the position is the mapped one — see
    /// [`crate::logic::stream_view`].
    pub fn of(presentation: &RunningPresentation, revision: u64) -> Self {
        let design = StreamDesign::of(&presentation.get_current_stream_design());

        let chapters = presentation
            .presentation
            .iter()
            .map(|chapter| StreamChapter {
                title: chapter.source_file.name.clone(),
                slides: chapter
                    .slides_for_stream()
                    .iter()
                    .map(StreamSlide::of)
                    .collect(),
            })
            .collect();

        let position = presentation
            .stream_position()
            .map(|(chapter, slide)| StreamPosition { chapter, slide });

        StreamState {
            revision,
            running: true,
            design,
            chapters,
            position,
            blacked_out: presentation.is_black_screen,
            // Only while a video is what is up. Sending the last position of a
            // video that has been left would have every phone quietly seeking
            // a file it is no longer showing.
            video: presentation
                .get_current_slide()
                .filter(|slide| matches!(slide.slide_content, SlideContent::Video(_)))
                .map(|_| StreamVideoState {
                    playing: presentation.video.playing,
                    position: presentation.video.position,
                    duration: presentation.video.duration,
                }),
        }
    }

    /// The slide that is up, if there is one.
    ///
    /// Not read by the program itself — the page works this out for itself
    /// from the same fields — but it is the operation the planned stage view
    /// and running-order sheet are built on, and it is tested here so that the
    /// meaning of a position cannot drift away from what a viewer assumes.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn current_slide(&self) -> Option<&StreamSlide> {
        let position = self.position?;
        self.chapters
            .get(position.chapter)?
            .slides
            .get(position.slide)
    }

    /// Every picture the running order refers to, each named once.
    ///
    /// What the program has to render and hand to the server before a viewer
    /// asking for it gets anything — the design's background as well as the
    /// slides', since a page whose background never arrives is as wrong as a
    /// slide whose picture does not.
    pub fn media(&self) -> Vec<String> {
        let mut named: Vec<String> = Vec::new();
        if let Some(background) = &self.design.background_image {
            named.push(background.clone());
        }
        for chapter in &self.chapters {
            for slide in &chapter.slides {
                if let StreamSlide::Picture { media } = slide
                    && !named.contains(media)
                {
                    named.push(media.clone());
                }
            }
        }
        named
    }

    /// Every video the running order refers to, each named once.
    ///
    /// Apart from [`media`](Self::media) because the two are handed over
    /// differently: a picture is sent as bytes and held in memory, while a
    /// video is registered by its path and served from there in pieces. See
    /// [`crate::logic::stream::publish_video`].
    pub fn videos(&self) -> Vec<String> {
        let mut named: Vec<String> = Vec::new();
        for chapter in &self.chapters {
            for slide in &chapter.slides {
                if let StreamSlide::Video { media, .. } = slide
                    && !named.contains(media)
                {
                    named.push(media.clone());
                }
            }
        }
        named
    }
}

impl StreamDesign {
    fn of(design: &PresentationDesign) -> Self {
        let PresentationDesignSettings::Template(template) = &design.presentation_design_settings
        else {
            // A hand-written HTML design cannot be reduced to three values, so
            // the page falls back to something readable rather than guessing.
            return StreamDesign::default();
        };

        // Every role a slide has, taken from the design the same way the
        // projection takes them — one font for the words, another for the
        // lines shown ahead, another for the meta line, another for a
        // headline. A page that dresses them all alike is not showing the
        // design, which is what a spoiler in the wrong face and the wrong
        // size looked like.
        let font = template.get_default_font();
        let spoiler = template.get_default_spoiler_font();
        let meta = template.get_default_meta_font();
        let title = template.get_default_headline_font();

        let main_size = relative_size(&font.font_size);
        StreamDesign {
            text_css: font_css(&font),
            spoiler_css: font_css(&spoiler),
            meta_css: font_css(&meta),
            title_css: font_css(&title),
            spoiler_scale: scale_against(&spoiler.font_size, main_size),
            meta_scale: scale_against(&meta.font_size, main_size),
            title_scale: scale_against(&title.font_size, main_size),
            block_gap: template
                .main_content_spoiler_content_padding
                .to_css_string(),
            notation: StreamNotation {
                vocal_font: abcjs_vocal_font(&font, &template.notation.font_size),
                staff_separation: staff_separation(template.notation.staff_line_height),
                block_css: notation_block_style(&template.notation),
            },
            background_image: template
                .background_image
                .as_ref()
                .map(|picture| media_id(&picture.as_source().path.to_string_lossy())),
            background_transparency: template.background_transparency,
            background: template.get_background_color_as_hex_string(),
            foreground: format!(
                "#{:02x}{:02x}{:02x}",
                font.color.r, font.color.g, font.color.b
            ),
            font_family: family_list(&font.font_family),
        }
    }
}

/// A design's face, with something every device has behind it.
///
/// The design's face is a file on the presenting machine and a viewer's browser
/// has never heard of it, so what reaches the page can never be one name: it
/// has to end in a family that is certain to resolve.
fn family_list(family: &Option<CssFontFamily>) -> String {
    match family {
        Some(family) => format!("{}, system-ui, sans-serif", family.to_css_string()),
        None => "system-ui, sans-serif".to_string(),
    }
}

impl StreamSlide {
    fn of(slide: &Slide) -> Self {
        match &slide.slide_content {
            SlideContent::Title(title) => StreamSlide::Title {
                text: title.title_text.clone(),
                meta: non_empty(title.meta_text.clone()),
            },

            SlideContent::SingleLanguageMainContent(main) => {
                let text = main.clone().main_text();
                // A markdown slide arrives as rendered HTML; a phone wants the
                // words, not the markup.
                let text = match get_markdown_html(&text) {
                    Some(html) => html_to_plain_text(html),
                    None => text,
                };
                StreamSlide::Text {
                    rows: lyrics(&text),
                    spoiler: main
                        .clone()
                        .spoiler_text()
                        .map(|spoiler| lines_of(&spoiler))
                        .unwrap_or_default(),
                    meta: non_empty(meta_text_of(&slide.slide_content)),
                }
            }

            SlideContent::MultiLanguageMainContent(multi) => StreamSlide::Text {
                rows: multi
                    .main_text_list
                    .iter()
                    .flat_map(|text| lyrics(text))
                    .collect(),
                spoiler: multi
                    .spoiler_text_vector
                    .iter()
                    .flat_map(|text| lines_of(text))
                    .collect(),
                meta: non_empty(multi.meta_text.clone()),
            },

            // The rows in the order the design puts them, staff included, and
            // without the lyrics the notation already prints under its own
            // notes — exactly the reading the projection makes. Dropping the
            // staff and keeping those lyrics, as this used to, showed a phone
            // the words of a notation slide twice the size of everything and
            // the melody not at all.
            SlideContent::Complex(complex) => StreamSlide::Text {
                rows: complex.rows_without_repetition().map(StreamRow::of).collect(),
                spoiler: complex
                    .spoiler
                    .iter()
                    .filter(|row| !row.is_notation())
                    .flat_map(|row| lines_of(&row.content))
                    .collect(),
                meta: non_empty(complex.meta_text.clone()),
            },

            SlideContent::SimplePicture(picture) => StreamSlide::Picture {
                media: media_id(&get_picture_path(picture)),
            },

            SlideContent::PdfPage(page) => StreamSlide::Picture {
                media: media_id(&format!("{}#page={}", page.pdf_path, page.page_number)),
            },

            SlideContent::Video(video) => StreamSlide::Video {
                media: media_id(&video.video_path),
                autostart: video.autostart,
                looping: video.looping,
            },

            SlideContent::Empty(_) => StreamSlide::Empty,
        }
    }
}

impl StreamRow {
    fn of(row: &SlideRow) -> Self {
        if row.is_notation() {
            StreamRow::Notation {
                abc: row.content.clone(),
            }
        } else {
            StreamRow::Lyrics {
                lines: lines_of(&row.content),
            }
        }
    }
}

/// A block of words as the one row it is, or nothing where it has none.
fn lyrics(text: &str) -> Vec<StreamRow> {
    let lines = lines_of(text);
    if lines.is_empty() {
        return Vec::new();
    }
    vec![StreamRow::Lyrics { lines }]
}

/// One is the ratio of a thing to itself, and the sensible thing to assume
/// when two sizes cannot be compared.
fn one() -> f32 {
    1.0
}

/// A role's font, as the CSS a page can wear.
///
/// A shadow and an outline are turned explicitly *off* where the font has
/// neither, because both inherit: the words of a slide are dressed on the
/// element every block sits inside, so a spoiler the design gave no shadow
/// would quietly wear the one belonging to the line above it. The projection
/// has no such problem — there, each block is given its own complete style.
///
/// The face is restated for the same reason. Only the *size* is the page's to
/// decide.
fn font_css(font: &crate::logic::settings::FontRepresentation) -> String {
    let mut css = without_properties(
        &crate::logic::css::CssHandler::from(font.clone()).to_string(),
        &["font-family", "font-size"],
    );
    css.push_str(&format!("font-family: {};", family_list(&font.font_family)));
    if !font.shadow {
        css.push_str("text-shadow: none;");
    }
    if font.outline.is_none() {
        css.push_str("-webkit-text-stroke: 0;");
    }
    css
}

/// A size as a bare number, for comparing one against another.
///
/// Only ever used as the two halves of a ratio, so the unit cancels — which is
/// why mixing units is answered with nothing rather than with a conversion
/// that would need to know how large a viewer's screen is.
fn relative_size(size: &CssSize) -> Option<(f32, &'static str)> {
    match size {
        CssSize::Px(value) => Some((*value, "px")),
        CssSize::Pt(value) => Some((*value, "pt")),
        CssSize::Em(value) => Some((*value, "em")),
        CssSize::Percentage(value) => Some((*value, "%")),
        CssSize::Null => None,
    }
}

/// How large one size is against another, as a fraction.
///
/// Falls back to "the same" when the two cannot be compared — two sizes in
/// different units, or one the design never set. Showing a spoiler at the size
/// of the words is wrong, but it is a great deal less wrong than showing it at
/// some number arrived at by guessing what a point is worth on a phone.
fn scale_against(size: &CssSize, against: Option<(f32, &'static str)>) -> f32 {
    let (Some((value, unit)), Some((base, base_unit))) = (relative_size(size), against) else {
        return 1.0;
    };
    if unit != base_unit || base <= 0.0 || value <= 0.0 {
        return 1.0;
    }
    // Kept within reason: a design with an absurd ratio should not be able to
    // make a line invisible or push it off the screen.
    (value / base).clamp(0.2, 3.0)
}

/// Drops whole declarations from a run of CSS.
///
/// The design's font CSS is taken as it is and two properties are removed
/// rather than the rest being listed here: listing them would mean a shadow,
/// an outline or anything else a design grows would stop at the projection and
/// never reach a viewer, which is the bug this exists to prevent.
fn without_properties(css: &str, unwanted: &[&str]) -> String {
    css.split(';')
        .map(str::trim)
        .filter(|declaration| !declaration.is_empty())
        .filter(|declaration| {
            let property = declaration
                .split(':')
                .next()
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            !unwanted.contains(&property.as_str())
        })
        .map(|declaration| format!("{declaration};"))
        .collect()
}

/// The lines of a piece of text, without the blank ones.
fn lines_of(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    reason = "these design structs keep private fields, so `..Default::default()`               is not available outside the module that defines them"
)]
mod tests {
    use super::*;
    use crate::logic::sourcefiles::{SourceFile, SourceFileType};
    use crate::logic::states::SlideChapter;
    use std::path::PathBuf;

    fn lyrics_row(lines: &[&str]) -> StreamRow {
        StreamRow::Lyrics {
            lines: lines.iter().map(|line| line.to_string()).collect(),
        }
    }

    /// The words of a slide, with any staves left out.
    fn only_lyrics(rows: &[StreamRow]) -> Vec<String> {
        rows.iter()
            .filter_map(|row| match row {
                StreamRow::Lyrics { lines } => Some(lines.clone()),
                StreamRow::Notation { .. } => None,
            })
            .flatten()
            .collect()
    }

    fn chapter(name: &str, slides: Vec<Slide>) -> SlideChapter {
        SlideChapter::new(
            slides,
            SourceFile {
                name: name.to_string(),
                path: PathBuf::from(format!("{name}.song")),
                file_type: SourceFileType::Song,
                md5_hash: None,
                relative_path: None,
            },
            None,
            None,
        )
    }

    /// A viewer is sent the whole service, not only the slide that is up. The
    /// stage view and the running-order sheet are reads of this same state,
    /// and neither should need the program to send anything else.
    #[test]
    fn a_viewer_is_sent_the_whole_running_order() {
        let presentation = RunningPresentation::new(vec![
            chapter(
                "Amazing Grace",
                vec![
                    Slide::new_title_slide("Amazing Grace".to_string(), None),
                    Slide::new_content_slide("Amazing grace\nhow sweet".to_string(), None, None),
                ],
            ),
            chapter("Handout", vec![Slide::new_pdf_page_slide("h.pdf".into(), 1)]),
        ]);

        let state = StreamState::of(&presentation, 1);

        assert!(state.running);
        assert_eq!(state.chapters.len(), 2);
        assert_eq!(state.chapters[0].title, "Amazing Grace");
        assert_eq!(state.chapters[0].slides.len(), 2);
        assert_eq!(state.chapters[1].slides.len(), 1);
    }

    /// Where a service divides the song differently for the phones, that
    /// division is what travels — and the position that travels with it is the
    /// mapped one, or a viewer would be told to look at a slide that means
    /// something else.
    #[test]
    fn a_stream_with_its_own_division_sends_that_one() {
        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![
                Slide::new_content_slide("one\ntwo".to_string(), None, None),
                Slide::new_content_slide("three\nfour".to_string(), None, None),
            ],
        )]);
        presentation.presentation[0].stream_slides = Some(vec![Slide::new_content_slide(
            "one\ntwo\nthree\nfour".to_string(),
            None,
            None,
        )]);
        presentation.presentation[0].stream_slide_map = vec![0, 0];

        // The wall moves on; the phones stay where they are, because the slide
        // they are showing already holds what the wall has just put up.
        presentation.next_slide();
        let state = StreamState::of(&presentation, 1);

        assert_eq!(state.chapters[0].slides.len(), 1, "the phones' division");
        assert_eq!(
            state.position,
            Some(StreamPosition {
                chapter: 0,
                slide: 0
            })
        );
        assert!(matches!(
            state.current_slide(),
            Some(StreamSlide::Text { rows, .. })
                if rows == &[lyrics_row(&["one", "two", "three", "four"])]
        ));
    }

    /// A service that asked for nothing of its own is sent the projection,
    /// exactly as before. This is what most services are, and the cost of the
    /// feature to them has to be nothing at all.
    #[test]
    fn without_a_second_division_the_projection_is_what_travels() {
        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![
                Slide::new_content_slide("one".to_string(), None, None),
                Slide::new_content_slide("two".to_string(), None, None),
            ],
        )]);
        presentation.next_slide();

        let state = StreamState::of(&presentation, 1);

        assert_eq!(state.chapters[0].slides.len(), 2);
        assert_eq!(state.position.map(|position| position.slide), Some(1));
    }

    /// The design a viewer sees is the stream's where the service named one.
    /// Sending the projection's would leave a phone showing forty-point type
    /// over a photograph meant for a wall ten metres away.
    #[test]
    fn a_stream_design_is_what_reaches_a_viewer() {
        use crate::logic::settings::{
            PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate,
        };

        let plain = |background: &str| {
            let mut template = PresentationDesignTemplate::default();
            template
                .set_background_color_from_hex_str(background)
                .expect("a colour");
            PresentationDesign {
                presentation_design_settings: PresentationDesignSettings::Template(template),
                ..PresentationDesign::default()
            }
        };

        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        presentation.presentation[0].presentation_design_option = Some(plain("#102030"));
        presentation.presentation[0].stream_design_option = Some(plain("#fafafa"));

        let sent = StreamState::of(&presentation, 1).design;

        assert_eq!(
            sent.background.to_lowercase(),
            "#fafafa",
            "the phones design, not the walls"
        );
    }

    /// Where the presentation stands has to point *into* what was sent, or a
    /// page cannot tell what is up.
    #[test]
    fn the_position_points_at_the_slide_that_is_up() {
        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![
                Slide::new_content_slide("first".to_string(), None, None),
                Slide::new_content_slide("second".to_string(), None, None),
            ],
        )]);
        presentation.next_slide();

        let state = StreamState::of(&presentation, 1);

        assert_eq!(
            state.position,
            Some(StreamPosition {
                chapter: 0,
                slide: 1
            })
        );
        assert!(matches!(
            state.current_slide(),
            Some(StreamSlide::Text { rows, .. }) if rows == &[lyrics_row(&["second"])]
        ));
    }

    /// The words a slide shows, one entry per line, so the page can break them
    /// where the slide does instead of guessing.
    #[test]
    fn a_slide_is_sent_as_the_lines_it_shows() {
        let slide = Slide::new_content_slide(
            "Amazing grace\nhow sweet the sound".to_string(),
            Some("that saved a wretch".to_string()),
            None,
        );

        let StreamSlide::Text { rows, spoiler, .. } = StreamSlide::of(&slide) else {
            panic!("a content slide is text");
        };
        assert_eq!(rows, vec![lyrics_row(&["Amazing grace", "how sweet the sound"])]);
        assert_eq!(spoiler, vec!["that saved a wretch"]);
    }

    /// Blank lines are the slide's spacing, not something to read aloud.
    #[test]
    fn blank_lines_are_left_out() {
        let slide = Slide::new_content_slide("first\n\n   \nsecond".to_string(), None, None);

        let StreamSlide::Text { rows, .. } = StreamSlide::of(&slide) else {
            panic!("a content slide is text");
        };
        assert_eq!(rows, vec![lyrics_row(&["first", "second"])]);
    }

    /// A markdown slide reaches the presentation as rendered HTML. A phone
    /// wants the words, not the markup.
    #[test]
    fn markdown_reaches_a_viewer_as_words() {
        let slide = Slide::new_content_slide(
            format!(
                "{}<h1>Vorbereitende Notizen</h1><p>Aktueller Text</p>",
                crate::logic::presentation::MARKDOWN_HTML_PREFIX
            ),
            None,
            None,
        );

        let StreamSlide::Text { rows, .. } = StreamSlide::of(&slide) else {
            panic!("a markdown slide is text");
        };
        let lines = only_lyrics(&rows);
        assert_eq!(lines, vec!["Vorbereitende Notizen", "Aktueller Text"]);
        assert!(
            !lines.iter().any(|line| line.contains('<')),
            "no markup reaches the page"
        );
    }

    /// The melody reaches a viewer. It used to be dropped, and a notation slide
    /// arrived as nothing but its words — the one thing on it that the
    /// projection does *not* set as words, because the notation prints them
    /// under its own notes.
    #[test]
    fn the_notes_are_sent_and_the_words_under_them_are_not_repeated() {
        use cantara_songlib::slides::{ComplexSlide, SlideRow};

        let slide = Slide {
            slide_content: SlideContent::Complex(ComplexSlide {
                rows: vec![
                    SlideRow::notation("X:1\nM:4/4\nK:G\nGABc|\nw:Sei nicht stolz", 4),
                    SlideRow::lyrics(None, "Sei nicht stolz").also_shown_in_notation(),
                    SlideRow::lyrics(Some("fr".to_string()), "Ne sois pas fier"),
                ],
                spoiler: vec![SlideRow::lyrics(None, "Denn wer sich rühmen will,")],
                meta_text: Some("Jan Martin Reckel".to_string()),
                line_count: 1,
            }),
            ..Slide::new_content_slide(String::new(), None, None)
        };

        let StreamSlide::Text { rows, spoiler, meta } = StreamSlide::of(&slide) else {
            panic!("a complex slide is text");
        };

        assert_eq!(
            rows,
            vec![
                StreamRow::Notation {
                    abc: "X:1\nM:4/4\nK:G\nGABc|\nw:Sei nicht stolz".to_string()
                },
                lyrics_row(&["Ne sois pas fier"]),
            ],
            "the staff is sent, and the words it already prints are not sent again"
        );
        assert_eq!(spoiler, vec!["Denn wer sich rühmen will,"]);
        assert_eq!(meta.as_deref(), Some("Jan Martin Reckel"));
    }

    /// The order the user configured is the order a viewer sees: a staff asked
    /// for between two languages belongs between them, not above both.
    #[test]
    fn a_staff_stays_where_the_design_put_it() {
        use cantara_songlib::slides::{ComplexSlide, SlideRow};

        let slide = Slide {
            slide_content: SlideContent::Complex(ComplexSlide {
                rows: vec![
                    SlideRow::lyrics(Some("en".to_string()), "Amazing grace"),
                    SlideRow::notation("X:1\nK:C\nCDEF|", 4),
                    SlideRow::lyrics(Some("de".to_string()), "Erstaunliche Gnade"),
                ],
                spoiler: vec![],
                meta_text: None,
                line_count: 1,
            }),
            ..Slide::new_content_slide(String::new(), None, None)
        };

        let StreamSlide::Text { rows, .. } = StreamSlide::of(&slide) else {
            panic!("a complex slide is text");
        };

        assert_eq!(rows.len(), 3);
        assert!(matches!(rows[1], StreamRow::Notation { .. }), "got: {rows:?}");
    }

    /// The staff a viewer's browser engraves has to be engraved with the same
    /// numbers as the one on the wall, or the two are different arrangements of
    /// the same tune.
    #[test]
    fn a_viewer_engraves_with_the_designs_own_numbers() {
        use crate::logic::settings::{
            PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate,
        };

        let mut template = PresentationDesignTemplate::default();
        template.notation.staff_line_height = 1.5;
        template.notation.width_percent = 80.0;
        let design = PresentationDesign {
            presentation_design_settings: PresentationDesignSettings::Template(template),
            ..PresentationDesign::default()
        };
        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        presentation.presentation[0].presentation_design_option = Some(design);

        let notation = StreamState::of(&presentation, 1).design.notation;

        assert!(!notation.vocal_font.is_empty(), "the words under the notes have a size");
        assert_eq!(
            notation.staff_separation,
            Some(69.0),
            "one and a half times the engraver's own spacing"
        );
        assert!(notation.block_css.contains("80%"), "got: {}", notation.block_css);
    }

    /// A design that leaves the spacing alone says nothing about it, so the
    /// page engraves exactly as the engraver would on its own.
    #[test]
    fn the_engravers_own_spacing_is_not_overridden() {
        let presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);

        assert_eq!(
            StreamState::of(&presentation, 1).design.notation.staff_separation,
            None
        );
    }

    /// Each block wears the design's face for *its* role. Sending only the main
    /// text's family meant a design that sets the spoiler in a different face
    /// was not the design a viewer saw.
    #[test]
    fn every_block_keeps_its_own_face() {
        use crate::logic::css::CssFontFamily;
        use crate::logic::settings::{
            FontRepresentation, PresentationDesign, PresentationDesignSettings,
            PresentationDesignTemplate,
        };

        let mut main = FontRepresentation::default();
        main.font_family = Some(CssFontFamily::with_family("Cormorant".to_string()));
        let mut spoiler = FontRepresentation::default();
        spoiler.font_family = Some(CssFontFamily::with_family("Courier Prime".to_string()));

        let mut template = PresentationDesignTemplate::default();
        template.fonts = vec![main, spoiler];
        template.spoiler_index = Some(1);
        let design = PresentationDesign {
            presentation_design_settings: PresentationDesignSettings::Template(template),
            ..PresentationDesign::default()
        };
        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        presentation.presentation[0].presentation_design_option = Some(design);

        let sent = StreamState::of(&presentation, 1).design;

        assert!(sent.text_css.contains("Cormorant"), "got: {}", sent.text_css);
        assert!(
            sent.spoiler_css.contains("Courier Prime"),
            "got: {}",
            sent.spoiler_css
        );
        assert!(
            sent.spoiler_css.contains("sans-serif"),
            "and a face every device has behind it, got: {}",
            sent.spoiler_css
        );
        assert!(
            !sent.spoiler_css.contains("font-size"),
            "the size is still the page's, got: {}",
            sent.spoiler_css
        );
    }

    /// A picture is named rather than sent: the page fetches it from the
    /// server, so it does not travel again with every slide change.
    #[test]
    fn a_pdf_page_is_named_not_sent() {
        let slide = Slide::new_pdf_page_slide("handout.pdf".to_string(), 3);

        let StreamSlide::Picture { media } = StreamSlide::of(&slide) else {
            panic!("a PDF page is a picture");
        };
        assert_eq!(media, media_id("handout.pdf#page=3"));
        assert_ne!(media, media_id("handout.pdf#page=4"));
    }

    /// The same picture in two places in a service is one picture, so a viewer
    /// fetches it once.
    #[test]
    fn a_picture_used_twice_is_named_once() {
        let presentation = RunningPresentation::new(vec![chapter(
            "Handout",
            vec![
                Slide::new_pdf_page_slide("h.pdf".to_string(), 1),
                Slide::new_content_slide("a reading".to_string(), None, None),
                Slide::new_pdf_page_slide("h.pdf".to_string(), 1),
                Slide::new_pdf_page_slide("h.pdf".to_string(), 2),
            ],
        )]);

        let state = StreamState::of(&presentation, 1);

        assert_eq!(state.media().len(), 2, "two distinct pages, named once each");
    }

    /// Blanking the projection blanks every phone with it — otherwise the
    /// words stay up in the room after the moderator has taken them off the
    /// wall.
    #[test]
    fn a_blank_screen_reaches_the_viewers() {
        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        presentation.toggle_black_screen();

        assert!(StreamState::of(&presentation, 1).blacked_out);
    }

    /// Between presentations the address stays open and says so, so it can be
    /// handed out and tried before anything begins.
    #[test]
    fn nothing_running_is_something_to_say() {
        let waiting = StreamState::waiting(7);

        assert!(!waiting.running);
        assert_eq!(waiting.revision, 7);
        assert!(waiting.chapters.is_empty());
        assert_eq!(waiting.current_slide(), None);
    }

    /// The page is written against this shape; it has to survive the trip.
    #[test]
    fn the_state_survives_being_sent() {
        let presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        let state = StreamState::of(&presentation, 3);

        let json = serde_json::to_string(&state).expect("serialises");
        let back: StreamState = serde_json::from_str(&json).expect("and comes back");

        assert_eq!(back, state);
        // The page switches on this, so the name matters.
        assert!(json.contains(r#""kind":"text""#), "got: {json}");
    }

    /// A design's background picture reaches a viewer too. Without it the page
    /// is the right colour and the wrong presentation — and it has to be in
    /// the list of what to prepare, or it is named and never sent.
    #[test]
    fn a_design_background_is_named_and_asked_for() {
        use crate::logic::settings::{
            PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate,
        };
        use crate::logic::sourcefiles::{ImageSourceFile, SourceFile, SourceFileType};

        let picture = ImageSourceFile::new(SourceFile {
            name: "sunrise".to_string(),
            path: PathBuf::from("sunrise.png"),
            file_type: SourceFileType::Image,
            md5_hash: None,
            relative_path: None,
        })
        .expect("a picture is a picture");

        let mut template = PresentationDesignTemplate::default();
        template.background_image = Some(picture);
        template.background_transparency = 40;
        let design = PresentationDesign {
            presentation_design_settings: PresentationDesignSettings::Template(template),
            ..PresentationDesign::default()
        };

        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        presentation.presentation[0].presentation_design_option = Some(design);

        let state = StreamState::of(&presentation, 1);

        assert_eq!(
            state.design.background_image.as_deref(),
            Some(media_id("sunrise.png").as_str())
        );
        assert_eq!(state.design.background_transparency, 40);
        assert!(
            state.media().contains(&media_id("sunrise.png")),
            "the background is among the pictures to prepare"
        );
    }

    /// A design that is only a colour names no picture, so the page does not
    /// go asking for one that will never exist.
    #[test]
    fn a_plain_design_names_no_background_picture() {
        let presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);

        let state = StreamState::of(&presentation, 1);

        assert_eq!(state.design.background_image, None);
        assert!(state.media().is_empty());
    }

    /// A design's shadow, outline, weight and alignment reach a viewer. They
    /// are what makes light text readable over a background picture, and a
    /// page without them is not the presentation the audience is looking at.
    #[test]
    fn a_design_sends_more_than_a_colour() {
        use crate::logic::settings::{
            FontOutline, FontRepresentation, PresentationDesign, PresentationDesignSettings,
            PresentationDesignTemplate,
        };

        let mut font = FontRepresentation::default();
        font.shadow = true;
        font.weight = 800;
        font.outline = Some(FontOutline::default());

        let mut template = PresentationDesignTemplate::default();
        template.fonts = vec![font];
        let design = PresentationDesign {
            presentation_design_settings: PresentationDesignSettings::Template(template),
            ..PresentationDesign::default()
        };

        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        presentation.presentation[0].presentation_design_option = Some(design);

        let css = StreamState::of(&presentation, 1).design.text_css;

        assert!(css.contains("text-shadow"), "got: {css}");
        assert!(css.contains("font-weight"), "got: {css}");
        assert!(css.contains("text-stroke"), "the outline too, got: {css}");
    }

    /// The size and the family stay behind: a phone is a fifth of the size of a
    /// projection and has never heard of the font on the presenting machine.
    #[test]
    fn the_size_and_the_family_are_the_pages_to_decide() {
        let presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);

        let design = StreamState::of(&presentation, 1).design;

        assert!(!design.text_css.contains("font-size"), "got: {}", design.text_css);
        // The face is sent, with something every device has behind it.
        assert!(design.text_css.contains("sans-serif"), "got: {}", design.text_css);
        assert!(design.font_family.contains("sans-serif"));
    }

    /// Dropping a property must not take its neighbours with it, nor be fooled
    /// by a property whose name merely starts the same way.
    #[test]
    fn dropping_a_property_leaves_the_rest_alone() {
        let css = "font-size: 12pt;font-weight: 700;font-family: Georgia;text-shadow: 1px 1px 2px black;";

        let kept = without_properties(css, &["font-family", "font-size"]);

        assert_eq!(kept, "font-weight: 700;text-shadow: 1px 1px 2px black;");
    }

    /// The design gives the lines shown ahead a font of their own. A page that
    /// dresses them like the words being sung is not showing the design — and
    /// the *relationship* between the two sizes is what has to survive, since
    /// the sizes themselves mean nothing on a phone.
    #[test]
    fn a_spoiler_keeps_its_own_look_and_its_proportion() {
        use crate::logic::settings::{
            FontRepresentation, PresentationDesign, PresentationDesignSettings,
            PresentationDesignTemplate,
        };

        let mut main = FontRepresentation::default();
        main.font_size = CssSize::Pt(60.0);
        let mut spoiler = FontRepresentation::default();
        spoiler.font_size = CssSize::Pt(30.0);
        spoiler.shadow = false;

        let mut template = PresentationDesignTemplate::default();
        template.fonts = vec![main, spoiler];
        template.spoiler_index = Some(1);

        let design = PresentationDesign {
            presentation_design_settings: PresentationDesignSettings::Template(template),
            ..PresentationDesign::default()
        };
        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        presentation.presentation[0].presentation_design_option = Some(design);

        let sent = StreamState::of(&presentation, 1).design;

        assert_eq!(sent.spoiler_scale, 0.5, "half the size, as the design says");
        assert!(!sent.spoiler_css.is_empty(), "and dressed in its own right");
    }

    /// Sizes that cannot be compared are not guessed at: a ratio between a
    /// point and an em would need to know how large the viewer's screen is.
    #[test]
    fn sizes_that_cannot_be_compared_are_left_alone() {
        assert_eq!(scale_against(&CssSize::Em(2.0), relative_size(&CssSize::Pt(60.0))), 1.0);
        assert_eq!(scale_against(&CssSize::Null, relative_size(&CssSize::Pt(60.0))), 1.0);
        assert_eq!(scale_against(&CssSize::Pt(30.0), None), 1.0);
    }

    /// A design cannot make a line invisible or push it off the screen.
    #[test]
    fn an_absurd_proportion_is_reined_in() {
        let base = relative_size(&CssSize::Pt(10.0));

        assert_eq!(scale_against(&CssSize::Pt(1.0), base), 0.2);
        assert_eq!(scale_against(&CssSize::Pt(1000.0), base), 3.0);
    }

    /// The gap a design puts between the two blocks of a slide travels with
    /// it, so the page does not invent a spacing of its own.
    #[test]
    fn the_gap_between_the_blocks_is_the_designs() {
        use crate::logic::settings::{
            PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate,
        };

        let mut template = PresentationDesignTemplate::default();
        template.main_content_spoiler_content_padding = CssSize::Em(4.0);
        let design = PresentationDesign {
            presentation_design_settings: PresentationDesignSettings::Template(template),
            ..PresentationDesign::default()
        };
        let mut presentation = RunningPresentation::new(vec![chapter(
            "Amazing Grace",
            vec![Slide::new_content_slide("Amazing grace".to_string(), None, None)],
        )]);
        presentation.presentation[0].presentation_design_option = Some(design);

        assert_eq!(StreamState::of(&presentation, 1).design.block_gap, "4em");
    }

    /// A block the design gave no shadow must not inherit one from the block
    /// above it. Everything a slide shows sits inside the element carrying the
    /// main text's style, and `text-shadow` inherits.
    #[test]
    fn a_block_without_a_shadow_says_so() {
        use crate::logic::settings::FontRepresentation;

        let plain = FontRepresentation::default();
        assert!(!plain.shadow, "the fixture has no shadow to begin with");

        let css = font_css(&plain);

        assert!(css.contains("text-shadow: none"), "got: {css}");
        assert!(css.contains("-webkit-text-stroke: 0"), "got: {css}");
    }
}
