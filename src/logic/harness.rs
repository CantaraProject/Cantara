//! Starting Cantara in a shape a browser test can drive.
//!
//! Stage 4 of `docs/specs/0004-testing-playwright.md`.
//!
//! # The problem this solves
//!
//! Playwright drives browsers. The surface worth driving here is the **stream
//! viewer** — the page a phone in the congregation opens — and that page is
//! served by a helper process that Cantara starts and feeds. To get one in
//! front of Playwright, something has to be running a service.
//!
//! The tempting shortcut is to serve the viewer page from a little test server
//! with a canned state behind it. That would be quick and it would be worth
//! very little: the defects this is meant to catch — the wrong view at an
//! address, a design that arrives as a wall of text, a video that never
//! starts — all live in the path *between* Cantara and the page, and a canned
//! state is exactly that path removed.
//!
//! So this does not shortcut it. It calls
//! [`enable_viewer`](crate::logic::network_host::enable_viewer) and
//! [`publish`](crate::logic::network_host::publish) — the same two functions
//! the stream switch and the presentation loop call — and everything after
//! that is the real thing: the real helper process, the real rendering through
//! `stream_render`, the real socket, the real page. What is replaced is only
//! the window: instead of an operator choosing songs and pressing "next", a
//! test asks over HTTP.
//!
//! # Why behind a feature and not behind a flag
//!
//! This opens a port and takes instructions on it. Cantara is careful not to
//! do that — the whole reason the network side is a separate process that
//! knows nothing is so that a service cannot be changed by whatever reaches
//! the socket. A hidden command-line flag in a released binary would undo
//! that, and "it is only reachable if you pass `--harness`" is the kind of
//! reasoning that holds until somebody finds a way to pass it.
//!
//! So `test-harness` is a cargo feature that `default` does not include. A
//! released Cantara does not have this code in it at all.
//!
//! # What it does not cover
//!
//! It drives one Cantara, on one machine, against a browser Playwright
//! launched. It says nothing about a phone on a hall's wi-fi, about a hundred
//! of them, or about the projection window — that last one is stage 5.
//!
//! And one thing it genuinely cannot do: **it has no web view, so it cannot
//! rasterise a PDF page.** That rendering is pdf.js, running inside the
//! window. Pages are stood in for by a known picture instead — see
//! [`hand_over_media`], which says what that does and does not license a test
//! to assert.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

use crate::logic::fixtures;
use crate::logic::network_server::ServedView;
use crate::logic::settings::{MonitorLayout, PresentationDesign, SpeakerNextPosition};
use crate::logic::states::RunningPresentation;

/// The argument that starts this instead of the program.
pub const FLAG: &str = "--test-harness";

/// Where the harness itself listens for instructions.
///
/// A different port from the stream's. They are two different things — one is
/// the service as the congregation sees it, the other is the test pulling the
/// strings — and running them on one socket would mean a test could reach the
/// controls by mistake and, worse, that a viewer could.
const CONTROL_PORT: u16 = 8431;

/// Where the stream is served, which is what Playwright opens.
const STREAM_PORT: u16 = 8430;

/// The addresses the harness serves, and what is at each.
///
/// Three, because the defect that prompted most of 0003 needed three to show
/// itself: the congregation's plain view, a stage monitor, and a second
/// monitor in a different layout. With one view, "every address shows the same
/// thing" and "every address shows the right thing" are the same observation.
fn views() -> Vec<ServedView> {
    vec![
        ServedView {
            path: "/".to_string(),
            id: fixtures::view_id(1),
        },
        ServedView {
            path: "/stage".to_string(),
            id: fixtures::view_id(2),
        },
        ServedView {
            path: "/speaker".to_string(),
            id: fixtures::view_id(3),
        },
    ]
}

/// The design each of those views is shown in.
fn design_for(view: uuid::Uuid) -> Option<PresentationDesign> {
    match view {
        id if id == fixtures::view_id(2) => Some(fixtures::slide_list_design()),
        id if id == fixtures::view_id(3) => Some(fixtures::speaker_design(SpeakerNextPosition::Right)),
        _ => None,
    }
}

