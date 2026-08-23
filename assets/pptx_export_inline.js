// Builds a .pptx from the deck description produced by `src/logic/pptx.rs`.
//
// This file deliberately knows nothing about songs, slides or fonts: it walks
// the JSON and calls PptxGenJS. Every decision about what a slide should look
// like is made on the Rust side, where it can be tested without a browser.
//
// Two modes, because the two targets save files in completely different ways:
//
//   "download" — the web build. PptxGenJS hands the file to the browser.
//   "base64"   — the desktop build. The bytes come back to Rust, which writes
//                them where the user's file dialog points. The desktop WebView
//                silently drops the `<a download>` click that `writeFile` uses,
//                so the download mode looks like it worked and produces nothing.
//
// Dioxus runs this inside an async function, so the object returned at the end
// arrives back in Rust as the result of `document::eval(..).await`.
//
// The Rust side substitutes the __PLACEHOLDER__ tokens before evaluating this.
var deck = __DECK__;
var fileName = __FILE_NAME__;
var mode = __MODE__;

try {
    // 1. Load PptxGenJS once; the promise is cached so a second export
    //    does not fetch it again.
    if (typeof window.PptxGenJS === 'undefined') {
        if (!window.__pptxgenInitPromise) {
            window.__pptxgenInitPromise = new Promise(function (resolve, reject) {
                var script = document.createElement('script');
                script.src = __PPTXGEN_URL__;
                script.onload = function () { resolve(); };
                script.onerror = function () {
                    // Let the next attempt try again rather than caching the failure.
                    window.__pptxgenInitPromise = null;
                    reject(new Error('could not load PptxGenJS from ' + script.src));
                };
                document.head.appendChild(script);
            });
        }
        await window.__pptxgenInitPromise;
    }

    if (typeof window.PptxGenJS === 'undefined') {
        return { ok: false, error: 'PptxGenJS loaded but did not register itself' };
    }

    // 2. Build the deck.
    var pptx = new window.PptxGenJS();

    // PptxGenJS only knows a handful of named layouts, so the size is
    // defined explicitly to match what the Rust side computed.
    pptx.defineLayout({ name: 'CANTARA', width: deck.width, height: deck.height });
    pptx.layout = 'CANTARA';

    deck.slides.forEach(function (slideSpec) {
        var slide = pptx.addSlide();
        slide.background = { color: slideSpec.background };

        slideSpec.shapes.forEach(function (shape) {
            if (shape.kind === 'text') {
                slide.addText(shape.text, {
                    x: shape.x,
                    y: shape.y,
                    w: shape.w,
                    h: shape.h,
                    fontSize: shape.font_size,
                    fontFace: shape.font_face,
                    color: shape.color,
                    bold: shape.bold,
                    italic: shape.italic,
                    align: shape.align,
                    valign: shape.valign,
                    // Keeps a long verse inside its box instead of letting
                    // it run off the slide.
                    shrinkText: shape.shrink_to_fit,
                    // The Rust side already joined the lines; PptxGenJS
                    // turns "\n" into line breaks when this is set.
                    breakLine: true
                });
            } else if (shape.kind === 'image') {
                slide.addImage({
                    data: shape.data,
                    x: shape.x,
                    y: shape.y,
                    w: shape.w,
                    h: shape.h
                });
            } else if (shape.kind === 'media') {
                // PowerPoint plays this itself, from bytes carried inside the
                // .pptx — so the deck still works on a machine that has never
                // seen the original file. Which formats it can play is decided
                // on the Rust side; see `powerpoint_can_play`.
                slide.addMedia({
                    type: 'video',
                    data: shape.data,
                    x: shape.x,
                    y: shape.y,
                    w: shape.w,
                    h: shape.h
                });
            }
        });
    });

    // 3. Hand over the result.
    if (mode === 'base64') {
        var data = await pptx.write({ outputType: 'base64' });
        if (!data) {
            return { ok: false, error: 'PptxGenJS produced no data' };
        }
        return { ok: true, slides: deck.slides.length, data: data };
    }

    await pptx.writeFile({ fileName: fileName });
    return { ok: true, slides: deck.slides.length };
} catch (error) {
    return { ok: false, error: String(error && error.message ? error.message : error) };
}
