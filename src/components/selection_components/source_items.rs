use crate::logic::sourcefiles::{SourceFile, SourceFileType};
use crate::logic::states::SelectedItemRepresentation;
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use rust_i18n::t;

/// What a click on an element in the list means.
///
/// The same list serves two views with opposite intents: the selection view
/// collects elements for a presentation, the detail view opens one to look at.
/// Making that explicit keeps the lists shared instead of duplicated — and the
/// detail view used to do nothing on a click because the behaviour was wired
/// straight into the item.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ItemClickAction {
    /// Add the element to the presentation selection.
    #[default]
    AddToSelection,
    /// Open the element in the detail view.
    OpenDetail,
}

#[component]
pub(crate) fn SongSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    #[props(default)]
    click_action: ItemClickAction,
) -> Element {
    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                let _ = document::eval("initSelectionLayout();").await;
            },
            for (id , _) in source_files
                .read()
                .iter()
                .enumerate()
                .filter(|(_, sf)| sf.file_type == SourceFileType::Song)
            {
                SongSourceItem {
                    id,
                    source_files,
                    active_detailed_item_id,
                    selected_items,
                click_action,
                }
            }
        }
    }
}

#[component]
fn SongSourceItem(
    source_files: Signal<Vec<SourceFile>>,
    id: usize,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_detailed_item_id: Signal<Option<usize>>,
    #[props(default)]
    click_action: ItemClickAction,
) -> Element {
    let source_file = source_files.read().get(id).cloned();
    let Some(source_file) = source_file else {
        return rsx! {};
    };

    rsx! {
        div {
            role: "button",
            class: if active_detailed_item_id() == Some(id) {
                "outline secondary selection_item selection_item-active"
            } else {
                "outline secondary selection_item"
            },
            tabindex: 0,
            onclick: move |_| {
                match click_action {
                    ItemClickAction::AddToSelection => {
                        selected_items
                            .write()
                            .push(SelectedItemRepresentation::new_with_sourcefile(
                                source_file.clone(),
                            ));
                    }
                    ItemClickAction::OpenDetail => active_detailed_item_id.set(Some(id)),
                }
            },
            oncontextmenu: move |_| {
                active_detailed_item_id.set(Some(id));
            },
            {source_file.name.clone()}
        }
    }
}

#[component]
pub(crate) fn ImageSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    #[props(default)]
    click_action: ItemClickAction,
) -> Element {
    // Counts up as thumbnails arrive; reading it here is what makes the list
    // draw them. See [`crate::logic::images`] for why they are not simply read
    // while this renders — doing that froze the window for as long as it took
    // to encode every photograph in the library.
    let mut thumbnails_ready: Signal<u64> = use_signal(crate::logic::images::thumbnail_generation);

    use_effect(move || {
        let paths: Vec<std::path::PathBuf> = source_files
            .read()
            .iter()
            .filter(|file| file.file_type == SourceFileType::Image)
            .map(|file| file.path.clone())
            .collect();
        crate::logic::images::prepare_thumbnails(paths);

        // The pictures are made on threads, which cannot write to a signal, so
        // the list looks for new ones instead. It stops as soon as they are
        // all in; a scan that brings more starts this effect again.
        spawn(async move {
            loop {
                let generation = crate::logic::images::thumbnail_generation();
                if generation != *thumbnails_ready.peek() {
                    thumbnails_ready.set(generation);
                }
                if !crate::logic::images::thumbnails_in_progress() {
                    return;
                }
                let _ = document::eval("await new Promise(r => setTimeout(r, 150))").await;
            }
        });
    });

    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                let _ = document::eval("initSelectionLayout();").await;
            },
            // The thumbnail is looked up here and handed down, rather than
            // each item reading the counter for itself. Reading it there made
            // every picture in the library re-render each time any one of them
            // arrived; this way an item is redrawn when *its* thumbnail turns
            // up and not before.
            for (id , thumbnail) in source_files
                .read()
                .iter()
                .enumerate()
                .filter(|(_, sf)| sf.file_type == SourceFileType::Image)
                .map(|(id, sf)| (id, crate::logic::images::thumbnail(&sf.path)))
            {
                ImageSourceItem {
                    key: "{id}",
                    id,
                    source_files,
                    active_detailed_item_id,
                    selected_items,
                    click_action,
                    thumbnail,
                }
            }
        }
    }
}