/// A service by the name a test asks for it by.
///
/// The names are [`fixtures::every_slide_kind`]'s, so a Playwright test asking
/// for "video" and a Rust markup test asserting on a video slide are looking
/// at the same service. That is the point of the fixtures being shared: a
/// browser failure that cannot be lined up against a markup test tells you
/// only that *something* is wrong.
fn service_named(name: &str) -> Option<RunningPresentation> {
    if name == "words" {
        return Some(fixtures::song_service_on_the_words());
    }
    if name == "song" {
        return Some(fixtures::song_service());
    }
    fixtures::every_slide_kind()
        .into_iter()
        .find(|kind| kind.name == name)
        .map(|kind| kind.running)
}

/// The service the harness is currently showing, with each view's design on it.
fn shown(name: &str, widgets: bool) -> Option<RunningPresentation> {
    let mut running = service_named(name)?;

    for view in views() {
        let design = match (design_for(view.id), widgets) {
            // The widget cases go through the same builder as everything else
            // rather than being a fourth view: a clock on a stage monitor is a
            // property of the design, and a test about clocks should be
            // looking at the monitor it would really be on.
            (Some(_), true) if view.id == fixtures::view_id(2) => Some(fixtures::monitor_design(
                MonitorLayout::SlideList { context: None },
                fixtures::both_widgets(),
            )),
            (design, _) => design,
        };

        if let Some(design) = design {
            running = fixtures::shown_in(running, view.id, design);
        }
    }

    Some(running)
}

/// Runs the harness until it is told to stop, or the process is killed.
pub fn run() -> Result<(), String> {
    // The control port first, before anything is started.
    //
    // It is the cheapest thing that can fail and the likeliest — a harness
    // from a previous run that was killed with the tests still attached. Doing
    // it first means the failure is immediate and *nothing has been left
    // behind*: taking it last, as this did, started a helper process which
    // then outlived the harness that gave up, held the stream port, and made
    // the next run fail differently.
    let listener = TcpListener::bind(("127.0.0.1", CONTROL_PORT)).map_err(|error| {
        format!(
            "the control port {CONTROL_PORT} could not be opened: {error}. \
             A harness from an earlier run is probably still going."
        )
    })?;

    let address = crate::logic::network_host::enable_viewer(
        STREAM_PORT,
        // No password. What is being tested is the page, and a login in front
        // of every test would be a login in front of every test.
        String::new(),
        views(),
    )?;

    // The port that was *asked for*, or nothing.
    //
    // Cantara falls back to another port when the chosen one is taken, which
    // is right for a person — a service should not fail to stream because
    // something else on the machine has 8420. It is wrong here: the tests open
    // a fixed address, so a harness that quietly moved would leave Playwright
    // waiting on a port nothing answers, and the message it eventually gives
    // is that the server never started.
    //
    // That is exactly the shape of failure this whole suite exists to find, so
    // it should not be in the suite's own scaffolding.
    if !address.ends_with(&format!(":{STREAM_PORT}")) {
        crate::logic::network_host::disable_viewer();
        return Err(format!(
            "the stream was asked for port {STREAM_PORT} and came up at {address} \
             instead, which is not where the tests will look. Something else on \
             this machine has that port."
        ));
    }

    // Read by whoever is running this by hand. The Playwright configuration
    // waits for the stream itself to answer rather than for a line here.
    println!("stream at {address}");
    println!("control on http://127.0.0.1:{CONTROL_PORT}");

    // The clock, brought round once a second.
    //
    // Not decoration: Cantara's own loop does exactly this, and without it a
    // monitor view served over the network shows the time the slide came up
    // and holds it there — which was a reported defect, fixed in 0003. A
    // harness that left this out would let that defect come back with the
    // browser test still green, which is worse than not having the test.
    //
    // This is also the answer to a thing the browser tests found about *this
    // module*: it claimed to run "the same two functions the presentation loop
    // calls", and the presentation loop calls more than two.
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            crate::logic::network_host::refresh_time_widgets();
        }
    });

    // Something on screen from the start. A test that had to ask before it
    // could see anything would make every test's first step the same step, and
    // "the address is open before the service begins" is itself worth being
    // able to look at.
    show("words", false)?;

    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                if handle(stream) == Instruction::Stop {
                    break;
                }
            }
            Err(error) => eprintln!("a control connection failed: {error}"),
        }
    }

    crate::logic::network_host::disable_viewer();
    Ok(())
}

