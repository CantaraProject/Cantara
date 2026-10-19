//! Where each screen is drawn on the map of them, and what the pointer is over.
//!
//! The screens are listed as text today — a name, a resolution, a word for the
//! primary. That says which screens exist and nothing about where they are,
//! which is the question somebody standing at the technician's desk actually
//! has. `docs/specs/0007-screen-map.md` asks for them to be drawn instead, in
//! the proportions and the arrangement the platform reports, and for windows
//! to be put on them by dragging.
//!
//! This is the arithmetic for that, with no page in it: the box the screens
//! occupy together, the transform from the virtual desktop into the box the
//! map is drawn in, and which screen a point is inside. What it looks like and
//! how the dragging works is [`crate::components::screen_map`]; the pointer
//! mechanics underneath are [`crate::components::drag`].
//!
//! # Coordinates
//!
//! Two spaces, and keeping them apart is most of what this module is for.
//!
//! * **Virtual desktop** — what [`MonitorInfo`] reports. One origin for every
//!   screen, in physical pixels. A screen placed to the left of the primary
//!   one has a negative `x`; a screen above it a negative `y`. Neither is an
//!   error, and both are ordinary on a desk with two monitors.
//! * **The map** — a box on the page, in whatever unit the caller draws in,
//!   with its origin at its own top left.
//!
//! [`MapTransform`] goes from the first to the second. Nothing else should do
//! that arithmetic by hand.

use crate::logic::screens::MonitorInfo;

/// The smallest a screen is drawn, in the map's units.
///
/// Without a floor, a 1024×768 auxiliary beside a 5K display is a sliver a few
/// pixels wide with a name that cannot be shown — technically in proportion,
/// and useless for the one thing the map is for. A floor bends the truth a
/// little for the extreme case so that the ordinary case stays readable.
const MIN_EDGE: f64 = 24.0;

/// The rectangle every screen together occupies, in the virtual desktop.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bounds {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Bounds {
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

/// The box a screen is drawn in, in the map's own coordinates.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Placed {
    /// Which screen this is: its position in the list that was passed in.
    pub id: usize,
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Placed {
    /// Whether a point in the map's coordinates is inside this screen.
    ///
    /// The right and bottom edges are outside, so that two screens drawn
    /// edge to edge — which is what a desktop that is one continuous surface
    /// reports — never both claim the same point.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.left
            && x < self.left + self.width
            && y >= self.top
            && y < self.top + self.height
    }
}

/// What turns a screen's place in the virtual desktop into its place on the
/// map.
///
/// Built by [`map_transform`], which is where the reasoning is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MapTransform {
    bounds: Bounds,
    scale: f64,
    /// What is added after scaling, to centre the arrangement in the box.
    offset: (f64, f64),
}

impl MapTransform {
    /// Where a screen lands on the map.
    pub fn place(&self, monitor: &MonitorInfo) -> Placed {
        let (x, y) = (monitor.position.0 as f64, monitor.position.1 as f64);
        let (width, height) = (monitor.size.0 as f64, monitor.size.1 as f64);

        Placed {
            id: monitor.id,
            left: (x - self.bounds.left) * self.scale + self.offset.0,
            top: (y - self.bounds.top) * self.scale + self.offset.1,
            width: (width * self.scale).max(MIN_EDGE),
            height: (height * self.scale).max(MIN_EDGE),
        }
    }
}

/// The rectangle the given screens occupy together.
///
/// `None` for no screens: there is no box around nothing, and every caller
/// has to deal with that case anyway.
pub fn bounds(monitors: &[MonitorInfo]) -> Option<Bounds> {
    let mut monitors = monitors.iter();
    let first = monitors.next()?;

    let mut bounds = Bounds {
        left: first.position.0 as f64,
        top: first.position.1 as f64,
        right: first.position.0 as f64 + first.size.0 as f64,
        bottom: first.position.1 as f64 + first.size.1 as f64,
    };

    for monitor in monitors {
        let (x, y) = (monitor.position.0 as f64, monitor.position.1 as f64);
        bounds.left = bounds.left.min(x);
        bounds.top = bounds.top.min(y);
        bounds.right = bounds.right.max(x + monitor.size.0 as f64);
        bounds.bottom = bounds.bottom.max(y + monitor.size.1 as f64);
    }

    Some(bounds)
}

