# 0007 — A map of the screens, and dropping windows onto it

Status: **concept.** Nothing is built. The open questions in
[Questions](#questions-that-need-answering) have to be answered before the
implementation can start; several of them change the shape of the settings
file, so they are not details to be decided while coding.

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
[Question 6](#6-does-the-drag-reuse-the-reorder-machinery).

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

See [Question 1](#1-what-happens-to-settingspresentation_screen) and
[Question 2](#2-is-the-presenter-console-still-placed-by-name).

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

**What is on it.** One chip per thing that wants a screen:

* every enabled view whose output is a screen (`ViewOutput::Screen`),
* the presenter console, when it is shown in a window of its own,
* possibly the main window — see
  [Question 3](#3-is-the-main-window-on-the-map).

A chip sitting on a rectangle means "this window opens on this screen". A chip
in a holding area beside the map means "automatic" — the view names no screen
and `place_screen_views` decides. Automatic is a real and common answer, not an
unset one, so it needs a place on screen; see
[Question 4](#4-where-does-automatic-live-on-the-map).

**Dragging.** Picking a chip up and dropping it on a rectangle sets that
thing's screen to that monitor's `screen_key`. Dropping it on the holding area
sets it back to `None`. A drop outside anything is no change. The drop-downs
stay and show the same state — a map is hard to use with a keyboard, and the
drop-downs are how this stays usable without a pointer
([Question 7](#7-what-does-this-look-like-without-a-pointer)).

**Live preview.** Dropping takes effect in the settings immediately and is
saved, as every other setting on these pages is. It does **not** move a window
that is already open; whether a running service follows a change is
[Question 5](#5-what-does-a-drop-do-to-a-running-service).

## Where the code would go

* `src/logic/screen_map.rs` — the arithmetic, with no page in it: the bounding
  box of a set of monitors, the transform from virtual-desktop coordinates to
  the box the map is drawn in, and which monitor contains a given point. Pure
  functions over `&[MonitorInfo]`, unit-tested the way `reorder.rs` is,
  including the awkward arrangements: a vertical stack, a portrait screen, a
  negative origin (a screen to the left of the primary reports a negative `x`),
  and the single-screen case.
* `src/components/screen_map.rs` — the component: the rectangles, the chips,
  the pointer handling, the drop.
* `assets/screen_map.css` — its styles, beside the other per-feature sheets.
* Used from `settings_components.rs` (screen settings) and from
  `presentation_options.rs` (the view editor), which is also where the two
  callers' differing chip sets are decided.

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
  automatic. The existing reorder specs show how a pointer drag is driven.
  Whether monitors can be faked in the test harness at all is
  [Question 8](#8-can-the-browser-tests-see-more-than-one-screen).

## Questions that need answering

These are for the user. Each one changes what gets built.

### 1. What happens to `Settings::presentation_screen`?

It is dead as a setting (see above). Three ways out:

a. **Remove the drop-down**, keep the field as the migration seed it actually
   is, and let the map and the view editor be the only places a presentation's
   screen is chosen. Honest, and one less thing that lies to the user.
b. **Make it mean something again**: the screen of the reference view, written
   through to that view. Then the settings page and the selection page are two
   views of one state, which is the least surprising thing, but it ties a
   global setting to whichever view happens to be the reference.
c. Leave it. Not recommended — a control that does nothing is worse than no
   control.

**Recommendation: (a)**, with the map in its place.

### 2. Is the presenter console still placed by name?

`presenter_screen` *is* live — it is read when the console window opens. But it
stores a bare name, which breaks for nameless or duplicate screens. Should it
be migrated to a `screen_key` (a one-line migration that keeps existing values
working through the name fallback), and should the console become a chip on the
map like everything else?

**Recommendation: yes to both.**

### 3. Is the main window on the map?

The main window — the one with the library and the running order — is the
operator's. It is not placed by Cantara today at all; the window manager puts
it where it likes, and `window_state.rs` deliberately does **not** remember its
position, with a stated reason: a window restored onto a screen that has since
been unplugged is invisible with nothing to say why.

Showing it on the map as a fixed, non-draggable chip ("you are here") would
make the picture complete and help an operator orient themselves. Letting it be
*dragged* means remembering a window position, which contradicts that decision.

**Recommendation: show it, do not let it be dragged** — unless you want the
position remembered after all, in which case that is its own decision with its
own failure mode.

### 4. Where does "automatic" live on the map?

A view with `monitor_name: None` is not unassigned — `place_screen_views` gives
it a definite screen, preferring one that is not the primary and that no other
view has claimed. So a chip could be drawn either:

a. in a holding area beside the map labelled "Automatisch", honest about the
   fact that nothing has been chosen; or
b. **on the screen it would actually land on**, drawn differently (dashed,
   faded) to say "it is here, but because nothing was chosen, and it may move".

(b) tells the operator more — it answers "where will this go?", which is the
question they came with. But it needs a holding area anyway, as the place to
drag a chip *back* to.

**Recommendation: (b), plus a holding area to drop onto.**

### 5. What does a drop do to a running service?

Views cannot be enabled during a running service today (0003 lists this as not
done). Can a window be *moved* during one — drop a chip and the window jumps to
the other screen — or does a change take effect at the next start?

Moving a live window is the useful version and also the dangerous one: the
projection goes black for a moment in front of the congregation.

**Recommendation: take effect at the next start**, with the map showing where
things will go rather than where they are, and the chips marked as pending
while a service runs. Moving live windows is its own spec.

### 6. Does the drag reuse the reorder machinery?

`reorder.rs` answers a one-dimensional question (which gap). The map asks a
two-dimensional one (which rectangle). The pointer handling — threshold,
capture, touch-action, measure-once — is identical and worth sharing; the
arithmetic is not.

Options: factor the pointer handling out of `components/reorder.rs` into
something both use, or write a second, smaller drag for the map and accept the
duplication. The first is cleaner and risks disturbing four working lists
(running order, sidebar, designs, slide settings) shortly after 0006 settled
them; see [0001](0001-duplicated-code.md) on how the project treats
duplication.

**Recommendation: write the map's drag separately first**, and factor out
afterwards once the shared part is known rather than guessed. Say if you would
rather have it shared from the start.

### 7. What does this look like without a pointer?

Keeping the drop-downs is the plain answer, and this spec assumes it. Is that
enough, or should the chips also be keyboard-operable (focus a chip, arrow keys
to move it between screens)? The latter is real work for a case the drop-downs
already cover.

**Recommendation: drop-downs, and the map is the convenience.**

### 8. Can the browser tests see more than one screen?

The Playwright suite runs the web build in a browser, where
`enumerate_monitors` returns nothing — it is behind `feature = "desktop"`.
Testing the map end-to-end needs a way to feed it a fabricated set of monitors.
Is a test-only injection point acceptable (a harness hook, as
[`src/logic/harness.rs`](../../src/logic/harness.rs) already is for other
things), or should the browser tests be left to cover only what the web build
can show, with the geometry covered in Rust alone?

**Recommendation: a harness hook**, so that the drag itself is tested on a
realistic two-screen arrangement and not only its arithmetic.

### 9. Does the map appear in both places, or one?

The screen settings and the selection page's view options both deal with
screens. Two maps is two places to keep in step; one map means one of those
pages sends the user to the other.

**Recommendation: both**, as the same component with a different set of chips —
the settings map carries the console and the main window, the view editor's
carries the views. Say if you would rather have it in one place only.

### 10. What happens when a screen is unplugged while the map is open?

The list is refreshed today by a button ("Aktualisieren"). Should the map poll,
or watch for the platform's display-change event, or keep the button? A chip
sitting on a screen that is no longer there has to go somewhere — back to the
holding area, or stay as a ghost that recovers when the screen comes back?

**Recommendation: keep the button, and draw a vanished screen as a ghost
rectangle** so that an unplugged beamer does not silently lose its assignment —
which is what `resolve_monitor`'s fallback already tries to protect against.

## What this is not

* Not a display-arrangement tool. Cantara draws the arrangement the operating
  system reports; it does not change it. Moving a screen is the desktop's job.
* Not a window manager. Chips say which screen a window opens on, not where
  within it or at what size — that stays fullscreen-or-maximised as it is now.
* Not a change to `place_screen_views`. The map is a way of setting the inputs
  that function already takes.
