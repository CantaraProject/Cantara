//! This module provides components for adjusting the song slide settings

use crate::logic::settings::use_settings;
use cantara_songlib::slides::{LanguageConfiguration, ShowMetaInformation, SlideElement, SlideSettings};
use dioxus::core_macro::{component, rsx};
use dioxus::dioxus_core::Element;
use dioxus::hooks::use_signal;
use dioxus::logger::tracing;
use dioxus::prelude::*;
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
        use_memo(move || selected_slide_settings_option.read().clone().unwrap_or_default());

    rsx! {
        div { class: "wrapper",
            header { class: "top-bar",
                h2 { {t!("settings.song_slide_settings_edit_header", title = index + 1).to_string()} }
            }
            main { class: "container-fluid content height-100",

                MetaSettings {
                    slide_settings: selected_slide_settings(),
                    on_settings_changed: move |updated_settings: SlideSettings| {
                        {
                            let mut settings_write = settings.write();
                            if let Some(origin_settings) = settings_write
                                .song_slide_settings
                                .get_mut(index as usize)
                            {
                                *origin_settings = updated_settings;
                            }
                        }
                        // Persist right away: the edit page has no save button,
                        // so without this every change was lost on restart.
                        settings.peek().save();
                    },
                }
            }
            footer { class: "bottom-bar",
                button {
                    onclick: move |_| {
                        nav.replace(crate::Route::SettingsPage {});
                    },
                    {t!("settings.close").to_string()}
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

    use_effect(move || {
        let new_value = selected_slide_settings_index()
            .and_then(|index| song_slide_settings.read().get(index).cloned());
        selected_slide_settings.set(new_value);
    });

    rsx! {
        hgroup {
            h4 { {t!("settings.song_slide_headline").to_string()} }
            p { {t!("settings.song_slide_description").to_string()} }
        }

        div { class: "grid",
            div {
                // Here we would ideally have a SlideSettingsSelector component
                // similar to PresentationDesignSelector, but for now we'll use a simple select
                select {
                    onchange: move |event| {
                        let index = event.value().parse::<usize>().unwrap_or(0);
                        selected_slide_settings_index.set(Some(index));
                    },
                    for (index , _) in song_slide_settings.read().iter().enumerate() {
                        option {
                            value: index.to_string(),
                            selected: selected_slide_settings_index() == Some(index),
                            {format!("Slide Setting {}", index + 1)}
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
                                    selected_slide_settings_index
                                        .set(Some(0).filter(|_| !song_slide_settings.read().is_empty()));
                                }
                            }
                        },
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
            h6 { {format!("Slide Setting {}", index.map_or(0, |i| i + 1))} }
            p { {format!("{:?}", slide_settings)} }
            if let Some(index) = index {
                button {
                    onclick: move |_| {
                        nav.push(crate::Route::SongSlideSettingsPage {
                            index: index as u16,
                        });
                    },
                    {t!("general.edit").to_string()}
                }
                button { class: "secondary", onclick: move |_| onclone.call(()),
                    {t!("general.duplicate").to_string()}
                }
                button {
                    class: "secondary",
                    onclick: move |event| {
                        event.prevent_default();
                        let js = t!("dialogs.confirm_deletion").to_string();
                        async move {
                            match document::eval(
                                    &crate::components::shared_components::js_yes_no_box(js),
                                )
                                .await
                            {
                                Ok(value) if value.as_bool().unwrap_or(false) => {
                                    tracing::debug!("Deletion confirmed.");
                                    ondelete.call(());
                                }
                                _ => tracing::debug!("Deletion aborted or failed."),
                            }
                        }
                    },
                    {t!("general.delete").to_string()}
                }
            }
        }
    }
}

/// Which layout a song's slides use. Mirrors [`LanguageConfiguration`] without
/// its payload, so it can back a set of radio buttons.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DisplayMode {
    Single,
    Multi,
    Complex,
}

impl DisplayMode {
    fn of(configuration: &LanguageConfiguration) -> DisplayMode {
        match configuration {
            LanguageConfiguration::SingleLanguage(_) => DisplayMode::Single,
            LanguageConfiguration::MultiLanguage(_) => DisplayMode::Multi,
            LanguageConfiguration::Complex(_) => DisplayMode::Complex,
        }
    }
}

/// Carry as much of the previous configuration over to another layout as makes
/// sense, so that switching modes to have a look does not throw the languages
/// away.
fn switch_mode(current: &LanguageConfiguration, wanted: DisplayMode) -> LanguageConfiguration {
    let languages: Vec<String> = match current {
        LanguageConfiguration::SingleLanguage(language) => language.iter().cloned().collect(),
        LanguageConfiguration::MultiLanguage(languages) => languages.clone(),
        LanguageConfiguration::Complex(elements) => elements
            .iter()
            .filter_map(|element| match element {
                SlideElement::Lyrics(language) => Some(language.clone()),
                SlideElement::Notation => None,
            })
            .collect(),
    };

    match wanted {
        DisplayMode::Single => LanguageConfiguration::SingleLanguage(languages.first().cloned()),
        DisplayMode::Multi => LanguageConfiguration::MultiLanguage(languages),
        DisplayMode::Complex => {
            let mut elements = vec![SlideElement::Notation];
            elements.extend(languages.into_iter().map(SlideElement::Lyrics));
            LanguageConfiguration::Complex(elements)
        }
    }
}

/// The rows of a complex layout, with buttons to add, reorder and remove them.
#[component]
fn ComplexRowEditor(
    /// The rows currently configured
    elements: Vec<SlideElement>,
    /// Called with the new list whenever it changes
    on_changed: EventHandler<Vec<SlideElement>>,
) -> Element {
    let mut new_language = use_signal(String::new);

    rsx! {
        small { {t!("display.rows_hint").to_string()} }

        if elements.is_empty() {
            p { class: "display-rows-empty", {t!("display.no_rows").to_string()} }
        }

        ul { class: "display-rows",
            for (index , element) in elements.iter().cloned().enumerate() {
                li { key: "{index}", class: "display-row",
                    span { class: "display-row-label",
                        match &element {
                            SlideElement::Notation => t!("display.row_notation").to_string(),
                            SlideElement::Lyrics(language) => language.clone(),
                        }
                    }
                    div { class: "display-row-buttons",
                        button {
                            r#type: "button",
                            class: "outline",
                            title: t!("display.move_up").to_string(),
                            disabled: index == 0,
                            onclick: {
                                let elements = elements.clone();
                                move |_| {
                                    let mut next = elements.clone();
                                    next.swap(index, index - 1);
                                    on_changed.call(next);
                                }
                            },
                            "↑"
                        }
                        button {
                            r#type: "button",
                            class: "outline",
                            title: t!("display.move_down").to_string(),
                            disabled: index + 1 >= elements.len(),
                            onclick: {
                                let elements = elements.clone();
                                move |_| {
                                    let mut next = elements.clone();
                                    next.swap(index, index + 1);
                                    on_changed.call(next);
                                }
                            },
                            "↓"
                        }
                        button {
                            r#type: "button",
                            class: "outline secondary",
                            title: t!("display.remove_row").to_string(),
                            onclick: {
                                let elements = elements.clone();
                                move |_| {
                                    let mut next = elements.clone();
                                    next.remove(index);
                                    on_changed.call(next);
                                }
                            },
                            "✕"
                        }
                    }
                }
            }
        }

        div { class: "display-row-add",
            button {
                r#type: "button",
                class: "outline",
                onclick: {
                    let elements = elements.clone();
                    move |_| {
                        let mut next = elements.clone();
                        next.push(SlideElement::Notation);
                        on_changed.call(next);
                    }
                },
                {t!("display.add_notation").to_string()}
            }
            input {
                r#type: "text",
                value: "{new_language}",
                placeholder: t!("display.language_placeholder").to_string(),
                oninput: move |event| new_language.set(event.value()),
            }
            button {
                r#type: "button",
                class: "outline",
                disabled: new_language.read().trim().is_empty(),
                onclick: {
                    let elements = elements.clone();
                    move |_| {
                        let language = new_language.read().trim().to_lowercase();
                        if language.is_empty() {
                            return;
                        }
                        let mut next = elements.clone();
                        next.push(SlideElement::Lyrics(language));
                        new_language.set(String::new());
                        on_changed.call(next);
                    }
                },
                {t!("display.add_language").to_string()}
            }
        }
    }
}

/// Everything about what a song's slides show: the layout, the languages, the
/// meta information line and how much text goes on one slide.
#[component]
fn MetaSettings(
    /// The slide settings which should be edited
    slide_settings: SlideSettings,

    /// A closure which is called each time when the slide settings have been changed
    on_settings_changed: EventHandler<SlideSettings>,
) -> Element {
    let mut settings = use_signal(|| slide_settings);

    let max_lines_display = move || match settings().max_lines {
        Some(lines) => lines.to_string(),
        None => String::new(),
    };

    let mode = move || DisplayMode::of(&settings().language);

    // Applies a change and notifies the parent in one step, so no call site can
    // forget the notification.
    let mut update = move |change: &dyn Fn(&mut SlideSettings)| {
        {
            let mut writer = settings.write();
            change(&mut writer);
        }
        on_settings_changed.call(settings());
    };

    rsx! {
        h3 { {t!("display.section").to_string()} }
        form {
            fieldset {
                legend { {t!("display.mode").to_string()} }

                for (value , label , hint) in [
                    (DisplayMode::Single, "display.mode_single", "display.mode_single_hint"),
                    (DisplayMode::Multi, "display.mode_multi", "display.mode_multi_hint"),
                    (DisplayMode::Complex, "display.mode_complex", "display.mode_complex_hint"),
                ]
                {
                    label {
                        input {
                            r#type: "radio",
                            name: "display-mode",
                            checked: mode() == value,
                            onchange: move |_| {
                                update(
                                    &|settings: &mut SlideSettings| {
                                        settings.language = switch_mode(&settings.language, value);
                                    },
                                );
                            },
                        }
                        {t!(label).to_string()}
                        br {}
                        small { {t!(hint).to_string()} }
                    }
                }
            }

            // --- Layout specific options ---
            match settings().language {
                LanguageConfiguration::SingleLanguage(language) => rsx! {
                    label {
                        {t!("display.language").to_string()}
                        input {
                            r#type: "text",
                            value: language.clone().unwrap_or_default(),
                            placeholder: t!("display.language_auto").to_string(),
                            onchange: move |event| {
                                let value = event.value().trim().to_lowercase();
                                update(
                                    &|settings: &mut SlideSettings| {
                                        settings
                                            .language = LanguageConfiguration::SingleLanguage(
                                            if value.is_empty() { None } else { Some(value.clone()) },
                                        );
                                    },
                                );
                            },
                        }
                    }
                },
                LanguageConfiguration::MultiLanguage(languages) => rsx! {
                    label {
                        {t!("display.language").to_string()}
                        input {
                            r#type: "text",
                            value: languages.join(", "),
                            placeholder: t!("display.languages_all").to_string(),
                            onchange: move |event| {
                                let value = event.value();
                                let languages: Vec<String> = value
                                    .split(',')
                                    .map(|entry| entry.trim().to_lowercase())
                                    .filter(|entry| !entry.is_empty())
                                    .collect();
                                update(
                                    &|settings: &mut SlideSettings| {
                                        settings
                                            .language = LanguageConfiguration::MultiLanguage(
                                            languages.clone(),
                                        );
                                    },
                                );
                            },
                        }
                    }
                },
                LanguageConfiguration::Complex(elements) => rsx! {
                    fieldset {
                        legend { {t!("display.rows").to_string()} }
                        ComplexRowEditor {
                            elements: elements.clone(),
                            on_changed: move |next: Vec<SlideElement>| {
                                update(
                                    &|settings: &mut SlideSettings| {
                                        settings.language = LanguageConfiguration::Complex(next.clone());
                                    },
                                );
                            },
                        }
                    }
                },
            }

            fieldset {
                legend { {t!("general.meta_information").to_string()} }

                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: settings().title_slide,
                        onchange: move |event| {
                            let checked = event.checked();
                            update(&|settings: &mut SlideSettings| settings.title_slide = checked);
                        },
                    }
                    {t!("display.title_slide").to_string()}
                }

                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: settings().show_spoiler,
                        onchange: move |event| {
                            let checked = event.checked();
                            update(&|settings: &mut SlideSettings| settings.show_spoiler = checked);
                        },
                    }
                    {t!("display.show_spoiler").to_string()}
                }

                label {
                    input {
                        r#type: "checkbox",
                        role: "switch",
                        checked: settings().empty_last_slide,
                        onchange: move |event| {
                            let checked = event.checked();
                            update(
                                &|settings: &mut SlideSettings| settings.empty_last_slide = checked,
                            );
                        },
                    }
                    {t!("display.empty_last_slide").to_string()}
                }

                label {
                    {t!("display.meta_syntax").to_string()}
                    input {
                        r#type: "text",
                        value: settings().meta_syntax.clone(),
                        placeholder: t!("display.meta_syntax_hint").to_string(),
                        onchange: move |event| {
                            let value = event.value();
                            update(&|settings: &mut SlideSettings| {
                                settings.meta_syntax = value.clone()
                            });
                        },
                    }
                }
            }

            fieldset {
                legend { {t!("display.meta_where").to_string()} }

                for (label , read , write) in meta_positions() {
                    label {
                        input {
                            r#type: "checkbox",
                            checked: read(&settings().show_meta_information),
                            onchange: move |event| {
                                let checked = event.checked();
                                update(
                                    &|settings: &mut SlideSettings| {
                                        write(&mut settings.show_meta_information, checked)
                                    },
                                );
                            },
                        }
                        {t!(label).to_string()}
                    }
                }
            }

            fieldset {
                label {
                    {t!("display.max_lines").to_string()}
                    input {
                        r#type: "number",
                        min: "1",
                        max: "20",
                        value: max_lines_display(),
                        placeholder: t!("display.max_lines_placeholder").to_string(),
                        onchange: move |event| {
                            let raw = event.value();
                            let value = raw.trim().parse::<usize>().ok().filter(|lines| *lines > 0);
                            update(&|settings: &mut SlideSettings| settings.max_lines = value);
                        },
                    }
                }
            }
        }
    }
}

/// The three places the meta information line can appear, as
/// (translation key, reader, writer) so the checkboxes can be generated.
#[allow(clippy::type_complexity)]
fn meta_positions() -> [(
    &'static str,
    fn(&ShowMetaInformation) -> bool,
    fn(&mut ShowMetaInformation, bool),
); 3] {
    [
        (
            "display.meta_on_title",
            |show| show.title_slide,
            |show, value| show.title_slide = value,
        ),
        (
            "display.meta_on_first",
            |show| show.first_slide,
            |show, value| show.first_slide = value,
        ),
        (
            "display.meta_on_last",
            |show| show.last_slide,
            |show, value| show.last_slide = value,
        ),
    ]
}
