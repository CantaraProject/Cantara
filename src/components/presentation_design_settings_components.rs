//! This module provides components for adjusting the presentation designs

use crate::logic::settings::{PresentationDesign, use_settings};
use dioxus::core_macro::{component, rsx};
use dioxus::dioxus_core::Element;
use dioxus::hooks::use_signal;
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use rust_i18n::t;

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
                PresentationDesignMetaSettings {
                    presentation_design: selected_presentation_design(),
                    on_pd_changed: move |pd: PresentationDesign| {
                        let mut settings_write = settings.write();
                        let mut origin_pd = settings_write.presentation_designs.get_mut(index as usize).unwrap();
                        origin_pd.name = pd.name;
                        origin_pd.description = pd.description;
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

#[component]
pub fn PresentationDesignMetaSettings(
    /// The presentation design which Meta information shuold be able to be edited
    presentation_design: PresentationDesign,

    /// A closure which is called each time when the presentation design has been changed
    on_pd_changed: EventHandler<PresentationDesign>,
) -> Element {

    let mut pd = use_signal(|| presentation_design);

    rsx! {
        h3 { "Meta information" }
        form {
            fieldset {
                label {
                    "Name",
                    input {
                        value: pd().name,
                        onchange: move |event| {
                            pd.write().name = event.value().clone();
                            on_pd_changed.call(pd());
                        }
                    }
                }

                label {
                    "Description",
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