#!/usr/bin/env python3
"""Check the landing page before it ships. Standard library only.

  check_site.py [--site site] [--only files,links,parity,head,budget,contrast,motion]

Fills site/ twice into a temporary folder with apply_release.py (once with
scripts/site/fixtures/release.json, once with --no-release) and runs every
gate on both copies, the way Pages would publish them. Ported, in spirit,
from the Versorium landing checks (tests/landing): the gates that need no
browser.

  files     CNAME, required files, app colour tokens in sync, workflow pins
  links     every internal href/src/srcset resolves, every #fragment exists
  parity    Spanish and English pages have the same structure, and each
            page's FAQPage JSON-LD equals its visible FAQ
  head      CSP, canonical/hreflang, no inline styles or scripts, images
            sized, described and lazy
  budget    page, CSS, script and font sizes (raw and gzip), visible words
  contrast  WCAG 2.x contrast of the token pairs the page uses, both modes
  motion    the motion contract: html.anim/html.motion gates and halt() in
            the scripts, a reduced-motion block that stops everything, no
            keyframe or transition on layout properties, scroll-linked
            effects only inside @supports, motion never hides text or
            images, no autoplaying video, motion.js size

Exit status 1 when any gate fails; every failure is printed.
"""

from __future__ import annotations

import argparse
import gzip
import json
import re
import subprocess
import sys
import tempfile
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]
APPLY = ROOT / "scripts/site/apply_release.py"
FIXTURE = ROOT / "scripts/site/fixtures/release.json"
REPO = "MAECLY/coverly-roms"
HOST = "coverly-roms.maecly.com"
CSP = (
    "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; "
    "font-src 'self'; connect-src 'self'; manifest-src 'self'; object-src 'none'; "
    "base-uri 'self'; form-action 'none'"
)
PAGES = {"es": "index.html", "en": "en/index.html"}
REQUIRED = [
    "index.html", "en/index.html", "404.html", "CNAME", ".nojekyll", "robots.txt",
    "sitemap.xml", "llms.txt", "agents.txt", "site.webmanifest",
    "en/site.webmanifest", "favicon.ico", "assets/site.css", "assets/theme.js",
    "assets/motion.js", "assets/favicon.svg", "assets/icon-192.png",
    "assets/icon-512.png", "assets/apple-touch-icon.png", "assets/og.png",
    "assets/og-en.png", "assets/fonts/archivo-display-800.woff2",
    "assets/fonts/OFL.txt",
]
# Raw bytes / gzip bytes. Each landing page, the stylesheet, all scripts.
BUDGET = {
    "page": (48_000, 12_500),
    "css": (44_000, 10_500),
    "scripts": (16_000, 5_500),
    "font": (14_336, None),
}
# motion.js alone (raw bytes): Versorium keeps 6 KB; the hero story, the
# terminal and the reveals fit in a little more.
MOTION_JS = 8_000
# Versorium keeps 680; this page carries a comparison, a layout table and
# a download matrix, so it gets more room, and the gate keeps it from growing.
MAX_WORDS = 1_450
ACTION_PIN = re.compile(r"uses:\s*[\w./-]+@([0-9a-f]{40})\s+#\s*v\d")


