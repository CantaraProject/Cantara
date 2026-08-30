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
pub fn enable_viewer(port: u16, password: String) -> Result<String, String> {
    offer(port, |offer| offer.viewer = Some(password.clone()))
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

    // A helper that will not take it is a helper that has gone. Dropping it
    // here is what puts the switches back to where the truth is.
    if !tell(helper, ToChild::Presentation(Box::new(presentation))) {
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
    /// thrown, a helper appears, and the address it reports answers.
    ///
    /// The half that had never been tested — under a test harness
    /// `current_exe` is the harness — and the half that has since broken
    /// twice. It starts the real binary, so it is skipped where that has not
    /// been built.
    #[test]
    fn a_switch_puts_a_server_on_the_network() {
        let helper = std::path::Path::new("target/debug/cantara.exe");
        let helper = if helper.exists() {
            helper
        } else {
            let unix = std::path::Path::new("target/debug/cantara");
            if !unix.exists() {
                eprintln!("skipped: the binary has not been built");
                return;
            }
            unix
        };

        // `cargo test` does not rebuild the binary — it builds this harness —
        // so what is lying in `target/debug` may be from before the flag the
        // helper is started with even existed. That binary opens a window
        // instead of connecting back, and the test then fails fifteen seconds
        // later saying the helper did not start, which is true and says
        // nothing about the code being tested. Older than the harness is the
        // one honest reading: skip, and say why.
        if is_older_than_this_test(helper) {
            eprintln!("skipped: target/debug/cantara is older than this test — `cargo build` first");
            return;
        }

        // SAFETY: single-threaded at this point in the test, and read only by
        // `helper_executable` below.
        unsafe { std::env::set_var("CANTARA_TEST_HELPER", helper) };

        // Port 0: whatever is free, so that a machine already running Cantara
        // does not fail this.
        let address = match enable_viewer(0, String::new()) {
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
