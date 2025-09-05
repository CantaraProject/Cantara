//! This module provides components for adjusting the song slide settings

use crate::components::shared_components::{DeleteIcon, EditIcon, NumberedValidatedLengthInput};
use crate::logic::settings::{use_settings};
use cantara_songlib::slides::SlideSettings;
use dioxus::core_macro::{component, rsx};
use dioxus::dioxus_core::Element;
use dioxus::hooks::use_signal;
use dioxus::logger::tracing;
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// This page contains the settings for song slides
#[component]
pub fn SongSlideSettingsPage(
    /// The index of the song slide settings
    index: u16,
) -> Element {
    let nav = navigator();
    let mut settings = use_settings();

    let selected_slide_settings_option: Signal<Option<SlideSettings>> =
        use_signal(|| {
            settings
                .read()
                .song_slide_settings
                .clone()
                .get(index as usize)
                .cloned()
        });

    if selected_slide_settings_option.read().is_none() {
        // If no selected settings are available, redirect to the settings page
        nav.replace(crate::Route::SettingsPage {});
        return rsx! {};
    }

    // From here on, the selected_slide_settings is guaranteed to be Some
    let selected_slide_settings =
        use_memo(move || selected_slide_settings_option.read().clone().unwrap());

    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar",
                h2 { { t!("settings.song_slide_settings_edit_header", title = index + 1) } }
            }
            main {
                class: "container-fluid content height-100",

                MetaSettings {
                    slide_settings: selected_slide_settings(),
                    on_settings_changed: move |updated_settings: SlideSettings| {
                        let mut settings_write = settings.write();
                        let origin_settings = settings_write.song_slide_settings.get_mut(index as usize).unwrap();
                        *origin_settings = updated_settings;
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

/// Component for modifying song slide settings.
#[component]
pub fn SongSlideSettings(song_slide_settings: Signal<Vec<SlideSettings>>) -> Element {
    let mut selected_slide_settings_index = use_signal(|| Some(0));
    let mut selected_slide_settings = use_signal(|| None::<SlideSettings>);
    let mut settings = use_settings();

    use_effect(move || {
        let new_value = selected_slide_settings_index()
            .and_then(|index| song_slide_settings.read().get(index).cloned());
        selected_slide_settings.set(new_value);
    });

    rsx! {
        hgroup {
            h4 { { t!("settings.song_slide_headline") } }
            p { { t!("settings.song_slide_description") } }
        }

        div {
            class: "grid",
            div {
                // Here we would ideally have a SlideSettingsSelector component
                // similar to PresentationDesignSelector, but for now we'll use a simple select
                select {
                    onchange: move |event| {
                        let index = event.value().parse::<usize>().unwrap_or(0);
                        selected_slide_settings_index.set(Some(index));
                    },
                    for (index, _) in song_slide_settings.read().iter().enumerate() {
                        option {
                            value: index.to_string(),
                            selected: selected_slide_settings_index() == Some(index),
                            { format!("Slide Setting {}", index + 1) }
                        }
                    }
                }
            }
            div {
                if let Some(selected_settings) = selected_slide_settings() {
                    SongSlideSettingsCard {
                        slide_settings: selected_settings,
                        index: selected_slide_settings_index(),
                        onclone: move |_| {
                            if let Some(settings) = selected_slide_settings() {
                                song_slide_settings.write().push(settings);
                                let new_len = song_slide_settings.read().len();
                                tracing::debug!("Cloned slide settings. New length: {}", new_len);
                            }
                        },
                        ondelete: move |_| {
                            if let Some(index) = selected_slide_settings_index() {
                                if index < song_slide_settings.read().len() {
                                    song_slide_settings.write().remove(index);
                                    selected_slide_settings_index.set(Some(0).filter(|_| !song_slide_settings.read().is_empty()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Displays an article with details and actions for song slide settings.
#[component]
fn SongSlideSettingsCard(
    slide_settings: SlideSettings,
    index: Option<usize>,
    onclone: EventHandler<()>,
    ondelete: EventHandler<()>,
) -> Element {
    let nav = use_navigator();
    rsx! {
        article {
            h6 { { format!("Slide Setting {}", index.map_or(0, |i| i + 1)) } }
            p { { format!("{:?}", slide_settings) } }
            if let Some(index) = index {
                button {
                    onclick: move |_| {
                        nav.push(crate::Route::SongSlideSettingsPage { index: index as u16 });
                    },
                    { t!("general.edit") }
                }
                button {
                    class: "secondary",
                    onclick: move |_| onclone.call(()),
                    { t!("general.duplicate") }
                }
                button {
                    class: "secondary",
                    onclick: move |event| {
                        event.prevent_default();
                        let js = t!("dialogs.confirm_deletion").to_string();
                        async move {
                            match document::eval(&crate::components::shared_components::js_yes_no_box(js)).await {
                                Ok(value) if value.as_bool().unwrap_or(false) => {
                                    tracing::debug!("Deletion confirmed.");
                                    ondelete.call(());
                                }
                                _ => tracing::debug!("Deletion aborted or failed."),
                            }
                        }
                    },
                    { t!("general.delete") }
                }
            }
        }
    }
}

/// This component allows the setting up of meta settings for song slides
#[component]
fn MetaSettings(
    /// The slide settings which should be edited
    slide_settings: SlideSettings,

    /// A closure which is called each time when the slide settings have been changed
    on_settings_changed: EventHandler<SlideSettings>,
) -> Element {
    let mut settings = use_signal(|| slide_settings);

    // Helper function to display the max_lines value
    let max_lines_display = move || {
        match settings().max_lines {
            Some(lines) => lines.to_string(),
            None => "".to_string(),
        }
    };

    rsx! {
        h3 { { t!("general.meta_information") } }
        form {
            fieldset {
                // Title Slide setting
                label {
                    input {
                        type: "checkbox",
                        role: "switch",
                        checked: settings().title_slide,
                        onchange: move |event| {
                            {
                                let mut settings_write = settings.write();
                                settings_write.title_slide = event.checked();
                            } // Drop the mutable borrow
                            on_settings_changed.call(settings());
                        }
                    }
                    { "Show Title Slide" }
                }

                // Show Spoiler setting
                label {
                    input {
                        type: "checkbox",
                        role: "switch",
                        checked: settings().show_spoiler,
                        onchange: move |event| {
                            {
                                let mut settings_write = settings.write();
                                settings_write.show_spoiler = event.checked();
                            } // Drop the mutable borrow
                            on_settings_changed.call(settings());
                        }
                    }
                    { "Show Spoiler" }
                }

                // Empty Last Slide setting
                label {
                    input {
                        type: "checkbox",
                        role: "switch",
                        checked: settings().empty_last_slide,
                        onchange: move |event| {
                            {
                                let mut settings_write = settings.write();
                                settings_write.empty_last_slide = event.checked();
                            } // Drop the mutable borrow
                            on_settings_changed.call(settings());
                        }
                    }
                    { "Empty Last Slide" }
                }

                // Meta Syntax setting
                label {
                    { "Meta Syntax" }
                    input {
                        type: "text",
                        value: settings().meta_syntax.clone(),
                        onchange: move |event| {
                            {
                                let mut settings_write = settings.write();
                                settings_write.meta_syntax = event.value().clone();
                            } // Drop the mutable borrow
                            on_settings_changed.call(settings());
                        }
                    }
                }

                // Max Lines setting
                label {
                    { "Max Lines Per Slide" }
                    input {
                        type: "number",
                        min: "1",
                        max: "20",
                        value: max_lines_display(),
                        placeholder: "Optional",
                        onchange: move |event| {
                            let event_value = event.value().to_string(); // Create a longer-lived binding
                            let value = event_value.trim();
                            {
                                let mut settings_write = settings.write();
                                if value.is_empty() {
                                    settings_write.max_lines = None;
                                } else {
                                    settings_write.max_lines = Some(value.parse().unwrap_or(4));
                                }
                            } // Drop the mutable borrow
                            on_settings_changed.call(settings());
                        }
                    }
                }
            }
        }
    }
}