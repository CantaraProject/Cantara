//! Where a dragged item would land, and what moving it does to a list.
//!
//! Cantara has four lists the user can put in order by dragging: the running
//! order, the source icons in the selection's sidebar, the presentation designs
//! and the slide settings. They used to be three implementations of dragging,
//! and only one of them worked with a finger. What they have in common is
//! here — the arithmetic, with no page in it, so that it can be tested without
//! a browser. What a drag looks like on screen is
//! [`crate::components::reorder`].
//!
//! # Gaps, not items
//!
//! A drop goes *between* two items, so positions are counted the way an
//! insertion index is: gap `0` is before the first item and gap `len` is after
//! the last. Moving an item down by one is therefore a move into gap
//! `from + 2` — the gap below the item after it. The two gaps touching an item
//! are where it already is, and are no move at all.
//!
//! # Coordinates
//!
//! Everything here is in one coordinate space, whichever the caller chooses.
//! The page measures the items once, when a drag starts, in the coordinates of
//! the viewport at that moment; when the page scrolls under a drag, the caller
//! adds the distance scrolled to the pointer rather than measuring again. See
//! [`crate::components::reorder`].

/// How far a press has to travel before it counts as a drag, in pixels.
///
/// Without it, pressing an item to open it flashed the drop marker for as long
/// as the button was down: the press *might* have been the start of a drag, and
/// the list said so before it could know. A few pixels of movement is what
/// tells the two apart, and is also what stops a hand that is not quite steady
/// on a touchscreen from turning every tap into a drag.
pub const DRAG_THRESHOLD: f64 = 5.0;

/// How close to the edge of the scrolling area the pointer has to be before
/// the area scrolls by itself, in pixels.
///
/// On a phone three designs are already taller than the screen. Moving the
/// first behind the last means scrolling while dragging, and a touch that has
/// been claimed for a drag does not scroll the page any more.
pub const AUTO_SCROLL_BAND: f64 = 48.0;

/// The most the scrolling area moves per step, in pixels — reached with the
/// pointer at the very edge, or past it.
pub const AUTO_SCROLL_MAX_STEP: f64 = 18.0;

/// Where an item is on the screen, as it was when the drag started.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ItemExtent {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl ItemExtent {
    fn vertical_middle(&self) -> f64 {
        (self.top + self.bottom) / 2.0
    }

    fn horizontal_middle(&self) -> f64 {
        (self.left + self.right) / 2.0
    }
}

/// How the items of a list are laid out, which decides how the pointer is
/// read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    /// One item under another. Only the pointer's height matters.
    Column,
    /// Items side by side, wrapping onto the next line where the width runs
    /// out — the design tiles. Where the window is narrow enough for one tile
    /// per line, this is read as a column.
    Grid,
}

/// Where a drop would go, and where the marker for it is drawn.
///
/// The gap alone is not enough to draw it. In a grid, the gap after the last
/// tile of a line is the same gap as the one before the first tile of the next
/// line, and the marker belongs on the side the pointer is on — not on a line
/// the pointer is nowhere near.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DropTarget {
    /// The gap the item would be moved into.
    pub gap: usize,
    /// The item the marker is drawn against.
    pub item: usize,
    /// Whether the marker is drawn after that item rather than before it.
    pub after: bool,
    /// Whether the items were read as a column, so that the marker is a line
    /// above or below the item rather than one to its left or right.
    pub column: bool,
}

impl DropTarget {
    fn before(item: usize, column: bool) -> Self {
        DropTarget {
            gap: item,
            item,
            after: false,
            column,
        }
    }

    fn after(item: usize, column: bool) -> Self {
        DropTarget {
            gap: item + 1,
            item,
            after: true,
            column,
        }
    }
}

/// Where a drop would go, given where the pointer is. `None` for an empty
/// list, where there is nothing to draw a marker against.
pub fn drop_target(layout: Layout, items: &[ItemExtent], x: f64, y: f64) -> Option<DropTarget> {
    if items.is_empty() {
        return None;
    }

    let lines = lines_of(items);
    // A grid narrow enough for one item per line *is* a column, and is read
    // as one: dividing a full-width tile at its horizontal middle would make
    // the end of the list reachable only from the right half of the last tile.
    let is_a_column = layout == Layout::Column || lines.iter().all(|line| line.len() == 1);

    if is_a_column {
        return Some(column_target(items, y));
    }
    Some(grid_target(items, &lines, x, y))
}

