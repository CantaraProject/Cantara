//! This module includes the components for song selection

use super::shared_components::{ImageIcon, MusicIcon};
use crate::TEST_STATE;
use crate::logic::presentation;
use crate::logic::settings::PresentationDesign;
use crate::logic::sourcefiles::SourceFileType;
use crate::logic::states::{RunningPresentation, SelectedItemRepresentation};
use crate::{Route, logic::settings::Settings, logic::sourcefiles::SourceFile};
use cantara_songlib::slides::SlideSettings;
use dioxus::desktop::tao;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::*;
use dioxus_free_icons::icons::fa_solid_icons::{FaArrowDown, FaArrowUp};
use dioxus_router::prelude::*;
use rust_i18n::t;
use std::{fs, rc::Rc};

rust_i18n::i18n!("locales", fallback = "en");

/// Helper function to read the content of a source file
fn read_source_file_content(source_file: &SourceFile) -> Option<String> {
    if source_file.file_type == SourceFileType::Song {
        match fs::read_to_string(&source_file.path) {
            Ok(content) => Some(content),
            Err(_) => None,
        }
    } else {
        None
    }
}

/// Struct to represent a search result
#[derive(Clone, PartialEq)]
struct SearchResult {
    source_file: SourceFile,
    matched_content: Option<String>,
    is_title_match: bool,
}

/// Helper function to perform fuzzy search on source files
fn search_source_files(source_files: &[SourceFile], query: &str) -> Vec<SearchResult> {
    if query.is_empty() {
        return Vec::new();
    }

    let query = query.to_lowercase();
    let mut results = Vec::new();

    for source_file in source_files {
        let name_lower = source_file.name.to_lowercase();
        let is_title_match = name_lower.contains(&query);

        // Check if the query matches the title
        if is_title_match {
            results.push(SearchResult {
                source_file: source_file.clone(),
                matched_content: None,
                is_title_match: true,
            });
            continue;
        }

        // Check if the query matches the content (for song files)
        if source_file.file_type == SourceFileType::Song {
            if let Some(content) = read_source_file_content(source_file) {
                let content_lower = content.to_lowercase();
                if content_lower.contains(&query) {
                    // Find the context around the match
                    let match_index = content_lower.find(&query).unwrap();

                    // Convert byte indices to char indices for safe slicing
                    let content_chars: Vec<char> = content.chars().collect();
                    let content_lower_chars: Vec<char> = content_lower.chars().collect();

                    // Find the character index corresponding to the byte index
                    let mut char_count: usize = 0;
                    let mut match_char_index: usize = 0;

                    for (i, c) in content_lower.char_indices() {
                        if i == match_index {
                            match_char_index = char_count;
                            break;
                        }
                        char_count += 1;
                    }

                    // Calculate safe character indices for the context
                    let start_char = match_char_index.saturating_sub(30);
                    let end_char = (match_char_index + query.chars().count() + 30).min(content_chars.len());

                    // Create the context string from character indices
                    let context: String = content_chars[start_char..end_char].iter().collect();

                    results.push(SearchResult {
                        source_file: source_file.clone(),
                        matched_content: Some(context),
                        is_title_match: false,
                    });
                }
            }
        }
    }

    // Sort results: title matches first, then content matches
    results.sort_by(|a, b| {
        if a.is_title_match && !b.is_title_match {
            std::cmp::Ordering::Less
        } else if !a.is_title_match && b.is_title_match {
            std::cmp::Ordering::Greater
        } else {
            a.source_file.name.cmp(&b.source_file.name)
        }
    });

    results
}

