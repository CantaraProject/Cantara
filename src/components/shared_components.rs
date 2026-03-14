//! Shared components reusable across different parts of the program.

use crate::components::presentation_components::PresentationRendererComponent;
use crate::logic::presentation::{create_amazing_grace_presentation, create_single_item_presentation};
use crate::logic::settings::{CssSize, PresentationDesign};
use crate::logic::states::{RunningPresentation, SelectedItemRepresentation};
use cantara_songlib::slides::SlideSettings;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::{FaFilePdf, FaFileCode, FaImage, FaMusic, FaPenToSquare};

#[component]
pub fn DeleteIcon() -> Element {
    rsx! { Icon { icon: FaTrashCan } }
}

#[component]
pub fn EditIcon() -> Element {
    rsx! { Icon { icon: FaPenToSquare } }
}

#[component]
pub fn MusicIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaMusic, width: width.unwrap_or(20) } }
}

#[component]
pub fn ImageIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaImage, width: width.unwrap_or(20) } }
}

#[component]
pub fn PdfIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaFilePdf, width: width.unwrap_or(20) } }
}

#[component]
pub fn MarkdownIcon(width: Option<u32>) -> Element {
    rsx! { Icon { icon: FaFileCode, width: width.unwrap_or(20) } }
}

/// A component displaying multiple presentation designs in an "Amazing Grace" presentation.
#[component]
pub fn PresentationDesignSelector(
    presentation_designs: Signal<Vec<PresentationDesign>>,
    song_slide_settings: Option<SlideSettings>,
    viewer_width: usize,
    active_item: Signal<Option<usize>>,
) -> Element {
    let song_slide_settings = use_signal(|| song_slide_settings.unwrap_or_default());

    rsx! {
        div {
            class: "presentation-design-selector",
            for (index, design) in presentation_designs.read().iter().enumerate() {
                span {
                    class: format!("presentation-design-selector-item {}", if active_item() == Some(index) { "active" } else { "" }),
                    tabindex: index,
                    key: "{index}",
                    SelectablePresentationViewer {
                        presentation: create_amazing_grace_presentation(design, &song_slide_settings()),
                        width: viewer_width,
                        title: design.name.clone(),
                        index,
                        current_selection: active_item
                    }
                }
            }
        }
    }
}

/// A wrapper component around PresentationViewer that allows selecting it.
#[component]
fn SelectablePresentationViewer(
    presentation: RunningPresentation,
    width: usize,
    title: String,
    index: usize,
    current_selection: Signal<Option<usize>>,
) -> Element {
    rsx! {
        PresentationViewer {
            presentation,
            width,
            title: Some(title),
            selected: Some(index == current_selection().unwrap_or(usize::MAX)),
            onclick: move |_| {
                tracing::debug!("Selected Presentation: {}", index);
                current_selection.set(Some(index));
            }
        }
    }
}

#[component]
pub fn PresentationViewer(
    presentation: RunningPresentation,
    width: usize,
    title: Option<String>,
    selected: Option<bool>,
    onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    // Render at native presentation resolution and scale down to desired width
    let (native_w, native_h) = presentation.presentation_resolution;
    let zoom_factor = width as f64 / native_w as f64;
    let zoom_css = format!("zoom: {};", zoom_factor);
    let css_class = selected.map_or("rounded-corners-inactive", |s| {
        if s {
            "rounded-corners-active"
        } else {
            "rounded-corners-inactive"
        }
    });

    let mut presentation_signal = use_signal(|| presentation.clone());
    if *presentation_signal.peek() != presentation {
        presentation_signal.set(presentation.clone());
    }

    rsx! {
        div {
            class: format!("{} presentation-preview inline-div", css_class),
            style: format!("position: relative; width: {}px; height: {}px; {}", native_w, native_h, zoom_css),
            onclick: move |event| if let Some(onclick_event) = onclick { onclick_event.call(event) },
            PresentationRendererComponent {
                running_presentation: presentation_signal,
                fire_timer: false,
            }
            if let Some(title) = title {
                div {
                    class: "presentation-title",
                    style: "position: absolute; top: 0; right: 0; display: flex; align-items: center; justify-content: center; font-size: 30pt; background-color: black; color: white; z-index: 99;",
                    { title }
                }
            }
        }
    }
}

