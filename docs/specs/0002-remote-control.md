# 0002 — Remote control

Status: **implemented**. The open decisions were answered — see the last
section, which also records where the implementation departed from what is
written below, and what has and has not been verified.

## What is being asked for

A second switch beside the streaming switch. With it on, someone on the local
network opens an address in a browser and gets **the presenter console** — not
a view of it, the console itself: the same next and previous, the same jump
sidebar, the same grid, the same video controls, the same everything. Behind a
password, as the stream is.

The operator is then not tied to the machine. A musician at the piano advances
the slides from a tablet; a second person at the back takes over when the one
at the front has to sing.

## The constraint that decides the design

> Exactly the same presenter console, not a second one that looks like it.

This is the whole difficulty, and it is worth being precise about why.

The existing stream *viewer* took the other road: `assets/stream_viewer.html`
is 31 KB of hand-written HTML and JavaScript that renders slides a second time,
in a second language, from a data structure (`logic/stream/protocol.rs`) that
exists to feed it. That is defensible for what it does — a viewer shows one
slide and has no controls, and a phone in a pew should load one small file.

The console is not that. It has a text view and a grid view, a jump sidebar, a
live slide preview that runs the full renderer, video transport controls, a
size slider, keyboard handling, and a black-screen toggle. Writing it a second
time in JavaScript would be several thousand lines duplicating
`presenter_console_components.rs`, `presentation_components.rs` and
`jump_sidebar.rs` — and every future change to any of them would have to be
made twice, in two languages, by whoever remembered. Within a month the remote
console would be subtly the old one.

So: **the browser must run the console component that is already in the
program.** Everything below follows from that.

---

## Approach

### Recommended: LiveView

`dioxus-liveview` 0.7.10 exists and matches the dioxus version in `Cargo.toml`
(0.7.9, 0.7.10 in the lock file). It runs the component on the presenting
machine and sends the browser DOM patches over a WebSocket; events come back
the same way. The browser holds no application logic at all.

**Not in the Cantara process, though — in a helper process.** That is not a
preference; `dioxus_html` allows one event converter per *process* and the
desktop's and LiveView's are incompatible. See "The crash, and what it cost"
below, which is where this was learnt.

What that buys, against the constraint above:

- The remote console is `PresenterConsolePage` — the function, not a copy of
  it. A change to the console reaches the browser because it is the same code
  path, not because someone remembered.
- The stylesheet is `presenter_console.css`, the same asset.
- No second protocol for what the console shows. `logic/stream/protocol.rs`
  stays what it is: the *viewer's* data, for phones in pews.

What it costs, honestly:

- A dependency, and one that is less travelled than the rest of dioxus.
- One `VirtualDom` per connected browser, running on the machine that is also
  playing the service video. For the one or two consoles this feature is for,
  that is nothing; it would not scale to a congregation, and it does not have
  to.
- A dropped connection drops that VirtualDom. The presentation is untouched —
  it lives in the main window — but the remote page comes back as a fresh
  console rather than resuming mid-gesture.

### The named fallback: ship the web build

Cantara already builds for the web (`dx build --platform web`, in
`.github/workflows/dioxus.yml`), and that build already contains a working
presenter console — the one the web app runs, synchronised through
`localStorage` between two tabs (`logic/sync.rs`). The desktop could embed that
bundle and serve it, and the browser would run the console as wasm, talking to
the desktop over HTTP instead of `localStorage`.

Same component, also no duplication. But it needs a real protocol where
LiveView needs none, and it makes the desktop release depend on a web build —
two `dx` invocations, an artifact passed between them, and several megabytes of
wasm inside the desktop binary. More robust over a flaky network; considerably
more build engineering.

**Recommendation: LiveView, with a spike first** (below). If the spike fails,
this is the fallback, and the work in stages 1–3 is not wasted — the host
adapter and the bridge are needed either way.

### Rejected: a hand-written console page

