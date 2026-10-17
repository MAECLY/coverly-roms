/* The showcase choreography (docs/07-build-and-release/landing-showcase-spec.md).
   Runs after motion.js and only when it set html.motion (no reduced motion,
   no Save-Data, setup worked). It only toggles classes; CSS does the moving.
   Every module registers a stop() that puts its piece in the finished state
   the HTML already shows, so halt() means "finish", never "cancel". motion.js
   halts with a "cr-halt" event; reduced motion turned on mid-visit halts too. */
/* WP4, the #seguro round trip: user-started, so it also runs under
   reduced motion (instant swaps). HTML = applied; this arms "before". */
(function () {
  "use strict";
  var box = document.querySelector(".demo"), ctl = box && box.querySelector(".dm-ctl");
  if (!ctl) return;
  var out = box.querySelector(".demo-status"), bs = ctl.querySelectorAll("button"), now = "is-applied";
  var next = { "is-before": "sim", "is-sim": "apply", "is-applied": "undo", "is-undo": "sim" };
  var to = { sim: "is-sim", apply: "is-applied", undo: "is-undo" };
  var set = function (st, b) {
    box.classList.remove(now);
    box.classList.add(now = st);
    Array.prototype.forEach.call(bs, function (x) {
      x.hidden = false;
      x.setAttribute("aria-pressed", x === b ? "true" : "false");
      if (x.getAttribute("data-s") === next[st]) x.removeAttribute("aria-disabled");
      else x.setAttribute("aria-disabled", "true");
    });
    out.textContent = (b || ctl).getAttribute("data-say");
  };
  ctl.addEventListener("click", function (e) {
    var b = e.target.closest("button");
    if (b && !b.getAttribute("aria-disabled")) set(to[b.getAttribute("data-s")], b);
  });
  set("is-before");
})();
(function () {
  "use strict";
  var html = document.documentElement;
  if (!html.classList.contains("motion") || !window.IntersectionObserver) return;
  var stops = [];
  function halt() {
    stops.splice(0).forEach(function (s) { try { s(); } catch (x) {} });
  }
  document.addEventListener("cr-halt", halt);
  var still = matchMedia("(prefers-reduced-motion: reduce)");
  var off = function () { if (still.matches) halt(); };
  if (still.addEventListener) still.addEventListener("change", off);
  else if (still.addListener) still.addListener(off);

  /* One shared observer. when(el, ratio, fn) runs fn once, as soon as that
     share of el is on screen (at once if it already is). Art that leaves the
     screen gets [data-off], which pauses its animations (CSS). */
  var waits = [];
  var io = new IntersectionObserver(function (entries) {
    entries.forEach(function (e) {
      var el = e.target;
      if (el.matches("svg.art")) {
        if (e.isIntersecting) el.removeAttribute("data-off");
        else el.setAttribute("data-off", "");
      }
      waits = waits.filter(function (w) {
        if (w.el !== el || !e.isIntersecting || e.intersectionRatio < w.ratio) return true;
        try { w.fn(); } catch (x) { halt(); }
        return false;
      });
    });
  }, { threshold: [0, 0.25, 0.4, 0.5, 0.75] });
  function when(el, ratio, fn) {
    waits.push({ el: el, ratio: ratio, fn: fn });
    io.unobserve(el);
    io.observe(el);
  }
  Array.prototype.forEach.call(document.querySelectorAll("svg.art"), function (el) { io.observe(el); });

  /* WP2, the Lean-To: .is-armed shows the start frame, .play runs the CSS
     timeline once (the wide scene or the mobile strip, whichever is seen
     first), and both go at the end, the frame the HTML shows. A slow load
     gets the finished shelf. ↻ replays; it stays put and keeps focus. */
  function hero(when, stops) {
    var box = document.querySelector(".hero"), arts = box ? box.querySelectorAll(".hero-art") : [];
    if (!arts.length) return;
    var cl = box.classList, replay = box.querySelector(".replay"), t = 0, went = 0;
    var ready = function (on) {
      if (!replay) return;
      if (on) replay.removeAttribute("aria-disabled");
      else replay.setAttribute("aria-disabled", "true");
    };
    var stop = function () {
      clearTimeout(t);
      cl.remove("is-armed", "play");
      cl.add("is-played");
      ready(true);
    };
    var play = function () {
      went = 1;
      clearTimeout(t);
      cl.remove("play");
      cl.add("is-armed");
      void box.offsetWidth;
      cl.add("play");
      ready(false);
      t = setTimeout(stop, 2900);
    };
    stops.push(stop);
    if (performance.now() > 4000) return stop();
    cl.add("is-armed");
    Array.prototype.forEach.call(arts, function (a) {
      when(a, 0.4, function () { if (!went) play(); });
    });
    if (replay) replay.addEventListener("click", function () {
      if (replay.getAttribute("aria-disabled")) return;
      try { play(); } catch (x) { stop(); }
    });
  }

  /* Modules, one per work package: hero() WP2, demo() WP4, waiting() WP5,
     finale() WP6. Each: find its piece, stops.push(stop), when(...). */
  var modules = [hero];
  try {
    modules.forEach(function (m) { m(when, stops); });
  } catch (x) {
    halt();
  }
})();
