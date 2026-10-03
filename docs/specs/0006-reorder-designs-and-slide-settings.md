# 0006 — Reordering presentation designs and slide settings

Status: **draft, waiting for review.** Nothing below has been built yet. The
questions in the second part each carry a proposal and an empty **Answer:**.

## The brief

The order of the presentation designs and of the slide settings cannot be
changed today. Both should be reorderable by drag and drop, simply and
smoothly:

* **Presentation designs** — the tiles in "Präsentationseinstellungen" can be
  dragged to the place where they should be. This has to work well on touch
  screens too.
* **Slide settings** — the drop-down in "Folien-Einstellungen" is replaced by a
  list, and that list can be reordered the same way.

The implementation is covered by tests both in Rust and in Playwright.

## Why the order matters at all

The order is not cosmetic. It is what every place that offers a design or a
division shows (the default in the selection's options, the per-element
choice, the choice per view), and for slide settings without a name it *is*
the name: "Folien-Einstellungen 2" is a position. Somebody who has built up
eight designs over the years and uses two of them every Sunday wants those two
at the front.

## What is already there

Three things in the code decide most of this spec, and are worth knowing
before reading the questions.

**1. A touch-capable drag already exists, in the running order.**
[`selected_list.rs`](../../src/components/selection_components/selected_list.rs)
was rebuilt on pointer events precisely because the mouse-event version did not
work on a phone. Its module doc records the lessons: an implicit pointer
capture on touch, `touch-action: none` on a grip only so that the list still
scrolls, a movement threshold so that a tap is not a drag, rows measured once
at the start, the selection renumbered along with the move. Its pure functions
— `drop_index`, `reorder`, `index_after_move`, `is_a_drag` — are already unit
tested.

This spec does not invent a second way of dragging. It takes that one, moves
the parts that are not about a running order into a shared place, and uses it
three times.

**2. Both lists are referred to by position.** `Settings` keeps
`default_design_index` and `default_slide_settings_index`, `StreamSettings`
keeps `design_index` and `slide_settings_index`, and every `View` keeps the
same pair. `delete_presentation_design` already exists because removing an
entry silently re-pointed every one of those at the neighbour. **Moving an
entry has exactly the same problem**, and a worse version of it: a deletion at
least changes the number of entries, a move changes nothing anyone would
notice until the wrong design is on the wall.

(The choices *per element* in a running order are not affected: they hold a
copy of the design or division and are matched by name or content —
`design_position` / `slide_settings_position` in `presentation_options.rs`.
Saved selections likewise carry copies. Neither cares about the order.)

**3. The two lists are edited differently.** The designs section writes
straight into the settings. The slide settings section edits a *copy* in a
signal, which an effect in `SettingsContent` mirrors back. Its delete button is
a plain `Vec::remove` on that copy — so deleting a division today **does**
re-point the default and every view at the neighbour, which is the very defect
`delete_presentation_design` was written to stop. Reordering through the same
copy would inherit it.

---

# What the brief leaves open

Ordered by how expensive the wrong answer is later.

## 1. Moving an entry must carry every stored choice with it

Not really a question, but the one thing that must not be got wrong, so it
comes first and is written down. If design 1 is the default and is dragged to
position 3, the default must be position 3 afterwards — not whatever now sits
at position 1. The same for the stream's choice and for every view, and the
same for slide settings.

**Proposed:** two operations on `Settings`, next to
`delete_presentation_design` and for the same reason (the bookkeeping lives
with the lists, where a second caller cannot forget it):

```rust
pub fn move_presentation_design(&mut self, from: usize, to_gap: usize) -> Option<usize>;
pub fn move_song_slide_settings(&mut self, from: usize, to_gap: usize) -> Option<usize>;
```

Both renumber every stored position with the existing `index_after_move` rule.
The UI never touches the vectors directly.

**Answer:**

## 2. Do the two lists move together?

The two lists are coupled today in two places: `ensure_slide_settings_for_designs`
keeps at least as many divisions as designs, and `delete_presentation_design`
deletes the *division at the same position* along with the design. That
coupling is a leftover — nothing else treats design 3 and division 3 as a
pair, and the UI shows them as unrelated lists.

Once both lists can be reordered independently, "the division at the same
position" stops meaning anything at all: after one drag, deleting a design
would delete some unrelated division.

**Proposed:** the lists are reordered independently (that is what the brief
describes), and **deleting a design no longer deletes a division.** The
`ensure_slide_settings_for_designs` fix-up is kept, since it only ever adds and
is harmless. Existing tests for the coupled deletion are changed to assert the
new behaviour.

