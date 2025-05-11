//! This module provides functionality for rendering the slides in HTML for the presentation

use cantara_songlib::slides::*;
use dioxus::prelude::*;

use crate::logic::settings::PresentationDesign;

#[component]
pub fn PresentationPage(
    presentation: Signal<Vec<Slide>>,
    current_slide_number: Signal<usize>,
    presentation_design_settings: Signal<PresentationDesign>,
) -> Element {
    rsx! {
        div {
            style: "all: initial;margin:0;width:100%;height:100%;"
        }
    }
}
