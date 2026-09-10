# 0003 — Monitor views, and the end of the fixed two outputs

Status: **in progress.** The five decisions below are taken. Stages 1, 2, 3a and
3½ are built; the order of what remains has changed — see
[What stage 3 found](#what-stage-3-found).

The chain is now complete for a **screen** monitor view: a design is made in the
editor ("Darstellungsart"), a view is added on the selection screen and pointed
at that design and a screen, and the window it opens draws the layout with its
widgets.

The network stream now draws with those same components too — see
[One rendering, not two](#one-rendering-not-two). A phone is served markup made
by the projector's own components, so what the room sees and what a pew sees
cannot drift apart by one of them learning about a feature and the other not.

The speaker layout can put the next slide **below or beside** the current one,
and both are drawn at the presentation's own size and scaled to fit — see
[Fitting a slide into a smaller box](#fitting-a-slide-into-a-smaller-box).

Not done: enabling a view *during* a running service, `MonitorLayout::Custom`,
and WebAssembly widgets.

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

**Built.** `ViewList` in
[presentation_options.rs](../../src/components/selection_components/presentation_options.rs),
on the selection screen's *Allgemein* tab — beside the running order, where the
rest of a service's decisions are made, rather than in the settings.

Each row is one view: its name, where it goes, the design and the slide
division it shows it with, and whether it is on. There may be as many as the
service needs.

It replaced two things that were the same three choices written twice — the
general design-and-division pair, and a panel of its own for the stream's pair.
A third output would have been a third copy. The `DesignSelect` and
`SlideSettingsSelect` components already existed and are reused unchanged, so
every list of designs in the program still offers the same names.

What is deliberately not offered:

* **A network path.** Choosing "the network" gives a view the stream's own
  `/`. Stage 3b is not built, so any other path would be a configuration the
  settings accept and nothing serves. The validation for one exists and is
  right; the server that would use it does not.
* **An enabled switch on the network view.** Whether the stream runs is the
  switch below it, and that is deliberately not remembered between sessions —
  a view that carried it would start broadcasting services nobody asked to
  broadcast. The row says so instead.
* **Deleting the reference view.** `Settings::delete_view` refuses it, and the
  row shows why rather than a button that would fail.

### What this changed underneath

`StreamDefaults::of` read the stream's design from `StreamSettings` while the
editor wrote it to the view. Left alone, a design chosen in the new list would
have quietly done nothing. It now reads the view — one place, and
`StreamSettings::design_index` is what `ensure_views` migrates *from* rather
than what anything reads.

### Still to do here

Decision 3 also wants views enabled and disabled *during* a running service.
The switch exists, but `enabled` is read once when the presentation starts, so
a view turned on mid-service does not open until the next one. That is the
remaining half of stage 3a.

## One rendering, not two

Found by using the feature: a monitor design set on the stream view reached a
phone as a plain wall of text. The layout and the widgets were not missing from
the data — the page serving viewers had simply never heard of such things.

Cantara drew a slide **twice**. The window drew it with the components in
`presentation_components`; `assets/stream_viewer.html` drew it again, in ~800
lines of JavaScript, from a description that `stream/protocol.rs` built for it.
A second renderer only ever knows the features the first had when it was
written, which is a duplication that cannot be kept in step by discipline.

So the page stops rendering. What it is given is HTML produced by the same
components, through `dioxus-ssr` —
[stream_render.rs](../../src/components/stream_render.rs). A feature added to a
slide, a design or a monitor layout reaches the network by existing.

### Why SSR and not a liveview session per viewer

The remote presenter console solves the same problem with `dioxus-liveview`:
the component runs on the server and the browser is sent DOM patches. That is
the obvious precedent, and it was rejected here for one reason — **scale**.

A console is one operator. A stream is the congregation. Liveview means a
server-side `VirtualDom` and a websocket *per viewer*; a hundred phones is a
hundred `VirtualDom`s in the helper process, on a church laptop that is also
driving a projector. The stream was built as a static page precisely to fan one
rendering out to many readers, and that property is worth keeping.

Server-side rendering keeps both: **one** renderer, and **one** render per
change rather than per viewer.

### Why in Cantara and not in the helper

The obvious place to render is the helper, next to the socket. It is the wrong
place, and both reasons come down to the helper deliberately knowing nothing:

* **Pictures.** A background and a picture slide are inlined as data URLs out
  of `logic::images`, whose cache is filled from the library on disk. The
  helper has neither. Cantara has both, warm, because it is already showing the
  same slide on the projector.
* **Settings.** Which design a view uses is a setting, and the helper has none
  — so that a service cannot be changed by whatever reaches the socket.

### What this turned up

`PresentationRendererComponent` gated its slide behind a signal that only
became true in `onmounted`. In a browser that is a trick to replay the entry
animation; in any rendering without a mount event it means **there is no slide
at all** — the first SSR of an audience design produced a background and
nothing else. It now starts shown, and the animation is unaffected because a
CSS animation plays when the element is inserted either way. Content that
exists only after a browser event is content no server-side rendering can
produce.

### What is done, and what is left

Done and tested (nine tests in `stream_render`): the rendering entry point,
audience designs, monitor layouts, widgets, corner placement, stability of the
output across identical renders, and that moving the presentation changes it.

Also done: `for_network`, which rewrites the addresses in a rendering so they
mean something on another device. A rendering is made for the machine that made
it, and `/cantara-video/…` is answered by the asset handler *inside* Cantara's
web view. On the WebKitGTK platforms it is worse — a video's `src` is an
absolute `http://127.0.0.1:…` URL, and loopback on a phone is the phone. Both
become `video/…` on the stream's own origin. Pictures need nothing: they are
inlined as data URLs and carry their own bytes. Six tests, including the two
failure modes above and the rule that the encoded path is what the server looks
the file up by and must come through untouched.

### The switch-over

Done. `network_host::publish` renders the slide once per change and sends it
with the presentation; the helper passes it into `StreamState::html`; the page
puts it on the stage. Which design is not a question the caller has to answer —
`get_current_stream_design` is what `StreamState::of` already read, so the
rendering and the rest of the payload cannot disagree about what the phones are
being shown.

Deleted from `stream_viewer.html`: `applyDesign`, `currentSlide`, `showMeta`,
`notationBlock`, `dressed`, `spaced`, the `#backdrop` element and the `#meta`
element — every piece that existed to turn a payload into a slide. A test
asserts three of those function names never come back, because a second
renderer is easy to reintroduce a function at a time and each addition looks
reasonable on its own.

Two details the switch turned up:

* **Redrawing on every update would restart the video.** Updates arrive several
  times a second while a video plays. The stage is now rebuilt only when the
  *markup* changes — a moving video does not change it — and where the new
  markup names the file already playing, the element already playing is put
  back in its place rather than a fresh one being downloaded.
* **Notation was not in the markup at all.** `AbcNotationRenderer` emitted an
  empty box and engraved it from an `onmounted` handler, so a rendering made
  without a mount event carried no notation anywhere. The source now travels in
  `data-abc` and `data-vocal-font`, and the page engraves from those with the
  same library. The same lesson as `presentation_is_visible`: markup that
  describes itself is markup any renderer can finish.

### Three things that only showed up in a browser

All three are the same shape: something the *window* supplies at runtime, which
a rendering made without a window cannot.

* **A video showed as an empty rectangle over the background.** The address was
  rewritten to the encoded file path, but the server files the videos of a
  running service under `media_id` — an MD5 of the source — because that is how
  every other piece of media it serves is addressed. The element was there and
  nothing answered it. `for_network` now decodes the path and hashes it, so the
  address is the name the server registered.
* **A PDF page showed nothing at all.** It is drawn by pdf.js into a canvas,
  and a rendering without a browser carries an empty box. The page already
  travels as a picture — Cantara renders it and sends the bytes, as it always
  has — so the canvas says which page it depicts in `data-pdf`/`data-page` and
  becomes the `<img>` that asks for it. The rewrite needs to know nothing about
  PDFs beyond the name the picture is filed under.

  Then it appeared at its own size in the middle of the design's background.
  `slide_container_style` gives a picture and a video a cell with a height —
  `height: 100%` against a parent that has none is nothing — and a PDF page was
  not on that list. It never showed here, because pdf.js sizes its own canvas;
  over the network the page is an image, and an image told to fill a cell with
  no height fills nothing. A page is now fitted exactly as a picture slide is,
  from the *same* two style constants rather than a second set of rules that
  happened to look similar — it is a picture there, so it has to be fitted like
  one.
* **`Unable to find a document in the renderer`, at error level, on every
  render.** `document::Link` and `document::Script` — which every slide
  renderer uses — look one up in the context and log when there is none. The
  SSR root now provides a `NoOpDocument`: the same fallback they were using,
  except that finding it is not an error. A no-op is right here in any case,
  since the markup is a fragment and whoever serves it says how it is dressed.

The pattern is worth naming, because it has now caught four things — the mount
gate on `presentation_is_visible`, the notation source, the PDF page, and the
document. **Markup that depends on a browser event to become itself is markup
no server-side rendering can produce.** Where the window fills something in
after mounting, the value has to be in the markup as well.

### Verified

The served page was rendered to a file and opened in a browser. A PDF page's
geometry was measured there: a 1280×720 stage, a 1240×680 cell inside the
design's padding, and the picture filling it with `object-fit: contain` — the
same box a picture slide and a video get.

For a song slide: The stage
carries the design's own values, inline, exactly as the window does: black
background, white text, `place-items: center stretch`, `text-align: center`,
main content at 32pt (42.67px) and the spoiler at the design's ratio (29.87px),
with the fade class applied. Four stylesheets load. Content and styling match
the projection.

Also tested against a running server: the rendering reaches `/state`, and the
page carries a rule from each of the three component stylesheets — dropping one
would leave a monitor design's slide list unstyled and nothing else would
notice.

### Two things server-side rendering cannot finish by itself

Worth stating plainly, because "the browser looks exactly like the local
screen, across every source" is not something SSR delivers on its own:

* **Video.** SSR emits the `<video>` element and `for_network` points it at the
  right file, but *where the video is* is a running value. The existing
  `StreamVideoState` — playing, position, duration, sent with every update — is
  the right mechanism and stays; the page keeps the small piece of script that
  pulls its element onto that position. This is synchronisation, not rendering,
  and it does not belong in the markup.
* **Notation and PDF pages.** Both are drawn *by the browser* locally: `abcjs`
  engraves a staff from its source, and pdf.js renders a page to a canvas. SSR
  emits the container and no more. The stream already solves the PDF case by
  sending the page as a picture (`media`), which is the same answer this should
  keep; notation needs `abcjs` to run on the delivered markup, exactly as it
  does in the window.

So the honest shape of the finished thing is: **one renderer for everything
that is layout and text, and a small amount of script for the three things that
are inherently client-side** — video position, staves, and PDF pages. That is
not a second renderer; it is the same markup being finished where it is shown.

## Fitting a slide into a smaller box

A slide's type is set in **points**, which are absolute. Put the renderer into
a smaller box and the words come out at their full size and overflow it: the box
shrinks and the text does not. Every other preview in Cantara therefore draws
the slide at its native size and shrinks the whole thing with a `transform`, and
the speaker layout has to as well — its two slides are exactly that, a slide in
a box that is not the size of a screen.

What is different here is that the factor cannot be worked out in Rust. The
other previews compute it from a width they know: a thumbnail is 400 pixels
because something said so. This box is a share of a window whose size is the
operator's business, and on the network a share of a phone nobody here can
measure. Measuring it in the browser would work in a window and nowhere else —
the stream is served as markup with no Dioxus behind it, so there is no
`onmounted` to measure in. That is the same rule that has now caught five other
things in this feature.

So the browser works it out, from the box itself:

```css
.monitor-slide-frame { container-type: size; }
.monitor-slide-stage {
    width: var(--slide-width);          /* the presentation's own size */
    transform: scale(min(
        calc(100cqw / var(--slide-width)),
        calc(100cqh / var(--slide-height))
    ));
}
```

`min()` of the two ratios is the one that fits in both directions, which is what
keeps the proportions. The size to scale *from* comes from
`RunningPresentation::layout_size` — the width the presentation window measured
— so the slides break their lines where the wall breaks them rather than at some
assumed 1920.

Measured in a browser at 1280×720 with the next slide to the right and a quarter
share: current 912×691, next 306×693, the stage declared 1920×1080 and scaled by
0.475008. 1920 × 0.475008 = 912, which fills the width exactly; 1080 × 0.475008
= 513, which fits inside 691. The narrower ratio won, as it should.

An engine that cannot divide two lengths drops the declaration and draws the
slide unscaled — visibly wrong rather than silently squeezed, which is the
better of the two failures.

## 3b′: one division per view

`Division` was a pair — `Projection` and `Stream` — from when a service had
exactly two outputs, and a chapter carried one alternative set of slides to go
with it: `stream_slides`, `stream_slide_map`, `stream_design_option`. A second
network view had nowhere to put its own division, which is why 3b could not be
built on top of it.

Now `Division` is `Projection` or `View(Uuid)`, and a chapter carries
`view_slides: HashMap<Uuid, ViewDivision>` — a design, a set of slides and the
map from the projection's slides to them, per view that asked for one. A view
that asked for nothing gets no entry at all, so an ordinary service still holds
exactly one set of slides.

Keyed by the view's identity rather than its position, for the reason the
identity was added in the first place: a running order outlives an edit to the
view list, and a chapter built while "Stream" was second would otherwise start
describing whatever became second after a view above it was deleted.

`StreamDefaults` became `ViewDefaults` and answers for every view rather than
for "the stream"; `StreamState::of` is told which division it is building for;
the helper is told which view it serves, so its state, its pictures and its
videos cannot disagree about what a viewer is looking at.

Sixty-seven call sites across seven files, and the projection is still the
reference: slide numbers, the console's counting and the whole-multiple rule on
divisions are all described against it, exactly as before.

## 3b: several views on one socket

Built on top of 3b′, and — as expected once the model was right — a change to
the server alone.

The server holds a state **per view** rather than one, and a map from address
to view. `/state` and `/events` take the view as a query parameter; the page is
told its own identity when it is served and sends it back with everything it
asks for. Without that, two addresses would be two ways to the same slides.

### Why the addresses are a lookup and not routes

The router is built when the server goes up. What is on offer arrives
afterwards — Cantara says so over the socket, and may say something different
halfway through a service. A route cannot be added to a built `axum` router; a
lookup can change under one. So a view's address is answered by the **fallback**
handler, which resolves the path against what is currently offered and serves
the page for that view, or a plain not-found for an address nobody was given.

### One channel, not one per view

Every viewer waits on the same channel and each remembers the revision it was
last told. A change to one view wakes them all; the ones whose own view has not
moved go back to sleep without sending. A hall's worth of phones is a hall's
worth of sleeping tasks either way, and one channel is one thing to keep in
step.

### The editor offers addresses again

A network view's path is editable, validated by the `check_network_path` that
was written in stage 2 and until now had nothing to guard. A path already taken
by another view is refused as well: two views on one address is two handlers on
one path, and the second would never be reached. A new network view starts at
the bare address if it is free and at one named after itself otherwise.

Rendering is still **once per view per change**, not once per viewer — the
property that made a static page the right answer for a congregation, and it
survives having several of them.

## What the Android build said

The mobile build broke on `stream_render`, and the error was worth more than
the fix. `path_of_video_url` is `#[cfg(feature = "desktop")]`; the renderer
that called it was `#[cfg(not(target_arch = "wasm32"))]`, so on a phone it was
compiled without the thing it needed.

The real fault was underneath: **the whole network side is desktop-only and the
module gating did not say so.** `network_host` and `network_server` are gated
on `desktop`, but `logic::stream` — the protocol *and* the server — was gated
on "not the browser", so a phone build carried a server it can never start and
a protocol nothing there can speak. Every item in it warned as unused, which is
the compiler saying exactly that.

Gated with the thing they exist for, the warnings go and the binary loses code
it could not use: `logic::stream`, `components::stream_render`, the publishing
effect in `main` with its two helpers, and the `Range`-header parsing that only
a video server needs. Verified across all four configurations — desktop tests
and clippy, `--features mobile`, and the wasm target.

The lesson is the same shape as the one about markup: **gate a module on what
it is for, not on what it is not.** `not(wasm32)` is not a statement about
anything; `feature = "desktop"` is.

## Three follow-ups from a real service

**The console's stream preview drew a plain slide.** It reached for
`StaticSlideRendererComponent` directly, so the operator saw a single slide
while the phones showed a stage monitor. Every other preview in the program had
already been routed through `DesignedPresentation`; this one had been missed.
Fourth place, same fix.

**The clock stood still on a streamed monitor.** A window redraws its widgets on
a timer of its own. A view served over the network is HTML rendered *when
something changes*, and a second passing is not a change — so the time a slide
came up stayed on the phone until the next slide. `refresh_time_widgets` renders
again once a second, and only for views whose design actually carries a clock or
a chapter timer (`MonitorDesign::has_live_widget`): an ordinary service must not
pay for a slide rendered every second.

**The slide was not scaled at all.** The speaker layout fitted its slides with

```css
transform: scale(min(calc(100cqw / var(--slide-width)), …));
```

which says exactly the right thing and is dropped whole by any engine without
container-query *division*. Cantara's own web view is one of them, and a dropped
declaration is no scaling: the current slide overflowed its box and the next one
beside it was drawn at full size. I had flagged the support risk when writing
it; flagging is not verifying.

It is measured now, by `assets/monitor_slide_scale.js` — **one file**, which the
window evaluates when a slide frame mounts and the served page runs after it is
given new markup. The arithmetic is the same arithmetic; a second copy would be
a second thing to get wrong.

### And then nothing was drawn at all

The first version set `transform: scale(0)` until the measuring had happened,
reasoning that a wall of oversized letters is worse than a blank frame. That was
wrong twice over. A frame that is never measured is then blank *for good* — and
a frame is often not laid out in the tick it was put on the page, so measuring
once and returning early on "no size yet" left it at nothing. The monitor showed
its clock and its timer over an empty screen.

Two changes, and the second is the one that matters:

* **Watched, not measured once.** A `ResizeObserver` is told the moment a box
  *has* a size, which is exactly the moment worth measuring, and again whenever
  it changes. Measuring on a timer would have been guessing at how long to wait.
* **The fallback is never invisible.** The container-query form is back as the
  CSS default: where the engine supports it the slide is right before a line of
  script has run, and where it does not the declaration is dropped and the slide
  is drawn at full size — wrong, and *visible*. The script then sets the real
  one either way.

Measured in a browser at 1280×720, without touching anything by hand: a 912×691
frame settles at scale 0.475 → 912×513, and the 304×691 frame beside it at
0.1583 → 304×171. Both fill their frames, both still 16:9.

Two lessons, and they are the same lesson: **CSS that is exactly right and
unsupported is worse than arithmetic that is dull and runs**, and **a default
that hides everything when one step does not run is the same failure wearing a
different hat**. Both fail silently and look like nothing was written.

## The refresh took the other addresses down with it

Adding the once-a-second refresh for clocks broke the addresses that have none.
It sent a `rendered` map holding **only** the views with a clock on them, and
the helper replaces what it has with what it is given — so a second after the
presentation started, the bare address had no rendering at all and showed the
page's "…" over an empty screen.

The fix is one function. `render_for_views` is now the only place a rendering is
made, and both the ordinary publish and the refresh go through it: what an
address is shown cannot depend on which of the two last spoke, and the two
cannot come to disagree about which design a view uses. The refresh still asks
first whether *anything* being served follows the clock, so an ordinary service
pays nothing — but when it does render, it renders for everything.

Rendering the quiet views again costs a slide or two per second. That is worth
more than the saving, because the saving is what broke it.

## Fitting, and when a box has a size

The measured fit was still not landing in the browser. A `ResizeObserver` is
told when an *observed* size changes, and a box can arrive at its size without
that being a change the observer reports — so a frame measured too early stayed
unmeasured.

It now asks again on the next animation frame while the box has no size,
bounded at sixty tries and stopping if the frame has left the page, and the
served page re-fits on `resize` and `orientationchange` as well. Verified in a
browser by giving the wrapper its size *after* the script had already run once
against a zero-sized viewport, with no resize event fired by hand: a 912×691
frame settles at 0.475 and the 304×691 beside it at 0.1583, both exact.

The shape of all three of these is the same and worth saying once more:
**anything that depends on a single well-timed moment will eventually miss it.**
The mount that never came, the observer that was not told, the map that held
only part of what it replaced.

## A clock ticking is not a slide change

With the renderings restored, the fit was right for a second and then jumped
back. The cause was the refresh again, from the other side: a monitor view with
a clock is re-rendered once a second, so the markup differs every second — and
the page rebuilt its whole stage for it.

Rebuilding tore down and rebuilt everything on the stage every second. The slide
frames were re-created, so they lost the size they had been fitted to; the video
element was replaced and started again; every staff was engraved afresh. The
visible part of that was the scale jumping back once a second, but the video
restarting is the worse of the two.

The page now compares the two renderings with the widgets blanked out. Where
they are equal — the same slide with a different time on it — it replaces what
the widgets *say* and nothing else. Nothing moves, so nothing has to be measured
or engraved again and the video is not touched. Anything else is still a full
rebuild, and a mismatch in the number of widgets falls back to one rather than
putting one widget's words into another.

Two tests, on both sides of the assumption. In Rust: two renderings a second
apart differ only inside the widgets — if that ever stopped holding, the page
would rebuild and the jump would return. In a browser, with real markup: the
patch path is taken, the slide frame is *the same DOM node* afterwards, its
fitted transform is untouched, and the clock reads the new time.

## Video on a monitor view

A video slide on a stage monitor came out as a `<video>` that nothing ever
started: paused at nothing, which is a blank rectangle where the film should be.

The mechanism for this already existed and is the presenter console's. Its
overview shows a *picture* of the video that moves with the video — an element
that takes no commands, makes no sound and reports nothing, pulled onto the
published position a few times a second (`MirrorsTheVideo` and
`video::mirror_script`). A monitor view wants exactly that, and for exactly the
same reason: it shows, it does not control.

So it asks for the context rather than growing a video path of its own. One
line, and the rule the whole feature rests on stays intact — the projection is
the one that plays.

## The pipeline

Every command the workflow runs was reproduced locally and passes: `cargo test`,
`cargo clippy --all-targets -- -D warnings`, `cargo clippy -- -D warnings`,
`dx build -r`, and `dx build --platform web --release` — the last of which also
proved the new `monitor_view.css` asset is picked up by the bundler. The Android
job's build needs the NDK and could not be run here; its compile was checked
with `cargo check --release --no-default-features --features mobile`, which is
the same feature set the failing log showed.

One real weakness was found and fixed rather than guessed at. The two tests that
start a **real helper process** decided whether to run by comparing file times
between `target/debug/cantara` and the test harness. On a fresh checkout the
order those are built in is not fixed, so on a build server they ran or did not,
by coin flip — and a test that sometimes runs is worse than one that does not:
it fails for reasons unrelated to the change and the failure cannot be
reproduced. They now skip when `CI` is set, and say so. They still run locally,
which is where a process-spawning test belongs.

## Still open

* Whether a pinned view should be able to *follow with an offset* ("always the
  next chapter") rather than a fixed index. It reads as the thing people
  actually want for a band monitor, but it needs the fallback rules thinking
  through again, and `Chapter { index }` is enough to learn from first.
* Where the UI for setting a view's focus goes. The selection screen owns
  enabling; focus may belong there too, or beside the view's own definition in
  the settings.
