//! This module provides functionality for rendering the slides in HTML for the presentation

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use cantara_songlib::slides::*;
use dioxus::html::completions::CompleteWithBraces::strong;
use dioxus::prelude::*;
use regex::Regex;
use rgb::RGBA8;
use rust_i18n::t;

use crate::logic::css::{CssHandler, PlaceItems};
use crate::logic::presentation::{get_markdown_html, get_picture_path};
use crate::logic::settings::{CssSize, HorizontalAlign, VerticalAlign};
#[cfg(target_arch = "wasm32")]
use crate::logic::sync::{
    SYNC_KEY_ACTIVE, SYNC_KEY_POSITION, SYNC_KEY_POSITION_FROM_CONSOLE, SYNC_KEY_PRESENTATION,
    SYNC_KEY_QUIT,
};
use crate::{
    MAIN_CSS,
    logic::{
        settings::{AfterLastSlide, FontRepresentation, PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate, SlideTransition},
        states::RunningPresentation,
    },
};

const PRESENTATION_CSS: Asset = asset!("/assets/presentation.css");
const PRESENTATION_JS: Asset = asset!("/assets/presentation_positioning.js");
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
    // In that case the running_presentations signal will be empty, and we load data from localStorage.
    #[cfg(target_arch = "wasm32")]
    {
        if running_presentations.read().is_empty() {
            // Try to load synced presentation data from localStorage
            if let Some(json) = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(SYNC_KEY_PRESENTATION).ok().flatten())
            {
                if let Ok(rp) = serde_json::from_str::<RunningPresentation>(&json) {
                    running_presentations.write().push(rp);
                }
            }
        }
    }

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
    #[cfg(not(target_arch = "wasm32"))]
    let is_synced_tab = false;

    // If there's still no presentation data, close the window (desktop) or show an error
    if running_presentations.read().is_empty() {
        #[cfg(feature = "desktop")]
        dioxus::desktop::window().close();

        return rsx! {
            document::Link { rel: "stylesheet", href: MAIN_CSS }
            div {
                style: "all: initial; margin:0; width:100%; height:100%; background-color: black; color: white; display: flex; align-items: center; justify-content: center;",
                p { "No presentation data found." }
            }
        };
    }

    let mut running_presentation: Signal<RunningPresentation> =
        use_signal(move || running_presentations.get(0).unwrap().clone());

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
            if is_routed {
                nav.replace(crate::Route::Selection {});
            }
            return;
        }
        if let Some(rp) = current.first() {
            if !rp.eq_ignoring_scroll(&running_presentation.peek()) {
                running_presentation.set(rp.clone());
            }
        }
    });

    // local→shared: push local changes (e.g. user clicked next slide) back
    // to the shared signal. Uses .peek() for the shared read to avoid
    // subscribing to it (only local changes should trigger this effect).
    #[cfg(not(feature = "desktop"))]
    use_effect(move || {
        let local = running_presentation.read().clone();
        let shared = running_presentations.peek();
        if let Some(first) = shared.first() {
            if !first.eq_ignoring_scroll(&local) {
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
        let mut last_sync_json = use_signal(|| String::new());
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
                {
                    if !json.is_empty() && json != *last_sync_json.peek() {
                        last_sync_json.set(json.clone());
                        if let Ok(rp) = serde_json::from_str::<RunningPresentation>(&json) {
                            if *running_presentation.peek() != rp {
                                running_presentation.set(rp);
                            }
                        }
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
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
        document::Title { { t!("presentation.title").to_string() } }
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
                                let _ = document::eval("
                                    if (document.fullscreenElement) {
                                        document.exitFullscreen();
                                    } else {
                                        document.documentElement.requestFullscreen();
                                    }
                                ").await;
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
            PresentationRendererComponent {
                running_presentation: running_presentation
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
                        { t!("presenter.quit").to_string() }
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
            div {
                style: "
                    all: initial;
                    margin:0;
                    width:100%;
                    height:100%;
                    background-color: black;
                ",
                p {
                    { "No presentation data found." },
                }
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

    let css_presentation_background_color = use_memo(move || current_pds().background_color);

    let css_main_content_font_size = use_memo(move || {
        current_pds
            .read()
            .fonts
            .first()
            .unwrap_or(&FontRepresentation::default())
            .font_size
            .clone()
    });

    let css_main_text_color: Memo<RGBA8> =
        use_memo(move || current_pds.read().clone().fonts.first().unwrap().color);
    let css_padding_left: Memo<CssSize> = use_memo(move || current_pds().padding.left);
    let css_padding_right: Memo<CssSize> = use_memo(move || current_pds().padding.right);
    let css_padding_top: Memo<CssSize> = use_memo(move || current_pds().padding.top);
    let css_padding_bottom: Memo<CssSize> = use_memo(move || current_pds().padding.bottom);
    let css_text_align: Memo<HorizontalAlign> = use_memo(move || {
        current_pds
            .read()
            .fonts
            .first()
            .unwrap()
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

        if let Some(image) = pds.background_image {
            css.background_image(image.as_source().path.to_str().unwrap_or_default());
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
                div {
                    style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; background-color: black; z-index: 1000;",
                }
            }
            div {
                class: "background",
                style: background_css()
            }
            if presentation_is_visible() {
                {
                    let slide_content = current_slide.read().clone().unwrap().slide_content.clone();
                    let container_style = slide_container_style(&slide_content);
                    let tc = transition_class();

                    rsx! {
                        div {
                            class: "slide-container {tc}",
                            style: "{container_style}",
                            key: "{current_slide_number}",
                            SlideContentRenderer {
                                slide_content: slide_content,
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

#[component]
fn TitleSlideComponent(
    title_slide: TitleSlide,
    title_font_representation: FontRepresentation,
) -> Element {
    // Build the CSS
    let css_handler: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(title_font_representation.clone()));
        css
    });
    let css_handler_string: Memo<String> = use_memo(move || css_handler.to_string());

    rsx! {
        div {
            class: "headline",
            style: css_handler_string(),
            p {
                style: css_handler_string(),
                { title_slide.title_text }
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
            div {
                class: "main-content",
                style: main_css.read().to_string(),
                p {
                    style: main_css.read().to_string(),
                    for (num, line) in main_slide.clone().main_text().split("\n").enumerate() {
                        { line }
                        if num < number_of_main_content_lines -1 {
                            br { }
                        }
                    }
                }
            }
            if let Some(spoiler_content) = main_slide.spoiler_text() {
                div {
                    class: "distance",
                    style: distance_css.read().to_string(),
                }
                div {
                    class: "spoiler-content",
                    style: spoiler_css.read().to_string(),
                    p {
                        style: spoiler_css.read().to_string(),
                        for (num, line) in spoiler_content.split("\n").enumerate() {
                            { line }
                            if num < spoiler_content.split("\n").count() - 1 {
                                br { }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EmptySlideComponent() -> Element {
    rsx! {
        div {
            class: "empty-content",
        }
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
#[component]
fn SlideContentRenderer(
    slide_content: SlideContent,
    pds: PresentationDesignTemplate,
    running_presentation: Option<Signal<RunningPresentation>>,
) -> Element {
    match slide_content {
        SlideContent::Title(title_slide) => rsx! {
            TitleSlideComponent {
                title_slide: title_slide.clone(),
                title_font_representation: pds.get_default_headline_font()
            }
        },
        SlideContent::SingleLanguageMainContent(main_slide) => {
            let text = main_slide.clone().main_text();
            if let Some(html) = get_markdown_html(&text) {
                let html_owned = html.to_string();
                rsx! {
                    MarkdownSlideComponent {
                        html_content: html_owned,
                        running_presentation: running_presentation,
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
        SlideContent::Empty(_) => rsx! {
            EmptySlideComponent {}
        },
        SlideContent::SimplePicture(picture_slide) => rsx! {
            SimplePictureSlideComponent {
                picture_slide: picture_slide.clone()
            }
        },
        _ => rsx! { p { "No content provided" } }
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
    let re = Regex::new(r"(?i)<([a-z1-6]+)([^>]*)>").unwrap();

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
                if (signal_pos - dom_pos).abs() > SCROLL_SYNC_THRESHOLD {
                    if let Some(first) = shared.write().first_mut() {
                        first.markdown_scroll_position = dom_pos;
                    }
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
            style: format!("overflow-y: auto; max-height: 100%; padding: 1em 2em; box-sizing: border-box; {}", font_css).to_string(),
            dangerous_inner_html: html_content
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
            div {
                style: "width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; z-index: 2;",
                PdfPageCanvas {
                    pdf_path: base_path,
                    page_num: page_num,
                }
            }
        };
    }

    rsx! {
        div {
            style: "width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; z-index: 2;",
            img {
                src: "{path}",
                style: "max-width: 100%; max-height: 100%; object-fit: contain;",
            }
        }
    }
}

/// Renders a single PDF page onto a <canvas> via PDF.js.
/// Reads the PDF data on the Rust side (from filesystem on desktop, from VFS on web)
/// and sends base64‑encoded data to JavaScript so that file-access restrictions are avoided.
///
/// On desktop, PDF.js is loaded from bundled node_modules assets.
/// On web (WASM), PDF.js is loaded from a CDN.
///
/// All JavaScript code is inlined in the `document::eval()` call so that rendering
/// is self-contained and does not depend on external script loading order.
#[component]
fn PdfPageCanvas(pdf_path: String, page_num: u32) -> Element {
    let canvas_id = format!(
        "pdf-canvas-{}-{}",
        pdf_path.replace(['/', '\\', '.', ' ', ':'], "-"),
        page_num
    );

    // Read the PDF file and encode it as base64 so we can hand it to JS
    let base64_data = use_memo({
        let pdf_path = pdf_path.clone();
        move || {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::fs::read(&*pdf_path)
                    .map(|bytes| BASE64.encode(&bytes))
                    .unwrap_or_default()
            }
            #[cfg(target_arch = "wasm32")]
            {
                crate::logic::settings::RepositoryType::web_read_file(&pdf_path)
                    .map(|bytes| BASE64.encode(&bytes))
                    .unwrap_or_default()
            }
        }
    });

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
                let b64 = base64_data.read().clone();
                let pdfjs_url = pdfjs_url.clone();
                let worker_url = worker_url.clone();
                spawn(async move {
                    // Use serde_json to safely escape all string values for JavaScript
                    let js_pdfjs_url = serde_json::to_string(&pdfjs_url).unwrap_or_default();
                    let js_worker_url = serde_json::to_string(&worker_url).unwrap_or_default();
                    let js_b64 = serde_json::to_string(&b64).unwrap_or_default();
                    let js_cache_key = serde_json::to_string(&pdf_path).unwrap_or_default();
                    let js_canvas_id = serde_json::to_string(&canvas_id).unwrap_or_default();

                    // Self-contained JS: loads PDF.js if needed, decodes PDF, renders page.
                    // Uses string replacement instead of format!() to avoid double-brace noise.
                    let js = include_str!("../../assets/pdf_render_inline.js")
                        .replace("__PDFJS_URL__", &js_pdfjs_url)
                        .replace("__WORKER_URL__", &js_worker_url)
                        .replace("__BASE64__", &js_b64)
                        .replace("__CACHE_KEY__", &js_cache_key)
                        .replace("__PAGE_NUM__", &page_num.to_string())
                        .replace("__CANVAS_ID__", &js_canvas_id);

                    let _ = document::eval(&js).await;
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
        if let Some(ref image) = pds.background_image {
            css.background_image(image.as_source().path.to_str().unwrap_or_default());
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
        div {
            class: "presentation",
            style: css_handler.to_string(),
            div {
                class: "background",
                style: "{background_css}"
            }
            div {
                class: "slide-container",
                style: "{container_style}",
                SlideContentRenderer {
                    slide_content: slide_content,
                    pds: pds,
                }
            }
        }
    }
}
