// Keeps animation frames arriving in a window that is not being drawn.
//
// PDF.js draws a page in chunks and asks for the next chunk with
// `requestAnimationFrame`. A window that is hidden, minimised or wholly
// covered by another one is given no frames, so the drawing stops wherever it
// got to and the canvas stays black. Both of Cantara's windows are regularly
// in that state: the presentation covers the main window on a single screen,
// and the console is behind it until the moderator brings it forward.
//
// This is what left most of the presenter console's grid empty. It looked
// selective because it is: a page simple enough to be drawn in one chunk never
// asks for a second frame and appears, while every page with more on it stops
// half-drawn. A title slide came through, a page of bullet points did not.
//
// The timer only ever gets there first when a frame does not arrive, so a
// window that is being drawn normally keeps its own pace.
//
// Prepended to every script that draws a PDF — see `with_pdf_document` — so
// that it cannot be missed by one of them, which is exactly how the detail
// view's scrolling document came to open on a column of empty pages.
try {
    if (!window.__frameFallbackInstalled) {
        window.__frameFallbackInstalled = true;
        var __nativeRequestFrame = window.requestAnimationFrame.bind(window);
        var __nativeCancelFrame = window.cancelAnimationFrame.bind(window);
        var __frameTimers = new Map();
        window.requestAnimationFrame = function (callback) {
            var handle;
            var fired = false;
            var run = function (time) {
                if (fired) return;
                fired = true;
                var timer = __frameTimers.get(handle);
                if (timer !== undefined) {
                    clearTimeout(timer);
                    __frameTimers.delete(handle);
                }
                callback(time);
            };
            handle = __nativeRequestFrame(run);
            __frameTimers.set(handle, setTimeout(function () { run(performance.now()); }, 50));
            return handle;
        };
        window.cancelAnimationFrame = function (handle) {
            var timer = __frameTimers.get(handle);
            if (timer !== undefined) {
                clearTimeout(timer);
                __frameTimers.delete(handle);
            }
            __nativeCancelFrame(handle);
        };
    }
} catch (e) {
    console.error('frame_fallback_inline: ' + e);
}
