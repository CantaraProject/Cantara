//! The screens, drawn as they are arranged, with the windows that go on them.
//!
//! `docs/specs/0007-screen-map.md`. The screens used to be a list of text; this
//! draws them in the proportions and the arrangement the platform reports, and
//! lets a window be put on one by dragging a chip onto it.
//!
//! The arithmetic — the bounding box, the transform, which screen a point is
//! inside — is [`crate::logic::screen_map`], where it is tested without a page.
//! The pointer mechanics are [`crate::components::drag`], shared with the four
//! lists that are reordered by dragging. What is here is the drawing, the
//! chips, and what a drop means.
//!
//! # What a chip says, and what it does not
//!
//! A chip on a screen means "this window opens on this screen". That is not
//! the same as "this window is on this screen now": a change takes effect the
//! next time the presentation starts, which is decision D5 of the spec, so
//! that tidying the settings in the middle of a service does not black out the
//! projection. Moving the windows of a running service is a control of its own
//! and is not built yet.
//!
//! # Chosen, and where it lands
//!
//! These are two different things and the map shows both (D4). A view that
//! names no screen is not unplaced — [`place_screen_views`] gives it a
//! definite one, preferring a screen that is not the operator's and that no
//! other view has claimed. Drawing such a chip in a holding area would hide
//! the answer to the question the operator actually came with, which is "where
//! will this go?". So it is drawn on the screen it will land on, dashed, and
//! the holding area is what a chip is dragged *back* to.
//!
//! The same applies to a screen that has been unplugged (D10). It is not
//! drawn — it is not there — and its windows appear, dashed, on whatever
//! screen they now fall back to. What is *stored* is not touched: a key keeps
//! naming the absent screen, so that plugging the beamer back in restores the
//! arrangement instead of silently leaving it on the laptop.

use crate::components::drag::{Aim, use_pointer_drag};
use crate::logic::live_windows::WindowTarget;
use crate::logic::screen_map::{Placed, place_all, screen_at};
use crate::logic::screens::{MonitorInfo, resolve_monitor, screen_key};
use crate::logic::settings::{Settings, View, ViewOutput};
use dioxus::prelude::*;
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

/// The map's own styles. Carried by the component, as the other per-feature
/// sheets are.
const SCREEN_MAP_CSS: Asset = asset!("/assets/screen_map.css");

/// The element the drag measures its drop zones inside, and captures a mouse
/// to. See [`crate::components::drag`].
const MAP_ID: &str = "screen-map";

/// The box the screens are laid out in, in the units the map is drawn in.
///
/// The map is drawn as an SVG-like grid of absolutely positioned boxes inside
/// a fixed viewport, rather than measured from the page: measuring would mean
/// a round trip before anything could be drawn, and the arrangement would jump
/// once it arrived. A fixed box scaled by CSS keeps the proportions — which
/// are the point — without that.
///
/// Wide and shallow on purpose. The box is not the arrangement: the
/// arrangement is fitted into it and centred in whichever direction it does
/// not fill, so the box's own shape costs nothing but the room it takes on a
/// settings page that is already long. Screens sit beside one another far more
/// often than above one another, so a taller box spent most of its height on
/// empty margin and pushed what comes after the map off the window.
const MAP_WIDTH: f64 = 600.0;
const MAP_HEIGHT: f64 = 200.0;

/// A window that wants a screen.
#[derive(Clone, PartialEq, Debug)]
pub struct Chip {
    /// What the user calls it.
    pub label: String,
    /// Which window this is, and what a drop changes.
    pub kind: ChipKind,
    /// The screen it is set to, as a [`screen_key`]. `None` is "automatic".
    pub chosen: Option<String>,
}

/// Which window a chip stands for, which is what a drop has to write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChipKind {
    /// One of [`Settings::views`], by its position.
    View(usize),
    /// The presenter console, which has a screen of its own in the settings.
    PresenterConsole,
    /// The window Cantara is being operated in. Shown so that the operator can
    /// tell which rectangle they are looking at the map on, and not draggable:
    /// where the main window sits is the window manager's business, and
    /// remembering a position is a promise the next session may not be able to
    /// keep — see [`crate::logic::window_state`] (D3).
    MainWindow,
}

