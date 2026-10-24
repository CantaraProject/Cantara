//! Asking for access to the phone's shared storage, where Android withholds
//! it.
//!
//! Two places ask. The directory browser, so that a folder chosen in it is
//! one whose songs can be read; and a notice at the root of the app, for a
//! library that is already set up but cannot be read — after an update from a
//! version that never asked, or when the user took the permission back.
//!
//! Neither learns the answer from Android (see [`crate::logic::storage_access`]),
//! so both watch for it: once a second, for as long as they are on screen.
//! When access arrives the library is scanned again — it was scanned at the
//! start, when every song was still hidden.
//!
//! On every other platform there is nothing to ask, and both render nothing.

use dioxus::prelude::*;
// The browser build renders the empty notice below, which has nothing to say.
#[cfg(not(target_arch = "wasm32"))]
use rust_i18n::t;

#[cfg(not(target_arch = "wasm32"))]
use crate::logic::{
    settings::{RepositoryType, Settings},
    states::LibraryRefresh,
    storage_access,
};

rust_i18n::i18n!("locales", fallback = "en");

/// How often an open request looks whether access has been granted. The
/// user comes back from the settings page by hand, so anything shorter is
/// not noticed, and anything much longer is.
#[cfg(not(target_arch = "wasm32"))]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// The explanation and the button, for as long as access is missing.
///
/// Calls `on_granted` once, when access arrives, and then renders nothing.
/// Renders nothing from the start where access is not restricted, or already
/// there.
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn StorageAccessRequest(on_granted: EventHandler<()>) -> Element {
    let mut granted = use_signal(storage_access::has_access);
    let mut library_refresh = use_context::<LibraryRefresh>();

    use_future(move || async move {
        while !*granted.peek() {
            crate::logic::timer::sleep(POLL_INTERVAL).await;
            if storage_access::has_access() {
                granted.set(true);
                // The library was scanned while its songs were hidden.
                library_refresh.request();
                on_granted.call(());
            }
        }
    });

    if granted() {
        return rsx! {};
    }

    rsx! {
        section { class: "storage-access-request",
            p { { t!("storage_access.explanation").to_string() } }
            p {
                small { { t!("storage_access.how").to_string() } }
            }
            button {
                class: "primary",
                onclick: move |_| storage_access::request(),
                { t!("storage_access.grant").to_string() }
            }
        }
    }
}

/// There is no shared storage a browser could ask for.
#[cfg(target_arch = "wasm32")]
#[component]
pub fn StorageAccessRequest(on_granted: EventHandler<()>) -> Element {
    let _ = on_granted;
    rsx! {}
}

/// The notice at the root of the app, for a library that is set up but
/// cannot be read.
///
/// Shown at the start when at least one repository is a folder on the device
/// and access to it is missing. "Later" closes it for this start; it comes back
/// on the next, because the library stays empty until access is granted.
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn StorageAccessNotice() -> Element {
    let settings = use_context::<Signal<Settings>>();

    // Decided once, at the start: asked in a hook rather than the body, which
    // runs again on every redraw.
    let needed = use_hook(|| {
        storage_access::is_restricted_platform() && !storage_access::has_access()
    });
    let mut open = use_signal(|| true);

    let has_local_repository = settings
        .read()
        .repositories
        .iter()
        .any(|repository| matches!(repository.repository_type, RepositoryType::LocaleFilePath(_)));

    if !needed || !open() || !has_local_repository {
        return rsx! {};
    }

    rsx! {
        dialog {
            open: true,
            class: "cantara-dialog",
            role: "dialog",
            aria_modal: "true",
            article {
                header {
                    h3 { { t!("storage_access.title").to_string() } }
                }

                StorageAccessRequest { on_granted: move |_| open.set(false) }

                footer { class: "cantara-dialog-actions",
                    button {
                        class: "secondary",
                        onclick: move |_| open.set(false),
                        { t!("storage_access.later").to_string() }
                    }
                }
            }
        }
    }
}

/// There is no shared storage a browser could ask for.
#[cfg(target_arch = "wasm32")]
#[component]
pub fn StorageAccessNotice() -> Element {
    rsx! {}
}
