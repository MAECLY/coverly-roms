# Brand research

This page collects what we learned before defining the Coverly ROMs brand:
how the retro-gaming tools and the admired desktop utilities build their
identity, which clichés the category is full of, what each platform asks of
an app icon, and how to organise the design tokens. The brand decisions
themselves live in `brand.md`; this page is the evidence behind them.

## Why Coverly reads as generic today

- **The accent is a framework default.** `--cr-accent` is `#4338ca`
  (Tailwind `indigo-700`) in light mode and `#818cf8` (`indigo-400`) in dark.
  KNULLI and ROCKNIX ship the MkDocs Material default `#4051b5`, about 5% away.
  Linear (`#5e6ad2`) and Obsidian (violet) sit next to it too.
- **The status colours are Tailwind defaults** (`#16a34a`, `#dc2626`,
  `yellow-500/600`, and `cyan-700` / `violet-600` / `amber-700` for the source
  badges), hard-coded outside the tokens.
- **There is no mark.** The desktop app shows a white "C" ring on an indigo
  gradient (`src-tauri/icons/*`, `public/favicon.png`); the landing shows a
  filled slot next to a dashed one (`site/assets/favicon.svg`). A user sees
  one in the Dock and the other in the browser tab.
- **The app has no brand type.** It uses the system stack. Only the landing
  has a display face (Archivo Display 800, `wdth` 72).
- **The only real identity is on the landing:** the slot metaphor (solid = has
  art, dashed = missing) and a 12-colour palette of fake cover art
  (`.p1`–`.p12`). Neither reaches the app.
- **Two sources of truth.** `site/assets/site.css` copies the 8 `--cr-*` tokens
  by hand and adds landing-only ones (`--ok`, `--line`, `--dash`, `--code-*`,
  `--shot-*`). `check_site.py` only keeps the 8 shared ones in sync.

## Method

About 30 category sources and 25 utility-app sources were read. For most
brands the live homepage and its CSS were downloaded, the most frequent hex
values and `font-family` declarations counted, and the logo files sampled.

- **(CSS)** means the value comes from the site's production stylesheet
  (fetched 2026-10-17).
- **~** means it was sampled from logo pixels and rounded to steps of 12 per
  channel: close, not official.
- **[3rd-party]** means it comes from a brand-aggregator site and was not
  checked against the product.

## References: retro gaming, ROM and emulation tools

