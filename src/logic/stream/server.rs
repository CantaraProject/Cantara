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

/// The name of the cookie a viewer is given once they have proved they know
/// the password.
const SESSION_COOKIE: &str = "cantara_stream";

/// A picture a slide refers to, ready to be served.
#[derive(Clone)]
pub struct Media {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

/// What the server and the program both reach into.
struct Shared {
    /// The current state. Viewers wait on this rather than asking for it.
    state: watch::Sender<Arc<StreamState>>,
    /// The pictures the running order refers to, by the name the state gives
    /// them.
    media: RwLock<HashMap<String, Media>>,
    /// What a viewer has to know. Empty means anyone may watch.
    password: String,
    /// Handed out when the password is given correctly.
    session: String,
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
    /// Told once, when the server should stop.
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
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
            password,
            session: new_session_token(),
        });

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
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
                    let address = SocketAddr::from(([0, 0, 0, 0], port));
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

                    let app = router(thread_shared);
                    let served = axum::serve(listener, app)
                        .with_graceful_shutdown(async {
                            let _ = shutdown_rx.await;
                        })
                        .await;
                    if let Err(error) = served {
                        log::error!("the streaming server stopped: {error}");
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
            shutdown: Some(shutdown_tx),
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
        if let Ok(mut media) = self.shared.media.write() {
            media.insert(id, Media { bytes, content_type });
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
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
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
        .route("/media/{id}", get(media))
        .route("/login", post(login))
        .with_state(shared)
}

async fn page() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        VIEWER_PAGE,
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
    use futures_util::stream;

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
    });

    Sse::new(updates)
        .keep_alive(KeepAlive::default())
        .into_response()
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

    match found {
        Some(picture) => (
            [
                (header::CONTENT_TYPE, picture.content_type),
                // Named after its contents, so it can be kept for good.
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            picture.bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "no such picture").into_response(),
    }
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

    let cookie = format!("{SESSION_COOKIE}={}; Path=/; SameSite=Lax", shared.session);
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
/// Found by asking the operating system which interface it *would* use to
/// reach the outside world, rather than by listing every interface and
/// guessing. No packet is sent — a UDP socket that has never been written to
/// sends nothing — and no name server is consulted, so this works on a hall
/// network with no internet at all.
fn local_address() -> IpAddr {
    let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            socket.connect((Ipv4Addr::new(203, 0, 113, 1), 80))?;
            socket.local_addr()
        })
        .map(|address| address.ip());

    match probe {
        Ok(address) if !address.is_unspecified() => address,
        // No network at all. `localhost` is at least true, and says plainly
        // that nobody else is going to reach it.
        _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared_with(password: &str) -> Shared {
        Shared {
            state: watch::channel(Arc::new(StreamState::waiting(0))).0,
            media: RwLock::new(HashMap::new()),
            password: password.to_string(),
            session: "s3cret-session".to_string(),
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
}