For the reasons in "The constraint that decides the design". Not a candidate.

---

## The spike, before anything else

Half a day, throwaway branch. A `LiveViewPool` on the existing axum server, one
route, rendering `PresenterConsolePage` with a stub presentation. It answers
four questions, all of which have to be yes:

1. **Assets.** Does `asset!()` — `MAIN_CSS`, `PRESENTER_CONSOLE_CSS`, the
   bundled fonts — resolve to a URL the axum server can answer?
   `dioxus-asset-resolver` is already in the tree for the desktop build; the
   question is whether the same resolution works when the page is served over
   HTTP rather than the `dioxus://` scheme.
2. **`document::eval`.** The console uses it (`presenter_console_components.rs`
   lines 112, 230, 505, 613). LiveView supports eval over the socket, but the
   50 ms polling loop at line 112 would be twenty round trips a second per
   client. It has to be replaced anyway — see stage 1 — but the spike must show
   that the eval that *stays* works.
3. **Thread affinity.** LiveView builds each VirtualDom on a pool thread. A
   `Signal` is `UnsyncStorage` and belongs to the runtime that made it, so the
   main window's `Signal<Vec<RunningPresentation>>` almost certainly cannot be
   handed to it the way the second desktop window gets it
   (`selection_components.rs:624`, `with_root_context`). Confirm that, because
   the whole bridge in stage 2 exists to work around it. If it turns out a
   `SyncSignal` can cross, stage 2 gets much smaller.
4. **The desktop paths.** With `--features desktop` the console's
   `#[cfg(feature = "desktop")]` blocks compile *into the remote VirtualDom
   too*, and `dioxus::desktop::window().close()` (lines 64, 121, 270) has no
   window to close there. Confirm what it does — panic, or quietly nothing.

---

## Architecture

```
   ┌─ Cantara (desktop) ────────────────────────────────┐
   │                                                    │
   │  main window          Signal<Vec<RunningPresentation>>
   │       │                        ▲   │               │
   │       │        ConsoleCommand  │   │ watch<RunningPresentation>
   │       │                        │   ▼               │
   │  ┌────┴─────────── console bridge ──────────────┐   │
   │  │                                              │   │
   │  │   remote VirtualDom  =  PresenterConsolePage │   │
   │  └──────────────────┬───────────────────────────┘   │
   │                     │ DOM patches / events (WS)     │
   │   axum server ──────┴────────────────────────────── │
   └─────────────────────┬──────────────────────────────┘
                         │  http://<machine>:<port>/console
                   a browser on the network
```

### Two servers, because they cannot be one

The design here was one server with two switches: same port, same address,
different path. It could not be built — not for want of trying, but because
the console cannot run in this process at all, and a process cannot share
another's listening socket.

So `logic/stream` is untouched, still doing exactly what it did for viewers,
and the console is served by the helper process on **its own port**: the
stream's port plus one, or any free port if that is taken. Two addresses to
read off the panel instead of one. That is the price of the crash below, and
it is worth being explicit that it is a price rather than a design.

### Routes (all served by the helper)

| Route | What it is |
|---|---|
| `GET /console` | The login form, or the LiveView glue page once authenticated |
| `POST /console/login` | Takes the password, sets the console cookie |
| `WS /console/ws` | The LiveView socket. Refused without the cookie |
| `GET /assets/*` | The program's own stylesheets, fonts and scripts |
| `GET /cantara-video/*` | The videos of the running service |
| `GET /` | Redirects to `/console`, for anyone who typed the bare address |

The viewer's `/`, `/state`, `/events`, `/media`, `/video` and `/login` are on
the other port and do not change.

### The host adapter (stage 1)

The console currently decides *how it is hosted* at compile time: `#[cfg(feature
= "desktop")]` for the polling sync and the window close, `#[cfg(target_arch =
"wasm32")]` for the `localStorage` sync. A remote console is a fourth host in
the same binary as the desktop one, so a `cfg` cannot express it.

