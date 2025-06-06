//! This module provides components for adjusting the presentation designs

use crate::logic::settings::{PresentationDesign, use_settings, PresentationDesignTemplate, PresentationDesignSettings};
use dioxus::core_macro::{component, rsx};
use dioxus::dioxus_core::Element;
use dioxus::hooks::use_signal;
use dioxus::html::completions::CompleteWithBraces::button;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use rust_i18n::t;
use crate::logic::sourcefiles::{ImageSourceFile, SourceFile};

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
                        let mut origin_pd = settings_write.presentation_designs.get_mut(index as usize).unwrap();
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

#[component]
fn DesignTemplateSettings(
    /// The presentation design which Meta information should be able to be edited
    presentation_design_template: PresentationDesignTemplate,

    /// An event which is called each time when the presentation design template has been changed
    /// by the component
    onchange: EventHandler<PresentationDesignTemplate>,
) -> Element {
    let mut pdt = use_signal(|| presentation_design_template);
    let mut use_background_image: Signal<bool> = use_signal(|| match pdt().background_image {
        None => false,
        Some(_) => true,
    });

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
                    PictureSelector {
                        onchange: move |background_image| {
                            pdt.write().background_image = Some(background_image);
                            onchange.call(pdt());
                        }
                    }
                }
            }
        }
    )
}

/// A component which allows the selection of a picture
#[component]
fn PictureSelector(
    default_selection_index: Option<usize>,

    /// The event will be called if a picture has been selected
    onchange: Option<EventHandler<ImageSourceFile>>
) -> Element {
    let mut source_files: Signal<Vec<SourceFile>> = use_context();
    let image_source_files: Memo<Vec<ImageSourceFile>> = use_memo(move || {
        source_files().into_iter()
            .filter_map(|source_file| ImageSourceFile::new(source_file))
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
                    if selection_index == idx { true } else { false }
                } else { false },
                onclick: move |isf| {
                    selection_index.set(Some(idx));
                    if let Some(onchange_event) = onchange {
                        onchange_event.call(isf);
                    }
                }
            }
        }
    }
}

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