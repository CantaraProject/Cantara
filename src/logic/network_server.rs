//! Cantara's network side: one process, one socket, both services.
//!
//! A browser on the hall's network reaches the presenter console here, and so
//! does a phone in a pew reaching the stream. They are one address and one
//! port because they are one server — the viewer's routes come from
//! [`crate::logic::stream::server`] and are merged with the console's onto the
//! same listener.
//!
//! Nothing here decides anything. What is offered, to whom, and what there is
//! to show all arrive over a socket from Cantara itself; this process serves
//! them. See [`crate::logic::network_host`] for the other half.
//!
//! # Why a process of its own
//!
//! Because `dioxus_html` keeps *one* event converter for the whole process —
//! `EVENT_CONVERTER`, a global — and the two renderers disagree about what an
//! event is. `dioxus_desktop` installs a converter that reads a mounted
//! element as its `DesktopElement` and a form event as its `DesktopFormData`;
//! `dioxus_liveview::LiveViewPool::new` overwrites it with one that reads the
//! same events as its own types. Neither tolerates the other's: both
//! `unwrap()` the downcast, in a place that cannot unwind.
//!
//! In one process the last one installed wins and the other aborts the
//! program. Cantara's own crash was the plainest possible version of it —
//! creating the pool switched the converter, the main window mounted an
//! element, and the process died at `events.rs:91`.
//!
//! So the console runs where nothing has installed a desktop converter: a
//! second copy of this binary, started with `--remote-console`, which never
//! calls [`dioxus::launch`] and therefore never gets one. It holds no state of
//! its own — it is given the presentation over a socket and sends back what
//! the operator does with it, which is the bridge in
//! [`crate::logic::remote_console`] with a process boundary in the middle.
//!
//! # What it is given
//!
//! Nothing on the command line but a port and a token, because a command line
//! is readable by everything on the machine. It connects to the parent on the
//! loopback interface, proves it is the child that was started, and is then
//! told which port to serve on and what password to ask for. The same
//! reasoning, and the same shape, as [`crate::logic::video_server`].

use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, RwLock};

use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::remote_console::{self, ConsoleCommand};
use super::states::RunningPresentation;
use super::stream::protocol::StreamState;
use super::stream::StreamServer;

/// The argument that turns this binary into the console helper.
pub const FLAG: &str = "--remote-console";

/// The name of the cookie a browser is given once it has proved it may drive
/// the presentation.
const CONSOLE_COOKIE: &str = "cantara_console";

/// The page that asks for the password.
const LOGIN_PAGE: &str = include_str!("../../assets/console_login.html");

/// The program's stylesheets, put straight into the page.
///
/// The console asks for these itself, with the `document::Link` elements every
/// other window uses, and LiveView does inject them — as a `<link>` pointing at
/// the hashed asset URL, which the browser then has to fetch back through
/// `/assets/…`. That is one more thing that has to be right in a process that
/// was not started by the bundler, and when it is not right the console arrives
/// unstyled, which is what happened.
///
/// Compiled in instead. A couple of hundred kilobytes on the first load of a
/// page that is then live for the whole service, no second request, and
/// nothing to resolve: the console is styled or the binary does not build.
/// None of these sheets refers to a font or a picture by URL, so there is
/// nothing left dangling.
///
/// All four of them, and that took a second try. `main.css` holds Cantara's
/// own rules *on top of* PicoCSS, which `App` registers separately — and the
/// helper never runs `App`. With only Cantara's own rules the console arrived
/// looking like an unstyled document with a few things in the right places,
/// which is exactly what was reported. `presentation.css` is here for the
/// same reason: the console previews slides with the renderer the projection
/// uses, and that renderer's stylesheet is registered by whoever hosts it.
const PICO_CSS: &str = include_str!("../../node_modules/@picocss/pico/css/pico.min.css");
const MAIN_CSS: &str = include_str!("../../assets/main.css");
const PRESENTATION_CSS: &str = include_str!("../../assets/presentation.css");
const CONSOLE_CSS: &str = include_str!("../../assets/presenter_console.css");

