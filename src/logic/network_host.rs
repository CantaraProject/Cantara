//! Starting, feeding and stopping Cantara's network side.
//!
//! The counterpart to [`crate::logic::network_server`], which explains why
//! that is a process of its own. This half is what the two switches in the
//! presentation options talk to: it starts the helper, tells it what to offer,
//! hands it the presentation and the pictures whenever they change, applies
//! what a remote operator does, and stops it again.
//!
//! Both services — the stream to the pews and the console — are one helper on
//! one port. Either switch starts it; the last one to go off stops it.
//!
//! Everything here is best-effort by design. A helper that will not start is
//! reported to the user and changes nothing else; a helper that dies mid
//! service takes the remote console with it and leaves the projection exactly
//! as it was. The presentation is the main window's, and nothing in this file
//! is allowed to be a reason for it to stop.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use super::network_server::{Configuration, Offer, ToChild, ToParent, FLAG};
use super::remote_console::{self, ConsoleCommand};
use super::states::RunningPresentation;

/// The running helper, if there is one.
fn helper() -> &'static Mutex<Option<Helper>> {
    static HELPER: OnceLock<Mutex<Option<Helper>>> = OnceLock::new();
    HELPER.get_or_init(|| Mutex::new(None))
}

/// How many browsers have the console open, as the helper last reported.
static CONNECTED: AtomicUsize = AtomicUsize::new(0);

/// A helper process and the socket to it. Dropping this stops it.
struct Helper {
    process: Child,
    /// The writing half. The reading half belongs to the thread that pumps it.
    to_child: TcpStream,
    /// Where it is being served: the address a browser is given, without a
    /// path.
    address: String,
    /// What the helper was last told, so that the same news is not sent twice.
    last_sent: Option<RunningPresentation>,
    /// The pictures it already has, so that none is rendered or sent twice.
    /// A picture is named after its content, so a name it has seen is a
    /// picture it has.
    sent_media: std::collections::HashSet<String>,
    /// What it is offering. Kept here so that throwing one switch does not
    /// disturb the other.
    offer: Offer,
}

impl Drop for Helper {
    fn drop(&mut self) {
        // Closing the socket is how the helper is asked to go; killing it is
        // how it is made to. Both, in that order, because a helper waiting on
        // a browser that will not close its tab would otherwise keep the port.
        let _ = self.to_child.shutdown(std::net::Shutdown::Both);
        let _ = self.process.kill();
        let _ = self.process.wait();
        CONNECTED.store(0, Ordering::Relaxed);
    }
}

/// Starts offering the stream to viewers, and says where to find it.
pub fn enable_viewer(
    port: u16,
    password: String,
    views: Vec<super::network_server::ServedView>,
) -> Result<String, String> {
    offer(port, |offer| {
        offer.viewer = Some(password.clone());
        // Which view is served at which address. A chapter holds a division
        // per view, so without this the helper would not know which slides
        // belong at which address — and with only one it would serve the
        // projection's whatever the view was set to.
        offer.views = views;
    })
}

/// Says which addresses the stream serves, while it is already serving.
///
/// The list is settled when the switch is thrown, and it was then never looked
/// at again: a view added, renamed or given a different address during a
/// service did not reach the helper. Its new address answered "not found", and
/// — worse — an address whose view the helper had never heard of fell back to
/// whichever state happened to be first, so the congregation's address could
/// show a stage monitor.
///
/// Does nothing when the stream is off, or when the list is what it already
/// was: this is called on every change to the presentation, and a message per
/// slide would be a message per slide for nothing.
pub fn serve_views(views: Vec<super::network_server::ServedView>) {
    let unchanged = {
        let Ok(held) = helper().lock() else {
            return;
        };
        match held.as_ref() {
            // Nothing is being served, so there is nothing to correct. The
            // switch will carry the current list when it is thrown.
            Some(running) if running.offer.viewer.is_none() => true,
            Some(running) => running.offer.views == views,
            None => true,
        }
    };
    if unchanged {
        return;
    }

    let _ = offer(0, |offer| offer.views = views);
}

/// Stops offering it. The helper stays up while the console is still on.
pub fn disable_viewer() {
    let _ = offer(0, |offer| offer.viewer = None);
}

/// Starts offering the presenter console, and says where to find it.
///
/// `password` may be empty. What that means is the operator's decision — see
/// [`Offer`] — and the panel with the switch says it plainly rather than
/// refusing to switch on.
pub fn enable_console(port: u16, password: String) -> Result<String, String> {
    offer(port, |offer| offer.console = Some(password.clone()))
        .map(|address| format!("{address}/console"))
}

