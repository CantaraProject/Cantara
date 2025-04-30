//! This module includes the components for song selection

use dioxus::prelude::*;
use dioxus_router::prelude::navigator;

#[component]
fn Selection() -> Element {
    let nav = navigator();
    let settings: Signal<Settings> = use_context();

    if settings.read().song_repos.is_empty() || !settings.read().wizard_completed {
        nav.replace(Route::Wizard {});
    }

    rsx! {
        div {
            "Selection Page"
        }
    }
}
