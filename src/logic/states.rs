use std::{fs, path::PathBuf};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::{settings::{PresentationDesign, SlideTimerSettings, SlideTransition}, sourcefiles::SourceFile};
use cantara_songlib::slides::{Slide, SlideSettings};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Default)]
pub struct Settings {
    pub song_repos: Vec<Repository>,
    pub wizard_completed: bool,
}

impl Settings {
    /// Load settings from storage or creates a new default settings if
    /// the program is run for the first time.
    pub fn load() -> Self {
        match get_settings_file() {
            Some(file) => match std::fs::read_to_string(file) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(settings) => settings,
                    Err(_) => Self::default(),
                },
                Err(_) => Self::default(),
            },
            None => Self::default(),
        }
    }

    pub fn save(&self) {
        if let Some(file) = get_settings_file() {
            let _ = fs::create_dir_all(get_settings_folder().unwrap());
            if std::fs::write(file, serde_json::to_string_pretty(self).unwrap()).is_ok() {}
        }
    }

    pub fn add_repository(&mut self, repo: Repository) {
        if !self.song_repos.contains(&repo) {
            self.song_repos.push(repo);
        }
    }

    pub fn add_repository_folder(&mut self, folder: String) {
        self.song_repos.push(Repository::LocaleFilePath(folder));
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub enum Repository {
    LocaleFilePath(String),
    Remote(String),
}

#[derive(Clone)]
pub struct RuntimeInformation {
    pub language: String,
}

pub fn get_settings_file() -> Option<PathBuf> {
    get_settings_folder().map(|settings_folder| settings_folder.join("settings.json"))
}

pub fn get_settings_folder() -> Option<PathBuf> {
    dirs::config_local_dir().map(|dir| dir.join("cantara"))
}

/// This struct represents a selected item
#[derive(Clone, PartialEq)]
pub struct SelectedItemRepresentation {
    /// The source file of the selected item
    pub source_file: SourceFile,

    /// The [PresentationDesignSettings] as an option. If [None], the default [PresentationDesign] will be used.
    pub presentation_design_option: Option<PresentationDesign>,

    /// The [PresentationDesign] as an option. If [None], the default [PresentationDesign] will be used.
    pub slide_settings_option: Option<SlideSettings>,

    /// Optional inline markdown content for spontaneous markdown text.
    /// When set, this content is used instead of reading from the source file path.
    pub inline_markdown: Option<String>,

    /// Optional timer settings for automatic slide advance. If [None], no timer is used.
    pub timer_settings_option: Option<SlideTimerSettings>,

    /// The transition effect for this selection. Uses the default (Fade) when not set.
    pub transition_option: SlideTransition,
}

impl SelectedItemRepresentation {
    pub fn new_with_sourcefile(source_file: SourceFile) -> Self {
        SelectedItemRepresentation {
            source_file,
            presentation_design_option: None,
            slide_settings_option: None,
            inline_markdown: None,
            timer_settings_option: None,
            transition_option: SlideTransition::default(),
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
        }
    }

    /// Go to the next slide (if any exists).
    /// Resets `markdown_scroll_position` to 0 so the new slide starts at the top.
    pub fn next_slide(&mut self) {
        if let Some(ref mut pos) = self.position {
            if pos.try_next(&self.presentation).is_ok() {
                self.markdown_scroll_position = 0.0;
            }
        }
    }

    /// Go to the previous slide (if any exists).
    /// Resets `markdown_scroll_position` to 0 so the new slide starts at the top.
    pub fn previous_slide(&mut self) {
        if let Some(ref mut pos) = self.position {
            if pos.try_back(&self.presentation).is_ok() {
                self.markdown_scroll_position = 0.0;
            }
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
        self.position.clone().map(|pos| {
            self.presentation
                .get(pos.chapter())
                .unwrap()
                .slides
                .get(pos.chapter_slide())
                .unwrap()
                .clone()
        })
    }

    pub fn get_current_presentation_design(&self) -> PresentationDesign {
        match self.position.clone() {
            Some(pos) => self
                .presentation
                .get(pos.chapter())
                .unwrap()
                .presentation_design_option
                .clone()
                .unwrap_or(PresentationDesign::default()),
            None => PresentationDesign::default(),
        }
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
            && self.presentation_resolution == other.presentation_resolution
    }

    pub fn get_current_slide_settings(&self) -> SlideSettings {
        match self.position.clone() {
            Some(pos) => self
                .presentation
                .get(pos.chapter())
                .unwrap()
                .slide_settings_option
                .clone()
                .unwrap_or(SlideSettings::default()),
            None => SlideSettings::default(),
        }
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
                let chapter_len = self
                    .presentation
                    .get(pos.chapter())
                    .map(|ch| ch.slides.len())
                    .unwrap_or(0);
                pos.chapter_slide() + 1 >= chapter_len
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
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RunningPresentationPosition {
    /// The number of the current chapter
    chapter: usize,

    /// The number of the current slide of the chapter
    chapter_slide: usize,

    /// The total slide number of all chapters
    slide_total: usize,
}

impl RunningPresentationPosition {
    /// Creates a new position if there is at least one slide available
    pub fn new(presentation: &Vec<SlideChapter>) -> Option<Self> {
        if !presentation.is_empty() && !presentation.first().unwrap().slides.is_empty() {
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
    pub fn try_next(&mut self, presentation: &Vec<SlideChapter>) -> Result<(), ()> {
        if self.chapter_slide < self.cur_chapter_slide_length(presentation) - 1 {
            self.chapter_slide += 1;
            self.slide_total += 1;
            Ok(())
        } else if self.chapter < presentation.len() - 1 {
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
    pub fn try_back(&mut self, presentation: &Vec<SlideChapter>) -> Result<(), ()> {
        if self.chapter_slide > 0 {
            self.chapter_slide -= 1;
            self.slide_total -= 1;
            Ok(())
        } else if self.chapter > 0 {
            self.chapter -= 1;
            self.chapter_slide = self.cur_chapter_slide_length(presentation) - 1;
            self.slide_total -= 1;
            Ok(())
        } else {
            Err(())
        }
    }

    /// Helper function for getting the current slide length
    fn cur_chapter_slide_length(&self, presentation: &Vec<SlideChapter>) -> usize {
        presentation.get(self.chapter).unwrap().slides.len()
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
    pub slides: Vec<Slide>,
    pub source_file: SourceFile,
    pub presentation_design_option: Option<PresentationDesign>,
    pub slide_settings_option: Option<SlideSettings>,
    /// Optional timer settings for automatic slide advance.
    #[serde(default)]
    pub timer_settings_option: Option<SlideTimerSettings>,
    /// The transition effect for this chapter.
    #[serde(default)]
    pub transition_option: SlideTransition,
}

impl SlideChapter {
    pub fn new(
        slides: Vec<Slide>,
        source_file: SourceFile,
        presentation_design: Option<PresentationDesign>,
        slide_settings: Option<SlideSettings>,
    ) -> Self {
        SlideChapter {
            slides,
            source_file,
            presentation_design_option: presentation_design,
            slide_settings_option: slide_settings,
            timer_settings_option: None,
            transition_option: SlideTransition::default(),
        }
    }
}

fn default_presentation_resolution() -> (u32, u32) {
    (1920, 1080)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_load_settings() {
        let settings = get_settings_folder().unwrap();
        dbg!(&settings);
        println!("Settings folder: {:?}", settings);
    }

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
}