/// Stops offering it. The helper stays up while the stream is still on.
pub fn disable_console() {
    let _ = offer(0, |offer| offer.console = None);
}

/// Changes what is on offer, starting or stopping the helper as that requires.
///
/// One function for both switches, because they are one server: the first one
/// on starts it, the last one off stops it, and in between a switch is a
/// message rather than a restart — a service being streamed does not stop
/// being streamed because somebody opened the console.
fn offer(port: u16, change: impl FnOnce(&mut Offer)) -> Result<String, String> {
    let mut held = helper()
        .lock()
        .map_err(|_| "the network server is in a bad state".to_string())?;

    if let Some(running) = held.as_mut() {
        change(&mut running.offer);

        if running.offer.viewer.is_none() && running.offer.console.is_none() {
            // Nothing left to serve. Dropping the helper stops it and gives
            // the port back.
            held.take();
            return Ok(String::new());
        }

        let offer = running.offer.clone();
        let address = running.address.clone();
        if !tell(running, ToChild::Offering(offer)) {
            held.take();
            return Err("the network server stopped listening".to_string());
        }
        return Ok(address);
    }

    let mut wanted = Offer::default();
    change(&mut wanted);
    if wanted.viewer.is_none() && wanted.console.is_none() {
        // Switching off something that was never on.
        return Ok(String::new());
    }

    start(port, wanted, held)
}

/// Starts the helper, with what it is to offer.
fn start(
    port: u16,
    wanted: Offer,
    mut held: std::sync::MutexGuard<'_, Option<Helper>>,
) -> Result<String, String> {
    // The helper connects back to this, on the loopback interface, and proves
    // it is the process that was started. The password never goes on a command
    // line, where every other program on the machine could read it.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| format!("the console helper could not be waited for: {error}"))?;
    let ipc_port = listener
        .local_addr()
        .map_err(|error| format!("the console helper has no address: {error}"))?
        .port();
    let token = uuid::Uuid::new_v4().as_simple().to_string();

    let mut command = Command::new(helper_executable()?);
    command
        .arg(FLAG)
        .arg(ipc_port.to_string())
        .arg(&token)
        .stdin(std::process::Stdio::null());
    no_console_window(&mut command);

    let process = command
        .spawn()
        .map_err(|error| format!("the console helper could not be started: {error}"))?;

    // From here on there is a process running. Every way out of this function
    // has to take it with it, or a helper nobody knows about goes on serving
    // the presentation. `Spawned` is that guarantee; it is disarmed once the
    // helper is stored and its lifetime becomes [`Helper`]'s business.
    let mut spawned = Spawned(Some(process));

    let mut to_child = accept_helper(&listener, &token)?;

    let configuration = Configuration {
        port,
        offer: wanted.clone(),
    };
    let encoded = serde_json::to_string(&configuration)
        .map_err(|error| format!("the console helper cannot be configured: {error}"))?;
    writeln!(to_child, "{encoded}")
        .map_err(|error| format!("the console helper could not be told what to do: {error}"))?;

    // Wait to be told that it is up, so that a port already in use is reported
    // here — beside the switch that did not go on — rather than as a browser
    // failing to connect later.
    let mut reader = BufReader::new(
        to_child
            .try_clone()
            .map_err(|error| format!("the console helper could not be read: {error}"))?,
    );

    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return Err("the console helper stopped before it started".to_string());
    }
    let port = match serde_json::from_str::<ToParent>(line.trim()) {
        Ok(ToParent::Serving { port }) => port,
        Ok(ToParent::Failed { reason }) => return Err(reason),
        Ok(_) => return Err("the console helper said something unexpected".to_string()),
        Err(error) => return Err(format!("the console helper could not be read: {error}")),
    };

    // The handshake is over; from here the socket is read until Cantara or the
    // helper goes away, and a deadline on that would be a disconnection every
    // time nothing happened for a while.
    //
    // On *this* handle, which is the one the pump below reads from. Clearing
    // it on the socket it was cloned from is not enough — a clone carries the
    // timeout it was made with, and its own from then on. That mistake was
    // silent and complete: the pump gave up five seconds after the console
    // opened, so every button pressed on it after that reached nothing, while
    // the console itself went on looking as though it had worked.
    reader
        .get_ref()
        .set_read_timeout(None)
        .map_err(|error| format!("the console helper could not be read: {error}"))?;

    let address = format!("http://{}:{}", super::stream::local_address(), port);

    std::thread::Builder::new()
        .name("cantara-console-host".to_string())
        .spawn(move || pump(reader))
        .map_err(|error| format!("no thread for the console helper: {error}"))?;

    let helper = Helper {
        process: match spawned.0.take() {
            Some(process) => process,
            // Unreachable: nothing takes it before this line. Reported rather
            // than unwrapped, because a panic here would be in the middle of
            // preparing a service.
            None => return Err("the network server was lost while starting".to_string()),
        },
        to_child,
        address: address.clone(),
        last_sent: None,
        sent_media: std::collections::HashSet::new(),
        offer: wanted,
    };

    *held = Some(helper);
    Ok(address)
}

