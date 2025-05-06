//! This module includes the components for song selection

use crate::{
    settings::Settings,
    sourcefiles::{get_source_files, SourceFile},
    Route,
};
use dioxus::{html::u::outline, prelude::*};
use dioxus_router::prelude::navigator;
use rust_i18n::t;
use std::path::Path;

#[component]
pub fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();

    let filter_string: Signal<String> = use_signal(|| "".to_string());

    if settings.read().song_repos.is_empty() || !settings.read().wizard_completed {
        nav.replace(Route::Wizard {});
    }

    let source_files: Signal<Vec<SourceFile>> = use_signal(|| settings.read().get_sourcefiles());

    rsx! {
        header {
            class: "top-bar no-padding",
            SearchInput { input_signal: filter_string }
        }
        main {
            class: "container-fluid content",
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
fn SearchInput(input_signal: Signal<String>) -> Element {
    rsx! {
        div {
            role: "group",
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
            class: "outline",
            tabindex: 0,
            p {
                { item.name }
            }
        }
    }
}
