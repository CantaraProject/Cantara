//! Serving video files to the window that plays them.
//!
//! One of these is mounted in every window that can show a slide, because an
//! asset handler belongs to a web view and each window has its own. What it
//! answers, and why the file is served rather than inlined, is in
//! [`crate::logic::video`].
//!
//! Only the desktop has a handler to register. The web build's library lives in
//! a zip in memory rather than on a disk, and a browser will not let a page
//! read a file by path — video there is a piece of work of its own.
//!
//! On the WebKitGTK platforms nothing asks this for a video, whatever it is
//! mounted in: that web view will not play media from the page's own scheme at
//! all, and a slide there points at the loopback server in
//! [`crate::logic::video_server`] instead. The handler stays registered
//! because it is the same code answering either way, and because what is
//! mounted in a window should not depend on which platform the window is on.

use dioxus::prelude::*;

/// Answers this window's requests for video files.
///
/// Renders nothing.
#[component]
pub fn VideoAssetHost() -> Element {
    #[cfg(feature = "desktop")]
    desktop::register();

    rsx! {}
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{use_asset_handler, wry::http::Response};

    /// Registers the handler for the window this is called in.
    ///
    /// The reading happens on a thread of its own. The handler is called on the
    /// thread that runs the window, and a browser's *first* request for a video
    /// carries no `Range` — so the answer to it is the whole file. Reading a
    /// few hundred megabytes there freezes the window that is showing the
    /// presentation for as long as the disk takes, which is what made a video
    /// stutter as it started. Answering is asynchronous by design; this uses
    /// that.
    pub(super) fn register() {
        use_asset_handler(crate::logic::video::VIDEO_HANDLER, move |request, responder| {
            let path = request.uri().path().to_string();
            let range = range_header(&request);
            std::thread::spawn(move || {
                responder.respond(answer(&path, range));
            });
        });
    }

    /// The `Range` the request carries, if any.
    fn range_header(request: &dioxus::desktop::AssetRequest) -> Option<String> {
        request
            .headers()
            .get("Range")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    }

    /// What to send back for a request for `path`.
    ///
    /// Which file, which bytes and which status are decided in
    /// [`crate::logic::video::answer_video_request`], where the loopback
    /// server the WebKitGTK platforms use decides the same things from the
    /// same code — two servers handing out the same file must not be able to
    /// answer the same request differently. All that is left here is putting
    /// the answer into the shape this web view wants.
    fn answer(path: &str, range: Option<String>) -> Response<Vec<u8>> {
        let answer = crate::logic::video::answer_video_request(path, range.as_deref());

        let mut builder = Response::builder().status(answer.status);
        for (name, value) in &answer.headers {
            builder = builder.header(*name, value);
        }
        builder
            .body(answer.body)
            .unwrap_or_else(|_| status(500))
    }

    /// A reply with nothing in it but its status.
    fn status(code: u16) -> Response<Vec<u8>> {
        Response::builder()
            .status(code)
            .body(Vec::new())
            .unwrap_or_else(|_| Response::new(Vec::new()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::logic::video::video_url;

        /// The handler serves whatever URL it is handed, and the page that
        /// hands it one is not necessarily one Cantara wrote — a path can
        /// arrive in an imported running order. So it serves videos and
        /// nothing else, whatever it is asked for.
        #[test]
        fn test_only_a_video_is_served() {
            for path in [
                "/etc/passwd",
                "C:/Windows/System32/config/SAM",
                "settings.json",
                "../../secrets.txt",
            ] {
                let response = answer(&video_url(path), None);
                assert_eq!(
                    response.status(),
                    403,
                    "{path} should not have been served"
                );
            }
        }

        /// A URL that is not one of ours at all.
        #[test]
        fn test_a_url_that_is_not_a_video_url_is_refused() {
            assert_eq!(answer("/assets/main.css", None).status(), 400);
        }

        /// A video that is not there is a 404, not a crash and not an empty
        /// 200 that a player would take for a broken file.
        #[test]
        fn test_a_video_that_is_not_there_is_a_404() {
            let response = answer(&video_url("/nowhere/missing.mp4"), None);
            assert_eq!(response.status(), 404);
        }

        /// Serving a real file, whole and in pieces. The temporary file is
        /// named `.mp4` because that is what the guard above looks at.
        #[test]
        fn test_a_video_is_served_whole_and_in_pieces() {
            let folder = tempfile::tempdir().expect("a temporary folder");
            let path = folder.path().join("clip.mp4");
            let contents: Vec<u8> = (0..=255u8).collect();
            std::fs::write(&path, &contents).expect("the file can be written");
            let url = video_url(&path.to_string_lossy());

            // The whole thing.
            let whole = answer(&url, None);
            assert_eq!(whole.status(), 200);
            assert_eq!(whole.body(), &contents);
            assert_eq!(
                whole.headers().get("Accept-Ranges").unwrap(),
                "bytes",
                "without this a web view will not seek at all"
            );

            // A piece out of the middle, which is what a seek is.
            let piece = answer(&url, Some("bytes=10-19".to_string()));
            assert_eq!(piece.status(), 206);
            assert_eq!(piece.body(), &contents[10..=19]);
            assert_eq!(
                piece.headers().get("Content-Range").unwrap(),
                "bytes 10-19/256"
            );

            // The end, which is where an MP4 keeps its index.
            let tail = answer(&url, Some("bytes=-8".to_string()));
            assert_eq!(tail.status(), 206);
            assert_eq!(tail.body(), &contents[248..=255]);
        }

        /// A file of no bytes has no first byte either. This used to fabricate
        /// a one-byte range, fail to read it, and report a 500 — an internal
        /// error for a file that is merely empty, which is a wrong thing to
        /// send somebody looking for.
        #[test]
        fn test_an_empty_video_is_a_404_rather_than_a_server_error() {
            let folder = tempfile::tempdir().expect("a temporary folder");
            let path = folder.path().join("nothing.mp4");
            std::fs::write(&path, b"").expect("the file can be written");

            let response = answer(&video_url(&path.to_string_lossy()), None);

            assert_eq!(response.status(), 404);
        }

        /// A range past the end is refused rather than answered with the
        /// beginning of the file.
        #[test]
        fn test_a_range_past_the_end_is_a_416() {
            let folder = tempfile::tempdir().expect("a temporary folder");
            let path = folder.path().join("clip.mp4");
            std::fs::write(&path, vec![0u8; 100]).expect("the file can be written");

            let response = answer(
                &video_url(&path.to_string_lossy()),
                Some("bytes=500-".to_string()),
            );

            assert_eq!(response.status(), 416);
            assert_eq!(response.headers().get("Content-Range").unwrap(), "bytes */100");
        }
    }
}
