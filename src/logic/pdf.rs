//! Talking to the PDF viewer that lives in the page.
//!
//! The rendering itself is in `assets/pdf_viewer.js`, loaded once per window.
//! What is left on this side is small on purpose: reading the file, handing it
//! over once, and asking for a page. Everything a caller sends is a few dozen
//! bytes — where the whole rendering program used to be re-sent as source text
//! on every draw.
//!
//! Two kinds of caller:
//!
//! - A view that shows a page draws it into a canvas of its own; see
//!   [`crate::components::presentation_components::PdfPageCanvas`].
//! - Anything that needs a page as a *picture* — the pptx export, and streaming
//!   later — asks [`page_image`], which draws it with nothing on screen.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::prelude::*;

/// The viewer, loaded once per window by whichever root renders PDFs.
pub const PDF_VIEWER_JS: Asset = asset!("/assets/pdf_viewer.js");

/// PDF.js, and the worker it hands the parsing to.
///
/// The desktop serves them from the bundled `node_modules`; the web build has
/// no file system to serve them from and takes them from a CDN.
#[cfg(not(target_arch = "wasm32"))]
pub const PDFJS_LIB: Asset = asset!("/node_modules/pdfjs-dist/build/pdf.min.mjs");
#[cfg(not(target_arch = "wasm32"))]
pub const PDFJS_WORKER: Asset = asset!("/node_modules/pdfjs-dist/build/pdf.worker.min.mjs");
#[cfg(target_arch = "wasm32")]
pub const PDFJS_CDN_LIB: &str = "https://cdn.jsdelivr.net/npm/pdfjs-dist@4.10.38/build/pdf.min.mjs";
#[cfg(target_arch = "wasm32")]
pub const PDFJS_CDN_WORKER: &str =
    "https://cdn.jsdelivr.net/npm/pdfjs-dist@4.10.38/build/pdf.worker.min.mjs";

fn pdfjs_urls() -> (String, String) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        (format!("{}", PDFJS_LIB), format!("{}", PDFJS_WORKER))
    }
    #[cfg(target_arch = "wasm32")]
    {
        (PDFJS_CDN_LIB.to_string(), PDFJS_CDN_WORKER.to_string())
    }
}

