//! Runtime state and presentation navigation types used by UI components.
//!
//! This module intentionally contains only in-memory state representations and
//! navigation behavior. Persistent application configuration is implemented in
//! [`crate::logic::settings`].

use dioxus::prelude::Signal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    settings::{PresentationDesign, SelectionSidebarType, SlideTimerSettings, SlideTransition},
    sourcefiles::SourceFile,
};
use cantara_songlib::slides::{Slide, SlideSettings};

#[derive(Clone)]
pub struct RuntimeInformation {
    pub language: String,
}

/// Whether the web build's one-time redirect to the detail view has already
/// happened.
///
/// This has to live in a context provided by `App`, which stays mounted for
/// the program's lifetime. A `Signal` owned by the selection view itself would
/// be recreated (and reset to `false`) every time that view mounts, which is
/// exactly what happens when the footer button navigates back to it — the
/// redirect would fire again immediately and undo the navigation.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub struct InitialRouteState {
    pub redirected_to_detail: Signal<bool>,
}

/// Which kind of element the library list is showing.
///
/// Held by `App` rather than by the two views that draw the list, for two
/// reasons. The selection view and the detail view show the *same* list, and a
/// user who was looking through the PDFs in one of them is still looking
/// through the PDFs after switching to the other — a signal owned by a view
/// would start over at whatever it was initialised with every time that view
/// mounts. And the detail view mounts constantly: opening an element writes
/// the element's identifier into the address, which is a route change, which
/// re-creates the view. That is what threw the list back to the songs the
/// moment a picture or a PDF was opened.
#[derive(Clone, Copy)]
pub struct LibraryFilterState {
    pub active: Signal<SelectionSidebarType>,
}

/// The kind of element the library list starts on: whichever the user has put
/// at the top of the sidebar.
///
/// The sidebar can be reordered by dragging, and the top button is what a user
/// means by "the one I work with" — starting on the songs regardless was only
/// ever the order the buttons happened to be declared in.
pub fn first_sidebar_type(order: &[SelectionSidebarType]) -> SelectionSidebarType {
    order
        .first()
        .copied()
        .or_else(|| crate::logic::settings::default_sidebar_order().first().copied())
        .unwrap_or(SelectionSidebarType::Songs)
}

/// This struct represents a selected item
#[derive(Clone, PartialEq, Debug)]
pub struct SelectedItemRepresentation {
    /// The source file of the selected item
    pub source_file: SourceFile,

    /// The [PresentationDesignSettings] as an option. If [None], the default [PresentationDesign] will be used.
    pub presentation_design_option: Option<PresentationDesign>,

    /// The [PresentationDesign] as an option. If [None], the default [PresentationDesign] will be used.
    pub slide_settings_option: Option<SlideSettings>,

    /// The design the network stream shows this element in, where it is not
    /// the one on the wall. [None] falls back to the service's general choice,
    /// and that in turn to the projection's own — a phone showing the same
    /// thing as the projector is the ordinary case and costs nothing.
    pub stream_design_option: Option<PresentationDesign>,

    /// The same, for how this element is divided into slides on a phone. A
    /// congregation reading from their own screens can be given the whole
    /// verse while the wall goes two lines at a time.
    pub stream_slide_settings_option: Option<SlideSettings>,

    /// Optional inline markdown content for spontaneous markdown text.
    /// When set, this content is used instead of reading from the source file path.
    pub inline_markdown: Option<String>,

    /// Optional timer settings for automatic slide advance. If [None], no timer is used.
    pub timer_settings_option: Option<SlideTimerSettings>,

    /// The transition effect for this selection. Uses the default (Fade) when not set.
    pub transition_effect: SlideTransition,
}

impl SelectedItemRepresentation {
    pub fn new_with_sourcefile(source_file: SourceFile) -> Self {
        SelectedItemRepresentation {
            source_file,
            presentation_design_option: None,
            slide_settings_option: None,
            stream_design_option: None,
            stream_slide_settings_option: None,
            inline_markdown: None,
            timer_settings_option: None,
            transition_effect: SlideTransition::default(),
        }
    }
}