/// What a control request asked for.
#[derive(PartialEq, Eq, Debug)]
enum Instruction {
    Carry,
    Stop,
}

/// Answers one control request.
///
/// Deliberately the smallest HTTP server that can be written rather than
/// another `axum` app. It answers three paths for one client on one machine;
/// a framework here would be more code to read and no more correct.
fn handle(mut stream: TcpStream) -> Instruction {
    let Some(request) = first_line(&stream) else {
        return Instruction::Carry;
    };

    // "GET /show?service=video HTTP/1.1"
    let target = request.split_whitespace().nth(1).unwrap_or("/");
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };
    let parameters = parameters(query);

    let outcome = match path {
        "/show" => {
            let service = parameters
                .get("service")
                .cloned()
                .unwrap_or_else(|| "words".to_string());
            let widgets = parameters.get("widgets").map(String::as_str) == Some("yes");
            show(&service, widgets).map(|()| format!("showing {service}"))
        }
        "/next" => advance(1).map(|position| format!("at slide {position}")),
        "/previous" => advance(-1).map(|position| format!("at slide {position}")),
        "/stop" => {
            answer(&mut stream, 200, "stopping");
            return Instruction::Stop;
        }
        other => Err(format!("no such control: {other}")),
    };

    match outcome {
        Ok(said) => answer(&mut stream, 200, &said),
        Err(why) => answer(&mut stream, 400, &why),
    }

    Instruction::Carry
}

/// The request line, or nothing if the client said nothing usable.
fn first_line(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    Some(line)
}

/// `a=1&b=2` as a map, with the percent-encoding undone.
fn parameters(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_string(), unescape(value)))
        .collect()
}

/// Percent-decoding, and `+` for a space.
///
/// Enough for the names this takes — "slide list", "pdf page" — which is all
/// that reaches it.
fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                match u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    Ok(byte) => {
                        out.push(byte as char);
                        index += 3;
                    }
                    Err(_) => {
                        out.push('%');
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte as char);
                index += 1;
            }
        }
    }

    out
}

/// The service being shown, kept so that "next" has something to advance.
static SHOWING: std::sync::Mutex<Option<RunningPresentation>> = std::sync::Mutex::new(None);

/// Puts a named service on the stream.
fn show(name: &str, widgets: bool) -> Result<(), String> {
    let running = shown(name, widgets).ok_or_else(|| format!("no such service: {name}"))?;

    let mut held = SHOWING.lock().map_err(|_| "the harness is confused")?;
    *held = Some(running.clone());
    // The pictures first, then the slide that refers to them.
    //
    // The other order raced: the helper can broadcast HTML naming
    // `media/<id>` and answer a viewer's request for it before the picture has
    // arrived. A 404 to an `<img>` is final — the browser does not try again —
    // so a viewer who happened to connect in that window saw an empty box for
    // as long as the slide was up.
    //
    // Registering first cannot race the other way: a picture nothing refers to
    // is simply unused.
    hand_over_media(&running);
    crate::logic::network_host::publish(Some(running));
    Ok(())
}

/// Files a picture under every name the current service will ask for.
///
/// # What this stands in for, and what it does not
///
/// A picture a viewer asks for is one Cantara *rendered* — and for a PDF page
/// that rendering is done by pdf.js, inside the window's web view, through
/// [`crate::logic::pdf::page_image`]. **The harness has no window**, so it
/// cannot produce the real page, and nothing here pretends it can.
///
/// What it can do is the half that broke. In 0003 a PDF page reached a viewer
/// as an empty box because the markup asked for `media/<id>` and the server
/// held the page under a *different* id — the address and the bytes disagreed,
/// and the picture itself was never the problem. That is exactly what filing a
/// known stand-in under the name the state gives it tests: the browser asks,
/// and something answers.
///
/// So a browser test built on this may assert that a page slide **resolves to
/// a picture the viewer can load**. It may not assert that the picture is the
/// right page. That belongs to stage 5, which has a window.
fn hand_over_media(running: &RunningPresentation) {
    let divisions: Vec<crate::logic::states::Division> = views()
        .iter()
        .map(|view| crate::logic::states::Division::View(view.id))
        .collect();

    for division in divisions {
        let state = crate::logic::stream::protocol::StreamState::of(running, 0, division);
        for id in crate::logic::network_host::media_wanted(state.media()) {
            crate::logic::network_host::publish_media(id, stand_in_picture(), "image/png");
        }
    }
}

