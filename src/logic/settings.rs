//! This module contains the logic and structures for managing, loading and saving the program's settings.

use crate::logic::css::{CssFontFamily, CssString};
use crate::logic::sourcefiles::{ImageSourceFile, SourceFile, get_source_files};
use cantara_songlib::slides::SlideSettings;
use dioxus::prelude::*;
use reqwest::{Client as AsyncClient, blocking::Client};
use rgb::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use zip::ZipArchive;

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
    /// There is a default added when none is found.
    #[serde(default = "default_presentation_design_vec")]
    pub presentation_designs: Vec<PresentationDesign>,

    /// The configured song slide settings in Cantara
    /// There is a default added when none is found.
    #[serde(default = "default_song_slide_vec")]
    pub song_slide_settings: Vec<SlideSettings>,

    /// A boolean variable which determines if presentations should start in fullscreen mode by default.
    #[serde(default = "default_always_start_fullscreen")]
    pub always_start_fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repositories: vec![],
            wizard_completed: false,
            presentation_designs: default_presentation_design_vec(),
            song_slide_settings: default_song_slide_vec(),
            always_start_fullscreen: default_always_start_fullscreen(),
        }
    }
}

/// This creates the default presentation designs
fn default_presentation_design_vec() -> Vec<PresentationDesign> {
    vec![PresentationDesign::default()]
}

/// This creates the default slide settings
fn default_song_slide_vec() -> Vec<SlideSettings> {
    vec![SlideSettings::default()]
}

/// This returns the default value for always_start_fullscreen
fn default_always_start_fullscreen() -> bool {
    false
}

impl Settings {
    /// Cleans up all temporary resources associated with all repositories
    pub fn cleanup_all_repositories(&self) {
        for repo in &self.repositories {
            repo.cleanup();
        }
        // Also clean up any orphaned temporary directories
        RepositoryType::cleanup_all_temp_dirs();
    }

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
            if std::fs::write(file, serde_json::to_string_pretty(self).unwrap()).is_ok() {}
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
        let name: &str = get_last_dir(&folder).unwrap_or(&folder);

        self.repositories
            .push(Repository::new_local_folder(name.into(), folder));
    }

    /// Add a new remote ZIP repository given as URL to the settings.
    ///
    /// # Arguments
    /// * `name` - A user-friendly name for the repository
    /// * `url` - The URL to the ZIP file
    pub fn add_remote_zip_repository(&mut self, name: String, url: String) {
        self.repositories
            .push(Repository::new_remote_zip(name, url));
    }

    /// Add a new remote ZIP repository given as URL to the settings.
    /// The name will be derived from the URL if possible.
    ///
    /// # Arguments
    /// * `url` - The URL to the ZIP file
    pub fn add_remote_zip_repository_url(&mut self, url: String) {
        // Extract a name from the URL (last part of the path before the extension)
        let name = url
            .split('/')
            .next_back()
            .unwrap_or(&url)
            .split('.')
            .next()
            .unwrap_or(&url)
            .to_string();

        self.repositories
            .push(Repository::new_remote_zip(name, url));
    }

    /// Get all elements of all repositories as a vector of [SourceFile]
    pub fn get_sourcefiles(&self) -> Vec<SourceFile> {
        let mut source_files: Vec<SourceFile> = vec![];
        self.repositories
            .iter()
            .for_each(|repo| source_files.extend(repo.repository_type.get_files()));

        source_files.sort();
        source_files.dedup();

        source_files
    }

    /// Get all elements of all repositories as a vector of [SourceFile] asynchronously.
    /// This is the async version of `get_sourcefiles`.
    pub async fn get_sourcefiles_async(&self) -> Vec<SourceFile> {
        let mut source_files: Vec<SourceFile> = vec![];

        // Process each repository asynchronously
        for repo in &self.repositories {
            let files = repo.repository_type.get_files_async().await;
            source_files.extend(files);
        }

        source_files.sort();
        source_files.dedup();

        source_files
    }
}

/// This struct reprents a repository
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Repository {
    /// A user given name for the repository which makes it easier to identify it
    pub name: String,

    /// Whether the repository is removable
    pub removable: bool,

    /// Whether the user has writing permissions to the repository
    pub writing_permissions: bool,

    /// The type of the repository-linked to it are additional information
    pub repository_type: RepositoryType,
}

