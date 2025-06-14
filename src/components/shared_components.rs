//! This submodule contains shared components which can be reused among different parts of the program.

use super::presentation_components::PresentationRendererComponent;
use crate::logic::presentation::create_amazing_grace_presentation;
use crate::logic::settings::PresentationDesign;
use crate::logic::states::RunningPresentation;
use cantara_songlib::slides::SlideSettings;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::{FaImage, FaMusic, FaPenToSquare};

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
pub fn MusicIcon(width: Option<u32>) -> Element {
    rsx! {
        Icon {
            icon: FaMusic,
            width: width.unwrap_or(20),
        }
    }
}

#[component]
pub fn ImageIcon(width: Option<u32>) -> Element {
    rsx! {
        Icon {
            icon: FaImage,
            width: width.unwrap_or(20),
        }
    }
}

/// A component which displays multiple presentation designs in an "Amazing Grace" presentation and allows to select one
#[component]
pub fn PresentationDesignSelecter(
    presentation_designs: Vec<PresentationDesign>,
    song_slide_settings: Option<SlideSettings>,
    default_selection: Option<usize>,
    viewer_width: usize,
    active_item: Signal<Option<usize>>,
) -> Element {
    let song_slide_settings_signal = use_signal(|| song_slide_settings);
    let mut presentation_designs_signal = use_signal(|| presentation_designs);

    let mut presentations: Signal<Vec<RunningPresentation>> =
        use_signal(move || {
            let mut presentation_signals = vec![];
            for design in presentation_designs_signal() {
                let presentation =
                    create_amazing_grace_presentation(
                        &design,
                        &match song_slide_settings_signal() {
                            Some(slide_settings_signal) => slide_settings_signal,
                            None => SlideSettings::default(),
                        },
                    );
                presentation_signals.push(presentation);
            }
            presentation_signals
        });

    rsx! {
        div {
            class: "presentation-design-selecter",
            for (number, presentation) in presentations().into_iter().enumerate() {
                span {
                    class: format!("presentation-design-selecter-item {}", match active_item() {
                        Some(active_item) => if active_item == number { "active" } else { "" },
                        None => "",
                    }),
                    key: number,
                    tabindex: number,
                    SelectablePresentationViewer {
                        presentation: presentation,
                        width: viewer_width,
                        title: presentation_designs_signal().get(number).unwrap().name.clone(),
                        index: number,
                        current_selection: active_item,
                    }
                }
            }
        }
    }
}

/// A wrapper component around the PresentationViewer which allows selecting it
#[component]
pub fn SelectablePresentationViewer(
    presentation: RunningPresentation,
    width: usize,
    title: Option<String>,
    index: usize,
    current_selection: Signal<Option<usize>>,
) -> Element {
    let mut selected = use_signal(move || Some(*current_selection.read() == Some(index)));

    use_effect(move || {
        selected.set(Some(*current_selection.read() == Some(index)));
    });

    rsx! {
        PresentationViewer {
            presentation,
            width,
            title,
            selected: selected,
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
    selected: Option<Signal<Option<bool>>>,
    onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let scale_percentage = ((width as f64 / 1024_f64) * 100.0).round();
    let zoom_css_string = format!("zoom: {}%;", scale_percentage);

    let presentation_signal = use_signal(|| presentation);

    let css_class = use_memo(move || match selected {
        Some(selected) => {
            if *selected.read() == Some(true) {
                "rounded-corners-active"
            } else {
                "rounded-corners-inactive"
            }
        }
        None => "rounded-corners-inactive",
    });

    rsx! {
        div {
            class: format!("{} {}", css_class(), "presentation-preview inline-div"),
            style: format!("{}{}", "position: relative;width:1024px;height:576px;", zoom_css_string),
            onclick: move |event| {
                if let Some(onclick) = onclick { onclick.call(event) }
            },

            PresentationRendererComponent {
                running_presentation: presentation_signal
            }

            if let Some(title) = title {
                div {
                    class: "presentation-title",
                    style: "zoom:100%!important;position: absolute;top: 0;right: 0;display: flex;align-items: center;justify-content: center;font-size: 30pt;background-color:black;color:white;z-index:99!important;",
                    { title }
                }
            }
        }
    }
}

/// Provides an Example Presentation Viewer in 16:9 format scaled down to a fixed with
#[component]
pub fn ExamplePresentationViewer(
    presentation_design: PresentationDesign,
    song_slide_settings: Option<Signal<SlideSettings>>,
    width: usize,
    increase_font_size_in_percent: Option<usize>,
) -> Element {
    let presentation_signal = use_signal(|| {
        create_amazing_grace_presentation(
            &presentation_design,
            &match song_slide_settings {
                Some(slide_settings_signal) => slide_settings_signal(),
                None => SlideSettings::default(),
            },
        )
    });

    rsx! {
        PresentationViewer {
            presentation: presentation_signal(),
            width: width,
        }
    }
}

/// A helper function which generates the Java Script code for a dialog box with 'yes' and 'no'
/// (or 'abort' button).
/// Due to Dioxus' structures, the `document::eval` still has to be called from within the component.
///
/// # Arguments
/// - `promt`: The question or message which should be shown to the user as [String]
///
/// # Returns
/// - The JavaScript code to create a dialog box with Yes|No options and the prompt as message.
pub fn js_yes_no_box(promt: String) -> String {
    format!("return confirm('{}');", promt)
}