/// The picture every rendered page and thumbnail is stood in for by.
///
/// A real file rather than a handful of invented bytes, so that a browser
/// actually decodes it — an `<img>` pointing at something that is not an image
/// is indistinguishable, from a test's point of view, from an `<img>` pointing
/// at nothing.
fn stand_in_picture() -> Vec<u8> {
    std::fs::read(fixtures::picture_path()).unwrap_or_default()
}

/// Moves the service on, and says where it ended up.
fn advance(by: i32) -> Result<usize, String> {
    let mut held = SHOWING.lock().map_err(|_| "the harness is confused")?;
    let running = held.as_mut().ok_or("nothing is being shown")?;

    if by > 0 {
        running.next_slide();
    } else {
        running.previous_slide();
    }

    let position = running.position.as_ref().map(|at| at.slide_total()).unwrap_or(0);
    let moved = running.clone();
    // Media first, then the slide — the same ordering `show` explains. Moving
    // to another slide is moving to other pictures.
    hand_over_media(&moved);
    crate::logic::network_host::publish(Some(moved));
    Ok(position)
}

/// Writes an answer, and does not mind if the client has already gone.
fn answer(stream: &mut TcpStream, status: u16, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} OK\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every service the harness offers can actually be built.
    ///
    /// A Playwright test that asks for a name nothing answers to fails in the
    /// browser, minutes later, with a timeout — and a timeout says "the page
    /// did not do what you expected", which is the wrong thing to go looking
    /// for. This fails here instead, in a second, saying which name.
    #[test]
    fn every_service_the_harness_offers_can_be_built() {
        let mut names: Vec<String> = fixtures::every_slide_kind()
            .into_iter()
            .map(|kind| kind.name.to_string())
            .collect();
        names.push("words".to_string());
        names.push("song".to_string());

        for name in names {
            assert!(
                shown(&name, false).is_some(),
                "the harness offers {name} and cannot build it"
            );
            assert!(
                shown(&name, true).is_some(),
                "the harness offers {name} with widgets and cannot build it"
            );
        }
    }

    /// The three views are shown in three different designs.
    ///
    /// What the browser tests are for. If two of the harness's addresses came
    /// out identical, the test that says "each address shows its own view"
    /// would pass on a Cantara that had lost the distinction entirely.
    #[test]
    fn the_harnesss_three_views_are_genuinely_three() {
        let running = shown("words", false).expect("the default service builds");

        let rendered: Vec<String> = views()
            .iter()
            .map(|view| {
                crate::components::stream_render::render_presentation(
                    &running,
                    Some(running.current_design_in(crate::logic::states::Division::View(view.id))),
                )
            })
            .collect();

        assert_ne!(rendered[0], rendered[1], "/ and /stage render alike");
        assert_ne!(rendered[1], rendered[2], "/stage and /speaker render alike");
        assert_ne!(rendered[0], rendered[2], "/ and /speaker render alike");
    }

    /// The control parameters are read the way a browser writes them.
    #[test]
    fn a_control_request_is_read_as_it_is_sent() {
        let read = parameters("service=pdf+page&widgets=yes");

        assert_eq!(read.get("service").map(String::as_str), Some("pdf page"));
        assert_eq!(read.get("widgets").map(String::as_str), Some("yes"));

        assert_eq!(unescape("slide%20list"), "slide list");
        assert_eq!(unescape("two+languages"), "two languages");
        // A stray `%` is a stray `%`, not a panic. Nothing legitimate sends
        // one, which is exactly why it should be the thing that arrives.
        assert_eq!(unescape("100%"), "100%");
        assert_eq!(unescape("%zz"), "%zz");
    }
}
