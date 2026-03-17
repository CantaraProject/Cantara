(async function() {
    try {
        // 1. Load PDF.js via dynamic import if not already loaded
        if (!window.__pdfjsLib) {
            if (!window.__pdfjsInitPromise) {
                window.__pdfjsInitPromise = import(__PDFJS_URL__).then(function(lib) {
                    lib.GlobalWorkerOptions.workerSrc = __WORKER_URL__;
                    window.__pdfjsLib = lib;
                });
            }
            await window.__pdfjsInitPromise;
        }
        if (!window.__pdfjsLib) {
            console.error('PDF.js failed to initialize');
            return;
        }

        // 2. Decode base64 and cache the parsed PDF document
        if (!window.__pdfDocCache) window.__pdfDocCache = {};
        var cacheKey = __CACHE_KEY__;
        if (!window.__pdfDocCache[cacheKey]) {
            var raw = atob(__BASE64__);
            var arr = new Uint8Array(raw.length);
            for (var i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
            window.__pdfDocCache[cacheKey] = await window.__pdfjsLib.getDocument({ data: arr }).promise;
        }

        // 3. Get the requested page
        var page = await window.__pdfDocCache[cacheKey].getPage(__PAGE_NUM__);

        // 4. Wait two animation frames so the browser has computed layout dimensions
        await new Promise(function(r) {
            requestAnimationFrame(function() { requestAnimationFrame(r); });
        });

        // 5. Find the canvas element
        var canvas = document.getElementById(__CANVAS_ID__);
        if (!canvas) {
            // Canvas was removed from DOM (component unmounted) — silently exit
            return;
        }

        // 6. Cancel any in-progress render for this canvas
        if (!window.__pdfRenderTasks) window.__pdfRenderTasks = {};
        var prevTask = window.__pdfRenderTasks[__CANVAS_ID__];
        if (prevTask) {
            try { prevTask.cancel(); } catch(_) {}
            delete window.__pdfRenderTasks[__CANVAS_ID__];
        }

        // 7. Determine available space from the presentation container
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

        // 8. Scale the page to fit (uniform, no stretching)
        var unscaledVp = page.getViewport({ scale: 1 });
        var scale = Math.min(w / unscaledVp.width, h / unscaledVp.height);
        var vp = page.getViewport({ scale: scale });

        canvas.width = vp.width;
        canvas.height = vp.height;

        // 9. Re-check canvas is still in DOM right before render
        if (!document.getElementById(__CANVAS_ID__)) {
            return;
        }

        // 10. Render, tracking the task so it can be cancelled
        var renderTask = page.render({ canvasContext: canvas.getContext('2d'), viewport: vp });
        window.__pdfRenderTasks[__CANVAS_ID__] = renderTask;
        await renderTask.promise;
        delete window.__pdfRenderTasks[__CANVAS_ID__];
    } catch (e) {
        // Suppress RenderingCancelledException — this is expected during live updates
        if (e && e.name === 'RenderingCancelledException') return;
        console.error('PDF render error:', e);
    }
})();
