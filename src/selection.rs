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

#[component]
pub fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();

    let filter_string: Signal<String> = use_signal(|| "".to_string());

    if settings.read().song_repos.is_empty() || !settings.read().wizard_completed {
        nav.replace(Route::Wizard {});
    }

    let source_files: Signal<Vec<SourceFile>> = use_signal(|| settings.read().get_sourcefiles());

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

        header {
            class: "top-bar no-padding",
            SearchInput {
                input_signal: filter_string,
                element_signal: input_element_signal
            }
        }
        main {
            class: "container-fluid content",
            onkeydown: move |_| async move {
                if let Some(searchinput) = input_element_signal() {
                    let _ = searchinput.set_focus(true).await;
                }
            },
            for item in source_files.read().iter() {
                    SourceItem { item: item.clone() }
            }
        }
        footer {
            class: "bottom-bar",
            p {
                "Start Presentation"
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
fn SourceItem(item: SourceFile) -> Element {
    rsx! {
        div {
            role: "button",
            class: "outline selection_item",
            tabindex: 0,
            { item.name }
        }
    }
}
