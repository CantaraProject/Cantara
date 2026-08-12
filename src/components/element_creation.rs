//! Creating an element, and moving one between repositories.
//!
//! Both belong to the detail view, and both are the same kind of thing: a
//! change to the files on disk rather than to what is on screen. The rules
//! they follow live in [`crate::logic::repository_files`] — nothing is ever
//! overwritten, and only a local folder can be written to. What is here is the
//! asking: which repository, what name, and for a song the chance to paste the
//! text in and have its verses found.
//!
//! Neither appears in the web build. A browser has no folders to write into,
//! and a button that can only report that is worse than no button.

use crate::logic::settings::{Settings, use_settings};
use crate::logic::sourcefiles::SourceFile;
use crate::logic::states::LibraryRefresh;
use dioxus::prelude::*;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// The repositories a file can be written into, as (position, name) pairs.
///
/// A repository that is not a local folder, or one the user has marked
/// read-only, cannot be a target — see
/// [`Settings::writable_repositories`](crate::logic::settings::Settings::writable_repositories).
fn writable(settings: &Settings) -> Vec<(usize, String)> {
    settings
        .writable_repositories()
        .into_iter()
        .map(|(index, repository)| (index, repository.name.clone()))
        .collect()
}

/// The button that starts a new element, and the dialog behind it.
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn NewElementButton() -> Element {
    let mut open = use_signal(|| false);
    let settings = use_settings();
    let targets = use_memo(move || writable(&settings.read()));

    rsx! {
        button {
            class: "outline",
            // Without a folder to write into there is nothing this could do.
            // Disabled rather than hidden, with the reason on hover: a user
            // who has only remote libraries should learn why, not wonder
            // where the button went.
            disabled: targets().is_empty(),
            title: match targets().is_empty() {
                true => t!("detail.new_needs_writable_repository").to_string(),
                false => t!("detail.new_element").to_string(),
            },
            onclick: move |_| open.set(true),
            { t!("detail.new_element").to_string() }
        }

        if open() {
            NewElementDialog { open }
        }
    }
}

/// There is nothing to write into in a browser.
#[cfg(target_arch = "wasm32")]
#[component]
pub fn NewElementButton() -> Element {
    rsx! {}
}