/// Component to display search results
#[component]
fn SearchResults(
    search_results: Signal<Vec<SearchResult>>, 
    query: Signal<String>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>
) -> Element {
    let results = search_results.read().clone();
    if results.is_empty() {
        return rsx! { div {} };
    }

    let query_str = query.read().clone();

    rsx! {
        div {
            class: "search-results scrollable-container",
            style: "max-height: 300px; overflow-y: auto; margin-top: 10px; border: 1px solid #ccc; border-radius: 4px; padding: 10px;",
            h3 { {t!("search.results")} }

            for result in results.iter() {
                {
                    let source_file = result.source_file.clone();
                    let matched_content = result.matched_content.clone();
                    let is_title_match = result.is_title_match;

                    rsx! {
                        div {
                            class: "search-result",
                            style: "margin-bottom: 10px; padding: 5px; border-bottom: 1px solid #eee;",
                            div {
                                class: "search-result-title",
                                style: "font-weight: bold; cursor: pointer;",
                                onclick: move |_| {
                                    selected_items.write().push(
                                        SelectedItemRepresentation::new_with_sourcefile(source_file.clone())
                                    );
                                },
                                // For title matches, we'll manually split and highlight
                                if is_title_match {
                                    {
                                        let title = source_file.name.clone();
                                        let title_lower = title.to_lowercase();
                                        let query_lower = query_str.to_lowercase();

                                        if let Some(pos) = title_lower.find(&query_lower) {
                                            // Convert to character indices for safe slicing
                                            let title_chars: Vec<char> = title.chars().collect();

                                            // Find the character index corresponding to the byte index
                                            let mut char_pos: usize = 0;
                                            for (i, _) in title_lower.char_indices() {
                                                if i == pos {
                                                    break;
                                                }
                                                char_pos += 1;
                                            }

                                            // Calculate the end position in character indices
                                            let query_char_len = query_lower.chars().count();
                                            let char_end = char_pos + query_char_len;

                                            // Create the substrings using character indices
                                            let before: String = title_chars[0..char_pos].iter().collect();
                                            let highlight: String = title_chars[char_pos..char_end].iter().collect();
                                            let after: String = title_chars[char_end..].iter().collect();

                                            rsx! {
                                                span { {before} }
                                                span { 
                                                    style: "background-color: yellow; font-weight: bold;",
                                                    {highlight} 
                                                }
                                                span { {after} }
                                            }
                                        } else {
                                            rsx! { span { {title.clone()} } }
                                        }
                                    }
                                } else {
                                    span { {source_file.name.clone()} }
                                }
                            }

                            if let Some(content) = matched_content {
                                div {
                                    class: "search-result-content",
                                    style: "margin-top: 5px; font-size: 0.9em; color: #666;",
                                    // For content matches, we'll manually split and highlight
                                    {
                                        let content_lower = content.to_lowercase();
                                        let query_lower = query_str.to_lowercase();

                                        if let Some(pos) = content_lower.find(&query_lower) {
                                            // Convert to character indices for safe slicing
                                            let content_chars: Vec<char> = content.chars().collect();

                                            // Find the character index corresponding to the byte index
                                            let mut char_pos: usize = 0;
                                            for (i, _) in content_lower.char_indices() {
                                                if i == pos {
                                                    break;
                                                }
                                                char_pos += 1;
                                            }

                                            // Calculate the end position in character indices
                                            let query_char_len = query_lower.chars().count();
                                            let char_end = char_pos + query_char_len;

                                            // Create the substrings using character indices
                                            let before: String = content_chars[0..char_pos].iter().collect();
                                            let highlight: String = content_chars[char_pos..char_end].iter().collect();
                                            let after: String = content_chars[char_end..].iter().collect();

                                            rsx! {
                                                span { "..." {before} }
                                                span { 
                                                    style: "background-color: yellow; font-weight: bold;",
                                                    {highlight} 
                                                }
                                                span { {after} "..." }
                                            }
                                        } else {
                                            rsx! { span { "..." {content.clone()} "..." } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();

    let filter_string: Signal<String> = use_signal(|| "".to_string());
    let mut search_results: Signal<Vec<SearchResult>> = use_signal(Vec::new);

    let mut source_files: Signal<Vec<SourceFile>> = use_context();
    let selected_items: Signal<Vec<SelectedItemRepresentation>> = use_context();
    let active_selected_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_detailed_item_id: Signal<Option<usize>> = use_signal(|| None);
    let active_selection_filter: Signal<SelectionFilterOptions> =
        use_signal(|| SelectionFilterOptions::Songs);
    let mut running_presentations: Signal<Vec<RunningPresentation>> = use_context();

    let input_element_signal: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    // Update search results when filter_string changes
    use_effect(move || {
        let query = filter_string.read().clone();
        if !query.is_empty() {
            let results = search_source_files(&source_files.read(), &query);
            search_results.set(results);
        } else {
            search_results.set(Vec::new());
        }
    });

    // Update search results when filter_string changes
    use_effect(move || {
        let query = filter_string.read().clone();
        if !query.is_empty() {
            let results = search_source_files(&source_files.read(), &query);
            search_results.set(results);
        } else {
            search_results.set(Vec::new());
        }
    });

    let default_presentation_design_memo =
        use_memo(move || match settings.read().presentation_designs.first() {
            Some(design) => design.clone(),
            None => PresentationDesign::default(),
        });

    let default_song_slide_settings_memo = use_memo(move || {
        settings
            .read()
            .song_slide_settings
            .first()
            .unwrap_or(&SlideSettings::default())
            .clone()
    });

    use_effect(move || {
        if !settings.read().wizard_completed {
            nav.replace(Route::Wizard {});
        }

        use_future(move || async move {
            let files = settings.read().get_sourcefiles_async().await;
            source_files.set(files);
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

            // Display search results if there are any
            SearchResults {
                search_results: search_results,
                query: filter_string,
                selected_items: selected_items
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
                        onclick: move |_| { nav.push(crate::Route::SettingsPage {}); },
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
                        onclick: move |_| start_presentation(&selected_items.read().clone(), &mut running_presentations, &default_presentation_design_memo(), &default_song_slide_settings_memo()),
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
                        { t!("general.close") }
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
    default_slide_settings: &SlideSettings,
) {
    // Create the presentation

    use super::presentation_components::PresentationPage;
    use dioxus::desktop::Config;

    if presentation::add_presentation(
        selected_items,
        running_presentations,
        default_presentation_design,
        default_slide_settings,
    )
    .is_some()
    {
        // Create a new window if running on desktop
        let presentation_dom =
            VirtualDom::new(PresentationPage).with_root_context(*running_presentations);

        let window = tao::window::WindowBuilder::new()
            .with_resizable(true)
            .with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0))
            .with_maximized(true)
            .with_decorations(true)
            .with_visible(true);

        dioxus::desktop::window().new_window(
            presentation_dom,
            Config::new().with_menu(None).with_window(window),
        );
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
