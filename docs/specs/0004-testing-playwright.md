# 0004 — Testing: what a service is allowed to do to us

Status: **all five stages done**, and the review of them applied — see "What
the review turned up" at the end. Stage 3 found a remote arbitrary file write
and the review found three more holes of the same kind; those two sections are
the ones worth reading.

Cantara has 735 tests over 55.000 lines, and they are good tests: named for the
behaviour they protect, most of them carrying the reason they exist. They are
also almost entirely *below* the place where the program actually breaks.

The evidence is a single sitting. Building [0003](0003-add-monitor-view.md) —
monitor views, several network views, one rendering instead of two — produced
these defects, in this order:

| What was reported | Found by |
| --- | --- |
| "Encountered panic: Any { .. }" when a presentation started | using it |
| A black strip and a white screen instead of a monitor view | using it |
| PDF and video slides showing only the background | using it |
| PDF pages at their own size in the middle of the design | using it |
| Both addresses showing the same view | using it |
| `/test` → not found, `/` → waiting | using it |
| Nothing at all on the monitor slide | using it |
| Scaling right for one second, then wrong | using it |
| Video on a monitor staying black | using it |

Nine defects, none of them caught by a test. Every one was found by a person
starting the program and looking at it. That is what this document is about.

The note this spec grew from puts it as "the stability of a church service
presentation should be handled with the same care as a nuclear reactor's safety
system". The instinct is right and the analogy is worth one correction, because
it decides where the effort goes: a reactor's safety case ranks hazards by
consequence and spends accordingly. So should this. **A blank screen in the
middle of a song is catastrophic. A colour two shades off is not.** The tiers
below are ordered by what failure costs during a service, not by what is
pleasant to automate.

## What can actually be reached

This is the first thing to settle, because getting it wrong would mean writing
a spec that promises coverage no tool can deliver.

**Playwright drives browsers.** Cantara's primary product is a desktop
application: a `wry` window, which is WebKitGTK on Linux, WebView2 on Windows
and WKWebView on macOS. Playwright cannot attach to any of those the way it
attaches to a browser it launched itself. So Playwright reaches:

* the **web build** (`dx serve --platform web`) — selection, detail view,
  settings, design editor, and the routed presentation and console pages;
* the **stream viewer page**, served over HTTP by the helper — a genuine web
  page, and the surface where most of the defects above lived;
* the **remote presenter console**, likewise served over HTTP.

It does **not** reach the desktop presentation window, the desktop console
window, or a monitor view on a second screen. Those are exactly where a service
happens, and no amount of Playwright changes that. What covers them is the tier
below.

## Three tiers, by what they can see

### 1. Markup tests — what a component *says*

`dioxus-ssr` renders a component to HTML with no browser at all, and
`assert_rsx_eq` compares two renderings. The crate is already a dependency and
[`stream_render`](../../src/components/stream_render.rs) already tests this way.

This tier is cheap, fast, runs in CI on every platform, and it is the one that
catches the class that hurt most in 0003. Five of the nine defects were the same
mistake wearing different hats: **markup that depends on a browser event to
become itself.**

* `presentation_is_visible` was false until `onmounted` fired, so a rendering
  without a mount had no slide in it at all.
* The ABC notation source lived only in the mount handler, so the staff was an
  empty box anywhere else.
* A PDF page was a `<canvas>` that said nothing about which page it was.
* No `Document` in the context, logged at error level on every render.
* A PDF slide's cell had no height, which never showed because pdf.js sizes its
  own canvas.

Every one of those is visible in the rendered HTML. A test that renders each
slide type through each design kind and asserts the markup carries what it
needs would have caught four of the five before anyone saw them.

**Scope:** every `SlideContent` variant × audience design × monitor design ×
both monitor layouts. Assert on *properties*, not on a golden string — a golden
file over generated markup breaks on every whitespace change and teaches people
to run the update command without looking.

#### What stage 1 turned up

Built as [`slide_markup`](../../src/components/slide_markup.rs), 17 tests over
the matrix, on fixtures in [`fixtures`](../../src/logic/fixtures.rs). Two
things came out of writing it that are worth recording.