Replace those branches with a context provided when the VirtualDom is built:

```rust
enum ConsoleHost {
    /// A route in the main window. Leaving means navigating back.
    MainWindow,
    /// Its own desktop window. Leaving means closing it.
    SeparateWindow,
    /// A browser tab across the network. Leaving means saying goodbye.
    Remote,
}
```

Three behaviours hang off it, and each is a bug if it is got wrong:

- **Going away.** Navigate, close the window, or render a "the presentation has
  ended" page. `dioxus::desktop::window().close()` must not run for a remote
  client.
- **Staying in step.** The desktop polling loop, the `localStorage` loop, or
  the bridge below. This also gets rid of the 50 ms `document::eval` loop for
  remote clients, which LiveView would turn into network traffic.
- **The audio.** `PresenterConsolePage` claims the machine's audio on mount
  (line 50) — while a console is open the projection mutes, because the console
  is where the operator is. **A remote console must not claim it.** Someone
  opening the remote console from the back of the hall would otherwise silence
  the room's video. This is the single most likely thing to be missed, and it
  is a live bug the moment the feature ships without it.

This stage is worth doing on its own merits — it is the same kind of cleanup as
[0001](0001-duplicated-code.md), replacing compile-time branching with a named
runtime distinction.

### The bridge (stage 2)

The remote VirtualDom needs a `Signal<Vec<RunningPresentation>>` in its context,
because that is what the console reads and writes. It cannot be *the* signal
(see spike question 3), so it is a local one kept in step over channels:

- **Out:** a `tokio::sync::watch<RunningPresentation>` the main window publishes
  to whenever the presentation changes — the same moment it already calls
  `stream::publish`. The remote adapter waits on it and writes the local signal.
- **In:** an `mpsc<ConsoleCommand>` the remote adapter sends on, drained by the
  main window and applied to the real signal.

`ConsoleCommand` should be the operator's *intent*, not a serialized
presentation: `NextSlide`, `PreviousSlide`, `JumpTo { chapter, slide }`,
`ToggleBlackScreen`, `Video(VideoCommand)`, `Quit`. Two reasons. A whole
`RunningPresentation` travelling in both directions is what makes the existing
desktop sync need `eq_ignoring_scroll` and a comment about race conditions; and
an intent can be checked, refused and logged, where a state cannot.

Echo suppression matters: a command applied on the main window comes back
through the watch channel and must not be re-applied. The existing code solves
the same problem with `last_seen_shared` / `last_seen_local`
(`presenter_console_components.rs:107`); the bridge should not invent a second
answer.

**Open:** two remote consoles at once. The bridge as described allows it, and
they would both work — the last command wins, which is also what happens with a
desktop console and a remote one. Alternatively the second connection is
refused with "a remote console is already open". Refusing is simpler to reason
about; allowing is friendlier when someone's phone locked and they reopened the
page. Recommendation: allow, and show in the switch panel how many are
connected.

### Settings on the remote side

The console writes settings: the view mode (line 384), the grid size (line
575), each followed by `settings.read().save()`. A remote client doing that
would rewrite the operator's settings file from across the room.

The remote VirtualDom should get a `Settings` signal seeded from a snapshot at
connect, with saving disabled — the remote operator may switch to grid view and
size their thumbnails, and none of it touches the machine's settings or the
disk. This wants a small change at the two call sites (ask the host whether
settings are persistent) rather than a second console.

### Media

The console previews slides, and a slide can be a picture, a PDF page or a
video. On the desktop those reach the page as local file paths through the
asset handler in `components/video_host.rs`; a browser across the network
cannot read them.

`logic/video_server.rs` already solves exactly this problem for WebKitGTK, and
its comment is the design: *the same bytes, the same answer, a different
transport* — both paths call `answer_video_request`. The remote console should
do the same rather than growing a media pipeline of its own, and the stream
server's existing `/media/{id}` and `/video/{id}` handlers are most of it.