/// A running presentation that holds all state needed to display and navigate slides.
///
/// This struct is shared between the presentation window and the presenter console
/// via a `Signal<Vec<RunningPresentation>>` context. On desktop, each window runs
/// a separate VirtualDom, so changes are synchronized via a polling loop (see
/// `PresentationPage` and `PresenterConsolePage`).
///
/// ## Scroll position and `eq_ignoring_scroll`
///
/// The `markdown_scroll_position` field is synced separately by `MarkdownSlideComponent`
/// using its own dedicated polling loop. To prevent scroll updates from triggering
/// full component re-renders or interfering with slide navigation, the cross-window
/// sync loops compare presentations using [`eq_ignoring_scroll`](Self::eq_ignoring_scroll)
/// rather than the derived `PartialEq`. Slide navigation methods (`next_slide`,
/// `previous_slide`, `jump_to`) automatically reset the scroll position to 0.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RunningPresentation {
    pub presentation: Vec<SlideChapter>,
    pub position: Option<RunningPresentationPosition>,
    /// Whether the presentation is currently showing a black screen
    pub is_black_screen: bool,
    /// The resolution of the presentation screen in pixels (width, height).
    /// Defaults to 1920x1080 (16:9) when no monitor info is available.
    #[serde(default = "default_presentation_resolution")]
    pub presentation_resolution: (u32, u32),
    /// The current DOM `scrollTop` value for markdown slides, synchronized between
    /// the presentation window and the presenter console preview. This field is
    /// excluded from [`eq_ignoring_scroll`](Self::eq_ignoring_scroll) comparisons
    /// and is synced by a dedicated polling loop in `MarkdownSlideComponent`.
    #[serde(default)]
    pub markdown_scroll_position: f64,
    /// The size the presentation is actually laid out at, in CSS pixels, as
    /// the presentation window measures it.
    ///
    /// Not the same as [`presentation_resolution`](Self::presentation_resolution),
    /// which is the monitor in *physical* pixels: a screen at 150% scaling
    /// lays a window out at two thirds of that. The console's preview has to
    /// use this number, or its text breaks in different places from the screen
    /// the audience is looking at — and a preview that breaks its lines
    /// somewhere else is not a preview.
    ///
    /// `None` until the presentation window has measured itself; the monitor's
    /// size stands in until then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_layout: Option<(f64, f64)>,
}

impl RunningPresentation {
    /// Helper function to create a new [RunningPresentation] data structure
    pub fn new(presentation: Vec<SlideChapter>) -> Self {
        RunningPresentation {
            presentation: presentation.clone(),
            position: RunningPresentationPosition::new(&presentation),
            is_black_screen: false,
            presentation_resolution: default_presentation_resolution(),
            markdown_scroll_position: 0.0,
            presentation_layout: None,
        }
    }

    /// The size a slide is laid out at, which is what anything showing the
    /// same slide beside it has to use.
    pub fn layout_size(&self) -> (f64, f64) {
        self.presentation_layout.unwrap_or((
            self.presentation_resolution.0 as f64,
            self.presentation_resolution.1 as f64,
        ))
    }

    /// Go to the next slide (if any exists).
    /// Resets `markdown_scroll_position` to 0 so the new slide starts at the top.
    pub fn next_slide(&mut self) {
        if let Some(ref mut pos) = self.position
            && pos.try_next(&self.presentation).is_ok() {
                self.markdown_scroll_position = 0.0;
            }
    }

    /// Go to the previous slide (if any exists).
    /// Resets `markdown_scroll_position` to 0 so the new slide starts at the top.
    pub fn previous_slide(&mut self) {
        if let Some(ref mut pos) = self.position
            && pos.try_back(&self.presentation).is_ok() {
                self.markdown_scroll_position = 0.0;
            }
    }

    /// Jump to a specific chapter and slide position.
    /// Resets `markdown_scroll_position` to 0 so the new slide starts at the top.
    pub fn jump_to(&mut self, chapter: usize, slide: usize) {
        if chapter < self.presentation.len() {
            let chapter_slides = &self.presentation[chapter].slides;
            if slide < chapter_slides.len() {
                // Calculate the total slide number
                let mut total: usize = 0;
                for i in 0..chapter {
                    total += self.presentation[i].slides.len();
                }
                total += slide;

                self.position = Some(RunningPresentationPosition {
                    chapter,
                    chapter_slide: slide,
                    slide_total: total,
                });
                self.markdown_scroll_position = 0.0;
            }
        }
    }