/// Asking what to create, and creating it.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn NewElementDialog(open: Signal<bool>) -> Element {
    use crate::logic::repository_files::{NewFileKind, create, file_name_for};
    use crate::logic::song_text::{Guess, guess};

    let settings = use_settings();
    let mut library_refresh = use_context::<LibraryRefresh>();
    let targets = use_memo(move || writable(&settings.read()));

    let mut kind = use_signal(|| NewFileKind::Song);
    let mut title = use_signal(String::new);
    let mut pasted = use_signal(String::new);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    // The repository the file goes into. The first writable one to begin with,
    // which is the one most installations have.
    let mut target = use_signal(|| targets().first().map(|(index, _)| *index).unwrap_or(0));

    // What Cantara makes of the pasted text, recomputed as it is typed or
    // pasted. Shown before anything is written: the guess is a head start, and
    // the user is the one who decides whether it is a good one.
    let guessed: Memo<Guess> = use_memo(move || match kind() == NewFileKind::Song {
        true => guess(&pasted()),
        false => Guess::default(),
    });

    // A pasted text that names the song saves typing the title twice.
    use_effect(move || {
        let found = guessed().title;
        if !found.is_empty() && title.peek().trim().is_empty() {
            title.set(found);
        }
    });

    let close = move |_| open.set(false);

    let mut save = move || {
        let Some(folder) = settings.read().repository_folder(target()) else {
            error.set(Some(t!("detail.new_needs_writable_repository").to_string()));
            return;
        };

        let chosen_title = title().trim().to_string();
        let content = match kind() {
            NewFileKind::Song if !guessed().is_empty() => {
                let mut song = guessed().to_song();
                if !chosen_title.is_empty() {
                    song.title = chosen_title.clone();
                }
                match cantara_songlib::exporter::song_yml::song_yml_from_song(&song) {
                    Ok(yml) => yml,
                    Err(reason) => {
                        error.set(Some(reason.to_string()));
                        return;
                    }
                }
            }
            other => other.initial_content(&chosen_title),
        };

        match create(&folder, &file_name_for(&chosen_title, kind()), &content) {
            Ok(_) => {
                // The file is on disk; the list has to be read again before it
                // can be seen.
                library_refresh.request();
                open.set(false);
            }
            Err(problem) => {
                let (key, parameters) = problem.message_key();
                error.set(Some(crate::components::shared_components::translate(
                    key,
                    &parameters,
                )));
            }
        }
    };

    rsx! {
        div { class: "modal-overlay export-menu-overlay", onclick: close,
            div {
                class: "export-menu-modal new-element-modal",
                onclick: move |event: Event<MouseData>| event.stop_propagation(),

                h3 { { t!("detail.new_element").to_string() } }

                fieldset {
                    for offered in NewFileKind::ALL.iter().copied() {
                        label {
                            input {
                                r#type: "radio",
                                name: "new-element-kind",
                                checked: kind() == offered,
                                onchange: move |_| kind.set(offered),
                            }
                            { t!(offered.label_key()).to_string() }
                        }
                    }
                }

                label {
                    { t!("detail.new_title").to_string() }
                    input {
                        r#type: "text",
                        value: "{title}",
                        oninput: move |event| title.set(event.value()),
                    }
                }

                label {
                    { t!("detail.new_repository").to_string() }
                    select {
                        onchange: move |event| {
                            if let Ok(chosen) = event.value().parse::<usize>() {
                                target.set(chosen);
                            }
                        },
                        for (index, name) in targets() {
                            option {
                                value: "{index}",
                                selected: target() == index,
                                "{name}"
                            }
                        }
                    }
                }

                // Only a song has a structure worth guessing at. A Markdown
                // document is written in the editor, where it is already shown
                // the way it will look.
                if kind() == NewFileKind::Song {
                    label {
                        { t!("detail.new_paste_label").to_string() }
                        textarea {
                            rows: "8",
                            placeholder: t!("detail.new_paste_placeholder").to_string(),
                            value: "{pasted}",
                            oninput: move |event| pasted.set(event.value()),
                        }
                    }

                    if !pasted().trim().is_empty() {
                        GuessPreview { guessed: guessed() }
                    }
                }

                if let Some(message) = error() {
                    p { class: "new-element-error", "{message}" }
                }

                div { class: "export-menu-actions",
                    button { class: "secondary", onclick: close,
                        { t!("general.cancel").to_string() }
                    }
                    button {
                        onclick: move |_| save(),
                        { t!("detail.new_create").to_string() }
                    }
                }
            }
        }
    }
}

/// What Cantara made of the pasted text, before anything is written.
///
/// A list rather than an editor: this is here to be checked, not corrected.
/// Everything in it can be changed afterwards in the ordinary editor, and
/// saying so here would be less use than simply showing what was found.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn GuessPreview(guessed: crate::logic::song_text::Guess) -> Element {
    use crate::components::detail_components::tag_label;

    if guessed.is_empty() {
        return rsx! {
            p { class: "new-element-guess-empty",
                { t!("detail.new_paste_nothing_found").to_string() }
            }
        };
    }

    rsx! {
        div { class: "new-element-guess",
            h4 { { t!("detail.new_paste_found").to_string() } }

            if !guessed.tags.is_empty() {
                dl { class: "new-element-guess-tags",
                    for (name, value) in guessed.tags.iter() {
                        dt { { tag_label(name) } }
                        dd { "{value}" }
                    }
                }
            }

            ul {
                for part in guessed.parts.iter() {
                    li {
                        strong {
                            {
                                part.label.clone().unwrap_or_else(|| {
                                    format!("{} {}", part.part_type.as_str(), part.number)
                                })
                            }
                        }
                        // The first line is enough to recognise a verse by,
                        // and a dialog listing every line of every verse is a
                        // wall of text nobody reads.
                        span { class: "new-element-guess-line",
                            { part.lines.first().cloned().unwrap_or_default() }
                        }
                    }
                }
            }
        }
    }
}

