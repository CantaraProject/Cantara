//! Exporting the selection: the menu, the formats, and where the files go.
//!
//! The selection view offers this; what a format *is* and how a document is
//! built lives in [`crate::logic::export`] and [`crate::logic::pptx`]. What is
//! left here is the part that needs a user: which format they picked, where
//! the file should go, and what to tell them when it did not work.
//!
//! Saving differs per platform and that is the reason this is not one
//! function: the desktop opens a file dialog, every other build hands the file
//! to the platform as a download.

use super::{ExportError, ExportFormat, ExportedFile, SelectedItemRepresentation, Song, SourceFile, SourceFileType, song_from_content};
use crate::logic::export::ExportCategory;
use crate::logic::pptx::{PptxConversion, PptxDeck, deck_from_slides};
use crate::logic::selection_io::{
    SelectionFile, SelectionFormat, SelectionIoError, write_selection,
};
use crate::logic::settings::use_settings;
use dioxus::prelude::*;
use rust_i18n::t;
use std::collections::HashMap;

rust_i18n::i18n!("locales", fallback = "en");

const PPTX_EXPORT_JS: &str = include_str!("../../../assets/pptx_export_inline.js");
/// PptxGenJS is bundled from `node_modules` so that an export works offline.
///
/// Minification has to stay off: the bundle publishes itself with a top-level
/// `var PptxGenJS = ...`, which only becomes a global because a classic script
/// puts its `var`s on `window`. The minifier renames it — the file then loads
/// without error and registers nothing at all.
#[cfg(not(target_arch = "wasm32"))]
const PPTXGEN_LIB: Asset = asset!(
    "/node_modules/pptxgenjs/dist/pptxgen.bundle.js",
    AssetOptions::js().with_minify(false)
);
/// The web build has no `node_modules`, so it takes the library from a CDN.
#[cfg(target_arch = "wasm32")]
const PPTXGEN_CDN_LIB: &str = "https://cdn.jsdelivr.net/npm/pptxgenjs@4.0.1/dist/pptxgen.bundle.js";

/// Read and parse every song of the selection.
///
/// Items that are not songs — pictures, PDFs, Markdown — are skipped rather
/// than reported: a selection usually mixes them, and the user asked to export
/// the songs.
fn songs_of_selection(
    selected_items: &[SelectedItemRepresentation],
) -> Result<Vec<Song>, ExportError> {
    selected_items
        .iter()
        .filter(|item| item.source_file.file_type == SourceFileType::Song)
        .map(|item| {
            let content = read_source_file_content(&item.source_file)?;
            song_from_content(item.source_file.file_name(), &content)
        })
        .collect()
}

fn read_source_file_content(source_file: &SourceFile) -> Result<String, ExportError> {
    crate::logic::sourcefiles::read_source_file(source_file).map_err(|reason| {
        ExportError::Unreadable {
            name: source_file.name.clone(),
            reason,
        }
    })
}

/// The same for an error from [`crate::logic::selection_io`].
///
/// Its errors carry their parameters rather than their sentence, so that the
/// module stays free of anything to do with views; putting them in is this
/// side's job.
fn selection_error_message(error: &SelectionIoError) -> String {
    let (key, parameters) = error.message_key();
    let mut message = t!(key).to_string();
    for (name, value) in parameters {
        message = message.replace(&format!("%{{{name}}}"), &value);
    }
    message
}

/// Turn an [`ExportError`] into a message in the user's language.
fn export_error_message(error: &ExportError) -> String {
    match error {
        ExportError::NoSongs => t!("selection.export_error_no_songs").to_string(),
        ExportError::Unreadable { name, reason } => {
            t!("selection.export_error_unreadable", name = name, reason = reason).to_string()
        }
        ExportError::UnsupportedFormat { name } => {
            t!("selection.export_error_unsupported", name = name).to_string()
        }
        ExportError::Unparsable { name, reason } => {
            t!("selection.export_error_unparsable", name = name, reason = reason).to_string()
        }
        ExportError::SongFailed { title, reason } => {
            t!("selection.export_error_song_failed", title = title, reason = reason).to_string()
        }
        ExportError::Render(reason) => {
            t!("selection.export_error_render", reason = reason).to_string()
        }
        ExportError::Write { path, reason } => {
            t!("selection.export_error_write", path = path, reason = reason).to_string()
        }
    }
}