    /// Returns the total number of slides across all chapters
    pub fn total_slides(&self) -> usize {
        self.presentation.iter().map(|ch| ch.slides.len()).sum()
    }

    /// Toggle the black screen state
    pub fn toggle_black_screen(&mut self) {
        self.is_black_screen = !self.is_black_screen;
    }

    pub fn get_current_slide(&self) -> Option<Slide> {
        self.position.as_ref().and_then(|pos| {
            self.presentation
                .get(pos.chapter())?
                .slides
                .get(pos.chapter_slide())
                .cloned()
        })
    }

    pub fn get_current_presentation_design(&self) -> PresentationDesign {
        match self.position.as_ref() {
            Some(pos) => self
                .presentation
                .get(pos.chapter())
                .and_then(|ch| ch.presentation_design_option.clone())
                .unwrap_or_default(),
            None => PresentationDesign::default(),
        }
    }

    /// The design a viewer on the network sees, for the chapter that is up.
    pub fn get_current_stream_design(&self) -> PresentationDesign {
        match self.position.as_ref() {
            Some(pos) => self
                .presentation
                .get(pos.chapter())
                .and_then(|chapter| chapter.design_for_stream())
                .unwrap_or_default(),
            None => PresentationDesign::default(),
        }
    }

    /// Where a viewer on the network stands, as a chapter and a slide within
    /// it.
    ///
    /// The same place as the projection where the two show the same slides,
    /// and the mapped one where the service asked the stream to divide the
    /// song differently.
    pub fn stream_position(&self) -> Option<(usize, usize)> {
        let position = self.position.as_ref()?;
        let chapter = self.presentation.get(position.chapter())?;
        Some((
            position.chapter(),
            chapter.stream_slide_for(position.chapter_slide()),
        ))
    }

    /// The slide a viewer on the network is looking at.
    ///
    /// What the presenter console previews beside the projection's, so that a
    /// moderator can see both of the things the congregation can see.
    pub fn get_current_stream_slide(&self) -> Option<Slide> {
        let (chapter_index, slide_index) = self.stream_position()?;
        self.presentation
            .get(chapter_index)?
            .slides_for_stream()
            .get(slide_index)
            .cloned()
    }

    /// Whether the chapter that is up shows a viewer on the network something
    /// other than what the projection shows.
    pub fn current_stream_differs(&self) -> bool {
        self.position
            .as_ref()
            .and_then(|position| self.presentation.get(position.chapter()))
            .is_some_and(|chapter| chapter.stream_differs())
    }

    /// Compares two `RunningPresentation` instances for structural equality,
    /// ignoring `markdown_scroll_position`.
    ///
    /// This is the primary comparison used by the cross-window sync polling loops
    /// in `PresentationPage` and `PresenterConsolePage`. It detects meaningful
    /// state changes (slide navigation, black screen toggle, resolution change)
    /// without being triggered by scroll position updates.
    ///
    /// Using the derived `PartialEq` (which includes `markdown_scroll_position`)
    /// for sync would cause scroll position writes from `MarkdownSlideComponent`
    /// to trigger full component re-renders and race with slide navigation,
    /// leading to slide changes being reverted.
    pub fn eq_ignoring_scroll(&self, other: &Self) -> bool {
        self.presentation == other.presentation
            && self.position == other.position
            && self.is_black_screen == other.is_black_screen
            && self.presentation_layout == other.presentation_layout
            && self.presentation_resolution == other.presentation_resolution
    }

    /// Returns the transition for the current chapter.
    pub fn get_current_transition(&self) -> SlideTransition {
        match self.position.clone() {
            Some(pos) => self
                .presentation
                .get(pos.chapter())
                .map(|ch| ch.transition_option)
                .unwrap_or_default(),
            None => SlideTransition::default(),
        }
    }

    /// Returns the timer settings for the current chapter, if any.
    pub fn get_current_timer_settings(&self) -> Option<SlideTimerSettings> {
        match self.position.clone() {
            Some(pos) => self
                .presentation
                .get(pos.chapter())
                .and_then(|ch| ch.timer_settings_option.clone()),
            None => None,
        }
    }

