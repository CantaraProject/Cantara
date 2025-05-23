//! This module includes the components for song selection

use crate::TEST_STATE;
use crate::logic::presentation;
use crate::logic::sourcefiles::SourceFileType;
use crate::logic::states::{RunningPresentation, SelectedItemRepresentation};
use super::shared_components::{ImageIcon, MusicIcon};
use crate::{Route, logic::settings::Settings, logic::sourcefiles::SourceFile};
use dioxus::prelude::*;
use dioxus_router::prelude::navigator;
use rust_i18n::t;
use std::rc::Rc;

use crate::logic::settings::PresentationDesign;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::*;
use dioxus_free_icons::icons::fa_solid_icons::{FaArrowDown, FaArrowUp};
use super::presentation_components::PresentationPage;

rust_i18n::i18n!("locales", fallback = "en");

#[component]
pub fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();

    let filter_string: Signal<String> = use_signal(|| "".to_string());

    let mut source_files: Signal<Vec<SourceFile>> = use_context();
    let selected_items: Signal<Vec<SelectedItemRepresentation>> = use_context();
    let active_selected_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_detailed_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_selection_filter: Signal<SelectionFilterOptions> =
        use_signal(|| SelectionFilterOptions::Songs);
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    let input_element_signal: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    let default_presentation_design_signal =
        use_signal(|| match settings.read().presentation_designs.get(0) {
            Some(design) => design.clone(),
            None => PresentationDesign::default(),
        });

    use_effect(move || {
        if !settings.read().wizard_completed {
            nav.replace(Route::Wizard {});
        }

        use_future(move || async move {
            source_files.set(settings.read().get_sourcefiles());
        });
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
                id: "selection-content",
                class: "content content-background height-100",
                onmounted: move |_| async move {
                    // This is necessary because we need to run the adjustDivHeight javascript function once to prevent wrong sizening of the elements.
                    let _ = document::eval("adjustDivHeight();").await;
                },
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
                        SelectionFilterSideBar {
                            active_selection: active_selection_filter
                        }
                        if active_selection_filter() == SelectionFilterOptions::Songs {
                            SongSourceItems {
                                source_files: source_files,
                                active_detailed_item_id: active_detailed_item_id,
                                selected_items: selected_items
                            }
                        }
                        if active_selection_filter() == SelectionFilterOptions::Pictures {
                            ImageSourceItems {
                                source_files: source_files,
                                active_detailed_item_id: active_detailed_item_id,
                                selected_items: selected_items
                            }
                        }
                    },

                    // The area where the selected elements are shown
                    if !selected_items.read().is_empty() {
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
                        onclick: move |_| { nav.push(crate::Route::SettingsPage); },
                        class: "outline secondary smaller-buttons",
                        span {
                            class: "desktop-only",
                            { t!("settings.settings_button") }
                        }
                    },
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
                        onclick: move |_| start_presentation(&selected_items.read().clone(), &mut running_presentations, &default_presentation_design_signal()),
                        span {
                            class: "desktop-only",
                            { t!("selection.start_presentation") }
                        }
                    }
                }
            }
        }

        if active_detailed_item_id.read().is_some() {
            SourceDetailView {
                source_files: source_files,
                active_detailed_item_id: active_detailed_item_id,
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

#[component]
fn SongSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                // This is necessary because we need to run the adjustDivHeight javascript function once to prevent wrong sizening of the elements.
                let _ = document::eval("adjustDivHeight();").await;
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

/// This component renders one source item which can be selected
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

/// The component renders the list of available pictures
#[component]
fn ImageSourceItems(
    source_files: Signal<Vec<SourceFile>>,
    active_detailed_item_id: Signal<Option<usize>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
) -> Element {
    rsx! {
        div {
            class: "scrollable-container",
            onmounted: move |_| async move {
                // This is necessary because we need to run the adjustDivHeight javascript function once to prevent wrong sizening of the elements.
                let _ = document::eval("adjustDivHeight();").await;
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
                    active_selected_item_id.set(Some(id))
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
                        selected_items.write().remove(id);
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

/// The component for setting up presentation options
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
            p { { TEST_STATE.read().clone() } }
        }
    }
}

/// This component provides a Detail View for a source file which will open as a modal dialog (in front of anything else)
/// if the signal active_detailed_item_id is set to a non None value.
#[component]
fn SourceDetailView(
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
                    p { { t!("selection.detail_view") } }
                }
                table {
                    tbody {
                        tr {
                            td { strong { { t!("general.type") } } }
                            td {
                                match item().file_type {
                                    SourceFileType::Song => t!("general.song"),
                                    SourceFileType::Image => t!("general.picture"),
                                    SourceFileType::Presentation => t!("general.presentation"),
                                    SourceFileType::Video => t!("general.video")
                                }
                            }
                        }
                        tr {
                            td { strong { { t!("general.title") } } }
                            td { { item.read().name.clone() } }
                        }
                        tr {
                            td { strong { { t!("general.file_path") } } }
                            td { { path_string } }
                        }
                    }
                }
                footer {
                    button {
                        onclick: move |_| { active_detailed_item_id.set(None) },
                        { { t!("general.close") } }
                    }
                }
            }
        }
    }
}

/// Helper function to start a presentation from the selection page
/// It will create the presentation and open the window
#[cfg(feature = "desktop")]
fn start_presentation(
    selected_items: &Vec<SelectedItemRepresentation>,
    running_presentations: &mut Signal<Vec<RunningPresentation>>,
    default_presentation_design: &PresentationDesign,
) {
    // Create the presentation

    use super::presentation_components::PresentationPage;
    use dioxus::desktop::Config;

    if presentation::add_presentation(
        selected_items,
        running_presentations,
        default_presentation_design,
    )
    .is_some()
    {
        // Create a new window if running on desktop
        let presentation_dom =
            VirtualDom::new(PresentationPage).with_root_context(*running_presentations);

        dioxus::desktop::window().new_window(presentation_dom, Config::new().with_menu(None));
    }
}

/// An enum representing the active selection (songs, pictures, presentations)
#[derive(Clone, PartialEq)]
enum SelectionFilterOptions {
    Songs,
    Pictures,
    Presentations,
}

/// This component renders a sidebar for the selection where the user can filter the sources
#[component]
fn SelectionFilterSideBar(active_selection: Signal<SelectionFilterOptions>) -> Element {
    rsx! {
        div {
            class: "selection-sidebar",
            // Song Selection
            div {
                role: "button",
                class: match active_selection() {
                    SelectionFilterOptions::Songs => "outline",
                    _ => "outline secondary"
                },
                style: "padding: 12px;",
                onclick: move |_| active_selection.set(SelectionFilterOptions::Songs),
                MusicIcon {
                }
            }
            // Picture Selection
            div {
                role: "button",
                class: match active_selection() {
                    SelectionFilterOptions::Pictures => "outline",
                    _ => "outline secondary"
                },
                style: "padding: 12px;",
                onclick: move |_| active_selection.set(SelectionFilterOptions::Pictures),
                ImageIcon {
                }
            }
        }
    }
}
