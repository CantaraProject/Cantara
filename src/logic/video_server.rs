//! The server that hands videos to the web view on the WebKitGTK platforms.
//!
//! Everywhere else a video reaches the page through the asset handler in
//! [`crate::components::video_host`]: a path on the page's own origin,
//! answered without a socket being involved. That is the better arrangement
//! and it is what Windows and macOS keep.
//!
//! WebKitGTK cannot use it. The page is served from the custom `dioxus://`
//! scheme, and WebKitGTK's media player will not load media from a custom URI
//! scheme at all — a `<video>` pointed at one fails with `FormatError` before
//! the first byte is asked for, so the request never reaches the handler and
//! there is nothing it could have answered differently. The picture stays on
//! whatever was behind it, which is the design's background, and nothing in
//! the program is told that anything went wrong.
//!
//! What its media player *does* accept is ordinary HTTP. So on those platforms
//! the same bytes are served over a socket on the loopback interface instead,
//! by this. It answers exactly what the asset handler answers — both call
//! [`crate::logic::video::answer_video_request`] — and the difference is only
//! in how the answer travels.
//!
//! Two things it must get right, both of them learned from WebKitGTK refusing
//! the alternative:
//!
//! * A request carrying a `Range` is answered with `206` and never with `200`.
//!   WebKitGTK's source element treats a `200` to a range request as an error
//!   and gives up on the video.
//! * The address is the loopback interface and nothing else. This serves files
//!   from the user's disk; nobody outside the machine has any business asking.
//!
//! And one thing worth being careful about even so: every other program on the
//! machine can reach a loopback port, so the URL carries a token made when the
//! server starts. Without it a request is refused whatever it asks for, which
//! keeps the port from being a way for anything else running as the user to
//! read their video library.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::extract::{Request, State};
use axum::response::{IntoResponse, Response};
use axum::routing::any;

/// Where a video is fetched from, origin and token: something like
/// `http://127.0.0.1:41234/2f0c…`. A [`crate::logic::video::video_url`] goes
/// straight onto the end of it.
///
/// Started the first time it is wanted, which is the first video slide of the
/// first service — a program that never shows one never opens a socket. It
/// then stays up for as long as Cantara runs: the windows come and go, and a
/// video that is playing must not be interrupted because one of them closed.
///
/// `None` when the server could not be started at all. The caller falls back
/// to the asset handler's URL, which on these platforms will not play — but a
/// video that does not play is what was already happening, and it is better
/// than refusing to draw the slide.
pub fn origin() -> Option<&'static str> {
    static ORIGIN: OnceLock<Option<String>> = OnceLock::new();
    ORIGIN.get_or_init(start).as_deref()
}

/// Puts the server up, and says where it is.
fn start() -> Option<String> {
    let token = uuid::Uuid::new_v4().as_simple().to_string();
    let served_token = Arc::new(token.clone());

    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Option<u16>>();

    let thread = std::thread::Builder::new()
        .name("cantara-video".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                // Two windows draw the same slide, so two players ask for the
                // same file at once; a single worker would have one of them
                // waiting on the other's read.
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    log::error!("no runtime for the video server: {error}");
                    let _ = ready_tx.send(None);
                    return;
                }
            };

            runtime.block_on(async move {
                // Port 0: any free one. Nothing outside the program needs to
                // know which, so there is no reason to insist on a number and
                // fail when something else already has it.
                let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
                let listener = match tokio::net::TcpListener::bind(address).await {
                    Ok(listener) => listener,
                    Err(error) => {
                        log::error!("the video server could not take a port: {error}");
                        let _ = ready_tx.send(None);
                        return;
                    }
                };
                let port = match listener.local_addr() {
                    Ok(address) => address.port(),
                    Err(error) => {
                        log::error!("the video server has no address: {error}");
                        let _ = ready_tx.send(None);
                        return;
                    }
                };
                let _ = ready_tx.send(Some(port));

                let app = Router::new()
                    .fallback(any(serve))
                    .with_state(served_token);
                if let Err(error) = axum::serve(listener, app).await {
                    log::error!("the video server stopped: {error}");
                }
            });
        })
        .ok()?;
    // The thread outlives this function on purpose; it is only joined by the
    // process ending.
    drop(thread);

    // Waited for, so that the first slide is drawn with a URL that already
    // answers rather than one that is about to.
    let port = ready_rx.recv().ok()??;
    Some(format!("http://127.0.0.1:{port}/{token}"))
}

