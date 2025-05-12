//! This module provides functionality for rendering the slides in HTML for the presentation

use cantara_songlib::slides::*;
use dioxus::prelude::*;

use crate::logic::settings::{
    PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate,
};

#[component]
pub fn PresentationPage(
    presentation: Signal<Vec<Slide>>,
    current_slide_number: Signal<usize>,
    presentation_design_settings: Signal<PresentationDesignSettings>,
) -> Element {
    let presentation_design_template =
        use_memo(move || match presentation_design_settings.read().clone() {
            PresentationDesignSettings::Template(template) => Some(template),
            _ => None,
        });
    rsx! {
        div {
            style: "
                all: initial;
                margin:0;
                width:100%;
                height:100%;
                background-color: rgb({presentation_design_template.unwrap().get_background_as_rgb_string()});
                color: rgba({presentation_design_template.unwrap().main_content_fonts.get(0).unwrap().get_color_as_rgba_string()});
            ",
            {
                if let Some(slide) = presentation.get(*current_slide_number.read()) {
                    match slide.slide_content.clone() {
                        SlideContent::Title(title_slide) => { title_slide.title_text },
                        _ => { "No content provided".to_string() }
                    }
                } else {
                    "".to_string()
                }
            }
        }
    }
}
