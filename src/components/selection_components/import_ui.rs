//! Opening a selection file: choosing it, seeing what is in it, and deciding
//! what of it should be kept.
//!
//! The formats themselves and everything that can be decided without a user
//! are in [`crate::logic::selection_io`]. What is left here is the part that
//! needs one — and the reason it needs one is that opening somebody else's
//! running order may otherwise write songs into their library and designs into
//! their settings without ever asking.
//!
//! So the file is read in two steps. It is opened and looked at first, and the
//! dialog says what would happen: how many elements are already in the
//! library, how many would be brought along, how many cannot be shown at all.
//! Only the second step, the button, writes anything.

use super::SelectedItemRepresentation;
use crate::components::shared_components::translate;
use crate::logic::selection_io::{
    ImportOutcome, ResolvedSelection, SelectionIoError, import_designs, resolve_selection,
};
use crate::logic::settings_io::{
    ImportedFile, import_design, import_slide_settings, read_import,
};
use crate::logic::slide_summary::{POSITION_SEPARATOR, summary_lines};
use crate::logic::settings::use_settings;
use crate::logic::sourcefiles::SourceFile;
use cantara_songlib::slides::SlideSettings;
use dioxus::prelude::*;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// Where the elements an import brings along are written.
#[derive(Clone, PartialEq, Eq, Debug)]
enum ImportTarget {
    /// Into one of the user's own repositories, where it stays.
    Repository(usize),
    /// Into a folder that belongs to this run of the program: the selection
    /// works today and the library is left alone.
    ThisSessionOnly,
}

/// Turns an error from the reader into a sentence in the user's language.
fn message_of(error: &SelectionIoError) -> String {
    let (key, parameters) = error.message_key();
    let mut message = t!(key).to_string();
    // `t!` needs its parameters at compile time, so they are put in here —
    // which also keeps [`SelectionIoError`] free of anything to do with views.
    for (name, value) in parameters {
        message = message.replace(&format!("%{{{name}}}"), &value);
    }
    message
}

/// Asks the platform for a file and gives back its name and content.
///
/// Only the desktop has a file dialog. Everywhere else the file arrives
/// through an `<input type="file">` in the dialog itself, which is why this
/// returns nothing there.
#[cfg(feature = "desktop")]
fn pick_selection_file() -> Result<Option<(String, Vec<u8>)>, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter(
            t!("selection.import_file_filter").to_string(),
            &["zip", "songtex", "json"],
        )
        .pick_file()
    else {
        // Closing the dialog is not a failure and needs no message.
        return Ok(None);
    };

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let bytes = std::fs::read(&path).map_err(|error| {
        t!(
            "selection.export_error_unreadable",
            name = path.display().to_string(),
            reason = error.to_string()
        )
        .to_string()
    })?;
    Ok(Some((name, bytes)))
}

/// The button that opens a selection file, and the dialog that follows it.
#[component]
pub(crate) fn ImportButton(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    let mut opened: Signal<Option<(String, Vec<u8>)>> = use_signal(|| None);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        button {
            class: "outline secondary smaller-buttons",
            onclick: move |_| {
                error.set(None);
                #[cfg(feature = "desktop")]
                match pick_selection_file() {
                    Ok(Some(file)) => opened.set(Some(file)),
                    Ok(None) => {}
                    Err(message) => error.set(Some(message)),
                }
                // Without a file dialog the dialog itself asks for the file,
                // so the button only has to open it.
                #[cfg(not(feature = "desktop"))]
                opened.set(Some((String::new(), Vec::new())));
            },
            span { class: "mobile-only",
                dioxus_free_icons::Icon {
                    icon: dioxus_free_icons::icons::fa_solid_icons::FaFileImport,
                }
            }
            span { class: "desktop-only", {t!("selection.import").to_string()} }
        }

        if opened.read().is_some() {
            ImportDialog {
                opened,
                selected_items,
            }
        }

        if let Some(message) = error() {
            p { class: "export-save-error", role: "alert", {message} }
        }
    }
}

