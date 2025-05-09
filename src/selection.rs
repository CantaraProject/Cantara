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
    let active_selected_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_detailed_item_id: Signal<Option<usize>> = use_signal(|| None);

    let input_element_signal: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

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

                    // The area where the selectable elements (sources) are shown
                    div {
                        class: "height-100",
                        div {
                            class: "scrollable-container",
                            for (id, _) in source_files.read().iter().enumerate() {
                                SourceItem {
                                    id: id.clone(),
                                    source_files: source_files,
                                    active_detailed_item_id: active_detailed_item_id,
                                    selected_items: selected_items
                                }
                            }
                        }
                    },

                    // The area where the selected elements are shown
                    if selected_items.read().len() > 0 {
                        div {
                            class: "height-100 scrollable-container",
                            SelectedItems {
                                selected_items: selected_items,
                                active_selected_item_id: active_selected_item_id
                            }
                        }
                    }

                    // The area of distinct presentation settings
                    div {
                        class: "desktop-only",
                        PresentationOptions {
                            selected_items: selected_items,
                            active_selected_item_id: active_selected_item_id
                        }
                    }
                }
            }
            footer {
                class: "bottom-bar",
                div {
                    class: "no-padding width-100",
                    role: "group",
                    button {
                        class: "outline secondary smaller-buttons",
                        span {
                            class: "desktop-only",
                            { t!("selection.import") }
                        }
                    },
                    button {
                        class: "outline secondary smaller-buttons",
                        span {
                            class: "desktop-only",
                            { t!("selection.export") }
                        }
                    },
                    button {
                        class: "primary smaller-buttons",
                        span {
                            class: "desktop-only",
                            { t!("selection.start_presentation") }
                        }
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
            role: "group",
            onmounted: move |element| element_signal.set(Some(element.data())),
            input {
                id: "searchinput",
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
                SelectedItemRepresentation {
                    source_file: source_files.get(id).unwrap().clone(),
                }
            ); },
            oncontextmenu: move |_| {

            },
            { source_files.get(id).unwrap().clone().name }
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
fn SelectedItems(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_selected_item_id: Signal<Option<usize>>,
) -> Element {
    rsx! {
        div {
            class: "selected-container",
            for (number, _) in selected_items.read().iter().enumerate() {
                SelectedItem {
                    selected_items: selected_items,
                    id: number,
                    active_selected_item_id: active_selected_item_id
                }
            }
        }
    }
}

/// This component renders a selected item
#[component]
fn SelectedItem(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    id: usize,
    active_selected_item_id: Signal<Option<usize>>,
) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
            style: "display: flex; align-items: left;",
            tabindex: 0,
            span {
                style: "flex-grow: 1;",
                onclick: move |_| {
                    active_selected_item_id.set(Some(id.clone()))
                },
                { selected_items.read().get(id).unwrap().source_file.name.clone() },
            }

            // Delete a selected item
            span {
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
                    onclick: move |_| {
                        if *active_selected_item_id.read() == Some(id) {
                            active_selected_item_id.set(None);
                        }
                        selected_items.write().remove(id.clone());
                    },
                    Icon {
                        icon: FaTrashCan,
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PresentationOptionTabState {
    General,
    Specific,
}

#[component]
fn PresentationOptions(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_selected_item_id: Signal<Option<usize>>,
) -> Element {
    let mut tab_state: Signal<PresentationOptionTabState> =
        use_signal(|| PresentationOptionTabState::General);
    use_effect(move || {
        if active_selected_item_id.read().is_some() {
            tab_state.set(PresentationOptionTabState::Specific);
        }
    });
    rsx! {
        if active_selected_item_id.read().is_some() {
            div {
                role: "group",
                button {
                    class: "smaller-buttons",
                    class: if *tab_state.read() != PresentationOptionTabState::General {
                        "secondary"
                    },
                    onclick: move |_| { tab_state.set(PresentationOptionTabState::General) },
                    "General"
                }
                button {
                    class: "smaller-buttons",
                    class: if *tab_state.read() != PresentationOptionTabState::Specific {
                        "secondary"
                    },
                    onclick: move |_| { tab_state.set(PresentationOptionTabState::Specific) },
                    "Specific"
                }
            }
            p {
                "The active selected number is: {active_selected_item_id.read().unwrap()}"
            }
        }
    }
}
