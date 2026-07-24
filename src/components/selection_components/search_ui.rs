use crate::logic::search::SearchResult;
use crate::logic::states::SelectedItemRepresentation;
use dioxus::prelude::*;
use rust_i18n::t;
use std::rc::Rc;

/// Component to display search results
#[component]
pub(crate) fn SearchResults(
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
            onclick: move |event| {
                event.stop_propagation();
            },
            onmounted: move |element| {
                let _ = element.set_focus(true);
            },
            onkeydown: move |event: Event<KeyboardData>| {
                let key = event.key();
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
                            if index < 10 {
                                div {
                                    style: "display: inline-block; margin-right: 5px; font-weight: bold; color: #666;",
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
                                    search_visible.set(false);
                                },
                                if is_title_match {
                                    {
                                        let title = source_file.name.clone();
                                        let title_lower = title.to_lowercase();
                                        let query_lower = query_str.to_lowercase();

                                        if let Some(pos) = title_lower.find(&query_lower) {
                                            let title_chars: Vec<char> = title.chars().collect();

                                            let mut char_pos: usize = 0;
                                            for (i, _) in title_lower.char_indices() {
                                                if i == pos {
                                                    break;
                                                }
                                                char_pos += 1;
                                            }

                                            let query_char_len = query_lower.chars().count();
                                            let char_end = char_pos + query_char_len;

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
                                    {
                                        let content_lower = content.to_lowercase();
                                        let query_lower = query_str.to_lowercase();

                                        if let Some(pos) = content_lower.find(&query_lower) {
                                            let content_chars: Vec<char> = content.chars().collect();

                                            let mut char_pos: usize = 0;
                                            for (i, _) in content_lower.char_indices() {
                                                if i == pos {
                                                    break;
                                                }
                                                char_pos += 1;
                                            }

                                            let query_char_len = query_lower.chars().count();
                                            let char_end = char_pos + query_char_len;

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
pub(crate) fn SearchInput(
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
