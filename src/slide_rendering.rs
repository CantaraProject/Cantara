//! This module provides functionality for rendering the slides in HTML for the presentation

use cantara_songlib::slides::*;
use dioxus::prelude::*;

use crate::{
    logic::{
        settings::{PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate},
        states::RunningPresentationPosition,
    },
    RUNNING_PRESENTATIONS,
};

fn get_current_position_slide_and_design(
) -> Option<(RunningPresentationPosition, Slide, PresentationDesign)> {
    if RUNNING_PRESENTATIONS.get(0).is_none() {
        return None;
    }

    if RUNNING_PRESENTATIONS.get(0).unwrap().position.is_none() {
        return None;
    }

    let current_position: RunningPresentationPosition = RUNNING_PRESENTATIONS
        .get(0)
        .unwrap()
        .position
        .clone()
        .unwrap()
        .clone();

    // We can safely unwrap here because we have checked the existence of the slide already before.
    let current_slide: Slide = RUNNING_PRESENTATIONS
        .get(0)
        .unwrap()
        .get_current_slide()
        .unwrap();

    let presentation_design: PresentationDesign = RUNNING_PRESENTATIONS
        .get(0)
        .unwrap()
        .presentation
        .get(current_position.chapter())
        .unwrap()
        .presentation_design
        .clone()
        .unwrap_or(PresentationDesign::default());

    Some((current_position, current_slide, presentation_design))
}

#[component]
pub fn PresentationPage() -> Element {
    let current_slide = use_memo(|| RUNNING_PRESENTATIONS.get(0).unwrap().get_current_slide());
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
                color: rgba({current_pds.read().clone().main_content_fonts.get(0).unwrap().get_color_as_rgba_string()});
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
