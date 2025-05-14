//! This module contains components for displaying and manipulating the program and presentation settings

use crate::{logic::settings::*, shared_components::DeleteIcon};
use dioxus::prelude::*;
use rfd::FileDialog;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// This page contains the general settings for Cantara
#[component]
pub fn SettingsPage() -> Element {
    let nav = use_navigator();
    let settings = use_settings();

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
                hr { }
                PresentationSettings {  }
            }
            footer {
                button {
                    onclick: move |_| {
                        settings.read().save();
                        nav.replace(crate::Route::Selection);
                    },
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
    let mut settings = use_settings();

    let mut select_directory = move || {
        let path = FileDialog::new().pick_folder();
        let mut settings = settings.write();

        if let Some(path) = path {
            if path.is_dir() && path.exists() {
                let chosen_directory = path.to_str().unwrap_or_default().to_string();
                settings.add_repository_folder(chosen_directory.to_string());
            }
        }
    };

    rsx! {
        hgroup {
            h3 { { t!("settings.repositories_headline") } },
            p { { t!("settings.repositories_description") } }
        }
        for (index, repository) in settings.read().repositories.clone().iter().enumerate() {
            article {
                class: "listed-article",
                if let Repository::LocaleFilePath(string) = repository {
                    h6 { { t!("settings.repositories_local_dir") } }
                    { string.clone() }
                }
                if let Repository::Remote(string) = repository {
                    h6 { { t!("settings.repositories_remote_dir") } }
                    { string.clone() }
                }
                if settings.read().repositories.len() > 1 {
                    div {
                        style: "float:right",
                        onclick: move |_| {
                            let mut settings = settings.write();
                            settings.repositories.remove(index);
                        },
                        DeleteIcon { }
                    }
                }
            }
        }

        button {
            class: "smaller-buttons",
            onclick: move |_| { select_directory(); },
            "Add a new folder"
        }
    }
}

#[component]
fn PresentationSettings() -> Element {
    rsx! {
        hgroup {
            h4 { { t!("settings.presentation_headline") } }
            p { { t!("settings.presentation_description") } }
        }
    }
}
