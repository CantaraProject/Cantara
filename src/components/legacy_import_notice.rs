//! Telling the user that their Cantara 2 installation was taken over.
//!
//! Shown once, at the end of the very start on which the import happened, and
//! never again: the notice is picked up from
//! [`crate::logic::legacy_import::take_notice`], which hands it over exactly
//! once. Nothing about it is written to disk — see the note there for why a
//! flag in the settings would be the worse arrangement.
//!
//! It is a notice rather than a question. The import has already happened by
//! the time this is drawn, and everything it did can be undone in the settings
//! — so there is nothing here to agree to, only something to have read.

use dioxus::prelude::*;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// The notice, if this start imported a Cantara 2 configuration.
///
/// Mounted once, at the root of the app, beside the dialog host. Renders
/// nothing at all on every other start.
#[component]
pub fn LegacyImportNotice() -> Element {
    // Taken in a hook rather than in the body: the body runs again on every
    // redraw, and the second run would find the letterbox empty and close the
    // dialog the user is reading.
    #[cfg(not(target_arch = "wasm32"))]
    let report = use_hook(crate::logic::legacy_import::take_notice);
    #[cfg(target_arch = "wasm32")]
    let report: Option<crate::logic::legacy_import::LegacyImportReport> = None;

    let mut open = use_signal(|| report.is_some());

    let Some(report) = report.filter(|_| open()) else {
        return rsx! {};
    };

    let design_name = t!("legacy_import.design_name").to_string();
    let division_name = t!("legacy_import.division_name").to_string();

    rsx! {
        dialog {
            open: true,
            class: "cantara-dialog",
            role: "dialog",
            aria_modal: "true",
            // Escape closes it, as it closes every other dialog in the
            // program. There is nothing to lose by closing it.
            onkeydown: move |event: Event<KeyboardData>| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    open.set(false);
                }
            },
            article {
                header {
                    h3 { { t!("legacy_import.title").to_string() } }
                }

                p { class: "cantara-dialog-text", { t!("legacy_import.explanation").to_string() } }

                ul { class: "legacy-import-list",
                    if let Some(name) = &report.repository {
                        li { { t!("legacy_import.repository", name = name).to_string() } }
                    }
                    if let Some(path) = &report.missing_repository {
                        li { class: "legacy-import-warning",
                            { t!("legacy_import.missing_repository", path = path).to_string() }
                        }
                    }
                    li { { t!("legacy_import.design", name = design_name).to_string() } }
                    li { { t!("legacy_import.division", name = division_name).to_string() } }
                    if let Some(family) = &report.font_family {
                        li { { t!("legacy_import.font", name = family).to_string() } }
                    }
                    if report.background_image {
                        li { { t!("legacy_import.background_image").to_string() } }
                    }
                }

                p { class: "legacy-import-note",
                    { t!("legacy_import.not_everything").to_string() }
                }
                p { class: "legacy-import-source",
                    {
                        t!(
                            "legacy_import.source",
                            path = report.source.display().to_string()
                        ).to_string()
                    }
                }

                footer { class: "cantara-dialog-actions",
                    button {
                        class: "primary",
                        autofocus: true,
                        onclick: move |_| open.set(false),
                        { t!("legacy_import.close").to_string() }
                    }
                }
            }
        }
    }
}
