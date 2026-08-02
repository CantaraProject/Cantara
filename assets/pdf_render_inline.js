// Draws one page of an already-loaded PDF onto a canvas.
//
// This script carries no document data. The bytes are handed over once by
// `pdf_load_inline.js` and kept in `window.__pdfDocCache`; every later slide of
// the same file only sends this short script. That matters: a presentation
// changes slides constantly, and shipping a multi-megabyte base64 string
// through the IPC each time is what made long PDFs crawl.
//
// Returns `{ missing: true }` when the document is not in the cache, which is
// the caller's signal to load it and try again.
//
// The Rust side substitutes the __PLACEHOLDER__ tokens before evaluating this.
try {
    var cacheKey = __CACHE_KEY__;
    var canvasId = __CANVAS_ID__;
    var pageNum = __PAGE_NUM__;

    if (!window.__pdfjsLib || !window.__pdfDocCache || !window.__pdfDocCache[cacheKey]) {
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
