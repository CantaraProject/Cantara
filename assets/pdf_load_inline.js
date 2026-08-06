// Hands one PDF's bytes to PDF.js and keeps the parsed document on `window`.
//
// This is the only script that carries the file, and it runs once per document
// per window. `pdf_render_inline.js` then draws any page of it without sending
// the bytes again.
//
// The Rust side substitutes the __PLACEHOLDER__ tokens before evaluating this.
try {
    var cacheKey = __CACHE_KEY__;

    // 1. Load PDF.js via dynamic import if it is not there yet. The promise is
    //    cached so several documents share a single load.
    if (!window.__pdfjsLib) {
        if (!window.__pdfjsInitPromise) {
            window.__pdfjsInitPromise = import(__PDFJS_URL__).then(function (lib) {
                lib.GlobalWorkerOptions.workerSrc = __WORKER_URL__;
                window.__pdfjsLib = lib;
            });
        }
        await window.__pdfjsInitPromise;
    }

    if (!window.__pdfjsLib) {
        return { ok: false, error: 'PDF.js failed to initialise' };
    }

    if (!window.__pdfDocCache) window.__pdfDocCache = {};

    // Another slide of the same document may have started loading it while we
    // were waiting for the library; the promise is cached so both wait on one
    // parse instead of doing the work twice.
    if (!window.__pdfDocLoads) window.__pdfDocLoads = {};

    if (!window.__pdfDocCache[cacheKey]) {
        if (!window.__pdfDocLoads[cacheKey]) {
            window.__pdfDocLoads[cacheKey] = (async function () {
                var raw = atob(__BASE64__);
                var arr = new Uint8Array(raw.length);
                for (var i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
                var doc = await window.__pdfjsLib.getDocument({ data: arr }).promise;
                window.__pdfDocCache[cacheKey] = doc;
                delete window.__pdfDocLoads[cacheKey];
                return doc;
            })();
        }
        await window.__pdfDocLoads[cacheKey];
    }

    // The document is here, so the canvases that `pdf_render_inline.js` told
    // to wait can stop waiting. A failed load deliberately leaves the request
    // standing: it expires on its own, and clearing it here would send every
    // one of them after the same unreadable file at once.
    if (window.__pdfDocRequests) delete window.__pdfDocRequests[cacheKey];

    // Wake them rather than leaving them to notice by themselves. This is what
    // turns the presenter console's grid from twenty canvases each asking the
    // page every tenth of a second into twenty that are told once.
    if (window.__pdfDocWaiters) {
        var waiters = window.__pdfDocWaiters[cacheKey] || [];
        delete window.__pdfDocWaiters[cacheKey];
        for (var w = 0; w < waiters.length; w++) {
            try { waiters[w](); } catch (_) { }
        }
    }

    return { ok: true, pages: window.__pdfDocCache[cacheKey].numPages };
} catch (e) {
    delete window.__pdfDocLoads[__CACHE_KEY__];
    console.error('pdf_load_inline: ' + e);
    return { ok: false, error: String(e && e.message ? e.message : e) };
}
