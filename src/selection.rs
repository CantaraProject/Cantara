//! This module includes the components for song selection

use dioxus::prelude::*;
use dioxus_router::prelude::navigator;
use rust_i18n::t;
use crate::{settings::Settings, Route};

#[component]
pub fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();
    
    let filter_string: Signal<String> = use_signal(|| "".to_string());
    
    if settings.read().song_repos.is_empty() || !settings.read().wizard_completed {
        nav.replace(Route::Wizard {});
    }

    rsx! {
        header {
            SearchInput { input_signal: filter_string }
        }
        div {
            "Selection Page"
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