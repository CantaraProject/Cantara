//! The search field and the list of what it found.
//!
//! Both views use these: the selection view collects a hit into the
//! presentation, the detail view opens it. What is searched and how is decided
//! in [`crate::logic::search`]; this module only shows the result.

use crate::components::selection_components::source_items::ItemClickAction;
use crate::logic::search::SearchResult;
use crate::logic::sourcefiles::SourceFile;
use crate::logic::states::SelectedItemRepresentation;
use dioxus::prelude::*;
use rust_i18n::t;
use std::rc::Rc;

/// Text with the characters the search matched picked out.
///
/// The positions come from the search itself, which is the only thing that
/// knows what it matched — with a fuzzy match they are scattered through the
/// word rather than forming one run, so looking for the query in the text
/// again here would highlight the wrong characters, or none at all.
#[component]
fn Highlighted(text: String, positions: Vec<usize>) -> Element {
    if positions.is_empty() {
        return rsx! { span { {text} } };
    }

    // Neighbouring positions become one span, so a match of three letters is
    // one highlight rather than three.
    let mut runs: Vec<(String, bool)> = Vec::new();
    for (index, character) in text.chars().enumerate() {
        let highlighted = positions.contains(&index);
        match runs.last_mut() {
            Some((run, run_highlighted)) if *run_highlighted == highlighted => {
                run.push(character);
            }
            _ => runs.push((character.to_string(), highlighted)),
        }
    }

    rsx! {
        for (run , highlighted) in runs {
            if highlighted {
                mark { class: "search-highlight", {run} }
            } else {
                span { {run} }
            }
        }
    }
}

/// Component to display search results
#[component]
pub(crate) fn SearchResults(
    search_results: Signal<Vec<SearchResult>>,
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    search_visible: Signal<bool>,

    /// The library the results point into, so a hit can be opened by index.
    source_files: Signal<Vec<SourceFile>>,

    /// What picking a result means. The detail view opens it; the selection
    /// view collects it. Without this a search in the detail view quietly added
    /// the song to the presentation instead of showing it.
    #[props(default)]
    click_action: ItemClickAction,

    /// Where the detail view records what is open.
    active_detailed_item_id: Signal<Option<usize>>,
) -> Element {
    let results = search_results.read().clone();
    if results.is_empty() {
        return rsx! { div {} };
    }

    rsx! {
        div {
            // No `tabindex` and no focus of its own: the search field keeps the
            // focus for as long as the search is open, so that typing — and
            // above all backspace — always reaches the query.
            class: "search-results scrollable-container",
            onclick: move |event| {
                event.stop_propagation();
            },
            h3 { {t!("search.results").to_string()} }

            for (index , result) in results.iter().enumerate() {
                {
                    let source_file = result.source_file.clone();
                    let excerpt = result.excerpt.clone();
                    let title_highlights = result.title_highlights.clone();

                    rsx! {
                        div { class: "search-result",
                            if index < 10 {
                                span { class: "search-result-shortcut",
                                    {
                                        let number = if index == 9 { "0".to_string() } else { (index + 1).to_string() };
                                        t!("search.result_number", number => number).to_string()
                                    }
                                }
                            }
                            div {
                                class: "search-result-title",
                                onclick: {
                                    let source_file = source_file.clone();
                                    move |_| {
                                        pick(
                                            &source_file,
                                            click_action,
                                            selected_items,
                                            source_files,
                                            active_detailed_item_id,
                                        );
                                        search_visible.set(false);
                                    }
                                },
                                Highlighted {
                                    text: source_file.name.clone(),
                                    positions: title_highlights,
                                }
                            }

                            if let Some(excerpt) = excerpt {
                                div { class: "search-result-content",
                                    span { "…" }
                                    Highlighted { text: excerpt.text, positions: excerpt.highlights }
                                    span { "…" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Acts on a chosen result: the selection view collects it, the detail view
/// opens it.
///
/// Shared by the click on a result and the keyboard shortcut, so the two can
/// never come to mean different things.
pub(crate) fn pick(
    source_file: &SourceFile,
    click_action: ItemClickAction,
    mut selected_items: Signal<Vec<SelectedItemRepresentation>>,
    source_files: Signal<Vec<SourceFile>>,
    mut active_detailed_item_id: Signal<Option<usize>>,
) {
    match click_action {
        ItemClickAction::AddToSelection => {
            selected_items
                .write()
                .push(SelectedItemRepresentation::new_with_sourcefile(
                    source_file.clone(),
                ));
        }
        ItemClickAction::OpenDetail => {
            if let Some(index) = index_of(&source_files.read(), source_file) {
                active_detailed_item_id.set(Some(index));
            }
        }
    }
}

#[component]
pub(crate) fn SearchInput(
    input_signal: Signal<String>,
    element_signal: Signal<Option<Rc<MountedData>>>,

    /// Called when the user presses Escape in the field. Both views use it to
    /// put the result list away — which is the field's business, since it is
    /// the field that holds the focus while the search is open.
    #[props(default)]
    on_escape: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            role: "group",
            onmounted: move |element| element_signal.set(Some(element.data())),
            input {
                id: "searchinput",
                r#type: "search",
                name: "search",
                placeholder: t!("search").to_string(),
                aria_label: t!("search").to_string(),
                value: input_signal,
                oninput: move |event| {
                    input_signal.set(event.value());
                },
                onkeydown: move |event: Event<KeyboardData>| {
                    if event.key() == Key::Escape {
                        on_escape.call(());
                    }
                },
            }
        }
    }
}

/// Where a search hit sits in the library.
///
/// Matched by path: a result carries a copy of the source file, and two songs
/// can share a name but never a path.
fn index_of(source_files: &[SourceFile], wanted: &SourceFile) -> Option<usize> {
    source_files
        .iter()
        .position(|candidate| candidate.path == wanted.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::sourcefiles::SourceFileType;
    use std::path::PathBuf;

    fn file(name: &str, path: &str) -> SourceFile {
        SourceFile {
            name: name.to_string(),
            path: PathBuf::from(path),
            file_type: SourceFileType::Song,
            md5_hash: None,
            relative_path: None,
        }
    }

    /// Two songs may well share a title — the library in the screenshots has
    /// several — so the hit has to be found by path.
    #[test]
    fn test_a_hit_is_found_by_path_not_by_name() {
        let files = vec![
            file("Seht unsern Gott", "/a/Seht unsern Gott.song"),
            file("Seht unsern Gott", "/b/Seht unsern Gott.song"),
        ];

        assert_eq!(index_of(&files, &files[1]), Some(1));
    }

    #[test]
    fn test_a_hit_outside_the_library_is_not_invented() {
        let files = vec![file("A", "/a.song")];

        assert_eq!(index_of(&files, &file("B", "/b.song")), None);
    }
}