    /// Returns true if the current slide is the last slide in its chapter.
    pub fn is_last_slide_in_chapter(&self) -> bool {
        match self.position.clone() {
            Some(pos) => {
                let chapter_len_opt = self
                    .presentation
                    .get(pos.chapter())
                    .map(|ch| ch.slides.len());

                match chapter_len_opt {
                    // Only consider it the last slide if the chapter exists and has at least one slide.
                    Some(chapter_len) if chapter_len > 0 => {
                        let current_index = pos.chapter_slide();
                        // `current_index` is zero-based; we're on the last slide if it's exactly the last index.
                        current_index + 1 == chapter_len
                    }
                    // Missing or empty chapter, or any other invalid state: not the last slide.
                    _ => false,
                }
            }
            None => false,
        }
    }

    /// Restart the current chapter from its first slide.
    pub fn restart_current_chapter(&mut self) {
        if let Some(ref pos) = self.position {
            let chapter = pos.chapter();
            self.jump_to(chapter, 0);
        }
    }
}

/// This represents a position in a running presentation.
/// This struct should always be save in that sense that the presentation does exist.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct RunningPresentationPosition {
    /// The number of the current chapter
    chapter: usize,

    /// The number of the current slide of the chapter
    chapter_slide: usize,

    /// The total slide number of all chapters
    slide_total: usize,
}

impl RunningPresentationPosition {
    /// Creates a position from raw values. Used when restoring position
    /// after a presentation update.
    pub fn from_raw(chapter: usize, chapter_slide: usize, slide_total: usize) -> Self {
        RunningPresentationPosition {
            chapter,
            chapter_slide,
            slide_total,
        }
    }

    /// Creates a new position if there is at least one slide available
    pub fn new(presentation: &[SlideChapter]) -> Option<Self> {
        let has_first_slide = presentation
            .first()
            .is_some_and(|chapter| !chapter.slides.is_empty());

        if has_first_slide {
            Some(RunningPresentationPosition {
                chapter: 0,
                chapter_slide: 0,
                slide_total: 0,
            })
        } else {
            None
        }
    }

    /// Tries to go to the next position if it exists (and returns okay),
    /// if the next position does not exist, an error will be returned.
    pub fn try_next(&mut self, presentation: &[SlideChapter]) -> Result<(), ()> {
        let chapter_len = self.cur_chapter_slide_length(presentation);
        if chapter_len > 0 && self.chapter_slide < chapter_len - 1 {
            self.chapter_slide += 1;
            self.slide_total += 1;
            Ok(())
        } else if self.chapter < presentation.len().saturating_sub(1) {
            self.chapter += 1;
            self.chapter_slide = 0;
            self.slide_total += 1;
            Ok(())
        } else {
            Err(())
        }
    }

    /// Tries to go to the next position if it exists (and returns okay),
    /// if the next position does not exist, an error will be returned.
    pub fn try_back(&mut self, presentation: &[SlideChapter]) -> Result<(), ()> {
        if self.chapter_slide > 0 {
            self.chapter_slide -= 1;
            self.slide_total -= 1;
            Ok(())
        } else if self.chapter > 0 {
            self.chapter -= 1;
            self.chapter_slide = self.cur_chapter_slide_length(presentation).saturating_sub(1);
            self.slide_total -= 1;
            Ok(())
        } else {
            Err(())
        }
    }

    /// Helper function for getting the current slide length
    fn cur_chapter_slide_length(&self, presentation: &[SlideChapter]) -> usize {
        presentation
            .get(self.chapter)
            .map(|ch| ch.slides.len())
            .unwrap_or(0)
    }

    /// Get the number of the current chapter
    pub fn chapter(&self) -> usize {
        self.chapter
    }

    /// Get the number of the current slide in the current chapter
    pub fn chapter_slide(&self) -> usize {
        self.chapter_slide
    }

    /// Get the total slide number position
    pub fn slide_total(&self) -> usize {
        self.slide_total
    }
}