/// The gap a drop goes into, in a column. A row's own midpoint divides it —
/// above it the drop goes before the row, below it after.
fn column_target(items: &[ItemExtent], y: f64) -> DropTarget {
    for (index, item) in items.iter().enumerate() {
        if y < item.vertical_middle() {
            return DropTarget::before(index, true);
        }
    }
    DropTarget::after(items.len() - 1, true)
}

/// The gap a drop goes into, in tiles that wrap.
///
/// First the line: the one the pointer is in, or the nearest — a pointer above
/// the first line is the start of the list and below the last is its end,
/// which is the only way to reach either without precision. Then, within the
/// line, each tile is divided at its horizontal middle.
fn grid_target(
    items: &[ItemExtent],
    lines: &[std::ops::Range<usize>],
    x: f64,
    y: f64,
) -> DropTarget {
    let (first, last) = (&lines[0], &lines[lines.len() - 1]);
    if y < line_top(items, first) {
        return DropTarget::before(0, false);
    }
    if y > line_bottom(items, last) {
        return DropTarget::after(items.len() - 1, false);
    }

    // The line whose extent holds the pointer, or — between two lines — the
    // one whose edge is nearer.
    let line = lines
        .iter()
        .min_by(|a, b| {
            distance_to_line(items, a, y)
                .partial_cmp(&distance_to_line(items, b, y))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(first)
        .clone();

    for index in line.clone() {
        if x < items[index].horizontal_middle() {
            return DropTarget::before(index, false);
        }
    }
    DropTarget::after(line.end - 1, false)
}

/// The items grouped into lines, as ranges of their positions.
///
/// Tiles wrap in reading order, so a line is a run of consecutive items, and a
/// new line begins with the first item that starts below the middle of the
/// line so far. The middle rather than the bottom: tiles of slightly different
/// heights on one line must not be split up.
fn lines_of(items: &[ItemExtent]) -> Vec<std::ops::Range<usize>> {
    let mut lines: Vec<std::ops::Range<usize>> = Vec::new();
    let mut start = 0;
    for index in 1..items.len() {
        let line = start..index;
        let middle = (line_top(items, &line) + line_bottom(items, &line)) / 2.0;
        if items[index].top > middle {
            lines.push(line);
            start = index;
        }
    }
    lines.push(start..items.len());
    lines
}

fn line_top(items: &[ItemExtent], line: &std::ops::Range<usize>) -> f64 {
    items[line.clone()].iter().map(|item| item.top).fold(f64::INFINITY, f64::min)
}

fn line_bottom(items: &[ItemExtent], line: &std::ops::Range<usize>) -> f64 {
    items[line.clone()].iter().map(|item| item.bottom).fold(f64::NEG_INFINITY, f64::max)
}

fn distance_to_line(items: &[ItemExtent], line: &std::ops::Range<usize>, y: f64) -> f64 {
    let (top, bottom) = (line_top(items, line), line_bottom(items, line));
    if y < top {
        top - y
    } else if y > bottom {
        y - bottom
    } else {
        0.0
    }
}

/// Whether a press that started at `start` has moved far enough to be a drag,
/// in any direction — a tile can be picked up and carried sideways.
pub fn is_a_drag(start: (f64, f64), now: (f64, f64)) -> bool {
    let (dx, dy) = (now.0 - start.0, now.1 - start.1);
    (dx * dx + dy * dy).sqrt() >= DRAG_THRESHOLD
}

/// Where an item moved from `from` into gap `to` ends up, or `None` when that
/// is no move at all — onto one of its own two gaps, or out of range.
///
/// Removing the item first shifts every gap below it up by one, which is the
/// correction here.
pub fn landing_position(len: usize, from: usize, to: usize) -> Option<usize> {
    if from >= len || to > len || to == from || to == from + 1 {
        return None;
    }
    Some(if to > from { to - 1 } else { to })
}

/// Moves the item at `from` into the gap `to`, and says where it ended up.
pub fn move_into_gap<T>(items: &mut Vec<T>, from: usize, to: usize) -> Option<usize> {
    let landed = landing_position(items.len(), from, to)?;
    let item = items.remove(from);
    items.insert(landed, item);
    Some(landed)
}

/// The gap that moves the item at `index` by `offset` places — one up is `-1`,
/// one down is `1`. `None` past either end.
pub fn gap_for_step(len: usize, index: usize, offset: isize) -> Option<usize> {
    let target = index as isize + offset;
    if index >= len || target < 0 || target as usize >= len || offset == 0 {
        return None;
    }
    let target = target as usize;
    Some(if target > index { target + 1 } else { target })
}

/// Where the item that was at `watched` ends up when the item at `from` is
/// moved to `landed`.
///
/// The items are identified by their position in the list, so moving one
/// renumbers the items it passed. Anything else holding a position — which one
/// is open beside the list, which design is the default — has to be renumbered
/// with them, or it goes on pointing at the position and stops pointing at the
/// item.
pub fn index_after_move(watched: usize, from: usize, landed: usize) -> usize {
    if watched == from {
        return landed;
    }

    // Taking the item out pulls everything below it up one …
    let without = if watched > from { watched - 1 } else { watched };
    // … and putting it back pushes everything from its new place down one.
    if without >= landed { without + 1 } else { without }
}

/// How far the scrolling area should move for a pointer at `pointer`, given
/// where the area begins and ends on screen. Negative is towards the start.
///
/// Zero outside the band along each edge, and faster the nearer the edge:
/// a pointer that has only just entered the band should not throw the page.
pub fn auto_scroll_step(pointer: f64, area_start: f64, area_end: f64) -> f64 {
    // An area smaller than both bands together has no middle to rest in, and
    // would scroll back and forth under a still pointer.
    if area_end - area_start <= 2.0 * AUTO_SCROLL_BAND {
        return 0.0;
    }

    let from_start = pointer - area_start;
    let from_end = area_end - pointer;
    if from_start < AUTO_SCROLL_BAND {
        let depth = ((AUTO_SCROLL_BAND - from_start) / AUTO_SCROLL_BAND).min(1.0);
        return -(depth * AUTO_SCROLL_MAX_STEP).max(1.0);
    }
    if from_end < AUTO_SCROLL_BAND {
        let depth = ((AUTO_SCROLL_BAND - from_end) / AUTO_SCROLL_BAND).min(1.0);
        return (depth * AUTO_SCROLL_MAX_STEP).max(1.0);
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `count` rows of 20 pixels, one under the other.
    fn rows(count: usize) -> Vec<ItemExtent> {
        (0..count)
            .map(|index| ItemExtent {
                left: 0.0,
                top: index as f64 * 20.0,
                right: 200.0,
                bottom: index as f64 * 20.0 + 20.0,
            })
            .collect()
    }

    /// `count` tiles of 100 × 50, `per_line` to a line, 10 apart.
    fn tiles(count: usize, per_line: usize) -> Vec<ItemExtent> {
        (0..count)
            .map(|index| {
                let (column, line) = ((index % per_line) as f64, (index / per_line) as f64);
                ItemExtent {
                    left: column * 110.0,
                    top: line * 60.0,
                    right: column * 110.0 + 100.0,
                    bottom: line * 60.0 + 50.0,
                }
            })
            .collect()
    }

    fn gap(layout: Layout, items: &[ItemExtent], x: f64, y: f64) -> usize {
        drop_target(layout, items, x, y).map(|target| target.gap).unwrap_or(0)
    }

    /// The upper half of a row means "before it", the lower half "after it".
    /// That is what makes a drop land where the marker is drawn rather than a
    /// row away from it.
    #[test]
    fn test_the_midpoint_of_a_row_divides_it() {
        let rows = rows(3);

        assert_eq!(gap(Layout::Column, &rows, 0.0, 1.0), 0, "the top of the first row");
        assert_eq!(gap(Layout::Column, &rows, 0.0, 9.0), 0, "still its upper half");
        assert_eq!(gap(Layout::Column, &rows, 0.0, 11.0), 1, "its lower half is the gap after");
        assert_eq!(gap(Layout::Column, &rows, 0.0, 29.0), 1, "the upper half of the second");
        assert_eq!(gap(Layout::Column, &rows, 0.0, 31.0), 2, "and its lower half");
    }

    /// Past the last row is the gap at the end, not the last gap before it —
    /// otherwise nothing could be moved to the bottom of the list.
    #[test]
    fn test_below_the_last_row_is_the_end_of_the_list() {
        assert_eq!(gap(Layout::Column, &rows(3), 0.0, 500.0), 3);
        assert_eq!(
            drop_target(Layout::Column, &rows(3), 0.0, 500.0),
            Some(DropTarget { gap: 3, item: 2, after: true, column: true }),
            "and the marker is drawn under the last row"
        );
    }

    /// A pointer above the list belongs at its start. A drag that begins on a
    /// row and travels upwards out of the list reports coordinates above it.
    #[test]
    fn test_above_the_list_is_its_start() {
        assert_eq!(gap(Layout::Column, &rows(3), 0.0, -100.0), 0);
    }

    /// An empty list has nothing to drop against, and asking must not index
    /// into nothing.
    #[test]
    fn test_an_empty_list_has_no_target() {
        assert_eq!(drop_target(Layout::Column, &[], 0.0, 42.0), None);
        assert_eq!(drop_target(Layout::Grid, &[], 0.0, 42.0), None);
    }

    /// A tile is divided at its horizontal middle: its left half is the gap
    /// before it, its right half the gap after.
    #[test]
    fn test_a_tile_is_divided_at_its_middle() {
        let tiles = tiles(3, 3);

        assert_eq!(gap(Layout::Grid, &tiles, 10.0, 25.0), 0);
        assert_eq!(gap(Layout::Grid, &tiles, 60.0, 25.0), 1);
        assert_eq!(gap(Layout::Grid, &tiles, 120.0, 25.0), 1);
        assert_eq!(gap(Layout::Grid, &tiles, 170.0, 25.0), 2);
    }

    /// Right of the last tile of a line is the gap after it — drawn after that
    /// tile, where the pointer is, and not before the first tile of the next
    /// line, which is the same gap somewhere else entirely.
    #[test]
    fn test_right_of_a_line_is_the_gap_after_its_last_tile() {
        let tiles = tiles(5, 2);

        assert_eq!(
            drop_target(Layout::Grid, &tiles, 500.0, 25.0),
            Some(DropTarget { gap: 2, item: 1, after: true, column: false })
        );
        assert_eq!(
            drop_target(Layout::Grid, &tiles, 10.0, 85.0),
            Some(DropTarget { gap: 2, item: 2, after: false, column: false }),
            "the same gap from the start of the next line"
        );
    }

    /// Above everything is the start and below everything the end, wherever
    /// the pointer is sideways.
    #[test]
    fn test_outside_the_grid_is_its_start_or_its_end() {
        let tiles = tiles(5, 2);

        assert_eq!(gap(Layout::Grid, &tiles, 500.0, -40.0), 0);
        assert_eq!(gap(Layout::Grid, &tiles, 0.0, 900.0), 5);
    }

    /// The last line need not be full. Right of its only tile is the end of
    /// the list.
    #[test]
    fn test_a_half_full_last_line() {
        let tiles = tiles(5, 2);

        assert_eq!(gap(Layout::Grid, &tiles, 10.0, 145.0), 4, "before the lone tile");
        assert_eq!(gap(Layout::Grid, &tiles, 300.0, 145.0), 5, "and after it");
    }

    /// Between two lines, the nearer one decides.
    #[test]
    fn test_between_two_lines_the_nearer_one_decides() {
        let tiles = tiles(4, 2);

        // The first line ends at 50 and the second begins at 60.
        assert_eq!(gap(Layout::Grid, &tiles, 10.0, 52.0), 0);
        assert_eq!(gap(Layout::Grid, &tiles, 10.0, 58.0), 2);
    }

    /// One tile per line is a column, whatever the layout was meant to be —
    /// a phone. Read sideways, the end of the list would only be reachable
    /// from the right half of the last tile.
    #[test]
    fn test_one_tile_per_line_is_read_as_a_column() {
        let tiles = tiles(3, 1);

        assert_eq!(gap(Layout::Grid, &tiles, 10.0, 40.0), 1, "the lower half of the first");
        assert_eq!(gap(Layout::Grid, &tiles, 10.0, 140.0), 2, "the upper half of the last");
        assert_eq!(gap(Layout::Grid, &tiles, 10.0, 160.0), 3, "the lower half of the last");
    }

    /// A single tile is a line of one, and so is read as a column: the two
    /// gaps either side of it are its upper and its lower half.
    #[test]
    fn test_a_single_tile() {
        let tiles = tiles(1, 3);

        assert_eq!(gap(Layout::Grid, &tiles, 90.0, 10.0), 0);
        assert_eq!(gap(Layout::Grid, &tiles, 10.0, 40.0), 1);
    }

    /// Tiles of slightly different heights on one line are one line.
    #[test]
    fn test_ragged_tiles_on_one_line_stay_one_line() {
        let mut tiles = tiles(3, 3);
        tiles[1].top = 4.0;
        tiles[1].bottom = 60.0;

        assert_eq!(lines_of(&tiles), vec![0..3]);
    }

    /// A press is not a drag until it has travelled.
    #[test]
    fn test_a_press_that_has_not_moved_is_not_a_drag() {
        assert!(!is_a_drag((10.0, 100.0), (10.0, 100.0)), "a click that did not move");
        assert!(!is_a_drag((10.0, 100.0), (12.0, 102.0)), "a hand that is not quite steady");
        assert!(!is_a_drag((10.0, 100.0), (10.0, 96.0)), "and the same upwards");
    }

    /// Once it has travelled it is a drag, in any direction.
    #[test]
    fn test_a_press_that_has_moved_far_enough_is_a_drag() {
        assert!(is_a_drag((10.0, 100.0), (10.0, 100.0 + DRAG_THRESHOLD)));
        assert!(is_a_drag((10.0, 100.0), (10.0, 100.0 - DRAG_THRESHOLD)));
        assert!(is_a_drag((10.0, 100.0), (10.0 + DRAG_THRESHOLD, 100.0)), "sideways");
        assert!(is_a_drag((10.0, 100.0), (400.0, 400.0)));
    }

    /// Moving down means skipping the item below, because taking the item out
    /// first pulls every later gap up by one.
    #[test]
    fn test_moving_an_item_down_lands_it_where_it_was_dropped() {
        let mut items = vec!["a", "b", "c", "d"];

        assert_eq!(move_into_gap(&mut items, 0, 2), Some(1));
        assert_eq!(items, vec!["b", "a", "c", "d"]);
    }

    #[test]
    fn test_moving_an_item_up_lands_it_where_it_was_dropped() {
        let mut items = vec!["a", "b", "c", "d"];

        assert_eq!(move_into_gap(&mut items, 3, 1), Some(1));
        assert_eq!(items, vec!["a", "d", "b", "c"]);
    }

    #[test]
    fn test_an_item_can_be_moved_to_the_end() {
        let mut items = vec!["a", "b", "c"];

        assert_eq!(move_into_gap(&mut items, 0, 3), Some(2));
        assert_eq!(items, vec!["b", "c", "a"]);
    }

    /// Both gaps touching an item are where it already is.
    #[test]
    fn test_dropping_an_item_back_where_it_was_changes_nothing() {
        for gap in [1, 2] {
            let mut items = vec!["a", "b", "c"];
            assert_eq!(move_into_gap(&mut items, 1, gap), None);
            assert_eq!(items, vec!["a", "b", "c"]);
        }
    }

    /// The indices come from a pointer and from a list that anything else may
    /// have shortened in the meantime, so neither is trusted.
    #[test]
    fn test_an_impossible_move_is_refused_rather_than_panicking() {
        let mut items = vec!["a", "b"];

        assert_eq!(move_into_gap(&mut items, 5, 1), None);
        assert_eq!(move_into_gap(&mut items, 0, 9), None);
        assert_eq!(items, vec!["a", "b"]);
    }

    /// Every move of every list up to five long: the result is the same items,
    /// the moved one is where the move says, and the gaps touching an item are
    /// no move.
    #[test]
    fn test_every_move_is_a_permutation_that_lands_where_it_says() {
        for len in 1..=5 {
            for from in 0..len {
                for to in 0..=len {
                    let mut items: Vec<usize> = (0..len).collect();
                    let landed = move_into_gap(&mut items, from, to);

                    let mut sorted = items.clone();
                    sorted.sort();
                    assert_eq!(sorted, (0..len).collect::<Vec<_>>(), "{from} → {to} of {len}");

                    match landed {
                        Some(landed) => assert_eq!(items[landed], from, "{from} → {to} of {len}"),
                        None => {
                            assert!(to == from || to == from + 1, "{from} → {to} of {len} refused");
                            assert_eq!(items, (0..len).collect::<Vec<_>>());
                        }
                    }
                }
            }
        }
    }

    /// The renumbering agrees with what the move actually did, for every item
    /// of every list up to five long. This is the property every stored choice
    /// in the settings rests on.
    #[test]
    fn test_renumbering_agrees_with_the_move() {
        for len in 1..=5 {
            for from in 0..len {
                for to in 0..=len {
                    let mut items: Vec<usize> = (0..len).collect();
                    let Some(landed) = move_into_gap(&mut items, from, to) else {
                        continue;
                    };
                    for watched in 0..len {
                        let follows = index_after_move(watched, from, landed);
                        assert_eq!(
                            items[follows], watched,
                            "{watched} after moving {from} → {to} of {len}"
                        );
                    }
                }
            }
        }
    }

    /// The renumbering is a permutation, for any landing place.
    #[test]
    fn test_renumbering_is_a_permutation_of_the_list() {
        let length = 6;
        for from in 0..length {
            for landed in 0..length {
                let mut moved: Vec<usize> =
                    (0..length).map(|row| index_after_move(row, from, landed)).collect();
                moved.sort();
                assert_eq!(moved, (0..length).collect::<Vec<_>>());
            }
        }
    }

    /// One step up or down is the gap the keyboard and the arrow buttons use.
    #[test]
    fn test_one_step_is_one_place() {
        let mut items = vec!["a", "b", "c"];
        let gap = gap_for_step(3, 1, 1).unwrap_or(0);
        assert_eq!(move_into_gap(&mut items, 1, gap), Some(2));
        assert_eq!(items, vec!["a", "c", "b"]);

        let gap = gap_for_step(3, 2, -1).unwrap_or(0);
        assert_eq!(move_into_gap(&mut items, 2, gap), Some(1));
        assert_eq!(items, vec!["a", "b", "c"]);
    }

    /// Past either end there is no step to take.
    #[test]
    fn test_no_step_past_the_ends() {
        assert_eq!(gap_for_step(3, 0, -1), None);
        assert_eq!(gap_for_step(3, 2, 1), None);
        assert_eq!(gap_for_step(3, 1, 0), None);
        assert_eq!(gap_for_step(0, 0, 1), None);
    }

    /// Nothing scrolls while the pointer is away from the edges.
    #[test]
    fn test_no_scrolling_in_the_middle() {
        assert_eq!(auto_scroll_step(500.0, 0.0, 1000.0), 0.0);
        assert_eq!(auto_scroll_step(AUTO_SCROLL_BAND, 0.0, 1000.0), 0.0);
        assert_eq!(auto_scroll_step(1000.0 - AUTO_SCROLL_BAND, 0.0, 1000.0), 0.0);
    }

    /// Near the top it scrolls up, near the bottom down, and faster the
    /// nearer the edge.
    #[test]
    fn test_scrolling_grows_towards_the_edge() {
        let near_top = auto_scroll_step(40.0, 0.0, 1000.0);
        let at_top = auto_scroll_step(0.0, 0.0, 1000.0);
        assert!(near_top < 0.0 && at_top < near_top, "{near_top} {at_top}");

        let near_bottom = auto_scroll_step(960.0, 0.0, 1000.0);
        let at_bottom = auto_scroll_step(1000.0, 0.0, 1000.0);
        assert!(near_bottom > 0.0 && at_bottom > near_bottom, "{near_bottom} {at_bottom}");
    }

    /// Past the edge is as fast as it gets, not faster.
    #[test]
    fn test_scrolling_is_bounded() {
        assert_eq!(auto_scroll_step(-500.0, 0.0, 1000.0), -AUTO_SCROLL_MAX_STEP);
        assert_eq!(auto_scroll_step(5000.0, 0.0, 1000.0), AUTO_SCROLL_MAX_STEP);
    }

    /// An area with no room between its two bands never scrolls by itself.
    #[test]
    fn test_a_tiny_area_does_not_scroll() {
        assert_eq!(auto_scroll_step(10.0, 0.0, 80.0), 0.0);
    }
}
