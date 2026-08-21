//! The HTTP server that offers the presentation to the network.
//!
//! It runs on a Tokio runtime of its own, on a thread of its own. The window
//! must not be able to stall it and it must not be able to stall the window —
//! a viewer whose phone went to sleep mid-song is not a reason for a slide
//! change to wait, and a slow slide change is not a reason for a viewer to be
//! dropped.
//!
//! What passes between the two is one [`watch`] channel carrying the current
//! [`StreamState`], and one map of pictures. The program writes; every viewer
//! reads. Nothing is polled: a viewer's connection sleeps until the state it
//! is watching changes, which is what makes a slide change appear on a phone
//! at the same moment it appears on the wall.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, RwLock};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::sync::watch;

use super::protocol::StreamState;

/// The page a viewer is served. Self-contained on purpose: a phone on a church
/// network should need nothing but the one address.
const VIEWER_PAGE: &str = include_str!("../../../assets/stream_viewer.html");

/// The engraver, for the slides that carry a staff.
///
/// Served rather than inlined — half a megabyte of library in front of the
/// first slide would be paid for by every viewer of every service, and most
/// services have no notation in them at all. Served *by Cantara* rather than
/// fetched from a CDN, because the network this runs on frequently has no way
/// out: the same reason the presentation window loads it from `node_modules`.
const ABCJS_LIB: &[u8] = include_bytes!("../../../node_modules/abcjs/dist/abcjs-basic-min.js");

/// The name of the cookie a viewer is given once they have proved they know
/// the password.
const SESSION_COOKIE: &str = "cantara_stream";

/// A picture a slide refers to, ready to be served.
#[derive(Clone)]
pub struct Media {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    /// What these particular bytes are, so a viewer can be told "still the
    /// same" without them being sent again — and, more importantly, told that
    /// they are *not* the same when the picture behind them has changed.
    pub etag: String,
}

/// What the server and the program both reach into.
struct Shared {
    /// The current state. Viewers wait on this rather than asking for it.
    state: watch::Sender<Arc<StreamState>>,
    /// The pictures the running order refers to, by the name the state gives
    /// them.
    media: RwLock<HashMap<String, Media>>,
    /// Where the videos are, by the same kind of id the pictures use.
    ///
    /// A path rather than the bytes, which every other kind of media is held
    /// as: a service video is tens or hundreds of megabytes, holding it in
    /// memory would cost that much for as long as the presentation runs, and
    /// it is served in pieces anyway — a browser asks for the part it is about
    /// to play, so the whole file is never wanted at once. See
    /// [`crate::logic::video::parse_byte_range`].
    videos: RwLock<HashMap<String, std::path::PathBuf>>,
    /// What a viewer has to know. Empty means anyone may watch.
    password: String,
    /// Handed out when the password is given correctly.
    session: String,
    /// Goes true once, when the server should stop.
    ///
    /// The accept loop is not the only thing that has to hear about this. A
    /// viewer's event stream never ends by itself — that is what makes it a
    /// live stream — so a graceful shutdown that only stops accepting waits
    /// for every open browser tab, which from the presenter's side looks
    /// exactly like the program hanging.
    stopping: watch::Sender<bool>,
}

impl Shared {
    /// Whether a request may be answered.
    ///
    /// With no password there is nothing to prove, which is the ordinary case
    /// in a hall: a congregation should not have to log in to read the words.
    fn may_watch(&self, headers: &HeaderMap) -> bool {
        if self.password.is_empty() {
            return true;
        }
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|cookie| cookie.trim().split_once('='))
            .any(|(name, value)| name == SESSION_COOKIE && value == self.session)
    }
}

/// A running server. Dropping it stops it.
pub struct StreamServer {
    shared: Arc<Shared>,
    /// The thread the runtime lives on, so that stopping can be waited for.
    thread: Option<std::thread::JoinHandle<()>>,
    /// Counts the states published, so a viewer can tell one from the next.
    revision: u64,
    port: u16,
}