/// Contains slide, the source file and the presentation design for each chapter (e.g. a song)
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SlideChapter {
    /// Stable identifier for matching chapters across presentation updates.
    /// Generated once at slide generation time.
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub slides: Vec<Slide>,
    pub source_file: SourceFile,
    pub presentation_design_option: Option<PresentationDesign>,
    pub slide_settings_option: Option<SlideSettings>,

    /// The design the network stream shows this chapter in, where that is not
    /// the projection's. [None] means the phones look like the wall.
    #[serde(default)]
    pub stream_design_option: Option<PresentationDesign>,

    /// A second division of the same song, for the phones.
    ///
    /// [None] — the ordinary case — means the stream shows
    /// [`slides`](Self::slides) itself, and nothing here has to be kept in
    /// step with anything. A second set only exists where the service asked
    /// for one, and then [`stream_slide_map`](Self::stream_slide_map) says
    /// which of these slides each slide of the projection is showing part of.
    #[serde(default)]
    pub stream_slides: Option<Vec<Slide>>,

    /// For every slide of [`slides`](Self::slides), the index into
    /// [`stream_slides`](Self::stream_slides) that shows it.
    ///
    /// Worked out once, when the slides are generated, rather than every time
    /// a viewer is told where things stand: it depends only on the two sets of
    /// slides, and both are fixed for as long as the presentation runs.
    #[serde(default)]
    pub stream_slide_map: Vec<usize>,

    /// Optional timer settings for automatic slide advance.
    #[serde(default)]
    pub timer_settings_option: Option<SlideTimerSettings>,
    /// The transition effect for this chapter.
    #[serde(default)]
    pub transition_option: SlideTransition,
    /// Inline markdown content, if this chapter was created from an inline
    /// (spontaneous) markdown item rather than a file on disk.
    /// Stored here so `update_presentation` can use it as part of the chapter
    /// fingerprint to distinguish two items that share the same `source_file.path`
    /// but have different content (e.g. two inline-text items).
    #[serde(default)]
    pub inline_markdown: Option<String>,
}

impl SlideChapter {
    pub fn new(
        slides: Vec<Slide>,
        source_file: SourceFile,
        presentation_design: Option<PresentationDesign>,
        slide_settings: Option<SlideSettings>,
    ) -> Self {
        SlideChapter {
            id: Uuid::new_v4(),
            slides,
            source_file,
            presentation_design_option: presentation_design,
            slide_settings_option: slide_settings,
            stream_design_option: None,
            stream_slides: None,
            stream_slide_map: Vec::new(),
            timer_settings_option: None,
            transition_option: SlideTransition::default(),
            inline_markdown: None,
        }
    }

    /// The slides the network stream shows for this chapter.
    ///
    /// The projection's own, unless the service asked for a second division.
    pub fn slides_for_stream(&self) -> &[Slide] {
        match &self.stream_slides {
            Some(slides) => slides,
            None => &self.slides,
        }
    }

    /// Which slide the stream is showing while the projection shows `slide`.
    ///
    /// The same index where there is no second division, and the mapped one
    /// where there is. Clamped rather than trusted: a map is generated
    /// alongside the slides, and a presentation restored from an older session
    /// may have one that no longer fits.
    pub fn stream_slide_for(&self, slide: usize) -> usize {
        if self.stream_slides.is_none() {
            return slide;
        }
        let last = self.slides_for_stream().len().saturating_sub(1);
        self.stream_slide_map.get(slide).copied().unwrap_or(0).min(last)
    }

    /// The design the stream shows this chapter in — its own where it has one,
    /// and otherwise the projection's.
    pub fn design_for_stream(&self) -> Option<PresentationDesign> {
        self.stream_design_option
            .clone()
            .or_else(|| self.presentation_design_option.clone())
    }

    /// Whether a viewer is being shown something other than the projection.
    ///
    /// What the presenter console asks before offering a second preview: with
    /// nothing differing there is nothing to preview, and a second picture of
    /// the same slide is just clutter beside the first.
    pub fn stream_differs(&self) -> bool {
        self.stream_slides.is_some() || self.stream_design_option.is_some()
    }
}

