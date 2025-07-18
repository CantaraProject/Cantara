//! Shared components reusable across different parts of the program.

use crate::components::presentation_components::PresentationRendererComponent;
use crate::logic::presentation::create_amazing_grace_presentation;
use crate::logic::settings::{CssSize, PresentationDesign};
use crate::logic::states::RunningPresentation;
use cantara_songlib::slides::SlideSettings;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::{FaImage, FaMusic, FaPenToSquare};

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
                    key: index,
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
    let scale_percentage = ((width as f64 / 1024.0) * 100.0).round();
    let zoom_css = format!("zoom: {}%;", scale_percentage);
    let css_class = selected.map_or("rounded-corners-inactive", |s| {
        if s {
            "rounded-corners-active"
        } else {
            "rounded-corners-inactive"
        }
    });

    rsx! {
        div {
            class: format!("{} presentation-preview inline-div", css_class),
            style: format!("position: relative; width: 1024px; height: 576px; {}", zoom_css),
            onclick: move |event| if let Some(onclick_event) = onclick { onclick_event.call(event) },
            PresentationRendererComponent {
                running_presentation: use_signal(|| presentation)
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
