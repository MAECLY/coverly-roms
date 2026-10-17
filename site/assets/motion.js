/* What moves, once, and only towards the finished page the HTML already
   shows: the hero shelf fills its five missing covers. Any error removes
   the motion class and leaves that page. Also: the third-party request
   counter in the footer. */
(function () {
  "use strict";

  var html = document.documentElement;

  /* Off-origin http(s) requests seen by resource timing. */
  function counter() {
    var out = document.querySelector(".req-count"), ext = document.querySelector(".proof-ext");
    if (!out || !window.PerformanceObserver || !performance.getEntriesByType) return;
    var seen = {}, n = 0;
    var look = function (list) {
      list.forEach(function (e) {
        try {
          var u = new URL(e.name);
          if (/^https?:$/.test(u.protocol) && u.origin !== location.origin && !seen[e.name]) seen[e.name] = ++n;
        } catch (x) {}
      });
      out.textContent = n;
      if (ext) ext.hidden = n === 0;
    };
    look(performance.getEntriesByType("resource"));
    try {
      new PerformanceObserver(function (l) { look(l.getEntries()); }).observe({ type: "resource", buffered: true });
    } catch (x) {}
  }

  function finish(shelf, gaps, have, total) {
    for (var i = 0; i < gaps.length; i++) gaps[i].classList.add("is-filled");
    if (have) have.textContent = total;
    shelf.classList.add("is-done");
  }

  /* The hero shelf: each dashed slot gets its cover, one after another,
     while the status line counts up to the full library. It waits until
     most of the grid is on screen (on a phone the gaps sit below the
     fold), and gives up and shows the finished shelf after WAIT ms. */
  var WAIT = 10000;
  function shelf() {
    var box = document.querySelector(".shelf");
    if (!box) return;
    var grid = box.querySelector(".shelf-grid") || box;
    var gaps = box.querySelectorAll(".tile.gap");
    var have = box.querySelector(".have");
    var total = box.querySelectorAll(".tile").length;
    if (!gaps.length || performance.now() > 2600) return finish(box, gaps, have, total);
    var timers = [], io = null, done = false;
    var stop = function () {
      if (done) return;
      done = true;
      timers.forEach(clearTimeout);
      if (io) io.disconnect();
      finish(box, gaps, have, total);
    };
    var step = function (i) {
      timers.push(setTimeout(function () {
        try {
          gaps[i].classList.add("is-filled");
          if (have) have.textContent = total - gaps.length + i + 1;
          if (i + 1 < gaps.length) step(i + 1);
          else timers.push(setTimeout(stop, 500));
        } catch (x) { stop(); }
      }, i === 0 ? 600 : 380));
    };
    var play = function () {
      if (io) io.disconnect();
      io = null;
      clearTimeout(timers.shift());
      step(0);
    };
    /* motion.js owns the timing now: drop the CSS fallback that would
       fill the gaps at 4 s whether or not anyone is looking. */
    box.classList.add("is-live");
    if (have) have.textContent = total - gaps.length;
    timers.push(setTimeout(stop, WAIT));
    if (window.IntersectionObserver) {
      io = new IntersectionObserver(function (entries) {
        for (var k = 0; k < entries.length; k++) if (entries[k].intersectionRatio >= 0.6) return play();
      }, { threshold: [0.6] });
      io.observe(grid);
    } else {
      var r = grid.getBoundingClientRect();
      if (r.top >= 0 && r.bottom <= innerHeight) play();
      else stop();
    }
    var still = matchMedia("(prefers-reduced-motion: reduce)");
    var off = function () { if (still.matches) stop(); };
    if (still.addEventListener) still.addEventListener("change", off);
    else if (still.addListener) still.addListener(off);
  }

  counter();
  if (!html.classList.contains("anim") || !window.matchMedia) return html.classList.remove("anim");
  try {
    shelf();
  } catch (x) {
    html.classList.remove("anim");
  }
})();