class Page(HTMLParser):
    """Collects what the gates look at from one HTML file."""

    VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link",
            "meta", "source", "track", "wbr"}

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.ids: set[str] = set()
        self.refs: list[tuple[str, str, str]] = []  # (tag, attribute, value)
        self.tags: list[tuple[str, dict[str, str]]] = []
        self.scripts: list[tuple[dict[str, str], str]] = []
        self.headings: list[tuple[str, str]] = []
        self.sections: list[str] = []
        self.summaries: list[str] = []
        self.answers: list[str] = []
        self.words: list[str] = []
        self.styles = 0
        self._script: dict[str, str] | None = None
        self._buf: list[str] = []
        self._heading: str | None = None
        self._summary = False
        self._answer = False
        self._in_details = 0
        self._in_faq = 0
        self._skip = 0

    def handle_starttag(self, tag, attrs):
        a = {k: (v or "") for k, v in attrs}
        self.tags.append((tag, a))
        if "id" in a:
            self.ids.add(a["id"])
        if "style" in a:
            self.styles += 1
        for key in ("href", "src"):
            if key in a:
                self.refs.append((tag, key, a[key]))
        if "srcset" in a:
            for part in a["srcset"].split(","):
                if part.strip():
                    self.refs.append((tag, "srcset", part.strip().split()[0]))
        if tag == "script":
            self._script = a
            self._buf = []
        if tag in ("style", "script", "noscript", "template"):
            self._skip += 1
        if tag in ("h1", "h2", "h3"):
            self._heading = tag
            self._buf = []
        if tag == "section":
            self.sections.append(a.get("id", a.get("class", "")))
        if tag == "div" and "faq" in a.get("class", "").split():
            self._in_faq = 1
        if tag == "details" and self._in_faq:
            self._in_details += 1
        if tag == "summary" and self._in_details:
            self._summary = True
            self._buf = []
        if tag == "p" and self._in_details:
            self._answer = True
            self._buf = []

    def handle_endtag(self, tag):
        if tag == "script" and self._script is not None:
            self.scripts.append((self._script, "".join(self._buf)))
            self._script = None
        if tag in ("style", "script", "noscript", "template"):
            self._skip = max(0, self._skip - 1)
        if tag == self._heading:
            self.headings.append((tag, " ".join("".join(self._buf).split())))
            self._heading = None
        if tag == "summary" and self._summary:
            self.summaries.append(" ".join("".join(self._buf).split()))
            self._summary = False
        if tag == "p" and self._answer:
            self.answers.append(" ".join("".join(self._buf).split()))
            self._answer = False
        if tag == "details":
            self._in_details = max(0, self._in_details - 1)

    def handle_data(self, data):
        if self._script is not None or self._heading or self._summary or self._answer:
            self._buf.append(data)
        if not self._skip:
            self.words.extend(re.findall(r"[\wÀ-ÿ'’.-]+", data))


def parse(path: Path) -> Page:
    p = Page()
    p.feed(path.read_text(encoding="utf-8"))
    return p


class Report:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.notes: list[str] = []

    def fail(self, gate: str, msg: str) -> None:
        self.failures.append(f"[{gate}] {msg}")

    def note(self, gate: str, msg: str) -> None:
        self.notes.append(f"[{gate}] {msg}")


# ── files ──────────────────────────────────────────────────────────────

def tokens(css: str, block: str) -> dict[str, str]:
    m = re.search(re.escape(block) + r"\s*\{(.*?)\}", css, re.S)
    return dict(re.findall(r"(--cr-[\w-]+):\s*([^;]+);", m.group(1))) if m else {}


def gate_files(site: Path, r: Report) -> None:
    for rel in REQUIRED:
        if not (site / rel).is_file():
            r.fail("files", f"missing {rel}")
    cname = (site / "CNAME").read_text(encoding="utf-8").strip() if (site / "CNAME").is_file() else ""
    if cname != HOST:
        r.fail("files", f"CNAME is {cname!r}, not {HOST}")
    app = (ROOT / "src/styles/globals.css").read_text(encoding="utf-8")
    land = (site / "assets/site.css").read_text(encoding="utf-8").replace('"', "'")
    block = re.search(r"/\* tokens:begin.*?\*/(.*?)/\* tokens:end \*/", land, re.S)
    if not block:
        r.fail("files", "site.css has no tokens:begin/end block")
    else:
        want = tokens(app, ":root")
        got = tokens(block.group(1), ":root")
        if want != got:
            r.fail("files", f"site.css tokens drift from the app: {got} != {want}")
        dark_app = tokens(app, ":root[data-theme='dark']")
        dark_land = tokens(land, ":root[data-theme='dark']")
        if dark_app != dark_land:
            r.fail("files", f"dark tokens drift from the app: {dark_land} != {dark_app}")
        media = tokens(land, ":root:not([data-theme='light'])")
        if dark_app != media:
            r.fail("files", f"prefers-color-scheme tokens drift from the app: {media}")
    wf = ROOT / ".github/workflows/pages.yml"
    if wf.is_file():
        for line in wf.read_text(encoding="utf-8").splitlines():
            if line.strip().startswith("- uses:") or line.strip().startswith("uses:"):
                if not ACTION_PIN.search(line):
                    r.fail("files", f"pages.yml action not pinned to a SHA: {line.strip()}")


# ── links ──────────────────────────────────────────────────────────────