**The rule that must not be broken:** the remote routes serve only what the
running presentation refers to, from an allowlist built when the presentation
is built. A route that takes a path from the client and reads it is a remote
file-read hole in a program that is, by design, listening on every interface.
`video_server.rs` guards its loopback port with a token for a strictly smaller
version of this risk; this one is on the network.

---

## Security

The viewer's password protects *reading along*. This one protects *control of
the service*, which is a different prize: someone who reaches it can blank the
screen, skip songs or end the presentation in front of the congregation.

1. **A password of its own**, `settings.stream.remote_password`, separate from
   the viewer password. Recommended, not optional: the viewer password is meant
   to be given out — it is read from the front, put on a slide, printed in a
   sheet — and everyone who has it would otherwise be able to take over.
2. **Empty means open, and that is the operator's call.** An earlier draft of
   this document had an empty password *refuse* to switch remote control on.
   That was paternalism: a locked room on a network with nothing else on it is
   a real situation, and a program that insists on a password there is in the
   way rather than being careful. The person running the service knows which
   network they are on. What the program owes them is a plain sentence beside
   the switch saying what an empty password means — which is what
   `selection.remote_no_password` says — and not a decision taken on their
   behalf.
3. **A session of its own.** A second cookie name and a second token, so that a
   viewer session is never a console session. `session_cookie`,
   `constant_time_eq` and the `HttpOnly`/`SameSite` handling in
   `stream/server.rs` are reused as they stand.
4. **Slow the guessing down.** The viewer login has no rate limit, which is
   defensible for a read-only page on a LAN. Control deserves better: a short
   delay after a wrong password and a cap on attempts per connection. Cheap,
   and it keeps a four-character password from being walked through in seconds.
5. **The switch is not persisted.** Like streaming, and for the reason already
   written down in `StreamSwitch`: a program that was remote-controlled once
   must not quietly offer it again next Sunday. The *password* is a setting; the
   *switch* is a decision about this service.
6. **Still plain HTTP.** Everything `logic/stream/mod.rs` says about that
   applies here and should be repeated in the UI text: the password keeps the
   curious out, not someone who is actually trying. This is for a church LAN.

---

## The user interface

One switch, in the same panel as the streaming switch — `StreamSwitch` in
`components/selection_components/presentation_options.rs`, the general tab of
the presentation options — because it is the same kind of decision about the
same service, and because that panel already knows how to show an address and
copy it to the clipboard.

- Switch: *Diese Präsentation fernsteuern* / *Remote-control this presentation*.
- When on: the address, `http://<machine>:<port>/console`, in the same
  clickable `code.stream-address` the stream uses, and how many browsers are
  connected.
- When the remote password is empty: the switch works, with a line saying that
  anyone who reaches the address can drive the presentation and where to set a
  password if that is not wanted.
- The password field goes next to the existing one in `StreamSettingsSection`
  (`components/settings_components.rs:284`).

New locale keys in `locales/selection.yml` (switch, address hint, "no password
set", connected count) and `locales/settings.yml` (the password field and its
explanation), each with `en` and `de` — both files carry both languages
throughout and a missing `de` shows English to a German user.

---

## What happens when

| Situation | What should happen |
|---|---|
| Remote console opened, no presentation running | The page says so and waits, as the stream viewer's page does. It must not close or error. |
| Presentation ends while a remote console is open | The page says the presentation has ended. No `window().close()`, no navigation to a route that does not exist in a browser. |
| Remote clicks "quit presentation" | The presentation ends, on the machine, for everyone. **Open:** whether a remote client may do this at all. Recommendation: yes — it is the console, and half a console is a worse thing to explain than a confirm dialog. |
| Connection drops mid-service | The projection is untouched. The page reconnects to a fresh console at the current position. |
| Switch turned off while someone is connected | Sockets close, the page says the remote console was switched off. |
| Both a desktop console and a remote one | Both work, both see the same presentation. |

---

## Testing

