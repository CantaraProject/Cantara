//! This module provides components for adjusting the presentation designs

use crate::logic::settings::{
    CssSize, PresentationDesign, PresentationDesignSettings, PresentationDesignTemplate,
    TopBottomLeftRight, VerticalAlign, use_settings,
};
use crate::logic::sourcefiles::{ImageSourceFile, SourceFile};
use dioxus::core_macro::{component, rsx};
use dioxus::dioxus_core::Element;
use dioxus::hooks::use_signal;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use rust_i18n::t;
use std::path::PathBuf;

rust_i18n::i18n!("locales", fallback = "en");

/// This page contains the general settings for Cantara
#[component]
pub fn PresentationDesignSettingsPage(
    /// The index of the presentation design
    index: u16,
) -> Element {
    let nav = navigator();
    let mut settings = use_settings();

    let selected_presentation_design_option: Signal<Option<PresentationDesign>> =
        use_signal(|| {
            settings
                .read()
                .presentation_designs
                .clone()
                .get(index as usize)
                .cloned()
        });

    if selected_presentation_design_option.read().is_none() {
        // If no selected design is available, redirect to the settings page
        nav.replace(crate::Route::SettingsPage {});
        return rsx! {};
    }

    // From here on, the selected_presentation_design is guaranteed to be Some

    let selected_presentation_design =
        use_memo(move || selected_presentation_design_option.read().clone().unwrap());

    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar",
                h2 { { t!("settings.presentation_designs_edit_header", title = selected_presentation_design().name) } }
            }
            main {
                class: "container-fluid content height-100",

                MetaSettings {
                    presentation_design: selected_presentation_design(),
                    on_pd_changed: move |pd: PresentationDesign| {
                        let mut settings_write = settings.write();
                        let origin_pd = settings_write.presentation_designs.get_mut(index as usize).unwrap();
                        origin_pd.name = pd.name;
                        origin_pd.description = pd.description;
                    }
                }

                if let PresentationDesignSettings::Template(pd_template) = selected_presentation_design().presentation_design_settings {
                    hr { }
                    DesignTemplateSettings {
                        presentation_design_template: pd_template,
                        onchange: move |new_pdt: PresentationDesignTemplate| {
                            let mut settings_write = settings.write();
                            if let PresentationDesignSettings::Template(pdt) = &mut settings_write.presentation_designs.get_mut(index as usize).unwrap().presentation_design_settings {
                                *pdt = new_pdt.clone();
                            }
                        }
                    }
                }

            }
            footer {
                class: "bottom-bar",
                button {
                    onclick: move |_| {
                        nav.replace(crate::Route::SettingsPage {});
                    },
                    { t!("settings.close") }
                }
            }
        }
    }
}

/// This component allow the setting up of meta settings for presentation designs
#[component]
fn MetaSettings(
    /// The presentation design which Meta information should be able to be edited
    presentation_design: PresentationDesign,

    /// A closure which is called each time when the presentation design has been changed
    on_pd_changed: EventHandler<PresentationDesign>,
) -> Element {
    let mut pd = use_signal(|| presentation_design);

    rsx! {
        h3 { { t!("general.meta_information") } }
        form {
            fieldset {
                label {
                    { t!("general.name") },
                    input {
                        value: pd().name,
                        onchange: move |event| {
                            pd.write().name = event.value().clone();
                            on_pd_changed.call(pd());
                        }
                    }
                }

                label {
                    { t!("general.description") },
                    input {
                        value: pd().description,
                        onchange: move |event| {
                            pd.write().description = event.value().clone();
                            on_pd_changed.call(pd());
                        }
                    }
                }
            }
        }
    }
}