def gate_links(site: Path, r: Report) -> None:
    pages = sorted(site.rglob("*.html"))
    ids = {p: parse(p).ids for p in pages}
    for page in pages:
        doc = parse(page)
        for tag, attr, ref in doc.refs:
            if not ref or ref.startswith(("mailto:", "data:")):
                continue
            u = urlsplit(ref)
            if u.scheme in ("http", "https"):
                if u.netloc == HOST and tag == "a":
                    r.fail("links", f"{page.relative_to(site)}: absolute link to this site: {ref}")
                continue
            if u.scheme:
                continue
            if u.path.startswith("/"):
                target = site / unquote(u.path.lstrip("/"))
            elif u.path:
                target = (page.parent / unquote(u.path)).resolve()
            else:
                target = page
            if target.is_dir():
                target = target / "index.html"
            if not target.is_file():
                r.fail("links", f"{page.relative_to(site)}: {attr} {ref} does not exist")
                continue
            if u.fragment and target.suffix == ".html":
                if u.fragment not in ids.get(target.resolve(), ids.get(target, set())) and \
                        u.fragment not in parse(target).ids:
                    r.fail("links", f"{page.relative_to(site)}: #{u.fragment} not in {target.name}")


# ── parity ─────────────────────────────────────────────────────────────

def faq_ld(doc: Page) -> list[tuple[str, str]]:
    for a, body in doc.scripts:
        if a.get("type") != "application/ld+json":
            continue
        data = json.loads(body)
        for node in data.get("@graph", [data]):
            if node.get("@type") == "FAQPage":
                return [(q["name"], q["acceptedAnswer"]["text"]) for q in node["mainEntity"]]
    return []


def shape(doc: Page) -> dict[str, object]:
    count = lambda name: sum(1 for t, _ in doc.tags if t == name)  # noqa: E731
    return {
        "sections": len(doc.sections),
        "h1": [h for h, _ in doc.headings].count("h1"),
        "h2": [h for h, _ in doc.headings].count("h2"),
        "h3": [h for h, _ in doc.headings].count("h3"),
        "faq": len(doc.summaries),
        "pictures": count("picture"),
        "img": count("img"),
        "source": count("source"),
        "a": count("a"),
        "li": count("li"),
        "tr": count("tr"),
        "tiles": sum(1 for t, a in doc.tags if "tile" in a.get("class", "").split()),
        "gaps": sum(1 for t, a in doc.tags if "gap" in a.get("class", "").split()),
        "data-os": sorted(a["data-os"] for t, a in doc.tags if "data-os" in a),
    }


def gate_parity(site: Path, r: Report) -> None:
    docs = {lang: parse(site / rel) for lang, rel in PAGES.items()}
    es, en = shape(docs["es"]), shape(docs["en"])
    for key in es:
        if es[key] != en[key]:
            r.fail("parity", f"{key}: es {es[key]} != en {en[key]}")
    for lang, doc in docs.items():
        visible = list(zip(doc.summaries, doc.answers[: len(doc.summaries)]))
        ld = faq_ld(doc)
        if visible != ld:
            r.fail("parity", f"{lang}: FAQPage JSON-LD differs from the visible FAQ")
        if not ld:
            r.fail("parity", f"{lang}: no FAQPage JSON-LD")
    r.note("parity", f"shape {es}")


# ── head ───────────────────────────────────────────────────────────────

def gate_head(site: Path, r: Report) -> None:
    for page in sorted(site.rglob("*.html")):
        rel = page.relative_to(site).as_posix()
        if rel.startswith("og"):
            continue
        doc = parse(page)
        metas = {a.get("http-equiv"): a.get("content") for t, a in doc.tags if t == "meta"}
        if metas.get("Content-Security-Policy") != CSP:
            r.fail("head", f"{rel}: CSP missing or different")
        if doc.styles:
            r.fail("head", f"{rel}: {doc.styles} inline style attribute(s)")
        for a, body in doc.scripts:
            if a.get("type") == "application/ld+json":
                try:
                    json.loads(body)
                except json.JSONDecodeError as e:
                    r.fail("head", f"{rel}: JSON-LD does not parse: {e}")
            elif "src" not in a or body.strip():
                r.fail("head", f"{rel}: inline script")
        for t, a in doc.tags:
            if t == "img":
                for need in ("width", "height", "alt"):
                    if need not in a:
                        r.fail("head", f"{rel}: <img {a.get('src')}> has no {need}")
                if rel in PAGES.values() and a.get("loading") != "lazy":
                    r.fail("head", f"{rel}: <img {a.get('src')}> is not lazy (all shots sit below the hero)")
        if rel in PAGES.values():
            links = [(a.get("rel"), a.get("hreflang"), a.get("href")) for t, a in doc.tags if t == "link"]
            canon = [h for rl, _, h in links if rl == "canonical"]
            want = f"https://{HOST}/" + ("en/" if rel.startswith("en/") else "")
            if canon != [want]:
                r.fail("head", f"{rel}: canonical {canon} != {want}")
            langs = sorted(hl for rl, hl, _ in links if rl == "alternate" and hl)
            if langs != ["en", "es", "x-default"]:
                r.fail("head", f"{rel}: hreflang {langs}")
            if len([h for h, _ in doc.headings if h == "h1"]) != 1:
                r.fail("head", f"{rel}: not exactly one h1")
        if rel == "404.html" and not any(
                t == "meta" and a.get("name") == "robots" and "noindex" in a.get("content", "")
                for t, a in doc.tags):
            r.fail("head", "404.html is not noindex")