/// What the bundled `pdf.js` expects a browser to have and some browsers do
/// not yet.
///
/// `Map.prototype.getOrInsertComputed` is new — new enough that the Chromium
/// web view Cantara draws its own windows in has it and a current Firefox does
/// not. Without it every PDF slide in a remote console is a black rectangle
/// and the only sign of why is `TypeError: this[#Ar].getOrInsertComputed is
/// not a function` in a console nobody is looking at.
///
/// Written to the specification's behaviour and only where it is missing, so a
/// browser that has the real thing is untouched. It goes in ahead of
/// everything else on the page, because `pdf.js` is loaded on demand and may
/// arrive at any moment after that.
const MAP_POLYFILL: &str = r#"
(function () {
    function define(prototype, name, implementation) {
        if (typeof prototype[name] !== "function") {
            Object.defineProperty(prototype, name, {
                value: implementation,
                writable: true,
                configurable: true,
                enumerable: false,
            });
        }
    }
    function getOrInsert(key, value) {
        if (!this.has(key)) { this.set(key, value); }
        return this.get(key);
    }
    function getOrInsertComputed(key, compute) {
        if (!this.has(key)) { this.set(key, compute(key)); }
        return this.get(key);
    }
    for (const prototype of [Map.prototype, WeakMap.prototype]) {
        define(prototype, "getOrInsert", getOrInsert);
        define(prototype, "getOrInsertComputed", getOrInsertComputed);
    }
})();
"#;

/// How long a wrong password is answered with nothing.
///
/// Guessing at a password over a network is the one attack this feature has,
/// and it costs nothing to make it slow.
const WRONG_PASSWORD_DELAY: std::time::Duration = std::time::Duration::from_millis(750);

/// What the parent tells the child once it has proved itself.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Configuration {
    /// The port to serve on. Both services share it.
    pub port: u16,
    /// What is on offer to begin with.
    pub offer: Offer,
}

/// Which of the two services are being offered, and what each asks for.
///
/// `None` means switched off. An empty password means no password: for the
/// stream that is the ordinary case in a hall, and for the console it is the
/// operator's decision — a locked room on a network with nothing else on it is
/// a real situation, and a program that insists on a password there is in the
/// way rather than being careful. The panel with the switches says plainly
/// what an empty one means.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Offer {
    pub viewer: Option<String>,
    pub console: Option<String>,
}

/// What travels from the parent to the child.
#[derive(Serialize, Deserialize)]
pub enum ToChild {
    /// The presentation as it now stands, or nothing between services.
    ///
    /// Everything a viewer is shown is worked out from this, here — see
    /// [`crate::logic::stream::protocol::StreamState::of`]. The console gets
    /// the same value, because it is the same presentation.
    Presentation(Box<Option<RunningPresentation>>),

    /// A picture a slide refers to, rendered into bytes.
    ///
    /// Rendered by Cantara rather than here, because a PDF page is drawn by
    /// the web view Cantara has and this process has not — see
    /// [`crate::logic::pdf::page_image`]. Everything else about serving it
    /// belongs here.
    Media {
        id: String,
        bytes: Vec<u8>,
        content_type: String,
    },

    /// Where the video on the current slide has got to, so that a phone shows
    /// the same moment of it as the room does.
    VideoPosition(Option<(f64, f64, bool)>),

    /// A switch was thrown.
    Offering(Offer),
}

/// What travels back.
#[derive(Serialize, Deserialize)]
pub enum ToParent {
    /// The presentation as a remote console now has it.
    Update(Box<RunningPresentation>),
    /// End the presentation.
    Quit,
    /// How many browsers have the console open.
    Connections(usize),
    /// The console is being served, at this address.
    Serving { port: u16 },
    /// It could not be, and why.
    Failed { reason: String },
}

/// Runs the helper. Returns when the parent goes away.
///
/// Called from `main` before anything else has a chance to start a window, and
/// never returns to it.
pub fn run(ipc_port: u16, token: &str) -> Result<(), String> {
    // The helper never runs `App`, which is where Cantara picks its language,
    // so it would otherwise serve an English console to a German operator.
    if let Some(locale) = sys_locale::get_locale() {
        rust_i18n::set_locale(&locale);
    }

    let mut socket = TcpStream::connect(("127.0.0.1", ipc_port))
        .map_err(|error| format!("the console helper could not reach Cantara: {error}"))?;

    writeln!(socket, "{token}")
        .map_err(|error| format!("the console helper could not announce itself: {error}"))?;

    // The configuration, read a byte at a time rather than through a buffered
    // reader: whatever the buffer took past the end of the line would be lost
    // when the socket is handed to the runtime below, and the first thing
    // Cantara sends after it is the presentation.
    let line = read_line(&socket)?;

    let configuration: Configuration = serde_json::from_str(line.trim()).map_err(|error| {
        format!("the console helper was told something it cannot read: {error}")
    })?;

    serve(configuration, socket)
}

