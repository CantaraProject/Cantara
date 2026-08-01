// Renders one ABC notation snippet into a container using abcjs.
//
// Cantara's song library hands out each notation row as a complete ABC tune
// (its own X:/M:/L:/K: header plus one `w:` lyrics line per system), so this
// script only has to load the library and draw it.
//
// The Rust side substitutes the __PLACEHOLDER__ tokens before evaluating this.
(async function () {
    var containerId = __CONTAINER_ID__;

    function container() {
        return document.getElementById(containerId);
    }

    function fail(reason) {
        var element = container();
        if (!element) return;
        element.innerHTML = '';
        // The lyrics are shown in their own rows underneath, so an empty staff
        // is less disruptive on a projector than a wall of raw ABC source.
        element.setAttribute('data-abc-error', reason);
        console.error('abc_render_inline: ' + reason);
    }

    try {
        // 1. Load abcjs once. The promise is cached on `window` so that the
        //    several notation rows of a slide share a single load.
        if (typeof window.ABCJS === 'undefined') {
            if (!window.__abcjsInitPromise) {
                window.__abcjsInitPromise = new Promise(function (resolve, reject) {
                    var script = document.createElement('script');
                    script.src = __ABCJS_URL__;
                    script.onload = function () { resolve(); };
                    script.onerror = function () {
                        // Let the next slide try again rather than caching the failure.
                        window.__abcjsInitPromise = null;
                        reject(new Error('could not load abcjs from ' + script.src));
                    };
                    document.head.appendChild(script);
                });
            }
            await window.__abcjsInitPromise;
        }

        if (typeof window.ABCJS === 'undefined') {
            fail('abcjs loaded but did not register itself');
            return;
        }

        // 2. Wait two animation frames so the browser has laid the container
        //    out — abcjs sizes the staff to the width it finds.
        await new Promise(function (resolve) {
            requestAnimationFrame(function () { requestAnimationFrame(resolve); });
        });

        var element = container();
        if (!element) {
            // The slide moved on while we were loading; nothing to draw into.
            return;
        }

        element.removeAttribute('data-abc-error');
        element.innerHTML = '';

        window.ABCJS.renderAbc(element, __ABC_NOTATION__, {
            responsive: 'resize',
            paddingtop: 0,
            paddingbottom: 0,
            paddingleft: 0,
            paddingright: 0,
            staffwidth: element.clientWidth > 0 ? element.clientWidth : 700,
            // The lyrics belong to the staff, so they scale with it.
            format: {
                vocalfont: __VOCAL_FONT__
            }
        });

        // abcjs draws in black; a presentation may well be light on dark, so
        // the colour is inherited from the surrounding slide instead.
        element.querySelectorAll('path, rect, text, tspan').forEach(function (node) {
            if (node.getAttribute('fill') !== 'none') {
                node.setAttribute('fill', 'currentColor');
            }
            if (node.getAttribute('stroke') && node.getAttribute('stroke') !== 'none') {
                node.setAttribute('stroke', 'currentColor');
            }
        });
    } catch (error) {
        fail(String(error && error.message ? error.message : error));
    }
})();