# ── budget ─────────────────────────────────────────────────────────────

def sizes(paths: list[Path]) -> tuple[int, int]:
    raw = b"".join(p.read_bytes() for p in paths)
    return len(raw), len(gzip.compress(raw, 9))


def gate_budget(site: Path, r: Report) -> None:
    checks = {f"page {rel}": ("page", [site / rel]) for rel in PAGES.values()}
    checks["css"] = ("css", [site / "assets/site.css"])
    checks["scripts"] = ("scripts", sorted((site / "assets").glob("*.js")))
    checks["font"] = ("font", sorted((site / "assets/fonts").glob("*.woff2")))
    for name, (kind, paths) in checks.items():
        raw, gz = sizes(paths)
        max_raw, max_gz = BUDGET[kind]
        r.note("budget", f"{name}: {raw} B raw, {gz} B gzip")
        if raw > max_raw or (max_gz and gz > max_gz):
            r.fail("budget", f"{name}: {raw} B raw / {gz} B gzip over {max_raw} / {max_gz}")
    for lang, rel in PAGES.items():
        n = len(parse(site / rel).words)
        r.note("budget", f"{lang}: {n} visible words")
        if n > MAX_WORDS:
            r.fail("budget", f"{lang}: {n} visible words, over {MAX_WORDS}")


# ── contrast ───────────────────────────────────────────────────────────

def lum(hex_: str) -> float:
    h = hex_.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    c = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def ratio(a: str, b: str) -> float:
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def gate_contrast(site: Path, r: Report) -> None:
    css = (site / "assets/site.css").read_text(encoding="utf-8").replace('"', "'")

    def block(sel: str) -> dict[str, str]:
        out: dict[str, str] = {}
        for m in re.finditer(re.escape(sel) + r"\s*\{(.*?)\}", css, re.S):
            out.update(dict(re.findall(r"(--[\w-]+):\s*(#[0-9a-fA-F]{6})\s*;", m.group(1))))
        return out

    light = block(":root")
    dark_only = block(":root[data-theme='dark']")
    if not dark_only:
        r.fail("contrast", "no :root[data-theme='dark'] block in site.css")
    dark = {**light, **dark_only}
    pairs = [  # (foreground, background, minimum)
        ("--cr-text", "--cr-bg", 4.5), ("--cr-text", "--cr-surface", 4.5),
        ("--cr-text-muted", "--cr-bg", 4.5), ("--cr-text-muted", "--cr-surface", 4.5),
        ("--cr-accent", "--cr-bg", 4.5), ("--cr-accent", "--cr-surface", 4.5),
        ("--cr-accent-text", "--cr-accent", 4.5), ("--ok", "--cr-surface", 3.0),
        ("--code-text", "--code-bg", 4.5), ("--code-muted", "--code-bg", 4.5),
        ("--dash", "--cr-bg", 2.0),
    ]
    for mode, t in (("light", light), ("dark", dark)):
        for fg, bg, need in pairs:
            if fg not in t or bg not in t:
                r.fail("contrast", f"{mode}: {fg} or {bg} undefined")
                continue
            got = ratio(t[fg], t[bg])
            if got < need:
                r.fail("contrast", f"{mode}: {fg} on {bg} is {got:.2f}:1, needs {need}")
    # Tile art: white names on each palette colour.
    for cls, colour in re.findall(r"\.(p\d+)\s*\{\s*background:\s*(#[0-9a-f]{6});\s*\}", css):
        got = ratio("#ffffff", colour)
        if got < 4.5:
            r.fail("contrast", f"white on tile .{cls} {colour} is {got:.2f}:1")


# ── motion ─────────────────────────────────────────────────────────────