/// A JavaScript string literal, so a path with a quote or a backslash in it —
/// which every Windows path has — cannot break the call.
fn js(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// Wraps a call so it waits for the viewer to have been loaded.
///
/// The script tag is put into the head by an effect, and a call can be made
/// before the browser has run it. Rather than have every caller worry about
/// that, the wait is here: a tenth of a second at worst, once per window.
fn call(body: &str) -> String {
    format!(
        "for (var i = 0; i < 100 && !window.cantaraPdf; i++) {{ \
             await new Promise(function (r) {{ setTimeout(r, 20); }}); \
         }}\n\
         if (!window.cantaraPdf) return {{ error: 'the PDF viewer was not loaded' }};\n\
         {body}"
    )
}

/// The bytes of a PDF, wherever the build keeps them.
fn read_pdf(path: &str) -> Option<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    {
        crate::logic::settings::RepositoryType::web_read_file(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(path).ok()
    }
}

/// Makes sure the window holds the document, reading and handing it over if it
/// does not.
///
/// The file crosses into the page once per window. Several callers may ask at
/// the same time — the presenter console's overview mounts every slide of a
/// document at once — and only the first of them pays: the rest find the
/// document already being fetched and wait for it, rather than each sending
/// their own copy of the same megabytes.
pub async fn ensure_open(path: &str) -> bool {
    let already = document::eval(&call(&format!(
        "return {{ open: window.cantaraPdf.isOpen({0}), opening: window.cantaraPdf.isOpening({0}) }};",
        js(path)
    )))
    .await;

    match already {
        Ok(state) => {
            if state.get("open").and_then(|open| open.as_bool()) == Some(true) {
                return true;
            }
            // Someone else is fetching it. Wait for them and look again.
            if state.get("opening").and_then(|opening| opening.as_bool()) == Some(true) {
                let waited = document::eval(&call(&format!(
                    "for (var i = 0; i < 300 && !window.cantaraPdf.isOpen({0}); i++) {{ \
                         await new Promise(function (r) {{ setTimeout(r, 20); }}); \
                     }}\n\
                     return {{ open: window.cantaraPdf.isOpen({0}) }};",
                    js(path)
                )))
                .await;
                if waited
                    .ok()
                    .and_then(|state| state.get("open").and_then(|open| open.as_bool()))
                    == Some(true)
                {
                    return true;
                }
            }
        }
        Err(error) => {
            log::error!("could not reach the PDF viewer for {path}: {error:?}");
            return false;
        }
    }

    let Some(bytes) = read_pdf(path) else {
        log::warn!("could not read the PDF {path}");
        return false;
    };

    let (pdfjs, worker) = pdfjs_urls();
    let handover = document::eval(&call(&format!(
        "return await window.cantaraPdf.open({}, {}, {}, {});",
        js(path),
        js(&BASE64.encode(&bytes)),
        js(&pdfjs),
        js(&worker),
    )))
    .await;

    match handover {
        Ok(result) if result.get("ok").and_then(|ok| ok.as_bool()) == Some(true) => true,
        Ok(result) => {
            log::error!(
                "could not open the PDF {path}: {}",
                result
                    .get("error")
                    .and_then(|error| error.as_str())
                    .unwrap_or("unknown")
            );
            false
        }
        Err(error) => {
            log::error!("could not open the PDF {path}: {error:?}");
            false
        }
    }
}

/// Draws a page onto a canvas that is on screen.
///
/// `transition` is the CSS class of the effect the slide should arrive with,
/// started at the moment the page appears. The canvas is not rebuilt between
/// the pages of one document — that is what keeps the previous page up until
/// the new one has been drawn — so there is no new element to start an
/// animation by itself.
pub async fn show(canvas_id: &str, path: &str, page: u32, transition: &str) -> bool {
    if !ensure_open(path).await {
        return false;
    }
    let drawn = document::eval(&call(&format!(
        "return await window.cantaraPdf.show({}, {}, {}, {});",
        js(canvas_id),
        js(path),
        page,
        js(transition)
    )))
    .await;

    match drawn {
        Ok(result) => result.get("drawn").and_then(|drawn| drawn.as_bool()) == Some(true),
        Err(error) => {
            log::error!("could not draw page {page} of {path}: {error:?}");
            false
        }
    }
}

/// Builds the scrolling document of the detail view over a container of empty
/// canvases.
pub async fn setup_scroll(container_id: &str, path: &str) -> bool {
    if !ensure_open(path).await {
        return false;
    }
    let built = document::eval(&call(&format!(
        "return await window.cantaraPdf.setupScroll({}, {});",
        js(container_id),
        js(path)
    )))
    .await;

    match built {
        Ok(result) => result.get("ok").and_then(|ok| ok.as_bool()) == Some(true),
        Err(error) => {
            log::error!("could not build the scrolling view of {path}: {error:?}");
            false
        }
    }
}

/// How wide a page is drawn for anything that is not being displayed.
///
/// The width a slide has on a full-HD screen, which is what a deck exported
/// from this presentation is meant to look like.
pub const EXPORT_WIDTH: u32 = 1920;

/// A page of a PDF as a `data:` URL, drawn with nothing on screen.
///
/// This is the operation that makes a page usable away from the presentation.
/// The pptx export needs every slide as a picture whether or not it is being
/// shown, and streaming will want the same; neither has a canvas to draw into
/// and neither should have to put one on screen to get a picture out.
///
/// Rendering happens in the window's own page, so a window has to exist — on
/// the desktop and on the web there always is one. It does *not* have to be
/// visible: a page renders just as well while the window is behind another one
/// or minimised, which is what the frame fallback in `pdf_viewer.js` is for.
pub async fn page_image(path: &str, page: u32, width: u32) -> Option<String> {
    if !ensure_open(path).await {
        return None;
    }
    let rendered = document::eval(&call(&format!(
        "return await window.cantaraPdf.pageImage({}, {}, {});",
        js(path),
        page,
        width
    )))
    .await;

    match rendered {
        Ok(result) => match result.get("data").and_then(|data| data.as_str()) {
            Some(data) => Some(data.to_string()),
            None => {
                log::warn!(
                    "could not render page {page} of {path}: {}",
                    result
                        .get("error")
                        .and_then(|error| error.as_str())
                        .unwrap_or("no picture came back")
                );
                None
            }
        },
        Err(error) => {
            log::error!("could not render page {page} of {path}: {error:?}");
            None
        }
    }
}

/// A picture slide's path, split into the document and the page it names.
///
/// A PDF added to a presentation becomes a picture slide whose path carries the
/// page as a fragment — `handout.pdf#page=2`. Anything else is an ordinary
/// picture and has no page.
pub fn pdf_page_of(path: &str) -> Option<(String, u32)> {
    let document = path.split('#').next().unwrap_or(path).to_string();
    if !document.to_lowercase().ends_with(".pdf") {
        return None;
    }
    let page = path
        .split("#page=")
        .nth(1)
        .and_then(|number| number.parse().ok())
        .unwrap_or(1);
    Some((document, page))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path with a backslash or a quote in it — which is to say every path on
    /// Windows — must not be able to break the call it is put into.
    #[test]
    fn a_path_cannot_break_the_call_it_goes_into() {
        assert_eq!(js(r"C:\Songs\a.pdf"), r#""C:\\Songs\\a.pdf""#);
        assert_eq!(js(r#"od"d.pdf"#), r#""od\"d.pdf""#);
    }

    /// The page a picture slide names, which is how a PDF reaches a
    /// presentation.
    #[test]
    fn a_picture_slide_names_a_document_and_a_page() {
        assert_eq!(
            pdf_page_of("handout.pdf#page=2"),
            Some(("handout.pdf".to_string(), 2))
        );
        // No page named is the first one.
        assert_eq!(
            pdf_page_of("handout.PDF"),
            Some(("handout.PDF".to_string(), 1))
        );
    }

    /// An ordinary picture is not a PDF page and must not be treated as one.
    #[test]
    fn a_picture_that_is_not_a_pdf_names_no_page() {
        assert_eq!(pdf_page_of("background.png"), None);
        assert_eq!(pdf_page_of(""), None);
    }

    /// Every call waits for the viewer rather than assuming the script tag has
    /// been run — it is put into the head by an effect, and a call can come
    /// first.
    #[test]
    fn a_call_waits_for_the_viewer() {
        let body = call("return 1;");

        assert!(body.contains("window.cantaraPdf"));
        assert!(body.ends_with("return 1;"));
    }
}