/// This component implements the actual settings for any presentation design which is a
/// design template.
/// Include further settings components here.
#[component]
fn DesignTemplateSettings(
    /// The presentation design which Meta information should be able to be edited
    presentation_design_template: PresentationDesignTemplate,

    /// An event which is called each time when the presentation design template has been changed
    /// by the component
    onchange: EventHandler<PresentationDesignTemplate>,
) -> Element {
    let mut pdt = use_signal(|| presentation_design_template);
    let mut use_background_image: Signal<bool> = use_signal(|| pdt().background_image.is_some());

    rsx!(
        h3 { { t!("settings.presentation_design_configuration") } }
        h4 { { t!("settings.background") } }
        form {
            fieldset {
                label {
                    { t!("settings.color") }
                    input {
                        type: "color",
                        value: pdt().get_background_color_as_hex_string(),
                        onchange: move |event| {
                            _ = pdt.write().set_background_color_from_hex_str(&event.value());
                            onchange.call(pdt());
                        }
                    }
                }

                label {
                    input {
                        type: "checkbox",
                        role: "switch",
                        checked: use_background_image,
                        onchange: move |event| {
                            use_background_image.set(event.checked());
                        }
                    }
                    { t!("settings.use_background_image") }
                }

                if use_background_image() {
                    if let Some(background_image) = pdt().background_image {
                        PictureSelector {
                            onchange: move |background_image| {
                                pdt.write().background_image = Some(background_image);
                                onchange.call(pdt());
                            },
                            already_selected_image_path: background_image.into_inner().path,
                        }
                    } else {
                        PictureSelector {
                            onchange: move |background_image| {
                                pdt.write().background_image = Some(background_image);
                                onchange.call(pdt());
                            }
                        }
                    }

                    // Adjust the background image transparency over a range input
                    label {
                        span { { format!("{}: {}%",
                            t!("settings.background_image_transparency"),
                                pdt.read().background_transparency) } }
                        input {
                            type: "range",
                            min: 0,
                            max: 100,
                            value: pdt.read().background_transparency,
                            oninput: move |event| {
                                pdt.write().background_transparency = event.value().parse().unwrap_or(0);
                                onchange.call(pdt());
                            }
                        }

                    }
                }
            }
        }

        h4 { { t!("settings.padding") } }
        PaddingInput {
            default_padding: pdt().padding,
            onchange: move |data| {
                pdt.write().padding = data;
                onchange.call(pdt());
            }
        }

        // Here the settings for the vertical alignment of the content are included
        h5 { { t!("settings.vertical_alignment.title") } }
        VerticalAlignmentSelector {
            default: pdt().vertical_alignment,
            onchange: move |data| {
                pdt.write().vertical_alignment = data;
                onchange.call(pdt());
            }
        }
    )
}

/// A component which allows the selection of a picture
#[component]
fn PictureSelector(
    default_selection_index: Option<usize>,

    /// This can be given if an image is already set up. It will then be selected as default.
    already_selected_image_path: Option<PathBuf>,

    /// The event will be called if a picture has been selected
    onchange: Option<EventHandler<ImageSourceFile>>,
) -> Element {
    let source_files: Signal<Vec<SourceFile>> = use_context();
    let image_source_files: Memo<Vec<ImageSourceFile>> = use_memo(move || {
        source_files()
            .into_iter()
            .filter_map(ImageSourceFile::new)
            .collect()
    });
    let mut selection_index = use_signal(|| default_selection_index);

    rsx! {
        for (idx, source_file) in image_source_files().iter().enumerate() {
            PictureSelectorItem {
                source_file: source_file.clone(),
                height: "130px",
                max_width: "200px",
                active: if let Some(selection_index) = selection_index() {
                    selection_index == idx
                } else if Some(source_file.clone().into_inner().path) == already_selected_image_path { true }
                else { false },
                onclick: move |image_source_file| {
                    selection_index.set(Some(idx));
                    if let Some(onchange_event) = onchange {
                        onchange_event.call(image_source_file);
                    }
                }
            }
        }
    }
}

/// A component representing a single item (picture) in the [PictureSelector] component
#[component]
fn PictureSelectorItem(
    max_width: String,
    height: String,
    source_file: ImageSourceFile,
    onclick: EventHandler<ImageSourceFile>,
    active: bool,
) -> Element {
    // We need a source file signal here due to the use in the closure
    let sourcefile_signal = use_signal(|| source_file);
    rsx! {
        button {
            role: "button",
            class: if active { "outline" } else { "outline secondary" },
            "data-tooltip": sourcefile_signal().into_inner().name,
            onclick: move |event| {
                onclick.call(sourcefile_signal());
                event.prevent_default();
            },
            img {
                max_width: "180px",
                height: "100px",
                src: sourcefile_signal().into_inner().path.to_str().unwrap_or("").to_string(),
            }
        }
    }
}

