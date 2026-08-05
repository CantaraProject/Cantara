//! This module provides functionality for rendering the slides in HTML for the presentation

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use cantara_songlib::slides::*;
use dioxus::prelude::*;
use regex::Regex;
use rust_i18n::t;
use uuid::Uuid;

use crate::logic::css::{CssHandler, PlaceItems};
use crate::logic::presentation::{get_markdown_html, get_picture_path};
use crate::logic::settings::{CssSize, HorizontalAlign, VerticalAlign};
#[cfg(target_arch = "wasm32")]
use crate::logic::sync::{
    SYNC_KEY_ACTIVE, SYNC_KEY_FILES, SYNC_KEY_POSITION, SYNC_KEY_POSITION_FROM_CONSOLE,
    SYNC_KEY_PRESENTATION, SYNC_KEY_QUIT,
};
use crate::{
    MAIN_CSS,
    logic::{
        settings::{AfterLastSlide, FontRepresentation, NotationSettings, PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate, SlideTransition},
        states::RunningPresentation,
    },
};

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
const PDFJS_LIB: Asset = asset!("/node_modules/pdfjs-dist/build/pdf.min.mjs");
#[cfg(not(target_arch = "wasm32"))]
const PDFJS_WORKER: Asset = asset!("/node_modules/pdfjs-dist/build/pdf.worker.min.mjs");
/// CDN URL for PDF.js library (used on the web/WASM target where node_modules are unavailable).
/// Loaded via dynamic `import()` in JavaScript, which does not support Subresource Integrity (SRI).
#[cfg(target_arch = "wasm32")]
const PDFJS_CDN_LIB: &str = "https://cdn.jsdelivr.net/npm/pdfjs-dist@4.10.38/build/pdf.min.mjs";
#[cfg(target_arch = "wasm32")]
const PDFJS_CDN_WORKER: &str = "https://cdn.jsdelivr.net/npm/pdfjs-dist@4.10.38/build/pdf.worker.min.mjs";

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
            if let Some(files_json) = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(SYNC_KEY_FILES).ok().flatten())
            {
                use crate::logic::settings::RepositoryType;
                use std::collections::HashMap;

                if let Ok(files) = serde_json::from_str::<HashMap<String, String>>(&files_json) {
                    for (path, b64) in &files {
                        if let Ok(bytes) = base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            b64,
                        ) {
                            RepositoryType::store_web_file(path, bytes);
                        }
                    }
                }
                // Clean up to free localStorage space
                let _ = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .map(|s| s.remove_item(SYNC_KEY_FILES));
            }

            web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(SYNC_KEY_PRESENTATION).ok().flatten())
                .and_then(|json| serde_json::from_str(&json).ok())
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
    let is_synced_tab = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(SYNC_KEY_ACTIVE).ok().flatten())
        .map(|v| v == "true")
        .unwrap_or(false);
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
            let rp = running_presentation.read();
            if let Ok(json) = serde_json::to_string(&*rp) {
                let _ = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .map(|s| s.set_item(SYNC_KEY_POSITION, &json));
            }
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
                let quit = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .and_then(|s| s.get_item(SYNC_KEY_QUIT).ok().flatten())
                    .map(|v| v == "true")
                    .unwrap_or(false);
                if quit {
                    running_presentations.write().clear();
                    // Close this tab
                    let _ = document::eval("window.close()").await;
                    return;
                }

                // Read position updates from the presenter console
                if let Some(json) = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .and_then(|s| s.get_item(SYNC_KEY_POSITION_FROM_CONSOLE).ok().flatten())
                    && !json.is_empty() && json != *last_sync_json.peek() {
                        last_sync_json.set(json.clone());
                        if let Ok(rp) = serde_json::from_str::<RunningPresentation>(&json)
                            && *running_presentation.peek() != rp {
                                // Load any new VFS files (e.g. PDFs) that the
                                // update_presentation call stored in localStorage
                                if let Some(files_json) = web_sys::window()
                                    .and_then(|w| w.local_storage().ok().flatten())
                                    .and_then(|s| s.get_item(SYNC_KEY_FILES).ok().flatten())
                                {
                                    use crate::logic::settings::RepositoryType;
                                    use std::collections::HashMap;

                                    if let Ok(files) = serde_json::from_str::<HashMap<String, String>>(&files_json) {
                                        for (path, b64) in &files {
                                            if let Ok(bytes) = BASE64.decode(b64) {
                                                RepositoryType::store_web_file(path, bytes);
                                            }
                                        }
                                    }
                                    let _ = web_sys::window()
                                        .and_then(|w| w.local_storage().ok().flatten())
                                        .map(|s| s.remove_item(SYNC_KEY_FILES));
                                }
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
            let _ = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .map(|s| {
                    // Signal quit to any synced tabs
                    let _ = s.set_item(SYNC_KEY_QUIT, "true");
                    // Perform full cleanup of sync-related keys to avoid stale state
                    let _ = s.remove_item(SYNC_KEY_ACTIVE);
                    let _ = s.remove_item(SYNC_KEY_PRESENTATION);
                    let _ = s.remove_item(SYNC_KEY_POSITION);
                    let _ = s.remove_item(SYNC_KEY_POSITION_FROM_CONSOLE);
                    let _ = s.remove_item(SYNC_KEY_FILES);
                });
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
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
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
                    Key::Character(ref c) if c == "b" || c == "B" => {
                        running_presentation.write().toggle_black_screen();
                    }
                    _ => {}
                }
            },
            PresentationRendererComponent { running_presentation }

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

