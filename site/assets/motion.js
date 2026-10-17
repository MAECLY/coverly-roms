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
     while the status line counts up to the full library. */
  function shelf() {
    var box = document.querySelector(".shelf");
    if (!box) return;
    var gaps = box.querySelectorAll(".tile.gap");
    var have = box.querySelector(".have");
    var total = box.querySelectorAll(".tile").length;
    var r = box.getBoundingClientRect();
    var visible = r.bottom > 0 && r.top < innerHeight;
    if (!gaps.length || !visible || performance.now() > 2600) return finish(box, gaps, have, total);
    var timers = [];
    var stop = function () {
      timers.forEach(clearTimeout);
      finish(box, gaps, have, total);
    };
    if (have) have.textContent = total - gaps.length;
    var step = function (i) {
      timers.push(setTimeout(function () {
        try {
          gaps[i].classList.add("is-filled");
          if (have) have.textContent = total - gaps.length + i + 1;
          if (i + 1 < gaps.length) step(i + 1);
          else timers.push(setTimeout(stop, 500));
        } catch (x) { stop(); }
      }, i === 0 ? 900 : 380));
    };
    step(0);
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