/// Answers one request for a piece of a video.
async fn serve(State(token): State<Arc<String>>, request: Request) -> Response {
    let path = request.uri().path().to_string();

    // The token is the whole of what stands between this port and the user's
    // video library, so nothing at all is answered without it — not even to
    // say whether a file exists.
    if !path.starts_with(&format!("/{token}/")) {
        return (axum::http::StatusCode::NOT_FOUND, "no such video").into_response();
    }

    let range = request
        .headers()
        .get(axum::http::header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    // Reading a piece of a video is a blocking read of up to the whole file,
    // and doing that on a runtime thread would hold up every other request on
    // it — including the other window asking for the same video.
    let answer = tokio::task::spawn_blocking(move || {
        crate::logic::video::answer_video_request(&path, range.as_deref())
    })
    .await;

    let Ok(answer) = answer else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "the video could not be read",
        )
            .into_response();
    };

    let mut response = (
        axum::http::StatusCode::from_u16(answer.status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        answer.body,
    )
        .into_response();

    let headers = response.headers_mut();
    for (name, value) in &answer.headers {
        if let (Ok(name), Ok(value)) = (
            axum::http::HeaderName::from_bytes(name.as_bytes()),
            axum::http::HeaderValue::from_str(value),
        ) {
            headers.insert(name, value);
        }
    }
    // The page is on another origin than this server — that is the whole point
    // of the arrangement — so drawing a frame of the video into a canvas is
    // refused unless the bytes were allowed to be read across it. That is what
    // the poster frame of a video in an exported deck is made from; see
    // [`crate::logic::video::still_frame`]. Whoever asks has the token
    // already, so this grants nothing that the token did not.
    headers.insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
        axum::http::HeaderValue::from_static("*"),
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server answers a real file over a real socket, in the pieces a
    /// player asks for — a `206` for a range and never a `200`, which is the
    /// case WebKitGTK gives up on.
    #[test]
    fn test_a_video_is_served_over_the_loopback_interface() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let path = folder.path().join("clip.mp4");
        let contents: Vec<u8> = (0..=255u8).collect();
        std::fs::write(&path, &contents).expect("the file can be written");

        let origin = origin().expect("the server starts");
        let url = format!("{origin}{}", crate::logic::video::video_url(&path.to_string_lossy()));

        let client = reqwest::blocking::Client::new();

        let whole = client.get(&url).send().expect("answers");
        assert_eq!(whole.status().as_u16(), 200);
        assert_eq!(
            whole.headers().get("accept-ranges").map(|v| v.to_str().unwrap()),
            Some("bytes"),
            "without this a player will not seek at all"
        );
        assert_eq!(whole.bytes().expect("a body").to_vec(), contents);

        let piece = client
            .get(&url)
            .header("Range", "bytes=10-19")
            .send()
            .expect("answers");
        assert_eq!(
            piece.status().as_u16(),
            206,
            "a range answered with a 200 is what WebKitGTK gives up on"
        );
        assert_eq!(
            piece.headers().get("content-range").map(|v| v.to_str().unwrap()),
            Some("bytes 10-19/256")
        );
        assert_eq!(piece.bytes().expect("a body").to_vec(), contents[10..=19].to_vec());
    }

    /// Every other program on the machine can reach a loopback port. Without
    /// the token nothing is answered — this is what keeps the port from being
    /// a way to read the user's video library.
    #[test]
    fn test_nothing_is_served_without_the_token() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let path = folder.path().join("clip.mp4");
        std::fs::write(&path, vec![0u8; 32]).expect("the file can be written");

        let origin = origin().expect("the server starts");
        let port = origin
            .rsplit_once(':')
            .and_then(|(_, rest)| rest.split('/').next())
            .expect("the origin has a port")
            .to_string();
        let without_token = format!(
            "http://127.0.0.1:{port}{}",
            crate::logic::video::video_url(&path.to_string_lossy())
        );

        let answer = reqwest::blocking::Client::new()
            .get(&without_token)
            .send()
            .expect("answers");

        assert_eq!(answer.status().as_u16(), 404);
    }
}