**A defect, found by the second test that ran.** A notation staff and a PDF
canvas each carry an `id` for the script that draws into them, and both were a
fresh `Uuid` per render. So *every rendering of such a slide differed from the
last* — and the stream compares what it is about to send against what it last
sent, precisely so that an unchanged state is not pushed to every phone again.
That check could never hold. A notation slide on screen meant every phone in
the building rebuilding its page once a second, re-engraving the staff and
restarting any video on it, for as long as the slide was up. It also defeated
the patch-only-the-widgets path in `stream_viewer.html`, which exists to stop
exactly that.

The fix is the scope's own number instead of a random one: assigned by the
`VirtualDom` as it builds the tree, so it is stable for the same tree and
unique within it. Neither a random id nor a hash of the content has both
properties.

This is the argument for the tier in one example. The defect is invisible on
the machine it happens on, it needs a phone and a stopwatch to see, and it
looks nothing like an identifier when you are watching it.

**Fixtures have to be real.** The first draft named files that did not exist.
A picture slide is inlined by *reading* the file, so a made-up path renders as
`<div style="width: 100%; height: 100%;"></div>` — an empty box, with nothing
in the markup to say what it was meant to hold. The test asserting that a
picture is drawn passed against that. A fixture that cannot fail is worse than
no fixture, because it is counted.

### 2. Rust integration tests — the network side, end to end

The helper is a real process with a real socket, and it can be started from a
test. Two such tests exist already
([`network_host`](../../src/logic/network_host.rs)): one asserts the switch puts
a server on the network, one that two addresses serve two different views.

This tier owns everything the note lists under "error cases" and "resistance
against cyberattacks", because none of it is a browser's business:

* **A ZIP bomb in a remote repository.** `logic::settings` downloads and unpacks
  archives. The test is a crafted archive and an assertion that unpacking is
  refused before it fills the disk — a bound on the declared uncompressed size
  and on the number of entries, not a timeout.
* **A malformed PDF.** `lopdf` is handed a file that is not one. The assertion
  is that the element reports itself as unreadable and the *rest of the running
  order still builds*.
* **An address with characters that are not allowed.** Already covered by
  `check_network_path`; the integration half is that a rejected path never
  reaches the router.
* **Load on the stream.** Here the requirement needs restating, because "resist
  a DDoS" is not a property this program can have: it is a plain HTTP server on
  a hall network. The property that *matters*, and that is testable, is
  **nothing arriving on the socket may disturb the projection**. A hundred
  viewers connecting, disconnecting mid-slide, or asking for a video range that
  makes no sense must leave the presentation window untouched. That is a real
  assertion about a real risk.

#### What stage 3 turned up

The four items, in the order they were built. Two of them were not tests at
all in the end: the code they were meant to test did not exist yet.

**The ZIP bomb was worse than a bomb.** `logic::settings` unpacked a downloaded
archive with `temp_dir.path().join(file.name())` and `io::copy` — no bound on
the size, no bound on the number of entries, and **no check on the name**.
`Path::join` with an absolute path does not append, it *replaces*: an entry
named `/etc/passwd` would have been written to `/etc/passwd`. That is a remote
arbitrary file write, triggered by adding a song repository somebody sent you
a link to.

So the unpacking became [`archive`](../../src/logic/archive.rs): a budget on
total size, on any single file and on the entry count, and every path taken
through `enclosed_name`. The budget is checked **twice** — once against the
size the archive declares, which is cheap and turns away an obvious bomb before
a byte is written, and again while reading, because the declared size is a
number the attacker wrote. 14 tests.

This also removed a duplication that was the reason the hole existed in two
places: the desktop unpacked to a folder and the web build into a map, as two
copies of the same loop, so each would have had to grow the same checks
separately. What they share — *deciding what is safe to take* — is now one
function; where the bytes go is still the caller's.