/// Displays an example presentation in 16:9 format scaled to a fixed width.
#[component]
pub fn ExamplePresentationViewer(
    presentation_design: PresentationDesign,
    song_slide_settings: Option<Signal<SlideSettings>>,
    width: usize,
    increase_font_size_in_percent: Option<usize>,
) -> Element {
    let presentation = create_amazing_grace_presentation(
        &presentation_design,
        &song_slide_settings.map_or(SlideSettings::default(), |s| s()),
    );

    rsx! {
        PresentationViewer {
            presentation,
            width,
        }
    }
}

/// Displays a live preview of the currently selected item with its actual slides,
/// transition effects, and countdown timer bar. Click advances to the next slide.
#[component]
pub fn SelectedItemPreview(
    selected_item: SelectedItemRepresentation,
    default_presentation_design: PresentationDesign,
    default_slide_settings: SlideSettings,
    width: usize,
) -> Element {
    let timer_seconds = selected_item
        .timer_settings_option
        .as_ref()
        .map(|t| t.timer_seconds);

    let presentation = create_single_item_presentation(
        &selected_item,
        &default_presentation_design,
        &default_slide_settings,
    );

    let mut presentation_signal = use_signal(|| presentation.clone());
    // Only reset when slide content/settings change, not when position changes due to clicks
    if presentation_signal.peek().presentation != presentation.presentation {
        presentation_signal.set(presentation.clone());
    }

    let current_slide_number = use_memo(move || {
        presentation_signal
            .read()
            .position
            .as_ref()
            .map(|p| p.slide_total())
            .unwrap_or(0)
    });

    let total_slides = use_memo(move || presentation_signal.read().total_slides());

    // Render at native presentation resolution and scale down to desired width
    let (native_w, native_h) = presentation.presentation_resolution;
    let zoom_factor = width as f64 / native_w as f64;
    let zoom_css = format!("zoom: {};", zoom_factor);

    rsx! {
        div {
            class: "presentation-preview",
            style: format!(
                "position: relative; width: {}px; height: {}px; cursor: pointer; overflow: hidden; border-radius: 8px; {}",
                native_w, native_h, zoom_css
            ),
            PresentationRendererComponent {
                running_presentation: presentation_signal,
                fire_timer: true,
            }
            // Countdown timer bar at the bottom
            if let Some(seconds) = timer_seconds {
                div {
                    key: "{current_slide_number()}",
                    style: format!(
                        "position: absolute; bottom: 0; left: 0; height: 6px; width: 100%; background: rgba(255, 255, 255, 0.7); z-index: 100; animation: countdownBar {}s linear forwards;",
                        seconds
                    ),
                }
            }
            // Slide counter overlay
            div {
                style: "position: absolute; bottom: 8px; right: 8px; background: rgba(0, 0, 0, 0.6); color: white; padding: 2px 8px; border-radius: 4px; font-size: 20px; z-index: 100;",
                { format!("{} / {}", current_slide_number() + 1, total_slides()) }
            }
        }
    }
}

/// Generates JavaScript for a yes/no dialog box.
pub fn js_yes_no_box(prompt: String) -> String {
    format!("return confirm('{}');", prompt)
}

#[component]
pub fn NumberedValidatedLengthInput(
    value: CssSize,
    placeholder: String,
    onchange: EventHandler<CssSize>,
) -> Element {
    let mut value_signal = use_signal(|| value);
    rsx! {
        input {
            placeholder,
            value: value_signal.read().get_float(),
            inputmode: "numeric",
            onchange: move |event| {
                value_signal.write().set_float(event.value().parse().unwrap_or(0.0));
                onchange.call(value_signal());
            }
        }
        select {
            name: "unit",
            required: true,
            onchange: move |event: Event<FormData>| {
                match event.value().as_str() {
                    "px" => value_signal.set(CssSize::Px(value_signal().get_float())),
                    "pt" => value_signal.set(CssSize::Pt(value_signal().get_float())),
                    "em" => value_signal.set(CssSize::Em(value_signal().get_float())),
                    "%"  => value_signal.set(CssSize::Percentage(value_signal().get_float())),
                    _    => value_signal.set(CssSize::Px(value_signal().get_float()))
                };
                onchange.call(value_signal());
            },
            option {
                key: "px",
                selected: matches!(value_signal(), CssSize::Px(_)) || value_signal() == CssSize::Null,
                "px"
            }
            option {
                key: "pt",
                selected: matches!(value_signal(), CssSize::Pt(_)),
                "pt"
            }
            option {
                key: "em",
                selected: matches!(value_signal(), CssSize::Em(_)),
                "em"
            }
            option {
                key: "%",
                selected: matches!(value_signal(), CssSize::Percentage(_)),
                "%"
            }
        }
    }
}
