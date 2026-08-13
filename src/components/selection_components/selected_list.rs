//! The running order: what has been picked for the presentation, in the order
//! it will be shown, and the ways of changing that order.
//!
//! # Why pointer events
//!
//! This list used to be driven by `onmousedown` / `onmouseenter` / `onmouseup`.
//! That works with a mouse and with nothing else: a touch produces at best a
//! late and partial emulation of those events, so on a phone or a tablet — two
//! of the four targets Cantara is built for — the order could only be changed
//! with the arrow buttons on each row. It also cancelled the drag whenever the
//! pointer left the list, which a quick drag does all the time.
//!
//! Pointer events are one set of events for mouse, pen and touch. Two things
//! about them shape the code below:
//!
//! * A touch **implicitly captures** to the element it started on, so every
//!   move and the release itself are delivered there and bubble up to the
//!   container — which is where they are handled. A mouse has no capture
//!   without reaching into the DOM, but a mouse is over the element it is
//!   moving across anyway, so the same handler sees both.
//! * A touch that is not claimed scrolls the page. Claiming it is
//!   `touch-action: none`, and that is put on the grip alone: a list long
//!   enough to scroll must still scroll under the finger everywhere else.
//!
//! Which row the pointer is over is worked out from the coordinate rather than
//! from `onpointerenter` on each row, because under an implicit capture the
//! sibling rows see no events at all. The rows are measured once when the drag
//! starts; nothing moves during it, so once is enough.

use crate::components::shared_components::{ImageIcon, MarkdownIcon, MusicIcon, PdfIcon};
use crate::logic::sourcefiles::SourceFileType;
use crate::logic::states::SelectedItemRepresentation;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::{FaArrowDown, FaArrowUp, FaGripVertical};
use rust_i18n::t;
use std::rc::Rc;

/// Where a row is on the screen, as it was when the drag started.
///
/// Only the vertical extent matters: the list is a column, so where a drop
/// would land is decided by the pointer's Y alone.
#[derive(Clone, Copy, PartialEq, Debug)]
struct RowExtent {
    top: f64,
    bottom: f64,
}

/// The gap a drop would go into, given where the pointer is.
///
/// Gaps are counted the way an insertion index is: `0` is above the first row
/// and `rows.len()` is below the last. A row's own midpoint divides it — above
/// it the drop goes before the row, below it after.
fn drop_index(rows: &[RowExtent], pointer_y: f64) -> usize {
    for (index, row) in rows.iter().enumerate() {
        let middle = (row.top + row.bottom) / 2.0;
        if pointer_y < middle {
            return index;
        }
    }
    rows.len()
}

/// Moves the item at `from` into the gap `to`, and says where it ended up.
///
/// `to` counts gaps, so moving an item down by one means `to == from + 2`: the
/// gap below the row after it. Removing the item first shifts every gap below
/// it up by one, which is the correction here. A move onto either of its own
/// two gaps is no move at all.
fn reorder<T>(items: &mut Vec<T>, from: usize, to: usize) -> Option<usize> {
    if from >= items.len() || to > items.len() {
        return None;
    }
    if to == from || to == from + 1 {
        return None;
    }

    let item = items.remove(from);
    let insert_at = if to > from { to - 1 } else { to };
    items.insert(insert_at, item);
    Some(insert_at)
}

