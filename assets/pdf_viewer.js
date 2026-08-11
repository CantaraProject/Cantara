// Everything Cantara does with a PDF, living in the page rather than being
// sent to it.
//
// This file is loaded **once per window**, like the presentation's stylesheet
// and its other scripts. What used to happen instead was that the Rust side
// shipped the whole rendering program as source text on every single draw:
// thirteen kilobytes per slide change, and two hundred and seventy across
// twenty-one separate calls to open the presenter console's overview. That is
// what made changing a slide slow and the overview slower. Now Rust says which
// page it wants — `cantaraPdf.show('canvas-id', 'file.pdf', 3)` — and the work
// happens here, where the document already is.
//
// Three things are kept between calls, and keeping them is the whole point:
//
//   __docs    the parsed documents, one per file
//   __pages   pages already drawn, keyed by document, page and width
//   __opening documents currently being fetched, so twenty thumbnails of one
//             file ask Rust for it once between them
//
// The API is deliberately small:
//
//   isOpen(key)                  — is the document here?
//   open(key, base64, urls)      — hand it over; safe to call concurrently
//   pageCount(key)
//   show(canvasId, key, page)    — draw onto a canvas that is on screen
//   pageImage(key, page, width)  — draw a page and give back a `data:` URL,
//                                  with nothing on screen and nothing shown;
//                                  what the pptx export and, later, streaming
//                                  need
//   prefetch(key, pages, width)  — draw pages ahead, in the background
//   setupScroll(containerId, key)— the detail view's scrolling document