#[component]
fn ImageSourceItem(
    source_files: Signal<Vec<SourceFile>>,
    id: usize,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_detailed_item_id: Signal<Option<usize>>,
    #[props(default)]
    click_action: ItemClickAction,
    /// A scaled-down copy of the picture, inlined into the page — a file
    /// system path in `src` is nothing a web view can fetch, and the picture
    /// itself is far more than a list needs. `None` until it has been made;
    /// see [`crate::logic::images`].
    thumbnail: Option<String>,
) -> Element {
    let source_file = source_files.read().get(id).cloned();
    let Some(source_file) = source_file else {
        return rsx! {};
    };
    let image_src = thumbnail;

    rsx! {
        div {
            role: "button",
            class: if active_detailed_item_id() == Some(id) {
                "outline secondary selection_item selection_item-active"
            } else {
                "outline secondary selection_item"
            },
            tabindex: 0,
            onclick: move |_| {
                match click_action {
                    ItemClickAction::AddToSelection => {
                        selected_items
                            .write()
                            .push(SelectedItemRepresentation::new_with_sourcefile(
                                source_file.clone(),
                            ));
                    }
                    ItemClickAction::OpenDetail => active_detailed_item_id.set(Some(id)),
                }
            },
            oncontextmenu: move |_| {
                active_detailed_item_id.set(Some(id));
            },
            {source_file.name.clone()}
            br {}
            // The box keeps its size while the thumbnail is being made, so the
            // list does not jump about as the pictures come in.
            match image_src {
                Some(image_src) => rsx! {
                    // The web view decodes a picture before it can draw it,
                    // and a library's worth of them decoded at once is a
                    // stall of its own — after all the work of not reading
                    // them on the render. `lazy` leaves the ones that are
                    // scrolled out of sight alone, `async` keeps the decoding
                    // of the rest off the thread that draws.
                    img {
                        height: "300px",
                        src: "{image_src}",
                        alt: "{source_file.name}",
                        loading: "lazy",
                        decoding: "async",
                    }
                },
                None => rsx! {
                    div { class: "picture-placeholder", aria_busy: true }
                },
            }
        }
    }
}

#[component]
pub(crate) fn PdfSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    #[props(default)]
    click_action: ItemClickAction,
) -> Element {
    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                let _ = document::eval("initSelectionLayout();").await;
            },
            for (id , _) in source_files
                .read()
                .iter()
                .enumerate()
                .filter(|(_, sf)| sf.file_type == SourceFileType::Pdf)
            {
                PdfSourceItem {
                    id,
                    source_files,
                    active_detailed_item_id,
                    selected_items,
                click_action,
                }
            }
        }
    }
}

#[component]
fn PdfSourceItem(
    source_files: Signal<Vec<SourceFile>>,
    id: usize,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_detailed_item_id: Signal<Option<usize>>,
    #[props(default)]
    click_action: ItemClickAction,
) -> Element {
    let source_file = source_files.read().get(id).cloned();
    let Some(source_file) = source_file else {
        return rsx! {};
    };

    rsx! {
        div {
            role: "button",
            class: if active_detailed_item_id() == Some(id) {
                "outline secondary selection_item selection_item-active"
            } else {
                "outline secondary selection_item"
            },
            tabindex: 0,
            onclick: move |_| {
                match click_action {
                    ItemClickAction::AddToSelection => {
                        selected_items
                            .write()
                            .push(SelectedItemRepresentation::new_with_sourcefile(
                                source_file.clone(),
                            ));
                    }
                    ItemClickAction::OpenDetail => active_detailed_item_id.set(Some(id)),
                }
            },
            oncontextmenu: move |_| {
                active_detailed_item_id.set(Some(id));
            },
            {source_file.name.clone()}
        }
    }
}

#[component]
pub(crate) fn MarkdownSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    #[props(default)]
    click_action: ItemClickAction,
) -> Element {
    let mut spontaneous_text: Signal<String> = use_signal(String::new);

    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                let _ = document::eval("initSelectionLayout();").await;
            },
            for (id , _) in source_files
                .read()
                .iter()
                .enumerate()
                .filter(|(_, sf)| sf.file_type == SourceFileType::Markdown)
            {
                MarkdownSourceItem {
                    id,
                    source_files,
                    active_detailed_item_id,
                    selected_items,
                click_action,
                }
            }
            details {
                summary { {t!("selection.markdown.add_text").to_string()} }
                textarea {
                    rows: "8",
                    placeholder: t!("selection.markdown.placeholder").to_string(),
                    value: spontaneous_text,
                    oninput: move |event| {
                        spontaneous_text.set(event.value());
                    },
                }
                button {
                    class: "outline",
                    disabled: spontaneous_text.read().trim().is_empty(),
                    onclick: move |_| {
                        let text = spontaneous_text.read().clone();
                        if !text.trim().is_empty() {
                            let source_file = SourceFile {
                                name: t!("selection.markdown.spontaneous_name").to_string(),
                                path: std::path::PathBuf::new(),
                                file_type: SourceFileType::Markdown,
                                md5_hash: None,
                                relative_path: None,
                            };
                            let mut item = SelectedItemRepresentation::new_with_sourcefile(source_file);
                            item.inline_markdown = Some(text.clone());
                            selected_items.write().push(item);
                            spontaneous_text.set(String::new());
                        }
                    },
                    {t!("selection.markdown.add_button").to_string()}
                }
            }
        }
    }
}

