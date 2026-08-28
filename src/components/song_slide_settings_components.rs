//! This module provides components for adjusting the song slide settings

use crate::components::shared_components::{MetadataFieldset, translate};
use crate::logic::settings::{SongSlideSettings, use_settings};
use crate::logic::slide_summary::{POSITION_SEPARATOR, summary_lines};
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

    // Read from the settings on every render rather than copied once, for the
    // reason the design editor learned the hard way: an editor working on a
    // copy of its own shows one thing while the rest of the program uses
    // another.
    let selected: Memo<Option<SongSlideSettings>> =
        use_memo(move || settings.read().song_slide_settings.get(index as usize).cloned());

    // Every hook is claimed before the way out below.
    let division = use_memo(move || selected().unwrap_or_default());

    // The page has no save button: what is edited here goes into the settings
    // as it is typed and is written out when the page is left.
    use_drop(move || {
        if let Ok(settings) = settings.try_read() {
            settings.save();
        }
    });

    if selected.read().is_none() {
        // If no selected settings are available, redirect to the settings page
        nav.replace(crate::Route::SettingsPage {});
        return rsx! {};
    }

    // Applies a change to the division in the settings, whatever it is.
    let mut update = move |change: &dyn Fn(&mut SongSlideSettings)| {
        let mut settings_write = settings.write();
        if let Some(current) = settings_write.song_slide_settings.get_mut(index as usize) {
            change(current);
        }
    };

    rsx! {
        div { class: "wrapper",
            header { class: "top-bar",
                h2 {
                    {
                        t!(
                            "settings.song_slide_settings_edit_header",
                            title = division().display_name(index as usize),
                        )
                            .to_string()
                    }
                }
            }
            main { class: "container-fluid content height-100",

                MetadataFieldset {
                    name: division().name,
                    description: division().description,
                    name_placeholder: t!("settings.slide_settings_name_placeholder").to_string(),
                    on_changed: move |(name, description): (String, String)| {
                        update(&|current: &mut SongSlideSettings| {
                            current.name = name.clone();
                            current.description = description.clone();
                        });
                    },
                }

                hr {}

                DisplaySettings {
                    slide_settings: division().settings,
                    on_settings_changed: move |updated: SlideSettings| {
                        update(&|current: &mut SongSlideSettings| {
                            current.settings = updated.clone();
                        });
                    },
                }
            }
            footer { class: "bottom-bar",
                button {
                    onclick: move |_| {
                        settings.read().save();
                        nav.replace(crate::Route::SettingsPage {});
                    },
                    {t!("settings.close").to_string()}
                }
            }
        }
    }
}