impl StreamServer {
    /// Puts the server up on `port`.
    ///
    /// Returns what went wrong rather than panicking: the port may well be
    /// taken, and that is something to tell the user about, not to fall over
    /// on in the middle of preparing a service.
    pub fn start(port: u16, password: String) -> Result<StreamServer, String> {
        let shared = Arc::new(Shared {
            state: watch::channel(Arc::new(StreamState::waiting(0))).0,
            media: RwLock::new(HashMap::new()),
            videos: RwLock::new(HashMap::new()),
            password,
            session: new_session_token(),
            stopping: watch::channel(false).0,
        });

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u16, String>>();

        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("cantara-stream".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(format!("no runtime for the server: {error}")));
                        return;
                    }
                };

                runtime.block_on(async move {
                    let address = SocketAddr::new(bind_address(), port);
                    let listener = match tokio::net::TcpListener::bind(address).await {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = ready_tx.send(Err(describe_bind_failure(port, &error)));
                            return;
                        }
                    };
                    // The port that was actually taken. A `port` of 0 asks
                    // the operating system for any free one, and the answer is
                    // what a user has to be shown.
                    let bound = match listener.local_addr() {
                        Ok(address) => address.port(),
                        Err(error) => {
                            let _ = ready_tx.send(Err(format!("the server has no address: {error}")));
                            return;
                        }
                    };
                    let _ = ready_tx.send(Ok(bound));

                    // Politely first: stop accepting, let what is in flight
                    // finish.
                    let graceful = {
                        let mut stopping = thread_shared.stopping.subscribe();
                        async move {
                            let _ = stopping.wait_for(|stopping| *stopping).await;
                        }
                    };
                    // And then, shortly after, regardless. A viewer who leaves
                    // a tab open must not be able to hold the program still,
                    // and the alternative to a deadline here is a switch that
                    // sometimes never comes back.
                    let deadline = {
                        let mut stopping = thread_shared.stopping.subscribe();
                        async move {
                            let _ = stopping.wait_for(|stopping| *stopping).await;
                            tokio::time::sleep(STOP_DEADLINE).await;
                        }
                    };

                    let app = router(thread_shared);
                    tokio::select! {
                        served = axum::serve(listener, app)
                            .with_graceful_shutdown(graceful) => {
                            if let Err(error) = served {
                                log::error!("the streaming server stopped: {error}");
                            }
                        }
                        _ = deadline => {
                            log::warn!(
                                "a viewer was still connected when streaming was                                  switched off; stopping without waiting for it"
                            );
                        }
                    }
                });
            })
            .map_err(|error| format!("no thread for the server: {error}"))?;

        // Wait for the socket, so that a port already in use is reported here
        // rather than as a viewer failing to connect later.
        let port = match ready_rx.recv() {
            Ok(Ok(bound)) => bound,
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(error);
            }
            Err(_) => {
                let _ = thread.join();
                return Err("the streaming server stopped before it started".to_string());
            }
        };

        Ok(StreamServer {
            shared,
            thread: Some(thread),
            revision: 0,
            port,
        })
    }

    /// The port the server actually took, which is not the one that was asked
    /// for when that was 0.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The address to type into a phone.
    pub fn address(&self) -> String {
        format!("http://{}:{}", local_address(), self.port)
    }

    /// Tells every viewer where the presentation now stands.
    ///
    /// Cheap and safe to call on every change: a viewer that is asleep misses
    /// the intermediate states and is woken with the latest, which is the only
    /// one that was ever worth having.
    pub fn publish(&mut self, mut state: StreamState) {
        self.revision += 1;
        state.revision = self.revision;
        // `send_replace` rather than `send`: `send` reports failure when there
        // are no viewers *and leaves the value alone*, so everything published
        // before the first phone opened the address would be thrown away and
        // that viewer would arrive to a presentation that had not started.
        // Nobody listening is the ordinary state of affairs here, not an
        // error.
        self.shared.state.send_replace(Arc::new(state));
    }

    /// Hands over a picture a slide refers to, under the name the state gives
    /// it.
    pub fn publish_media(&self, id: String, bytes: Vec<u8>, content_type: &'static str) {
        let etag = format!("\"{:x}\"", md5::compute(&bytes));
        if let Ok(mut media) = self.shared.media.write() {
            media.insert(
                id,
                Media {
                    bytes,
                    content_type,
                    etag,
                },
            );
        }
    }

    /// Whether a picture has already been handed over, so the program does not
    /// render one twice.
    pub fn has_media(&self, id: &str) -> bool {
        self.shared
            .media
            .read()
            .map(|media| media.contains_key(id))
            .unwrap_or(false)
    }
}

impl Drop for StreamServer {
    fn drop(&mut self) {
        // `send_replace` rather than `send`: a `watch` sender reports failure
        // and throws the value away when nobody is listening, and whether the
        // runtime happens to be subscribed yet is not something stopping
        // should depend on.
        self.shared.stopping.send_replace(true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/state", get(state))
        .route("/events", get(events))
        .route("/abcjs.js", get(abcjs))
        .route("/media/{id}", get(media))
        .route("/video/{id}", get(video))
        .route("/login", post(login))
        .with_state(shared)
}

async fn page() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        VIEWER_PAGE,
    )
}

/// The engraver, asked for the first time a slide carries a staff.
///
/// Not behind the password: it is a public library, the same bytes anyone can
/// download from npm, and nothing about the service is in it. Behind the
/// password it would also have to be fetched again by every viewer who has not
/// logged in yet, which is the one case where the half megabyte hurts.
async fn abcjs() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            // The bytes only change when Cantara does, and a viewer's browser
            // should not fetch half a megabyte once per slide.
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        ABCJS_LIB,
    )
}

/// The state as it stands. What the page asks for once, on opening.
async fn state(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    if !shared.may_watch(&headers) {
        return locked();
    }
    Json(&*shared.state.borrow().clone()).into_response()
}

