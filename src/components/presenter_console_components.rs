//! This module contains the components for the Presenter Console window.
//! The presenter console shows the current slide text, a live preview, and navigation controls.

use crate::logic::presentation::{get_markdown_html, get_picture_path, html_to_plain_text};
use crate::logic::settings::{PresentationDesign, PresenterConsoleView, use_settings};
use crate::logic::states::RunningPresentation;
#[cfg(target_arch = "wasm32")]
use crate::logic::sync::{
    SYNC_KEY_ACTIVE, SYNC_KEY_POSITION, SYNC_KEY_POSITION_FROM_CONSOLE, SYNC_KEY_PRESENTATION,
    SYNC_KEY_QUIT,
};
use crate::MAIN_CSS;
use cantara_songlib::slides::{SlideContent, SlideRow};
use dioxus::prelude::*;
use rust_i18n::t;

use super::jump_sidebar::{JumpSidebar, JumpTarget};
use super::presentation_components::{
    PresentationRendererComponent, PresentationRole, StaticSlideRendererComponent,
};

/// The stylesheet of the presenter console.
///
/// Public for the same reason as [`PRESENTATION_CSS`](super::presentation_components::PRESENTATION_CSS):
/// [`App`](crate::App) registers it, and the registration below is what the
/// separate console window on the desktop needs.
pub const PRESENTER_CONSOLE_CSS: Asset = asset!("/assets/presenter_console.css");

rust_i18n::i18n!("locales", fallback = "en");