**Alternative:** keep the coupling and leave it alone. Then this spec should at
least say that deleting a design after a reorder removes an arbitrary
division, and somebody will report it.

**Answer:**

## 3. Unnamed slide settings are named by their position

`SongSlideSettings::display_name` falls back to "Folien-Einstellungen *n*" for
a division without a name — *n* being its position. Reorder two unnamed
divisions and the list shows… exactly what it showed before: "…1", "…2". The
move happened, the details card on the right changed, but the names did not,
and to the user it looks as if the drop was refused.

**Proposed:** when a division without a name is moved, every unnamed division
in the list is given its current fallback name as a real name first, in the
language the program is running in. From then on each one keeps its name
wherever it goes. This changes nothing that was visible before the move, and is
done in `move_song_slide_settings`, so it is covered by the unit tests.

**Alternative:** leave the names positional and accept that reordering
unnamed entries looks like nothing happened. Or show a neutral "Unbenannt"
instead of a number — but that makes several unnamed entries
indistinguishable, which is worse.

Designs do not have this problem: a design always has a name ("Default" for a
fresh one).

**Answer:**

## 4. What "smooth" looks like while dragging

Two common ways to show where a drop will land:

* **Insertion marker** — the dragged item stays where it is, faded, and a line
  shows the gap it will go into. This is what the running order does.
* **Live reflow** — the other items move out of the way as the pointer passes,
  as in SortableJS or on a phone's home screen.

Live reflow looks smoother, but every reflow moves the items the drop position
is computed from, so they have to be measured again on each movement, and with
wrapping tiles of 400 px a reflow can push a tile onto the next line under the
finger. That is where such implementations get jittery.

**Proposed:** an insertion marker, the same as the running order, so that the
program has *one* way of dragging. The smoothness comes from three things that
cost little: the item follows the finger as a semi-transparent "ghost"
(a CSS transform on a copy, not a reflow); the marker fades in rather than
jumping; and the moved item is highlighted briefly where it landed. All
transitions are switched off under `prefers-reduced-motion`.

**Answer:**

## 5. How a touch starts a drag on a design tile

A touch that the page does not claim scrolls it. Claiming every touch on a
400 px tile would make the settings page impossible to scroll on a phone,
where the tiles fill the whole width. The tile also contains its own buttons
(‹ and › page through the preview slides) and is itself a button (it selects
the design).

**Proposed:** the same rule as in the running order.

* **Mouse and pen:** the whole tile can be dragged, after the 5 px threshold.
  A press on ‹ or › never starts a drag.
* **Touch:** only a **grip** in the tile's top-left corner, the one element
  with `touch-action: none`. It is always shown on devices with a coarse
  pointer (`@media (pointer: coarse)`), and on hover elsewhere. Large enough
  for a thumb (at least 44 × 44 px, per the usual touch-target guidance).

**Alternative:** long-press anywhere on the tile to start a drag, as on a phone
home screen. More discoverable to some, but it collides with the browser's own
long-press (context menu, text selection) and needs a timer with its own edge
cases. It could be added later on top of the grip without changing anything
else.

**Answer:**

## 6. Dragging past the edge of the screen

On a phone, three designs are already taller than the screen. The running
order never had to solve this: it is a short list in its own panel. Here,
moving the first design behind the fifth means scrolling while dragging, and
the page will not scroll by itself while a touch is claimed.

