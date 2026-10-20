//! The windows a running service has open, and moving them to another screen.
//!
//! Decision D5 of `docs/specs/0007-screen-map.md`. Dropping a window onto a
//! screen in the settings does **not** move it: a change takes effect the next
//! time the presentation starts, so that tidying up mid-service does not black
//! out the projection in front of the congregation. But an operator who has
//! just decided they want the beamer on the other output should not have to
//! stop the service to get it, so there is one explicit control — "apply now"
//! — and this is what it calls.
//!
//! # Why a registry, and why weak
//!
//! A window opened by `start_presentation` used to be forgotten the moment it
//! existed: [`dioxus::desktop::DesktopContext::new_window`]'s answer was
//! dropped. Nothing needed it, because nothing could change a window after it
//! was opened. Moving one needs it back, so each window is remembered here as
//! it resolves, together with the screen it was actually put on — which is
//! what makes "this chip has been moved since the service started" a question
//! with an answer.
//!
//! Each is held **weakly**. A presentation window closes itself (see
//! `PresentationPage`, which calls `window().close()` when the service ends),
//! and a strong handle here would keep the whole webview alive after that and
//! hand out a window that is not on screen any more. A `Weak` that no longer
//! upgrades is exactly the fact "that window is gone", so the registry needs
//! no teardown path and cannot disagree with reality.
//!
//! # What is here and what is not
//!
//! The arithmetic — what each window *should* be on, and which ones are
//! therefore out of date — is [`wanted_screens`] and [`pending`], which are
//! plain functions over plain data and are tested. Only [`move_to`] touches a
//! window, and only a desktop build has one to touch.

use crate::logic::screens::{MonitorInfo, place_screen_views, resolve_monitor, screen_key};
use crate::logic::settings::Settings;

/// Which window of a running service this is.
///
/// The main window is not here. Where it sits is the window manager's
/// business, which is decision D3 and the reason its chip cannot be dragged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowTarget {
    /// One of [`Settings::views`], by its position — the same number
    /// [`crate::components::screen_map::ChipKind::View`] carries.
    View(usize),
    /// The presenter console, when it has a window of its own.
    PresenterConsole,
}

/// A window and the screen it is on, as a [`screen_key`]. `None` is a window
/// that was opened without a screen to go on — there were none to report.
pub type WindowScreen = (WindowTarget, Option<String>);

/// Which screen each window of a running service would be opened on if the
/// service started now.
///
/// The same two rules the opening itself uses — [`place_screen_views`] for the
/// views and [`resolve_monitor`] for the console — rather than a third reading
/// of the settings that could come to differ from them.
pub fn wanted_screens(settings: &Settings, monitors: &[MonitorInfo]) -> Vec<WindowScreen> {
    let mut wanted: Vec<WindowScreen> = place_screen_views(&settings.views, monitors)
        .iter()
        .map(|placement| {
            (
                WindowTarget::View(placement.index),
                placement.monitor.as_ref().map(screen_key),
            )
        })
        .collect();

    if settings.show_presenter_console && !settings.presenter_console_in_main_window {
        wanted.push((
            WindowTarget::PresenterConsole,
            resolve_monitor(monitors, &settings.presenter_screen, true)
                .as_ref()
                .map(screen_key),
        ));
    }

    wanted
}

/// The open windows that are not on the screen the settings now name.
///
/// A window nobody has an opinion about any more — a view that has since been
/// switched off, say — is not pending. "Apply now" moves the windows that
/// exist to where they should be; opening and closing windows during a service
/// is what 0003 left undone and is still not in scope.
pub fn pending(open: &[WindowScreen], wanted: &[WindowScreen]) -> Vec<WindowTarget> {
    open.iter()
        .filter(|(target, on)| {
            wanted
                .iter()
                .any(|(other, should_be)| other == target && should_be != on)
        })
        .map(|(target, _)| *target)
        .collect()
}