/// How to draw `monitors` inside a box `width` by `height`.
///
/// One scale for both directions, so that a screen keeps its shape: a 16:9
/// beamer drawn as a square would be a picture of something that is not there.
/// The arrangement is then centred in whichever direction it does not fill,
/// because a map pushed into a corner reads as a mistake rather than as a
/// layout.
///
/// `None` when there is nothing to draw, or when the box has no room in it.
pub fn map_transform(monitors: &[MonitorInfo], width: f64, height: f64) -> Option<MapTransform> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let bounds = bounds(monitors)?;

    // A screen reporting a size of nothing would divide by zero. It should not
    // happen and does, on a laptop lid that has just closed.
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return None;
    }

    let scale = (width / bounds.width()).min(height / bounds.height());
    let offset = (
        (width - bounds.width() * scale) / 2.0,
        (height - bounds.height() * scale) / 2.0,
    );

    Some(MapTransform {
        bounds,
        scale,
        offset,
    })
}

/// Every screen, in the order given, with the box it is drawn in.
pub fn place_all(monitors: &[MonitorInfo], width: f64, height: f64) -> Vec<Placed> {
    let Some(transform) = map_transform(monitors, width, height) else {
        return Vec::new();
    };
    monitors
        .iter()
        .map(|monitor| transform.place(monitor))
        .collect()
}