impl Repository {
    /// Cleans up any temporary resources associated with this repository
    pub fn cleanup(&self) {
        if let RepositoryType::RemoteZip(url) = &self.repository_type {
            RepositoryType::cleanup_temp_dir(url);
        }
    }

    pub fn new_local_folder(name: String, path: String) -> Self {
        Repository {
            name,
            removable: true,
            writing_permissions: true,
            repository_type: RepositoryType::LocaleFilePath(path),
        }
    }

    /// Creates a new repository that downloads and extracts a remote ZIP file.
    ///
    /// # Arguments
    /// * `name` - A user-friendly name for the repository
    /// * `url` - The URL to the ZIP file
    ///
    /// # Returns
    /// A new `Repository` instance configured to use a remote ZIP file
    pub fn new_remote_zip(name: String, url: String) -> Self {
        Repository {
            name,
            removable: true,
            writing_permissions: false, // ZIP repositories are read-only
            repository_type: RepositoryType::RemoteZip(url),
        }
    }

    /// Get the count of source files in this repository
    pub fn get_source_file_count(&self) -> usize {
        self.repository_type.get_files().len()
    }

    /// Get the count of source files in this repository asynchronously
    pub async fn get_source_file_count_async(&self) -> usize {
        self.repository_type.get_files_async().await.len()
    }
}

/// The enum represents the different types of repositories.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum RepositoryType {
    /// A repository that is a local folder represented by a file path.
    LocaleFilePath(String),

    /// A repository that is a remote URL.
    /// Hint: This is not implemented yet!
    Remote(String),

    /// A repository that is a remote ZIP file which is downloaded and extracted temporarily.
    /// The String contains the URL to the ZIP file.
    RemoteZip(String),
}