impl ChipKind {
    fn is_draggable(&self) -> bool {
        !matches!(self, ChipKind::MainWindow)
    }

    /// The window of a running service this chip stands for, if it stands for
    /// one. The main window is never one of them (D3).
    ///
    /// A chip names its view by position, which is what the editor beside it
    /// works in; a window is known by the view's id, which survives a view
    /// above it being deleted. See [`WindowTarget::View`].
    fn target(&self, settings: &Settings) -> Option<WindowTarget> {
        match self {
            ChipKind::View(index) => settings
                .views
                .get(*index)
                .map(|view| WindowTarget::View(view.id)),
            ChipKind::PresenterConsole => Some(WindowTarget::PresenterConsole),
            ChipKind::MainWindow => None,
        }
    }
}

/// Where a chip is drawn, and why.
///
/// Worked out by the caller rather than by the map — see [`map_chips`] — so
/// that the map itself subscribes to nothing. A component that read the
/// settings would be redrawn by every keystroke anywhere on the settings page;
/// one whose whole input is these props is redrawn only when a chip has
/// actually moved.
#[derive(Clone, PartialEq, Debug)]
pub struct DrawnChip {
    chip: Chip,
    /// The screen it is drawn on, as a position in the monitor list.
    ///
    /// `None` puts it in the holding area, which happens only when there is no
    /// screen at all to draw it on.
    on: Option<usize>,
    /// Whether it is drawn dashed: it is where it is because something worked
    /// it out, not because anybody chose it.
    inferred: bool,
    /// Whether the window this stands for is open on a *different* screen
    /// right now.
    ///
    /// A drop does not move a window that is already open; it takes effect at
    /// the next start, or when "apply now" is pressed (D5). Until one of those
    /// happens the map is showing something that is not yet true, and saying
    /// so is the whole reason this flag exists.
    pending: bool,
}

/// Everything the map draws, worked out from the settings in one call.
///
/// This is the whole of the map's input, and the reason [`ScreenMap`] takes
/// props rather than reading the settings itself.
/// `pending` is [`crate::logic::live_windows::pending`]'s answer: the windows
/// a running service has open somewhere other than where the map now says.
pub fn map_chips(
    settings: &Settings,
    monitors: &[MonitorInfo],
    pending: &[WindowTarget],
) -> Vec<DrawnChip> {
    let mut drawn = draw_chips(&settings_chips(settings), monitors, settings);
    for chip in drawn.iter_mut() {
        chip.pending = chip
            .chip
            .kind
            .target(settings)
            .is_some_and(|target| pending.contains(&target));
    }
    drawn
}

/// Every chip, and the screen it is drawn on.
///
/// Chosen screens first, so that a chip that names one is never bumped off it
/// by the arithmetic for one that does not.
fn draw_chips(chips: &[Chip], monitors: &[MonitorInfo], settings: &Settings) -> Vec<DrawnChip> {
    // Where each view actually lands, by the one rule that decides it — the
    // same answer the windows themselves will get when the service starts.
    // The map calls this rather than merely agreeing with it afterwards, which
    // is what makes a dashed chip able to say anything at all (D4).
    let placements = crate::logic::screens::place_screen_views(&settings.views, monitors);

    chips
        .iter()
        .map(|chip| {
            // What the user chose, if that screen is here.
            let chosen = chip.chosen.as_ref().and_then(|key| {
                monitors
                    .iter()
                    .position(|monitor| screen_key(monitor) == *key)
            });
            if let Some(on) = chosen {
                return DrawnChip {
                    chip: chip.clone(),
                    on: Some(on),
                    inferred: false,
                    pending: false,
                };
            }

            // Nothing chosen, or a screen that is not plugged in. Either way
            // the question is the same: where will it actually go?
            let lands_on = match chip.kind {
                ChipKind::View(index) => placements
                    .iter()
                    .find(|placement| placement.index == index)
                    .and_then(|placement| placement.monitor.as_ref())
                    .and_then(|monitor| {
                        monitors.iter().position(|other| other.id == monitor.id)
                    }),
                // The console prefers the operator's screen, which is the
                // opposite of what a projection prefers.
                ChipKind::PresenterConsole => {
                    resolve_monitor(monitors, &chip.chosen, true).and_then(|monitor| {
                        monitors.iter().position(|other| other.id == monitor.id)
                    })
                }
                ChipKind::MainWindow => monitors.iter().position(|monitor| monitor.is_primary),
            };

            DrawnChip {
                chip: chip.clone(),
                on: lands_on,
                inferred: true,
                pending: false,
            }
        })
        .collect()
}

