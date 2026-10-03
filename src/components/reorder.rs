//! Putting a list in order by dragging, with a mouse, a pen or a finger.
//!
//! Four lists in Cantara are put in order this way: the running order, the
//! source icons in the selection's sidebar, the presentation designs and the
//! slide settings. They share this one implementation, which grew out of the
//! running order's — see `docs/specs/0006-reorder-designs-and-slide-settings.md`.
//! The arithmetic (where a drop lands, what a move does to the positions) is in
//! [`crate::logic::reorder`], where it is tested without a page.
//!
//! # Why pointer events
//!
//! The running order used to be driven by `onmousedown` / `onmouseenter` /
//! `onmouseup`. That works with a mouse and with nothing else: a touch produces
//! at best a late and partial emulation of those events, so on a phone or a
//! tablet — two of the four targets Cantara is built for — nothing could be
//! dragged at all. Pointer events are one set of events for mouse, pen and
//! touch. Three things about them shape the code below:
//!
//! * A touch **implicitly captures** to the element it started on, so every
//!   move and the release itself are delivered there and bubble up to the list
//!   — which is where they are handled. A mouse is captured explicitly, once
//!   the press has become a drag, so that it can leave the list (to scroll the
//!   page at its edge) without the list losing sight of it.
//! * A touch that is not claimed scrolls the page. Claiming it is
//!   `touch-action: none`, and that is put on a grip only, so that a list long
//!   enough to scroll still scrolls under the finger everywhere else. A list
//!   too short to need scrolling — the sidebar — may claim the whole item.
//! * Which item the pointer is over is worked out from the coordinate rather
//!   than from events on the items, because under a capture the other items
//!   see no events at all. The items are measured once when the press starts;
//!   nothing reflows during a drag — the marker is drawn without taking space,
//!   and the carried item is moved with a transform — so once is enough. When
//!   the page scrolls under the drag, the distance scrolled is added to the
//!   pointer instead of measuring again.

use crate::logic::reorder::{
    DropTarget, ItemExtent, Layout, auto_scroll_step, drop_target, gap_for_step, is_a_drag,
    landing_position,
};
use dioxus::prelude::*;
use std::rc::Rc;
use std::time::Duration;

/// A press on an item that has not become a drag yet.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Pending {
    item: usize,
    /// Where the pointer went down, to measure the distance from.
    start: (f64, f64),
    pointer_id: i32,
    /// Which press this is, so that a measurement still under way for an
    /// earlier one is not mistaken for this one's.
    generation: u64,
}

/// The state of one list's drag, and what the list's elements call into.
///
/// Every field is a signal, so this is `Copy` and can be moved into as many
/// event handlers as the list needs.
#[derive(Clone, Copy, PartialEq)]
pub struct ReorderDrag {
    layout: Layout,
    /// The `id` of the element holding the items. It is what the scrolling
    /// area is found from, and what a mouse is captured to.
    container_id: &'static str,
    len: CopyValue<usize>,
    on_move: Callback<(usize, usize)>,

    pending: Signal<Option<Pending>>,
    dragging: Signal<Option<usize>>,
    target: Signal<Option<DropTarget>>,
    /// The item that has just been moved, marked so that the change can be
    /// followed to where it landed.
    landed: Signal<Option<usize>>,

    handles: Signal<Vec<Option<Rc<MountedData>>>>,
    extents: Signal<Vec<ItemExtent>>,
    /// The pointer, in the viewport's coordinates.
    pointer: Signal<(f64, f64)>,
    /// How far the scrolling area has moved since the items were measured.
    scrolled: Signal<f64>,
    /// Where the scrolling area is on screen: its top and bottom.
    area: Signal<Option<(f64, f64)>>,
    scrolling: Signal<bool>,
    generation: Signal<u64>,
    /// Set when a drag ends, so that the click a browser delivers after a
    /// press-and-release is not taken for a click on the item.
    swallow_click: Signal<bool>,
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