/// How a save attempt ended.
enum SaveOutcome {
    /// Files were written.
    Written(usize),
    /// The user closed the file dialog without choosing.
    ///
    /// Only a build with a file dialog can be cancelled; the others hand the
    /// file straight to the platform, which is why this is never constructed
    /// there — the arm that handles it is shared by all of them.
    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    Cancelled,
}

/// Write the rendered files, asking the user where they should go.
///
/// A format that produces one file per song asks for a directory; a single
/// document asks for a file name.
///
/// Only the desktop has a native file dialog — `rfd` is a desktop-only
/// dependency. Mobile and the web hand the file to the WebView instead, which
/// is why the split is by feature rather than by `wasm32`: Android is neither
/// wasm nor a desktop, and gating on the target arch alone compiled `rfd` into
/// a build that does not have it.
#[cfg(feature = "desktop")]
fn save_exported_files(
    files: &[ExportedFile],
    format: ExportFormat,
) -> Result<SaveOutcome, ExportError> {
    let write = |path: &std::path::Path, content: &str| {
        std::fs::write(path, content).map_err(|error| ExportError::Write {
            path: path.display().to_string(),
            reason: error.to_string(),
        })
    };

    if format.one_file_per_song() && files.len() > 1 {
        let Some(directory) = rfd::FileDialog::new()
            .set_title(t!("selection.export_choose_folder").to_string())
            .pick_folder()
        else {
            return Ok(SaveOutcome::Cancelled);
        };

        for file in files {
            write(
                &directory.join(format!("{}.{}", file.name, format.extension())),
                &file.content,
            )?;
        }
        return Ok(SaveOutcome::Written(files.len()));
    }

    let Some(file) = files.first() else {
        return Ok(SaveOutcome::Written(0));
    };

    let Some(path) = rfd::FileDialog::new()
        .set_file_name(format!("{}.{}", file.name, format.extension()))
        .save_file()
    else {
        return Ok(SaveOutcome::Cancelled);
    };

    write(&path, &file.content)?;
    Ok(SaveOutcome::Written(1))
}

/// Without a file dialog — the web and mobile — each file is offered to the
/// WebView as a download, which is where the platform then puts it.
#[cfg(not(feature = "desktop"))]
fn save_exported_files(
    files: &[ExportedFile],
    format: ExportFormat,
) -> Result<SaveOutcome, ExportError> {
    for file in files {
        let name = serde_json::to_string(&format!("{}.{}", file.name, format.extension()))
            .map_err(|error| ExportError::Render(error.to_string()))?;
        let content = serde_json::to_string(&file.content)
            .map_err(|error| ExportError::Render(error.to_string()))?;

        spawn(async move {
            let js = format!(
                r#"
                (function() {{
                    const blob = new Blob([{content}], {{ type: 'text/plain;charset=utf-8' }});
                    const url = URL.createObjectURL(blob);
                    const link = document.createElement('a');
                    link.href = url;
                    link.download = {name};
                    document.body.appendChild(link);
                    link.click();
                    document.body.removeChild(link);
                    URL.revokeObjectURL(url);
                }})();
                "#
            );
            let _ = document::eval(&js).await;
        });
    }

    Ok(SaveOutcome::Written(files.len()))
}