/// Whether any chip is drawn somewhere other than where it is set to.
///
/// What is drawn is where a window will go; what is stored is what the user
/// asked for. When a screen is unplugged the two differ, and the map says so
/// in a line rather than letting the picture quietly contradict the drop-down
/// beside it (D10).
fn any_screen_missing(chips: &[DrawnChip], monitors: &[MonitorInfo]) -> bool {
    chips.iter().any(|drawn| {
        drawn.chip.chosen.as_ref().is_some_and(|key| {
            !monitors
                .iter()
                .any(|monitor| screen_key(monitor) == *key)
        })
    })
}

/// The chips the screen settings show: every enabled view that wants a screen,
/// the presenter console when it has a window of its own, and the main window.
pub fn settings_chips(settings: &Settings) -> Vec<Chip> {
    let mut chips: Vec<Chip> = settings
        .views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.enabled)
        .filter_map(|(index, view)| match &view.output {
            ViewOutput::Screen { monitor_name } => Some(Chip {
                label: view_label(view),
                kind: ChipKind::View(index),
                chosen: monitor_name.clone(),
            }),
            // A view served over the network has no screen to be put on.
            ViewOutput::Network { .. } => None,
        })
        .collect();

    if settings.show_presenter_console && !settings.presenter_console_in_main_window {
        chips.push(Chip {
            label: t!("settings.presenter_screen").to_string(),
            kind: ChipKind::PresenterConsole,
            chosen: settings.presenter_screen.clone(),
        });
    }

    chips.push(Chip {
        label: t!("settings.screen_map.main_window").to_string(),
        kind: ChipKind::MainWindow,
        chosen: None,
    });

    chips
}

/// What a view is called on a chip. A view may be left unnamed, and an empty
/// chip would say nothing at all.
fn view_label(view: &View) -> String {
    match view.name.trim().is_empty() {
        true => t!("settings.screen_map.unnamed_view").to_string(),
        false => view.name.clone(),
    }
}

/// What a screen is called on the map.
///
/// A monitor's reported name is often empty, and on some platforms always is.
/// The number stands in, and the size is shown underneath either way, because
/// two screens on a machine are nearly always different sizes and that is what
/// a person can match against what is in front of them.
fn screen_name(monitor: &MonitorInfo) -> String {
    match monitor.name.trim().is_empty() {
        true => t!("settings.screen_map.screen_number", number = monitor.id + 1).to_string(),
        false => monitor.name.clone(),
    }
}