The stream module's own arrangement is the model: `protocol.rs` is tested
without a socket in sight, and that is why it has tests at all.

- **The bridge, without a browser.** Commands in → the expected change to a
  `RunningPresentation`; a change out → the expected watch update; a command
  echoed back does not double-apply. Pure functions over
  `RunningPresentation`, in the style of `logic/stream/protocol.rs`'s tests.
- **Login and sessions.** The right password sets the console cookie; the wrong
  one does not; a viewer cookie does not open the socket; an empty remote
  password refuses to enable at all.
- **The media allowlist.** A path that is in the running presentation is
  served; one that is not is refused, including the obvious traversal attempts.
- **The host adapter.** That `ConsoleHost::Remote` neither claims audio nor
  saves settings. Both are one-line assertions and both are the bugs that would
  otherwise ship.
- **By hand**, on a real network: a phone and a laptop, a service with a song,
  a PDF and a video, the machine's wifi turned off and on again mid-song.

---

## Work plan

| Stage | What | Size |
|---|---|---|
| 0 | The spike above | half a day |
| 1 | `ConsoleHost` context; the three cfg-branches become runtime branches; audio and settings-saving hang off it | small–medium |
| 2 | The bridge: `ConsoleCommand`, the watch channel, the adapter that feeds a local signal. Tested without a socket | medium |
| 3 | Server: the second switch in `logic/stream`, the `/console` routes, login, session, rate limit | medium |
| 4 | LiveView wiring: pool, socket, glue page, assets | medium, mostly unknown until stage 0 |
| 5 | Media allowlist and the console's picture/video paths | small–medium |
| 6 | UI: switch, address, disabled state, locale keys, settings field | small |
| 7 | Tests and the manual matrix | small |

Stages 1 and 2 are worth having whether or not LiveView survives the spike, and
stage 1 is worth having even if this feature is never built.

## Open decisions

1. A separate remote password (recommended) or the viewer's one.
   → **separate**.
2. Whether enabling remote control implies enabling the viewer stream
   (recommended: no). → **no**: two independent switches over one server.
3. One remote console at a time, or several (recommended: several).
   → **several**.
4. Whether a remote client may end the presentation (recommended: yes).
   → **yes**.
5. LiveView or the wasm bundle, if the spike is ambiguous rather than clearly
   yes or no. → **LiveView**; the spike was clear.

---

## What was built

### The spike, answered

1. **Assets.** Yes. `dioxus::asset_resolver::native::serve_asset` — the
   resolver the desktop build already carries — answers a path and hands back
   an HTTP response, which is one axum route.
2. **`document::eval`.** Supported over the socket. The 50 ms polling loop is
   skipped for a remote console all the same, because it does not need it: see
   the sync note below.
3. **Thread affinity.** Confirmed, and it is not a maybe.
   `LiveViewPool::launch_virtualdom` calls `spawn_pinned` on its own
   `LocalPoolHandle`, so the `VirtualDom` is *built* on a pool thread. The main
   window's signal cannot go with it. The bridge is required.
4. **The desktop paths.** Two of them, not one. `window().close()` in the
   console was the expected one; `VideoAssetHost` registering a webview asset
   handler was not, and a remote `VirtualDom` has no webview to register with.
   Both now ask the host.

### The crash, and what it cost

The first implementation put the `LiveViewPool` in the Cantara process, as this
document describes. Starting a presentation with remote control enabled killed
the program:

```
panicked at dioxus-liveview-0.7.10\src\events.rs:91:54:
called `Option::unwrap()` on a `None` value
thread caused non-unwinding panic. aborting.
```

`dioxus_html` keeps **one event converter for the whole process** — a
`static EVENT_CONVERTER: RwLock<…>` in `events/mod.rs`. `dioxus_desktop`
installs its own at launch (`app.rs:88`), reading a mounted element as a
`DesktopElement` and a form event as a `DesktopFormData`.
`LiveViewPool::new()` overwrites it (`pool.rs:31`) with one that reads the same
events as `LiveviewElement` and `SerializedFormData`. Both `unwrap()` the
downcast; neither tolerates the other's events; and the panic happens across an
`extern "C"` boundary, where it cannot unwind and aborts instead.

