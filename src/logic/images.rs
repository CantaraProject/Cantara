//! Getting a picture from the library onto the screen.
//!
//! A web view will not load `C:\Songs\logo.png` from an `img` tag. It resolves
//! the attribute as a URL against the page, finds nothing there, and shows the
//! placeholder icon a broken image gets — which is what every picture in
//! Cantara had become. The page has no access to the file system; the program
//! does, so the program reads the file and hands the bytes to the page as a
//! `data:` URL.
//!
//! That is the same route a PDF already takes to the page, and unlike a custom
//! protocol handler it needs no registration: the presentation window and the
//! separate presenter console are web views of their own, and the web build
//! has no file system at all — a `data:` URL works in all of them.
//!
//! Encoding is not free, so each picture is encoded once and kept. See
//! [`image_data_url`].

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// The encoded pictures, keyed by the path they were read from.
static IMAGE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// How much encoded picture data is kept before the cache starts over.
///
/// A design's background picture is re-encoded on nothing but a cache miss, so
/// what this bounds is memory, not work. Sixty-four megabytes holds every
/// picture of an ordinary library and still cannot run away on a library of
/// scanned posters.
const CACHE_BUDGET: usize = 64 * 1024 * 1024;

fn cache() -> &'static Mutex<HashMap<String, String>> {
    IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The media type a file name promises.
///
/// Only the formats [`SourceFileType`](crate::logic::sourcefiles::SourceFileType)
/// accepts as a picture can turn up here; anything else is a file the library
/// scan would not have offered in the first place.
fn media_type(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "image/png"
    }
}

/// The bytes of a picture, wherever the build keeps them.
fn read_image(path: &str) -> Option<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    {
        crate::logic::settings::RepositoryType::web_read_file(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(path).ok()
    }
}

/// A picture as something an `img` tag or a `url(…)` can actually load.
///
/// Returns `None` when the file cannot be read, so a caller can leave the tag
/// out rather than render a broken one.
pub fn image_data_url(path: &Path) -> Option<String> {
    image_data_url_str(&path.to_string_lossy())
}

/// [`image_data_url`] for a path that is already a string — which is what the
/// song library hands back for a picture slide.
pub fn image_data_url_str(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }

    // A picture that is already inline, or one served over the network, is a
    // URL the page can follow on its own.
    if path.starts_with("data:") || path.starts_with("http://") || path.starts_with("https://") {
        return Some(path.to_string());
    }

    if let Ok(map) = cache().lock()
        && let Some(cached) = map.get(path)
    {
        return Some(cached.clone());
    }

    let bytes = read_image(path)?;
    let url = format!("data:{};base64,{}", media_type(path), BASE64.encode(&bytes));

    if let Ok(mut map) = cache().lock() {
        if map.values().map(String::len).sum::<usize>() + url.len() > CACHE_BUDGET {
            map.clear();
        }
        map.insert(path.to_string(), url.clone());
    }

    Some(url)
}

// ── Thumbnails for the list of pictures ──────────────────────────────────────
//
// The list is a different problem from a single picture on a slide. It shows
// every picture of the library at once, at a couple of hundred pixels each,
// and inlining them the way [`image_data_url`] does meant reading and encoding
// the whole library — hundreds of megabytes of photographs — during the render
// that was supposed to draw the list. The window stopped responding until it
// was done.
//
// So the list gets scaled-down copies instead, made on background threads and
// filled in as they arrive. A view asks [`thumbnail`] for what is ready and
// draws a placeholder for the rest, and watches [`thumbnail_generation`] to
// know when to look again.

/// What is known about one picture's thumbnail.
#[derive(Clone)]
enum Thumbnail {
    /// A background thread has it.
    Loading,
    /// Done — `None` when the picture could not be read or decoded.
    Done(Option<String>),
}

static THUMBNAILS: OnceLock<Mutex<HashMap<PathBuf, Thumbnail>>> = OnceLock::new();

/// Bumped every time a thumbnail lands, which is how a view notices without
/// the background threads having to reach into it. Signals cannot be touched
/// from another thread, and a counter can.
static THUMBNAIL_GENERATION: AtomicU64 = AtomicU64::new(0);

/// How many pictures are still being worked on.
static THUMBNAILS_OUTSTANDING: AtomicUsize = AtomicUsize::new(0);

/// The longest edge of a list thumbnail, in pixels.
///
/// The list draws them 300 pixels tall, so this still has something in hand
/// for a high-resolution screen while being a thousandth of the data of the
/// photograph it came from.
const THUMBNAIL_MAX_EDGE: u32 = 600;