/// What the file holds, and what keeping it would do.
#[component]
fn ImportDialog(
    /// The file that was opened: its name and its bytes. Empty on a platform
    /// without a file dialog, where the dialog asks for it itself.
    opened: Signal<Option<(String, Vec<u8>)>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    let mut settings = use_settings();
    let source_files: Signal<Vec<SourceFile>> = use_context();

    let mut file: Signal<Option<(String, Vec<u8>)>> = use_signal(|| {
        opened
            .read()
            .clone()
            .filter(|(name, bytes)| !name.is_empty() || !bytes.is_empty())
    });
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut import_assets = use_signal(|| true);
    let mut import_designs_too = use_signal(|| true);
    // The repository chosen last time, as long as it is still one that can be
    // written to; the first that can otherwise. A library of nothing but
    // downloaded repositories has none, and then the only offer is a folder
    // that lasts as long as the program does.
    let target = use_signal(|| {
        let settings = settings.read();
        let writable = settings.writable_repositories();
        let chosen = writable
            .iter()
            .map(|(index, _)| *index)
            .find(|index| *index == settings.import_repository_index)
            .or_else(|| writable.first().map(|(index, _)| *index));

        match chosen {
            Some(index) => ImportTarget::Repository(index),
            None => ImportTarget::ThisSessionOnly,
        }
    });

    // What is in the file, looked at but not acted on. Recomputed whenever
    // another file is opened, which is what makes the dialog usable twice.
    //
    // A running order, a design and a slide division all arrive through this
    // one button, because a user has a file and should not have to know which
    // of Cantara's kinds it is. See [`crate::logic::settings_io::read_import`].
    let document = use_memo(move || {
        let (name, bytes) = file.read().clone()?;
        Some(read_import(&bytes, &name))
    });

    // Only a running order has to be looked up in the library; the other two
    // kinds are settings and stand for themselves.
    let resolved: Memo<Option<ResolvedSelection>> = use_memo(move || {
        let ImportedFile::Selection(document) = document()?.ok()? else {
            return None;
        };
        // A selection file carries divisions, not the names this user gave
        // theirs, so they are compared against the divisions alone.
        let known_divisions: Vec<SlideSettings> = settings
            .read()
            .song_slide_settings
            .iter()
            .map(|named| named.settings.clone())
            .collect();
        Some(resolve_selection(
            &document,
            &source_files.read(),
            &settings.read().presentation_designs,
            &known_divisions,
        ))
    });

    let close = move |_| {
        opened.set(None);
    };

    // Where a file that brings something along may put it: the repository the
    // user picked, or a folder that lasts as long as the program does.
    let target_directory = move || match (&*target.read(), import_assets()) {
        (_, false) | (ImportTarget::ThisSessionOnly, _) => session_directory(),
        (ImportTarget::Repository(index), _) => settings
            .read()
            .repository_folder(*index)
            .or_else(session_directory),
    };

    let keep = move |_| {
        let Some(Ok(opened_file)) = document() else {
            return;
        };

        match opened_file {
            ImportedFile::Selection(_) => {
                let Some(resolved) = resolved() else {
                    return;
                };
                let Some(directory) = target_directory() else {
                    error.set(Some(t!("selection.import_error_no_target").to_string()));
                    return;
                };

                match crate::logic::selection_io::import_selection(&resolved, &directory) {
                    Ok(outcome) => {
                        if import_designs_too() {
                            {
                                let mut settings_write = settings.write();
                                import_designs(&mut settings_write, &resolved);
                            }
                            settings.read().save();
                        }
                        announce(&outcome);
                        selected_items.set(outcome.items);
                        opened.set(None);
                    }
                    Err(reason) => error.set(Some(message_of(&reason))),
                }
            }

            ImportedFile::Design(package) => {
                // The background picture goes into a repository, so that it is
                // also a picture the user can reach elsewhere.
                let Some(directory) = target_directory() else {
                    error.set(Some(t!("selection.import_error_no_target").to_string()));
                    return;
                };

                let outcome = {
                    let mut settings_write = settings.write();
                    import_design(&mut settings_write, &package, &directory)
                };
                match outcome {
                    Ok(outcome) => {
                        settings.read().save();
                        log::info!(
                            "imported a design; kept {} font(s), added: {}",
                            outcome.fonts_kept.len(),
                            outcome.added
                        );
                        opened.set(None);
                    }
                    Err(reason) => error.set(Some(message_of(&reason))),
                }
            }

            ImportedFile::SlideSettings(division) => {
                {
                    let mut settings_write = settings.write();
                    import_slide_settings(&mut settings_write, &division);
                }
                settings.read().save();
                opened.set(None);
            }
        }
    };

    rsx! {
        div { class: "modal-overlay export-menu-overlay", onclick: close,
            div {
                class: "export-menu-modal import-modal",
                onclick: move |event: Event<MouseData>| event.stop_propagation(),
                h3 { {t!("selection.import_title").to_string()} }

                // Only where the platform has no file dialog: the file is
                // asked for here instead.
                if cfg!(not(feature = "desktop")) {
                    label {
                        {t!("selection.import_choose_file").to_string()}
                        input {
                            r#type: "file",
                            accept: ".zip,.songtex,.json",
                            onchange: move |event: Event<FormData>| async move {
                                let Some(chosen) = event.files().first().cloned() else {
                                    return;
                                };
                                match chosen.read_bytes().await {
                                    Ok(bytes) => file.set(Some((chosen.name(), bytes.to_vec()))),
                                    Err(reason) => {
                                        error.set(Some(reason.to_string()));
                                    }
                                }
                            },
                        }
                    }
                }

                match document() {
                    None => rsx! {
                        p { {t!("selection.import_choose_file").to_string()} }
                    },
                    Some(Err(reason)) => rsx! {
                        p { class: "export-save-error", role: "alert", {message_of(&reason)} }
                    },

                    // One design, with whatever it needs to look right.
                    Some(Ok(ImportedFile::Design(package))) => {
                        let fonts = package.fonts_to_keep();
                        rsx! {
                            ul { class: "import-summary",
                                li {
                                    {
                                        t!(
                                            "selection.import_summary_design",
                                            name = package.design.name.clone(),
                                        )
                                            .to_string()
                                    }
                                }
                                if package.background_image.is_some() {
                                    li { {t!("selection.import_summary_background").to_string()} }
                                }
                                if !fonts.is_empty() {
                                    li {
                                        {
                                            t!(
                                                "selection.import_summary_fonts",
                                                count = fonts.len(),
                                                families = fonts.join(", "),
                                            )
                                                .to_string()
                                        }
                                    }
                                }
                            }
                            if package.background_image.is_some() {
                                label {
                                    {t!("selection.import_target").to_string()}
                                    ImportTargetSelect { target, settings }
                                }
                            }
                        }
                    }

                    // One slide division: what it does, in the same words the
                    // settings page uses for the ones the user already has.
                    Some(Ok(ImportedFile::SlideSettings(division))) => rsx! {
                        ul { class: "import-summary",
                            li {
                                {
                                    t!(
                                        "selection.import_summary_slide_settings",
                                        name = division.display_name(0),
                                    )
                                        .to_string()
                                }
                            }
                            for (key , parameters) in summary_lines(&division.settings) {
                                li { key: "{key}", {summary_sentence(key, &parameters)} }
                            }
                        }
                    },

                    Some(Ok(ImportedFile::Selection(_))) => {
                        let resolved = resolved().unwrap_or_default();
                        let in_library = resolved.items.len()
                            - resolved.new_asset_count()
                            - resolved.missing_count();
                        rsx! {
                            ul { class: "import-summary",
                                li {
                                    {
                                        t!(
                                            "selection.import_summary_items",
                                            count = resolved.items.len(),
                                        )
                                            .to_string()
                                    }
                                }
                                li {
                                    {t!("selection.import_summary_known", count = in_library).to_string()}
                                }
                                if resolved.new_asset_count() > 0 {
                                    li {
                                        {
                                            t!(
                                                "selection.import_summary_new",
                                                count = resolved.new_asset_count(),
                                            )
                                                .to_string()
                                        }
                                    }
                                }
                                if resolved.missing_count() > 0 {
                                    li { class: "import-summary-missing",
                                        {
                                            t!(
                                                "selection.import_summary_missing",
                                                count = resolved.missing_count(),
                                            )
                                                .to_string()
                                        }
                                    }
                                }
                                if !resolved.new_designs.is_empty()
                                    || !resolved.new_slide_settings.is_empty()
                                {
                                    li {
                                        {
                                            t!(
                                                "selection.import_summary_designs",
                                                designs = resolved.new_designs.len(),
                                                slide_settings = resolved.new_slide_settings.len(),
                                            )
                                                .to_string()
                                        }
                                    }
                                }
                            }

                            if resolved.new_asset_count() > 0 {
                                fieldset {
                                    label {
                                        input {
                                            r#type: "checkbox",
                                            role: "switch",
                                            checked: import_assets(),
                                            onchange: move |event: Event<FormData>| {
                                                import_assets.set(event.checked());
                                            },
                                        }
                                        {t!("selection.import_assets").to_string()}
                                    }

                                    if import_assets() {
                                        label {
                                            {t!("selection.import_target").to_string()}
                                            ImportTargetSelect { target, settings }
                                        }
                                    }
                                }
                            }

                            if !resolved.new_designs.is_empty()
                                || !resolved.new_slide_settings.is_empty()
                            {
                                fieldset {
                                    label {
                                        input {
                                            r#type: "checkbox",
                                            role: "switch",
                                            checked: import_designs_too(),
                                            onchange: move |event: Event<FormData>| {
                                                import_designs_too.set(event.checked());
                                            },
                                        }
                                        {t!("selection.import_designs").to_string()}
                                    }
                                    small { {t!("selection.import_designs_hint").to_string()} }
                                }
                            }
                        }
                    }
                }

                if let Some(message) = error() {
                    p { class: "export-save-error", role: "alert", {message} }
                }

                div { class: "export-menu-actions",
                    button { class: "outline secondary", onclick: close,
                        {t!("general.cancel").to_string()}
                    }
                    button {
                        class: "primary",
                        disabled: !matches!(document(), Some(Ok(_))),
                        onclick: keep,
                        {t!("selection.import_confirm").to_string()}
                    }
                }
            }
        }
    }
}

