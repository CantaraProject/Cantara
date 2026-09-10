// Fits a slide into a box that is not the size of a screen.
//
// A slide's type is set in *points*, which are absolute: put the renderer into
// a smaller box and the words come out at their full size and overflow it. So
// the slide is laid out at the presentation's own size and the whole thing is
// shrunk with a transform — the same trick every other preview in Cantara
// uses.
//
// The factor cannot be worked out in Rust: the box is a share of a window
// whose size is the operator's business, and on the network a share of a phone
// nobody here can measure. It was CSS for a while —
// `scale(min(calc(100cqw / var(--slide-width)), …))` — which is exactly right
// and is dropped by any engine without container-query *division*. Cantara's
// own web view is one of them, and a dropped declaration means no scaling at
// all: the slide overflowed its box and the next one beside it was drawn at
// full size.
//
// So it is measured. This runs in two places and is one file for that reason:
// the window evaluates it when a slide frame mounts, and the page served to
// the network runs it after it has been given new markup. The arithmetic is
// the same arithmetic, and a second copy of it would be a second thing to get
// wrong.
(function () {
  function fit(frame) {
    var stage = frame.querySelector('.monitor-slide-stage');
    if (!stage) return;

    var style = getComputedStyle(frame);
    // The presentation's own size, as the markup states it. Falling back to
    // 16:9 at 1920 rather than to nothing: a frame with no size named is
    // better drawn at a guess than left overflowing.
    var slideWidth = parseFloat(style.getPropertyValue('--slide-width')) || 1920;
    var slideHeight = parseFloat(style.getPropertyValue('--slide-height')) || 1080;

    var width = frame.clientWidth;
    var height = frame.clientHeight;

    // Not laid out yet.
    //
    // Asking again on the next frame rather than waiting to be told: a box may
    // get its size without its *observed* size changing in a way that fires an
    // observer, and a slide left unscaled is a wall of letters. Bounded, so a
    // frame that never gets a size — one on a slide that has been left — does
    // not ask for ever.
    if (!width || !height) {
      var tries = (parseInt(frame.dataset.cantaraTries, 10) || 0) + 1;
      frame.dataset.cantaraTries = tries;
      if (tries < 60 && frame.isConnected) {
        requestAnimationFrame(function () { fit(frame); });
      }
      return;
    }
    delete frame.dataset.cantaraTries;

    // The smaller of the two ratios is the one that fits in both directions,
    // which is what keeps the proportions.
    var scale = Math.min(width / slideWidth, height / slideHeight);

    stage.style.transformOrigin = 'top left';
    stage.style.transform = 'scale(' + scale + ')';
    // Centred in whichever direction has room left over, so a 4:3 slide in a
    // 16:9 box does not sit against the left edge.
    stage.style.left = Math.max(0, (width - slideWidth * scale) / 2) + 'px';
    stage.style.top = Math.max(0, (height - slideHeight * scale) / 2) + 'px';
  }

  function fitAll() {
    document.querySelectorAll('.monitor-slide-frame').forEach(fit);
  }

  // Watched rather than measured once.
  //
  // A frame is not always laid out in the tick it was put on the page: asked
  // then, it reports no size, and a scale computed from nothing is nothing.
  // Measuring on a timer would be guessing at how long to wait. A
  // `ResizeObserver` is told the moment the box *has* a size, which is exactly
  // the moment worth measuring — and again whenever it changes, so a window
  // dragged to another screen refits without anything having to notice.
  //
  // One observer for the page, however many times this file is evaluated: the
  // window evaluates it per slide frame, and an observer per frame per slide
  // change would pile up over a service.
  var observer = window.__cantaraSlideObserver;
  if (!observer && typeof ResizeObserver === 'function') {
    observer = new ResizeObserver(function (entries) {
      entries.forEach(function (entry) { fit(entry.target); });
    });
    window.__cantaraSlideObserver = observer;
  }

  if (observer) {
    document.querySelectorAll('.monitor-slide-frame').forEach(function (frame) {
      // Observing the same element twice would call back twice; the flag says
      // this one is already watched.
      if (frame.dataset.cantaraObserved) return;
      frame.dataset.cantaraObserved = 'yes';
      observer.observe(frame);
    });
  } else {
    // No observer to be had. Falling back to the window's own resize is worse
    // — it says nothing about a box that has just appeared — but it is better
    // than never measuring at all.
    if (!window.__cantaraSlideScale) {
      window.__cantaraSlideScale = true;
      window.addEventListener('resize', fitAll);
    }
  }

  // And now, for anything already laid out, so a slide that is ready does not
  // wait for a callback.
  fitAll();
})();
