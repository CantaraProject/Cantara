//! This module provides monitor/screen enumeration for multi-screen presentation support.

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

    // If a specific monitor is configured, try to find it
    if let Some(name) = configured_name
        && let Some(monitor) = monitors.iter().find(|m| &m.name == name) {
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
            taken.push(monitor.name.clone());
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
            .filter(|monitor| !taken.contains(&monitor.name))
            // The same preference the projection has always had: the screen
            // that is not the one Cantara is being operated on.
            .find(|monitor| !monitor.is_primary)
            .or_else(|| {
                monitors
                    .iter()
                    .find(|monitor| !taken.contains(&monitor.name))
            })
            .cloned();

        let monitor = free.or_else(|| resolve_monitor(monitors, &None, false));
        if let Some(ref monitor) = monitor {
            taken.push(monitor.name.clone());
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
}
