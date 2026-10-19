# 0007 — A map of the screens, and dropping windows onto it

Status: **the map is built; two decisions are not.** The ten questions this document opened have
been answered and are recorded as [Decisions](#decisions) — each one carries
what was asked, what was chosen, and what it costs. Three of them change the
shape of the settings file and one changes shared code the running order
depends on, so they are worth reading before the first line is written. The
loose ends the answers themselves created are at the
[end](#what-the-answers-left-open).

What is built: the map itself with the chips and the dragging, and the
projection's drop-down — D1, D2, D3, D4, D6, D7, D9 and D10. What is not:
**D5** ("Jetzt übernehmen" — a change takes effect at the next start and there
is no way to move a running service's windows) and **D8** (no harness hook, so
no browser test touches the map; the geometry and the chip placement are
covered by Rust tests alone).

## The brief

The screens Cantara can see are listed today as three lines of text:

> **Erkannte Bildschirme**
> **DP-2** 1920x1080 *(Primär)*

That list says which screens exist. It says nothing about where they are —
which one is to the left, which hangs above the stage, which is the big one.
Somebody standing at the technician's desk with a laptop, a beamer and a stage
monitor has to work that out from names like `DP-2` and `HDMI-A-1`, and the
only way to find out is to start a service and see where the window lands.

This spec asks for two things:

1. **The screens are drawn, not listed** — as a small map, in the real
   proportions and the real arrangement the operating system reports, the way
   the display settings of every desktop environment draw them.
2. **Windows are placed by dragging them onto that map** — the presentation
   views, the presenter console and the presentation window itself. The
   drop-downs that do this today stay, and the map is a second way of saying
   the same thing, not a replacement.

## Why this is worth doing

The information is already there. `MonitorInfo` in
[`src/logic/screens.rs`](../../src/logic/screens.rs) carries `position` and
`size` in virtual screen coordinates for every monitor — exactly what is needed
to draw the arrangement — and the UI currently throws the position away and
prints the size as text.

The cost of getting the assignment wrong is paid in front of a congregation:
the sermon outline on the wall, the lyrics on the stage monitor nobody can
read, and an operator dragging windows around with a service waiting. A picture
that says "this one, the wide one on the right, is the beamer" is the whole
feature.

## What is already there

Four things in the code decide most of this spec.

**1. Monitors are enumerated with their geometry.**
[`enumerate_monitors`](../../src/logic/screens.rs) returns `MonitorInfo { id,
name, position, size, is_primary }` for every screen, in virtual-desktop
coordinates and physical pixels. Everything the map needs to draw itself is in
that struct; nothing new has to be asked of the platform.

**2. A screen is identified by a key, not by its name.**
[`screen_key`](../../src/logic/screens.rs) is `"{name}#{id}"`, because on
Windows and Wayland the name is frequently empty and two identical monitors
report the same string. `resolve_monitor` matches on the whole key first and
falls back to the name, so a screen that moved in the enumeration is still
found.

**3. Placement already has one rule, and it is tested.**
[`place_screen_views`](../../src/logic/screens.rs) decides which view lands on
which screen: named views first, then the unnamed ones onto screens nobody has
claimed, so that two windows never open on top of one another. It has a
thorough test suite. **The map must feed this function rather than go round
it**, or the two will disagree about where a window is going.

**4. A touch-capable drag already exists, twice over.**
[`src/logic/reorder.rs`](../../src/logic/reorder.rs) and
[`src/components/reorder.rs`](../../src/components/reorder.rs) were built for
0006: pointer events (mouse, pen and finger in one code path), a movement
threshold so a tap is not a drag, `touch-action: none` on a grip only, items
measured once at the start of a drag. The arithmetic is separate from the page
so that it can be tested in Rust.

That machinery is built for **one-dimensional lists** — it answers "which gap
between two rows is the pointer nearest". The map needs "which rectangle is the
pointer inside", which is a different question. See
[D6](#d6-the-pointer-handling-is-shared).

## Two things that are broken today, and are in scope

Writing this spec turned up two defects in the existing screen settings. Both
touch the same code, and fixing them separately would mean touching it twice.

**The "Präsentationsbildschirm" drop-down does nothing.** It writes
`Settings::presentation_screen`, and the only place that reads that field is
[`ensure_views`](../../src/logic/settings.rs), which runs **only when the view
list is empty** — that is, once, when a settings file written before 0003 is
migrated. On every installation that has been started since, changing that
drop-down has no effect whatsoever: the screen a presentation goes to comes
from the views' own `ViewOutput::Screen { monitor_name }`, set on the selection
page. The field is a migration seed wearing the costume of a setting.

**The drop-downs store the name, not the key.** Both selects in
[`settings_components.rs`](../../src/components/settings_components.rs) write
`monitor.name` and compare against `monitor.name`, while `resolve_monitor`
expects a `screen_key`. It happens to work through the name fallback, right up
until two screens report the same name or none — which is precisely the case
`screen_key` exists for, and precisely what a hall with two identical monitors
has. The selection page's view editor does it correctly (`screen_value`); the
settings page does not.

Both are fixed here: see [D1](#d1-the-screen-drop-down-is-made-to-mean-something-again)
and [D2](#d2-the-presenter-console-moves-to-screen_key-and-onto-the-map).

## The map

A block, in the screen settings and on the selection page's view options,
showing every monitor as a rectangle.

**Geometry.** The rectangles keep the aspect ratio and the relative position
the platform reports. The bounding box of all monitors is scaled to fit the
available width; a 4K screen beside a 1080p one is drawn twice as large,
because that is what it is, and a screen the platform places below another is
drawn below it. A screen rotated into portrait is drawn in portrait — its
reported `size` already is.

**What each rectangle says.** The name (or `Monitor n` where there is none),
the resolution, a mark for the primary, and — the point of the whole exercise —
the windows currently assigned to it.

**Where it lives.** In the screen settings, once ([D9](#d9-one-map-in-the-screen-settings)).
It carries every chip there is; the view editor on the selection page keeps its
drop-down and points at it.

**What is on it.** One chip per thing that wants a screen:

* every enabled view whose output is a screen (`ViewOutput::Screen`),
* the presenter console, when it is shown in a window of its own,
* the main window, as a fixed chip that cannot be dragged
  ([D3](#d3-the-main-window-is-shown-but-not-draggable)).

A chip sitting on a rectangle means "this window opens on this screen". A chip
that names no screen is drawn **on the screen it would actually land on**,
dashed, because `place_screen_views` gives it a definite answer and that answer
is what the operator came to find out ([D4](#d4-automatic-is-drawn-where-it-lands-and-there-is-a-holding-area-too)).
A holding area beside the map, labelled "Automatisch", is what such a chip is
dragged back *to*.

**Dragging.** Picking a chip up and dropping it on a rectangle sets that
thing's screen to that monitor's `screen_key`. Dropping it on the holding area
sets it back to `None`. A drop outside anything is no change. The drop-downs
stay and show the same state — a map is hard to use with a keyboard, and the
drop-downs are how this stays usable without a pointer
([D7](#d7-the-drop-downs-are-the-keyboard-path)).

**When a drop takes effect.** Immediately in the settings, and saved, as every
other setting on these pages is — but it does not disturb a service that is
already running. While a presentation runs, a chip that has been moved is drawn
as pending, with one explicit control to **apply it now**; that is the only
thing that moves a live window
([D5](#d5-next-start-by-default-with-an-explicit-apply-now)).

## Where the code would go

* `src/logic/screen_map.rs` — the arithmetic, with no page in it: the bounding
  box of a set of monitors, the transform from virtual-desktop coordinates to
  the box the map is drawn in, and which monitor contains a given point. Pure
  functions over `&[MonitorInfo]`, unit-tested the way `reorder.rs` is,
  including the awkward arrangements: a vertical stack, a portrait screen, a
  negative origin (a screen to the left of the primary reports a negative `x`),
  and the single-screen case.
* `src/components/drag.rs` — the pointer handling lifted out of
  `components/reorder.rs` so that the map and the four existing lists share one
  ([D6](#d6-the-pointer-handling-is-shared)): the movement threshold, the
  capture, `touch-action`, measure-once, the auto-scroll at the edge. What a
  drag *means* stays with each caller — a gap for a list, a rectangle for the
  map.

  Beside the components rather than in `logic/`, which this spec first said.
  `logic/` is where things that can be tested without a page live, and this
  module is nothing but page: signals, mounted elements and `document::eval`.
  The arithmetic it leans on is already in `logic/reorder.rs` and stays there.
* `src/components/screen_map.rs` — the component: the rectangles, the chips,
  the drop.
* `assets/screen_map.css` — its styles, beside the other per-feature sheets.
* Used from `settings_components.rs` only. `presentation_options.rs` keeps its
  per-view drop-down and gains a link to the screen settings.

## Testing

* **Rust unit tests** on `screen_map.rs` for the geometry: the arrangements
  above, and that a point inside a rectangle resolves to the monitor it was
  drawn from, for every monitor in a set.
* **Rust unit tests** that a drop writes a `screen_key` and that
  `place_screen_views` then puts the window where the map said it would. This
  is the test that catches the map and the placement rule drifting apart.
* **Playwright**, beside `settings-reorder.spec.js`, for the drag itself: that
  a chip dragged onto a rectangle stays there, that the drop-down beside it
  agrees afterwards, and that a chip dragged to the holding area goes back to
  automatic. The existing reorder specs show how a pointer drag is driven. The
  browser build sees no monitors at all, so the suite is fed a fabricated set
  through a harness hook ([D8](#d8-a-harness-hook-feeds-the-tests-fabricated-screens)).
* **The existing reorder suites are the regression test for
  [D6](#d6-the-pointer-handling-is-shared).** `selection-reorder.spec.js` and
  `settings-reorder.spec.js` cover the four lists whose drag is being moved out
  from under them; they must pass unchanged, on touch as well as mouse, before
  the shared module is considered done.

## Decisions

Ten questions were opened when this spec was written. All ten are answered
below: what was asked, what was chosen, and — where the choice costs something
— what it costs. Where the answer differs from what the spec first recommended,
that is said plainly.

### D1. The screen drop-down is made to mean something again

**Asked:** `Settings::presentation_screen` is dead as a setting — the only
reader is `ensure_views`, which runs once, on a pre-0003 settings file. Remove
the drop-down, make it live again, or leave it?

**Decided:** make it live again. The spec recommended removing it; the
drop-down stays instead, and is wired to the screen of the **reference view** —
the projection — and written through to that view's
`ViewOutput::Screen { monitor_name }`.

This is the better answer for the person using it. "Auf welchem Bildschirm
läuft die Präsentation?" is a settings question, and sending them to the
selection page's view editor to answer it is sending them somewhere they would
not think to look. The settings page and the selection page then show one piece
of state from two directions, which is what a user already assumes they do.

**What it costs.** `presentation_screen` stops being a stored field and becomes
a *projection* of the reference view — read from it, written to it. Keeping it
as a second copy would mean two fields drifting apart, with nothing to say
which one the window will actually use. So:

* The field stays in the struct for `ensure_views` to migrate from, and is
  marked as what it is: a migration seed, not state.
* The drop-down reads `settings.reference_view()` and writes that view's
  output. When there is no reference view — no enabled screen view at all — the
  drop-down has nothing to point at and is disabled with a line saying so,
  rather than silently writing somewhere.
* It writes a `screen_key`, not a name. See [D2](#d2-the-presenter-console-moves-to-screen_key-and-onto-the-map).

**Built.** The drop-down reads and writes the reference view's
`ViewOutput::Screen`, through the same `assign_screen` the map's drop uses, so
the picture and the drop-down are one piece of state and cannot come to mean
different things. `presentation_screen` stays in the struct and is documented
as what it is: a seed `ensure_views` reads once, on a settings file written
before views existed, and nothing else reads at all.

`Settings::reference_view_position` was added for it — the same answer
`reference_view` gives, as a position rather than a borrow, so that an editor
can write to that view. A test pins the two together, because a drop-down
writing a different view than the rest of the program reads would be a bug
nobody would see until a service.

Where there is no reference view with a screen — it is served to the network,
or there are no views at all — the drop-down is disabled and says why. That is
the reading [the open questions](#what-the-answers-left-open) flagged as worth
looking at on screen before settling; it is built the cautious way and can
still be changed to offer creating a view instead.

### D2. The presenter console moves to `screen_key`, and onto the map

**Asked:** `presenter_screen` is live but stores a bare name, which breaks for
nameless or duplicate screens. Migrate it to a `screen_key`, and give the
console a chip?

**Decided:** both.

The migration is one pass at load: a stored value with no `#` in it is a name
from before this change, and is matched against the enumerated monitors by name
and rewritten as that monitor's key. A name that matches nothing is left alone
— `resolve_monitor`'s name fallback still handles it, and a screen that is
merely unplugged today must not lose its assignment permanently.

The same applies to any `monitor_name` already stored in a view by the settings
page's old drop-down, which had the same defect.

### D3. The main window is shown, but not draggable

**Asked:** does the operator's own window belong on the map, given that
`window_state.rs` deliberately does not remember a window's position?

**Decided:** shown, not draggable — as recommended. It is drawn as a fixed
chip, visibly different from the others, on whichever screen it is currently
on. It is "you are here", and it is what makes the map readable: without it an
operator cannot tell which rectangle they are looking at the map *on*.

Nothing about window positions is remembered, and the reason in
`window_state.rs` stands untouched.

### D4. "Automatic" is drawn where it lands, and there is a holding area too

**Asked:** does an unassigned chip sit in a holding area, or on the screen
`place_screen_views` would actually give it?

**Decided:** on the screen it would land on, drawn dashed — and a holding area
as well, as the place to drag a chip back to.

This is the answer that makes the map worth building. The question an operator
has is "where will this go?", and a chip in a holding area labelled
"Automatisch" does not answer it. A dashed chip on the beamer says: here, and
not because you chose it.

**What it costs.** The map must call `place_screen_views` to draw itself, not
merely to agree with it afterwards — which is the right dependency, and the
thing the cross-test in [Testing](#testing) guards.

### D5. Next start by default, with an explicit "apply now"

**Asked:** does a drop move a window that is already open?

**Decided:** not by itself. A drop takes effect at the next start, as
recommended — **and** a running presentation can be made to follow, through one
explicit control.

So, while a service is running, a chip that has been moved is drawn as pending,
and a button beside the map — "Jetzt übernehmen" — moves the live windows onto
the screens the map now shows. Nothing moves until it is pressed.

This keeps the dangerous thing deliberate. Moving the projection window is a
moment of black in front of the congregation; that is acceptable when somebody
has just decided they want it, and not acceptable as a side effect of tidying
up the settings mid-service.

**What it costs.** Moving a live window is new machinery: the windows opened by
`start_presentation` are not tracked in a way that lets them be repositioned
afterwards, and a `tao` window has to be found again, moved, and re-fullscreened
on the new monitor. This is the largest single piece of work the ten answers
added, and it is the one most likely to want a stage of its own.

**Not in scope even so:** enabling or disabling a view during a running service.
That is still what 0003 left undone, and "apply now" repositions the windows
that exist rather than opening new ones.

### D6. The pointer handling is shared

**Asked:** reuse the reorder drag, or write a second one for the map?

**Decided:** share it. The spec recommended writing the map's drag separately
and factoring out afterwards; sharing from the start was chosen instead.

The pointer handling is lifted out of `components/reorder.rs` into a module
both use — threshold, capture, `touch-action`, measure-once, edge auto-scroll —
and each caller keeps its own answer to "what is under the pointer": a gap for
a list, a rectangle for the map.

**What it costs, and how it is paid.** Four working lists sit on that code —
the running order, the sidebar, the designs, the slide settings — and 0006
settled them recently. The protection is that all four are covered by Playwright
(`selection-reorder.spec.js`, `settings-reorder.spec.js`), so the extraction is
done **first**, as its own commit, with those suites passing unchanged before
the map is written at all. If the shared shape turns out to fight the list case,
the extraction is reverted on its own without touching the map.

**Built.** [`src/components/drag.rs`](../../src/components/drag.rs) is that
module. `PointerDrag<T>` owns the press, the threshold, the capture, the
measuring, the edge scrolling and the release; the caller supplies two things —
`aim`, asked on every move what the pointer is over, and `on_drop`, given what
was carried and what it was let go on. `T` is whatever the caller's `aim`
returns and is never looked into. `ReorderDrag` is now a wrapper over it that
answers `drop_target` to the first and moves an item to the second, and its
public API did not change, so the four lists were not touched at all.

The one thing the extraction had to be careful about: a drop zone is not
necessarily the thing being dragged. In a list they are the same items, which
is why the old code could use one index for both; on the map the chips are
dragged and the *monitors* are the zones. So `PointerDrag` counts zones and
carries the pressed thing's number as an opaque `usize`, and it is the caller's
business whether the two coincide.

### D7. The drop-downs are the keyboard path

**Asked:** is keeping the drop-downs enough without a pointer, or do the chips
need to be keyboard-operable?

**Decided:** the drop-downs are enough. The map is the convenience; every
assignment it can make is also available from a `select`, which is reachable,
labelled and operable by keyboard and screen reader alike. The map itself is
marked as presentational so that it is not an obstacle to tab through.

### D8. A harness hook feeds the tests fabricated screens

**Asked:** can the Playwright suite see more than one screen?

**Decided:** it cannot — `enumerate_monitors` is behind `feature = "desktop"`
and the browser build has none — so a test-only injection point is added, in
the manner of [`src/logic/harness.rs`](../../src/logic/harness.rs), which the
project already uses for exactly this kind of thing.

The hook supplies a fabricated `Vec<MonitorInfo>`, and the browser suite drives
the drag against a realistic arrangement: a primary laptop, a beamer to its
right, a stage monitor above. The geometry stays covered by Rust unit tests;
the hook is what lets the *drag* be tested end to end rather than only its
arithmetic.

### D9. One map, in the screen settings

**Asked:** does the map appear in both the screen settings and the selection
page's view options, or in one place?

**Decided:** one place. The spec recommended both; one was chosen.

**Which place follows from the other answers, and is the screen settings.**
Three of the chips — the presenter console ([D2](#d2-the-presenter-console-moves-to-screen_key-and-onto-the-map)),
the main window ([D3](#d3-the-main-window-is-shown-but-not-draggable)) and the
projection's own drop-down ([D1](#d1-the-screen-drop-down-is-made-to-mean-something-again))
— belong to the settings page and have no place in a per-view editor. A map in
the view editor would be the same picture with things missing from it.

So the screen settings carry the one map, with every chip on it, and the view
editor in `presentation_options.rs` keeps its per-view drop-down and gains a
link to the settings. A view's screen is settable from both; only the settings
page draws the picture.

### D10. An unplugged screen's windows are shown on the screen they fall back to

**Asked:** what happens to a chip whose screen has been unplugged — holding
area, or ghost rectangle?

**Decided:** neither. The vanished screen is not drawn, and its chips appear on
the screen they would actually land on, dashed — exactly as an "automatic" chip
does ([D4](#d4-automatic-is-drawn-where-it-lands-and-there-is-a-holding-area-too)).
The spec had recommended a ghost rectangle; this is simpler and says the same
true thing.

The assignment itself is **not** cleared. `resolve_monitor` already keeps a
stored key through a screen's absence and finds it again by name when it comes
back, and the map must not undo that by rewriting the setting to whatever the
fallback picked. What is drawn is where the window will go; what is stored is
what the user asked for. A short line under the map says so when the two differ.

The screen list is refreshed by the existing "Aktualisieren" button; polling
and display-change events stay out of scope.

## What was built

[`src/logic/screen_map.rs`](../../src/logic/screen_map.rs) holds the
arithmetic: the bounding box of a set of monitors, the transform into the box
the map is drawn in, and which screen a point is inside. One scale for both
directions so a screen keeps its shape, the arrangement centred in whichever
direction it does not fill, and a floor on how small a screen may be drawn —
without it, a 1024×768 auxiliary beside a 5K display is a sliver a few pixels
wide, which is in proportion and useless. Sixteen tests cover the vertical
stack, the portrait screen, the negative origin, one screen, no screens, a box
with no room, a screen reporting no size, and the seam between two screens.

[`src/components/screen_map.rs`](../../src/components/screen_map.rs) draws it
and carries the decisions. Its own tests are about where a chip is *drawn*
rather than about pixels: a chosen screen is solid, an inferred one is dashed,
two automatic views never land on the same screen, and a chip whose screen is
unplugged moves to its fallback while the stored key stays put.

One thing worth recording, because it was nearly got wrong. The drop's hit test
runs over the rectangles **the page laid out**, not the ones `place_all`
computed — a border, a padding or a rounding in the CSS would otherwise put the
seam between two screens somewhere other than where the operator is aiming. The
measured rectangles are fed through the same `screen_at` the arithmetic is
tested on, so there is one hit test rather than two that could disagree.

Two things were fixed along the way that D2 called for: the console's
drop-down now writes a `screen_key` rather than a bare name, and
`upgrade_screen_key` migrates what earlier versions stored. A name matching no
connected screen is **kept, not cleared** — the beamer is probably in the
cupboard, and clearing it would move the projection to the laptop for good.

## What the answers left open

Three things the decisions themselves created, to be settled when the work is
planned rather than now:

1. **"Jetzt übernehmen" is a stage of its own.** [D5](#d5-next-start-by-default-with-an-explicit-apply-now)
   needs the open presentation windows to be findable and movable after the
   fact, which nothing today requires. The map is useful without it; it should
   not be what holds the map up.
2. **The order of the work is fixed by [D6](#d6-the-pointer-handling-is-shared).**
   The shared-drag extraction lands first, on its own, green. Then the map.
3. **What "the reference view" means when there is none** is handled by
   disabling the drop-down ([D1](#d1-the-screen-drop-down-is-made-to-mean-something-again)),
   which is correct but worth looking at on screen before settling: a disabled
   control with an explanation may read worse than an enabled one that creates
   a view.

## What this is not

* Not a display-arrangement tool. Cantara draws the arrangement the operating
  system reports; it does not change it. Moving a screen is the desktop's job.
* Not a window manager. Chips say which screen a window opens on, not where
  within it or at what size — that stays fullscreen-or-maximised as it is now.
* Not a change to `place_screen_views`. The map sets the inputs that function
  already takes, and calls it to draw itself
  ([D4](#d4-automatic-is-drawn-where-it-lands-and-there-is-a-holding-area-too));
  the rule about which window lands where is not touched.
* Not a way to start or stop a view mid-service. "Jetzt übernehmen"
  ([D5](#d5-next-start-by-default-with-an-explicit-apply-now)) moves the
  windows that are open; enabling a view during a running service is still what
  [0003](0003-add-monitor-view.md) left undone.
