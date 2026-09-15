//! The about page: what the program is, who wrote it, and under what licence.
//!
//! A page of its own rather than a section of the settings, because it is an
//! address people send each other — the web build serves Cantara over a
//! network, and the Affero licence the program is under is precisely about
//! what a network user is owed. See `docs/specs/0005-add-info-page.md`.
//!
//! What is *decided* here is nothing: the version, the copyright and which
//! language's prose to show all come from [`crate::logic::about`], which can
//! be tested without a screen. This module only draws them.

use crate::logic::about;
use crate::logic::presentation::markdown_to_html;
use crate::logic::states::AboutEntryState;
use dioxus::prelude::*;
use rust_i18n::t;
use sys_locale::get_locale;

rust_i18n::i18n!("locales", fallback = "en");

#[component]
pub fn AboutPage() -> Element {
    // The same call `App` makes before `rust_i18n::set_locale`, and for the
    // same reason it is repeated rather than read back: `rust_i18n` takes a
    // locale but offers no way to ask which one it was given.
    let locale = get_locale().unwrap_or_else(|| String::from("en-US"));

    // Rendered once for the locale rather than on every draw: the text is
    // compiled in and cannot change while the page is open.
    let prose = use_memo(use_reactive!(|locale| markdown_to_html(about::text_for(
        &locale
    ))));

    rsx! {
        div { class: "wrapper",
            header { class: "top-bar",
                h2 { {t!("about.headline").to_string()} }
            }

            main { class: "container-fluid content height-100",
                article { class: "about-card",
                    // The name, the version and the copyright on one side, the
                    // logo on the other: this block is the program identifying
                    // itself, and the mark is part of that rather than an
                    // illustration of the prose below.
                    div { class: "about-identity",
                        div { class: "about-identity-text",
                            hgroup {
                                // The name of the program, which is a name and
                                // is not translated.
                                h1 { "Cantara" }
                                p { {t!("about.description").to_string()} }
                            }

                            p { class: "about-version",
                                {t!("about.version", version => about::version()).to_string()}
                            }
                            p { class: "about-copyright", {about::copyright()} }
                        }

                        // Decoration beside the name it repeats, so it is
                        // hidden from a screen reader rather than read out
                        // twice. The drawing and not the small PNG: this is
                        // shown far larger than that one has pixels for.
                        img {
                            class: "about-logo",
                            src: crate::LOGO_SVG,
                            alt: "",
                            aria_hidden: "true",
                        }
                    }

                    // The prose from `docs/about/`. Raw HTML in it reaches the
                    // page; why that is safe for these files and for nothing
                    // else is in `logic::about`.
                    div { class: "about-text", dangerous_inner_html: "{prose}" }
                }
            }

            footer { class: "bottom-bar",
                button {
                    onclick: move |_| {
                        // The navigator is taken here rather than at the top of
                        // the component, and that is not a detail. Asking for it
                        // while drawing makes the page undrawable outside a
                        // router — including in the tests below, which render it
                        // with no browser at all, and which is the tier that
                        // exists because five of 0003's nine defects were markup
                        // that needed something ambient before it became itself.
                        // Going back is something the button *does*, not
                        // something the page *is*.
                        let nav = navigator();

                        // Back to wherever the reader came from — but only if
                        // they came from inside Cantara at all. The page is an
                        // address of its own: it can be linked, bookmarked and
                        // shared, which is the point of having the source offer
                        // on it.
                        //
                        // The navigator's own `can_go_back` is not the question
                        // to ask. On the web it reports the *browser's* history,
                        // which holds whatever the tab was showing before
                        // Cantara — so a visitor who followed a link would be
                        // sent back out of the program. The door records that it
                        // was used instead; see `states::AboutEntryState`.
                        //
                        // Read here rather than while drawing, for the same
                        // reason as the navigator above: a page that needs a
                        // context to render is a page the markup tests cannot
                        // read.
                        let came_from_inside = try_consume_context::<AboutEntryState>()
                            .is_some_and(|mut entry| {
                                let inside = (entry.from_inside)();
                                // Spent: a later visit by link is a visit by
                                // link, whatever happened earlier in this run.
                                entry.from_inside.set(false);
                                inside
                            });

                        if came_from_inside && nav.can_go_back() {
                            nav.go_back();
                        } else {
                            nav.replace(crate::Route::SettingsPage {});
                        }
                    },
                    {t!("about.back").to_string()}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything the page exists to say has to be *in the markup*, with no
    /// browser involved.
    ///
    /// This is the tier `docs/specs/0004-testing-playwright.md` was written
    /// for: five of the nine defects it was written after were markup that
    /// needed a browser event before it became itself, and a page whose body
    /// arrives empty looks perfectly healthy from the outside.
    fn rendered() -> String {
        let mut dom = VirtualDom::new(AboutPage);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn the_page_names_the_program_and_its_version() {
        let html = rendered();

        assert!(html.contains("Cantara"), "got {html}");
        assert!(html.contains(about::version()), "no version: {html}");
    }

    /// The copyright is the reason half of this page exists, and it is
    /// computed rather than written down — so it is worth checking that what
    /// was computed actually arrived.
    #[test]
    fn the_page_carries_the_copyright() {
        assert!(rendered().contains(&about::copyright()));
    }

    /// The prose is Markdown converted to HTML and pushed in as raw markup.
    /// A conversion that silently produced nothing, or a language lookup that
    /// found nothing, would both leave a page that still has its headings.
    #[test]
    fn the_prose_is_rendered_as_markup_and_not_as_source() {
        let html = rendered();

        // A heading from the Markdown, as a heading.
        assert!(html.contains("<h2>"), "the prose was not converted: {html}");
        // And the licence, which is what a reader comes here for.
        assert!(html.contains("Affero"), "no licence named: {html}");
        // `##` surviving into the page would mean the source was shown raw.
        assert!(!html.contains("## "), "shown as source: {html}");
    }

    /// The logo is drawn, and it is drawn as decoration.
    ///
    /// It sits beside the name it repeats, so a screen reader that announced
    /// it would say "Cantara" twice; an `alt` of anything at all would be that
    /// mistake. The empty `alt` is the assertion here, not an omission.
    #[test]
    fn the_logo_is_there_and_is_not_read_out_twice() {
        let html = rendered();

        assert!(html.contains("about-logo"), "no logo: {html}");
        assert!(html.contains(".svg"), "not the drawing: {html}");
        assert!(html.contains(r#"aria-hidden="true""#), "got {html}");
    }

    /// The way out has to be drawn even before anything is clicked — a footer
    /// that appeared only after a mount would leave the page with no exit in
    /// exactly the rendering these tests read.
    #[test]
    fn the_page_has_a_way_out() {
        let html = rendered();

        assert!(html.contains("<button"), "got {html}");
        assert!(html.contains(&t!("about.back").to_string()), "got {html}");
    }
}
