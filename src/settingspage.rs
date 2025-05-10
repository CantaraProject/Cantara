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
                class: "top-bar content",
                h2 { { t!("settings.settings") } }
            }
            main {
                class: "container height-100",
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
        RepositorySettings {}
    }
}

#[component]
fn RepositorySettings() -> Element {
    let settings = use_settings();
    rsx! {
        article {
            header {
                hgroup {
                    h3 { "Repositories" },
                    p { "Select one or multiple Repositories where Cantara will load source files from. "}
                }
            }
            ul {
                for repository in *settings.read().repositories.clone() {
                    li {
                        match repository {
                            Repository::LocaleFilePath(string) => &string,
                            Repository::Remote(string) => ""
                        }
                    }
                }
            }

        }
    }
}