/// The map, with the given chips on it.
///
/// `chips` is [`map_chips`]' answer: where every window is drawn, and whether
/// anybody chose that. The map works nothing out from the settings itself, so
/// that it is redrawn when a chip moves and not when any other setting on the
/// page is touched.
///
/// `on_assign` is called with the chip and the screen it was dropped on — as a
/// [`screen_key`], or `None` for the holding area, which is "automatic".
#[component]
pub fn ScreenMap(
    monitors: Vec<MonitorInfo>,
    chips: Vec<DrawnChip>,
    on_assign: Callback<(ChipKind, Option<String>)>,
) -> Element {
    let placed: Vec<Placed> = place_all(&monitors, MAP_WIDTH, MAP_HEIGHT);
    let missing = any_screen_missing(&chips, &monitors);
    let drawn = chips;

    // The drop zones are the screens, plus the holding area last. Note that
    // they are *not* the things being dragged — the chips are — which is the
    // distinction `PointerDrag` keeps: it counts zones and carries the
    // pressed chip's number as an opaque value.
    let zone_count = placed.len() + 1;
    let holding_area = placed.len();

    let monitors_for_drop = monitors.clone();
    let chips_for_drop = drawn.clone();

    let drag = use_pointer_drag(
        MAP_ID,
        zone_count,
        // What the pointer is over: a screen, or the holding area.
        //
        // The zones are the rectangles as the *page* laid them out, not as
        // `place_all` computed them — a border, a padding or a rounding in the
        // CSS would otherwise put the seam between two screens in a slightly
        // different place than the one the operator is aiming at. They are run
        // through the same [`screen_at`] the arithmetic is tested on, so there
        // is one hit test rather than two that could disagree.
        move |aim: Aim| {
            let measured: Vec<Placed> = aim
                .zones
                .iter()
                .enumerate()
                .map(|(index, zone)| Placed {
                    id: index,
                    left: zone.left,
                    top: zone.top,
                    width: zone.right - zone.left,
                    height: zone.bottom - zone.top,
                })
                .collect();
            screen_at(&measured, aim.x, aim.y)
        },
        move |(chip_index, zone): (usize, usize)| {
            let Some(chip) = chips_for_drop.get(chip_index) else {
                return;
            };
            let key = match zone == holding_area {
                // Back to "automatic".
                true => None,
                false => monitors_for_drop.get(zone).map(screen_key),
            };
            // A drop on a zone that is no longer there changes nothing, which
            // is also what a drop outside everything does.
            if zone != holding_area && key.is_none() {
                return;
            }
            on_assign.call((chip.chip.kind, key));
        },
    );

    rsx! {
        document::Stylesheet { href: SCREEN_MAP_CSS }

        div {
            id: MAP_ID,
            class: if drag.is_dragging() { "screen-map screen-map-active" } else { "screen-map" },
            // The map is a picture of something the drop-downs below also say.
            // Marked as such so that it is not an obstacle to tab through: the
            // keyboard path is the drop-downs (D7).
            role: "presentation",
            onpointermove: move |event| drag.moved(&event.data()),
            onpointerup: move |_| drag.released(),
            onpointercancel: move |_| drag.cancelled(),
            onpointerenter: move |event| drag.entered(&event.data()),
            ondragstart: crate::components::drag::refuse_native_drag,

            div {
                class: "screen-map-stage",
                style: "aspect-ratio: {MAP_WIDTH} / {MAP_HEIGHT};",

                for screen in placed.iter() {
                    {
                        let index = monitors
                            .iter()
                            .position(|monitor| monitor.id == screen.id)
                            .unwrap_or(0);
                        let monitor = &monitors[index];
                        let left = screen.left / MAP_WIDTH * 100.0;
                        let top = screen.top / MAP_HEIGHT * 100.0;
                        let width = screen.width / MAP_WIDTH * 100.0;
                        let height = screen.height / MAP_HEIGHT * 100.0;
                        let is_target = drag.is_dragging() && drag.target() == Some(index);

                        rsx! {
                            div {
                                key: "{screen.id}",
                                class: if is_target { "screen-map-screen screen-map-over" } else { "screen-map-screen" },
                                style: "left: {left}%; top: {top}%; width: {width}%; height: {height}%;",
                                onmounted: move |event| drag.mounted(index, event),

                                div { class: "screen-map-screen-name", {screen_name(monitor)} }
                                div { class: "screen-map-screen-size",
                                    {format!("{} × {}", monitor.size.0, monitor.size.1)}
                                }
                                if monitor.is_primary {
                                    div { class: "screen-map-primary",
                                        {t!("settings.primary_monitor").to_string()}
                                    }
                                }

                                div { class: "screen-map-chips",
                                    for (chip_index, drawn_chip) in drawn.iter().enumerate() {
                                        if drawn_chip.on == Some(index) {
                                            ChipView {
                                                key: "{chip_index}",
                                                index: chip_index,
                                                chip: drawn_chip.clone(),
                                                drag,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Where a chip goes back to when it should be worked out rather
            // than chosen. Also where every chip sits when there is no screen
            // to draw at all.
            div {
                class: if drag.is_dragging() && drag.target() == Some(holding_area) {
                    "screen-map-holding screen-map-over"
                } else {
                    "screen-map-holding"
                },
                onmounted: move |event| drag.mounted(holding_area, event),

                div { class: "screen-map-holding-label",
                    {t!("settings.automatic").to_string()}
                }
                div { class: "screen-map-chips",
                    for (chip_index, drawn_chip) in drawn.iter().enumerate() {
                        if drawn_chip.on.is_none() {
                            ChipView {
                                key: "{chip_index}",
                                index: chip_index,
                                chip: drawn_chip.clone(),
                                drag,
                            }
                        }
                    }
                }
            }
        }

        if missing {
            p {
                class: "screen-map-note",
                {t!("settings.screen_map.screen_missing").to_string()}
            }
        }
    }
}

/// One window's chip.
#[component]
fn ChipView(
    index: usize,
    chip: DrawnChip,
    drag: crate::components::drag::PointerDrag<usize>,
) -> Element {
    let draggable = chip.chip.kind.is_draggable();
    let carried = drag.carried() == Some(index);

    let mut class = String::from("screen-map-chip");
    if chip.inferred {
        class.push_str(" screen-map-chip-inferred");
    }
    if !draggable {
        class.push_str(" screen-map-chip-fixed");
    }
    if carried {
        class.push_str(" screen-map-chip-carried");
    }
    if chip.pending {
        class.push_str(" screen-map-chip-pending");
    }

    // The carried chip follows the pointer with a transform, so that nothing
    // around it reflows and the measurements taken when the drag started stay
    // true. Every other chip says `transform: none` outright rather than
    // nothing: Dioxus merges a new `style` into the old one, and a chip whose
    // style simply left the transform out would keep the last one it was
    // given — see `ReorderDrag::item_style`, where this was first paid for.
    let transform = match carried.then(|| drag.carry_offset()).flatten() {
        Some((dx, dy)) => format!("transform: translate({dx}px, {dy}px);"),
        None => "transform: none;".to_string(),
    };

    rsx! {
        div {
            class,
            style: "{transform} {grip_style(draggable)}",
            title: chip.chip.label.clone(),
            onpointerdown: move |event| {
                if draggable {
                    // A finger may drag a chip anywhere on it: a chip is small
                    // and has no room for a grip of its own, and the map is
                    // not a list that needs to scroll under the finger.
                    drag.press(index, &event.data(), true);
                }
            },
            {chip.chip.label.clone()}
        }
    }
}

/// A draggable chip claims the touch that starts on it; a fixed one leaves it
/// to the page.
fn grip_style(draggable: bool) -> &'static str {
    match draggable {
        true => crate::components::drag::GRIP_STYLE,
        false => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::settings::{ViewFocus, ViewOutput};

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

    fn settings_with(views: Vec<View>) -> Settings {
        Settings {
            views,
            ..Default::default()
        }
    }

    /// A chip that names a screen is drawn on it, and not dashed: somebody
    /// chose this.
    #[test]
    fn a_chip_that_names_a_screen_is_drawn_on_it() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let settings = settings_with(vec![screen_view(
            "Projection",
            Some(&screen_key(&monitors[1])),
            true,
        )]);
        let chips = settings_chips(&settings);

        let drawn = draw_chips(&chips, &monitors, &settings);
        let projection = &drawn[0];

        assert_eq!(projection.on, Some(1));
        assert!(!projection.inferred, "a chosen screen was drawn as inferred");
    }

    /// The decision this component exists to carry (D4): a chip that names no
    /// screen is drawn on the screen it will actually land on, dashed — not
    /// parked in the holding area where it would answer nothing.
    #[test]
    fn a_chip_that_names_no_screen_is_drawn_where_it_will_land() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let settings = settings_with(vec![screen_view("Projection", None, true)]);
        let chips = settings_chips(&settings);

        let drawn = draw_chips(&chips, &monitors, &settings);
        let projection = &drawn[0];

        assert_eq!(
            projection.on,
            Some(1),
            "the projection was not drawn on the screen place_screen_views gives it"
        );
        assert!(
            projection.inferred,
            "a screen nobody chose was drawn as though somebody had"
        );
    }

    /// Two views that both say "wherever is free" are drawn on different
    /// screens, because that is where they will actually open. A map that drew
    /// both on the beamer would be showing the very bug
    /// `place_screen_views` exists to prevent.
    #[test]
    fn two_automatic_views_are_drawn_on_different_screens() {
        let monitors = vec![
            monitor(0, "Laptop", true),
            monitor(1, "Beamer", false),
            monitor(2, "Bühne", false),
        ];
        let settings = settings_with(vec![
            screen_view("Projection", None, true),
            screen_view("Stage", None, true),
        ]);
        let chips = settings_chips(&settings);

        let drawn = draw_chips(&chips, &monitors, &settings);

        assert_ne!(
            drawn[0].on, drawn[1].on,
            "two views that chose nothing were drawn on the same screen"
        );
    }

    /// A screen that has been unplugged (D10): the chip appears on whatever
    /// screen it now falls back to, dashed, and the stored key is untouched so
    /// that plugging the beamer back in restores the arrangement.
    #[test]
    fn a_chip_whose_screen_is_gone_is_drawn_where_it_falls_back_to() {
        let beamer = monitor(5, "Beamer", false);
        let monitors = vec![monitor(0, "Laptop", true)];
        let settings = settings_with(vec![screen_view(
            "Projection",
            Some(&screen_key(&beamer)),
            true,
        )]);
        let chips = settings_chips(&settings);

        let drawn = draw_chips(&chips, &monitors, &settings);

        assert_eq!(drawn[0].on, Some(0), "the chip vanished with its screen");
        assert!(drawn[0].inferred, "a fallback was drawn as a choice");
        assert_eq!(
            chips[0].chosen,
            Some(screen_key(&beamer)),
            "the stored screen was overwritten by the fallback"
        );
        assert!(
            any_screen_missing(&drawn, &monitors),
            "nothing told the user that the chosen screen is not there"
        );
    }

    /// With every chosen screen present there is nothing to explain, and the
    /// note under the map stays away.
    #[test]
    fn nothing_is_said_when_every_chosen_screen_is_there() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let settings = settings_with(vec![screen_view(
            "Projection",
            Some(&screen_key(&monitors[1])),
            true,
        )]);

        assert!(!any_screen_missing(
            &draw_chips(&settings_chips(&settings), &monitors, &settings),
            &monitors
        ));
    }

    /// A view served to the network has no screen, and a view that is switched
    /// off is not going to open. Neither gets a chip.
    #[test]
    fn only_enabled_screen_views_get_a_chip() {
        let settings = settings_with(vec![
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
        ]);

        let chips = settings_chips(&settings);
        let views: Vec<&Chip> = chips
            .iter()
            .filter(|chip| matches!(chip.kind, ChipKind::View(_)))
            .collect();

        assert_eq!(views.len(), 1);
        assert_eq!(views[0].kind, ChipKind::View(2));
    }

    /// The main window is always there to orient by, and is never draggable
    /// (D3).
    #[test]
    fn the_main_window_is_shown_and_cannot_be_dragged() {
        let settings = settings_with(vec![screen_view("Projection", None, true)]);

        let chips = settings_chips(&settings);
        let main = chips
            .iter()
            .find(|chip| chip.kind == ChipKind::MainWindow)
            .expect("the main window is not on the map");

        assert!(!main.kind.is_draggable());
    }

    /// The console gets a chip only when it is a window of its own. Shown
    /// inside the main window it has no screen to be put on.
    #[test]
    fn the_console_gets_a_chip_only_when_it_has_its_own_window() {
        let mut settings = settings_with(vec![screen_view("Projection", None, true)]);
        settings.show_presenter_console = true;

        settings.presenter_console_in_main_window = false;
        assert!(
            settings_chips(&settings)
                .iter()
                .any(|chip| chip.kind == ChipKind::PresenterConsole)
        );

        settings.presenter_console_in_main_window = true;
        assert!(
            !settings_chips(&settings)
                .iter()
                .any(|chip| chip.kind == ChipKind::PresenterConsole),
            "the console was given a screen it does not have a window for"
        );
    }

    /// The console prefers the operator's screen where nothing is chosen,
    /// which is the opposite of what a projection prefers.
    #[test]
    fn the_console_is_drawn_on_the_operators_screen_by_default() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let mut settings = settings_with(vec![screen_view("Projection", None, true)]);
        settings.show_presenter_console = true;
        settings.presenter_console_in_main_window = false;

        let chips = settings_chips(&settings);
        let drawn = draw_chips(&chips, &monitors, &settings);
        let console = drawn
            .iter()
            .find(|drawn| drawn.chip.kind == ChipKind::PresenterConsole)
            .expect("no console chip");

        assert_eq!(console.on, Some(0), "the console was not put on the laptop");
    }

    /// With no screens at all every chip sits in the holding area, and nothing
    /// panics. This is the desktop build whose enumeration came back empty.
    #[test]
    fn with_no_screens_every_chip_sits_in_the_holding_area() {
        let settings = settings_with(vec![screen_view("Projection", None, true)]);
        let chips = settings_chips(&settings);

        let drawn = draw_chips(&chips, &[], &settings);

        assert!(drawn.iter().all(|drawn| drawn.on.is_none()));
    }

    /// An unnamed view still says something on its chip.
    #[test]
    fn an_unnamed_view_still_has_a_label() {
        let settings = settings_with(vec![screen_view("   ", None, true)]);

        let chips = settings_chips(&settings);

        assert!(!chips[0].label.trim().is_empty());
    }

    /// D5: while a service is running, a chip whose window is still open on
    /// another screen is marked, because the map is showing a plan rather than
    /// what is on the wall.
    #[test]
    fn a_window_that_has_not_moved_yet_is_drawn_as_pending() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let settings = settings_with(vec![screen_view(
            "Projection",
            Some(&screen_key(&monitors[0])),
            true,
        )]);

        let drawn = map_chips(
            &settings,
            &monitors,
            &[WindowTarget::View(settings.views[0].id)],
        );

        assert!(drawn[0].pending, "a window that is elsewhere was drawn as settled");
        assert!(
            drawn
                .iter()
                .filter(|chip| chip.chip.kind != ChipKind::View(0))
                .all(|chip| !chip.pending),
            "a chip nothing is waiting on was marked"
        );
    }

    /// With nothing running there is nothing to be out of date, and no chip is
    /// marked — which is the ordinary case, and the one the mark must not
    /// clutter.
    #[test]
    fn nothing_is_pending_with_no_service_running() {
        let monitors = vec![monitor(0, "Laptop", true), monitor(1, "Beamer", false)];
        let settings = settings_with(vec![screen_view("Projection", None, true)]);

        let drawn = map_chips(&settings, &monitors, &[]);

        assert!(drawn.iter().all(|chip| !chip.pending));
    }

    /// A screen with no name is called by its number, not left blank.
    #[test]
    fn a_nameless_screen_is_called_by_its_number() {
        let name = screen_name(&monitor(1, "", false));

        assert!(!name.trim().is_empty());
        assert!(name.contains('2'), "screen 1 was not called \"2\": {name}");
    }
}