/// Reads one line from `socket`.
///
/// A byte at a time, which is slow and does not matter: this is one line, once,
/// at startup. A buffered reader would be faster and would swallow whatever
/// came after the line into a buffer that is dropped when the socket is handed
/// to the runtime — and what comes after it is the presentation.
fn read_line(socket: &TcpStream) -> Result<String, String> {
    use std::io::Read;

    let mut reader = socket;
    let mut line = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        match reader.read(&mut byte) {
            Ok(0) => return Err("Cantara said nothing to the console helper".to_string()),
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
            }
            Err(error) => {
                return Err(format!(
                    "the console helper could not be told what to do: {error}"
                ))
            }
        }
    }

    String::from_utf8(line).map_err(|error| format!("Cantara said something unreadable: {error}"))
}

/// Puts the console up and pumps the socket until it closes.
///
/// Everything inside the runtime is an ordinary async task, deliberately.
/// Blocking tasks cannot be cancelled, and dropping a runtime *waits* for
/// them: a helper whose command pump was blocked on a channel that never
/// closes kept the process alive after Cantara had gone, still serving the
/// console. One was found running an hour later, which is exactly the kind of
/// thing a helper process must never do.
fn serve(configuration: Configuration, socket: TcpStream) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime for the network server: {error}"))?;

    let shared = Arc::new(Shared {
        password: RwLock::new(configuration.offer.console),
        session: new_session_token(),
    });

    socket
        .set_nonblocking(true)
        .map_err(|error| format!("the helper could not be made async: {error}"))?;

    runtime.block_on(async move {
        let socket = tokio::net::TcpStream::from_std(socket)
            .map_err(|error| format!("the helper could not be made async: {error}"))?;
        let (from_parent, mut to_parent) = socket.into_split();

        // One writer, so that the things which report to Cantara — what the
        // operator did, how many are connected, whether this started at all —
        // cannot interleave halfway through a line.
        let (say, mut said) = tokio::sync::mpsc::unbounded_channel::<ToParent>();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            while let Some(message) = said.recv().await {
                let Ok(encoded) = serde_json::to_string(&message) else {
                    continue;
                };
                if to_parent
                    .write_all(format!("{encoded}\n").as_bytes())
                    .await
                    .is_err()
                {
                    return;
                }
            }
        });

        // One server, both services. The stream's routes come from
        // [`crate::logic::stream::server`] exactly as they always have; the
        // console's are handed to it to serve from the same socket.
        //
        // The port asked for, or any free one: a port that is taken is not a
        // reason to leave the service without a console, and the panel is told
        // which port it got either way.
        let server = match StreamServer::start_with(
            configuration.port,
            configuration.offer.viewer.clone(),
            router(Arc::clone(&shared)),
        ) {
            Ok(server) => server,
            Err(first) => {
                log::warn!(
                    "the network server could not take port {}: {first}",
                    configuration.port
                );
                match StreamServer::start_with(
                    0,
                    configuration.offer.viewer.clone(),
                    router(Arc::clone(&shared)),
                ) {
                    Ok(server) => server,
                    Err(error) => {
                        let reason = format!("there is no port to serve on: {error}");
                        let _ = say.send(ToParent::Failed {
                            reason: reason.clone(),
                        });
                        // Long enough for the writer to get it out before the
                        // runtime goes.
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        return Err(reason);
                    }
                }
            }
        };
        let _ = say.send(ToParent::Serving {
            port: server.port(),
        });

        // What the operator does on the console, on its way back to the
        // program.
        {
            let say = say.clone();
            tokio::spawn(async move {
                let Some(mut commands) = remote_console::take_commands() else {
                    return;
                };
                while let Some(command) = commands.recv().await {
                    let message = match command {
                        ConsoleCommand::Update(presentation) => ToParent::Update(presentation),
                        ConsoleCommand::Quit => ToParent::Quit,
                    };
                    if say.send(message).is_err() {
                        return;
                    }
                }
            });
        }

        // How many browsers have a console open, whenever that changes.
        {
            let say = say.clone();
            tokio::spawn(async move {
                let mut last = usize::MAX;
                loop {
                    let now = remote_console::connected();
                    if now != last {
                        last = now;
                        if say.send(ToParent::Connections(now)).is_err() {
                            return;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            });
        }

        // Everything Cantara has to say. Reading this is also how the helper
        // notices that Cantara has gone: the socket ends, and so does the
        // helper — see [`crate::logic::network_host`], which is the only thing
        // that ever stops it deliberately.
        let listening = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;

            let mut server = server;
            let mut shown = Shown::default();
            let mut reader = tokio::io::BufReader::new(from_parent);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    // The socket ended: Cantara has gone.
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }

                match serde_json::from_str::<ToChild>(line.trim()) {
                    Ok(message) => shown.apply(message, &mut server, &shared),
                    Err(error) => log::warn!("Cantara said something unreadable: {error}"),
                }
            }
        });

        let _ = listening.await;
        Ok(())
    })
}