Writing the fixtures taught something worth keeping: **a hostile fixture built
with a well-behaved tool is not hostile.** The `zip` writer normalises an
entry's name on the way in, so asking it for `/etc/passwd` produces a harmless
relative `etc/passwd` and the test passes for the wrong reason. The archives
that carry a name no writer will produce are laid out byte by byte, and there
is a test asserting that the fixture really does carry the name — a test of a
test, and not idle, since a fixture that quietly stopped being hostile is
exactly the failure this whole document is about.

**The damaged PDF was already survivable, and silent.** `build_presentation`
skips an element it cannot read and builds the rest — the important half, and
now asserted for four kinds of damaged file, for a file that is no longer
there, and for a damaged song. What it did *not* do was say anything: the arm
was `Err(_) => { // TODO }`. The element the operator put in the running order
is simply not there on Sunday, with nothing anywhere to say why, and the
natural conclusion is that they forgot to add it. It logs now. That is not
enough — this belongs in front of the person building the order, at the moment
they build it — and it is listed under "still open" below.

**The rejected address was not rejected.** `check_network_path` is called by
the editor, and by nothing else. The list of addresses handed to the helper was
built **twice**, in two places, and neither checked anything — and the two had
already begun to filter differently. A settings file is not the editor: it can
be hand-edited, or written by another version. And a bad address fails
quietly — a *reserved* one like `/console` is shadowed by the route that owns
it, so the view exists, the switch says the stream is on, and whoever opens
that address is shown somebody else's page. One `Settings::served_views` now,
which checks, drops what it must, and keeps the rest: one typo does not cost
the other two views their stream.

**The load property, restated.** "Resist a DDoS" is not something this program
can be, and a test claiming it would be a test claiming something false. What
is asserted instead is *nothing arriving on the socket may disturb the
projection*: 64 viewers at once, probes and traversal attempts, ranges that
make no sense, connections abandoned mid-response, connections that say nothing
at all — and after each, the service being served is still the service that is
running. Plus the half that is easy to forget: the operator can still press
"next" while all that is going on.

**One more thing the stage turned up, from a failure of my own.** Adding the
damaged PDFs to `testfiles/` broke two unrelated tests that count the documents
in there, with the message `6 != 2`. `testfiles/` is *the library* — what a
church would have — and test inputs that are not library content do not belong
in it. Those now live in `fixtures/`. The two counting tests name the documents
they expect instead, so the next failure says which file was missing rather
than arithmetic.

### 3. Playwright — the browser surfaces

`playwright.config.js` starts `dx serve` on port 8080 with
`reuseExistingServer: !process.env.CI`, as the Dioxus guide describes.

What is worth testing here, in order of what failure costs:

1. **The stream viewer.** Open the address, see the slide the projection shows;
   follow a slide change; a monitor design shows its layout and not a wall of
   text; the clock updates without the slide being rebuilt; a video plays and
   follows the room's position. Almost the whole of 0003's damage was here.
2. **The remote console.** Log in, drive the presentation, see the projection
   follow.
3. **The web build.** Choose songs, build a running order, open the settings,
   edit a design, see the preview change.

#### What stages 4 and 5 turned up

**Stage 4 does not fake the server.** The tempting shortcut was to serve the
viewer page from a little test server with a canned state behind it. That would
have been quick and worth very little: every defect worth catching here lives
in the path *between* Cantara and the page, and a canned state is that path
removed. So [`harness`](../../src/logic/harness.rs) calls `enable_viewer` and
`publish` — the same functions the stream switch and the presentation loop
call — and everything after them is real: the real helper process, the real
rendering, the real socket, the real page. What is replaced is the window: a
test asks over HTTP instead of an operator pressing keys.

It is behind a **cargo feature, not a flag**. It opens a port and takes
instructions on it, which is exactly what the rest of this program is careful
not to do — the network side is a separate process that knows nothing precisely
so that a service cannot be changed by whatever reaches the socket. "Only
reachable if you pass `--harness`" is reasoning that holds until somebody finds
a way to pass it. A released Cantara does not contain this code.

16 tests in [`tests/browser`](../../tests/browser/), and writing them found
three things:

* **The harness's own claim was too strong.** It said it ran "the same two
  functions the presentation loop calls"; the loop calls more than two. Two
  tests failed because of it — the clock on a streamed monitor never moved,
  because `refresh_time_widgets` is called on a timer by Cantara's loop and the
  harness had no timer. That is a defect the harness would have *hidden*: the
  browser test would have gone green on a Cantara whose streamed clocks had
  frozen.
* **The harness fell back to another port in silence.** Cantara moves to a free
  port when the chosen one is taken, which is right for a person and wrong
  here — the tests open a fixed address, so a moved harness left Playwright
  waiting on a port nothing answered, and the message was "the server never
  started". Scaffolding failing the way the product must not is not acceptable
  in a suite about exactly that. It now refuses to start.
* **A PDF page cannot be rasterised without a window.** That rendering is
  pdf.js, inside the web view. The harness stands a known picture in under the
  right name, which tests the half that actually broke in 0003 — the markup
  asked for `media/<id>` and the server held the page under a *different* id.
  A browser test here may assert that a page resolves to a picture the viewer
  can load. It may not assert that the picture is the right page.

**Stage 5 measures geometry, as decided.**
[`measure`](../../src/logic/measure.rs) opens the real projection window, draws
five services through `DesignedPresentation` — the same decision point the
projection uses — and checks the numbers. Every rule in it is one of 0003's
reported defects turned into an inequality: a stage of no size, a stage that is
a strip, a slide scaled to nothing, a slide overflowing its frame, a text slide
with no text.

Building it found a duplication of the worst kind. The Linux window preparation
(`GDK_BACKEND`, `WEBKIT_DISABLE_DMABUF_RENDERER`) lived inside `launch_app` and
nowhere else, which was right while that was the only thing opening a window.
The measure window came up blank without it — **a measuring rig reproducing the
exact failure it exists to detect.** Same again with the page's wrapper: without
`all: initial; width:100%; height:100%` the stage measured 1264×377 in a
1280×720 window, and the first version of these checks reported that as fine.
Both are now one shared thing (`window_platform::prepare`,
`PRESENTATION_WINDOW_STYLE`).

And a smaller lesson worth keeping: **stdout is block-buffered when it is not a
terminal.** Every diagnostic this mode printed sat in a buffer until the process
ended, so a cut-short run produced nothing and looked identical to a hang. The
runs worth reading are exactly the ones that do not exit cleanly.

**Honest status of stage 5.** Its rules are unit-tested and it produced real
measurements against a real window — but the machine it was built on stopped
opening GUI windows part-way through (Cantara itself no longer opens one there
either), so the end-to-end run could not be repeated. It is wired into CI under
`xvfb` with `continue-on-error` until it has proved itself on a runner, which is
the honest place for a check nobody has yet watched go green twice.

### What no tier reaches

The desktop presentation window and a monitor view on a second screen. Naming
this plainly is part of the spec, because pretending otherwise is how a suite
comes to be trusted for something it does not do.

Two ways to close it, neither free:

* **A screenshot mode.** Cantara starts, builds a presentation from a named
  file, renders it, writes a PNG and exits. Comparable in CI, and it exercises
  the real window and the real design pipeline. It does not exercise input.
* **WebDriver against WebView2**, on Windows only, which is the one platform
  whose web view speaks a protocol Playwright understands.

The first is the better first step and is a small program. Until one of them
exists, the desktop window is verified by a person, and the release checklist
should say so rather than implying the suite covers it.

## The test environment

The note asks for a fixed environment, and it is right: a suite that depends on
whatever library the machine happens to have is a suite that fails for reasons
nobody can reproduce.

