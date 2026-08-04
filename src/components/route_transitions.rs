//! The transition between the routes of the application.
//!
//! The whole route tree is wrapped in [`AnimatedLayout`], which renders
//! `dioxus-motion`'s [`AnimatedOutlet`] instead of the plain `Outlet`. While a
//! navigation is in flight, that outlet renders the old *and* the new page at
//! the same time and interpolates the opacity of both of them. The
//! corresponding CSS lives in `assets/main.css` (`.route-container` /
//! `.route-content`); without it the two pages would stack vertically instead
//! of on top of each other.
//!
//! Every route uses the same effect — a short cross-fade. A page change is a
//! reaction to a click and should not be noticed as an animation of its own, so
//! there is deliberately no movement and no direction: nothing slides, nothing
//! scales, and the transition is over before it can get in the way.

use crate::Route;
use dioxus::prelude::*;
use dioxus_motion::prelude::*;

/// How long a page change takes.
///
/// Short enough to stay below the threshold where a transition begins to feel
/// like waiting, but long enough for the eye to register the change as a fade
/// rather than as a jump.
const FADE_DURATION_MS: u64 = 140;

/// The root layout of the route tree: renders the animated outlet and supplies
/// the timing used by it.
///
/// This has to be a layout rather than something in `App`, because
/// [`AnimatedOutlet`] reads the router's outlet context and therefore has to
/// live *inside* the router.
#[component]
pub fn AnimatedLayout() -> Element {
    // A tween in the context takes precedence over `dioxus-motion`'s default
    // spring. The spring would overshoot and settle, which costs time and draws
    // attention; a linear fade of a fixed length does not.
    use_context_provider(|| Store::new(Tween::new(Duration::from_millis(FADE_DURATION_MS))));

    rsx! {
        AnimatedOutlet::<Route> {}
    }
}