#[component]
fn MarkdownSourceItem(
    source_files: Signal<Vec<SourceFile>>,
    id: usize,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_detailed_item_id: Signal<Option<usize>>,
    #[props(default)]
    click_action: ItemClickAction,
) -> Element {
    let source_file = source_files.read().get(id).cloned();
    let Some(source_file) = source_file else {
        return rsx! {};
    };

    rsx! {
        div {
            role: "button",
            class: if active_detailed_item_id() == Some(id) {
                "outline secondary selection_item selection_item-active"
            } else {
                "outline secondary selection_item"
            },
            tabindex: 0,
            onclick: move |_| {
                match click_action {
                    ItemClickAction::AddToSelection => {
                        selected_items
                            .write()
                            .push(SelectedItemRepresentation::new_with_sourcefile(
                                source_file.clone(),
                            ));
                    }
                    ItemClickAction::OpenDetail => active_detailed_item_id.set(Some(id)),
                }
            },
            oncontextmenu: move |_| {
                active_detailed_item_id.set(Some(id));
            },
            {source_file.name.clone()}
        }
    }
}

#[component]
pub(crate) fn SourceDetailView(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
) -> Element {
    let active_id = *active_detailed_item_id.read();
    let Some(active_id) = active_id else {
        return rsx! {};
    };
    let item = source_files.read().get(active_id).cloned();
    let Some(item) = item else {
        return rsx! {};
    };
    let path_string = item.path.to_string_lossy().to_string();

    rsx! {
        dialog { style: "position: fixed", open: true,
            article {
                header {
                    p { {t!("selection.detail_view").to_string()} }
                }
                table {
                    tbody {
                        tr {
                            td {
                                strong { {t!("general.type").to_string()} }
                            }
                            td {
                                match item.file_type {
                                    SourceFileType::Song => t!("general.song").to_string(),
                                    SourceFileType::Image => t!("general.picture").to_string(),
                                    SourceFileType::Presentation => t!("general.presentation").to_string(),
                                    SourceFileType::Video => t!("general.video").to_string(),
                                    SourceFileType::Pdf => t!("general.pdf").to_string(),
                                    SourceFileType::Markdown => t!("general.markdown").to_string(),
                                }
                            }
                        }
                        tr {
                            td {
                                strong { {t!("general.title").to_string()} }
                            }
                            td { {item.name.clone()} }
                        }
                        tr {
                            td {
                                strong { {t!("general.file_path").to_string()} }
                            }
                            td { {path_string.clone()} }
                        }
                    }
                }
                footer {
                    button { onclick: move |_| { active_detailed_item_id.set(None) },
                        {t!("general.close").to_string()}
                    }
                }
            }
        }
    }
}

pub(crate) async fn process_dropped_files(
    event: DragEvent,
    mut source_files: Signal<Vec<SourceFile>>,
    mut selected_items: Signal<Vec<SelectedItemRepresentation>>,
) {
    let files = event.data().files();
    for file_data in files {
        let file_name = file_data.name();
        // Only the desktop keeps dropped files where they lie; the web build
        // copies the bytes into its VFS below.
        #[cfg(not(target_arch = "wasm32"))]
        let file_path = file_data.path();
        // The same classifier the directory scan uses, so a file that can be
        // opened from a repository can also be dropped onto the window.
        let Some(file_type) = SourceFileType::of(&file_name) else {
            log::info!("Skipping dropped file with unsupported extension: {file_name}");
            continue;
        };

        let stem = SourceFileType::display_name(&file_name);

        let content = match file_data.read_bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("Failed to read dropped file '{}': {}", file_name, e);
                continue;
            }
        };

        let md5_hash = Some(format!("{:x}", md5::compute(&*content)));

        #[cfg(target_arch = "wasm32")]
        {
            use crate::logic::settings::RepositoryType;
            let vfs_path = format!("drop://{}", file_name);
            RepositoryType::store_web_file(&vfs_path, content.to_vec());
            let sf = SourceFile {
                name: stem,
                path: std::path::PathBuf::from(&vfs_path),
                file_type,
                md5_hash,
                relative_path: None,
            };
            selected_items
                .write()
                .push(SelectedItemRepresentation::new_with_sourcefile(sf.clone()));
            source_files.write().push(sf);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let sf = SourceFile {
                name: stem,
                path: file_path,
                file_type,
                md5_hash,
                relative_path: None,
            };
            selected_items
                .write()
                .push(SelectedItemRepresentation::new_with_sourcefile(sf.clone()));
            source_files.write().push(sf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The selection view does not pass the action, so the default decides what
    /// a click there does. It has to stay "collect for the presentation" —
    /// anything else would silently repurpose the main view.
    #[test]
    fn test_a_click_collects_by_default() {
        assert_eq!(ItemClickAction::default(), ItemClickAction::AddToSelection);
    }

    /// The two intents must stay distinguishable; the detail view showed
    /// nothing at all while both lived in one hard-wired handler.
    #[test]
    fn test_the_two_intents_are_distinct() {
        assert_ne!(ItemClickAction::AddToSelection, ItemClickAction::OpenDetail);
    }
}