/// The state, and every state after it, as server-sent events.
///
/// One long-lived response per viewer. It sleeps on the channel and writes
/// only when something has actually changed, so a hundred phones cost a
/// hundred sleeping tasks rather than a hundred requests a second.
async fn events(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures_util::{StreamExt, stream};

    if !shared.may_watch(&headers) {
        return locked();
    }

    let receiver = shared.state.subscribe();

    // The first thing a viewer gets is where things stand *now*; after that,
    // every change as it happens.
    //
    // The order matters and is the whole of it: waiting for a change before
    // sending anything means a page that connects between two slides sits on
    // its loading placeholder until the next one — which, in the middle of a
    // reading, is minutes. So the state is yielded first and the wait comes
    // after.
    // Ends the stream when the server is stopping, so that closing down does
    // not have to wait for a viewer to close their tab.
    let stop = {
        let mut stopping = shared.stopping.subscribe();
        async move {
            let _ = stopping.wait_for(|stopping| *stopping).await;
        }
    };

    let updates = stream::unfold((receiver, true), |(mut receiver, first)| async move {
        if !first && receiver.changed().await.is_err() {
            // The program has gone; the stream ends and the page reconnects.
            return None;
        }
        let state = receiver.borrow_and_update().clone();
        let event = Event::default().json_data(&*state).unwrap_or_default();
        Some((
            Ok::<Event, std::convert::Infallible>(event),
            (receiver, false),
        ))
    })
    .take_until(Box::pin(stop));

    Sse::new(updates)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Says where the video with this id is, so it can be served from there.
///
/// The path is kept rather than the bytes; see [`Shared::videos`].
impl StreamServer {
    pub fn publish_video(&self, id: String, path: std::path::PathBuf) {
        if let Ok(mut videos) = self.shared.videos.write() {
            videos.insert(id, path);
        }
    }

    /// Whether a video of that id has been published.
    pub fn has_video(&self, id: &str) -> bool {
        self.shared
            .videos
            .read()
            .map(|videos| videos.contains_key(id))
            .unwrap_or(false)
    }
}

/// Serves a video, in whatever piece of it the browser asked for.
///
/// Range requests are the whole point: without them a phone cannot seek, and
/// it will not begin playing until the entire file has arrived. The same
/// parsing as the desktop's own handler, so the two cannot drift apart —
/// see [`crate::logic::video::parse_byte_range`].
async fn video(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    use crate::logic::video::{ByteRange, parse_byte_range};
    use std::io::{Read, Seek, SeekFrom};

    if !shared.may_watch(&headers) {
        return locked();
    }

    let found = shared
        .videos
        .read()
        .ok()
        .and_then(|videos| videos.get(&id).cloned());

    let Some(path) = found else {
        return (StatusCode::NOT_FOUND, "no such video").into_response();
    };

    // Only something that is a video by its name, exactly as the desktop's own
    // handler insists. Both are reached with an id the program registered, so
    // neither *should* ever be pointed at anything else — but the paths come
    // from a running order, and a running order can be imported from a file
    // somebody else wrote. This route is reachable from the network, so the
    // consequence of being wrong here is worse: every file the user running
    // Cantara can read, handed to anyone who can open the stream.
    if crate::logic::sourcefiles::SourceFileType::of(
        &path.file_name().unwrap_or_default().to_string_lossy(),
    ) != Some(crate::logic::sourcefiles::SourceFileType::Video)
    {
        return (StatusCode::FORBIDDEN, "not a video").into_response();
    }

    let Ok(mut file) = std::fs::File::open(&path) else {
        return (StatusCode::NOT_FOUND, "the video is no longer there").into_response();
    };
    let Ok(metadata) = file.metadata() else {
        return (StatusCode::NOT_FOUND, "the video cannot be read").into_response();
    };
    let length = metadata.len();

    let content_type = crate::logic::sourcefiles::mime_type_of_video(
        &path.file_name().unwrap_or_default().to_string_lossy(),
    );

    let asked = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let (wanted, partial) = match &asked {
        Some(header) => match parse_byte_range(header, length) {
            Some(wanted) => (wanted, true),
            None => {
                // Refused rather than answered with the beginning of the file:
                // a player that asked to start ten minutes in and silently got
                // the opening would play the wrong thing.
                return (
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    [(header::CONTENT_RANGE, format!("bytes */{length}"))],
                )
                    .into_response();
            }
        },
        None => match ByteRange::whole(length) {
            Some(whole) => (whole, false),
            None => return (StatusCode::NOT_FOUND, "the video is empty").into_response(),
        },
    };

    let mut body = vec![0u8; wanted.length() as usize];
    if file.seek(SeekFrom::Start(wanted.start)).is_err()
        || file.read_exact(&mut body).is_err()
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, "the video could not be read")
            .into_response();
    }

    let status = if partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };

    let mut response = (status, body).into_response();
    let response_headers = response.headers_mut();
    if let Ok(value) = header::HeaderValue::from_str(content_type) {
        response_headers.insert(header::CONTENT_TYPE, value);
    }
    // Without this a browser will not seek at all: it reads the absence of the
    // header as "the whole file or nothing".
    response_headers.insert(header::ACCEPT_RANGES, header::HeaderValue::from_static("bytes"));
    if partial
        && let Ok(value) = header::HeaderValue::from_str(&format!(
            "bytes {}-{}/{}",
            wanted.start, wanted.end, length
        ))
    {
        response_headers.insert(header::CONTENT_RANGE, value);
    }

    response
}

