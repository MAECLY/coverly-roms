/* The newest release, read in the browser. pages.yml already fills the page
   with the latest release when it deploys; this only covers the minutes
   before that redeploy (or a failed one). It changes nothing unless every
   download on the page has its file in the newer release, and any error
   leaves the built page as it is. Also: the copy buttons. */
(function () {
  "use strict";
  var BASE = "https://github.com/MAECLY/coverly-roms/releases/";
  var API = "https://api.github.com/repos/MAECLY/coverly-roms/releases/latest";
  var KEY = "cr-release";
  var es = document.documentElement.lang === "es";

  function all(sel) { return document.querySelectorAll(sel); }
  function semver(v) {
    var m = /^(\d+)\.(\d+)\.(\d+)$/.exec(v || "");
    return m ? [+m[1], +m[2], +m[3]] : null;
  }
  function newer(a, b) {
    for (var i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] > b[i];
    return false;
  }

  function apply(rel) {
    if (!rel || rel.draft || rel.prerelease) return;
    var tag = rel.tag_name, v = String(tag).slice(1), n = semver(v);
    if (!n || tag !== "v" + v || rel.html_url !== BASE + "tag/" + tag) return;
    var shown = all('[data-rel="version"]');
    var built = shown.length ? shown[0].textContent : "", b = semver(built);
    if (b && !newer(n, b)) return;
    var files = {}, assets = rel.assets || [], links = all("a[data-asset]"), plan = [], i;
    for (i = 0; i < assets.length; i++) files[assets[i].name] = assets[i];
    for (i = 0; b && i < links.length; i++) {
      var name = links[i].getAttribute("href").split("/").pop().split(built).join(v);
      var f = files[name], url = BASE + "download/" + tag + "/" + name;
      if (!f || f.state !== "uploaded" || !(f.size > 0) || f.browser_download_url !== url) return;
      plan.push([links[i], url, (f.size / 1048576).toFixed(1)]);
    }
    for (i = 0; i < plan.length; i++) {
      var a = plan[i][0], mib = es ? plan[i][2].replace(".", ",") : plan[i][2];
      a.setAttribute("href", plan[i][1]);
      a.textContent = a.textContent.replace(/\d+[.,]\d(?= MiB)/, mib);
    }
    for (i = 0; i < shown.length; i++) shown[i].textContent = v;
    var date = all('[data-rel="date"]'), day = new Date(rel.published_at);
    for (i = 0; i < date.length && !isNaN(day); i++) {
      date[i].textContent = day.toLocaleDateString(es ? "es-ES" : "en-GB",
        { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
    }
    var notes = all('a[data-rel="notes"]');
    for (i = 0; i < notes.length; i++) notes[i].setAttribute("href", rel.html_url);
    var fresh = all("[data-rel-new]");
    for (i = 0; i < fresh.length; i++) fresh[i].hidden = false;
  }

  /* One request per tab every 10 minutes: unauthenticated, GitHub allows
     60 an hour per address. Only what apply() reads is kept. */
  function load() {
    try {
      var c = JSON.parse(sessionStorage.getItem(KEY));
      if (c && Date.now() - c.t < 6e5) return apply(c.r);
    } catch (e) {}
    if (!window.fetch) return;
    var ctl = window.AbortController ? new AbortController() : null;
    var timer = setTimeout(function () { if (ctl) ctl.abort(); }, 8000);
    fetch(API, { headers: { Accept: "application/vnd.github+json" }, signal: ctl && ctl.signal })
      .then(function (res) { return res.status === 404 ? null : res.ok ? res.json() : Promise.reject(); })
      .then(function (r) {
        clearTimeout(timer);
        var keep = r && {
          tag_name: r.tag_name, html_url: r.html_url, draft: r.draft, prerelease: r.prerelease,
          published_at: r.published_at,
          assets: (r.assets || []).map(function (a) {
            return { name: a.name, size: a.size, state: a.state, browser_download_url: a.browser_download_url };
          })
        };
        try { sessionStorage.setItem(KEY, JSON.stringify({ t: Date.now(), r: keep })); } catch (e) {}
        apply(keep);
      })["catch"](function () { clearTimeout(timer); });
  }

  function copies() {
    var list = all("button[data-copy]");
    if (!navigator.clipboard) return;
    for (var i = 0; i < list.length; i++) {
      var btn = list[i], say = document.createElement("span");
      say.className = "sr-only";
      say.setAttribute("role", "status");
      btn.parentNode.appendChild(say);
      btn.hidden = false;
      btn.addEventListener("click", function () {
        var me = this, code = me.parentNode.querySelector("code"), label = me.getAttribute("data-label") || me.textContent;
        me.setAttribute("data-label", label);
        navigator.clipboard.writeText(code.textContent).then(function () {
          me.textContent = me.parentNode.lastChild.textContent = me.getAttribute("data-copy");
          setTimeout(function () { me.textContent = label; me.parentNode.lastChild.textContent = ""; }, 2000);
        }, function () {});
      });
    }
  }

  copies();
  load();
})();