There is no way to compose the two from outside: the types they downcast to
live in private modules of their crates, so a converter that tries both cannot
be written by a third party. Whichever renderer installs last works and the
other kills the program. The spike in this document asked four questions and
this was not among them — it can only be seen when a desktop webview *and* a
LiveView dom deliver events in one process, which no compiler check and no
socket test reaches.

**So the console moved into a helper process**: the same binary, started with
`--remote-console`, which never calls `dioxus::launch` and therefore never gets
a desktop converter. It is given the presentation over a loopback socket and
sends back what the operator does with it — the bridge in this document, with a
process boundary in the middle. `logic/remote_console_child.rs` serves the
console; `logic/remote_console_host.rs` starts, feeds and stops it.

What that costs: a second port, a helper process to supervise, and the
presentation crossing a socket as JSON. What it keeps: the console is still
`PresenterConsolePage` — the same function, the same stylesheet, the same
everything — which was the whole point.

**A helper must never outlive Cantara.** It serves the running service to the
network, so one left behind is a stale console on the church wifi with nobody
watching it. Two things make sure it does not: the parent kills it when the
switch goes off or its handle is dropped, and the helper exits by itself the
moment the socket to Cantara ends — which is what happens when Cantara is
closed, killed, or crashes. The second is the one that matters, because the
first does not run when the program dies suddenly.

### Where the implementation departs from the design above

**`ConsoleCommand` carries a whole `RunningPresentation`, not an intent.** The
argument above for intents was that "an intent can be checked, refused and
logged, where a state cannot" — which assumed the browser speaks this protocol.
With LiveView it does not: the browser sends DOM events, the console component
turns them into the same writes a click in the window makes, and the
presentation never leaves the process. So the door is no wider, and the cost of
intents would have been rewriting every control in the console for the remote
case — the opposite of running the same console. `ConsoleCommand::Quit` remains
a command, because ending a presentation is not a state of one.

**Stage 5 (media) was two routes, not a pipeline.** The renderer already avoids
filesystem paths, because a webview cannot fetch them either: pictures are
inlined as data URLs (`logic::images::image_data_url_str`) and PDF pages are
handed to `pdf.js` base64-encoded through `document::eval`. Both work over the
socket unchanged. Only videos needed serving, and they are answered by
`logic::video::answer_video_request` — the same function that answers the
window and the WebKitGTK loopback server.

**A switched-off console answers `404`, not `401`.** Uniformly, through
`console_refusal`: a console that is off is not there, and "unauthorised" would
advertise a door that does not exist. Once it is on, an unauthenticated request
gets `401` — except `/console` itself, which serves the login form, because
that is the address a person types.

**The socket's password check runs after axum's upgrade extractor.** A plain
`GET /console/ws` is therefore refused with `400` before the handler is
reached; a real handshake without the session gets `401`, and with it, `101`.
The test speaks the handshake over a `TcpStream` to pin exactly that.

### The shape of it

| Where | What |
|---|---|
| `logic/console_host.rs` | `ConsoleHost` — main window, its own window, remote. Audio, settings-saving and how to leave hang off it |
| `logic/remote_console.rs` | The bridge: the `watch` out, the `mpsc` back, `apply`, and the connection count. Used in *both* processes — the socket is what joins them |
| `logic/remote_console_host.rs` | Starts the helper, proves it over a token, feeds it the presentation, applies what comes back, and kills it |
| `logic/remote_console_child.rs` | The helper: the console's routes, its password, and the LiveView pool |
| `components/remote_console.rs` | `RemoteConsoleRoot` — the contexts the console expects, and the two loops that keep this connection's copy in step |
| `assets/console_login.html` | The login form, self-contained because everything else is behind it |
| `presenter_console_components.rs` | Host-aware: `leave`, `remember`, the audio claim, which sync runs |
| `components/video_host.rs` | Registers a webview asset handler only where there is a webview |
| `presentation_options.rs` | The switch, the address, the connected count |
| `settings_components.rs` | The console password field |
| `main.rs` | `--remote-console` before anything opens a window; the publisher and the command pump |

