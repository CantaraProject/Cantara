//! This module provides functionality for rendering the slides in HTML for the presentation

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use cantara_songlib::slides::*;
use dioxus::prelude::*;
use rgb::RGBA8;
use rust_i18n::t;

use crate::logic::css::{CssHandler, PlaceItems};
use crate::logic::presentation::get_picture_path;
use crate::logic::settings::{CssSize, HorizontalAlign, VerticalAlign};
#[cfg(target_arch = "wasm32")]
use crate::logic::sync::{
    SYNC_KEY_ACTIVE, SYNC_KEY_POSITION, SYNC_KEY_POSITION_FROM_CONSOLE, SYNC_KEY_PRESENTATION,
    SYNC_KEY_QUIT,
};
use crate::{
    MAIN_CSS,
    logic::{
        settings::{FontRepresentation, PresentationDesignSettings, PresentationDesignTemplate},
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
    let nav = navigator();

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

    // If there's still no presentation data, show an error
    if running_presentations.read().is_empty() {
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

    // Sync changes from the shared signal into the local signal (e.g. from presenter console)
    // Also close/navigate back if the presentation was ended (signal cleared).
    use_effect(move || {
        let current = running_presentations.read();
        if current.is_empty() {
            #[cfg(feature = "desktop")]
            dioxus::desktop::window().close();
            // On web, navigate back if routed
            #[cfg(not(feature = "desktop"))]
            if is_routed {
                nav.replace(crate::Route::Selection {});
            }
            return;
        }
        if let Some(rp) = current.first() {
            if *rp != *running_presentation.peek() {
                running_presentation.set(rp.clone());
            }
        }
    });

    // Sync changes from this window back to the shared signal
    use_effect(move || {
        let local = running_presentation.read().clone();
        let mut shared = running_presentations.write();
        if let Some(first) = shared.first_mut() {
            if *first != local {
                *first = local;
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
                    Key::F11 => {
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
                    // Determine if the current slide is a picture slide so we can
                    // give its container full height while keeping text slides
                    // content-sized for proper grid vertical alignment.
                    let slide_content = current_slide.read().clone().unwrap().slide_content.clone();
                    let container_style = if matches!(slide_content, SlideContent::SimplePicture(_)) {
                        "height: 100%;"
                    } else {
                        ""
                    };

                    rsx! {
                        div {
                            class: "slide-container presentation-fade-in",
                            style: "{container_style}",
                            key: "{current_slide_number}",
                            {
                                match slide_content {
                                    SlideContent::Title(title_slide) => rsx! {
                                        TitleSlideComponent {
                                            title_slide: title_slide.clone(),
                                            title_font_representation: current_pds.read().get_default_headline_font()
                                        }
                                    },
                                    SlideContent::SingleLanguageMainContent(main_slide) => rsx! {
                                        SingleLanguageMainContentSlideRenderer {
                                            main_slide: main_slide.clone(),
                                            main_content_font: current_pds.read().get_default_font(),
                                            spoiler_content_font: current_pds.read().get_default_spoiler_font(),
                                            distance: current_pds().main_content_spoiler_content_padding,
                                        }
                                    },
                                    SlideContent::Empty(empty_slide) => rsx! {
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
