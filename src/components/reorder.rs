//! Putting a list in order by dragging, with a mouse, a pen or a finger.
//!
//! Four lists in Cantara are put in order this way: the running order, the
//! source icons in the selection's sidebar, the presentation designs and the
//! slide settings. They share this one implementation, which grew out of the
//! running order's — see `docs/specs/0006-reorder-designs-and-slide-settings.md`.
//! The arithmetic (where a drop lands, what a move does to the positions) is in
//! [`crate::logic::reorder`], where it is tested without a page.
//!
//! # What is here and what is not
//!
//! The pointer mechanics are not here any more. Noticing that a press has
//! become a drag, capturing the pointer, measuring, scrolling at the edge and
//! following the pointer are in [`crate::components::drag`], because the
//! screen map of `docs/specs/0007-screen-map.md` needs exactly the same
//! handling and asks a different question with it. What is left here is what
//! makes a drag a *reordering*: reading the pointer as a gap between two rows,
//! drawing the marker there, and moving an item into it — by pointer, and by
//! keyboard.

use crate::components::drag::{Aim, PointerDrag, use_pointer_drag};
use crate::logic::reorder::{
    DropTarget, Layout, drop_target, gap_for_step, landing_position,
};
use dioxus::prelude::*;

pub use crate::components::drag::{GRIP_STYLE, refuse_native_drag};

/// The state of one list's drag, and what the list's elements call into.
///
/// Every field is a signal, so this is `Copy` and can be moved into as many
/// event handlers as the list needs.
#[derive(Clone, Copy, PartialEq)]
pub struct ReorderDrag {
    drag: PointerDrag<DropTarget>,
    len: CopyValue<usize>,
    on_move: Callback<(usize, usize)>,
    /// The item that has just been moved, marked so that the change can be
    /// followed to where it landed.
    landed: Signal<Option<usize>>,
}

/// A drag for a list of `len` items, laid out as `layout`, inside the element
/// with the id `container_id`. `on_move` is called with the item that was
/// moved and the gap it was dropped into — see [`crate::logic::reorder`] — and
/// is what actually changes the list.
pub fn use_reorder_drag(
    layout: Layout,
    container_id: &'static str,
    len: usize,
    on_move: impl FnMut((usize, usize)) + 'static,
) -> ReorderDrag {
    let on_move = use_callback(on_move);
    let mut stored_len = use_hook(|| CopyValue::new(len));
    // Not a signal: the length is read when a press starts, and writing a
    // signal while rendering would only render again.
    stored_len.set(len);

    let mut landed = use_signal(|| None);

    // The list's two answers to the shared drag: what the pointer is over —
    // the gap between two rows — and what dropping there does.
    let drag = use_pointer_drag(
        container_id,
        len,
        move |aim: Aim| drop_target(layout, &aim.zones, aim.x, aim.y),
        move |(from, target): (usize, DropTarget)| {
            let len = *stored_len.peek();
            if let Some(position) = landing_position(len, from, target.gap) {
                on_move.call((from, target.gap));
                landed.set(Some(position));
            }
        },
    );

    ReorderDrag {
        drag,
        len: stored_len,
        on_move,
        landed,
    }
}

