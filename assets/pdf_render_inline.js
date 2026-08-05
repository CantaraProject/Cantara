// Draws one page of an already-loaded PDF onto a canvas.
//
// This script carries no document data. The bytes are handed over once by
// `pdf_load_inline.js` and kept in `window.__pdfDocCache`; every later slide of
// the same file only sends this short script. That matters: a presentation
// changes slides constantly, and shipping a multi-megabyte base64 string
// through the IPC each time is what made long PDFs crawl.
//
// Returns `{ missing: true }` when the document is not in the cache and this
// canvas is the one that should fetch it, and `{ waiting: true }` while another
// canvas is already doing so. Both are signals to the caller; see there.
//
// The Rust side substitutes the __PLACEHOLDER__ tokens before evaluating this.
try {
    var cacheKey = __CACHE_KEY__;
    var canvasId = __CANVAS_ID__;
    var pageNum = __PAGE_NUM__;

    // How long a request for a document is believed to be on its way. Past
    // this, whoever asks next fetches it instead — see below.
    var REQUEST_TTL = 4000;

    // PDF.js draws a page in chunks and asks for the next chunk with
    // `requestAnimationFrame`. A window that is hidden, minimised or wholly
    // covered by another one is given no frames, so the drawing stops
    // wherever it got to and the canvas stays black. Both of Cantara's
    // windows are regularly in that state: the presentation covers the main
    // window on a single screen, and the console is behind it until the
    // moderator brings it forward.
    //
    // This is what left most of the presenter console's grid empty. It looked
    // selective because it is: a page simple enough to be drawn in one chunk
    // never asks for a second frame and appears, while every page with more
    // on it stops half-drawn. A title slide came through, a page of bullet
    // points did not.
    //
    // The timer only ever gets there first when a frame does not arrive, so a
    // window that is being drawn normally keeps its own pace.
    if (!window.__frameFallbackInstalled) {
        window.__frameFallbackInstalled = true;
        var nativeRequest = window.requestAnimationFrame.bind(window);
        var nativeCancel = window.cancelAnimationFrame.bind(window);
        var timers = new Map();
        window.requestAnimationFrame = function (callback) {
            var handle;
            var fired = false;
            var run = function (time) {
                if (fired) return;
                fired = true;
                var timer = timers.get(handle);
                if (timer !== undefined) {
                    clearTimeout(timer);
                    timers.delete(handle);
                }
                callback(time);
            };
            handle = nativeRequest(run);
            timers.set(handle, setTimeout(function () { run(performance.now()); }, 50));
            return handle;
        };
        window.cancelAnimationFrame = function (handle) {
            var timer = timers.get(handle);
            if (timer !== undefined) {
                clearTimeout(timer);
                timers.delete(handle);
            }
            nativeCancel(handle);
        };
    }

    if (!window.__pdfDocRequests) window.__pdfDocRequests = {};

    if (!window.__pdfjsLib || !window.__pdfDocCache || !window.__pdfDocCache[cacheKey]) {
        // The grid of the presenter console mounts every slide of a document
        // at once. Left alone, each of those canvases asks Rust for a copy of
        // the file, and twenty copies of a scanned score cross the IPC, get
        // decoded and get parsed into twenty separate documents — which is
        // why most of that grid stayed empty. Only the first canvas asks; the
        // rest are told to wait and try again.
        //
        // What is recorded is the *time* of the request rather than a promise
        // to wait on. A canvas can be unmounted at any moment — every slide
        // change does it — and a promise its loader never settles would leave
        // every other canvas of that document waiting for good. A timestamp
        // cannot do that: it goes stale, and the next canvas to look takes
        // the job over.
        var now = Date.now();
        var asked = window.__pdfDocRequests[cacheKey];
        if (asked && now - asked < REQUEST_TTL) {
            return { waiting: true };
        }
        window.__pdfDocRequests[cacheKey] = now;
        return { missing: true };
    }

    var page = await window.__pdfDocCache[cacheKey].getPage(pageNum);

    // Wait two animation frames so the browser has computed layout dimensions.
    // A hidden or occluded window never fires them, so the wait gives up after
    // a moment rather than leaving the canvas blank forever.
    await Promise.race([
        new Promise(function (r) {
            requestAnimationFrame(function () { requestAnimationFrame(r); });
        }),
        new Promise(function (r) { setTimeout(r, 200); })
    ]);

    var canvas = document.getElementById(canvasId);
    if (!canvas) {
        // The slide moved on while we were waiting; nothing to draw into.
        return { rendered: false };
    }

    // Cancel any in-progress render for this canvas.
    if (!window.__pdfRenderTasks) window.__pdfRenderTasks = {};
    var prevTask = window.__pdfRenderTasks[canvasId];
    if (prevTask) {
        try { prevTask.cancel(); } catch (_) { }
        delete window.__pdfRenderTasks[canvasId];
    }

    // Determine the available space from the presentation container.
    var el = canvas.closest('.presentation') || canvas.parentElement;
    var w = el ? el.clientWidth : 0;
    var h = el ? el.clientHeight : 0;

    if (el && w > 0 && h > 0) {
        var cs = getComputedStyle(el);
        w -= (parseFloat(cs.paddingLeft) || 0) + (parseFloat(cs.paddingRight) || 0);
        h -= (parseFloat(cs.paddingTop) || 0) + (parseFloat(cs.paddingBottom) || 0);
    }

    if (w <= 0) w = window.innerWidth || 800;
    if (h <= 0) h = window.innerHeight || 600;

    // Scale the page to fit, uniformly — no stretching.
    var unscaledVp = page.getViewport({ scale: 1 });
    var scale = Math.min(w / unscaledVp.width, h / unscaledVp.height);
    var vp = page.getViewport({ scale: scale });

    canvas.width = vp.width;
    canvas.height = vp.height;

    // Re-check the canvas is still in the DOM right before rendering.
    if (!document.getElementById(canvasId)) {
        return { rendered: false };
    }

    var renderTask = page.render({ canvasContext: canvas.getContext('2d'), viewport: vp });
    window.__pdfRenderTasks[canvasId] = renderTask;
    await renderTask.promise;
    delete window.__pdfRenderTasks[canvasId];

    return { rendered: true };
} catch (e) {
    // Expected while slides change quickly; the newer render wins.
    if (e && e.name === 'RenderingCancelledException') {
        return { rendered: false };
    }
    console.error('pdf_render_inline: ' + e);
    return { rendered: false, error: String(e && e.message ? e.message : e) };
}