/// What the network side is currently showing, and what it does with each
/// thing Cantara says.
///
/// Held here rather than asked for: a viewer's state is worked out from the
/// presentation and the video's position together, and either of them can
/// arrive without the other.
#[derive(Default)]
struct Shown {
    presentation: Option<RunningPresentation>,
    /// Where the video on the current slide has got to. Sent several times a
    /// second while one is playing and not at all otherwise.
    video: Option<(f64, f64, bool)>,
}

impl Shown {
    fn apply(&mut self, message: ToChild, server: &mut StreamServer, console: &Arc<Shared>) {
        match message {
            ToChild::Presentation(presentation) => {
                self.presentation = *presentation;
                // The console works on the presentation itself; the viewers
                // are shown what is made of it below.
                remote_console::publish(self.presentation.clone());
                self.register_videos(server);
                self.publish(server);
            }

            ToChild::VideoPosition(position) => {
                if self.video == position {
                    return;
                }
                self.video = position;
                self.publish(server);
            }

            ToChild::Media {
                id,
                bytes,
                content_type,
            } => {
                // The content type crosses the socket as a `String` and is
                // served as a `&'static str`; only the two Cantara renders to
                // are accepted, which is also what keeps a header from being
                // whatever a message said it was.
                let content_type = match content_type.as_str() {
                    "image/jpeg" => "image/jpeg",
                    "image/png" => "image/png",
                    other => {
                        log::warn!("a picture arrived as {other}, which is not served");
                        return;
                    }
                };
                server.publish_media(id, bytes, content_type);
            }

            ToChild::Offering(offer) => {
                server.set_viewer(offer.viewer);
                if let Ok(mut password) = console.password.write() {
                    *password = offer.console;
                }
            }
        }
    }

    /// Tells the viewers where things stand.
    fn publish(&self, server: &mut StreamServer) {
        let state = match &self.presentation {
            Some(running) => StreamState::of(running, 0).with_live_video(self.video),
            // Between services. The address stays open and says so.
            None => StreamState::waiting(0),
        };
        server.publish(state);
    }

    /// Says where the videos of this service are, so the server can serve them
    /// from where they lie.
    ///
    /// The paths come out of the presentation itself, which is the same list
    /// Cantara renders pictures from — see
    /// [`crate::logic::stream::protocol::media_sources`]. A video is far too
    /// large to send over the socket and does not need to be: this process can
    /// read the file.
    fn register_videos(&self, server: &StreamServer) {
        let Some(running) = &self.presentation else {
            return;
        };
        let state = StreamState::of(running, 0);
        let sources = crate::logic::stream::protocol::media_sources(std::slice::from_ref(running));

        for id in state.videos() {
            if server.has_video(&id) {
                continue;
            }
            if let Some(path) = sources.get(&id) {
                server.publish_video(id, std::path::PathBuf::from(path));
            }
        }
    }
}