/// Which of the drawn screens a point on the map is inside.
///
/// The last one wins where two overlap. Screens should not overlap — the
/// platform reports one continuous surface — but [`MIN_EDGE`] can make two
/// tiny ones do so, and a later screen is the one drawn on top, so it is the
/// one the pointer looks to be over.
pub fn screen_at(placed: &[Placed], x: f64, y: f64) -> Option<usize> {
    placed
        .iter()
        .rev()
        .find(|screen| screen.contains(x, y))
        .map(|screen| screen.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: usize, position: (i32, i32), size: (u32, u32)) -> MonitorInfo {
        MonitorInfo {
            id,
            name: format!("Screen {id}"),
            position,
            size,
            is_primary: id == 0,
        }
    }

    /// The ordinary desk: a laptop with a beamer plugged in to the right of
    /// it. Both are drawn, side by side, in the order the platform reports.
    #[test]
    fn two_screens_side_by_side_are_drawn_side_by_side() {
        let monitors = vec![
            monitor(0, (0, 0), (1920, 1080)),
            monitor(1, (1920, 0), (1920, 1080)),
        ];

        let placed = place_all(&monitors, 400.0, 200.0);

        assert_eq!(placed.len(), 2);
        assert!(
            placed[0].left < placed[1].left,
            "the second screen was not drawn to the right of the first"
        );
        assert_eq!(placed[0].top, placed[1].top, "they are not level");
        assert!((placed[0].width - placed[1].width).abs() < 0.01);
    }

    /// The proportions are the screens' own. A screen twice as wide is drawn
    /// twice as wide, which is the whole reason for drawing them at all.
    #[test]
    fn a_bigger_screen_is_drawn_bigger() {
        let monitors = vec![
            monitor(0, (0, 0), (1920, 1080)),
            monitor(1, (1920, 0), (3840, 2160)),
        ];

        let placed = place_all(&monitors, 600.0, 300.0);

        assert!(
            (placed[1].width - placed[0].width * 2.0).abs() < 0.01,
            "the 4K screen was not drawn twice the width of the 1080p one"
        );
        assert!((placed[1].height - placed[0].height * 2.0).abs() < 0.01);
    }

    /// A screen keeps its shape. One scale for both directions, so a 16:9
    /// screen is drawn 16:9 however the box it goes in is shaped.
    #[test]
    fn a_screen_keeps_its_aspect_ratio() {
        let monitors = vec![monitor(0, (0, 0), (1920, 1080))];

        // A box far wider than it is tall. Stretching to fill it would make a
        // 16:9 screen into something else entirely.
        let placed = place_all(&monitors, 1000.0, 100.0);

        let drawn = placed[0].width / placed[0].height;
        assert!(
            (drawn - 1920.0 / 1080.0).abs() < 0.01,
            "the screen was drawn {drawn:.3} wide for its height, not 16:9"
        );
    }

    /// A screen the platform places *above* another is drawn above it. A
    /// stage monitor hung over the desk is a real arrangement, and one that
    /// a map laying everything out in a row would misdescribe.
    #[test]
    fn a_screen_stacked_above_another_is_drawn_above_it() {
        let monitors = vec![
            monitor(0, (0, 0), (1920, 1080)),
            monitor(1, (0, -1080), (1920, 1080)),
        ];

        let placed = place_all(&monitors, 200.0, 400.0);

        assert!(
            placed[1].top < placed[0].top,
            "the screen above was not drawn above"
        );
        assert_eq!(placed[0].left, placed[1].left, "they are not aligned");
    }

    /// A screen turned on its side is drawn on its side. Portrait screens are
    /// ordinary for song texts, and the platform already reports the size
    /// turned round.
    #[test]
    fn a_portrait_screen_is_drawn_in_portrait() {
        let monitors = vec![monitor(0, (0, 0), (1080, 1920))];

        let placed = place_all(&monitors, 400.0, 400.0);

        assert!(
            placed[0].height > placed[0].width,
            "a portrait screen was drawn in landscape"
        );
    }

    /// A screen to the *left* of the primary one reports a negative `x`. That
    /// is not an error and must not push the map off its own box: everything
    /// is drawn relative to the bounding box, not to the origin.
    #[test]
    fn a_screen_at_a_negative_origin_is_still_drawn_inside_the_box() {
        let monitors = vec![
            monitor(0, (0, 0), (1920, 1080)),
            monitor(1, (-1920, 0), (1920, 1080)),
        ];

        let placed = place_all(&monitors, 400.0, 200.0);

        assert!(
            placed.iter().all(|screen| screen.left >= -0.01 && screen.top >= -0.01),
            "a screen was drawn outside the box: {placed:?}"
        );
        assert!(
            placed[1].left < placed[0].left,
            "the screen to the left was not drawn to the left"
        );
    }

    /// The arrangement is centred in the direction it does not fill. Two
    /// screens side by side in a tall box sit in the middle of it, not against
    /// the top edge.
    #[test]
    fn the_arrangement_is_centred_in_the_direction_it_does_not_fill() {
        let monitors = vec![
            monitor(0, (0, 0), (1920, 1080)),
            monitor(1, (1920, 0), (1920, 1080)),
        ];

        let placed = place_all(&monitors, 400.0, 400.0);

        let above = placed[0].top;
        let below = 400.0 - (placed[0].top + placed[0].height);
        assert!(
            (above - below).abs() < 0.01,
            "the arrangement was not centred vertically: {above} above, {below} below"
        );
    }

    /// The laptop on the kitchen table. One screen fills the box it is given,
    /// and is still a map.
    #[test]
    fn one_screen_fills_the_map() {
        let monitors = vec![monitor(0, (0, 0), (1920, 1080))];

        let placed = place_all(&monitors, 400.0, 225.0);

        assert_eq!(placed.len(), 1);
        assert!((placed[0].width - 400.0).abs() < 0.01);
        assert!((placed[0].left).abs() < 0.01);
    }

    /// No screens at all — which is every non-desktop build, and a desktop one
    /// whose enumeration failed. Nothing is drawn, and nothing panics.
    #[test]
    fn no_screens_draw_nothing() {
        assert_eq!(bounds(&[]), None);
        assert_eq!(map_transform(&[], 400.0, 200.0), None);
        assert!(place_all(&[], 400.0, 200.0).is_empty());
    }

    /// A box with no room in it. The map is laid out before it has been
    /// measured, and a width of zero must not divide by itself.
    #[test]
    fn a_box_with_no_room_draws_nothing() {
        let monitors = vec![monitor(0, (0, 0), (1920, 1080))];

        assert_eq!(map_transform(&monitors, 0.0, 200.0), None);
        assert_eq!(map_transform(&monitors, 400.0, 0.0), None);
        assert!(place_all(&monitors, 0.0, 0.0).is_empty());
    }

    /// A screen reporting no size at all — a laptop lid that has just closed.
    /// Nothing is drawn rather than a division by zero.
    #[test]
    fn a_screen_of_no_size_draws_nothing() {
        let monitors = vec![monitor(0, (0, 0), (0, 0))];

        assert_eq!(map_transform(&monitors, 400.0, 200.0), None);
    }

    /// A screen far smaller than the rest is still big enough to see and to
    /// drop something onto.
    #[test]
    fn a_very_small_screen_is_still_drawn_big_enough_to_hit() {
        let monitors = vec![
            monitor(0, (0, 0), (7680, 4320)),
            monitor(1, (7680, 0), (640, 480)),
        ];

        let placed = place_all(&monitors, 400.0, 225.0);

        assert!(
            placed[1].width >= MIN_EDGE && placed[1].height >= MIN_EDGE,
            "the small screen was drawn too small to hit: {:?}",
            placed[1]
        );
    }

    /// The point of the whole module: a point inside a screen resolves to that
    /// screen, for every screen in the arrangement. This is what a drop uses.
    #[test]
    fn a_point_inside_a_screen_finds_that_screen() {
        let monitors = vec![
            monitor(0, (0, 0), (1920, 1080)),
            monitor(1, (1920, 0), (1920, 1080)),
            monitor(2, (0, -1080), (1920, 1080)),
        ];

        let placed = place_all(&monitors, 400.0, 400.0);

        for screen in &placed {
            let middle = (
                screen.left + screen.width / 2.0,
                screen.top + screen.height / 2.0,
            );
            assert_eq!(
                screen_at(&placed, middle.0, middle.1),
                Some(screen.id),
                "the middle of screen {} did not resolve to it",
                screen.id
            );
        }
    }

    /// A point outside every screen is over nothing. A drop there changes
    /// nothing, which is how a drag is called off.
    #[test]
    fn a_point_outside_every_screen_is_over_nothing() {
        let monitors = vec![monitor(0, (0, 0), (1920, 1080))];
        let placed = place_all(&monitors, 400.0, 400.0);

        assert_eq!(screen_at(&placed, -10.0, -10.0), None);
        assert_eq!(screen_at(&placed, 1000.0, 1000.0), None);
    }

    /// Two screens drawn edge to edge do not both claim the seam between them.
    /// The platform reports one continuous surface, so this is the ordinary
    /// case, not an edge case.
    #[test]
    fn the_seam_between_two_screens_belongs_to_exactly_one() {
        let monitors = vec![
            monitor(0, (0, 0), (1920, 1080)),
            monitor(1, (1920, 0), (1920, 1080)),
        ];

        let placed = place_all(&monitors, 400.0, 200.0);
        let seam = placed[0].left + placed[0].width;
        let middle = placed[0].top + placed[0].height / 2.0;

        assert_eq!(
            screen_at(&placed, seam, middle),
            Some(1),
            "the seam did not belong to the screen that starts there"
        );
    }

    /// The identity a placement carries is the screen's own, not its position
    /// in the drawing. `place_all` is given the list in enumeration order, and
    /// a caller resolving a drop reads `id` back out.
    #[test]
    fn a_placement_carries_the_screens_own_id() {
        let monitors = vec![monitor(3, (0, 0), (1920, 1080)), monitor(7, (1920, 0), (1920, 1080))];

        let placed = place_all(&monitors, 400.0, 200.0);

        assert_eq!(placed[0].id, 3);
        assert_eq!(placed[1].id, 7);
    }
}
