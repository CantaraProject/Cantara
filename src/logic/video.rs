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

// ── Serving the file in pieces ───────────────────────────────────────────────

/// The piece of a file a `Range` header asks for, as inclusive byte offsets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ByteRange {
    pub start: u64,
    /// Inclusive, as HTTP counts it: `bytes=0-0` is the first byte.
    pub end: u64,
}

impl ByteRange {
    /// How many bytes this covers.
    pub fn length(self) -> u64 {
        self.end - self.start + 1
    }

    /// The whole of a file of this size.
    pub fn whole(length: u64) -> Option<ByteRange> {
        (length > 0).then(|| ByteRange {
            start: 0,
            end: length - 1,
        })
    }
}

/// What a `Range` header asks for, clamped to a file of `length` bytes.
///
/// Only a single range is honoured. A browser asking for several at once wants
/// a `multipart/byteranges` reply, and no video element does that — they ask
/// for one piece, play it, and ask for the next. Answering a multi-range
/// request with the whole file is allowed and is what happens here.
///
/// `None` means the request cannot be satisfied and the caller should say so
/// with `416`: an unreadable header, or a start beyond the end of the file. It
/// deliberately does *not* mean "send the whole thing" — a player that asked to
/// start at ten minutes and silently got the beginning would play the wrong
/// part of the video.
pub fn parse_byte_range(header: &str, length: u64) -> Option<ByteRange> {
    if length == 0 {
        return None;
    }
    let spec = header.trim().strip_prefix("bytes=")?;
    // The first of several, if several were asked for.
    let spec = spec.split(',').next()?.trim();
    let (from, to) = spec.split_once('-')?;

    let range = match (from.trim(), to.trim()) {
        // `bytes=-500`: the *last* 500 bytes. Which is how a player finds the
        // index at the end of an MP4 that was not prepared for streaming, so it
        // is not an exotic case at all.
        ("", suffix) => {
            let wanted: u64 = suffix.parse().ok()?;
            if wanted == 0 {
                return None;
            }
            ByteRange {
                start: length.saturating_sub(wanted),
                end: length - 1,
            }
        }
        // `bytes=100-`: from there to the end.
        (start, "") => {
            let start: u64 = start.parse().ok()?;
            ByteRange {
                start,
                end: length - 1,
            }
        }
        // `bytes=100-200`, clamped: a player may ask past the end, and the
        // answer to that is the rest of the file rather than an error.
        (start, end) => {
            let start: u64 = start.parse().ok()?;
            let end: u64 = end.parse().ok()?;
            ByteRange {
                start,
                end: end.min(length - 1),
            }
        }
    };

    // A start past the end of the file is the one thing worth refusing.
    (range.start < length && range.start <= range.end).then_some(range)
}

// ── Which window is allowed to make sound ───────────────────────────────────
//
// A running presentation is drawn by more than one window at a time: the
// projection the room is looking at, and the presenter console in front of the
// operator. Both draw the same slide, so both hold a `<video>` element with the
// same file in it. If both played it aloud, the room would hear the same audio
// twice, tens of milliseconds apart — which is not a doubled volume but a
// flanging echo, and it is worse than either window alone.
//
// So one of them owns the sound and the others are muted. The console owns it
// when there is one: that is where the operator is, it is where the controls
// are, and it is the window whose sound card is wired to the hall. Where there
// is no console the projection owns it, because otherwise nothing would.
//
// This is a property of the *machine*, not of a presentation or a slide, which
// is why it lives in a process-wide value rather than in any component's state.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// Which window is playing the sound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AudioOwner {
    /// The presenter console. Claimed while one is on screen.
    Console,
    /// The window the room is looking at. What is left when there is no
    /// console.
    Projection,
}

impl AudioOwner {
    fn code(self) -> u8 {
        match self {
            AudioOwner::Console => 1,
            AudioOwner::Projection => 2,
        }
    }
}

/// Who currently owns the sound; `0` for nobody.
static AUDIO_OWNER: AtomicU8 = AtomicU8::new(0);