// This struct holds the temporary directory for a remote ZIP repository
// It's not included in serialization/deserialization
thread_local! {
    static TEMP_DIRS: std::cell::RefCell<std::collections::HashMap<String, tempfile::TempDir>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

impl RepositoryType {
    /// Cleans up the temporary directory for a specific URL
    pub fn cleanup_temp_dir(url: &str) {
        TEMP_DIRS.with(|temp_dirs| {
            let mut temp_dirs = temp_dirs.borrow_mut();
            if temp_dirs.remove(url).is_some() {
                log::info!("Cleaned up temporary directory for URL: {}", url);
            }
        });
    }

    /// Cleans up all temporary directories
    pub fn cleanup_all_temp_dirs() {
        TEMP_DIRS.with(|temp_dirs| {
            let mut temp_dirs = temp_dirs.borrow_mut();
            let urls: Vec<String> = temp_dirs.keys().cloned().collect();
            for url in urls {
                temp_dirs.remove(&url);
                log::info!("Cleaned up temporary directory for URL: {}", url);
            }
        });
    }

    /// Get files which are provided by the repository.
    pub fn get_files(&self) -> Vec<SourceFile> {
        match self {
            RepositoryType::LocaleFilePath(path_string) => {
                get_source_files(Path::new(&path_string))
            }
            RepositoryType::RemoteZip(url) => {
                // Check if we already have a temporary directory for this URL
                let mut files = vec![];

                TEMP_DIRS.with(|temp_dirs| {
                    let mut temp_dirs = temp_dirs.borrow_mut();

                    // If we already have a temporary directory for this URL, use it
                    if let Some(temp_dir) = temp_dirs.get(url) {
                        log::info!("Using existing temporary directory for URL: {}", url);
                        files = get_source_files(temp_dir.path());
                    } else {
                        // Otherwise, download and extract the ZIP file
                        log::info!("Downloading and extracting ZIP file from URL: {}", url);
                        match self.download_and_extract_zip(url) {
                            Ok(temp_dir) => {
                                let path = temp_dir.path().to_path_buf();
                                log::info!("Extracted ZIP file to temporary directory: {:?}", path);
                                files = get_source_files(&path);
                                // Store the temporary directory so it persists
                                temp_dirs.insert(url.clone(), temp_dir);
                            }
                            Err(e) => {
                                log::error!("Failed to download or extract ZIP file: {}", e);
                            }
                        }
                    }
                });

                files
            }
            _ => vec![],
        }
    }

    /// Get files which are provided by the repository asynchronously.
    /// This is the async version of `get_files`.
    pub async fn get_files_async(&self) -> Vec<SourceFile> {
        match self {
            RepositoryType::LocaleFilePath(path_string) => {
                get_source_files(Path::new(&path_string))
            }
            RepositoryType::RemoteZip(url) => {
                // Check if we already have a temporary directory for this URL
                let mut files = vec![];

                TEMP_DIRS.with(|temp_dirs| {
                    let temp_dirs = temp_dirs.borrow_mut();

                    // If we already have a temporary directory for this URL, use it
                    if let Some(temp_dir) = temp_dirs.get(url) {
                        log::info!("Using existing temporary directory for URL: {}", url);
                        files = get_source_files(temp_dir.path());
                    }
                });

                // If we don't have a temporary directory yet, download and extract the ZIP file
                if files.is_empty() {
                    log::info!("Downloading and extracting ZIP file from URL: {}", url);
                    match self.download_and_extract_zip_async(url).await {
                        Ok(temp_dir) => {
                            let path = temp_dir.path().to_path_buf();
                            log::info!("Extracted ZIP file to temporary directory: {:?}", path);
                            files = get_source_files(&path);

                            // Store the temporary directory so it persists
                            TEMP_DIRS.with(|temp_dirs| {
                                let mut temp_dirs = temp_dirs.borrow_mut();
                                temp_dirs.insert(url.clone(), temp_dir);
                            });
                        }
                        Err(e) => {
                            log::error!("Failed to download or extract ZIP file: {}", e);
                        }
                    }
                }

                files
            }
            _ => vec![],
        }
    }

    /// Downloads a ZIP file from a URL and extracts it to a temporary directory.
    /// Returns the temporary directory if successful, or an error if the download or extraction fails.
    fn download_and_extract_zip(&self, url: &str) -> Result<TempDir, String> {
        // Create a temporary directory to store the downloaded ZIP file
        let temp_dir =
            TempDir::new().map_err(|e| format!("Failed to create temporary directory: {}", e))?;

        // Create a temporary file path for the downloaded ZIP
        let zip_path = temp_dir.path().join("download.zip");

        // Download the ZIP file
        let response = Client::new()
            .get(url)
            .send()
            .map_err(|e| format!("Failed to download ZIP file: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to download ZIP file: HTTP status {}",
                response.status()
            ));
        }

        // Create the file and write the response body to it
        let mut file = fs::File::create(&zip_path)
            .map_err(|e| format!("Failed to create temporary file: {}", e))?;

        let content = response
            .bytes()
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        file.write_all(&content)
            .map_err(|e| format!("Failed to write to temporary file: {}", e))?;

        // Open the ZIP file
        let file = fs::File::open(&zip_path)
            .map_err(|e| format!("Failed to open downloaded ZIP file: {}", e))?;

        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("Failed to parse ZIP file: {}", e))?;

        // Extract the ZIP file
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to access ZIP entry: {}", e))?;

            let outpath = temp_dir.path().join(file.name());

            if file.name().ends_with('/') {
                // Create directory
                fs::create_dir_all(&outpath)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            } else {
                // Create parent directory if it doesn't exist
                if let Some(parent) = outpath.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                    }
                }

                // Extract file
                let mut outfile = fs::File::create(&outpath)
                    .map_err(|e| format!("Failed to create output file: {}", e))?;

                io::copy(&mut file, &mut outfile)
                    .map_err(|e| format!("Failed to write output file: {}", e))?;
            }
        }

        // Return the temporary directory
        Ok(temp_dir)
    }

    /// Downloads a ZIP file from a URL and extracts it to a temporary directory asynchronously.
    /// Returns the temporary directory if successful, or an error if the download or extraction fails.
    /// This is the async version of `download_and_extract_zip`.
    async fn download_and_extract_zip_async(&self, url: &str) -> Result<TempDir, String> {
        // Create a temporary directory to store the downloaded ZIP file
        let temp_dir =
            TempDir::new().map_err(|e| format!("Failed to create temporary directory: {}", e))?;

        // Create a temporary file path for the downloaded ZIP
        let zip_path = temp_dir.path().join("download.zip");

        // Download the ZIP file using the async client
        let response = AsyncClient::new()
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to download ZIP file: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to download ZIP file: HTTP status {}",
                response.status()
            ));
        }

        // Create the file and write the response body to it
        let mut file = fs::File::create(&zip_path)
            .map_err(|e| format!("Failed to create temporary file: {}", e))?;

        let content = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        file.write_all(&content)
            .map_err(|e| format!("Failed to write to temporary file: {}", e))?;

        // Open the ZIP file
        let file = fs::File::open(&zip_path)
            .map_err(|e| format!("Failed to open downloaded ZIP file: {}", e))?;

        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("Failed to parse ZIP file: {}", e))?;

        // Extract the ZIP file
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to access ZIP entry: {}", e))?;

            let outpath = temp_dir.path().join(file.name());

            if file.name().ends_with('/') {
                // Create directory
                fs::create_dir_all(&outpath)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            } else {
                // Create parent directory if it doesn't exist
                if let Some(parent) = outpath.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                    }
                }

                // Extract file
                let mut outfile = fs::File::create(&outpath)
                    .map_err(|e| format!("Failed to create output file: {}", e))?;

                io::copy(&mut file, &mut outfile)
                    .map_err(|e| format!("Failed to write output file: {}", e))?;
            }
        }

        // Return the temporary directory
        Ok(temp_dir)
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
}