async fn media(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !shared.may_watch(&headers) {
        return locked();
    }
    let found = shared
        .media
        .read()
        .ok()
        .and_then(|media| media.get(&id).cloned());

    let Some(picture) = found else {
        return (StatusCode::NOT_FOUND, "no such picture").into_response();
    };

    // A picture is named after where it came from, not after what is in it —
    // so the same name can come to mean different bytes, when someone edits
    // their background between two services. Telling a browser to keep it for
    // good, as this used to, meant it never saw the new one. It is kept, but
    // checked: an unchanged picture costs one small answer, and a changed one
    // is fetched again.
    let unchanged = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|given| given.split(',').any(|tag| tag.trim() == picture.etag));

    let Ok(etag) = HeaderValue::from_str(&picture.etag) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "bad picture name").into_response();
    };
    let caching = [
        (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        (header::ETAG, etag),
    ];

    if unchanged {
        return (StatusCode::NOT_MODIFIED, caching).into_response();
    }
    (
        caching,
        [(header::CONTENT_TYPE, picture.content_type)],
        picture.bytes,
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct Login {
    password: String,
}

/// Takes a password and, if it is the right one, hands out a session.
async fn login(State(shared): State<Arc<Shared>>, Json(given): Json<Login>) -> Response {
    if !constant_time_eq(given.password.as_bytes(), shared.password.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "wrong password").into_response();
    }

    let cookie = session_cookie(&shared.session);
    match HeaderValue::from_str(&cookie) {
        Ok(cookie) => ([(header::SET_COOKIE, cookie)], "welcome").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn locked() -> Response {
    (StatusCode::UNAUTHORIZED, "a password is needed").into_response()
}

/// Compares two secrets without giving away where they first differ.
///
/// Overkill for a password on a church network, and cheap enough that there is
/// no reason to write the version that leaks.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |differences, (a, b)| differences | (a ^ b))
        == 0
}

/// The cookie that says a viewer has given the password.
///
/// `HttpOnly` because nothing on the viewer's page has any business reading
/// it: the cookie is set by the server and sent back by the browser, and no
/// script is part of that. It costs nothing and takes the cookie out of reach
/// of anything that might one day manage to run on the page.
fn session_cookie(session: &str) -> String {
    format!("{SESSION_COOKIE}={session}; Path=/; SameSite=Lax; HttpOnly")
}

/// How long a viewer's connection is given to finish after streaming has been
/// switched off, before it is cut.
///
/// Long enough that an ordinary request in flight completes, short enough that
/// nobody reads it as the program having hung.
const STOP_DEADLINE: std::time::Duration = std::time::Duration::from_secs(3);

/// The address the server listens on: every interface this machine has.
///
/// Deliberately *not* loopback. Loopback would work perfectly on the machine
/// running Cantara and be invisible to every other device on the network,
/// which is the entire point of the feature — so the difference is worth a
/// name and a test rather than four digits buried in a call.
fn bind_address() -> IpAddr {
    IpAddr::V4(Ipv4Addr::UNSPECIFIED)
}

/// A token nobody can guess, for the session cookie.
fn new_session_token() -> String {
    uuid::Uuid::new_v4().as_simple().to_string()
}

/// Says why a port could not be taken, in words that suggest what to do.
fn describe_bind_failure(port: u16, error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::AddrInUse => {
            format!("port {port} is already in use — choose another one in the settings")
        }
        std::io::ErrorKind::PermissionDenied => {
            format!("port {port} may not be used — choose one above 1024 in the settings")
        }
        _ => format!("the streaming server could not use port {port}: {error}"),
    }
}

/// This machine's address on the network it is actually attached to.
///
/// Two ways of asking, because neither is enough on its own.
///
/// The routing table is asked first: which interface *would* be used to reach
/// the outside world. No packet is sent — a UDP socket that has never been
/// written to sends nothing — and no name server is consulted. That is the
/// right answer on an ordinary network, and it picks the sensible interface on
/// a machine that has several.
///
/// But a hall network with no route to the internet answers "nowhere", and the
/// fallback was then `localhost` — an address that works on the machine
/// showing it and nowhere else, which is precisely the network a church is
/// most likely to be on. So the interfaces are then listed and a private
/// address taken from them.
fn local_address() -> IpAddr {
    if let Some(routed) = address_by_route() {
        return routed;
    }
    if let Ok(addresses) = local_ip_address::list_afinet_netifas() {
        let candidates: Vec<IpAddr> = addresses.into_iter().map(|(_name, ip)| ip).collect();
        if let Some(found) = best_address(&candidates) {
            return found;
        }
    }
    // No network at all. `localhost` is at least true, and says plainly that
    // nobody else is going to reach it.
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

/// Which address the operating system would use to reach the outside world.
fn address_by_route() -> Option<IpAddr> {
    let address = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            // Reserved for documentation, so it is never a real host, and no
            // packet is sent to it either way.
            socket.connect((Ipv4Addr::new(203, 0, 113, 1), 80))?;
            socket.local_addr()
        })
        .ok()?
        .ip();

    (!address.is_unspecified() && !address.is_loopback()).then_some(address)
}

