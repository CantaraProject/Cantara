//! This module provides functionality for rendering the slides in HTML for the presentation

use cantara_songlib::slides::*;
use dioxus::prelude::*;
use rust_i18n::t;

use crate::{
    MAIN_CSS,
    logic::{
        settings::{FontRepresentation, PresentationDesignSettings, PresentationDesignTemplate},
        states::RunningPresentation,
    },
};

const PRESENTATION_CSS: Asset = asset!("/assets/presentation.css");
const PRESENTATION_JS: Asset = asset!("/assets/presentation_positioning.js");

rust_i18n::i18n!("locales", fallback = "en");

/// The presentation page as the entry point for the presentation window
#[component]
pub fn PresentationPage() -> Element {
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    let running_presentation: Signal<RunningPresentation> =
        use_signal(move || running_presentations.get(0).unwrap().clone());

    use_effect(move || {
        *running_presentations.write().get_mut(0).unwrap() = running_presentation.read().clone();
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Title { { t!("presentation.title")} }
        PresentationRendererComponent {
            running_presentation: running_presentation
        }
    }
}

/// The actual presentation rendering component which can be used to render presentations accordingly
/// It takes a signal and rewrites to it when the presentation position changes
#[component]
pub fn PresentationRendererComponent(running_presentation: Signal<RunningPresentation>) -> Element {
    let current_slide: Memo<Option<Slide>> =
        use_memo(move || running_presentation.read().get_current_slide());

    let current_slide_number: Memo<usize> =
        use_memo(move || match running_presentation.read().clone().position {
            Some(position) => position.slide_total(),
            None => 0,
        });

    let mut presentation_is_visible = use_signal(|| false);

    let mut go_to_next_slide = move || {
        running_presentation.write().next_slide();
        presentation_is_visible.set(false);
        presentation_is_visible.set(true);
    };

    let mut go_to_previous_slide = move || {
        running_presentation.write().previous_slide();
        presentation_is_visible.set(false);
        presentation_is_visible.set(true);
    };

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
        running_presentation
            .read()
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

    let css_presentation_background_color =
        use_memo(move || current_pds.read().clone().get_background_as_rgb_string());
    let css_headline_font_size = use_memo(move || {
        current_pds
            .read()
            .main_content_fonts
            .first()
            .unwrap_or(&FontRepresentation::default())
            .headline_font_size
    });
    let css_maincontent_font_size = use_memo(move || {
        current_pds
            .read()
            .main_content_fonts
            .first()
            .unwrap_or(&FontRepresentation::default())
            .font_size
    });
    let css_spoiler_font_size = use_memo(move || {
        current_pds
            .read()
            .main_content_fonts
            .first()
            .unwrap_or(&FontRepresentation::default())
            .spoiler_font_size
    });
    let css_main_text_color: Memo<String> = use_memo(move || {
        current_pds
            .read()
            .clone()
            .main_content_fonts
            .first()
            .unwrap()
            .get_color_as_rgba_string()
    });
    let css_padding_left: Memo<String> =
        use_memo(move || current_pds.read().padding.left.to_css_string());
    let css_padding_right: Memo<String> =
        use_memo(move || current_pds.read().padding.right.to_css_string());
    let css_padding_top: Memo<String> =
        use_memo(move || current_pds.read().padding.top.to_css_string());
    let css_padding_bottom: Memo<String> =
        use_memo(move || current_pds.read().padding.bottom.to_css_string());
    let css_text_align: Memo<String> = use_memo(move || {
        current_pds
            .read()
            .main_content_fonts
            .first()
            .unwrap()
            .horizontal_alignment
            .to_css_string()
    });

    let custom_css_style: Memo<String> = use_memo(move || {
        format!(
            "{};{};{};{};{};{};",
            format!(
                "background-color: rgb({});",
                css_presentation_background_color()
            ),
            format!("padding-left: {};", css_padding_left()),
            format!("padding-right: {}", css_padding_right()),
            format!("padding-top: {}", css_padding_top()),
            format!("padding-bottom: {}", css_padding_bottom()),
            format!("color: rgba({})!important", css_main_text_color)
        )
    });

    rsx! {
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
        document::Script { src: PRESENTATION_JS }
        div {
            class: "presentation",
            style: custom_css_style(),

            tabindex: 0,
            onkeydown: move |event: Event<KeyboardData>| {
                match event.key() {
                    Key::ArrowRight => go_to_next_slide(),
                    Key::ArrowLeft => go_to_previous_slide(),
                    _ => {}
                }
            },
            onclick: move |_| {
                go_to_next_slide();
            },
            oncontextmenu: move |_| {
                go_to_previous_slide();
            },
            onmounted: move |_| {
                presentation_is_visible.set(true);
            },
            if presentation_is_visible() {
                div {
                    class: "slide-container presentation-fade-in",
                    key: "{current_slide_number}",
                    {
                        match current_slide.read().clone().unwrap().slide_content.clone() {
                            SlideContent::Title(title_slide) => rsx! {
                                TitleSlideComponent {
                                    title_slide: title_slide.clone(),
                                    css_headline_font_size: css_headline_font_size(),
                                    css_text_color: css_main_text_color(),
                                    css_text_align: css_text_align()
                                }
                            },
                            SlideContent::SingleLanguageMainContent(main_slide) => rsx! {
                                SlingleLanguageMainContentSlide {
                                    main_slide: main_slide.clone(),
                                    current_pds: current_pds.read().clone(),
                                    css_main_content_size: css_maincontent_font_size(),
                                    css_spoiler_content_size: css_spoiler_font_size(),
                                    css_text_color: css_main_text_color(),
                                    css_text_align: css_text_align()
                                }
                            },
                            _ => rsx! { p { "No content provided" } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TitleSlideComponent(
    title_slide: TitleSlide,
    css_headline_font_size: String,
    css_text_color: String,
    css_text_align: String,
) -> Element {
    rsx! {
        div {
            id: "headline",
            style: format!("font-size: {}px!important;color: rgba({})!important;text-align: {}!important;", css_headline_font_size, css_text_color, css_text_align),
            p {
                style: format!("font-size: {}px!important;color: rgba({})!important;text-align: {}!important;", css_headline_font_size, css_text_color, css_text_align),
                { title_slide.title_text }
            }
        }
    }
}

#[component]
fn SlingleLanguageMainContentSlide(
    main_slide: SingleLanguageMainContentSlide,
    current_pds: PresentationDesignTemplate,
    css_main_content_size: String,
    css_spoiler_content_size: String,
    css_text_color: String,
    css_text_align: String,
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
                style: format!("font-size: {}px!important;color: rgba({})!important;text-align: {}!important;", css_main_content_size, css_text_color, css_text_align),
                p {
                    style: format!("font-size: {}px;color: rgba({})!important;text-align: {}!important;", css_main_content_size, css_text_color, css_text_align),
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
                    style: format!("font-size: {}px!important;color: rgba({})!important;text-align: {}!important;", css_spoiler_content_size, css_text_color, css_text_align),
                    p {
                        style: format!("font-size: {}px!important;color: rgba({})!important;text-align: {}!important;", css_spoiler_content_size, css_text_color, css_text_align),
                        { spoiler_content }
                    }
                }
            }
        }
    }
}
