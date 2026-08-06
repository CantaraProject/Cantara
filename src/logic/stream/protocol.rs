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

use crate::components::presentation_components::meta_text_of;
use crate::logic::presentation::{get_markdown_html, get_picture_path, html_to_plain_text};
use crate::logic::css::CssString;
use crate::logic::settings::{PresentationDesign, PresentationDesignSettings};
use crate::logic::states::RunningPresentation;
use cantara_songlib::slides::{Slide, SlideContent};

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
    /// Words to read or sing. One entry per line, so the page can break them
    /// where the slide does rather than guessing.
    Text {
        lines: Vec<String>,
        /// The next lines, where the design shows them ahead.
        spoiler: Vec<String>,
        meta: Option<String>,
    },
    /// A picture, or a page of a PDF — anything that has to be *seen* rather
    /// than read. `media` names it in [`crate::logic::stream`]'s store; the
    /// page fetches it from the server rather than having it pushed through
    /// every update.
    Picture { media: String },
    /// Deliberately nothing: the gap between two elements of a service.
    Empty,
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
}

impl Default for StreamDesign {
    fn default() -> Self {
        StreamDesign {
            background: "#000000".to_string(),
            foreground: "#ffffff".to_string(),
            font_family: "system-ui, sans-serif".to_string(),
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
    pub fn of(presentation: &RunningPresentation, revision: u64) -> Self {
        let design = StreamDesign::of(&presentation.get_current_presentation_design());

        let chapters = presentation
            .presentation
            .iter()
            .map(|chapter| StreamChapter {
                title: chapter.source_file.name.clone(),
                slides: chapter.slides.iter().map(StreamSlide::of).collect(),
            })
            .collect();

        let position = presentation
            .position
            .as_ref()
            .map(|position| StreamPosition {
                chapter: position.chapter(),
                slide: position.chapter_slide(),
            });

        StreamState {
            revision,
            running: true,
            design,
            chapters,
            position,
            blacked_out: presentation.is_black_screen,
        }
    }

    /// The slide that is up, if there is one.
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
    /// asking for it gets anything.
    pub fn media(&self) -> Vec<String> {
        let mut named: Vec<String> = Vec::new();
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
}

impl StreamDesign {
    fn of(design: &PresentationDesign) -> Self {
        let PresentationDesignSettings::Template(template) = &design.presentation_design_settings
        else {
            // A hand-written HTML design cannot be reduced to three values, so
            // the page falls back to something readable rather than guessing.
            return StreamDesign::default();
        };

        let font = template.fonts.first().cloned().unwrap_or_default();
        StreamDesign {
            background: template.get_background_color_as_hex_string(),
            foreground: format!(
                "#{:02x}{:02x}{:02x}",
                font.color.r, font.color.g, font.color.b
            ),
            // A whole family list: the design's face is a file on the
            // presenting machine and a viewer's browser has never heard of it,
            // so what reaches the page has to end in something every device
            // has.
            font_family: match &font.font_family {
                Some(family) => format!("{}, system-ui, sans-serif", family.to_css_string()),
                None => "system-ui, sans-serif".to_string(),
            },
        }
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
                    lines: lines_of(&text),
                    spoiler: main
                        .clone()
                        .spoiler_text()
                        .map(|spoiler| lines_of(&spoiler))
                        .unwrap_or_default(),
                    meta: non_empty(meta_text_of(&slide.slide_content)),
                }
            }

            SlideContent::MultiLanguageMainContent(multi) => StreamSlide::Text {
                lines: multi
                    .main_text_list
                    .iter()
                    .flat_map(|text| lines_of(text))
                    .collect(),
                spoiler: multi
                    .spoiler_text_vector
                    .iter()
                    .flat_map(|text| lines_of(text))
                    .collect(),
                meta: non_empty(multi.meta_text.clone()),
            },

            // The notation of a complex slide is ABC source, which is no use to
            // a reader; the lyrics under it are exactly what a phone wants.
            SlideContent::Complex(complex) => StreamSlide::Text {
                lines: complex
                    .rows
                    .iter()
                    .filter(|row| !row.is_notation())
                    .flat_map(|row| lines_of(&row.content))
                    .collect(),
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

            SlideContent::Empty(_) => StreamSlide::Empty,
        }
    }
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
mod tests {
    use super::*;
    use crate::logic::sourcefiles::{SourceFile, SourceFileType};
    use crate::logic::states::SlideChapter;
    use std::path::PathBuf;

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
            Some(StreamSlide::Text { lines, .. }) if lines == &["second"]
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

        let StreamSlide::Text { lines, spoiler, .. } = StreamSlide::of(&slide) else {
            panic!("a content slide is text");
        };
        assert_eq!(lines, vec!["Amazing grace", "how sweet the sound"]);
        assert_eq!(spoiler, vec!["that saved a wretch"]);
    }

    /// Blank lines are the slide's spacing, not something to read aloud.
    #[test]
    fn blank_lines_are_left_out() {
        let slide = Slide::new_content_slide("first\n\n   \nsecond".to_string(), None, None);

        let StreamSlide::Text { lines, .. } = StreamSlide::of(&slide) else {
            panic!("a content slide is text");
        };
        assert_eq!(lines, vec!["first", "second"]);
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

        let StreamSlide::Text { lines, .. } = StreamSlide::of(&slide) else {
            panic!("a markdown slide is text");
        };
        assert_eq!(lines, vec!["Vorbereitende Notizen", "Aktueller Text"]);
        assert!(
            !lines.iter().any(|line| line.contains('<')),
            "no markup reaches the page"
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
}