/// What the console's routes share.
struct Shared {
    /// What a browser has to type. Empty means anyone who can reach the
    /// address may drive the presentation, and `None` means the console is
    /// switched off — see [`Offer`].
    password: RwLock<Option<String>>,
    /// Handed out once it has been typed.
    session: String,
}

impl Shared {
    /// Whether a request may drive the presentation.
    fn may_control(&self, headers: &HeaderMap) -> bool {
        let Ok(password) = self.password.read() else {
            // The lock is only ever taken to read a string. If it is poisoned
            // something has already gone very wrong, and the safe answer is no.
            return false;
        };
        let Some(password) = password.as_ref() else {
            // Switched off: there is no console here to drive.
            return false;
        };
        if password.is_empty() {
            return true;
        }
        let password = password.clone();
        drop(password);

        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|cookie| cookie.trim().split_once('='))
            .any(|(name, value)| name == CONSOLE_COOKIE && value == self.session)
    }
}

fn router(shared: Arc<Shared>) -> Router {
    let pool = Arc::new(dioxus_liveview::LiveViewPool::new());

    // No `/` here: the bare address belongs to the stream's own page, which is
    // served from this same socket — see [`crate::logic::stream::server`]. The
    // console is at `/console`, which is what the panel shows. Two handlers
    // for one path is a panic in the server thread, and the helper goes on
    // reporting itself as up while answering nothing.
    Router::new()
        .route("/console", get(page))
        .route("/console/login", post(login))
        .route(
            "/console/ws",
            get(
                move |upgrade: WebSocketUpgrade, headers: HeaderMap, State(shared)| {
                    socket(pool, shared, upgrade, headers)
                },
            ),
        )
        .route("/assets/{*path}", get(asset))
        .route(
            &format!("/{}/{{*path}}", crate::logic::video::VIDEO_HANDLER),
            get(video),
        )
        .with_state(shared)
}

/// The console page: the glue that connects the browser to the socket, or the
/// form that asks for the password first.
async fn page(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    if !shared.may_control(&headers) {
        return (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            LOGIN_PAGE,
        )
            .into_response();
    }

    let glue = dioxus_liveview::interpreter_glue("/console/ws");
    let page = format!(
        r#"<!DOCTYPE html>
<html>
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>Cantara</title>
        <script>{MAP_POLYFILL}</script>
        <style>{PICO_CSS}</style>
        <style>{MAIN_CSS}</style>
        <style>{PRESENTATION_CSS}</style>
        <style>{CONSOLE_CSS}</style>
    </head>
    <body><div id="main"></div></body>
    {glue}
</html>"#
    );

    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response()
}

#[derive(Deserialize)]
struct Password {
    password: String,
}