/// The section of the settings page that lists the slide divisions.
#[component]
pub fn SongSlideSettingsSection(
    song_slide_settings: Signal<Vec<SongSlideSettings>>,
) -> Element {
    let mut selected_slide_settings_index = use_signal(|| Some(0));

    let selected_slide_settings = use_memo(move || {
        selected_slide_settings_index()
            .and_then(|index| song_slide_settings.read().get(index).cloned())
    });

    rsx! {
        hgroup {
            h4 { {t!("settings.song_slide_headline").to_string()} }
            p { {t!("settings.song_slide_description").to_string()} }
        }

        div { class: "grid",
            div {
                select {
                    onchange: move |event| {
                        let index = event.value().parse::<usize>().unwrap_or(0);
                        selected_slide_settings_index.set(Some(index));
                    },
                    for (index , division) in song_slide_settings.read().iter().enumerate() {
                        option {
                            value: index.to_string(),
                            selected: selected_slide_settings_index() == Some(index),
                            {division.display_name(index)}
                        }
                    }
                }
            }
            div {
                if let Some(selected) = selected_slide_settings() {
                    SongSlideSettingsCard {
                        division: selected,
                        index: selected_slide_settings_index(),
                        onclone: move |_| {
                            if let Some(division) = selected_slide_settings() {
                                song_slide_settings.write().push(division);
                                let new_len = song_slide_settings.read().len();
                                tracing::debug!("Cloned slide settings. New length: {}", new_len);
                            }
                        },
                        ondelete: move |_| {
                            if let Some(index) = selected_slide_settings_index()
                                && index < song_slide_settings.read().len() {
                                    song_slide_settings.write().remove(index);
                                    selected_slide_settings_index
                                        .set((!song_slide_settings.read().is_empty()).then_some(0));
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
    division: SongSlideSettings,
    index: Option<usize>,
    onclone: EventHandler<()>,
    ondelete: EventHandler<()>,
) -> Element {
    let nav = use_navigator();
    let position = index.unwrap_or(0);
    let mut export_error: Signal<Option<String>> = use_signal(|| None);

    // A division is a handful of switches, so it travels as a JSON file a
    // person can read — see [`crate::logic::settings_io`].
    let division_to_export = division.clone();
    let export = move |_| {
        export_error.set(None);
        let outcome =
            match crate::logic::settings_io::write_slide_settings(&division_to_export, position) {
                Ok((name, bytes)) => {
                    crate::components::shared_components::save_file(&name, &bytes).map(|_| ())
                }
                Err(error) => {
                    let (key, parameters) = error.message_key();
                    Err(translate(key, &parameters))
                }
            };

        if let Err(message) = outcome {
            log::warn!("the slide settings could not be exported: {message}");
            export_error.set(Some(message));
        }
    };

    rsx! {
        article {
            h6 { {division.display_name(position)} }
            if !division.description.trim().is_empty() {
                p { {division.description.clone()} }
            }

            // What the division does, rather than the struct it is. This used
            // to be `{:?}` of the settings — braces, field names and all —
            // which said everything except what a reader wanted to know.
            ul { class: "slide-settings-summary",
                for (key , parameters) in summary_lines(&division.settings) {
                    li { key: "{key}", {summary_sentence(key, &parameters)} }
                }
            }

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
                button { class: "secondary", onclick: export,
                    {t!("settings.export_slide_settings").to_string()}
                }
                button {
                    class: "secondary",
                    onclick: move |event| {
                        event.prevent_default();
                        let question = t!("dialogs.confirm_deletion").to_string();
                        async move {
                            if crate::components::dialogs::confirm_box(question).await {
                                tracing::debug!("Deletion confirmed.");
                                ondelete.call(());
                            } else {
                                tracing::debug!("Deletion aborted.");
                            }
                        }
                    },
                    {t!("general.delete").to_string()}
                }
            }
            if let Some(message) = export_error() {
                p { class: "export-save-error", role: "alert", {message} }
            }
        }
    }
}

/// One line of the summary, in the reader's language.
///
/// The positions of the metadata line arrive as keys of their own so that a
/// language may order and join them as it likes; here they become the list the
/// sentence is built around.
fn summary_sentence(key: &'static str, parameters: &[(&'static str, String)]) -> String {
    let translated: Vec<(&str, String)> = parameters
        .iter()
        .map(|(name, value)| match *name {
            "positions" => (
                *name,
                value
                    .split(POSITION_SEPARATOR)
                    .map(|position| t!(position).to_string())
                    .collect::<Vec<String>>()
                    .join(", "),
            ),
            _ => (*name, value.clone()),
        })
        .collect();

    translate(key, &translated)
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
fn DisplaySettings(
    /// The slide settings which should be edited
    slide_settings: SlideSettings,

    /// A closure which is called each time when the slide settings have been changed
    on_settings_changed: EventHandler<SlideSettings>,
) -> Element {
    let mut settings = use_signal(|| slide_settings.clone());

    // The signal is initialised once, but the route can switch to another
    // division while this component stays alive and is handed the new
    // division's settings as a prop. Without following the prop, the fields
    // would keep showing — and, on the next edit, write back — the settings of
    // the division that was open before, over the one now selected.
    use_effect(use_reactive!(|slide_settings| {
        if *settings.peek() != slide_settings {
            settings.set(slide_settings);
        }
    }));

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

                // The template beside what it does. A metadata line is written
                // by trying it: the conditionals mean the same template says
                // different things about different songs, and reading one
                // without seeing the result is guesswork. See
                // [`MetaSyntaxEditor`].
                MetaSyntaxEditor {
                    value: settings().meta_syntax.clone(),
                    on_changed: move |value: String| {
                        update(&|settings: &mut SlideSettings| {
                            settings.meta_syntax = value.clone()
                        });
                    },
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

/// The metadata template, beside what it does.
///
/// A metadata line is not written, it is *tried*: `{{#if author}}` means the
/// same template says one thing about a hymn with an author and another about
/// a hymn without, and a template that Handlebars cannot read shows nothing at
/// all — silently, in the middle of a service. So the template is on the left
/// and the result of it on the right, on one well-known song. See
/// [`crate::logic::slide_summary::meta_preview`].
///
/// The field is a `textarea` rather than a line: a real template has a line per
/// piece of information, and a single-line field showed one long run of text
/// with the breaks invisible. It grows with what is in it, up to the point
/// where growing further would push the preview off the screen.
#[component]
fn MetaSyntaxEditor(
    /// The template as the division holds it.
    value: String,

    /// Called with the template as it is typed. The page keeps the settings up
    /// to date as it goes and writes them out when it is left, so there is
    /// nothing to commit here.
    on_changed: EventHandler<String>,
) -> Element {
    use crate::logic::slide_summary::{
        MetaPreview, PREVIEW_AUTHOR, PREVIEW_BIBLE, PREVIEW_TITLE, meta_preview,
    };

    // Tall enough to show that this holds more than one line, and never so
    // tall that the preview beside it is pushed out of view.
    let rows = value.lines().count().clamp(3, 12);
    let preview = meta_preview(&value);

    rsx! {
        div { class: "grid",
            label {
                {t!("display.meta_syntax").to_string()}
                textarea {
                    class: "meta-syntax-input",
                    rows: "{rows}",
                    // A template is code, and every web view's spell checker
                    // underlines all of it.
                    spellcheck: false,
                    value: "{value}",
                    placeholder: t!("display.meta_syntax_hint").to_string(),
                    oninput: move |event| on_changed.call(event.value()),
                }
            }

            div {
                label { {t!("display.meta_preview").to_string()} }
                figure { class: "meta-syntax-preview",
                    match preview {
                        // `pre-line` in the stylesheet, because the line breaks
                        // of the template are what the slide will show.
                        MetaPreview::Line(line) => rsx! {
                            p { class: "meta-syntax-preview-line", "{line}" }
                        },
                        MetaPreview::Nothing => rsx! {
                            p { class: "meta-syntax-preview-empty",
                                {t!("display.meta_preview_nothing").to_string()}
                            }
                        },
                        MetaPreview::Broken(reason) => rsx! {
                            p { class: "meta-syntax-preview-broken",
                                {t!("display.meta_preview_broken").to_string()}
                            }
                            code { class: "meta-syntax-preview-reason", "{reason}" }
                        },
                    }
                    figcaption {
                        {
                            t!(
                                "display.meta_preview_song",
                                title = PREVIEW_TITLE,
                                author = PREVIEW_AUTHOR,
                                bible = PREVIEW_BIBLE,
                            )
                                .to_string()
                        }
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
