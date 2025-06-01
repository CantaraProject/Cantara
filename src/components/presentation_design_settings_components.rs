//! This module provides components for adjusting the presentation designs

use crate::logic::settings::{PresentationDesign, use_settings};
use dioxus::core_macro::{component, rsx};
use dioxus::dioxus_core::Element;
use dioxus::hooks::use_signal;
use dioxus::prelude::*;
use dioxus_router::hooks::use_navigator;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// This page contains the general settings for Cantara
#[component]
pub fn PresentationDesignSettingsPage(index: u16) -> Element {
    let nav = use_navigator();
    let settings = use_settings();

    let presentation_designs: Signal<Vec<PresentationDesign>> =
        use_signal(|| settings.read().presentation_designs.clone());

    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar",
                h2 { { t!("settings.settings") } }
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
