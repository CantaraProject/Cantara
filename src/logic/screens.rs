//! This module provides monitor/screen enumeration for multi-screen presentation support.
//!
//! Finding out what screens there are needs a window system, so
//! [`enumerate_monitors`] is a desktop build's alone. Everything else here —
//! what a screen is called for storage, which screen a view resolves to — is
//! arithmetic over a list of [`MonitorInfo`] and is compiled wherever the
//! `screens` feature is, which includes the browser build the Playwright
//! tests drive. That build gets its list from [`harness_monitors`].

#[cfg(feature = "desktop")]
use dioxus::desktop::DesktopContext;
use serde::{Deserialize, Serialize};

/// Information about a connected monitor/screen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorInfo {
    /// Index of the monitor in the enumeration order
    pub id: usize,
    /// Human-readable name of the monitor (may be empty on some platforms)
    pub name: String,
    /// Position of the monitor in virtual screen coordinates
    pub position: (i32, i32),
    /// Size of the monitor in physical pixels
    pub size: (u32, u32),
    /// Whether this is the primary monitor
    pub is_primary: bool,
}

/// Enumerates all available monitors using the desktop context.
#[cfg(feature = "desktop")]
pub fn enumerate_monitors(desktop: &DesktopContext) -> Vec<MonitorInfo> {
    let primary = desktop.primary_monitor();
    let monitors: Vec<_> = desktop.available_monitors().collect();

    monitors
        .into_iter()
        .enumerate()
        .map(|(id, monitor)| {
            let name = monitor.name().unwrap_or_default();
            let position = monitor.position();
            let size = monitor.size();
            let is_primary = primary
                .as_ref()
                .map(|p| p.name() == monitor.name() && p.position() == monitor.position())
                .unwrap_or(false);

            MonitorInfo {
                id,
                name,
                position: (position.x, position.y),
                size: (size.width, size.height),
                is_primary,
            }
        })
        .collect()
}

/// The screens a browser test is given, because a browser has none.
///
/// The map is a picture drawn by a page and set by a pointer, and neither of
/// those can be checked without a browser — but a browser build enumerates no
/// screens, so there has never been anything for a browser test to point at.
/// This is the way in, and it is behind `test-harness` for the same reason
/// [`crate::logic::harness`] is: a released build should not contain it.
///
/// Three screens by default, because two is not enough to tell the arithmetic
/// apart from a coincidence: a primary laptop, a beamer to its right in a
/// different resolution, and a stage monitor above the laptop. That covers a
/// screen that is not the operator's, a negative origin, and two screens that
/// must be drawn at different sizes.
///
/// A test that needs a different arrangement writes one to local storage under
/// [`HARNESS_MONITORS_KEY`], the same way the reorder tests seed the settings.
/// That is the real way in and out of a browser build; there is no control
/// port here and nothing a test could get right that the program gets wrong.
#[cfg(all(feature = "screens", not(feature = "desktop")))]
pub fn harness_monitors() -> Vec<MonitorInfo> {
    // Local storage is a browser's. A native build without `desktop` — a phone
    // build with the harness switched on — has neither it nor screens, and is
    // given the default arrangement rather than failing to compile.
    #[cfg(target_arch = "wasm32")]
    let seeded: Option<Vec<MonitorInfo>> = crate::logic::web_storage::read(HARNESS_MONITORS_KEY);
    #[cfg(not(target_arch = "wasm32"))]
    let seeded: Option<Vec<MonitorInfo>> = None;

    seeded.unwrap_or_else(|| {
        vec![
            MonitorInfo {
                id: 0,
                name: "Laptop".to_string(),
                position: (0, 0),
                size: (1920, 1080),
                is_primary: true,
            },
            MonitorInfo {
                id: 1,
                name: "Beamer".to_string(),
                position: (1920, 0),
                size: (1280, 800),
                is_primary: false,
            },
            MonitorInfo {
                id: 2,
                name: "Bühne".to_string(),
                position: (0, -1080),
                size: (1920, 1080),
                is_primary: false,
            },
        ]
    })
}

/// Where a browser test puts the screens it wants the map drawn from.
#[cfg(all(feature = "screens", not(feature = "desktop")))]
pub const HARNESS_MONITORS_KEY: &str = "cantara-harness-monitors";