Nothing on the command line but a port and a token: a command line is readable
by every process on the machine, so the password reaches the helper over the
socket, after it has proved which process it is. `logic/video_server.rs` guards
its loopback port the same way, for a smaller version of the same risk.

The sync needed no new mechanism. A remote console's root gives it a
`Signal<Vec<RunningPresentation>>` of its own, and the console's existing
reactive shared↔local effects — the ones the web build uses — run against it,
because a remote console is one `VirtualDom` exactly as the web build is. What
the bridge keeps in step is that root signal.

### Five bugs the browser found

The compiler, the unit tests and the socket harness all passed before any of
these; every one of them needed the thing actually running.

1. **The console said nothing was running.** `PresenterConsolePage` asks
   `running_presentations.peek()` whether there is anything to show, and `peek`
   deliberately does not subscribe — right for the consoles that are told what
   happened by a sync loop, wrong for a remote one, which has only its own
   render to learn from. It said "no presentation is running" for the rest of
   the service, whatever started on the machine. It now reads that signal.
   (Confirmed by removing the fix again and watching the console fail to
   appear: the harness only ever received an `eval` frame.)

2. **A console switched on mid-service got nothing**, because switching a
   switch is not a change to the presentation and the publisher is woken by
   changes to the presentation. The streaming switch has the same problem and
   solves it with a counter; this one hands over what is running the moment the
   helper is up. The presentation is also published from the streaming
   publisher's own effect and from a 200 ms watch, because the presentation is
   driven from windows whose writes are not a reliable wake-up elsewhere —
   `publish` compares before it writes, so saying it three times costs three
   comparisons.

3. **Every button on the console did nothing, five seconds in.** The thread
   that reads the helper's messages read through a *clone* of the socket, made
   while the handshake had a five-second read timeout on it. A clone carries
   the timeout it was made with and keeps its own from then on, so clearing it
   on the original — which is what the code did — changed nothing. Five seconds
   of quiet then looked exactly like the helper having gone, the thread
   returned, and everything the operator pressed after that reached nothing
   while the console went on looking as though it had worked. That is the
   symptom that was reported: the console on slide 4, the projection on slide
   1, and no complaint from either.

   `a_cloned_socket_keeps_its_own_read_timeout` pins the platform behaviour
   that caused it.

4. **Every button did nothing, again — for a second reason.** With the socket
   fixed the commands reached the program and stopped there. The main window
   *awaited* the command channel, and a message sent from the thread that reads
   the helper wakes a `Waker`; whether that reaches a `VirtualDom` driven by a
   window's event loop is the very question every other cross-window path here
   answers by polling. It is now drained by a fifty-millisecond loop —
   `remote_console::drain` — beside the publishing that goes the other way. The
   giveaway was that the *other* direction worked perfectly: driving from the
   native console updated the remote one, because that road was already a poll.

5. **PDF slides were black rectangles.** Not our plumbing at all:
   `Map.prototype.getOrInsertComputed` is new enough that the Chromium web view
   Cantara draws its own windows in has it and a current Firefox does not, and
   the bundled `pdf.js` uses it — `TypeError: this[#Ar].getOrInsertComputed is
   not a function`, in a browser console nobody was looking at. The page now
   carries a small polyfill for `Map` and `WeakMap`, written to the
   specification's behaviour, ahead of everything else and only where the real
   thing is missing.

   Worth remembering for the rest of this feature: the web view Cantara draws
   its own windows in is *newer* than the browsers the console is aimed at.