impl Default for PresentationDesign {
    fn default() -> Self {
        PresentationDesign {
            name: "Default".to_string(),
            description: "".to_string(),
            presentation_design_settings: PresentationDesignSettings::default(),
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
    /// The font configuration for all kinds of contents
    pub fonts: Vec<FontRepresentation>,

    /// The index of the font configuration for default headlines
    headline_index: Option<u16>,

    /// The index of the font configuration for default spoilers
    pub spoiler_index: Option<u16>,

    /// The index of the font configuration for default meta-block
    pub meta_index: Option<u16>,

    /// The vertical alignment of the content
    pub vertical_alignment: VerticalAlign,

    /// The factor for the font size of the spoiler content relative to the main content font size
    pub spoiler_content_fontsize_factor: f64,

    /// The background color of the presentation
    pub background_color: RGB8,

    /// The background color transparancy towards an image (0-255)
    pub background_transparency: u8,

    /// The padding of the presentation (top, bottom, left, right)
    pub padding: TopBottomLeftRight,

    /// An optional background picture
    pub background_image: Option<ImageSourceFile>,

    /// The distance between the main content and the spoiler content
    pub main_content_spoiler_content_padding: CssSize,
}

impl PresentationDesignTemplate {
    /// Returns the background color as an RGB string which can be used in CSS
    /// for example: pure black would equal to (0, 0, 0)
    pub fn get_background_as_rgb_string(&self) -> String {
        format!(
            "{}, {}, {}",
            self.background_color.r, self.background_color.g, self.background_color.b
        )
    }

    /// Returns the background color as a hexadecimal string
    /// for example, pure black would equal to #000000
    pub fn get_background_color_as_hex_string(&self) -> String {
        rgb_to_hex_string(&self.background_color)
    }

    /// Set the background color from a hex str if the hex string is valid.
    /// Returns `Ok(())` if the setting was successfully and `Err(())` if the validation of the string failed.
    pub fn set_background_color_from_hex_str(&mut self, hex_string: &str) -> Result<(), ()> {
        match hex_string_to_rgb(hex_string) {
            Some(rgb) => {
                self.background_color = rgb;
                Ok(())
            }
            None => Err(()),
        }
    }

    pub fn headline_index(&self) -> Option<u16> {
        self.headline_index
    }

    pub fn spoiler_index(&self) -> Option<u16> {
        self.spoiler_index
    }

    /// Sets the headline index if it does exist.
    /// If it does not exist, no change will occur.
    pub fn set_headline_index(&mut self, headline_index: Option<u16>) {
        match headline_index {
            Some(index) => {
                if (index as usize) < self.fonts.len() {
                    self.headline_index = Some(index);
                }
            }
            None => self.headline_index = None,
        }
    }

    /// Sets the spoiler index if it does exist.
    /// If it does not exist, no change will occur.
    pub fn set_spoiler_index(&mut self, spoiler_index: Option<u16>) {
        match spoiler_index {
            Some(index) => {
                if (index as usize) < self.fonts.len() {
                    self.spoiler_index = Some(index);
                }
            }
            None => self.spoiler_index = None,
        }
    }

    /// Gets the default [FontRepresentation] (the first element of the `fonts` vector or the configured default
    /// font as a fallback
    pub fn get_default_font(&self) -> FontRepresentation {
        match self.fonts.first() {
            Some(font) => font.clone(),
            None => FontRepresentation::default(),
        }
    }

    /// Gets the default font [FontRepresentation] for the spoiler part.
    /// If none is defined, the system default will be returned as a fallback.
    pub fn get_default_spoiler_font(&self) -> FontRepresentation {
        match self.spoiler_index {
            Some(spoiler_index) => match self.fonts.get(spoiler_index as usize) {
                Some(font) => font.clone(),
                None => FontRepresentation::default_spoiler(),
            },
            None => FontRepresentation::default_spoiler(),
        }
    }

    /// Gets the default font [FontRepresentation] for the headline part.
    /// If none is defined, the system default will be returned as a fallback.
    pub fn get_default_headline_font(&self) -> FontRepresentation {
        match self.headline_index {
            Some(headline_index) => match self.fonts.get(headline_index as usize) {
                Some(font) => font.clone(),
                None => FontRepresentation::default(),
            },
            None => FontRepresentation::default(),
        }
    }

    /// Gets the default font [FontRepresentation] for the meta part.
    /// If none is defined, the system default will be returned as a fallback.
    pub fn get_default_meta_font(&self) -> FontRepresentation {
        match self.meta_index {
            Some(meta_index) => match self.fonts.get(meta_index as usize) {
                Some(font) => font.clone(),
                None => FontRepresentation::default_meta(),
            },
            None => FontRepresentation::default_meta(),
        }
    }
}

impl Default for PresentationDesignTemplate {
    fn default() -> Self {
        PresentationDesignTemplate {
            fonts: vec![
                FontRepresentation::default(),
                FontRepresentation::default_spoiler(),
                FontRepresentation::default_meta(),
            ],
            headline_index: Some(0),
            spoiler_index: Some(1),
            meta_index: Some(2),
            vertical_alignment: VerticalAlign::default(),
            spoiler_content_fontsize_factor: 0.6,
            background_color: Rgb::new(0, 0, 0),
            background_transparency: 0,
            padding: default_padding(),
            background_image: None,
            main_content_spoiler_content_padding: CssSize::Px(20.0),
        }
    }
}

/// Represents a font representation for an element in the presentation
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct FontRepresentation {
    /// The font family. If 'None', the web default will be displayed.
    pub font_family: Option<CssFontFamily>,

    /// The font size for normal paragraphs, song lyrics, etc.
    pub font_size: CssSize,

    /// Whether to show a shadow around the font
    pub shadow: bool,

    /// The height of the line (distance above and below)
    pub line_height: f64,

    /// The color of the font
    pub color: RGBA8,

    /// The horizontal alignment of the block
    pub horizontal_alignment: HorizontalAlign,
}

impl FontRepresentation {
    pub fn get_color_as_rgba_string(&self) -> String {
        format!(
            "{}, {}, {}, {}",
            self.color.r, self.color.g, self.color.b, self.color.a
        )
    }

    pub fn default_spoiler() -> Self {
        let mut default = Self::default();
        default
            .font_size
            .set_float(default.font_size.get_float() * 0.7);
        default
    }

    fn default_meta() -> FontRepresentation {
        let mut default = Self::default();
        default
            .font_size
            .set_float(default.font_size.get_float() * 0.5);
        default
    }
}

impl Default for FontRepresentation {
    fn default() -> Self {
        FontRepresentation {
            font_family: None,
            font_size: CssSize::Pt(32.0),
            shadow: false,
            line_height: 1.2,
            color: Rgba::new(255, 255, 255, 255),
            horizontal_alignment: HorizontalAlign::default(),
        }
    }
}

/// The horizontal alignment of a block
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Default)]
pub enum HorizontalAlign {
    Left,

