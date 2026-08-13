//! A list of places to jump to, beside a long view.
//!
//! Two views in Cantara are long enough that finding something in them means
//! scrolling and hoping: the settings, which run to half a dozen sections, and
//! the presenter console, whose service may be twenty elements. Both are
//! easier with a list of headings beside them that says where one is and takes
//! one somewhere else in a click.
//!
//! The component knows nothing about either. It is handed a list of targets,
//! which one is current, and what to do when one is chosen — so what a target
//! *means* stays with the view that has them. The settings jump to a section
//! of the page; the console jumps to the first slide of a chapter. Neither is
//! this module's business.
//!
//! # Where it appears
//!
//! Only where there is room: below `62rem` it is not drawn at all, because a
//! column of headings beside a narrow page leaves too little for the page. It
//! sticks to the top of its column while the view scrolls past, which is the
//! whole point of having it.
//!
//! Optionally it folds away, for a view where the reader may want the width
//! back. That is off by default: a list that can be lost is worth less than
//! one that is simply there, and only the caller knows which its view is.

use dioxus::prelude::*;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// The sidebar's own styles.
///
/// Carried by the component rather than added to `main.css`, so that a view
/// which uses it gets its appearance with it — including the presenter console,
/// which is a window of its own on the desktop.
const JUMP_SIDEBAR_CSS: Asset = asset!("/assets/jump_sidebar.css");

/// One place the sidebar can take the reader to.
#[derive(Clone, PartialEq, Debug)]
pub struct JumpTarget {
    /// What the reader is shown.
    pub label: String,

    /// Where it goes, in whatever terms the view uses.
    ///
    /// For a page this is the `id` of an element, which is what
    /// [`scroll_to_section`] needs. A view that jumps somewhere other than
    /// a place on the page — the console jumps to a slide — can leave it
    /// empty and use the position handed to `on_select` instead.
    pub id: String,
}