    ReorderDrag {
        layout,
        container_id,
        len: stored_len,
        on_move,
        pending: use_signal(|| None),
        dragging: use_signal(|| None),
        target: use_signal(|| None),
        landed: use_signal(|| None),
        handles: use_signal(Vec::new),
        extents: use_signal(Vec::new),
        pointer: use_signal(|| (0.0, 0.0)),
        scrolled: use_signal(|| 0.0),
        area: use_signal(|| None),
        scrolling: use_signal(|| false),
        generation: use_signal(|| 0),
        swallow_click: use_signal(|| false),
    }
}

impl ReorderDrag {
    /// Whether something is being carried right now.
    pub fn is_dragging(&self) -> bool {
        (self.dragging)().is_some()
    }

    /// Remembers the element of item `index`, so that the list can be measured
    /// when a drag starts. For the item's `onmounted`.
    pub fn mounted(mut self, index: usize, event: Event<MountedData>) {
        let mut handles = self.handles.write();
        if handles.len() <= index {
            handles.resize(index + 1, None);
        }
        handles[index] = Some(event.data());
    }

    /// A press on item `index`. For the item's `onpointerdown`, and for its
    /// grip's.
    ///
    /// A mouse or a pen may start a drag anywhere on the item. A finger may
    /// only where the item says it may — on the grip, or anywhere when
    /// `touch_anywhere` is set — because claiming a touch is what turns it
    /// into a drag rather than a scroll.
    pub fn press(mut self, index: usize, event: &PointerData, touch_allowed: bool) {
        // A second finger or a right-click is not a drag.
        if !event.is_primary()
            || event
                .trigger_button()
                .is_some_and(|button| button != dioxus::html::input_data::MouseButton::Primary)
        {
            return;
        }
        if event.pointer_type() == "touch" && !touch_allowed {
            return;
        }

        self.swallow_click.set(false);
        self.landed.set(None);
        let generation = *self.generation.peek() + 1;
        self.generation.set(generation);

        let client = event.coordinates().client();
        let pending = Pending {
            item: index,
            start: (client.x, client.y),
            pointer_id: event.pointer_id(),
            generation,
        };
        self.pending.set(Some(pending));
        self.pointer.set(pending.start);
        self.scrolled.set(0.0);

        // Measured now rather than when the drag starts, because asking the
        // renderer for a rectangle costs a round trip per item and the answer
        // has to be ready by the first move. Nothing shifts in between, so
        // measuring early is measuring correctly.
        spawn(async move {
            let measured = self.measure().await;
            let area = scrolling_area(self.container_id).await;

            // Another press has started since — this measurement is not its.
            if self.pending.peek().as_ref().map(|pending| pending.generation) != Some(generation) {
                return;
            }
            // An item that cannot be measured gives the whole drag up: a
            // missing one would shift every item after it, and the drop would
            // land somewhere that was never pointed at.
            match measured {
                Some(extents) => {
                    self.extents.set(extents);
                    self.area.set(area);
                    // A quick hand may already be carrying the item: the
                    // moves before now had nothing to aim at.
                    if self.dragging.peek().is_some() {
                        self.aim();
                    }
                }
                None => self.end(),
            }
        });
    }

    async fn measure(&self) -> Option<Vec<ItemExtent>> {
        let len = *self.len.peek();
        let handles: Vec<Option<Rc<MountedData>>> =
            self.handles.peek().iter().take(len).cloned().collect();
        if handles.len() < len {
            return None;
        }

        let mut extents = Vec::with_capacity(len);
        for handle in handles {
            let rect = handle?.get_client_rect().await.ok()?;
            extents.push(ItemExtent {
                left: rect.min_x(),
                top: rect.min_y(),
                right: rect.max_x(),
                bottom: rect.max_y(),
            });
        }
        Some(extents)
    }