/// Reads the helper's stream until it ends, applying what the remote operator
/// does.
///
/// A read that fails ends this: the helper has gone, and there is nothing left
/// to listen to. That is only true because the socket has no read timeout —
/// with one, "nothing was said for a while" would look exactly the same as
/// "the helper has gone", which is the bug this function was fixed for.
fn pump(mut reader: BufReader<TcpStream>) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        match serde_json::from_str::<ToParent>(line.trim()) {
            Ok(ToParent::Update(presentation)) => {
                remote_console::send(ConsoleCommand::Update(presentation));
            }
            Ok(ToParent::Quit) => remote_console::send(ConsoleCommand::Quit),
            Ok(ToParent::Connections(count)) => CONNECTED.store(count, Ordering::Relaxed),
            Ok(ToParent::Serving { .. }) | Ok(ToParent::Failed { .. }) => {}
            Err(error) => log::warn!("the console helper said something unreadable: {error}"),
        }
    }
}

/// Which program to start as the helper.
///
/// Cantara itself, with a flag — one binary, two jobs. A test may point this
/// somewhere else, which is what lets the half of this module that starts and
/// speaks to a real helper be tested at all: under a test harness
/// `current_exe` is the harness, and it has never heard of `--remote-console`.
fn helper_executable() -> Result<std::path::PathBuf, String> {
    #[cfg(test)]
    if let Ok(path) = std::env::var("CANTARA_TEST_HELPER") {
        return Ok(std::path::PathBuf::from(path));
    }

    std::env::current_exe().map_err(|error| format!("Cantara cannot find itself: {error}"))
}

/// A spawned process that is killed unless somebody takes responsibility for
/// it.
///
/// Every failure between spawning the helper and storing it goes through this:
/// a `?` that returned without it would leave a process holding the console
/// port with nothing in Cantara able to stop it.
struct Spawned(Option<Child>);

impl Drop for Spawned {
    fn drop(&mut self) {
        if let Some(mut process) = self.0.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

/// How long the helper is given to connect back and prove itself.
///
/// Long enough for a slow machine to start a second copy of the program, short
/// enough that a helper that will never come back does not hold the switch —
/// and the panel — still.
const HANDSHAKE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(15);

/// Takes the helper's connection, refusing anything else that reaches the port
/// first.
///
/// Anything on this machine can connect to a loopback port; only the process
/// that was handed the token is the helper. Everything else is hung up on, and
/// the wait has an end: without one, a helper that died on startup would leave
/// the program waiting for it for ever.
fn accept_helper(listener: &TcpListener, token: &str) -> Result<TcpStream, String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("the console helper could not be waited for: {error}"))?;

    let deadline = std::time::Instant::now() + HANDSHAKE_DEADLINE;

