//! The search field and the list of what it found.
//!
//! Both views use these: the selection view collects a hit into the
//! presentation, the detail view opens it. What is searched and how is decided
//! in [`crate::logic::search`]; this module only shows the result.

use crate::components::selection_components::source_items::ItemClickAction;
use crate::logic::presentation::markdown_to_html;
use crate::logic::search::{Excerpt, SearchResult, highlight_in_html};
use crate::logic::sourcefiles::{SourceFile, SourceFileType};
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

/// The passage a hit was found in, and where in the element it sits.
///
/// The box is the point: a line of lyrics needs the verse around it and the
/// name of that verse before it means anything, and a sentence out of a sermon
/// needs the paragraph. What can be shown differs by format, so this is the one
/// place that knows how each of them is read.
#[component]
fn ResultContext(excerpt: Excerpt, file_type: SourceFileType) -> Element {
    rsx! {
        div { class: "search-result-context",
            if let Some(label) = excerpt.label.clone() {
                span { class: "search-result-label", "{label}" }
            }

            div { class: "search-result-content",
                match file_type {
                    // A document is read as what it is. Rendering it means the
                    // positions the search recorded no longer point at what is
                    // on screen, so the words are found again in the rendered
                    // HTML — see [`highlight_in_html`].
                    SourceFileType::Markdown => {
                        let html = markdown_to_html(&excerpt.text);
                        let html = highlight_in_html(&html, &excerpt.matched_words());
                        rsx! { div { class: "search-result-markdown", dangerous_inner_html: "{html}" } }
                    }
                    // Lyrics and the text of a page are read as they were
                    // written, line for line: a verse whose lines run together
                    // is not a verse any more.
                    _ => rsx! {
                        if excerpt.cut_before {
                            span { "… " }
                        }
                        Highlighted {
                            text: excerpt.text.clone(),
                            positions: excerpt.highlights.clone(),
                        }
                        if excerpt.cut_after {
                            span { " …" }
                        }
                    },
                }
            }
        }
    }
}

