//! This module contains the logic and structures for managing, loading and saving the program's settings.

use cantara_songlib::slides::SlideSettings;
use dioxus::prelude::*;
use rgb::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::logic::sourcefiles::{get_source_files, SourceFile};

/// Returns the settings of the program
///
/// # Panics
/// When the settings are not available -> if you call this function before they are set in the main function.
pub fn use_settings() -> Signal<Settings> {
    use_context()
}

/// The struct representing Cantara's settings.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Settings {
    /// A vector with the repositories which Cantara uses
    /// This should at least contain one element.
    pub repositories: Vec<Repository>,

    /// A boolean variable which is set to true when the initial wizard has been completed once.
    /// It can't be changed from the user interface.
    pub wizard_completed: bool,

    /// The configured presentation designs in Cantara.
    /// The default one is added as default.
    #[serde(default = "default_presentation_design_vec")]
    pub presentation_designs: Vec<PresentationDesign>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repositories: vec![],
            wizard_completed: false,
            presentation_designs: default_presentation_design_vec(),
        }
    }
}

/// This creates the default presentation designs if there are none available
fn default_presentation_design_vec() -> Vec<PresentationDesign> {
    vec![PresentationDesign::default()]
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

    /// Save the current settings to storage.
    pub fn save(&self) {
        if let Some(file) = get_settings_file() {
            let _ = fs::create_dir_all(get_settings_folder().unwrap());
            if std::fs::write(file, serde_json::to_string_pretty(self).unwrap()).is_ok() {  }
        }
    }

    /// Add a new repository to the settings if the repository is not already present (avoiding duplicates).
    pub fn add_repository(&mut self, repo: Repository) {
        if !self.repositories.contains(&repo) {
            self.repositories.push(repo);
        }
    }

    /// Add a new repository folder given as String to the settings if the repository is not already present (avoiding duplicates).
    pub fn add_repository_folder(&mut self, folder: String) {
        self.repositories.push(Repository::LocaleFilePath(folder));
    }

    /// Get all elements of all repositories as a vector of [SourceFile]
    pub fn get_sourcefiles(&self) -> Vec<SourceFile> {
        let mut source_files: Vec<SourceFile> = vec![];
        self.repositories
            .iter()
            .for_each(|repo| source_files.extend(repo.get_files()));

        source_files.sort();
        source_files.dedup();

        source_files
    }
}

/// The enum representing the different types of repositories.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum Repository {
    /// A repository that is a local folder represented by a file path.
    LocaleFilePath(String),

    /// A repository that is a remote URL.
    /// Hint: This is not implemented yet!
    Remote(String),
}

impl Repository {
    /// Get files which are provided by the repository.
    pub fn get_files(&self) -> Vec<SourceFile> {
        match self {
            Repository::LocaleFilePath(path_string) => get_source_files(Path::new(&path_string)),
            _ => vec![],
        }
    }
}

fn get_settings_file() -> Option<PathBuf> {
    get_settings_folder().map(|settings_folder| settings_folder.join("settings.json"))
}

fn get_settings_folder() -> Option<PathBuf> {
    dirs::config_local_dir().map(|dir| dir.join("cantara"))
}

/// A configured Presentation Design which is used both for creating the presentation slides as well as for rendering them.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct PresentationDesign {
    /// A name which helps to identify the design
    pub name: String,

    /// A description (can be empty)
    pub description: String,

    /// Presentation Design settings for that PresentationDesign
    pub presentation_design_settings: PresentationDesignSettings,

    /// Settings for the slide creation process
    pub slide_settings: SlideSettings,
}

impl Default for PresentationDesign {
    fn default() -> Self {
        PresentationDesign {
            name: "Default".to_string(),
            description: "".to_string(),
            presentation_design_settings: PresentationDesignSettings::default(),
            slide_settings: SlideSettings::default(),
        }
    }
}

/// This enum describes the general design of the presentation (background color, font-colors etc.).
/// It can be configured via a Template or imputed by direct HTML/CSS
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum PresentationDesignSettings {
    /// Describe the design via a template set up in Cantara
    Template(PresentationDesignTemplate),

    /// Manually specified template with HTML/CSS/Javascript (not implemented yet)
    Custom(String),
}

impl Default for PresentationDesignSettings {
    fn default() -> Self {
        PresentationDesignSettings::Template(PresentationDesignTemplate::default())
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct PresentationDesignTemplate {
    pub main_content_fonts: Vec<FontRepresentation>,
    pub vertical_alignment: VerticalAlign,
    pub spoiler_content_fontsize_factor: f64,
    pub background_color: RGB8,
    pub background_transparancy: u8,
}

impl PresentationDesignTemplate {
    pub fn get_background_as_rgb_string(&self) -> String {
        format!(
            "{}, {}, {}",
            self.background_color.r,
            self.background_color.g,
            self.background_color.b
        )
    }
}

impl Default for PresentationDesignTemplate {
    fn default() -> Self {
        PresentationDesignTemplate {
            main_content_fonts: vec![FontRepresentation::default()],
            vertical_alignment: VerticalAlign::default(),
            spoiler_content_fontsize_factor: 0.6,
            background_color: Rgb::new(0, 0, 0),
            background_transparancy: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct FontRepresentation {
    pub font_family: Option<String>,
    pub font_size: usize,
    pub headline_font_size: usize,
    pub spoiler_font_size: usize,
    pub shadow: bool,
    pub line_height: f64,
    pub color: RGBA8,
    pub horizontal_alignment: HorizontalAlign,
}

impl FontRepresentation {
    pub fn get_color_as_rgba_string(&self) -> String {
        format!(
            "{}, {}, {}, {}",
            self.color.r,
            self.color.g,
            self.color.b,
            self.color.a
        )
    }
}

impl Default for FontRepresentation {
    fn default() -> Self {
        FontRepresentation {
            font_family: None,
            font_size: 36,
            headline_font_size: 44,
            spoiler_font_size: 18,
            shadow: false,
            line_height: 1.2,
            color: Rgba::new(255, 255, 255, 255),
            horizontal_alignment: HorizontalAlign::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum HorizontalAlign {
    Left,

    #[default]
    Centered,

    Right,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum VerticalAlign {
    Top,

    #[default]
    Middle,

    Bottom,
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
