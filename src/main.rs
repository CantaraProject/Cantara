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
use crate::components::dialogs::DialogHost;
use crate::components::presentation_components::{
    BundledFontFaces, MORPH_JS, PRESENTATION_CSS, PRESENTATION_JS,
};
use crate::components::presenter_console_components::PRESENTER_CONSOLE_CSS;
use crate::components::route_transitions::RouteFadeLayout;
use crate::components::song_slide_settings_components::SongSlideSettingsPage;
use crate::components::wizard_components::Wizard;
use dioxus::prelude::*;
use logic::settings::*;
use logic::sourcefiles::SourceFile;
use logic::states::{self, RunningPresentation, SelectedItemRepresentation};
use sys_locale::get_locale;

rust_i18n::i18n!("locales", fallback = "en");

/// The CSS file provided by PicoCSS
const PICO_CSS: Asset = asset!("/node_modules/@picocss/pico/css/pico.min.css");

/// Cantara's own CSS file with additions to the PicoCSS definitions
const MAIN_CSS: Asset = asset!("/assets/main.css");

// `assets/positioning.js` used to be loaded here. It measured the bars and
// wrote pixel heights onto the scrolling boxes, tracked which swipe panel was
// in view, scrolled to one on request, and sent stray keystrokes to the search
// field. The first is a column flexbox — see the note on `.wrapper` in
// `assets/main.css` — and the rest are ordinary handlers in
// `components::selection_components`.

/// The Cantara Logo
pub const LOGO: Asset = asset!("/assets/cantara-logo_small.png");

/// The favicon / window icon
const FAVICON: Asset = asset!("/assets/favicon.png");

/// The routes of the application.
///
/// All of them live inside [`RouteFadeLayout`], which renders the outlet and
/// lets the page arriving in it fade in, so that a page change is a short fade
/// rather than a jump. Every page fades the same way — see
/// [`route_transitions`](components::route_transitions) for why there is no
/// route-specific effect, and why the fade is not an animated outlet.
#[derive(Routable, PartialEq, Clone)]
#[rustfmt::skip]
pub enum Route {
    #[layout(RouteFadeLayout)]
    /// The selection route allows the user to select songs or other elements for the presentation
    #[route("/")]
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
    Detail { element: Vec<String> },

    /// The wizard is shown when the program is run for the first time (no configuration file exists)
    #[route("/wizard")]
    Wizard {},

    /// The settings page is shown when explicitly called
    #[route("/settings")]
    SettingsPage {},

    /// The presentation design settings page with a dynamic index
    #[route("/settings/design/:index")]
    PresentationDesignSettingsPage { index: u16 },

    /// The song slide settings page with a dynamic index
    #[route("/settings/slide/:index")]
    SongSlideSettingsPage { index: u16 },

    /// The presenter console shown in the main window during a presentation
    #[route("/presenter")]
    PresenterConsolePage {},

    /// The presentation view shown in the same tab (when presenter console is disabled)
    /// or opened in a new tab (when presenter console is enabled, on web).
    #[route("/presentation")]
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

        // The program is perfectly usable without its icon in the task bar, so
        // a picture that cannot be decoded is written to the log and the
        // window opens with the system's default icon instead of not at all.
        let icon = {
            let icon_bytes = include_bytes!("../assets/favicon.png");
            match image::load_from_memory(icon_bytes) {
                Ok(icon_image) => {
                    let icon_rgba = icon_image.to_rgba8();
                    let (width, height) = icon_rgba.dimensions();
                    match tao::window::Icon::from_rgba(icon_rgba.into_raw(), width, height) {
                        Ok(icon) => Some(icon),
                        Err(error) => {
                            dioxus::logger::tracing::warn!("the window icon could not be built: {error}");
                            None
                        }
                    }
                }
                Err(error) => {
                    dioxus::logger::tracing::warn!("the window icon could not be read: {error}");
                    None
                }
            }
        };

        let mut window = tao::window::WindowBuilder::new()
            .with_resizable(true)
            .with_title("Cantara")
            .with_decorations(true)
            .with_visible(true)
            .with_window_icon(icon);

