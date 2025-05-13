//! This module provides functionality for rendering the slides in HTML for the presentation

use cantara_songlib::slides::*;
use dioxus::prelude::*;

use crate::{
    logic::{
        settings::{PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate},
        states::RunningPresentationPosition,
    },
    RUNNING_PRESENTATIONS, TEST_STATE,
};

#[component]
pub fn PresentationPage() -> Element {
    let current_slide: Memo<Option<Slide>> = use_memo(|| match RUNNING_PRESENTATIONS.get(0) {
        Some(presentation) => presentation.clone().get_current_slide(),
        None => None,
    });
    if current_slide.read().clone().is_none() {
        return rsx! {
            div {
                style: "
                    all: initial;
                    margin:0;
                    width:100%;
                    height:100%;
                    background-color: black);
                ",
                p {
                    { "No presentation data found:" },
                    { TEST_STATE.read().clone() }
                }
            }
        };
    }

    let current_design = use_memo(|| {
        RUNNING_PRESENTATIONS
            .get(0)
            .unwrap()
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

    rsx! {
        div {
            style: "
                all: initial;
                margin:0;
                width:100%;
                height:100%;
                background-color: rgb({current_pds.read().clone().get_background_as_rgb_string()});
                color: white;
            ",
            {
                match current_slide.read().clone().unwrap().slide_content.clone() {
                    SlideContent::Title(title_slide) => { title_slide.title_text },
                    _ => { "No content provided".to_string() }
                }
            }
        }
    }
}
