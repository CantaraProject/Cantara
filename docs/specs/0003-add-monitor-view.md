# 0003 — Monitor views, and the end of the fixed two outputs

Status: **in progress.** The five decisions below are taken. Stages 1, 2, 3a and
3½ are built; the order of what remains has changed — see
[What stage 3 found](#what-stage-3-found).

A monitor design can be **made, configured and drawn**: the design editor
offers "Darstellungsart", both layouts render, and the clock and chapter-timer
widgets work.

What is still missing is the last link in the chain: **there is no way to
create a view.** The migration makes the two Cantara always had — the projection
and the stream — and nothing offers to add a third. So a monitor design can be
built and a window would draw it correctly, but no window is asking for one.
See [The view editor](#the-view-editor), which is the next thing to build and
the point at which the feature becomes usable.

Cantara today can put a service onto exactly two surfaces, and both of them are
aimed at the congregation: the projection, and — since [0002](0002-remote-control.md)
— the stream to the pews. Everyone who is *making* the service happen is left
looking at the same wall as the people in front of them. The speaker cannot see
what comes next without turning round. The band cannot see where in the song
the operator is. The technician at the back has the presenter console, which is
the only screen in the building that tells anyone anything, and it is bolted to
the one machine that drives the projection.

This document asks for a second kind of view — a *monitor view*, made for the
people on the platform — and, because a second kind of view does not fit
anywhere in the current model, for the model itself to change: from two
built-in outputs to as many views as the user cares to define.

## What is being asked for

1. A presentation design says what *kind* of view it describes: an audience
   view (everything that exists today) or a monitor view.
2. A monitor view has its own layouts — a slide list, a speaker view, or a
   template the user writes.
3. A monitor view can carry *widgets*: a clock, a timer counting the current
   chapter, and eventually something the user supplies.
4. The user can define any number of views and assign each one to a screen or
   to a network address, instead of choosing between "projection" and "stream".
5. Every view that is running shows up in the presenter console.

Point 4 is the one that costs. Points 1–3 are new code beside existing code;
point 4 is a change to the shape of the settings file, to how presentation
windows are opened, and to what the network helper is told to serve. The rest
of this document is mostly about doing point 4 without breaking a service.

## What this is not

* Not a second presenter console. A monitor view shows; it does not control.
  The one place that drives a presentation stays the console (locally or over
  the remote, as 0002 built it). This is deliberate: two people who can both
  press "next" is a bug report waiting in a service.
* Not per-viewer personalisation. A network monitor view is one page served to
  whoever opens that address; it does not know who is looking at it.
* Not a change to how slides are *built*. Chapters, slide settings and the
  slide division stay exactly as they are. A monitor view is another reading of
  the same `RunningPresentation`.

## The constraint that decides the design

Everything a view needs is already in `RunningPresentation`
([states.rs:248](../../src/logic/states.rs:248)): the chapters, the position
within them, the black-screen flag, the video state. A monitor view needs the
same value and nothing more — the previous slide, the next slide, the chapter
it is in and how long it has been there are all derivable from it.

That is what makes this affordable. No new synchronisation, no second channel,
no second source of truth. Every mechanism that already carries the
presentation to a second surface — the desktop window that renders it, the
helper process that serves it — carries a monitor view unchanged. What changes
is which component gets rendered at the far end, and that is a value in the
design, not a build target.

The corollary is a rule worth stating: **a monitor view must never be able to
change the presentation.** It receives; it does not send. The remote console
already has a password of its own precisely because being able to watch and
being able to drive are different rights (see `StreamSettings::remote_password`,
[settings.rs:173](../../src/logic/settings.rs:173)), and a monitor view served
on the network sits on the watching side of that line.

## The data model

### Kind, on the design

`PresentationDesign` ([settings.rs:1624](../../src/logic/settings.rs:1624))
gains nothing at its top level. The kind lives one level down, in
`PresentationDesignSettings`, because the settings a monitor view needs and the
settings an audience view needs have almost nothing in common — fonts and
padding are shared, but a slide list has no vertical alignment and an audience
view has no widgets.

```rust
pub enum PresentationDesignSettings {
    /// Describes an audience view. Exactly what exists today.
    Template(PresentationDesignTemplate),

    /// Manually specified HTML/CSS/JS. Still not implemented.
    Custom(String),

    /// Describes a monitor view — for the platform, not the pews.
    Monitor(MonitorDesign),
}
```

Adding a variant to a `serde`-tagged enum is backwards compatible in the
direction that matters: an old settings file has no `Monitor` designs in it and
reads unchanged. A file written by a new Cantara and read by an old one will
fail on that design, which is acceptable and is what the version field in
`settings_io` ([settings_io.rs:75](../../src/logic/settings_io.rs:75)) exists to
report on for exported designs.

**Open:** whether `MonitorDesign` should embed `PresentationDesignTemplate`
rather than restate fonts, colours and padding. Embedding avoids a second font
editor and lets the existing design editor be reused for the shared half;
restating avoids a struct half of whose fields are meaningless. The
recommendation is to embed, with the fields a monitor view ignores documented
as ignored.

### What a monitor view shows

```rust
pub struct MonitorDesign {
    /// The shared look: fonts, background, padding.
    pub base: PresentationDesignTemplate,

    /// The layout.
    pub layout: MonitorLayout,

    /// What is shown alongside the slides, and where.
    pub widgets: Vec<MonitorWidget>,
}

pub enum MonitorLayout {
    /// Every slide of the presentation, the current one marked, the ones
    /// before and after it readable. The presenter console's list without
    /// the buttons.
    SlideList {
        /// How many slides either side are drawn. `None` draws all of them
        /// and scrolls the current one into view.
        context: Option<usize>,
    },

    /// The current slide, large; the next one, small. For whoever is
    /// speaking.
    Speaker {
        /// The share of the height the next slide takes, 0.0–1.0.
        next_slide_share: f64,
    },

    /// A Handlebars template the user writes, held in a file beside the
    /// settings. See [Templates as files](#templates-as-files).
    Custom { template_file: String },
}
```

The three are the three named in the original request. `SlideList` is close
enough to what the presenter console already draws
([presenter_console_components.rs](../../src/components/presenter_console_components.rs))
that the list itself should be lifted out of the console and shared rather than
written a second time — that is exactly the kind of duplication
[0001](0001-duplicated-code.md) is about.

### The template context

`Custom` is the variant that has to be got right, because a template is a
public interface: once a user has written one, the names in it cannot be
changed without breaking their file. So the context is specified here, before
anything renders it.

Handlebars is already in the tree, but only transitively (via a build
dependency); this makes it a direct one.

```json
{
  "current": { "index": 0, "chapter_index": 0, "title": "…", "lines": ["…"], "tags": ["…"], "kind": "song|markdown|image|video|title" },
  "next":    { … same shape, null on the last slide },
  "previous":{ … same shape, null on the first slide },
  "chapter": { "index": 0, "title": "…", "slide_count": 4, "slide_in_chapter": 1 },
  "presentation": { "slide_count": 42, "chapter_count": 7 },
  "state": { "black_screen": false, "elapsed_in_chapter_seconds": 137, "elapsed_total_seconds": 1802 },
  "widgets": { "<widget id>": "<rendered html>" }
}
```

Rules that go with it:

* **Additive only.** Keys may be added in later versions; a key that has been
  published is not renamed or removed.
* **Escaped by default.** Slide text goes through Handlebars' HTML escaping.
  A template that wants Cantara's own rendered slide markup asks for it
  explicitly (`{{{current.html}}}`), and that markup is the same string the
  audience view produces, not user input.
* **No network, no filesystem, no helpers with side effects.** A template
  renders from the context and nothing else.
* **A template that fails to compile or render does not take the view down.**
  It shows an error inside the monitor view — that screen is on a platform, in
  front of a congregation, and a blank one is worse than an ugly one.

### Widgets

```rust
pub struct MonitorWidget {
    /// Stable identifier, used as the key in the template context.
    pub id: String,
    pub kind: WidgetKind,
    pub placement: WidgetPlacement,
}

pub enum WidgetKind {
    /// Date and time, formatted for the active locale.
    Clock { format: ClockFormat },

    /// How long the presentation has been in the current chapter — how long
    /// the sermon has run, how long this song has gone on.
    ChapterTimer { warn_after: Option<Duration> },

    /// User-supplied. See below.
    Custom(CustomWidget),
}
```

`Clock` uses the existing localisation
([localisation.rs](../../src/logic/localisation.rs)) rather than a format
string of its own, so a German installation gets a German date without the user
configuring one.

`ChapterTimer` needs one thing the model does not have today: **when the
current chapter was entered.** `RunningPresentationPosition`
([states.rs:702](../../src/logic/states.rs:702)) knows which chapter is current
but not since when. The timer is therefore not purely derivable from the
published state, and something has to record the transition.

Where that lives is a real decision, so it is called out:

* Recording it in `RunningPresentation` means every surface — window, stream,
  network monitor — agrees on the number without doing anything, because the
  value travels with the presentation as everything else does. It costs a field
  that changes on every chapter change and is serialised to the helper.
* Computing it locally in each view means no protocol change, but two monitors
  in the same building disagree by however long their connections differ, and a
  browser that reloads restarts the sermon clock at zero. That second failure
  rules it out.

**Recommendation: a field on `RunningPresentation`,** holding the wall-clock
instant the chapter was entered, serialised as a UTC timestamp. Views compute
the elapsed time from it. A reload then shows the right number, which is the
whole point of the widget.

### Custom widgets: JavaScript and WebAssembly

The original request asks for custom widgets implemented in JavaScript or
WebAssembly. This is the highest-risk item in the document and it should be the
last thing built, for two reasons.

First, the surfaces differ. A desktop monitor view is a web view Cantara
controls, and script in it runs with whatever that web view can reach. A
network monitor view is a page in someone else's browser. "The same widget"
does not mean the same thing in both places.

Second, a widget is a file the user got from somewhere. Cantara's design files
are already shareable ([settings_io.rs](../../src/logic/settings_io.rs)), and a
design that carries executable code is a design that carries executable code to
whoever it is sent to.

So: **custom widgets are staged separately and behind an explicit opt-in.**
Concretely — a design import that contains a custom widget says so, in plain
words, and the widget does not run until the user has said it may. Import of an
unreviewed design must never be a silent path to running code.

**Open:** whether the first shipped form should be WebAssembly only. Wasm is
sandboxed by construction and gets no DOM access unless it is handed one, which
is a far smaller thing to get right than script in a privileged web view. The
inclination is yes — Wasm first, script later or never — but this needs a look
at what a widget author would actually have to write.

## Views, and what they are shown on

This is the restructuring the rest depends on.

### Today

Two outputs, each with its own settings and its own switch: the projection
(`presentation_screen`, [settings.rs:90](../../src/logic/settings.rs:90)) and
the stream (`StreamSettings`, [settings.rs:147](../../src/logic/settings.rs:147),
with `design_index` and `slide_settings_index` naming what the phones get). The
window is opened in
[selection_components.rs:561](../../src/components/selection_components.rs:561);
the network side is a single helper with an `Offer { viewer, console }`
([network_server.rs:201](../../src/logic/network_server.rs:201)) on one port.

### Proposed

```rust
pub struct View {
    pub name: String,
    /// Index into `Settings::presentation_designs`.
    pub design_index: usize,
    /// Index into `Settings::song_slide_settings`; `None` follows the
    /// projection's division.
    pub slide_settings_index: Option<usize>,
    pub output: ViewOutput,
    /// Whether this view is running. Changeable from the selection screen
    /// while a presentation is on — see [Switching views mid-service](#switching-views-mid-service).
    pub enabled: bool,
    /// Where this view is looking. See [Focus](#focus).
    pub focus: ViewFocus,
}

pub enum ViewOutput {
    /// A window on a screen. `None` picks one the way it is picked today.
    Screen { monitor_name: Option<String> },
    /// A path on the network helper's port: `/`, `/stage`, `/band`.
    Network { path: String },
}
```

Designs stay referenced by index and not copied, for the reason
`StreamSettings::design_index` already gives: editing a design has to reach
every view built from it. An index past the end of its list is read as "no
choice" rather than as a reason to fall over mid-service — the same rule
`StreamDefaults::of` ([stream_view.rs:52](../../src/logic/stream_view.rs:52))
already follows.

Three things fall out of this that the design must handle:

* **One view is the reference.** Slide numbers, the console's counting, and the
  `map_slides` contract in [stream_view.rs](../../src/logic/stream_view.rs) all
  assume one authoritative slide sequence. That stays the projection. A list of
  views needs to name which one it is, and the constraint that a view's slide
  division must hold a whole number of the reference's slides
  (`stream_slide_settings`) applies to every view, not just the stream. Views
  may look at *different places* in that sequence — see [Focus](#focus) — but
  there is still only one sequence.
* **Network paths must be validated.** They are user input becoming routes on a
  live server. Restrict to a short character set, require uniqueness, and
  reserve `/console` and the asset and video prefixes that
  [network_server.rs:695](../../src/logic/network_server.rs:695) already claims
  — two handlers on one path is a panic in the server thread, and the helper
  goes on reporting itself as up while answering nothing.
* **Screens can disappear.** A view assigned to a monitor that is not plugged
  in must degrade to a clear message in the console, not to a window nobody can
  see. `resolve_monitor` ([screens.rs:51](../../src/logic/screens.rs:51)) has
  the fallback behaviour already.

### Migration

Old settings files must open and behave identically. The migration is
mechanical and should be written and tested before any UI exists:

| Old | New |
| --- | --- |
| `presentation_screen` | `View { name: "Projection", design_index: None, output: Screen { monitor_name }, enabled: true }`, and it is the reference view |
| `StreamSettings::design_index` / `slide_settings_index` | `View { name: "Stream", output: Network { path: "/" }, enabled: false, … }` |
| no views at all | both of the above |

The old fields are read for one release and then dropped; `#[serde(default)]`
on the new list, plus a "if empty, build from the old fields" step, is the whole
mechanism. The port, the passwords and the remote console are untouched — they
belong to the server, not to a view.

Two details of the table are decisions rather than transcription:

* **The projection view names no design of its own.** Copying
  `default_design_index` into it would pin the wall to whichever design
  happened to be the default at the moment of migration, and changing the
  default afterwards would silently stop reaching the projection. `None` — "the
  service's design" — is what the wall has always meant.
* **The stream view is always created, and always disabled.** Whether streaming
  is on has deliberately never been remembered between sessions, so there is no
  stored answer to migrate; an enabled stream view would start putting services
  on the network for people who never switched it on. It is created even for
  somebody who has never streamed, because the alternative is guessing from
  settings that look untouched, and a disabled view costs nothing.

## The presenter console

Every running view is listed, with its name, its output, and whether it is
actually up. This is the only place that reports a view failing to start — a
screen that vanished, a path that collided, a helper that would not run — and
0002's rule holds: a view that will not start is reported and changes nothing
else. The presentation is the main window's, and nothing here is allowed to be
a reason for it to stop.

Whether views can be switched on and off *during* a service from the console is
worth having but is not required by the first version.

## What must not be written twice

This feature is a second reading of things the program already does, and the
easiest way to build it is to write each of them a second time. [0001](0001-duplicated-code.md)
is a whole document about what that costs here — a bug fixed in one copy and
left in the other. So, named in advance, the places where reuse is the design
and not an optimisation:

* **The slide list.** `MonitorLayout::SlideList` is the presenter console's list
  without its buttons. The list is lifted out of
  [presenter_console_components.rs](../../src/components/presenter_console_components.rs)
  into a shared component taking "is it interactive" as a property; the console
  then uses that component too. If the monitor view ends up with a list of its
  own, this stage has failed.
* **The slide itself.** `Speaker` draws the current slide large and the next one
  small. Both are the ordinary slide rendering
  ([presentation_components.rs](../../src/components/presentation_components.rs))
  at two sizes — not a second renderer that happens to look similar. The
  console's preview already does exactly this; whatever it uses is what these
  use.
* **The design editor.** Decision 1 embeds `PresentationDesignTemplate` so that
  the fonts, colours and padding of a monitor design are edited by the existing
  editor
  ([presentation_design_settings_components.rs](../../src/components/presentation_design_settings_components.rs)).
  Only the monitor-specific half — layout, widgets — is new UI.
* **The slide-division constraint.** `stream_slide_settings`
  ([stream_view.rs](../../src/logic/stream_view.rs)) already works out what
  division a second view may use given the projection's. It becomes the rule for
  every view rather than being reimplemented per view; the stream stops being a
  special case and becomes a `View` like the others.
* **Monitor resolution.** `resolve_monitor` ([screens.rs:51](../../src/logic/screens.rs:51))
  already answers "which screen, given a configured name, and what if it is
  gone". Every `Screen` view goes through it.
* **The window-opening path.** One function opens a view's window, called once
  per view, rather than the projection's path and the console's path growing a
  third sibling in
  [selection_components.rs:561](../../src/components/selection_components.rs:561).

The migration in stage 2 is what makes most of this possible: once the
projection and the stream are `View`s, the code that serves "a view" is written
once and the two existing outputs stop being separate code paths.

## Work plan

Each stage is meant to leave the program working.

1. ~~**`elapsed_in_chapter`.** The field on `RunningPresentation`, set on chapter
   change, serialised to the helper. Nothing renders it yet. Small, and it
   unblocks the timer widget.~~ **Done.** Built as
   `RunningPresentation::chapter_entered_at`, a wall-clock
   [`Timestamp`](../../src/logic/timer.rs) rather than an elapsed count — a
   duration published every second would be a change to the presentation every
   second, and every view already knows what time it is. The three ways of
   moving now share one `moved` method, which is where the clock is restarted
   and where the scroll reset that all three already duplicated now lives.
   A rebuild of the running order keeps the clock when the same element is
   still up.
2. ~~**The `View` list and its migration.** The settings model, the migration
   from the old fields, and the tests for both. No UI, no behaviour change: the
   program builds the same two views it always did, from the new list.~~
   **Done.** `View`, `ViewOutput` and `ViewFocus` in
   [settings.rs](../../src/logic/settings.rs), with `Settings::views` and
   `reference_view_index`; `ensure_views` does the migration. Views joined the
   design-deletion bookkeeping in `delete_presentation_design`, so a deleted
   design moves every view's choice by the same rule as the stream's.
   `check_network_path` refuses a colliding or malformed path where the user
   types it; the server's router now names the same constants, so the two
   cannot disagree about what is taken. Nothing reads the list yet — that is
   stage 3.

   The two `ensure_*` sequences in `Settings::load`, one per target, became one
   `bring_up_to_date`. They had already drifted, and adding a fifth step to
   only one of them is precisely the failure [0001](0001-duplicated-code.md)
   describes.
3. **Windows and routes driven by the list.** Split in two once the code was
   read; see [What stage 3 found](#what-stage-3-found).

   a. ~~`selection_components.rs` opens a window per `Screen` view.~~ **Done.**
      `place_screen_views` in [screens.rs](../../src/logic/screens.rs) decides
      which views get a window and on which screen; `open_view_window` is the
      one path by which a presentation window is made, called once per
      placement. Each window is told which view it is drawing, which is the
      seam stage 4 needs. Behaviour is unchanged for every existing
      configuration: one enabled `Screen` view, on the screen
      `presentation_screen` named.

   b. **The helper's `Offer` becomes a set of paths.** Not done, and it should
      not be done next — see below.
3½. **Making a monitor design, and saying so.** *Not in the original plan at
   all* — the work plan went straight from the model to the layouts and never
   said where a user creates one. Found by trying to use the feature and
   discovering there was nowhere to turn it on. **Done.**

   The presentation design editor has a **Darstellungsart** field among the
   meta information: presentation view, or monitor view. Switching carries the
   fonts, colours and padding across (`PresentationDesignSettings::into_kind`)
   — the point of decision 1 — and losing the layout and widgets when
   switching away is documented rather than hidden. A monitor design is edited
   by `MonitorDesignSettings` *plus* the ordinary `DesignTemplateSettings`, so
   there is one font editor and one colour picker, not two.

4. ~~**`MonitorLayout::SlideList` and `Speaker`.**~~ **Done.**
   [monitor_view.rs](../../src/components/monitor_view.rs) draws both, and
   `PresentationPage` picks it over the audience renderer when the view this
   window is showing names a monitor design.

   `PresenterTextPanel` became `SlideList`, shared, with `interactive` and
   `context` props — the console passes neither and behaves exactly as before;
   the monitor view passes `interactive: false` because it shows and does not
   control. There is one slide list, not two.

   Two things this turned up. `StaticSlideRendererComponent` matched on
   `Template` alone and gave a monitor design the *default* template, so a
   slide on a stage monitor would have come out in Cantara's colours instead
   of the design's. And `peek_next_slide` had to be written: the speaker
   layout needs the slide after this one, and it has to cross into the next
   chapter — what follows the last verse of a song is the next element, which
   is exactly what a speaker wants to see.

5. ~~**Widgets: clock and chapter timer.**~~ **Done.** Both draw, in the corner
   they were given, redrawn by one timer per view rather than one per widget.
   The chapter timer counts from `chapter_entered_at` (stage 1) and warns by
   colour once its limit has passed — nothing on a monitor view interrupts a
   service.

   The clock made `chrono` a direct dependency. Neither the standard library
   nor this program can turn a count of milliseconds into a *local* time on any
   target, and the clock on a stage monitor is the clock on that building's
   wall. It was already in the lock file, so it costs no compilation. Date and
   time patterns live in `locales/common.yml`, because the way a date is
   written belongs to the language rather than to a setting — and they are
   numeric, since `chrono` writes month names in English only.
6. **`MonitorLayout::Custom`,** with the context above and the error-in-view
   behaviour.
7. **Custom widgets,** Wasm first, behind the import opt-in — only if 1–6 have
   been in use for a while and the need is still real.

## What stage 3 found

Two things the plan above did not know, both from reading the network side
rather than from building it.

### The reserved list was wrong

`check_network_path` was written in stage 2 against the console's router, which
claims `/console` and `/assets`. But *two* routers are merged onto that one
socket, and the stream's
([stream/server.rs](../../src/logic/stream/server.rs)) puts `/state`,
`/events`, `/abcjs.js`, `/media`, `/video` and `/login` at the top level beside
them. `/video` is a perfectly natural name for a view, and it would have been a
panic in the server thread at the moment a service started — with the helper
still reporting itself as up.

Fixed, with the full list in `RESERVED_PATHS` and a test in the stream server
asserting each route it declares is refused to a view. That test is the link
between the two, since the settings cannot name the stream server on every
target.

### An `f64` timestamp does not survive `serde_json`

Stage 1 stored `chapter_entered_at` as milliseconds in an `f64`, reasoning that
the browser's clock is one anyway. It round-tripped through JSON in the test
that checked it, and then the *suite* began failing about one run in three — in
`test_running_presentation_serialization`, and sometimes elsewhere, on a
presentation that had become unequal to itself.

`serde_json` reads floats back approximately by default; exact round-tripping is
behind its `float_roundtrip` feature. `1788605525443.4739` written out came back
as a different number. Every path this value takes is a JSON one — to the helper
process, to the browser tab the web build synchronises through — so the type is
now an `i64` count of milliseconds, which JSON carries exactly. Nothing here
wanted sub-millisecond precision; the widget counts in seconds.

Two lessons, both in the code as comments:

* A round-trip test that samples one value cannot establish that a type
  round-trips. The test now takes ten thousand.
* The tests that check the chapter clock were asserting that a timestamp
  *changed*, which at millisecond resolution is not observably true inside one
  test. They now mark the field with a value the program would never write, so
  "restarted" and "left alone" both have a definite answer regardless of how
  fast the machine is.

### Multiple network views need per-view slides, which do not exist yet

The plan treated 3b as plumbing: give the helper a set of paths instead of one.
It is not, and the reason is in the data rather than in the server.

A view that differs from the projection needs its own division of the song, and
that division is currently *the stream's*, singular, baked into the chapter:
`SlideChapter::stream_slides`, `stream_slide_map` and `stream_design_option`
([states.rs](../../src/logic/states.rs)), with `Division::{Projection, Stream}`
naming the two. Everything that counts slides, maps a projection slide to the
one a phone is showing, or publishes state to a viewer is written against that
pair.

So serving N network views means generalising "the stream's second division"
into "each view's division" — through `presentation.rs` where chapters are
built, `stream_view.rs` where the mapping is worked out, `states.rs` where it is
counted, and the stream protocol that publishes it. That is a real piece of
work, and it is the same piece of work stage 4 needs in order to draw a monitor
view that is looking somewhere else.

Doing 3b first would mean building a server that serves N paths with identical
bytes: no observable difference, and no test that can tell a correct
implementation from a broken one. The recommendation is therefore to **reorder**
— generalise the division from a pair to a list first, then let both the
network views and the monitor layouts land on top of it:

* **3b′.** `Division::{Projection, Stream}` becomes a per-view division;
  `stream_slides`/`stream_slide_map` become one per view that asks for one.
  Pure, and testable exactly where `stream_view.rs` is tested today. No
  behaviour change: the stream is the one view that has a second division.
* **4.** The monitor layouts, which now have somewhere to get their slides
  from.
* **3b.** The helper serves a nested router per network view. Now there is
  something different at each path, and a test can say so.

Until 3b lands, a `Network` view other than the stream's own `/` is a
configuration that the settings will accept and nothing will serve. Stage 3a
does not create one — the migration makes exactly the stream view it always
had — but the view editor must not offer to make one before 3b is built.

## Testing

The pure parts carry the weight, as in
[stream_view.rs](../../src/logic/stream_view.rs), which tests the whole
projection-to-stream mapping without a socket, a window or a song file:

* Migration: old settings JSON in, expected `View` list out. One case per old
  configuration, including "stream off" and "no designs at all".
* Template context: a `RunningPresentation` in, the JSON above out. First
  slide, last slide, single-slide chapter, empty presentation.
* Template rendering: a template that does not compile, one that references a
  missing key, one that is fine. The first two must render an error, not panic.
* Path validation: collisions with `/console`, with the asset prefix, with each
  other; empty; characters that are not allowed.
* Slide-division constraint: a monitor view whose division straddles a
  projection slide is corrected the way `stream_slide_settings` corrects it.

What needs a real run: a window per view on a real second screen, a network
monitor view opened in a browser, and a screen unplugged mid-presentation.

## Decisions taken

1. **`MonitorDesign` embeds `PresentationDesignTemplate`.** One font editor, one
   colour editor, reused. The fields a monitor layout ignores are documented as
   ignored rather than removed.
2. **Custom widgets are WebAssembly only.** No user-supplied JavaScript, in this
   version or later ones unless the case is made again. The sandbox is the
   feature.
3. **Views are switched on and off from the selection screen, not the console**
   — including while a presentation is running. The console *reports* what is
   up; the selection screen is where it is changed. See
   [Switching views mid-service](#switching-views-mid-service).
4. **A monitor view may show a different chapter or slide from the projection.**
   This is a change to the model, not a detail — see [Focus](#focus).
5. **`Custom` templates are stored as files** beside the settings, referenced by
   name, not inlined into the settings JSON.

### Focus

Decision 4 removes the assumption that every view is looking at the same place.
A band monitor showing the next song while the sermon is on the wall is a real
request, and the model has to carry it.

What it does *not* remove is the reference view. Slide numbering, the console's
counting and the whole-multiple constraint on slide divisions still need one
authoritative sequence, and that stays the projection. What changes is that a
view names where it is looking *relative to* that sequence:

```rust
pub enum ViewFocus {
    /// Show whatever the reference view shows. The ordinary case, and the
    /// only one the reference view itself may have.
    Follow,

    /// Show a fixed chapter, from its first slide, regardless of where the
    /// projection is. The band monitor on the next song.
    Chapter { index: usize },

    /// Show a fixed chapter and slide within it.
    Slide { chapter: usize, slide: usize },
}
```

Rules:

* The reference view is always `Follow`. Anything else is refused, not
  clamped — a projection that has stopped following the operator is not a
  degraded state to recover from, it is a service going wrong.
* A focus naming a chapter that no longer exists falls back to `Follow` rather
  than to a blank screen, and the console says so. Chapters are rebuilt
  whenever the selection changes (`update_presentation`,
  [presentation.rs:745](../../src/logic/presentation.rs:745)), and a monitor
  pinned to chapter 5 of a selection that now has three is an ordinary
  consequence of editing during a service.
* A pinned view still gets the whole `RunningPresentation`; only the position
  it renders differs. Nothing new travels to the helper.
* `elapsed_in_chapter` is the *reference* view's chapter time. A pinned view
  showing a chapter nobody is in has no meaningful elapsed time, and the
  template context reports `null` there rather than a number that means
  nothing.

### Switching views mid-service

Decision 3 puts the switch on the selection screen, which is where the
presentation options already live
([selection_components/presentation_options.rs](../../src/components/selection_components/presentation_options.rs)).
The presentation goes on running while the operator is back on that screen —
that is already true today — so enabling a view has to open a window or add a
route against a live presentation, and disabling one has to close it without
touching the others.

That makes `enabled` a value the running presentation reacts to, not just a
starting condition, and it is the reason stage 3 of the work plan is about
driving windows and routes from the list rather than reading the list once at
start. A view toggled on mid-service is shown the presentation as it stands,
immediately; it does not wait for the next slide change.

### Templates as files

Decision 5: a `Custom` layout holds a file name, not a template body.

* Templates live in a `templates/` directory beside the settings file, so that
  the same folder that is backed up carries them.
* `MonitorLayout::Custom { template: String }` becomes
  `Custom { template_file: String }`, holding a bare file name — not a path.
  Anything with a separator or a `..` in it is refused when the settings are
  read, for the same reason the video handler refuses them
  ([network_server.rs](../../src/logic/network_server.rs)): a design file is
  something a user is sent, and a template name is not allowed to reach out of
  its directory.
* A named template that is missing renders the error-in-view, like one that
  does not compile. It does not stop the view from opening.
* Design export ([settings_io.rs](../../src/logic/settings_io.rs)) has to carry
  the template file alongside the design, or an exported monitor design arrives
  broken. This is the same problem the font and image carrying already solves
  there.

## The view editor

The missing link, and the one thing between the feature working and the feature
being usable. Everything underneath it exists: `Settings::views` is migrated
into and validated, `place_screen_views` decides which window goes on which
screen, `open_view_window` opens them, and `MonitorViewComponent` draws a
monitor design correctly. What nobody can do is *make a view*.

It belongs in the settings, beside the presentation designs, and it needs:

* A list of the views, each showing its name, its design and its output, with
  add and delete.
* For a `Screen` view, a monitor chosen from `enumerate_monitors` — or "any
  free screen", which is what `None` means and what `place_screen_views`
  already handles properly for more than one view.
* A design chosen from `presentation_designs`, where a monitor design is what
  makes the view a monitor view.
* The reference view marked, and not deletable — everything is counted against
  it. `Settings::reference_view_index` is already there.
* **No `Network` output offered yet.** Stage 3b is not built, so a network view
  other than the stream's own `/` would be a configuration the settings accept
  and nothing serves. The path validation exists and is right; what is missing
  is the server that would use it.

Decision 3 also wants enabling and disabling from the *selection screen* while
a presentation runs, which is a separate piece: `enabled` is currently read once
when the presentation starts.

## Still open

* Whether a pinned view should be able to *follow with an offset* ("always the
  next chapter") rather than a fixed index. It reads as the thing people
  actually want for a band monitor, but it needs the fallback rules thinking
  through again, and `Chapter { index }` is enough to learn from first.
* Where the UI for setting a view's focus goes. The selection screen owns
  enabling; focus may belong there too, or beside the view's own definition in
  the settings.
