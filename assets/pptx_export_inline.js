// Writes a .pptx from the deck description produced by `src/logic/pptx.rs`.
//
// This file deliberately knows nothing about songs, slides or fonts: it walks
// the JSON and calls PptxGenJS. Every decision about what a slide should look
// like is made on the Rust side, where it can be tested without a browser.
//
// The Rust side substitutes the __PLACEHOLDER__ tokens before evaluating this.
(async function () {
    var deck = __DECK__;
    var fileName = __FILE_NAME__;

    function fail(reason) {
        window.__cantaraPptxResult = { ok: false, error: reason };
        console.error('pptx_export_inline: ' + reason);
    }

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
            fail('PptxGenJS loaded but did not register itself');
            return;
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
                }
            });
        });

        // 3. Hand the file to the browser's download machinery.
        await pptx.writeFile({ fileName: fileName });
        window.__cantaraPptxResult = { ok: true, slides: deck.slides.length };
    } catch (error) {
        fail(String(error && error.message ? error.message : error));
    }
})();