/// Counts up whenever the answer changes, so a window that is already showing
/// a video notices that it has gained or lost the sound.
static AUDIO_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Takes the sound for this kind of window.
///
/// A console takes it from a projection that had it. A projection does *not*
/// take it from a console — that is the whole rule, and it is written here
/// rather than at the two call sites so that it cannot be half-applied.
pub fn claim_audio(owner: AudioOwner) {
    let current = AUDIO_OWNER.load(Ordering::SeqCst);
    if current == owner.code() {
        return;
    }
    if owner == AudioOwner::Projection && current == AudioOwner::Console.code() {
        return;
    }
    AUDIO_OWNER.store(owner.code(), Ordering::SeqCst);
    AUDIO_GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Gives the sound up, if this kind of window is the one holding it.
///
/// Called when a console closes. Nobody owns it afterwards, and the projection
/// claims it on its next look — rather than being handed it here, because the
/// projection may have closed too.
pub fn release_audio(owner: AudioOwner) {
    if AUDIO_OWNER
        .compare_exchange(owner.code(), 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        AUDIO_GENERATION.fetch_add(1, Ordering::Relaxed);
    }
}

/// Whether this kind of window should be playing the sound.
pub fn owns_audio(owner: AudioOwner) -> bool {
    AUDIO_OWNER.load(Ordering::SeqCst) == owner.code()
}

/// Counts up when the answer to [`owns_audio`] can have changed.
pub fn audio_generation() -> u64 {
    AUDIO_GENERATION.load(Ordering::Relaxed)
}

/// Forgets who owns the sound.
///
/// Only the tests need it: in the program the owner is claimed and released by
/// the windows themselves, and there is no moment where it has to be forced
/// back to nobody.
#[cfg(test)]
pub fn reset_audio_owner() {
    AUDIO_OWNER.store(0, Ordering::SeqCst);
    AUDIO_GENERATION.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a video element actually sends, in the order it sends it: the
    /// first bytes to find out what the file is, then the end for the index,
    /// then the middle when somebody seeks.
    #[test]
    fn test_the_ranges_a_player_asks_for() {
        let length = 1_000;

        assert_eq!(
            parse_byte_range("bytes=0-", length),
            Some(ByteRange { start: 0, end: 999 }),
            "the whole file from the start"
        );
        assert_eq!(
            parse_byte_range("bytes=0-1023", length),
            Some(ByteRange { start: 0, end: 999 }),
            "asking past the end gets the rest, not an error"
        );
        assert_eq!(
            parse_byte_range("bytes=-500", length),
            Some(ByteRange { start: 500, end: 999 }),
            "the last 500 bytes — where an MP4 keeps its index"
        );
        assert_eq!(
            parse_byte_range("bytes=200-299", length),
            Some(ByteRange { start: 200, end: 299 }),
            "the middle, which is a seek"
        );
    }

    /// The inclusive end is the part that is easy to get wrong by one, and
    /// getting it wrong by one truncates every response.
    #[test]
    fn test_the_end_is_inclusive() {
        assert_eq!(
            parse_byte_range("bytes=0-0", 10).map(ByteRange::length),
            Some(1),
            "one byte, not zero"
        );
        assert_eq!(
            parse_byte_range("bytes=0-", 10).map(ByteRange::length),
            Some(10),
            "the whole file, not one short of it"
        );
        assert_eq!(ByteRange::whole(10), Some(ByteRange { start: 0, end: 9 }));
    }

    /// A suffix longer than the file is the whole file, not a negative offset.
    #[test]
    fn test_asking_for_more_of_the_end_than_there_is() {
        assert_eq!(
            parse_byte_range("bytes=-9999", 100),
            Some(ByteRange { start: 0, end: 99 })
        );
    }

    /// Refused rather than answered with the wrong part of the file. A player
    /// that asked to start at ten minutes and silently got the beginning would
    /// play the wrong thing, which is harder to notice than an error.
    #[test]
    fn test_a_range_that_cannot_be_satisfied_is_refused() {
        assert_eq!(parse_byte_range("bytes=500-", 100), None, "starts past the end");
        assert_eq!(parse_byte_range("bytes=100-", 100), None, "starts at the end");
        assert_eq!(parse_byte_range("bytes=-0", 100), None, "the last nothing");
        assert_eq!(parse_byte_range("bytes=0-", 0), None, "an empty file");
    }

    /// The header comes from outside and is parsed rather than trusted.
    #[test]
    fn test_a_header_that_makes_no_sense_is_refused() {
        for header in [
            "",
            "bytes",
            "bytes=",
            "bytes=abc-def",
            "items=0-10",
            "bytes=10-5",
            "bytes=--5",
            "bytes=99999999999999999999999999-",
        ] {
            assert_eq!(
                parse_byte_range(header, 100),
                None,
                "{header:?} should not have been read as a range"
            );
        }
    }

    /// Several ranges at once get the first one. No video element asks for
    /// more than one, and answering with `multipart/byteranges` for the sake of
    /// something that does not happen is not worth the code.
    #[test]
    fn test_several_ranges_get_the_first_one() {
        assert_eq!(
            parse_byte_range("bytes=0-99, 200-299", 1000),
            Some(ByteRange { start: 0, end: 99 })
        );
    }

    /// A serial guard. The owner is process-wide, so these tests are talking
    /// about the same value and must not run over one another.
    fn audio_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// With no console, the projection is what makes the sound — otherwise
    /// nothing would.
    #[test]
    fn test_the_projection_makes_the_sound_when_there_is_no_console() {
        let _guard = audio_lock();
        reset_audio_owner();

        claim_audio(AudioOwner::Projection);

        assert!(owns_audio(AudioOwner::Projection));
        assert!(!owns_audio(AudioOwner::Console));
    }

    /// A console takes the sound off the projection. Both playing aloud is the
    /// same audio twice, tens of milliseconds apart.
    #[test]
    fn test_a_console_takes_the_sound_from_the_projection() {
        let _guard = audio_lock();
        reset_audio_owner();
        claim_audio(AudioOwner::Projection);

        claim_audio(AudioOwner::Console);

        assert!(owns_audio(AudioOwner::Console));
        assert!(!owns_audio(AudioOwner::Projection));
    }

    /// …and the projection does not take it back while the console is there.
    /// The projection claims on every look, so without this rule the two would
    /// take turns and the sound would stutter between windows.
    #[test]
    fn test_the_projection_does_not_take_the_sound_back_from_a_console() {
        let _guard = audio_lock();
        reset_audio_owner();
        claim_audio(AudioOwner::Console);

        claim_audio(AudioOwner::Projection);

        assert!(owns_audio(AudioOwner::Console), "the console lost the sound");
        assert!(!owns_audio(AudioOwner::Projection));
    }

    /// When the console closes, the projection can have it again.
    #[test]
    fn test_closing_the_console_gives_the_sound_back() {
        let _guard = audio_lock();
        reset_audio_owner();
        claim_audio(AudioOwner::Console);

        release_audio(AudioOwner::Console);
        assert!(!owns_audio(AudioOwner::Console));
        assert!(!owns_audio(AudioOwner::Projection), "nobody owns it yet");

        claim_audio(AudioOwner::Projection);
        assert!(owns_audio(AudioOwner::Projection));
    }

    /// Giving up something you do not hold changes nothing. A projection that
    /// closes while the console is playing must not silence the console.
    #[test]
    fn test_releasing_what_you_do_not_hold_changes_nothing() {
        let _guard = audio_lock();
        reset_audio_owner();
        claim_audio(AudioOwner::Console);

        release_audio(AudioOwner::Projection);

        assert!(owns_audio(AudioOwner::Console));
    }

    /// A window already showing a video has to notice that it has gained or
    /// lost the sound, and it notices by this counter moving.
    #[test]
    fn test_the_counter_moves_when_the_owner_changes_and_not_otherwise() {
        let _guard = audio_lock();
        reset_audio_owner();
        claim_audio(AudioOwner::Projection);

        let before = audio_generation();
        claim_audio(AudioOwner::Projection);
        assert_eq!(audio_generation(), before, "claiming twice is not a change");

        claim_audio(AudioOwner::Console);
        assert_ne!(audio_generation(), before, "the console taking it is");
    }

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
