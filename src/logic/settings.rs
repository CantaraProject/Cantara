//! This module contains the logic and structures for managing, loading and saving the program's settings.

use crate::logic::css::{CssFontFamily, CssString};
use crate::logic::sourcefiles::{ImageSourceFile, SourceFile};
use crate::logic::tag_mapping::TagMapping;
// The directory scan and the paths it works on exist on the desktop only; the
// web build reads its repositories from an in-memory VFS instead.
#[cfg(not(target_arch = "wasm32"))]
use crate::logic::sourcefiles::{count_source_files, get_source_files};
// `Path` goes with the directory scan above and so is desktop-only; `PathBuf`
// is not — `repository_folder` hands one back on every target.
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;
use cantara_songlib::slides::SlideSettings;
use dioxus::prelude::*;
use reqwest::Client as AsyncClient;
use rgb::*;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
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
    pub song_slide_settings: Vec<SongSlideSettings>,

    /// Which of the [`presentation_designs`](Self::presentation_designs) an
    /// element is shown with when it does not name one of its own.
    ///
    /// Kept as a position in the list rather than as a copy of the design, so
    /// that editing that design reaches every presentation built from it.
    /// Zero — the first design — is what a settings file written before this
    /// existed reads as, which is the behaviour it had.
    #[serde(default)]
    pub default_design_index: usize,

    /// Which of the [`song_slide_settings`](Self::song_slide_settings) an
    /// element is divided into slides by when it does not name its own. See
    /// [`default_design_index`](Self::default_design_index).
    #[serde(default)]
    pub default_slide_settings_index: usize,

    /// Which repository an imported selection puts songs into that the
    /// library does not have yet.
    ///
    /// Only a local folder can be written to, so a position naming any other
    /// kind of repository is read as "the first local one" — see
    /// [`Self::import_repository_path`].
    #[serde(default)]
    pub import_repository_index: usize,

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

    /// The order of the source-type filter buttons in the selection sidebar.
    /// When `None` or empty, the default order (Songs → Pictures → PDFs) is used.
    #[serde(default)]
    pub sidebar_order: Vec<SelectionSidebarType>,

    /// Whether the live preview is docked into the presentation design editor
    /// on narrow screens. Wide screens always show it beside the settings, so
    /// this only records the choice made where space is tight.
    #[serde(default = "default_show_design_preview")]
    pub show_design_preview: bool,

    /// How a running presentation is offered to browsers on the network.
    ///
    /// Whether it *is* offered is not kept here: streaming is switched on for
    /// the presentation at hand, next to the rest of its options, and is not
    /// something the program should quietly start doing again next time it
    /// opens. These are the settings that describe *how*, and they are worth
    /// keeping.
    #[serde(default)]
    pub stream: StreamSettings,

    /// Tag names this installation reads as other tag names.
    ///
    /// A library grown from several collections calls the same thing by
    /// several names, and a meta line asking for `{{composer}}` stays empty
    /// for the files that say `author`. These rules close that gap at the
    /// moment the slides are built — no file is touched, and a rule removed
    /// here leaves everything exactly as it was. See
    /// [`crate::logic::tag_mapping`].
    #[serde(default)]
    pub tag_mappings: Vec<TagMapping>,
}

/// What the streaming server is set up to do, when it is switched on.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct StreamSettings {
    /// The port to listen on.
    pub port: u16,

    /// What a viewer has to type before they are shown anything.
    ///
    /// Empty means no password at all: anyone on the network who opens the
    /// address can watch, which is usually the point in a hall full of people.
    /// It travels in the clear either way — this is plain HTTP on a local
    /// network, and a password here keeps the curious out, not an attacker.
    pub password: String,

    /// Which of [`Settings::presentation_designs`] the phones are shown, as an
    /// index into that list. `None` — the ordinary case — means they are shown
    /// the same design as the projection.
    ///
    /// An index rather than a copy, so that editing a design reaches the
    /// stream as it reaches the wall. The two lists a user maintains are
    /// exactly the choice on offer here: a stream design is a presentation
    /// design, built and previewed in the same editor.
    #[serde(default)]
    pub design_index: Option<usize>,

    /// The same, for [`Settings::song_slide_settings`] — how a song is divided
    /// into slides for a phone.
    ///
    /// What is chosen here is not always what is used: the projection is the
    /// reference, and the line wrap is reconciled against it by
    /// [`crate::logic::stream_view::stream_slide_settings`].
    #[serde(default)]
    pub slide_settings_index: Option<usize>,
}

impl Default for StreamSettings {
    fn default() -> Self {
        StreamSettings {
            port: default_stream_port(),
            password: String::new(),
            design_index: None,
            slide_settings_index: None,
        }
    }
}

/// The port streaming listens on unless it is changed.
///
/// High enough to need no privileges, and not one of the ports something else
/// on a church laptop is likely to have taken.
pub const fn default_stream_port() -> u16 {
    8420
}

/// The design preview starts docked: seeing the effect of a setting is the
/// point of the editor, and it can be folded away when space is tight.
fn default_show_design_preview() -> bool {
    true
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

/// Represents an individual source-type button in the selection sidebar.
/// The order of these values in `Settings::sidebar_order` determines the
/// display order of the sidebar icons.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SelectionSidebarType {
    Songs,
    Pictures,
    Pdfs,
    Markdown,
    Videos,
}

/// Returns the default sidebar order: Songs → Pictures → Videos → PDFs → Markdown.
///
/// Videos sit beside pictures because that is what they are to somebody
/// building a service: something to show rather than something to read.
///
/// A user who has already arranged the sidebar has their order kept, and it
/// will not mention videos — see [`Settings::ensure_sidebar_order`], which adds
/// what is missing rather than replacing what is there.
pub fn default_sidebar_order() -> Vec<SelectionSidebarType> {
    vec![
        SelectionSidebarType::Songs,
        SelectionSidebarType::Pictures,
        SelectionSidebarType::Videos,
        SelectionSidebarType::Pdfs,
        SelectionSidebarType::Markdown,
    ]
}

/// Specifies what happens after the last slide of a chapter when a timer is active.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default, Debug)]
pub enum AfterLastSlide {
    /// Go to the next slide in the next chapter (if available), default behavior.
    #[default]
    GoToNextChapter,
    /// Restart from the first slide of the current chapter.
    RestartCurrentChapter,
}

/// Settings for the automatic slide advance timer.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct SlideTimerSettings {
    /// Number of seconds before automatically advancing to the next slide.
    pub timer_seconds: u32,
    /// What to do after reaching the last slide of the chapter.
    pub after_last_slide: AfterLastSlide,
}

impl Default for SlideTimerSettings {
    fn default() -> Self {
        SlideTimerSettings {
            timer_seconds: 5,
            after_last_slide: AfterLastSlide::default(),
        }
    }
}

