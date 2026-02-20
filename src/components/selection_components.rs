//! This module includes the components for song selection

use super::shared_components::{ExamplePresentationViewer, ImageIcon, MusicIcon};
use crate::TEST_STATE;
use crate::logic::presentation;
use crate::logic::search::{SearchResult, search_source_files};
use crate::logic::settings::PresentationDesign;
use crate::logic::sourcefiles::SourceFileType;
use crate::logic::states::{RunningPresentation, SelectedItemRepresentation};
use crate::logic::settings::{Settings, use_settings};
use crate::logic::sourcefiles::SourceFile;
use crate::Route;
use cantara_songlib::slides::SlideSettings;
use dioxus::desktop::tao;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::*;
use dioxus_free_icons::icons::fa_solid_icons::{FaArrowDown, FaArrowUp};
use rust_i18n::t;
use std::rc::Rc;

rust_i18n::i18n!("locales", fallback = "en");

/// Component to display search results
#[component]
fn SearchResults(
    search_results: Signal<Vec<SearchResult>>,
    query: Signal<String>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    search_visible: Signal<bool>,
) -> Element {
    let results = search_results.read().clone();
    if results.is_empty() {
        return rsx! { div {} };
    }

    let query_str = query.read().clone();

    rsx! {
        div {
            class: "search-results scrollable-container",
            tabindex: 0,
            // Prevent clicks inside search results from closing them
            onclick: move |event| {
                event.stop_propagation();
            },
            onmounted: move |element| {
                let _ = element.set_focus(true);
            },
            onkeydown: move |event: Event<KeyboardData>| {
                let key = event.key();

                // Handle Escape key to close search results
                if key == Key::Escape {
                    search_visible.set(false);
                    event.stop_propagation();
                }
            },
            h3 { { t!("search.results").to_string() } }

            for (index, result) in results.iter().enumerate() {
                {
                    let source_file = result.source_file.clone();
                    let matched_content = result.matched_content.clone();
                    let is_title_match = result.is_title_match;

                    rsx! {
                        div {
                            class: "search-result",
                            style: "margin-bottom: 10px; padding: 5px; border-bottom: 1px solid #eee;",
                            // Show number for first 10 results
                            if index < 10 {
                                div {
                                    style: "display: inline-block; margin-right: 5px; font-weight: bold; color: #666;",
                                    // Use 0 for the 10th item
                                    {
                                        let number = if index == 9 { "0" } else { &(index + 1).to_string() };
                                        t!("search.result_number", number => number).to_string()
                                    }
                                }
                            }
                            div {
                                class: "search-result-title",
                                style: "font-weight: bold; cursor: pointer;",
                                onclick: move |_| {
                                    selected_items.write().push(
                                        SelectedItemRepresentation::new_with_sourcefile(source_file.clone())
                                    );
                                    // Close search results after selection
                                    search_visible.set(false);
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
    let mut search_visible: Signal<bool> = use_signal(|| false);

    let mut source_files: Signal<Vec<SourceFile>> = use_context();
    let mut selected_items: Signal<Vec<SelectedItemRepresentation>> = use_context();
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
            let has_results = !results.is_empty();
            search_results.set(results);
            search_visible.set(has_results);
        } else {
            search_results.set(Vec::new());
            search_visible.set(false);
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
            style: "position: relative;",
            // Add onkeydown handler to the wrapper div to handle number key presses globally
            onkeydown: move |event: Event<KeyboardData>| {
                // Handle number keys for quick selection when search results are visible
                if search_visible() {
                    let key_str = event.key().to_string();
                    if key_str.len() == 1 {
                        if let Some(digit) = key_str.chars().next().and_then(|c| c.to_digit(10)) {
                            let index = if digit == 0 { 9 } else { (digit as usize) - 1 };
                            let results = search_results.read();
                            if index < results.len() {
                                selected_items.write().push(
                                    SelectedItemRepresentation::new_with_sourcefile(results[index].source_file.clone())
                                );
                                // Close search results after selection
                                search_visible.set(false);
                                event.stop_propagation();
                            }
                        }
                    }
                }
            },
            header {
                class: "top-bar no-padding",
                SearchInput {
                    input_signal: filter_string,
                    element_signal: input_element_signal
                }
            }

            // Display search results if there are any and search_visible is true
            if search_visible() {
                SearchResults {
                    search_results: search_results,
                    query: filter_string,
                    selected_items: selected_items,
                    search_visible: search_visible
                }
            }
            main {
                id: "selection-content",
                class: "content content-background height-100",
                // Close search results when clicking on the main content
                onclick: move |_| {
                    if search_visible() {
                        search_visible.set(false);
                    }
                },
                onmounted: move |_| async move {
                    // This is necessary because we need to run the adjustDivHeight javascript function once to prevent wrong sizening of the elements.
                    let _ = document::eval("adjustDivHeight();").await;
                },
                onkeydown: move |event: Event<KeyboardData>| async move {
                    // Don't focus search input if a number key is pressed and search results are visible
                    let key = event.key().to_string();
                    if search_visible() && key.len() == 1 && key.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                        return;
                    }

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
                            { t!("settings.settings_button").to_string() }
                        }
                    },
                    button {
                        class: "outline secondary smaller-buttons",
                        span {
                            class: "desktop-only",
                            { t!("selection.import").to_string() }
                        }
                    },
                    button {
                        class: "outline secondary smaller-buttons",
                        span {
                            class: "desktop-only",
                            { t!("selection.export").to_string() }
                        }
                    },
                    button {
                        class: "primary smaller-buttons",
                        onclick: move |_| start_presentation(&selected_items.read().clone(), &mut running_presentations, &default_presentation_design_memo(), &default_song_slide_settings_memo()),
                        span {
                            class: "desktop-only",
                            { t!("selection.start_presentation").to_string() }
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
    // Track drag state for custom mouse-based reordering
    let mut dragging_from: Signal<Option<usize>> = use_signal(|| None);
    let mut hover_over: Signal<Option<usize>> = use_signal(|| None);
    // Animation signals: target index to animate and a flip to retrigger animation
    let mut anim_target: Signal<Option<usize>> = use_signal(|| None);
    let mut anim_flip: Signal<bool> = use_signal(|| false);

    rsx! {
        div {
            class: "selected-container",
            onmouseup: move |_| {
                if let (Some(from), Some(to)) = (dragging_from(), hover_over()) {
                    if from != to {
                        let mut items = selected_items.write();
                        let len_before = items.len();
                        if from < len_before && to <= len_before {
                            let item = items.remove(from);
                            let insert_at = if to > from { to - 1 } else { to };
                            let final_index = insert_at;
                            items.insert(insert_at, item);
                            // trigger animation on the moved item
                            anim_target.set(Some(final_index));
                            anim_flip.set(!anim_flip());
                        }
                    }
                }
                dragging_from.set(None);
                hover_over.set(None);
            },
            onmouseleave: move |_| {
                // Cancel drag if pointer leaves container
                dragging_from.set(None);
                hover_over.set(None);
            },
            for (number, _) in selected_items.read().iter().enumerate() {
                SelectedItem {
                    selected_items: selected_items,
                    id: number,
                    active_selected_item_id: active_selected_item_id,
                    dragging_from: dragging_from,
                    hover_over: hover_over,
                    anim_target: anim_target,
                    anim_flip: anim_flip,
                }
            }
            // Bottom drop zone to allow moving below the last item
            if dragging_from().is_some() {
                div {
                    style: {
                        let active = hover_over() == Some(selected_items.read().len());
                        let mut s = String::from("height: 12px; margin-top: 6px; border-top: 2px dashed #bbb;");
                        if active { s.push_str(" border-color: #666;"); }
                        s
                    },
                    onmouseenter: move |_| {
                        hover_over.set(Some(selected_items.read().len()));
                    },
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
    dragging_from: Signal<Option<usize>>,
    hover_over: Signal<Option<usize>>,
    anim_target: Signal<Option<usize>>,
    anim_flip: Signal<bool>,
) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item",
            style: {
                let mut s = String::from("display: flex; align-items: left; cursor: grab; transition: background-color 300ms ease-out;");
                if dragging_from().is_some() && hover_over() == Some(id) {
                    s.push_str(" outline: 2px dashed #888; background-color: rgba(0,0,0,0.03);");
                }
                if anim_target() == Some(id) {
                    s.push_str(" background-color: rgba(255,230,150,0.8);");
                }
                s
            },
            tabindex: 0,
            onmouseenter: move |_| {
                if dragging_from.read().is_some() {
                    hover_over.set(Some(id));
                }
            },
            onmouseup: move |_| {
                // If mouse is released over the same item, the container onmouseup will also handle it
            },
            span {
                style: "flex-grow: 1;",
                onmousedown: move |_| {
                    anim_target.set(None);
                    dragging_from.set(Some(id));
                    hover_over.set(Some(id));
                },
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
    let settings = use_settings();

    use_effect(move || {
        if active_selected_item_id.read().is_some() {
            tab_state.set(PresentationOptionTabState::Specific);
        }
    });

    let active_id = active_selected_item_id.read();
    if active_id.is_none() {
        return rsx! {};
    }
    let item_index = active_id.unwrap();

    let preview_pd = use_memo(move || {
        let items = selected_items.read();
        let idx_opt = active_selected_item_id();
        if let Some(idx) = idx_opt {
            items
                .get(idx)
                .and_then(|item| item.presentation_design_option.clone())
                .unwrap_or_else(|| settings.read().presentation_designs[0].clone())
        } else {
            settings.read().presentation_designs[0].clone()
        }
    });

    let preview_ss = use_memo(move || {
        let items = selected_items.read();
        let idx_opt = active_selected_item_id();
        if let Some(idx) = idx_opt {
            items
                .get(idx)
                .and_then(|item| item.slide_settings_option.clone())
                .unwrap_or_else(|| settings.read().song_slide_settings[0].clone())
        } else {
            settings.read().song_slide_settings[0].clone()
        }
    });

    let mut current_ss_signal = use_signal(|| SlideSettings::default());
    use_effect(move || {
        current_ss_signal.set(preview_ss());
    });

    rsx! {
        div {
            role: "group",
            button {
                class: "smaller-buttons",
                class: if *tab_state.read() != PresentationOptionTabState::General {
                    "secondary"
                },
                onclick: move |_| { tab_state.set(PresentationOptionTabState::General) },
                { t!("selection.presentation_options.tab.general").to_string() }
            }
            button {
                class: "smaller-buttons",
                class: if *tab_state.read() != PresentationOptionTabState::Specific {
                    "secondary"
                },
                onclick: move |_| { tab_state.set(PresentationOptionTabState::Specific) },
                { t!("selection.presentation_options.tab.specific").to_string() }
            }
        }

        match *tab_state.read() {
            PresentationOptionTabState::General => {
                rsx! {
                    p { { TEST_STATE.read().clone() } }
                }
            }
            PresentationOptionTabState::Specific => {
                let items = selected_items.read();
                let item = items.get(item_index).cloned().unwrap();

                rsx! {
                    div {
                        class: "grid",
                        div {
                            label { { t!("selection.presentation_options.design").to_string() } }
                            select {
                                onchange: move |evt| {
                                    let val = evt.value();
                                    let mut items = selected_items.write();
                                    if val == "default" {
                                        items[item_index].presentation_design_option = None;
                                    } else if let Ok(idx) = val.parse::<usize>() {
                                        items[item_index].presentation_design_option = Some(settings.read().presentation_designs[idx].clone());
                                    }
                                },
                                option {
                                    value: "default",
                                    selected: item.presentation_design_option.is_none(),
                                    { t!("selection.presentation_options.default").to_string() }
                                }
                                for (idx, pd) in settings.read().presentation_designs.iter().enumerate() {
                                    option {
                                        value: "{idx}",
                                        selected: item.presentation_design_option.as_ref().map_or(false, |p| p.name == pd.name),
                                        "{pd.name}"
                                    }
                                }
                            }
                        }
                        div {
                            label { { t!("selection.presentation_options.slide_settings").to_string() } }
                            select {
                                onchange: move |evt| {
                                    let val = evt.value();
                                    let mut items = selected_items.write();
                                    if val == "default" {
                                        items[item_index].slide_settings_option = None;
                                    } else if let Ok(idx) = val.parse::<usize>() {
                                        items[item_index].slide_settings_option = Some(settings.read().song_slide_settings[idx].clone());
                                    }
                                },
                                option {
                                    value: "default",
                                    selected: item.slide_settings_option.is_none(),
                                    { t!("selection.presentation_options.default").to_string() }
                                }
                                for (idx, _) in settings.read().song_slide_settings.iter().enumerate() {
                                    option {
                                        value: "{idx}",
                                        selected: item.slide_settings_option.as_ref().map_or(false, |s| s == &settings.read().song_slide_settings[idx]),
                                        { format!("{} {}", t!("selection.presentation_options.slide_settings").to_string(), idx + 1) }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        style: "margin-top: 20px; display: flex; flex-direction: column; align-items: center;",
                        ExamplePresentationViewer {
                            presentation_design: preview_pd(),
                            song_slide_settings: Some(current_ss_signal),
                            width: 400,
                        }
                    }
                }
            }
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
                                    SourceFileType::Video => t!("general.video").to_string()
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
