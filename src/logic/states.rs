use std::{fs, path::PathBuf};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::{settings::PresentationDesign, sourcefiles::SourceFile};
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
}

impl SelectedItemRepresentation {
    pub fn new_with_sourcefile(source_file: SourceFile) -> Self {
        SelectedItemRepresentation {
            source_file,
            presentation_design_option: None,
            slide_settings_option: None,
        }
    }
}

/// A created presentation which is able to run
#[derive(Clone, PartialEq)]
pub struct RunningPresentation {
    pub presentation: Vec<SlideChapter>,
    pub position: Option<RunningPresentationPosition>,
}

impl RunningPresentation {
    /// Helper function to create a new [RunningPresentation] data structure
    pub fn new(presentation: Vec<SlideChapter>) -> Self {
        RunningPresentation {
            presentation: presentation.clone(),
            position: RunningPresentationPosition::new(&presentation),
        }
    }

    /// Go to the next slide (if any exists)
    pub fn next_slide(&mut self) {
        if let Some(ref mut pos) = self.position {
            let _ = pos.try_next(&self.presentation);
        }
    }

    /// Go to the previous slide (if any exists)
    pub fn previous_slide(&mut self) {
        if let Some(ref mut pos) = self.position {
            let _ = pos.try_back(&self.presentation);
        }
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
}

/// This represents a position in a running presentation.
/// This struct should always be save in that sense that the presentation does exist.
#[derive(Clone, PartialEq)]
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
#[derive(Clone, PartialEq)]
pub struct SlideChapter {
    pub slides: Vec<Slide>,
    pub source_file: SourceFile,
    pub presentation_design_option: Option<PresentationDesign>,
    pub slide_settings_option: Option<SlideSettings>,
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
        }
    }
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
}
