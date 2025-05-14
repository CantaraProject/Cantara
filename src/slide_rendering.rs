//! This module provides functionality for rendering the slides in HTML for the presentation

use cantara_songlib::slides::*;
use dioxus::prelude::*;

use crate::logic::{
    settings::{FontRepresentation, PresentationDesignSettings, PresentationDesignTemplate},
    states::RunningPresentation,
};

const PRESENTATION_CSS: Asset = asset!("/assets/presentation.css");
const PRESENTATION_JS: Asset = asset!("/assets/presentation_positioning.js");

#[component]
pub fn PresentationPage() -> Element {
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    let current_slide: Memo<Option<Slide>> = use_memo(move || match running_presentations.get(0) {
        Some(presentation) => presentation.clone().get_current_slide(),
        None => None,
    });

    // Stop rendering if no slide can be rendered.
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
                }
            }
        };
    }

    let current_design = use_memo(move || {
        running_presentations
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

    // Set the CSS variables from the loaded PresentationDesign
    use_effect(move || {
        document::eval(&format!(
            r#"var r = document.querySelector(':root');
            r.style.setProperty('--var-presentation-background-color', 'rgb({})');
            r.style.setProperty('--var-headline-font-size', '{}px');
            r.style.setProperty('--var-maincontent-font-size', '{}px');
            r.style.setProperty('--var-spoiler-font-size', '{}px');
            r.style.setProperty('--var-main-text-color', 'rgb({})')
            "#,
            current_pds.read().clone().get_background_as_rgb_string(),
            (current_pds
                .read()
                .main_content_fonts.first()
                .unwrap_or(&FontRepresentation::default())
                .headline_font_size),
            (current_pds
                .read()
                .main_content_fonts.first()
                .unwrap_or(&FontRepresentation::default())
                .font_size),
            (current_pds
                .read()
                .main_content_fonts.first()
                .unwrap_or(&FontRepresentation::default())
                .spoiler_font_size),
            current_pds
                .read()
                .clone()
                .main_content_fonts.first()
                .unwrap()
                .get_color_as_rgba_string()
        ));
    });

    rsx! {
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
        document::Script { src: PRESENTATION_JS }
        div {
            id: "presentation",
            tabindex: 0,
            onkeydown: move |event: Event<KeyboardData>| {
                match event.key() {
                    Key::ArrowRight => running_presentations.write().get_mut(0).unwrap().next_slide(),
                    Key::ArrowLeft => running_presentations.write().get_mut(0).unwrap().previous_slide(),
                    _ => {}
                }
            },
            onclick: move |_| {
                running_presentations.write().get_mut(0).unwrap().next_slide();
            },
            {
                match current_slide.read().clone().unwrap().slide_content.clone() {
                    SlideContent::Title(title_slide) => rsx! {
                        TitleSlideComponent {
                            title_slide: title_slide.clone(),
                            current_pds: current_pds.read().clone()
                        }
                    },
                    SlideContent::SingleLanguageMainContent(main_slide) => rsx! {
                        SlingleLanguageMainContentSlide {
                            main_slide: main_slide.clone(),
                            current_pds: current_pds.read().clone()
                        }
                    },
                    _ => rsx! { p { "No content provided" } }
                }
            }
        }
    }
}

#[component]
fn TitleSlideComponent(
    title_slide: TitleSlide,
    current_pds: PresentationDesignTemplate,
) -> Element {
    rsx! {
        div {
            id: "headline",
            p { { title_slide.title_text } }
        }
    }
}

#[component]
fn SlingleLanguageMainContentSlide(
    main_slide: SingleLanguageMainContentSlide,
    current_pds: PresentationDesignTemplate,
) -> Element {
    let number_of_main_content_lines = {
        let cloned_main_slide = main_slide.clone();
        let main_text = cloned_main_slide.main_text();
        let lines: Vec<&str> = main_text.split("\n").collect();
        lines.len()
    };

    rsx! {
        div {
            id: "singlelanguage-main-content",
            div {
                class: "main-content",
                p {
                    for (num, line) in main_slide.clone().main_text().split("\n").enumerate() {
                        { line }
                        if num < number_of_main_content_lines -1 {
                            br { }
                        }
                    }
                }
            }
            if let Some(spoiler_content) = Some(main_slide.spoiler_text()) {
                div {
                    class: "spoiler-content",
                    p {
                        { spoiler_content }
                    }
                }
            }
        }
    }
}