fn thumbnails() -> &'static Mutex<HashMap<PathBuf, Thumbnail>> {
    THUMBNAILS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The thumbnail of a picture, if one has been made.
///
/// Never reads a file: a view calls this while it renders, and that is the
/// place the whole arrangement exists to keep free of file access. `None`
/// means "not ready yet", which the list draws as a placeholder.
pub fn thumbnail(path: &Path) -> Option<String> {
    let map = thumbnails().lock().ok()?;
    match map.get(path) {
        Some(Thumbnail::Done(thumbnail)) => thumbnail.clone(),
        _ => None,
    }
}

/// Counts up as thumbnails arrive. A view that remembers the last value it saw
/// knows whether there is anything new to draw.
pub fn thumbnail_generation() -> u64 {
    THUMBNAIL_GENERATION.load(Ordering::Relaxed)
}

/// Whether any thumbnail is still being made.
pub fn thumbnails_in_progress() -> bool {
    THUMBNAILS_OUTSTANDING.load(Ordering::Relaxed) > 0
}

/// Makes the thumbnails of `paths` that do not have one yet.
///
/// Returns at once; the work happens on background threads. Calling it again
/// with the same pictures does nothing, so a view may call it on every scan.
pub fn prepare_thumbnails(paths: Vec<PathBuf>) {
    let todo: Vec<PathBuf> = {
        let Ok(mut map) = thumbnails().lock() else {
            return;
        };
        paths
            .into_iter()
            .filter(|path| {
                // Only a picture nothing is known about is claimed. Writing
                // `Loading` unconditionally — and only queueing the entries
                // that were new — put every *finished* thumbnail back into a
                // state nothing would ever complete, which is what left the
                // list showing its placeholders for good: the scan makes them
                // once, and the list asking a second time threw them away.
                match map.entry(path.clone()) {
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(Thumbnail::Loading);
                        true
                    }
                    std::collections::hash_map::Entry::Occupied(_) => false,
                }
            })
            .collect()
    };

    if todo.is_empty() {
        return;
    }
    THUMBNAILS_OUTSTANDING.fetch_add(todo.len(), Ordering::Relaxed);

    // The web build has no threads. Its pictures are already in memory rather
    // than on a disk, so the cost is the encoding alone and it is paid here.
    #[cfg(target_arch = "wasm32")]
    {
        for path in todo {
            let made = image_data_url(&path);
            finish_thumbnail(path, made);
        }
    }

    // One thread hands the pictures out to the others, so that this call
    // returns immediately and the pictures are read several at a time.
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        crate::logic::parallel::map_parallel(&todo, |path| {
            let made = make_thumbnail(path);
            finish_thumbnail(path.clone(), made);
        });
        // Nothing may be left saying `Loading` that no longer has a thread
        // behind it: a picture whose decoder gave up mid-way would otherwise
        // keep the list looking for a thumbnail that is never coming.
        abandon_unfinished(&todo);
    });
}