/// Writes the current selection as a PowerPoint deck.
///
/// The file is built by PptxGenJS from the deck description in
/// [`crate::logic::pptx`], so no native PowerPoint library is needed.
///
/// Where the bytes go differs by target, exactly as it does for the text
/// formats: the desktop asks Rust to write the file to a path the user picked,
/// the web lets the browser download it. Letting PptxGenJS download on the
/// desktop does not work — the WebView drops the `<a download>` click without
/// an error, so the export reports success and produces no file.
fn export_pptx(
    deck: &PptxDeck,
    file_name: &str,
    mut message: Signal<Option<String>>,
    mut close_dialog: Signal<bool>,
) {
    // On the desktop the user picks the destination first: it is the one step
    // that can be cancelled, and doing it before the work avoids building a
    // deck nobody asked to keep.
    #[cfg(feature = "desktop")]
    let target_path = {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(file_name)
            .save_file()
        else {
            return;
        };
        path
    };

    let Ok(deck_json) = serde_json::to_string(deck) else {
        message.set(Some(t!("selection.export_error_render", reason = "deck").to_string()));
        return;
    };
    let Ok(name_json) = serde_json::to_string(file_name) else {
        return;
    };
    let Ok(url_json) = serde_json::to_string(&pptxgen_url()) else {
        return;
    };

    // Only the desktop takes the bytes back into Rust to write them itself;
    // everywhere else the WebView receives the file directly.
    let mode = if cfg!(feature = "desktop") {
        "\"base64\""
    } else {
        "\"download\""
    };

    let script = PPTX_EXPORT_JS
        .replace("__DECK__", &deck_json)
        .replace("__FILE_NAME__", &name_json)
        .replace("__PPTXGEN_URL__", &url_json)
        .replace("__MODE__", mode);

    spawn(async move {
        // The shim returns `{ok, error, data}`; anything else means the
        // evaluation itself broke. Either way the user hears about it — a file
        // that never arrives is otherwise indistinguishable from a dead button.
        let outcome = match document::eval(&script).await {
            Ok(value) => {
                if value.get("ok").and_then(|ok| ok.as_bool()) == Some(true) {
                    Ok(value
                        .get("data")
                        .and_then(|data| data.as_str())
                        .unwrap_or_default()
                        .to_string())
                } else {
                    Err(value
                        .get("error")
                        .and_then(|error| error.as_str())
                        .unwrap_or("unknown")
                        .to_string())
                }
            }
            Err(error) => Err(format!("{error:?}")),
        };

        #[cfg(feature = "desktop")]
        let outcome = outcome.and_then(|data| write_pptx_file(&data, &target_path));

        match outcome {
            Ok(_) => close_dialog.set(false),
            Err(reason) => {
                log::error!("could not write the PowerPoint file: {reason}");
                message.set(Some(
                    t!("selection.export_error_render", reason = reason).to_string(),
                ));
            }
        }
    });
}

/// Decodes the deck PptxGenJS produced and writes it to `path`.
#[cfg(feature = "desktop")]
fn write_pptx_file(base64_data: &str, path: &std::path::Path) -> Result<String, String> {
    use base64::Engine;

    if base64_data.is_empty() {
        return Err("PptxGenJS returned no data".to_string());
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|error| error.to_string())?;

    std::fs::write(path, bytes).map_err(|error| error.to_string())?;
    Ok(path.display().to_string())
}

/// Writes a selection file wherever the platform puts files.
fn save_selection_file(file: &SelectionFile) -> Result<SaveOutcome, String> {
    match crate::components::shared_components::save_file(&file.name, &file.bytes)? {
        true => Ok(SaveOutcome::Written(1)),
        false => Ok(SaveOutcome::Cancelled),
    }
}

/// Where PptxGenJS is loaded from.
fn pptxgen_url() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("{}", PPTXGEN_LIB)
    }
    #[cfg(target_arch = "wasm32")]
    {
        PPTXGEN_CDN_LIB.to_string()
    }
}

/// Copy a string to the system clipboard.
///
/// The desktop build goes through the webview so that the same code path works
/// on every platform Cantara ships on.
fn copy_to_clipboard(text: &str) {
    let Ok(payload) = serde_json::to_string(text) else {
        return;
    };

    spawn(async move {
        let script = format!(
            r#"
            (async function() {{
                const text = {payload};
                try {{
                    await navigator.clipboard.writeText(text);
                }} catch (error) {{
                    // Older webviews have no async clipboard API.
                    const area = document.createElement('textarea');
                    area.value = text;
                    area.style.position = 'fixed';
                    area.style.opacity = '0';
                    document.body.appendChild(area);
                    area.select();
                    document.execCommand('copy');
                    document.body.removeChild(area);
                }}
            }})();
            "#
        );
        let _ = document::eval(&script).await;
    });
}