# What a keyframe or transition may move: compositor-friendly or paint-only
# properties. Anything that moves layout (sizes, offsets, margins) shifts
# the page, so it fails, except the FAQ's height, which answers a click.
MOVES = {
    "transform", "opacity", "visibility", "clip-path", "stroke-dashoffset",
    "background-color", "background-size", "outline-color", "color",
    "border-color", "box-shadow",
}
USER_LAYOUT = {("::details-content", "height"), ("::details-content", "content-visibility")}
JS_GATES = ("html.anim", "html.motion", "html.vt-ink")
TEXT = re.compile(r"(^|[\s>+~])(h[1-6]|p|li|a|img|picture|figure|button|\.btn)$")


def css_rules(css: str) -> list[tuple[list[str], str, str]]:
    """(enclosing at-rule preludes, selector, body) for every style rule."""
    css = re.sub(r"/\*.*?\*/", "", css, flags=re.S)
    out: list[tuple[list[str], str, str]] = []
    stack: list[str] = []
    buf = ""
    i = 0
    while i < len(css):
        ch = css[i]
        if ch == "{":
            prelude = buf.strip()
            buf = ""
            if prelude.startswith("@") and not prelude.startswith(("@font-face", "@view-transition")):
                stack.append(prelude)
            else:
                end = css.index("}", i)
                out.append((stack[:], prelude, css[i + 1:end]))
                i = end
        elif ch == "}":
            if stack:
                stack.pop()
            buf = ""
        else:
            buf += ch
        i += 1
    return out


def decls(body: str) -> list[tuple[str, str]]:
    return [(k.strip().lower(), v.strip()) for k, _, v in
            (d.partition(":") for d in body.split(";")) if v.strip()]


def seconds(tok: str) -> float | None:
    m = re.fullmatch(r"(-?[\d.]+)(m?s)", tok)
    if not m:
        return None
    return float(m.group(1)) / (1000 if m.group(2) == "ms" else 1)


def gate_hiding_keyframes(rules: list[tuple[list[str], str, str]], r: Report) -> None:
    """Keyframes that start hidden (opacity 0 / visibility hidden) may only
    hold text hidden while scrolling drives them, or not at all: a delay
    with a backwards fill keeps a heading invisible until it ends."""
    hiding: set[str] = set()
    for stack, sel, body in rules:
        frames = [p for p in stack if p.startswith("@keyframes")]
        if frames and any(s.strip() in ("from", "0%") for s in sel.split(",")) and any(
                (p, v) in (("opacity", "0"), ("visibility", "hidden")) for p, v in decls(body)):
            hiding.add(frames[-1].split()[1])
    for stack, sel, body in rules:
        if any(p.startswith("@keyframes") for p in stack):
            continue
        d = dict(decls(body))
        anim = d.get("animation", d.get("animation-name", ""))
        for part in anim.split(","):
            toks = part.split()
            if not any(t in hiding for t in toks):
                continue
            texty = [s.strip() for s in sel.split(",")
                     if TEXT.search(re.sub(r":[\w-]+(\([^)]*\))?", "", s.strip()))]
            if not texty:
                continue
            times = [t for t in (seconds(x) for x in toks) if t is not None]
            delay = seconds(d["animation-delay"]) if "animation-delay" in d else \
                (times[1] if len(times) > 1 else 0.0)
            fill = d.get("animation-fill-mode", "") + " " + part
            scrolled = "animation-range" in d and \
                any("@supports (animation-timeline: view())" in p for p in stack)
            name = next(t for t in toks if t in hiding)
            if delay and delay > 1:
                r.fail("motion", f"{texty[0]}: {name} hides text for {delay:g}s before it shows")
            elif delay and not scrolled and re.search(r"\b(both|backwards)\b", fill):
                r.fail("motion", f"{texty[0]}: {name} holds text hidden through its delay")


