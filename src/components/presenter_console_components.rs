//! This module contains the components for the Presenter Console window.
//! The presenter console shows the current slide text, a live preview, and navigation controls.

use crate::logic::presentation::get_picture_path;
use crate::logic::states::RunningPresentation;
#[cfg(target_arch = "wasm32")]
use crate::logic::sync::{
    SYNC_KEY_ACTIVE, SYNC_KEY_POSITION, SYNC_KEY_POSITION_FROM_CONSOLE, SYNC_KEY_PRESENTATION,
    SYNC_KEY_QUIT,
};
use crate::MAIN_CSS;
use cantara_songlib::slides::SlideContent;
use dioxus::prelude::*;
use rust_i18n::t;

use super::shared_components::PresentationViewer;

const PRESENTER_CONSOLE_CSS: Asset = asset!("/assets/presenter_console.css");

rust_i18n::i18n!("locales", fallback = "en");

/// The entry point for the presenter console window.
/// Works both as a routed page in the main window and as a standalone window
/// (via `with_root_context`).
#[component]
pub fn PresenterConsolePage() -> Element {
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();
    let nav = navigator();

    // Detect whether we are hosted in the main window (router available with known routes)
    // vs. a separate window. In the main window the presenter console is reached via a route,
    // so we can navigate back. In a separate window we close it.
    let is_main_window = nav.can_go_back();

    let mut running_presentation: Signal<RunningPresentation> =
        use_signal(move || running_presentations.get(0).unwrap().clone());

    // Sync changes from the shared signal into the local signal
    use_effect(move || {
        let current = running_presentations.read();
        if let Some(rp) = current.first() {
            if *rp != *running_presentation.peek() {
                running_presentation.set(rp.clone());
            }
        }
    });

    // Sync changes from presenter console back to the shared signal
    use_effect(move || {
        let local = running_presentation.read().clone();
        let mut shared = running_presentations.write();
        if let Some(first) = shared.first_mut() {
            if *first != local {
                *first = local;
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
        let mut last_sync_json = use_signal(|| String::new());
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
            nav.replace(crate::Route::Selection {});
        } else {
            #[cfg(feature = "desktop")]
            dioxus::desktop::window().close();
        }
    };

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: PRESENTER_CONSOLE_CSS }
        document::Title { { t!("presenter.title").to_string() } }

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

            PresenterHeader {}

            PresenterContent {
                running_presentation: running_presentation
            }

            PresenterControlBar {
                running_presentation: running_presentation,
                on_quit: move |_| quit_presentation()
            }
        }
    }
}

/// Status bar at the top of the presenter console
#[component]
fn PresenterHeader() -> Element {
    rsx! {
        header {
            class: "presenter-header",
            h3 { { t!("presenter.status_running").to_string() } }
        }
    }
}

/// Main content area with text panel (left) and preview panel (right)
#[component]
fn PresenterContent(running_presentation: Signal<RunningPresentation>) -> Element {
    rsx! {
        main {
            class: "presenter-content",
            PresenterTextPanel {
                running_presentation: running_presentation
            }
            PresenterPreviewPanel {
                running_presentation: running_presentation
            }
        }
    }
}

