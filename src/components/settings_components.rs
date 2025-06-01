//! This module contains components for displaying and manipulating the program and presentation settings

use super::shared_components::{DeleteIcon, EditIcon, PresentationDesignSelecter};
use crate::{Route, logic::settings::*};
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use rfd::FileDialog;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// This page contains the general settings for Cantara

/// The component representing the settings page in Cantara. It loads the settings from persistance
/// and provides the structure of the settings page. It is the entry component for the other components
/// of this module.
#[component]
pub fn SettingsPage() -> Element {
    let nav = use_navigator();
    let mut settings = use_settings();

    let presentation_designs: Signal<Vec<PresentationDesign>> =
        use_signal(|| settings.read().presentation_designs.clone());

    use_effect(move || {
        if *presentation_designs.read() != settings.read().presentation_designs {
            settings.write().presentation_designs = presentation_designs.read().clone();
            tracing::debug!(
                "Updated presentation designs, length: {}",
                presentation_designs.read().len()
            );
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
                SettingsContent {
                    presentation_designs
                }
            }
            footer {
                class: "bottom-bar",
                button {

                    onclick: move |_| {
                        settings.read().save();
                        nav.replace(crate::Route::Selection {});
                    },
                    { t!("settings.close") }
                }
            }
        }
    }
}

/// This components provides the settings component and is designed as a middleware between the
/// [SettingsPage] and its children.
#[component]
fn SettingsContent(presentation_designs: Signal<Vec<PresentationDesign>>) -> Element {
    rsx! {
        RepositorySettings {}
        hr { }
        PresentationSettings {
            presentation_designs: presentation_designs
        }
    }
}

/// Implements the logic for adding, editing and deleting repositories
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
                                        if new_name_unwrapped.trim() != "" && new_name_unwrapped != *"null" {
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

/// Component for modifying the presentation design settings
#[component]
fn PresentationSettings(presentation_designs: Signal<Vec<PresentationDesign>>) -> Element {
    let selected_presentation_design_index: Signal<Option<usize>> = use_signal(|| Some(0));

    let mut selected_presentation_design: Signal<Option<PresentationDesign>> = use_signal(|| None);

    // Update the selected_presentation_design signal whenever the index changes
    use_effect(move || {
        selected_presentation_design.set(match *selected_presentation_design_index.read() {
            Some(index) => presentation_designs.read().get(index).cloned(),
            None => None,
        });
    });

    // Update the presentation_designs signal whenever the selected_presentation_design changes
    let update_selected_design = move || {
        if let Some(index) = *selected_presentation_design_index.read() {
            if let Some(design) = selected_presentation_design.read().clone() {
                if let Some(writable_pd_ref) = presentation_designs.write().get_mut(index) {
                    *writable_pd_ref = design.clone()
                }
            }
        }
    };

    rsx! {
        hgroup {
            h4 { { t!("settings.presentation_headline") } }
            p { { t!("settings.presentation_description") } }
        }
        div {
            class: "grid",
            div {
                PresentationDesignSelecter {
                    presentation_designs: presentation_designs,
                    viewer_width: 400,
                    active_item: selected_presentation_design_index
                }
            }
            div {
                if selected_presentation_design.read().is_some() {
                    article {
                        h6  { { selected_presentation_design().unwrap().name } }
                        button {
                            onclick: move |_| {
                                let nav = use_navigator();
                                nav.push(Route::PresentationDesignSettingsPage {
                                    index: selected_presentation_design_index.read().unwrap() as u16
                                });
                            },
                            { t!("general.edit") }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PresentationDesignCard(presentation_design: PresentationDesign) -> Element {
    rsx! {}
}
