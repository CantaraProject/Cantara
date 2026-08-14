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
    use crate::logic::video::{ByteRange, parse_byte_range, path_of_video_url};
    use dioxus::desktop::{use_asset_handler, wry::http::Response};
    use std::io::{Read, Seek, SeekFrom};

    /// Registers the handler for the window this is called in.
    pub(super) fn register() {
        use_asset_handler(crate::logic::video::VIDEO_HANDLER, move |request, responder| {
            let response = answer(request.uri().path(), range_header(&request));
            responder.respond(response);
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
    /// Kept apart from the responder so that the decisions in it — which file,
    /// which bytes, which status — are ordinary code rather than something that
    /// only happens inside a web view.
    fn answer(path: &str, range: Option<String>) -> Response<Vec<u8>> {
        let Some(file_path) = path_of_video_url(path) else {
            return status(400);
        };

        let file_path = std::path::PathBuf::from(&file_path);
        // The URL says which file, and the URL came back out of the web view.
        // Only something that is a video by its name is served: this handler
        // can otherwise be asked for any file the user running Cantara can
        // read, and the page that asks is not necessarily one Cantara wrote —
        // a video slide could name a path that arrived in an imported running
        // order.
        if crate::logic::sourcefiles::SourceFileType::of(
            &file_path.file_name().unwrap_or_default().to_string_lossy(),
        ) != Some(crate::logic::sourcefiles::SourceFileType::Video)
        {
            return status(403);
        }

        let Ok(mut file) = std::fs::File::open(&file_path) else {
            return status(404);
        };
        let Ok(metadata) = file.metadata() else {
            return status(404);
        };
        let length = metadata.len();

        let mime = crate::logic::sourcefiles::mime_type_of_video(
            &file_path.file_name().unwrap_or_default().to_string_lossy(),
        );

        // No `Range` means the whole file, and that is a `200`. A `Range` that
        // cannot be satisfied is a `416` rather than the whole file, or a
        // player that asked to start ten minutes in would play the beginning
        // and nobody would know why.
        let (wanted, partial) = match &range {
            Some(header) => match parse_byte_range(header, length) {
                Some(wanted) => (wanted, true),
                None => {
                    return Response::builder()
                        .status(416)
                        .header("Content-Range", format!("bytes */{length}"))
                        .body(Vec::new())
                        .unwrap_or_else(|_| status(416));
                }
            },
            None => match ByteRange::whole(length) {
                Some(whole) => (whole, false),
                None => (ByteRange { start: 0, end: 0 }, false),
            },
        };

        let mut body = vec![0u8; wanted.length() as usize];
        if file.seek(SeekFrom::Start(wanted.start)).is_err() {
            return status(500);
        }
        match file.read_exact(&mut body) {
            Ok(()) => {}
            Err(_) => return status(500),
        }

        let builder = Response::builder()
            .header("Content-Type", mime)
            // Without this a web view will not seek at all: it takes the
            // absence of the header to mean the whole file or nothing.
            .header("Accept-Ranges", "bytes")
            .header("Content-Length", body.len().to_string());

        let builder = if partial {
            builder.status(206).header(
                "Content-Range",
                format!("bytes {}-{}/{}", wanted.start, wanted.end, length),
            )
        } else {
            builder.status(200)
        };

        builder.body(body).unwrap_or_else(|_| status(500))
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
