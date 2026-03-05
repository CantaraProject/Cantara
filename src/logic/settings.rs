//! This module contains the logic and structures for managing, loading and saving the program's settings.

use crate::logic::css::{CssFontFamily, CssString};
use crate::logic::sourcefiles::{ImageSourceFile, SourceFile, get_source_files};
use cantara_songlib::slides::SlideSettings;
use dioxus::prelude::*;
use reqwest::Client as AsyncClient;
#[cfg(not(target_arch = "wasm32"))]
use reqwest::blocking::Client;
use rgb::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    fs,
    io::{self, Write},
};
#[cfg(not(target_arch = "wasm32"))]
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

    /// The name of the monitor to use for presentations. None means automatic (prefer non-primary).
    #[serde(default)]
    pub presentation_screen: Option<String>,

    /// The name of the monitor to use for the presenter console. None means automatic (prefer primary).
    #[serde(default)]
    pub presenter_screen: Option<String>,

    /// Whether to show the presenter console when starting a presentation.
    #[serde(default = "default_show_presenter_console")]
    pub show_presenter_console: bool,

    /// Whether to show the presenter console in the main window instead of a separate window.
    #[serde(default = "default_presenter_console_in_main_window")]
    pub presenter_console_in_main_window: bool,

    /// Which view mode to use for the presenter console left panel.
    #[serde(default)]
    pub presenter_console_view: PresenterConsoleView,

    /// The thumbnail column width (in pixels) for the presenter console grid view.
    #[serde(default = "default_presenter_console_grid_size")]
    pub presenter_console_grid_size: u32,
}

/// The view mode for the presenter console left panel.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
pub enum PresenterConsoleView {
    /// Text-based list view (default, existing behaviour)
    #[default]
    Text,
    /// Grid overview showing slide thumbnails grouped by chapter
    Grid,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repositories: vec![],
            wizard_completed: false,
            presentation_designs: default_presentation_design_vec(),
            song_slide_settings: default_song_slide_vec(),
            always_start_fullscreen: default_always_start_fullscreen(),
            presentation_screen: None,
            presenter_screen: None,
            show_presenter_console: default_show_presenter_console(),
            presenter_console_in_main_window: default_presenter_console_in_main_window(),
            presenter_console_view: PresenterConsoleView::default(),
            presenter_console_grid_size: default_presenter_console_grid_size(),
        }
    }
}