| Product | Icon concept | Palette | Type | Takeaway |
|---|---|---|---|---|
| [ES-DE](https://es-de.org) | Crimson angled tile with a white "ES" monogram and a long flat shadow | logo ~`#9c0018`; web `#fec600` (CSS), links `#1356d9` | Montserrat, Quicksand, Open Sans | Recognisable monogram, but red logo vs yellow web: no single colour. |
| [RetroArch](https://www.retroarch.com) | Pixel invader glyph + outlined art-deco wordmark | Bootstrap 3 defaults `#337ab7`… (CSS) | Hind, Helvetica Neue | Memorable glyph inside a stock template. The team admits it ([#5278](https://github.com/libretro/RetroArch/issues/5278)). |
| [libretro-thumbnails](https://github.com/libretro-thumbnails/libretro-thumbnails) | None | — | — | Pure infrastructure. Useful vocabulary: `Named_Boxarts/Snaps/Titles/Logos`, PNG, 512 px wide, `&*/:?"` → `_`. |
| [Skraper](https://www.skraper.net) | Bevelled lime-green gradient letters | Bootstrap 3 `#337ab7` (CSS) | Questrial | Dated 2000s style, exclamation-heavy voice ("Damn fast!"). |
| [ScreenScraper](https://www.screenscraper.fr) | "SCRAPER" wordmark + animated GIF | gold `#e2ba59` ×1,506, `#f0cf69`, `#725124` (CSS) | Cuprum, Arial | Owns gold, but cluttered. |
| [SteamGridDB](https://www.steamgriddb.com) | 3–4 landscape rectangles fanned like cards | ~`#306c90` ~`#3c84b4` ~`#54b4f0` on ~`#242430`; `theme-color #5fb4f0` | Open Sans | Closest to our grid metaphor and a data source of ours. **Coverly must not use stacked blue rectangles.** |
| [Steam ROM Manager](https://github.com/SteamGridDB/steam-rom-manager) | None of its own | — | — | Casual, joking voice; lives under SteamGridDB. |
| [LaunchBox / Big Box](https://www.launchbox-app.com) | Isometric outlined cube (the "box" = the library) | ~`#b43ca8` ~`#d8fc0c` ~`#fc4800` ~`#30b4fc`; web `#337ab7` | Open Sans | Good idea, loud 90s primaries. |
| [Playnite](https://playnite.link) | Gamepad silhouette, pink-red → orange gradient | ~`#fc2448` → ~`#fc7818`; Bootswatch Slate `#3a3f44` | Helvetica Neue | The category cliché: indistinguishable. |
| [Pegasus](https://pegasus-frontend.org) | No official logo spec found | — | Noto Sans | Forgettable. |
| [OpenEmu](https://openemu.org) | Chrome bevelled 80s script wordmark | `#9c2931`, `#2f0c0f` (CSS) | Inter | Polished, Apple-like voice, but fully synthwave. |
| Delta (iOS) | Stylised Δ, replaced after Adobe complained ([Kotaku](https://kotaku.com/delta-emulator-ios-logo-icon-change-adobe-legal-apple-1851485799), [Digital Trends](https://www.digitaltrends.com/gaming/delta-ios-emulator-app-adobe-logo-change/), [releases](https://github.com/rileytestut/Delta/releases)) | — | — | AGPL like us. Check abstract glyphs against big-company marks, not only Nintendo. |
| [RetroDECK](https://retrodeck.net) | Black circle with "D" bracket + "R" | ~`#b454e4` → ~`#2490fc`; Bootstrap 5 `#0d6efd` | Exo, `--pixel-font`, Nunito Sans | Clean monogram, category gradient. |
| [EmuDeck](https://www.emudeck.com) | Rounded play-triangle with a "×" face button | ~`#b4189c` ~`#783cb4` ~`#0060d8`; `#ec3457` `#ac30d3` (CSS) | Avenir | Well made, same gradient family. |
| [Batocera](https://batocera.org) | Arcade joystick, sunset gradient, scanlines | ~`#f0a848` ~`#f08454`; Bootstrap 4 `#007bff` | system | Three clichés in one mark. |
| [KNULLI](https://knulli.org) | Olive grasshopper on black | ~`#cccc6c` ~`#485418`; MkDocs `#4051b5` | Oxygen, Oxygen Mono | Most distinctive in its group, because it uses **no gaming imagery**. |
| [ROCKNIX](https://rocknix.org) | Type-only, "ROCK" coral + "NIX" grey | ~`#ff5555`; MkDocs `#4051b5` | Oxygen | Calm; the two-tone split is the only device. |
| [MustardOS (muOS)](https://muos.dev) | Gold hexagon badge with a white "M" | `#c7af26` `#fdce68` `#ca7601` (CSS) | system-ui, mono | Owns mustard yellow. |
| [Anbernic](https://www.anbernic.com) | Wide notched techno wordmark | ~`#d80000`, `--button-outline-color #cc1111` | Barlow | Recognisable only by the red on the device. |
| [Retroid](https://www.goretroid.com) | Slanted sporty grey wordmark | grey | — | 2000s sporting goods. |
| [AYN](https://www.ayntec.com) | No logo spec found | `theme-color #333333` | Josefin Sans, Nunito | Neutral, forgettable. |
| Miyoo | — (site returned 403) | — | — | Its community (Onion OS) is better known than its brand. |
| [Analogue](https://www.analogue.co) | Restrained wordmark; 2017 rebrand by [Cory Schmitz](https://coryschmitz.com/Analogue) | greyscale only: `#1b1b1b` `#7f7f7f` `#d9d9d9` `#f3f3f3` (CSS) | Circular XX, Unica 77-style grotesk, `neueBit` as accent only | **The benchmark.** No gaming clichés; "library" and "scholarly" language; colour comes from product photos. |
| [ModRetro](https://modretro.com) | Wide rounded caps wordmark | violet `#733caa`, aubergine `#28233c`, pink `#f802a1` (CSS) | Kallisto, DIN 2014 | Premium "heirloom" tone; political baggage via Anduril ([Retrododo](https://retrododo.com/modretro-chromatic-transparent-plastic-review/)). |
| [RetroAchievements](https://retroachievements.org) | "RA" monogram with inline stroke ([SVG](https://commons.wikimedia.org/wiki/File:RetroAchievements_logo_square.svg)) | R ~`#0c60d8`, A ~`#cc9000` | — (Cloudflare blocked fetch) | Blue + gold trophy feeling. |

## References: admired indie and desktop utilities

| Product | Icon concept | Palette | Type | Takeaway |
|---|---|---|---|---|
| [Raycast](https://www.raycast.com/blog/a-fresh-look-and-feel) | A keycap modelled on the Magic Keyboard; Liquid Glass update in 2025 ([blog](https://www.raycast.com/blog/the-new-raycast)) | `#ff6363` brand only, on near-black; neutral UI (CSS) | Inter, Geist Mono | **One object carries all three surfaces:** keycap icon → keyboard hero → keycap hints in the UI. |
| [Linear](https://linear.app/now/how-we-redesigned-the-linear-ui) | Monochrome mark used as a rounded chip ([brand](https://linear.app/brand)) | `#5e6ad2` / `#7170ff` on `#08090a` (CSS) | Inter Display + Inter | Themes generated in LCH from 3 inputs (base, accent, contrast) instead of 98 variables. |
| Ghostty | Ghost on a CRT inside a device frame, more detail at large sizes ([post](https://x.com/mitchellh/status/1866226936051056719)) | `--brand-color #3551f3` (CSS) | JetBrains Mono, Pretendard | The icon is a picture of the product's own screen; users can recolour it ([discussion](https://github.com/ghostty-org/ghostty/discussions/4296)). |
| [Obsidian](https://obsidian.md/blog/new-obsidian-icon/) | A knapped obsidian shard, legible in one colour, shipped in theme variants | Tailwind violet ramp `#8b5cf6`… (CSS) | Inter | Name-driven metaphor; colour as a theme parameter ([HN](https://news.ycombinator.com/item?id=36171747)). |
| [Zed](https://zed.dev/brand) | Mark with an optional slight gradient; separate Stable and Preview icons | `#1348DC` + black + white, also as `lab()` (CSS) | IBM Plex Sans, Lilex ([fonts](https://github.com/zed-industries/zed-fonts)) | A strict three-colour rule and channel-specific icons. |
| Things 3 | Checkmark in a white box on blue ([gallery](https://www.iosicongallery.com/icons/things-3-2025-11-06/)) | `#2576eb` + cool neutrals (CSS); older `#2F7EF0` [3rd-party] | system UI only | The icon is the app's main interaction. Two Apple Design Awards ([Wikipedia](https://en.wikipedia.org/wiki/Things_(software))). |
| [Tailscale](https://together.agency/work/tailscale/) | A dot grid with some dots filled | warm neutrals `#faf9f8` `#eeebea` `#2e2d2d`, accent `#3f5db3` (CSS) | Inter, custom mono | **Closest analogue to our slot grid:** the logo is also the background supergraphic ([analysis](https://design.withfudge.com/share/tailscale.com-design)). |
| CleanShot X | Sticker peeling back over a dark square [3rd-party] ([source](https://www.jordanharbinger.com/sponsor/cleanshot-x-for-mac/)) | — | Google Sans Flex | The icon shows the act the app performs. |
| Arc | "A"/arc letterform ([Figma](https://www.figma.com/community/file/1124408666561142028/arc-icon-by-the-browser-company)) | sources disagree ([loftlyy](https://www.loftlyy.com/en/arc-browser)) | — | A multi-colour personality is hard to keep coherent. |
| IINA | Redrawn for Big Sur ([ifun](https://www.ifun.de/iina-1-1-freier-mac-videoplayer-erreicht-meilenstein-160863/)) | warm off-whites `#f7f6f3`, navy `#162636`, amber `#c16407` (CSS) | Rubik | Warm paper + one accent works for a utility. |
| Transmission | Icon by Hugo van Heuven; shows transfer rate on the Dock ([ifun](https://www.ifun.de/bittorrent-mac-app-transmission-erscheint-in-version-3-0-154392/)) | — | — | An icon off the platform grid looks wrong ([forum](https://forum.transmissionbt.com/viewtopic.php?p=71022)); the icon can carry live state. |
| Bruno | Named after the creator's dog ([source](https://www.ministryoftesting.com/insights/looking-for-a-new-client-for-testing-apis-presenting-bruno)) | `#1E59FF` (CSS) | Geist | A name and a mascot give warmth in a dry category. |

## Patterns across both sets

1. **Most open-source web presences are stock framework themes**: Bootstrap 3
   (`#337ab7`), 4 (`#007bff`), 5 (`#0d6efd`), Bootswatch Slate, MkDocs Material
   (`#4051b5`). Our indigo belongs to this family.
2. **Logo colours already taken:** red (ES-DE, Anbernic, ROCKNIX), blue
   (SteamGridDB, RetroAchievements, Zed, Things), purple/magenta → blue
   gradients (RetroDECK, EmuDeck, OpenEmu, ModRetro), pink → orange sunsets
   (Playnite, Batocera), yellow/gold/mustard (ES-DE web, MustardOS,
   ScreenScraper), lime/olive (Skraper, KNULLI), violet (Obsidian), indigo
   (Linear).
3. **The best icons show the product's own object or action**: Raycast's
   keycap, Things' checkbox, Ghostty's terminal, Tailscale's dot grid. Not an
   abstract letter.
4. **One accent, many neutrals.** The accent marks brand, focus and the
   primary action, never decoration (Linear, Zed, Things, Raycast, Tailscale).
5. **Tokens are generated, not hand-picked** (Linear in LCH, Zed in `lab()`,
   Obsidian as a 50–950 ramp).
6. **The landing reuses the icon's metaphor as its main graphic and shows real
   app UI** (Raycast's keyboard, Ghostty's terminal, Tailscale's dot grid).
7. **Type stays modest**: one display face, a neutral UI sans, and often a
   monospace as the voice for data. Only Analogue and ModRetro pay for type,
   and they are the premium brands.
8. **Voice:** open-source tools use exclamation-heavy fan enthusiasm or flat
   feature lists; premium brands use reverent preservation language. **Nobody
   speaks calmly and carefully about touching the user's files**, which is
   exactly what simulate → apply → undo does.
9. **The icon must survive in one colour** (Obsidian; Tahoe Tinted/Clear) and
   **detail varies by size** (Ghostty; Windows' hand-tuned 16/24/32).

## Clichés to avoid

| Cliché | Who uses it | Why to avoid it |
|---|---|---|
| Gamepad or controller silhouette | Playnite, libretro RetroPad ([#110](https://github.com/libretro/retroarch-assets/issues/110)) | Instantly generic; modern pads echo Xbox/PlayStation trade dress. |
| D-pad cross, ABXY face buttons | EmuDeck "×", many themes | Nintendo's cross D-pad is iconic company design: tired and legally unclear. |
| Game Boy shape, notched grey cartridge | Anbernic devices, ModRetro | Nintendo holds shape registrations for the Game Boy (US 5,958,568 and 5,604,523) ([thatsatrademark](https://www.thatsatrademark.com/blog/nintendo-handheld-consoles)). A Nintendo-style cartridge is a similar risk. |
| Joystick / arcade stick | Batocera | Default arcade imagery. |
| Pixel fonts, 8-bit sprites, invader glyphs | RetroArch, RetroDECK `--pixel-font` | Read as a substitute for substance ([Medium](https://medium.com/mr-plan-publication/the-pixelated-paradox-why-our-obsession-with-retro-gaming-might-erase-its-legacy-9f3b99ca0e82), [datagubbe](https://datagubbe.se/crt/)). Analogue limits it to one accent face. |
| CRT scanlines, curvature, glow | Batocera | Overused and does not recreate the real look ([GamesRadar](https://www.gamesradar.com/hardware/retro/what-are-scanlines/)). |
| Synthwave: neon cyan/magenta, chrome, sunsets | OpenEmu, Batocera, Playnite, EmuDeck | The category default ([StyleShift](https://styleshift.design/styles/synthwave)). |
| Purple → blue "tech" gradients | RetroDECK, EmuDeck | Three direct neighbours already use them. |
| Abstract letter glyph without a check | Delta | Adobe forced a change. Check against big-company marks. |
| Nintendo/Sega/Sony names or system logos in our own marks | — | Nintendo enforces hard: the Yuzu settlement ($2.4M and the domain) and 8,535 repos hit by DMCA ([Game Developer](https://www.gamedeveloper.com/business/nintendo-hits-8-500-yuzu-repositories-with-dmca-takedowns), [Wikipedia](https://en.wikipedia.org/wiki/Yuzu_(emulator))). Those were copyright cases, but they show the appetite. System logos stay inside the user's library, never in our icon, OG images or store assets. |

## Platform icon rules

### macOS 26 Tahoe (Liquid Glass)

- iPhone, iPad and Mac share a **1024 px canvas** with a rounded-rectangle
  shape ([WWDC25 session 361](https://developer.apple.com/videos/play/wwdc2025/361/)).
- Pre-Tahoe grid, still used for the `.icns` fallback: an **824×824 body** with
  a **185.4 px** continuous-corner radius, a 100 px gutter, and a suggested
  shadow of 28 px blur, 12 px down, black at 50%. These are community numbers,
  not Apple docs ([forum](https://developer.apple.com/forums/thread/670578),
  [uxdesign](https://bootcamp.uxdesign.cc/creating-app-icons-for-macos-11-and-up-1132ccd479bc)).
- **Icon jail:** a non-squircle icon is drawn inside a grey squircle
  ([lapcatsoftware](https://lapcatsoftware.com/articles/2025/6/2.html),
  [9to5mac](https://9to5mac.com/2025/08/07/macos-26-icon-changes/),
  [Wikipedia](https://en.wikipedia.org/wiki/MacOS_Tahoe)). We infer this also
  hits Tauri apps that ship only a non-conforming `.icns`.
- **Icon Composer `.icon`:** up to 4 groups of 1–4 layers. Export flat, opaque
  SVG (preferred) or PNG at canvas size, text converted to outlines. Do not
  bake in blur, shadow, specular, translucency, background gradients or the
  mask. Modes: Default, Dark, Clear (light/dark), Tinted (light/dark); for
  mono/tinted make the main element white and the rest greys. One colour per
  layer, so a dark variant is one fill change. Per-size hinting on Mac is no
  longer necessary.
- **Shipping it** ([successfulsoftware](https://successfulsoftware.net/2025/09/26/updating-application-icons-for-macos-26-tahoe-and-liquid-glass/)):
  compile the `.icon` to `Assets.car` with `actool` (Xcode 26, macOS 26 SDK),
  set `CFBundleIconName` in `Info.plist`, and put **both** `Assets.car` and the
  legacy `.icns` in `Resources`.
- **Tauri v2** ([docs](https://v2.tauri.app/develop/icons/)): `tauri icon`
  takes a square PNG/SVG and generates `icon.icns`, `icon.ico` (16, 24, 32, 48,
  64, 256, with 32 first) and the PNGs. It has no `.icon`/`Assets.car` step, so
  a Tahoe-native icon needs a manual `actool` step (inference).

### Windows 11

Source: [app icon construction](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction).

- ICO minimum: **16, 24, 32, 48, 256**. Windows takes an exact match or scales
  down from the next size up.
- Sizes by scale factor:

  | Surface | 100% | 125% | 150% | 200% | 250% | 300% | 400% |
  |---|---|---|---|---|---|---|---|
  | Context menu, title bar, tray | 16 | 20 | 24 | 32 | 40 | 48 | 64 |
  | Taskbar | 24 | 30 | 36 | 48 | 60 | 72 | 96 |
  | Start pins | 32 | 40 | 48 | 64 | 80 | 96 | 256 |

- Transparent backgrounds work best; a plated icon needs light and dark plates.
- MSIX needs `AppList.targetsize-*` PNGs plus `_altform-unplated` and
  `_altform-lightunplated`, or the icon gets "an undesirable backplate".
- Hand-tune 16, 24 and 32 instead of downscaling 256
  ([creativefreedom](https://creativefreedom.co.uk/icon-designers-blog/windows-ico-made-simple/)).

### Linux (hicolor, Flatpak)

- PNG or SVG, square, named with the **app ID**, under
  `share/icons/hicolor/<size>/apps/` and `scalable/apps/`. Max 512×512
  ([Flatpak](https://docs.flatpak.org/en/latest/conventions.html),
  [icon theme spec](https://specifications.freedesktop.org/icon-theme/latest/)).
- Usual set: 16, 24, 32, 48, 64, 128, 256, 512 + scalable.

### Web: favicon, touch icon, PWA

Source: [Evil Martians](https://evilmartians.com/chronicles/how-to-favicon-in-2021-six-files-that-fit-most-needs).

- `favicon.ico` (32, optional 16 layer), `icon.svg`, `apple-touch-icon.png`
  (180), `icon-192.png`, `icon-512.png`, a 512 maskable icon, and
  `manifest.webmanifest`.
- Dark mode inside the SVG with a `prefers-color-scheme` `<style>`.
- Apple touch icon: logo at about **140 px centred on 180**, **opaque**
  background (transparency renders black).
- Maskable: content inside a centred circle of **radius 40%** of the width;
  a separate `"purpose": "maskable"` entry, not `"any maskable"`
  ([web.dev](https://web.dev/articles/maskable-icon)).
- At 16 px only 2–3 shapes survive, so draw a simplified favicon glyph.
- Sources disagree on Safari SVG favicons: always ship the ICO/PNG fallback.

### Icon deliverables for Coverly

- Master 1024 SVG on the 824 body, 185.4 radius.
- Tahoe `.icon` from flat layers (background, slot grid, the slot being filled,
  optional highlight) + `.icns` fallback, `Assets.car`, `CFBundleIconName`.
- ICO with 16, 24, 32, 48, 64 and 256 (32 first), hand-tuned small frames.
- Linux hicolor PNGs 32–512 + scalable SVG named with the app ID.
- Web set above, generated by **one script** from the same master, so the
  Dock icon and the browser tab can no longer differ.
- Optional Stable and Nightly variants, as Zed does.

## Token system recommendation

### Standards and building blocks

- **W3C DTCG 2025.10** is the first stable format
  ([format](https://www.designtokens.org/tr/2025.10/format/),
  [color](https://www.designtokens.org/tr/2025.10/color/)): `.tokens.json`,
  `$value` / `$type` / `$description` / `$extensions`, `{alias}` references,
  `$extends` for theme variants, colours as `{colorSpace, components, alpha,
  hex}`. Write colours as `oklch` with a `hex` fallback.
- **Tooling:** Style Dictionary v4 does not fully support 2025.10 yet
  ([docs](https://styledictionary.com/info/dtcg/)); Terrazzo claims full
  support ([docs](https://terrazzo.app/docs/)). For our small set a
  dependency-free Node script (~150 lines) is simpler; use `tz check` only as
  a validator, if at all.
- **Radix 12-step scales**
  ([scale](https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale),
  [composition](https://www.radix-ui.com/colors/docs/palette-composition/composing-a-palette)):
  1–2 backgrounds, 3–5 element states, 6–8 borders and rings, 9–10 solid
  fills, 11–12 text. Step 11 reaches APCA Lc 60 and step 12 Lc 90 on step 2.
  Our warm grey is close to **Sand** (`#fdfdfc … #21201c`, dark `#111110 …
  #eeeeec`).
- **Material 3** ([Color.md](https://github.com/material-components/material-components-android/blob/master/docs/theming/Color.md),
  [roles](https://m3.material.io/styles/color/roles)): dark mode is a
  separate mapping (primary tone 40 → 80), elevation is a ladder of surface
  tones, and a "container" role is the tinted low-emphasis accent.
- **Apple HIG** ([color](https://developer.apple.com/design/human-interface-guidelines/color)):
  name text by emphasis level and keep the tint for interactive elements only.
- **OKLCH** ([Evil Martians](https://evilmartians.com/chronicles/oklch-in-css-why-quit-rgb-hsl)):
  perceptual lightness gives predictable contrast across hues. Taper chroma at
  the ends of a scale and gamut-map in the build. `oklch()` needs Safari 15.4+;
  relative colour syntax needs Safari 18+, and Tauri on macOS 12 or older
  WebKitGTK may not have it ([Tauri webviews](https://v2.tauri.app/reference/webview-versions/)).
  **Author in OKLCH, emit static sRGB, and keep `oklch()` only in a P3 block.**
- **Tailwind 3** needs channel values for opacity modifiers:
  `rgb(var(--x) / <alpha-value>)`
  ([customizing colors](https://v3.tailwindcss.com/docs/customizing-colors),
  [v3.1](https://tailwindcss.com/blog/tailwindcss-v3-1)).
- **shadcn/ui** ([theming](https://ui.shadcn.com/docs/theming)) uses surface +
  `-foreground` pairs. Components only use class names (`bg-primary`,
  `ring-ring`…), so we can map those names onto `--cr-*` in
  `tailwind.config.ts` and keep our prefix.
- **Accessibility:** WCAG 2.2 AA is the gate — 4.5:1 text, 3:1 large text,
  3:1 for control boundaries, focus indicators and **the dashed missing-slot
  outline** ([1.4.3](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html),
  [1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html),
  [2.4.13](https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html)).
  APCA is an informational second check, mainly for dark mode
  ([APCA](https://apcacontrast.com/), [intro](https://git.apcacontrast.com/documentation/APCAeasyIntro.html),
  [bronze](https://www.readtech.org/ARC/tests/bronze-simple-mode/)).
- **Motion, radius, spacing, elevation:** Carbon durations 70–700 ms and
  productive easings ([Carbon](https://carbondesignsystem.com/elements/motion/overview/));
  Primer radii 3/6/12 and control heights 24–48
  ([Primer](https://primer.style/foundations/primitives/size)); Atlassian's
  sunken / default / raised / overlay surfaces, each with a shadow
  ([Atlassian](https://atlassian.design/foundations/elevation)).

### Contrast of what we have

| Pair | Ratio | Result |
|---|---|---|
| Accent `#4338ca` on white | 7.9:1 | Passes |
| Dark accent `#818cf8` on `#141413` | 6.18:1 | Passes |
| Muted text `#5f5c58` on `#f7f7f5` | 6.2:1 | Passes |
| Border `#e3e1dd` on `#f7f7f5` | 1.22:1 | Fine as a separator; fails 3:1 for control borders |
| Landing `--dash` `#a8a49e` on `#f7f7f5` | ~2.3:1 | **Fails 3:1** for the brand's key graphic |
| White on `#16a34a` (`.art-ws__inuse`) | 3.3:1 | **Fails AA** |
| `#dc2626` on the dark surface | 3.45:1 | Weak |

Candidate values: sand-9 `#8d8d86` on sand-1 reaches 3.28:1 (enough for
inputs and dashes; sand-8 is only 1.89:1); sand-11 `#63635e` on sand-2 is
5.73:1; amber-9 `#ffc53d` takes dark text (10.33:1); teal-9 `#12a594` with
white text is only 3.07:1 and teal-11 `#008573` as text is 4.48:1 (fails by a
hair), so a teal accent must be darkened for text.

### Recommended architecture

```
tokens/
  primitives.tokens.json    # sand 1–12, brand 1–12, status scales; radius, duration, easing, font
  theme.light.tokens.json   # semantic aliases → primitives
  theme.dark.tokens.json    # same keys, $extends light where equal
scripts/build-tokens.mjs    # resolve aliases, OKLCH → sRGB, contrast gate, emit
src/styles/tokens.css       # generated, committed (app)
site/assets/tokens.css      # generated, committed (landing)
```

- **One source, two consumers.** The landing stops copying tokens by hand and
  `check_site.py`'s partial sync check is replaced by `npm run tokens --check`,
  which fails when a generated file is stale or a declared contrast pair misses
  its target.
- **Three tiers.** Primitives (`--cr-sand-1…12`, `--cr-brand-1…12`, status
  scales) are never used in components. Semantic tokens are:
  `--cr-bg`, `--cr-surface`, `--cr-surface-raised`, `--cr-surface-sunken`,
  `--cr-hover`, `--cr-active`, `--cr-text`, `--cr-text-secondary`,
  `--cr-text-disabled`, `--cr-border`, `--cr-border-strong` / `--cr-input`,
  `--cr-primary` (+ `-fg`, `-hover`, `-subtle`, `-subtle-fg`), `--cr-ring`,
  `--cr-success` / `--cr-warning` / `--cr-danger` / `--cr-info` (+ `-fg`,
  `-subtle`), **`--cr-slot-filled` / `--cr-slot-missing`**, `--cr-code-bg` /
  `--cr-code-fg`, `--cr-chart-1…5`. Non-colour: `--cr-radius-sm/md/lg/xl/full`
  (4/6/8/12/9999 px), `--cr-control-sm/md/lg` (28/32/40 px),
  `--cr-duration-fast/base/slow/slower` (110/150/240/400 ms),
  `--cr-ease-standard/enter/exit/ink/spring`, `--cr-shadow-raised/overlay`,
  `--cr-font-display/body/mono`.
- **Naming:** keep `--cr-`; kebab-case, category → role → variant → state;
  JSON path = CSS name. Fold `--ok`, `--line`, `--dash`, `--shot-*` into this
  namespace and delete them.
- **Emitted CSS:** RGB channels for Tailwind plus `-c` full colours for plain
  CSS; dark under `:root[data-theme='dark']` and generated (never hand-copied)
  under `prefers-color-scheme`; an optional `@media (color-gamut: p3)` block;
  durations collapsed under `prefers-reduced-motion`.
- **Contrast gate in the build** (both themes): text on bg/surface/raised
  ≥ 4.5; text-secondary ≥ 4.5; primary-fg on primary ≥ 4.5; subtle-fg on
  subtle ≥ 4.5; input and slot-missing ≥ 3; ring ≥ 3; each status `-fg` on its
  fill ≥ 4.5; APCA as a report.
- **Focus:** `:focus-visible { outline: 2px solid var(--cr-ring);
  outline-offset: 2px }` everywhere; never `outline: none` without a
  replacement.
- **Dark mode is designed, not inverted:** no pure black or white, lighter
  surfaces for higher elevation, a lighter and slightly desaturated accent,
  status colours re-checked, shadows kept.

## Whitespace Coverly can own

1. **The empty slot as the signature.** Every competitor shows finished art;
   nobody shows what is missing. A dashed outline that turns solid can be the
   logo, the loading state, the empty state and the success animation. Use one
   slot or a flat 2×2 / 3×3 grid with exactly one dashed cell — never a fanned
   stack of blue rectangles (SteamGridDB).
2. **A calm library voice.** "Nothing is written until you say so." "Every
   change can be undone." Analogue proves the audience likes reverent, careful
   language; no open-source tool uses it. Warm, plain, no exclamation marks.
3. **A warm neutral base.** Everyone else uses cold slate or pure black and
   white. Our `#f7f7f5` / `#1c1b1a` / `#141413` already differ: make them
   deliberate — paper, ink, archive-box board.
4. **An accent nobody owns.** Red, blue, purple, magenta, orange, gold and
   lime are taken. Open: **deep teal / jade / ink-green**, and **warm vermilion
   only as a small "label tape" mark**. Teal on warm paper is unique in this
   set. Drop indigo.
5. **Type with character, without pixels.** A grotesk or condensed
   "catalogue label" face (the Archivo family the landing already uses) plus a
   monospace for paths and counts, shared by app and landing. A pixel face at
   most as a tiny easter egg, never in the logo.
6. **Objects from libraries, not consoles:** index cards, shelf and spine
   labels, contact sheets, the corner of a matted photo. They say "curation"
   with no trademark risk. KNULLI shows a non-gaming emblem can be the most
   distinctive mark in its group.
7. **State-based colour as a system:** missing = dashed, muted; filled = solid
   ink; applied = accent. No competitor has a state language. Make
   "missing → filled" the rhythm across icon, landing and app, and optionally a
   live Dock badge with the missing count (as Transmission shows its rate).
8. **One object on every surface,** as Raycast does with its keycap: the slot
   is the icon, the landing supergraphic (as Tailscale's dot grid), the
   `SlotMark` and `EmptyState` components, and the art placeholder in the app.

## Gaps

- Miyoo (403) and RetroAchievements (Cloudflare) web colours are unverified.
- No logo specs found for Pegasus, AYN or Miyoo.
- `~` values are pixel samples, not official brand values.
- m3.material.io does not render through fetch; M3 tones come from the
  Android `Color.md` and motion values from secondary sources. The Apple HIG
  page was only partly readable.
- The Tailwind `darkMode: ['selector', …]` option (≥ 3.4.1) was recalled, not
  re-fetched. It is optional anyway: `src/lib/theme.ts` already sets both
  `.dark` and `data-theme`, so `darkMode: 'class'` works.
- No design write-ups were found for Sindre Sorhus' apps, Hand Mirror or the
  Tauri showcase apps.
