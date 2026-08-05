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
use std::path::Path;
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