    loop {
        match listener.accept() {
            Ok((stream, peer)) => {
                if !peer.ip().is_loopback() {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                }
                match proves_itself(&stream, token) {
                    Ok(true) => {
                        stream.set_nonblocking(false).map_err(|error| {
                            format!("the console helper could not be read: {error}")
                        })?;
                        listener.set_nonblocking(false).map_err(|error| {
                            format!("the console helper could not be waited for: {error}")
                        })?;
                        return Ok(stream);
                    }
                    Ok(false) | Err(_) => {
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("the console helper did not answer: {error}")),
        }

        if std::time::Instant::now() >= deadline {
            return Err("the console helper did not start".to_string());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Whether the first line this connection sends is the token it was given.
fn proves_itself(stream: &TcpStream, token: &str) -> Result<bool, std::io::Error> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;

    let mut given = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut given)?;

    Ok(given.trim() == token)
}

/// Whether the stream is being offered to viewers.
pub fn is_viewer_enabled() -> bool {
    with_helper(|helper| helper.offer.viewer.is_some()).unwrap_or(false)
}

/// Whether the console is being offered.
pub fn is_console_enabled() -> bool {
    with_helper(|helper| helper.offer.console.is_some()).unwrap_or(false)
}

/// The address a viewer types, if there is one to type.
pub fn viewer_address() -> Option<String> {
    with_helper(|helper| {
        helper
            .offer
            .viewer
            .is_some()
            .then(|| helper.address.clone())
    })
    .flatten()
}

/// The address the console is at, if it is being offered.
pub fn console_address() -> Option<String> {
    with_helper(|helper| {
        helper
            .offer
            .console
            .is_some()
            .then(|| format!("{}/console", helper.address))
    })
    .flatten()
}

fn with_helper<T>(read: impl FnOnce(&Helper) -> T) -> Option<T> {
    helper()
        .lock()
        .ok()
        .and_then(|held| held.as_ref().map(read))
}

/// Says one thing to the helper, reporting whether it got there.
fn tell(helper: &mut Helper, message: ToChild) -> bool {
    let Ok(encoded) = serde_json::to_string(&message) else {
        log::warn!("something could not be encoded for the network server");
        // Not the helper's fault, and not a reason to take it down.
        return true;
    };
    writeln!(helper.to_child, "{encoded}").is_ok()
}

/// How many browsers have it open.
pub fn connected() -> usize {
    CONNECTED.load(Ordering::Relaxed)
}

/// Hands the presentation to the helper, if one is running.
///
/// Safe to call as often as anything likes — on every change, from a poll,
/// from the switch — because it sends only what is new. Everything that can
/// notice a change calls this, and the ones that notice the same change twice
/// cost a comparison.
pub fn publish(presentation: Option<RunningPresentation>) {
    let Ok(mut held) = helper().lock() else {
        return;
    };
    let Some(helper) = held.as_mut() else {
        return;
    };

    // Only what is new. The scroll position is excluded for the reason
    // [`RunningPresentation::eq_ignoring_scroll`] gives — it is reported
    // several times a second by whoever is scrolling and is not news.
    let unchanged = match (&helper.last_sent, &presentation) {
        (Some(sent), Some(now)) => sent.eq_ignoring_scroll(now),
        (None, None) => true,
        _ => false,
    };
    if unchanged {
        return;
    }
    helper.last_sent = presentation.clone();

    // The slide as HTML, drawn by the very same components the projector
    // uses — see [`crate::components::stream_render`]. Rendered *here*,
    // once per change, rather than by the helper (which has no library and
    // no settings) or per viewer (which is what ruled out a liveview
    // session each).
    //
    // Which design is a question the presentation answers for itself:
    // `get_current_stream_design` is what `StreamState::of` already reads,
    // so the rendering and the rest of the payload cannot disagree about
    // what the phones are being shown.
    let rendered = match presentation.as_ref() {
        Some(running) => render_for_views(running, &helper.offer.views),
        None => std::collections::HashMap::new(),
    };

    // A helper that will not take it is a helper that has gone. Dropping it
    // here is what puts the switches back to where the truth is.
    if !tell(
        helper,
        ToChild::Presentation {
            presentation: Box::new(presentation),
            rendered,
        },
    ) {
        log::warn!("the network server stopped listening; it is off");
        held.take();
    }
}

/// The presentation as HTML, once for every view being served.
///
/// The one place a rendering is made. Both the ordinary publish and the
/// once-a-second refresh for clocks go through it, so that what an address is
/// shown cannot depend on which of the two last spoke — and so that the two
/// cannot come to disagree about which design a view uses.
///
/// Rendered once per view per change, not once per viewer: that is the property
/// that made a static page the right answer for a congregation, and it survives
/// having several of them.
fn render_for_views(
    running: &RunningPresentation,
    views: &[super::network_server::ServedView],
) -> std::collections::HashMap<uuid::Uuid, String> {
    views
        .iter()
        .map(|view| {
            let division = crate::logic::states::Division::View(view.id);
            (
                view.id,
                crate::components::stream_render::for_network(
                    &crate::components::stream_render::render_presentation(
                        running,
                        Some(running.current_design_in(division)),
                    ),
                ),
            )
        })
        .collect()
}

/// Renders again for the views whose design shows the time.
///
/// A monitor view may carry a clock or a timer counting the current element.
/// In a window those redraw on a timer of their own; over the network they
/// cannot, because what a viewer is shown is HTML rendered when *something
/// changes* — and the passing of a second is not a change to the presentation.
/// So a stage monitor on a phone showed the time the slide came up and held it
/// there until the next slide.
///
/// Called on a timer. Does nothing at all unless a view being served is a
/// monitor with such a widget on it, so an ordinary service pays nothing for
/// this.
pub fn refresh_time_widgets() {
    let Ok(mut held) = helper().lock() else {
        return;
    };
    let Some(helper) = held.as_mut() else {
        return;
    };
    let Some(running) = helper.last_sent.clone() else {
        return;
    };

    // Is there anything here that follows the clock? If not, this second costs
    // nothing at all.
    let ticking = helper.offer.views.iter().any(|view| {
        running
            .current_design_in(crate::logic::states::Division::View(view.id))
            .presentation_design_settings
            .monitor()
            .is_some_and(|monitor| monitor.has_live_widget())
    });
    if !ticking {
        return;
    }

    // Every view, not only the ones with a clock on them.
    //
    // The helper is *given* the renderings, and what it is given replaces what
    // it had — so a map holding only the ticking views left every other
    // address with nothing at all. The bare address went blank a second after
    // the presentation started, which is as bad as this gets.
    //
    // Rendering the others again costs a slide or two per second and is the
    // same code path as an ordinary publish, which is worth more than the
    // saving. See `render_for_views`.
    let rendered = render_for_views(&running, &helper.offer.views);

    // The presentation itself has not changed, so `last_sent` is left as it
    // is: this is the same state drawn at a later moment, not a new one.
    if !tell(
        helper,
        ToChild::Presentation {
            presentation: Box::new(Some(running)),
            rendered,
        },
    ) {
        log::warn!("the network server stopped listening; it is off");
        held.take();
    }
}

/// Which pictures the helper has not been given yet.
///
/// Asked before rendering rather than after, because rendering a PDF page is
/// the expensive part and doing it twice for the same picture is the thing
/// worth avoiding.
pub fn media_wanted(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let Ok(held) = helper().lock() else {
        return Vec::new();
    };
    let Some(helper) = held.as_ref() else {
        return Vec::new();
    };

    ids.into_iter()
        .filter(|id| !helper.sent_media.contains(id))
        .collect()
}

/// Hands over a picture, under the name the state gives it.
pub fn publish_media(id: String, bytes: Vec<u8>, content_type: &'static str) {
    let Ok(mut held) = helper().lock() else {
        return;
    };
    let Some(helper) = held.as_mut() else {
        return;
    };

    if !helper.sent_media.insert(id.clone()) {
        return;
    }
    if !tell(
        helper,
        ToChild::Media {
            id,
            bytes,
            content_type: content_type.to_string(),
        },
    ) {
        held.take();
    }
}

/// Says where the video on the current slide has got to.
///
/// Sent as it changes and not oftener: a phone leaves its own playback alone
/// until it is more than half a second out.
pub fn publish_video_position(position: Option<(f64, f64, bool)>) {
    let Ok(mut held) = helper().lock() else {
        return;
    };
    let Some(helper) = held.as_mut() else {
        return;
    };

    if !tell(helper, ToChild::VideoPosition(position)) {
        held.take();
    }
}

/// Keeps a console window from flashing up beside the helper on Windows.
#[cfg(target_os = "windows")]
fn no_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    /// `CREATE_NO_WINDOW`, from the Windows API.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn no_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{Ipv4Addr, TcpListener, TcpStream};

    /// Cantara's own half of starting the network server: the switch is
    /// Serialises the tests that start a real helper.
    ///
    /// There is one helper per *process* — see [`helper`] — and `cargo test`
    /// runs tests in parallel. Two tests that each switch the stream on and
    /// off are then fighting over one global: one stops the helper the other
    /// is in the middle of asking. It failed about one run in fifteen, which
    /// is the worst rate for a thing like this.
    static ONE_HELPER_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The helper binary, if there is one worth starting.
    ///
    /// Lifted out of the test below so a second test that needs a real helper
    /// does not repeat the reasoning about what is lying in `target/debug`.
    fn test_helper() -> Option<std::path::PathBuf> {
        // Never on a build server.
        //
        // Whether these run at all turned on a comparison of file times
        // between the binary and the test harness, and on a fresh checkout the
        // order those are built in is not fixed — so on CI a test that spawns
        // a process and binds a socket ran or did not, by coin flip. A test
        // that sometimes runs is worse than one that does not: it fails for
        // reasons that have nothing to do with the change under test, and the
        // failure is unreproducible.
        //
        // They are worth having and they are worth running — by hand, on a
        // machine where `target/debug/cantara` is the binary you just built.
        // `CI` is set by every build server worth the name, GitHub Actions
        // included.
        if std::env::var_os("CI").is_some() {
            eprintln!("skipped: starts a real process, which is not for a build server");
            return None;
        }

        let helper = std::path::Path::new("target/debug/cantara.exe");
        let helper = if helper.exists() {
            helper
        } else {
            let unix = std::path::Path::new("target/debug/cantara");
            if !unix.exists() {
                eprintln!("skipped: the binary has not been built");
                return None;
            }
            unix
        };

        if is_older_than_this_test(helper) {
            eprintln!("skipped: target/debug/cantara is older than this test — `cargo build` first");
            return None;
        }

        Some(helper.to_path_buf())
    }

    /// The switch is thrown, a helper appears, and the address it reports
    /// answers.
    ///
    /// The half that had never been tested — under a test harness
    /// `current_exe` is the harness — and the half that has since broken
    /// twice. It starts the real binary, so it is skipped where that has not
    /// been built.
    #[test]
    fn a_switch_puts_a_server_on_the_network() {
        // A poisoned lock means another helper test panicked; the guard is
        // still what serialises this one, so it is taken either way.
        let _helper_test = ONE_HELPER_AT_A_TIME
            .lock()
            .unwrap_or_else(|held| held.into_inner());

        let Some(helper) = test_helper() else {
            return;
        };

        // SAFETY: single-threaded at this point in the test, and read only by
        // `helper_executable` below.
        unsafe { std::env::set_var("CANTARA_TEST_HELPER", helper) };

        // Port 0: whatever is free, so that a machine already running Cantara
        // does not fail this.
        // Two views, because one of them is the case that was broken: a view
        // at an address of its own answered "not found", and the bare address
        // said the presentation had not begun. The helper is *started* with
        // this offer — throwing the switch is what starts it — so nothing
        // arrives afterwards to correct a helper that ignored it.
        let stage = uuid::Uuid::from_u128(0x51a9e);
        let address = match enable_viewer(
            0,
            String::new(),
            vec![
                crate::logic::network_server::ServedView {
                    path: "/".to_string(),
                    id: uuid::Uuid::nil(),
                },
                crate::logic::network_server::ServedView {
                    path: "/test".to_string(),
                    id: stage,
                },
            ],
        ) {
            Ok(address) => address,
            Err(reason) => panic!("the switch did not go on: {reason}"),
        };
        assert!(is_viewer_enabled(), "the switch says it is on");
        assert_eq!(viewer_address().as_deref(), Some(address.as_str()));

        let port = address
            .rsplit(':')
            .next()
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or_else(|| panic!("no port in {address}"));

        // The address is the one to read out, so it is the one that has to
        // answer.
        let answered = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            std::time::Duration::from_secs(5),
        );
        assert!(
            answered.is_ok(),
            "nothing is listening on the address the panel shows: {address}"
        );

        // The address a view was given answers, and answers with *its* view.
        //
        // This is what the report was: `/test` said "not found" because the
        // helper had thrown away the offer it was started with, so it was
        // serving no views at all — which is also why the bare address said
        // the presentation had not begun.
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("a client");

        let named = client
            .get(format!("http://127.0.0.1:{port}/test"))
            .send()
            .expect("the named address answers");
        assert!(
            named.status().is_success(),
            "a view's own address was not served: {}",
            named.status()
        );
        assert!(
            named.text().unwrap_or_default().contains(&stage.to_string()),
            "the page at /test was not told which view it shows"
        );

        // And an address nobody was given is still not an address.
        let unknown = client
            .get(format!("http://127.0.0.1:{port}/nothing-here"))
            .send()
            .expect("answers");
        assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);

        // And the console shares it, rather than taking one of its own.
        let console = enable_console(0, "control".to_string()).expect("the console goes on");
        assert_eq!(console, format!("{address}/console"));

        disable_viewer();
        assert!(!is_viewer_enabled());
        assert!(is_console_enabled(), "the console is left running");

        disable_console();
        assert!(!is_console_enabled());
        assert!(
            viewer_address().is_none(),
            "with both switches off there is no address"
        );
    }