(function () {
    if (window.cantaraPdf) return;

    // ── Animation frames in a window that is not being drawn ─────────────────
    //
    // PDF.js draws a page in chunks and asks for the next chunk with
    // `requestAnimationFrame`. A window that is hidden, minimised or wholly
    // covered by another one is given no frames, so the drawing stops wherever
    // it got to and the canvas stays black. Both of Cantara's windows are
    // regularly in that state: the presentation covers the main window on a
    // single screen, and the console is behind it until the moderator brings it
    // forward. It is also the state a window is in while a deck is being
    // exported, where nothing is on screen at all.
    //
    // The timer only ever gets there first when a frame does not arrive, so a
    // window that is being drawn normally keeps its own pace.
    (function installFrameFallback() {
        var request = window.requestAnimationFrame.bind(window);
        var cancel = window.cancelAnimationFrame.bind(window);
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
            handle = request(run);
            timers.set(handle, setTimeout(function () { run(performance.now()); }, 50));
            return handle;
        };
        window.cancelAnimationFrame = function (handle) {
            var timer = timers.get(handle);
            if (timer !== undefined) {
                clearTimeout(timer);
                timers.delete(handle);
            }
            cancel(handle);
        };
    })();

    // ── State ────────────────────────────────────────────────────────────────

    /** Parsed documents, by key (the file's path). */
    var docs = {};
    /** Documents being fetched, by key: a promise that settles when they are. */
    var opening = {};
    /** PDF.js itself, once it has been imported. */
    var lib = null;
    var libLoading = null;
    /** Pages already drawn: `key#page@width` to a canvas holding the page. */
    var pages = new Map();

    /** How many drawn pages are kept. A page at presentation size is several
     *  megabytes, so this is the balance between never drawing a page twice
     *  and holding a whole document in pictures. */
    var PAGE_LIMIT = 16;

    function pageKey(key, page, width) {
        return key + '#' + page + '@' + Math.round(width);
    }

    function remember(cacheKey, drawn) {
        pages.delete(cacheKey);
        pages.set(cacheKey, drawn);
        while (pages.size > PAGE_LIMIT) {
            pages.delete(pages.keys().next().value);
        }
    }

    // ── Loading ──────────────────────────────────────────────────────────────

    async function loadLib(pdfjsUrl, workerUrl) {
        if (lib) return lib;
        if (!libLoading) {
            libLoading = import(pdfjsUrl).then(function (loaded) {
                loaded.GlobalWorkerOptions.workerSrc = workerUrl;
                lib = loaded;
                return lib;
            });
        }
        return await libLoading;
    }

    function bytesOf(base64) {
        var raw = atob(base64);
        var bytes = new Uint8Array(raw.length);
        for (var i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
        return bytes;
    }

    // ── Drawing ──────────────────────────────────────────────────────────────

    /** Draws a page into a canvas of its own, at `width`×`height` at most. */
    async function drawPage(key, page, width, height) {
        var doc = docs[key];
        if (!doc) return null;

        var target = pageKey(key, page, width);
        var cached = pages.get(target);
        if (cached) {
            remember(target, cached);
            return cached;
        }

        var pdfPage = await doc.getPage(page);
        var unscaled = pdfPage.getViewport({ scale: 1 });
        var scale = height
            ? Math.min(width / unscaled.width, height / unscaled.height)
            : width / unscaled.width;
        var viewport = pdfPage.getViewport({ scale: scale });

        var buffer = document.createElement('canvas');
        buffer.width = Math.max(1, Math.round(viewport.width));
        buffer.height = Math.max(1, Math.round(viewport.height));
        await pdfPage.render({
            canvasContext: buffer.getContext('2d'),
            viewport: viewport,
        }).promise;

        var drawn = { canvas: buffer, width: viewport.width, height: viewport.height };
        remember(target, drawn);
        return drawn;
    }

    /** Puts a finished page onto a canvas that is on screen, in one go.
     *
     *  A canvas is cleared the moment its `width` is assigned, so drawing into
     *  the one being looked at means showing nothing until the page is
     *  finished — a blink of empty screen in front of an audience. The page is
     *  drawn beside it and copied across when it is complete, so what is on
     *  screen goes straight from the old page to the new one. */
    function present(canvas, drawn) {
        canvas.style.width = Math.round(drawn.width) + 'px';
        canvas.style.height = Math.round(drawn.height) + 'px';
        canvas.width = drawn.canvas.width;
        canvas.height = drawn.canvas.height;
        canvas.getContext('2d').drawImage(drawn.canvas, 0, 0);
        canvas.style.visibility = 'visible';
    }

    /** The slide a canvas sits in, which is what decides how big its page is
     *  drawn. Only a real slide: falling back to the parent would mean
     *  measuring a box that is itself sized by the canvas. */
    function slideOf(canvas) {
        return canvas.closest('.presentation');
    }

    /** The space a canvas has, from the slide it sits in. */
    function spaceFor(canvas) {
        var box = slideOf(canvas) || canvas.parentElement;
        var width = box ? box.clientWidth : 0;
        var height = box ? box.clientHeight : 0;
        if (box && width > 0 && height > 0) {
            var style = getComputedStyle(box);
            width -= (parseFloat(style.paddingLeft) || 0) + (parseFloat(style.paddingRight) || 0);
            height -= (parseFloat(style.paddingTop) || 0) + (parseFloat(style.paddingBottom) || 0);
        }
        if (width <= 0) width = window.innerWidth || 800;
        if (height <= 0) height = window.innerHeight || 600;
        return { width: width, height: height };
    }

    /** Draws the page again when the slide it is in changes size.
     *
     *  A page is drawn for the room it has, and that room is not always known
     *  when the drawing is asked for: the presenter console's overview builds
     *  its thumbnails before the stylesheet that lays the grid out has
     *  arrived, so the slide is still a fraction of its final size and the
     *  page came out as a stamp in the middle of it. It stayed that way until
     *  something asked for the page again — paging on, or reopening the view.
     *
     *  Only the slide is watched, never the canvas's own parent: the canvas is
     *  sized from what is measured here, so measuring a box that the canvas
     *  sizes would be a loop. The size the last drawing was made for is kept,
     *  so the answer the observer gives on its very first call — the size that
     *  was just used — draws nothing a second time. */
    function redrawWhenResized(canvas, space, redraw) {
        canvas.__cantaraRedraw = redraw;
        canvas.__cantaraDrawnFor = space;

        if (canvas.__cantaraResizeWatched) return;

        var slide = slideOf(canvas);
        if (!slide || typeof ResizeObserver === 'undefined') return;
        canvas.__cantaraResizeWatched = true;

        var pending = null;
        var observer = new ResizeObserver(function () {
            if (!canvas.isConnected) {
                observer.disconnect();
                canvas.__cantaraResizeWatched = false;
                if (pending) clearTimeout(pending);
                return;
            }
            var now = spaceFor(canvas);
            var was = canvas.__cantaraDrawnFor || { width: 0, height: 0 };
            // A pixel either way is not worth redrawing a page for; a slide
            // that has just been laid out differs by far more than that.
            if (Math.abs(now.width - was.width) < 2 && Math.abs(now.height - was.height) < 2) {
                return;
            }
            // Drawn once the size has come to rest. The overview's thumbnail
            // slider changes it continuously, and a page drawn for every step
            // of it would be a dozen renderings nobody sees.
            if (pending) clearTimeout(pending);
            pending = setTimeout(function () {
                pending = null;
                if (!canvas.isConnected) return;
                canvas.__cantaraDrawnFor = spaceFor(canvas);
                if (canvas.__cantaraRedraw) canvas.__cantaraRedraw();
            }, 120);
        });
        observer.observe(slide);
    }

    // ── The API ──────────────────────────────────────────────────────────────

    window.cantaraPdf = {
        isOpen: function (key) {
            return !!docs[key];
        },

        pageCount: function (key) {
            return docs[key] ? docs[key].numPages : 0;
        },

        /** Whether someone is already fetching this document.
         *
         *  The Rust side asks before reading a file off disk and encoding it:
         *  the overview mounts every slide of a document at once, and without
         *  this each of them would hand the page its own copy of the same
         *  megabytes. */
        isOpening: function (key) {
            return !!opening[key];
        },

        /** Hands a document over. Safe to call while another call for the same
         *  document is still running — the second one waits for the first
         *  rather than parsing it again. */
        open: async function (key, base64, pdfjsUrl, workerUrl) {
            if (docs[key]) return { ok: true, pages: docs[key].numPages };
            if (opening[key]) {
                try { await opening[key]; } catch (e) { /* fall through and try */ }
                if (docs[key]) return { ok: true, pages: docs[key].numPages };
            }

            opening[key] = (async function () {
                var pdfjs = await loadLib(pdfjsUrl, workerUrl);
                docs[key] = await pdfjs.getDocument({ data: bytesOf(base64) }).promise;
            })();

            try {
                await opening[key];
                return { ok: true, pages: docs[key].numPages };
            } catch (e) {
                console.error('cantaraPdf.open: ' + e);
                return { ok: false, error: String(e && e.message ? e.message : e) };
            } finally {
                delete opening[key];
            }
        },

        /** Draws a page onto a canvas that is on screen.
         *
         *  Returns `{ missing: true }` when the document is not here, which is
         *  the caller's signal to hand it over and ask again. */
        show: async function (canvasId, key, page, transition) {
            try {
                if (!docs[key]) return { missing: true };

                var canvas = document.getElementById(canvasId);
                if (!canvas) return { drawn: false };

                var space = spaceFor(canvas);
                var drawn = await drawPage(key, page, space.width, space.height);
                if (!drawn) return { missing: true };

                // The canvas may have gone while the page was being drawn.
                var target = document.getElementById(canvasId);
                if (!target) return { drawn: false };
                present(target, drawn);

                // What was just drawn fits the slide as it is *now*. Should
                // the slide turn out to be a different size a moment later —
                // the overview's grid settling, a window being resized, the
                // thumbnail slider being moved — the page is drawn again for
                // the size it then has. Without a transition: it is the same
                // page, only sharper, and replaying the effect would look like
                // a slide change that did not happen.
                redrawWhenResized(target, space, function () {
                    window.cantaraPdf.show(canvasId, key, page, '');
                });

                // The canvas is not rebuilt between the pages of one document
                // — that is what keeps the page that is up there until the next
                // one has been drawn, so there is never an empty screen in
                // front of an audience. But a CSS animation only runs when the
                // element carrying it is created, so the effect the user chose
                // is started by hand here, at the moment the new page appears.
                // No effect chosen means nothing happens at all.
                if (transition) {
                    target.classList.remove(transition);
                    // Reading the layout is what makes the browser treat the
                    // class as newly added rather than never removed.
                    void target.offsetWidth;
                    target.classList.add(transition);
                }

                // The pages either side, drawn while this one is being looked
                // at, so moving to the next slide costs one `drawImage`.
                window.cantaraPdf.prefetch(key, [page + 1, page - 1], space.width, space.height);
                return { drawn: true };
            } catch (e) {
                if (e && e.name === 'RenderingCancelledException') return { drawn: false };
                console.error('cantaraPdf.show: ' + e);
                return { drawn: false, error: String(e && e.message ? e.message : e) };
            }
        },

        /** A page as a `data:` URL, drawn with nothing on screen.
         *
         *  This is what makes a page usable away from the presentation: the
         *  pptx export needs every slide as a picture whether or not it is
         *  being shown, and streaming will want the same. Nothing here touches
         *  the document being displayed, so it can be called at any time —
         *  including while the window is in the background, which is what the
         *  frame fallback above is for. */
        pageImage: async function (key, page, width) {
            try {
                if (!docs[key]) return { missing: true };
                var drawn = await drawPage(key, page, width, 0);
                if (!drawn) return { missing: true };
                return { data: drawn.canvas.toDataURL('image/png') };
            } catch (e) {
                console.error('cantaraPdf.pageImage: ' + e);
                return { error: String(e && e.message ? e.message : e) };
            }
        },

        /** Draws pages ahead of time. Deliberately not awaited by its caller:
         *  the page on screen is finished, and this is what the time until the
         *  next slide change is for. */
        prefetch: function (key, wanted, width, height) {
            var doc = docs[key];
            if (!doc || window.__cantaraPdfPrefetching) return;

            var todo = wanted.filter(function (page) {
                return page >= 1 && page <= doc.numPages
                    && !pages.has(pageKey(key, page, width));
            });
            if (todo.length === 0) return;

            window.__cantaraPdfPrefetching = true;
            (async function () {
                try {
                    for (var i = 0; i < todo.length; i++) {
                        await drawPage(key, todo[i], width, height);
                    }
                } catch (e) {
                    // Nothing depends on this having worked.
                } finally {
                    window.__cantaraPdfPrefetching = false;
                }
            })();
        },

        /** Turns a container of empty canvases into a scrolling document.
         *
         *  What a reader expects a document to be: one page under the next in a
         *  column, the way PDF.js's own viewer shows it. Only the pages near
         *  the viewport are drawn — a scanned score runs to hundreds — and they
         *  are drawn again at the right size when the pane is resized.
         *
         *  Deliberately driven by scroll and resize rather than by an
         *  `IntersectionObserver`, whose callbacks are not delivered to a
         *  window that is not being drawn. */
        setupScroll: async function (containerId, key) {
            try {
                var doc = docs[key];
                if (!doc) return { missing: true };

                var container = document.getElementById(containerId);
                if (!container) return { ok: false, error: 'the container is gone' };

                if (!window.__cantaraPdfScrolls) window.__cantaraPdfScrolls = {};
                var previous = window.__cantaraPdfScrolls[containerId];
                if (previous) {
                    try { container.removeEventListener('scroll', previous.onScroll); } catch (_) { }
                    try { previous.resizeObserver.disconnect(); } catch (_) { }
                }
                var view = { onScroll: null, resizeObserver: null, drawing: new Set() };
                window.__cantaraPdfScrolls[containerId] = view;

                var canvases = Array.prototype.slice.call(
                    container.querySelectorAll('canvas[data-page]')
                );

                // A canvas remembers the width it was drawn at so it is not
                // drawn again for nothing. That mark belongs to the document it
                // came from: showing a different one means drawing every page
                // afresh, however wide the column is.
                if (container.dataset.doc !== key) {
                    container.dataset.doc = key;
                    canvases.forEach(function (canvas) { delete canvas.dataset.drawnAt; });
                }

                function columnWidth() {
                    var style = getComputedStyle(container);
                    return Math.max(1, Math.floor(
                        container.clientWidth
                        - (parseFloat(style.paddingLeft) || 0)
                        - (parseFloat(style.paddingRight) || 0)
                    ));
                }

                // Every page gets its place before anything is drawn, so the
                // column has its full height at once and the scrollbar does not
                // jump as the pages arrive. The first page's shape stands in for
                // all of them — a document's pages are almost always the same
                // size, and asking two hundred times would hold the view up for
                // a layout that each page corrects as it is drawn.
                var first = await doc.getPage(1);
                var shape = first.getViewport({ scale: 1 });
                canvases.forEach(function (canvas) {
                    canvas.style.width = '100%';
                    if (!canvas.dataset.drawnAt) {
                        canvas.style.aspectRatio = shape.width + ' / ' + shape.height;
                    }
                });

                async function drawInto(canvas) {
                    var page = parseInt(canvas.dataset.page, 10);
                    var width = columnWidth();
                    if (canvas.dataset.drawnAt === String(width)) return;
                    if (view.drawing.has(page)) return;
                    view.drawing.add(page);
                    try {
                        var drawn = await drawPage(key, page, width, 0);
                        if (!drawn || !canvas.isConnected) return;
                        canvas.width = drawn.canvas.width;
                        canvas.height = drawn.canvas.height;
                        canvas.style.width = '100%';
                        canvas.style.height = 'auto';
                        canvas.style.aspectRatio = drawn.width + ' / ' + drawn.height;
                        canvas.getContext('2d').drawImage(drawn.canvas, 0, 0);
                        canvas.dataset.drawnAt = String(width);
                    } catch (e) {
                        if (!e || e.name !== 'RenderingCancelledException') {
                            console.error('cantaraPdf.setupScroll: page ' + page + ': ' + e);
                        }
                    } finally {
                        view.drawing.delete(page);
                    }
                }

                function drawVisible() {
                    var box = container.getBoundingClientRect();
                    var margin = box.height;
                    canvases.forEach(function (canvas) {
                        var page = canvas.getBoundingClientRect();
                        if (page.bottom >= box.top - margin && page.top <= box.bottom + margin) {
                            drawInto(canvas);
                        }
                    });
                }

                drawVisible();

                var scrollTimer = null;
                view.onScroll = function () {
                    if (scrollTimer) return;
                    scrollTimer = setTimeout(function () {
                        scrollTimer = null;
                        drawVisible();
                    }, 80);
                };
                container.addEventListener('scroll', view.onScroll, { passive: true });

                // Watched on the column itself rather than on the window: the
                // pane is one of three that share the width and can change
                // without the window doing so.
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
                console.error('cantaraPdf.setupScroll: ' + e);
                return { ok: false, error: String(e && e.message ? e.message : e) };
            }
        },
    };
})();