/// A folder that belongs to this run of the program.
///
/// What is written here is readable for as long as Cantara is open and is
/// cleaned up by the operating system afterwards, which is what "just let me
/// look at it" needs: the selection works, and nothing of the user's is
/// touched.
#[cfg(not(target_arch = "wasm32"))]
fn session_directory() -> Option<std::path::PathBuf> {
    let directory = std::env::temp_dir().join("cantara-imported-selection");
    std::fs::create_dir_all(&directory).ok()?;
    Some(directory)
}

/// The web build has no file system, so there is nowhere to put an element the
/// library does not have. Its import keeps what it can find and says how much
/// it had to leave out.
#[cfg(target_arch = "wasm32")]
fn session_directory() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::new())
}

/// Says in the log what an import came to.
///
/// Not a dialog: the running order is on screen the moment this returns, and
/// what came of the import is visible in it.
fn announce(outcome: &ImportOutcome) {
    log::info!(
        "imported {} element(s), wrote {} file(s), left out {}",
        outcome.items.len(),
        outcome.written.len(),
        outcome.left_out
    );
}

/// One line of a slide-division summary, in the reader's language.
///
/// The same as on the settings page — see
/// [`crate::components::song_slide_settings_components`] — because what the
/// division does should read the same wherever it is shown.
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

/// Where an import may put what it brings along.
///
/// Its own component because two kinds of file need it: a running order writes
/// songs, a design writes its background picture, and both go wherever the
/// user says.
#[component]
fn ImportTargetSelect(
    target: Signal<ImportTarget>,
    settings: Signal<crate::logic::settings::Settings>,
) -> Element {
    let mut settings = settings;
    rsx! {
        select {
            onchange: move |event: Event<FormData>| {
                match event.value().parse::<usize>() {
                    Ok(index) => {
                        target.set(ImportTarget::Repository(index));
                        settings.write().import_repository_index = index;
                        settings.read().save();
                    }
                    Err(_) => target.set(ImportTarget::ThisSessionOnly),
                }
            },
            for (index , repository) in settings.read().writable_repositories() {
                option {
                    value: "{index}",
                    selected: *target.read() == ImportTarget::Repository(index),
                    "{repository.name}"
                }
            }
            option {
                value: "session",
                selected: *target.read() == ImportTarget::ThisSessionOnly,
                {t!("selection.import_target_session").to_string()}
            }
        }
    }
}
