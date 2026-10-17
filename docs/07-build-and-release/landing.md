# Landing page

The public page at **https://coverly-roms.maecly.com** is the static site in
`site/`: Spanish at `/` (also `x-default`), English at `/en/`, plus `404.html`,
`robots.txt`, `sitemap.xml`, `llms.txt`, `agents.txt` and the web manifests.
Plain HTML, one stylesheet and two small scripts (`theme.js` for the colour
mode and OS hint, `motion.js` for the hero's single animation and the
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
head tags, sized/described/lazy images, size budgets, visible word count and
WCAG contrast of the token pairs.

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