impl JumpTarget {
    /// A target that is a section of the page.
    pub fn section(id: impl Into<String>, label: impl Into<String>) -> JumpTarget {
        JumpTarget {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// The list of jump targets beside a long view.
///
/// `active` is the position in `targets` the reader is at, or `None` when that
/// cannot be said. `on_select` is given the position that was chosen — the
/// component does not act on it, because only the view knows what jumping
/// there means.
#[component]
pub fn JumpSidebar(
    targets: Vec<JumpTarget>,
    /// Which target the view is currently at.
    #[props(default)]
    active: Option<usize>,
    on_select: EventHandler<usize>,
    /// Whether the reader may fold the list away. Off by default.
    #[props(default)]
    collapsible: bool,
    /// The heading above the list. The generic one when not given.
    #[props(default)]
    title: Option<String>,
) -> Element {
    let mut collapsed = use_signal(|| false);

    // Nothing to jump to is not a short list, it is no list: a heading with
    // nothing under it would only take width away from the view.
    if targets.is_empty() {
        return rsx! {};
    }

    let heading = title.unwrap_or_else(|| t!("general.jump_to").to_string());

    rsx! {
        document::Stylesheet { href: JUMP_SIDEBAR_CSS }

        // An `aside` carrying the navigation role rather than a `nav`. Pico
        // styles a `nav`'s list as a row of inline items — which is what a
        // navigation bar is, and the opposite of what this is. Winning that on
        // specificity would work until the next rule; not provoking it is the
        // better answer.
        aside {
            class: if collapsed() { "jump-sidebar collapsed" } else { "jump-sidebar" },
            role: "navigation",
            aria_label: heading.clone(),

            div { class: "jump-sidebar-head",
                if !collapsed() {
                    span { class: "jump-sidebar-title", "{heading}" }
                }
                if collapsible {
                    button {
                        r#type: "button",
                        class: "outline secondary jump-sidebar-toggle",
                        aria_expanded: (!collapsed()).to_string(),
                        // The button says «, which is no name at all. `title`
                        // shows a tooltip but is not dependably read as the
                        // name either, so the words are stated outright — a
                        // screen reader would otherwise announce the glyph.
                        aria_label: match collapsed() {
                            true => t!("general.jump_expand").to_string(),
                            false => t!("general.jump_collapse").to_string(),
                        },
                        title: match collapsed() {
                            true => t!("general.jump_expand").to_string(),
                            false => t!("general.jump_collapse").to_string(),
                        },
                        onclick: move |_| {
                            let folded = collapsed();
                            collapsed.set(!folded);
                        },
                        if collapsed() { "»" } else { "«" }
                    }
                }
            }

            if !collapsed() {
                ul { class: "jump-sidebar-list",
                    for (index , target) in targets.iter().enumerate() {
                        li { key: "{target.id}-{index}",
                            button {
                                r#type: "button",
                                class: match active == Some(index) {
                                    true => "jump-sidebar-entry active",
                                    false => "jump-sidebar-entry",
                                },
                                // Says which one is current to a screen reader,
                                // which cannot see that it is marked.
                                aria_current: match active == Some(index) {
                                    true => "true",
                                    false => "false",
                                },
                                onclick: move |_| on_select.call(index),
                                "{target.label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Scrolls the page to the section with this `id`.
///
/// For the views whose targets are places on the page. A view that jumps
/// somewhere else does not call this.
pub fn scroll_to_section(id: &str) {
    // `block: "start"` rather than the default, so the heading lands at the top
    // of the scrolling area and the section is read from its beginning.
    let _ = document::eval(&format!(
        r#"const target = document.getElementById("{id}");
           if (target) {{ target.scrollIntoView({{ behavior: "smooth", block: "start" }}); }}"#
    ));
}

/// Follows which section is on screen, so the sidebar can mark it.
///
/// Uses an `IntersectionObserver` and not a scroll handler, deliberately. A
/// handler runs on every scroll event and does its work on the thread that is
/// trying to scroll; the observer is told once what to watch for and reports
/// only when a section actually comes or goes. On a page that already has
/// trouble scrolling smoothly, the difference is the point.
///
/// Returns the position of the section nearest the top of the view.
pub fn use_section_spy(ids: Vec<String>) -> Signal<Option<usize>> {
    let mut active: Signal<Option<usize>> = use_signal(|| None);

    // The list arrives as a prop and cannot be watched by an effect, so it is
    // mirrored into a signal first — the same move the previews make.
    let mut watched: Signal<Vec<String>> = use_signal(|| ids.clone());
    if *watched.peek() != ids {
        watched.set(ids);
    }

    use_effect(move || {
        let ids = watched();
        if ids.is_empty() {
            return;
        }

        let list = ids
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<String>>()
            .join(",");

        spawn(async move {
            let mut script = document::eval(&format!(
                r#"const ids = [{list}];
                   const seen = new Map();
                   let reported = -1;

                   // The element that actually scrolls. The page sits inside
                   // one rather than scrolling the document itself, and the
                   // end-of-scroll rule below needs to ask the right box how
                   // far it has left to go.
                   function scroller() {{
                       let node = document.getElementById(ids[0]);
                       while (node && node !== document.body) {{
                           const style = getComputedStyle(node);
                           if (/(auto|scroll)/.test(style.overflowY)) {{ return node; }}
                           node = node.parentElement;
                       }}
                       return document.scrollingElement || document.body;
                   }}

                   function report() {{
                       let current = -1;

                       // The last section can never reach the top of the view:
                       // there is nothing under it to scroll up. Once the box
                       // is at its end, the section being read is the last one
                       // on screen — otherwise clicking the bottom entry marks
                       // whichever section happens to sit above it.
                       const box = scroller();
                       const atEnd =
                           box.scrollTop + box.clientHeight >= box.scrollHeight - 4;

                       if (atEnd) {{
                           for (let index = ids.length - 1; index >= 0; index -= 1) {{
                               if (seen.get(ids[index])) {{ current = index; break; }}
                           }}
                       }} else {{
                           // Otherwise the one nearest the top of the list that
                           // is on screen. Reading downwards means the heading
                           // one has just scrolled under stays marked.
                           for (let index = 0; index < ids.length; index += 1) {{
                               if (seen.get(ids[index])) {{ current = index; break; }}
                           }}
                       }}

                       if (current !== -1 && current !== reported) {{
                           reported = current;
                           dioxus.send(current);
                       }}
                   }}

                   const observer = new IntersectionObserver(function (entries) {{
                       for (const entry of entries) {{
                           seen.set(entry.target.id, entry.intersectionRatio > 0);
                       }}
                       report();
                   }}, {{
                       // A section counts as current once its top third is in
                       // view, so the mark moves with the reading and not with
                       // the very last pixel of the section before it.
                       rootMargin: "0px 0px -66% 0px",
                   }});

                   for (const id of ids) {{
                       const element = document.getElementById(id);
                       if (element) {{ observer.observe(element); }}
                   }}

                   // Asked again once the scrolling has come to rest. The
                   // observer only speaks when a section crosses the edge of
                   // the view, and the last stretch to the very bottom crosses
                   // nothing — which is exactly where the rule above matters.
                   // `scrollend` fires once when the movement stops, so this
                   // costs nothing per frame.
                   scroller().addEventListener("scrollend", report);
                   "#
            ));

            // The observer keeps reporting for as long as the page is there.
            while let Ok(index) = script.recv::<i64>().await {
                if index >= 0 {
                    active.set(Some(index as usize));
                }
            }
        });
    });

    active
}