#[component]
pub(crate) fn SelectedItems(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_selected_item_id: Signal<Option<usize>>,
) -> Element {
    // Which row is being dragged, and which gap it would drop into.
    let mut dragging_from: Signal<Option<usize>> = use_signal(|| None);
    let mut drop_at: Signal<Option<usize>> = use_signal(|| None);
    // Highlights the row that has just been moved, so a change made by drag or
    // by keyboard can be seen where it landed.
    let mut anim_target: Signal<Option<usize>> = use_signal(|| None);

    // The handle of every row, so the list can be measured when a drag starts.
    let row_handles: Signal<Vec<Option<Rc<MountedData>>>> = use_signal(Vec::new);
    // Where the rows were when it did.
    let mut row_extents: Signal<Vec<RowExtent>> = use_signal(Vec::new);

    let mut end_drag = move || {
        dragging_from.set(None);
        drop_at.set(None);
        row_extents.set(Vec::new());
    };

    let mut commit_drag = move || {
        if let (Some(from), Some(to)) = (dragging_from(), drop_at())
            && let Some(landed) = reorder(&mut selected_items.write(), from, to)
        {
            anim_target.set(Some(landed));
        }
        end_drag();
    };

    rsx! {
        div {
            class: "selected-container",
            // `move` and `up` are taken here rather than on the row: a touch
            // captures to the row it began on, so the rows it travels over see
            // nothing, while everything bubbles to this element.
            onpointermove: move |event: Event<PointerData>| {
                if dragging_from().is_none() {
                    return;
                }
                let pointer_y = event.data().coordinates().client().y;
                let extents = row_extents.read();
                if extents.is_empty() {
                    return;
                }
                let gap = drop_index(&extents, pointer_y);
                if drop_at() != Some(gap) {
                    drop_at.set(Some(gap));
                }
            },
            onpointerup: move |_| commit_drag(),
            // The system took the pointer away — a phone call, a gesture the
            // browser claimed. Nothing was dropped, so nothing moves.
            onpointercancel: move |_| end_drag(),
            // Leaving the list no longer cancels: a drag quick enough to be
            // worth making leaves it constantly, and the drop used to be lost.
            // What does end it is coming back with the button already let go,
            // which is the only sign available that the release happened out
            // there.
            onpointerenter: move |event: Event<PointerData>| {
                if dragging_from().is_some() && event.data().held_buttons().is_empty() {
                    end_drag();
                }
            },

            for (number , _) in selected_items.read().iter().enumerate() {
                SelectedItem {
                    key: "{number}",
                    selected_items,
                    id: number,
                    active_selected_item_id,
                    dragging_from,
                    drop_at,
                    anim_target,
                    row_handles,
                    row_extents,
                }
            }

            // The gap below the last row. It only exists while something is
            // being dragged, so the list does not carry a dashed line about
            // with it the rest of the time.
            if dragging_from().is_some() {
                div {
                    class: if drop_at() == Some(selected_items.read().len()) {
                        "selected-drop-gap selected-drop-gap-active"
                    } else {
                        "selected-drop-gap"
                    },
                }
            }
        }
    }
}

