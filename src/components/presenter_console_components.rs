//! This module contains the components for the Presenter Console window.
//! The presenter console shows the current slide text, a live preview, and navigation controls.

use crate::logic::states::RunningPresentation;
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

    let mut go_to_next_slide = move || {
        running_presentation.write().next_slide();
    };

    let mut go_to_previous_slide = move || {
        running_presentation.write().previous_slide();
    };

    let mut quit_presentation = move || {
        running_presentations.write().clear();
        if is_main_window {
            nav.replace(crate::Route::Selection {});
        } else {
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
                match event.key() {
                    Key::ArrowRight => go_to_next_slide(),
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
                                    class: if is_active { "presenter-slide-item active" } else { "presenter-slide-item" },
                                    id: if is_active { "active-slide" },
                                    onclick: move |_| {
                                        running_presentation.write().jump_to(ch_idx, sl_idx);
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

/// Bottom control bar with navigation buttons and black screen toggle
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