#[cfg(feature = "desktop")]
mod live {
    use super::{WindowScreen, WindowTarget};
    use crate::logic::screens::{MonitorInfo, screen_key};
    use dioxus::desktop::tao::dpi::{PhysicalPosition, PhysicalSize};
    use dioxus::desktop::tao::monitor::MonitorHandle;
    use dioxus::desktop::tao::window::{Fullscreen, Window};
    use dioxus::desktop::{DesktopContext, WeakDesktopContext};
    use std::cell::RefCell;

    struct LiveWindow {
        target: WindowTarget,
        /// Weak on purpose — see the module's own documentation.
        window: WeakDesktopContext,
        on: Option<String>,
    }

    thread_local! {
        /// Every window the running service has open.
        ///
        /// Thread-local rather than a signal: windows are a property of the
        /// one `tao` event loop, every one of them is created and moved on its
        /// thread, and each window is a `VirtualDom` of its own that inherits
        /// no signal from the window that opened it.
        static OPEN: RefCell<Vec<LiveWindow>> = const { RefCell::new(Vec::new()) };
    }

    /// Remembers a window that has just opened, and the screen it was put on.
    pub fn remember(target: WindowTarget, window: &DesktopContext, on: Option<String>) {
        OPEN.with_borrow_mut(|open| {
            open.retain(|live| live.window.strong_count() > 0 && live.target != target);
            open.push(LiveWindow {
                target,
                window: std::rc::Rc::downgrade(window),
                on,
            });
        });
    }

    /// What the running service has open, and where each of them is.
    pub fn open_windows() -> Vec<WindowScreen> {
        OPEN.with_borrow_mut(|open| {
            open.retain(|live| live.window.strong_count() > 0);
            open.iter()
                .map(|live| (live.target, live.on.clone()))
                .collect()
        })
    }

    /// Moves every open window onto the screen it should be on, and remembers
    /// where it now is.
    ///
    /// Windows that are already right are left alone: moving one is a flash of
    /// black, and there is no reason to spend it on a window that is not
    /// changing screens.
    pub fn move_to(monitors: &[MonitorInfo], wanted: &[WindowScreen]) {
        OPEN.with_borrow_mut(|open| {
            open.retain(|live| live.window.strong_count() > 0);

            for live in open.iter_mut() {
                let Some((_, should_be)) = wanted.iter().find(|(target, _)| *target == live.target)
                else {
                    continue;
                };
                if *should_be == live.on {
                    continue;
                }
                let Some(window) = live.window.upgrade() else {
                    continue;
                };
                let monitor = should_be.as_ref().and_then(|key| {
                    monitors
                        .iter()
                        .find(|monitor| screen_key(monitor) == *key)
                });
                let Some(monitor) = monitor else {
                    continue;
                };

                match live.target {
                    WindowTarget::View(_) => put_fullscreen_on(&window, monitor),
                    WindowTarget::PresenterConsole => put_maximised_on(&window, monitor),
                }
                live.on = should_be.clone();
            }
        });
    }

    /// A presentation window: borderless fullscreen on the given screen.
    ///
    /// Fullscreen is dropped first and taken again afterwards. A window that
    /// is already fullscreen ignores a move on every platform Cantara runs on
    /// — the compositor owns its geometry — so the only way onto another
    /// screen is to stop being fullscreen, go there, and ask again. The screen
    /// is named in the request as well as moved to, because on Wayland the
    /// position is a hint and the handle is not.
    fn put_fullscreen_on(window: &Window, monitor: &MonitorInfo) {
        window.set_fullscreen(None);
        window.set_outer_position(PhysicalPosition::new(monitor.position.0, monitor.position.1));
        window.set_inner_size(PhysicalSize::new(monitor.size.0, monitor.size.1));
        window.set_fullscreen(Some(Fullscreen::Borderless(handle_for(window, monitor))));
    }

    /// The presenter console: maximised on the given screen, and not
    /// fullscreen — it is a window the operator works in, with a title bar
    /// they may want.
    fn put_maximised_on(window: &Window, monitor: &MonitorInfo) {
        window.set_maximized(false);
        window.set_outer_position(PhysicalPosition::new(monitor.position.0, monitor.position.1));
        window.set_inner_size(PhysicalSize::new(monitor.size.0, monitor.size.1));
        window.set_maximized(true);
    }

