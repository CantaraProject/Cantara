// Draws one page of an already-loaded PDF onto a canvas.
//
// This script carries no document data. The bytes are handed over once by
// `pdf_load_inline.js` and kept in `window.__pdfDocCache`; every later slide of
// the same file only sends this short script. That matters: a presentation
// changes slides constantly, and shipping a multi-megabyte base64 string
// through the IPC each time is what made long PDFs crawl.
//
// Returns `{ missing: true }` when the document is not in the cache and this
// canvas is the one that should fetch it, and `{ waiting: true }` when another
// canvas is already doing so and this one should try again. Both are signals to
// the caller; see there.
//
// The Rust side substitutes the __PLACEHOLDER__ tokens before evaluating this.
try {
    var cacheKey = __CACHE_KEY__;
    var canvasId = __CANVAS_ID__;
    var pageNum = __PAGE_NUM__;

    // How long a request for a document is believed to be on its way. Past
    // this, whoever asks next fetches it instead — see below.
    var REQUEST_TTL = 4000;

    // How many drawn pages are kept. A page is a few megabytes at presentation
    // size, so this is the balance between never redrawing a page the audience
    // has already seen and holding a whole document in pictures.
    var PAGE_CACHE_LIMIT = 12;

    if (!window.__pdfDocCache) window.__pdfDocCache = {};
    if (!window.__pdfDocRequests) window.__pdfDocRequests = {};
    if (!window.__pdfDocWaiters) window.__pdfDocWaiters = {};
    // Pages that have already been drawn, keyed by document, page and the size
    // they were drawn at.
    if (!window.__pdfPages) window.__pdfPages = new Map();

    // ── Drawing ─────────────────────────────────────────────────────────────

    // Puts a finished page onto the visible canvas in one go.
    //
    // A canvas is cleared the moment its `width` is assigned, so drawing into
    // the one on screen means showing the background until the page is
    // finished — which for a slide change is a blink of empty screen in front
    // of the audience. The page is therefore drawn into a canvas of its own
    // and only copied across once it is complete.
    function present(canvas, drawn) {
        canvas.style.width = Math.round(drawn.cssWidth) + 'px';
        canvas.style.height = Math.round(drawn.cssHeight) + 'px';
        canvas.width = drawn.canvas.width;
        canvas.height = drawn.canvas.height;
        canvas.getContext('2d').drawImage(drawn.canvas, 0, 0);
        // The canvas is transparent until there is a page on it, and comes up
        // over the slide being held behind it. A page that has to be rendered
        // arrives a moment after the slide changed, and appearing all at once
        // at that point is a jolt in the middle of a presentation; this way it
        // is the end of the crossfade rather than an event of its own.
        canvas.style.opacity = '1';
    }

    function pageKey(page, width) {
        return cacheKey + '#' + page + '@' + Math.round(width);
    }

    function remember(key, drawn) {
        window.__pdfPages.delete(key);
        window.__pdfPages.set(key, drawn);
        while (window.__pdfPages.size > PAGE_CACHE_LIMIT) {
            var oldest = window.__pdfPages.keys().next().value;
            window.__pdfPages.delete(oldest);
        }
    }

    // Draws `page` at `width`×`height` device pixels into a canvas of its own.
    async function draw(doc, number, width, height, taskKey) {
        var page = await doc.getPage(number);
        var unscaled = page.getViewport({ scale: 1 });
        var scale = Math.min(width / unscaled.width, height / unscaled.height);
        var viewport = page.getViewport({ scale: scale });

        var buffer = document.createElement('canvas');
        buffer.width = Math.max(1, Math.round(viewport.width));
        buffer.height = Math.max(1, Math.round(viewport.height));

        if (!window.__pdfRenderTasks) window.__pdfRenderTasks = {};
        var previous = window.__pdfRenderTasks[taskKey];
        if (previous) {
            try { previous.cancel(); } catch (_) { }
        }
        var task = page.render({
            canvasContext: buffer.getContext('2d'),
            viewport: viewport,
        });
        window.__pdfRenderTasks[taskKey] = task;
        await task.promise;
        if (window.__pdfRenderTasks[taskKey] === task) {
            delete window.__pdfRenderTasks[taskKey];
        }

        return { canvas: buffer, cssWidth: viewport.width, cssHeight: viewport.height };
    }

    // ── Is the document here? ───────────────────────────────────────────────

    if (!window.__pdfjsLib || !window.__pdfDocCache[cacheKey]) {
        // The grid of the presenter console mounts every slide of a document
        // at once. Left alone, each of those canvases asks Rust for a copy of
        // the file, and twenty copies of a scanned score cross the IPC, get
        // decoded and get parsed into twenty separate documents — which is
        // why most of that grid stayed empty. Only the first canvas asks; the
        // rest wait to be told the document has arrived.
        //
        // What is recorded is the *time* of the request rather than a promise
        // to wait on. A canvas can be unmounted at any moment — every slide
        // change does it — and a promise its loader never settles would leave
        // every other canvas of that document waiting for good. A timestamp
        // cannot do that: it goes stale, and the next canvas to look takes
        // the job over.
        var now = Date.now();
        var asked = window.__pdfDocRequests[cacheKey];
        if (!asked || now - asked >= REQUEST_TTL) {
            window.__pdfDocRequests[cacheKey] = now;
            return { missing: true };
        }

        // Wait to be woken by `pdf_load_inline.js` rather than asking again
        // and again. Twenty canvases polling the page several times a second
        // is a great deal of traffic across the IPC at exactly the moment the
        // document is being carried the other way.
        var waiters = window.__pdfDocWaiters[cacheKey];
        if (!waiters) {
            waiters = [];
            window.__pdfDocWaiters[cacheKey] = waiters;
        }
        await new Promise(function (resolve) {
            var settled = false;
            var wake = function () {
                if (settled) return;
                settled = true;
                resolve();
            };
            waiters.push(wake);
            // Never wait past the point at which the request counts as stale,
            // so that a loader which never delivers costs one wait and not a
            // canvas that stays blank.
            setTimeout(wake, REQUEST_TTL);
        });

        if (!window.__pdfjsLib || !window.__pdfDocCache[cacheKey]) {
            return { waiting: true };
        }
    }

    var doc = window.__pdfDocCache[cacheKey];

    // ── The canvas and the space it has ─────────────────────────────────────

    var canvas = document.getElementById(canvasId);
    if (!canvas) {
        return { rendered: false };
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

    if (w <= 0 || h <= 0) {
        // The box has no size yet. One frame is usually all it takes; a window
        // that is not being drawn never gets there, which is what the fallback
        // above is for.
        await new Promise(function (r) { requestAnimationFrame(function () { r(); }); });
        w = el ? el.clientWidth : 0;
        h = el ? el.clientHeight : 0;
    }
    if (w <= 0) w = window.innerWidth || 800;
    if (h <= 0) h = window.innerHeight || 600;

    var key = pageKey(pageNum, w);

    // A page that has been drawn at this size already goes up at once. This is
    // the ordinary case for a slide change: the page after the current one was
    // drawn ahead of time, below, so moving to it costs a single `drawImage`
    // and the audience sees no gap at all.
    var cached = window.__pdfPages.get(key);
    if (cached) {
        present(canvas, cached);
        remember(key, cached);
        prerender(w, h);
        return { rendered: true, cached: true };
    }

    var drawn = await draw(doc, pageNum, w, h, canvasId);
    remember(key, drawn);

    // The canvas may have gone while the page was being drawn.
    var target = document.getElementById(canvasId);
    if (!target) {
        return { rendered: false };
    }
    present(target, drawn);
    prerender(w, h);
    return { rendered: true };

    // Draws the pages either side of this one into the cache, so that the next
    // slide is already there when it is called for. Deliberately not awaited:
    // the page on screen is finished, and this is what the time until the next
    // slide change is for.
    function prerender(width, height) {
        if (window.__pdfPrerendering) return;
        var neighbours = [pageNum + 1, pageNum - 1].filter(function (number) {
            return number >= 1 && number <= doc.numPages
                && !window.__pdfPages.has(pageKey(number, width));
        });
        if (neighbours.length === 0) return;

        window.__pdfPrerendering = true;
        (async function () {
            try {
                for (var i = 0; i < neighbours.length; i++) {
                    var number = neighbours[i];
                    var ahead = await draw(doc, number, width, height, 'prerender');
                    remember(pageKey(number, width), ahead);
                }
            } catch (e) {
                // Nothing depends on this having worked.
            } finally {
                window.__pdfPrerendering = false;
            }
        })();
    }
} catch (e) {
    // Expected while slides change quickly; the newer render wins.
    if (e && e.name === 'RenderingCancelledException') {
        return { rendered: false };
    }
    console.error('pdf_render_inline: ' + e);
    return { rendered: false, error: String(e && e.message ? e.message : e) };
}
