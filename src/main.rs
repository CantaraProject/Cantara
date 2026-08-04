//! Cantara is an open source song presentation software that allows people to present song lyrics for a bigger audience to sing together.
//!
//! While the program was originally written in Free Pascal/Lazarus, this repository is a rewrite in Rust using Dioxus.
//!
//! # Structure
//! - The [main] function is the entry point for the program which handles the initializing and startup.
//! - Modules ending with `_components` contain the dioxus components used in the program and some helper functions used by the components
//! - The [logic] module provides the business logic of the program including repositories, settings and states.
//!
//! ## Additional crates
//! The parsing of the song files, the song structures and the side generation are part of the [cantara_songlib] crate.

// Make sure that no terminal window is shown on windows
#![windows_subsystem = "windows"]

mod components;
mod logic;

use crate::components::presentation_components::PresentationPage;
use crate::components::presentation_design_settings_components::PresentationDesignSettingsPage;
use crate::components::presenter_console_components::PresenterConsolePage;
use crate::components::selection_components::Selection;
use crate::components::settings_components::SettingsPage;
use crate::components::detail_components::Detail;
use crate::components::presentation_components::BundledFontFaces;
use crate::components::route_transitions::AnimatedLayout;
use crate::components::song_slide_settings_components::SongSlideSettingsPage;
use crate::components::wizard_components::Wizard;
use dioxus::prelude::*;
use dioxus_motion::prelude::*;
use logic::settings::*;
use logic::sourcefiles::SourceFile;
use logic::states::{self, RunningPresentation, SelectedItemRepresentation};
use sys_locale::get_locale;

rust_i18n::i18n!("locales", fallback = "en");

/// The CSS file provided by PicoCSS
const PICO_CSS: Asset = asset!("/node_modules/@picocss/pico/css/pico.min.css");

/// Cantara's own CSS file with additions to the PicoCSS definitions
const MAIN_CSS: Asset = asset!("/assets/main.css");

/// JavaScript helper functions which are used for styling and keyboard event handling
const POSITIONING_JS: Asset = asset!("/assets/positioning.js");

/// The Cantara Logo
pub const LOGO: Asset = asset!("/assets/cantara-logo_small.png");

/// The favicon / window icon
const FAVICON: Asset = asset!("/assets/favicon.png");

/// The test state for debugging purposes (will be removed in the final version)
static TEST_STATE: GlobalSignal<String> = Global::new(|| "test".to_string());

/// The routes of the application.
///
/// All of them live inside [`AnimatedLayout`], which renders an animated outlet
/// instead of a plain one, so that a page change is a short cross-fade instead
/// of a jump. Every route uses the same `Fade` — see
/// [`route_transitions`](components::route_transitions) for why there is no
/// route-specific effect.
#[derive(Routable, PartialEq, Clone, MotionTransitions)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AnimatedLayout)]
    /// The selection route allows the user to select songs or other elements for the presentation
    #[route("/")]
    #[transition(Fade)]
    Selection {},


    /// The detail view shows and edits one element at a time.
    ///
    /// The trailing segment names the element that is open, so a link leads
    /// straight to it: `/detail/a3f9c2b1`. It is a catch-all rather than a
    /// second route, so that opening an element only changes this field
    /// instead of swapping the route — the view keeps its state, and the fade
    /// stays where it belongs, between the views.
    /// Everything about that identifier is in [`logic::element_id`].
    #[route("/detail/:..element")]
    #[transition(Fade)]
    Detail { element: Vec<String> },

    /// The wizard is shown when the program is run for the first time (no configuration file exists)
    #[route("/wizard")]
    #[transition(Fade)]
    Wizard {},

    /// The settings page is shown when explicitly called
    #[route("/settings")]
    #[transition(Fade)]
    SettingsPage {},

    /// The presentation design settings page with a dynamic index
    #[route("/settings/design/:index")]
    #[transition(Fade)]
    PresentationDesignSettingsPage { index: u16 },

    /// The song slide settings page with a dynamic index
    #[route("/settings/slide/:index")]
    #[transition(Fade)]
    SongSlideSettingsPage { index: u16 },

    /// The presenter console shown in the main window during a presentation
    #[route("/presenter")]
    #[transition(Fade)]
    PresenterConsolePage {},

    /// The presentation view shown in the same tab (when presenter console is disabled)
    /// or opened in a new tab (when presenter console is enabled, on web).
    #[route("/presentation")]
    #[transition(Fade)]
    PresentationPage {},
}

