//! This module contains components for displaying and manipulating the program and presentation settings

use crate::shared_components::PresentationDesignSelecter;
use crate::{
    logic::settings::*,
    shared_components::{DeleteIcon, EditIcon, ExamplePresentationViewer},
};
use dioxus::prelude::*;
use rfd::FileDialog;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// This page contains the general settings for Cantara
#[component]
pub fn SettingsPage() -> Element {
    let nav = use_navigator();
    let mut settings = use_settings();

    let presentation_designs: Signal<Vec<PresentationDesign>> =
        use_signal(|| settings.read().presentation_designs.clone());

    use_effect(move || {
        if *presentation_designs.read() != settings.read().presentation_designs {
            settings.write().presentation_designs = presentation_designs.read().clone();
        }
    });

    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar",
                h2 { { t!("settings.settings") } }
            }
            main {
                class: "container-fluid content height-100",
                SettingsContent {}
                hr { }
                PresentationSettings {
                    presentation_designs: presentation_designs
                }
            }
            footer {
                class: "bottom-bar",
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
                h6 {
                    { repository.name.clone() }
                    div {
                        style: "float:right",
                        span {
                            onclick: move |_| {
                                async move {
                                    let mut settings = settings.write();
                                    let new_name = match document::eval("return prompt('Please enter a new name: ', '');").await {
                                        Ok(str) => Some(str.to_string().replace("\"", "")),
                                        Err(_) => None
                                    };
                                    if new_name.is_some() {
                                        let new_name_unwrapped = new_name.clone().unwrap();
                                        if new_name_unwrapped.trim() != "" && new_name_unwrapped != "null".to_string() {
                                            settings.repositories.get_mut(index).unwrap().name = new_name_unwrapped.trim().to_string();
                                        }
                                    }
                                }
                            },
                            EditIcon {  }
                        }
                        if settings.read().repositories.len() > 1 && settings.read().repositories.get(index).unwrap().removable {
                            span {
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

                if let RepositoryType::LocaleFilePath(string) = &repository.repository_type {
                    div { { t!("settings.repositories_local_dir") }
                        br { }
                        pre { { string.clone() } }
                    }
                }
                if let RepositoryType::Remote(string) = &repository.repository_type {
                    div { { t!("settings.repositories_remote_dir") }
                        br { }
                        { string.clone() }
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
fn PresentationSettings(presentation_designs: Signal<Vec<PresentationDesign>>) -> Element {
    let selected_presentation_design: Signal<Option<usize>> = use_signal(|| None);

    rsx! {
        hgroup {
            h4 { { t!("settings.presentation_headline") } }
            p { { t!("settings.presentation_description") } }
        }

        PresentationDesignSelecter {
            presentation_designs: presentation_designs,
            viewer_width: 400,
            active_item: selected_presentation_design
        }
    }
}
