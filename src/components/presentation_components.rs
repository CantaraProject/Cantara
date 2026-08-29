//! This module provides functionality for rendering the slides in HTML for the presentation

use cantara_songlib::slides::*;
use dioxus::prelude::*;
use regex::Regex;
use rust_i18n::t;
use std::path::PathBuf;
use uuid::Uuid;

use crate::logic::css::{CssHandler, PlaceItems};
use crate::logic::presentation::{get_markdown_html, get_picture_path};
use crate::logic::settings::{CssSize, HorizontalAlign, VerticalAlign};
#[cfg(target_arch = "wasm32")]
use crate::logic::sync::{
    SYNC_KEY_ACTIVE, SYNC_KEY_FILES, SYNC_KEY_POSITION, SYNC_KEY_POSITION_FROM_CONSOLE,
    SYNC_KEY_PRESENTATION, SYNC_KEY_QUIT,
};
#[cfg(target_arch = "wasm32")]
use crate::logic::web_storage;
use crate::{
    MAIN_CSS,
    logic::{
        settings::{AfterLastSlide, FontRepresentation, NotationSettings, PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate, SlideTransition},
        states::RunningPresentation,
    },
};

/// Puts the files a synced presentation needs — PDFs, so far — into this tab's
/// virtual file system, and forgets them again.
///
/// The presentation tab has no disk to read from: whatever it is to show came
/// across in local storage, base64-encoded, and has to be unpacked before the
/// first render. Forgotten afterwards because local storage is small and a PDF
/// is not, and because the tab that wrote it writes it again whenever it
/// changes.
#[cfg(target_arch = "wasm32")]
fn restore_synced_files() {
    use crate::logic::settings::RepositoryType;
    use std::collections::HashMap;

    let Some(files) = web_storage::read::<HashMap<String, String>>(SYNC_KEY_FILES) else {
        return;
    };

    for (path, encoded) in &files {
        if let Ok(bytes) =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        {
            RepositoryType::store_web_file(path, bytes);
        }
    }
    web_storage::remove(SYNC_KEY_FILES);
}


/// The stylesheet and the scripts of the presentation.
///
/// Public because [`App`](crate::App) registers them itself — see the comment
/// there for why a route may not be the one to do that. The components below
/// still register them as well, for the separate windows the desktop opens:
/// each of those runs its own `VirtualDom`, in which `App` does not exist.
pub const PRESENTATION_CSS: Asset = asset!("/assets/presentation.css");
pub const PRESENTATION_JS: Asset = asset!("/assets/presentation_positioning.js");
/// Installs the observer behind [`SlideTransition::Morph`].
pub const MORPH_JS: Asset = asset!("/assets/morph_transition.js");

/// The `%%staffsep` value at which abcjs engraves exactly as it does without
/// the directive at all.
///
/// Measured, not documented: rendering the same tune with and without the
/// directive gives an identical height at 46, and 2 units either side shift it
/// by about 3px. The design's staff line height is a multiple of this, so 1.0
/// leaves the engraving untouched.
const ABCJS_NEUTRAL_STAFF_SEPARATION: f64 = 46.0;
const ABC_RENDER_JS: &str = include_str!("../../assets/abc_render_inline.js");
/// abcjs is bundled from `node_modules` so that notation renders without a
/// network connection — a presentation in a church hall often has none.
#[cfg(not(target_arch = "wasm32"))]
/// Already minified, and minifying it again risks breaking the UMD wrapper that
/// publishes `window.ABCJS` — the way it broke PptxGenJS.
const ABCJS_LIB: Asset = asset!(
    "/node_modules/abcjs/dist/abcjs-basic-min.js",
    AssetOptions::js().with_minify(false)
);
/// On the web target `node_modules` is not available, so the library comes
/// from a CDN, as it does for PDF.js.
#[cfg(target_arch = "wasm32")]
const ABCJS_CDN_LIB: &str = "https://cdn.jsdelivr.net/npm/abcjs@6.6.4/dist/abcjs-basic-min.js";
#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
/// CDN URL for PDF.js library (used on the web/WASM target where node_modules are unavailable).
/// Loaded via dynamic `import()` in JavaScript, which does not support Subresource Integrity (SRI).
#[cfg(target_arch = "wasm32")]
#[cfg(target_arch = "wasm32")]

rust_i18n::i18n!("locales", fallback = "en");

/// The presentation page as the entry point for the presentation window.
/// Works as a standalone desktop window, an in-app routed page, or a synced
/// new-tab presentation on the web target.
#[component]
pub fn PresentationPage() -> Element {
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    // On web, check if this is a synced new-tab presentation (opened by the presenter console).
    // In that case the running_presentations signal will be empty, and we load data from
    // localStorage. The data is stored in a local variable first; it is pushed into the
    // shared signal via use_effect (after rendering completes) to avoid writing to a signal
    // during the render phase, which causes "RefCell already borrowed" panics on web.
    #[cfg(target_arch = "wasm32")]
    let synced_rp: Option<RunningPresentation> = {
        if running_presentations.peek().is_empty() {
            // Restore VFS file data (e.g. PDFs) from localStorage so that
            // PdfPageCanvas can read them. This must happen before the
            // presentation renders.
            restore_synced_files();

            web_storage::read(SYNC_KEY_PRESENTATION)
        } else {
            None
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    let synced_rp: Option<RunningPresentation> = None;

    // On non-desktop builds, navigator() is used to detect whether this is a routed page
    // and to navigate back on quit. On desktop this page always runs as a standalone window
    // (without a router), so calling navigator() would panic.
    #[cfg(not(feature = "desktop"))]
    let nav = navigator();
    // Detect whether we are a standalone window (desktop) or a routed page (web/in-app).
    #[cfg(not(feature = "desktop"))]
    let is_routed = nav.can_go_back();
    // On web, detect if this is a synced tab by checking localStorage flag
    #[cfg(target_arch = "wasm32")]
    let is_synced_tab = web_storage::flag(SYNC_KEY_ACTIVE);
    #[cfg(all(not(target_arch = "wasm32"), not(feature = "desktop")))]
    let is_synced_tab = false;

    // If there's still no presentation data (and no synced data from localStorage),
    // close the window (desktop) or show an error.
    if running_presentations.peek().is_empty() && synced_rp.is_none() {
        #[cfg(feature = "desktop")]
        dioxus::desktop::window().close();

        return rsx! {
            document::Link { rel: "stylesheet", href: MAIN_CSS }
            BundledFontFaces {}
            div { style: "all: initial; margin:0; width:100%; height:100%; background-color: black; color: white; display: flex; align-items: center; justify-content: center;",
                p { "No presentation data found." }
            }
        };
    }

    let Some(initial_rp) = synced_rp.or_else(|| running_presentations.peek().first().cloned()) else {
        #[cfg(feature = "desktop")]
        dioxus::desktop::window().close();

        return rsx! {
            document::Link { rel: "stylesheet", href: MAIN_CSS }
            BundledFontFaces {}
            div { style: "all: initial; margin:0; width:100%; height:100%; background-color: black; color: white; display: flex; align-items: center; justify-content: center;",
                p { "No presentation data found." }
            }
        };
    };
    let mut running_presentation: Signal<RunningPresentation> =
        use_signal(move || initial_rp);

    // When this window/component is destroyed (e.g. user closes the window),
    // clear the shared running presentations so the presenter console also closes.
    // Use try_write() instead of write() to avoid a panic when the owning scope
    // (the main window's App component) has already been dropped before this
    // use_drop callback fires — which can happen on Windows when a drag-drop
    // event triggers an unexpected teardown sequence.
    use_drop(move || {
        if let Ok(mut guard) = running_presentations.try_write() {
            guard.clear();
        }
    });

    // Deferred: for synced new-tab presentations, push the localStorage data
    // into the shared signal after rendering (not during) so other subscribers
    // (e.g. the shared→local effect) can see it.
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if is_synced_tab && running_presentations.peek().is_empty() {
            running_presentations.write().push(running_presentation.peek().clone());
        }
    });

    // ── Desktop: polling-based bidirectional sync ──────────────────────────
    //
    // On desktop, each window runs a separate VirtualDom instance. Dioxus
    // reactive primitives (use_effect, Signal subscriptions) only fire within
    // a single VirtualDom, so they CANNOT propagate changes across windows.
    //
    // A previous approach used reactive use_effect hooks alongside a polling
    // loop, but this caused race conditions: when the other window updated the
    // shared signal, the reactive local→shared effect would re-fire, read the
    // stale local value, and overwrite the shared signal — reverting the slide
    // change. The fix is to use a SINGLE polling loop as the sole sync
    // mechanism on desktop, with no reactive effects involved.
    //
    // The loop runs every 50ms and tracks both sides independently:
    //
    //   last_seen_shared  — snapshot of the shared signal from the previous tick.
    //                       When this differs from the current shared value, the
    //                       OTHER window must have pushed an update → pull it
    //                       into the local signal.
    //
    //   last_seen_local   — snapshot of the local signal from the previous tick.
    //                       When this differs from the current local value, THIS
    //                       window's user action caused a change → push it to
    //                       the shared signal.
    //
    // The shared-changed branch is checked FIRST (higher priority), so incoming
    // slide changes from the presenter console are never overwritten by a stale
    // local push.
    //
    // All comparisons use `eq_ignoring_scroll` to exclude the
    // `markdown_scroll_position` field, which is synced independently by
    // `MarkdownSlideComponent`. This prevents scroll position updates from
    // triggering full component re-renders or interfering with slide navigation.
    //
    // The loop also monitors whether the shared signal was cleared (presentation
    // ended) and closes the window in that case.
    #[cfg(feature = "desktop")]
    use_future(move || async move {
        let mut last_seen_shared = running_presentations.peek()
            .first().cloned().unwrap_or_else(|| running_presentation.peek().clone());
        let mut last_seen_local = running_presentation.peek().clone();

        loop {
            let _ = document::eval("await new Promise(r => setTimeout(r, 50))").await;

            // Presentation ended (signal cleared by use_drop) → close window
            if running_presentations.peek().is_empty() {
                dioxus::desktop::window().close();
                return;
            }

            let current_shared = running_presentations.peek()
                .first().cloned();
            let current_local = running_presentation.peek().clone();

            if let Some(ref shared_rp) = current_shared {
                // Shared signal changed (other window pushed an update) → pull into local
                if !shared_rp.eq_ignoring_scroll(&last_seen_shared) {
                    last_seen_shared = shared_rp.clone();
                    if !shared_rp.eq_ignoring_scroll(&current_local) {
                        last_seen_local = shared_rp.clone();
                        running_presentation.set(shared_rp.clone());
                    }
                }
                // Local signal changed (this window's user action) → push to shared
                else if !current_local.eq_ignoring_scroll(&last_seen_local) {
                    last_seen_local = current_local.clone();
                    if !current_local.eq_ignoring_scroll(shared_rp) {
                        // Merge local non-scroll changes with the current shared scroll position
                        let mut merged = current_local.clone();
                        merged.markdown_scroll_position = shared_rp.markdown_scroll_position;
                        last_seen_shared = merged.clone();
                        if let Some(first) = running_presentations.write().first_mut() {
                            *first = merged;
                        }
                    }
                }
            }
        }
    });

    // ── Web: reactive bidirectional sync ─────────────────────────────────
    //
    // On the web there is only a single VirtualDom, so reactive use_effect
    // hooks work correctly and no polling is needed.

    // shared→local: propagate changes from the shared signal (e.g. from the
    // synced presenter console tab) into the local signal. Also navigates
    // back to selection if the presentation was ended.
    #[cfg(not(feature = "desktop"))]
    use_effect(move || {
        let current = running_presentations.read();
        if current.is_empty() {
            // Drop the read guard BEFORE navigating — on web (single VirtualDom),
            // nav.replace() triggers a synchronous re-render/diff that would
            // attempt to borrow the same RefCell, causing a "RefCell already
            // borrowed" panic.
            drop(current);
            if is_routed {
                nav.replace(crate::Route::Selection {});
            }
            return;
        }
        if let Some(rp) = current.first()
            && !rp.eq_ignoring_scroll(&running_presentation.peek()) {
                let rp = rp.clone();
                drop(current);
                running_presentation.set(rp);
            }
    });

    // local→shared: push local changes (e.g. user clicked next slide) back
    // to the shared signal. Uses .peek() for the shared read to avoid
    // subscribing to it (only local changes should trigger this effect).
    #[cfg(not(feature = "desktop"))]
    use_effect(move || {
        let local = running_presentation.read().clone();
        let shared = running_presentations.peek();
        if let Some(first) = shared.first()
            && !first.eq_ignoring_scroll(&local) {
                drop(shared);
                if let Some(first) = running_presentations.write().first_mut() {
                    // Merge local changes into the shared state, but preserve the
                    // shared markdown_scroll_position to avoid overwriting a newer
                    // scroll value with a stale local one.
                    let mut merged = local;
                    merged.markdown_scroll_position = first.markdown_scroll_position;
                    *first = merged;
                }
            }
    });

    // On web synced tab: write position changes to localStorage for the presenter console
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if is_synced_tab {
            web_storage::write(SYNC_KEY_POSITION, &*running_presentation.read());
        }
    });

    // On web synced tab: poll for position changes from the presenter console
    #[cfg(target_arch = "wasm32")]
    {
        let mut last_sync_json = use_signal(String::new);
        use_future(move || async move {
            // If this is not a synced tab, do nothing.
            if !is_synced_tab {
                return;
            }
            loop {
                // Wait ~150ms between polls
                let _ = document::eval("await new Promise(r => setTimeout(r, 150))").await;

                // Check if the presentation was quit by the presenter console
                if web_storage::flag(SYNC_KEY_QUIT) {
                    running_presentations.write().clear();
                    // Close this tab
                    let _ = document::eval("window.close()").await;
                    return;
                }

                // Read position updates from the presenter console
                if let Some(json) = web_storage::text(SYNC_KEY_POSITION_FROM_CONSOLE)
                    && !json.is_empty() && json != *last_sync_json.peek() {
                        last_sync_json.set(json.clone());
                        if let Ok(rp) = serde_json::from_str::<RunningPresentation>(&json)
                            && *running_presentation.peek() != rp {
                                // Load any new VFS files (e.g. PDFs) that the
                                // update_presentation call stored in localStorage
                                restore_synced_files();
                                running_presentation.set(rp);
                            }
                    }
            }
        });
    }

    // Context menu state
    let mut show_context_menu = use_signal(|| false);
    let mut context_menu_x = use_signal(|| 0.0f64);
    let mut context_menu_y = use_signal(|| 0.0f64);

    let mut quit_presentation = move || {
        // Clean up sync state on web
        #[cfg(target_arch = "wasm32")]
        {
            // Signal quit to any synced tabs, then clear the rest so that
            // nothing stale is read back.
            web_storage::write_text(SYNC_KEY_QUIT, "true");
            web_storage::remove_all(&[
                SYNC_KEY_ACTIVE,
                SYNC_KEY_PRESENTATION,
                SYNC_KEY_POSITION,
                SYNC_KEY_POSITION_FROM_CONSOLE,
                SYNC_KEY_FILES,
            ]);
        }
        running_presentations.write().clear();
        #[cfg(feature = "desktop")]
        dioxus::desktop::window().close();
        #[cfg(not(feature = "desktop"))]
        {
            if is_synced_tab {
                // Close this tab (best effort, may be blocked by browser)
                let _ = document::eval("window.close()");
            } else if is_routed {
                nav.replace(crate::Route::Selection {});
            }
        }
    };

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        BundledFontFaces {}
        // Serves this window's video files. An asset handler belongs to a web
        // view, and the presentation window is a window of its own with a web
        // view of its own — registering one in the main window does nothing
        // for it. That is why the video played in the presenter console, which
        // lives in the main window, and showed nothing here.
        crate::components::video_host::VideoAssetHost {}
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
        // The PDF viewer, loaded once per window. Registered *here*, at the
        // root, for the reason written above about the stylesheets: a
        // registration made by a scope that is dropped before its effect runs
        // is lost, and Dioxus never inserts the same src twice — so a script
        // asked for from inside a slide or a thumbnail may never arrive at
        // all. That is what left the presenter console's overview blank.
        document::Script { src: crate::logic::pdf::PDF_VIEWER_JS }
        document::Title { {t!("presentation.title").to_string()} }
        // This div is needed for fullscreen mode
        div {
            tabindex: 0,
            style: "
                    all: initial;
                    margin:0;
                    width:100%;
                    height:100%;
                ",
            onclick: move |_| {
                // Close context menu on any click
                show_context_menu.set(false);
            },
            oncontextmenu: move |event: Event<MouseData>| {
                event.prevent_default();
                let coords = event.page_coordinates();
                context_menu_x.set(coords.x);
                context_menu_y.set(coords.y);
                show_context_menu.set(true);
            },
            onkeydown: move |event: Event<KeyboardData>| {
                // Close context menu on any key press
                show_context_menu.set(false);
                match event.key() {
                    Key::F5 | Key::F11 => {
                        #[cfg(feature = "desktop")]
                        {
                            let desktop = dioxus::desktop::window();
                            let is_fullscreen = desktop.fullscreen().is_some();
                            desktop.set_fullscreen(!is_fullscreen);
                        }
                        #[cfg(not(feature = "desktop"))]
                        {
                            use_future(move || async move {
                                let _ = document::eval(
                                        "
                                                                            if (document.fullscreenElement) {
                                                                                document.exitFullscreen();
                                                                            } else {
                                                                                document.documentElement.requestFullscreen();
                                                                            }
                                                                        ",
                                    )
                                    .await;
                            });
                        }
                    }
                    Key::Escape => {
                        quit_presentation();
                    }
                    Key::Character(ref c) if c == "b" || c == "B" || c == "." => {
                        running_presentation.write().toggle_black_screen();
                    }
                    _ => {}
                }
            },
            PresentationRendererComponent { running_presentation }

            // Where a video is operated from when there is no console to
            // operate it in. A video nobody can pause is not something to put
            // in front of a congregation — and the console, where these
            // normally live, may simply not be open.
            //
            // The bar floats over the projection and fades out of the way; see
            // `.presentation-video-controls` in `assets/presentation.css`.
            if !crate::logic::video::owns_audio(crate::logic::video::AudioOwner::Console) {
                div { class: "presentation-video-controls",
                    crate::components::presenter_console_components::VideoControls {
                        running_presentation,
                    }
                }
            }

            // Context menu overlay
            if *show_context_menu.read() {
                div {
                    class: "presentation-context-menu",
                    style: "left: {context_menu_x}px; top: {context_menu_y}px;",
                    div {
                        class: "presentation-context-menu-item",
                        onclick: move |_| {
                            show_context_menu.set(false);
                            quit_presentation();
                        },
                        {t!("presenter.quit").to_string()}
                    }
                }
            }
        }
    }
}