**Proposed:** **edge auto-scroll.** While a drag is active and the pointer is
within 48 px of the top or bottom of the scrolling container, the container
scrolls in that direction, faster the closer to the edge. Positions are
measured once at the start of the drag in *content* coordinates (client
coordinate plus the container's scroll offset), and every pointer position is
converted the same way, so the measurements stay valid while scrolling. The
same applies to the slide settings list.

**Answer:**

## 7. The keyboard

The running order can be reordered with Alt+↑/↓ on a focused row, which is the
only way to reorder it without a pointer at all.

**Proposed:** the same in both new places. In the slide settings list Alt+↑/↓;
on the design tiles Alt+←/→ and Alt+↑/↓ (the tiles wrap, so both make sense
and mean "one earlier" / "one later"). Focus stays on the moved item.

In passing: each tile currently has `tabindex: index`, which is
`tabindex="0"` for the first tile and a *positive* tab index for every other
one — so every tile after the first is reached by Tab before anything else on
the page. That becomes `tabindex: 0` for all tiles.

**Answer:**

## 8. What the slide settings list looks like

The brief asks for a list instead of the drop-down. The details card stays to
the right, as now, and the list sits where the drop-down was.

**Proposed:** one row per division, showing the name and, if there is one, the
description in smaller type underneath it, plus a grip on the left (visible on
touch devices, as in the running order). A click on the row selects it, and
the row that is shown in the card on the right is marked the way the running
order marks its open row (`selection_item-active`). Under the width where the
grid collapses, the list sits above the card. The list is a `ul` with
`role="listbox"` and its rows `role="option"` with `aria-selected`, so a screen
reader announces what the drop-down used to announce.

**Answer:**

## 9. When the change is saved

**Proposed:** once per completed drop, never while dragging; a cancelled drag
(pointer cancelled by the system, released outside the window, Escape) saves
nothing, because nothing changed. The slide settings section stops editing a
copy and works on the settings directly, as the designs section already does —
which also fixes the delete defect described under "What is already there".
The mirroring effect in `SettingsContent` goes away with the copy.

**Answer:**

## 10. What is out of scope

* The sidebar of the selection view (`sidebar.rs`) has a third, older drag
  implementation on mouse events, with the same touch defect the running order
  had. It would be the natural next user of the shared code, but it is a
  separate change.
* Undo. Dragging back is the undo.
* Reordering inside the design editor (fonts, widgets, monitor widget rows).

**Answer:**

---

# Implementation concept

Four stages, each of which leaves the program working and tested.

## Stage 1 — the shared logic, without a page

**New module `src/logic/reorder.rs`**, holding what in
`selected_list.rs` is not about a running order:

```rust
/// How far a press has to travel before it counts as a drag.
pub const DRAG_THRESHOLD: f64 = 5.0;

/// An item's box on screen, in content coordinates.
pub struct ItemExtent { pub left: f64, pub top: f64, pub right: f64, pub bottom: f64 }

/// The gap in a single column a drop goes into (today's `drop_index`).
pub fn drop_index_in_column(items: &[ItemExtent], y: f64) -> usize;

/// The gap in a wrapping row of tiles a drop goes into.
pub fn drop_index_in_grid(items: &[ItemExtent], x: f64, y: f64) -> usize;

/// Moves the item at `from` into gap `to` (today's `reorder`).
pub fn move_into_gap<T>(items: &mut Vec<T>, from: usize, to: usize) -> Option<usize>;

/// Where a position ends up after a move (today's `index_after_move`).
pub fn index_after_move(watched: usize, from: usize, landed: usize) -> usize;

/// The scroll speed for a pointer this close to an edge; 0 outside the band.
pub fn auto_scroll_step(pointer: f64, viewport_start: f64, viewport_end: f64) -> f64;
```

`drop_index_in_grid` groups the tiles into lines by their vertical extent,
picks the line the pointer is in (or the nearest one), and inside it divides
each tile at its horizontal midpoint. Right of the last tile of a line is the
gap after that tile, not the start of the next line, because that is where the
marker is drawn.

`selected_list.rs` is changed to use this module. Its behaviour does not
change, and its existing tests move along with the functions.

**`Settings::move_presentation_design` / `move_song_slide_settings`**
(question 1), `Settings::delete_song_slide_settings` (question 9, with the same
bookkeeping as the design deletion), and the decoupled
`delete_presentation_design` (question 2) and naming of unnamed entries
(question 3), depending on the answers.

## Stage 2 — the slide settings list

`SongSlideSettingsSection` loses its `song_slide_settings` prop and reads the
settings from `use_settings()`. The `select` becomes the list from question 8.
The drag state (`pending`, `dragging_from`, `drop_at`, measurements) and the
pointer handlers follow the running order's structure: `pointermove`, `up` and
`cancel` on the list container, `pointerdown` on the row (mouse/pen) and on the
grip (touch). The selected index follows a move with `index_after_move`.

To keep this from becoming a third copy of the same handler code, the drag
state and its handlers become a small hook, `use_reorder_drag`, in
`src/components/reorder.rs`, used here, by the design tiles and by the running
order. It takes the layout (`Column` / `Grid`) and a callback
`on_move(from, to_gap)`.

## Stage 3 — the design tiles

`PresentationDesignSelector` gets the grip, the ghost, the vertical insertion
marker between tiles, the keyboard handling and the `tabindex` fix. It is only
used on the settings page, so it does not need a switch to turn reordering
off. The ‹ / › buttons stop pointer propagation so they never start a drag.
`PresentationSettings` passes `on_move`, which calls
`Settings::move_presentation_design`, makes the selection follow, and saves.

New CSS in `assets/main.css` beside `.selected-item-grip`: grip, ghost,
marker, the landed highlight, the coarse-pointer rule, and the
`prefers-reduced-motion` exceptions.

Stable hooks for the tests: `data-reorder-index` on every item, the classes
`reorder-grip`, `reorder-marker`, `reorder-dragging`, and the ids
`#presentation-design-list` and `#slide-settings-list`.

## Stage 4 — Playwright

New file `tests/browser/settings-reorder.spec.js`, against the web build at
`/Cantara/settings`.

**Seeding.** The tests need at least three entries with different names in
each list. The web build keeps its settings in `localStorage` under
`cantara-settings`, so a helper `seedSettings(page, mutate)` opens the page
once, reads the JSON, lets the test rename and duplicate entries, writes it
back and reloads. That is the real persistence path, and no control port is
needed.

**Touch.** Playwright's `touchscreen` only taps. Touch drags are sent through
the Chrome DevTools Protocol (`Input.dispatchTouchEvent`: `touchStart`, several
`touchMove`, `touchEnd`) in a `describe` that sets `hasTouch: true`, `isMobile:
true` and a phone-sized viewport. The suite runs on Chromium only (spec 0004),
so this is allowed.

**Cases, for both lists unless noted:**

1. A mouse drag from the first to the last position reorders the entries in
   the DOM.
2. The new order survives a reload (it was saved to `localStorage`).
3. The selected entry follows the move: the card on the right still shows the
   dragged one.
4. The stored choices follow the move: after moving the default design, the
   `default_design_index` in the stored JSON still names the same design.
5. A click without movement selects and does not reorder (threshold).
6. A cancelled drag (`pointercancel`, Escape) changes nothing and saves
   nothing.
7. Touch: a drag on the grip reorders.
8. Touch: a swipe on the tile or row *outside* the grip scrolls the page and
   does not reorder.
9. Touch on a phone viewport: dragging the first design past the bottom of the
   screen auto-scrolls and lands it at the end.
10. Keyboard: Alt+↓ (list) / Alt+→ (tiles) moves the focused entry and keeps
    focus on it.
11. Designs only: ‹ / › still page through the preview and do not start a drag.
12. Slide settings only: there is no `select` in the section any more, and
    the active row has `aria-selected="true"`.

## Rust tests

**`logic::reorder` (unit tests):**

* The existing column tests, moved unchanged.
* Grid: a pointer left of the first tile of a line is the gap before it; right
  of the last tile is the gap after it; above everything is 0; below
  everything is `len`; lines of unequal length (the last line half full); a
  single tile; an empty list has exactly one gap.
* `move_into_gap` over every `(from, to)` pair for lists up to length 5: the
  result is a permutation, the moved item lands at the returned position, and
  the two gaps next to an item are no move.
* `index_after_move` agrees with what `move_into_gap` actually did, for every
  watched position (property test over the same pairs).
* `auto_scroll_step` is 0 outside the band, increases toward the edge, and has
  the right sign at each end.

**`logic::settings` (unit tests):**

* After `move_presentation_design`, the default, the stream's choice and each
  view's choice name the **same design by name** as before — checked over every
  `(from, to)` pair on a settings value with four distinct designs and choices
  spread across them. The same for `move_song_slide_settings`.
* A `None` choice stays `None`.
* An impossible move (out of range, onto its own gap) returns `None` and leaves
  the settings unchanged, compared with `==`.
* `delete_song_slide_settings` renumbers like `delete_presentation_design`.
* Depending on question 2: deleting a design leaves the slide settings
  untouched.
* Depending on question 3: moving an unnamed division names every unnamed one
  with its previous fallback name; named ones are left alone.
* The `settings_migration` invariant test (every stored choice points at
  something that exists) is extended to run once more after a few moves.
* A serialise/deserialise round trip keeps the order.

**Markup tests (`dioxus-ssr`)**, in the shape of `slide_markup`:

* The slide settings section renders a `ul#slide-settings-list` with one
  `role="option"` per division, exactly one with `aria-selected="true"`, and no
  `select`.
* Every design tile carries `data-reorder-index`, a grip with
  `touch-action: none`, and `tabindex="0"`.
* Outside a drag there is no marker and no ghost in the markup.

## What none of this checks

Whether a drag *feels* smooth on a real tablet, and the desktop window
(WebKitGTK/WebView2/WKWebView), which Playwright cannot reach (spec 0004). Both
need a person with the device for a few minutes before release: one Android
phone, one iPad, and the desktop build with a touchscreen if one is at hand.