fn default_presenter_console_grid_size() -> u32 {
    250
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

/// This returns the default value for show_presenter_console
fn default_show_presenter_console() -> bool {
    true
}

/// This returns the default value for presenter_console_in_main_window
fn default_presenter_console_in_main_window() -> bool {
    true
}

impl Settings {
    /// Cleans up all temporary resources associated with all repositories
    pub fn cleanup_all_repositories(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            for repo in &self.repositories {
                repo.cleanup();
            }
            // Also clean up any orphaned temporary directories
            RepositoryType::cleanup_all_temp_dirs();
        }
    }

    /// Load settings from storage or creates a new default settings if
    /// the program is run for the first time.
    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let json = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item("cantara-settings").ok().flatten());
            let mut settings = match json {
                Some(j) => serde_json::from_str(&j).unwrap_or_default(),
                None => Self::default(),
            };
            settings.ensure_default_presentation_design();
            settings.ensure_slide_settings_for_designs();
            settings.migrate_github_zip_repos();
            settings.ensure_bundled_repos();
            return settings;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut settings = match get_settings_file() {
                Some(file) => match std::fs::read_to_string(file) {
                    Ok(content) => match serde_json::from_str(&content) {
                        Ok(settings) => settings,
                        Err(_) => Self::default(),
                    },
                    Err(_) => Self::default(),
                },
                None => Self::default(),
            };
            settings.ensure_default_presentation_design();
            settings.ensure_slide_settings_for_designs();
            settings.migrate_github_zip_repos();
            settings
        }
    }

    /// Save the current settings to storage.
    pub fn save(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .map(|s| s.set_item("cantara-settings", &json));
            }
            return;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(file) = get_settings_file() {
                let _ = fs::create_dir_all(get_settings_folder().unwrap());
                if std::fs::write(file, serde_json::to_string_pretty(self).unwrap()).is_ok() {}
            }
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
    /// **Platform-specific behaviour on WASM:** if `url` is a GitHub archive URL
    /// (e.g. `https://github.com/owner/repo/archive/refs/heads/main.zip` or a
    /// `codeload.github.com` download link), the repository is stored as
    /// [`RepositoryType::GitHub`] instead of [`RepositoryType::RemoteZip`]. This
    /// avoids CORS failures caused by GitHub's redirect chain to `codeload.github.com`,
    /// and always resolves to the default branch via the GitHub API.
    ///
    /// # Arguments
    /// * `url` - The URL to the ZIP file
    pub fn add_remote_zip_repository_url(&mut self, url: String) {
        // GitHub archive URLs should be stored as GitHub-type repositories:
        // - On WASM this avoids CORS issues caused by GitHub's redirect chain
        // - On mobile/desktop this ensures a consistent download path via the GitHub API
        if let Some((owner, repo)) = RepositoryType::parse_github_from_zip_url(&url) {
            self.add_github_repository(owner, repo, None);
            return;
        }

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

    /// Add a new GitHub repository to the settings.
    ///
    /// # Arguments
    /// * `owner` - The GitHub repository owner (user or organization)
    /// * `repo` - The GitHub repository name
    /// * `token` - An optional personal access token for private repositories
    pub fn add_github_repository(
        &mut self,
        owner: String,
        repo: String,
        token: Option<String>,
    ) {
        self.repositories
            .push(Repository::new_github(owner, repo, token));
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

    /// Ensures that at least one presentation design exists.
    /// If there are no presentation designs, a default one is created.
    pub fn ensure_default_presentation_design(&mut self) {
        if self.presentation_designs.is_empty() {
            self.presentation_designs.push(PresentationDesign::default());
        }
    }

    /// Ensures that there are at least as many slide settings as presentation designs.
    /// If there are fewer slide settings, adds default slide settings until there are enough.
    pub fn ensure_slide_settings_for_designs(&mut self) {
        let design_count = self.presentation_designs.len();
        let slide_count = self.song_slide_settings.len();

        if slide_count < design_count {
            // Add default slide settings until there are at least as many as presentation designs
            for _ in 0..(design_count - slide_count) {
                self.song_slide_settings.push(SlideSettings::default());
            }
        }
    }

    /// Migrates any `RemoteZip` repositories whose URLs are GitHub archive URLs
    /// (github.com/.../archive/... or codeload.github.com/...) to `GitHub` type repositories.
    ///
    /// This avoids CORS issues on WASM caused by GitHub's redirect chain, and on mobile
    /// it ensures a consistent download path via the GitHub API which always fetches the
    /// default branch, avoiding failures from stale branch references.
    pub fn migrate_github_zip_repos(&mut self) {
        for repo in &mut self.repositories {
            if let RepositoryType::RemoteZip(url) = &repo.repository_type {
                if let Some((owner, repo_name)) =
                    RepositoryType::parse_github_from_zip_url(url)
                {
                    repo.repository_type = RepositoryType::GitHub {
                        owner,
                        repo: repo_name,
                        token: None,
                    };
                }
            }
        }
    }

    /// On WASM, ensures that all build-time bundled repositories are present in the
    /// settings and their embedded file data is loaded into the in-memory VFS.
    ///
    /// Bundled repositories are configured via the `CANTARA_BUNDLED_REPOS` environment
    /// variable at build time (set in CI/CD). They are:
    /// - Added as `GitHub`-type repositories with `removable: false`
    /// - Not modifiable or deletable by the user in WebAssembly
    /// - Automatically skip the welcome wizard when present
    ///
    /// This method is a no-op when no repositories were bundled at build time.
    #[cfg(target_arch = "wasm32")]
    pub fn ensure_bundled_repos(&mut self) {
        use crate::logic::bundled_repos;

        let bundled = bundled_repos::get_bundled_repos();
        if bundled.is_empty() {
            return;
        }

        // Always populate WEB_FILES with embedded data (in-memory, lost on page reload)
        let files = bundled_repos::get_bundled_files();
        if !files.is_empty() {
            WEB_FILES.with(|web_files| {
                let mut web_files = web_files.borrow_mut();
                for (path, data) in files {
                    web_files
                        .entry(path.to_string())
                        .or_insert_with(|| data.to_vec());
                }
            });
        }

        let mut changed = false;

        // Add bundled repos to settings if not already present
        for &(owner, repo) in bundled {
            let already_exists = self.repositories.iter().any(|r| {
                matches!(
                    &r.repository_type,
                    RepositoryType::GitHub { owner: o, repo: r, .. }
                    if o == owner && r == repo
                )
            });
            if !already_exists {
                let mut new_repo =
                    Repository::new_github(owner.to_string(), repo.to_string(), None);
                new_repo.removable = false;
                self.repositories.push(new_repo);
                changed = true;
            }
        }

        // Ensure bundled repos are always non-removable (even if loaded from storage)
        for r in &mut self.repositories {
            if let RepositoryType::GitHub {
                owner, repo: rname, ..
            } = &r.repository_type
            {
                if bundled
                    .iter()
                    .any(|&(o, n)| o == owner.as_str() && n == rname.as_str())
                {
                    if r.removable {
                        r.removable = false;
                        changed = true;
                    }
                }
            }
        }

        // Skip the wizard when bundled repos are present
        if !self.wizard_completed {
            self.wizard_completed = true;
            changed = true;
        }

        if changed {
            self.save();
        }
    }

    /// No-op on non-WASM targets. Bundled repos are only used for WebAssembly builds.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn ensure_bundled_repos(&mut self) {
        // Bundled repos are only relevant for WASM targets
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
        #[cfg(not(target_arch = "wasm32"))]
        match &self.repository_type {
            RepositoryType::RemoteZip(url) => {
                RepositoryType::cleanup_temp_dir(url);
            }
            RepositoryType::GitHub { owner, repo, .. } => {
                RepositoryType::cleanup_temp_dir(&RepositoryType::github_cache_key(owner, repo));
            }
            _ => {}
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

    /// Creates a new repository backed by a GitHub repository via the GitHub API.
    ///
    /// # Arguments
    /// * `owner` - The owner of the GitHub repository (user or organization)
    /// * `repo` - The name of the GitHub repository
    /// * `token` - An optional personal access token for private repositories
    ///
    /// # Returns
    /// A new `Repository` instance configured to use a GitHub repository
    pub fn new_github(owner: String, repo: String, token: Option<String>) -> Self {
        let name = format!("{}/{}", owner, repo);
        Repository {
            name,
            removable: true,
            writing_permissions: false, // GitHub repositories are read-only
            repository_type: RepositoryType::GitHub { owner, repo, token },
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RepositoryType {
    /// A repository that is a local folder represented by a file path.
    LocaleFilePath(String),

    /// A repository that is a remote URL.
    /// Hint: This is not implemented yet!
    Remote(String),

    /// A repository that is a remote ZIP file which is downloaded and extracted temporarily.
    /// The String contains the URL to the ZIP file.
    RemoteZip(String),

    /// A repository that is a GitHub repository, accessed via the GitHub API.
    /// The zipball of the default branch (main/master) is downloaded and extracted.
    GitHub {
        /// The owner of the GitHub repository (user or organization)
        owner: String,
        /// The name of the GitHub repository
        repo: String,
        /// An optional personal access token for authenticating with private repositories
        token: Option<String>,
    },
}

// On non-WASM platforms, extracted ZIPs are stored in TempDir instances on the filesystem.
#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static TEMP_DIRS: std::cell::RefCell<std::collections::HashMap<String, TempDir>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

// On WASM, extracted ZIP contents are stored in memory (virtual filesystem).
#[cfg(target_arch = "wasm32")]
thread_local! {
    static WEB_FILES: std::cell::RefCell<std::collections::HashMap<String, Vec<u8>>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Strips a `refs/heads/` or `refs/tags/` prefix from a git ref string,
/// returning just the branch or tag name.
#[cfg(any(target_arch = "wasm32", test))]
fn normalize_git_ref<'a>(ref_part: &'a str) -> &'a str {
    ref_part
        .strip_prefix("refs/heads/")
        .or_else(|| ref_part.strip_prefix("refs/tags/"))
        .unwrap_or(ref_part)
}

/// On WASM, transforms GitHub archive URLs to GitHub API zipball URLs
/// which support CORS headers required by browser fetch.
/// Non-GitHub URLs are returned unchanged.
#[cfg(any(target_arch = "wasm32", test))]
fn cors_friendly_url(url: &str) -> String {
    // Transform https://github.com/{owner}/{repo}/archive/... to
    // https://api.github.com/repos/{owner}/{repo}/zipball/{ref}
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() == 3 {
            let owner = parts[0];
            let repo = parts[1];
            if let Some(archive_path) = parts[2].strip_prefix("archive/") {
                let ref_part = archive_path.strip_suffix(".zip").unwrap_or(archive_path);
                return format!(
                    "https://api.github.com/repos/{}/{}/zipball/{}",
                    owner, repo, normalize_git_ref(ref_part)
                );
            }
        }
    }
    // Transform https://codeload.github.com/{owner}/{repo}/legacy.zip/{ref} and
    // https://codeload.github.com/{owner}/{repo}/zip/{ref} to
    // https://api.github.com/repos/{owner}/{repo}/zipball/{ref}
    if let Some(rest) = url.strip_prefix("https://codeload.github.com/") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() == 3 {
            let owner = parts[0];
            let repo = parts[1];
            let ref_path = parts[2]
                .strip_prefix("legacy.zip/")
                .or_else(|| parts[2].strip_prefix("zip/"));
            if let Some(ref_part) = ref_path {
                return format!(
                    "https://api.github.com/repos/{}/{}/zipball/{}",
                    owner, repo, normalize_git_ref(ref_part)
                );
            }
        }
    }
    url.to_string()
}

impl RepositoryType {
    /// Cleans up the temporary directory for a specific URL (desktop only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn cleanup_temp_dir(url: &str) {
        TEMP_DIRS.with(|temp_dirs| {
            let mut temp_dirs = temp_dirs.borrow_mut();
            if temp_dirs.remove(url).is_some() {
                log::info!("Cleaned up temporary directory for URL: {}", url);
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub fn cleanup_temp_dir(_url: &str) {}

    /// Cleans up all temporary directories (desktop only).
    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(target_arch = "wasm32")]
    pub fn cleanup_all_temp_dirs() {}

    /// Returns the GitHub API zipball URL for a given owner and repo.
    /// This URL fetches the default branch's latest commit as a ZIP archive.
    pub fn github_zipball_url(owner: &str, repo: &str) -> String {
        format!("https://api.github.com/repos/{}/{}/zipball", owner, repo)
    }

    /// Returns a cache key for a GitHub repository, used for temporary directory management.
    pub fn github_cache_key(owner: &str, repo: &str) -> String {
        format!("github://{}/{}", owner, repo)
    }

    /// Parses a GitHub repository identifier string (e.g. "owner/repo" or "https://github.com/owner/repo")
    /// into (owner, repo) tuple. Returns None if the format is invalid.
    pub fn parse_github_repo(input: &str) -> Option<(String, String)> {
        let trimmed = input.trim().trim_end_matches('/');

        // Try to parse as a full GitHub URL
        if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
            let parts: Vec<&str> = rest.splitn(3, '/').collect();
            if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
                return Some((parts[0].to_string(), parts[1].to_string()));
            }
        }

        // Try to parse as "owner/repo"
        let parts: Vec<&str> = trimmed.splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }

        None
    }

    /// Parses a GitHub archive ZIP URL (e.g. from a github.com/archive or codeload.github.com
    /// download link) into an `(owner, repo)` tuple. Returns `None` for non-GitHub URLs.
    ///
    /// Handles:
    /// - `https://github.com/{owner}/{repo}/archive/...`
    /// - `https://codeload.github.com/{owner}/{repo}/legacy.zip/...`
    /// - `https://codeload.github.com/{owner}/{repo}/zip/...`
    pub fn parse_github_from_zip_url(url: &str) -> Option<(String, String)> {
        // https://github.com/{owner}/{repo}/archive/...
        if let Some(rest) = url.strip_prefix("https://github.com/") {
            let parts: Vec<&str> = rest.splitn(3, '/').collect();
            if parts.len() == 3
                && !parts[0].is_empty()
                && !parts[1].is_empty()
                && parts[2].starts_with("archive/")
            {
                return Some((parts[0].to_string(), parts[1].to_string()));
            }
        }
        // https://codeload.github.com/{owner}/{repo}/legacy.zip/... or /zip/...
        if let Some(rest) = url.strip_prefix("https://codeload.github.com/") {
            let parts: Vec<&str> = rest.splitn(3, '/').collect();
            if parts.len() == 3 && !parts[0].is_empty() && !parts[1].is_empty() {
                if parts[2].starts_with("legacy.zip/") || parts[2].starts_with("zip/") {
                    return Some((parts[0].to_string(), parts[1].to_string()));
                }
            }
        }
        None
    }

    /// Get files which are provided by the repository.
    /// On WASM, local file paths are not supported; only remote ZIP repositories work.
    pub fn get_files(&self) -> Vec<SourceFile> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self {
                RepositoryType::LocaleFilePath(path_string) => {
                    get_source_files(Path::new(&path_string))
                }
                RepositoryType::RemoteZip(url) => {
                    let mut files = vec![];
                    TEMP_DIRS.with(|temp_dirs| {
                        let mut temp_dirs = temp_dirs.borrow_mut();
                        if let Some(temp_dir) = temp_dirs.get(url) {
                            log::info!("Using existing temporary directory for URL: {}", url);
                            files = get_source_files(temp_dir.path());
                        } else {
                            log::info!("Downloading and extracting ZIP file from URL: {}", url);
                            match self.download_and_extract_zip(url, None) {
                                Ok(temp_dir) => {
                                    let path = temp_dir.path().to_path_buf();
                                    log::info!("Extracted ZIP file to temporary directory: {:?}", path);
                                    files = get_source_files(&path);
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
                RepositoryType::GitHub { owner, repo, token } => {
                    let cache_key = Self::github_cache_key(owner, repo);
                    let url = Self::github_zipball_url(owner, repo);
                    let mut files = vec![];
                    TEMP_DIRS.with(|temp_dirs| {
                        let mut temp_dirs = temp_dirs.borrow_mut();
                        if let Some(temp_dir) = temp_dirs.get(&cache_key) {
                            log::info!("Using existing temporary directory for GitHub repo: {}/{}", owner, repo);
                            files = get_source_files(temp_dir.path());
                        } else {
                            log::info!("Downloading GitHub repository: {}/{}", owner, repo);
                            match self.download_and_extract_zip(&url, token.as_deref()) {
                                Ok(temp_dir) => {
                                    let path = temp_dir.path().to_path_buf();
                                    log::info!("Extracted GitHub repo to temporary directory: {:?}", path);
                                    files = get_source_files(&path);
                                    temp_dirs.insert(cache_key, temp_dir);
                                }
                                Err(e) => {
                                    log::error!("Failed to download GitHub repository: {}", e);
                                }
                            }
                        }
                    });
                    files
                }
                _ => vec![],
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            // Synchronous local file paths are not available on web.
            // Use get_files_async() instead for RemoteZip repositories.
            let prefix = self.web_vfs_prefix();
            if prefix.is_empty() {
                return vec![];
            }
            WEB_FILES.with(|files| {
                files
                    .borrow()
                    .keys()
                    .filter(|k| k.starts_with(&prefix))
                    .filter_map(|path| SourceFile::from_web_path(path))
                    .collect()
            })
        }
    }

    /// Get files which are provided by the repository asynchronously.
    pub async fn get_files_async(&self) -> Vec<SourceFile> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self {
                RepositoryType::LocaleFilePath(path_string) => {
                    get_source_files(Path::new(&path_string))
                }
                RepositoryType::RemoteZip(url) => {
                    let mut files = vec![];
                    TEMP_DIRS.with(|temp_dirs| {
                        let temp_dirs = temp_dirs.borrow_mut();
                        if let Some(temp_dir) = temp_dirs.get(url) {
                            log::info!("Using existing temporary directory for URL: {}", url);
                            files = get_source_files(temp_dir.path());
                        }
                    });
                    if files.is_empty() {
                        log::info!("Downloading and extracting ZIP file from URL: {}", url);
                        match self.download_and_extract_zip_async(url, None).await {
                            Ok(temp_dir) => {
                                let path = temp_dir.path().to_path_buf();
                                log::info!("Extracted ZIP file to temporary directory: {:?}", path);
                                files = get_source_files(&path);
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
                RepositoryType::GitHub { owner, repo, token } => {
                    let cache_key = Self::github_cache_key(owner, repo);
                    let url = Self::github_zipball_url(owner, repo);
                    let mut files = vec![];
                    TEMP_DIRS.with(|temp_dirs| {
                        let temp_dirs = temp_dirs.borrow_mut();
                        if let Some(temp_dir) = temp_dirs.get(&cache_key) {
                            log::info!("Using existing temporary directory for GitHub repo: {}/{}", owner, repo);
                            files = get_source_files(temp_dir.path());
                        }
                    });
                    if files.is_empty() {
                        log::info!("Downloading GitHub repository: {}/{}", owner, repo);
                        match self.download_and_extract_zip_async(&url, token.as_deref()).await {
                            Ok(temp_dir) => {
                                let path = temp_dir.path().to_path_buf();
                                log::info!("Extracted GitHub repo to temporary directory: {:?}", path);
                                files = get_source_files(&path);
                                TEMP_DIRS.with(|temp_dirs| {
                                    let mut temp_dirs = temp_dirs.borrow_mut();
                                    temp_dirs.insert(cache_key, temp_dir);
                                });
                            }
                            Err(e) => {
                                log::error!("Failed to download GitHub repository: {}", e);
                            }
                        }
                    }
                    files
                }
                _ => vec![],
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            match self {
                RepositoryType::RemoteZip(url) => {
                    let prefix = format!("web-zip://{}", url);
                    // Return cached files if already downloaded
                    let cached: Vec<SourceFile> = WEB_FILES.with(|files| {
                        files
                            .borrow()
                            .keys()
                            .filter(|k| k.starts_with(&prefix))
                            .filter_map(|path| SourceFile::from_web_path(path))
                            .collect()
                    });
                    if !cached.is_empty() {
                        return cached;
                    }
                    // Download and extract in memory
                    let download_url = cors_friendly_url(url);
                    log::info!("Downloading ZIP from URL (web): {}", download_url);
                    self.download_and_extract_zip_wasm(&download_url, &prefix, None).await
                }
                RepositoryType::GitHub { owner, repo, token } => {
                    let prefix = format!("web-github://{}/{}", owner, repo);
                    // Return cached files if already downloaded
                    let cached: Vec<SourceFile> = WEB_FILES.with(|files| {
                        files
                            .borrow()
                            .keys()
                            .filter(|k| k.starts_with(&prefix))
                            .filter_map(|path| SourceFile::from_web_path(path))
                            .collect()
                    });
                    if !cached.is_empty() {
                        return cached;
                    }
                    // Download and extract in memory
                    let download_url = Self::github_zipball_url(owner, repo);
                    log::info!("Downloading GitHub repo (web): {}/{}", owner, repo);
                    self.download_and_extract_zip_wasm(&download_url, &prefix, token.as_deref()).await
                }
                _ => vec![],
            }
        }
    }

    /// Returns the VFS prefix for this repository on WASM.
    #[cfg(target_arch = "wasm32")]
    fn web_vfs_prefix(&self) -> String {
        match self {
            RepositoryType::RemoteZip(url) => format!("web-zip://{}", url),
            RepositoryType::GitHub { owner, repo, .. } => {
                format!("web-github://{}/{}", owner, repo)
            }
            _ => String::new(),
        }
    }

    /// Downloads a ZIP file and extracts it to the WASM in-memory VFS.
    #[cfg(target_arch = "wasm32")]
    async fn download_and_extract_zip_wasm(
        &self,
        download_url: &str,
        prefix: &str,
        token: Option<&str>,
    ) -> Vec<SourceFile> {
        let mut request = AsyncClient::new()
            .get(download_url)
            .header("User-Agent", "Cantara");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        match request.send().await {
            Ok(response) => match response.bytes().await {
                Ok(bytes) => {
                    let cursor = std::io::Cursor::new(bytes);
                    match ZipArchive::new(cursor) {
                        Ok(mut archive) => {
                            for i in 0..archive.len() {
                                if let Ok(mut entry) = archive.by_index(i) {
                                    if entry.name().ends_with('/') {
                                        continue;
                                    }
                                    let name = entry.name().to_string();
                                    let path = format!("{}/{}", prefix, name);
                                    let mut content = Vec::new();
                                    let _ = std::io::Read::read_to_end(&mut entry, &mut content);
                                    WEB_FILES.with(|files| {
                                        files.borrow_mut().insert(path, content);
                                    });
                                }
                            }
                        }
                        Err(e) => log::error!("Failed to parse ZIP archive: {}", e),
                    }
                }
                Err(e) => log::error!("Failed to read response bytes: {}", e),
            },
            Err(e) => log::error!("Failed to download ZIP: {}", e),
        }
        WEB_FILES.with(|files| {
            files
                .borrow()
                .keys()
                .filter(|k| k.starts_with(prefix))
                .filter_map(|path| SourceFile::from_web_path(path))
                .collect()
        })
    }

    /// Reads a file from the web VFS by its virtual path.
    #[cfg(target_arch = "wasm32")]
    pub fn web_read_file(path: &str) -> Option<Vec<u8>> {
        WEB_FILES.with(|files| files.borrow().get(path).cloned())
    }

    /// Downloads a ZIP file from a URL and extracts it to a temporary directory (desktop only).
    /// Optionally includes an authorization token for authenticated requests (e.g. private GitHub repos).
    #[cfg(not(target_arch = "wasm32"))]
    fn download_and_extract_zip(
        &self,
        url: &str,
        token: Option<&str>,
    ) -> Result<TempDir, String> {
        let temp_dir =
            TempDir::new().map_err(|e| format!("Failed to create temporary directory: {}", e))?;
        let zip_path = temp_dir.path().join("download.zip");
        let client = Client::builder()
            .http1_only()
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
        let mut request = client
            .get(url)
            .header("User-Agent", "Cantara");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        let response = request
            .send()
            .map_err(|e| format!("Failed to download ZIP file: {}", e))?;
        if !response.status().is_success() {
            return Err(format!(
                "Failed to download ZIP file: HTTP status {}",
                response.status()
            ));
        }
        let mut file = fs::File::create(&zip_path)
            .map_err(|e| format!("Failed to create temporary file: {}", e))?;
        let content = response
            .bytes()
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        file.write_all(&content)
            .map_err(|e| format!("Failed to write to temporary file: {}", e))?;
        let file = fs::File::open(&zip_path)
            .map_err(|e| format!("Failed to open downloaded ZIP file: {}", e))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("Failed to parse ZIP file: {}", e))?;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to access ZIP entry: {}", e))?;
            let outpath = temp_dir.path().join(file.name());
            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                    }
                }
                let mut outfile = fs::File::create(&outpath)
                    .map_err(|e| format!("Failed to create output file: {}", e))?;
                io::copy(&mut file, &mut outfile)
                    .map_err(|e| format!("Failed to write output file: {}", e))?;
            }
        }
        Ok(temp_dir)
    }

    /// Downloads a ZIP file and extracts it to a temporary directory asynchronously (desktop only).
    /// Optionally includes an authorization token for authenticated requests (e.g. private GitHub repos).
    #[cfg(not(target_arch = "wasm32"))]
    async fn download_and_extract_zip_async(
        &self,
        url: &str,
        token: Option<&str>,
    ) -> Result<TempDir, String> {
        let temp_dir =
            TempDir::new().map_err(|e| format!("Failed to create temporary directory: {}", e))?;
        let zip_path = temp_dir.path().join("download.zip");
        let client = AsyncClient::builder()
            .http1_only()
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
        let mut request = client
            .get(url)
            .header("User-Agent", "Cantara");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to download ZIP file: {}", e))?;
        if !response.status().is_success() {
            return Err(format!(
                "Failed to download ZIP file: HTTP status {}",
                response.status()
            ));
        }
        let mut file = fs::File::create(&zip_path)
            .map_err(|e| format!("Failed to create temporary file: {}", e))?;
        let content = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        file.write_all(&content)
            .map_err(|e| format!("Failed to write to temporary file: {}", e))?;
        let file = fs::File::open(&zip_path)
            .map_err(|e| format!("Failed to open downloaded ZIP file: {}", e))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("Failed to parse ZIP file: {}", e))?;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to access ZIP entry: {}", e))?;
            let outpath = temp_dir.path().join(file.name());
            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                    }
                }
                let mut outfile = fs::File::create(&outpath)
                    .map_err(|e| format!("Failed to create output file: {}", e))?;
                io::copy(&mut file, &mut outfile)
                    .map_err(|e| format!("Failed to write output file: {}", e))?;
            }
        }
        Ok(temp_dir)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn get_settings_file() -> Option<PathBuf> {
    get_settings_folder().map(|settings_folder| settings_folder.join("settings.json"))
}

#[cfg(not(target_arch = "wasm32"))]
fn get_settings_folder() -> Option<PathBuf> {
    // Try config_local_dir first (works on desktop Linux, macOS, Windows).
    // Fall back to data_local_dir and then home_dir for mobile (Android/iOS)
    // where the config dir might not be available.
    dirs::config_local_dir()
        .or_else(dirs::data_local_dir)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .map(|dir| dir.join("cantara"))
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

    #[test]
    fn test_cors_friendly_url_github_heads() {
        assert_eq!(
            cors_friendly_url(
                "https://github.com/reckel-jm/cantara-songrepo/archive/refs/heads/master.zip"
            ),
            "https://api.github.com/repos/reckel-jm/cantara-songrepo/zipball/master"
        );
    }

    #[test]
    fn test_cors_friendly_url_github_tags() {
        assert_eq!(
            cors_friendly_url(
                "https://github.com/owner/repo/archive/refs/tags/v1.0.0.zip"
            ),
            "https://api.github.com/repos/owner/repo/zipball/v1.0.0"
        );
    }

    #[test]
    fn test_cors_friendly_url_github_short() {
        assert_eq!(
            cors_friendly_url("https://github.com/owner/repo/archive/main.zip"),
            "https://api.github.com/repos/owner/repo/zipball/main"
        );
    }

    #[test]
    fn test_cors_friendly_url_non_github() {
        let url = "https://example.com/some/archive.zip";
        assert_eq!(cors_friendly_url(url), url);
    }

    #[test]
    fn test_cors_friendly_url_codeload_legacy_zip_heads() {
        assert_eq!(
            cors_friendly_url(
                "https://codeload.github.com/reckel-jm/cantara-songrepo/legacy.zip/refs/heads/master"
            ),
            "https://api.github.com/repos/reckel-jm/cantara-songrepo/zipball/master"
        );
    }

    #[test]
    fn test_cors_friendly_url_codeload_legacy_zip_tags() {
        assert_eq!(
            cors_friendly_url(
                "https://codeload.github.com/owner/repo/legacy.zip/refs/tags/v1.0.0"
            ),
            "https://api.github.com/repos/owner/repo/zipball/v1.0.0"
        );
    }

    #[test]
    fn test_cors_friendly_url_codeload_zip_heads() {
        assert_eq!(
            cors_friendly_url(
                "https://codeload.github.com/owner/repo/zip/refs/heads/main"
            ),
            "https://api.github.com/repos/owner/repo/zipball/main"
        );
    }

    #[test]
    fn test_ensure_default_presentation_design_when_empty() {
        let mut settings = Settings {
            presentation_designs: vec![],
            ..Default::default()
        };
        assert!(settings.presentation_designs.is_empty());
        settings.ensure_default_presentation_design();
        assert_eq!(settings.presentation_designs.len(), 1);
        assert_eq!(settings.presentation_designs[0].name, "Default");
    }

    #[test]
    fn test_ensure_default_presentation_design_when_not_empty() {
        let mut settings = Settings::default();
        let original_count = settings.presentation_designs.len();
        settings.ensure_default_presentation_design();
        assert_eq!(settings.presentation_designs.len(), original_count);
    }

    #[test]
    fn test_deserialize_empty_presentation_designs_gets_default() {
        let json = r#"{"repositories":[],"wizard_completed":false,"presentation_designs":[],"song_slide_settings":[]}"#;
        let mut settings: Settings = serde_json::from_str(json).unwrap();
        assert!(settings.presentation_designs.is_empty());
        settings.ensure_default_presentation_design();
        assert_eq!(settings.presentation_designs.len(), 1);
    }

    #[test]
    fn test_github_zipball_url() {
        assert_eq!(
            RepositoryType::github_zipball_url("reckel-jm", "cantara-songrepo"),
            "https://api.github.com/repos/reckel-jm/cantara-songrepo/zipball"
        );
    }

    #[test]
    fn test_github_cache_key() {
        assert_eq!(
            RepositoryType::github_cache_key("owner", "repo"),
            "github://owner/repo"
        );
    }

    #[test]
    fn test_parse_github_repo_owner_repo() {
        let (owner, repo) = RepositoryType::parse_github_repo("owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_github_repo_full_url() {
        let (owner, repo) =
            RepositoryType::parse_github_repo("https://github.com/reckel-jm/cantara-songrepo")
                .unwrap();
        assert_eq!(owner, "reckel-jm");
        assert_eq!(repo, "cantara-songrepo");
    }

    #[test]
    fn test_parse_github_repo_full_url_trailing_slash() {
        let (owner, repo) =
            RepositoryType::parse_github_repo("https://github.com/owner/repo/").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_github_repo_invalid() {
        assert!(RepositoryType::parse_github_repo("invalid").is_none());
        assert!(RepositoryType::parse_github_repo("").is_none());
        assert!(RepositoryType::parse_github_repo("/").is_none());
    }

    #[test]
    fn test_repository_new_github() {
        let repo = Repository::new_github(
            "reckel-jm".to_string(),
            "cantara-songrepo".to_string(),
            None,
        );
        assert_eq!(repo.name, "reckel-jm/cantara-songrepo");
        assert!(repo.removable);
        assert!(!repo.writing_permissions);
        assert_eq!(
            repo.repository_type,
            RepositoryType::GitHub {
                owner: "reckel-jm".to_string(),
                repo: "cantara-songrepo".to_string(),
                token: None,
            }
        );
    }

    #[test]
    fn test_repository_new_github_with_token() {
        let repo = Repository::new_github(
            "owner".to_string(),
            "private-repo".to_string(),
            Some("ghp_test123".to_string()),
        );
        assert_eq!(repo.name, "owner/private-repo");
        if let RepositoryType::GitHub { token, .. } = &repo.repository_type {
            assert_eq!(token.as_deref(), Some("ghp_test123"));
        } else {
            panic!("Expected GitHub repository type");
        }
    }

    #[test]
    fn test_github_repository_type_serialization() {
        let repo_type = RepositoryType::GitHub {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            token: Some("token123".to_string()),
        };
        let json = serde_json::to_string(&repo_type).unwrap();
        let deserialized: RepositoryType = serde_json::from_str(&json).unwrap();
        assert_eq!(repo_type, deserialized);
    }

    #[test]
    fn test_add_github_repository() {
        let mut settings = Settings::default();
        settings.add_github_repository(
            "owner".to_string(),
            "repo".to_string(),
            None,
        );
        assert_eq!(settings.repositories.len(), 1);
        assert_eq!(settings.repositories[0].name, "owner/repo");
    }

    #[test]
    fn test_parse_github_from_zip_url_github_archive() {
        let (owner, repo) = RepositoryType::parse_github_from_zip_url(
            "https://github.com/reckel-jm/cantara-songrepo/archive/refs/heads/master.zip",
        )
        .unwrap();
        assert_eq!(owner, "reckel-jm");
        assert_eq!(repo, "cantara-songrepo");
    }

    #[test]
    fn test_parse_github_from_zip_url_codeload_legacy_zip() {
        let (owner, repo) = RepositoryType::parse_github_from_zip_url(
            "https://codeload.github.com/reckel-jm/cantara-songrepo/legacy.zip/refs/heads/master",
        )
        .unwrap();
        assert_eq!(owner, "reckel-jm");
        assert_eq!(repo, "cantara-songrepo");
    }

    #[test]
    fn test_parse_github_from_zip_url_codeload_zip() {
        let (owner, repo) = RepositoryType::parse_github_from_zip_url(
            "https://codeload.github.com/owner/repo/zip/refs/heads/main",
        )
        .unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_github_from_zip_url_non_github() {
        assert!(
            RepositoryType::parse_github_from_zip_url("https://example.com/some.zip").is_none()
        );
    }

    #[test]
    fn test_parse_github_from_zip_url_plain_github_url() {
        // Plain github.com URL without archive path should not match
        assert!(
            RepositoryType::parse_github_from_zip_url("https://github.com/owner/repo").is_none()
        );
    }

    #[test]
    fn test_ensure_bundled_repos_noop_on_desktop() {
        // On non-WASM targets, ensure_bundled_repos should be a no-op
        let mut settings = Settings::default();
        assert!(settings.repositories.is_empty());
        assert!(!settings.wizard_completed);
        settings.ensure_bundled_repos();
        // On desktop, nothing should change
        assert!(settings.repositories.is_empty());
        assert!(!settings.wizard_completed);
    }

    #[test]
    fn test_add_remote_zip_repository_url_github_archive_migrates() {
        let mut settings = Settings::default();
        settings.add_remote_zip_repository_url(
            "https://github.com/owner/repo/archive/refs/heads/main.zip".to_string(),
        );
        // Should be stored as GitHub type, not RemoteZip
        assert_eq!(settings.repositories.len(), 1);
        match &settings.repositories[0].repository_type {
            RepositoryType::GitHub { owner, repo, token } => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert!(token.is_none());
            }
            other => panic!("Expected GitHub repository type, got {:?}", other),
        }
    }

    #[test]
    fn test_add_remote_zip_repository_url_non_github_stays_remote_zip() {
        let mut settings = Settings::default();
        settings.add_remote_zip_repository_url(
            "https://example.com/songs.zip".to_string(),
        );
        // Should remain as RemoteZip
        assert_eq!(settings.repositories.len(), 1);
        match &settings.repositories[0].repository_type {
            RepositoryType::RemoteZip(url) => {
                assert_eq!(url, "https://example.com/songs.zip");
            }
            other => panic!("Expected RemoteZip repository type, got {:?}", other),
        }
    }

    #[test]
    fn test_migrate_github_zip_repos() {
        let mut settings = Settings::default();
        // Add a GitHub archive URL as RemoteZip
        settings.repositories.push(Repository::new_remote_zip(
            "Test".to_string(),
            "https://github.com/owner/repo/archive/refs/heads/main.zip".to_string(),
        ));
        // Add a non-GitHub RemoteZip that should not be migrated
        settings.repositories.push(Repository::new_remote_zip(
            "Other".to_string(),
            "https://example.com/songs.zip".to_string(),
        ));
        settings.migrate_github_zip_repos();

        // First repo should be migrated to GitHub type
        match &settings.repositories[0].repository_type {
            RepositoryType::GitHub { owner, repo, .. } => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
            }
            other => panic!("Expected GitHub repository type, got {:?}", other),
        }
        // Second repo should remain as RemoteZip
        match &settings.repositories[1].repository_type {
            RepositoryType::RemoteZip(url) => {
                assert_eq!(url, "https://example.com/songs.zip");
            }
            other => panic!("Expected RemoteZip repository type, got {:?}", other),
        }
    }
}