/// A name for a screen that is actually its own.
///
/// `MonitorInfo::name` is what the platform reports, and on Windows and
/// Wayland it is frequently empty and frequently the same for two screens —
/// two identical monitors from the same maker report the same string, or none.
/// A view pointed at "" would then match the first screen, or every screen,
/// and two views asking for different monitors would be given the same one.
///
/// So a screen is identified by its name *and* its place in the enumeration.
/// The name is kept in front because it is what a person recognises, and the
/// position disambiguates rather than replacing it.
pub fn screen_key(monitor: &MonitorInfo) -> String {
    format!("{}#{}", monitor.name, monitor.id)
}

/// The monitor a stored key names, if it is still there.
///
/// Matched on the whole key first. A screen that has moved in the enumeration
/// — one unplugged from in front of it — is then looked for by name alone,
/// because a name that is not empty is still the better evidence of which
/// screen a person meant.
fn monitor_by_key<'a>(monitors: &'a [MonitorInfo], key: &str) -> Option<&'a MonitorInfo> {
    if let Some(exact) = monitors.iter().find(|monitor| screen_key(monitor) == key) {
        return Some(exact);
    }

    let name = key.rsplit_once('#').map(|(name, _)| name).unwrap_or(key);
    if name.is_empty() {
        return None;
    }
    monitors.iter().find(|monitor| monitor.name == name)
}

/// Turns a stored screen name from before [`screen_key`] into a key.
///
/// The settings page used to write `monitor.name` where everything else
/// expects a key. It worked through the name fallback in [`monitor_by_key`],
/// right up until two screens report the same name or none — which is exactly
/// what `screen_key` exists for, and exactly what a hall with two identical
/// monitors has. See decision D2 of `docs/specs/0007-screen-map.md`.
///
/// A value with a `#` in it is already a key and is left alone. A bare name
/// that matches a screen that is here becomes that screen's key. A bare name
/// that matches nothing is left alone rather than cleared: the screen is
/// probably just unplugged, and the fallback still finds it when it comes
/// back. Clearing it would quietly move the projection to the laptop and keep
/// it there.
pub fn upgrade_screen_key(stored: &Option<String>, monitors: &[MonitorInfo]) -> Option<String> {
    let name = stored.as_ref()?;
    if name.contains('#') || name.is_empty() {
        return stored.clone();
    }
    monitors
        .iter()
        .find(|monitor| monitor.name == *name)
        .map(screen_key)
        .or_else(|| stored.clone())
}

/// Resolves which monitor to use for presentation based on settings.
/// If `configured_name` is Some, tries to find a monitor with that name.
/// Otherwise, prefers a non-primary monitor (for presentation) or primary monitor (for presenter console).
pub fn resolve_monitor(
    monitors: &[MonitorInfo],
    configured_name: &Option<String>,
    prefer_primary: bool,
) -> Option<MonitorInfo> {
    if monitors.is_empty() {
        return None;
    }

    // If a specific monitor is configured, try to find it. By its whole key,
    // not its name: a name is often empty and often shared — see
    // [`screen_key`].
    if let Some(key) = configured_name
        && let Some(monitor) = monitor_by_key(monitors, key)
    {
        return Some(monitor.clone());
    }

    // Auto-select: prefer primary or non-primary based on the flag
    if prefer_primary {
        monitors
            .iter()
            .find(|m| m.is_primary)
            .or(monitors.first())
            .cloned()
    } else {
        monitors
            .iter()
            .find(|m| !m.is_primary)
            .or(monitors.first())
            .cloned()
    }
}

/// A view that is to be given a window, and the screen it goes on.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedView {
    /// Which of [`crate::logic::settings::Settings::views`] this is.
    ///
    /// The position rather than the view itself, so that whoever opens the
    /// window can read the view back out of the settings it belongs to
    /// without a second copy going stale.
    pub index: usize,

    /// The monitor it lands on, or `None` when there is no monitor to be had
    /// — in which case the window is opened as an ordinary one, which is what
    /// a single-screen machine has always done.
    pub monitor: Option<MonitorInfo>,
}

