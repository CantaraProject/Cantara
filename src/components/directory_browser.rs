//! A directory browser modal component for selecting directories on mobile platforms.
//!
//! On Android, this replaces the JavaScript prompt() dialog with a visual directory
//! browser that lets users navigate the filesystem and select a directory.

use std::path::PathBuf;

use dioxus::prelude::*;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// A directory entry for display in the browser.
#[derive(Clone, PartialEq)]
struct DirEntry {
    name: String,
    path: PathBuf,
}

/// Returns a reasonable starting directory for browsing.
/// On Android, tries common external storage paths first, then falls back to home dir.
fn get_default_start_directory() -> PathBuf {
    // Common Android external storage paths
    for path_str in &["/storage/emulated/0", "/sdcard"] {
        let path = PathBuf::from(path_str);
        if path.exists() && path.is_dir() {
            return path;
        }
    }

    // Fall back to home directory
    if let Some(home) = dirs::home_dir() {
        return home;
    }

    // Last resort: root directory
    PathBuf::from("/")
}

/// Lists subdirectories of the given path, sorted alphabetically.
/// Hidden directories (starting with '.') are excluded.
fn list_subdirectories(path: &PathBuf) -> Result<Vec<DirEntry>, String> {
    let read_dir = std::fs::read_dir(path).map_err(|e| e.to_string())?;

    let mut entries: Vec<DirEntry> = read_dir
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().map_or(false, |ft| ft.is_dir())
                && !entry.file_name().to_string_lossy().starts_with('.')
        })
        .map(|entry| DirEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path(),
        })
        .collect();

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

/// A modal component for browsing and selecting a directory.
///
/// This component provides a visual file system browser that replaces the text-based
/// JavaScript prompt on mobile platforms. It shows subdirectories of the current path
/// and allows navigating up and down the directory tree.
///
/// # Props
/// - `show`: Signal controlling the visibility of the modal
/// - `on_select`: Callback invoked with the selected directory path when the user confirms
/// - `on_cancel`: Optional callback invoked when the user cancels the selection
#[component]
pub fn DirectoryBrowserModal(
    show: Signal<bool>,
    on_select: EventHandler<String>,
    on_cancel: Option<EventHandler<()>>,
) -> Element {
    let mut current_path: Signal<PathBuf> = use_signal(get_default_start_directory);
    let mut entries: Signal<Vec<DirEntry>> = use_signal(Vec::new);
    let mut error_message: Signal<Option<String>> = use_signal(|| None);

    // Update directory entries when the current path changes
    use_effect(move || {
        let path = current_path.read().clone();
        match list_subdirectories(&path) {
            Ok(dirs) => {
                entries.set(dirs);
                error_message.set(None);
            }
            Err(err) => {
                entries.set(Vec::new());
                error_message.set(Some(err));
            }
        }
    });

    if !show() {
        return rsx! {};
    }

    let current_path_value = current_path.read().clone();
    let path_display = current_path_value.to_string_lossy().to_string();
    let has_parent = current_path_value
        .parent()
        .is_some_and(|p| p != current_path_value);

    rsx! {
        dialog {
            open: true,
            article {
                header {
                    h3 { { t!("settings.directory_browser.title").to_string() } }
                    p {
                        style: "word-break: break-all; margin-bottom: 0;",
                        code { "{path_display}" }
                    }
                }

                div {
                    style: "max-height: 50vh; overflow-y: auto;",

                    if let Some(ref err) = *error_message.read() {
                        p {
                            style: "color: var(--pico-del-color);",
                            { t!("settings.directory_browser.error").to_string() }
                            br {}
                            small { "{err}" }
                        }
                    }

                    if has_parent {
                        div {
                            class: "directory-entry",
                            onclick: move |_| {
                                let parent = current_path.read().parent().map(|p| p.to_path_buf());
                                if let Some(parent) = parent {
                                    current_path.set(parent);
                                }
                            },
                            "📁 ↑ .."
                        }
                    }

                    for entry in entries.read().clone().into_iter() {
                        div {
                            class: "directory-entry",
                            key: "{entry.name}",
                            onclick: {
                                let path = entry.path.clone();
                                move |_| {
                                    current_path.set(path.clone());
                                }
                            },
                            "📁 {entry.name}"
                        }
                    }

                    if entries.read().is_empty() && error_message.read().is_none() {
                        p {
                            style: "font-style: italic; color: var(--pico-muted-color);",
                            { t!("settings.directory_browser.empty").to_string() }
                        }
                    }
                }

                footer {
                    div {
                        role: "group",
                        button {
                            class: "secondary",
                            onclick: move |_| {
                                show.set(false);
                                if let Some(handler) = on_cancel {
                                    handler.call(());
                                }
                            },
                            { t!("settings.directory_browser.cancel").to_string() }
                        }
                        button {
                            onclick: move |_| {
                                let path = current_path.read().to_string_lossy().to_string();
                                on_select.call(path);
                                show.set(false);
                            },
                            { t!("settings.directory_browser.select").to_string() }
                        }
                    }
                }
            }
        }
    }
}
