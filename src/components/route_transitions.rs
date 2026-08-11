//! The transition between the routes of the application.
//!
//! A page change is a reaction to a click and should not be noticed as an
//! animation of its own, so the page that arrives fades in over a fraction of
//! a second: nothing slides, nothing scales, and the transition is over before
//! it can get in the way. The fade itself is a CSS animation on
//! [`RouteFadeLayout`]'s element — see `.route-fade` in `assets/main.css`.
//!
//! # Why not `dioxus-motion`'s animated outlet
//!
//! It used to render `AnimatedOutlet`, which interpolates the opacity of the
//! page being left and the page being entered while both are on screen. The
//! cost of showing two pages at once is that the tree under the outlet changes
//! shape three times per navigation: a plain outlet before, both pages side by
//! side and absolutely positioned during, and a plain outlet again after. The
//! arriving page is therefore built twice — once inside the transition and
//! once when it has settled — and the layout switches from the normal flow to
//! absolute positioning and back, which makes `sticky` and `fixed` elements
//! jump. Together that looked like the window reloading itself on every
//! navigation, and on a page that takes a moment to build, the second build
//! landed seconds after the first, throwing away where the reader had scrolled
//! to. (The same double build is what the comment about lost stylesheets in
//! [`crate::App`] is about.)
//!
//! Fading in the arriving page alone needs none of that: the page is built
//! once, stays in the normal flow, and the browser animates the opacity
//! without Rust being involved at all.

use crate::Route;
use dioxus::prelude::*;

/// The root layout of the route tree: renders the outlet, and lets the page
/// inside it fade in when the route changes.
///
/// The element is keyed by the *variant* of the route rather than by the route
/// itself, because that is what decides whether a different page is being
/// shown. A CSS animation restarts when its element is created, so a key that
/// changes with the page gives each page its fade — while a route that only
/// changes its parameters (the detail view puts the open element into the
/// address) keeps its element and is updated in place, without being thrown
/// away and rebuilt.
#[component]
pub fn RouteFadeLayout() -> Element {
    let route = use_route::<Route>();

    rsx! {
        div { key: "{page_key(&route)}", class: "route-fade",
            Outlet::<Route> {}
        }
    }
}

/// What tells one page from another for the purpose of the fade.
///
/// The variant, not the route: a detail view that is asked for a different
/// element is the same page showing something else, and rebuilding it would
/// throw away everything it holds.
fn page_key(route: &Route) -> String {
    format!("{:?}", std::mem::discriminant(route))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two different pages have to be two different elements, or the one
    /// arriving would inherit the other's finished animation and never fade.
    #[test]
    fn each_page_is_its_own_element() {
        assert_ne!(
            page_key(&Route::Selection {}),
            page_key(&Route::SettingsPage {})
        );
        assert_ne!(
            page_key(&Route::SettingsPage {}),
            page_key(&Route::PresentationDesignSettingsPage { index: 0 })
        );
    }

    /// Opening another element in the detail view is not a page change: the
    /// address carries the element, and the view keeps what it holds — which
    /// it cannot do if the element it lives in is replaced.
    #[test]
    fn opening_another_element_keeps_the_detail_view() {
        let one = Route::Detail {
            element: vec!["a3f9c2b1".to_string()],
        };
        let other = Route::Detail {
            element: vec!["7d10ee54".to_string()],
        };

        assert_eq!(page_key(&one), page_key(&other));
    }

    /// The same page asked for with different parameters stays one element,
    /// but a *different* page must not share its key by accident.
    #[test]
    fn a_design_and_a_slide_setting_are_different_pages() {
        assert_ne!(
            page_key(&Route::PresentationDesignSettingsPage { index: 0 }),
            page_key(&Route::SongSlideSettingsPage { index: 0 })
        );
        assert_eq!(
            page_key(&Route::PresentationDesignSettingsPage { index: 0 }),
            page_key(&Route::PresentationDesignSettingsPage { index: 3 })
        );
    }
}