impl ReorderDrag {
    /// Whether an item is being carried right now.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_dragging()
    }

    /// Remembers the element of item `index`, so that the list can be measured
    /// when a drag starts. For the item's `onmounted`.
    pub fn mounted(self, index: usize, event: Event<MountedData>) {
        self.drag.mounted(index, event);
    }

    /// A press on item `index`. For the item's `onpointerdown`, and for its
    /// grip's.
    ///
    /// A mouse or a pen may start a drag anywhere on the item. A finger may
    /// only where the item says it may — on the grip, or anywhere when
    /// `touch_allowed` is set — because claiming a touch is what turns it into
    /// a drag rather than a scroll.
    pub fn press(mut self, index: usize, event: &PointerData, touch_allowed: bool) {
        if self.drag.press(index, event, touch_allowed) {
            // Only once the press has been taken up: a second finger, or a
            // right-click, must not clear the mark on the item that last
            // moved.
            self.landed.set(None);
        }
    }

    /// The pointer moved. For the list's `onpointermove`.
    pub fn moved(self, event: &PointerData) {
        self.drag.moved(event);
    }

    /// The pointer was let go. For the list's `onpointerup`.
    pub fn released(self) {
        self.drag.released();
    }

    /// The system took the pointer away — a phone call, a gesture the browser
    /// claimed — or Escape was pressed. Nothing was dropped, so nothing moves.
    pub fn cancelled(self) {
        self.drag.cancelled();
    }

    /// The pointer came back into the list. For the list's `onpointerenter`.
    ///
    /// Leaving the list does not end a drag: one quick enough to be worth
    /// making leaves it constantly. What does end it is coming back with the
    /// button already let go, which is the only sign available that the
    /// release happened out there.
    pub fn entered(self, event: &PointerData) {
        self.drag.entered(event);
    }

    /// Whether a click on an item is really the end of a drag, and so not a
    /// click at all. For the item's `onclick`, before anything else it does.
    pub fn swallows_click(self) -> bool {
        self.drag.swallows_click()
    }

    /// Moves item `index` by `offset` places — from the keyboard, or from an
    /// arrow button — and keeps the focus on it where it lands.
    pub fn step(mut self, index: usize, offset: isize) {
        let len = *self.len.peek();
        let Some(gap) = gap_for_step(len, index, offset) else {
            return;
        };
        let Some(landed) = landing_position(len, index, gap) else {
            return;
        };
        self.on_move.call((index, gap));
        self.landed.set(Some(landed));

        // The elements are numbered by their position, so the one that had
        // the focus now shows a different item. The focus goes with the item.
        if let Some(handle) = self.drag.handle(landed) {
            spawn(async move {
                let _ = handle.set_focus(true).await;
            });
        }
    }

    /// The keyboard on item `index`.
    ///
    /// Alt and an arrow moves it, which keeps the arrows on their own free for
    /// whatever the list does with them. On a grip, which is there for nothing
    /// else, the arrows alone are enough. Escape drops nothing.
    fn key(self, index: usize, event: &KeyboardData, on_grip: bool) -> bool {
        if event.key() == Key::Escape && self.drag.escape() {
            return true;
        }
        if !on_grip && !event.modifiers().alt() {
            return false;
        }
        let offset = match event.key() {
            Key::ArrowUp | Key::ArrowLeft => -1,
            Key::ArrowDown | Key::ArrowRight => 1,
            _ => return false,
        };
        self.step(index, offset);
        true
    }

    /// The keyboard on item `index`, as an event handler: does what
    /// [`Self::key`] does, and keeps what it handled to the item. For the
    /// item's `onkeydown`.
    ///
    /// Kept to the item, because the selection view sends every key that
    /// reaches it to the search field — typing anywhere looks a song up. That
    /// is right for a letter, and wrong for the keys a list item answers to:
    /// the Alt of an Alt-and-arrow arrived there first and took the focus
    /// away, so the arrow that followed found no row to move. A modifier on
    /// its own is the start of a shortcut on the item, not typing.
    pub fn keydown(self, index: usize, event: &Event<KeyboardData>, on_grip: bool) -> bool {
        let handled = self.key(index, &event.data(), on_grip);
        if handled {
            event.prevent_default();
        }
        if handled
            || matches!(
                event.key(),
                Key::Alt | Key::Control | Key::Shift | Key::Meta | Key::AltGraph
            )
        {
            event.stop_propagation();
        }
        handled
    }

    /// The classes item `index` carries: whether it is being carried, whether
    /// it has just landed, and whether the drop marker is drawn against it.
    pub fn item_class(&self, index: usize) -> String {
        let mut class = String::from("reorder-item");
        if self.drag.carried() == Some(index) {
            class.push_str(" reorder-dragging");
        } else if (self.landed)() == Some(index) {
            class.push_str(" reorder-landed");
        }
        if let Some(target) = self.drag.target()
            && target.item == index
            && self.drag.is_dragging()
        {
            class.push_str(match (target.column, target.after) {
                (true, false) => " reorder-marker-above",
                (true, true) => " reorder-marker-below",
                (false, false) => " reorder-marker-left",
                (false, true) => " reorder-marker-right",
            });
        }
        class
    }

    /// The inline style item `index` carries: the carried one follows the
    /// pointer. A transform, so that nothing around it reflows and the
    /// measurements taken at the start stay true.
    ///
    /// Every other item says `transform: none` outright rather than nothing.
    /// Dioxus *merges* a new `style` into the old one: a property the new
    /// value does not name is put back from the old (`set_attribute.ts` in
    /// `dioxus-interpreter-js`). A dropped item whose style simply left the
    /// transform out kept the last one it was given — and stayed hanging where
    /// the pointer had let go of it, beside its new place in the list.
    pub fn item_style(&self, index: usize) -> String {
        const AT_REST: &str = "transform: none;";
        if self.drag.carried() != Some(index) {
            return AT_REST.to_string();
        }
        match self.drag.carry_offset() {
            Some((dx, dy)) => format!("transform: translate({dx}px, {dy}px);"),
            None => AT_REST.to_string(),
        }
    }

    /// The class of the list itself while a drag is under way.
    pub fn container_class(&self) -> &'static str {
        if self.is_dragging() { "reorder-container reorder-active" } else { "reorder-container" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gap a keyboard step uses is the one the move expects.
    #[test]
    fn test_a_step_down_lands_one_place_further() {
        let gap = gap_for_step(4, 1, 1);
        assert_eq!(gap.and_then(|gap| landing_position(4, 1, gap)), Some(2));
    }
}