/// The transition effect to use between slides.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default, Debug)]
pub enum SlideTransition {
    /// No transition – slide appears instantly.
    None,
    /// Fade in (default, previously hardcoded).
    #[default]
    Fade,
    /// Slide in from the right (new slide enters from right).
    SlideFromRight,
    /// Slide in from the left (new slide enters from left).
    SlideFromLeft,
    /// Zoom in from the center.
    ZoomIn,
    /// Transform one slide into the next: text that appears on both slides
    /// travels to its new place instead of being faded out and back in.
    Morph,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repositories: vec![],
            wizard_completed: false,
            presentation_designs: default_presentation_design_vec(),
            song_slide_settings: default_song_slide_vec(),
            default_design_index: 0,
            default_slide_settings_index: 0,
            import_repository_index: 0,
            always_start_fullscreen: default_always_start_fullscreen(),
            presentation_screen: None,
            presenter_screen: None,
            show_presenter_console: default_show_presenter_console(),
            presenter_console_in_main_window: default_presenter_console_in_main_window(),
            presenter_console_view: PresenterConsoleView::default(),
            stream: StreamSettings::default(),
            presenter_console_grid_size: default_presenter_console_grid_size(),
            sidebar_order: default_sidebar_order(),
            show_design_preview: default_show_design_preview(),
            tag_mappings: Vec::new(),
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
fn default_song_slide_vec() -> Vec<SongSlideSettings> {
    vec![SongSlideSettings::default()]
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

/// Bring a stored settings document up to the current shape.
///
/// Cantara 0.3 and earlier wrote `show_meta_information` as one of the strings
/// `"None"`, `"FirstSlide"`, `"LastSlide"` or `"FirstSlideAndLastSlide"`,
/// because the song library modelled it as an enum. Version 0.2 of the library
/// replaced that with a struct of three independent flags so that the title
/// slide became selectable on its own.
///
/// Without this step the whole settings file would fail to parse and
/// [`Settings::load`] would fall back to the defaults, silently discarding
/// every repository, presentation design and font the user had set up.
fn migrate_settings_json(json: &str) -> String {
    let Ok(mut document) = serde_json::from_str::<serde_json::Value>(json) else {
        // Not valid JSON at all; leave it to the caller's error handling.
        return json.to_string();
    };

    fn upgrade(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(meta) = map.get("show_meta_information")
                    && let Some(name) = meta.as_str() {
                        let (title_slide, first_slide, last_slide) = match name {
                            "FirstSlide" => (false, true, false),
                            "LastSlide" => (false, false, true),
                            "FirstSlideAndLastSlide" => (false, true, true),
                            // "None" and anything unrecognised mean "nowhere".
                            _ => (false, false, false),
                        };
                        map.insert(
                            "show_meta_information".to_string(),
                            serde_json::json!({
                                "title_slide": title_slide,
                                "first_slide": first_slide,
                                "last_slide": last_slide,
                            }),
                        );
                    }
                for nested in map.values_mut() {
                    upgrade(nested);
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(upgrade),
            _ => {}
        }
    }

    upgrade(&mut document);
    serde_json::to_string(&document).unwrap_or_else(|_| json.to_string())
}

/// Moves a chosen position along after the entry at `removed` has been deleted.
///
/// The chosen entry itself becoming "no choice" is deliberate: the thing that
/// was picked is gone, and the alternative — leaving the position and letting
/// it point past the end — comes back to life the moment the list grows again,
/// silently choosing something the user never picked.
fn forget_choice(chosen: &mut Option<usize>, removed: usize) {
    match *chosen {
        Some(index) if index == removed => *chosen = None,
        Some(index) if index > removed => *chosen = Some(index - 1),
        _ => {}
    }
}

/// The same, for a choice that has no "none" to fall back to and so falls back
/// to the first.
fn shift_default(chosen: &mut usize, removed: usize) {
    if *chosen == removed {
        *chosen = 0;
    } else if *chosen > removed {
        *chosen -= 1;
    }
}

impl Settings {
    /// Load settings from storage or creates a new default settings if
    /// the program is run for the first time.
    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let json = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item("cantara-settings").ok().flatten());
            let mut settings = match json {
                Some(j) => serde_json::from_str(&migrate_settings_json(&j)).unwrap_or_default(),
                None => Self::default(),
            };
            settings.ensure_default_presentation_design();
            settings.ensure_slide_settings_for_designs();
            settings.ensure_sidebar_order();
            settings.migrate_github_zip_repos();
            settings.ensure_bundled_repos();
            settings
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            // A settings file that cannot be read or understood is not worth
            // reporting: the defaults are a working configuration, and the
            // wizard picks the user up from there.
            //
            // Whether there *is* one is a different question from whether it
            // could be read, and the two are kept apart here: a file that is
            // there but unreadable is not a first start, and must not lead to
            // a Cantara 2 configuration being imported over settings that are
            // merely damaged.
            let stored = get_settings_file().and_then(|file| std::fs::read_to_string(file).ok());
            let first_start = stored.is_none();
            let mut settings: Settings = stored
                .and_then(|content| serde_json::from_str(&migrate_settings_json(&content)).ok())
                .unwrap_or_default();
            settings.ensure_default_presentation_design();
            settings.ensure_slide_settings_for_designs();
            settings.ensure_sidebar_order();
            settings.migrate_github_zip_repos();

            // Nobody starts with an empty program if they have been using
            // Cantara 2: their library, design and metadata line are on this
            // machine already. See [`crate::logic::legacy_import`].
            if first_start
                && let Some(report) = crate::logic::legacy_import::import_from_cantara_2(&mut settings)
            {
                crate::logic::legacy_import::leave_notice(report);
            }

            settings
        }
    }

    /// Save the current settings to storage.
    ///
    /// A failure is written to the log and otherwise passed over: most of the
    /// places that save do so as a side effect of an edit, and a dialog there
    /// would interrupt what the user is doing for something they can do
    /// nothing about. Where losing the settings is the point — leaving the
    /// settings page — [`Settings::try_save`] says what went wrong instead.
    pub fn save(&self) {
        if let Err(error) = self.try_save() {
            dioxus::logger::tracing::error!("the settings could not be saved: {error}");
        }
    }

    /// Save the current settings to storage, and say why if that did not work.
    ///
    /// The message is meant to be shown: it names the file or the storage that
    /// could not be written, so that a settings directory that is read-only or
    /// full is something the user can act on rather than guess at.
    pub fn try_save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("the settings could not be encoded: {error}"))?;

        #[cfg(target_arch = "wasm32")]
        {
            let storage = web_sys::window()
                .and_then(|window| window.local_storage().ok().flatten())
                .ok_or_else(|| "this browser offers no local storage".to_string())?;
            storage
                .set_item("cantara-settings", &json)
                .map_err(|_| "the browser refused to store the settings".to_string())?;
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let file = get_settings_file()
                .ok_or_else(|| "no settings file location could be determined".to_string())?;

            if let Some(folder) = get_settings_folder()
                && let Err(error) = fs::create_dir_all(&folder)
            {
                return Err(format!("{} could not be created: {error}", folder.display()));
            }

            std::fs::write(&file, json)
                .map_err(|error| format!("{} could not be written: {error}", file.display()))
        }
    }

    /// Add a new repository folder given as String to the settings if the repository is not already present (avoiding duplicates).
    pub fn add_repository_folder(&mut self, folder: String) {
        let name: &str = get_last_dir(&folder).unwrap_or(&folder);

        self.repositories
            .push(Repository::new_local_folder(name.into(), folder));
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

    /// The files of the given repositories.
    ///
    /// Taking the repositories rather than the whole settings lets a caller
    /// depend on just those: the scan reads every file to fingerprint it and
    /// parses every PDF for the search cache, so it must not be triggered by an
    /// unrelated setting.
    pub async fn sourcefiles_of_async(repositories: &[Repository]) -> Vec<SourceFile> {
        let mut source_files: Vec<SourceFile> = vec![];

        for repo in repositories {
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

    /// The design an element is shown with when it does not name one itself.
    ///
    /// A position that no longer has a design behind it — the chosen one was
    /// deleted since — falls back to the first rather than leaving the
    /// presentation without a design in the middle of a service.
    pub fn default_presentation_design(&self) -> PresentationDesign {
        self.presentation_designs
            .get(self.default_design_index)
            .or_else(|| self.presentation_designs.first())
            .cloned()
            .unwrap_or_default()
    }

    /// The slide division an element is given when it does not name one
    /// itself. Falls back like [`Self::default_presentation_design`].
    pub fn default_song_slide_settings(&self) -> SlideSettings {
        self.song_slide_settings
            .get(self.default_slide_settings_index)
            .or_else(|| self.song_slide_settings.first())
            .map(|named| named.settings.clone())
            .unwrap_or_default()
    }

    /// Which repositories an import could be written into.
    ///
    /// A downloaded one is a copy of somebody else's library that is unpacked
    /// again on every start, so writing a song into it would lose the song.
    /// Only a folder on this computer is offered.
    pub fn writable_repositories(&self) -> Vec<(usize, &Repository)> {
        self.repositories
            .iter()
            .enumerate()
            .filter(|(_, repository)| {
                matches!(repository.repository_type, RepositoryType::LocaleFilePath(_))
                    && repository.writing_permissions
            })
            .collect()
    }

    /// The folder a repository is, where it is one on this computer.
    ///
    /// `None` for a downloaded repository, which is unpacked afresh on every
    /// start — a song written into one would be gone by the next.
    pub fn repository_folder(&self, index: usize) -> Option<PathBuf> {
        match self
            .repositories
            .get(index)
            .map(|repository| &repository.repository_type)
        {
            Some(RepositoryType::LocaleFilePath(path)) => Some(PathBuf::from(path)),
            _ => None,
        }
    }

    /// Deletes the design at `index`, along with the slide division that
    /// belongs to it, and moves every stored choice along with them.
    ///
    /// The choices — the general default, and what the streamed view is set to
    /// — are kept as positions in these lists, and `Vec::remove` shifts
    /// everything after the hole down by one. Deleting a design therefore
    /// silently re-points every choice that sat after it at its neighbour: a
    /// service set up to project design 3 would quietly start projecting what
    /// used to be design 4. Nothing catches this later, because the position
    /// is perfectly valid — it simply means something else now.
    ///
    /// Doing the deletion here rather than at the button is the point. The
    /// bookkeeping belongs with the lists it is about, where it cannot be left
    /// out of a second caller.
    pub fn delete_presentation_design(&mut self, index: usize) {
        if index >= self.presentation_designs.len() {
            return;
        }

        self.presentation_designs.remove(index);
        forget_choice(&mut self.stream.design_index, index);
        shift_default(&mut self.default_design_index, index);

        // The two lists are kept in step, but only the design list is known to
        // have had this position — so the slide divisions move only if one was
        // actually removed.
        if index < self.song_slide_settings.len() {
            self.song_slide_settings.remove(index);
            forget_choice(&mut self.stream.slide_settings_index, index);
            shift_default(&mut self.default_slide_settings_index, index);
        }

        self.ensure_slide_settings_for_designs();
    }

    /// Ensures that there are at least as many slide settings as presentation designs.
    /// If there are fewer slide settings, adds default slide settings until there are enough.
    pub fn ensure_slide_settings_for_designs(&mut self) {
        let design_count = self.presentation_designs.len();
        let slide_count = self.song_slide_settings.len();

        if slide_count < design_count {
            // Add default slide settings until there are at least as many as presentation designs
            for _ in 0..(design_count - slide_count) {
                self.song_slide_settings.push(SongSlideSettings::default());
            }
        }
    }

    /// Puts any sidebar entry the saved order does not mention at the end of
    /// it.
    ///
    /// The order is the user's arrangement and is kept as they left it. But it
    /// was written out when Cantara had fewer kinds of source than it has now,
    /// and an entry that is in no saved order is an icon that never appears —
    /// which is how adding videos would have hidden them from everybody who had
    /// ever touched the sidebar.
    ///
    /// Appended rather than inserted in the default position: the point of a
    /// saved order is that things stay where they were put, and the new one has
    /// nowhere it has to be.
    pub fn ensure_sidebar_order(&mut self) {
        if self.sidebar_order.is_empty() {
            self.sidebar_order = default_sidebar_order();
            return;
        }

        for entry in default_sidebar_order() {
            if !self.sidebar_order.contains(&entry) {
                self.sidebar_order.push(entry);
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
            if let RepositoryType::RemoteZip(url) = &repo.repository_type
                && let Some((owner, repo_name)) =
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
                && bundled
                    .iter()
                    .any(|&(o, n)| o == owner.as_str() && n == rname.as_str())
                    && r.removable {
                        r.removable = false;
                        changed = true;
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

    /// How many files this repository holds, for the settings page to show.
    ///
    /// Deliberately *not* the length of [`RepositoryType::get_files_async`]:
    /// that reads and hashes every file of the library, which is seconds of
    /// work for a number next to a folder name — and the settings page asked
    /// for it once per repository every time it was drawn.
    pub async fn get_source_file_count_async(&self) -> usize {
        self.repository_type.get_file_count_async().await
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
fn normalize_git_ref(ref_part: &str) -> &str {
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

    /// Returns the GitHub API zipball URL for a given owner and repo.
    /// This URL fetches the default branch's latest commit as a ZIP archive.
    pub fn github_zipball_url(owner: &str, repo: &str) -> String {
        format!("https://api.github.com/repos/{}/{}/zipball", owner, repo)
    }

    #[cfg(not(target_arch = "wasm32"))]
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
            if parts.len() == 3 && !parts[0].is_empty() && !parts[1].is_empty()
                && (parts[2].starts_with("legacy.zip/") || parts[2].starts_with("zip/")) {
                    return Some((parts[0].to_string(), parts[1].to_string()));
                }
        }
        None
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
                            files = get_source_files(&archive_content_root(temp_dir.path()));
                        }
                    });
                    if files.is_empty() {
                        log::info!("Downloading and extracting ZIP file from URL: {}", url);
                        match self.download_and_extract_zip_async(url, None).await {
                            Ok(temp_dir) => {
                                let path = temp_dir.path().to_path_buf();
                                log::info!("Extracted ZIP file to temporary directory: {:?}", path);
                                files = get_source_files(&archive_content_root(&path));
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
                            files = get_source_files(&archive_content_root(temp_dir.path()));
                        }
                    });
                    if files.is_empty() {
                        log::info!("Downloading GitHub repository: {}/{}", owner, repo);
                        match self.download_and_extract_zip_async(&url, token.as_deref()).await {
                            Ok(temp_dir) => {
                                let path = temp_dir.path().to_path_buf();
                                log::info!("Extracted GitHub repo to temporary directory: {:?}", path);
                                files = get_source_files(&archive_content_root(&path));
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
                            .filter_map(|path| Self::source_file_from_web_path_with_md5(path, &prefix))
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
                            .filter_map(|path| Self::source_file_from_web_path_with_md5(path, &prefix))
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

    /// How many files this repository holds.
    ///
    /// Where the files are already there — a local folder, or an archive that
    /// has been unpacked once already — only their names are looked at. That
    /// is all a count needs, and it is what keeps the settings page from
    /// reading the whole library from disk every time it is drawn. A
    /// repository that has not been fetched yet still has to be fetched, and
    /// then the ordinary scan answers.
    pub async fn get_file_count_async(&self) -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self {
                RepositoryType::LocaleFilePath(path_string) => {
                    return count_source_files(Path::new(&path_string));
                }
                RepositoryType::RemoteZip(url) => {
                    if let Some(count) = Self::count_in_temp_dir(url) {
                        return count;
                    }
                }
                RepositoryType::GitHub { owner, repo, .. } => {
                    let cache_key = Self::github_cache_key(owner, repo);
                    if let Some(count) = Self::count_in_temp_dir(&cache_key) {
                        return count;
                    }
                }
                _ => return 0,
            }
        }

        self.get_files_async().await.len()
    }

    /// How many files the already-unpacked copy of `cache_key` holds, if there
    /// is one.
    #[cfg(not(target_arch = "wasm32"))]
    fn count_in_temp_dir(cache_key: &str) -> Option<usize> {
        TEMP_DIRS.with(|temp_dirs| {
            temp_dirs
                .borrow()
                .get(cache_key)
                .map(|temp_dir| count_source_files(&archive_content_root(temp_dir.path())))
        })
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
                            // The same wrapper directory the desktop strips
                            // after extracting — see `archive_content_root`.
                            let wrapper = archive_wrapper_directory(&archive);
                            for i in 0..archive.len() {
                                if let Ok(mut entry) = archive.by_index(i) {
                                    if entry.name().ends_with('/') {
                                        continue;
                                    }
                                    let name = entry.name().to_string();
                                    let name = match &wrapper {
                                        Some(wrapper) => name
                                            .strip_prefix(wrapper.as_str())
                                            .unwrap_or(&name)
                                            .to_string(),
                                        None => name,
                                    };
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
                .filter_map(|path| Self::source_file_from_web_path_with_md5(path, prefix))
                .collect()
        })
    }

    /// Reads a file from the web VFS by its virtual path.
    #[cfg(target_arch = "wasm32")]
    pub fn web_read_file(path: &str) -> Option<Vec<u8>> {
        WEB_FILES.with(|files| files.borrow().get(path).cloned())
    }

    /// Stores a file in the web VFS. Used for temporarily adding dropped files on WASM targets.
    #[cfg(target_arch = "wasm32")]
    pub fn store_web_file(path: &str, content: Vec<u8>) {
        WEB_FILES.with(|files| {
            files.borrow_mut().insert(path.to_string(), content);
        });
    }

    /// Creates a [SourceFile] from a web VFS path and computes its MD5 hash from the stored content.
    /// This is the preferred way to create SourceFiles on WASM because it includes the MD5 hash.
    #[cfg(target_arch = "wasm32")]
    fn source_file_from_web_path_with_md5(path: &str, repository_prefix: &str) -> Option<SourceFile> {
        let mut sf = SourceFile::from_web_path(path, repository_prefix)?;
        sf.md5_hash = WEB_FILES.with(|files| {
            files
                .borrow()
                .get(path)
                .map(|content| format!("{:x}", md5::compute(content)))
        });
        Some(sf)
    }

    /// Downloads a ZIP file and extracts it to a temporary directory asynchronously (desktop only).
    /// Optionally includes an authorization token for authenticated requests (e.g. private GitHub repos).
    #[cfg(not(target_arch = "wasm32"))]
    async fn download_and_extract_zip_async(
        &self,
        url: &str,
        token: Option<&str>,
    ) -> Result<TempDir, String> {
        let temp_dir = create_temp_dir()?;
        let zip_path = temp_dir.path().join("download.zip");
        #[allow(unused_mut)]
        let mut builder = AsyncClient::builder().http1_only();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            builder = builder.use_preconfigured_tls(mobile_tls_config());
        }
        let client = builder
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
                if let Some(parent) = outpath.parent()
                    && !parent.exists() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
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

/// Creates a new temporary directory in a platform-appropriate location.
///
/// On Android, the standard `/tmp` directory does not exist. The `tempfile` crate's
/// `TempDir::new()` would fail because `std::env::temp_dir()` returns `/tmp` when
/// `TMPDIR` is not set. Instead, we create temporary directories inside the app's
/// private storage obtained via JNI (`Context.getFilesDir()`).
///
/// On other platforms, this delegates to `TempDir::new()` which uses the system's
/// standard temp directory.
/// The single directory every entry of the archive lies in, `"name/"`, if there
/// is one.
///
/// The web build never writes the archive to a file system, so it has to spot
/// the wrapper directory in the entry names rather than after unpacking; the
/// reason for stripping it is the same as in [`archive_content_root`].
#[cfg(target_arch = "wasm32")]
fn archive_wrapper_directory<R: std::io::Read + std::io::Seek>(
    archive: &ZipArchive<R>,
) -> Option<String> {
    let mut names = archive.file_names().filter(|name| !name.is_empty());
    let first = names.next()?;
    let wrapper = format!("{}/", first.split('/').next()?);

    names
        .all(|name| name.starts_with(&wrapper))
        .then_some(wrapper)
}

/// Where the content of an extracted archive actually begins.
///
/// A zipball from GitHub wraps the whole repository in a single directory whose
/// name carries the commit it was built from — `cantara-songrepo-4f2ab9c`. That
/// name changes with every update of the repository, so it must not end up in a
/// file's [`relative_path`](SourceFile::relative_path): the identifiers the
/// detail view puts into its URLs are derived from that, and they are supposed
/// to outlive both the download and the update. Stripping it also makes them
/// agree with the web build, which unpacks the same repository without the
/// wrapper.
///
/// Anything that is not wrapped in exactly one directory is returned unchanged.
#[cfg(not(target_arch = "wasm32"))]
fn archive_content_root(dir: &Path) -> PathBuf {
    let mut entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries.flatten(),
        Err(_) => return dir.to_path_buf(),
    };

    match (entries.next(), entries.next()) {
        (Some(only), None) if only.path().is_dir() => only.path(),
        _ => dir.to_path_buf(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn create_temp_dir() -> Result<TempDir, String> {
    // On Android, use the app's private files directory as the temp base
    #[cfg(target_os = "android")]
    {
        if let Some(base) = get_android_files_dir() {
            let tmp_base = base.join("tmp");
            std::fs::create_dir_all(&tmp_base)
                .map_err(|e| format!("Failed to create Android temp base directory: {}", e))?;
            return TempDir::new_in(&tmp_base)
                .map_err(|e| format!("Failed to create temporary directory on Android: {}", e));
        }
        return Err("Failed to obtain Android files directory for temp storage".to_string());
    }

    #[allow(unreachable_code)]
    TempDir::new().map_err(|e| format!("Failed to create temporary directory: {}", e))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get_settings_folder() -> Option<PathBuf> {
    // On Android, the `dirs` crate cannot resolve standard config/data directories
    // because the HOME and XDG_* environment variables are not set by the Android runtime.
    // Use JNI to query the app's private files directory instead.
    #[cfg(target_os = "android")]
    if let Some(dir) = get_android_files_dir() {
        return Some(dir.join("cantara"));
    }

    // Try config_local_dir first (works on desktop Linux, macOS, Windows).
    // Fall back to data_local_dir and then home_dir for mobile (iOS)
    // where the config dir might not be available.
    dirs::config_local_dir()
        .or_else(dirs::data_local_dir)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .map(|dir| dir.join("cantara"))
}

/// Creates a rustls `ClientConfig` using embedded Mozilla root certificates
/// (from the `webpki-root-certs` crate) instead of the platform verifier.
///
/// On Android, the default TLS configuration in reqwest uses
/// `rustls-platform-verifier`, which performs certificate verification via JNI
/// and a Java helper class (`rustls-platform-verifier-android`). If that class
/// is not bundled in the APK (which is the case for Dioxus-built apps), the
/// JNI class-loading fails and causes a fatal SIGABRT.
///
/// By providing a pre-configured `ClientConfig` with WebPKI roots we bypass
/// the platform verifier entirely, while still verifying server certificates
/// against Mozilla's trusted root CA bundle.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn mobile_tls_config() -> rustls::ClientConfig {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.add_parsable_certificates(
        webpki_root_certs::TLS_SERVER_ROOT_CERTS.iter().cloned(),
    );
    rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth()
}

/// Returns the app's private files directory on Android via JNI.
///
/// Uses `ndk-context` to obtain the Android Activity reference, then calls
/// `Context.getFilesDir().getAbsolutePath()` through JNI to get a persistent,
/// app-private directory path.
#[cfg(target_os = "android")]
fn get_android_files_dir() -> Option<PathBuf> {
    // `::` because `dioxus::prelude::*` also brings a `jni` into scope; without
    // it the name is ambiguous and the Android build stops here.
    use ::jni::JavaVM;
    use ::jni::errors::Error as JniError;
    use ::jni::objects::{JObject, JString};
    // jni 0.22 wants method names and signatures pre-encoded rather than as
    // `&str`; both macros do that at compile time.
    use ::jni::{jni_sig, jni_str};
    use log::{error, info};

    let ctx = ndk_context::android_context();

    // Safety: the VM pointer from ndk-context is valid for the app's lifetime.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };

    // jni 0.22 hands out the environment through a callback rather than a
    // guard, so the whole conversation with Java happens in here and the
    // local references are released when it returns.
    let result: Result<PathBuf, JniError> = vm.attach_current_thread(|env| {
        // Safety: the context pointer is a valid Activity jobject managed by
        // android-activity.
        let activity = unsafe { JObject::from_raw(env, ctx.context().cast()) };

        // Context.getFilesDir() -> java.io.File
        let files_dir = env
            .call_method(&activity, jni_str!("getFilesDir"), jni_sig!("()Ljava/io/File;"), &[])?
            .l()?;

        // File.getAbsolutePath() -> java.lang.String
        let path = env
            .call_method(
                &files_dir,
                jni_str!("getAbsolutePath"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;

        let path = env.cast_local::<JString>(path)?;
        let text = path.try_to_string(env)?;

        Ok(PathBuf::from(text))
    });

    match result {
        Ok(path) => {
            info!("Android files directory: {}", path.display());
            Some(path)
        }
        Err(error) => {
            error!("Could not ask Android for the app's files directory: {error}");
            None
        }
    }
}

/// A configured Presentation Design which is used both for creating the presentation slides as well as for rendering them.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
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

/// A slide division the user maintains, under a name.
///
/// [`SlideSettings`] is the song library's, and says how a song is broken into
/// slides. What it has no room for is what the *user* needs to tell one of
/// them from another in a list — so the name and the description are added
/// here rather than there.
///
/// The division itself is flattened into the same JSON object, which is what
/// keeps a settings file written before this existed readable: the two new
/// fields are simply absent and default to empty.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct SongSlideSettings {
    /// What the user calls it. Empty for a division that has never been named
    /// — the views then fall back to its position in the list.
    #[serde(default)]
    pub name: String,

    /// What it is for, in the user's own words.
    #[serde(default)]
    pub description: String,

    /// The division itself, as the song library understands it.
    #[serde(flatten)]
    pub settings: SlideSettings,
}

impl SongSlideSettings {
    /// The name to show for the division at `index`.
    ///
    /// A division that has never been named is called by its position, which
    /// is what the list showed before names existed.
    pub fn display_name(&self, index: usize) -> String {
        match self.name.trim().is_empty() {
            true => format!("{} {}", t!("settings.slide_settings"), index + 1),
            false => self.name.clone(),
        }
    }
}

impl From<SlideSettings> for SongSlideSettings {
    fn from(settings: SlideSettings) -> Self {
        SongSlideSettings {
            name: String::new(),
            description: String::new(),
            settings,
        }
    }
}

/// This enum describes the general design of the presentation (background color, font-colors etc.).
/// It can be configured via a Template or imputed by direct HTML/CSS
///
/// The two variants differ a lot in size, and that is left as it is: a design
/// exists once per configured design — a handful per installation, never a
/// collection worth the indirection — and the template is read on every frame
/// the presentation renders.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
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

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
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

    /// The distance between the main content and the spoiler content.
    ///
    /// Also used between the title and its meta line, so a design only has to
    /// state one "distance between the two blocks of a slide".
    pub main_content_spoiler_content_padding: CssSize,

    /// How the notation block is drawn.
    #[serde(default)]
    pub notation: NotationSettings,

    /// Whether the title on a title slide is set in bold.
    ///
    /// Kept apart from the headline block's weight so that turning it on does
    /// not also thicken the body text — by default both are the same block.
    #[serde(default)]
    pub title_bold: bool,
}

/// How the notation block of a complex slide is drawn.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NotationSettings {
    /// The width of the staff as a percentage of the content width.
    ///
    /// At 100 the staff spans exactly the same box as the text blocks around
    /// it, so the two line up on the left and right edges.
    pub width_percent: f64,

    /// Where a staff narrower than the content sits.
    pub horizontal_alignment: HorizontalAlign,

    /// The height of one staff line, as a multiple of the engraver's default.
    ///
    /// Reads like the `line_height` of a text block: 1.0 is normal spacing,
    /// larger values open the systems up.
    pub staff_line_height: f64,

    /// The size of the words printed under the notes.
    pub font_size: CssSize,
}

impl Default for NotationSettings {
    fn default() -> Self {
        NotationSettings {
            width_percent: 100.0,
            horizontal_alignment: HorizontalAlign::Centered,
            staff_line_height: 1.0,
            // Matches the default spoiler size, which is what the notation
            // lyrics were drawn at before this became configurable.
            font_size: CssSize::Pt(22.4),
        }
    }
}

impl PresentationDesignTemplate {
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

    pub fn spoiler_index(&self) -> Option<u16> {
        self.spoiler_index
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

    /// The block configured for `language`, if a design defines one.
    ///
    /// A block claims a language by carrying its code; the comparison ignores
    /// case and surrounding space so that `"DE"` and `"de "` still match a
    /// song tagged `de`.
    pub fn font_for_language(&self, language: &str) -> Option<FontRepresentation> {
        let wanted = language.trim().to_lowercase();
        if wanted.is_empty() {
            return None;
        }

        self.fonts
            .iter()
            .find(|font| {
                font.language
                    .as_deref()
                    .map(|code| code.trim().to_lowercase() == wanted)
                    .unwrap_or(false)
            })
            .cloned()
    }

    /// The block a row of a complex slide is drawn with.
    ///
    /// A row is drawn with the block that claims its language; where no block
    /// does, it falls back to the main block. That one rule covers both cases
    /// the design has to handle: the first row of a slide is its main text and
    /// normally lands on the main block, and a song in a language the design
    /// was never set up for still gets drawn.
    pub fn font_for_row(&self, language: Option<&str>) -> FontRepresentation {
        language
            .and_then(|code| self.font_for_language(code))
            .unwrap_or_else(|| self.get_default_font())
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
            notation: NotationSettings::default(),
            title_bold: false,
        }
    }
}

/// Represents a font representation for an element in the presentation
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
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

    /// How heavy the type is drawn, as a CSS font weight (100–900).
    #[serde(default = "default_font_weight")]
    pub weight: u16,

    /// Whether the type is slanted.
    ///
    /// A switch rather than a degree, unlike [`weight`](Self::weight): a face
    /// either has an italic or it does not, and where it does not the browser
    /// slants the upright one — which is the same thing every other program
    /// does with the same button.
    #[serde(default)]
    pub italic: bool,

    /// An outline drawn around the glyphs. Keeps light text readable on a busy
    /// background image without darkening the whole slide.
    #[serde(default)]
    pub outline: Option<FontOutline>,

    /// How the shadow is drawn when [`FontRepresentation::shadow`] is on.
    #[serde(default)]
    pub shadow_style: FontShadow,

    /// The language this block is for, as a language code such as `"de"`.
    ///
    /// Only meaningful for a complex presentation, where one slide shows the
    /// same passage in several languages: a row is drawn with the block
    /// carrying its language, and falls back to the main block when no block
    /// claims it. `None` means the block is not tied to a language.
    #[serde(default)]
    pub language: Option<String>,
}

/// An outline drawn around the glyphs of a [`FontRepresentation`].
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct FontOutline {
    pub color: RGBA8,
    /// The stroke width in pixels. Anything above roughly 3 starts to close up
    /// the counters of the letters.
    pub width: f64,
}

impl Default for FontOutline {
    fn default() -> Self {
        FontOutline {
            color: Rgba::new(0, 0, 0, 255),
            width: 1.0,
        }
    }
}

/// How a [`FontRepresentation`]'s shadow is drawn.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct FontShadow {
    pub color: RGBA8,
    pub offset_x: f64,
    pub offset_y: f64,
    pub blur: f64,
}

impl Default for FontShadow {
    fn default() -> Self {
        FontShadow {
            // A soft, slightly offset black shadow: enough to lift text off a
            // photograph without reading as an effect.
            color: Rgba::new(0, 0, 0, 180),
            offset_x: 2.0,
            offset_y: 2.0,
            blur: 6.0,
        }
    }
}

/// Regular weight — what type is drawn at unless the design says otherwise.
fn default_font_weight() -> u16 {
    400
}

/// The weight the bold switch turns type up to.
pub const BOLD_WEIGHT: u16 = 700;

/// From where up type reads as bold rather than merely a little heavier.
///
/// Semibold is included: a design set to 600 has a bold switch that says so,
/// which is less surprising than a switch that is off while the text on the
/// slide is plainly heavy.
const BOLD_THRESHOLD: u16 = 600;

impl FontRepresentation {
    /// Whether this block reads as bold.
    ///
    /// Derived from [`weight`](Self::weight) rather than kept beside it: two
    /// fields saying the same thing is two fields that can disagree, and the
    /// weight is the one a stylesheet is written from. The bold switch in the
    /// settings is a view of this and of [`set_bold`](Self::set_bold).
    pub fn is_bold(&self) -> bool {
        self.weight >= BOLD_THRESHOLD
    }

    /// Turns the weight up to bold, or back down to regular.
    ///
    /// Turning it off lands on regular even from light, which is the one place
    /// this loses something the weight list can say. The alternative —
    /// remembering what it was before — makes a switch whose off position
    /// depends on history, and there is a weight list right beside it for
    /// anyone who wants light.
    pub fn set_bold(&mut self, bold: bool) {
        self.weight = match bold {
            true => BOLD_WEIGHT,
            false => default_font_weight(),
        };
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
            weight: default_font_weight(),
            italic: false,
            outline: None,
            shadow_style: FontShadow::default(),
            language: None,
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

    /// Justified text without hyphenation
    Justify,

    /// Justified text with automatic hyphenation (`hyphens: auto`)
    JustifyWithHyphenation,
}

impl CssString for HorizontalAlign {
    fn to_css_string(&self) -> String {
        match self {
            HorizontalAlign::Left => "left".to_string(),
            HorizontalAlign::Centered => "center".to_string(),
            HorizontalAlign::Right => "right".to_string(),
            HorizontalAlign::Justify => "justify".to_string(),
            HorizontalAlign::JustifyWithHyphenation => "justify".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Default)]
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
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
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

    /// Build a settings document in the shape Cantara 0.3 wrote: everything as
    /// it is today, but `show_meta_information` back as a plain string.
    fn settings_json_with_old_meta(name: &str) -> String {
        let mut document = serde_json::to_value(Settings::default()).unwrap();

        let slide_settings = document
            .get_mut("song_slide_settings")
            .and_then(|value| value.as_array_mut())
            .expect("the default settings carry slide settings");
        assert!(
            !slide_settings.is_empty(),
            "the fixture needs at least one slide setting"
        );

        for entry in slide_settings.iter_mut() {
            entry
                .as_object_mut()
                .unwrap()
                .insert("show_meta_information".to_string(), serde_json::json!(name));
            entry
                .as_object_mut()
                .unwrap()
                .insert("meta_syntax".to_string(), serde_json::json!("{{title}}"));
        }

        serde_json::to_string(&document).unwrap()
    }

    /// A settings file written before streaming existed has no `stream`
    /// section, and must still load with everything else in it intact — the
    /// alternative is a user losing their repositories to an upgrade.
    #[test]
    fn test_settings_without_a_stream_section_still_load() {
        let mut document = serde_json::to_value(Settings::default()).expect("serialises");
        document
            .as_object_mut()
            .expect("an object")
            .remove("stream")
            .expect("the section is written");

        let settings: Settings = serde_json::from_value(document).expect("loads without it");

        assert_eq!(settings.stream, StreamSettings::default());
        assert_eq!(settings.stream.port, default_stream_port());
        assert!(
            settings.stream.password.is_empty(),
            "no password means anyone on the network can watch, which is the default"
        );
    }

    /// Settings written by Cantara 0.3 and earlier stored
    /// `show_meta_information` as a plain string, because the song library's
    /// `ShowMetaInformation` was an enum. It is a struct of three flags now.
    ///
    /// Deserialising the whole settings file fails on the old shape, and
    /// `Settings::load` then falls back to the defaults — which would throw
    /// away every repository, design and font the user had configured, not
    /// just this one field.
    #[test]
    fn test_settings_from_an_older_version_still_load() {
        let old = settings_json_with_old_meta("FirstSlideAndLastSlide");

        // Without the migration the whole document is rejected …
        assert!(
            serde_json::from_str::<Settings>(&old).is_err(),
            "the fixture no longer reproduces the old shape"
        );

        // … and with it, everything survives.
        let settings: Settings =
            serde_json::from_str(&migrate_settings_json(&old)).expect("old settings should load");

        let slide_settings = &settings
            .song_slide_settings
            .first()
            .expect("the slide settings survived")
            .settings;

        assert!(slide_settings.show_meta_information.first_slide);
        assert!(slide_settings.show_meta_information.last_slide);
        assert!(!slide_settings.show_meta_information.title_slide);
        assert_eq!(slide_settings.meta_syntax, "{{title}}");
    }

    #[test]
    fn test_every_old_meta_name_is_understood() {
        let cases = [
            ("None", (false, false, false)),
            ("FirstSlide", (false, true, false)),
            ("LastSlide", (false, false, true)),
            ("FirstSlideAndLastSlide", (false, true, true)),
        ];

        for (name, expected) in cases {
            let json = migrate_settings_json(&settings_json_with_old_meta(name));
            let settings: Settings =
                serde_json::from_str(&json).unwrap_or_else(|error| panic!("{name}: {error}"));

            let show = settings.song_slide_settings[0].settings.show_meta_information;
            assert_eq!(
                (show.title_slide, show.first_slide, show.last_slide),
                expected,
                "for {name}"
            );
        }
    }

    /// Settings already in the new shape must pass through untouched.
    #[test]
    fn test_current_settings_are_left_alone() {
        let current = serde_json::to_string(&Settings::default()).unwrap();
        let migrated = migrate_settings_json(&current);

        // Compared as documents rather than as text. The migration reads the
        // settings into a `serde_json::Value` and writes them back out, and a
        // `Value` holds its keys sorted while `Settings` writes them in the
        // order it declares them — so the two strings differ by key order alone
        // even when not one value was touched. Comparing the strings made this
        // test depend on those two orders happening to agree, which is not
        // something either side promises and which stopped being true without
        // anything here changing.
        //
        // What the test is about is that nothing was changed, and that is a
        // statement about the documents.
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&migrated).expect("valid JSON"),
            serde_json::from_str::<serde_json::Value>(&current).expect("valid JSON"),
        );
    }

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

    /// The general half of the presentation options picks which of the
    /// configured designs "Default" means, and that is what everything
    /// showing an element without one of its own has to use.
    #[test]
    fn the_chosen_design_is_what_default_means() {
        let second = PresentationDesign {
            name: "Dark".to_string(),
            ..PresentationDesign::default()
        };
        let settings = Settings {
            presentation_designs: vec![PresentationDesign::default(), second],
            default_design_index: 1,
            ..Default::default()
        };

        assert_eq!(settings.default_presentation_design().name, "Dark");
    }

    /// A design deleted since it was chosen must not leave a service without
    /// one in the middle of it.
    #[test]
    fn a_default_that_is_no_longer_there_falls_back_to_the_first() {
        let settings = Settings {
            presentation_designs: vec![PresentationDesign::default()],
            default_design_index: 7,
            default_slide_settings_index: 7,
            ..Default::default()
        };

        assert_eq!(
            settings.default_presentation_design().name,
            settings.presentation_designs[0].name
        );
        assert_eq!(
            settings.default_song_slide_settings(),
            settings.song_slide_settings[0].settings
        );
    }

    /// An import has to go into a folder on this computer. A downloaded
    /// repository is unpacked afresh on every start, so a song written into
    /// one would be gone by the next — it is not offered.
    #[test]
    fn only_a_folder_on_this_computer_can_be_imported_into() {
        let settings = Settings {
            repositories: vec![
                Repository::new_remote_zip(
                    "Shared".to_string(),
                    "https://example.org/songs.zip".to_string(),
                ),
                Repository::new_local_folder("Mine".to_string(), "/songs".to_string()),
            ],
            ..Default::default()
        };

        let writable: Vec<usize> = settings
            .writable_repositories()
            .iter()
            .map(|(index, _)| *index)
            .collect();
        assert_eq!(writable, vec![1]);
        assert_eq!(settings.repository_folder(1), Some(PathBuf::from("/songs")));
        assert_eq!(settings.repository_folder(0), None);
        assert_eq!(settings.repository_folder(9), None);
    }

    /// A settings file written before the choice existed reads as the first
    /// entry, which is what it did back then.
    #[test]
    fn an_older_settings_file_keeps_the_first_entry_as_its_default() {
        let json = r#"{"repositories":[],"wizard_completed":true}"#;
        let settings: Settings =
            serde_json::from_str(json).expect("an older settings file still reads");

        assert_eq!(settings.default_design_index, 0);
        assert_eq!(settings.default_slide_settings_index, 0);
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

#[cfg(test)]
#[allow(
    clippy::field_reassign_with_default,
    reason = "these design structs keep private fields, so `..Default::default()`               is not available outside the module that defines them"
)]
mod design_block_tests {
    use super::*;

    fn template_with_language(code: &str) -> PresentationDesignTemplate {
        let mut template = PresentationDesignTemplate::default();
        template.fonts.push(FontRepresentation {
            language: Some(code.to_string()),
            font_size: CssSize::Pt(11.0),
            ..FontRepresentation::default()
        });
        template
    }

    /// A row is drawn with the block that claims its language.
    #[test]
    fn test_a_row_takes_the_block_for_its_language() {
        let template = template_with_language("de");

        let font = template.font_for_row(Some("de"));

        assert_eq!(font.font_size, CssSize::Pt(11.0));
    }

    /// A song in a language the design was never set up for still has to be
    /// drawn, so it falls back to the main block.
    #[test]
    fn test_an_unclaimed_language_falls_back_to_the_main_block() {
        let template = template_with_language("de");

        let font = template.font_for_row(Some("fi"));

        assert_eq!(font.font_size, template.get_default_font().font_size);
    }

    /// The classic `.song` format carries no language at all.
    #[test]
    fn test_a_row_without_a_language_uses_the_main_block() {
        let template = template_with_language("de");

        assert_eq!(
            template.font_for_row(None).font_size,
            template.get_default_font().font_size
        );
    }

    /// A language code is a label a user types, so matching must not hinge on
    /// how they typed it.
    #[test]
    fn test_language_matching_ignores_case_and_space() {
        let template = template_with_language(" DE ");

        assert_eq!(template.font_for_row(Some("de")).font_size, CssSize::Pt(11.0));
        assert_eq!(template.font_for_row(Some("De")).font_size, CssSize::Pt(11.0));
    }

    /// An empty code claims nothing — otherwise a half-filled block would
    /// silently capture every row.
    #[test]
    fn test_an_empty_code_claims_nothing() {
        let mut template = PresentationDesignTemplate::default();
        template.fonts.push(FontRepresentation {
            language: Some("  ".to_string()),
            font_size: CssSize::Pt(11.0),
            ..FontRepresentation::default()
        });

        assert_ne!(template.font_for_row(Some("de")).font_size, CssSize::Pt(11.0));
    }

    /// Settings written before these fields existed have to keep loading.
    #[test]
    fn test_an_old_font_block_still_loads() {
        let json = r#"{
            "font_family": null,
            "font_size": {"Pt": 40.0},
            "shadow": false,
            "line_height": 1.2,
            "color": {"r": 255, "g": 255, "b": 255, "a": 255},
            "horizontal_alignment": "Centered"
        }"#;

        let font: FontRepresentation = serde_json::from_str(json).expect("old settings must load");

        assert_eq!(font.weight, 400);
        assert!(!font.italic);
        assert!(font.outline.is_none());
        assert!(font.language.is_none());
    }

    /// The bold switch in the settings is a view of the weight, so that there
    /// is one thing stored and not two that can disagree.
    #[test]
    fn test_bold_is_the_weight_read_as_a_switch() {
        let mut font = FontRepresentation::default();
        assert!(!font.is_bold(), "regular type is not bold");

        font.set_bold(true);
        assert_eq!(font.weight, BOLD_WEIGHT);
        assert!(font.is_bold());

        font.set_bold(false);
        assert_eq!(font.weight, 400);
        assert!(!font.is_bold());

        // Semibold reads as bold: a switch that is off while the text on the
        // slide is plainly heavy is the more surprising answer.
        font.weight = 600;
        assert!(font.is_bold());
        font.weight = 500;
        assert!(!font.is_bold());

        // Turning it off from light lands on regular. The weight list beside
        // the switch is there for anyone who wants light back.
        font.weight = 300;
        assert!(!font.is_bold());
        font.set_bold(false);
        assert_eq!(font.weight, 400);
    }

    /// The same for a design written before the notation had settings.
    #[test]
    fn test_an_old_template_gets_default_notation_settings() {
        let template = PresentationDesignTemplate::default();
        let mut value = serde_json::to_value(&template).unwrap();
        value.as_object_mut().unwrap().remove("notation");
        value.as_object_mut().unwrap().remove("title_bold");

        let loaded: PresentationDesignTemplate =
            serde_json::from_value(value).expect("old settings must load");

        assert_eq!(loaded.notation.width_percent, 100.0);
        assert!(!loaded.title_bold);
    }

    /// A design deleted from the middle must not drag every later choice onto
    /// its neighbour.
    ///
    /// This is the case that has no symptom until someone notices the wrong
    /// design on the wall: the stored position stays perfectly valid, it just
    /// means something else afterwards.
    #[test]
    fn deleting_a_design_keeps_the_later_choices_pointing_at_the_same_thing() {
        let mut settings = Settings::default();
        settings.presentation_designs = (0..4)
            .map(|number| PresentationDesign {
                name: format!("design {number}"),
                ..PresentationDesign::default()
            })
            .collect();
        settings.song_slide_settings = vec![SongSlideSettings::default(); 4];
        // Everything points at the third design.
        settings.stream.design_index = Some(2);
        settings.stream.slide_settings_index = Some(2);
        settings.default_design_index = 2;
        settings.default_slide_settings_index = 2;

        settings.delete_presentation_design(0);

        assert_eq!(settings.presentation_designs[1].name, "design 2");
        assert_eq!(settings.stream.design_index, Some(1), "still design 2");
        assert_eq!(settings.stream.slide_settings_index, Some(1));
        assert_eq!(settings.default_design_index, 1);
        assert_eq!(settings.default_slide_settings_index, 1);
    }

    /// Deleting the very design a choice names leaves no choice — rather than a
    /// position that springs back to life pointing at something else once the
    /// list grows again.
    #[test]
    fn deleting_the_chosen_design_is_no_choice_and_stays_that_way() {
        let mut settings = Settings::default();
        settings.presentation_designs = vec![PresentationDesign::default(); 3];
        settings.song_slide_settings = vec![SongSlideSettings::default(); 3];
        settings.stream.design_index = Some(2);
        settings.stream.slide_settings_index = Some(2);
        settings.default_design_index = 2;

        settings.delete_presentation_design(2);

        assert_eq!(settings.stream.design_index, None);
        assert_eq!(settings.stream.slide_settings_index, None);
        assert_eq!(settings.default_design_index, 0, "falls back to the first");

        // The list grows past where the old choice pointed. Nothing may come
        // back.
        settings.presentation_designs = vec![PresentationDesign::default(); 5];
        assert_eq!(settings.stream.design_index, None);
    }

    /// A choice sitting before the deleted design is not affected by it.
    #[test]
    fn deleting_a_later_design_leaves_an_earlier_choice_alone() {
        let mut settings = Settings::default();
        settings.presentation_designs = vec![PresentationDesign::default(); 3];
        settings.song_slide_settings = vec![SongSlideSettings::default(); 3];
        settings.stream.design_index = Some(0);
        settings.default_design_index = 0;

        settings.delete_presentation_design(2);

        assert_eq!(settings.stream.design_index, Some(0));
        assert_eq!(settings.default_design_index, 0);
    }

    /// Asking to delete something that is not there changes nothing.
    #[test]
    fn deleting_past_the_end_does_nothing() {
        let mut settings = Settings::default();
        settings.presentation_designs = vec![PresentationDesign::default(); 2];
        settings.song_slide_settings = vec![SongSlideSettings::default(); 2];
        settings.stream.design_index = Some(1);

        settings.delete_presentation_design(9);

        assert_eq!(settings.presentation_designs.len(), 2);
        assert_eq!(settings.stream.design_index, Some(1));
    }
}
