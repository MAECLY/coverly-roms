/* What moves, once, and only towards the finished page the HTML already
   shows. theme.js sets html.anim in <head> (no reduced motion, no
   Save-Data); this file adds html.motion once its setup worked. Any error,
   or reduced motion turned on mid-visit, runs halt(): every stop() puts
   its piece in the finished state and both classes go. Also: the
   third-party request counter in the footer. */
(function () {
  "use strict";

  var html = document.documentElement;
  var stops = [];

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

  function onScreen(el) {
    var r = el.getBoundingClientRect();
    return r.width > 0 && r.bottom > 0 && r.top < innerHeight;
  }

  function once(target, threshold, fn) {
    var io = new IntersectionObserver(function (entries) {
      if (!entries.some(function (e) { return e.isIntersecting; })) return;
      io.disconnect();
      fn();
    }, { threshold: threshold });
    io.observe(target);
    return io;
  }

  function timers() {
    var list = [];
    var t = function (fn, ms, stop) {
      list.push(setTimeout(function () {
        try { fn(); } catch (x) { stop(); }
      }, ms));
    };
    t.clear = function () { list.forEach(clearTimeout); list = []; };
    return t;
  }

  /* The terminal types each command in turn; a line shows when its turn
     comes. The untyped rest stays in place, hidden, so no glyph moves.
     Skipped when it is already on screen. */
  function terminal() {
    var term = document.querySelector(".term");
    var steps = term ? term.querySelectorAll(".t-step") : [];
    if (!steps.length || onScreen(term)) return;
    var cmds = [], texts = [], later = timers();
    var done = document.createElement("span"), rest = document.createElement("span");
    rest.className = "t-rest";
    for (var k = 0; k < steps.length; k++) {
      cmds[k] = steps[k].querySelector(".t-cmd");
      texts[k] = cmds[k].textContent;
    }
    var stop = function () {
      later.clear();
      for (var j = 0; j < steps.length; j++) {
        cmds[j].textContent = texts[j];
        steps[j].classList.remove("t-wait");
      }
      term.classList.remove("typing");
    };
    stops.push(stop);
    var type = function (s, i) {
      if (s >= steps.length) {
        term.classList.remove("typing");
        return later(function () { cmds[s - 1].textContent = texts[s - 1]; }, 1400, stop);
      }
      var text = texts[s];
      if (i === 0) {
        if (s) cmds[s - 1].textContent = texts[s - 1];
        steps[s].classList.remove("t-wait");
        cmds[s].textContent = "";
        cmds[s].appendChild(done);
        cmds[s].appendChild(rest);
      }
      done.textContent = text.slice(0, i);
      rest.textContent = text.slice(i);
      if (i >= text.length) return later(function () { type(s + 1, 0); }, 420, stop);
      later(function () { type(s, i + 1); }, 18 + ((i * 37) % 17) + (text.charAt(i) === " " ? 30 : 0), stop);
    };
    term.classList.add("typing");
    for (var m = 0; m < steps.length; m++) steps[m].classList.add("t-wait");
    once(term, 0.45, function () { later(function () { type(0, 0); }, 300, stop); });
  }

  /* Already on screen at load: shown, not played. */
  function reveals() {
    var items = document.querySelectorAll("[data-reveal]");
    for (var i = 0; i < items.length; i++) {
      var el = items[i];
      if (onScreen(el)) el.classList.add("is-in", "no-anim");
      else once(el, 0.35, el.classList.add.bind(el.classList, "is-in"));
    }
  }

  /* Steps 3 and 4 act out what their screenshot shows (SVG drawn over
     the still; step 4's cover is vector art from the sprite). Pinned, the
     figures share one spot, so the step's text decides when; ↻ plays it
     again. */
  function clip(fig) {
    var step = fig.closest(".step"), button = step.querySelector(".clip-replay");
    var later = timers(), cl = step.classList;
    var ready = function (on) {
      if (on) button.removeAttribute("aria-disabled");
      else button.setAttribute("aria-disabled", "true");
    };
    var stop = function () { later.clear(); cl.remove("is-playing"); cl.add("is-played"); ready(true); };
    stops.push(stop);
    var play = function () {
      if (button.getAttribute("aria-disabled")) return;
      cl.remove("is-playing", "is-played");
      void fig.offsetWidth;
      cl.add("is-playing");
      ready(false);
      later(function () { cl.add("is-played"); ready(true); }, +fig.getAttribute("data-clip-ms") || 3600, stop);
    };
    var pinned = getComputedStyle(step).display === "contents";
    once(pinned ? step.querySelector(".step-text") : fig, pinned ? 0.6 : 0.5, function () {
      try { play(); } catch (x) { stop(); }
    });
    button.addEventListener("click", function () {
      try { play(); } catch (x) { stop(); }
    });
  }

  /* shelf.js listens for "cr-halt" and finishes its pieces too. */
  function halt() {
    stops.forEach(function (s) { try { s(); } catch (x) {} });
    try { document.dispatchEvent(new Event("cr-halt")); } catch (x) {}
    html.classList.remove("anim", "motion");
  }

  counter();
  if (!html.classList.contains("anim") || !window.IntersectionObserver || !window.matchMedia) return html.classList.remove("anim");
  try {
    terminal();
    reveals();
    Array.prototype.forEach.call(document.querySelectorAll("[data-clip]"), clip);
    html.classList.add("motion");
  } catch (x) {
    halt();
  }

  /* Motion turned off mid-visit. */
  var still = matchMedia("(prefers-reduced-motion: reduce)");
  var off = function () { if (still.matches) halt(); };
  if (still.addEventListener) still.addEventListener("change", off);
  else if (still.addListener) still.addListener(off);
})();