/// The actual presentation rendering component which can be used to render presentations accordingly
/// It takes a signal and rewrites to it when the presentation position changes
#[component]
pub fn PresentationRendererComponent(
    /// The running presentation as a signal: This will be changed by the component if the user moves the current slide
    running_presentation: Signal<RunningPresentation>,
    /// Whether this instance should fire the auto-advance timer.
    /// Defaults to `true`. Set to `false` in secondary views (presenter console preview,
    /// example viewer) so only the primary presentation window drives the timer.
    #[props(default = true)]
    fire_timer: bool,
) -> Element {
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
        presentation_is_visible.set(false);
        presentation_is_visible.set(true);
    };

    let mut go_to_previous_slide = move || {
        running_presentation.write().previous_slide();
        presentation_is_visible.set(false);
        presentation_is_visible.set(true);
    };

    // Auto-advance timer: each time the slide changes, a new `spawn`-ed task
    // is launched via `use_effect`. A generation counter ensures that only the
    // most-recent timer fires – if the user (or a previous timer) navigated to
    // a new slide before the sleep completed, the old task detects the changed
    // generation and exits without advancing again.
    //
    // `fire_timer` is false in secondary views (presenter console preview, example
    // viewer) so that only the primary presentation window drives the timer.
    // Without this guard every window hosting a PresentationRendererComponent would
    // independently advance the slide, causing slides to be skipped.
    let mut timer_generation: Signal<u64> = use_signal(|| 0);

    use_effect(move || {
        // Track slide changes by reading current_slide_number (subscribes to it)
        let _ = current_slide_number();

        // Only the primary presentation window should fire the timer.
        if !fire_timer {
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

    let background_css: Memo<String> = use_memo(move || {
        let mut css: CssHandler = CssHandler::new();
        let pds = current_pds();

        // A `url()` pointing into the file system is as unreachable for the
        // page as an `img` source is, so the picture is inlined the same way.
        if let Some(source) = pds
            .background_image
            .as_ref()
            .and_then(|image| crate::logic::images::image_data_url(&image.as_source().path))
        {
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
            class: "presentation",
            style: css_handler.read().to_string(),

            tabindex: 0,
            onkeydown: move |event: Event<KeyboardData>| {
                let key = event.key();
                match key {
                    Key::ArrowRight | Key::Enter => go_to_next_slide(),
                    Key::Character(ref c) if c == " " => go_to_next_slide(),
                    Key::ArrowLeft => go_to_previous_slide(),
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
            if presentation_is_visible() {
                if let Some(slide) = current_slide.read().clone() {
                    {
                        let slide_content = slide.slide_content.clone();
                        let container_style = slide_container_style(&slide_content);
                        let tc = transition_class();

                        rsx! {
                            div {
                                class: "slide-container {tc}",
                                style: "{container_style}",
                                key: "{current_slide_number}",
                                SlideContentRenderer {
                                    slide_content,
                                    pds: current_pds(),
                                    running_presentation: Some(running_presentation),
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
    // Build the CSS
    let css_handler: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();
        css.opacity(1.0);
        css.z_index(2);
        let mut font = title_font_representation.clone();
        if bold {
            font.weight = font.weight.max(700);
        }
        css.extend(&CssHandler::from(font));
        css
    });
    let css_handler_string: Memo<String> = use_memo(move || css_handler.to_string());

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
        div { class: "headline", style: css_handler_string(),
            p { style: css_handler_string(), {title_slide.title_text} }
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

    let main_css: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(main_content_font.clone()));
        css
    });

    let distance_css: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.min_height(distance.clone().unwrap_or(CssSize::Em(4.0)));

        css
    });

    let spoiler_css: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(spoiler_content_font.clone()));
        css
    });

    rsx! {
        div {
            div { class: "main-content", style: main_css.read().to_string(),
                p { style: main_css.read().to_string(),
                    for (num , line) in main_slide.clone().main_text().split("\n").enumerate() {
                        {line}
                        if num < number_of_main_content_lines - 1 {
                            br {}
                        }
                    }
                }
            }
            if let Some(spoiler_content) = main_slide.spoiler_text() {
                div { class: "distance", style: distance_css.read().to_string() }
                div {
                    class: "spoiler-content",
                    style: spoiler_css.read().to_string(),
                    p { style: spoiler_css.read().to_string(),
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
    let main_css: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();
        css.set_important(true);
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(main_content_font.clone()));
        css
    });

    let distance_css: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();
        css.set_important(true);
        css.min_height(distance.clone().unwrap_or(CssSize::Em(4.0)));
        css
    });

    rsx! {
        div {
            for (lang_idx , text) in multi_slide.main_text_list.iter().enumerate() {
                div { class: "language-section",
                    p {
                        class: "language-label",
                        style: "font-weight: bold; margin-top: 0.5em;",
                        {format!("Language {}", lang_idx + 1)}
                    }
                    p { style: main_css.read().to_string(),
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
                        style: distance_css.read().to_string(),
                    }
                }
            }
            if !multi_slide.spoiler_text_vector.is_empty() {
                div { class: "distance", style: distance_css.read().to_string() }
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

/// Prefixes the tune with a `%%staffsep` directive for the wanted line height.
///
/// `staff_line_height` is a multiple of the engraver's own spacing, so 1.0
/// changes nothing and the directive is left out entirely.
fn with_staff_separation(abc: &str, staff_line_height: f64) -> String {
    let factor = staff_line_height.clamp(0.2, 5.0);
    if (factor - 1.0).abs() < f64::EPSILON {
        return abc.to_string();
    }

    let separation = (ABCJS_NEUTRAL_STAFF_SEPARATION * factor).round();
    format!("%%staffsep {separation}\n{abc}")
}

/// The box the staff is drawn in: how wide it is and where it sits.
///
/// At 100% it is exactly the box the text rows use, so the staff and the words
/// line up on both edges — the notation block gets no padding of its own.
fn notation_block_style(notation: &NotationSettings) -> String {
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
fn abcjs_vocal_font(font: &FontRepresentation, size: &CssSize) -> String {
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
        SlideContent::SimplePicture(_) => "height: 100%;",
        SlideContent::SingleLanguageMainContent(main_slide) => {
            if get_markdown_html(&main_slide.clone().main_text()).is_some() {
                "height: 100%;"
            } else {
                ""
            }
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
fn meta_text_of(slide_content: &SlideContent) -> Option<String> {
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
        SlideContent::Empty(_) | SlideContent::SimplePicture(_) | SlideContent::PdfPage(_) => None,
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
    let css = use_hook(crate::logic::fonts::bundled_font_face_css);

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
) -> Element {
    let meta_text = meta_text_of(&slide_content);
    let meta_font = pds.get_default_meta_font();

    // The title slide shows the meta information right below the headline, so
    // it reads as part of the title. Every other slide keeps it out of the way
    // in the bottom corner.
    let is_title_slide = matches!(slide_content, SlideContent::Title(_));

    rsx! {
        {slide_body(slide_content, pds, running_presentation)}
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
            SimplePictureSlideComponent { picture_slide: picture_slide.clone() }
        },
        SlideContent::PdfPage(pdf_slide) => rsx! {
            PdfPageCanvas {
                pdf_path: pdf_slide.pdf_path.clone(),
                page_num: pdf_slide.page_number,
            }
        },
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
fn SimplePictureSlideComponent(picture_slide: SimplePictureSlide) -> Element {
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
                PdfPageCanvas { pdf_path: base_path, page_num }
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
                style: "max-width: 100%; max-height: 100%; object-fit: contain;",
            }
        }
    }
}

/// Reads a PDF and returns it base64-encoded, ready to hand to PDF.js.
///
/// Only called on a cache miss: the bytes are the expensive part of showing a
/// PDF slide, and they only have to cross into the page once per document.
fn read_pdf_as_base64(pdf_path: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(pdf_path)
            .map(|bytes| BASE64.encode(&bytes))
            .unwrap_or_default()
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::logic::settings::RepositoryType::web_read_file(pdf_path)
            .map(|bytes| BASE64.encode(&bytes))
            .unwrap_or_default()
    }
}

/// How long to wait between two looks at whether another canvas has finished
/// handing the document over.
const PDF_WAIT_STEP_MS: u32 = 120;

/// How many such looks before giving up.
///
/// This has to outlast the point at which `pdf_render_inline.js` declares a
/// request stale (four seconds), or a canvas whose loader was unmounted before
/// it delivered would give up rather than fetch the document itself.
const PDF_WAIT_ATTEMPTS: usize = 60;

/// Renders a single PDF page onto a `<canvas>` via PDF.js.
///
/// The document is parsed once per window and kept in `window.__pdfDocCache`;
/// each slide then only sends the short script in `pdf_render_inline.js`. The
/// file is read and base64-encoded **only when that cache misses**, because
/// doing it per slide meant a multi-megabyte string crossed the IPC on every
/// slide change — the reason long PDFs used to stall the presentation.
///
/// The data is read on the Rust side (file system on desktop, VFS on web) so
/// that the page's file-access restrictions do not apply. On desktop PDF.js
/// comes from the bundled `node_modules` assets, on the web from a CDN.
#[component]
pub(crate) fn PdfPageCanvas(pdf_path: String, page_num: u32) -> Element {
    // Use a unique ID per mount cycle to prevent conflicts when the component
    // unmounts and remounts during live updates. Old async render tasks will
    // target a canvas ID that no longer exists in the DOM and exit gracefully.
    let mount_id = use_hook(Uuid::new_v4);
    let canvas_id = format!(
        "pdf-canvas-{}-{}-{}",
        pdf_path.replace(['/', '\\', '.', ' ', ':'], "-"),
        page_num,
        mount_id.as_simple()
    );

    // Get URLs for PDF.js library and worker
    #[cfg(not(target_arch = "wasm32"))]
    let pdfjs_url = format!("{}", PDFJS_LIB);
    #[cfg(not(target_arch = "wasm32"))]
    let worker_url = format!("{}", PDFJS_WORKER);
    #[cfg(target_arch = "wasm32")]
    let pdfjs_url = PDFJS_CDN_LIB.to_string();
    #[cfg(target_arch = "wasm32")]
    let worker_url = PDFJS_CDN_WORKER.to_string();

    rsx! {
        canvas {
            id: "{canvas_id}",
            style: "display: block; max-width: 100%; max-height: 100%;",
            onmounted: move |_| {
                let canvas_id = canvas_id.clone();
                let pdf_path = pdf_path.clone();
                let pdfjs_url = pdfjs_url.clone();
                let worker_url = worker_url.clone();

                spawn(async move {
                    let js_cache_key = serde_json::to_string(&pdf_path).unwrap_or_default();
                    let js_canvas_id = serde_json::to_string(&canvas_id).unwrap_or_default();

                    let render_js = include_str!("../../assets/pdf_render_inline.js")
                        .replace("__CACHE_KEY__", &js_cache_key)
                        .replace("__PAGE_NUM__", &page_num.to_string())
                        .replace("__CANVAS_ID__", &js_canvas_id);

                    // 1. Try the document already in the page. This is the
                    //    common case — every slide after the first.
                    //
                    //    While another canvas of the same document is fetching
                    //    it, this one is told to wait rather than to ask for a
                    //    copy of its own; it looks again in a moment, and the
                    //    document is normally there by the second or third
                    //    look. Should that canvas never deliver — it may be
                    //    unmounted mid-flight — the request goes stale and
                    //    this one is told to fetch the document itself, which
                    //    is what the budget below outlasts.
                    let mut attempt = 0;
                    loop {
                        match document::eval(&render_js).await {
                            Ok(value) => {
                                if value.get("missing").and_then(|m| m.as_bool()) == Some(true) {
                                    break;
                                }
                                if value.get("waiting").and_then(|w| w.as_bool()) != Some(true) {
                                    return;
                                }
                            }
                            Err(error) => {
                                log::error!("could not render the PDF page: {error:?}");
                                return;
                            }
                        }

                        attempt += 1;
                        if attempt > PDF_WAIT_ATTEMPTS {
                            log::warn!(
                                "gave up waiting for {pdf_path} to be loaded by another slide"
                            );
                            return;
                        }
                        let _ = document::eval(&format!(
                            "await new Promise(r => setTimeout(r, {PDF_WAIT_STEP_MS}))"
                        ))
                        .await;
                    }

                    // 2. First page of this document in this window: pay for
                    //    reading and encoding it, then hand it over once.
                    let base64_data = read_pdf_as_base64(&pdf_path);
                    if base64_data.is_empty() {
                        log::warn!("PDF data empty for {pdf_path}, skipping render");
                        return;
                    }

                    let load_js = include_str!("../../assets/pdf_load_inline.js")
                        .replace("__PDFJS_URL__", &serde_json::to_string(&pdfjs_url).unwrap_or_default())
                        .replace("__WORKER_URL__", &serde_json::to_string(&worker_url).unwrap_or_default())
                        .replace("__CACHE_KEY__", &js_cache_key)
                        .replace("__BASE64__", &serde_json::to_string(&base64_data).unwrap_or_default());

                    match document::eval(&load_js).await {
                        Ok(value) if value.get("ok").and_then(|ok| ok.as_bool()) == Some(true) => {}
                        Ok(value) => {
                            log::error!(
                                "could not load the PDF {pdf_path}: {}",
                                value.get("error").and_then(|e| e.as_str()).unwrap_or("unknown")
                            );
                            return;
                        }
                        Err(error) => {
                            log::error!("could not load the PDF {pdf_path}: {error:?}");
                            return;
                        }
                    }

                    // 3. Now the cache holds it, so the page can be drawn.
                    if let Err(error) = document::eval(&render_js).await {
                        log::error!("could not render the PDF page: {error:?}");
                    }
                });
            },
        }
    }
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
        if let Some(source) = pds
            .background_image
            .as_ref()
            .and_then(|image| crate::logic::images::image_data_url(&image.as_source().path))
        {
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