    /// The pointer moved. For the list's `onpointermove`.
    pub fn moved(mut self, event: &PointerData) {
        let client = event.coordinates().client();
        let pointer = (client.x, client.y);

        // A press becomes a drag only once it has travelled. Until then
        // nothing is shown, so pressing an item to open it does not flash the
        // marker.
        if self.dragging.peek().is_none() {
            let Some(pending) = *self.pending.peek() else {
                return;
            };
            if !is_a_drag(pending.start, pointer) {
                return;
            }
            self.dragging.set(Some(pending.item));
            capture_pointer(self.container_id, pending.pointer_id);
        }

        self.pointer.set(pointer);
        self.aim();

        if let Some((top, bottom)) = *self.area.peek()
            && auto_scroll_step(pointer.1, top, bottom) != 0.0
            && !*self.scrolling.peek()
        {
            self.scrolling.set(true);
            spawn(async move { self.scroll_while_at_the_edge().await });
        }
    }

    /// Works out where a drop would land from where the pointer is now.
    fn aim(&mut self) {
        let (x, y) = *self.pointer.peek();
        let y = y + *self.scrolled.peek();
        let target = drop_target(self.layout, &self.extents.peek(), x, y);
        if *self.target.peek() != target {
            self.target.set(target);
        }
    }

    /// Scrolls the page for as long as a drag is held near its edge.
    async fn scroll_while_at_the_edge(mut self) {
        loop {
            let Some((top, bottom)) = *self.area.peek() else {
                break;
            };
            if self.dragging.peek().is_none() {
                break;
            }
            let step = auto_scroll_step(self.pointer.peek().1, top, bottom);
            if step == 0.0 {
                break;
            }

            let applied = scroll_by(self.container_id, step).await;
            if applied == 0.0 {
                // At the end of the page already: nothing to wait for.
                break;
            }
            let scrolled = *self.scrolled.peek() + applied;
            self.scrolled.set(scrolled);
            self.aim();

            crate::logic::timer::sleep(Duration::from_millis(16)).await;
        }
        self.scrolling.set(false);
    }

    /// The pointer was let go. For the list's `onpointerup`.
    pub fn released(mut self) {
        let dragged = *self.dragging.peek();
        let target = *self.target.peek();
        if let (Some(from), Some(target)) = (dragged, target) {
            let len = *self.len.peek();
            if let Some(landed) = landing_position(len, from, target.gap) {
                self.on_move.call((from, target.gap));
                self.landed.set(Some(landed));
            }
        }
        if dragged.is_some() {
            self.swallow_click.set(true);
        }
        self.end();
    }

    /// The system took the pointer away — a phone call, a gesture the browser
    /// claimed — or Escape was pressed. Nothing was dropped, so nothing moves.
    pub fn cancelled(mut self) {
        if self.dragging.peek().is_some() {
            self.swallow_click.set(true);
        }
        self.end();
    }

    /// The pointer came back into the list. For the list's `onpointerenter`.
    ///
    /// Leaving the list does not end a drag: one quick enough to be worth
    /// making leaves it constantly. What does end it is coming back with the
    /// button already let go, which is the only sign available that the
    /// release happened out there.
    pub fn entered(self, event: &PointerData) {
        if self.pending.peek().is_some() && event.held_buttons().is_empty() {
            self.cancelled();
        }
    }

    fn end(&mut self) {
        self.pending.set(None);
        self.dragging.set(None);
        self.target.set(None);
        self.extents.set(Vec::new());
        self.area.set(None);
        self.scrolled.set(0.0);
    }

