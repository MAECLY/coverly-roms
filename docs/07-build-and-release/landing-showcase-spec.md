# Landing showcase spec: "La estantería viva" / "The Living Shelf"

Status: **approved concept, ready to build** (branch `feat/landing-showcase`).
Inputs: [landing-motion-research.md](landing-motion-research.md) (audit, techniques,
sources), three competing concepts and three judges. All three judges picked
**The Living Shelf**. This document merges it with the grafts the judges agreed
on and resolves the points where they disagreed. Where this spec and the
research disagree, this spec wins. [landing.md](landing.md) stays the
reference for how the site works today. WP6 updates it from this spec.

Owner's ask: make the landing much more dynamic and beautiful, so people notice
the app and like it, while it stays free, honest and fast.

---

## 0. Decisions taken in this synthesis

| # | Question | Decision | Why |
|---|---|---|---|
| S1 | Does the hero's leaning cover tip in? | **No.** In the hero it only *nudges* (12° → 9° → 12°) and stays in the icon pose. It seats **once**, in the closing, right above the CTA. | Story economy (judge 2). The hero, the icon and the OG image show the same pose, and the payoff is saved for the end of the page. |
| S2 | Spring on the nudge? | **No.** `easing.standard`. Spring is used only for the Manila tag drops (brand §6). | Judge 3: reject spring on the nudge. |
| S3 | Pointer tilt and lamp sheen | **Cut.** | Judges 2 and 3: a stock hover trope. The slot grammar carries the motion. |
| S4 | Mini Lean-To under every H2 | **Cut.** Section heads get only the shelf-rule draw. | It turns the signature into a tic (judge 2). The Lean-To appears in exactly three places: hero, #seguro demo, closing. |
| S5 | The 28-cover drifting "wall" interlude | **Cut.** It is replaced by one gallery row of 10 covers in #formatos, which "develops" once. No drift. | Judges 2 and 3: NFT-adjacent filler, and a fourth timeline. |
| S6 | Pinned seam / range handle | **Not built.** The demo in #seguro uses three native buttons. | Word and byte headroom. Judge 1 made it optional. |
| S7 | Honest ending | **Yes.** In the closing, 2 slots (1 on mobile) stay dashed after the fill and pulse once, next to an `aria-hidden` mono `152`. | All three judges: "Coverly never guesses" as the emotional payoff. |
| S8 | Closing fill technique | **One `clip-path: inset()` layer** for the covers and one for the spines, scrubbed by `view()`. Not 24 per-slot animations. | Judge 3: one compositor animation, less CSS. |
| S9 | #como dark band | **Yes, greyboard-dark** (`.band-dark` scope emitted by `build_tokens.mjs`), with a static weave. Buckram stays for spines and rules only. | Contrast rhythm; brand §2/§6 (judge 2). Cut it if the contrast gate or the owner review fails. |
| S10 | Pacing | One loud moment (hero), one hands-on piece (#seguro), one echo/payoff (closing). Everything else is a quiet beat. **At most 3 running animations at any scroll position. Nothing else moves while the hero plays.** | All judges. |
| S11 | `MOVES` (animatable properties in the motion gate) | **Unchanged.** Only transform, opacity, clip-path, stroke-dashoffset, box-shadow and colour crossfades. No offset-path, no `@property` scrub, no filter animation. | Living Shelf's cheapest-gates property. |
| S12 | Cover colour | Landing-only **cover inks** exception (brand §2), one hue per cover at two strengths over Paper. Generator flag `--palette ink` gives the greyboard fallback. **Needs the owner's sign-off on the contact sheet before WP1 commits the sprite.** | Brand risk 1 in every concept. |

---

## 1. Story

The whole page is **one long library shelf** seen from the front, and the
visitor's scroll is the run. Every section stands on a shelf board drawn with
the brand's shelf rule (1 px `--cr-border` with a 3 px Buckram cap).

- **Hero**: a shelf with gaps. Coverly simulates (riso ghost proofs), applies
  (inks register, photo-corners fold away, Manila tabs drop) and commits
  (Buckram spines). The last cover is left **leaning at 12° against the last
  slot**: the app icon, literally. It rocks once, like an invitation.
- **Middle**: the four steps are seen through a camera that moves in on what
  matters. In #seguro the visitor runs the round trip themselves:
  Simular → Aplicar → Deshacer, and the card comes back exactly as it was.
- **Closing**: a full-width shelf fills from left to right as it scrolls into
  view. Two slots stay dashed (Coverly never guesses), and the leaning cover
  from the hero finally tips upright into the last slot, right above the
  Download button. The page opens and closes on the same object, and the last
  move belongs to the visitor.

Grammar, never broken: **dashed = not yet, solid = done, Buckram = Coverly did
it, Manila tab = you can still undo it.** Motion only ever enacts that rule.

The art is IP-safe: generative geometric covers with invented titles as seeds
(the names already in the page data: Æther Rally, Crónicas Quest 64…). No
console silhouettes, no pixel fonts, no CRT, no real box art.

---

## 2. Motion vocabulary (from `design/tokens.json`)

| Token (CSS) | Value | Used for here |
|---|---|---|
| `--cr-duration-fast` | 110 ms | corner fold (each), CTA seat, FAQ marker |
| `--cr-duration-base` | 150 ms | Manila flash, tab drop, reduced-motion crossfades |
| `--cr-duration-slow` | 240 ms | dashed → solid crossfade, spine draw, slot fill |
| `--cr-duration-slower` | 400 ms | hero beats (proof, register) |
| `--cr-ease-standard` | `cubic-bezier(.2,0,0,1)` | nudge, marker, rule draw |
| `--cr-ease-enter` / `--cr-ease-exit` | `(0,0,.38,.9)` / `(.2,0,1,.9)` | appear / fold away |
| `--cr-ease-ink` | `cubic-bezier(.2,.7,.2,1)` | **every apply, register, fill, undo** |
| `--cr-ease-spring` | `cubic-bezier(.34,1.5,.64,1)` | **tag drops only** ("Tu equipo", closing tab) |

Landing-only additions (in `site.css`, prefixed `--lp-` so the token-drift
gate ignores them):

- `--lp-ease-swing`: a damped pendulum `linear()` curve (5 swings, decaying
  to 0). Used only for the accession tag in #descargar.
- `--lp-dur-hero: 2300ms`: the total of the hero timeline, for the replay
  lock.

**Fix in WP1:** `@keyframes land` uses `--cr-ease-spring` while applying.
Brand §6 forbids that; it becomes `--cr-ease-ink` (the keyframes are retired
in WP2 anyway, but the bug is fixed first so it never ships).

---

## 3. The single signature moment: "The Lean-To" (hero)

Inline `<svg class="art hero-scene">` (viewBox `0 0 640 420`, `width`/`height`
attributes plus CSS `aspect-ratio`, `aria-hidden="true" focusable="false"`),
inside the existing `<figure class="shelf">`, whose `div[role=img]` keeps an
updated `aria-label`. The shelf: a rule that bleeds past the right edge of the
viewport, 7 slots at 3:4, and an 8th slot cut off by the edge.

- **Default HTML (end frame):** slots 1, 2, 4 and 6 filled (plain covers).
  Slots 3 and 5 applied (cover + Manila tab + Buckram spine). Slot 7 missing
  (dashed + photo-corners) with a cover leaning on it at 12°, contact shadow
  underneath. Slot 8 peeks, dashed.
- **Start state** (added by `shelf.js` only, under `html.motion`, class
  `.is-armed`): slots 3 and 5 show dashed outlines and corners, no cover, no
  tab, no spine; the shelf rule is undrawn. Text is never part of the art, so
  nothing readable is hidden.
- **Trigger:** `.play` is added when ≥ 40% of the scene is on screen,
  **immediately at load if it is already visible** (no 250 ms wait). Plays once.
  The `↻ Repetir` button replays it (the existing `.replay` contract: dimmed with
  `aria-disabled` while playing, focus stays on it).

| t (ms) | Beat | What moves | Duration / easing |
|---|---|---|---|
| 0–320 | Shelf | Shelf rule draws left → right (`pathLength=1`, `stroke-dashoffset` 1 → 0). The aria-hidden dashed underline under *portada* / *cover* in the H1 draws with it. | 320 ms, `ease-enter` |
| 120–560 | Gaps | Dashes of slots 3 and 5 march in once (dashoffset), 60 ms stagger. | `slow`, `ease-standard` |
| 480–960 | **Simulate** | Riso ghost proof in slots 3 and 5: the cover's two layers (tone and key) appear offset by `translate(3px,-2px)` against each other, opacity 0.45. Dashed outline and corners still visible: "a plan, nothing written". | `slower`, `ease-enter` |
| 1040–1480 | **Apply** | Layers register (translate → 0, opacity → 1), 140 ms stagger. Dashed outline crossfades to solid. The four photo-corners fold away **one by one** (scale 1 → 0 from the outer corner, 60 ms stagger). The Manila tab slides down (translateY −6 → 0) with a 150 ms Manila flash. | register `slower` `ease-ink`; corners `fast` `ease-exit`; tab `base` `ease-enter` |
| 1480–1800 | **Commit** | 3 px Buckram spines draw under slots 3 and 5 (scaleX 0 → 1, origin left), 80 ms stagger. The H1 underline turns solid. The mini-slot counter under the scene reads `6/7`. | `slow`, `ease-ink` |
| 1800–2240 | **Nudge** | The leaning cover on slot 7 lifts 2 px and rocks 12° → 9° → 12° around its bottom-right contact point. The contact shadow tightens and relaxes with it. **It does not seat.** | 440 ms, `ease-standard` |
| 2240–2300 | Hand-off | Slot 8, cut off at the edge, pulses its dashes once (one dash cycle). This is the cue to scroll. | 600 ms, `ease-standard` (ends ~2840) |

Rules:

- Pure CSS keyframes with `animation-delay`, all under
  `html.motion .hero-scene.play` and `@media (prefers-reduced-motion:
  no-preference)`. `shelf.js` only toggles `.is-armed`/`.play`, and its stop()
  removes both (end frame = HTML).
- SVG groups that transform use `transform-box: fill-box` and a
  `transform-origin` set in CSS (Safari).
- The H1 and `.sub` never move or fade. `.sub` stays the LCP element.
- **Mobile (< 1024 px):** a second `svg.art.hero-strip` (viewBox `0 0 320 64`,
  height 64 px reserved) sits between the H1 and `.sub`. It has 5 slots
  (2 gaps + the leaning cover) and plays the same beats at the same times, so
  the moment happens above the fold. The wide scene is `display:none` below
  1024 px and the strip above it. No CSS layout swap on load.
- The old 4×3 CSS grid, its status line and its `motion.js` code are
  **retired** (about 1.5 KB of HTML and CSS and about 2 KB of JS back; about
  60 visible words back, which pays for the demo and terminal lines).
- Header brand mark (WP1): the Lean-To inlined at 24 px from
  `design/icon/glyph-24.svg` geometry. On `:hover`/`:focus-visible` the cover
  tips 12° → 0° and back (`slow` each way, `ease-standard`). The link's
  accessible name becomes the visible text "COVERLY ROMs" (fixes
  `label-content-name-mismatch`).

---

## 4. Section by section

Each row lists the trigger, the motion, the SVG art and the static/no-JS end
state. "view()" means CSS scroll-driven, inside
`@supports (animation-timeline: view())`; without support the end state shows.

### 4.1 Topbar (WP1)

- **Motion:** the scroll-progress line becomes a row of 24 tiny 3:4 slots: a
  dashed row with a solid Buckram row over it, revealed by
  `clip-path: inset(0 calc(100% - progress) 0 0)` on `animation-timeline:
  scroll(root)`. The header Download button entrance stays.
- **Art:** one inline `svg.art.progress-slots` (24 `<use href="#ms">`, about
  600 B). Hidden below 480 px (the existing line stays there).
- **Static:** the dashed row only (no progress shown). That is correct, because
  the reader has not read anything.

### 4.2 Hero (WP2)

See §3.

### 4.3 Section heads, every H2 (WP3)

- **Motion:** the shelf rule under each H2 draws (`stroke-dashoffset` or a
  `scaleX` on the 1 px line, Buckram cap first). `view()`, range
  `entry 10%` → `cover 30%`. It replaces `@keyframes bar-fill`.
- **Art:** CSS only (`::after` lines). No Lean-To glyph (S4).
- **Static:** rule fully drawn.

### 4.4 #como, four steps (WP3): the greyboard-dark band

- **Band:** `#como` gets `class="band-dark"`. `build_tokens.mjs` emits a
  `.band-dark { … }` block in `site/assets/tokens.css` only (never in
  `src/styles/tokens.css`), copying the dark semantic values, and in dark mode
  it maps to `surface-sunken`. A static weave tile
  (`assets/art/weave.svg`, two crossed hairlines) at 4% opacity on
  `::before`. The contrast gate must pass on the scope.
- **Camera:** each screenshot sits in a `.cam` wrapper that scales and
  translates (transform only) into the region that matters, scrubbed by the
  existing `--s1…--s4` view timelines: step 1 → the "4553 sin portada" footer,
  step 2 → the "nada escrito" banner, step 3 → the 95% candidate, step 4 →
  the cover slot. Max scale 1.9 so 6 px UI text reads at about 11 px. The
  camera targets are CSS custom properties per step (`--cx`, `--cy`, `--cz`)
  on `.cam-1…4`.
- **Rail:** a sticky vertical rail of four 3:4 slots in the empty left column
  (≥ 1024 px). A slot is dashed until its step is active, then solid with a
  spine (same timelines, `ease-ink`).
- **Callouts:** leader lines (`pathLength=1`, 1 px, `--cr-text-secondary`)
  draw with dashoffset from the zoomed region to the existing mono annotation.
  They are added inside the existing `svg.annot` overlays (1280×800). The
  existing ellipses stay.
- **Step 2 stamp:** a petrol outline "proof" ring (two concentric strokes, the
  second offset 2 px as riso misregistration) presses in (scale 1.08 → 1,
  opacity 0.001 → 1, `base`, `ease-enter`), holds, lifts away. No text.
- **Step 4:** the blurry 48×68 crop is replaced by a vector cover
  (`<use href="assets/art/covers.svg#c-…">`) that flies into the slot (same
  3.2 s clip). It ends with the Manila tab peeling (rotate −18° about its
  top-right corner, then back, `slow` `ease-ink`) to show that undo exists.
- **Static / reduce:** full screenshots uncropped, rail solid, callouts drawn,
  stamp visible, step 4 cover seated with its tab.

### 4.5 #seguro, the round trip you perform (WP4)

The section's moment is hands-on (Playdate's "delight at the point of
commitment").

- **Art:** `svg.art.demo` (viewBox `0 0 560 220`): a card frame (the SD card,
  rounded rect, generic, no logos) holding a 5-slot shelf. Slots 2 and 4 are
  missing; slot 5 has an old cover that the visitor has chosen to replace.
  Outside the card frame is the **app-data folder** (folder glyph,
  greyboard with only its tab in Manila), matching the copy: "Las copias…
  van a la carpeta de datos de la app, no a la SD".
- **Controls:** three native `<button type="button">` with `aria-pressed`:
  **Simular · Aplicar · Deshacer** (es) / **Simulate · Apply · Undo** (en),
  plus `<p class="demo-status" aria-live="polite">` reusing the retired hero
  strings ("Simulación lista: 3 portadas, nada escrito", "3 portadas
  añadidas", "Tu tarjeta vuelve como estaba."). The buttons are rendered with
  `hidden` and `shelf.js` removes it.
- **States:**
  - *Before* (set by JS on init): slots 2 and 4 dashed, slot 5 old cover.
  - *Simular*: ghost proofs (two misregistered layers, 0.45) in 2, 4 and 5.
    Nothing else changes.
  - *Aplicar*: (1) a copy of the old slot-5 cover slides **out of the card
    into the folder** (backup first); (2) the proofs register into 2, 4 and 5
    (`ease-ink`, 140 ms stagger), the corners fold, the Manila tabs drop.
    The tabs stay (you can still undo).
  - *Deshacer*: the tabs peel (rotate −18° about the corner, `slow`,
    `ease-ink`), the new covers slide down and out, and the old cover rides back
    from the folder into slot 5. It ends exactly on *Before*.
  - Buttons that do not apply are `aria-disabled` (Aplicar before Simular,
    Deshacer before Aplicar), never removed, so focus never jumps.
- **Implementation:** `document.startViewTransition` when available and
  motion is allowed (FLIP of the moving cover between slot and folder via
  `view-transition-name` set by class), otherwise a plain class swap. Never a
  view transition under reduce, and focus is never moved.
- **Pictograms:** five 20 px stroke pictograms before each `<dt>` (dry-run
  toggle, dashed slot, folder, boundary box, pause). `pathLength=1`, line-drawn
  once on entry via `view()` with an 80 ms stagger through `--i` classes
  `.i1…i5`. From `kit.svg`.
- **Reports screenshot:** the same `.cam` zoom onto the "Sin portada" column.
- **Static / no JS:** buttons hidden, scene shows the *applied* state (tabs on,
  old cover in the folder). Under reduce the demo works with instant swaps plus
  a ≤ 150 ms opacity crossfade.

### 4.6 #formatos, gallery row and terminal (WP4)

- **Gallery row (replaces the wall):** `svg.art.gallery` above the layout
  table: one shelf of 10 generated covers grouped in runs by the on-SD folder
  names already in the table, with an `aria-hidden` mono folder label drawn as
  a path (no `<text>`). Covers "develop" once on entry (`view()`): the tint
  layer, then the key layer (opacity, 60 ms stagger), then the spine. **No
  drift.** On `:hover` of a table row (and `:focus-within` of the scroll
  region) the matching run lifts 3 px with `--cr-shadow-raised`
  (`fast`, `ease-standard`). One gap in the row stays dashed (the honest-gap
  motif).
- **Table:** keep the row-by-row slot fill. A 6 px ink chip per row matches the
  gallery runs.
- **Terminal:** after each typed command, one output line types after a
  200 ms pause, in `--cr-code-accent` / `--cr-success`:
  `4553 sin portada` · `plan: 4553 · 0 escritos` · `✓ 4401 aplicadas` ·
  `↶ 4401 restauradas` (en: `4553 missing` · `plan: 4553 · 0 written` ·
  `✓ 4401 applied` · `↶ 4401 restored`). The lines are in the HTML (the
  finished page) and are hidden only while typing, like the commands. Rows
  have a fixed height (no CLS). The cursor becomes a 1ch × 1.33ch dashed slot
  (CSS `background-image` dash pattern) that blinks 3 times and stops (WCAG
  2.2.2).
- **Static:** all output lines shown, no cursor.

### 4.7 #comparar (WP5)

- **Motion:** on entry (`view()`, `entry 20%` → `cover 40%`) the Coverly column
  highlight wipes top → bottom (`clip-path: inset(0 0 100% 0)` → `inset(0)`)
  and a check glyph draws in (dashoffset) in each cell where Coverly answers
  yes, staggered 40 ms through `.i1…`. Nothing else moves. The honest tone.
- **Art:** one `<symbol id="ck">` in `kit.svg`, reused via `<use>`.
- **Static:** highlight and checks shown.

### 4.8 #descargar (WP5)

- **No release:** the empty right column holds `svg.art.waiting`: a large 3:4
  dashed slot with photo-corners, and a Manila **accession tag on a string**
  with `v0.1.0` drawn as stroke-glyph paths (from the generator's
  7-glyph stroke font; no `<text>`, no words). On entry the slot's dashes march
  **one** cycle (1.2 s, `ease-standard`) and stop. The tag swings once and
  settles (`--lp-ease-swing`, 1.2 s, rotate about the string's pin; a playful
  context, not an apply). **No leaning cover here** (S1: the Lean-To appears
  only in the hero, the demo and the closing).
- **Release:** the OS cards fill like slots (dashed → solid border colour,
  90 ms stagger, `ease-ink`), and the detected-OS "Tu equipo" tag drops in with
  `--cr-ease-spring` (allowed: tag drop).
- **"Lo que aún no hace":** the dashed bullets draw in. The text never hides.

### 4.9 #preguntas (WP5)

- **Motion:** the `<summary>` marker becomes a 10×13 mini slot: dashed when
  closed (CSS dash pattern), solid Buckram with a 2 px spine when open, as a
  `fast` colour crossfade. The existing `interpolate-size` open stays.
- **Art:** CSS only (about 300 B).

### 4.10 Closing: the payoff (WP6)

- **Art:** `svg.art.finale` full-bleed (viewBox `0 0 1440 200` desktop; a second
  `svg.art.finale-m` `0 0 390 160` with 8 slots for < 768 px). The shelf rule
  spans the viewport. Three stacked layers:
  1. outlines: 24 dashed slots with corners (`<use href="assets/art/kit.svg#sm">`);
  2. covers: 21 covers (`<use href="assets/art/covers.svg#c-…">`), with **no
     cover in slots 9 and 17** (mobile: slot 5), and nothing in slot 24;
  3. spines: 21 Buckram bands.
  Slot 24 (desktop) / 8 (mobile) is centred above the CTA and holds the
  **Lean-To group** from the hero.
- **Motion** (`view()` on `.closing`, range `entry 0%` → `contain 40%`):
  - the covers layer wipes in left → right with **one** `clip-path: inset(0
    calc(100% - p) 0 0)`; the spines layer follows with the same wipe offset
    by about 6% (the spine draws just after its cover lands). `ease-ink` as
    the timing function inside the range.
  - `contain 40%` → `contain 50%`: the two honest gaps pulse their dashes
    once; an `aria-hidden` mono `152` (a path glyph, not text) fades in above
    them.
  - `contain 45%` → `contain 60%`: the Lean-To tips 12° → 0° about its
    bottom-right corner and seats (the same keyframes as the hero nudge's
    pose, ending upright), with a 1 px settle; its Manila tab drops
    (`--cr-ease-spring`, tag drop); the shadow resolves.
  - then the CTA's Buckram spine (`box-shadow` inset on `.btn-primary`)
    draws under the button.
- **Fallback without scroll timelines** (Firefox < 159, Safari < 26):
  `shelf.js` adds `.landed` when 50% of the section is visible, which plays
  the same end-state keyframes once over about 1.6 s (time-based, not
  scrubbed).
- **Static / reduce / no JS:** the full shelf with the two honest gaps and
  `152`, the Lean-To seated with its tab, CTA spine drawn.
- The current 136×44 `closing-mark` is retired.

### 4.11 Footer (WP5)

- The "0 third-party requests" figure sits in a tiny dashed mini-slot that
  **stays dashed forever**: an empty slot that is supposed to be empty. Static.
  The colour-mode view-transition spread stays.

### 4.12 Global surface (WP1)

- `assets/art/grain.svg` (`feTurbulence`, `numOctaves="2"`, about 400 B),
  rendered at 4% opacity on `body::before`, `position: fixed`,
  `pointer-events: none`. **Never animated.** Both themes checked; in dark
  mode 3%.
- One light model: top-left light. Every slot and cover in the art has a
  contact-shadow ellipse (`--cr-text` at 0.12 opacity, 0.2 in dark).
  Screenshots keep `--cr-shadow-raised`.
- The same-document view transition on the theme toggle and the cross-document
  one for es ↔ en stay. The hero shelf rule gets
  `view-transition-name: shelf` so it stays still across es ↔ en.

---

## 5. Assets

### 5.1 Files under `site/assets/art/` (cached, shared by es and en)

| File | Made by | Contents | Cap |
|---|---|---|---|
| `covers.svg` | **generated** by `scripts/site/gen_covers.mjs` | 8 pattern families × about 5 seeds = **40 cover `<symbol>`s** `c-01…c-40` (viewBox `0 0 30 40`), plus a 7-glyph stroke font `g-v g-0 g-1 g-5 g-2 g-dot g-x` (for `v0.1.0`, `152`) | ≤ 18,000 B raw (`COVERS_SVG`) |
| `kit.svg` | hand-authored, run through SVGO | `sm` (missing slot, dashed, `pathLength=1`), `sf` (filled slot), `pc` (one photo-corner), `tab`, `spine`, `ms` (mini slot), `ck` (check), `p-dry p-slot p-folder p-box p-pause` (pictograms), `folder`, `card`, `stag` (string tag), `lean` (the Lean-To cover group) | ≤ 4,000 B |
| `grain.svg` | hand-authored | static `feTurbulence` tile, 160×160 | ≤ 600 B |
| `weave.svg` | hand-authored | 2 crossed hairlines, 8×8 pattern tile | ≤ 400 B |

The art bucket in the budget gate caps them together at 24,000 / 7,000 B.

**Colour in sprites (CSP- and `<use>`-safe):** page CSS does not reach inside
a `<use>` shadow tree, and the CSP forbids inline `style`. So **every symbol
in a sprite is monochrome `currentColor`**:

- covers: the key layer is `fill="currentColor"`; the tone layer is
  `fill="currentColor" fill-opacity=".28"`. The Paper ground is the CSS
  `background` of the wrapping `<svg>` (or a `<rect class="ground">` in the
  page). The ink is set per `<use>` with a class: `.ink-0 … .ink-5 { color: … }`.
- kit parts: one colour each; the page composes them with several `<use>`s,
  each with its own colour class (`.c-missing`, `.c-tab`, `.c-spine`).
- `stroke-dashoffset` and `stroke-dasharray` are inherited, so animating them
  on the `<use>` element reaches the symbol's path.

WP1 starts with a 15-minute spike at `http://localhost:8095` in Chrome,
Safari and Firefox to confirm external `<use>` + `currentColor` + inherited
dashoffset. If a sprite-internal `<style>` with `var(--ink-b)` also works in
all three, the generator *may* emit a true second ink. Otherwise single-hue it
is.

### 5.2 Inline SVG (in the HTML, per locale)

| Where | Class | Approx. raw | Notes |
|---|---|---|---|
| Header mark | `svg.art.mark` | 450 B | glyph-24 geometry, `currentColor` |
| Progress row | `svg.art.progress-slots` | 600 B | 24 × `<use href="#ms">` (inline `<symbol>` in this svg) |
| Hero scene + strip | `svg.art.hero-scene`, `svg.art.hero-strip` | 3.5 KB | **covers inlined** (5 symbols) so first paint needs no request |
| H1 underline | `span.kw-slot` | 80 B | CSS only, `aria-hidden` |
| #como rail, callouts, stamp | `svg.art.rail`, inside `svg.annot` | 1.0 KB | |
| #seguro demo + pictograms | `svg.art.demo`, `svg.art.pict` ×5 | 1.6 KB | |
| #formatos gallery | `svg.art.gallery` | 0.9 KB | `<use>` from sprites |
| #descargar waiting slot | `svg.art.waiting` | 0.6 KB | norelease block only |
| Closing finale + mobile | `svg.art.finale`, `svg.art.finale-m` | 1.6 KB | `<use>` from sprites |

Every `svg.art`: `aria-hidden="true" focusable="false"`, `viewBox`,
`width`/`height`, no `<text>`, `<image>`, `<script>` or `<foreignObject>`
(the **art** gate enforces all of this).

### 5.3 The cover generator, `scripts/site/gen_covers.mjs`

- Node, zero dependencies, deterministic. Seeded PRNG `mulberry32` with
  invented titles as seeds (list in the script; no trademarks).
- 8 families, fixed grammar, geometric only: `grid`, `rings`, `horizon-sun`,
  `bands`, `chevron`, `blocks` (flat isometric-free stacked rects), `dots`
  (halftone), `flow` (3 fixed curves). Each cover = tone layer + key layer +
  a title band (a solid bar, no text), at most 12 path commands per layer, with
  coordinates rounded to 0.5.
- Inks (one per cover, `.ink-0…5`), from primitives, muted 20% toward
  greyboard.11 (light) / greyboard-dark.11 (dark), computed by the script:
  info.9, success.9, warning.11, danger.9, greyboard.11, greyboard.9 and their
  `-dark` counterparts. **Never buckram (spines only) and never manila (tabs
  only).**
- Outputs:
  1. `site/assets/art/covers.svg`;
  2. the ink classes, light and dark (`:root[data-theme='dark']` and the
     `prefers-color-scheme` guard), written into `site/assets/site.css`
     between `/* covers:begin GENERATED by scripts/site/gen_covers.mjs */` and
     `/* covers:end */`;
  3. with `--contact <path>`: an HTML contact sheet of all 40 covers in both
     themes, written **outside `site/`** (scratch dir) for the owner review.
- `--check` exits 1 if either output is stale (the files gate runs it, like
  `build_tokens.mjs --check`). `--palette ink` emits the greyboard fallback.

### 5.4 Brand exception (WP1 writes it, owner signs off)

New paragraph in `docs/08-brand/brand.md` §2: **Landing cover inks**.
Landing only, never in the app UI. Illustrated covers on the landing may use
one muted ink per cover from the primitive list above (two strengths over
Paper). Buckram stays for spines and rules, Manila for tabs. Slot outlines
keep `--cr-slot-missing` at ≥ 3:1. Fallback: `--palette ink`.

---

## 6. JS / CSS architecture

### 6.1 Files

| File | Role | Changes |
|---|---|---|
| `site/assets/theme.js` | sets `html.anim` | unchanged |
| `site/assets/motion.js` | existing contract: `html.motion`, `halt()`, reveals, step clips, terminal, request counter | WP1: `halt()` dispatches `document.dispatchEvent(new Event("cr-halt"))`. WP2: the old `hero()` is removed. WP3: the step-4 clip uses the vector cover. WP4: terminal output lines. Stays ≤ 9,500 B. |
| `site/assets/shelf.js` (new, `defer`, after motion.js) | the showcase choreography | WP1 skeleton; each later WP adds one module |
| `site/assets/site.css` | all styles | one marked block per WP (below) |
| `site/assets/tokens.css` | generated | WP3: `.band-dark` scope |

`shelf.js` contract (the motion gate checks the strings):

```js
(function () {
  "use strict";
  var html = document.documentElement;
  if (!html.classList.contains("motion")) return;   // motion.js ran and no reduce
  var stops = [];
  function halt() { stops.forEach(function (s) { try { s(); } catch (x) {} }); stops = []; }
  document.addEventListener("cr-halt", halt);
  matchMedia("(prefers-reduced-motion: reduce)").addEventListener("change", function (m) { if (m.matches) halt(); });
  // one shared IntersectionObserver: when(el, ratio, fn) plays once, and
  // [data-off] pauses (animation-play-state) art that is off screen
  // modules: hero() WP2, demo() WP4, waiting() WP5, finale() WP6
})();
```

- Every module registers a `stop()` that puts its piece in the **HTML end
  state**. `halt()` = finish, not cancel.
- The #seguro demo also has to work **under reduce** (it is user-started). So
  the demo module initialises outside the `motion` guard, in a second tiny
  IIFE that only does instant class swaps when `html.motion` is absent.
- Per-element stagger: `--i` via `el.style.setProperty` (CSSOM is allowed by
  the CSP; markup never has `style=""`) or `.i1…i9` classes for CSS-only
  pieces.
- Load: `<script src="assets/shelf.js" defer>` after motion.js. Not
  render-blocking. `shelf.js` waits for motion.js's `html.motion` (both
  deferred, in order).

### 6.2 CSS organisation

`site.css` gets marked blocks so each WP owns its own region:

```
/* == showcase:surface == */     WP1 (grain, light model, --lp-* curves)
/* covers:begin GENERATED … */   WP1 generator only
/* == showcase:topbar == */      WP1
/* == showcase:hero == */        WP2 (replaces the old .shelf-grid rules)
/* == showcase:heads == */       WP3
/* == showcase:steps == */       WP3
/* == showcase:safety == */      WP4
/* == showcase:formats == */     WP4
/* == showcase:compare == */     WP5
/* == showcase:download == */    WP5
/* == showcase:faq == */         WP5
/* == showcase:footer == */      WP5
/* == showcase:closing == */     WP6 (replaces .closing-mark rules)
/* == showcase:reduce == */      shared; each WP appends its lines
```

All motion sits behind `html.anim`/`html.motion` **and**
`@media (prefers-reduced-motion: no-preference)`. Scroll-driven parts are
inside `@supports (animation-timeline: view())`, with `animation-timeline`
declared **after** the `animation` shorthand and `fill-mode: both`.

### 6.3 Budgets (applied now in `scripts/site/check_site.py`)

| Gate | Before | **Now** | Spend planned | Justification |
|---|---|---|---|---|
| page `index.html` (each) | 48,000 / 12,500 | **unchanged** | 36.8 → about 44 KB raw / 11.3 KB gz | the hero grid retirement pays for the inline art |
| css (`tokens.css` + `site.css`) | 54,000 / 12,500 | **64,000 / 15,000** | 49.5 → about 60 KB / 14 KB | hero keyframes 2 KB, steps camera and rail 1.5 KB, demo 1.2 KB, generated inks 1 KB, closing 1 KB, compare/download/FAQ/footer 1.5 KB, band scope 0.8 KB, minus the old grid 1.5 KB |
| scripts (all `.js` under `assets/`, now **rglob**) | 16,000 / 5,500 | **20,000 / 7,000** | 13.7 → about 18.4 KB / 6.3 KB | `shelf.js` ≤ 5 KB; motion.js shrinks after WP2. rglob closes the `assets/vendor/` loophole. |
| `motion.js` alone (`MOTION_JS`) | 9,500 | **unchanged** | 9.3 → about 7.6 KB | the hero leaves it; it gains cr-halt and the terminal lines |
| `shelf.js` alone (`SHELF_JS`) | — | **5,000** | about 4.2 KB | hero 0.9, IO and pause 0.6, demo 1.6, closing fallback 0.4, glue 0.5 |
| art (`assets/art/*.svg`) | — | **24,000 / 7,000** | about 21 KB / 5.5 KB | covers ≤ 18 KB, kit 4 KB, grain and weave 1 KB |
| `covers.svg` alone (`COVERS_SVG`) | — | **18,000** | about 15 KB | 40 covers at about 350 B |
| visible words (each locale) | 1,450 | **unchanged** | release-es 1,403 → about 1,395 | −60 (hero grid) +3 (buttons) +12 (terminal output) +about 20 (demo status) |
| `MOVES` | — | **unchanged** | — | S11 |

New gate behaviour (also in this commit):

- **motion**: if `shelf.js` exists it must be ≤ `SHELF_JS`, contain
  `classList.contains("motion")`, listen for `"cr-halt"`, and contain the
  reduced-motion `matchMedia`. Then motion.js must dispatch `"cr-halt"`.
- **art** (new gate): every inline `svg.art` is `aria-hidden="true"
  focusable="false"`, has `viewBox` and `width`/`height`, and contains no
  `text`/`image`/`img`/`script`/`foreignObject`. Every `assets/art/*.svg` has
  none of these, no external `href` and no `data:`. `covers.svg` ≤ 18,000 B.
- **links**: a `#fragment` into an `.svg` must name an `id` in that file
  (catches a `<use>` pointing at a cover that was not generated).
- WP1 adds `assets/shelf.js` and the art files to `REQUIRED`, and the
  `gen_covers.mjs --check` staleness check to the files gate.

---

## 7. Reduced motion and no-JS

The reduced-motion page is its own **finished still life**, not a stripped
copy. It equals the default HTML:

- **Hero:** slots 3 and 5 applied (tab + spine), slot 7 empty with the cover
  leaning at 12° (the icon pose), slot 8 peeking, `6/7`. Replay hidden.
- **Steps:** full screenshots, rail solid, callouts and stamp drawn, step 4
  cover seated with its tab.
- **#seguro:** the demo works (it answers a click) with instant swaps and an
  opacity crossfade ≤ 150 ms. No view transition is started.
- **Gallery, compare, download, FAQ:** finished; the tag hangs still.
- **Closing:** full shelf, two honest gaps, `152`, the Lean-To seated.
  (The page still opens leaning and closes seated.)

Mechanics:

- The CSS reduce block keeps `animation: none !important` and
  `transition: none !important` (which also stops `animation-timeline`), and
  adds an allowance for `.demo` opacity crossfades only.
- `theme.js` never sets `html.anim` under reduce or Save-Data; motion.js
  never sets `html.motion`; `shelf.js` returns early (except the demo).
- Turned on mid-visit: motion.js `halt()` → `cr-halt` → shelf.js `halt()`; every
  piece snaps to its end frame.
- **No JS:** the same finished page; demo buttons stay `hidden`; the applied
  demo state shows.
- **Assertions:** `document.getAnimations().length === 0` under reduce and with
  JS disabled (a DevTools evaluate step in §9).
- **Keyboard:** all interactive bits are native buttons, links or `details`
  with the brand focus ring. The brand-mark tip mirrors on `:focus-visible`.
- **Blinking:** only the terminal cursor (3 blinks, then stops). No loop
  anywhere lasts more than 5 s.

---

## 8. Performance plan

- **LCP stays `p.sub`** (live 1.7 s). The hero SVG is inline and decorative,
  painted in its end frame or start frame with no request: covers inlined, no
  sprite on the first screen, no web font in SVG. The mobile strip has a
  fixed 64 px height. Verify that the LCP element and render delay are
  unchanged with an `LCPBreakdown` trace.
- **CLS 0:** every `svg.art` has `width`/`height`/`viewBox` plus CSS
  `aspect-ratio`; terminal output rows have a fixed height; buttons in the
  demo are in the HTML (only `hidden` changes, inside a reserved box).
- **Requests:** +3 same-origin, non-blocking (`shelf.js`, `covers.svg`,
  `kit.svg`), plus `grain.svg`/`weave.svg` via CSS. The sprites are first
  needed below the fold; the browser fetches them when the first `<use>`
  renders. No preload, so nothing competes with the LCP.
- **Compositor-only motion:** transform, opacity, clip-path on HTML/SVG
  groups. `stroke-dashoffset` repaints small paths only. No filter, no
  animated `feTurbulence`, no blend modes.
- **Concurrency:** ≤ 3 running animations at any scroll position;
  `shelf.js` pauses off-screen art with `[data-off]` →
  `animation-play-state: paused`. Pause on `visibilitychange` (motion.js
  already does this).
- **`will-change`:** only on the hero leaning cover while `.play` and on the
  closing covers layer, removed at the end.
- **Targets:** Lighthouse mobile ≥ 95 live (floor 90, 3 runs, median), LCP
  < 2.5 s (expect about 1.7 s live), CLS < 0.05 (expect 0), TBT about 0.
- Transfer: about +10 KB gz first visit, all cached across es/en.

---

## 9. Test and verification plan

Run after **every** WP (the WP is not done until all of it passes):

1. `node scripts/site/gen_covers.mjs --check` (from WP1) and
   `make site-check`: all 8 gates, both release states, 0 failures.
2. Serve the release-filled copy: `make site RELEASE=scripts/site/fixtures/release.json`
   on **port 8095** (`python3 -m http.server 8095 -d <out>`); stop it only with
   `lsof -ti tcp:8095 -sTCP:LISTEN | xargs -r kill`. Also check the
   no-release copy. Never use `file://` (external `<use>` fails there).
3. Chrome DevTools MCP, `/` and `/en/`:
   - screenshots at 390×844 and 1440×900, light and dark, for the WP's sections;
   - `emulate` reduced motion → `document.getAnimations().length === 0`,
     finished still life, demo still usable;
   - JS disabled → same finished page, demo buttons absent;
   - keyboard pass: Tab through the header, replay, demo (aria-pressed and the
     live region announce), FAQ;
   - concurrency probe while scrolling:
     `document.getAnimations().filter(a => a.playState === "running").length <= 3`;
   - console: no errors, no CSP violations.
4. WP2 and WP6 also run performance traces at 4× CPU and Slow 4G
   (`LCPBreakdown`, `CLSCulprits`) and Lighthouse mobile ×3 (median) on both
   locales.
5. WP6 runs the full matrix once more on the finished page, plus Safari (macOS
   and, if available, iOS simulator) for `transform-box`, external `<use>`
   and dashoffset repaints, and Firefox (stable, no scroll timelines) for the
   end states and the closing `.landed` fallback.
6. es/en parity: each WP mirrors every markup change in `site/en/index.html`
   in the same commit (the parity gate will fail otherwise).

The mock app on port 1445 is not needed for this work; screenshots stay the
existing `site/assets/shots/*.webp`.

---

## 10. Work packages

Built **sequentially** in this order, one agent each. Each WP: owns the files
and CSS blocks listed, mirrors es/en, leaves `make site-check` green, commits in
granular conventional commits (no co-author or AI lines), **does not push**.
Shared files (`index.html`, `en/index.html`, `site.css`, `shelf.js`,
`motion.js`) are touched only in the region named for that WP.

### WP1 — Foundations: art pipeline, gates wiring, surface, header

- Spike (§5.1), then `scripts/site/gen_covers.mjs` → `site/assets/art/covers.svg`
  + the generated ink block in `site.css`; contact sheet to the scratch dir
  for the owner.
- `site/assets/art/kit.svg`, `grain.svg`, `weave.svg`.
- `site/assets/shelf.js` skeleton (§6.1) and the `<script defer>` tag in both pages.
- `motion.js`: `cr-halt` dispatch in `halt()`. `site.css`: fix `@keyframes land`
  to `ease-ink`; blocks `showcase:surface`, `showcase:topbar`, `covers:*`.
- Header in both pages: the inlined Lean-To mark, accessible name, progress
  slots.
- `check_site.py`: `REQUIRED` += shelf.js and art files; files gate runs
  `gen_covers.mjs --check`. `Makefile`: `site`/`site-check` run the generator
  check.
- `docs/08-brand/brand.md`: landing cover inks exception (§5.4).
- Commits: `fix(site): ink easing on the hero land keyframes`,
  `feat(site): generative cover sprite generator`,
  `feat(site): art kit, grain and weave tiles`,
  `feat(site): shelf.js skeleton joined to halt()`,
  `feat(site): inline brand mark and slot progress row`,
  `docs(brand): landing cover inks exception`.

### WP2 — Hero: "The Lean-To" signature moment

- Both pages: replace the `.shelf-grid` and its status with `svg.art.hero-scene`
  + `svg.art.hero-strip`, the mini-slot counter, the H1 `kw-slot` underline,
  the updated `aria-label`; keep `.replay`.
- `site.css` `showcase:hero` (replaces the old grid CSS) and its reduce lines.
- `shelf.js` hero module; `motion.js`: remove `hero()`.
- Commits: `feat(site): hero shelf scene`, `feat(site): Lean-To hero timeline`,
  `refactor(site): retire the CSS hero grid`.

### WP3 — Section heads and #como steps

- `site.css` `showcase:heads` (replaces `bar-fill`), `showcase:steps`.
- Both pages: `#como` band class, `.cam` wrappers, rail, callouts and stamp in
  `svg.annot`, vector step-4 cover.
- `motion.js`: step-4 clip uses the vector cover.
- `scripts/brand/build_tokens.mjs` + regenerated `site/assets/tokens.css`:
  `.band-dark` scope (site tokens only; the app tokens stay byte-identical).
- Commits: `feat(site): shelf-rule draw under section heads`,
  `feat(brand): band-dark token scope for the landing`,
  `feat(site): steps camera, rail and callouts`,
  `fix(site): vector cover in the step 4 clip`.

### WP4 — #seguro round trip and #formatos gallery and terminal

- Both pages: `#seguro` demo scene, buttons, live region, pictograms, reports
  `.cam`; `#formatos` gallery row, row chips, terminal output lines.
- `shelf.js` demo module (plus the reduce-safe init); `motion.js` terminal
  output and slot cursor.
- `site.css` `showcase:safety`, `showcase:formats`.
- Commits: `feat(site): round-trip demo in the safety section`,
  `feat(site): safety pictograms`, `feat(site): formats gallery row`,
  `feat(site): terminal output and slot cursor`.

### WP5 — #comparar, #descargar, FAQ and footer

- Both pages: compare checks, the waiting slot with its accession tag
  (norelease block), OS card fill hooks (release block), FAQ marker, footer
  dashed mount.
- `shelf.js` waiting-slot trigger (only if CSS `view()` cannot express it).
- `site.css` `showcase:compare`, `showcase:download`, `showcase:faq`,
  `showcase:footer`.
- Commits: `feat(site): comparison wipe and checks`,
  `feat(site): waiting slot in the download section`,
  `feat(site): FAQ slot marker`, `feat(site): empty slot for the request counter`.

### WP6 — Closing finale, OG, docs and the full verification

- Both pages: `.closing` finale (desktop and mobile), retire `closing-mark`.
- `shelf.js` `.landed` fallback; `site.css` `showcase:closing`.
- `make site-og` to re-render `site/assets/og.png` and `og-en.png` from the new
  hero end frame (if `og.mjs` draws the hero).
- `docs/07-build-and-release/landing.md`: the new motion map, budgets and the art
  gate (replace the stale 44,000/10,500 note).
- Full §9 matrix, Lighthouse ×3 on both locales.
- Commits: `feat(site): closing shelf finale`, `chore(site): re-render the OG
  cards`, `docs(landing): showcase motion map and budgets`.

---

## 11. Risks and how this spec answers them

| Risk | Answer |
|---|---|
| Owner declines the cover inks | `gen_covers.mjs --palette ink`; everything else is unchanged |
| Generative art reads as generic or NFT-like | 8 fixed families, one muted ink, 40 covers, no drift band; contact-sheet review before the sprite is committed |
| External `<use>` quirks | §5.1 spike first; hero covers inlined; serve over http only |
| Too much motion | §0 S10 pacing, ≤ 3 concurrent, probe in §9 |
| LCP regression on mobile | fixed-height strip, `.sub` never animated, trace in WP2 and WP6 |
| Word cap | hero grid retirement frees about 60 words; WP4 and WP5 re-count both locales |
| Firefox < 159 / Safari < 26 | finished states by default; `.landed` fallback for the closing |
| Dark mode glare | dark ink set generated; grain at 3% in dark |