        // The size the window was left at last time, if there was a last time.
        // Only the size: where the window *stood* is not restored, because a
        // screen that has since been unplugged would put it out of sight with
        // nothing to say why. See [`logic::window_state`].
        window = match logic::window_state::load() {
            Some(state) => window
                .with_inner_size(tao::dpi::LogicalSize::new(state.width, state.height))
                .with_maximized(state.maximized),
            None => window.with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0)),
        };
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

    // Keeps track of how large the window is, so that the next start can open
    // it the same way. Every window of the program reports through the same
    // handler, so the events of the presentation and of a separate presenter
    // console have to be told apart from this one's — otherwise going
    // fullscreen for a presentation would be remembered as the main window's
    // size. See [`logic::window_state`].
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::tao::event::{Event as WindowingEvent, WindowEvent};

        let main_window_id = dioxus::desktop::window().id();
        dioxus::desktop::use_wry_event_handler(move |event, _| {
            let WindowingEvent::WindowEvent {
                window_id, event, ..
            } = event
            else {
                return;
            };
            if *window_id != main_window_id {
                return;
            }
            match event {
                WindowEvent::Resized(_) | WindowEvent::Moved(_) => {
                    let window = dioxus::desktop::window();
                    let maximized = window.is_maximized();
                    let size = (!maximized).then(|| {
                        let logical = window
                            .inner_size()
                            .to_logical::<f64>(window.scale_factor());
                        (logical.width, logical.height)
                    });
                    logic::window_state::record(size, maximized);
                }
                // The last resize before the window closes is the one the user
                // meant to keep, and it is usually inside the interval the
                // periodic write skips.
                WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                    logic::window_state::flush();
                }
                _ => {}
            }
        });
    }

    let cloned_locale = locale.clone();
    use_context_provider(|| states::RuntimeInformation {
        language: cloned_locale,
    });

    // Initialize settings and provide them as a context to all components
    let settings: Signal<Settings> = use_signal(Settings::load);
    use_context_provider(|| settings);

    // The installed fonts are read on a thread of their own, started here so
    // that they are usually there by the time anyone opens a design — the
    // settings show what is available without ever waiting for them. See
    // [`logic::fonts`].
    use_hook(logic::fonts::prepare_system_fonts);

    // The source files and selected items should live here because they should stay persistent in the different routes.
    let mut source_files: Signal<Vec<SourceFile>> = use_context_provider(|| Signal::new(vec![]));
    let _: Signal<Vec<SelectedItemRepresentation>> = use_context_provider(|| Signal::new(vec![]));

    // Which kind of element the library list is showing. Kept here so that it
    // survives switching between the selection and the detail view, and so
    // that the detail view — which re-mounts every time an element is opened,
    // since that writes the address — does not throw it away. It starts on
    // whatever the user has dragged to the top of the sidebar.
    // Something that changes a file in a watched folder — the editor — asks
    // for a fresh scan through this.
    let library_refresh = use_context_provider(states::LibraryRefresh::new);

    use_context_provider(|| states::LibraryFilterState {
        active: Signal::new(states::first_sidebar_type(
            &settings.peek().sidebar_order,
        )),
    });

    // The running presentations given as a global signal
    let _: Signal<Vec<RunningPresentation>> = use_context_provider(|| Signal::new(vec![]));

    // Keeps every browser watching over the network in step with the
    // presentation.
    //
    // Here rather than in the presentation window, because this is the window
    // that outlives it: a viewer who opens the address between two services
    // is told to wait rather than met with a dead connection, which is the
    // whole reason the server stays up while the switch is on.
    //
    // Publishing is cheap and does nothing at all when streaming is off, so
    // this may follow every change without asking first.
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Counts up when streaming is switched on or off. Turning the switch is
        // not a change to the presentation, so without something for the
        // publisher below to watch, enabling streaming in the middle of a
        // service would publish nothing until the next slide change.
        let stream_generation: Signal<u64> = use_context_provider(|| Signal::new(0));
        let running_presentations: Signal<Vec<RunningPresentation>> = use_context();
        use_effect(move || {
            use logic::stream::protocol::StreamState;

            // Read before deciding, both of them. An effect subscribes to
            // what it *reads*, so returning early on "streaming is off" before
            // touching a signal meant this effect subscribed to nothing at all
            // and never ran a second time — the server sat on its opening
            // "nothing is running" for the rest of the session. And switching
            // streaming on is not a change to the presentation, so the switch
            // bumps a counter of its own to bring this back round.
            let _ = stream_generation();
            let presentations = running_presentations.read().clone();

            if !logic::stream::is_enabled() {
                return;
            }

            let state = match presentations.first() {
                Some(running) => StreamState::of(running, 0),
                // Between services. The address stays open and says so.
                None => StreamState::waiting(0),
            };

            // A picture cannot be sent as words, so the slides that are one are
            // rendered and handed over before the state that refers to them —
            // otherwise a viewer asks for a picture that is not there yet.
            let wanted = state.media();
            let sources = picture_sources(&presentations);

            spawn(async move {
                for id in wanted {
                    if logic::stream::has_media(&id) {
                        continue;
                    }
                    let Some(source) = sources.get(&id) else {
                        continue;
                    };
                    if let Some((bytes, content_type)) = render_for_stream(source).await {
                        logic::stream::publish_media(id, bytes, content_type);
                    }
                }
                logic::stream::publish(state);
            });
        });
    }

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
        // Subscribes this scan to the editor's requests as well; the value
        // itself means nothing beyond "something changed on disk".
        let _ = library_refresh.generation();

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

            // The list of pictures shows scaled-down copies, which are made on
            // background threads. Started here rather than when that list is
            // first drawn, so that they are usually there by the time anyone
            // looks — see [`logic::images`].
            crate::logic::images::prepare_thumbnails(
                files
                    .iter()
                    .filter(|file| {
                        file.file_type == logic::sourcefiles::SourceFileType::Image
                    })
                    .map(|file| file.path.clone())
                    .collect(),
            );

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
        // Every stylesheet and script the routes need is registered *here*,
        // although only some of the views use them.
        //
        // A `document::Link` puts its tag into the head from an effect that is
        // queued when the component mounts, and Dioxus discards the queued
        // effects of a scope that is dropped before they run. It also remembers
        // every href it has already seen and never inserts it twice, so a lost
        // insertion cannot be made up for later. A route is exactly such a
        // scope: the animated outlet (see
        // [`route_transitions`](components::route_transitions)) mounts the page
        // that is being navigated to once inside the running transition and
        // again after it has settled, and the first of those two is dropped —
        // taking the stylesheet with it. That is what left the presenter
        // console and the slide preview unstyled.
        //
        // `App` is the root component and is never unmounted, so its
        // registrations always arrive.
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
        document::Link { rel: "stylesheet", href: PRESENTER_CONSOLE_CSS }
        document::Script { src: PRESENTATION_JS }
        document::Script { src: MORPH_JS }
        // The PDF viewer, loaded once per window. Registered *here*, at the
        // root, for the reason written above about the stylesheets: a
        // registration made by a scope that is dropped before its effect runs
        // is lost, and Dioxus never inserts the same src twice — so a script
        // asked for from inside a slide or a thumbnail may never arrive at
        // all. That is what left the presenter console's overview blank.
        document::Script { src: crate::logic::pdf::PDF_VIEWER_JS }
        // Makes the fonts shipped in `assets/fonts/` usable by name.
        BundledFontFaces {}
        document::Link { rel: "icon", href: FAVICON }
        document::Title { "Cantara" }

        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Meta { name: "content-language", content: locale }

        Router::<Route> {}

        // Whatever question is being asked, drawn over the page. Mounted at the
        // root and never unmounted, so that a dialog opened from a view that is
        // navigating away still has somewhere to appear. See
        // [`components::dialogs`].
        DialogHost {}
    }
}

