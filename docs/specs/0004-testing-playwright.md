# 0004 — Testing: what a service is allowed to do to us

Status: **draft**, nothing built yet.

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

## Order

Ordered by cost of failure, not by ease.

1. **Markup tests** for every slide type × design kind. Cheapest, and catches
   the class that produced five of nine defects.
2. **Migration fixtures.** Small, and it guards the only irreversible thing.
3. **Rust integration tests** for the network side: the error cases, the load
   property, the archive bounds.
4. **Playwright for the stream viewer.** The highest-value browser surface.
5. **A screenshot mode** for the desktop window, and Playwright for the
   remaining web surfaces.

Tiers 1 and 2 are worth having before a 3.0 release. Tiers 3 to 5 are worth
having before the release *after* it, and are the reason to keep the release
after it small.

## Decisions taken

* **Playwright runs on every push, on Ubuntu only.** Browsers are slow, but the
  suite is worth the wait, and one platform is enough: the surfaces Playwright
  reaches are served over HTTP and rendered by the browser it brings with it,
  so a second operating system would exercise the same code twice.
* **Fixture media is fetched, not committed.** The *addresses* are committed —
  a Wikimedia URL and a checksum — and the files are downloaded once and
  cached. The checksum is what makes this reproducible; without it a fetched
  fixture is whatever the far end serves today.
* **The screenshot mode asserts on measured geometry, not on images.** A font
  update must not turn the whole suite red, and geometry is what the scaling
  work in 0003 was verified by — correctly, as it turned out.