    /// Whether a click on an item is really the end of a drag, and so not a
    /// click at all. For the item's `onclick`, before anything else it does.
    pub fn swallows_click(mut self) -> bool {
        let swallow = *self.swallow_click.peek();
        if swallow {
            self.swallow_click.set(false);
        }
        swallow
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
        let handle = self.handles.peek().get(landed).cloned().flatten();
        if let Some(handle) = handle {
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
        if event.key() == Key::Escape && self.pending.peek().is_some() {
            self.cancelled();
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
        if (self.dragging)() == Some(index) {
            class.push_str(" reorder-dragging");
        } else if (self.landed)() == Some(index) {
            class.push_str(" reorder-landed");
        }
        if let Some(target) = (self.target)()
            && target.item == index
            && (self.dragging)().is_some()
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
    pub fn item_style(&self, index: usize) -> String {
        if (self.dragging)() != Some(index) {
            return String::new();
        }
        let Some(pending) = (self.pending)() else {
            return String::new();
        };
        let (x, y) = (self.pointer)();
        let dx = x - pending.start.0;
        let dy = y + (self.scrolled)() - pending.start.1;
        format!("transform: translate({dx}px, {dy}px);")
    }

    /// The class of the list itself while a drag is under way.
    pub fn container_class(&self) -> &'static str {
        if self.is_dragging() { "reorder-container reorder-active" } else { "reorder-container" }
    }
}

/// The element that scrolls around the list, as a script finds it. The page
/// sits inside one rather than scrolling the document itself.
fn scroller_script(container_id: &str) -> String {
    format!(
        r#"function scroller() {{
               let node = document.getElementById("{container_id}");
               while (node && node !== document.body) {{
                   const style = getComputedStyle(node);
                   if (/(auto|scroll)/.test(style.overflowY) && node.scrollHeight > node.clientHeight) {{
                       return node;
                   }}
                   node = node.parentElement;
               }}
               return document.scrollingElement || document.body;
           }}"#
    )
}

/// Where the scrolling area around the list is on screen — its top and its
/// bottom, cut to what of it is actually in the window.
async fn scrolling_area(container_id: &'static str) -> Option<(f64, f64)> {
    let script = format!(
        r#"{}
           const box = scroller();
           const rect = box === document.scrollingElement
               ? {{ top: 0, bottom: window.innerHeight }}
               : box.getBoundingClientRect();
           return [Math.max(rect.top, 0), Math.min(rect.bottom, window.innerHeight)];"#,
        scroller_script(container_id)
    );
    let area = document::eval(&script).join::<(f64, f64)>().await.ok()?;
    Some(area)
}

/// Scrolls the area around the list by `step` and says how far it actually
/// went — less than asked, or nothing, at either end.
async fn scroll_by(container_id: &'static str, step: f64) -> f64 {
    let script = format!(
        r#"{}
           const box = scroller();
           const before = box.scrollTop;
           box.scrollTop = before + ({step});
           return box.scrollTop - before;"#,
        scroller_script(container_id)
    );
    document::eval(&script).join::<f64>().await.unwrap_or(0.0)
}

/// Sends every later event of this pointer to the list, wherever it goes.
///
/// A touch is captured to where it began anyway; a mouse is not, and without
/// this it would stop reporting to the list the moment it left it — which is
/// exactly what it does to reach the edge of the page and scroll.
fn capture_pointer(container_id: &'static str, pointer_id: i32) {
    let script = format!(
        r#"const list = document.getElementById("{container_id}");
           try {{ if (list) {{ list.setPointerCapture({pointer_id}); }} }} catch (_) {{}}"#
    );
    let _ = document::eval(&script);
}

/// The grip's `touch-action`, which is what lets a finger drag by it rather
/// than scroll the page. Named here because each list draws its own grip.
pub const GRIP_STYLE: &str = "touch-action: none;";

/// Keeps the browser's own drag-and-drop out of the way: a design tile has
/// pictures in it, and dragging a picture starts the browser's drag of the
/// picture instead of ours. For the list's `ondragstart`.
pub fn refuse_native_drag(event: Event<DragData>) {
    event.prevent_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The script that finds the scrolling area names the list it starts from.
    #[test]
    fn test_the_scroller_is_looked_for_from_the_list() {
        let script = scroller_script("slide-settings-list");
        assert!(script.contains(r#"getElementById("slide-settings-list")"#));
    }

    /// The gap a keyboard step uses is the one the move expects.
    #[test]
    fn test_a_step_down_lands_one_place_further() {
        let gap = gap_for_step(4, 1, 1);
        assert_eq!(gap.and_then(|gap| landing_position(4, 1, gap)), Some(2));
    }
}