    /// Whether `candidate` was built before this test harness was.
    ///
    /// Unknown times count as fresh: a filesystem that does not keep them is
    /// not a reason to stop testing what this tests.
    fn is_older_than_this_test(candidate: &std::path::Path) -> bool {
        let built = |path: std::path::PathBuf| {
            std::fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        };
        let (Some(helper), Some(harness)) = (
            built(candidate.to_path_buf()),
            std::env::current_exe().ok().and_then(built),
        ) else {
            return false;
        };
        helper < harness
    }

    /// Two network views, two addresses, two different renderings.
    ///
    /// Reported from a real service: with a monitor view at `/test` and a
    /// plain one at `/`, *both* addresses showed the monitor. The rendering
    /// itself is right — see `stream_render` — so this exercises the whole
    /// way through: the offer, the paths the helper serves, the page each
    /// address is given, and the state that page then asks for.
    #[test]
    fn two_addresses_show_their_own_view() {
        // A poisoned lock means another helper test panicked; the guard is
        // still what serialises this one, so it is taken either way.
        let _helper_test = ONE_HELPER_AT_A_TIME
            .lock()
            .unwrap_or_else(|held| held.into_inner());

        use crate::logic::settings::{
            MonitorDesign, PresentationDesign, PresentationDesignSettings,
        };
        use crate::logic::stream_view::ViewDefaults;

        let Some(helper) = test_helper() else {
            return;
        };
        // SAFETY: single-threaded at this point in the test.
        unsafe { std::env::set_var("CANTARA_TEST_HELPER", helper) };

        let plain = uuid::Uuid::from_u128(30);
        let monitor = uuid::Uuid::from_u128(31);

        let address = enable_viewer(
            0,
            String::new(),
            vec![
                crate::logic::network_server::ServedView {
                    path: "/".to_string(),
                    id: plain,
                },
                crate::logic::network_server::ServedView {
                    path: "/test".to_string(),
                    id: monitor,
                },
            ],
        )
        .expect("the switch goes on");

        let port = address
            .rsplit(':')
            .next()
            .and_then(|port| port.parse::<u16>().ok())
            .expect("a port");

        // A running order in which the monitor view — and only it — is shown a
        // design of its own.
        let item = crate::logic::states::SelectedItemRepresentation::new_with_sourcefile(
            crate::logic::sourcefiles::SourceFile {
                name: "Amazing Grace".to_string(),
                path: std::path::PathBuf::from("testfiles/Amazing Grace.song"),
                file_type: crate::logic::sourcefiles::SourceFileType::Song,
                md5_hash: None,
                relative_path: None,
            },
        );
        let stage_design = PresentationDesign {
            name: "Stage".to_string(),
            description: String::new(),
            presentation_design_settings: PresentationDesignSettings::Monitor(
                MonitorDesign::default(),
            ),
        };
        let mut running = crate::logic::presentation::build_presentation(
            &vec![item],
            &PresentationDesign::default(),
            &cantara_songlib::slides::SlideSettings::default(),
            &[
                ViewDefaults {
                    id: plain,
                    design: None,
                    slide_settings: None,
                },
                ViewDefaults {
                    id: monitor,
                    design: Some(stage_design),
                    slide_settings: None,
                },
            ],
            &[],
        )
        .expect("a presentation");
        running.jump_to(0, 0);

        publish(Some(running));

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("a client");

        // What each address's page is told to ask for, then what it is given.
        let shown = |path: &str| {
            let page = client
                .get(format!("http://127.0.0.1:{port}{path}"))
                .send()
                .expect("the address answers")
                .text()
                .expect("a body");
            let id = page
                .split("var viewId = '")
                .nth(1)
                .and_then(|rest| rest.split('\'').next())
                .unwrap_or_default()
                .to_string();
            let state = client
                .get(format!("http://127.0.0.1:{port}/state?view={id}"))
                .send()
                .expect("the state answers")
                .text()
                .expect("a body");
            (id, state)
        };

        let (root_id, root_state) = shown("/");
        let (test_id, test_state) = shown("/test");

        assert_eq!(root_id, plain.to_string(), "/ was told the wrong view");
        assert_eq!(test_id, monitor.to_string(), "/test was told the wrong view");
        assert!(
            test_state.contains("monitor-view"),
            "/test is not the monitor view"
        );
        assert!(
            !root_state.contains("monitor-view"),
            "/ was given the monitor view as well"
        );

        disable_viewer();
    }

