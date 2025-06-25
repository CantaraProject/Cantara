//! This module provides functionality for rendering the slides in HTML for the presentation

use cantara_songlib::slides::*;
use dioxus::prelude::*;
use rgb::RGBA8;
use rust_i18n::t;

use crate::logic::css::{CssHandler, PlaceItems};
use crate::logic::settings::{CssSize, HorizontalAlign, VerticalAlign};
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
pub fn PresentationRendererComponent(
    /// The running presentation as a signal: This will be changed by the component if the user moves the current slide
    running_presentation: Signal<RunningPresentation>,
) -> Element {
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
                    { "No presentation data found." },
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

    let css_presentation_background_color = use_memo(move || current_pds().background_color);

    let css_main_content_font_size = use_memo(move || {
        current_pds
            .read()
            .fonts
            .first()
            .unwrap_or(&FontRepresentation::default())
            .font_size
            .clone()
    });

    let css_main_text_color: Memo<RGBA8> =
        use_memo(move || current_pds.read().clone().fonts.first().unwrap().color);
    let css_padding_left: Memo<CssSize> = use_memo(move || current_pds().padding.left);
    let css_padding_right: Memo<CssSize> = use_memo(move || current_pds().padding.right);
    let css_padding_top: Memo<CssSize> = use_memo(move || current_pds().padding.top);
    let css_padding_bottom: Memo<CssSize> = use_memo(move || current_pds().padding.bottom);
    let css_text_align: Memo<HorizontalAlign> = use_memo(move || {
        current_pds
            .read()
            .fonts
            .first()
            .unwrap()
            .horizontal_alignment
    });
    let css_place_items: Memo<PlaceItems> =
        use_memo(move || match current_pds.read().vertical_alignment {
            VerticalAlign::Top => PlaceItems::StartStretch,
            VerticalAlign::Middle => PlaceItems::CenterStretch,
            VerticalAlign::Bottom => PlaceItems::EndStretch,
        });

    // The CSS handler ([CssHandler]) takes all CSS arguments and builds the string from it.
    // We build it in a memo for the sake of consistency.
    let css_handler: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();

        css.background_color(current_pds().background_color);
        css.padding_left(current_pds().padding.left);
        css.padding_right(current_pds().padding.right);
        css.padding_top(current_pds().padding.top);
        css.padding_bottom(current_pds().padding.bottom);
        css.text_align(css_text_align());
        css.set_important(true);
        css.color(
            current_pds
                .read()
                .clone()
                .fonts
                .first()
                .unwrap_or(&FontRepresentation::default())
                .color,
        );
        css.place_items(css_place_items());

        css
    });

    let background_css: Memo<String> = use_memo(move || {
        let mut css: CssHandler = CssHandler::new();
        let pds = current_pds();

        if let Some(image) = pds.background_image {
            css.background_image(image.as_source().path.to_str().unwrap_or_default());
            css.background_size("cover");
            css.background_position("center");
            css.background_repeat("no-repeat");
            css.opacity(1.0 - pds.background_transparency as f32 / 100.0f32);
        }
        css.to_string()
    });

    rsx! {
        document::Link { rel: "stylesheet", href: PRESENTATION_CSS }
        document::Script { src: PRESENTATION_JS }
        div {
            class: "presentation",
            style: css_handler.read().to_string(),

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
            div {
                class: "background",
                style: background_css()
            }
            if presentation_is_visible() {
                div {
                    class: "slide-container presentation-fade-in",
                    key: "{current_slide_number}",
                    {
                        // This match controls which slide will be rendered depending on the SlideContent
                        // If the slide content is unknown, an error message with will be shown.
                        // This is intentional and *should not* happen in production.
                        match current_slide.read().clone().unwrap().slide_content.clone() {
                            SlideContent::Title(title_slide) => rsx! {
                                TitleSlideComponent {
                                    title_slide: title_slide.clone(),
                                    title_font_representation: current_pds.read().get_default_headline_font()
                                }
                            },
                            SlideContent::SingleLanguageMainContent(main_slide) => rsx! {
                                SingleLanguageMainContentSlideRenderer {
                                    main_slide: main_slide.clone(),
                                    main_content_font: current_pds.read().get_default_font(),
                                    spoiler_content_font: current_pds.read().get_default_spoiler_font()
                                }
                            },
                            SlideContent::Empty(empty_slide) => rsx! {
                                EmptySlideComponent {}
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
    title_font_representation: FontRepresentation,
) -> Element {
    // Build the CSS
    let css_handler: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(title_font_representation.clone()));
        css
    });
    let css_handler_string: Memo<String> = use_memo(move || css_handler.to_string());

    rsx! {
        div {
            class: "headline",
            style: css_handler_string(),
            p {
                style: css_handler_string(),
                { title_slide.title_text }
            }
        }
    }
}

#[component]
fn SingleLanguageMainContentSlideRenderer(
    /// The slide as a [SingleLanguageMainContentSlide]
    main_slide: SingleLanguageMainContentSlide,

    /// The [FontRepresentation] for the main content font.
    main_content_font: FontRepresentation,

    /// The [FontRepresentation] for the spoiler content font.
    spoiler_content_font: FontRepresentation,

    /// The distance between the main content and the spoiler, default is `4 em`.
    distance: Option<CssSize>,
) -> Element {
    let number_of_main_content_lines = {
        let cloned_main_slide = main_slide.clone();
        let main_text = cloned_main_slide.main_text();
        let lines: Vec<&str> = main_text.split("\n").collect();
        lines.len()
    };

    let main_css: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(main_content_font.clone()));
        css
    });

    let distance_css: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.min_height(distance.clone().unwrap_or(CssSize::Em(4.0)));

        css
    });

    let spoiler_css: Memo<CssHandler> = use_memo(move || {
        let mut css = CssHandler::new();

        css.set_important(true);
        css.opacity(1.0);
        css.z_index(2);
        css.extend(&CssHandler::from(spoiler_content_font.clone()));
        css
    });

    rsx! {
        div {
            div {
                class: "main-content",
                style: main_css.read().to_string(),
                p {
                    style: main_css.read().to_string(),
                    for (num, line) in main_slide.clone().main_text().split("\n").enumerate() {
                        { line }
                        if num < number_of_main_content_lines -1 {
                            br { }
                        }
                    }
                }
            }
            if let Some(spoiler_content) = main_slide.spoiler_text() {
                div {
                    class: "distance",
                    style: distance_css.read().to_string(),
                }
                div {
                    class: "spoiler-content",
                    style: spoiler_css.read().to_string(),
                    p {
                        style: spoiler_css.read().to_string(),
                        for (num, line) in spoiler_content.split("\n").enumerate() {
                            { line }
                            if num < spoiler_content.split("\n").count() - 1 {
                                br { }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EmptySlideComponent() -> Element {
    rsx! {
        div {
            class: "empty-content",
        }
    }
}