def gate_motion(site: Path, r: Report) -> None:
    js = (site / "assets/motion.js").read_text(encoding="utf-8")
    theme = (site / "assets/theme.js").read_text(encoding="utf-8")
    css = (site / "assets/site.css").read_text(encoding="utf-8").replace('"', "'")
    raw = len(js.encode())
    r.note("motion", f"motion.js: {raw} B raw")
    if raw > MOTION_JS:
        r.fail("motion", f"motion.js is {raw} B, over {MOTION_JS}")
    # The gates: html.anim only without reduced motion or Save-Data; any
    # error or a mid-visit switch to reduced motion runs halt().
    for need, where, text in (
        ("prefers-reduced-motion: reduce", "theme.js", theme), ("saveData", "theme.js", theme),
        ('classList.add("anim")', "theme.js", theme), ("function halt()", "motion.js", js),
        ('classList.remove("anim", "motion")', "motion.js", js),
        ('classList.add("motion")', "motion.js", js),
        ('matchMedia("(prefers-reduced-motion: reduce)")', "motion.js", js),
    ):
        if need not in text:
            r.fail("motion", f"{where} lacks {need!r}")
    rules = css_rules(css)
    reduce_body = " ".join(b for st, s, b in rules if any("prefers-reduced-motion: reduce" in p for p in st))
    reduce_sels = " ".join(s for st, s, b in rules if any("prefers-reduced-motion: reduce" in p for p in st))
    for need in ("animation: none !important", "transition: none !important"):
        if need not in reduce_body:
            r.fail("motion", f"reduced motion block lacks {need!r}")
    for need in ("::view-transition-new(*)", ".replay"):
        if need not in reduce_sels:
            r.fail("motion", f"reduced motion block does not cover {need}")
    for stack, sel, body in rules:
        frame = any(p.startswith("@keyframes") for p in stack)
        quiet = any("prefers-reduced-motion: no-preference" in p for p in stack)
        for prop, val in decls(body):
            if frame and prop not in MOVES:
                r.fail("motion", f"{stack[-1]} animates {prop} (layout)")
            if prop in ("transition", "transition-property") and not val.startswith("none"):
                for part in val.split(","):
                    name = part.split()[0] if part.split() else ""
                    if name not in MOVES and not any(s in sel and p == name for s, p in USER_LAYOUT):
                        r.fail("motion", f"{sel}: transition on {name} (layout)")
            if frame:
                continue
            if prop.startswith("animation-timeline") and \
                    not any("@supports (animation-timeline: view())" in p for p in stack):
                r.fail("motion", f"{sel}: animation-timeline outside @supports (animation-timeline: view())")
            moving = (prop in ("animation", "animation-name") and val != "none") or \
                (prop in ("transition", "transition-property") and val != "none")
            if moving and not quiet and not any(any(g in s for g in JS_GATES) for s in sel.split(",")) \
                    and not any("prefers-reduced-motion: reduce" in p for p in stack):
                r.fail("motion", f"{sel}: {prop} is not behind html.anim/html.motion or no-preference")
            if (prop, val) in (("opacity", "0"), ("visibility", "hidden")) and \
                    any(g in sel for g in JS_GATES):
                for s in sel.split(","):
                    if TEXT.search(re.sub(r":[\w-]+(\([^)]*\))?", "", s.strip())):
                        r.fail("motion", f"{s.strip()}: motion hides text or images")
    gate_hiding_keyframes(rules, r)
    for rel in PAGES.values():
        doc = parse(site / rel)
        for t, a in doc.tags:
            if t == "video" or "autoplay" in a:
                r.fail("motion", f"{rel}: <{t}> autoplays or is a video; use a play-once clip")
            if "replay" in a.get("class", "").split() and (t != "button" or a.get("type") != "button"):
                r.fail("motion", f"{rel}: .replay is not a <button type=button>")


GATES = {
    "files": gate_files, "links": gate_links, "parity": gate_parity,
    "head": gate_head, "budget": gate_budget, "contrast": gate_contrast,
    "motion": gate_motion,
}


def build(site: Path, out: Path, release: bool) -> None:
    args = [sys.executable, "-I", str(APPLY), "--repo", REPO, "--site", str(site), "--out", str(out)]
    args += ["--release-json", str(FIXTURE)] if release else ["--no-release"]
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL)


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    p.add_argument("--site", type=Path, default=ROOT / "site")
    p.add_argument("--only", default=",".join(GATES))
    p.add_argument("-v", "--verbose", action="store_true")
    a = p.parse_args(argv)
    only = [g.strip() for g in a.only.split(",") if g.strip()]
    unknown = [g for g in only if g not in GATES]
    if unknown:
        p.error(f"unknown gate(s): {', '.join(unknown)}")
    failed = 0
    with tempfile.TemporaryDirectory() as tmp:
        for variant, release in (("release", True), ("no-release", False)):
            out = Path(tmp) / variant
            build(a.site, out, release)
            r = Report()
            for g in only:
                GATES[g](out, r)
            if a.verbose:
                for n in r.notes:
                    print(f"  {variant} {n}")
            for f in r.failures:
                print(f"FAIL {variant} {f}")
            print(f"{variant}: {len(only)} gate(s), {len(r.failures)} failure(s)")
            failed += len(r.failures)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