* **Songs:** [`cantara-songrepo`](https://github.com/reckel-jm/cantara-songrepo),
  pinned to a commit rather than a branch. A moving fixture is not a fixture.
* **PDFs and videos:** public-domain files, fetched once and cached, with their
  checksums recorded. A long PDF for the "high pressure" case; a short video
  with a known duration so a position assertion means something.
* **Settings:** a set of prepared configuration files — one per shape worth
  testing: a plain projection, projection with a stream, projection with a
  stage monitor, three views at three addresses. These double as the migration
  fixtures below.

## Migration, which is the one irreversible thing

Not in the original note, and it belongs here more than anything else in it.
`Settings::ensure_views` rewrites the configuration of every existing user on
first start. A test suite that covers the whole program and not this is
covering the wrong end: a presentation that fails can be restarted, and a
configuration that has been rewritten wrongly is gone.

Real settings files from real installations — 0.2, 0.3, one with monitor views —
read, migrated, and asserted to produce the same service they described before.

#### What stage 2 turned up

Built as [`settings_migration`](../../src/logic/settings_migration.rs), 10
tests over three documents in `fixtures/settings/`.

**The fixtures are built, not collected.** Nobody's real settings file is in
this repository and none should be — one carries their song folders and their
stream password. Each fixture is instead the shape a version actually wrote,
with a configuration in it somebody would recognise. A real file is still
welcome: the invariant tests walk the directory rather than naming the files,
so dropping one in covers it.

**Writing them made a duplication visible.** The desktop loads settings from a
file and the web build from local storage, and both then did the same three
steps in a row — migrate the document, parse it, run every fixup — sharing
nothing but the habit. That is precisely the arrangement `bring_up_to_date`
was written to end, and it had only got half of it. The three steps are now
`Settings::from_stored`, which is what both call and what the tests exercise.
Testing the steps separately would have said nothing about the order they run
in, and the order is where a migration goes wrong.

The generic invariants are the ones worth naming, because they are what a
running Cantara leans on without ever checking: there is somewhere to project,
the reference view is one of the views, and every stored index names something
that still exists. An index past the end is a failure this program has had
before, and it surfaces during a service.

## Order

Ordered by cost of failure, not by ease.

1. ~~**Markup tests** for every slide type × design kind.~~ Done: 17 tests in
   [`slide_markup`](../../src/components/slide_markup.rs). Found one defect.
2. ~~**Migration fixtures.**~~ Done: 10 tests in
   [`settings_migration`](../../src/logic/settings_migration.rs).
3. ~~**Rust integration tests** for the network side.~~ Done: archive bounds
   in [`archive`](../../src/logic/archive.rs), damaged elements in
   [`presentation`](../../src/logic/presentation.rs), addresses in
   [`settings`](../../src/logic/settings.rs), load in
   [`stream::server`](../../src/logic/stream/server.rs).
4. ~~**Playwright for the stream viewer.**~~ Done: 16 tests in
   [`tests/browser`](../../tests/browser/), driven against
   [`harness`](../../src/logic/harness.rs).
5. ~~**A screenshot mode** for the desktop window.~~ Done as a *geometry* mode:
   [`measure`](../../src/logic/measure.rs). Playwright for the remaining web
   surfaces — the selection page, the settings, the design editor — is still
   open, and is the smallest of what is left.

All five are in. Tiers 1 and 2 were the ones 3.0 was waiting on; 3 to 5 were
meant to come after it, and arrived early because stage 3 turned up a remote
arbitrary file write and that changed the arithmetic.

## Decisions taken

* **Playwright runs on every push, on Ubuntu only.** Browsers are slow, but the
  suite is worth the wait, and one platform is enough: the surfaces Playwright
  reaches are served over HTTP and rendered by the browser it brings with it,
  so a second operating system would exercise the same code twice.
* **Fixture media is fetched, not committed** — *and in the end nothing had to
  be fetched at all.* The decision was made for the large public-domain files
  the original note asked for. What the tests actually needed turned out to be
  a one-page PDF and an eight-by-eight PNG of 74 bytes, both of which are in
  `testfiles/` and cost the repository nothing. The decision stands for the day
  something genuinely large is wanted; until then there is no network
  dependency in the suite, which is better than the arrangement it was
  authorising.
* **The screenshot mode asserts on measured geometry, not on images.** A font
  update must not turn the whole suite red, and geometry is what the scaling
  work in 0003 was verified by — correctly, as it turned out.

## What the review turned up

The pull request drew a machine review of thirteen comments. It was a good
review — most were correct, and three found defects worse than anything the
tests had. It is recorded here because the *pattern* is the interesting part.

### The pipeline failure

Windows only, and mine: a test compared a path against `"etc/passwd"` while
that platform spells it `etc\passwd`. **A path is not its spelling.** The
helper the tests use now hands back a `PathBuf` and the assertion is built from
components. Nothing about the extractor was wrong.

Worth noting where the review got this one: it flagged the same test for the
wrong reason — it read the crate's `enclosed_name` documentation ("can't be an
absolute path") as meaning an absolute entry is *rejected*, and recommended
asserting a refusal. It is not rejected; it is made **relative**, which is
equally safe. Acting on the suggestion would have inverted a correct test. My
own comment above that call had said the same misleading thing, which is
presumably where the reading came from; it now says which of the two happens.

### Three real defects, all the same shape

Each is a place where a comment claimed a guarantee the code did not give.

* **The budget was not hard.** `read_entries` handed the callback one byte more
  than the limit, so a file of exactly the remaining size could be told from
  one cut off at the ceiling. Neat, and wrong: the sentinel byte *reaches the
  callback*, which has already written it. On the desktop that is one byte past
  a gibibyte; in the browser the callback stores what it is given. The
  distinction is now made after the callback, by probing the entry. The test
  that should have caught it asserted `written <= 1025` — written around the
  implementation rather than around the promise.
* **The download was never bounded.** The archive limits bound what an archive
  *expands to*; the body was read into memory in one call before any of them
  ran. A server could exhaust the machine with the compressed response and meet
  no limit at all — while the comment beside it claimed this path "bounds how
  much of this machine an archive gets". The desktop download is now streamed
  to disk against a `download_bytes` budget.
* **Half an archive was still a repository.** `read_entries` stops at the first
  refusal on the stated principle that half an archive is no use. The web
  build's callback wrote each file into the global map as it arrived, so an
  archive whose tenth entry was hostile left nine files behind and the code
  read them back as a working repository. Entries are staged and published only
  on success.

### Three multi-view defects left over from 0003

Streaming bugs rather than testing ones, and all of the "a view silently does
not work" class this spec keeps meeting.

* **Media was collected for one view.** The picture handoff still asked the
  singular `stream_view()`, from before a service could be streamed at several
  addresses. A second network view with a division of its own has slides the
  first never shows, so its pictures were never sent: that address named
  `media/<id>` and the server had nothing under it.
* **A view added mid-service got no HTML.** `serve_views` updates the
  addresses; `publish` returns early when the presentation has not changed.
  Between them, a view added or re-designed during a service was an address
  with nothing behind it — or, worse, with the *old* design's markup, which
  looks like it is working.
* **Two views could claim one address.** The helper keeps one view per path, so
  the second vanished. `served_views` now reports and drops it, as it already
  did for a reserved path.

### Two things the review got wrong

Recorded because "the reviewer said so" is not a reason, and checking cost less
than arguing would have.

* **`GDK_BACKEND=x11` is not a regression.** Flagged as having been moved out
  of the Wayland/DRI condition. `git show 6a97744:src/main.rs` has it outside
  that condition too; the extraction changed nothing. Whether it *should* be
  conditional is a fair question and a separate one.
* **`browser.newPage()` does inherit `baseURL`.** Flagged as breaking a test
  that demonstrably passes. Playwright Test wraps `browser` so pages made
  through it carry the project's context options — checked with a throwaway
  spec rather than assumed.

### The macOS failure: a race that was always there

After Windows went green, macOS failed — and on a test nobody in this work had
touched, `search::tests::search_pdf_with_text`. It passed on Linux and on
Windows.

That shape is worth recognising on sight. **The code is the same on all three
platforms; only the interleaving differs.** A failure on one platform and not
the others, in a test with no platform-dependent code in it, is a scheduling
race until proved otherwise.

It was. The search index is a handful of global maps, and
`refresh_search_cache` **clears** them before filling them. Rust runs tests in
parallel. So `search_pdf_with_text` filled the index with a PDF's page text and
then read it back, while `refresh_cache_includes_markdown` — running beside it —
refreshed the same global index with a single markdown file and wiped the pages
out from under it. Nothing about PDFs, or searching, or macOS.

Diagnosed rather than guessed: reverting the fix and running the search tests
twenty times at eight threads reproduced it **on Linux, once in twenty**. With
the fix, nought in twenty, and nought in five full-suite runs at sixteen
threads. A fix for a race that has only been *reasoned* about is a fix nobody
can tell from a coincidence.

The remedy is the one this codebase already uses for the same problem:
`ONE_HELPER_AT_A_TIME` serialises the tests that share the network helper, and
`ONE_SEARCH_AT_A_TIME` now serialises the ten that share the search index.

The wider point belongs in this document rather than beside the fix. A suite of
785 tests will contain races, and a race that fires once in twenty runs is
worse than a test that fails every time: it teaches people to press the button
again. This one had been in the repository for some while, firing on whichever
platform happened to interleave badly, and it surfaced here only because this
work made CI run the suite where somebody was watching.

### The documentation warnings, which were one mistake and not thirty-eight

`cargo doc` reported thirty-eight unresolved links and one bare URL. Most of
them named items that plainly existed, in the very module whose documentation
was pointing at them, which is the shape of a systematic cause rather than
thirty-eight typos.

It was one: **a `mod` declaration carrying an outer `///` comment while the
module's own file carries inner `//!` documentation.** Rustdoc merges the two
and resolves the merged block's links at the *declaration* site — so
`stream_view.rs` saying "see [`map_slides`]" was resolved in `logic/mod.rs`,
where no such item is in scope. That is why the warnings had no file and line
against them, and why they looked so arbitrary.

Confirmed before acting: removing the one-line outer comment above
`pub mod stream_view;` made that module's warnings disappear.

Thirty-seven modules were in that state, and the outer comments were either a
restatement of the module's own first line or a note about why the module is
`#[cfg]`-gated. Both are worth keeping where a person reading `mod.rs` will see
them, and neither is worth having merged into the module's page. They are plain
`//` comments now: nothing is lost, the merge stops, and the links resolve.

That left ten genuine wrong targets — a renamed field, a renamed type, a method
that only exists under `cfg(test)`, a module that only exists under the
`test-harness` feature. Those are fixed one at a time, and the ones pointing at
conditionally-compiled items are *named* rather than linked, because there is
no page for a reader to follow.

Worth recording in this document rather than only in a commit message, because
it is the same lesson as the rest of it: thirty-eight symptoms, one cause, and
the way to tell was that the failures made no sense individually.

### The one that improved a test instead of the code

The load test asked for a range on a video id nothing was registered under, so
every request was answered 404 *at the lookup* and the range was never parsed.
It asserted only that a response came back — which it would have done on a
server that believed every range it was given. It now registers a real video
and asserts `416`.

That immediately found something, and it was the test's fault again: a
multi-range request is answered `206` with the first range, which
`parse_byte_range` does deliberately and the RFC permits. The assertion now
checks that `Content-Range` names the piece actually sent — the property that
matters, because a 206 whose header disagrees with its body has a player
assembling the file wrongly.

## The first of the web build's own pages: the search

[`search-keyboard.spec.js`](../../tests/browser/search-keyboard.spec.js), 14
tests over looking a song up and taking one with the keyboard. It closes the
first half of the "Playwright does not touch the web build's own pages" item
below, and three things came out of writing it.

**The library is compiled in, not seeded.** The stream tests put a service on
the stream through a control port; there is no equivalent here, and inventing
one would have meant a second harness. There was no need:
`CANTARA_BUNDLED_REPOS=local/testsongs` already makes `build.rs` embed
`bundled_repos/local/testsongs` into the WebAssembly, and
`Settings::ensure_bundled_repos` already loads it and skips the wizard. So the
page opens with a library and nothing to set up. The cost is that
`playwright.config.js` now starts two servers for every run.

**It went green locally and red in CI, twice, for two reasons worth keeping.**

*The library was not in the repository.* `bundled_repos/` is `.gitignore`d —
in a release build CI clones real song repositories into it — so on a fresh
checkout it does not exist. The way that failed is the part worth recording:
`build.rs` takes the *list* of repositories from the environment variable and
their *files* from the directory, and does not mind when the directory is
missing. It emitted a repository with no files behind it,
`ensure_bundled_repos` saw a bundled repository and skipped the welcome wizard,
and the tests were handed an application that looked entirely healthy — right
route, search field present — with an empty library and nothing to find. The
library is now copied from `testfiles/`, which is in the repository, by
[`bundle-library.mjs`](../../tests/browser/bundle-library.mjs).

*A development server says it is ready before it is.* `dx serve` opens its port
within two seconds and answers **200** while it builds, with a shell that has
no WebAssembly behind it. Playwright decides a server is ready by reading a
status code, so it cannot tell that apart from the finished application, and no
choice of URL helps — the dev server answers 200 for missing assets too. CI
declared it ready at two seconds and ran the whole suite against a page that
was five minutes from existing. Locally the retries had covered the gap, which
is worse than failing: the suite was green until the build was slow. The tests
are now given [`serve-web.mjs`](../../tests/browser/serve-web.mjs), forty lines
of static file server that opens its port after `dx build` has finished, so
that "the port is open" means "the application is there". Nothing in these
tests needs hot reloading; every one of them navigates.

**A test asserted the old behaviour and was right to fail.** Escape used to put
the result list away and leave the query standing, to be corrected rather than
retyped. That is not a behaviour this field can have any more: a browser empties
an `input type="search"` on Escape *by itself, without firing an event*, and the
field now keeps its own text instead of being redrawn from the query — so what
the user saw was an empty field over a query Cantara still believed in. Escape
now says so outright and clears both. The divergence had been shipped and
nobody had noticed; the test found it on its first run.

**Two regression tests were written, then deleted.** They were for the
swallowed letters — the defect that started all of this — and the honest way to
check a regression test is to put the defect back. So the old `value:` binding
went back in and the tests were run against it. **They passed.** They had to:
the old binding loses a letter only if a redraw can land between a keystroke
and the event reaching Rust, which in the desktop's `wry` window is an IPC round
trip across a process boundary and in the web build is not a gap at all — the
event and its redraw are one turn of one event loop.

So the defect is a desktop defect, and "What can actually be reached" at the top
of this document already said Playwright cannot go there. What is left is one
test of the surface as a browser behaves, labelled with exactly what it does
not cover. A test that cannot fail on the defect it is named after is worse
than no test: it is a claim of coverage that is not there, and it would have
been kept without that fifteen-minute check.

## Still open

* **A damaged element says nothing to the person building the order.** It is
  skipped and logged, and that is all. The right behaviour is to say so where
  the running order is built, while there is still time to act. See
  `build_presentation`.
* **The web build's download is bounded only by the archive limits.** The
  desktop streams its download against a budget; the browser still reads the
  body in one call, because `bytes_stream` needs a reqwest feature this project
  does not enable. The exposure is a tab's memory rather than a machine's, and
  the browser imposes limits of its own — but it is not the same guarantee, and
  it should not be described as if it were.
* **The window check has not been watched go green on a runner.** It is in CI
  under `continue-on-error`; that should come off once it has.
* **Playwright does not touch most of the web build's own pages** — settings
  and the design editor. The search over the library is now covered; see
  below.
* **Nothing systematically looks for the remaining races.** One was found by
  CI failing on macOS; there is no reason to think it was the only one. Running
  the suite repeatedly at a high thread count is cheap and is not done.
* **Nothing measures colour.** Geometry cannot see whether anything was
  painted: an element of the right size in black on black measures perfectly.
  A person still has to look at the screen before a release, and the checklist
  should say so rather than implying the suite covers it.
