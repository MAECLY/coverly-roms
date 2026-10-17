# Landing page

The public page at **https://coverly-roms.maecly.com** is the static site in
`site/`: Spanish at `/` (also `x-default`), English at `/en/`, plus `404.html`,
`robots.txt`, `sitemap.xml`, `llms.txt`, `agents.txt` and the web manifests.
Plain HTML, one stylesheet and two small scripts (`theme.js` for the colour
mode and OS hint, `motion.js` for the motion described below and the
third-party request counter). No framework, no build step, no request to any
other server (the CSP forbids it).

It follows the conventions of the Pluryprint and Versorium landings: the same
section order, bilingual setup, SEO and agent files, and release-filled
download section.

## How it follows the releases

Nothing in `site/` names a version, a date, a download link or a file size.
Those places carry `{{release.*}}` markers inside `<!-- release:begin -->` …
`<!-- release:end -->` blocks, next to `<!-- norelease:begin -->` blocks with
the "not published yet" state.

`scripts/site/apply_release.py` copies `site/` to an output folder and:

- with `--release-json` (the `GET /repos/MAECLY/coverly-roms/releases/latest`
  record), keeps the release blocks and fills every marker; it refuses a
  draft or pre-release, a tag that is not `v<SemVer>`, and any missing,
  ambiguous, empty or misplaced download (the app bundles are matched by
  pattern, like `release_tools.py build_artifacts()`, so the
  `Coverly.ROMs_<v>_…` prefix that `release.yml` stages is not hardcoded);
- with `--no-release`, keeps the norelease blocks and drops every marker.

`.github/workflows/pages.yml` runs `scripts/site/check_site.py`, asks the API
for the latest release, uses `--no-release` only when it answers 404, and
deploys the filled copy. Publishing a release redeploys it (through a
`workflow_dispatch` run on `main`, since the `github-pages` environment only
deploys from `main`). `src-tauri/tests/site_release.rs` tests the script and
the workflow against `scripts/site/fixtures/release.json`.

## Local preview and checks

```bash
make site                                            # before the first release
make site RELEASE=scripts/site/fixtures/release.json # with a (fake) release
make site-check                                      # every gate, both states
```

`check_site.py` builds both states and checks: required files and the CNAME,
the colour tokens against `src/styles/globals.css` (between `tokens:begin` and
`tokens:end`), action pins in `pages.yml`, internal links and `#fragments`,
Spanish/English structural parity, FAQ JSON-LD against the visible FAQ, CSP and
head tags, sized/described/lazy images, size budgets, visible word count,
WCAG contrast of the token pairs and the motion contract (see Motion).

## Motion

The motion follows the Pluryprint and Versorium landings, and the same
rules hold:

- **The HTML is the finished page.** Motion only plays towards it. Text,
  headings and screenshots are never faded or slid in; only decorative
  parts (the hero's gaps, the layout table's slots, the closing mark, the
  annotation strokes) get a starting pose.
- **Two gates.** `theme.js` sets `html.anim` in `<head>`, before the first
  paint, unless the visitor asked for reduced motion or turned on
  Save-Data. `motion.js` adds `html.motion` once its setup worked. Any
  error, or reduced motion switched on mid-visit, runs `halt()`: each
  piece's `stop()` puts it in its finished state and both classes go.
- **Shown, not played.** Anything already on screen at load gets
  `.is-in.no-anim`; a load slower than 2.6 s shows the hero finished.
- **CSS fallbacks.** Without `motion.js`, the hero's covers still arrive at
  4 s. Scroll-linked effects sit inside
  `@supports (animation-timeline: view())`; without it (Firefox, older
  Safari) everything stays finished.
- **Reduced motion** stops every animation, transition and view
  transition, and hides the replay button. Easing tokens are
  `--ease-ink` and `--ease-spring`.

What moves:

| Where | What | Trigger |
| --- | --- | --- |
| Hero shelf | Plays the app's own sequence, with the app's words. **Armed**: five dashed gaps, "7 of 12". **Dry run**: each gap is marked in the accent colour, "Dry run ready: 5 covers, nothing written". **Writing art…**: each cover wipes in as the count reaches 12. **Done**: "5 covers added". Then a ↻ Replay button shows; while it replays it stays in place, dimmed (`aria-disabled`), so keyboard focus stays on it. | 40% of the grid on screen, however long that takes (a phone reader may scroll later). Only a load slower than 2.6 s shows the finished shelf straight away. |
| Header | A reading-progress line under the bar; the Download button appears once the hero's buttons scroll away | Scroll (CSS) |
| Section headlines | The slot under each H2 fills | Scroll (CSS) |
| Four steps, ≥1024 px | One sticky screenshot; each step's text cross-fades its own in (named view timelines `--s1`…`--s4`) | Scroll (CSS) |
| Steps 1 and 2 | An ellipse is drawn around "4553 without cover" and "Nothing has been written yet" | Scroll (CSS) |
| Layout table | Each slot a format can write goes from dashed to solid, row by row | `[data-reveal]` |
| Terminal | Each command is typed in turn. The untyped rest stays in place, hidden, so no glyph moves. | 45% on screen; skipped if already visible |
| Closing | The last slot of a small shelf fills | `[data-reveal]` |
| Colour mode | The new mode spreads from the click (`startViewTransition`); ES/EN switches cross-fade (`@view-transition`) | Click or key |
| Buttons, FAQ | 1 px lift on hover; answers open with a height transition | Hover, open |