#[component]
pub(crate) fn ExportMenu(
    /// Signal to control the visibility of the export menu
    show_export_menu: Signal<bool>,
    /// The currently selected items to export
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    let settings = use_settings();
    // What is being exported. The dialog opens on the running order: it is
    // what a service is put together for, and the one thing that can be
    // opened again afterwards.
    let mut category: Signal<ExportCategory> = use_signal(|| ExportCategory::Selection);
    let mut export_format: Signal<ExportFormat> = use_signal(|| ExportFormat::PlainText);
    let mut template: Signal<String> =
        use_signal(|| ExportFormat::default_template().to_string());
    let mut copied = use_signal(|| false);

    let song_count = use_memo(move || {
        selected_items
            .read()
            .iter()
            .filter(|item| item.source_file.file_type == SourceFileType::Song)
            .count()
    });

    // The pictures the deck needs, rendered as they become available.
    //
    // A slide that is a picture — or a page of a PDF, which is one — cannot be
    // turned into a shape: it has to be *rendered*, and for a PDF that means
    // asking the viewer in the page, which is asynchronous. So they are
    // collected here and handed to the deck builder, which stays a pure
    // translation that can be tested without a browser.
    //
    // Nothing has to be on screen for this. The page is drawn beside whatever
    // the window is showing and comes back as a picture, which is what lets a
    // deck be exported without the presentation ever being started.
    let mut pictures: Signal<HashMap<String, String>> = use_signal(HashMap::new);

    let wanted_pictures: Memo<Vec<String>> = use_memo(move || {
        if *export_format.read() != ExportFormat::Pptx {
            return Vec::new();
        }
        // The same design and division a presentation would use, so what is
        // exported is what the audience would have seen.
        let design = settings.read().default_presentation_design();
        let slide_settings = settings.read().default_song_slide_settings();
        crate::logic::presentation::build_presentation(
            &selected_items.read(),
            &design,
            &slide_settings,
            // An export is the projection on paper. What the phones were shown
            // has no bearing on it.
            &crate::logic::stream_view::StreamDefaults::default(),
            &settings.read().tag_mappings,
        )
        .map(|presentation| {
            let slides: Vec<_> = presentation
                .presentation
                .iter()
                .flat_map(|chapter| chapter.slides.clone())
                .collect();
            crate::logic::pptx::pictures_needed(&slides)
        })
        .unwrap_or_default()
    });

    use_effect(move || {
        let wanted = wanted_pictures();
        spawn(async move {
            for key in wanted {
                if pictures.peek().contains_key(&key) {
                    continue;
                }
                // A PDF names a page; a video is carried whole or as a still;
                // anything else is a picture file.
                let rendered = if crate::logic::sourcefiles::SourceFileType::of(&key)
                    == Some(crate::logic::sourcefiles::SourceFileType::Video)
                {
                    video_for_deck(&key).await
                } else {
                    match crate::logic::pdf::pdf_page_of(&key) {
                        Some((document, page)) => {
                            crate::logic::pdf::page_image(
                                &document,
                                page,
                                crate::logic::pdf::EXPORT_WIDTH,
                            )
                            .await
                        }
                        None => crate::logic::images::image_data_url(std::path::Path::new(&key)),
                    }
                };
                if let Some(data) = rendered {
                    pictures.write().insert(key, data);
                }
            }
        });
    });

    // A PowerPoint deck is built from the presentation slides rather than from
    // the songs, so it takes its own path through the design.
    let conversion: Memo<Option<PptxConversion>> = use_memo(move || {
        if *export_format.read() != ExportFormat::Pptx {
            return None;
        }
        // The same design and division a presentation would use, so what is
        // exported is what the audience would have seen.
        let design = settings.read().default_presentation_design();
        let slide_settings = settings.read().default_song_slide_settings();

        crate::logic::presentation::build_presentation(
            &selected_items.read(),
            &design,
            &slide_settings,
            // An export is the projection on paper. What the phones were shown
            // has no bearing on it.
            &crate::logic::stream_view::StreamDefaults::default(),
            &settings.read().tag_mappings,
        )
        .map(|presentation| {
            let slides: Vec<_> = presentation
                .presentation
                .iter()
                .flat_map(|chapter| chapter.slides.clone())
                .collect();
            deck_from_slides(&slides, &design, &pictures.read())
        })
    });

    // The document is rendered as soon as the dialog opens and again whenever
    // the format or the template changes, so the preview always shows what
    // would be saved — and a format that cannot be produced says why instead of
    // doing nothing when the button is pressed.
    let rendered: Memo<Result<Vec<ExportedFile>, String>> = use_memo(move || {
        let format = *export_format.read();

        if format == ExportFormat::Pptx {
            return match &*conversion.read() {
                Some(conversion) if !conversion.deck.is_empty() => {
                    let mut note =
                        t!("selection.export_pptx_note", count = conversion.deck.slides.len())
                            .to_string();
                    // PowerPoint has no way to draw a staff, so a notation
                    // layout silently loses it. Say so rather than let the user
                    // discover it in the finished file.
                    if conversion.skipped_notation > 0 {
                        note.push('\n');
                        note.push_str(&t!(
                            "selection.export_pptx_notation_note",
                            count = conversion.skipped_notation
                        ));
                    }
                    // A picture slide whose picture is not there yet. Saying so
                    // is the difference between waiting a moment and saving a
                    // deck with blank slides in it without knowing why.
                    if conversion.missing_pictures > 0 {
                        note.push('\n');
                        note.push_str(&t!(
                            "selection.export_pptx_pictures_pending",
                            count = conversion.missing_pictures
                        ));
                    }
                    // A video that could not be carried at all — unreadable,
                    // or too large for a deck and no frame of it could be
                    // taken either. The slide is not in the deck, and silence
                    // about that is how a service is rehearsed from a file
                    // that is missing something.
                    if conversion.skipped_videos > 0 {
                        note.push('\n');
                        note.push_str(&t!(
                            "selection.export_pptx_videos_skipped",
                            count = conversion.skipped_videos
                        ));
                    }
                    Ok(vec![ExportedFile {
                        name: "cantara-presentation".to_string(),
                        content: format!("{}\n\n{}", note, conversion.deck.to_json()),
                    }])
                }
                _ => Err(export_error_message(&ExportError::NoSongs)),
            };
        }

        let selected = selected_items.read().clone();
        let template = template.read().clone();
        songs_of_selection(&selected)
            .and_then(|songs| format.render_with(&songs, &template))
            .map_err(|error| export_error_message(&error))
    });

    let preview_text = use_memo(move || match &*rendered.read() {
        Ok(files) => files
            .first()
            .map(|file| file.content.clone())
            .unwrap_or_default(),
        Err(message) => message.clone(),
    });

    // Shown in the dialog when a save fails, so the dialog stays open with a
    // reason instead of quietly doing nothing.
    let mut save_error: Signal<Option<String>> = use_signal(|| None);

    // The running order as a file, which is a different thing from the songs
    // as a document — see [`crate::logic::selection_io`].
    let mut selection_format: Signal<SelectionFormat> =
        use_signal(|| SelectionFormat::CantaraZip);

    // One button exports, whatever the dialog is showing — see the mockup:
    // the user says *what* on the left and confirms at the bottom right. What
    // that means differs per category, and this is where it is decided.
    let handle_export = move |_| {
        save_error.set(None);

        if *category.read() == ExportCategory::Selection {
            let written = write_selection(
                &selected_items.read(),
                *selection_format.read(),
                t!("selection.save_selection_file_name").as_ref(),
                &|file: &SourceFile| crate::logic::sourcefiles::read_source_file_bytes(file),
            );

            let outcome = match written {
                Ok(file) => save_selection_file(&file),
                Err(error) => Err(selection_error_message(&error)),
            };

            match outcome {
                Ok(SaveOutcome::Written(_)) => show_export_menu.set(false),
                Ok(SaveOutcome::Cancelled) => {}
                Err(message) => {
                    log::warn!("the selection could not be saved: {message}");
                    save_error.set(Some(message));
                }
            }
            return;
        }

        let format = *export_format.read();

        // The binary formats are written by the browser, not by Rust.
        if format.is_binary() {
            if let Some(conversion) = conversion.read().clone() {
                export_pptx(
                    &conversion.deck,
                    "cantara-presentation.pptx",
                    save_error,
                    show_export_menu,
                );
            }
            return;
        }

        let outcome = match &*rendered.read() {
            Ok(files) => {
                save_exported_files(files, format).map_err(|error| export_error_message(&error))
            }
            Err(message) => Err(message.clone()),
        };

        match outcome {
            Ok(SaveOutcome::Written(count)) => {
                log::info!("exported {count} file(s) as {}", format.id());
                show_export_menu.set(false);
            }
            // Closing the file dialog is not a failure and needs no message.
            Ok(SaveOutcome::Cancelled) => {}
            Err(message) => {
                log::warn!("export failed: {message}");
                save_error.set(Some(message));
            }
        }
    };

    // Whether the button at the bottom could do anything at all.
    let can_export = move || match *category.read() {
        ExportCategory::Selection => !selected_items.read().is_empty(),
        _ => rendered.read().is_ok(),
    };

    rsx! {
        div {
            class: "modal-overlay export-menu-overlay",
            onclick: move |_| {
                show_export_menu.set(false);
            },
            div {
                class: "export-menu-modal",
                onclick: move |event: Event<MouseData>| {
                    event.stop_propagation();
                },
                h3 { {t!("selection.export_dialog_title").to_string()} }

                div { class: "export-menu-body",
                    // *What* is being exported, first: sheet music and a
                    // running order are different errands, and the settings
                    // that follow only make sense once that is decided.
                    nav {
                        class: "export-category-list",
                        aria_label: t!("selection.export_dialog_title").to_string(),
                        for option in ExportCategory::ALL {
                            button {
                                r#type: "button",
                                key: "{option.label_key()}",
                                class: if *category.read() == *option {
                                    "export-category export-category-active"
                                } else {
                                    "export-category"
                                },
                                aria_pressed: (*category.read() == *option).to_string(),
                                onclick: move |_| {
                                    category.set(*option);
                                    // Each category starts on its own first
                                    // format, so what is shown on the right
                                    // always belongs to what is chosen on the
                                    // left.
                                    if let Some(format) = option.first_format() {
                                        export_format.set(format);
                                    }
                                    copied.set(false);
                                    save_error.set(None);
                                },
                                {t!(option.label_key()).to_string()}
                            }
                        }
                    }

                    div { class: "export-category-pane",
                        p { class: "export-category-description",
                            {t!(category.read().description_key()).to_string()}
                        }

                        match *category.read() {
                            ExportCategory::Selection => rsx! {
                                label {
                                    {t!("selection.save_selection_format").to_string()}
                                    select {
                                        value: selection_format.read().id(),
                                        onchange: move |event| {
                                            if let Some(format) = SelectionFormat::of_id(&event.value()) {
                                                selection_format.set(format);
                                            }
                                        },
                                        for format in SelectionFormat::ALL {
                                            option {
                                                value: format.id(),
                                                {t!(format.label_key()).to_string()}
                                            }
                                        }
                                    }
                                }

                                if selection_format.read().holds_only_songs() {
                                    small { {t!("selection.save_selection_songs_only").to_string()} }
                                }

                                p { class: "export-category-summary",
                                    {
                                        t!(
                                            "selection.save_selection_summary",
                                            count = selected_items.read().len(),
                                            songs = song_count(),
                                        )
                                            .to_string()
                                    }
                                }
                            },

                            // Everything else renders a document, so the pane
                            // is the format, whatever that format needs, and
                            // what would come out of it.
                            chosen => rsx! {
                                if chosen.has_format_choice() {
                                    label {
                                        {t!("selection.export_format").to_string()}
                                        select {
                                            value: export_format.read().id(),
                                            onchange: move |event| {
                                                if let Some(format) = ExportFormat::from_id(&event.value()) {
                                                    export_format.set(format);
                                                    copied.set(false);
                                                }
                                            },
                                            for format in chosen.formats() {
                                                option {
                                                    value: format.id(),
                                                    {t!(format.label_key()).to_string()}
                                                }
                                            }
                                        }
                                    }
                                }

                                if export_format.read().needs_template() {
                                    label {
                                        {t!("selection.export_template").to_string()}
                                        textarea {
                                            class: "export-template-input",
                                            rows: "6",
                                            spellcheck: false,
                                            value: "{template}",
                                            oninput: move |event| template.set(event.value()),
                                        }
                                        small { {t!("selection.export_template_hint").to_string()} }
                                    }
                                }

                                if let Ok(files) = &*rendered.read() {
                                    if files.len() > 1 {
                                        small {
                                            {
                                                t!("selection.export_files_note", count = files.len())
                                                    .to_string()
                                            }
                                        }
                                    }
                                }

                                div { class: "export-menu-preview",
                                    label { {t!("selection.export_preview").to_string()} }
                                    textarea {
                                        class: if rendered.read().is_err() { "export-preview-text has-error" } else { "export-preview-text" },
                                        readonly: true,
                                        spellcheck: false,
                                        wrap: "off",
                                        value: "{preview_text}",
                                    }
                                    button {
                                        r#type: "button",
                                        class: "outline",
                                        disabled: rendered.read().is_err(),
                                        onclick: move |_| {
                                            copy_to_clipboard(&preview_text.read());
                                            copied.set(true);
                                        },
                                        if copied() {
                                            {t!("selection.export_copied").to_string()}
                                        } else {
                                            {t!("selection.export_copy").to_string()}
                                        }
                                    }
                                }
                            },
                        }
                    }
                }

                if let Some(message) = save_error() {
                    p { class: "export-save-error", role: "alert", {message} }
                }

                div { class: "export-menu-actions",
                    button {
                        class: "outline secondary",
                        onclick: move |_| {
                            show_export_menu.set(false);
                        },
                        {t!("general.cancel").to_string()}
                    }
                    button {
                        class: "primary",
                        disabled: !can_export(),
                        onclick: handle_export,
                        {t!("selection.export_save").to_string()}
                    }
                }
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("cantara-pptx-test-{name}.pptx"))
    }

    /// The bytes PptxGenJS produced have to reach the disk unchanged — a PPTX
    /// is a ZIP, and one wrong byte makes PowerPoint refuse the whole file.
    #[test]
    fn test_the_decoded_deck_is_written_verbatim() {
        // A minimal ZIP: the signature PowerPoint looks for first.
        let content = b"PK\x03\x04hello";
        let encoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(content)
        };
        let path = temp_path("verbatim");

        write_pptx_file(&encoded, &path).expect("the file should be written");

        let written = std::fs::read(&path).expect("the file should exist");
        assert_eq!(written, content);
        assert_eq!(&written[..2], b"PK", "the ZIP signature was mangled");

        let _ = std::fs::remove_file(&path);
    }

    /// An export that produced nothing must say so instead of leaving a
    /// zero-byte file behind that PowerPoint cannot open.

    #[test]
    fn test_empty_data_is_refused() {
        let path = temp_path("empty");

        assert!(write_pptx_file("", &path).is_err());
        assert!(!path.exists(), "an empty file was left behind");
    }

    #[test]
    fn test_damaged_data_is_refused() {
        let path = temp_path("damaged");

        assert!(write_pptx_file("not base64 !!!", &path).is_err());
        assert!(!path.exists(), "a damaged file was left behind");
    }
}