fn main() {
    #[cfg(feature = "desktop")]
    fn launch_app() {
        #[cfg(target_os = "linux")]
        {
            if std::path::Path::new("/dev/dri").exists()
                && std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland"
            {
                // Gnome Webkit is currently buggy under Wayland and KDE, so we will run it with XWayland mode.
                // See: https://github.com/DioxusLabs/dioxus/issues/3667
                unsafe {
                    // Disable explicit sync for NVIDIA drivers on Linux when using Way
                    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
                }
            }
            unsafe {
                std::env::set_var("GDK_BACKEND", "x11");
            }
        }

        use dioxus::desktop::tao;

        let icon = {
            let icon_bytes = include_bytes!("../assets/favicon.png");
            let icon_image = image::load_from_memory(icon_bytes).expect("Failed to load icon");
            let icon_rgba = icon_image.to_rgba8();
            let (width, height) = icon_rgba.dimensions();
            tao::window::Icon::from_rgba(icon_rgba.into_raw(), width, height)
                .expect("Failed to create window icon")
        };

        let window = tao::window::WindowBuilder::new()
            .with_resizable(true)
            .with_title("Cantara")
            .with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0))
            .with_decorations(true)
            .with_visible(true)
            .with_window_icon(Some(icon));
        dioxus::LaunchBuilder::new()
            .with_cfg(
                dioxus::desktop::Config::new()
                    .with_window(window)
                    .with_menu(None),
            )
            .launch(App);
    }

    #[cfg(not(feature = "desktop"))]
    fn launch_app() {
        dioxus::launch(App);
    }

    launch_app();
}

#[component]
fn App() -> Element {
    let locale = get_locale().unwrap_or_else(|| String::from("en-US"));

    rust_i18n::set_locale(&locale);

    // On Linux (especially GNOME), system theme detection might fail in the WebView.
    // We explicitly detect it and set the data-theme attribute for PicoCSS.
    #[cfg(all(feature = "desktop", target_os = "linux"))]
    use_effect(move || {
        match dark_light::detect() {
            Ok(mode) => match mode {
                dark_light::Mode::Dark => {
                    let _ = document::eval("document.documentElement.setAttribute('data-theme', 'dark')");
                },
                dark_light::Mode::Light => {
                    let _ = document::eval("document.documentElement.setAttribute('data-theme', 'light')");
                },
                _ => {}
            },
            Err(e) => {
                log::error!("Failed to detect system theme: {}", e);
                let _ = document::eval("document.documentElement.setAttribute('data-theme', 'light')");
            }
        }
    });

    let cloned_locale = locale.clone();
    use_context_provider(|| states::RuntimeInformation {
        language: cloned_locale,
    });

    // Initialize settings and provide them as a context to all components
    let settings: Signal<Settings> = use_signal(Settings::load);
    use_context_provider(|| settings);

    // The source files and selected items should live here because they should stay persistent in the different routes.
    let mut source_files: Signal<Vec<SourceFile>> = use_context_provider(|| Signal::new(vec![]));
    let _: Signal<Vec<SelectedItemRepresentation>> = use_context_provider(|| Signal::new(vec![]));

    // The running presentations given as a global signal
    let _: Signal<Vec<RunningPresentation>> = use_context_provider(|| Signal::new(vec![]));

    // Where a build starts. The desktop is built around assembling a
    // presentation, so it opens the selection; the web version is mostly used
    // to look songs up, so it opens the detail view.
    //
    // The actual navigation has to happen in the `Selection` component, since
    // only a descendant of `Router` (rendered below) can call `navigator()` —
    // calling it here, in `App` itself, panics because the router context
    // doesn't exist yet at this point in the render. What belongs here is only
    // the "have we already done this" flag, so it survives `Selection`
    // unmounting and remounting as the user navigates.
    #[cfg(target_arch = "wasm32")]
    use_context_provider(|| states::InitialRouteState {
        redirected_to_detail: Signal::new(false),
    });

    // Read the library here rather than in a view. It used to be loaded by the
    // selection view, which meant the list stayed empty for anyone who never
    // opened it — the web build starts in the detail view, so its songs never
    // appeared at all.
    //
    // Scanning is expensive: every file is read to fingerprint it and every PDF
    // is parsed for the search index, so this depends on the repositories alone
    // and not on the rest of the settings.
    let repositories = use_memo(move || settings.read().repositories.clone());
    let mut scan_generation: Signal<u64> = use_signal(|| 0);

    use_effect(move || {
        let repositories = repositories();

        // A scan takes seconds on a large library, so a second one can start
        // while the first is still running. Each claims a generation and only
        // publishes its result while that generation is still the current one
        // — otherwise a slow scan of the old repositories would land on top of
        // a finished scan of the new ones.
        //
        // `peek` rather than a read: this effect must not subscribe to the
        // counter it writes itself. The value has to be copied out of the
        // guard as well, since a borrow cannot be held across the `await`.
        let generation = *scan_generation.peek() + 1;
        scan_generation.set(generation);

        spawn(async move {
            let files = Settings::sourcefiles_of_async(&repositories).await;
            if *scan_generation.peek() != generation {
                return;
            }
            source_files.set(files.clone());

            #[cfg(not(target_arch = "wasm32"))]
            std::thread::spawn(move || {
                crate::logic::search::refresh_search_cache(&files);
            });
            #[cfg(target_arch = "wasm32")]
            crate::logic::search::refresh_search_cache(&files);
        });
    });

    rsx! {
        document::Link { rel: "stylesheet", href: PICO_CSS }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        // Makes the fonts shipped in `assets/fonts/` usable by name.
        BundledFontFaces {}
        document::Link { rel: "icon", href: FAVICON }
        document::Script { src: POSITIONING_JS }
        document::Title { "Cantara" }

        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Meta { name: "content-language", content: locale }

        Router::<Route> {}
    }
}