`check_site.py`'s `motion` gate holds the contract:

- the gates and `halt()` are present in the scripts;
- a reduced-motion block stops animations, transitions, view transitions
  and the replay button;
- keyframes and transitions only move transform, opacity, visibility,
  clip-path, SVG strokes and colours (the FAQ's height is the one exception,
  since it answers a click);
- `animation-timeline` only appears inside `@supports`;
- every other animation or transition sits behind `html.anim`,
  `html.motion` or `prefers-reduced-motion: no-preference`;
- motion never hides a heading, paragraph, list item, link, button or image,
  neither with a hiding rule nor with keyframes that start at `opacity: 0` or
  `visibility: hidden`: such an animation on text may not be delayed more than
  1 s, and may only fill backwards through a delay when scrolling drives it
  (`animation-range` inside `@supports`);
- no `<video>` or `autoplay`;
- `motion.js` is at most 8,000 bytes.

The budget gate allows 16,000 / 5,500 bytes (raw / gzip) for all scripts and
44,000 / 10,500 for the stylesheet.

**Before merging a motion change, check it in a browser:**

- Chrome at 1280×800 and 360×780, light and dark, with reduced motion
  emulated on and off.
- With reduced motion: `document.getAnimations()` returns nothing, and every
  heading, paragraph and image has opacity 1.
- A performance trace on load: CLS 0 and no long tasks. Measure the layout
  shift after scrolling the whole page too.
- 0 console errors.

**Not done yet: recordings of the real app.** These would be play-once
animated WebP clips, each with a still, as Versorium makes them with
`tests/landing/record.mjs` and `encode-clips.py`. Planned clips:

- Apply: dashed slots turn into covers.
- Undo: the covers go back to slots.

They need:

- a `?mockPause=off` flag in the mock backend, which otherwise pauses after
  25 items;
- Playwright in the dev dependencies;
- per-file size budgets in `check_site.py`: about 110 kB for each desktop
  clip and 75 kB for each phone clip.

## Screenshots and Open Graph cards

The screenshots under `site/assets/shots/{es,en}/` are real captures of the
browser UI with its mock backend (`make web`), whose art is labelled
placeholder art, so no capture shows real box art. Capture each screen at a
1600×1000 viewport with device pixel ratio 2 in light and dark, save them as
`<raw>/<lang>/<screen>-<light|dark>.png` and run
`python3 scripts/site/encode_shots.py <raw>` (needs `cwebp`): it writes the
2560-wide and `@1x` 1280-wide WebP files the `<picture>` elements use.

`scripts/site/og.html` renders the 1200×630 cards from the page's own CSS and
shots: copy it into a filled site, open `og.html?lang=es` or `?lang=en` at a
1200×630 viewport and save the screenshot as `site/assets/og.png` or
`og-en.png`.

The favicon and icons are a temporary glyph (a filled cover slot beside an
empty one) until the mark exists; the page uses the wordmark only
(`TRADEMARKS.md`). The display face is Archivo (OFL-1.1), instanced at width
72 and weight 800 and subset to Latin-1 (`site/assets/fonts/`).

## One-time setup (maintainer)

1. **Make Pages possible.** On the Free plan, GitHub Pages only publishes from
   a public repository, so this waits until `MAECLY/coverly-roms` is public
   (or the account has a plan with private Pages). GitHub Actions must be
   able to run (billing), since Pages deploys from `pages.yml`.
2. **Settings → Pages → Build and deployment → Source: GitHub Actions.**
3. **DNS:** in Cloudflare (the `maecly.com` zone), add a CNAME
   `coverly-roms` → `maecly.github.io`, **DNS only** (grey cloud), as for
   the Versorium and Pluryprint landings. `maecly.com` is already a verified
   domain of the organisation.
4. **Settings → Pages → Custom domain:** `coverly-roms.maecly.com` (also in
   `site/CNAME`), wait for the DNS check, then tick **Enforce HTTPS**.
5. Run **Actions → Pages → Run workflow** once. Until v0.1.0 is published the
   page shows the "not published yet" state; publishing the release redeploys
   it with the downloads.