/// What a deck should carry for the video at `path`: the film itself, or a
/// still of it.
///
/// The film, when PowerPoint can play the format and the file is small enough
/// to travel inside a `.pptx` — a deck that plays the video is what was asked
/// for. Otherwise a frame of it, which is the most a deck can honestly show of
/// a video it cannot play. `None` when neither could be had, and the export
/// then says a video was left out.
///
/// The size limit is not fussiness: the bytes reach PptxGenJS as base64 inside
/// a JSON document, so a hundred-megabyte film is something like a hundred and
/// forty megabytes of text to build, hold and parse. See
/// [`MAX_EMBEDDED_VIDEO_BYTES`](crate::logic::pptx::MAX_EMBEDDED_VIDEO_BYTES).
#[cfg(not(target_arch = "wasm32"))]
async fn video_for_deck(path: &str) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let small_enough = std::fs::metadata(path)
        .map(|data| data.len() <= crate::logic::pptx::MAX_EMBEDDED_VIDEO_BYTES)
        .unwrap_or(false);

    if small_enough && crate::logic::pptx::powerpoint_can_play(path)
        && let Ok(bytes) = std::fs::read(path)
    {
        let mime = crate::logic::sourcefiles::mime_type_of_video(path);
        return Some(format!("data:{mime};base64,{}", BASE64.encode(&bytes)));
    }

    crate::logic::video::still_frame(path).await
}

/// The web build keeps its library in memory and has no video support yet, so
/// there is nothing to put in a deck.
#[cfg(target_arch = "wasm32")]
async fn video_for_deck(_path: &str) -> Option<String> {
    None
}