/// The entry point for the presenter console window.
/// Works both as a routed page in the main window and as a standalone window
/// (via `with_root_context`).
#[component]
pub fn PresenterConsolePage() -> Element {
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    // Detect whether we are hosted in the main window (router available with known routes)
    // vs. a separate window. In the main window the presenter console is reached via a route,
    // so we can navigate back. In a separate window we close it.
    // We use try_consume_context to safely check for a router context, because calling
    // navigator() directly would panic in standalone desktop windows (no router present).
    let is_main_window = try_consume_context::<dioxus::router::RouterContext>().is_some();
    // Only acquire the navigator if a router is present to avoid panicking.
    let nav = if is_main_window { Some(navigator()) } else { None };

    if running_presentations.peek().is_empty() {
        if is_main_window {
            if let Some(nav) = &nav {
                nav.replace(crate::Route::Selection {});
            }
        } else {
            #[cfg(feature = "desktop")]
            dioxus::desktop::window().close();
        }
        return rsx! {};
    }

    let mut running_presentation: Signal<RunningPresentation> =
        use_signal(move || running_presentations.peek().first().cloned().unwrap_or_else(|| {
            // This fallback is only hit if the list is concurrently cleared between
            // the empty-check above and signal initialization.
            running_presentations
                .peek()
                .first()
                .cloned()
                .unwrap_or_else(|| {
                    // If it was cleared, the polling/effects will immediately close/navigate.
                    // Use the first available entry if it comes back in the same tick.
                    RunningPresentation::new(vec![])
                })
        }));

    // View mode signal, initialized from settings
    let settings = use_settings();
    let view: Signal<PresenterConsoleView> =
        use_signal(move || settings.read().presenter_console_view);

    // ── Desktop: polling-based bidirectional sync ──────────────────────────
    //
    // Single polling loop handles ALL synchronization between the local signal
    // and the shared context signal. Reactive use_effect hooks are NOT used on
    // desktop to avoid race conditions (see PresentationPage for full explanation).
    //
    // The loop tracks `last_seen_shared` and `last_seen_local` independently so
    // that only actual changes from each side trigger a sync. The shared-changed
    // branch is checked first to give incoming updates from the presentation
    // window priority over stale local state.
    //
    // All comparisons use `eq_ignoring_scroll` to exclude `markdown_scroll_position`,
    // which is synced independently by `MarkdownSlideComponent`.
    //
    // Also monitors whether the shared signal was cleared (presentation ended)
    // and navigates back or closes the window accordingly.
    #[cfg(feature = "desktop")]
    use_future(move || async move {
        let mut last_seen_shared = running_presentations.peek()
            .first().cloned().unwrap_or_else(|| running_presentation.peek().clone());
        let mut last_seen_local = running_presentation.peek().clone();

        loop {
            let _ = document::eval("await new Promise(r => setTimeout(r, 50))").await;

            // Presentation ended (signal cleared by PresentationPage's use_drop)
            if running_presentations.peek().is_empty() {
                if is_main_window {
                    if let Some(nav) = &nav {
                        nav.replace(crate::Route::Selection {});
                    }
                } else {
                    dioxus::desktop::window().close();
                }
                return;
            }

            let current_shared = running_presentations.peek()
                .first().cloned();
            let current_local = running_presentation.peek().clone();

            if let Some(ref shared_rp) = current_shared {
                // Shared changed (presentation window pushed an update) → pull into local
                if !shared_rp.eq_ignoring_scroll(&last_seen_shared) {
                    last_seen_shared = shared_rp.clone();
                    if !shared_rp.eq_ignoring_scroll(&current_local) {
                        last_seen_local = shared_rp.clone();
                        running_presentation.set(shared_rp.clone());
                    }
                }
                // Local changed (user clicked next/prev in console) → push to shared
                else if !current_local.eq_ignoring_scroll(&last_seen_local) {
                    last_seen_local = current_local.clone();
                    if !current_local.eq_ignoring_scroll(shared_rp) {
                        last_seen_shared = current_local.clone();
                        if let Some(first) = running_presentations.write().first_mut() {
                            // Preserve the shared scroll position to avoid clobbering
                            // scroll-sync updates with potentially stale local state.
                            let preserved_scroll = first.markdown_scroll_position;
                            *first = current_local;
                            first.markdown_scroll_position = preserved_scroll;
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

    // shared→local: propagate changes and handle presentation-ended navigation.
    #[cfg(not(feature = "desktop"))]
    use_effect(move || {
        let current = running_presentations.read();
        if current.is_empty() {
            // Drop the read guard BEFORE navigating — on web (single VirtualDom),
            // nav.replace() triggers a synchronous re-render/diff that would
            // attempt to borrow the same RefCell, causing a panic.
            drop(current);
            if is_main_window
                && let Some(nav) = &nav {
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

    // local→shared: push local changes back to the shared signal.
    #[cfg(not(feature = "desktop"))]
    use_effect(move || {
        let local = running_presentation.read().clone();
        let shared = running_presentations.peek();
        if let Some(first) = shared.first()
            && !first.eq_ignoring_scroll(&local) {
                // We are about to push non-scroll changes from `local` into the
                // shared signal. However, scroll synchronization writes directly
                // to the shared signal, and `eq_ignoring_scroll` prevents scroll-
                // only updates from being reflected in `running_presentation`.
                // To avoid overwriting a newer shared scroll position with a
                // stale local one, preserve the shared `markdown_scroll_position`
                // when applying the update.
                drop(shared);
                if let Some(first) = running_presentations.write().first_mut() {
                    let mut merged = local.clone();
                    merged.markdown_scroll_position = first.markdown_scroll_position;
                    *first = merged;
                }
            }
    });

    // On web: detect if a synced presentation tab is active
    #[cfg(target_arch = "wasm32")]
    let is_sync_active = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(SYNC_KEY_ACTIVE).ok().flatten())
        .map(|v| v == "true")
        .unwrap_or(false);

    // On web: write position changes to localStorage for the synced presentation tab
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if is_sync_active {
            let rp = running_presentation.read();
            if let Ok(json) = serde_json::to_string(&*rp) {
                let _ = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .map(|s| s.set_item(SYNC_KEY_POSITION_FROM_CONSOLE, &json));
            }
        }
    });

    // On web: poll for position changes from the synced presentation tab
    #[cfg(target_arch = "wasm32")]
    {
        let mut last_sync_json = use_signal(String::new);
        use_future(move || async move {
            // If sync is not active, do not poll.
            if !is_sync_active {
                return;
            }
            loop {
                let _ = document::eval("await new Promise(r => setTimeout(r, 150))").await;
                if let Some(json) = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .and_then(|s| s.get_item(SYNC_KEY_POSITION).ok().flatten())
                    && !json.is_empty() && json != *last_sync_json.peek() {
                        last_sync_json.set(json.clone());
                        if let Ok(rp) = serde_json::from_str::<RunningPresentation>(&json)
                            && *running_presentation.peek() != rp {
                                running_presentation.set(rp);
                            }
                    }
            }
        });
    }

    let mut go_to_next_slide = move || {
        running_presentation.write().next_slide();
    };

    let mut go_to_previous_slide = move || {
        running_presentation.write().previous_slide();
    };

    let mut quit_presentation = move || {
        // Clean up sync state on web
        #[cfg(target_arch = "wasm32")]
        {
            let _ = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .map(|s| {
                    let _ = s.set_item(SYNC_KEY_QUIT, "true");
                    let _ = s.remove_item(SYNC_KEY_ACTIVE);
                    let _ = s.remove_item(SYNC_KEY_PRESENTATION);
                    let _ = s.remove_item(SYNC_KEY_POSITION);
                    let _ = s.remove_item(SYNC_KEY_POSITION_FROM_CONSOLE);
                });
        }
        running_presentations.write().clear();
        if is_main_window {
            if let Some(nav) = nav.as_ref() {
                nav.replace(crate::Route::Selection {});
            }
        } else {
            #[cfg(feature = "desktop")]
            dioxus::desktop::window().close();
        }
    };

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: PRESENTER_CONSOLE_CSS }
        // The PDF viewer, loaded once per window. Registered *here*, at the
        // root, for the reason written above about the stylesheets: a
        // registration made by a scope that is dropped before its effect runs
        // is lost, and Dioxus never inserts the same src twice — so a script
        // asked for from inside a slide or a thumbnail may never arrive at
        // all. That is what left the presenter console's overview blank.
        document::Script { src: crate::logic::pdf::PDF_VIEWER_JS }
        // Only a window of its own gets its own name. Setting the title while
        // the console is a page *inside* the main window renamed the window —
        // or the browser tab — and nothing put the name back when the console
        // was left again, so it kept calling itself the presenter console for
        // the rest of the session. The main window is Cantara throughout; the
        // name is set once, by `App`.
        if !is_main_window {
            document::Title { {t!("presenter.title").to_string()} }
        }

        div {
            class: "presenter-console",
            tabindex: 0,
            onkeydown: move |event: Event<KeyboardData>| {
                let key = event.key();
                match key {
                    Key::ArrowRight | Key::Enter => go_to_next_slide(),
                    Key::Character(ref c) if c == " " => go_to_next_slide(),
                    Key::ArrowLeft => go_to_previous_slide(),
                    Key::Escape => {
                        quit_presentation();
                    }
                    Key::Character(ref c) if c == "b" || c == "B" => {
                        running_presentation.write().toggle_black_screen();
                    }
                    _ => {}
                }
            },

            PresenterHeader { view }

            PresenterContent { running_presentation, view }

            PresenterControlBar {
                running_presentation,
                on_quit: move |_| quit_presentation(),
                on_edit_selection: {
                    if is_main_window {
                        let nav_clone = nav;
                        Some(EventHandler::new(move |_: ()| {
                            if let Some(ref n) = nav_clone {
                                n.push(crate::Route::Selection {});
                            }
                        }))
                    } else {
                        None
                    }
                },
            }
        }
    }
}

/// Status bar at the top of the presenter console with view toggle buttons
#[component]
fn PresenterHeader(view: Signal<PresenterConsoleView>) -> Element {
    let mut settings = use_settings();
    let current_view = *view.read();

    rsx! {
        header { class: "presenter-header",
            h3 { {t!("presenter.status_running").to_string()} }
            div { class: "presenter-view-toggle",
                button {
                    class: if current_view == PresenterConsoleView::Text { "view-toggle-btn active" } else { "view-toggle-btn" },
                    onclick: move |_| {
                        view.set(PresenterConsoleView::Text);
                        settings.write().presenter_console_view = PresenterConsoleView::Text;
                        settings.read().save();
                    },
                    {t!("presenter.view_text").to_string()}
                }
                button {
                    class: if current_view == PresenterConsoleView::Grid { "view-toggle-btn active" } else { "view-toggle-btn" },
                    onclick: move |_| {
                        view.set(PresenterConsoleView::Grid);
                        settings.write().presenter_console_view = PresenterConsoleView::Grid;
                        settings.read().save();
                    },
                    {t!("presenter.view_grid").to_string()}
                }
            }
        }
    }
}

/// Main content area: switches between text+preview layout and grid overview
#[component]
fn PresenterContent(
    running_presentation: Signal<RunningPresentation>,
    view: Signal<PresenterConsoleView>,
) -> Element {
    // The elements of the service, as places to jump to. The same list the
    // dropdown in the control bar offers — but standing open beside the
    // console, where a glance says which one is running rather than a click.
    let rp = running_presentation.read();
    let chapters: Vec<JumpTarget> = rp
        .presentation
        .iter()
        .map(|chapter| JumpTarget {
            label: chapter.source_file.name.clone(),
            id: String::new(),
        })
        .collect();
    let current_chapter = rp.position.as_ref().map(|p| p.chapter());
    drop(rp);

    // Jumping to an element means its first slide — which is what the
    // dropdown does, and what "go to that song" means to a person.
    let jump = move |index: usize| {
        running_presentation.write().jump_to(index, 0);
    };

    let sidebar = rsx! {
        JumpSidebar {
            targets: chapters,
            active: current_chapter,
            title: t!("presenter.chapters").to_string(),
            on_select: jump,
        }
    };

    match *view.read() {
        PresenterConsoleView::Text => rsx! {
            main { class: "presenter-content presenter-content-with-jumps",
                { sidebar }
                PresenterTextPanel { running_presentation }
                PresenterPreviewPanel { running_presentation }
            }
        },
        PresenterConsoleView::Grid => rsx! {
            main { class: "presenter-content presenter-content-grid presenter-content-with-jumps",
                { sidebar }
                PresenterGridPanel { running_presentation }
            }
        },
    }
}

/// Left panel: scrollable chapter list with slide text
#[component]
fn PresenterTextPanel(running_presentation: Signal<RunningPresentation>) -> Element {
    let rp = running_presentation.read();
    let current_chapter = rp.position.as_ref().map(|p| p.chapter()).unwrap_or(0);
    let current_slide = rp.position.as_ref().map(|p| p.chapter_slide()).unwrap_or(0);

    rsx! {
        div { class: "presenter-text-panel",
            for (ch_idx , chapter) in rp.presentation.iter().enumerate() {
                div { class: "presenter-chapter",
                    h4 { class: if ch_idx == current_chapter { "presenter-chapter-title active" } else { "presenter-chapter-title" },
                        {chapter.source_file.name.clone()}
                    }
                    for (sl_idx , slide) in chapter.slides.iter().enumerate() {
                        {
                            let is_active = ch_idx == current_chapter && sl_idx == current_slide;
                            rsx! {
                                div {
                                    // key forces Dioxus to remount when the active slide changes,
                                    // ensuring onmounted fires on the newly-active element.
                                    key: "{ch_idx}-{sl_idx}-{is_active}",
                                    class: if is_active { "presenter-slide-item active" } else { "presenter-slide-item" },
                                    onclick: move |_| {
                                        running_presentation.write().jump_to(ch_idx, sl_idx);
                                    },
                                    onmounted: move |_| {
                                        if is_active {
                                            // Use JS scrollIntoView with block:'center' to
                                            // vertically center the active slide in the panel.
                                            let _ = document::eval(
                                                "requestAnimationFrame(function() { var el = document.querySelector('.presenter-slide-item.active'); if (el) { el.scrollIntoView({ behavior: 'smooth', block: 'center' }); } });",
                                            );
                                        }
                                    },
                                    PresenterSlideTextContent { slide_content: slide.slide_content.clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Grid overview panel: shows all slides as rendered thumbnails grouped by chapter,
/// with a slider to adjust thumbnail size.
#[component]
fn PresenterGridPanel(running_presentation: Signal<RunningPresentation>) -> Element {
    let mut settings = use_settings();
    let mut grid_size: Signal<u32> =
        use_signal(move || settings.read().presenter_console_grid_size);

    let rp = running_presentation.read();
    let current_chapter = rp.position.as_ref().map(|p| p.chapter()).unwrap_or(0);
    let current_slide = rp.position.as_ref().map(|p| p.chapter_slide()).unwrap_or(0);

    let size = *grid_size.read();
    // Columns of exactly the thumbnail's width. They used to stretch to fill
    // the row, which left the slide narrower than the cell it sat in and the
    // scale no longer the one the column was built for.
    let grid_style = format!("grid-template-columns: repeat(auto-fill, {}px);", size);
    // The size the presentation window is actually laid out at — not the
    // monitor's, which is in physical pixels and is two thirds larger on a
    // screen at 150% scaling. Laying the thumbnail out at a different size
    // breaks its text in different places, and then it is no longer a picture
    // of the slide.
    let (native_w, native_h) = rp.layout_size();
    // The slide is laid out at the presentation's size and then scaled down as
    // a whole — see `.slide-scale`. Everything keeps its proportions, which is
    // what makes the thumbnail a picture of the slide rather than the same
    // slide re-laid-out into a smaller page.
    let scale = size as f64 / native_w;
    // The scaled height matches the presentation aspect ratio
    let thumb_height = (size as f64 * native_h / native_w).round() as u32;

    rsx! {
        div { class: "presenter-grid-panel",
            // Size slider
            div { class: "presenter-grid-toolbar",
                label { class: "presenter-grid-size-label", {t!("presenter.grid_size").to_string()} }
                input {
                    r#type: "range",
                    class: "presenter-grid-size-slider",
                    min: "150",
                    max: "500",
                    value: "{size}",
                    oninput: move |evt| {
                        if let Ok(val) = evt.value().parse::<u32>() {
                            grid_size.set(val);
                            settings.write().presenter_console_grid_size = val;
                            settings.read().save();
                        }
                    },
                }
            }
            for (ch_idx , chapter) in rp.presentation.iter().enumerate() {
                {
                    let design = chapter
                        .presentation_design_option
                        .clone()
                        .unwrap_or(PresentationDesign::default());
                    rsx! {
                        div { class: "presenter-grid-chapter",
                            h4 { class: if ch_idx == current_chapter { "presenter-chapter-title active" } else { "presenter-chapter-title" },
                                {chapter.source_file.name.clone()}
                            }
                            div { class: "presenter-grid-slides", style: "{grid_style}",
                                for (sl_idx , slide) in chapter.slides.iter().enumerate() {
                                    {
                                        let is_active = ch_idx == current_chapter && sl_idx == current_slide;
                                        rsx! {
                                            div {
                                                key: "{ch_idx}-{sl_idx}-{is_active}",
                                                class: if is_active { "presenter-grid-slide active" } else { "presenter-grid-slide" },
                                                onclick: move |_| {
                                                    running_presentation.write().jump_to(ch_idx, sl_idx);
                                                },
                                                onmounted: move |_| {
                                                    if is_active {
                                                        let _ = document::eval(
                                                            "requestAnimationFrame(function() { var el = document.querySelector('.presenter-grid-slide.active'); if (el) { el.scrollIntoView({ behavior: 'smooth', block: 'center' }); } });",
                                                        );
                                                    }
                                                },
                                                div {
                                                    class: "presenter-grid-slide-inner slide-scale",
                                                    style: "width: {size}px; height: {thumb_height}px;",
                                                    div {
                                                        class: "slide-scale-inner",
                                                        style: "width: {native_w}px; height: {native_h}px; transform: scale({scale});",
                                                        StaticSlideRendererComponent { slide: slide.clone(), presentation_design: design.clone() }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Extracts and renders per-page PDF text for the presenter console.
///
/// The extraction calls `extract_pdf_page_text` which hits `PDF_PAGE_CACHE` first.
/// Because `refresh_search_cache` pre-populates the page cache when source files are
/// loaded, this is almost always an O(1) hash-map lookup with no file I/O.
/// On a cold cache miss it loads the entire PDF once and caches every page, so
/// subsequent slides of the same document are still instant.
///
/// The caller must supply `key: "{path}#{page_number}"` so Dioxus destroys and
/// re-creates this component (fresh extraction state) on every slide change.
#[component]
fn PdfPageTextContent(path: String, page_number: u32, page_info: String) -> Element {
    #[cfg(not(target_arch = "wasm32"))]
    let text = crate::logic::search::extract_pdf_page_text(
        std::path::Path::new(&path),
        page_number,
    );

    #[cfg(target_arch = "wasm32")]
    let text = crate::logic::settings::RepositoryType::web_read_file(&path)
        .and_then(|bytes| {
            crate::logic::search::extract_pdf_page_text_from_bytes(
                &bytes,
                page_number,
                &path,
            )
        });

    rsx! {
        div { class: "slide-text-content",
            em { "📄 {t!(\"general.pdf\")}{page_info}" }
            if let Some(text) = text {
                if !text.trim().is_empty() {
                    p { {text.trim().to_string()} }
                }
            }
        }
    }
}

/// The rows of a complex slide that a moderator can actually read.
///
/// Notation rows are dropped — their content is ABC source. Rows flagged
/// `redundant` are **kept**: they repeat the words printed under the notes, and
/// on a notation slide that is the only place the text appears as text.
fn readable_rows(rows: &[SlideRow]) -> Vec<String> {
    rows.iter()
        .filter(|row| !row.is_notation())
        .map(|row| row.content.clone())
        .filter(|content| !content.trim().is_empty())
        .collect()
}

/// Extracts and renders text from a slide for the presenter console text panel
#[component]
fn PresenterSlideTextContent(slide_content: SlideContent) -> Element {
    match slide_content {
        SlideContent::Title(title_slide) => {
            rsx! {
                div { class: "slide-text-title",
                    strong { {title_slide.title_text} }
                }
            }
        }
        SlideContent::SingleLanguageMainContent(main_slide) => {
            let text = main_slide.clone().main_text();
            if let Some(html) = get_markdown_html(&text) {
                let plain = html_to_plain_text(html);
                rsx! {
                    div { class: "slide-text-content",
                        p { {plain} }
                    }
                }
            } else {
                rsx! {
                    div { class: "slide-text-content",
                        p { {main_slide.clone().main_text()} }
                        if let Some(spoiler) = main_slide.spoiler_text() {
                            p { class: "slide-text-spoiler", {spoiler} }
                        }
                    }
                }
            }
        }
        SlideContent::Empty(_) => {
            rsx! {
                div { class: "slide-text-empty",
                    em { {t!("presenter.empty_slide").to_string()} }
                }
            }
        }
        SlideContent::SimplePicture(picture_slide) => {
            let path = get_picture_path(&picture_slide);
            let base_path = path.split('#').next().unwrap_or(&path);
            let is_pdf = base_path.to_lowercase().ends_with(".pdf");
            if is_pdf {
                // Extract page number from fragment (e.g. #page=2)
                let page_number: u32 = path.split("#page=").nth(1)
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(1);
                let page_info = format!(" ({})", t!("general.pdf_page", page => page_number));

                // Render the async sub-component; keyed so it re-creates on slide change.
                rsx! {
                    PdfPageTextContent {
                        key: "{base_path}#{page_number}",
                        path: base_path.to_string(),
                        page_number,
                        page_info,
                    }
                }
            } else {
                rsx! {
                    div { class: "slide-text-content",
                        em { "📄 {t!(\"general.picture\")}" }
                    }
                }
            }
        }
        // The words the congregation sees, whatever the layout puts them in.
        // The notation row is left out — its content is ABC source, not
        // something a moderator can read — but the lyrics row that repeats what
        // the notes carry is kept, because on a notation slide it is the only
        // place the text appears in readable form.
        SlideContent::Complex(complex_slide) => {
            let lines = readable_rows(&complex_slide.rows);
            let spoiler = readable_rows(&complex_slide.spoiler);

            rsx! {
                div { class: "slide-text-content",
                    for line in lines {
                        p { {line} }
                    }
                    for line in spoiler {
                        p { class: "slide-text-spoiler", {line} }
                    }
                }
            }
        }
        // A PDF page reached the console as "..." while the wildcard arm was
        // still here; it gets the same text extraction as a PDF picture slide.
        SlideContent::PdfPage(pdf_slide) => {
            let page_number = pdf_slide.page_number;
            let path = pdf_slide.pdf_path.clone();
            let page_info = format!(" ({})", t!("general.pdf_page", page => page_number));

            rsx! {
                PdfPageTextContent {
                    key: "{path}#{page_number}",
                    path,
                    page_number,
                    page_info,
                }
            }
        }
        SlideContent::MultiLanguageMainContent(multi_slide) => {
            rsx! {
                div { class: "slide-text-content",
                    for text in multi_slide.main_text_list.clone() {
                        p { {text} }
                    }
                    for text in multi_slide.spoiler_text_vector.clone() {
                        p { class: "slide-text-spoiler", {text} }
                    }
                }
            }
        }
    }
}

/// Right panel: live preview of the current slide using PresentationRendererComponent directly.
/// This uses the actual signal so that clicks inside the preview (next/previous slide)
/// are synced back to the shared running presentation state.
#[component]
fn PresenterPreviewPanel(running_presentation: Signal<RunningPresentation>) -> Element {
    let rp = running_presentation.read();
    let (native_w, native_h) = rp.layout_size();
    // The slide keeps the presentation's own layout and is scaled as a whole
    // — see `.slide-scale`. That is what makes the preview show the slide the
    // audience is looking at, down to where the text breaks, and what lets a
    // scroll position taken from one mean the same in the other.
    const PREVIEW_WIDTH: f64 = 480.0;
    let scale = PREVIEW_WIDTH / native_w;
    let preview_height = (PREVIEW_WIDTH * native_h / native_w).round();

    let timer_seconds = rp.get_current_timer_settings().map(|t| t.timer_seconds);
    let current_slide = rp.position.as_ref().map(|p| p.slide_total()).unwrap_or(0);
    let total_slides = rp.total_slides();

    rsx! {
        div { class: "presenter-preview-panel",
            h4 { {t!("presenter.preview").to_string()} }
            div {
                class: "presentation-preview slide-scale",
                style: "width: {PREVIEW_WIDTH}px; height: {preview_height}px; border-radius: 4px;",
                div {
                    class: "slide-scale-inner",
                    style: "width: {native_w}px; height: {native_h}px; transform: scale({scale});",
                    PresentationRendererComponent { running_presentation, role: PresentationRole::Follower }
                }
                // The timer and the counter belong to the console, not to the
                // slide, so they sit outside the scaled box and are read at
                // their own size. Inside it they were shrunk along with
                // everything else — a counter set in twenty pixels arrived on
                // screen in five.
                if let Some(seconds) = timer_seconds {
                    div {
                        key: "{current_slide}",
                        style: format!(
                            "position: absolute; bottom: 0; left: 0; height: 6px; width: 100%; background: rgba(255, 255, 255, 0.7); z-index: 100; animation: countdownBar {}s linear forwards;",
                            seconds,
                        ),
                    }
                }
                div { style: "position: absolute; bottom: 8px; right: 8px; background: rgba(0, 0, 0, 0.6); color: white; padding: 2px 8px; border-radius: 4px; font-size: 0.9rem; z-index: 100;",
                    {format!("{} / {}", current_slide + 1, total_slides)}
                }
            }

            StreamPreview { running_presentation }
        }
    }
}

/// What the congregation's phones are showing, where that is not what the wall
/// is showing.
///
/// A moderator can see the projection over their shoulder; the stream they
/// cannot see at all. Once a service gives the phones a design or a slide
/// division of their own, the second half of what the room is looking at is
/// invisible from the console — including, and this is the one that matters,
/// *which* slide it is on, since a wall going two lines at a time and phones
/// going four do not change together.
///
/// Nothing at all where the two agree, which is the ordinary case: a second
/// picture of the same slide is clutter beside the first.
#[component]
fn StreamPreview(running_presentation: Signal<RunningPresentation>) -> Element {
    let rp = running_presentation.read();
    if !rp.current_stream_differs() {
        return rsx! {};
    }

    let Some(slide) = rp.get_current_stream_slide() else {
        return rsx! {};
    };
    let design = rp.get_current_stream_design();

    // Laid out at the projection's size and scaled down, exactly as the
    // preview above it is. The two then sit one under the other at the same
    // size and can be read against each other, which is the whole point of
    // showing the second one — and a design's sizes are stated for the
    // presentation's geometry, so laying it out at anything else would be a
    // picture of a design nobody configured.
    //
    // Not a picture of the phone's own page: a viewer's browser reflows the
    // words to whatever screen it is on, and there is no one shape to draw.
    // What can be shown, and what a moderator needs, is *which slide* the
    // phones are on and what design it is wearing.
    let (native_w, native_h) = rp.layout_size();
    const PREVIEW_WIDTH: f64 = 480.0;
    let scale = PREVIEW_WIDTH / native_w;
    let preview_height = (PREVIEW_WIDTH * native_h / native_w).round();

    // Which slide the phones are on, counted in their own division — the whole
    // point of showing this is that it is not the projection's number.
    let position = rp.stream_position();
    let counter = position.and_then(|(chapter, slide)| {
        let chapter = rp.presentation.get(chapter)?;
        Some(format!("{} / {}", slide + 1, chapter.slides_for_stream().len()))
    });

    rsx! {
        h4 { style: "margin-top: 1rem;", {t!("presenter.stream_preview").to_string()} }
        div {
            class: "presentation-preview slide-scale",
            style: "width: {PREVIEW_WIDTH}px; height: {preview_height}px; border-radius: 4px;",
            div {
                class: "slide-scale-inner",
                style: "width: {native_w}px; height: {native_h}px; transform: scale({scale});",
                StaticSlideRendererComponent { slide, presentation_design: design }
            }
            if let Some(counter) = counter {
                div { style: "position: absolute; bottom: 8px; right: 8px; background: rgba(0, 0, 0, 0.6); color: white; padding: 2px 8px; border-radius: 4px; font-size: 0.9rem; z-index: 100;",
                    { counter }
                }
            }
        }
    }
}

/// Bottom control bar with navigation buttons, chapter jump dropdown, and black screen toggle
#[component]
fn PresenterControlBar(
    running_presentation: Signal<RunningPresentation>,
    on_quit: EventHandler<()>,
    #[props(default)]
    on_edit_selection: Option<EventHandler<()>>,
) -> Element {
    let rp = running_presentation.read();
    let current_total = rp
        .position
        .as_ref()
        .map(|p| p.slide_total() + 1)
        .unwrap_or(0);
    let total_slides = rp.total_slides();
    let is_black = rp.is_black_screen;
    let current_chapter = rp.position.as_ref().map(|p| p.chapter()).unwrap_or(0);
    let chapters: Vec<(usize, String)> = rp
        .presentation
        .iter()
        .enumerate()
        .map(|(i, ch)| (i, ch.source_file.name.clone()))
        .collect();

    rsx! {
        footer { class: "presenter-control-bar",
            div { class: "presenter-controls",
                button {
                    class: "secondary",
                    onclick: move |_| {
                        running_presentation.write().previous_slide();
                    },
                    {t!("presenter.previous").to_string()}
                }
                span { class: "slide-counter", {format!("{} / {}", current_total, total_slides)} }
                button {
                    class: "secondary",
                    onclick: move |_| {
                        running_presentation.write().next_slide();
                    },
                    {t!("presenter.next").to_string()}
                }
                // Chapter jump dropdown
                select {
                    class: "chapter-select",
                    onchange: move |evt| {
                        if let Ok(idx) = evt.value().parse::<usize>() {
                            running_presentation.write().jump_to(idx, 0);
                        }
                    },
                    for (idx , name) in chapters.iter() {
                        option { value: "{idx}", selected: *idx == current_chapter, {name.clone()} }
                    }
                }
                button {
                    class: if is_black { "contrast" } else { "outline secondary" },
                    onclick: move |_| {
                        running_presentation.write().toggle_black_screen();
                    },
                    {t!("presenter.black_screen").to_string()}
                }
                if let Some(ref handler) = on_edit_selection {
                    button {
                        class: "outline secondary",
                        onclick: {
                            let handler = *handler;
                            move |_| {
                                handler.call(());
                            }
                        },
                        {t!("presenter.edit_selection").to_string()}
                    }
                }
                button {
                    class: "outline secondary",
                    onclick: move |_| {
                        on_quit.call(());
                    },
                    {t!("presenter.quit").to_string()}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this guards against: a notation slide showed nothing but "..."
    /// in the console, because the lyrics row that carries the words under the
    /// notes is flagged redundant and was being filtered out.
    #[test]
    fn test_redundant_rows_are_still_readable() {
        let rows = vec![
            SlideRow::notation("X:1\nK:C\nCDEF|", 4),
            SlideRow::lyrics(Some("de".to_string()), "Sei nicht stolz auf das, was du bist")
                .also_shown_in_notation(),
        ];

        let lines = readable_rows(&rows);

        assert_eq!(lines, vec!["Sei nicht stolz auf das, was du bist"]);
    }

    /// ABC source is not something a moderator can read off a screen.
    #[test]
    fn test_notation_source_never_reaches_the_console() {
        let rows = vec![
            SlideRow::notation("X:1\nM:4/4\nK:G\nGABc|", 4),
            SlideRow::lyrics(Some("en".to_string()), "Amazing grace"),
        ];

        for line in readable_rows(&rows) {
            assert!(!line.contains("X:1"), "the ABC header leaked through");
            assert!(!line.contains("K:G"), "the ABC key leaked through");
        }
    }

    /// Every language stays, in the order the user asked for.
    #[test]
    fn test_every_language_is_listed_in_order() {
        let rows = vec![
            SlideRow::lyrics(Some("en".to_string()), "Amazing grace"),
            SlideRow::lyrics(Some("de".to_string()), "Erstaunliche Gnade"),
        ];

        assert_eq!(
            readable_rows(&rows),
            vec!["Amazing grace", "Erstaunliche Gnade"]
        );
    }

    #[test]
    fn test_blank_rows_are_dropped() {
        let rows = vec![
            SlideRow::lyrics(None, "   "),
            SlideRow::lyrics(None, "Sing to the Lord"),
        ];

        assert_eq!(readable_rows(&rows), vec!["Sing to the Lord"]);
    }
}