/// Every enabled view that wants a screen, with the monitor each one gets.
///
/// Views that name a monitor get that one, by
/// [`resolve_monitor`] — the same answer the projection has always had,
/// including its fallbacks for a screen that has since been unplugged.
///
/// Views that name none are dealt with afterwards, and take a screen no other
/// view has already claimed. This matters as soon as there is more than one:
/// two views both asking for "wherever is free" would otherwise both resolve
/// to the first non-primary monitor, and the second window would open exactly
/// on top of the first. One of the two would be invisible, with nothing on
/// screen to say why — and the operator would be looking for a window that is
/// there, in the right place, underneath another one.
///
/// Named views are placed first for the same reason: a view that asked for a
/// particular screen has said something definite, and should not lose it to a
/// view that said nothing. Where there are more views than screens the
/// leftovers fall back to [`resolve_monitor`]'s answer and do overlap — there
/// is nowhere else to put them, and a window on top of another is better than
/// no window at all.
pub fn place_screen_views(
    views: &[crate::logic::settings::View],
    monitors: &[MonitorInfo],
) -> Vec<PlacedView> {
    use crate::logic::settings::ViewOutput;

    let wanting_a_screen: Vec<(usize, &Option<String>)> = views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.enabled)
        .filter_map(|(index, view)| match &view.output {
            ViewOutput::Screen { monitor_name } => Some((index, monitor_name)),
            ViewOutput::Network { .. } => None,
        })
        .collect();

    let mut placed: Vec<PlacedView> = Vec::with_capacity(wanting_a_screen.len());
    let mut taken: Vec<String> = Vec::new();

    for (index, monitor_name) in wanting_a_screen
        .iter()
        .filter(|(_, monitor_name)| monitor_name.is_some())
    {
        let monitor = resolve_monitor(monitors, monitor_name, false);
        if let Some(ref monitor) = monitor {
            taken.push(screen_key(monitor));
        }
        placed.push(PlacedView {
            index: *index,
            monitor,
        });
    }

    for (index, _) in wanting_a_screen
        .iter()
        .filter(|(_, monitor_name)| monitor_name.is_none())
    {
        let free = monitors
            .iter()
            .filter(|monitor| !taken.contains(&screen_key(monitor)))
            // The same preference the projection has always had: the screen
            // that is not the one Cantara is being operated on.
            .find(|monitor| !monitor.is_primary)
            .or_else(|| {
                monitors
                    .iter()
                    .find(|monitor| !taken.contains(&screen_key(monitor)))
            })
            .cloned();

        let monitor = free.or_else(|| resolve_monitor(monitors, &None, false));
        if let Some(ref monitor) = monitor {
            taken.push(screen_key(monitor));
        }
        placed.push(PlacedView {
            index: *index,
            monitor,
        });
    }

    // Back into the order the views are in, so that what opens first is what
    // the user has first in their list rather than an artefact of naming a
    // screen.
    placed.sort_by_key(|placement| placement.index);
    placed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::settings::{View, ViewFocus, ViewOutput};

    fn monitor(id: usize, name: &str, is_primary: bool) -> MonitorInfo {
        MonitorInfo {
            id,
            name: name.to_string(),
            position: (id as i32 * 1920, 0),
            size: (1920, 1080),
            is_primary,
        }
    }

    fn screen_view(name: &str, monitor_name: Option<&str>, enabled: bool) -> View {
        View {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            design_index: None,
            slide_settings_index: None,
            output: ViewOutput::Screen {
                monitor_name: monitor_name.map(str::to_string),
            },
            enabled,
            focus: ViewFocus::Follow,
        }
    }

    /// The ordinary configuration, and the one every existing installation
    /// migrates to: one projection, no screen named, two monitors. It goes on
    /// the one that is not the operator's.
    #[test]
    fn the_projection_lands_on_the_screen_that_is_not_the_operators() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let views = vec![screen_view("Projection", None, true)];

        let placed = place_screen_views(&views, &monitors);

        assert_eq!(placed.len(), 1);
        assert_eq!(
            placed[0].monitor.as_ref().map(|monitor| monitor.name.as_str()),
            Some("Beamer")
        );
    }

    /// A view that names a screen gets that screen.
    #[test]
    fn a_view_that_names_a_screen_is_given_it() {
        let monitors = vec![
            monitor(0, "Laptop", true),
            monitor(1, "Beamer", false),
            monitor(2, "Bühne", false),
        ];
        let views = vec![screen_view("Stage", Some("Bühne"), true)];

        let placed = place_screen_views(&views, &monitors);

        assert_eq!(
            placed[0].monitor.as_ref().map(|monitor| monitor.name.as_str()),
            Some("Bühne")
        );
    }

    /// The failure this function exists to prevent: two views that both say
    /// "wherever is free" must not both be put on the same screen, where one
    /// window sits invisibly underneath the other.
    #[test]
    fn two_views_that_name_no_screen_do_not_land_on_the_same_one() {
        let monitors = vec![
            monitor(0, "Laptop", true),
            monitor(1, "Beamer", false),
            monitor(2, "Bühne", false),
        ];
        let views = vec![
            screen_view("Projection", None, true),
            screen_view("Stage", None, true),
        ];

        let placed = place_screen_views(&views, &monitors);

        let names: Vec<Option<&str>> = placed
            .iter()
            .map(|placement| placement.monitor.as_ref().map(|monitor| monitor.name.as_str()))
            .collect();
        assert_eq!(names, vec![Some("Beamer"), Some("Bühne")]);
    }

    /// A view that asked for a particular screen keeps it, even though the
    /// view that asked for nothing comes first in the list and would
    /// otherwise have taken it.
    #[test]
    fn naming_a_screen_beats_not_naming_one() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let views = vec![
            screen_view("Says nothing", None, true),
            screen_view("Wants the beamer", Some("Beamer"), true),
        ];

        let placed = place_screen_views(&views, &monitors);

        assert_eq!(
            placed[1].monitor.as_ref().map(|monitor| monitor.name.as_str()),
            Some("Beamer"),
            "the view that asked for the beamer did not get it"
        );
        assert_ne!(
            placed[0].monitor.as_ref().map(|monitor| monitor.name.as_str()),
            Some("Beamer"),
            "and the one that asked for nothing took it anyway"
        );
    }

    /// The result is in the order the views are, whatever order they were
    /// placed in. What opens first should be what the user put first.
    #[test]
    fn the_placements_come_back_in_the_order_the_views_are_in() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let views = vec![
            screen_view("Says nothing", None, true),
            screen_view("Wants the beamer", Some("Beamer"), true),
        ];

        let placed = place_screen_views(&views, &monitors);

        assert_eq!(
            placed.iter().map(|placement| placement.index).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    /// A view that is switched off is not opened, and a network view has no
    /// window to open.
    #[test]
    fn only_enabled_views_that_want_a_screen_are_placed() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let views = vec![
            screen_view("Off", None, false),
            View {
                id: uuid::Uuid::new_v4(),
                name: "Stream".to_string(),
                design_index: None,
                slide_settings_index: None,
                output: ViewOutput::Network {
                    path: "/".to_string(),
                },
                enabled: true,
                focus: ViewFocus::Follow,
            },
            screen_view("On", None, true),
        ];

        let placed = place_screen_views(&views, &monitors);

        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].index, 2);
    }

    /// Two screens that report no name at all are still two screens.
    ///
    /// Windows and Wayland frequently report an empty name, and two identical
    /// monitors report the same one. Matched by name, every such screen is the
    /// same screen: two views asking for different monitors would be given
    /// one, and the second window would open on top of the first.
    #[test]
    fn screens_with_no_name_are_told_apart() {
        let monitors = vec![
            monitor(0, "", true),
            monitor(1, "", false),
            monitor(2, "", false),
        ];
        let views = vec![
            screen_view("One", Some(&screen_key(&monitors[1])), true),
            screen_view("Two", Some(&screen_key(&monitors[2])), true),
        ];

        let placed = place_screen_views(&views, &monitors);

        assert_eq!(
            placed[0].monitor.as_ref().map(|monitor| monitor.id),
            Some(1)
        );
        assert_eq!(
            placed[1].monitor.as_ref().map(|monitor| monitor.id),
            Some(2),
            "two nameless screens were treated as one"
        );
    }

    /// The same for two screens that report the *same* name — a pair of
    /// identical monitors, which is what a hall usually has.
    #[test]
    fn screens_that_share_a_name_are_told_apart() {
        let monitors = vec![
            monitor(0, "Laptop", true),
            monitor(1, "BenQ", false),
            monitor(2, "BenQ", false),
        ];
        let views = vec![
            screen_view("One", Some(&screen_key(&monitors[1])), true),
            screen_view("Two", Some(&screen_key(&monitors[2])), true),
        ];

        let placed = place_screen_views(&views, &monitors);

        assert_ne!(
            placed[0].monitor.as_ref().map(|monitor| monitor.id),
            placed[1].monitor.as_ref().map(|monitor| monitor.id),
            "two screens of the same name were treated as one"
        );
    }

    /// A screen that has moved in the enumeration — one unplugged from in
    /// front of it — is still found by its name, which is the better evidence
    /// of which screen a person meant.
    #[test]
    fn a_named_screen_is_found_again_after_it_has_moved() {
        let before = monitor(2, "Beamer", false);
        let after = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];

        let found = resolve_monitor(&after, &Some(screen_key(&before)), false);

        assert_eq!(found.map(|monitor| monitor.id), Some(1));
    }

    /// But an *unnamed* screen that has moved is not guessed at: there is
    /// nothing to recognise it by, and picking one at random would put the
    /// presentation on a screen nobody chose.
    #[test]
    fn an_unnamed_screen_that_has_moved_is_not_guessed_at() {
        let before = monitor(3, "", false);
        let after = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];

        let found = resolve_monitor(&after, &Some(screen_key(&before)), false);

        // Falls back the way an unplugged screen always has: to a screen that
        // exists, rather than to nothing at all.
        assert!(found.is_some());
        assert_eq!(found.map(|monitor| monitor.name), Some("Beamer".to_string()));
    }

    /// A machine with one screen still shows the presentation — on the screen
    /// it has. This is the laptop on the kitchen table, and it must not be a
    /// configuration that opens no window.
    #[test]
    fn one_screen_is_still_a_screen() {
        let monitors = vec![monitor(0, "Laptop", true)];
        let views = vec![screen_view("Projection", None, true)];

        let placed = place_screen_views(&views, &monitors);

        assert_eq!(
            placed[0].monitor.as_ref().map(|monitor| monitor.name.as_str()),
            Some("Laptop")
        );
    }

    /// More views than screens: the leftovers overlap rather than vanishing.
    /// There is nowhere else to put them, and a window on top of another is
    /// something the operator can move.
    #[test]
    fn more_views_than_screens_still_all_get_a_window() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let views = vec![
            screen_view("One", None, true),
            screen_view("Two", None, true),
            screen_view("Three", None, true),
        ];

        let placed = place_screen_views(&views, &monitors);

        assert_eq!(placed.len(), 3);
        assert!(
            placed.iter().all(|placement| placement.monitor.is_some()),
            "a view was left with no screen at all"
        );
    }

    /// No monitors could be enumerated at all. The view is still placed, with
    /// no screen, and the window is opened as an ordinary one.
    #[test]
    fn a_view_is_still_placed_when_there_are_no_monitors() {
        let views = vec![screen_view("Projection", None, true)];

        let placed = place_screen_views(&views, &[]);

        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].monitor, None);
    }

    /// A bare name written by the old settings page becomes the key
    /// everything else expects.
    #[test]
    fn a_stored_name_is_upgraded_to_a_key() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];

        let upgraded = upgrade_screen_key(&Some("Beamer".to_string()), &monitors);

        assert_eq!(upgraded, Some(screen_key(&monitors[1])));
    }

    /// A value that is already a key is left exactly as it is, so that the
    /// migration can run on every load without doing anything the second
    /// time.
    #[test]
    fn a_key_survives_the_upgrade_unchanged() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let key = screen_key(&monitors[1]);

        assert_eq!(upgrade_screen_key(&Some(key.clone()), &monitors), Some(key));
    }

    /// A name matching no screen that is plugged in right now is kept, not
    /// cleared. The beamer is probably in the cupboard, and clearing it would
    /// move the projection to the laptop for good.
    #[test]
    fn a_name_whose_screen_is_absent_is_kept() {
        let monitors = vec![monitor(0, "Laptop", true)];

        let upgraded = upgrade_screen_key(&Some("Beamer".to_string()), &monitors);

        assert_eq!(upgraded, Some("Beamer".to_string()));
    }

    /// "Automatic" stays automatic.
    #[test]
    fn nothing_stored_upgrades_to_nothing() {
        let monitors = vec![monitor(0, "Laptop", true)];

        assert_eq!(upgrade_screen_key(&None, &monitors), None);
    }

    /// A name that was upgraded resolves to the screen it names, which is the
    /// whole point of doing it.
    #[test]
    fn an_upgraded_key_resolves_to_the_right_screen_among_namesakes() {
        let monitors = vec![
            monitor(0, "Laptop", true),
            monitor(1, "BenQ", false),
            monitor(2, "BenQ", false),
        ];

        let upgraded = upgrade_screen_key(&Some("BenQ".to_string()), &monitors);
        let resolved = resolve_monitor(&monitors, &upgraded, false);

        assert_eq!(
            resolved.map(|monitor| monitor.id),
            Some(1),
            "the upgrade did not pin the choice to one of the two namesakes"
        );
    }
}
