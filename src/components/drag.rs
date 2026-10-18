//! Carrying something with a mouse, a pen or a finger.
//!
//! This is the half of a drag that is the same wherever one happens: noticing
//! that a press has become a drag, capturing the pointer, measuring what can
//! be dropped onto, scrolling when the pointer is held at the edge, following
//! the pointer with the carried thing, and letting go. It knows nothing about
//! *what* is being dragged or *what it means* to drop it there.
//!
//! The other half — what the pointer is over, and what happens when it is
//! released there — belongs to whoever is doing the dragging:
//!
//! * [`crate::components::reorder`] reads the pointer as a *gap between two
//!   rows* and drops by moving an item into it. Four lists use it: the running
//!   order, the sidebar, the presentation designs and the slide settings.
//! * The screen map (`docs/specs/0007-screen-map.md`) reads it as *which
//!   monitor rectangle the pointer is inside* and drops by assigning a window
//!   to that screen.
//!
//! Two different questions, one set of pointer mechanics. This module was
//! lifted out of `reorder.rs`, where it had grown for the running order, when
//! the second caller arrived — see decision D6 of 0007.
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
//!   move and the release itself are delivered there and bubble up to the
//!   container — which is where they are handled. A mouse is captured
//!   explicitly, once the press has become a drag, so that it can leave the
//!   container (to scroll the page at its edge) without the container losing
//!   sight of it.
//! * A touch that is not claimed scrolls the page. Claiming it is
//!   [`GRIP_STYLE`], and that is put on a grip only, so that a list long
//!   enough to scroll still scrolls under the finger everywhere else. A
//!   container too short to need scrolling — the sidebar — may claim all of
//!   itself.
//! * What the pointer is over is worked out from the coordinate rather than
//!   from events on the things underneath, because under a capture they see no
//!   events at all. They are measured once when the press starts; nothing
//!   reflows during a drag — a marker is drawn without taking space, and the
//!   carried thing is moved with a transform — so once is enough. When the
//!   page scrolls under the drag, the distance scrolled is added to the
//!   pointer instead of measuring again.

use crate::logic::reorder::{ItemExtent, auto_scroll_step, is_a_drag};
use dioxus::prelude::*;
use std::rc::Rc;
use std::time::Duration;

/// A press that has not become a drag yet, or the drag it became.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Press {
    /// What was pressed, as the caller numbers such things. Opaque here.
    item: usize,
    /// Where the pointer went down, to measure the distance from.
    start: (f64, f64),
    pointer_id: i32,
    /// Which press this is, so that a measurement still under way for an
    /// earlier one is not mistaken for this one's.
    generation: u64,
}

/// What the pointer is over, asked of the caller.
///
/// `x` and `y` are in the coordinates the drop zones were measured in — the
/// viewport as it stood when the press started, plus however far the area has
/// scrolled since. The caller does not have to think about the scrolling.
pub struct Aim {
    pub x: f64,
    pub y: f64,
    /// Where each drop zone was, in the order they were registered with
    /// [`PointerDrag::mounted`].
    ///
    /// An `Rc` because it is handed over on every pointer move, and a drag
    /// produces a great many of those.
    pub zones: Rc<Vec<ItemExtent>>,
}

/// The pointer half of one drag, and what the elements taking part call into.
///
/// `T` is whatever the caller works out from the pointer — a gap for a list, a
/// monitor for the screen map. It is only ever carried from [`Aim`] to the
/// drop, never looked into here.
///
/// Every field is a signal, so this is `Copy` and can be moved into as many
/// event handlers as the container needs.
#[derive(Clone, Copy, PartialEq)]
pub struct PointerDrag<T: Copy + PartialEq + 'static> {
    /// The `id` of the element holding the drop zones. It is what the
    /// scrolling area is found from, and what a mouse is captured to.
    container_id: &'static str,
    /// How many drop zones there are to measure.
    zone_count: CopyValue<usize>,
    /// What the pointer is over, answered by the caller.
    aim: Callback<Aim, Option<T>>,
    /// What was carried, and what it was dropped on.
    on_drop: Callback<(usize, T)>,

    pending: Signal<Option<Press>>,
    dragging: Signal<Option<usize>>,
    target: Signal<Option<T>>,

    handles: Signal<Vec<Option<Rc<MountedData>>>>,
    zones: Signal<Rc<Vec<ItemExtent>>>,
    /// The pointer, in the viewport's coordinates.
    pointer: Signal<(f64, f64)>,
    /// How far the scrolling area has moved since the zones were measured.
    scrolled: Signal<f64>,
    /// Where the scrolling area is on screen: its top and bottom.
    area: Signal<Option<(f64, f64)>>,
    scrolling: Signal<bool>,
    generation: Signal<u64>,
    /// Set when a drag ends, so that the click a browser delivers after a
    /// press-and-release is not taken for a click on the thing.
    swallow_click: Signal<bool>,
}