/// Takes the password and, if it is the right one, hands out a session.
async fn login(State(shared): State<Arc<Shared>>, Json(given): Json<Password>) -> Response {
    let Some(password) = shared.password.read().ok().and_then(|held| held.clone()) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    if !constant_time_eq(given.password.as_bytes(), password.as_bytes()) {
        // After the comparison rather than before it, so that the delay says
        // nothing about the password either.
        tokio::time::sleep(WRONG_PASSWORD_DELAY).await;
        return (StatusCode::UNAUTHORIZED, "wrong password").into_response();
    }

    let cookie = format!(
        "{CONSOLE_COOKIE}={}; Path=/; SameSite=Lax; HttpOnly",
        shared.session
    );
    match HeaderValue::from_str(&cookie) {
        Ok(cookie) => ([(header::SET_COOKIE, cookie)], "welcome").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// One browser, one console.
async fn socket(
    pool: Arc<dioxus_liveview::LiveViewPool>,
    shared: Arc<Shared>,
    upgrade: WebSocketUpgrade,
    headers: HeaderMap,
) -> Response {
    if !shared.may_control(&headers) {
        return locked();
    }

    upgrade.on_upgrade(move |socket| async move {
        // Counted for as long as the socket is open, so the panel with the
        // switch in it can say how many consoles are out there.
        let _connected = remote_console::Connection::open();

        let _ = pool
            .launch_virtualdom(dioxus_liveview::axum_socket(socket), move || {
                dioxus::prelude::VirtualDom::new(
                    crate::components::remote_console::RemoteConsoleRoot,
                )
            })
            .await;
    })
}

/// A file of the program's own: a stylesheet, a font, the PDF viewer.
async fn asset(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(path): Path<String>,
) -> Response {
    if !shared.may_control(&headers) {
        return locked();
    }

    match dioxus::asset_resolver::native::serve_asset(&format!("/assets/{path}")) {
        Ok(response) => {
            let (parts, body) = response.into_parts();
            (parts.status, parts.headers, body).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// A video of the running service.
///
/// The same answer the window gets, from the same function — see
/// [`crate::logic::video::answer_video_request`], where the rules about ranges
/// and about which files may be served at all are written down.
async fn video(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(path): Path<String>,
) -> Response {
    if !shared.may_control(&headers) {
        return locked();
    }

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let answer = crate::logic::video::answer_video_request(
        &format!("/{}/{}", crate::logic::video::VIDEO_HANDLER, path),
        range,
    );

    let mut response = Response::builder().status(answer.status);
    for (name, value) in &answer.headers {
        response = response.header(*name, value);
    }
    match response.body(axum::body::Body::from(answer.body)) {
        Ok(response) => response,
        Err(error) => {
            log::error!("a video could not be answered: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn locked() -> Response {
    (StatusCode::UNAUTHORIZED, "a password is needed").into_response()
}

/// Compares two secrets without giving away where they first differ.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |differences, (a, b)| differences | (a ^ b))
        == 0
}

/// A token nobody can guess, for the session cookie.
fn new_session_token() -> String {
    uuid::Uuid::new_v4().as_simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared_with(password: &str) -> Shared {
        Shared {
            password: RwLock::new(Some(password.to_string())),
            session: "c0nsole-session".to_string(),
        }
    }

    /// A console that is switched off is not there for anybody, cookie or no
    /// cookie — the stream may still be being served from the same socket.
    fn shared_switched_off() -> Shared {
        Shared {
            password: RwLock::new(None),
            session: "c0nsole-session".to_string(),
        }
    }

    fn cookies(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        match HeaderValue::from_str(value) {
            Ok(value) => {
                headers.insert(header::COOKIE, value);
            }
            Err(error) => panic!("a cookie header that cannot be built: {error}"),
        }
        headers
    }

    /// An empty password is the operator's decision, and it means what it
    /// says: anyone who can reach the address may drive the presentation.
    #[test]
    fn without_a_password_anyone_who_reaches_it_may_drive() {
        let shared = shared_with("");

        assert!(shared.may_control(&HeaderMap::new()));
        assert!(shared.may_control(&cookies("something=else")));
    }

    /// With one set, nothing is answered until it has been given.
    #[test]
    fn with_a_password_a_session_is_needed() {
        let shared = shared_with("control");

        assert!(!shared.may_control(&HeaderMap::new()));
        assert!(!shared.may_control(&cookies("cantara_console=guessed")));
        assert!(shared.may_control(&cookies("cantara_console=c0nsole-session")));
    }

    /// A browser sends every cookie it has for the address, so the right one
    /// has to be found among the others rather than assumed to be alone.
    #[test]
    fn the_session_is_found_among_other_cookies() {
        let shared = shared_with("control");

        assert!(shared.may_control(&cookies("theme=dark; cantara_console=c0nsole-session; a=b")));
        assert!(!shared.may_control(&cookies("cantara_console_x=c0nsole-session")));
    }

    /// The viewer's stream cookie is a different cookie, and must not open
    /// this.
    #[test]
    fn a_viewer_session_is_not_a_console_session() {
        let shared = shared_with("control");

        assert!(!shared.may_control(&cookies("cantara_stream=c0nsole-session")));
    }

    #[test]
    fn a_console_that_is_switched_off_answers_nobody() {
        let shared = shared_switched_off();

        assert!(!shared.may_control(&HeaderMap::new()));
        assert!(!shared.may_control(&cookies("cantara_console=c0nsole-session")));
    }

    #[test]
    fn passwords_are_compared_without_leaking_them() {
        assert!(constant_time_eq(b"open sesame", b"open sesame"));
        assert!(!constant_time_eq(b"open sesame", b"open sesamf"));
        assert!(!constant_time_eq(b"open", b"open sesame"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn two_sessions_are_never_the_same() {
        assert_ne!(new_session_token(), new_session_token());
    }
}
