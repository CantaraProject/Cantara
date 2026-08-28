//! Starting, feeding and stopping the console helper.
//!
//! The counterpart to [`crate::logic::remote_console_child`], which explains
//! why the console is a process of its own. This half is what the switch in
//! the presentation options talks to: it starts the helper, hands it the
//! presentation whenever it changes, applies what the remote operator does,
//! and stops it again.
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

use super::remote_console::{self, ConsoleCommand};
use super::remote_console_child::{Configuration, ToChild, ToParent, FLAG};
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
    /// Where the console is being served.
    address: String,
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

/// Starts offering the presenter console, and says where to find it.
///
/// `password` may be empty. What that means is the operator's decision — see
/// [`Configuration::password`] — and the panel with the switch says it plainly
/// rather than refusing to switch on.
pub fn enable(port: u16, password: String) -> Result<String, String> {
    let mut held = helper()
        .lock()
        .map_err(|_| "the console helper is in a bad state".to_string())?;

    // Switching on twice is not an error, it is the same answer twice.
    if let Some(running) = held.as_ref() {
        return Ok(running.address.clone());
    }

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

    let executable =
        std::env::current_exe().map_err(|error| format!("Cantara cannot find itself: {error}"))?;
    let mut command = Command::new(executable);
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

    let configuration = Configuration { port, password };
    let encoded = serde_json::to_string(&configuration)
        .map_err(|error| format!("the console helper cannot be configured: {error}"))?;
    writeln!(to_child, "{encoded}")
        .map_err(|error| format!("the console helper could not be told what to do: {error}"))?;

    // Wait to be told that it is up, so that a port already in use is reported
    // here — beside the switch that did not go on — rather than as a browser
    // failing to connect later.
    let reader = BufReader::new(
        to_child
            .try_clone()
            .map_err(|error| format!("the console helper could not be read: {error}"))?,
    );
    let mut lines = reader.lines();
    let port = match lines.next() {
        Some(Ok(line)) => match serde_json::from_str::<ToParent>(&line) {
            Ok(ToParent::Serving { port }) => port,
            Ok(ToParent::Failed { reason }) => return Err(reason),
            Ok(_) => return Err("the console helper said something unexpected".to_string()),
            Err(error) => return Err(format!("the console helper could not be read: {error}")),
        },
        Some(Err(error)) => return Err(format!("the console helper could not be read: {error}")),
        None => return Err("the console helper stopped before it started".to_string()),
    };

    // The handshake is over; from here the socket is read until Cantara or the
    // helper goes away, and a deadline on that would be a disconnection every
    // time nothing happened for a while.
    to_child
        .set_read_timeout(None)
        .map_err(|error| format!("the console helper could not be read: {error}"))?;

    let address = format!("http://{}:{}/console", super::stream::local_address(), port);

    std::thread::Builder::new()
        .name("cantara-console-host".to_string())
        .spawn(move || pump(lines))
        .map_err(|error| format!("no thread for the console helper: {error}"))?;

    let helper = Helper {
        process: match spawned.0.take() {
            Some(process) => process,
            // Unreachable: nothing takes it before this line. Reported rather
            // than unwrapped, because a panic here would be in the middle of
            // preparing a service.
            None => return Err("the console helper was lost while starting".to_string()),
        },
        to_child,
        address: address.clone(),
    };

    *held = Some(helper);
    Ok(address)
}

/// Reads the helper's stream until it ends, applying what the remote operator
/// does.
fn pump(lines: std::io::Lines<BufReader<TcpStream>>) {
    for line in lines {
        let Ok(line) = line else {
            return;
        };
        match serde_json::from_str::<ToParent>(&line) {
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

/// Stops offering it. Doing this when nothing is running is not an error.
pub fn disable() {
    if let Ok(mut held) = helper().lock() {
        held.take();
    }
}

/// Whether the console is being offered.
pub fn is_enabled() -> bool {
    helper().lock().map(|held| held.is_some()).unwrap_or(false)
}

/// Where the console is, if it is being offered.
pub fn address() -> Option<String> {
    helper()
        .lock()
        .ok()
        .and_then(|held| held.as_ref().map(|helper| helper.address.clone()))
}

/// How many browsers have it open.
pub fn connected() -> usize {
    CONNECTED.load(Ordering::Relaxed)
}

/// Hands the presentation to the helper, if one is running.
///
/// Called on every change, and cheap when there is no helper: nothing is
/// encoded until there is somebody to send it to.
pub fn publish(presentation: Option<RunningPresentation>) {
    let Ok(mut held) = helper().lock() else {
        return;
    };
    let Some(helper) = held.as_mut() else {
        return;
    };

    let Ok(encoded) = serde_json::to_string(&ToChild::Presentation(Box::new(presentation))) else {
        log::warn!("the presentation could not be sent to the console helper");
        return;
    };

    // A helper that will not take it is a helper that has gone. Dropping it
    // here is what puts the switch back to where the truth is.
    if writeln!(helper.to_child, "{encoded}").is_err() {
        log::warn!("the console helper stopped listening; the remote console is off");
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