    /// The platform's own handle for a screen Cantara has described.
    ///
    /// Matched on name *and* position: a name is frequently empty and
    /// frequently shared between two identical screens (which is why nothing
    /// is stored by name — see [`screen_key`]), and two screens cannot share a
    /// position.
    fn handle_for(window: &Window, monitor: &MonitorInfo) -> Option<MonitorHandle> {
        window.available_monitors().find(|handle| {
            handle.name().unwrap_or_default() == monitor.name
                && (handle.position().x, handle.position().y) == monitor.position
        })
    }
}

#[cfg(feature = "desktop")]
pub use live::{move_to, open_windows, remember};

/// A build with no windows of its own has none to move. The browser build is
/// one: it draws the map for the Playwright tests (D8) and has no presentation
/// window behind it, so "apply now" has nothing to offer and never appears.
#[cfg(not(feature = "desktop"))]
pub fn open_windows() -> Vec<WindowScreen> {
    Vec::new()
}

#[cfg(not(feature = "desktop"))]
pub fn move_to(_monitors: &[MonitorInfo], _wanted: &[WindowScreen]) {}

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

    fn screen_view(name: &str, monitor_name: Option<&str>) -> View {
        View {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            design_index: None,
            slide_settings_index: None,
            output: ViewOutput::Screen {
                monitor_name: monitor_name.map(str::to_string),
            },
            enabled: true,
            focus: ViewFocus::Follow,
        }
    }

    /// What a window should be on is what the opening itself would decide: a
    /// view that names a screen gets that one.
    #[test]
    fn a_view_wants_the_screen_it_names() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let settings = Settings {
            views: vec![screen_view("Wall", Some(&screen_key(&monitors[0])))],
            ..Default::default()
        };

        let wanted = wanted_screens(&settings, &monitors);

        assert_eq!(
            wanted,
            vec![(WindowTarget::View(0), Some(screen_key(&monitors[0])))]
        );
    }

    /// The console is only a window of its own when the settings say so, and
    /// only then is there anything to move.
    #[test]
    fn the_console_is_only_wanted_when_it_has_its_own_window() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let mut settings = Settings {
            views: vec![screen_view("Wall", None)],
            ..Default::default()
        };
        settings.show_presenter_console = true;

        settings.presenter_console_in_main_window = true;
        assert!(
            !wanted_screens(&settings, &monitors)
                .iter()
                .any(|(target, _)| *target == WindowTarget::PresenterConsole)
        );

        settings.presenter_console_in_main_window = false;
        assert!(
            wanted_screens(&settings, &monitors)
                .iter()
                .any(|(target, _)| *target == WindowTarget::PresenterConsole)
        );
    }

    /// A window that is where it should be is not pending, which is what keeps
    /// "apply now" away until there is something for it to do.
    #[test]
    fn nothing_is_pending_while_every_window_is_where_it_should_be() {
        let open = vec![(WindowTarget::View(0), Some("Beamer#1".to_string()))];

        assert!(pending(&open, &open).is_empty());
    }

    /// The case the button exists for: a chip was dropped on another screen
    /// while the service was running.
    #[test]
    fn a_window_whose_screen_has_changed_is_pending() {
        let open = vec![(WindowTarget::View(0), Some("Laptop#0".to_string()))];
        let wanted = vec![(WindowTarget::View(0), Some("Beamer#1".to_string()))];

        assert_eq!(pending(&open, &wanted), vec![WindowTarget::View(0)]);
    }

    /// A window nobody has an opinion about any more is left where it is.
    /// Switching a view off during a service is not what this control does.
    #[test]
    fn a_window_with_no_placement_left_is_not_pending() {
        let open = vec![(WindowTarget::View(3), Some("Laptop#0".to_string()))];

        assert!(pending(&open, &[]).is_empty());
    }

    /// Nothing is open, so nothing can be out of date — the settings page of a
    /// machine with no service running.
    #[test]
    fn nothing_is_pending_with_no_windows_open() {
        let wanted = vec![(WindowTarget::View(0), Some("Beamer#1".to_string()))];

        assert!(pending(&[], &wanted).is_empty());
    }
}