/// Moves or copies one file into the repository at `target`, and says what
/// happened.
///
/// A free function rather than a closure in the component: both buttons need
/// it, and a closure that writes to a signal can only be called once from a
/// handler.
#[cfg(not(target_arch = "wasm32"))]
fn carry(
    moving: bool,
    path: &std::path::Path,
    target: usize,
    settings: Signal<Settings>,
    mut message: Signal<Option<String>>,
    mut library_refresh: LibraryRefresh,
) {
    use crate::logic::repository_files::{copy_into, move_into};

    let Some(folder) = settings.read().repository_folder(target) else {
        return;
    };

    let outcome = match moving {
        true => move_into(path, &folder),
        false => copy_into(path, &folder),
    };

    match outcome {
        Ok(_) => {
            // The file has changed place; the list has to be read again.
            library_refresh.request();
            message.set(Some(match moving {
                true => t!("detail.repository_moved").to_string(),
                false => t!("detail.repository_copied").to_string(),
            }));
        }
        Err(problem) => {
            let (key, parameters) = problem.message_key();
            message.set(Some(crate::components::shared_components::translate(
                key,
                &parameters,
            )));
        }
    }
}

/// Moving or copying the open element into another repository.
///
/// Sits with the element rather than in the settings because that is where the
/// question comes up: this song belongs in the other collection. Moving is a
/// move on disk and copying leaves the original where it is — the two are
/// separate buttons rather than a mode, so neither can happen by accident.
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn RepositoryPicker(file: SourceFile) -> Element {
    let settings = use_settings();
    let library_refresh = use_context::<LibraryRefresh>();
    let message: Signal<Option<String>> = use_signal(|| None);

    // The path is what every closure below needs, and `SourceFile` is not
    // `Copy`; taking it out once keeps them from fighting over it.
    let path = use_signal(|| file.path.clone());

    // Where the file could go: every writable repository it is not already in.
    let targets = use_memo(move || {
        let settings = settings.read();
        let here = path.read().parent().map(std::path::Path::to_path_buf);
        writable(&settings)
            .into_iter()
            .filter(|(index, _)| settings.repository_folder(*index) != here)
            .collect::<Vec<(usize, String)>>()
    });

    let mut chosen = use_signal(|| targets().first().map(|(index, _)| *index).unwrap_or(0));

    // A file that is only in one place, with nowhere else to put it.
    if targets().is_empty() {
        return rsx! {};
    }

    rsx! {
        details { class: "repository-picker",
            summary { { t!("detail.repository_section").to_string() } }
            label {
                { t!("detail.repository_target").to_string() }
                select {
                    onchange: move |event| {
                        if let Ok(index) = event.value().parse::<usize>() {
                            chosen.set(index);
                        }
                    },
                    for (index, name) in targets() {
                        option {
                            value: "{index}",
                            selected: chosen() == index,
                            "{name}"
                        }
                    }
                }
            }

            div { class: "repository-picker-actions",
                button {
                    class: "outline",
                    onclick: move |_| {
                        carry(true, &path.read(), chosen(), settings, message, library_refresh);
                    },
                    { t!("detail.repository_move").to_string() }
                }
                button {
                    class: "outline secondary",
                    onclick: move |_| {
                        carry(false, &path.read(), chosen(), settings, message, library_refresh);
                    },
                    { t!("detail.repository_copy").to_string() }
                }
            }

            if let Some(text) = message() {
                p { class: "repository-picker-message", "{text}" }
            }
        }
    }
}

/// A browser has no repositories on disk to move anything between.
#[cfg(target_arch = "wasm32")]
#[component]
pub fn RepositoryPicker(file: SourceFile) -> Element {
    let _ = file;
    rsx! {}
}