/// Component to display search results
#[component]
pub(crate) fn SearchResults(
    /// Which hits there are, and what taking one means.
    picker: ResultPicker,

    /// Which hit the keyboard is on — see [`SearchInput`], which moves it.
    active_result: Signal<usize>,
) -> Element {
    let search_results = picker.results;
    // Walking the list with the arrow keys past its bottom edge has to bring
    // the hit into view, or the selection is somewhere the user cannot see.
    // `nearest` so that a hit already on screen is left where it is.
    use_effect(move || {
        let _ = active_result();
        let _ = document::eval(
            "requestAnimationFrame(function () {
                 var hit = document.querySelector('.search-result-active');
                 if (hit) { hit.scrollIntoView({ block: 'nearest' }); }
             });",
        );
    });

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
                        div {
                            class: if index == active_result() { "search-result search-result-active" } else { "search-result" },
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
                                onclick: move |_| picker.take(index),
                                Highlighted {
                                    text: source_file.name.clone(),
                                    positions: title_highlights,
                                }
                            }

                            if let Some(excerpt) = excerpt {
                                ResultContext {
                                    excerpt,
                                    file_type: source_file.file_type,
                                }
                            }

                            // A picture has no text to quote, so it shows
                            // itself. The scaled-down copy is the one the
                            // library list uses and is made during the scan —
                            // nothing is read from disk here. See
                            // [`crate::logic::images`].
                            if source_file.file_type == SourceFileType::Image
                                && let Some(thumbnail) = crate::logic::images::thumbnail(&source_file.path)
                            {
                                div { class: "search-result-context",
                                    img {
                                        class: "search-result-picture",
                                        src: "{thumbnail}",
                                        alt: source_file.name.clone(),
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

/// Taking a hit out of the result list.
///
/// A hit can be taken in three ways — clicked, pressed Enter on, or named by
/// its `Alt` shortcut — and all three mean exactly the same thing. They used to
/// spell it out separately, and drifted: the shortcut left the query standing
/// in the field while Enter cleared it, and the click hid the list without
/// emptying it, so the next letter typed searched the old query. One way of
/// saying it, passed to whoever needs to say it.
///
/// Which hits there are and what taking one means both belong to the view, not
/// to the field or the list, which is why this travels as a value.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ResultPicker {
    /// The hits, so one can be named by its position in the list.
    pub(crate) results: Signal<Vec<SearchResult>>,

    /// The query, which is emptied once a hit has been taken — the search is
    /// over, and what was looked for should not be left standing in the way of
    /// the next search.
    pub(crate) query: Signal<String>,

    /// What taking a hit means. The detail view opens it; the selection view
    /// collects it. Without this a search in the detail view quietly added the
    /// song to the presentation instead of showing it.
    pub(crate) action: ItemClickAction,

    /// What the selection view collects into.
    pub(crate) selected_items: Signal<Vec<SelectedItemRepresentation>>,

    /// The library the results point into, so a hit can be opened by index.
    pub(crate) source_files: Signal<Vec<SourceFile>>,

    /// Where the detail view records what is open.
    pub(crate) active_detailed_item_id: Signal<Option<usize>>,
}

impl ResultPicker {
    /// Takes the hit at `index`, and has done with the search.
    ///
    /// Nothing hides the result list here: emptying the query does that, since
    /// a list of hits for nothing is no list at all.
    pub(crate) fn take(mut self, index: usize) {
        let Some(result) = self.results.read().get(index).cloned() else {
            return;
        };

        match self.action {
            ItemClickAction::AddToSelection => {
                self.selected_items
                    .write()
                    .push(SelectedItemRepresentation::new_with_sourcefile(
                        result.source_file,
                    ));
            }
            ItemClickAction::OpenDetail => {
                if let Some(index) = index_of(&self.source_files.read(), &result.source_file) {
                    self.active_detailed_item_id.set(Some(index));
                }
            }
        }

        self.clear_query();
    }

    /// Empties the field — both the query and what stands in the element.
    ///
    /// Setting the query to nothing is not enough: the element keeps its own
    /// text (see `initial_value` in [`SearchInput`]), which is the whole point
    /// of binding it that way, so it has to be told separately on the rare
    /// occasion that something other than the user empties it.
    fn clear_query(&mut self) {
        self.query.set(String::new());
        let _ = document::eval(
            "var field = document.getElementById('searchinput');
             if (field) { field.value = ''; }",
        );
    }
}

/// The search field, and the keyboard's way around the hits it produced.
///
/// Walking the result list is handled here rather than in [`SearchResults`],
/// because the field holds the focus for as long as the search is open: the
/// arrow keys arrive here, and nowhere else.
#[component]
pub(crate) fn SearchInput(
    input_signal: Signal<String>,
    element_signal: Signal<Option<Rc<MountedData>>>,

    /// Called when the user presses Escape in the field. Both views use it to
    /// put the result list away — which is the field's business, since it is
    /// the field that holds the focus while the search is open.
    #[props(default)]
    on_escape: EventHandler<()>,

    /// Which hits there are, and what taking one means — Enter takes the one
    /// the keyboard is on.
    picker: ResultPicker,

    /// Which hit the keyboard is on. Shared with [`SearchResults`], which marks
    /// it, and reset to the first hit by the view whenever the query changes —
    /// so a search that has just been typed can be taken with Enter alone.
    mut active_result: Signal<usize>,
) -> Element {
    let result_count = picker.results.read().len();

    rsx! {
        div {
            role: "group",
            input {
                id: "searchinput",
                onmounted: move |element| element_signal.set(Some(element.data())),
                r#type: "search",
                name: "search",
                placeholder: t!("search").to_string(),
                aria_label: t!("search").to_string(),
                // `initial_value`, not `value`: the field writes the query, and
                // nothing else ever does, so binding the signal back into the
                // element could only ever undo what the user just typed.
                //
                // `value` is a volatile attribute — every render writes it into
                // the element, whether or not the query changed. Between a
                // keystroke and the render it causes lies a round trip, and a
                // letter typed inside that window was overwritten by the older
                // query before its own event had been handled: typing quickly
                // lost letters, the more so the longer the search took. With
                // `initial_value` the element keeps what was typed into it and
                // the signal follows along.
                initial_value: input_signal(),
                oninput: move |event| {
                    input_signal.set(event.value());
                },
                onkeydown: move |event: Event<KeyboardData>| {
                    match event.key() {
                        Key::Escape => on_escape.call(()),
                        // Nothing to walk through and nothing to take: every
                        // other key is the query's.
                        _ if result_count == 0 => {}
                        Key::Enter => {
                            event.prevent_default();
                            picker.take(active_result().min(result_count - 1));
                        }
                        // Tab walks the list instead of leaving the field: the
                        // list is what there is to move around in while the
                        // search is open, and the field has to keep the focus
                        // for the next letter to reach the query. The arrow
                        // keys would otherwise move the caret.
                        //
                        // The ends are joined, so holding one key round-trips
                        // rather than sticking.
                        Key::ArrowDown | Key::Tab if !event.modifiers().shift() => {
                            event.prevent_default();
                            active_result.set((active_result() + 1) % result_count);
                        }
                        Key::ArrowUp | Key::Tab => {
                            event.prevent_default();
                            active_result.set((active_result() + result_count - 1) % result_count);
                        }
                        _ => {}
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