/// Marks as failed anything still claimed but not delivered.
#[cfg(not(target_arch = "wasm32"))]
fn abandon_unfinished(paths: &[PathBuf]) {
    let Ok(mut map) = thumbnails().lock() else {
        return;
    };
    for path in paths {
        if matches!(map.get(path), Some(Thumbnail::Loading)) {
            log::warn!("could not make a thumbnail of {}", path.display());
            map.insert(path.clone(), Thumbnail::Done(None));
            THUMBNAILS_OUTSTANDING.fetch_sub(1, Ordering::Relaxed);
            THUMBNAIL_GENERATION.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Records a finished thumbnail and lets the views know there is one more.
fn finish_thumbnail(path: PathBuf, made: Option<String>) {
    if let Ok(mut map) = thumbnails().lock() {
        map.insert(path, Thumbnail::Done(made));
    }
    THUMBNAILS_OUTSTANDING.fetch_sub(1, Ordering::Relaxed);
    THUMBNAIL_GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Reads a picture and scales it down to something a list can hold.
///
/// Only where the image library is compiled in. Everywhere else the picture is
/// inlined whole — still off the render, which is what made the list
/// unresponsive, just larger than it needs to be.
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
fn make_thumbnail(path: &Path) -> Option<String> {
    let picture = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;

    // `thumbnail` is the cheap filter on purpose: at this size the difference
    // is not visible and the library may hold hundreds of photographs.
    let small = picture.thumbnail(THUMBNAIL_MAX_EDGE, THUMBNAIL_MAX_EDGE);

    // A photograph goes back out as JPEG, which is a tenth of the size of the
    // same thing as PNG and indistinguishable at this scale — and the whole
    // list travels into the page as markup, so its size is what the window has
    // to carry. A picture with transparency stays PNG: JPEG has no
    // transparency, and a logo would come back with a black box around it.
    let (media_type, format) = if small.color().has_alpha() {
        ("image/png", image::ImageFormat::Png)
    } else {
        ("image/jpeg", image::ImageFormat::Jpeg)
    };

    let mut encoded: Vec<u8> = Vec::new();
    small
        .write_to(&mut std::io::Cursor::new(&mut encoded), format)
        .ok()?;

    Some(format!("data:{};base64,{}", media_type, BASE64.encode(&encoded)))
}

#[cfg(all(not(feature = "desktop"), not(target_arch = "wasm32")))]
fn make_thumbnail(path: &Path) -> Option<String> {
    image_data_url(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page has to be told what it is being handed, or a JPEG arrives
    /// labelled as a PNG.
    #[test]
    fn the_media_type_follows_the_suffix() {
        assert_eq!(media_type("cover.jpg"), "image/jpeg");
        assert_eq!(media_type("cover.JPEG"), "image/jpeg");
        assert_eq!(media_type("cover.png"), "image/png");
    }

    /// A picture the page can already fetch is passed through untouched — it
    /// would be pointless to download it and hand it back inline.
    #[test]
    fn an_address_the_page_can_follow_is_left_alone() {
        assert_eq!(
            image_data_url_str("https://example.org/logo.png").as_deref(),
            Some("https://example.org/logo.png")
        );
        assert_eq!(
            image_data_url_str("data:image/png;base64,AAAA").as_deref(),
            Some("data:image/png;base64,AAAA")
        );
        assert_eq!(image_data_url_str(""), None);
    }

    /// A file that cannot be read gives nothing, so the caller can leave the
    /// tag out instead of rendering a broken picture.
    #[test]
    fn a_missing_file_gives_nothing() {
        assert_eq!(image_data_url(Path::new("testfiles/not-a-picture.png")), None);
    }

    /// Asking for a thumbnail must never read a file: the list asks while it
    /// is rendering, and reading there is what made the window stop
    /// responding.
    #[test]
    fn a_thumbnail_that_is_not_ready_is_not_waited_for() {
        assert_eq!(thumbnail(Path::new("assets/favicon.png")), None);
    }

    /// The list gets a small copy rather than the picture itself, made off the
    /// render and announced by the counter the list watches.
    #[test]
    fn a_thumbnail_is_made_in_the_background_and_is_smaller_than_the_picture() {
        let logo = PathBuf::from("assets/cantara-logo_small.png");
        let before = thumbnail_generation();

        prepare_thumbnails(vec![logo.clone()]);

        // The work is on another thread; the call above must not have waited
        // for it. Give it a moment to land.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while thumbnails_in_progress() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let small = thumbnail(&logo).expect("the bundled logo can be scaled down");
        assert!(small.starts_with("data:image/"), "got: {}", &small[..30.min(small.len())]);
        assert!(
            thumbnail_generation() > before,
            "the list is never told that the thumbnail arrived"
        );

        let whole = image_data_url(&logo).expect("the bundled logo can be read");
        assert!(
            small.len() <= whole.len(),
            "the list is being handed {} bytes where the picture itself is {}",
            small.len(),
            whole.len()
        );

        // Asking again must not queue the same picture a second time — and
        // above all must not take away the thumbnail that is already there.
        // The list asks on every scan and again when it is drawn, and doing
        // that used to leave every picture stuck behind its placeholder.
        prepare_thumbnails(vec![logo.clone()]);
        assert!(!thumbnails_in_progress());
        // Compared by length rather than by value: a mismatch would otherwise
        // print two screenfuls of base64.
        assert_eq!(
            thumbnail(&logo).map(|kept| kept.len()),
            Some(small.len()),
            "asking a second time threw the thumbnail away"
        );
    }

    /// What the page gets is a `data:` URL carrying the file, and the second
    /// request for the same file gives exactly the same one.
    #[test]
    fn a_picture_becomes_an_inline_address() {
        let logo = Path::new("assets/cantara-logo_small.png");

        let url = image_data_url(logo).expect("the bundled logo can be read");

        assert!(url.starts_with("data:image/png;base64,"), "got: {}", &url[..40.min(url.len())]);
        assert_eq!(image_data_url(logo).as_deref(), Some(url.as_str()));
    }
}
