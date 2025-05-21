//! This submodule contains shared components which can be reused among different parts of the program.

use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::FaPenToSquare;
use rust_i18n::t;

use crate::logic::presentation::create_amazing_grace_presentation;
use crate::logic::settings::{PresentationDesign, PresentationDesignSettings};
use crate::logic::states::RunningPresentation;
use crate::presentation_components::PresentationRendererComponent;

#[component]
pub fn DeleteIcon() -> Element {
    rsx! {
        Icon {
            icon: FaTrashCan,
        }
    }
}

#[component]
pub fn EditIcon() -> Element {
    rsx! {
        Icon {
            icon: FaPenToSquare,
        }
    }
}

/// A component which displays multiple presentation designs in an "Amazing Grace" presentation and allows to select one
#[component]
pub fn PresentationDesignSelecter(
    presentation_designs: Signal<Vec<PresentationDesign>>,
    default_selection: Option<usize>,
    viewer_width: usize,
    active_item: Signal<Option<usize>>,
) -> Element {
    let mut presentations: Signal<Vec<Signal<RunningPresentation>>> = use_signal(|| vec![]);

    use_effect(move || {
        for design in presentation_designs() {
            let presentation = use_signal(|| create_amazing_grace_presentation(&design));
            presentations.push(presentation);
        }
    });
    rsx! {
        div {
            class: "presentation-design-selecter",

            for (number, presentation) in presentations().iter().enumerate() {
                span {
                    class: format!("presentation-design-selecter-item {}", match active_item() {
                        Some(active_item) => if active_item == number { "active" } else { "" },
                        None => "",
                    }),
                    key: number,
                    onclick: move |_| active_item.set(Some(number)),
                    PresentationViewer {
                        presentation_signal: *presentation,
                        width: viewer_width,
                        title: presentation().get_current_presentation_design().clone().name
                    }
                }
            }
        }
    }
}

#[component]
pub fn PresentationViewer(
    presentation_signal: Signal<RunningPresentation>,
    width: usize,
    title: Option<String>,
) -> Element {
    let scale_percentage = ((width as f64 / 1024 as f64) * 100.0).round();
    let zoom_css_string = format!("zoom: {}%;", scale_percentage.to_string());

    rsx! {
        div {
            class: "rounded-corners presentation-preview inline-div",
            style: format!("{}{}", "position: relative;width:1024px;height:576px;", zoom_css_string),

            PresentationRendererComponent {
                running_presentation: presentation_signal
            }

            if let Some(title) = title {
                div {
                    class: "presentation-title",
                    style: "zoom:100%!important;position: absolute;top: 0;right: 0;display: flex;align-items: center;justify-content: center;font-size: 30pt;background-color:black;color:white;",
                    { title }
                }
            }
        }
    }
}

/// Provides an Example Presentation Viewer in 16:9 format scailed down to a fixed with
#[component]
pub fn ExamplePresentationViewer(
    presentation_design: PresentationDesign,
    width: usize,
    increase_font_size_in_percent: Option<usize>,
) -> Element {
    let presentation_signal =
        use_signal(|| create_amazing_grace_presentation(&presentation_design));

    rsx! {
        PresentationViewer {
            presentation_signal: presentation_signal,
            width: width,
        }
    }
}