/// What a rendering of a presentation is *for*.
///
/// Three things follow from it, and they do not line up with any single yes or
/// no — which is why this is a role and not a flag. It was one, called
/// `fire_timer`, and by the time it also decided which view publishes its
/// layout size and which hides its scrollbar, the name had stopped describing
/// what it did.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PresentationRole {
    /// The window the audience is looking at. Exactly one of these exists
    /// while a presentation is running.
    ///
    /// It drives the auto-advance timer, so a slide is not advanced once per
    /// window showing it; it measures and publishes the size it is laid out
    /// at, which is what everything showing the same slide beside it lays
    /// itself out at; and it hides its scrollbar, because the audience should
    /// not be looking at one.
    #[default]
    Audience,
    /// A view that follows the presentation without being it: the presenter
    /// console's preview, the design selector. It advances when the
    /// presentation does and never of its own accord.
    Follower,
    /// A preview that runs by itself — the options panel's example of a slide
    /// timer, which has to advance in order to show what was configured. It
    /// fires the timer as the audience view does but is not one: it keeps its
    /// scrollbar, and the size it happens to be laid out at is nobody else's
    /// business.
    SelfRunning,
}

impl PresentationRole {
    /// Whether this is the window the audience is looking at.
    pub fn is_audience_view(self) -> bool {
        matches!(self, PresentationRole::Audience)
    }

    /// Whether this view advances the slides by itself when a timer is set.
    pub fn fires_timer(self) -> bool {
        matches!(
            self,
            PresentationRole::Audience | PresentationRole::SelfRunning
        )
    }
}

