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
pub fn PresentationDesignSettingsPage(index: u16) -> Element {
    let nav = navigator();
    let settings = use_settings();

    let selected_presentation_design_option: Signal<Option<PresentationDesign>> =
        use_signal(|| match settings.read().presentation_designs.clone().get(index as usize) {
            Some(design) => Some(design.clone()),
            None => None,
        });
        
    if selected_presentation_design_option.read().is_none() {
        // If no selected design is available, redirect to the settings page
        nav.replace(crate::Route::SettingsPage {});
        return rsx! {};
    }
    
    // From here on, the selected_presentation_design is guaranteed to be Some
    
    let selected_presentation_design = use_memo(move || {
        selected_presentation_design_option.read().clone().unwrap()
    });
    
    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar",
                h2 { { t!("settings.presentation_designs_edit_header", title = selected_presentation_design().name) } }
            }
            main {
                class: "container-fluid content height-100",
                p { "Some Content "}
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
