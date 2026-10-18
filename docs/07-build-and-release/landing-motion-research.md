# Landing motion research

Research for making the landing (`site/`, see [landing.md](landing.md)) more
dynamic and more visually striking without breaking what it already does
well: a static, bilingual, tracker-free page that is finished without JS and
complete under `prefers-reduced-motion`.

Date: 2026-10-17. Branch: `feat/landing-showcase`. Brand rules referenced
are in [`docs/08-brand/brand.md`](../08-brand/brand.md).

Contents:

1. [Summary and decisions needed](#1-summary-and-decisions-needed)
2. [Current-landing audit](#2-current-landing-audit)
3. [What award-level sites do](#3-what-award-level-sites-do)
4. [Techniques catalogue](#4-techniques-catalogue)
5. [Tech stack recommendation](#5-tech-stack-recommendation)
6. [Byte budget](#6-byte-budget)
7. [Concept-art directions](#7-concept-art-directions)
8. [The plan for Coverly](#8-the-plan-for-coverly)
9. [Guardrails and how to measure](#9-guardrails-and-how-to-measure)
10. [Sources](#10-sources)

---

## 1. Summary and decisions needed

**Finding.** Coverly already has more motion than the sister sites
(Versorium, Pluryprint): scroll-driven step cross-fades, a hero
simulate/apply demo, line-drawn ellipses, typed terminal commands. What the
page lacks is **art direction** — colour, depth, an illustrated object — and
**one memorable moment**. The hero is a flat, near-monochrome grid; the
brand's own icon (the Lean-To) never appears in motion; the closing section
is a tiny mark in an empty band.

**Recommendation.** No libraries. Native CSS (scroll-driven animations,
`linear()` easing, `@starting-style`, view transitions), inline SVG drawn by
hand, and a short hand-written WAAPI timeline in our own script. Build one
orchestrated moment — **"The Lean-To becomes The Slot"** — open and close
the page on the same object, and space smaller moments between them.

**Decisions the owner must take before implementation** (each lands in its
own commit with a written reason):

| # | Decision | Why |
|---|---|---|
| D1 | Raise the budgets in `scripts/site/check_site.py` (proposal in §6) and change the `scripts` glob to recurse | There is 206 B left in `motion.js` and 0.9 KB gzip left for CSS. Nothing substantial fits today. A file in `assets/vendor/` would currently escape the gate. |
| D2 | Widen the motion gate's `MOVES` list (`check_site.py:453`) | Today only `transform, opacity, visibility, clip-path, stroke-dashoffset, background-color, background-size, outline-color, color, border-color, box-shadow` may animate. The plan needs `offset-distance`, `translate`/`rotate`/`scale`, `fill`, `stroke`, `stroke-dasharray` (all paint- or compositor-only). |
| D3 | A brand exception for a **landing cover palette** | brand.md §2 says filled slots in illustrations are solid `--cr-slot-filled` ink, so the hero is greyscale. The app's own screenshots already show saturated placeholder covers, so the landing is less lively than the product. Proposal: muted duotones derived from the tokens (greyboard plus desaturated petrol, rust, olive, plum), contrast-checked, Manila still tab-only. |
| D4 | Whether to accept a small visible-word increase | The page is ~50 words under the 1,450 cap. New art should carry no text; terminal output lines would add ~20 words per language. |

---

## 2. Current-landing audit

Measured on this branch with `check_site.py --only budget,motion -v`,
Chrome DevTools MCP (1440×900 and 390×844, light/dark, reduced motion) and
Lighthouse CLI mobile.

### 2.1 Baseline

| Check | Result |
|---|---|
| Lighthouse mobile, live (GitHub Pages, gzip) | **99** · FCP 1.2 s · LCP 1.7 s · TBT 0 · CLS 0 |
| Lighthouse mobile, local (no gzip), es and en | 95 · FCP 1.5 s · LCP 2.8 s · TBT 0 · CLS 0 |
| LCP element | `p.sub` (hero paragraph, text) |
| DevTools trace, 4× CPU + Slow 4G | LCP 2,015 ms (all render delay) · CLS 0.00 |
| Accessibility / best practices / SEO | 100 / 100 / 100 |
| DOM elements / animations running at load | 554 / 9 |
| Reduced motion | `document.getAnimations()` = 0, classes only `js` (verified) |

The live page has about **0.8 s of LCP headroom**. Anything new in the hero
must not add render-blocking bytes or change the LCP element.

### 2.2 Budgets today

| Gate | Now | Limit | Left |
|---|---|---|---|
| `motion.js` raw (`MOTION_JS`) | 9,294 B | 9,500 | **206 B** |
| scripts (`assets/*.js`) raw / gz | 13,687 / 4,491 | 16,000 / 5,500 | 2.3 KB / 1.0 KB |
| css (`tokens.css` + `site.css`) raw / gz | 49,487 / 11,570 | 54,000 / 12,500 | 4.5 KB / **0.9 KB** |
| page `index.html` raw / gz | 36,838 / 9,061 | 48,000 / 12,500 | ~11 KB / 3.4 KB |
| visible words es / en | 1,403 / 1,380 | 1,450 | ~50 / ~70 |

### 2.3 Hard constraints already in the page

- **CSP**: `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'`.
  No `data:` URIs (a grain texture must be a file in `assets/`), no inline
  `<style>` or `style=""`. Inline `<svg>` in HTML and `setProperty()` from JS
  are fine.
- **Motion contract** (keep it): the HTML is the finished page; motion is
  gated by `html.anim` (set in `<head>`) and `html.motion` (set when
  `motion.js` runs); every effect registers with `halt()`; scroll effects sit
  inside `@supports (animation-timeline: view())`; text is never hidden.
- **Brand §6**: `easing.spring` is never used on apply or undo. Buckram is
  never decoration. Manila stays under ~2% of a screen.

### 2.4 Section by section

| Section | Now | What feels flat | Opportunity |
|---|---|---|---|
| Topbar | `scroll()` progress line; header Download appears when hero buttons leave | Fine (utility) | Progress line as a row of tiny slots filling (brand progress primitive). Brand mark tips 12° → 0° on hover/focus (needs the mark inlined). |
| Hero | 4×3 CSS covers, 5 gaps; armed → dry run → applying (clip-path wipe every 380 ms) → done → Repetir | Monochrome; no depth, shelf or light; ~1.9 s of empty dashed gaps before the first cover lands; on 390×844 the art starts at y=597 (below the fold); no cover ever travels | The signature moment (§8.1); faster opening (~2.2 s total instead of ~3.8 s); a compact above-the-fold strip on mobile; duotone covers (D3); pointer tilt on desktop |
| Steps (`#como`, 3,032 px tall) | Sticky screenshot cross-faded via `--s1…--s4`; drawn ellipses; step 3/4 clips | Large empty bands; screenshots shown at 790 px from 1280 px captures, UI text ~6 px (~4 px on mobile); step 4 landed cover blurry (48×68 crop scaled ~2.7×) | Scroll-scrubbed **camera zoom** into the region that matters; sticky **step rail** of four slots in the empty column; drawn leader-line callouts; a "dry run · 0 written" stamp; an undo "tab peel" after step 4 |
| Safety (`#seguro`) | Five-item `<dl>` and a static screenshot | Reads like terms, yet it is the main differentiator | "Round trip" SVG: old cover slides into backups, new one lands with a tab, Undo swaps them back; small line-drawn pictograms per `<dt>` |
| Formats + terminal | Table slots fill row by row; commands type in turn | No output, so no payoff | Short output lines after each command (`aria-hidden` or counted words); slot-shaped cursor |
| Comparison | Static table, Coverly column highlighted | Expected | Column highlight sweep (`clip-path`), drawn check glyphs. Nothing more: the tone is honesty. |
| Download (no release) | Dashed notice and "not yet" list | Right half empty at 1440 | Large dashed 3:4 slot with photo-corners and `v0.1.0`, marching-ants dash once. With a release: OS cards fill like slots, "your system" tag drops in. |
| FAQ | `interpolate-size` open, chevron rotates | Fine | Marker becomes a mini slot: dashed closed, solid open |
| Closing | 136×44 four-slot mark, last slot scales in | **Biggest missed moment after the hero** | Full-bleed shelf finale (§8.2) |
| Footer | View-transition colour-mode spread; request counter "0" | Fine | Nothing needed |

**Bugs found** (fix regardless of the redesign):

- At 390 px the Manila `libretro` tab overlaps the `NEOGEO` system label.
- `@keyframes land` uses `--cr-ease-spring` while applying (brand §6 forbids it).
- Step 4 flying cover is a blurry upscaled crop.
- `a.brand` `aria-label="Coverly ROMs, inicio"` does not contain the visible
  "COVERLY ROMs" (`label-content-name-mismatch`).
- `llms.txt` has no H1 (Lighthouse agentic-browsing 67).
- Console: "Lazy-loaded images should have explicit dimensions" ×3 (likely
  the `<picture><source>` of `mark-dark`).
- Archivo ships its width axis (wdth 75–112) and the page never uses it.

---

## 3. What award-level sites do

First-hand teardowns of Linear, Raycast, Arc, Teenage Engineering, Nothing
and Analogue were not available; those rows rely on secondary sources and
are marked so.

| Site | Technique | Why it is memorable | Performance trick |
|---|---|---|---|
| **Lando Norris** (OFF+BRAND), Awwwards Site of the Year 2025 | WebGL/3D, Rive, GSAP, "cinematic scrolling" | Motion that moves like the subject (race speed) | Lazy loading, optimised assets (secondary) |
| **Messenger**, Awwwards Developer SOTY 2025 | A tiny WebGL planet | One self-contained world as the hook | — |
| **Vercel Ship '25** (basement.studio) | Ray-marched fluid hero in one shader; Motion `useTransform` sticky sections; header readable via `mix-blend-mode` | They dropped the first concept (spiky ferrofluid) after reviewers flagged trypophobia: concept art was tested for reactions | Mostly prerendered; only CTAs dynamic |
| **Vercel Ship '24 badge** | 3D lanyard badge with physics | You can grab and fling it; it is yours | — |
| **Vercel "What will you ship?"** (Rauno Freiberg) | Icons pixelate in (bitmap → canvas dots); orbit rings via `offset-path` | **Novelty pacing**: big moments never back to back, micro-motion between; "pompous" animations cut | **Layered hero**: CSS/SVG paints with no JS, a shader fades in after load, code-split and skipped on low-power devices. Reduced motion pauses (`animation-play-state: paused`) rather than removing |
| **Stripe gradient** | Small custom WebGL wrapper, mesh-gradient shader, skewed canvas | Living colour without content noise | Tiny wrapper instead of three.js |
| **Apple product pages** | Scroll-scrubbed image sequence on canvas in a ~500vh sticky stage | Scroll feels like handling an object | First frame, last frame, then in-betweens; WebP spritesheet (15.2 MB PNG → ~1.5 MB) |
| **Playdate** (Panic + Teenage Engineering) | Onboarding teaches each input; "crank to buy" with tension and confetti (A press as accessible fallback); overlapping-corner menu bounce | Delight sits exactly at the moment of commitment | 1-bit art: tiny assets, constraint as style |
| **Teenage Engineering** (secondary) | Radical restraint, accent only for interactive states | The constraint is the aesthetic | — |
| **Nothing** (secondary) | Dot-matrix type matching the hardware | A hardware detail becomes the typography | — |
| **Arc** (secondary) | Film-like install intro | "A movie starting to play" | — |
| **Linear / Emil Kowalski** (principles) | UI motion < 300 ms, `ease-out` for responses, transform/opacity only, transitions for interruptible motion, nothing on keyboard actions | "Speed as a feature" | CSS/WAAPI keep running when the main thread is busy |
| **Ghostty** | One product-object animation (ASCII ghost, 235 frames) | One object from the product's world | — |

**Patterns across them:**

1. One signature moment that comes from the product's own idea.
2. A hero that paints without JS and is enhanced afterwards.
3. Novelty pacing: not everything moves at once.
4. A physical metaphor you can touch (badge, crank, slot).
5. Reduced motion designed as its own state, not deleted.

---

## 4. Techniques catalogue

Support from MDN browser-compat data, checked 2026-10-17.

| Technique | Chrome | Safari | Firefox | Verdict for this site |
|---|---|---|---|---|
| Scroll-driven `animation-timeline: scroll()` / `view()` + `animation-range` | 115 | 26 (Sep 2025) | **159, releases 2026-10-27** (behind a flag before that) | Main storytelling tool. Keep the `@supports` guard: Firefox ESR and Safari ≤ 18 get the finished state. Declare `animation-timeline` *after* the `animation` shorthand. |
| Scroll-*triggered* (`animation-trigger`, `timeline-trigger`) | 146 | no | no | Not yet; keep `IntersectionObserver` |
| Same-document view transitions | 111 | 18 | 144 | Yes, for in-page state changes (FLIP a cover from missing to filled slot) |
| Cross-document VT (`@view-transition { navigation: auto }`) | 126 | 18.2 | no | Already used for es↔en; add types so the shelf rule stays put |
| `@starting-style` | 117 | 17.5 | 129 | Yes, entry transitions with no JS |
| `@property` | 85 | 16.4 | 128 | Yes (sheens, conic progress); needs a `MOVES` exception |
| `linear()` easing | 113 | 17.2 | 112 | Yes: tuned "ink settles" curve; springs/overshoot only on non-apply pieces |
| `offset-path` `path()` / `ray()` / `url()` | 64 / 116 / 116 | 16 / 16 / 17 | 72 / 122 / 122 | Yes: a cover arcing into a slot; needs `offset-distance` in `MOVES` |
| CSS `d: path()` morph | 52 | **parses, no effect** | 97 | Avoid. Morph with transform/clip-path or crossfade two paths |
| `sibling-index()` stagger | 138 | 26.2 | no | Use `--i` custom properties instead (set from JS, CSP forbids inline style) |

**SVG line drawing.** `pathLength="1"`, `stroke-dasharray: 1`,
`stroke-dashoffset: 1 → 0`; can be tied to a `view()` timeline. It repaints
(not composited), so keep paths short and play-once. If older Safari
ignores `pathLength`, measure with `getTotalLength()`.

**SVG: CSS vs WAAPI vs SMIL.** CSS/WAAPI on inline SVG by default. WAAPI
(`el.animate()`) costs 0 bytes of library, is cancellable, and
`.finish()` maps onto the existing `halt()` contract. SMIL is the only way
to animate inside `<img src=x.svg>` but ignores reduced motion unless the
SVG carries its own `@media` CSS and runs on the main thread in Safari:
avoid it for page content.

**Mask and clip reveals.** SVG masks in a 0–100 viewBox give blind/grid
reveals; the same in pure CSS with `clip-path: inset()` driven by `view()`.

**Faux 3D.** `perspective` on a shared parent; pointer JS writes `--px/--py`;
tiles `rotateX/Y` ≤ 4°. Only under `(hover: hover) and (pointer: fine)`,
mirrored on `:focus-visible`, off under reduced motion.

**Texture.** The 2026 "handmade" reaction to AI-polished visuals: grain,
paper, hand marks. Fits Buckram & Manila (book cloth, manila stock, photo
corners). Never animate SVG filters (`feTurbulence` repaints the whole
region on the CPU in Safari; iOS stutters reported). Use one static tile
file, `numOctaves` ≤ 2, opacity 0.04–0.06.

**Kinetic type.** Works only as a single hero-level moment. Optional
variant: closing headline settles from wdth 100 → 75 using the already
loaded Archivo axis (needs `font-stretch` in `MOVES` and a fixed-width
container against CLS). Low priority.

**Libraries, measured** (jsDelivr + esbuild `--minify` + `gzip -9`):

| Library | Licence | gzip | Verdict |
|---|---|---|---|
| Motion `animate` (`motion/mini`) 14.3.0 | MIT | 3.8 KB | Only if interruptible spring physics are truly needed |
| Motion `animate` + `scroll` + `inView` | MIT | 29.1 KB | No |
| Motion `inView` alone | MIT | 0.6 KB | Hand-write it (~300 B) |
| GSAP 3.15 core (+ ScrollTrigger 18 KB, DrawSVG 2.2 KB) | **Proprietary "Standard No Charge"** (Webflow-owned, revocable, redistribution unclear) | 28.3 KB | **Reject**: fails the OFL/MIT rule and ~7× the remaining budget |
| OGL / twgl / regl | Unlicense / MIT / MIT | 13 / 19.6 / 28.5 KB | No WebGL this iteration |
| lottie-web light | MIT | 46.5 KB + JSON | No |
| dotLottie web | MIT | ~496 KB WASM (fetched from a CDN by default) | No |
| Rive canvas-lite | MIT runtime | 323 KB WASM + 109 KB JS | No |
| Hand-written WebGL fullscreen shader | own | ~1.5–2.5 KB | Optional later layer only |

---

## 5. Tech stack recommendation

Zero dependencies, native first, inside the existing contract.

1. **CSS (~70% of the motion).** Scroll-driven `view()`/`scroll()`
   choreography, `linear()` curves, `@starting-style` entries,
   `@property` sheens, `offset-path` arcs, dash-drawn strokes. Every rule
   inside `@media (prefers-reduced-motion: no-preference)`, the `html.anim`
   gate and, for scroll effects, `@supports (animation-timeline: view())`.
   The reduced-motion block also sets `animation: none !important` to
   neutralise timelines.
2. **Inline SVG art** in the HTML: shelf rule, slots, leaning cover,
   photo-corners, Manila tab, Buckram spine. Hand-drawn paths, SVGO,
   `<symbol>`/`<use>` for repeats, `aria-hidden="true"`, no `<text>` (word
   cap), explicit `width`/`height`/`viewBox` (3:4) so nothing shifts.
3. **WAAPI in our own script** for the one orchestrated moment: an array
   of `el.animate(keyframes, {delay, easing, fill: 'both'})` (~1 KB),
   each pushing a stop that calls `anim.finish()` into `halt()`. Either
   inside a raised `motion.js` or a second deferred `showcase.js` counted
   by the gate.
4. **One shared `IntersectionObserver`** (`rootMargin: 0px 0px -15% 0px`,
   threshold 0.2, unobserve after first hit) for play-once reveals and as
   the fallback where scroll timelines are missing; a second use pauses
   offscreen loops. Also pause on `visibilitychange`.
5. **No GSAP, Lottie, Rive or WebGL** this iteration. Motion mini only if
   needed, vendored with its licence file and counted by a fixed gate.

---

## 6. Byte budget

Proposed caps for `check_site.py` (decision D1):

| Bucket | Now | Proposed cap | Spent on |
|---|---|---|---|
| `motion.js` raw | 9.3 / 9.5 KB | **13 KB** (or keep 9.5 KB and add `showcase.js`) | WAAPI hero timeline (~2 KB), pause-offscreen IO (~0.5 KB), pointer tilt (~0.2 KB) |
| scripts total raw / gz | 13.7 / 4.5 KB | **20 / 7 KB**, glob `rglob` | The above, plus room for optional Motion mini |
| css total raw / gz | 49.5 / 11.6 KB | **62 / 15 KB** | Scroll choreography, `offset-path`, VT rules, section moments (~8–10 KB raw) |
| page each raw / gz | 36.8 / 9.1 KB | 48 / 12.5 KB (keep) | Inline SVG scene and slots, 4–8 KB raw |
| new raster / static art | 0 | ≤ 40 KB total, lazy, sized | Grain tile `assets/grain.svg` or WebP ≤ 4 KB |
| third-party, CDN, WebGL, WASM | 0 | 0 | — |

Expected added transfer: **~6–9 KB gzip**, no new requests if SVG is
inlined. Compatible with Lighthouse mobile ≥ 90 while the LCP stays the
static hero text.

---

## 7. Concept-art directions

Fixed point for every direction: the empty Slot (3:4, dashed, photo-corners)
becomes a filled cover (solid, Manila tab = applied this run, Buckram spine
= committed) on the shelf rule. Each direction is a way to draw that one
change, not a second idea.

| Direction | Mood | How it animates | Cliché risk | Fit |
|---|---|---|---|---|
| **A. Clothbound library** | Petrol buckram, fine linen weave, blind/foil-stamped geometric patterns, two colours max (Bickford-Smith's Penguin Clothbound rule) | Pattern stroke draws in ~600 ms (stamping), fill switches to foil; spine slides into slot | Low–medium (heritage pastiche if ornament wins; keep motifs geometric) | **Base layer**: the brand vocabulary already is this |
| **B. Generative covers per system** | Each "game" gets a seeded face (grid, flow, radial, bands), after NYPL's generative eBook covers; invented titles, no IP | Layers build: tint, pattern strokes, title band; a row of slots "develops" like a contact sheet | Medium (unbounded flow fields read as AI/crypto; constrain to two inks + Paper, small fixed grammar) | **Strongest product fit**. Pre-bake as SVG `<symbol>` + per-cover custom properties, so no-JS shows filled covers |
| **C. Collector's catalogue** | Numbered plates, captions ("Plate 14 · cover missing"), after Bitmap Books' box-art collections | Plate index counts up; plates flip from MISSING to filled | High if real box art is used (IP). Only with B covers | Optional framing for captions |
| **D. Isometric shelf** | Small 30° bookcase diorama (eBoy as the "done well" reference) | Covers slide into recesses | **High** (2017–2021 "isometric SaaS") and it weakens the flat 3:4 signature | Avoid in the hero |
| **E. Paper-craft** | Greyboard layers, manila tag stock, photo-album corners, soft contact shadows | Corners fold over a settling cover (staggered 200 ms); tag swings and settles ~1.2 s; shadow via pseudo-element opacity | Medium (generic paper-cut landscapes); low if only brand objects are used | **Base layer**: photo-corners are already paper-craft |
| **F. Risograph print state** | Two inks (petrol + manila) on uncoated stock, misregistration, halftone | Simulated = misregistered ghost proof; apply = inks snap into register; undo = inks drift apart and fade | Medium if used as a global overlay; low as a state metaphor | **Gives *simulate* its own visual language** |
| **G. Generic cartridges** | Rounded notch, ribbed grip, label recess as a 3:4 slot; no console silhouettes | Cartridge lifts, label gets a cover, clicks back down | Medium (pixel fonts, "press start" nostalgia) | Secondary, palette-bound |
| **H. CRT-less handheld glow** | Memory-LCD grey, slight backlight bloom on the screenshot frame only | Screenshot "wakes" from 1-bit dither to full image | **Very high** (glitch, VHS, synthwave) | Only as the transition into the product-shot section, if at all |

**Synthesis.** A + E as the base (cloth, stamped pattern, photo-corners,
tag), covers from B (generated, IP-safe), F as the language for
*simulate*. Avoid D and H in the hero, any global grain overlay, pixel
fonts, real box art and console silhouettes, gradient blobs or mesh
gradients, particle fields, glassmorphism, cursor followers, fade-up on
every block, typewriter headlines, scroll-jacking, infinite marquees.

---

## 8. The plan for Coverly

### 8.1 The orchestrated moment: "The Lean-To becomes The Slot" (hero)

The icon made literal, playing once in about 2.2–2.4 s:

1. The hero shelf (inline SVG, Manila shelf rule, slight `perspective`)
   paints with the slots' dashed outlines drawing in.
2. Empty slots show riso-misregistered ghost covers (*simulate*).
3. A cover leaning ~12° against the main empty slot tips upright, travels a
   short `offset-path` arc and seats with `--cr-ease-ink` (no spring: it is
   *apply*). Its inks snap into register, the dash turns solid, the
   photo-corners fold over and the Manila tab slides out.
4. The other gaps fill in a cascade (~180 ms stagger).
5. Spines darken to Buckram last (*committed*).
6. The scene ends on the icon's composition: one cover still leaning
   against one empty slot, tying hero, icon and OG image together.

Rules: no JS needed to see the finished scene (it is the default markup);
start state added by JS; the headline and `.sub` never fade from 0 (LCP);
reduced motion shows the end frame; on 390 px a compact four-slot strip
sits above the fold without becoming the LCP element. Delight sits at the
commitment point (Playdate).

### 8.2 Supporting moments (lower novelty, spaced out)

| # | Moment | Section | Mechanism |
|---|---|---|---|
| 1 | Steps camera zoom + step rail + callouts | `#como` | `--s1…--s4` timelines drive `transform: scale/translate` on a `.cam` wrapper; rail of four slots goes dashed → solid; leader lines drawn with `stroke-dashoffset`. Also fixes illegible screenshots. |
| 2 | Simulate → apply → undo round trip | `#seguro` | SVG SD card ↔ backups; plays once on entry; undo peels the tab (`rotate` around the tab corner) |
| 3 | Shelf finale | closing | Full-bleed row of slots (~24 desktop, ~8 mobile) fills left → right on `view()`; ends on the Lean-To cover settling above the CTA. The page opens and closes on the same object. |
| 4 | Shelf rule draws under each H2 | global | `pathLength` line draw on `view()` |
| 5 | "Empty slot waiting for v0.1.0" | `#descargar` | Large dashed slot with photo-corners; marching-ants dash once |
| 6 | Micro-interactions | global | Slot cards tilt ≤ 4° and tab peeks on hover/focus; CTA seats 1 px with a spine shadow; < 300 ms, `ease-out`, pointer-fine only |
| 7 | Terminal output, FAQ mini-slot marker, comparison column sweep | various | Small CSS-only touches |
| 8 | Paper grain | global | One static `assets/grain.svg` tile at ~4–6% (file, not `data:`, because of the CSP) |
| 9 | es/en and theme switches | global | View transitions (already partly shipped) |

### 8.3 Priority

| # | Change | Impact | Cost | Needs |
|---|---|---|---|---|
| 1 | Hero moment + faster opening + mobile strip | High | ~3 KB SVG, 1.5 KB CSS, 0.8 KB JS | D1, D2 |
| 2 | Duotone generated covers (hero, closing) | High | ~3 KB | D3 |
| 3 | Steps camera zoom, rail, callouts | High | ~1.5 KB CSS | — |
| 4 | Closing shelf finale | Medium | ~1 KB | — |
| 5 | Safety round trip | Medium | ~2 KB | — |
| 6 | Download empty-slot art | Medium | ~0.6 KB | — |
| 7 | Terminal output, path reveal | Low | ~0.8 KB | D4 |
| 8 | Grain, depth/shadow system, `linear()` ink curve | Low | ~0.5 KB + 0.4 KB tile | — |
| — | Bug fixes from §2.4 | — | — | — |

---

## 9. Guardrails and how to measure

**Performance**

- Never start the LCP element at `opacity: 0` (Chrome ignores it as an LCP
  candidate until it repaints; one case went 5.3 s → 2.4 s by fixing this).
  Animate a decorative sibling, or use transform-only rise.
- Animate only transform, opacity, `clip-path`, short `stroke-dashoffset`.
  Never `width`/`height`/`top`/`left`, SVG filters, or `background-attachment: fixed`.
  `will-change` on a handful of elements at most.
- Reserve space for every animated element (SVG `width`/`height`,
  `aspect-ratio: 3/4`). Transforms cannot cause CLS.
- Novelty pacing doubles as a performance rule: fewer things animate at once.
- Optional: skip ambient loops when `hardwareConcurrency <= 4` or
  `deviceMemory <= 2`, without changing layout. Save-Data is already honoured.

**Accessibility**

- Reduced motion = complete, beautiful static composition (covers in slots,
  tab applied). Crossfades ≤ 150 ms at most.
- Any loop over 5 s needs a visible pause control (WCAG 2.2.2) or must stop
  itself; prefer play-once.
- Pointer effects mirrored on `:focus-visible`; nothing animates on
  keyboard-triggered actions.
- New art is `aria-hidden`, carries no text, and never replaces content.

**Measuring**

- `python3 scripts/site/check_site.py` (all gates) after every change.
- Chrome DevTools Performance panel at 4× CPU and Slow 4G: scroll through,
  check no dropped frames, no per-frame "Recalculate Style"/"Paint" from
  scroll-driven effects, and paint flashing only on intended elements.
- Rendering → emulate `prefers-reduced-motion: reduce`; also test no-JS.
- MCP: `performance_start_trace` with reload on
  `http://localhost:8095/` and `/en/`, then `performance_analyze_insight`
  for LCPBreakdown, CLSCulprits, ForcedReflow.
- Lighthouse mobile, incognito, median of 3 runs. Targets: performance
  ≥ 90, CLS < 0.05, LCP < 2.5 s, TBT < 200 ms. Compare against the live
  baseline in §2.1.
- Test sticky/scroll sections on Safari 26 (iOS now gets them) and Firefox
  159 once released.

---

## 10. Sources

Award winners and studio write-ups

- https://www.awwwards.com/annual-awards/winners
- https://www.itsoffbrand.com/our-work/lando-norris
- https://www.webgpu.com/showcase/mclaren-f1-driver-lando-norris-official-website/
- https://tympanus.net/codrops/2026/08/17/creativity-at-enterprise-scale-without-compromise-the-offbrand-story/
- https://vercel.com/blog/designing-and-building-the-vercel-ship-conference-platform
- https://www.awwwards.com/sites/vercel-ship
- https://vercel.com/blog/building-an-interactive-3d-event-badge-with-react-three-fiber
- https://rauno.me/craft/vercel
- https://emilkowal.ski/ui/great-animations
- https://linear.app/now/how-we-redesigned-the-linear-ui
- https://www.bram.us/2021/10/13/how-to-create-the-stripe-website-gradient-effect/
- https://exzenter.github.io/gradient-stripe/
- https://css-tricks.com/lets-make-one-of-those-fancy-scrolling-animations-used-on-apple-product-pages/
- https://geyer.dev/blog/css-image-sequence-animations/
- https://clipcontent.substack.com/p/the-playful-design-details-of-the
- https://blog.panic.com/the-story-of-playdate/
- https://www.sessions.edu/notes-on-design/delicious-design-playdate/
- https://blakecrosley.com/guides/design/teenage-engineering (secondary)
- https://design.withfudge.com/share/teenage.engineering-design (secondary)
- https://www.shadcn.io/design/nothing (secondary)
- https://design-milk.com/the-nothing-phone-3s-glyph-matrix-turns-notifications-into-pixel-art/
- https://en.wikipedia.org/wiki/Arc_(web_browser) (secondary)
- https://pierce.dev/notes/making-an-ascii-animation
- https://github.com/CommunityPokeOrg/ghostty-ascii-animation
- https://www.analogue.co/pocket

Platform features and compatibility

- https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/animation-timeline
- https://www.firefox.com/en-US/firefox/159.0beta/releasenotes/
- https://bugzilla.mozilla.org/show_bug.cgi?id=1676779
- https://webkit.org/blog/17101/a-guide-to-scroll-driven-animations-with-just-css/
- https://webkit.org/blog/17818/announcing-interop-2026/
- https://developer.chrome.com/docs/css-ui/scroll-driven-animations
- https://developer.chrome.com/blog/scroll-triggered-animations
- https://www.bram.us/2025/12/12/css-scroll-triggered-animations-are-coming-to-chrome/
- https://css-tricks.com/almanac/properties/a/animation-trigger/
- https://www.builder.io/blog/view-timeline
- https://developer.chrome.com/docs/web-platform/view-transitions/cross-document
- https://css-tricks.com/cross-document-view-transitions-part-1/
- https://initd.it/en/blog/view-transitions-baseline
- https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@starting-style
- https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@property
- https://web.dev/blog/at-property-baseline
- https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/offset-path
- https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/d
- https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/easing-function/linear
- https://developer.chrome.com/docs/css-ui/css-linear-easing-function
- https://www.joshwcomeau.com/animation/linear-timing-function/
- https://caniuse.com/mdn-css_types_sibling-index

SVG, texture and technique

- https://css-tricks.com/svg-line-animation-works/
- https://css-tricks.com/?p=301196
- https://codefronts.com/motion/css-scroll-animations/scroll-driven-svg-stroke-draw/
- https://tympanus.net/codrops/2026/03/11/svg-mask-transitions-on-scroll-with-gsap-and-scrolltrigger/
- https://www.smashingmagazine.com/2022/05/magical-svg-techniques/
- https://css-tricks.com/?p=193135
- https://www.creativebloq.com/javascript/css-js-smil-animations-21514229
- https://groups.google.com/a/chromium.org/g/blink-dev/c/5o0yiO440LM/m/ZDbCbGObAwAJ
- https://codefronts.com/motion/css-card-hover-effects/3d-tilt/
- https://www.freecodecamp.org/news/grainy-css-backgrounds-using-svg-filters/
- https://ibelick.com/blog/create-grainy-backgrounds-with-css
- https://zharr.is/posts/grain-of-the-screen/
- https://greensock.com/forums/topic/33075-gsap-and-feturbulence-mobile-performance/
- https://elements.envato.com/learn/web-design-trends
- https://studiomeyer.io/en/blog/webdesign-trends-2026

Performance

- https://www.debugbear.com/blog/opacity-animation-poor-lcp
- https://savvy.co.il/en/blog/wordpress-speed/hero-animation-lcp-optimization/

Libraries and licences

- https://motion.dev/docs/gsap-vs-motion
- https://motion.dev/docs/animate
- https://gsap.com/blog/3-13/
- https://gsap.com/standard-license
- https://css-tricks.com/gsap-is-now-completely-free-even-for-commercial-use/
- https://www.carmenansio.com/articles/animated-icon-libraries/
- https://lottiefiles.com/blog/lottie-animations/lottiefiles-or-rive
- https://docs.lottiefiles.com/en/runtimes/distributions/js/v0.x/advanced/performance
- https://github.com/oframe/ogl
- https://github.com/regl-project/regl

Concept art references

- https://penguinrandomhouse.ca/series/B45/penguin-clothbound-classics
- https://www.penguin.co.uk/discover/articles/clothbound-classics-little-history-making-of
- https://www.nypl.org/blog/2014/09/03/generative-ebook-covers
- https://www.bookofshapes.com/ (verify licence before any reuse)
- https://www.tylerxhobbs.com/words/flow-fields
- https://davidmatthew.ie/generative-art-javascript-svg/
- https://en.wikipedia.org/wiki/EBoy
- https://www.timeextension.com/news/2023/05/bitmap-books-pc-engine-the-box-art-collection-launches-this-june
- https://eyeondesign.aiga.org/tracking-the-rise-of-risograph-printing-with-glasgows-risotto-studio/

Sister sites

- https://versorium.maecly.com — recreated autosave/restore demos, raster WebP, no SVG art
- https://pluryprint.maecly.com — one looping hero illustration with a counter