fn default_presentation_resolution() -> (u32, u32) {
    (1920, 1080)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_running_presentation_serialization() {
        use crate::logic::sourcefiles::{SourceFile, SourceFileType};
        use cantara_songlib::slides::{Slide, SlideContent, EmptySlide};
        use std::path::PathBuf;

        let source_file = SourceFile {
            name: "Test Song".to_string(),
            path: PathBuf::from("test/path.song"),
            file_type: SourceFileType::Song,
            md5_hash: None,
            relative_path: None,
        };

        let slide = Slide {
            slide_content: SlideContent::Empty(EmptySlide { black_background: false }),
            linked_file: None,
        };

        let chapter = SlideChapter::new(
            vec![slide],
            source_file,
            None,
            None,
        );

        let rp = RunningPresentation::new(vec![chapter]);

        // Serialize to JSON
        let json = serde_json::to_string(&rp).expect("Failed to serialize RunningPresentation");
        assert!(!json.is_empty());

        // Deserialize back
        let rp2: RunningPresentation = serde_json::from_str(&json).expect("Failed to deserialize RunningPresentation");
        assert!(rp == rp2, "Deserialized presentation should match original");
        assert!(rp2.presentation.len() == 1);
        assert!(rp2.presentation[0].source_file.name == "Test Song");
        assert!(rp2.position.is_some());
        assert!(!rp2.is_black_screen);
    }

    /// A presentation of three slides, to move about in.
    fn three_slides() -> RunningPresentation {
        use crate::logic::sourcefiles::{SourceFile, SourceFileType};
        use cantara_songlib::slides::Slide;
        use std::path::PathBuf;

        let source_file = SourceFile {
            name: "Handout".to_string(),
            path: PathBuf::from("handout.pdf"),
            file_type: SourceFileType::Pdf,
            md5_hash: None,
            relative_path: None,
        };
        let slides: Vec<Slide> = (1..=3)
            .map(|page| Slide::new_pdf_page_slide("handout.pdf".to_string(), page))
            .collect();

        RunningPresentation::new(vec![SlideChapter::new(slides, source_file, None, None)])
    }

    /// Moving about a presentation: forwards, straight to a slide, and back.
    #[test]
    fn the_presentation_moves_where_it_is_told() {
        let mut rp = three_slides();
        assert_eq!(rp.position.as_ref().map(|p| p.slide_total()), Some(0));

        rp.next_slide();
        assert_eq!(rp.position.as_ref().map(|p| p.slide_total()), Some(1));

        rp.jump_to(0, 2);
        assert_eq!(rp.position.as_ref().map(|p| p.slide_total()), Some(2));

        rp.previous_slide();
        assert_eq!(rp.position.as_ref().map(|p| p.slide_total()), Some(1));
    }

    /// There is no slide after the last one, and asking for one must leave the
    /// presentation where it is rather than run off the end.
    #[test]
    fn the_presentation_stops_at_the_last_slide() {
        let mut rp = three_slides();
        rp.jump_to(0, 2);
        rp.next_slide();

        assert_eq!(rp.position.as_ref().map(|p| p.slide_total()), Some(2));
    }

    /// A position outside the presentation is not a position: asking to jump
    /// there must change nothing.
    #[test]
    fn a_jump_outside_the_presentation_is_ignored() {
        let mut rp = three_slides();
        rp.jump_to(0, 1);

        rp.jump_to(0, 99);
        rp.jump_to(7, 0);

        assert_eq!(rp.position.as_ref().map(|p| p.slide_total()), Some(1));
    }

    /// The console lays a slide out at the size the presentation window is
    /// actually using, not at the monitor's. They are different numbers on any
    /// screen that is not at 100% scaling — the monitor is in physical pixels
    /// and a window at 150% is laid out at two thirds of it — and laying the
    /// preview out at the wrong one breaks its text in different places from
    /// the screen the audience is looking at.
    #[test]
    fn a_slide_is_laid_out_at_the_size_the_presentation_uses() {
        let mut rp = three_slides();
        rp.presentation_resolution = (1920, 1080);

        // Nothing measured yet: the monitor stands in.
        assert_eq!(rp.layout_size(), (1920.0, 1080.0));

        // Measured: a window on that monitor at 150% scaling.
        rp.presentation_layout = Some((1280.0, 720.0));
        assert_eq!(rp.layout_size(), (1280.0, 720.0));
    }

    /// The measurement is made in the presentation window and needed in the
    /// console, so it has to survive the comparison the windows sync through —
    /// otherwise the console never hears about it.
    #[test]
    fn the_layout_size_reaches_the_other_window() {
        let mut measured = three_slides();
        let unmeasured = measured.clone();
        measured.presentation_layout = Some((1280.0, 720.0));

        assert!(
            !measured.eq_ignoring_scroll(&unmeasured),
            "a window that has measured itself differs from one that has not"
        );
    }
}