#[component]
fn SelectedItem(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    id: usize,
    active_selected_item_id: Signal<Option<usize>>,
    dragging_from: Signal<Option<usize>>,
    drop_at: Signal<Option<usize>>,
    anim_target: Signal<Option<usize>>,
    row_handles: Signal<Vec<Option<Rc<MountedData>>>>,
    row_extents: Signal<Vec<RowExtent>>,
) -> Element {
    let current_item = selected_items.read().get(id).cloned();
    let Some(current_item) = current_item else {
        return rsx! {};
    };

    let is_first = id == 0;
    let is_last = id + 1 >= selected_items.read().len();

    // Moves this row by one, from a button or from the keyboard, and marks
    // where it went so the change can be followed.
    let mut move_by = move |offset: isize| {
        let target = id as isize + offset;
        let mut items = selected_items.write();
        if target < 0 || target as usize >= items.len() {
            return;
        }
        let target = target as usize;
        items.swap(id, target);
        drop(items);
        anim_target.set(Some(target));
        // The selection follows the row it is on, or it would suddenly be on a
        // different song than the one that was just moved.
        if *active_selected_item_id.read() == Some(id) {
            active_selected_item_id.set(Some(target));
        }
    };

    let mut row_handle: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    rsx! {
        // The gap above this row, which is where a drop lands when the pointer
        // is in the upper half of it.
        if dragging_from().is_some() {
            div {
                class: if drop_at() == Some(id) {
                    "selected-drop-gap selected-drop-gap-active"
                } else {
                    "selected-drop-gap"
                },
            }
        }

        div {
            role: "button",
            class: if anim_target() == Some(id) {
                "outline secondary selection_item selected-item selected-item-moved"
            } else if dragging_from() == Some(id) {
                "outline secondary selection_item selected-item selected-item-dragging"
            } else {
                "outline secondary selection_item selected-item"
            },
            tabindex: 0,
            onmounted: move |event: Event<MountedData>| {
                let handle = event.data();
                row_handle.set(Some(handle.clone()));
                let mut handles = row_handles.write();
                if handles.len() <= id {
                    handles.resize(id + 1, None);
                }
                handles[id] = Some(handle);
            },
            onclick: move |_| { active_selected_item_id.set(Some(id)) },
            // The order can be changed without a pointer at all. Alt keeps it
            // clear of the arrow keys the list itself uses to move between
            // rows, and is what a sortable list is expected to answer to.
            onkeydown: move |event: Event<KeyboardData>| {
                if !event.modifiers().alt() {
                    return;
                }
                match event.key() {
                    Key::ArrowUp => {
                        event.prevent_default();
                        move_by(-1);
                    }
                    Key::ArrowDown => {
                        event.prevent_default();
                        move_by(1);
                    }
                    _ => {}
                }
            },

            // The grip, and the only part of the row a drag starts on.
            //
            // Not the whole row, because claiming a touch is what makes it a
            // drag instead of a scroll, and a list claiming every touch on it
            // could not be scrolled. `touch-action: none` sits on this element
            // alone — see `.selected-item-grip` in `assets/main.css`.
            span {
                class: "selected-item-grip",
                // Reachable with the keyboard, but *not* `role="button"`: Pico
                // draws those as filled buttons, and this is a grip, not a
                // control the row is about.
                tabindex: 0,
                aria_label: t!("selection.reorder_handle").to_string(),
                title: t!("selection.reorder_hint").to_string(),
                onpointerdown: move |event: Event<PointerData>| async move {
                    // A right-click or a second finger is not a drag.
                    if !event.data().is_primary() {
                        return;
                    }
                    // Starting a drag must not also open the row.
                    event.stop_propagation();

                    anim_target.set(None);
                    dragging_from.set(Some(id));
                    drop_at.set(Some(id));

                    // Measure the list once, now: the rows do not move while
                    // the drag runs, and asking for a rectangle costs a round
                    // trip to the renderer for every row.
                    let handles: Vec<Option<Rc<MountedData>>> = row_handles.read().clone();
                    let mut extents: Vec<RowExtent> = Vec::with_capacity(handles.len());
                    for handle in handles.iter() {
                        let Some(handle) = handle else {
                            // A row that could not be measured must not shift
                            // every row after it, so the whole drag is given up
                            // rather than dropping somewhere that was never
                            // pointed at.
                            extents.clear();
                            break;
                        };
                        match handle.get_client_rect().await {
                            Ok(rect) => extents.push(RowExtent {
                                top: rect.min_y(),
                                bottom: rect.max_y(),
                            }),
                            Err(_) => {
                                extents.clear();
                                break;
                            }
                        }
                    }

                    if extents.is_empty() {
                        dragging_from.set(None);
                        drop_at.set(None);
                    }
                    row_extents.set(extents);
                },
                onkeydown: move |event: Event<KeyboardData>| {
                    match event.key() {
                        Key::ArrowUp => {
                            event.prevent_default();
                            move_by(-1);
                        }
                        Key::ArrowDown => {
                            event.prevent_default();
                            move_by(1);
                        }
                        _ => {}
                    }
                },
                Icon { icon: FaGripVertical }
            }

            span { class: "selected-item-label",
                match current_item.source_file.file_type {
                    SourceFileType::Song => rsx! {
                        MusicIcon {}
                    },
                    SourceFileType::Image => rsx! {
                        ImageIcon {}
                    },
                    SourceFileType::Pdf => rsx! {
                        PdfIcon {}
                    },
                    SourceFileType::Markdown => rsx! {
                        MarkdownIcon {}
                    },
                    _ => rsx! {},
                }
                span { class: "selected-item-name", {current_item.source_file.name.clone()} }
            }

            // Every row keeps all three slots, whether or not it can use them:
            // the first row has nothing to move up and the last nothing to move
            // down, and leaving those buttons out shifted the remaining ones
            // sideways so that no column of icons lined up with the next. The
            // group never wraps either — a name long enough to take two lines
            // used to push the wastebasket onto a line of its own.
            span {
                class: "selected-item-actions",
                // A click on one of these acts on the row; it must not also
                // select it, and pressing one must not start a drag.
                onpointerdown: move |event: Event<PointerData>| event.stop_propagation(),
                onclick: move |event: Event<MouseData>| event.stop_propagation(),
                // Plain spans, deliberately: Pico draws anything carrying
                // `role="button"` as a filled button, which turned this row of
                // small icons into a row of solid blocks.
                if is_first {
                    span { class: "selected-item-action selected-item-action-empty" }
                } else {
                    span {
                        class: "selected-item-action",
                        title: t!("selection.move_up").to_string(),
                        onclick: move |_| move_by(-1),
                        Icon { icon: FaArrowUp }
                    }
                }
                if is_last {
                    span { class: "selected-item-action selected-item-action-empty" }
                } else {
                    span {
                        class: "selected-item-action",
                        title: t!("selection.move_down").to_string(),
                        onclick: move |_| move_by(1),
                        Icon { icon: FaArrowDown }
                    }
                }
                span {
                    class: "selected-item-action",
                    title: t!("general.delete").to_string(),
                    onclick: move |_| {
                        if *active_selected_item_id.read() == Some(id) {
                            active_selected_item_id.set(None);
                        }
                        selected_items.write().remove(id);
                    },
                    Icon { icon: FaTrashCan }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(count: usize) -> Vec<RowExtent> {
        (0..count)
            .map(|index| RowExtent {
                top: index as f64 * 20.0,
                bottom: index as f64 * 20.0 + 20.0,
            })
            .collect()
    }

    /// The upper half of a row means "before it", the lower half "after it".
    /// That is what makes a drop land where the dashed line is drawn rather
    /// than a row away from it.
    #[test]
    fn test_the_midpoint_of_a_row_divides_it() {
        let rows = rows(3);

        assert_eq!(drop_index(&rows, 1.0), 0, "the top of the first row");
        assert_eq!(drop_index(&rows, 9.0), 0, "still its upper half");
        assert_eq!(drop_index(&rows, 11.0), 1, "its lower half is the gap after");
        assert_eq!(drop_index(&rows, 29.0), 1, "the upper half of the second");
        assert_eq!(drop_index(&rows, 31.0), 2, "and its lower half");
    }

    /// Past the last row is the gap at the end, not the last gap before it —
    /// otherwise nothing could be moved to the bottom of the list.
    #[test]
    fn test_below_the_last_row_is_the_end_of_the_list() {
        assert_eq!(drop_index(&rows(3), 500.0), 3);
    }

    /// A pointer above the list belongs at its start. A drag that begins on a
    /// row and travels upwards out of the list reports coordinates above it.
    #[test]
    fn test_above_the_list_is_its_start() {
        assert_eq!(drop_index(&rows(3), -100.0), 0);
    }

    /// An empty list has one gap and it is the only answer; asking must not
    /// index into nothing.
    #[test]
    fn test_an_empty_list_has_only_the_one_gap() {
        assert_eq!(drop_index(&[], 42.0), 0);
    }

    /// Moving down means skipping the row below, because taking the item out
    /// first pulls every later gap up by one. This is the arithmetic the old
    /// implementation got right and the one a rewrite is most likely to break.
    #[test]
    fn test_moving_an_item_down_lands_it_where_it_was_dropped() {
        let mut items = vec!["a", "b", "c", "d"];

        assert_eq!(reorder(&mut items, 0, 2), Some(1));
        assert_eq!(items, vec!["b", "a", "c", "d"]);
    }

    #[test]
    fn test_moving_an_item_up_lands_it_where_it_was_dropped() {
        let mut items = vec!["a", "b", "c", "d"];

        assert_eq!(reorder(&mut items, 3, 1), Some(1));
        assert_eq!(items, vec!["a", "d", "b", "c"]);
    }

    #[test]
    fn test_an_item_can_be_moved_to_the_end() {
        let mut items = vec!["a", "b", "c"];

        assert_eq!(reorder(&mut items, 0, 3), Some(2));
        assert_eq!(items, vec!["b", "c", "a"]);
    }

    /// Both gaps touching a row are where it already is. Treating either as a
    /// move would report a change that did not happen and flash the row for it.
    #[test]
    fn test_dropping_an_item_back_where_it_was_changes_nothing() {
        for gap in [1, 2] {
            let mut items = vec!["a", "b", "c"];
            assert_eq!(reorder(&mut items, 1, gap), None);
            assert_eq!(items, vec!["a", "b", "c"]);
        }
    }

    /// The indices come from a pointer and from a list that anything else may
    /// have shortened in the meantime, so neither is trusted.
    #[test]
    fn test_an_impossible_move_is_refused_rather_than_panicking() {
        let mut items = vec!["a", "b"];

        assert_eq!(reorder(&mut items, 5, 1), None);
        assert_eq!(reorder(&mut items, 0, 9), None);
        assert_eq!(items, vec!["a", "b"]);
    }
}
