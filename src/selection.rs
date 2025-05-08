//! This module includes the components for song selection

use crate::{
    settings::Settings,
    sourcefiles::{get_source_files, SourceFile},
    Route,
};
use dioxus::{html::u::outline, prelude::*};
use dioxus_router::prelude::navigator;
use rust_i18n::t;
use std::rc::Rc;

use dioxus_free_icons::icons::fa_regular_icons::*;
use dioxus_free_icons::icons::fa_solid_icons::{FaArrowDown, FaArrowUp};
use dioxus_free_icons::Icon;

rust_i18n::i18n!("locales", fallback = "en");

#[component]
pub fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();

    let filter_string: Signal<String> = use_signal(|| "".to_string());

    if settings.read().song_repos.is_empty() || !settings.read().wizard_completed {
        nav.replace(Route::Wizard {});
    }

    let source_files: Signal<Vec<SourceFile>> = use_signal(|| settings.read().get_sourcefiles());
    let selected_items: Signal<Vec<SelectedItemRepresentation>> = use_signal(|| vec![]);

    let input_element_signal: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    use_effect(|| {
        document::eval(
            r#"function inputFocus(){
                document.getElementById("searchinput").focus();
            }
            window.onkeydown = inputFocus;"#,
        );
    });

    rsx! {
        div {
            class: "wrapper",
            header {
                class: "top-bar no-padding",
                SearchInput {
                    input_signal: filter_string,
                    element_signal: input_element_signal
                }
            }
            main {
                class: "content content-background height-100",
                onkeydown: move |_| async move {
                    if let Some(searchinput) = input_element_signal() {
                        let _ = searchinput.set_focus(true).await;
                    }
                },
                div {
                    class: "grid height-100",
                    div {
                        class: "height-100",
                        div {
                            class: "scrollable-container",
                            for item in source_files.read().iter() {
                                SourceItem {
                                    item: item.clone(),
                                    selected_items: selected_items
                                }
                            }
                        }
                    },
                    if selected_items.read().len() > 0 {
                        div {
                            class: "selected-container",
                            SelectedItems {
                                selected_items: selected_items
                            }
                        }
                    }
                }
            }
            footer {
                class: "bottom-bar",
                div {
                    class: "grid",
                    button {
                        class: "outline secondary smaller-buttons",
                        { t!("selection.import") }
                    },
                    button {
                        class: "outline secondary smaller-buttons",
                        { t!("selection.export") }
                    },
                    button {
                        class: "primary smaller-buttons",
                        { t!("selection.start_presentation") }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchInput(
    input_signal: Signal<String>,
    element_signal: Signal<Option<Rc<MountedData>>>,
) -> Element {
    rsx! {
        div {
            id: "searchinput",
            role: "group",
            onmounted: move |element| element_signal.set(Some(element.data())),
            input {
                type: "search",
                name: "search",
                placeholder: t!("search").to_string(),
                aria_label: t!("search").to_string(),
                value: input_signal,
                oninput: move |event| {
                    let value = event.value();
                    input_signal.set(value);
                },
            }
        }
    }
}

/// This component renders one source item which can be selected
#[component]
fn SourceItem(
    item: SourceFile,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
            tabindex: 0,
            onclick: move |_| { selected_items.write().push(
                SelectedItemRepresentation { source_file: item.clone() }
            ); },
            { item.clone().name }
        }
    }
}

/// This struct represents a selected item
#[derive(Clone, PartialEq)]
struct SelectedItemRepresentation {
    /// The source file of the selected item
    source_file: SourceFile,
}

#[component]
fn SelectedItems(selected_items: Signal<Vec<SelectedItemRepresentation>>) -> Element {
    rsx! {
        div {
            class: "selected-container",
            for (number, item) in selected_items.read().iter().enumerate() {
                SelectedItem {
                    item: item.clone(),
                    selected_items: selected_items,
                    id: number
                }
            }
        }
    }
}

/// This component renders a selected item
#[component]
fn SelectedItem(
    item: SelectedItemRepresentation,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    id: usize,
) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
            tabindex: 0,
            { item.source_file.name },

            // Delete a selected item
            div {
                class: "right-justified",
                // Move Item Up
                if id > 0 {
                    span {
                        onclick: move |_| { selected_items.write().swap(id, id-1); },
                        Icon {
                            icon: FaArrowUp,
                        }
                    }
                }
                if id < selected_items.len() - 1 {
                    span {
                        onclick: move |_| { selected_items.write().swap(id, id+1); },
                        Icon {
                            icon: FaArrowDown,
                        }
                    }
                }
                // Delete a selected item
                span {
                    onclick: move |_| { selected_items.write().remove(id.clone()); },
                    Icon {
                        icon: FaTrashCan,
                    }
                }
            }
        }
    }
}
