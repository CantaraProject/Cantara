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

pub mod logic;
pub mod presentation_components;
pub mod selection_components;
pub mod settings_components;
pub mod shared_components;
pub mod wizard_components;

use crate::settings_components::SettingsPage;
use dioxus::prelude::*;
use dioxus_motion::prelude::*;
use logic::settings::*;
use logic::sourcefiles::SourceFile;
use logic::states::{self, RunningPresentation, SelectedItemRepresentation};
use selection_components::Selection;
use sys_locale::get_locale;
use wizard_components::Wizard;

rust_i18n::i18n!("locales", fallback = "en");

/// The CSS file provided by PicoCSS
const PICO_CSS: Asset = asset!("/node_modules/@picocss/pico/css/pico.min.css");

/// Cantara's own CSS file with additions to the PicoCSS definitions
const MAIN_CSS: Asset = asset!("/assets/main.css");

/// JavaScript helper functions which are used for styling and keyboard event handling
const POSITIONING_JS: Asset = asset!("/assets/positioning.js");

/// The Cantara Logo
pub const LOGO: Asset = asset!("/assets/cantara-logo_small.png");

/// The test state for debugging purposes (will be removed in the final version)
static TEST_STATE: GlobalSignal<String> = Global::new(|| "test".to_string());

#[derive(Routable, PartialEq, Clone, MotionTransitions)]
#[rustfmt::skip]
pub enum Route {
    /// The selection route allows the user to select songs or other elements for the presentation
    #[route("/")]
    #[transition(Fade)]
    Selection,

    /// The wizard is shown when the program is run for the first time (no configuration file exists)
    #[route("/wizard")]
    #[transition(SlideLeft)]
    Wizard,

    /// The settings page is shown when explicitly called
    #[route("/settings")]
    #[transition(Fade)]
    SettingsPage
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
        let window = tao::window::WindowBuilder::new()
            .with_resizable(true)
            .with_title("Cantara")
            .with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0))
            .with_decorations(true)
            .with_visible(true);
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

    let cloned_locale = locale.clone();
    use_context_provider(|| states::RuntimeInformation {
        language: cloned_locale,
    });

    // Initialize settings and provide them as a context to all components
    let settings: Signal<Settings> = use_signal(Settings::load);
    use_context_provider(|| settings);

    // The source files and selected items should live here because they should stay persistent in the different routes.
    let _: Signal<Vec<SourceFile>> = use_context_provider(|| Signal::new(vec![]));
    let _: Signal<Vec<SelectedItemRepresentation>> = use_context_provider(|| Signal::new(vec![]));

    // The running presentations given as a global signal
    let _: Signal<Vec<RunningPresentation>> = use_context_provider(|| Signal::new(vec![]));

    rsx! {
        document::Link { rel: "stylesheet", href: PICO_CSS }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Script { src: POSITIONING_JS }
        document::Title { "Cantara" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Meta { name: "content-language", content: locale }

        Router::<Route> {}

    }
}
