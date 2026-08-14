//! Getting a video file into the page that draws the presentation.
//!
//! A picture from the library travels into the page as a `data:` URL — read,
//! base64-encoded and pasted into the markup. See [`crate::logic::images`].
//! That cannot be done with a video, for two reasons that both matter:
//!
//! * A service video is tens or hundreds of megabytes, and base64 makes it a
//!   third larger again. Building that string blocks whatever thread it happens
//!   on and the result has to be held in memory twice over.
//! * A `data:` URL cannot be seeked into. The whole thing has to arrive before
//!   anything can be played, and jumping to the middle means decoding
//!   everything before it. A presenter jumping to a position is exactly what
//!   the console is for.
//!
//! So the file is *served* instead, and the web view fetches the parts it wants
//! with range requests, as it would from any web server. On the desktop that
//! server is [`dioxus::desktop::use_asset_handler`], which answers requests
//! from the page's own origin without a socket being involved.
//!
//! The path is carried in the URL rather than in a table of open files, so a
//! window that is opened later — the projection, the presenter console — can
//! resolve a slide it was handed without asking anyone first.

/// The name the asset handler is registered under, and the first segment of
/// every URL it answers.
pub const VIDEO_HANDLER: &str = "cantara-video";

/// Where the page should fetch the video at `path` from.
///
/// The path is percent-encoded into one segment: a library path holds spaces,
/// `#`, `?` and — on Windows — backslashes and a drive letter, and every one of
/// those means something else in a URL.
pub fn video_url(path: &str) -> String {
    format!("/{VIDEO_HANDLER}/{}", encode_path(path))
}

/// The path a [`video_url`] was built from, or `None` when the URL is not one
/// of ours.
pub fn path_of_video_url(url: &str) -> Option<String> {
    let prefix = format!("/{VIDEO_HANDLER}/");
    let encoded = url.strip_prefix(&prefix).or_else(|| {
        // The web view hands the handler a whole URL rather than a path, so the
        // origin in front of it has to be tolerated.
        url.split_once(&prefix).map(|(_, rest)| rest)
    })?;
    // A query string is not part of the path. Nothing adds one today, but a
    // cache-buster is the obvious next thing to want.
    let encoded = encoded.split(['?', '#']).next().unwrap_or(encoded);
    decode_path(encoded)
}

/// Percent-encodes everything that is not plainly safe in a URL segment.
///
/// Deliberately strict — the unreserved set of RFC 3986 and nothing else. A
/// library path is user data, and guessing at which of the reserved characters
/// this particular web view will tolerate is how a file with a `#` in its name
/// stops playing.
fn encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// The other half of [`encode_path`].
///
/// `None` when the text is not something that function produced: a truncated
/// escape, or bytes that are not UTF-8 once put back together.
fn decode_path(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let digits = encoded.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(digits, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one property the pair has to have: whatever goes in comes back out.
    /// Everything below is a case that broke that at some point in a URL.
    #[test]
    fn test_a_path_survives_the_round_trip() {
        for path in [
            "/home/joe/service/intro.mp4",
            // Windows, which is where most of the awkwardness lives.
            r"C:\Users\Jan Martin\Videos\Lobpreis 2026.mp4",
            // Characters that mean something in a URL.
            "/library/Was ist #1?/clip&more.webm",
            "/library/100% Freude.mp4",
            // And characters that are not ASCII at all.
            "/bibliothek/Grüße aus Köln.mp4",
            "/图书馆/视频.mp4",
        ] {
            let url = video_url(path);
            assert_eq!(
                path_of_video_url(&url).as_deref(),
                Some(path),
                "the path did not survive being put into {url}"
            );
        }
    }

    /// The encoding leaves nothing in the URL that could end the segment early
    /// or start a query — that is the whole point of being strict about it.
    #[test]
    fn test_nothing_dangerous_survives_the_encoding() {
        let url = video_url("/library/a b?c#d&e/f.mp4");

        assert!(!url.contains(' '));
        assert!(!url.contains('?'));
        assert!(!url.contains('#'));
        assert!(!url.contains('&'));
        // The one slash that is left is the handler's own.
        assert_eq!(url.matches('/').count(), 2);
    }

    /// A URL that came from somewhere else is not answered as though it were a
    /// video path: the handler serves whatever it is given, so this is what
    /// keeps it from serving the rest of the file system.
    #[test]
    fn test_a_url_that_is_not_ours_is_refused() {
        assert_eq!(path_of_video_url("/assets/main.css"), None);
        assert_eq!(path_of_video_url("/"), None);
        assert_eq!(path_of_video_url(""), None);
    }

    /// The web view hands over a whole URL rather than a bare path, and the
    /// origin in front of it changes between platforms.
    #[test]
    fn test_the_origin_in_front_of_the_path_is_tolerated() {
        let url = video_url("/library/intro.mp4");

        for origin in ["http://dioxus.localhost", "https://dioxus.localhost", ""] {
            assert_eq!(
                path_of_video_url(&format!("{origin}{url}")).as_deref(),
                Some("/library/intro.mp4"),
                "the origin {origin} was not seen through"
            );
        }
    }

    /// A query string is not part of the path.
    #[test]
    fn test_a_query_string_is_not_part_of_the_path() {
        let url = format!("{}?v=2", video_url("/library/intro.mp4"));

        assert_eq!(path_of_video_url(&url).as_deref(), Some("/library/intro.mp4"));
    }

    /// Half an escape is not a path. The URL is attacker-adjacent data — it
    /// comes back from the web view — so it is parsed rather than trusted.
    #[test]
    fn test_a_broken_escape_is_refused_rather_than_panicking() {
        assert_eq!(path_of_video_url(&format!("/{VIDEO_HANDLER}/%")), None);
        assert_eq!(path_of_video_url(&format!("/{VIDEO_HANDLER}/%A")), None);
        assert_eq!(path_of_video_url(&format!("/{VIDEO_HANDLER}/%ZZ")), None);
        // Valid escapes that are not valid UTF-8 together.
        assert_eq!(path_of_video_url(&format!("/{VIDEO_HANDLER}/%FF%FE")), None);
    }
}
