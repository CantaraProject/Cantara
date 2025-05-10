//! This module contains components for displaying and manipulating the program and presentation settings

use crate::logic::settings::*;
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use rust_i18n::{i18n, t};

rust_i18n::i18n!("locales", fallback = "en");

/// This page contains the general settings for Cantara
#[component]
pub fn SettingsPage() -> Element {
    let nav = use_navigator();
    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar no-padding",
                h2 { { t!("settings.settings") } }
            }
            body {
                SettingsContent {}
            }
            footer {
                button {
                    onclick: move |_| { nav.replace(crate::Route::Selection); },
                    { t!("settings.close") }
                }
            }
        }
    }
}

#[component]
fn SettingsContent() -> Element {
    rsx! {
        h3 { "Folder" }

    }
}