/// Where every picture in a running order came from, by the name the stream
/// gives it.
///
/// The state a viewer receives names its pictures but does not say where they
/// are — a viewer has no file system and no business knowing about one. This
/// is the other half of that mapping, kept on this side.
#[cfg(not(target_arch = "wasm32"))]
fn picture_sources(
    running: &[RunningPresentation],
) -> std::collections::HashMap<String, String> {
    use cantara_songlib::slides::SlideContent;
    use logic::stream::protocol::media_id;

    let mut sources = std::collections::HashMap::new();
    for presentation in running {
        for chapter in &presentation.presentation {
            // The design's background picture, which is as much a part of what
            // a viewer sees as any slide. The design the *stream* uses, which
            // is the projection's unless the service asked for another.
            if let Some(design) = chapter.design_for_stream()
                && let logic::settings::PresentationDesignSettings::Template(template) =
                    &design.presentation_design_settings
                && let Some(picture) = &template.background_image
            {
                let path = picture.as_source().path.to_string_lossy().to_string();
                sources.insert(media_id(&path), path);
            }

            // Likewise the slides: where the stream has a division of its own,
            // those are the pictures a viewer will ask for.
            for slide in chapter.slides_for_stream() {
                let source = match &slide.slide_content {
                    SlideContent::SimplePicture(picture) => {
                        logic::presentation::get_picture_path(picture)
                    }
                    SlideContent::PdfPage(page) => {
                        format!("{}#page={}", page.pdf_path, page.page_number)
                    }
                    _ => continue,
                };
                sources.insert(media_id(&source), source);
            }
        }
    }
    sources
}

/// A picture, as bytes a browser can be handed.
///
/// A PDF page has to be rendered first, which happens in this window's own
/// page and needs nothing to be on screen — the same route the PowerPoint
/// export takes. See [`logic::pdf::page_image`].
#[cfg(not(target_arch = "wasm32"))]
async fn render_for_stream(source: &str) -> Option<(Vec<u8>, &'static str)> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let data_url = match logic::pdf::pdf_page_of(source) {
        Some((document, page)) => {
            logic::pdf::page_image(&document, page, STREAM_PICTURE_WIDTH).await?
        }
        None => logic::images::image_data_url(std::path::Path::new(source))?,
    };

    // `data:{type};base64,{payload}` — the server wants the bytes, not the
    // wrapper a web view needs.
    let (declared, payload) = data_url.split_once(";base64,")?;
    let content_type = if declared.contains("jpeg") {
        "image/jpeg"
    } else {
        "image/png"
    };
    let bytes = BASE64.decode(payload).ok()?;
    Some((bytes, content_type))
}

/// How wide a picture is rendered for a viewer.
///
/// A phone is not a projector: a full-resolution page would be several
/// megabytes over a hall's wi-fi for no visible gain on a screen a few inches
/// across.
#[cfg(not(target_arch = "wasm32"))]
const STREAM_PICTURE_WIDTH: u32 = 1280;