    /// Every served address keeps its rendering when the clock ticks.
    ///
    /// The once-a-second refresh for clocks used to send a map holding only
    /// the views that *have* a clock — and the helper replaces what it has
    /// with what it is given, so every other address lost its rendering a
    /// second after the presentation started. The bare address went blank
    /// while the stage monitor beside it carried on.
    #[test]
    fn a_refresh_renders_for_every_view_not_only_the_ticking_one() {
        use crate::logic::settings::{
            MonitorDesign, MonitorWidget, PresentationDesign, PresentationDesignSettings,
            WidgetKind, WidgetPlacement,
        };
        use crate::logic::stream_view::ViewDefaults;

        let plain = uuid::Uuid::from_u128(40);
        let ticking = uuid::Uuid::from_u128(41);

        // A monitor design with a clock on it — the one that follows the
        // clock — and a view that is shown the projection.
        let with_clock = PresentationDesign {
            name: "Stage".to_string(),
            description: String::new(),
            presentation_design_settings: PresentationDesignSettings::Monitor(MonitorDesign {
                widgets: vec![MonitorWidget {
                    kind: WidgetKind::Clock { with_date: false },
                    placement: WidgetPlacement::TopRight,
                }],
                ..MonitorDesign::default()
            }),
        };

        let item = crate::logic::states::SelectedItemRepresentation::new_with_sourcefile(
            crate::logic::sourcefiles::SourceFile {
                name: "Amazing Grace".to_string(),
                path: std::path::PathBuf::from("testfiles/Amazing Grace.song"),
                file_type: crate::logic::sourcefiles::SourceFileType::Song,
                md5_hash: None,
                relative_path: None,
            },
        );
        let mut running = crate::logic::presentation::build_presentation(
            &vec![item],
            &PresentationDesign::default(),
            &cantara_songlib::slides::SlideSettings::default(),
            &[
                ViewDefaults {
                    id: plain,
                    design: None,
                    slide_settings: None,
                },
                ViewDefaults {
                    id: ticking,
                    design: Some(with_clock),
                    slide_settings: None,
                },
            ],
            &[],
        )
        .expect("a presentation");
        running.jump_to(0, 0);

        let views = vec![
            crate::logic::network_server::ServedView {
                path: "/".to_string(),
                id: plain,
            },
            crate::logic::network_server::ServedView {
                path: "/stage".to_string(),
                id: ticking,
            },
        ];

        let rendered = render_for_views(&running, &views);

        assert!(
            rendered.contains_key(&plain),
            "the bare address lost its rendering"
        );
        assert!(rendered.contains_key(&ticking));
        assert!(
            rendered[&ticking].contains("monitor-view"),
            "the stage is not the monitor view"
        );
        assert!(
            !rendered[&plain].contains("monitor-view"),
            "the bare address was given the monitor view"
        );
    }

