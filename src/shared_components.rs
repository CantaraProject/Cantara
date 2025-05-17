//! This submodule contains shared components which can be reused among different parts of the program.

use dioxus::prelude::*;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::FaPenToSquare;
use dioxus_free_icons::Icon;
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

#[component]
pub fn PresentationDesignSelecter(
    presentation_designs: Signal<PresentationDesign>,
    default_selection: Option<usize>,
    viewer_width: usize,
    on_change: EventHandler<usize>,
) -> Element {
    rsx! {
        div {
            class: "presentation-design-selecter",
        }
    }

    // TODO: Implement the viewer component
}

#[component]
pub fn PresentationViewer (
    presentation_signal: Signal<RunningPresentation>,
    presentation_design: PresentationDesign,
    width: usize,
    increase_font_size_in_percent: Option<usize>,
) -> Element {
    let scale_percentage = ((width as f64 / 1024 as f64) * 100.0).round();
    let zoom_css_string = format!("zoom: {}%;", scale_percentage.to_string());

    let presentation_design = match increase_font_size_in_percent {
        None => presentation_design,
        Some(factor) => {
            let mut factored_presentation_design = presentation_design.clone();
            if let PresentationDesignSettings::Template(presentation_design_template) =
                factored_presentation_design.presentation_design_settings
            {
                let mut factored_presentation_design_template =
                    presentation_design_template.clone();
                for mut font in factored_presentation_design_template.main_content_fonts {
                    font.font_size *= factor;
                }
                factored_presentation_design.presentation_design_settings =
                    PresentationDesignSettings::Template(presentation_design_template);
            }
            factored_presentation_design
        }
    };

    rsx! {
        div {
            class: "rounded-corners presentation-preview",
            style: format!("{}{}", "position: relative;width:1024px;height:576px;", zoom_css_string),

            PresentationRendererComponent {
                running_presentation: presentation_signal
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
            presentation_design: presentation_design,
            width: width,
            increase_font_size_in_percent: increase_font_size_in_percent,
        }
    }
}