/// A drag inside the element with the id `container_id`, over `zone_count`
/// drop zones.
///
/// `aim` is asked, on every move, what the pointer is over; `on_drop` is given
/// what was carried and what it was let go on, and is what actually changes
/// anything.
pub fn use_pointer_drag<T: Copy + PartialEq + 'static>(
    container_id: &'static str,
    zone_count: usize,
    aim: impl FnMut(Aim) -> Option<T> + 'static,
    on_drop: impl FnMut((usize, T)) + 'static,
) -> PointerDrag<T> {
    let aim = use_callback(aim);
    let on_drop = use_callback(on_drop);
    let mut stored_count = use_hook(|| CopyValue::new(zone_count));
    // Not a signal: the count is read when a press starts, and writing a
    // signal while rendering would only render again.
    stored_count.set(zone_count);

    PointerDrag {
        container_id,
        zone_count: stored_count,
        aim,
        on_drop,
        pending: use_signal(|| None),
        dragging: use_signal(|| None),
        target: use_signal(|| None),
        handles: use_signal(Vec::new),
        zones: use_signal(|| Rc::new(Vec::new())),
        pointer: use_signal(|| (0.0, 0.0)),
        scrolled: use_signal(|| 0.0),
        area: use_signal(|| None),
        scrolling: use_signal(|| false),
        generation: use_signal(|| 0),
        swallow_click: use_signal(|| false),
    }
}