    /// The deadline has to be cleared on the handle that is *read from*, and
    /// clearing it there is enough on every platform.
    ///
    /// That is the whole of the bug which made the remote console look as
    /// though it worked and do nothing: the handshake gave the socket a
    /// deadline, the thread that reads the console's commands read through a
    /// clone made while that deadline was set, and five seconds later its
    /// first quiet moment looked exactly like the helper having gone. It gave
    /// up, and every button pressed after that reached nothing.
    ///
    /// The first version of this test asserted the other half of the surprise
    /// as well — that clearing the deadline on the socket the clone was *made
    /// from* leaves the clone with it. That is true where a clone is a socket
    /// of its own, and false on Linux, where `try_clone` is `dup` and both
    /// handles are the one socket with the one `SO_RCVTIMEO`. Asserting it
    /// failed every Linux build for a difference Cantara does not depend on:
    /// the code clears the deadline on the reader, which is right either way.
    /// What is left here is that, and it is what is tested.
    #[test]
    fn the_deadline_is_cleared_on_the_handle_that_is_read_from() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a port");
        let port = listener.local_addr().expect("an address").port();

        // A second of quiet against a tenth of a second of deadline. Ten times
        // the margin is what keeps the two reads below from depending on how
        // busy the machine running them is: the quiet has to outlast the
        // deadline for the first read to fail, and the reader has to reach the
        // second read before the quiet ends.
        let quiet = std::time::Duration::from_secs(1);
        let deadline = std::time::Duration::from_millis(100);

        let writer = std::thread::spawn(move || {
            let (mut sender, _) = listener.accept().expect("a connection");
            writeln!(sender, "first").expect("the first line");
            // This is the quiet the pump used to mistake for the end.
            std::thread::sleep(quiet);
            writeln!(sender, "second").expect("the second line");
        });

        let original = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        original
            .set_read_timeout(Some(deadline))
            .expect("a deadline for the handshake");

        let mut reader = BufReader::new(original.try_clone().expect("a clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("the first line");
        assert_eq!(line.trim(), "first");

        // Still under the handshake's deadline: a stretch with nothing in it
        // is an error, and an error is what the pump reads as "the helper has
        // gone".
        line.clear();
        assert!(
            reader.read_line(&mut line).is_err(),
            "a deadline turns a quiet socket into a broken one"
        );

        // What the code does: clears it on the handle the pump reads through.
        reader.get_ref().set_read_timeout(None).expect("cleared");
        line.clear();
        reader
            .read_line(&mut line)
            .expect("the quiet stretch is no longer the end of the helper");
        assert_eq!(line.trim(), "second");

        writer.join().expect("the writer finishes");
    }
}
