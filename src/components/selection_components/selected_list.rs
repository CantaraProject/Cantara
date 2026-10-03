//! The running order: what has been picked for the presentation, in the order
//! it will be shown, and the ways of changing that order.
//!
//! The rows are dragged the way every list in Cantara is — with a mouse or a
//! pen anywhere on the row, with a finger by the grip — see
//! [`crate::components::reorder`], which began here.

use crate::components::reorder::{GRIP_STYLE, ReorderDrag, refuse_native_drag, use_reorder_drag};
use crate::components::shared_components::{ImageIcon, MarkdownIcon, MusicIcon, PdfIcon, VideoIcon};
use crate::logic::reorder::{Layout, index_after_move, move_into_gap};
use crate::logic::sourcefiles::SourceFileType;
use crate::logic::states::SelectedItemRepresentation;
use dioxus::prelude::*;
use dioxus_free_icons::Icon;
use dioxus_free_icons::icons::fa_regular_icons::FaTrashCan;
use dioxus_free_icons::icons::fa_solid_icons::{FaArrowDown, FaArrowUp, FaGripVertical};
use rust_i18n::t;

/// The `id` of the list, which the drag finds its scrolling area from.
const LIST_ID: &str = "selected-items-list";

#[component]
pub(crate) fn SelectedItems(
    selected_items: Signal<Vec<SelectedItemRepresentation>>,
    active_selected_item_id: Signal<Option<usize>>,
) -> Element {
    let len = selected_items.read().len();
    let drag = use_reorder_drag(Layout::Column, LIST_ID, len, move |(from, to)| {
        let Some(landed) = move_into_gap(&mut selected_items.write(), from, to) else {
            return;
        };
        // The rows are numbered by their position, so a move renumbers every
        // row it passed. Whatever is open in the options beside the list has
        // to be renumbered with them, or the panel silently changes to a
        // different item.
        let active = *active_selected_item_id.peek();
        if let Some(active) = active {
            let follows = index_after_move(active, from, landed);
            if follows != active {
                active_selected_item_id.set(Some(follows));
            }
        }
    });

    rsx! {
        div {
            id: LIST_ID,
            class: "selected-container {drag.container_class()}",
            // `move` and `up` are taken here rather than on the row: a touch
            // captures to the row it began on, so the rows it travels over see
            // nothing, while everything bubbles to this element.
            onpointermove: move |event: Event<PointerData>| drag.moved(&event.data()),
            onpointerup: move |_| drag.released(),
            onpointercancel: move |_| drag.cancelled(),
            onpointerenter: move |event: Event<PointerData>| drag.entered(&event.data()),
            ondragstart: refuse_native_drag,

            for number in 0..len {
                SelectedItem {
                    key: "{number}",
                    selected_items,
                    id: number,
                    active_selected_item_id,
                    drag,
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
    drag: ReorderDrag,
) -> Element {
    let current_item = selected_items.read().get(id).cloned();
    let Some(current_item) = current_item else {
        return rsx! {};
    };

    let is_first = id == 0;
    let is_last = id + 1 >= selected_items.read().len();

    // Which row is open in the options beside the list is worth saying: the
    // panel changes under the reader when another row is picked, and without a
    // mark on the row there is nothing tying the two together. Drawn the way
    // the library list marks its open item.
    let active = if active_selected_item_id() == Some(id) { " selection_item-active" } else { "" };

    rsx! {
        div {
            role: "button",
            class: "outline secondary selection_item selected-item{active} {drag.item_class(id)}",
            style: drag.item_style(id),
            tabindex: 0,
            onmounted: move |event: Event<MountedData>| drag.mounted(id, event),
            onclick: move |_| {
                if !drag.swallows_click() {
                    active_selected_item_id.set(Some(id));
                }
            },
            // A mouse or a pen may start a drag anywhere on the row, the way it
            // always could. A finger may not: claiming a touch is what turns it
            // into a drag rather than a scroll, and a list that claimed every
            // touch on it could not be scrolled at all. That is what the grip
            // below is for, and why it is the only part of the row with
            // `touch-action: none` on it.
            onpointerdown: move |event: Event<PointerData>| drag.press(id, &event.data(), false),
            // The order can be changed without a pointer at all. Alt keeps it
            // clear of the arrow keys the list itself uses to move between
            // rows, and is what a sortable list is expected to answer to.
            onkeydown: move |event: Event<KeyboardData>| {
                drag.keydown(id, &event, false);
            },

            // The grip: the part of the row a *finger* can drag it by.
            //
            // Hidden where there is room for a pointer — see
            // `.selected-item-grip` in `assets/main.css`. On a wide screen the
            // whole row is draggable and the grip would only be one more thing
            // in a row that already has three buttons in it.
            span {
                class: "selected-item-grip reorder-grip",
                style: GRIP_STYLE,
                // Reachable with the keyboard, but *not* `role="button"`: Pico
                // draws those as filled buttons, and this is a grip, not a
                // control the row is about.
                tabindex: 0,
                aria_label: t!("selection.reorder_handle").to_string(),
                title: t!("selection.reorder_hint").to_string(),
                onpointerdown: move |event: Event<PointerData>| {
                    // The row's own handler would otherwise start the same drag
                    // a second time.
                    event.stop_propagation();
                    drag.press(id, &event.data(), true);
                },
                onkeydown: move |event: Event<KeyboardData>| {
                    drag.keydown(id, &event, true);
                },
                Icon { icon: FaGripVertical }
            }

            span { class: "selected-item-label",
                // The icon sits in a slot of its own rather than directly in
                // the row. Without one it is a flex item like any other: a long
                // name squeezed it narrower than the icon on the row above, and
                // a name that wrapped onto two lines put it halfway down the
                // block — which is what left the column of icons ragged instead
                // of straight. See `.selected-item-type` in `assets/main.css`.
                span { class: "selected-item-type",
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
                        SourceFileType::Video => rsx! {
                            VideoIcon {}
                        },
                        _ => rsx! {},
                    }
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
                        onclick: move |_| drag.step(id, -1),
                        Icon { icon: FaArrowUp }
                    }
                }
                if is_last {
                    span { class: "selected-item-action selected-item-action-empty" }
                } else {
                    span {
                        class: "selected-item-action",
                        title: t!("selection.move_down").to_string(),
                        onclick: move |_| drag.step(id, 1),
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