/// The address most likely to be the one another device on the same network
/// can reach.
///
/// A private IPv4 address is preferred above everything: that is what a home
/// or church network hands out, and it is what a phone on the same wi-fi will
/// be able to reach. Anything else is a guess, and loopback is not an answer
/// at all — a page that can only be opened on the machine already showing the
/// presentation is no use to anyone.
fn best_address(candidates: &[IpAddr]) -> Option<IpAddr> {
    let usable = |address: &&IpAddr| !address.is_loopback() && !address.is_unspecified();

    candidates
        .iter()
        .filter(usable)
        .find(|address| matches!(address, IpAddr::V4(v4) if v4.is_private()))
        .or_else(|| candidates.iter().filter(usable).find(|address| address.is_ipv4()))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared_with(password: &str) -> Shared {
        Shared {
            state: watch::channel(Arc::new(StreamState::waiting(0))).0,
            media: RwLock::new(HashMap::new()),
            videos: RwLock::new(HashMap::new()),
            password: password.to_string(),
            session: "s3cret-session".to_string(),
            stopping: watch::channel(false).0,
        }
    }

    fn cookies(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(value).unwrap());
        headers
    }

    /// No password is the ordinary case: a hall full of people should not have
    /// to log in to read the words.
    #[test]
    fn without_a_password_anyone_may_watch() {
        let shared = shared_with("");

        assert!(shared.may_watch(&HeaderMap::new()));
        assert!(shared.may_watch(&cookies("something=else")));
    }

    /// With one set, nothing is answered until it has been given.
    #[test]
    fn with_a_password_a_session_is_needed() {
        let shared = shared_with("open sesame");

        assert!(!shared.may_watch(&HeaderMap::new()));
        assert!(!shared.may_watch(&cookies("cantara_stream=guessed")));
        assert!(shared.may_watch(&cookies("cantara_stream=s3cret-session")));
    }

    /// A browser sends every cookie it has for the address, so the right one
    /// has to be found among the others rather than assumed to be alone.
    #[test]
    fn the_session_is_found_among_other_cookies() {
        let shared = shared_with("open sesame");

        assert!(shared.may_watch(&cookies("theme=dark; cantara_stream=s3cret-session; a=b")));
        assert!(!shared.may_watch(&cookies("theme=dark; cantara_stream_x=s3cret-session")));
    }

    /// Comparing secrets must not give away where they first differ.
    #[test]
    fn passwords_are_compared_without_leaking_them() {
        assert!(constant_time_eq(b"open sesame", b"open sesame"));
        assert!(!constant_time_eq(b"open sesame", b"open sesamf"));
        assert!(!constant_time_eq(b"open", b"open sesame"));
        assert!(constant_time_eq(b"", b""));
    }

    /// Two sessions must never collide, or one viewer's cookie would let
    /// another in.
    #[test]
    fn every_session_token_is_its_own() {
        let tokens: Vec<String> = (0..64).map(|_| new_session_token()).collect();
        let mut unique = tokens.clone();
        unique.sort();
        unique.dedup();

        assert_eq!(unique.len(), tokens.len());
        assert!(tokens.iter().all(|token| token.len() >= 32));
    }

    /// A port that is taken is the likeliest thing to go wrong, and the
    /// message has to say what to do about it.
    #[test]
    fn a_taken_port_says_so() {
        let taken = std::io::Error::new(std::io::ErrorKind::AddrInUse, "taken");

        let message = describe_bind_failure(8420, &taken);

        assert!(message.contains("8420"), "got: {message}");
        assert!(message.contains("already in use"), "got: {message}");
        assert!(message.contains("settings"), "it says where to change it");
    }

    /// The address is what a user types into a phone, so it has to be one.
    #[test]
    fn the_address_is_reachable_as_written() {
        let address = local_address();

        assert!(!address.is_unspecified(), "0.0.0.0 is not an address to type");
        assert!(!address.is_multicast());
    }

    // ── The server as a viewer meets it ─────────────────────────────────────
    //
    // These start a real server on a port the operating system picks and talk
    // to it over HTTP. Everything above tests a decision; these test that the
    // decisions are wired to a socket.

    /// Starts a server on any free port. Panics rather than returns, because a
    /// test that cannot get a socket has nothing left to say.
    fn serving(password: &str) -> StreamServer {
        StreamServer::start(0, password.to_string()).expect("a server on a free port")
    }

    fn client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("a client")
    }

    /// The cookie a response hands out, as a browser would send it back.
    ///
    /// Carried by hand rather than by a cookie jar, so what is checked is the
    /// header a browser is actually given.
    fn cookie_from(response: &reqwest::blocking::Response) -> String {
        response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .expect("a session was handed out")
            .to_string()
    }

    fn at(server: &StreamServer, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", server.port(), path)
    }

    /// Opening the address gives a page, not a file listing or an error.
    #[test]
    fn the_address_serves_a_page() {
        let server = serving("");

        let response = client().get(at(&server, "/")).send().expect("answers");

        assert!(response.status().is_success());
        assert!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .starts_with("text/html"),
            "a browser is told it is HTML"
        );
    }

    /// A viewer who opens the address before anything begins is told so,
    /// rather than being met with an error — the whole point of the server
    /// outliving the presentation.
    #[test]
    fn a_viewer_before_the_presentation_is_told_to_wait() {
        let server = serving("");

        let state: StreamState = client()
            .get(at(&server, "/state"))
            .send()
            .expect("answers")
            .json()
            .expect("with a state");

        assert!(!state.running);
        assert!(state.chapters.is_empty());
    }

    /// What the program publishes is what a viewer reads, and every change is
    /// numbered so a page can tell one from the next.
    #[test]
    fn what_is_published_is_what_is_served() {
        let mut server = serving("");
        server.publish(StreamState {
            running: true,
            chapters: vec![super::super::protocol::StreamChapter {
                title: "Amazing Grace".to_string(),
                slides: vec![],
            }],
            ..StreamState::default()
        });

        let first: StreamState = client()
            .get(at(&server, "/state"))
            .send()
            .expect("answers")
            .json()
            .expect("with a state");
        assert!(first.running);
        assert_eq!(first.chapters[0].title, "Amazing Grace");
        assert_eq!(first.revision, 1, "the first change is the first revision");

        server.publish(StreamState {
            running: true,
            ..StreamState::default()
        });
        let second: StreamState = client()
            .get(at(&server, "/state"))
            .send()
            .expect("answers")
            .json()
            .expect("with a state");
        assert_eq!(second.revision, 2, "every change counts up");
    }

    /// A video is served from where it is, in whatever piece the browser asked
    /// for. Range requests are the whole point: without them a phone will not
    /// begin playing until the entire file has arrived, and cannot seek at all.
    #[test]
    fn a_video_is_served_in_the_pieces_a_browser_asks_for() {
        let server = serving("");
        let folder = tempfile::tempdir().expect("a temporary folder");
        let path = folder.path().join("clip.mp4");
        let contents: Vec<u8> = (0..=255u8).collect();
        std::fs::write(&path, &contents).expect("the file can be written");

        assert!(!server.has_video("a-clip"));
        let missing = client().get(at(&server, "/video/a-clip")).send().expect("answers");
        assert_eq!(missing.status(), 404, "before it is registered");

        server.publish_video("a-clip".to_string(), path);
        assert!(server.has_video("a-clip"));

        // The whole thing, and the header without which no browser will seek.
        let whole = client().get(at(&server, "/video/a-clip")).send().expect("answers");
        assert_eq!(whole.status(), 200);
        assert_eq!(
            whole.headers().get("accept-ranges").and_then(|v| v.to_str().ok()),
            Some("bytes"),
            "without this a browser takes it as the whole file or nothing"
        );
        assert_eq!(whole.bytes().expect("bytes").as_ref(), contents.as_slice());

        // A piece out of the middle, which is what a seek is.
        let piece = client()
            .get(at(&server, "/video/a-clip"))
            .header("Range", "bytes=10-19")
            .send()
            .expect("answers");
        assert_eq!(piece.status(), 206);
        assert_eq!(
            piece.headers().get("content-range").and_then(|v| v.to_str().ok()),
            Some("bytes 10-19/256")
        );
        assert_eq!(piece.bytes().expect("bytes").as_ref(), &contents[10..=19]);

        // And a range that cannot be met is refused rather than answered with
        // the beginning of the file.
        let past_the_end = client()
            .get(at(&server, "/video/a-clip"))
            .header("Range", "bytes=9999-")
            .send()
            .expect("answers");
        assert_eq!(past_the_end.status(), 416);
    }

    /// The paths this route serves come from a running order, and a running
    /// order can be imported from a file somebody else wrote. The route is
    /// reachable from the network, so anything that is not a video by its name
    /// is refused however it came to be registered.
    #[test]
    fn only_a_video_is_served_to_the_network() {
        let server = serving("");
        let folder = tempfile::tempdir().expect("a temporary folder");
        let secret = folder.path().join("settings.json");
        std::fs::write(&secret, b"a token nobody should be handed").expect("written");

        server.publish_video("not-a-video".to_string(), secret);

        let refused = client()
            .get(at(&server, "/video/not-a-video"))
            .send()
            .expect("answers");

        assert_eq!(refused.status(), 403);
        assert!(
            !refused.text().unwrap_or_default().contains("token"),
            "the file was served anyway"
        );
    }

    /// A picture is fetched from the server rather than pushed with every
    /// update, so it has to be there once it has been handed over — and not
    /// before.
    #[test]
    fn a_picture_is_served_once_it_has_been_handed_over() {
        let server = serving("");
        let id = "a-page";

        assert!(!server.has_media(id));
        let missing = client().get(at(&server, "/media/a-page")).send().expect("answers");
        assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

        server.publish_media(id.to_string(), b"not really a png".to_vec(), "image/png");

        assert!(server.has_media(id));
        let found = client().get(at(&server, "/media/a-page")).send().expect("answers");
        assert!(found.status().is_success());
        assert_eq!(
            found.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/png"
        );
        assert_eq!(found.bytes().expect("bytes"), b"not really a png".as_slice());
    }

    /// A slide with a staff on it needs the engraver, and the network Cantara
    /// runs on frequently has no way out to a CDN — so it comes from here.
    #[test]
    fn the_engraver_is_served_by_cantara_itself() {
        let server = serving("");

        let response = client().get(at(&server, "/abcjs.js")).send().expect("answers");

        assert!(response.status().is_success());
        assert!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .contains("javascript"),
            "a browser is told it is a script"
        );
        let body = response.text().expect("a script");
        assert!(body.contains("ABCJS"), "and it is the engraver");
    }

    /// The engraver is a public library and nothing about the service is in it,
    /// so it is not behind the password — which also spares a viewer who has
    /// not logged in yet from fetching half a megabyte twice.
    #[test]
    fn the_engraver_is_not_behind_the_password() {
        let server = serving("open sesame");

        let response = client().get(at(&server, "/abcjs.js")).send().expect("answers");

        assert!(response.status().is_success());
    }

    /// The whole path, from a song on disk to what a browser is handed: a
    /// service that divides the song two lines at a time on the wall and four
    /// on the phones, published, and fetched back over HTTP.
    ///
    /// Everything below this is tested in pieces elsewhere. This is the one
    /// that would catch the pieces being wired together wrongly — a viewer sent
    /// the projection's slides with the stream's position, say, which each half
    /// would pass on its own.
    #[test]
    fn a_viewer_is_served_the_streams_own_division_of_the_song() {
        use crate::logic::presentation::build_presentation;
        use crate::logic::settings::PresentationDesign;
        use crate::logic::sourcefiles::{SourceFile, SourceFileType};
        use crate::logic::states::SelectedItemRepresentation;
        use crate::logic::stream_view::StreamDefaults;
        use cantara_songlib::slides::SlideSettings;

        let item = SelectedItemRepresentation::new_with_sourcefile(SourceFile {
            name: "Amazing Grace".to_string(),
            path: std::path::PathBuf::from("testfiles/Amazing Grace.song"),
            file_type: SourceFileType::Song,
            md5_hash: None,
            relative_path: None,
        });

        let mut presentation = build_presentation(
            &vec![item],
            &PresentationDesign::default(),
            &SlideSettings {
                max_lines: Some(2),
                ..SlideSettings::default()
            },
            &StreamDefaults {
                design: None,
                slide_settings: Some(SlideSettings {
                    max_lines: Some(4),
                    ..SlideSettings::default()
                }),
            },
                    &[],
)
        .expect("a presentation");

        // Two slide changes on the wall, which is one on the phones.
        presentation.next_slide();
        presentation.next_slide();
        let projection_slide = presentation
            .position
            .as_ref()
            .expect("a position")
            .chapter_slide();
        let (_, stream_slide) = presentation.stream_position().expect("a mapped position");

        let mut server = serving("");
        server.publish(StreamState::of(&presentation, 0));

        let served: StreamState = client()
            .get(at(&server, "/state"))
            .send()
            .expect("answers")
            .json()
            .expect("with a state");

        let projection_count = presentation.presentation[0].slides.len();
        assert!(
            served.chapters[0].slides.len() < projection_count,
            "the phones were sent their own, shorter division: {} against {projection_count}",
            served.chapters[0].slides.len()
        );
        assert_eq!(
            served.position.expect("a position").slide,
            stream_slide,
            "and the position that came with it is the mapped one"
        );
        assert!(
            stream_slide < projection_slide,
            "which on the third slide of the wall is not the wall's own number: \
             {stream_slide} against {projection_slide}"
        );
        assert!(
            served.current_slide().is_some(),
            "and it points at a slide that exists"
        );
    }

    /// With a password set, nothing is given away until it has been typed.
    #[test]
    fn a_password_is_asked_for_and_then_remembered() {
        let server = serving("open sesame");
        let client = client();

        let refused = client.get(at(&server, "/state")).send().expect("answers");
        assert_eq!(refused.status(), reqwest::StatusCode::UNAUTHORIZED);

        let wrong = client
            .post(at(&server, "/login"))
            .json(&serde_json::json!({ "password": "guessed" }))
            .send()
            .expect("answers");
        assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);

        let right = client
            .post(at(&server, "/login"))
            .json(&serde_json::json!({ "password": "open sesame" }))
            .send()
            .expect("answers");
        assert!(right.status().is_success());

        // The cookie the login handed out is enough from here on: a viewer
        // types the password once, not once per slide.
        let session = cookie_from(&right);
        let allowed = client
            .get(at(&server, "/state"))
            .header(header::COOKIE, session)
            .send()
            .expect("answers");
        assert!(allowed.status().is_success());
    }

    /// The password guards the pictures too. A running order of scanned music
    /// would otherwise be readable by anyone who guessed a name.
    #[test]
    fn a_password_guards_the_pictures_as_well() {
        let server = serving("open sesame");
        server.publish_media("a-page".to_string(), b"bytes".to_vec(), "image/png");

        let refused = client().get(at(&server, "/media/a-page")).send().expect("answers");

        assert_eq!(refused.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    /// Turning streaming off has to actually free the port, or turning it on
    /// again would fail on the address it was just using.
    #[test]
    fn stopping_gives_the_port_back() {
        let port = {
            let server = serving("");
            server.port()
        };

        // The same port, straight away.
        let again = StreamServer::start(port, String::new());

        assert!(
            again.is_ok(),
            "the port was not released: {:?}",
            again.err()
        );
    }

    /// A viewer is told where things stand the moment it connects, not when
    /// something next changes.
    ///
    /// This is what a page opened between two slides depends on. Waiting for a
    /// change first left it on its loading placeholder — for as long as the
    /// reading lasted.
    #[test]
    fn a_viewer_is_told_where_things_stand_on_connecting() {
        use std::io::Read;

        let mut server = serving("");
        server.publish(StreamState {
            running: true,
            chapters: vec![super::super::protocol::StreamChapter {
                title: "Amazing Grace".to_string(),
                slides: vec![],
            }],
            ..StreamState::default()
        });

        // Connecting *after* the change, and receiving without another one.
        let mut stream = client()
            .get(at(&server, "/events"))
            .send()
            .expect("the stream opens");

        let mut buffer = [0u8; 1024];
        let read = stream
            .read(&mut buffer)
            .expect("the current state arrives without waiting for a change");
        let first = String::from_utf8_lossy(&buffer[..read]);

        assert!(read > 0, "something arrives at once");
        assert!(
            first.contains("Amazing Grace"),
            "and it is the state that was published, got: {first}"
        );
    }

    /// A private address is what a phone on the same wi-fi can reach, so it
    /// wins over anything else on the machine.
    #[test]
    fn a_private_address_is_preferred() {
        let candidates = vec![
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(169, 254, 3, 4)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 0, 52)),
        ];

        assert_eq!(
            best_address(&candidates),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 52)))
        );
    }

    /// Loopback is never the answer: a page that opens only on the machine
    /// already showing the presentation is no use to anyone.
    #[test]
    fn loopback_is_never_offered() {
        let only_loopback = vec![IpAddr::V4(Ipv4Addr::LOCALHOST)];

        assert_eq!(best_address(&only_loopback), None);
        assert_eq!(best_address(&[]), None);
    }

    /// With nothing private on offer, any real address beats none — a machine
    /// on a network that hands out public addresses still has to be reachable.
    #[test]
    fn something_reachable_beats_nothing() {
        let candidates = vec![
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7)),
        ];

        assert_eq!(
            best_address(&candidates),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7)))
        );
    }

    /// Every private range counts, not just the one this machine happens to be
    /// on — 10.x and 172.16–31.x are as common in halls as 192.168.x.
    #[test]
    fn every_private_range_is_recognised() {
        for private in [
            Ipv4Addr::new(10, 0, 0, 5),
            Ipv4Addr::new(172, 20, 1, 2),
            Ipv4Addr::new(192, 168, 178, 30),
        ] {
            let candidates = vec![IpAddr::V4(Ipv4Addr::LOCALHOST), IpAddr::V4(private)];
            assert_eq!(best_address(&candidates), Some(IpAddr::V4(private)));
        }
    }

    /// A picture is named after where it came from, not after what is in it.
    /// So a browser may keep it, but has to check: a background edited between
    /// two services must reach the viewers who saw the old one.
    #[test]
    fn a_changed_picture_reaches_a_viewer_who_has_the_old_one() {
        let server = serving("");
        let client = client();
        server.publish_media("a-page".to_string(), b"the first one".to_vec(), "image/png");

        let first = client.get(at(&server, "/media/a-page")).send().expect("answers");
        assert!(first.status().is_success());
        let tag = first
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .expect("a picture says what it is")
            .to_string();

        // Asking again with what it already has: nothing to send.
        let again = client
            .get(at(&server, "/media/a-page"))
            .header(header::IF_NONE_MATCH, &tag)
            .send()
            .expect("answers");
        assert_eq!(again.status(), reqwest::StatusCode::NOT_MODIFIED);

        // The picture behind the name changes; the old answer must not stand.
        server.publish_media("a-page".to_string(), b"a different one".to_vec(), "image/png");
        let changed = client
            .get(at(&server, "/media/a-page"))
            .header(header::IF_NONE_MATCH, &tag)
            .send()
            .expect("answers");

        assert!(changed.status().is_success(), "the new picture is sent");
        assert_eq!(
            changed.bytes().expect("bytes"),
            b"a different one".as_slice()
        );
    }

    /// The one that would silently reduce this feature to "works on my
    /// machine": listening on loopback only.
    #[test]
    fn the_server_listens_on_every_interface() {
        let address = bind_address();

        assert!(address.is_unspecified(), "got: {address}");
        assert!(!address.is_loopback(), "loopback would hide it from the network");
    }

    /// Switching streaming off while somebody is watching must return at once.
    ///
    /// This is the shape of a real freeze: the presenter turns the switch off
    /// and the whole interface stands still until every browser tab pointed at
    /// Cantara is closed — because a graceful shutdown waits for connections,
    /// and an event stream has no end of its own.
    #[test]
    fn stopping_does_not_wait_for_viewers() {
        use std::io::{Read, Write};
        use std::time::{Duration, Instant};

        let server = StreamServer::start(0, String::new()).expect("started");
        let port = server.port();

        let mut viewer = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connected");
        viewer
            .write_all(b"GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("asked for the events");
        viewer
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set a timeout");

        // Read the opening state, so the stream is known to be live and not
        // merely connected.
        let mut opening = [0u8; 512];
        let read = viewer.read(&mut opening).expect("got the opening state");
        assert!(read > 0, "the viewer is watching");

        let started = Instant::now();
        drop(server);
        let took = started.elapsed();

        assert!(
            took < Duration::from_secs(2),
            "stopping waited {took:?} for a viewer who never went away"
        );
    }

    /// Nothing on the page reads the session cookie, so nothing should be able
    /// to. Cheap to keep, and awkward to add back after a page has grown a
    /// script that someone else can influence.
    #[test]
    fn the_session_cookie_is_out_of_reach_of_scripts() {
        let cookie = session_cookie("s3cret-session");

        assert!(cookie.contains("HttpOnly"), "got: {cookie}");
        assert!(cookie.contains("SameSite=Lax"), "got: {cookie}");
        assert!(cookie.starts_with("cantara_stream=s3cret-session;"), "got: {cookie}");
    }
}

