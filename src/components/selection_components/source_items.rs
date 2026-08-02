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
            class: "outline secondary selection_item",
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
                .filter(|(_, sf)| sf.file_type == SourceFileType::Image)
            {
                ImageSourceItem {
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
fn ImageSourceItem(
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
    let image_src = source_file.path.to_string_lossy().to_string();

    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
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
            img { height: "300px", src: "{image_src}" }
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
            class: "outline secondary selection_item",
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
    let mut spontaneous_text: Signal<String> = use_signal(|| String::new());

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
            class: "outline secondary selection_item",
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