    #[default]
    Centered,

    Right,
}

impl CssString for HorizontalAlign {
    fn to_css_string(&self) -> String {
        match self {
            HorizontalAlign::Left => "left".to_string(),
            HorizontalAlign::Centered => "center".to_string(),
            HorizontalAlign::Right => "right".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum VerticalAlign {
    Top,

    #[default]
    Middle,

    Bottom,
}

/// Returns the default padding for the presentation design
fn default_padding() -> TopBottomLeftRight {
    TopBottomLeftRight {
        top: CssSize::Px(20.0),
        bottom: CssSize::Px(20.0),
        left: CssSize::Px(20.0),
        right: CssSize::Px(20.0),
    }
}

/// Represens for distance values (top, bottom, left, right)
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct TopBottomLeftRight {
    pub top: CssSize,
    pub bottom: CssSize,
    pub left: CssSize,
    pub right: CssSize,
}

impl Default for TopBottomLeftRight {
    fn default() -> Self {
        TopBottomLeftRight {
            top: CssSize::Null,
            bottom: CssSize::Null,
            left: CssSize::Null,
            right: CssSize::Null,
        }
    }
}

/// A size value representing a CSS file
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub enum CssSize {
    Px(f32),
    Pt(f32),
    Em(f32),
    Percentage(f32),
    #[default]
    Null,
}

impl CssString for CssSize {
    fn to_css_string(&self) -> String {
        match self {
            CssSize::Px(size) => format!("{}px", size),
            CssSize::Pt(size) => format!("{}pt", size),
            CssSize::Em(size) => format!("{}em", size),
            CssSize::Percentage(size) => format!("{}%", size),
            CssSize::Null => "0".to_string(),
        }
    }
}

impl CssSize {
    /// Checks if the size is null or zero
    pub fn is_null(&self) -> bool {
        matches!(self, CssSize::Null)
            || matches!(self, CssSize::Px(0.0))
            || matches!(self, CssSize::Pt(0.0))
            || matches!(self, CssSize::Em(0.0))
            || matches!(self, CssSize::Percentage(0.0))
    }

    pub fn null() -> Self {
        CssSize::Null
    }

    /// Gets the inner float independent of the unit
    pub fn get_float(&self) -> f32 {
        match self {
            CssSize::Px(x) => *x,
            CssSize::Pt(x) => *x,
            CssSize::Em(x) => *x,
            CssSize::Percentage(x) => *x,
            CssSize::Null => 0.0,
        }
    }

    /// Sets a float and keeps the unit
    /// If the enum is [Null], it will turn into a [CssSize::Px].
    pub fn set_float(&mut self, value: f32) {
        match self {
            CssSize::Px(x) => *x = value,
            CssSize::Pt(x) => *x = value,
            CssSize::Em(x) => *x = value,
            CssSize::Percentage(x) => *x = value,
            CssSize::Null => *self = CssSize::Px(value),
        }
    }
}

/// Gets the last dir from a given path as String
fn get_last_dir(path: &str) -> Option<&str> {
    path.trim_end_matches(['\\', '/']) // Remove trailing separators
        .rsplit(['\\', '/']) // Split by either separator
        .next() // Get the last segment
        .filter(|s| !s.is_empty()) // Ensure it's not empty
}

/// Converts an [RGB8] value to a hex string
fn rgb_to_hex_string(rgb: &RGB8) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b)
}

/// Converts a hexadecimal color expression as string to an [RGB8] if possible
fn hex_string_to_rgb(hex_string: &str) -> Option<RGB8> {
    // Remove optional leading '#' and convert to uppercase for consistency
    let hex = hex_string.trim_start_matches('#').to_uppercase();

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
    fn test_color_conversion() {
        let color_hex_black = "#000000";
        let color_hex_white = "#FFFFFF";
        let color_hex_red = "#ff0000";

        assert_eq!(
            RGB8::new(0, 0, 0),
            hex_string_to_rgb(color_hex_black).unwrap()
        );
        assert_eq!(
            RGB8::new(255, 255, 255),
            hex_string_to_rgb(color_hex_white).unwrap()
        );
        assert_eq!(
            RGB8::new(255, 0, 0),
            hex_string_to_rgb(color_hex_red).unwrap()
        );
    }
}
