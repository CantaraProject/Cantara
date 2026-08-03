// A "morph" transition between slides.
//
// The other transitions fade or slide the whole new slide in, so a line that
// appears on both slides is destroyed and rebuilt. This one transforms instead:
// text that occurs on the old *and* the new slide travels from where it was to
// where it now belongs, growing or shrinking on the way, while only the text
// that genuinely changed fades. That is the same idea as PowerPoint's Morph or
// Keynote's Magic Move.
//
// It hooks the DOM rather than the Rust side: the renderer replaces
// `.slide-container` wholesale (it is keyed by slide number), so a
// MutationObserver sees every slide change without the renderer having to
// announce it.

(function () {
    if (window.__cantaraMorphInstalled) return;
    window.__cantaraMorphInstalled = true;

    var DURATION = 420;
    var EASING = 'cubic-bezier(0.2, 0, 0, 1)';
    // The elements that carry text. Keeping this list short keeps the matching
    // meaningful: a wrapper that contains everything would always "match".
    var TEXT_SELECTOR = 'p, .complex-slide-row';
    var MORPH_CLASS = 'presentation-morph';

    // What is currently on screen, so the next slide can morph out of it.
    var previous = null;

    function textKey(element) {
        return (element.textContent || '').replace(/\s+/g, ' ').trim();
    }

    function snapshot(container) {
        var boxes = new Map();
        container.querySelectorAll(TEXT_SELECTOR).forEach(function (element) {
            var key = textKey(element);
            if (!key) return;
            // First occurrence wins; a repeated line has no single origin.
            if (!boxes.has(key)) boxes.set(key, element.getBoundingClientRect());
        });
        return {
            boxes: boxes,
            html: container.outerHTML,
            rect: container.getBoundingClientRect()
        };
    }

    // A copy of the outgoing slide, laid over the new one so the change is a
    // transformation rather than a gap.
    function makeGhost(stage, snap) {
        var host = document.createElement('div');
        host.className = 'presentation-morph-ghost';
        var stageRect = stage.getBoundingClientRect();
        host.style.cssText =
            'position:absolute;pointer-events:none;z-index:3;' +
            'left:' + (snap.rect.left - stageRect.left) + 'px;' +
            'top:' + (snap.rect.top - stageRect.top) + 'px;' +
            'width:' + snap.rect.width + 'px;height:' + snap.rect.height + 'px;';
        host.innerHTML = snap.html;

        // The copy carries the ids of the elements it was made from. Two
        // elements with one id would send `getElementById` to whichever comes
        // first — and the notation and PDF renderers look their canvas up that
        // way, so a lingering copy could swallow their output.
        host.querySelectorAll('[id]').forEach(function (element) {
            element.removeAttribute('id');
        });

        stage.appendChild(host);
        return host;
    }

    function morph(stage, container, snap) {
        var ghost = makeGhost(stage, snap);
        var matched = new Set();

        container.querySelectorAll(TEXT_SELECTOR).forEach(function (element) {
            var key = textKey(element);
            var from = key ? snap.boxes.get(key) : undefined;
            var to = element.getBoundingClientRect();

            if (!from || to.width === 0 || to.height === 0) {
                // New text: it has nowhere to travel from, so it just arrives.
                element.animate(
                    [{ opacity: 0, transform: 'translateY(0.4em)' }, { opacity: 1, transform: 'none' }],
                    { duration: DURATION, easing: EASING, fill: 'backwards' }
                );
                return;
            }

            matched.add(key);

            // FLIP: start the element where the old one was, then let it settle
            // into its real position.
            var dx = from.left - to.left;
            var dy = from.top - to.top;
            var sx = from.width / to.width;
            var sy = from.height / to.height;

            element.animate(
                [
                    {
                        transformOrigin: 'top left',
                        transform: 'translate(' + dx + 'px,' + dy + 'px) scale(' + sx + ',' + sy + ')'
                    },
                    { transformOrigin: 'top left', transform: 'none' }
                ],
                { duration: DURATION, easing: EASING, fill: 'backwards' }
            );
        });

        // In the ghost, hide whatever travelled — it is being drawn by the real
        // element now — and fade out only the text that is genuinely gone.
        ghost.querySelectorAll(TEXT_SELECTOR).forEach(function (element) {
            if (matched.has(textKey(element))) {
                element.style.visibility = 'hidden';
            }
        });

        function drop() {
            if (ghost.parentNode) ghost.parentNode.removeChild(ghost);
        }

        var fade = ghost.animate([{ opacity: 1 }, { opacity: 0 }], {
            duration: DURATION,
            easing: EASING
        });
        fade.onfinish = drop;
        // A cancelled animation must not leave the copy on screen either.
        fade.oncancel = drop;
        // And neither must a window that never animates: an occluded window
        // does not tick its animations, so the copy would sit there at full
        // opacity over the live slide.
        setTimeout(drop, DURATION + 100);
    }

    function handle(container) {
        var stage = container.closest('.presentation');
        if (!stage) return;

        // Any leftover copy from a slide change that was interrupted.
        stage.querySelectorAll('.presentation-morph-ghost').forEach(function (old) {
            old.parentNode.removeChild(old);
        });

        var wanted = container.classList.contains(MORPH_CLASS);
        if (wanted && previous) {
            morph(stage, container, previous);
        }

        previous = snapshot(container);
    }

    var observer = new MutationObserver(function (records) {
        for (var i = 0; i < records.length; i++) {
            var added = records[i].addedNodes;
            for (var j = 0; j < added.length; j++) {
                var node = added[j];
                if (node.nodeType !== 1) continue;
                if (node.classList && node.classList.contains('slide-container')) {
                    handle(node);
                    return;
                }
            }
        }
    });

    observer.observe(document.body, { childList: true, subtree: true });

    // The first slide is already there when this script runs.
    var initial = document.querySelector('.slide-container');
    if (initial) previous = snapshot(initial);
})();
