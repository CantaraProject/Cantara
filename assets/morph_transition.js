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
    // Everything a slide is made of, not only its paragraphs. A picture that
    // is on both slides should travel and grow into its new place like a line
    // of text does — and so should a heading, a notation block and a page of a
    // PDF. Kept to the things that *are* content: a wrapper containing
    // everything would always "match" and nothing would ever move.
    var TEXT_SELECTOR =
        'p, .complex-slide-row, h1, h2, h3, h4, h5, h6, li, img, canvas, .notation-block';

    // How alike two lines have to be to count as the same one moving.
    //
    // The point of the morph is to resolve the *difference* between two
    // slides: a verse whose last word changes should slide across and settle,
    // not be thrown away and drawn again. Matching only on text that is
    // identical meant almost nothing ever matched between two slides of one
    // song, which is precisely where the effect is supposed to earn its keep.
    var SIMILAR_ENOUGH = 0.45;
    var MORPH_CLASS = 'presentation-morph';

    // What is currently on screen, so the next slide can morph out of it.
    var previous = null;

    // What identifies an element across two slides.
    ///
    // A picture is its source and a canvas is its kind: neither has text, and
    // both are the same thing on both slides when they carry the same file.
    function textKey(element) {
        var tag = element.tagName;
        if (tag === 'IMG') return 'img:' + (element.getAttribute('src') || '').slice(0, 120);
        if (tag === 'CANVAS') return 'canvas';
        return (element.textContent || '').replace(/\s+/g, ' ').trim();
    }

    function words(key) {
        return key.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);
    }

    // How much two lines have in common, from 0 to 1.
    function likeness(a, b) {
        if (a === b) return 1;
        var left = words(a);
        var right = words(b);
        if (left.length === 0 || right.length === 0) return 0;
        var pool = right.slice();
        var shared = 0;
        for (var i = 0; i < left.length; i++) {
            var at = pool.indexOf(left[i]);
            if (at >= 0) {
                shared++;
                pool.splice(at, 1);
            }
        }
        return (2 * shared) / (left.length + right.length);
    }

    // The line on the old slide this one came from, if any.
    ///
    // An exact match first — a line that is simply still there. Failing that,
    // the most alike line that nothing else has claimed, which is what lets a
    // verse that has changed by a word travel rather than blink.
    function origin(key, snap, taken) {
        if (snap.boxes.has(key) && !taken.has(key)) return key;
        var best = null;
        var bestScore = SIMILAR_ENOUGH;
        snap.boxes.forEach(function (_box, candidate) {
            if (taken.has(candidate)) return;
            var score = likeness(key, candidate);
            if (score > bestScore) {
                bestScore = score;
                best = candidate;
            }
        });
        return best;
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
            var came_from = key ? origin(key, snap, matched) : null;
            var from = came_from ? snap.boxes.get(came_from) : undefined;
            var to = element.getBoundingClientRect();

            if (!from || to.width === 0 || to.height === 0) {
                // New text: it has nowhere to travel from, so it just arrives.
                element.animate(
                    [{ opacity: 0, transform: 'translateY(0.4em)' }, { opacity: 1, transform: 'none' }],
                    { duration: DURATION, easing: EASING, fill: 'backwards' }
                );
                return;
            }

            matched.add(came_from);

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
