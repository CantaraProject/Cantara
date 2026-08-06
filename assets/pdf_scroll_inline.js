// Turns a container of empty canvases into a scrolling view of a PDF.
//
// This is what a reader expects a document to be: one page after another in a
// column you scroll through, the way PDF.js's own viewer shows it. The detail
// view used to page through a PDF with a next/previous button, which is a way
// of looking at slides, not at a document.
//
// Every page gets a canvas straight away, sized from the page's own aspect
// ratio so the column has its full height from the start and the scrollbar
// does not jump about while the pages arrive. Only the pages near the viewport
// are actually drawn — a scanned score of two hundred pages would otherwise be
// two hundred bitmaps — and they are drawn again at the right size when the
// pane is resized.
//
// The document itself must already be in `window.__pdfDocCache`;
// `pdf_load_inline.js` puts it there. The Rust side substitutes the
// __PLACEHOLDER__ tokens before evaluating this.
try {
    var cacheKey = __CACHE_KEY__;
    var containerId = __CONTAINER_ID__;

    var doc = window.__pdfDocCache && window.__pdfDocCache[cacheKey];
    if (!doc) {
        return { missing: true };
    }

    var container = document.getElementById(containerId);
    if (!container) {
        return { ok: false, error: 'the container is gone' };
    }

    // Re-evaluating for the same container — the pane was resized, or the view
    // came back — must not leave two observers watching the same pages.
    if (!window.__pdfScrollViews) window.__pdfScrollViews = {};
    var previous = window.__pdfScrollViews[containerId];
    if (previous) {
        try { container.removeEventListener('scroll', previous.onScroll); } catch (_) { }
        try { previous.resizeObserver.disconnect(); } catch (_) { }
    }

    var view = { onScroll: null, resizeObserver: null, rendering: new Set() };
    window.__pdfScrollViews[containerId] = view;

    // The width a page is drawn at: the column's, less whatever padding it has.
    function columnWidth() {
        var style = getComputedStyle(container);
        var width = container.clientWidth
            - (parseFloat(style.paddingLeft) || 0)
            - (parseFloat(style.paddingRight) || 0);
        return Math.max(1, Math.floor(width));
    }

    async function drawPage(canvas) {
        var number = parseInt(canvas.dataset.page, 10);
        var width = columnWidth();
        // Already drawn at this width; nothing to do.
        if (canvas.dataset.drawnAt === String(width)) return;
        if (view.rendering.has(number)) return;
        view.rendering.add(number);

        try {
            var page = await doc.getPage(number);
            var unscaled = page.getViewport({ scale: 1 });
            var viewport = page.getViewport({ scale: width / unscaled.width });

            // Drawn beside the page and copied across when it is finished, so
            // a page that is already on screen is never blanked while it is
            // being drawn again at a new size.
            var buffer = document.createElement('canvas');
            buffer.width = Math.max(1, Math.round(viewport.width));
            buffer.height = Math.max(1, Math.round(viewport.height));
            await page.render({
                canvasContext: buffer.getContext('2d'),
                viewport: viewport,
            }).promise;

            if (!canvas.isConnected) return;
            canvas.width = buffer.width;
            canvas.height = buffer.height;
            canvas.style.width = '100%';
            canvas.style.height = 'auto';
            // The page's own shape, now that it is known, in place of the one
            // borrowed from the first page.
            canvas.style.aspectRatio = viewport.width + ' / ' + viewport.height;
            canvas.getContext('2d').drawImage(buffer, 0, 0);
            canvas.dataset.drawnAt = String(width);
        } catch (e) {
            if (!e || e.name !== 'RenderingCancelledException') {
                console.error('pdf_scroll_inline: page ' + number + ': ' + e);
            }
        } finally {
            view.rendering.delete(number);
        }
    }

    // Give every page its place before anything is drawn, so the column is its
    // full height from the first moment and scrolling to the end does not pull
    // the ground out from under the reader.
    //
    // The shape of the first page stands in for all of them, rather than each
    // page being asked for its own: a document's pages are almost always the
    // same size, and asking two hundred times would hold the view up for the
    // sake of a layout that is corrected anyway the moment a page is drawn.
    var canvases = Array.prototype.slice.call(
        container.querySelectorAll('canvas[data-page]')
    );

    // A canvas remembers the width it was drawn at so it is not drawn again
    // for nothing. That mark belongs to the document it was drawn from: if the
    // view is now showing a different one, every page has to be drawn afresh
    // however wide the column is.
    if (container.dataset.doc !== cacheKey) {
        container.dataset.doc = cacheKey;
        canvases.forEach(function (canvas) {
            delete canvas.dataset.drawnAt;
        });
    }

    var first = await doc.getPage(1);
    var shape = first.getViewport({ scale: 1 });
    canvases.forEach(function (canvas) {
        canvas.style.width = '100%';
        if (!canvas.dataset.drawnAt) {
            canvas.style.aspectRatio = shape.width + ' / ' + shape.height;
        }
    });

    // Draws every page within a screenful of the viewport, worked out from the
    // geometry. A page already drawn at this width is skipped, so this is
    // cheap enough to run on every scroll.
    //
    // Deliberately not an `IntersectionObserver`, which is the obvious tool
    // and the wrong one here: its callbacks are not delivered to a window that
    // is not being drawn — hidden, minimised, covered — so the view opened on
    // a column of empty pages and scrolling through it changed nothing. The
    // same reason the frame fallback exists.
    function drawVisible() {
        var box = container.getBoundingClientRect();
        var margin = box.height;
        canvases.forEach(function (canvas) {
            var page = canvas.getBoundingClientRect();
            if (page.bottom >= box.top - margin && page.top <= box.bottom + margin) {
                drawPage(canvas);
            }
        });
    }

    drawVisible();

    // Keeping up with scrolling. Throttled: a scroll fires far more often than
    // a page can be drawn, and every run measures each page in the column.
    var scrollTimer = null;
    view.onScroll = function () {
        if (scrollTimer) return;
        scrollTimer = setTimeout(function () {
            scrollTimer = null;
            drawVisible();
        }, 80);
    };
    container.addEventListener('scroll', view.onScroll, { passive: true });

    // A resized pane means every drawn page is now the wrong size. Watched on
    // the column itself rather than on the window, because the pane is one of
    // three that share the width and can change without the window doing so.
    // Only the pages on screen are redrawn at once; the rest lose their mark
    // and are redrawn when they are scrolled to.
    var resizeTimer = null;
    view.resizeObserver = new ResizeObserver(function () {
        if (resizeTimer) clearTimeout(resizeTimer);
        resizeTimer = setTimeout(function () {
            var width = String(columnWidth());
            canvases.forEach(function (canvas) {
                if (canvas.dataset.drawnAt !== width) delete canvas.dataset.drawnAt;
            });
            drawVisible();
        }, 150);
    });
    view.resizeObserver.observe(container);

    return { ok: true, pages: doc.numPages };
} catch (e) {
    console.error('pdf_scroll_inline: ' + e);
    return { ok: false, error: String(e && e.message ? e.message : e) };
}