/// A component which allows the setting of padding (left, right, top, bottom)
#[component]
fn PaddingInput(
    default_padding: TopBottomLeftRight,
    onchange: EventHandler<TopBottomLeftRight>,
) -> Element {
    let mut padding: Signal<TopBottomLeftRight> = use_signal(|| default_padding);

    rsx!(
        div {
            class: "grid",
            div {
                label {
                    "Left",
                    fieldset {
                        role: "group",
                        NumberedValidatedLengthInput {
                            value: padding().left,
                            placeholder: "left",
                            onchange: move |value| {
                                padding.write().left = get_nullified_css_size(value);
                                onchange.call(padding());
                            }
                        }
                    },
                }
            }
            div {
                label {
                    "Right",
                    fieldset {
                        role: "group",
                        NumberedValidatedLengthInput {
                            value: padding().right,
                            placeholder: "right",
                            onchange: move |value| {
                                padding.write().right = get_nullified_css_size(value);
                                onchange.call(padding());
                            }
                        }
                    },
                }
            }
        }
        div {
            class: "grid",
            div {
                label {
                    "Top",
                    fieldset {
                        role: "group",
                        NumberedValidatedLengthInput {
                            value: padding().top,
                            placeholder: "top",
                            onchange: move |value: CssSize| {
                                padding.write().top = get_nullified_css_size(value);
                                onchange.call(padding());
                            }
                        }
                    },
                }
            }
            div {
                label {
                    "Bottom",
                    fieldset {
                        role: "group",
                        NumberedValidatedLengthInput {
                            value: padding().bottom,
                            placeholder: "bottom",
                            onchange: move |value: CssSize| {
                                // If the content is null, we will set it accordingly
                                padding.write().bottom = get_nullified_css_size(value);
                                onchange.call(padding());
                            }
                        }
                    },
                }
            }
        }
    )
}

#[component]
fn NumberedValidatedLengthInput(
    value: CssSize,
    placeholder: String,
    onchange: EventHandler<CssSize>,
) -> Element {
    let mut value_signal = use_signal(|| value);
    rsx! {
        input {
            placeholder,
            value: value_signal.read().get_float(),
            inputmode: "numeric",
            onchange: move |event| {
                value_signal.write().set_float(event.value().parse().unwrap_or(0.0));
                onchange.call(value_signal());
            }
        }
        select {
            name: "unit",
            required: true,
            onchange: move |event: Event<FormData>| {
                match event.value().as_str() {
                    "px" => value_signal.set(CssSize::Px(value_signal().get_float())),
                    "pt" => value_signal.set(CssSize::Pt(value_signal().get_float())),
                    "em" => value_signal.set(CssSize::Em(value_signal().get_float())),
                    "%"  => value_signal.set(CssSize::Percentage(value_signal().get_float())),
                    _    => value_signal.set(CssSize::Px(value_signal().get_float()))
                };
                onchange.call(value_signal());
            },
            option {
                selected: matches!(value_signal(), CssSize::Px(_)) || value_signal() == CssSize::Null,
                "px"
            }
            option {
                selected: matches!(value_signal(), CssSize::Px(_)) || value_signal() == CssSize::Null,
                "pt"
            }
            option {
                selected: matches!(value_signal(), CssSize::Em(_)),
                "em"
            }
            option {
                selected: matches!(value_signal(), CssSize::Percentage(_)),
                "%"
            }
        }
    }
}

/// Returns a [CssSize::Null] if the value is `0.0`. Else, the original value is cloned.
fn get_nullified_css_size(css_size: CssSize) -> CssSize {
    match css_size.get_float() {
        0.0 => CssSize::Null,
        _ => css_size.clone(),
    }
}

/// A component for selecting the vertical alignment (left, right, centered)
#[component]
fn VerticalAlignmentSelector(
    default: VerticalAlign,
    onchange: EventHandler<VerticalAlign>,
) -> Element {
    let mut value_signal = use_signal(|| default);
    rsx!(
        select {
            name: "vertical_align",
            required: true,
            aria_label: t!("settings.vertical_alignment.description").to_string(),
            onchange: move |event| {
                match event.value().as_str() {
                    "top" => value_signal.set(VerticalAlign::Top),
                    "middle" => value_signal.set(VerticalAlign::Middle),
                    "bottom" => value_signal.set(VerticalAlign::Bottom),
                    other => tracing::error!("Invalid option for vertical alignment selected, the value is: {}", other)
                    };
                onchange.call(value_signal());
            },
            option {
                value: "top",
                selected: value_signal() == VerticalAlign::Top,
                { t!("settings.vertical_alignment.top") }
            }
            option {
                value: "middle",
                selected: value_signal() == VerticalAlign::Middle,
                { t!("settings.vertical_alignment.middle") }
            }
            option {
                value: "bottom",
                selected: value_signal() == VerticalAlign::Bottom,
                { t!("settings.vertical_alignment.bottom") }
            }
        }
    )
}