And one thing that was not a bug but looked like one: **the console arrived
unstyled**, because `main.css` holds Cantara's own rules on top of PicoCSS,
which `App` registers separately and the helper never runs. All four
stylesheets — Pico, `main.css`, `presentation.css`, `presenter_console.css` —
are now compiled into the page it serves.

### Verified

- `cargo test`: 594 pass, including six for the bridge (echo suppression, a
  late command, scroll-only updates, quitting) and six for the console's
  password and sessions.
- `cargo clippy --all-targets`: clean.
- **The helper, end to end, against the built binary.** A stand-in parent
  speaks the IPC protocol to a real `--remote-console` process and then talks
  HTTP and WebSocket to it: the helper proves itself with its token, takes a
  port, serves the login form, refuses a wrong password, hands out
  `cantara_console` for the right one, serves the console page, upgrades the
  socket to `101` — and then **renders**. It sends 206 bytes for "nothing is
  running", and when a presentation is pushed down the IPC socket it redraws
  with 2321 bytes and reports one connected console. That is the real
  `PresenterConsolePage`, in a `VirtualDom`, over LiveView.
- **A burst of commands is drained into one change** — the program's half of
  the control direction, which cannot be seen from the helper's side of the
  socket.
- **A click in the console reaches Cantara.** A harness opens the console over
  a real WebSocket, sends genuine LiveView click events, and watches the IPC
  socket: a click on the console's own controls comes back as an `Update`. This
  is the direction the compiler cannot check at all.
- **The helper dies with Cantara.** A second harness starts one, checks the
  console is up, closes the socket the way a closing or killed Cantara would,
  and watches: it exits on its own within 0.2 s and gives the port back. This
  found a real bug — the command pump was a `spawn_blocking` task waiting on a
  channel that never closes, and dropping a runtime *waits* for blocking tasks,
  so the helper stayed up serving the presentation after Cantara had gone. One
  was found still running. Everything inside the helper's runtime is now an
  ordinary async task, which a runtime drop cancels.
- Nothing outside the tests `unwrap`s or `expect`s; `main.rs` now carries
  `#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]` to
  keep it that way. The lint was proved to fire by putting an `unwrap` back in
  and watching it complain — a zero from a lint nobody has seen work is not
  evidence.

### Not verified

A browser has now opened the page, on the hall's own network, and driven a
service with it: the console renders, follows the projection, and drives it.
The bugs listed above are what that produced, and all of them are fixed.

What is still unproven:

- **A packaged build.** Everything so far is `dx serve` and `cargo build`.
  `serve_asset` does resolve from the helper process — `pdf.js` and the PDF
  viewer are fetched through `/assets/…` and run — but whether it resolves from
  a *bundled* application on each platform has not been tried.
- **Video over the socket.** The route is there and answers with ranges; no
  service video has been played through a remote console yet.
- **The audio rule.** `ConsoleHost::Remote` does not claim the machine's audio,
  which is right, but it means opening a remote console does not mute the
  projection either. If an operator uses the remote console *as* their console,
  the room keeps making the sound — which is what should happen, and is worth
  confirming with a video.

## One port

Asked once the console was running: can the console and the viewer stream not
share a port? They cannot share a *process* — that is the crash above — and two
processes cannot share a listening socket. What is possible:

- **A. Move the viewer stream into the helper.** One process, one port, one
  server, and the two axum servers that exist today become one. The blocker is
  that stream media for PDF slides is rendered through `document::eval` in the
  web view (`logic::pdf::page_image`), which the helper has not got: the parent
  would keep rendering those and ship the bytes over the IPC socket it already
  has. **Agreed as the next step**, after the bugs.
- **B. Proxy `/console/*`** from the parent to the helper, WebSocket upgrade
  included. Everything stays where it is; every DOM patch takes an extra hop.

Meanwhile the stream's own port answers `/console` with a redirect to wherever
the console is, so there is one address to read out even though there are two
ports.