/// Left panel: scrollable chapter list with slide text
#[component]
fn PresenterTextPanel(running_presentation: Signal<RunningPresentation>) -> Element {
    let rp = running_presentation.read();
    let current_chapter = rp.position.as_ref().map(|p| p.chapter()).unwrap_or(0);
    let current_slide = rp.position.as_ref().map(|p| p.chapter_slide()).unwrap_or(0);

    rsx! {
        div {
            class: "presenter-text-panel",
            for (ch_idx, chapter) in rp.presentation.iter().enumerate() {
                div {
                    class: "presenter-chapter",
                    h4 {
                        class: if ch_idx == current_chapter { "presenter-chapter-title active" } else { "presenter-chapter-title" },
                        { chapter.source_file.name.clone() }
                    }
                    for (sl_idx, slide) in chapter.slides.iter().enumerate() {
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
                                                "requestAnimationFrame(function() { var el = document.querySelector('.presenter-slide-item.active'); if (el) { el.scrollIntoView({ behavior: 'smooth', block: 'center' }); } });"
                                            );
                                        }
                                    },
                                    PresenterSlideTextContent {
                                        slide_content: slide.slide_content.clone()
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

/// Extracts and renders text from a slide for the presenter console text panel
#[component]
fn PresenterSlideTextContent(slide_content: SlideContent) -> Element {
    match slide_content {
        SlideContent::Title(title_slide) => {
            rsx! {
                div {
                    class: "slide-text-title",
                    strong { { title_slide.title_text } }
                }
            }
        }
        SlideContent::SingleLanguageMainContent(main_slide) => {
            rsx! {
                div {
                    class: "slide-text-content",
                    p { { main_slide.clone().main_text() } }
                    if let Some(spoiler) = main_slide.spoiler_text() {
                        p {
                            class: "slide-text-spoiler",
                            { spoiler }
                        }
                    }
                }
            }
        }
        SlideContent::Empty(_) => {
            rsx! {
                div {
                    class: "slide-text-empty",
                    em { { t!("presenter.empty_slide").to_string() } }
                }
            }
        }
        SlideContent::SimplePicture(picture_slide) => {
            let path = get_picture_path(&picture_slide);
            let base_path = path.split('#').next().unwrap_or(&path);
            let is_pdf = base_path.to_lowercase().ends_with(".pdf");
            let label = if is_pdf {
                // Extract page number from fragment (e.g. #page=2)
                let page_info = path.split("#page=").nth(1)
                    .map(|p| format!(" ({})", t!("general.pdf_page", page => p)))
                    .unwrap_or_default();
                format!("{}{}", t!("general.pdf"), page_info)
            } else {
                t!("general.picture").to_string()
            };
            rsx! {
                div {
                    class: "slide-text-content",
                    em { "📄 {label}" }
                }
            }
        }
        _ => {
            rsx! {
                div {
                    class: "slide-text-unknown",
                    em { "..." }
                }
            }
        }
    }
}

/// Right panel: live preview of the current slide using the existing PresentationViewer
#[component]
fn PresenterPreviewPanel(running_presentation: Signal<RunningPresentation>) -> Element {
    rsx! {
        div {
            class: "presenter-preview-panel",
            h4 { { t!("presenter.preview").to_string() } }
            PresentationViewer {
                presentation: running_presentation.read().clone(),
                width: 480,
            }
        }
    }
}

/// Bottom control bar with navigation buttons, chapter jump dropdown, and black screen toggle
#[component]
fn PresenterControlBar(
    running_presentation: Signal<RunningPresentation>,
    on_quit: EventHandler<()>,
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
        footer {
            class: "presenter-control-bar",
            div {
                class: "presenter-controls",
                button {
                    class: "secondary",
                    onclick: move |_| {
                        running_presentation.write().previous_slide();
                    },
                    { t!("presenter.previous").to_string() }
                }
                span {
                    class: "slide-counter",
                    { format!("{} / {}", current_total, total_slides) }
                }
                button {
                    class: "secondary",
                    onclick: move |_| {
                        running_presentation.write().next_slide();
                    },
                    { t!("presenter.next").to_string() }
                }
                // Chapter jump dropdown
                select {
                    class: "chapter-select",
                    onchange: move |evt| {
                        if let Ok(idx) = evt.value().parse::<usize>() {
                            running_presentation.write().jump_to(idx, 0);
                        }
                    },
                    for (idx, name) in chapters.iter() {
                        option {
                            value: "{idx}",
                            selected: *idx == current_chapter,
                            { name.clone() }
                        }
                    }
                }
                button {
                    class: if is_black { "contrast" } else { "outline secondary" },
                    onclick: move |_| {
                        running_presentation.write().toggle_black_screen();
                    },
                    { t!("presenter.black_screen").to_string() }
                }
                button {
                    class: "outline secondary",
                    onclick: move |_| {
                        on_quit.call(());
                    },
                    { t!("presenter.quit").to_string() }
                }
            }
        }
    }
}