/// The actual presentation rendering component which can be used to render presentations accordingly
/// It takes a signal and rewrites to it when the presentation position changes
#[component]
pub fn PresentationRendererComponent(
    /// The running presentation as a signal: This will be changed by the component if the user moves the current slide
    running_presentation: Signal<RunningPresentation>,
    /// What this rendering is for. Defaults to [`PresentationRole::Audience`]:
    /// the presentation window renders it without saying so, and every other
    /// view has to name itself.
    #[props(default)]
    role: PresentationRole,
) -> Element {
    // Handed down rather than threaded through every slide component between
    // here and the `<video>` element that needs it. What a rendering is *for*
    // is the same for everything inside it, and the only thing that asks is
    // several layers down — see [`VideoSlideComponent`].
    use_context_provider(|| role);

    let current_slide: Memo<Option<Slide>> =
        use_memo(move || running_presentation.read().get_current_slide());

    let current_slide_number: Memo<usize> =
        use_memo(move || match running_presentation.read().clone().position {
            Some(position) => position.slide_total(),
            None => 0,
        });

    let mut presentation_is_visible = use_signal(|| false);

    let is_black_screen =
        use_memo(move || running_presentation.read().is_black_screen);

    // Derive the CSS transition class for the current chapter.
    let transition_class = use_memo(move || {
        match running_presentation.read().get_current_transition() {
            SlideTransition::None => "",
            SlideTransition::Fade => "presentation-fade-in",
            SlideTransition::SlideFromRight => "presentation-slide-from-right",
            SlideTransition::SlideFromLeft => "presentation-slide-from-left",
            SlideTransition::ZoomIn => "presentation-zoom-in",
            // The morph is driven by `morph_transition.js`, which watches for
            // the class rather than being told about the change.
            SlideTransition::Morph => "presentation-morph",
        }
    });

    let mut go_to_next_slide = move || {
        running_presentation.write().next_slide();
    };

    let mut go_to_previous_slide = move || {
        running_presentation.write().previous_slide();
    };

    // The slide on the stage, and what identifies the element it lives in.
    //
    // For everything but a PDF that is the slide number, so each slide gets an
    // element of its own — which is the only thing that starts a CSS animation
    // and therefore the only way the configured effect runs at all.
    //
    // For a PDF it is the *document*. Stepping through its pages then reuses
    // one element, and with it one canvas, so the page on screen stays until
    // the next has been drawn beside it and copied across: no empty screen
    // between two pages, whatever the effect. The effect itself is started by
    // `pdf_viewer.js` at the moment the new page appears, so the container
    // carries none — see [`crate::logic::pdf::show`].
    let stage: Memo<Option<(String, Slide, &'static str)>> = use_memo(move || {
        let slide = current_slide.read().clone()?;
        match pdf_document_of(&slide.slide_content) {
            Some(document) => Some((format!("pdf:{document}"), slide, "")),
            None => Some((
                format!("slide:{}", current_slide_number()),
                slide,
                transition_class(),
            )),
        }
    });

    // The presentation window measures the size it is actually laid out at and
    // publishes it, so that anything showing the same slide beside it — the
    // console's preview, its overview — lays it out at exactly the same size
    // and breaks its lines in the same places.
    //
    // Only the window the audience is looking at does this: it is the one
    // whose layout everything else has to match. Watched rather than measured
    // once, because a window that is not fullscreen can be resized in the
    // middle of a presentation.
    use_effect(move || {
        if !role.is_audience_view() {
            return;
        }
        spawn(async move {
            loop {
                if let Ok(measured) = document::eval(
                    "var el = document.querySelector('.presentation');
                     if (!el) return null;
                     return { w: el.clientWidth, h: el.clientHeight };",
                )
                .await
                    && let (Some(width), Some(height)) = (
                        measured.get("w").and_then(|w| w.as_f64()),
                        measured.get("h").and_then(|h| h.as_f64()),
                    )
                    && width > 0.0
                    && height > 0.0
                {
                    let measured = Some((width, height));
                    if running_presentation.peek().presentation_layout != measured {
                        running_presentation.write().presentation_layout = measured;
                    }
                }
                let _ = document::eval("await new Promise(r => setTimeout(r, 500))").await;
            }
        });
    });

    // Auto-advance timer: each time the slide changes, a new `spawn`-ed task
    // is launched via `use_effect`. A generation counter ensures that only the
    // most-recent timer fires – if the user (or a previous timer) navigated to
    // a new slide before the sleep completed, the old task detects the changed
    // generation and exits without advancing again.
    //
    // Only a view that [`PresentationRole::fires_timer`] does this. Without the
    // guard every window hosting a `PresentationRendererComponent` would
    // advance the slide independently, and slides would be skipped as fast as
    // there are windows.
    let mut timer_generation: Signal<u64> = use_signal(|| 0);

    use_effect(move || {
        // Track slide changes by reading current_slide_number (subscribes to it)
        let _ = current_slide_number();

        if !role.fires_timer() {
            return;
        }

        // Increment the generation so any in-flight timer task will abort.
        let generation_id = {
            let mut g = timer_generation.write();
            *g += 1;
            *g
        };

        let timer_opt = running_presentation.read().get_current_timer_settings();
        if let Some(timer) = timer_opt {
            let after_last = timer.after_last_slide;
            let seconds = if timer.timer_seconds == 0 { 1 } else { timer.timer_seconds } as u64;
            let ms = seconds * 1000;

            spawn(async move {
                // Sleep via JS setTimeout – works on both desktop (WebView) and web.
                // A pure Rust sleep (tokio/async_std) does not pump the WebView event loop.
                // The generation counter (checked below) is sufficient to prevent a stale
                // sleeping task from advancing the slide after the user navigated away.
                let js_sleep = format!("await new Promise(r => setTimeout(r, {ms}))");
                let _ = document::eval(&js_sleep).await;

                // If the slide changed while we were sleeping, abort.
                if *timer_generation.peek() != generation_id {
                    return;
                }

                let is_last = running_presentation.peek().is_last_slide_in_chapter();
                match (is_last, after_last) {
                    (true, AfterLastSlide::RestartCurrentChapter) => {
                        running_presentation.write().restart_current_chapter();
                    }
                    _ => {
                        running_presentation.write().next_slide();
                    }
                }
                presentation_is_visible.set(false);
                presentation_is_visible.set(true);
            });
        }
    });

    // Stop rendering if no slide can be rendered.
    if current_slide.read().clone().is_none() {
        return rsx! {
            div { style: "
                    all: initial;
                    margin:0;
                    width:100%;
                    height:100%;
                    background-color: black;
                ",
                p { {"No presentation data found."} }
            }
        };
    }

    let current_design = use_memo(move || {
        running_presentation
            .read()
            .get_current_presentation_design()
    });

    // The current presentation design settings
    let current_pds =
        use_memo(
            move || match current_design.read().presentation_design_settings.clone() {
                PresentationDesignSettings::Template(template) => template,
                _ => PresentationDesignTemplate::default(),
            },
        );

    let css_text_align: Memo<HorizontalAlign> = use_memo(move || {
        current_pds
            .read()
            .fonts
            .first()
            .unwrap_or(&FontRepresentation::default())
            .horizontal_alignment
    });
    let css_place_items: Memo<PlaceItems> =
        use_memo(move || match current_pds.read().vertical_alignment {
            VerticalAlign::Top => PlaceItems::StartStretch,
            VerticalAlign::Middle => PlaceItems::CenterStretch,
            VerticalAlign::Bottom => PlaceItems::EndStretch,
        });

    // The CSS handler ([CssHandler]) takes all CSS arguments and builds the string from it.
    // We build it in a memo for the sake of consistency.
    let css_handler: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();

        css.background_color(current_pds().background_color);
        css.padding_left(current_pds().padding.left);
        css.padding_right(current_pds().padding.right);
        css.padding_top(current_pds().padding.top);
        css.padding_bottom(current_pds().padding.bottom);
        css.text_align(css_text_align());
        css.set_important(true);
        css.color(
            current_pds
                .read()
                .clone()
                .fonts
                .first()
                .unwrap_or(&FontRepresentation::default())
                .color,
        );
        css.place_items(css_place_items());

        css
    });

    // Have the scaled-down background made, for the views that will use it.
    //
    // Asking here rather than where the picture is read, because that happens
    // while the view is being drawn and must not start any work. The counter is
    // read below so that the tile is redrawn once the small copy has landed;
    // until then it shows the full picture and looks no different.
    let mut thumbnails_ready: Signal<u64> = use_signal(crate::logic::images::thumbnail_generation);
    use_effect(move || {
        if role.is_audience_view() {
            return;
        }
        let Some(path) = current_pds()
            .background_image
            .as_ref()
            .map(|image| image.as_source().path.clone())
        else {
            return;
        };
        if crate::logic::images::thumbnail(&path).is_some() {
            return;
        }
        crate::logic::images::prepare_thumbnails(vec![path]);

        // A background thread cannot write to a signal, so the view looks for
        // the result instead and stops as soon as it is in.
        spawn(async move {
            loop {
                let generation = crate::logic::images::thumbnail_generation();
                if generation != *thumbnails_ready.peek() {
                    thumbnails_ready.set(generation);
                }
                if !crate::logic::images::thumbnails_in_progress() {
                    return;
                }
                let _ = document::eval("await new Promise(r => setTimeout(r, 100))").await;
            }
        });
    });

    let background_css: Memo<String> = use_memo(move || {
        // Read so that this is rebuilt when the scaled copy arrives.
        let _ = thumbnails_ready();
        let mut css: CssHandler = CssHandler::new();
        let pds = current_pds();

        // A `url()` pointing into the file system is as unreachable for the
        // page as an `img` source is, so the picture is inlined the same way.
        //
        // Everything that is not the audience's screen takes the scaled-down
        // copy. A design's background is a photograph off a camera, several
        // thousand pixels across, and the design tiles in the settings show it
        // four hundred pixels wide — where it is not merely displayed small but
        // rescaled on every paint. Scrolling past two of those asks for that
        // sixty times a second; on the wall the full picture is the point, in a
        // tile it is what makes the page stutter.
        //
        // The small copy is only taken when it is already there. Nothing here
        // may wait for a file: this runs while the view is being drawn.
        if let Some(source) = pds.background_image.as_ref().and_then(|image| {
            let path = &image.as_source().path;
            if !role.is_audience_view()
                && let Some(small) = crate::logic::images::thumbnail(path)
            {
                return Some(small);
            }
            crate::logic::images::image_data_url(path)
        }) {
            css.background_image(&source);
            css.background_size("cover");
            css.background_position("center");
            css.background_repeat("no-repeat");
            css.opacity(1.0 - pds.background_transparency as f32 / 100.0f32);
        } else {
            css.background_image_none();
            css.opacity(0.0);
        }
        css.to_string()
    });

    rsx! {
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
        document::Script { src: PRESENTATION_JS }
        document::Script { src: MORPH_JS }
        div {
            // The audience view says so in its class, so the stylesheet can
            // hide the one thing only the audience should not see: its
            // scrollbar. See `presentation.css`.
            class: if role.is_audience_view() { "presentation presentation-live" } else { "presentation" },
            style: css_handler.read().to_string(),

            tabindex: 0,
            onkeydown: move |event: Event<KeyboardData>| {
                let key = event.key();
                match key {
                    Key::ArrowRight | Key::ArrowDown | Key::PageDown | Key::Enter => go_to_next_slide(),
                    Key::Character(ref c) if c == " " => go_to_next_slide(),
                    Key::ArrowLeft | Key::ArrowUp | Key::PageUp => go_to_previous_slide(),
                    _ => {}
                }
            },
            onclick: move |_| {
                go_to_next_slide();
            },
            oncontextmenu: move |_| {
                go_to_previous_slide();
            },
            onmounted: move |_| {
                presentation_is_visible.set(true);
            },
            // Black screen overlay
            if is_black_screen() {
                div { style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; background-color: black; z-index: 1000;" }
            }
            div { class: "background", style: background_css() }
            // A keyed list of one, so that the key decides the element's
            // identity — outside a list it does not, which is why the
            // configured effect used to be re-triggered by hand in the click
            // and key handlers, and therefore not at all when the slide was
            // changed from the presenter console.
            if presentation_is_visible() {
                for (identity , slide , transition) in stage() {
                    {
                        let slide_content = slide.slide_content.clone();
                        let container_style = slide_container_style(&slide_content);

                        rsx! {
                            div {
                                key: "{identity}",
                                class: "slide-container {transition}",
                                style: "{container_style}",
                                SlideContentRenderer {
                                    slide_content,
                                    pds: current_pds(),
                                    running_presentation: Some(running_presentation),
                                    transition: transition_class(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TitleSlideComponent(
    title_slide: TitleSlide,
    title_font_representation: FontRepresentation,
    /// Font for the meta information line below the headline.
    meta_font: FontRepresentation,
    /// Whether the title is set in bold.
    bold: bool,
    /// The gap between the title and its meta line. The same distance the
    /// design uses between main content and spoiler, so a slide has one
    /// consistent rhythm and needs no setting of its own for this.
    meta_distance: CssSize,
) -> Element {
    // Built on every render rather than memoised. A `use_memo` closure is
    // created once and captures the props of the render that created it, so a
    // memo over nothing but props never runs again — the design could change
    // underneath it and the slide would keep the type it was first drawn with.
    // That is what stopped the design editor's preview following the font
    // settings; the meta line below did follow, because it was built here.
    //
    // There is nothing to save by memoising it either: this is a handful of
    // string pushes, against a render that is already laying out a slide.
    let css_handler_string = {
        let mut css = CssHandler::new();
        css.opacity(1.0);
        css.z_index(2);
        let mut font = title_font_representation.clone();
        if bold {
            font.weight = font.weight.max(crate::logic::settings::BOLD_WEIGHT);
        }
        css.extend(&CssHandler::from(font));
        css.to_string()
    };

    let meta_style = {
        let mut css = CssHandler::new();
        css.set_important(true);
        css.opacity(1.0);
        css.extend(&CssHandler::from(meta_font));
        css.margin_top(meta_distance);
        css.to_string()
    };
    let meta_text = title_slide
        .meta_text
        .clone()
        .filter(|text| !text.trim().is_empty());

    rsx! {
        div { class: "headline", style: "{css_handler_string}",
            p { style: "{css_handler_string}", {title_slide.title_text} }
            // On the title slide the meta information belongs under the
            // headline, in the normal flow, so it reads as part of the title.
            if let Some(text) = meta_text {
                p { class: "headline-meta-text", style: "{meta_style}", "{text}" }
            }
        }
    }
}

#[component]
fn SingleLanguageMainContentSlideRenderer(
    /// The slide as a [SingleLanguageMainContentSlide]
    main_slide: SingleLanguageMainContentSlide,

    /// The [FontRepresentation] for the main content font.
    main_content_font: FontRepresentation,

    /// The [FontRepresentation] for the spoiler content font.
    spoiler_content_font: FontRepresentation,

    /// The distance between the main content and the spoiler, default is `4 em`.
    distance: Option<CssSize>,
) -> Element {
    let number_of_main_content_lines = {
        let cloned_main_slide = main_slide.clone();
        let main_text = cloned_main_slide.main_text();
        let lines: Vec<&str> = main_text.split("\n").collect();
        lines.len()
    };

    // Built on every render rather than memoised — see the note in
    // [`TitleSlideComponent`] for why a memo over nothing but props is a memo
    // that never runs again.
    let main_css = {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(main_content_font.clone()));
        css.to_string()
    };

    let distance_css = {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.min_height(distance.clone().unwrap_or(CssSize::Em(4.0)));

        css.to_string()
    };

    let spoiler_css = {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(spoiler_content_font.clone()));
        css.to_string()
    };

    rsx! {
        div {
            div { class: "main-content", style: "{main_css}",
                p { style: "{main_css}",
                    for (num , line) in main_slide.clone().main_text().split("\n").enumerate() {
                        {line}
                        if num < number_of_main_content_lines - 1 {
                            br {}
                        }
                    }
                }
            }
            if let Some(spoiler_content) = main_slide.spoiler_text() {
                div { class: "distance", style: "{distance_css}" }
                div {
                    class: "spoiler-content",
                    style: "{spoiler_css}",
                    p { style: "{spoiler_css}",
                        for (num , line) in spoiler_content.split("\n").enumerate() {
                            {line}
                            if num < spoiler_content.split("\n").count() - 1 {
                                br {}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Renders a multi-language slide with the main content in multiple languages stacked vertically.
#[component]
fn MultiLanguageMainContentSlideRenderer(
    /// The slide as a [MultiLanguageMainContentSlide]
    multi_slide: MultiLanguageMainContentSlide,

    /// The [FontRepresentation] for the main content font.
    main_content_font: FontRepresentation,

    /// The [FontRepresentation] for the spoiler content font.
    spoiler_content_font: FontRepresentation,

    /// The distance between sections, default is `4 em`.
    distance: Option<CssSize>,
) -> Element {
    // Built on every render rather than memoised — see the note in
    // [`TitleSlideComponent`].
    let main_css = {
        let mut css = CssHandler::new();
        css.set_important(true);
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(main_content_font.clone()));
        css.to_string()
    };

    let distance_css = {
        let mut css = CssHandler::new();
        css.set_important(true);
        css.min_height(distance.clone().unwrap_or(CssSize::Em(4.0)));
        css.to_string()
    };

    rsx! {
        div {
            for (lang_idx , text) in multi_slide.main_text_list.iter().enumerate() {
                div { class: "language-section",
                    p {
                        class: "language-label",
                        style: "font-weight: bold; margin-top: 0.5em;",
                        {format!("Language {}", lang_idx + 1)}
                    }
                    p { style: "{main_css}",
                        for (num , line) in text.split("\n").enumerate() {
                            {line}
                            if num < text.split("\n").count() - 1 {
                                br {}
                            }
                        }
                    }
                }
                if lang_idx < multi_slide.main_text_list.len() - 1 {
                    div {
                        class: "distance",
                        style: "{distance_css}",
                    }
                }
            }
            if !multi_slide.spoiler_text_vector.is_empty() {
                div { class: "distance", style: "{distance_css}" }
                div { class: "spoiler-content",
                    p {
                        style: {
                            let mut css = CssHandler::new();
                            css.set_important(true);
                            css.opacity(1.0);
                            css.extend(&CssHandler::from(spoiler_content_font.clone()));
                            css.to_string()
                        },
                        for text in &multi_slide.spoiler_text_vector {
                            for (num , line) in text.split("\n").enumerate() {
                                {line}
                                if num < text.split("\n").count() - 1 {
                                    br {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Renders a complex slide with notation (ABCjs) and lyrics in multiple languages.
/// The notation is rendered using ABCjs library for musical notation display.
#[component]
fn ComplexSlideRenderer(
    /// The slide as a [ComplexSlide]
    complex_slide: ComplexSlide,

    /// The design, which decides how each row is drawn: a lyrics row takes the
    /// block claiming its language and falls back to the main block, and the
    /// notation row takes the notation settings.
    pds: PresentationDesignTemplate,
) -> Element {
    let spoiler_css = {
        let mut css = CssHandler::new();
        css.set_important(true);
        // The colour comes from the design's spoiler font. Pinning the opacity
        // here — as the other slide renderers do — keeps a stylesheet rule from
        // dimming it on top of that.
        css.opacity(1.0);
        css.extend(&CssHandler::from(pds.get_default_spoiler_font()));
        css.to_string()
    };

    // The rows come in the order the user configured, and that order is kept:
    // asking for "english, notation, german" puts the staff between the two
    // languages. Rows whose text the notation already prints underneath its
    // notes are dropped, so nothing is shown twice.
    let visible_rows: Vec<SlideRow> = complex_slide.rows_without_repetition().cloned().collect();

    let spoiler_rows: Vec<String> = complex_slide
        .spoiler
        .iter()
        .filter(|row| !row.is_notation())
        .map(|row| row.content.clone())
        .collect();

    let notation = pds.notation.clone();
    let notation_font = pds.get_default_font();
    let notation_style = notation_block_style(&notation);

    rsx! {
        div { class: "complex-slide",
            for row in visible_rows {
                if row.is_notation() {
                    div {
                        class: "complex-slide-row notation-row",
                        style: "{notation_style}",
                        AbcNotationRenderer {
                            abc_notation: row.content.clone(),
                            notation_font: notation_font.clone(),
                            lyrics_font_size: notation.font_size.clone(),
                            staff_line_height: notation.staff_line_height,
                        }
                    }
                } else {
                    {
                        // The block that claims this row's language, or the
                        // main block when none does.
                        let row_font = pds.font_for_row(row_language(&row).as_deref());
                        let mut css = CssHandler::new();
                        css.set_important(true);
                        css.opacity(1.0);
                        css.z_index(2);
                        css.extend(&CssHandler::from(row_font));
                        let row_style = css.to_string();

                        rsx! {
                            div {
                                class: "complex-slide-row lyrics-row",
                                style: "{row_style}",
                                for (line_number , line) in row.content.lines().enumerate() {
                                    if line_number > 0 {
                                        br {}
                                    }
                                    {line}
                                }
                            }
                        }
                    }
                }
            }

            if !spoiler_rows.is_empty() {
                div { class: "complex-slide-spoiler",
                    for text in spoiler_rows {
                        div { style: "{spoiler_css}",
                            for (line_number , line) in text.lines().enumerate() {
                                if line_number > 0 {
                                    br {}
                                }
                                {line}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The language a lyrics row is in, if the song stated one.
fn row_language(row: &SlideRow) -> Option<String> {
    match &row.kind {
        SlideRowKind::Lyrics { language } => language.clone(),
        SlideRowKind::Notation { .. } => None,
    }
}

/// The `%%staffsep` value a design's staff line height asks for, if it asks for
/// anything.
///
/// `staff_line_height` is a multiple of the engraver's own spacing, so 1.0
/// changes nothing and there is no directive to give.
///
/// Shared with [`crate::logic::stream`], which sends the number to a viewer's
/// browser to prepend there: a phone engraves the same tune as the projection,
/// so it has to be told the same spacing.
pub(crate) fn staff_separation(staff_line_height: f64) -> Option<f64> {
    let factor = staff_line_height.clamp(0.2, 5.0);
    if (factor - 1.0).abs() < f64::EPSILON {
        return None;
    }
    Some((ABCJS_NEUTRAL_STAFF_SEPARATION * factor).round())
}

/// Prefixes the tune with a `%%staffsep` directive for the wanted line height.
fn with_staff_separation(abc: &str, staff_line_height: f64) -> String {
    match staff_separation(staff_line_height) {
        Some(separation) => format!("%%staffsep {separation}\n{abc}"),
        None => abc.to_string(),
    }
}

/// The box the staff is drawn in: how wide it is and where it sits.
///
/// At 100% it is exactly the box the text rows use, so the staff and the words
/// line up on both edges — the notation block gets no padding of its own.
pub(crate) fn notation_block_style(notation: &NotationSettings) -> String {
    let width = notation.width_percent.clamp(10.0, 100.0);

    let margin = match notation.horizontal_alignment {
        HorizontalAlign::Left => "margin-left: 0; margin-right: auto;",
        HorizontalAlign::Right => "margin-left: auto; margin-right: 0;",
        // Justified text has no meaning for a staff; centring is the sane
        // reading of "fill the line" here.
        _ => "margin-left: auto; margin-right: auto;",
    };

    format!("width: {width}%; {margin}")
}

/// The `vocalfont` string abcjs expects for the words under the staff.
///
/// abcjs only accepts this as a `"family size"` **string**; an object is
/// silently ignored and the words stay at the library's small default, which is
/// unreadable from the back of a hall. The size is a point value that abcjs
/// scales by about 4/3 when drawing.
///
/// The size follows the configured spoiler text, so the words under the notes
/// are never smaller than the preview line on the same slide.
pub(crate) fn abcjs_vocal_font(font: &FontRepresentation, size: &CssSize) -> String {
    let points = match size {
        CssSize::Px(value) => value * 0.75,
        CssSize::Pt(value) => *value,
        CssSize::Em(value) => value * 12.0,
        CssSize::Percentage(value) => value / 100.0 * 12.0,
        CssSize::Null => 0.0,
    }
    // Never below abcjs's own default.
    .max(14.0);

    let family = font
        .font_family
        .as_ref()
        .and_then(|family| family.family.clone())
        .filter(|family| !family.trim().is_empty())
        .unwrap_or_else(|| "sans-serif".to_string());

    // abcjs reads the last whitespace-separated token as the size, so a family
    // name of several words still works.
    format!("{} {}", family.replace(['"', '\''], ""), points.round())
}

/// Renders one ABC notation snippet with [abcjs](https://abcjs.net).
///
/// Each notation row handed over by the song library is a complete ABC tune —
/// its own header plus one `w:` lyrics line per system — so it can be drawn
/// as-is.
///
/// The library is loaded by the render script itself rather than by a
/// `document::Script` tag: a tag gives no guarantee that the library has
/// arrived by the time the first slide draws, which used to leave the first
/// notation blank.
#[component]
pub(crate) fn AbcNotationRenderer(
    /// The ABC notation string to render
    abc_notation: String,
    /// Font settings for styling the notation
    notation_font: FontRepresentation,
    /// How large the words under the notes should be drawn.
    lyrics_font_size: CssSize,
    /// The height of one staff line, as a multiple of the engraver's default.
    staff_line_height: f64,

    /// Take the surrounding text colour instead of the font's own.
    ///
    /// A presentation font is chosen for a slide — white, as a rule, because
    /// slides are dark. Drawn on an ordinary page that is invisible, so
    /// anything outside a presentation asks to inherit instead.
    #[props(default)]
    inherit_color: bool,
) -> Element {
    let container_id = use_hook(|| format!("abc-{}", Uuid::new_v4().as_simple()));

    let notation_style = {
        let mut css = CssHandler::new();
        css.set_important(true);
        // The staff is drawn in `currentColor`, so leaving the colour out makes
        // it follow whatever the page uses — in either theme.
        css.extend(&CssHandler::from_font(notation_font.clone(), !inherit_color));
        css.to_string()
    };

    let vocal_font = abcjs_vocal_font(&notation_font, &lyrics_font_size);
    // The gap between systems only reacts to the `%%staffsep` directive in the
    // ABC source; the same value passed as a render option is ignored, exactly
    // as `vocalfont` is unless it sits inside `format`.
    let abc_notation = with_staff_separation(&abc_notation, staff_line_height);

    #[cfg(not(target_arch = "wasm32"))]
    let abcjs_url = format!("{}", ABCJS_LIB);
    #[cfg(target_arch = "wasm32")]
    let abcjs_url = ABCJS_CDN_LIB.to_string();

    rsx! {
        div {
            id: "{container_id}",
            class: "abc-notation-container",
            style: "{notation_style}",
            onmounted: move |_| {
                // Every value is passed through serde so that quotes and
                // newlines in the notation cannot break out of the script.
                let script = ABC_RENDER_JS
                    .replace(
                        "__CONTAINER_ID__",
                        &serde_json::to_string(&container_id).unwrap_or_default(),
                    )
                    .replace(
                        "__ABCJS_URL__",
                        &serde_json::to_string(&abcjs_url).unwrap_or_default(),
                    )
                    .replace(
                        "__ABC_NOTATION__",
                        &serde_json::to_string(&abc_notation).unwrap_or_default(),
                    )
                    .replace(
                        "__VOCAL_FONT__",
                        &serde_json::to_string(&vocal_font).unwrap_or_default(),
                    );
                spawn(async move {
                    if let Err(error) = document::eval(&script).await {
                        log::error!("could not render ABC notation: {error:?}");
                    }
                });
            },
        }
    }
}

#[component]
fn EmptySlideComponent() -> Element {
    rsx! {
        div { class: "empty-content" }
    }
}

/// Determines the container style for a slide based on its content type.
/// Picture and markdown slides need `height: 100%` to fill the grid cell,
/// so that their content can scroll or scale within a constrained area.
fn slide_container_style(slide_content: &SlideContent) -> &'static str {
    match slide_content {
        // A video is fitted into the cell like a picture, so it needs one with
        // a height: `height: 100%` against a parent that has none is zero.
        SlideContent::SimplePicture(_) | SlideContent::Video(_) => "height: 100%;",
        // A markdown slide scrolls, so it needs the whole cell to scroll
        // inside; the same slide holding plain lyrics is laid out by the
        // design and must not be stretched.
        SlideContent::SingleLanguageMainContent(main_slide)
            if get_markdown_html(&main_slide.clone().main_text()).is_some() =>
        {
            "height: 100%;"
        }
        _ => "",
    }
}

/// Renders the content of a single slide based on its [SlideContent] type.
/// Shared between [PresentationRendererComponent] and [StaticSlideRendererComponent]
/// to avoid duplicating the slide content matching logic.
/// The meta information line a slide carries, if any.
///
/// Which slides get one is decided by the song library from
/// `ShowMetaInformation`; the renderer only has to put it on screen.
/// `SingleLanguageMainContentSlide::meta_text` is private in the song library
/// and has no accessor, so it is read through serde — the same workaround this
/// module already uses for `SimplePictureSlide::picture_path`.
pub(crate) fn meta_text_of(slide_content: &SlideContent) -> Option<String> {
    let text = match slide_content {
        SlideContent::Title(title) => title.meta_text.clone(),
        SlideContent::MultiLanguageMainContent(multi) => multi.meta_text.clone(),
        SlideContent::Complex(complex) => complex.meta_text.clone(),
        SlideContent::SingleLanguageMainContent(_) => {
            serde_json::to_value(slide_content)
                .ok()
                .and_then(|value| {
                    value
                        .as_object()
                        .and_then(|map| map.values().next())
                        .and_then(|inner| inner.get("meta_text"))
                        .and_then(|meta| meta.as_str())
                        .map(String::from)
                })
        }
        // None of these carry text of their own to say anything about.
        SlideContent::Empty(_)
        | SlideContent::SimplePicture(_)
        | SlideContent::PdfPage(_)
        | SlideContent::Video(_) => None,
    };
    text.filter(|text| !text.trim().is_empty())
}

/// Declares the `@font-face` rules for the fonts bundled in `assets/fonts/`.
///
/// Needed in every window that draws text with a bundled family — the app shell
/// and each presentation window are separate documents, so the rules cannot be
/// inherited.
#[component]
pub fn BundledFontFaces() -> Element {
    // The fonts that came with an imported design are declared alongside the
    // bundled ones: to everything that draws, both are simply families that
    // exist. See [`crate::logic::fonts`].
    let css = use_hook(|| {
        format!(
            "{}{}",
            crate::logic::fonts::bundled_font_face_css(),
            crate::logic::fonts::imported_font_face_css()
        )
    });

    if css.is_empty() {
        return rsx! {};
    }

    rsx! {
        document::Style { {css} }
    }
}

/// The meta information line in the corner of a content slide.
///
/// Rendered as an overlay rather than inside the slide, because the slide
/// container is sized to its content — anything positioned inside it would
/// land on top of the last line of lyrics instead of at the bottom of the
/// screen.
#[component]
fn MetaTextCorner(text: String, meta_font: FontRepresentation) -> Element {
    let style = {
        let mut css = CssHandler::new();
        css.set_important(true);
        // The design's meta font already carries the intended colour.
        css.opacity(1.0);
        css.extend(&CssHandler::from(meta_font));
        css.to_string()
    };

    rsx! {
        div { class: "slide-meta-corner", style: "{style}", "{text}" }
    }
}

#[component]
fn SlideContentRenderer(
    slide_content: SlideContent,
    pds: PresentationDesignTemplate,
    running_presentation: Option<Signal<RunningPresentation>>,
    /// The effect a PDF page should arrive with. Every other slide gets its
    /// effect from its container being created; a PDF page's canvas is kept
    /// between pages, so it has to be told.
    #[props(default)]
    transition: String,
) -> Element {
    let meta_text = meta_text_of(&slide_content);
    let meta_font = pds.get_default_meta_font();

    // The title slide shows the meta information right below the headline, so
    // it reads as part of the title. Every other slide keeps it out of the way
    // in the bottom corner.
    let is_title_slide = matches!(slide_content, SlideContent::Title(_));

    rsx! {
        {slide_body(slide_content, pds, running_presentation, transition)}
        if let Some(text) = meta_text {
            if !is_title_slide {
                MetaTextCorner { text, meta_font }
            }
        }
    }
}

/// The slide itself, without the meta information line.
fn slide_body(
    slide_content: SlideContent,
    pds: PresentationDesignTemplate,
    running_presentation: Option<Signal<RunningPresentation>>,
    transition: String,
) -> Element {
    match slide_content {
        SlideContent::Title(title_slide) => rsx! {
            TitleSlideComponent {
                title_slide: title_slide.clone(),
                title_font_representation: pds.get_default_headline_font(),
                meta_font: pds.get_default_meta_font(),
                bold: pds.title_bold,
                meta_distance: pds.main_content_spoiler_content_padding.clone(),
            }
        },
        SlideContent::SingleLanguageMainContent(main_slide) => {
            let text = main_slide.clone().main_text();
            if let Some(html) = get_markdown_html(&text) {
                let html_owned = html.to_string();
                rsx! {
                    MarkdownSlideComponent {
                        html_content: html_owned,
                        running_presentation,
                        main_content_font: pds.get_default_font(),
                    }
                }
            } else {
                rsx! {
                    SingleLanguageMainContentSlideRenderer {
                        main_slide: main_slide.clone(),
                        main_content_font: pds.get_default_font(),
                        spoiler_content_font: pds.get_default_spoiler_font(),
                        distance: pds.main_content_spoiler_content_padding.clone(),
                    }
                }
            }
        },
        SlideContent::MultiLanguageMainContent(multi_slide) => rsx! {
            MultiLanguageMainContentSlideRenderer {
                multi_slide: multi_slide.clone(),
                main_content_font: pds.get_default_font(),
                spoiler_content_font: pds.get_default_spoiler_font(),
                distance: pds.main_content_spoiler_content_padding.clone(),
            }
        },
        SlideContent::Complex(complex_slide) => rsx! {
            ComplexSlideRenderer {
                complex_slide: complex_slide.clone(),
                pds: pds.clone(),
            }
        },
        SlideContent::Empty(_) => rsx! {
            EmptySlideComponent {}
        },
        SlideContent::SimplePicture(picture_slide) => rsx! {
            SimplePictureSlideComponent { picture_slide: picture_slide.clone(), transition }
        },
        SlideContent::PdfPage(pdf_slide) => rsx! {
            PdfPageCanvas {
                pdf_path: pdf_slide.pdf_path.clone(),
                page_num: pdf_slide.page_number,
            }
        },
        SlideContent::Video(video_slide) => rsx! {
            VideoSlideComponent { video_slide: video_slide.clone(), running_presentation }
        },
    }
}

/// Marks a rendering whose video is a *picture of* the one that is playing.
///
/// Handed down through the context rather than as a prop for the same reason
/// [`PresentationRole`] is: the thing that asks sits several layers below the
/// view that knows the answer, and everything in between has no business
/// carrying it. Provided by the console's overview around the thumbnail of the
/// slide that is up; read in [`VideoSlideComponent`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MirrorsTheVideo;

/// Plays the video of a video slide.
///
/// The playing is the web view's own: a `<video>` element, fed by the handler
/// in [`crate::logic::video`]. Cantara decodes nothing and draws no frames
/// itself — the engine the rest of the program is drawn by already does that,
/// with the machine's video hardware behind it.
///
/// Which window makes the sound is not a property of the slide but of the
/// machine: a presenter console and a projection window are two pages playing
/// the same file, and both unmuted is the same audio twice, a few dozen
/// milliseconds apart. One of them owns it and the rest are muted; the rule is
/// in [`crate::logic::video`].
#[component]
fn VideoSlideComponent(
    video_slide: VideoSlide,
    /// Where the playback of this video stands, shared with every other window
    /// showing the presentation. `None` for a preview that belongs to no
    /// running service — the design editor's sample slide.
    running_presentation: Option<Signal<RunningPresentation>>,
) -> Element {
    use crate::logic::video::{AudioOwner, audio_generation, claim_audio, owns_audio};

    let source = crate::logic::video::video_source_url(&video_slide.video_path);
    let mime = crate::logic::sourcefiles::mime_type_of_video(&video_slide.video_path);

    // What this window is. Anything that did not say — the design selector's
    // preview, a thumbnail — is a follower and never makes a sound.
    let role: PresentationRole = try_consume_context().unwrap_or(PresentationRole::Follower);

    // The projection asks for the sound each time it draws a video. It does not
    // get it while a console is open; see [`crate::logic::video::claim_audio`],
    // where that rule lives.
    let mut audio_changed: Signal<u64> = use_signal(audio_generation);
    use_effect(move || {
        if role == PresentationRole::Audience {
            claim_audio(AudioOwner::Projection);
        }
        // A console opening or closing changes the answer under a window that
        // is already showing the video, so it is looked for rather than
        // assumed. See [`crate::logic::timer`] for why the wait is not a script.
        spawn(async move {
            loop {
                crate::logic::timer::sleep(std::time::Duration::from_millis(250)).await;
                let generation = audio_generation();
                if generation != *audio_changed.peek() {
                    audio_changed.set(generation);
                }
            }
        });
    });
    // Read while rendering: that is what subscribes this element to it.
    let _ = audio_changed();

    let makes_the_sound = match role {
        PresentationRole::Audience => owns_audio(AudioOwner::Projection),
        PresentationRole::Follower => owns_audio(AudioOwner::Console),
        // A preview running by itself is an illustration of a setting, not the
        // service. It is never the thing the room hears.
        PresentationRole::SelfRunning => false,
    };

    // Whether this is the slide the service is on, or a picture of one.
    //
    // The overview draws every slide of the whole running order at once, each
    // through `StaticSlideRendererComponent`, which has no running presentation
    // to hand down — and that is the difference. Without it every video in the
    // service began playing the moment the overview was opened, twenty of them
    // at once, none of them the slide anybody was looking at.
    let is_live = running_presentation.is_some();

    // …and whether this picture of one is meant to move.
    //
    // The thumbnail of the slide that is up, in the console's overview: a
    // still of a video that is playing says nothing about what the room is
    // watching, so that one follows the video instead of showing its first
    // frame. Every other thumbnail is still a still — twenty decoders for
    // slides nobody is looking at is what the rule above is there to prevent.
    //
    // It is not the live element: it takes no commands, makes no sound, and is
    // not what the console reports the position from. It is pulled onto the
    // published position a few times a second, exactly as a following *window*
    // is. See [`crate::logic::video::mirror_script`].
    let mirrors = !is_live && try_consume_context::<MirrorsTheVideo>().is_some();

    if mirrors {
        use_effect(move || {
            spawn(async move {
                loop {
                    crate::logic::timer::sleep(std::time::Duration::from_millis(250)).await;
                    // Nobody is playing anything, so there is nothing to
                    // follow — the thumbnail keeps the frame it is on.
                    let Some((position, _, playing)) =
                        crate::logic::video::published_position()
                    else {
                        continue;
                    };
                    let script = crate::logic::video::mirror_script(position, playing);
                    let _ = document::eval(&script).await;
                }
            });
        });
    }

    // The slide says how the video starts; from then on the running
    // presentation says what it is doing, and every window follows the same
    // value. See [`crate::logic::states::VideoPlayback`].
    if let Some(mut running_presentation) = running_presentation {
        let autostart = video_slide.autostart;

        // Opening the slide is what starts the video — not the page being
        // built. The component is created when the presentation reaches this
        // slide and dropped when it leaves, so those are the two moments.
        use_hook(move || {
            spawn(async move {
                let mut state = running_presentation.write();
                state.video.playing = autostart;
                state.video.position = 0.0;
                state.video.duration = 0.0;
                // A jump belonging to the video that was up before this one
                // must not be carried out on this one.
                state.video.seek_to = None;
            });
        });

        // Leaving the slide stops it. A video that went on playing behind the
        // next slide would still be heard in the room, and would still be
        // holding the file open.
        use_drop(move || {
            if let Ok(mut state) = running_presentation.try_write() {
                state.video.playing = false;
                state.video.position = 0.0;
            }
            // …and the machine forgets where it was, so the next video does not
            // start measured against this one's clock.
            crate::logic::video::forget_position();
        });

        // What has been *asked* of the video, as against where it has got to.
        //
        // A memo over those four fields on purpose. Reading the running
        // presentation whole would subscribe this to the position as well, and
        // the position is written several times a second — so the script below
        // would be built and run at that rate, assigning volume and calling
        // `play()` on an element that was already doing both.
        let commands = use_memo(move || {
            let playback = &running_presentation.read().video;
            (
                playback.playing,
                playback.muted,
                playback.volume,
                playback.seek_to,
            )
        });

        // The last jump this window has carried out. A seek is a command that
        // happens *once*: the value stays in the running presentation after it
        // has been obeyed, and re-applying it would pull the video back to the
        // mark every time it played half a second past it — which is a video
        // that will not leave the spot it was sent to.
        let mut applied_seek: Signal<u64> = use_signal(|| 0);

        // Bringing the element into line with what has been asked of it.
        use_effect(move || {
            let (playing, muted, volume, seek_to) = commands();

            let seek = match seek_to {
                Some((seconds, serial)) if serial > *applied_seek.peek() => {
                    applied_seek.set(serial);
                    Some(seconds)
                }
                _ => None,
            };

            let script = crate::logic::video::control_script(
                playing,
                // A window that is not the one making the sound is muted
                // whatever the operator set, so that only one of them is heard.
                muted || !makes_the_sound,
                volume,
                seek,
            );
            spawn(async move {
                let _ = document::eval(&script).await;
            });
        });

        // A window that is not the one playing follows the one that is.
        //
        // Two `<video>` elements started at the same moment do not stay
        // together: two decoders on two clocks, one of them a scaled-down
        // preview. Left alone they are seconds apart within a few minutes, and
        // the console showing a different moment than the wall is the one thing
        // the console must not do.
        use_effect(move || {
            if makes_the_sound {
                return;
            }
            spawn(async move {
                loop {
                    crate::logic::timer::sleep(std::time::Duration::from_millis(500)).await;
                    let Some((published, _, _)) = crate::logic::video::published_position()
                    else {
                        // Nobody is playing, so there is nothing to follow.
                        continue;
                    };
                    let Ok(value) =
                        document::eval(&crate::logic::video::report_script()).await
                    else {
                        continue;
                    };
                    let Some(report) = value.as_array() else {
                        // The slide moved on; this window has no video left.
                        return;
                    };
                    let own = report.first().and_then(|v| v.as_f64()).unwrap_or(0.0);

                    if crate::logic::video::should_correct(own, published) {
                        let script = crate::logic::video::seek_script(published);
                        let _ = document::eval(&script).await;
                    }
                }
            });
        });

        // …and asking it back where it has got to. Only the window that owns
        // the sound reports: it is the one actually playing, and two windows
        // writing the position would fight over it several times a second.
        use_effect(move || {
            if !makes_the_sound {
                return;
            }
            spawn(async move {
                loop {
                    crate::logic::timer::sleep(std::time::Duration::from_millis(250)).await;
                    let Ok(value) =
                        document::eval(&crate::logic::video::report_script()).await
                    else {
                        continue;
                    };
                    let Some(report) = value.as_array() else {
                        // No video on screen any more: the slide moved on. The
                        // published position goes with it, so the next video
                        // does not start against this one's clock.
                        crate::logic::video::forget_position();
                        return;
                    };
                    let position = report.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let duration = report.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let playing = report.get(2).and_then(|v| v.as_bool()).unwrap_or(false);

                    // For the other windows, which follow this one. Kept out
                    // of the running presentation on purpose — see
                    // [`crate::logic::video::publish_position`].
                    crate::logic::video::publish_position(position, duration, playing);

                    let mut state = running_presentation.write();
                    state.video.position = position;
                    state.video.duration = duration;
                    // A video that ran to its end has stopped by itself, and
                    // the button in the console has to say so rather than
                    // offering to pause something that is not moving.
                    //
                    // Only that case: mirroring the element's paused state in
                    // general would undo a play the operator has just asked
                    // for, in the quarter second before the element gets round
                    // to starting.
                    let ran_out =
                        state.video.playing && !playing && duration > 0.0
                            && position >= duration - 0.25;
                    if ran_out {
                        state.video.playing = false;
                    }
                }
            });
        });
    }

    rsx! {
        video {
            // The live one is marked apart from the thumbnails: the scripts
            // that play, pause and seek reach the element by selector, and
            // the overview puts a `.slide-video` on screen for every slide of
            // the service. Without this they would command whichever of those
            // happened to come first in the document. The mirror is marked
            // apart from both — it is told where to be rather than asked to
            // go anywhere.
            class: if is_live {
                "slide-video slide-video-live"
            } else if mirrors {
                "slide-video slide-video-mirror"
            } else {
                "slide-video"
            },
            // Only where this is the slide the service is on. In the overview
            // it is a picture of a slide, and a picture does not play — the
            // one that moves is started by the script that keeps it level with
            // the video it is a picture of.
            autoplay: is_live && video_slide.autostart,
            r#loop: is_live && video_slide.looping,
            // One window per machine makes the sound, and the rest are silent —
            // two playing the same file tens of milliseconds apart is a
            // flanging echo rather than a louder video. A mirror is silent
            // whatever else is true: it is in the same window as the one that
            // *is* making the sound.
            muted: mirrors || !makes_the_sound,
            // No browser chrome. The presentation is a projection, and a
            // playback bar across the bottom of it belongs to the operator's
            // screen rather than to the room's.
            controls: false,
            playsinline: true,
            // A thumbnail wants the first frame and nothing more; the slide
            // that is up wants to be ready to play, and so does the one
            // thumbnail that is following it. Twenty thumbnails each fetching
            // a whole film is what the overview would otherwise cost.
            preload: if is_live || mirrors { "auto" } else { "metadata" },
            source { src: "{source}", r#type: "{mime}" }
        }
    }
}

/// Generates a CSS string from a [FontRepresentation] with `!important` flags,
/// for use as inline style on markdown slide containers.
fn markdown_font_css(font: FontRepresentation) -> String {
    let mut css = CssHandler::new();
    css.set_important(true);
    css.extend(&CssHandler::from(font));
    css.to_string()
}

/// This helper function injects a CSS style into all HTML tags of a string. That is needed
/// to override default CSS definitions coming from PicoCSS.
fn inject_css_into_html_elements(html: &str, css_style: &CssHandler) -> String {
    // Regex breakdown:
    // <([a-z1-6]+)  -> Matches the opening '<' and captures the tag name
    // (?![^>]*style=) -> A negative lookahead to ensure we don't double-up if a style already exists
    // [^>]* -> Matches any other attributes until the closing '>'
    // >             -> Matches the closing bracket
    let re = match Regex::new(r"(?i)<([a-z1-6]+)([^>]*)>") {
        Ok(re) => re,
        Err(_) => return html.to_string(),
    };

    // We use a replacement closure to handle the logic
    re.replace_all(html, |caps: &regex::Captures| {
        let tag = &caps[1];
        let attributes = &caps[2];
        let css_style_string = css_style.to_string();

        // List of common elements that don't support/need styling (void tags or metadata)
        let ignored_tags = ["html", "head", "meta", "link", "script", "style", "br", "hr"];

        if ignored_tags.contains(&tag.to_lowercase().as_str()) {
            format!("<{tag}{attributes}>")
        } else {
            // Check if style already exists to append, or just insert new
            if attributes.contains("style=") {
                // This is a simple version; real attribute parsing is complex!
                format!("<{tag}{attributes} style=\"{css_style_string}\">")
            } else {
                format!("<{tag} style=\"{css_style_string}\"{attributes}>")
            }
        }
    }).to_string()
}

/// A component for rendering a Markdown slide with scrollable content.
///
/// The HTML content (already converted from Markdown) is displayed inside a scrollable
/// container. Font colors are injected into all HTML elements via inline CSS to override
/// PicoCSS defaults.
///
/// ## Scroll synchronization
///
/// When `running_presentation` is `Some`, a bidirectional scroll sync polling loop runs
/// to keep the scroll position consistent between the presentation window and the
/// presenter console preview. The mechanism works as follows:
///
/// - Both windows (presentation and presenter console) share the same
///   `Signal<Vec<RunningPresentation>>` context handle. Writes from one window are
///   immediately visible to `.peek()` in the other.
/// - Every 50ms, the loop reads the DOM `scrollTop` of the `.markdown-slide` element
///   and compares it against the last known position:
///   - **Local scroll detected** (DOM changed): the new position is written to the
///     shared signal's `markdown_scroll_position` field, so the other window picks it up.
///   - **Remote scroll detected** (signal changed): the DOM `scrollTop` is updated via
///     JavaScript to match the signal value.
/// - A threshold of 2px prevents feedback loops between the two directions.
/// - DOM values are read via `document::eval` with an explicit `return` inside an IIFE,
///   which is required by Dioxus 0.7's desktop eval to propagate return values to Rust.
///
/// When `running_presentation` is `None` (used in static grid thumbnails), the polling
/// loop exits immediately and no synchronization takes place.
#[component]
fn MarkdownSlideComponent(
    html_content: String,
    running_presentation: Option<Signal<RunningPresentation>>,
    main_content_font: FontRepresentation,
) -> Element {
    /// Minimum pixel difference to trigger a scroll position sync update
    const SCROLL_SYNC_THRESHOLD: f64 = 2.0;
    /// Polling interval in milliseconds
    const POLL_MS: u32 = 50;

    // Access the shared context signal directly — both windows (presentation
    // and presenter console) share the exact same Signal handle, so writes
    // from one window are immediately visible to .peek() in the other.
    let mut shared: Signal<Vec<RunningPresentation>> = use_context();

    let font_css = markdown_font_css(main_content_font.clone());

    let mut html_content_css = CssHandler::new();
    html_content_css.set_important(true);
    html_content_css.color(main_content_font.color);

    let html_content = inject_css_into_html_elements(&html_content, &html_content_css);

    // Bidirectional scroll sync polling loop. Runs only when running_presentation
    // is Some (i.e. in the interactive presentation/preview, not in static thumbnails).
    // Reads/writes the shared context signal directly, bypassing local signal chains,
    // because reactive use_effect subscriptions don't reliably wake other windows'
    // event loops in Dioxus desktop (each window runs a separate VirtualDom).
    //
    // The loop captures the slide position at mount time and exits immediately if the
    // position changes (i.e. slide change). This ensures scroll sync never interferes
    // with slide navigation — slide changes always take priority.
    use_future(move || async move {
        // No sync needed for static thumbnails
        if running_presentation.is_none() { return; }

        // Capture the slide position when this component was mounted.
        // If the position changes, we must stop immediately — the component will
        // be unmounted/recreated for the new slide anyway.
        let initial_position = shared.peek()
            .first()
            .and_then(|rp| rp.position.clone());

        let mut last_pos: f64 = 0.0;
        loop {
            // Sleep using a JS-level await to keep the WebView event loop alive.
            // A Rust-side sleep (tokio/async_std) would not pump the WebView.
            let js_sleep = format!("await new Promise(r => setTimeout(r, {POLL_MS}))");
            let _ = document::eval(&js_sleep).await;

            // Check if the slide position changed — if so, stop this loop immediately.
            // Slide changes must never be interfered with by scroll sync writes.
            let current_position = shared.peek()
                .first()
                .and_then(|rp| rp.position.clone());
            if current_position != initial_position {
                break;
            }

            // Read the current DOM scroll position via JS eval.
            // Note: Dioxus 0.7 desktop eval requires an explicit `return` inside an
            // IIFE to propagate values back to Rust — a bare expression returns null.
            let dom_pos = {
                let js = r#"
                    return (function() {
                        var el = document.querySelector('.markdown-slide');
                        return el ? el.scrollTop : -1;
                    })();
                "#;
                document::eval(js).await.ok()
                    .and_then(|val| val.as_f64())
                    .unwrap_or(-1.0)
            };
            // Element not yet in the DOM (e.g. during initial render); retry next tick
            if dom_pos < 0.0 {
                continue;
            }

            // Read the shared signal without subscribing (peek avoids triggering re-renders)
            let signal_pos = shared.peek()
                .first()
                .map(|rp| rp.markdown_scroll_position)
                .unwrap_or(0.0);

            if (dom_pos - last_pos).abs() > SCROLL_SYNC_THRESHOLD {
                // Local user scrolled — push the new position to the shared signal
                // so the other window (presenter console or presentation) picks it up
                last_pos = dom_pos;
                if (signal_pos - dom_pos).abs() > SCROLL_SYNC_THRESHOLD
                    && let Some(first) = shared.write().first_mut() {
                        first.markdown_scroll_position = dom_pos;
                    }
            } else if (signal_pos - last_pos).abs() > SCROLL_SYNC_THRESHOLD {
                // Remote scroll detected (the other window updated the signal) —
                // apply the new scroll position to this window's DOM
                last_pos = signal_pos;
                let js = format!(
                    r#"
                    var el = document.querySelector('.markdown-slide');
                    if (el) {{ el.scrollTop = {}; }}
                    "#,
                    signal_pos
                );
                let _ = document::eval(&js).await;
            }
        }
    });

    rsx! {
        div {
            class: "markdown-slide",
            style: format!(
                "overflow-y: auto; max-height: 100%; padding: 1em 2em; box-sizing: border-box; {}",
                font_css,
            )
                .to_string(),
            dangerous_inner_html: html_content,
        }
    }
}

#[component]
fn SimplePictureSlideComponent(
    picture_slide: SimplePictureSlide,
    #[props(default)]
    transition: String,
) -> Element {
    let path = get_picture_path(&picture_slide);

    // Check if this is a PDF; the path may contain a #page=N fragment
    let base_path = path.split('#').next().unwrap_or(&path).to_string();
    let is_pdf = base_path.to_lowercase().ends_with(".pdf");

    if is_pdf {
        let page_num: u32 = path
            .split("#page=")
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        return rsx! {
            div { style: "width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; z-index: 2;",
                PdfPageCanvas { pdf_path: base_path, page_num, transition }
            }
        };
    }

    // The picture goes to the page inline. A file system path in `src` is not
    // something a web view can fetch, which is why every picture slide showed
    // the broken-image placeholder — see [`crate::logic::images`].
    let Some(source) = crate::logic::images::image_data_url_str(&path) else {
        log::warn!("could not read the picture {path}");
        return rsx! {
            div { style: "width: 100%; height: 100%; z-index: 2;" }
        };
    };

    rsx! {
        div { style: "width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; z-index: 2;",
            img {
                src: "{source}",
                // As large as the slide allows, never distorted. `max-width`
                // alone only ever shrinks, so a picture smaller than the slide
                // was left sitting in the middle at its own size instead of
                // filling the screen.
                style: "width: 100%; height: 100%; object-fit: contain;",
            }
        }
    }
}

/// Which document a slide shows a page of, if it shows one.
///
/// A PDF is the one kind of slide whose element is kept from one page to the
/// next: the canvas holds the page that is up until the next has been drawn,
/// so the identity of the element on the stage is the *document* rather than
/// the slide. Everything else — text, and a picture, which is inlined into the
/// page and arrives with it — is there as soon as it is drawn and gets an
/// element per slide.
fn pdf_document_of(content: &SlideContent) -> Option<String> {
    match content {
        SlideContent::PdfPage(page) => Some(page.pdf_path.clone()),
        SlideContent::SimplePicture(picture) => {
            crate::logic::pdf::pdf_page_of(&get_picture_path(picture))
                .map(|(document, _)| document)
        }
        _ => None,
    }
}

/// A page of a PDF, drawn by the viewer that lives in the page.
///
/// The element is deliberately kept between the pages of one document: what is
/// on it stays there until the next page has been drawn beside it and copied
/// across, so a slide change never shows an empty screen. That is also why the
/// effect the slide arrives with is started from `pdf_viewer.js` rather than by
/// a CSS class on a newly created element — there is no new element.
///
/// Everything about the rendering itself is in [`crate::logic::pdf`].
#[component]
pub(crate) fn PdfPageCanvas(
    pdf_path: String,
    page_num: u32,
    /// The CSS class of the configured effect, started when the page appears.
    /// Nothing chosen means nothing happens.
    #[props(default)]
    transition: String,
) -> Element {
    // One identity per mount. The element outlives a slide, so this must not
    // be derived from the page.
    let mount_id = use_hook(Uuid::new_v4);
    let canvas_id = format!("pdf-canvas-{}", mount_id.as_simple());

    // Drawn when the canvas appears *and* again whenever the page changes.
    // `onmounted` fires only when the element is created, and this element is
    // reused from one page to the next.
    {
        let canvas_id = canvas_id.clone();
        let pdf_path = pdf_path.clone();
        let transition = transition.clone();
        use_effect(use_reactive!(|(pdf_path, page_num, transition)| {
            let canvas_id = canvas_id.clone();
            let pdf_path = pdf_path.clone();
            let transition = transition.clone();
            spawn(async move {
                crate::logic::pdf::show(&canvas_id, &pdf_path, page_num, &transition).await;
            });
        }));
    }

    rsx! {
        canvas {
            id: "{canvas_id}",
            // Not shown until a page has been drawn onto it, so an empty
            // canvas is never part of the picture.
            style: "display: block; max-width: 100%; max-height: 100%; visibility: hidden;",
        }
    }
}

/// A PDF as a document to read: every page under the last, in a column that
/// scrolls.
///
/// The pages are drawn as they come near the viewport rather than all at once
/// — a scanned score runs to hundreds of them — and the mechanics of that are
/// in `cantaraPdf.setupScroll`.
#[component]
pub(crate) fn PdfScrollView(pdf_path: String, pages: u32) -> Element {
    let mount_id = use_hook(Uuid::new_v4);
    let container_id = format!("pdf-scroll-{}", mount_id.as_simple());

    // Built when the view appears and again whenever the document changes.
    // `onmounted` cannot do the second part: opening another PDF gives this
    // component new props but the same container element, so the node is
    // updated rather than mounted and the view would go on showing whichever
    // document was opened first.
    {
        let container_id = container_id.clone();
        let pdf_path = pdf_path.clone();
        use_effect(use_reactive!(|(pdf_path, pages)| {
            // The page count is a dependency because the canvases for a longer
            // document have to exist before the view is built over them.
            let _ = pages;
            let container_id = container_id.clone();
            let pdf_path = pdf_path.clone();
            spawn(async move {
                crate::logic::pdf::setup_scroll(&container_id, &pdf_path).await;
            });
        }));
    }

    rsx! {
        div {
            id: "{container_id}",
            class: "pdf-scroll",
            for page in 1..=pages.max(1) {
                canvas {
                    // The document is part of the key, so the canvases of one
                    // PDF are never handed on to the next: a reused element
                    // still carries the page drawn into it.
                    key: "{pdf_path}-{page}",
                    class: "pdf-scroll-page",
                    "data-page": "{page}",
                }
            }
        }
    }
}


/// The inline copy of a picture, at the size a preview needs it.
///
/// For the views that can live without it for a moment — a thumbnail, the
/// design preview. Reading and encoding a background photograph while
/// rendering blocks the window, and a design with a large picture took seconds
/// to open because of it. Here nothing is read on the render: the picture is
/// asked for, `None` is drawn until it lands, and the view is redrawn when it
/// does. See [`crate::logic::images`].
///
/// The scaled-down copy is preferred over the full one. A design's background
/// is whatever photograph the user picked, and those come off a camera —
/// several thousand pixels across. On the wall that is the point; in a preview
/// four hundred pixels wide it is not, and it is not free either, because the
/// picture is not merely shown small but **rescaled on every paint**. Scrolling
/// a settings page past one asks for that sixty times a second, and where the
/// engine does not keep the scaled copy, the scroll stalls and then jumps to
/// catch up.
fn use_inlined_picture(path: Option<PathBuf>) -> Option<String> {
    // The path arrives as a prop, which is not something an effect can watch,
    // so it is mirrored into a signal — as [`PresentationViewer`] does with
    // its presentation.
    let mut wanted: Signal<Option<PathBuf>> = use_signal(|| path.clone());
    if *wanted.peek() != path {
        wanted.set(path.clone());
    }

    // Counts up as pictures arrive. Read below, while rendering, because that
    // is what subscribes this view to it.
    let mut images_ready: Signal<u64> = use_signal(crate::logic::images::image_generation);
    let mut thumbnails_ready: Signal<u64> = use_signal(crate::logic::images::thumbnail_generation);

    // The small copy is the one that will be used; the full one is asked for
    // as well, so that something can be drawn while the scaling is still going
    // on and the preview is never blank.
    use_effect(move || {
        if let Some(path) = wanted() {
            crate::logic::images::prepare_thumbnails(vec![path]);
        }

        spawn(async move {
            loop {
                let generation = crate::logic::images::thumbnail_generation();
                if generation != *thumbnails_ready.peek() {
                    thumbnails_ready.set(generation);
                }
                if !crate::logic::images::thumbnails_in_progress() {
                    return;
                }
                let _ = document::eval("await new Promise(r => setTimeout(r, 100))").await;
            }
        });
    });

    use_effect(move || {
        let Some(path) = wanted() else {
            return;
        };
        if crate::logic::images::cached_image_data_url(&path).is_some() {
            return;
        }
        crate::logic::images::prepare_image_data_urls(vec![path]);

        // The encoding happens on a thread, which cannot write to a signal, so
        // the view looks for the result instead and stops as soon as it is in.
        spawn(async move {
            loop {
                let generation = crate::logic::images::image_generation();
                if generation != *images_ready.peek() {
                    images_ready.set(generation);
                }
                if !crate::logic::images::images_in_progress() {
                    return;
                }
                let _ = document::eval("await new Promise(r => setTimeout(r, 100))").await;
            }
        });
    });

    let _ = images_ready();
    let _ = thumbnails_ready();

    path.and_then(|path| {
        crate::logic::images::thumbnail(&path)
            .or_else(|| crate::logic::images::cached_image_data_url(&path))
    })
}

/// A static (non-interactive) slide renderer that renders a single slide with its
/// presentation design. Used for grid overview thumbnails. It reuses the same
/// sub-components as `PresentationRendererComponent` but without any interactivity
/// (no click/keyboard handlers, no black screen overlay, no fade-in animation).
#[component]
pub fn StaticSlideRendererComponent(
    slide: Slide,
    presentation_design: PresentationDesign,
) -> Element {
    let pds = match presentation_design.presentation_design_settings {
        PresentationDesignSettings::Template(ref template) => template.clone(),
        _ => PresentationDesignTemplate::default(),
    };

    // Asked for before anything is drawn, and drawn without it until it is
    // there — this view must never wait for a file.
    let background_source = use_inlined_picture(
        pds.background_image
            .as_ref()
            .map(|image| image.as_source().path.clone()),
    );

    let css_text_align = pds
        .fonts
        .first()
        .unwrap_or(&FontRepresentation::default())
        .horizontal_alignment;

    let css_place_items = match pds.vertical_alignment {
        VerticalAlign::Top => PlaceItems::StartStretch,
        VerticalAlign::Middle => PlaceItems::CenterStretch,
        VerticalAlign::Bottom => PlaceItems::EndStretch,
    };

    let css_handler = {
        let mut css = CssHandler::new();
        css.set_important(true);
        css.background_color(pds.background_color);
        css.padding_left(pds.padding.left.clone());
        css.padding_right(pds.padding.right.clone());
        css.padding_top(pds.padding.top.clone());
        css.padding_bottom(pds.padding.bottom.clone());
        css.text_align(css_text_align);
        css.set_important(true);
        css.color(
            pds.fonts
                .first()
                .unwrap_or(&FontRepresentation::default())
                .color,
        );
        css.place_items(css_place_items);
        css
    };

    let background_css = {
        let mut css = CssHandler::new();
        css.set_important(true);
        if let Some(source) = background_source.as_ref() {
            css.background_image(source);
            css.background_size("cover");
            css.background_position("center");
            css.background_repeat("no-repeat");
            css.opacity(1.0 - pds.background_transparency as f32 / 100.0f32);
        } else {
            css.background_image_none();
            css.opacity(0.0);
        }
        css.to_string()
    };

    let slide_content = slide.slide_content;
    let container_style = slide_container_style(&slide_content);

    rsx! {
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
        document::Script { src: PRESENTATION_JS }
        div { class: "presentation", style: css_handler.to_string(),
            div { class: "background", style: "{background_css}" }
            div { class: "slide-container", style: "{container_style}",
                SlideContentRenderer { slide_content, pds }
            }
        }
    }
}

#[cfg(test)]
mod role_tests {
    use super::*;

    /// Exactly one view is the screen the audience is looking at, and it is
    /// the one a caller gets without asking — the presentation window renders
    /// the component without naming a role.
    #[test]
    fn the_audience_view_is_the_one_nobody_has_to_ask_for() {
        assert_eq!(PresentationRole::default(), PresentationRole::Audience);
        assert!(PresentationRole::default().is_audience_view());
        assert!(!PresentationRole::Follower.is_audience_view());
        assert!(!PresentationRole::SelfRunning.is_audience_view());
    }

    /// A slide must not be advanced once per window showing it, so only the
    /// audience's screen and a preview that runs on its own fire the timer.
    #[test]
    fn only_a_view_that_runs_by_itself_advances_the_slides() {
        assert!(PresentationRole::Audience.fires_timer());
        assert!(PresentationRole::SelfRunning.fires_timer());
        assert!(!PresentationRole::Follower.fires_timer());
    }

    /// The reason this is a role rather than a flag: firing the timer and
    /// being the audience's screen are not the same question. The options
    /// panel's example of a slide timer has to advance to show what was
    /// configured, and would have hidden its scrollbar and published its
    /// layout size had the two been one boolean.
    #[test]
    fn running_by_itself_does_not_make_a_preview_the_audience_view() {
        assert!(PresentationRole::SelfRunning.fires_timer());
        assert!(!PresentationRole::SelfRunning.is_audience_view());
    }
}

#[cfg(test)]
mod stage_tests {
    use super::*;

    /// A picture slide, built the way [`get_picture_path`] reads one — the
    /// song library keeps the path private.
    fn picture(path: &str) -> SlideContent {
        SlideContent::SimplePicture(
            serde_json::from_value(serde_json::json!({ "picture_path": path }))
                .expect("a picture slide is its path"),
        )
    }

    /// Every page of one PDF shares an element, so stepping through it keeps
    /// the page that is up until the next has been drawn. What identifies that
    /// element is therefore the document, and it must not change with the
    /// page.
    #[test]
    fn every_page_of_one_pdf_belongs_to_the_same_element() {
        let first = Slide::new_pdf_page_slide("handout.pdf".to_string(), 1);
        let second = Slide::new_pdf_page_slide("handout.pdf".to_string(), 7);

        assert_eq!(
            pdf_document_of(&first.slide_content),
            pdf_document_of(&second.slide_content)
        );
        assert_eq!(
            pdf_document_of(&first.slide_content).as_deref(),
            Some("handout.pdf")
        );
    }

    /// A PDF reached as a picture slide — which is how one added to a
    /// presentation arrives — has to be recognised the same way, or its pages
    /// would each get an element of their own and the screen would empty
    /// between them.
    #[test]
    fn a_pdf_reached_as_a_picture_is_recognised() {
        assert_eq!(
            pdf_document_of(&picture("handout.pdf#page=2")).as_deref(),
            Some("handout.pdf")
        );
        assert_eq!(
            pdf_document_of(&picture("handout.pdf#page=9")).as_deref(),
            Some("handout.pdf")
        );
    }

    /// Two documents must not share an element: the canvas holds the page last
    /// drawn into it, and one document's page has no business showing while
    /// another is being opened.
    #[test]
    fn two_documents_do_not_share_an_element() {
        assert_ne!(
            pdf_document_of(&picture("a.pdf#page=1")),
            pdf_document_of(&picture("b.pdf#page=1"))
        );
    }

    /// Everything else gets an element per slide, which is what starts the
    /// effect it was configured to arrive with.
    #[test]
    fn everything_else_gets_an_element_of_its_own() {
        let contents = [
            Slide::new_content_slide("Amazing grace".to_string(), None, None).slide_content,
            Slide::new_title_slide("Amazing Grace".to_string(), None).slide_content,
            Slide::new_empty_slide(false).slide_content,
            picture("background.png"),
        ];

        for content in contents {
            assert_eq!(pdf_document_of(&content), None);
        }
    }
}

#[cfg(test)]
mod notation_tests {
    use super::*;

    /// A neutral setting must leave the tune exactly as the song library wrote
    /// it — an ABC file is data, not a place to leave stray directives.
    #[test]
    fn test_normal_line_height_leaves_the_tune_alone() {
        let abc = "X:1\nK:D\nA2 B2 |\n";

        assert_eq!(with_staff_separation(abc, 1.0), abc);
    }

    /// The directive has to come first: abcjs only honours it ahead of the
    /// tune, and only from the source — the render option is ignored.
    #[test]
    fn test_the_directive_is_prefixed() {
        let abc = "X:1\nK:D\nA2 B2 |\n";

        let wide = with_staff_separation(abc, 2.0);

        assert!(wide.starts_with("%%staffsep 92\n"), "got: {wide:?}");
        assert!(wide.ends_with(abc));
    }

    /// Half the height means half the separation, measured against the value
    /// at which abcjs engraves as it does untouched.
    #[test]
    fn test_the_factor_scales_the_separation() {
        let abc = "X:1\n";

        assert!(with_staff_separation(abc, 0.5).starts_with("%%staffsep 23\n"));
        assert!(with_staff_separation(abc, 1.5).starts_with("%%staffsep 69\n"));
    }

    /// An absurd setting must not produce an unusable staff.
    #[test]
    fn test_extreme_values_are_clamped() {
        let abc = "X:1\n";

        assert!(with_staff_separation(abc, 100.0).starts_with("%%staffsep 230\n"));
        assert!(with_staff_separation(abc, -5.0).starts_with("%%staffsep 9\n"));
    }

    /// At full width the staff gets the same box as the text rows, so the two
    /// line up; anything narrower is placed by the alignment.
    #[test]
    fn test_notation_block_width_and_alignment() {
        let full = NotationSettings::default();
        assert!(notation_block_style(&full).contains("width: 100%"));

        let left = NotationSettings {
            width_percent: 60.0,
            horizontal_alignment: HorizontalAlign::Left,
            ..NotationSettings::default()
        };
        let style = notation_block_style(&left);
        assert!(style.contains("width: 60%"));
        assert!(style.contains("margin-left: 0"));

        let right = NotationSettings {
            horizontal_alignment: HorizontalAlign::Right,
            ..NotationSettings::default()
        };
        assert!(notation_block_style(&right).contains("margin-left: auto"));
    }

    /// A width outside the usable range must not make the staff vanish.
    #[test]
    fn test_notation_width_is_clamped() {
        let tiny = NotationSettings {
            width_percent: 0.0,
            ..NotationSettings::default()
        };
        assert!(notation_block_style(&tiny).contains("width: 10%"));

        let huge = NotationSettings {
            width_percent: 400.0,
            ..NotationSettings::default()
        };
        assert!(notation_block_style(&huge).contains("width: 100%"));
    }
}

/// Does a slide follow the design it is drawn with?
///
/// These render a slide twice — once with a design, once with the design after
/// an edit — and assert that what comes out is different. That is the one
/// question the rest of the suite cannot answer: everything else here checks
/// what a *function* returns, and the bug this guards against was that a
/// perfectly correct stylesheet was built once and then never again.
///
/// A `use_memo` closure is created on the first render and captures that
/// render's props. A memo over nothing but props therefore never recomputes,
/// however often the props change. Three slide renderers were written that
/// way, and the symptom was the design editor's live preview: switching a font
/// to italic left the title exactly as it was, while the meta line underneath
/// — built without a memo — followed at once.
#[cfg(test)]
mod design_follows_tests {
    use super::*;
    use crate::logic::settings::PresentationDesign;
    use cantara_songlib::slides::Slide;

    /// Renders a slide with `design`, without a window.
    ///
    /// The design goes in through a signal that is written *between* renders,
    /// which is what a settings page does — a component rendered once with two
    /// different sets of props would not reproduce the bug, since the fault is
    /// in what a hook remembers across renders.
    fn render_twice(
        slide: Slide,
        first: PresentationDesign,
        second: PresentationDesign,
    ) -> (String, String) {
        use std::cell::RefCell;

        // A thread local rather than a signal, so that the test can change
        // what the harness renders without needing to be inside the Dioxus
        // runtime — and so that tests running side by side cannot see each
        // other's design.
        thread_local! {
            static INPUT: RefCell<Option<(Slide, PresentationDesign)>> =
                const { RefCell::new(None) };
        }

        #[component]
        fn Harness() -> Element {
            let Some((slide, design)) = INPUT.with(|input| input.borrow().clone()) else {
                return rsx! {};
            };
            rsx! {
                StaticSlideRendererComponent { slide, presentation_design: design }
            }
        }

        INPUT.with(|input| *input.borrow_mut() = Some((slide.clone(), first)));
        let mut dom = VirtualDom::new(Harness);
        dom.rebuild_in_place();
        let before = dioxus_ssr::render(&dom);

        // The design is edited and the page redrawn — the path a settings page
        // takes, and the one the memos did not survive.
        INPUT.with(|input| *input.borrow_mut() = Some((slide, second)));
        // `APP`, not `ROOT`: `ScopeId::ROOT` is the virtual DOM's own base
        // scope, and the component passed to `VirtualDom::new` is `APP`.
        dom.mark_dirty(ScopeId::APP);
        dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
        let after = dioxus_ssr::render(&dom);

        (before, after)
    }

    /// A design with every text block set the given way.
    fn design_with(change: impl Fn(&mut FontRepresentation)) -> PresentationDesign {
        let mut design = PresentationDesign::default();
        let PresentationDesignSettings::Template(template) =
            &mut design.presentation_design_settings
        else {
            panic!("the default design is a template");
        };
        for font in &mut template.fonts {
            change(font);
        }
        design
    }

    /// The title slide is what the design editor's preview opens on, and the
    /// one the bug was reported against.
    #[test]
    fn a_title_slide_follows_the_design() {
        let slide = Slide::new_title_slide("Amazing Grace".to_string(), None);

        let (upright, slanted) = render_twice(
            slide.clone(),
            design_with(|font| font.italic = false),
            design_with(|font| font.italic = true),
        );

        assert!(
            upright.contains("font-style:normal"),
            "the first render was not upright: {upright}"
        );
        assert!(
            slanted.contains("font-style:italic"),
            "switching the design to italic left the slide unchanged: {slanted}"
        );

        // …and back again, since a switch has two directions and only one of
        // them was ever exercised by hand.
        let (slanted, upright) = render_twice(
            slide,
            design_with(|font| font.italic = true),
            design_with(|font| font.italic = false),
        );
        assert!(slanted.contains("font-style:italic"), "got {slanted}");
        assert!(
            !upright.contains("font-style:italic"),
            "switching italic off left the slide slanted: {upright}"
        );
    }

    /// The same for the slide the congregation actually sings from, which is
    /// drawn by a different renderer that had the same three memos.
    #[test]
    fn a_content_slide_follows_the_design() {
        let slide = Slide::new_content_slide(
            "Amazing grace, how sweet the sound".to_string(),
            Some("That saved a wretch like me".to_string()),
            None,
        );

        let (light, heavy) = render_twice(
            slide.clone(),
            design_with(|font| font.set_bold(false)),
            design_with(|font| font.set_bold(true)),
        );

        assert!(light.contains("font-weight:400"), "got {light}");
        assert!(
            heavy.contains("font-weight:700"),
            "switching the design to bold left the slide unchanged: {heavy}"
        );

        // The spoiler is drawn from a font block of its own and had a memo of
        // its own, so it is worth its own turn.
        let (upright, slanted) = render_twice(
            slide,
            design_with(|font| font.italic = false),
            design_with(|font| font.italic = true),
        );
        assert!(
            slanted.matches("font-style:italic").count()
                > upright.matches("font-style:italic").count(),
            "the spoiler did not follow the design: {slanted}"
        );
    }

    /// The colour was never memoised and so never broke. It is asserted here
    /// so that a future tidy-up that "helpfully" wraps it in a memo is caught
    /// by this file rather than by somebody at a projector.
    #[test]
    fn the_colour_follows_the_design_too() {
        let (white, red) = render_twice(
            Slide::new_content_slide("Amazing grace".to_string(), None, None),
            design_with(|font| font.color = rgb::Rgba::new(255, 255, 255, 255)),
            design_with(|font| font.color = rgb::Rgba::new(255, 0, 0, 255)),
        );

        assert!(white.contains("255, 255, 255"), "got {white}");
        assert!(red.contains("255, 0, 0"), "got {red}");
    }
}
