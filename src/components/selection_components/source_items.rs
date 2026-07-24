use crate::logic::sourcefiles::{SourceFile, SourceFileType};
use crate::logic::states::SelectedItemRepresentation;
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use rust_i18n::t;

#[component]
pub(crate) fn SongSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                let _ = document::eval("initSelectionLayout();").await;
            },
            for (id, _) in source_files.read().iter().enumerate().filter(|(_, sf)| sf.file_type == SourceFileType::Song) {
                SongSourceItem {
                    id: id,
                    source_files: source_files,
                    active_detailed_item_id: active_detailed_item_id,
                    selected_items: selected_items
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
) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
            tabindex: 0,
            onclick: move |_| { selected_items.write().push(
                SelectedItemRepresentation::new_with_sourcefile(source_files.get(id).unwrap().clone())
            ); },
            oncontextmenu: move |_| {
                active_detailed_item_id.set(Some(id));
            },
            { source_files.get(id).unwrap().clone().name }
        }
    }
}

#[component]
pub(crate) fn ImageSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                let _ = document::eval("initSelectionLayout();").await;
            },
            for (id, _) in source_files.read().iter().enumerate().filter(|(_, sf)| sf.file_type == SourceFileType::Image) {
                ImageSourceItem {
                    id: id,
                    source_files: source_files,
                    active_detailed_item_id: active_detailed_item_id,
                    selected_items: selected_items
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
) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
            tabindex: 0,
            onclick: move |_| { selected_items.write().push(
                SelectedItemRepresentation::new_with_sourcefile(source_files.get(id).unwrap().clone())
            ); },
            oncontextmenu: move |_| {
                active_detailed_item_id.set(Some(id));
            },
            { source_files.get(id).unwrap().clone().name },
            br { },
            img {
                height: "300px",
                src: source_files.get(id).unwrap().clone().path.to_str().unwrap_or("")
            }
        }
    }
}

#[component]
pub(crate) fn PdfSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                let _ = document::eval("initSelectionLayout();").await;
            },
            for (id, _) in source_files.read().iter().enumerate().filter(|(_, sf)| sf.file_type == SourceFileType::Pdf) {
                PdfSourceItem {
                    id: id,
                    source_files: source_files,
                    active_detailed_item_id: active_detailed_item_id,
                    selected_items: selected_items
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
) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
            tabindex: 0,
            onclick: move |_| { selected_items.write().push(
                SelectedItemRepresentation::new_with_sourcefile(source_files.get(id).unwrap().clone())
            ); },
            oncontextmenu: move |_| {
                active_detailed_item_id.set(Some(id));
            },
            { source_files.get(id).unwrap().clone().name }
        }
    }
}

#[component]
pub(crate) fn MarkdownSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    let mut spontaneous_text: Signal<String> = use_signal(|| String::new());

    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                let _ = document::eval("initSelectionLayout();").await;
            },
            for (id, _) in source_files.read().iter().enumerate().filter(|(_, sf)| sf.file_type == SourceFileType::Markdown) {
                MarkdownSourceItem {
                    id: id,
                    source_files: source_files,
                    active_detailed_item_id: active_detailed_item_id,
                    selected_items: selected_items
                }
            }
            details {
                summary { { t!("selection.markdown.add_text").to_string() } }
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
                    { t!("selection.markdown.add_button").to_string() }
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
) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
            tabindex: 0,
            onclick: move |_| { selected_items.write().push(
                SelectedItemRepresentation::new_with_sourcefile(source_files.get(id).unwrap().clone())
            ); },
            oncontextmenu: move |_| {
                active_detailed_item_id.set(Some(id));
            },
            { source_files.get(id).unwrap().clone().name }
        }
    }
}

#[component]
pub(crate) fn SourceDetailView(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
) -> Element {
    let item = use_memo(move || {
        source_files
            .read()
            .get(active_detailed_item_id.unwrap())
            .unwrap()
            .clone()
    });
    let path_string = use_memo(move || item.read().path.to_str().unwrap_or("").to_string());

    rsx! {
        dialog {
            style: "position: fixed",
            open: true,
            article {
                header {
                    p { { t!("selection.detail_view").to_string() } }
                }
                table {
                    tbody {
                        tr {
                            td { strong { { t!("general.type").to_string() } } }
                            td {
                                match item().file_type {
                                    SourceFileType::Song => t!("general.song").to_string(),
                                    SourceFileType::Image => t!("general.picture").to_string(),
                                    SourceFileType::Presentation => t!("general.presentation").to_string(),
                                    SourceFileType::Video => t!("general.video").to_string(),
                                    SourceFileType::Pdf => t!("general.pdf").to_string(),
                                    SourceFileType::Markdown => t!("general.markdown").to_string()
                                }
                            }
                        }
                        tr {
                            td { strong { { t!("general.title").to_string() } } }
                            td { { item.read().name.clone() } }
                        }
                        tr {
                            td { strong { { t!("general.file_path").to_string() } } }
                            td { { path_string } }
                        }
                    }
                }
                footer {
                    button {
                        onclick: move |_| { active_detailed_item_id.set(None) },
                        { t!("general.close").to_string() }
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

        let file_name_path = std::path::Path::new(&file_name);
        let extension = file_path
            .extension()
            .or_else(|| file_name_path.extension())
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let file_type = match extension.as_str() {
            "song" => SourceFileType::Song,
            "png" | "jpg" | "jpeg" => SourceFileType::Image,
            "pdf" => SourceFileType::Pdf,
            _ => {
                log::info!(
                    "Skipping dropped file with unsupported extension: {}",
                    file_name
                );
                continue;
            }
        };

        let stem = file_path
            .file_stem()
            .or_else(|| file_name_path.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or(&file_name)
            .to_string();

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