impl<T: Copy + PartialEq + 'static> PointerDrag<T> {
    /// Whether something is being carried right now.
    pub fn is_dragging(&self) -> bool {
        (self.dragging)().is_some()
    }

    /// What is being carried, as the caller numbered it.
    pub fn carried(&self) -> Option<usize> {
        (self.dragging)()
    }

    /// What the pointer is currently over, as [`Aim`] last answered.
    pub fn target(&self) -> Option<T> {
        (self.target)()
    }

    /// How far the carried thing has been moved from where it was picked up.
    ///
    /// `None` when nothing is being carried. The scrolling is already in it.
    pub fn carry_offset(&self) -> Option<(f64, f64)> {
        let pending = (self.pending)()?;
        self.carried()?;
        let (x, y) = (self.pointer)();
        Some((x - pending.start.0, y + (self.scrolled)() - pending.start.1))
    }

    /// Remembers the element of drop zone `index`, so that it can be measured
    /// when a drag starts. For that element's `onmounted`.
    pub fn mounted(mut self, index: usize, event: Event<MountedData>) {
        let mut handles = self.handles.write();
        if handles.len() <= index {
            handles.resize(index + 1, None);
        }
        handles[index] = Some(event.data());
    }

    /// The element of drop zone `index`, for a caller that needs to do
    /// something else with it — put the focus on it, say.
    pub fn handle(&self, index: usize) -> Option<Rc<MountedData>> {
        self.handles.peek().get(index).cloned().flatten()
    }

    /// A press on `index`, which is the caller's number for whatever is being
    /// picked up. For its `onpointerdown`.
    ///
    /// A mouse or a pen may start a drag anywhere. A finger may only where the
    /// caller says it may — on a grip, or anywhere when the caller allows it —
    /// because claiming a touch is what turns it into a drag rather than a
    /// scroll.
    ///
    /// Answers whether the press was taken up, so that a caller with something
    /// of its own to clear at the start of a drag clears it only then.
    pub fn press(mut self, index: usize, event: &PointerData, touch_allowed: bool) -> bool {
        // A second finger or a right-click is not a drag.
        if !event.is_primary()
            || event
                .trigger_button()
                .is_some_and(|button| button != dioxus::html::input_data::MouseButton::Primary)
        {
            return false;
        }
        if event.pointer_type() == "touch" && !touch_allowed {
            return false;
        }

        self.swallow_click.set(false);
        let generation = *self.generation.peek() + 1;
        self.generation.set(generation);

        let client = event.coordinates().client();
        let pending = Press {
            item: index,
            start: (client.x, client.y),
            pointer_id: event.pointer_id(),
            generation,
        };
        self.pending.set(Some(pending));
        self.pointer.set(pending.start);
        self.scrolled.set(0.0);

        // Measured now rather than when the drag starts, because asking the
        // renderer for a rectangle costs a round trip per zone and the answer
        // has to be ready by the first move. Nothing shifts in between, so
        // measuring early is measuring correctly.
        spawn(async move {
            let measured = self.measure().await;
            let area = scrolling_area(self.container_id).await;

            // Another press has started since — this measurement is not its.
            if self.pending.peek().as_ref().map(|pending| pending.generation) != Some(generation) {
                return;
            }
            // A zone that cannot be measured gives the whole drag up: a
            // missing one would shift every zone after it, and the drop would
            // land somewhere that was never pointed at.
            match measured {
                Some(zones) => {
                    self.zones.set(Rc::new(zones));
                    self.area.set(area);
                    // A quick hand may already be carrying the thing: the
                    // moves before now had nothing to aim at.
                    if self.dragging.peek().is_some() {
                        self.aim();
                    }
                }
                None => self.end(),
            }
        });

        true
    }

    async fn measure(&self) -> Option<Vec<ItemExtent>> {
        let count = *self.zone_count.peek();
        let handles: Vec<Option<Rc<MountedData>>> =
            self.handles.peek().iter().take(count).cloned().collect();
        if handles.len() < count {
            return None;
        }

        let mut zones = Vec::with_capacity(count);
        for handle in handles {
            let rect = handle?.get_client_rect().await.ok()?;
            zones.push(ItemExtent {
                left: rect.min_x(),
                top: rect.min_y(),
                right: rect.max_x(),
                bottom: rect.max_y(),
            });
        }
        Some(zones)
    }

    /// The pointer moved. For the container's `onpointermove`.
    pub fn moved(mut self, event: &PointerData) {
        let client = event.coordinates().client();
        let pointer = (client.x, client.y);

        // A press becomes a drag only once it has travelled. Until then
        // nothing is shown, so pressing something to open it does not flash a
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

    /// Asks the caller where a drop would land from where the pointer is now.
    fn aim(&mut self) {
        let (x, y) = *self.pointer.peek();
        let y = y + *self.scrolled.peek();
        let zones = Rc::clone(&self.zones.peek());
        let target = self.aim.call(Aim { x, y, zones });
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

    /// The pointer was let go. For the container's `onpointerup`.
    pub fn released(mut self) {
        let dragged = *self.dragging.peek();
        let target = *self.target.peek();
        if let (Some(from), Some(target)) = (dragged, target) {
            self.on_drop.call((from, target));
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

    /// The pointer came back into the container. For its `onpointerenter`.
    ///
    /// Leaving does not end a drag: one quick enough to be worth making leaves
    /// constantly. What does end it is coming back with the button already let
    /// go, which is the only sign available that the release happened out
    /// there.
    pub fn entered(self, event: &PointerData) {
        if self.pending.peek().is_some() && event.held_buttons().is_empty() {
            self.cancelled();
        }
    }

    fn end(&mut self) {
        self.pending.set(None);
        self.dragging.set(None);
        self.target.set(None);
        self.zones.set(Rc::new(Vec::new()));
        self.area.set(None);
        self.scrolled.set(0.0);
    }

    /// Whether a click is really the end of a drag, and so not a click at all.
    /// For the pressed element's `onclick`, before anything else it does.
    pub fn swallows_click(mut self) -> bool {
        let swallow = *self.swallow_click.peek();
        if swallow {
            self.swallow_click.set(false);
        }
        swallow
    }

    /// Escape, which drops nothing. Answers whether there was a press to
    /// abandon.
    pub fn escape(self) -> bool {
        if self.pending.peek().is_some() {
            self.cancelled();
            return true;
        }
        false
    }
}

/// The element that scrolls around the container, as a script finds it. The
/// page sits inside one rather than scrolling the document itself.
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

/// Where the scrolling area around the container is on screen — its top and
/// its bottom, cut to what of it is actually in the window.
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

/// Scrolls the area around the container by `step` and says how far it
/// actually went — less than asked, or nothing, at either end.
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

/// Sends every later event of this pointer to the container, wherever it goes.
///
/// A touch is captured to where it began anyway; a mouse is not, and without
/// this it would stop reporting the moment it left — which is exactly what it
/// does to reach the edge of the page and scroll.
fn capture_pointer(container_id: &'static str, pointer_id: i32) {
    let script = format!(
        r#"const list = document.getElementById("{container_id}");
           try {{ if (list) {{ list.setPointerCapture({pointer_id}); }} }} catch (_) {{}}"#
    );
    let _ = document::eval(&script);
}

/// A grip's `touch-action`, which is what lets a finger drag by it rather than
/// scroll the page. Named here because each caller draws its own grip.
pub const GRIP_STYLE: &str = "touch-action: none;";

/// Keeps the browser's own drag-and-drop out of the way: a design tile has
/// pictures in it, and dragging a picture starts the browser's drag of the
/// picture instead of ours. For the container's `ondragstart`.
pub fn refuse_native_drag(event: Event<DragData>) {
    event.prevent_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The script that finds the scrolling area names the container it starts
    /// from.
    #[test]
    fn test_the_scroller_is_looked_for_from_the_container() {
        let script = scroller_script("slide-settings-list");
        assert!(script.contains(r#"getElementById("slide-settings-list")"#));
    }
}
