/* Runs in <head>, before first paint: colour mode, motion gate, OS hint.
   No-JS visitors get the system mode through CSS alone. */
(function () {
  "use strict";
  var html = document.documentElement;
  var KEY = "coverly:mode";
  var BG = { light: "#f2f3ef", dark: "#121514" }; /* --cr-bg (design/tokens.json) */
  var dark = window.matchMedia ? matchMedia("(prefers-color-scheme: dark)") : null;

  html.classList.add("js");

  function read() {
    try {
      var m = localStorage.getItem(KEY);
      return m === "light" || m === "dark" ? m : "system";
    } catch (e) {
      return "system";
    }
  }
  function resolve(mode) {
    return mode === "system" ? (dark && dark.matches ? "dark" : "light") : mode;
  }
  /* A <picture> follows prefers-color-scheme by itself; a forced mode
     re-points its dark <source> so the screenshots match the page. */
  function pictures(mode, theme) {
    var list = document.querySelectorAll("source[data-dark]");
    for (var i = 0; i < list.length; i++) {
      list[i].media = mode === "system" ? "(prefers-color-scheme: dark)" : theme === "dark" ? "all" : "not all";
    }
  }
  function apply(mode) {
    var theme = resolve(mode);
    html.setAttribute("data-theme", theme);
    html.setAttribute("data-mode", mode);
    var metas = document.querySelectorAll('meta[name="theme-color"]');
    for (var i = 0; i < metas.length; i++) {
      metas[i].content = mode === "system" ? (metas[i].media.indexOf("dark") > -1 ? BG.dark : BG.light) : BG[theme];
    }
    pictures(mode, theme);
    var radios = document.querySelectorAll('input[name="mode"]');
    for (var j = 0; j < radios.length; j++) radios[j].checked = radios[j].value === mode;
  }

  var mode = read();
  apply(mode);

  /* Motion only when nobody asked for less of it. */
  var still = window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches;
  var save = navigator.connection && navigator.connection.saveData;
  if (!still && !save) html.classList.add("anim");

  /* Marks the download card for this computer (CSS only reads it). */
  var ua = navigator.userAgent || "";
  var os = /iPhone|iPad|iPod|Android/.test(ua) ? "mobile" : /Mac/.test(ua) ? "mac" : /Win/.test(ua) ? "win" : /Linux|X11/.test(ua) ? "linux" : "other";
  html.setAttribute("data-os", os);

  function follow() { if (mode === "system") apply(mode); }
  if (dark) {
    if (dark.addEventListener) dark.addEventListener("change", follow);
    else if (dark.addListener) dark.addListener(follow);
  }
  window.addEventListener("storage", function (e) {
    if (e.key === KEY) { mode = read(); apply(mode); }
  });

  function choose(next, x, y) {
    var update = function () {
      mode = next;
      try {
        if (next === "system") localStorage.removeItem(KEY);
        else localStorage.setItem(KEY, next);
      } catch (e) {}
      apply(next);
    };
    var calm = !window.matchMedia || matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!document.startViewTransition || calm || resolve(next) === html.getAttribute("data-theme")) return update();
    html.style.setProperty("--ink-x", x + "px");
    html.style.setProperty("--ink-y", y + "px");
    html.classList.add("vt-ink");
    var end = function () { html.classList.remove("vt-ink"); };
    try {
      document.startViewTransition(update).finished.then(end, end);
    } catch (e) {
      end();
      update();
    }
  }

  document.addEventListener("DOMContentLoaded", function () {
    apply(mode);
    /* The phone section menu closes once a link is chosen. */
    var menu = document.querySelector(".navmenu");
    if (menu) menu.addEventListener("click", function (e) {
      if (e.target.closest("a")) menu.open = false;
    });
    /* The new mode spreads from where it was picked: the pointer, or the
       chosen pill from the keyboard. */
    var at = null, picker = document.querySelector(".mode");
    if (picker) picker.addEventListener("pointerdown", function (e) { at = [e.clientX, e.clientY]; });
    var radios = document.querySelectorAll('input[name="mode"]');
    for (var i = 0; i < radios.length; i++) {
      radios[i].addEventListener("change", function (e) {
        var r = e.target.parentNode.getBoundingClientRect();
        var p = at || [r.left + r.width / 2, r.top + r.height / 2];
        at = null;
        choose(e.target.value, p[0], p[1]);
      });
    }
  });
})();
