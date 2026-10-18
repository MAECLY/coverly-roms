# Coverly ROMs brand: "Buckram & Manila"

Status: **approved direction, v1** (2026-10-17). The colours, type and rules in this document are final. The icon is chosen by the owner from 10 candidates (see [Icon](#9-icon)).

Machine-readable source of truth: [`design/tokens.json`](../../design/tokens.json) (W3C DTCG 2025.10). Every hex and contrast ratio below was generated and checked by the same build script. If this document and `tokens.json` disagree, `tokens.json` wins.

---

## 1. Concept and story

**Coverly ROMs is a collector's shelf, not a gaming app.**

The product does one narrow, careful job. It finds the games in a library that have no cover art and fills only those gaps. It shows the plan first (simulate), writes only when you say so (apply), and can take every change back (undo). The audience is hobbyists who curate their handheld libraries the way other people curate record shelves or photo albums.

So the brand borrows from the objects a careful curator handles, not from consoles:

- **Buckram.** The petrol cloth on library-bound volumes. It is the primary colour, used for actions, focus and "Coverly did this".
- **Manila.** Label tape, shelf tags, file folders. A warm yellow used only as a small filled tag with dark ink.
- **Greyboard.** The board of archive boxes. Neutrals with a faint green-grey tint, not cream and not cold slate.
- **The empty mount.** A photo album page where a print is missing: four paper photo-corners around a blank space. This is exactly what Coverly fixes.

### Why this is ownable (research summary)

From the competitor research (ES-DE, RetroArch, Skraper, ScreenScraper, SteamGridDB, LaunchBox, Playnite, RetroDECK, EmuDeck, Batocera, KNULLI, ROCKNIX, MustardOS, Anbernic, Analogue, ModRetro, RetroAchievements):

- **Colours already taken:** red (ES-DE, Anbernic), blue (SteamGridDB, Bootstrap defaults), purple-to-blue gradients (RetroDECK, EmuDeck, OpenEmu), sunset pink/orange (Playnite, Batocera), gold/mustard (MustardOS, ScreenScraper), lime/olive (Skraper, KNULLI). **Petrol (hue about 217) is unclaimed.**
- **The old indigo `#4338ca` is the problem.** It is Tailwind `indigo-700`, about 5% away from the MkDocs Material default `#4051b5` that KNULLI and ROCKNIX use. It reads as a template colour, which is why the product felt like it had no identity.
- **Icon clichés avoided:** gamepads, D-pads, joysticks, cartridges, Game Boy shapes, pixel fonts, CRT scanlines, synthwave chrome, letter glyphs (Delta had to change its Δ after Adobe objected).
- **Voice gap:** competitors use gamer hype ("Damn fast!") or flat feature lists. Nobody uses calm, careful language, even though the work touches the user's files. Analogue proves premium audiences like a reverent, archival tone.
- **Pattern from admired utility apps** (Raycast, Things, Tailscale, Ghostty): one concrete object from the product's own world becomes the icon, the landing's main graphic and an in-app UI element. For Coverly that object is **the slot**.

---

## 2. The signature: the Slot

One shape system, used on every surface. A slot is a **3:4 portrait rectangle**, the proportion of box art.

| State | Drawing | Token | Meaning |
|---|---|---|---|
| **Missing** | 1.5px dashed outline (dash 4, gap 3) plus four small solid photo-corners | `--cr-slot-missing` | No art yet. 3.70:1 light, 4.60:1 dark, so it passes WCAG 1.4.11 |
| **Filled** | Solid cover image (or solid `--cr-slot-filled` ink in illustrations) | `--cr-slot-filled` | Has art, not touched by Coverly |
| **Applied this run** | A small Manila corner tab on the top-right, mono source label (`libretro`, `SGDB`, `SS`) | `--cr-slot-applied-run` | Filled by Coverly in the current run. Undo "peels it off" |
| **Applied and committed** | 3px Buckram band along the bottom edge, like a spine label | `--cr-slot-applied` | Filled by Coverly, run finished |

The rule is: **dashed = not yet, solid = done, Buckram = Coverly did it, Manila tab = you can still undo it.** No other UI element uses these four treatments, so the meaning stays clear.

### Where the slot appears

- **App icon:** a 2×2 grid of slots, one empty (see [Icon](#9-icon)).
- **App UI:** art placeholder, empty state, skeleton loader (dash draws in, then fills), success animation, progress (a row of slots filling left to right replaces the native `<progress>`), and the checkbox (a tiny mount: dashed when off, solid Buckram with a check when on).
- **Landing:** hero shelf supergraphic, section-heading underline (a dashed rule that turns solid on scroll), closing mark, OG image.
- **Dock badge (optional):** missing-art count in mono on a filled Manila tag.

### Supporting motif: the shelf rule

A 1px `--cr-border` line with a 3px Buckram cap on the left. Used on section headers and table group rows, like the edge strip of a shelf.

### Implementation rules

- **Draw dashes with SVG or a `background-image` pattern, not `border-style: dashed`.** Grids hold 1000+ virtualised cells, and CSS dashed borders are slow and render inconsistently between WebKit and Chromium.
- **Photo-corners appear only at 24px and larger.** Below that, the missing slot is a plain 1px outline.
- **Overlaps are drawn in solid ink, never with blend modes,** so they look the same in every webview.

### Landing cover inks (exception)

**Landing only, never in the app UI.** The illustrated covers on the landing
page (the generated sprite `site/assets/art/covers.svg`, made by
`scripts/site/gen_covers.mjs`, both in
[MAECLY/coverly-roms-landing](https://github.com/MAECLY/coverly-roms-landing)) may each carry **one muted ink**, in two
strengths over Paper: the key layer at full ink and the tone layer at 28%.
The inks come from the primitives `info.9`, `success.9`, `warning.11`,
`danger.9`, `greyboard.11` and `greyboard.9` (and their `-dark`
counterparts in dark mode), muted 20% toward `greyboard.11`
(`greyboard-dark.11` in dark) in OKLab. They are classes `.ink-0` to
`.ink-5`, set per cover on the page.

- Covers are illustrations, not status: an ink never means success, warning
  or error, and nothing in the page relies on it.
- **Buckram stays for spines and rules only, Manila for tabs only.** Neither
  is ever a cover ink.
- Slot outlines keep `--cr-slot-missing`, at 3:1 or more against what they
  sit on.
- The art is geometric and IP-safe: invented titles as seeds, no console
  silhouettes, pixel fonts, CRT effects or real box art.
- Fallback, if this exception is withdrawn:
  `node scripts/site/gen_covers.mjs --palette ink` regenerates every cover
  in greyboard only, with no other change to the page.

Status: proposed with the landing showcase; it needs the owner's sign-off on
the contact sheet (`make site-covers CONTACT=<file.html>` in the landing
repository).

---

## 3. Logo and wordmark

### Wordmark

`COVERLY` set in **Archivo, wdth 75, wght 800, letter-spacing -0.01em**, followed by **`ROMs`** set in **Archivo wdth 100, wght 600** inside a **filled Manila tag** (`--cr-manila` fill, `--cr-manila-fg` text, radius `--cr-radius-sm` 3px, horizontal padding 0.3em).

- On dark backgrounds the tag stays Manila (`#E0BC6C`) with the same ink (`#2A2111`).
- One-colour version (embossing, single-ink print): the tag becomes a 1.5px outlined box in the same ink as `COVERLY`.
- The same Archivo woff2 ships in the app and the landing, so the wordmark is identical on both.

### Lockups

1. **Horizontal (default):** icon on the left, wordmark on the right. Icon height = 1.4 × cap height of `COVERLY`. Gap = 0.5 × icon height.
2. **Wordmark only:** title bars, footers, narrow nav.
3. **Icon only:** app icon, favicon, social avatar.

### Clear space

Keep a margin of at least **the height of the `ROMs` tag** on every side of the wordmark, and **25% of the icon width** around the icon. Nothing (text, edges, other logos) enters that zone.

### Minimum sizes

| Asset | Screen | Print |
|---|---|---|
| Wordmark | 72px wide (tag text stays ≥ 9px) | 20mm |
| Horizontal lockup | 112px wide | 30mm |
| Icon (full detail with photo-corners) | 128px | 15mm |
| Icon (simplified glyph) | 16px | 5mm |

### Don'ts

- Do not recolour `COVERLY` in Buckram or any status colour. It is ink (`--cr-text`) or paper (`#FFFFFF` / `--cr-text` dark) only.
- Do not put `ROMs` in plain text or in the accent colour (the old style). It always sits in the tag.
- Do not stretch, outline, add shadows, glows, gradients, bevels or chrome.
- Do not use a pixel or bitmap font for the wordmark, ever.
- Do not set the wordmark in the expanded Archivo widths (wdth > 100). It must fit title bars.
- Do not place the wordmark on busy box art without a solid plate.
- Do not combine the mark with console-maker names or system logos (Nintendo, Sega, Sony and others). System names appear only as library data inside the UI.
- Do not use Manila for the whole background behind the logo.

---

## 4. Colour

### 4.1 Principles

1. **Neutrals do almost all the work.** Greyboard paper and ink cover about 90% of any screen.
2. **Buckram is the only interactive colour.** It means "you can act on this" or "Coverly did this": primary buttons, links, focus ring, selected state, applied band. Never decoration.
3. **Manila is a tag, never text on paper and never a status.** It appears as a filled chip with dark ink: the `ROMs` tag, the applied-this-run tab, source labels, the count badge. Keep it under about 2% of a screen.
4. **Status colours always come with an icon.** Never rely on colour alone (petrol vs info blue and success green can be confused by colour-blind users).
5. **Missing is shown by the dashed outline, not by colour.** Red is for errors only.
6. **Dark mode is designed, not inverted.** The accent lightens (`#0E5E6F` → `#5DB6C4`) and takes dark ink text. Higher surfaces are lighter.

### 4.2 Contrast targets (enforced by the build)

- Text roles on bg, surface, raised and sunken: **≥ 4.5:1** (WCAG 2.2 AA 1.4.3).
- Input borders, missing-slot dashes, focus ring: **≥ 3:1** against every surface they sit on (WCAG 1.4.11).
- `--cr-border` (1.31:1 light) is **decorative only**. Every input, checkbox, radio and slot outline uses `--cr-border-strong`.
- APCA is a secondary, informational check for dark mode.

### 4.3 Semantic palette

"vs bg" is the WCAG 2 ratio against that theme's `--cr-bg`. It matters for roles marked (text) or (UI). Fills (surfaces, subtle tints, Manila) are listed for reference. The `code-*` roles sit on the always-dark terminal block in both themes, so their light-mode "vs bg" number is irrelevant; see the checks table for `code-*` on `code-bg`.

### Light

| Role (`--cr-*`) | Primitive | Hex | OKLCH | vs bg |
|---|---|---|---|---|
| `bg` | `greyboard.2` | `#F2F3EF` | `oklch(0.962 0.005 117.9)` | — |
| `surface` | `greyboard.1` | `#FBFBF9` | `oklch(0.988 0.003 106.4)` | 1.08:1 |
| `surface-raised` | `white.1` | `#FFFFFF` | `oklch(1.000 0.000 89.9)` | 1.11:1 |
| `surface-sunken` | `greyboard.3` | `#E7E9E3` | `oklch(0.931 0.008 121.6)` | 1.10:1 |
| `hover` | `greyboard.4` | `#E0E3DC` | `oklch(0.912 0.010 125.7)` | 1.16:1 |
| `active` | `greyboard.5` | `#DADDD5` | `oklch(0.893 0.011 123.5)` | 1.23:1 |
| `border` | `greyboard.6` | `#D3D7CF` | `oklch(0.874 0.012 128.6)` | 1.31:1 |
| `border-strong` | `greyboard.9` | `#787F76` | `oklch(0.588 0.016 138.7)` | 3.70:1 (UI, ≥3) |
| `text` | `greyboard.12` | `#1B1E1B` | `oklch(0.231 0.007 145.3)` | 15.10:1 (text) |
| `text-secondary` | `greyboard.11` | `#535A52` | `oklch(0.459 0.016 141.7)` | 6.38:1 (text) |
| `text-disabled` | `greyboard.9` | `#787F76` | `oklch(0.588 0.016 138.7)` | 3.70:1 |
| `primary` | `buckram.9` | `#0E5E6F` | `oklch(0.446 0.075 216.7)` | 6.61:1 (text) |
| `primary-hover` | `buckram.10` | `#0A4D5B` | `oklch(0.389 0.065 216.3)` | 8.47:1 |
| `primary-fg` | `white.1` | `#FFFFFF` | `oklch(1.000 0.000 89.9)` | 1.11:1 |
| `primary-subtle` | `buckram.3` | `#DCEBEC` | `oklch(0.929 0.016 201.7)` | 1.10:1 |
| `primary-subtle-fg` | `buckram.11` | `#084551` | `oklch(0.361 0.060 215.1)` | 9.51:1 (text) |
| `ring` | `buckram.9` | `#0E5E6F` | `oklch(0.446 0.075 216.7)` | 6.61:1 (UI, ≥3) |
| `manila` | `manila.9` | `#E6C171` | `oklch(0.826 0.108 85.3)` | 1.54:1 |
| `manila-fg` | `manila.12` | `#2A2111` | `oklch(0.254 0.030 81.1)` | 14.23:1 |
| `manila-text` | `manila.11` | `#8A5F12` | `oklch(0.519 0.102 75.8)` | 5.06:1 (text) |
| `success` | `success.11` | `#2A6F38` | `oklch(0.484 0.111 147.7)` | 5.49:1 (text) |
| `success-bg` | `success.3` | `#E0F3E2` | `oklch(0.945 0.030 148.4)` | 1.04:1 |
| `warning` | `warning.11` | `#9F4B0C` | `oklch(0.511 0.130 49.8)` | 5.41:1 (text) |
| `warning-bg` | `warning.3` | `#FEEADB` | `oklch(0.949 0.030 59.3)` | 1.05:1 |
| `warning-fill` | `warning.9` | `#FAA453` | `oklch(0.789 0.140 61.6)` | 1.79:1 |
| `warning-fill-fg` | `greyboard.12` | `#1B1E1B` | `oklch(0.231 0.007 145.3)` | 15.10:1 |
| `danger` | `danger.11` | `#B3261E` | `oklch(0.501 0.178 28.7)` | 5.86:1 (text) |
| `danger-bg` | `danger.3` | `#FDE7E4` | `oklch(0.945 0.024 26.9)` | 1.06:1 |
| `danger-fill` | `danger.9` | `#B3261E` | `oklch(0.501 0.178 28.7)` | 5.86:1 |
| `danger-fill-fg` | `white.1` | `#FFFFFF` | `oklch(1.000 0.000 89.9)` | 1.11:1 |
| `info` | `info.11` | `#2D5BA6` | `oklch(0.479 0.131 259.7)` | 5.97:1 (text) |
| `info-bg` | `info.3` | `#E4EDFC` | `oklch(0.944 0.022 260.7)` | 1.06:1 |
| `slot-missing` | `greyboard.9` | `#787F76` | `oklch(0.588 0.016 138.7)` | 3.70:1 (UI, ≥3) |
| `slot-filled` | `greyboard.12` | `#1B1E1B` | `oklch(0.231 0.007 145.3)` | 15.10:1 |
| `slot-applied` | `buckram.9` | `#0E5E6F` | `oklch(0.446 0.075 216.7)` | 6.61:1 (UI, ≥3) |
| `slot-applied-run` | `manila.9` | `#E6C171` | `oklch(0.826 0.108 85.3)` | 1.54:1 |
| `code-bg` | `greyboard-dark.2` | `#121514` | `oklch(0.192 0.005 173.8)` | 16.48:1 |
| `code-fg` | `greyboard-dark.12` | `#ECEEEA` | `oklch(0.946 0.006 128.5)` | 1.05:1 (text) |
| `code-accent` | `buckram-dark.9` | `#5DB6C4` | `oklch(0.727 0.087 208.9)` | 2.10:1 |

### Dark

| Role (`--cr-*`) | Primitive | Hex | OKLCH | vs bg |
|---|---|---|---|---|
| `bg` | `greyboard-dark.2` | `#121514` | `oklch(0.192 0.005 173.8)` | — |
| `surface` | `greyboard-dark.3` | `#1A1E1D` | `oklch(0.230 0.006 179.1)` | 1.09:1 |
| `surface-raised` | `greyboard-dark.4` | `#202624` | `oklch(0.262 0.009 173.5)` | 1.19:1 |
| `surface-sunken` | `greyboard-dark.1` | `#0D0F0F` | `oklch(0.166 0.003 196.8)` | 1.05:1 |
| `hover` | `greyboard-dark.5` | `#262C2A` | `oklch(0.286 0.009 173.6)` | 1.29:1 |
| `active` | `greyboard-dark.6` | `#2D3331` | `oklch(0.315 0.009 173.7)` | 1.43:1 |
| `border` | `greyboard-dark.6` | `#2D3331` | `oklch(0.315 0.009 173.7)` | 1.43:1 |
| `border-strong` | `greyboard-dark.9` | `#79817E` | `oklch(0.596 0.010 171.6)` | 4.60:1 (UI, ≥3) |
| `text` | `greyboard-dark.12` | `#ECEEEA` | `oklch(0.946 0.006 128.5)` | 15.73:1 (text) |
| `text-secondary` | `greyboard-dark.11` | `#A8AFAB` | `oklch(0.747 0.010 161.3)` | 8.21:1 (text) |
| `text-disabled` | `greyboard-dark.9` | `#79817E` | `oklch(0.596 0.010 171.6)` | 4.60:1 |
| `primary` | `buckram-dark.9` | `#5DB6C4` | `oklch(0.727 0.087 208.9)` | 7.83:1 (text) |
| `primary-hover` | `buckram-dark.10` | `#7CC6D2` | `oklch(0.782 0.076 208.8)` | 9.51:1 |
| `primary-fg` | `buckram.12` | `#0A262C` | `oklch(0.250 0.035 214.2)` | 1.16:1 |
| `primary-subtle` | `buckram-dark.3` | `#16333A` | `oklch(0.302 0.037 215.5)` | 1.37:1 |
| `primary-subtle-fg` | `buckram-dark.11` | `#9FD7E0` | `oklch(0.844 0.059 208.4)` | 11.61:1 (text) |
| `ring` | `buckram-dark.9` | `#5DB6C4` | `oklch(0.727 0.087 208.9)` | 7.83:1 (UI, ≥3) |
| `manila` | `manila-dark.9` | `#E0BC6C` | `oklch(0.810 0.107 85.8)` | 10.13:1 |
| `manila-fg` | `manila.12` | `#2A2111` | `oklch(0.254 0.030 81.1)` | 1.16:1 |
| `manila-text` | `manila-dark.11` | `#E6C77F` | `oklch(0.840 0.097 87.0)` | 11.23:1 (text) |
| `success` | `success-dark.11` | `#6CC983` | `oklch(0.760 0.134 150.2)` | 9.03:1 (text) |
| `success-bg` | `success-dark.3` | `#172C1C` | `oklch(0.270 0.040 150.6)` | 1.24:1 |
| `warning` | `warning-dark.11` | `#F0A35A` | `oklch(0.777 0.128 62.5)` | 8.83:1 (text) |
| `warning-bg` | `warning-dark.3` | `#352211` | `oklch(0.271 0.040 61.6)` | 1.21:1 |
| `warning-fill` | `warning-dark.9` | `#F0A35A` | `oklch(0.777 0.128 62.5)` | 8.83:1 |
| `warning-fill-fg` | `greyboard-dark.2` | `#121514` | `oklch(0.192 0.005 173.8)` | 1.00:1 |
| `danger` | `danger-dark.11` | `#FF8A80` | `oklch(0.759 0.144 26.1)` | 8.05:1 (text) |
| `danger-bg` | `danger-dark.3` | `#381C1B` | `oklch(0.265 0.044 22.7)` | 1.18:1 |
| `danger-fill` | `danger-dark.9` | `#FF8A80` | `oklch(0.759 0.144 26.1)` | 8.05:1 |
| `danger-fill-fg` | `greyboard-dark.2` | `#121514` | `oklch(0.192 0.005 173.8)` | 1.00:1 |
| `info` | `info-dark.11` | `#8FB3F2` | `oklch(0.764 0.099 261.3)` | 8.66:1 (text) |
| `info-bg` | `info-dark.3` | `#19253B` | `oklch(0.265 0.045 261.9)` | 1.20:1 |
| `slot-missing` | `greyboard-dark.9` | `#79817E` | `oklch(0.596 0.010 171.6)` | 4.60:1 (UI, ≥3) |
| `slot-filled` | `greyboard-dark.12` | `#ECEEEA` | `oklch(0.946 0.006 128.5)` | 15.73:1 |
| `slot-applied` | `buckram-dark.9` | `#5DB6C4` | `oklch(0.727 0.087 208.9)` | 7.83:1 (UI, ≥3) |
| `slot-applied-run` | `manila-dark.9` | `#E0BC6C` | `oklch(0.810 0.107 85.8)` | 10.13:1 |
| `code-bg` | `greyboard-dark.1` | `#0D0F0F` | `oklch(0.166 0.003 196.8)` | 1.05:1 |
| `code-fg` | `greyboard-dark.12` | `#ECEEEA` | `oklch(0.946 0.006 128.5)` | 15.73:1 (text) |
| `code-accent` | `buckram-dark.9` | `#5DB6C4` | `oklch(0.727 0.087 208.9)` | 7.83:1 |

### Primitive scales

- **buckram**: 1`#F6FBFC` 2`#EBF6F7` 3`#DCEBEC` 4`#CCE3E7` 5`#B8D8DE` 6`#9DC6CF` 7`#75ACB8` 8`#458C9C` 9`#0E5E6F` 10`#0A4D5B` 11`#084551` 12`#0A262C`
- **buckram-dark**: 1`#0C1618` 2`#0E1D21` 3`#16333A` 4`#183E46` 5`#1A4D56` 6`#205F6A` 7`#2C7884` 8`#3C94A2` 9`#5DB6C4` 10`#7CC6D2` 11`#9FD7E0` 12`#D2EEF1`
- **danger**: 3`#FDE7E4` 9`#B3261E` 11`#B3261E`
- **danger-dark**: 3`#381C1B` 9`#FF8A80` 11`#FF8A80`
- **greyboard**: 1`#FBFBF9` 2`#F2F3EF` 3`#E7E9E3` 4`#E0E3DC` 5`#DADDD5` 6`#D3D7CF` 7`#BBC0B6` 8`#9AA197` 9`#787F76` 10`#676E65` 11`#535A52` 12`#1B1E1B`
- **greyboard-dark**: 1`#0D0F0F` 2`#121514` 3`#1A1E1D` 4`#202624` 5`#262C2A` 6`#2D3331` 7`#424A47` 8`#5D6562` 9`#79817E` 10`#8D9491` 11`#A8AFAB` 12`#ECEEEA`
- **info**: 3`#E4EDFC` 9`#2D5BA6` 11`#2D5BA6`
- **info-dark**: 3`#19253B` 9`#8FB3F2` 11`#8FB3F2`
- **manila**: 1`#FDFAF1` 2`#FCF5E3` 3`#FBEDCD` 4`#F9E6BB` 5`#F5DDA7` 6`#F0D291` 7`#EBCA80` 8`#E7C275` 9`#E6C171` 10`#DAB060` 11`#8A5F12` 12`#2A2111`
- **manila-dark**: 1`#17130C` 2`#1F1A0F` 3`#322710` 4`#413315` 5`#514019` 6`#675223` 7`#8B7035` 8`#B3944F` 9`#E0BC6C` 10`#EACA80` 11`#E6C77F` 12`#F5EBCE`
- **success**: 3`#E0F3E2` 9`#2A6F38` 11`#2A6F38`
- **success-dark**: 3`#172C1C` 9`#6CC983` 11`#6CC983`
- **warning**: 3`#FEEADB` 9`#FAA453` 11`#9F4B0C`
- **warning-dark**: 3`#352211` 9`#F0A35A` 11`#F0A35A`
- **white**: 1`#FFFFFF`

### Contrast checks (computed, WCAG 2)


**Light**

| Foreground | Background | Ratio | Min | Pass |
|---|---|---|---|---|
| `text` | `bg` | 15.1:1 | 4.5 | yes |
| `text` | `surface` | 16.24:1 | 4.5 | yes |
| `text` | `surface-raised` | 16.83:1 | 4.5 | yes |
| `text` | `surface-sunken` | 13.75:1 | 4.5 | yes |
| `text` | `hover` | 12.97:1 | 4.5 | yes |
| `text` | `active` | 12.25:1 | 4.5 | yes |
| `text` | `primary-subtle` | 13.73:1 | 4.5 | yes |
| `text-secondary` | `bg` | 6.38:1 | 4.5 | yes |
| `text-secondary` | `surface` | 6.87:1 | 4.5 | yes |
| `text-secondary` | `surface-raised` | 7.11:1 | 4.5 | yes |
| `text-secondary` | `surface-sunken` | 5.81:1 | 4.5 | yes |
| `text-secondary` | `hover` | 5.48:1 | 4.5 | yes |
| `primary` | `bg` | 6.61:1 | 4.5 | yes |
| `primary` | `surface` | 7.11:1 | 4.5 | yes |
| `primary` | `surface-raised` | 7.37:1 | 4.5 | yes |
| `primary-fg` | `primary` | 7.37:1 | 4.5 | yes |
| `primary-fg` | `primary-hover` | 9.44:1 | 4.5 | yes |
| `primary-subtle-fg` | `primary-subtle` | 8.65:1 | 4.5 | yes |
| `manila-fg` | `manila` | 9.24:1 | 4.5 | yes |
| `manila-text` | `bg` | 5.06:1 | 4.5 | yes |
| `manila-text` | `surface` | 5.44:1 | 4.5 | yes |
| `success` | `bg` | 5.49:1 | 4.5 | yes |
| `success` | `surface` | 5.91:1 | 4.5 | yes |
| `success` | `surface-sunken` | 5.0:1 | 4.5 | yes |
| `success` | `success-bg` | 5.27:1 | 4.5 | yes |
| `warning` | `bg` | 5.41:1 | 4.5 | yes |
| `warning` | `surface` | 5.81:1 | 4.5 | yes |
| `warning` | `surface-sunken` | 4.92:1 | 4.5 | yes |
| `warning` | `warning-bg` | 5.17:1 | 4.5 | yes |
| `danger` | `bg` | 5.86:1 | 4.5 | yes |
| `danger` | `surface` | 6.31:1 | 4.5 | yes |
| `danger` | `surface-sunken` | 5.34:1 | 4.5 | yes |
| `danger` | `danger-bg` | 5.52:1 | 4.5 | yes |
| `info` | `bg` | 5.97:1 | 4.5 | yes |
| `info` | `surface` | 6.42:1 | 4.5 | yes |
| `info` | `surface-sunken` | 5.43:1 | 4.5 | yes |
| `info` | `info-bg` | 5.64:1 | 4.5 | yes |
| `warning-fill-fg` | `warning-fill` | 8.41:1 | 4.5 | yes |
| `danger-fill-fg` | `danger-fill` | 6.54:1 | 4.5 | yes |
| `code-fg` | `code-bg` | 15.73:1 | 4.5 | yes |
| `code-muted` | `code-bg` | 8.21:1 | 4.5 | yes |
| `code-accent` | `code-bg` | 7.83:1 | 4.5 | yes |
| `border-strong` | `bg` | 3.7:1 | 3.0 | yes |
| `border-strong` | `surface` | 3.98:1 | 3.0 | yes |
| `border-strong` | `surface-raised` | 4.12:1 | 3.0 | yes |
| `border-strong` | `surface-sunken` | 3.37:1 | 3.0 | yes |
| `border-strong` | `hover` | 3.18:1 | 3.0 | yes |
| `ring` | `bg` | 6.61:1 | 3.0 | yes |
| `ring` | `surface` | 7.11:1 | 3.0 | yes |
| `ring` | `surface-raised` | 7.37:1 | 3.0 | yes |
| `ring` | `surface-sunken` | 6.02:1 | 3.0 | yes |
| `ring` | `primary-subtle` | 6.01:1 | 3.0 | yes |
| `slot-applied` | `surface-sunken` | 6.02:1 | 3.0 | yes |

**Dark**

| Foreground | Background | Ratio | Min | Pass |
|---|---|---|---|---|
| `text` | `bg` | 15.73:1 | 4.5 | yes |
| `text` | `surface` | 14.42:1 | 4.5 | yes |
| `text` | `surface-raised` | 13.18:1 | 4.5 | yes |
| `text` | `surface-sunken` | 16.46:1 | 4.5 | yes |
| `text` | `hover` | 12.18:1 | 4.5 | yes |
| `text` | `active` | 11.04:1 | 4.5 | yes |
| `text` | `primary-subtle` | 11.46:1 | 4.5 | yes |
| `text-secondary` | `bg` | 8.21:1 | 4.5 | yes |
| `text-secondary` | `surface` | 7.52:1 | 4.5 | yes |
| `text-secondary` | `surface-raised` | 6.88:1 | 4.5 | yes |
| `text-secondary` | `surface-sunken` | 8.59:1 | 4.5 | yes |
| `text-secondary` | `hover` | 6.36:1 | 4.5 | yes |
| `primary` | `bg` | 7.83:1 | 4.5 | yes |
| `primary` | `surface` | 7.18:1 | 4.5 | yes |
| `primary` | `surface-raised` | 6.56:1 | 4.5 | yes |
| `primary-fg` | `primary` | 6.75:1 | 4.5 | yes |
| `primary-fg` | `primary-hover` | 8.19:1 | 4.5 | yes |
| `primary-subtle-fg` | `primary-subtle` | 8.46:1 | 4.5 | yes |
| `manila-fg` | `manila` | 8.75:1 | 4.5 | yes |
| `manila-text` | `bg` | 11.23:1 | 4.5 | yes |
| `manila-text` | `surface` | 10.29:1 | 4.5 | yes |
| `success` | `bg` | 9.03:1 | 4.5 | yes |
| `success` | `surface` | 8.27:1 | 4.5 | yes |
| `success` | `surface-sunken` | 9.45:1 | 4.5 | yes |
| `success` | `success-bg` | 7.3:1 | 4.5 | yes |
| `warning` | `bg` | 8.83:1 | 4.5 | yes |
| `warning` | `surface` | 8.09:1 | 4.5 | yes |
| `warning` | `surface-sunken` | 9.24:1 | 4.5 | yes |
| `warning` | `warning-bg` | 7.27:1 | 4.5 | yes |
| `danger` | `bg` | 8.05:1 | 4.5 | yes |
| `danger` | `surface` | 7.37:1 | 4.5 | yes |
| `danger` | `surface-sunken` | 8.42:1 | 4.5 | yes |
| `danger` | `danger-bg` | 6.81:1 | 4.5 | yes |
| `info` | `bg` | 8.66:1 | 4.5 | yes |
| `info` | `surface` | 7.94:1 | 4.5 | yes |
| `info` | `surface-sunken` | 9.06:1 | 4.5 | yes |
| `info` | `info-bg` | 7.23:1 | 4.5 | yes |
| `warning-fill-fg` | `warning-fill` | 8.83:1 | 4.5 | yes |
| `danger-fill-fg` | `danger-fill` | 8.05:1 | 4.5 | yes |
| `code-fg` | `code-bg` | 16.46:1 | 4.5 | yes |
| `code-muted` | `code-bg` | 8.59:1 | 4.5 | yes |
| `code-accent` | `code-bg` | 8.2:1 | 4.5 | yes |
| `border-strong` | `bg` | 4.6:1 | 3.0 | yes |
| `border-strong` | `surface` | 4.21:1 | 3.0 | yes |
| `border-strong` | `surface-raised` | 3.85:1 | 3.0 | yes |
| `border-strong` | `surface-sunken` | 4.81:1 | 3.0 | yes |
| `border-strong` | `hover` | 3.56:1 | 3.0 | yes |
| `ring` | `bg` | 7.83:1 | 3.0 | yes |
| `ring` | `surface` | 7.18:1 | 3.0 | yes |
| `ring` | `surface-raised` | 6.56:1 | 3.0 | yes |
| `ring` | `surface-sunken` | 8.2:1 | 3.0 | yes |
| `ring` | `primary-subtle` | 5.71:1 | 3.0 | yes |
| `slot-applied` | `surface-sunken` | 8.2:1 | 3.0 | yes |

### 4.4 Name mapping (existing `--cr-*` and shadcn/ui)

The old 8 tokens keep working as aliases, so nothing breaks while components migrate.

| Legacy `--cr-*` | Now resolves to | Light | Dark |
|---|---|---|---|
| `--cr-bg` | `bg` | `#F2F3EF` | `#121514` |
| `--cr-surface` | `surface` | `#FBFBF9` | `#1A1E1D` |
| `--cr-text` | `text` | `#1B1E1B` | `#ECEEEA` |
| `--cr-text-muted` | `text-secondary` | `#535A52` | `#A8AFAB` |
| `--cr-accent` | `primary` | `#0E5E6F` | `#5DB6C4` |
| `--cr-accent-text` | `primary-fg` | `#FFFFFF` | `#0A262C` |
| `--cr-border` | `border` | `#D3D7CF` | `#2D3331` |
| `--cr-error` | `danger` | `#B3261E` | `#FF8A80` |

shadcn/ui class names map onto the same variables in `tailwind.config.ts`:

| shadcn name | `--cr-*` role | shadcn name | `--cr-*` role |
|---|---|---|---|
| `background` | `bg` | `muted` | `surface-sunken` |
| `foreground` | `text` | `muted-foreground` | `text-secondary` |
| `card` / `card-foreground` | `surface` / `text` | `accent` / `accent-foreground` | `hover` / `text` |
| `popover` / `popover-foreground` | `surface-raised` / `text` | `destructive` / `destructive-foreground` | `danger-fill` / `danger-fill-fg` |
| `primary` / `primary-foreground` | `primary` / `primary-fg` | `border` | `border` |
| `secondary` / `secondary-foreground` | `surface-sunken` / `text` | `input` | `border-strong` |
| `ring` | `ring` | `ok` / `ok-subtle` | `success` / `success-bg` |
| `warn` / `warn-subtle` / `warn-fill` | `warning` / `warning-bg` / `warning-fill` | `info` / `info-subtle` | `info` / `info-bg` |
| `chart-1…5` | `chart-1…5` | | |

**Watch out:** shadcn's `accent` is the neutral hover fill. The legacy `--cr-accent` is the brand primary. In new code, say `primary` for the brand colour.

---

## 5. Typography

All faces are **SIL Open Font License 1.1**, self-hosted as subset woff2 (Latin + Latin-1 Supplement + Latin Extended-A, so Spanish ñ, á, ¿, ¡ work). No pixel or bitmap face anywhere.

| Role | Family | Source | Why |
|---|---|---|---|
| **Display** | **Archivo** (variable, wdth 62–125, wght 100–900) | Omnibus-Type, [Google Fonts](https://fonts.google.com/specimen/Archivo), OFL | Already on the landing. A grotesque cut for labels and signage, so it reads like a printed box label. Distinctiveness comes from the narrow width (wdth 75) plus the Manila tag |
| **UI / body** | **Atkinson Hyperlegible Next** (variable, wght 200–800) | Braille Institute (2025), [Google Fonts](https://fonts.google.com/specimen/Atkinson+Hyperlegible+Next), OFL | Built to tell similar glyphs apart (Il1, 0O, rn/m), which matters for ROM filenames. Crisp at 13px on non-retina Windows and Linux. Not Inter, not system-ui |
| **Mono / data** | **Atkinson Hyperlegible Mono** (variable, wght 200–800) | Braille Institute (2025), [Google Fonts](https://fonts.google.com/specimen/Atkinson+Hyperlegible+Mono), OFL | The "label-maker readout": file paths, filenames, counts, source tags, the landing terminal. Same design family as the UI face, so inline paths sit well in sentences |

Documented alternative if Atkinson is rejected after testing: IBM Plex Sans + IBM Plex Mono (OFL).

**Tokens**

```css
--cr-font-display: 'Archivo', 'Archivo Display', 'Arial Narrow', sans-serif;
--cr-font-body:    'Atkinson Hyperlegible Next', system-ui, -apple-system, 'Segoe UI', sans-serif;
--cr-font-mono:    'Atkinson Hyperlegible Mono', ui-monospace, 'SF Mono', Menlo, monospace;
```

**Global rules**

- `font-variant-numeric: tabular-nums` on every count, table and progress number.
- Archivo is only for 24px and up, the wordmark and eyebrow labels. Everything else in the app is Atkinson.
- Loading: `font-display: swap` in the app (fonts are bundled, so the swap is instant), `optional` on the landing. Total payload target ≤ 180KB woff2.

### App scale (13px base)

| Token | Family | Size / line-height | Weight | Use |
|---|---|---|---|---|
| `type.app.caption` | body | 12 / 1.4 | 400 | hints, metadata |
| `type.app.base` | body | **13 / 1.45** | 400 | default UI text |
| `type.app.label` | body | 13 / 1.45 | 500 | buttons, form labels, tabs |
| `type.app.dense` | body | 14 / 1.45 | 400 | readable paragraphs in panels |
| `type.app.dialog` | body | 16 / 1.5 | 400 | dialog body |
| `type.app.title` | body | 20 / 1.3 | 600 | panel titles |
| `type.app.heading` | display (wdth 75) | 24 / 1.15 | 800 | screen headings |
| `type.app.display` | display (wdth 75) | 32 / 1.05 | 800 | empty-state headline, summary counts |
| `type.app.mono` | mono | 12 / 1.5 | 400–500 | paths, filenames, counts |

### Landing scale

| Token | Family | Size / line-height | Weight | Use |
|---|---|---|---|---|
| `type.landing.body` | body | 17 / 1.6 | 400 | paragraphs |
| `type.landing.lead` | body | 20 / 1.5 | 400 | hero sub |
| `type.landing.eyebrow` | display (wdth 112) | 13 / 1.2, uppercase, +0.06em | 600 | section labels (spine-label stamping) |
| `type.landing.h3` | display (wdth 75) | 24 / 1.15 | 800 | |
| `type.landing.h2` | display (wdth 75) | `clamp(2rem, 3vw + 1rem, 2.75rem)` / 1.02 | 800 | |
| `type.landing.h1` | display (wdth 75) | `clamp(3rem, 5.4vw + 1rem, 5.5rem)` / 0.96, -0.01em | 800 | hero |
| `type.landing.mono` | mono | 14 / 1.55 | 400 | terminal, inline paths |

---

## 6. Spacing, radius, elevation, motion

### Spacing

Tailwind's 4px grid is the spacing scale (`space.*` in tokens: 2, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48, 64, 80, 96px). Do not invent other values.

| Token | Value | Use |
|---|---|---|
| `control.sm` | 28px | compact toolbar buttons, inline inputs |
| `control.md` | 32px | default buttons, inputs, selects |
| `control.lg` | 40px | primary dialog actions |
| `control.landing` | 44px | landing CTAs (touch target) |
| Landing gutter | `clamp(16px, 4vw, 40px)` | page sides |
| Landing wrap | 1160px | max content width |

### Radius: small and crisp, like cover geometry

| Token | Value | Use |
|---|---|---|
| `--cr-radius-sm` | 3px | tags, chips, checkboxes, the `ROMs` tag, slots |
| `--cr-radius-md` | 5px | buttons, inputs, selects |
| `--cr-radius-lg` | 8px | cards, panels, popovers |
| `--cr-radius-xl` | 12px | dialogs, landing screenshots |
| `--cr-radius-full` | 9999px | pills, progress dots, avatars |
| icon body | 185.4px on the 824px body (1024 canvas) | app icon only |

### Border widths

`hairline` 1px (separators, inputs) · `slot` 1.5px (missing-slot dash) · `focus` 2px (focus ring) · `spine` 3px (applied band, shelf-rule cap).

### Elevation

Higher surfaces are lighter in dark mode. Each raised level pairs a surface token with a shadow.

| Level | Surface | Shadow (light) | Shadow (dark) | Use |
|---|---|---|---|---|
| sunken | `surface-sunken` | none | none | wells, the library grid backdrop, input fill |
| base | `bg` | none | none | app background |
| default | `surface` | none, 1px `border` | none, 1px `border` | cards, panels |
| raised | `surface` | `shadow.raised-light` | `shadow.raised-dark` | hover cards, sticky bars |
| overlay | `surface-raised` | `shadow.overlay-light` | `shadow.overlay-dark` | popover, menu, dialog, toast |

- `shadow.raised-light`: `0 1px 0 rgb(27 30 27 / 5%), 0 4px 12px -4px rgb(27 30 27 / 10%)`
- `shadow.overlay-light`: `0 1px 0 rgb(27 30 27 / 6%), 0 12px 32px -12px rgb(27 30 27 / 22%)`
- `shadow.raised-dark`: `0 1px 0 rgb(0 0 0 / 30%), 0 6px 16px -6px rgb(0 0 0 / 45%)`
- `shadow.overlay-dark`: `0 1px 0 rgb(0 0 0 / 30%), 0 16px 40px -16px rgb(0 0 0 / 70%)`
- Modal scrim: `rgb(13 15 15 / 55%)` in both themes.

### Focus

`:focus-visible { outline: 2px solid var(--cr-ring); outline-offset: 2px; }` everywhere, app and landing. Ring vs every surface is at least 5.7:1. Never `outline: none` without a replacement.

### Motion: "ink settles"

| Token | Value | Use |
|---|---|---|
| `duration.fast` | 110ms | hover, press, checkbox tick |
| `duration.base` | 150ms | menus, tooltips, the Manila flash |
| `duration.slow` | 240ms | dialogs, slot fill |
| `duration.slower` | 400ms | landing hero sequence steps |
| `easing.standard` | `cubic-bezier(.2, 0, 0, 1)` | most transitions |
| `easing.enter` / `easing.exit` | `(0, 0, .38, .9)` / `(.2, 0, 1, .9)` | appear / disappear |
| `easing.ink` | `cubic-bezier(.2, .7, .2, 1)` | slot fill |
| `easing.spring` | `cubic-bezier(.34, 1.5, .64, 1)` | playful only (tag drop, landing). **Never on apply, undo or destructive actions** |

**The slot fill** is the one shared motion primitive (loader, success, landing hero):

1. The dashed outline draws in clockwise (`slow`, `ink`).
2. The fill rises from the bottom, like a cover sliding into a sleeve (`slow`, `ink`).
3. The outline turns solid.
4. A 150ms Manila flash on the corner tab.

Under `prefers-reduced-motion: reduce` all durations become 0ms through the tokens, so components need no special code. The slot jumps straight to its final state.

---

## 7. Voice and copy

**A calm archivist who is also a hobbyist.** Warm through precision and respect for the collection, never through hype.

### Rules (both languages)

1. **No exclamation marks.** No "epic", "retro vibes", "level up", "Let's go".
2. **Short, declarative, present tense, second person.** Spanish uses **tú**.
3. **Say what will happen to the files before it happens.**
4. **Buttons are a verb plus an exact count.** "Apply 214 covers", never "Apply" alone when a count is known, never "Let's go!".
5. **Every error says what was NOT touched.** "Nothing was changed on your SD card."
6. **Numbers and paths in mono, never rounded or hidden.**
7. **Vocabulary of archives:** shelf, slot, gap, fill, label, file away, undo. Spanish: estante, hueco, rellenar, etiqueta, deshacer.
8. **Plain Spanish over anglicisms** when a clear word exists ("portada", not "cover"; "simular", not "dry-run"). Keep product nouns (ScreenScraper, SteamGridDB) as-is.
9. **Never name a console maker in brand copy.** System names appear only as library data.
10. **A small wink is allowed** in empty states and labels, never in errors or confirmations.

### Examples

| Moment | English | Español |
|---|---|---|
| Hero | Your library, minus the gaps. | Tu biblioteca, sin huecos. |
| Hero sub | Coverly finds every game without cover art and fills only those slots. Nothing is written until you say so. | Coverly encuentra cada juego sin portada y rellena solo esos huecos. No se escribe nada hasta que tú lo digas. |
| Promise | Every change can be undone. | Cada cambio se puede deshacer. |
| Promise | Your SD card stays yours. | Tu tarjeta SD sigue siendo tuya. |
| Audit summary | 312 games. 41 covers missing. | 312 juegos. Faltan 41 portadas. |
| Simulate | Here is what would change. Nothing has changed yet. | Esto es lo que cambiaría. Todavía no ha cambiado nada. |
| Apply button | Apply 41 covers | Aplicar 41 portadas |
| After apply | Filed 41 covers. Undo is one click away. | 41 portadas guardadas. Deshacer está a un clic. |
| Undo toast | 41 covers applied. Undo | 41 portadas aplicadas. Deshacer |
| Empty state | No missing art. This shelf is complete. | No falta ninguna portada. Este estante está completo. |
| Network error | Couldn't reach ScreenScraper. Nothing was changed on your SD card. | No se pudo conectar con ScreenScraper. No se ha cambiado nada en tu tarjeta SD. |
| Write error | Couldn't write to `/Volumes/SD/roms/gba`. The card may be read-only. Nothing was changed. | No se pudo escribir en `/Volumes/SD/roms/gba`. Puede que la tarjeta sea de solo lectura. No se ha cambiado nada. |

---

## 8. How the app and the landing share tokens

```
design/tokens.json              # source of truth (DTCG 2025.10): primitives -> semantic (light/dark) -> legacy + shadcn aliases
scripts/brand/build_tokens.mjs  # zero-dependency: resolve aliases, emit CSS, check contrast, --check mode for CI
src/styles/tokens.css           # GENERATED, imported first by src/styles/globals.css
```

The landing lives in [MAECLY/coverly-roms-landing](https://github.com/MAECLY/coverly-roms-landing) and keeps its own
`design/tokens.json` (a copy of this one) and its generated
`site/assets/tokens.css` with only the landing roles. Its weekly `Sync tokens`
workflow fetches this JSON from `main` and opens a pull request when it changed.

- **Three tiers.** Primitives (`--cr-buckram-9`, `--cr-greyboard-11` …) are never used directly by components. Components use semantic roles (`--cr-primary`, `--cr-text-secondary` …). Legacy and shadcn names are aliases.
- **Emitted format.** `--cr-<role>` is a full colour (`--cr-primary: #0e5e6f`), so plain CSS and the legacy names keep working with `var(--cr-*)`. The app file also emits RGB channels for Tailwind 3 opacity modifiers (`--cr-primary-rgb: 14 94 111`), the shadcn/ui names as full colours (`--background: #f2f3ef`) and the primitive scales; the landing file carries only the roles. Every hex is in sRGB, so no `oklch()` is emitted. No relative colour syntax at runtime. Run `pnpm tokens` (`make tokens`) after editing the JSON; `pnpm tokens:check` (`make tokens-check`, CI) fails on stale output or a contrast pair below its target.
- **Themes.** `:root` gets light. `:root[data-theme='dark']` gets dark, and so does the OS fallback, under the selector each surface already used: `:root:not([data-theme])` in the app (before the settings load) and `:root:not([data-theme='light'])` on the landing (no-JS visitors). Both blocks are generated, never hand-copied.
- **Tailwind.** `darkMode: ['selector', '[data-theme="dark"]']` so `dark:` variants follow the real theme toggle. `tailwind.config.ts` reads `design/tokens.json` through the generator, so colours map as `primary: 'rgb(var(--cr-primary-rgb) / <alpha-value>)'` (every role is also `cr-<role>`). Radius, duration, easing, shadows and font families map to the `--cr-*` variables.
- **Landing.** Uses `var(--cr-primary-c)` style full colours. Its private tokens (`--ok`, `--line`, `--dash`, `--code-*`, `--shot-*`, `--shadow`, `--ease-*`) are deleted and replaced by `--cr-success`, `--cr-border`, `--cr-slot-missing`, `--cr-code-*`, `--cr-shadow-*`, `--cr-ease-*`.
- **Drift guard.** `pnpm tokens:check` (CI) runs `build_tokens.mjs --check`, which fails when the generated file differs from a fresh build or when any contrast pair in §4 drops below its target. The landing repository checks its own copy.
- **Hard-coded colours.** Every literal in `globals.css` and `site.css` (Tailwind greens/reds/yellows, source badge colours, the `#a5b4fc` terminal prompt) moves to a token. Source badges become Manila tags with mono labels, identical across sources (the label text tells them apart, not the colour).

---

## 9. Icon

**Final choice: "Lean-To" (round 2, v05), chosen by the owner on 2026-10-17.** A solid paper cover leans about 12° against an upright empty slot, and both rest on a shelf rule: the gap on the shelf that Coverly fills. It replaced the "C" ring (app) and the two-slot favicon (landing), so the dock, the taskbar, the browser tab, the README and the landing all show the same mark. History: [round 1](icon-explorations/round-1/index.html) (10 concepts, #10 Shelf Gap picked) and [round 2](icon-explorations/round-2/index.html) (10 Shelf Gap variants, v05 Lean-To picked); see [`icon-explorations/README.md`](icon-explorations/README.md).

### Construction

- **Canvas:** 1024 px; body 824 × 824 squircle, rx 185.4 (100 px gutter), Buckram `#0E5E6F`. The glyph is drawn in 1024 space and scaled 0.8 into the body.
- **Cover:** 300 × 400 (3:4 portrait), rx 48, paper `#FBFBF9`, rotated 12° about its bottom-right corner. Its lowest point sits level with the slot's bottom edge and its top-right corner touches the slot's outer edge: it leans, it does not float.
- **Slot:** 302 × 404 outer (246 × 348 inner + 56 stroke), outer radius 50 to match the cover, paper outline, empty. The empty slot is the point: never fill it.
- **Shelf:** 680 × 56, rx 28, 34 units below the objects, same weight as the slot stroke. Manila `#E6C171` at 48 px and up; paper on the hand-tuned 32/24/16 px glyphs (Manila on Buckram is too weak that small).
- **Balance:** glyph bounding box centred horizontally and 3.5 units above centre (optical centre).
- **Layers:** three flat, opaque shapes (cover, slot, shelf) on the tile. No shadow, gloss, gradient, blur or blend mode; Icon Composer and the OS add depth.
- **One colour:** `app-mono.svg` is the glyph in a single ink on transparent; the cover reads as a solid, the slot through its outline (macOS 26 Tinted/Clear, Windows unplated, embossing).
- **Small sizes:** below 48 px never downscale `app.svg`. `glyph-32/24/16.svg` snap the slot and shelf to whole pixels, fill the tile edge to edge, and draw the 16 px lean as a one-pixel step with three mid-tone pixels.
- **Dark favicon:** tile `#205F6A`, paper `#ECEEEA` (never near-black).
- **In the app:** `src/components/brand/Logo.tsx` inlines `mark.svg` in `currentColor`, coloured by `--cr-accent` (Buckram `#0E5E6F` light, `#5DB6C4` dark), with the wordmark at a gap of 0.5 × icon height (`size`, `showWordmark` props). A test keeps its geometry in sync with `mark.svg`.

Full geometry, clear space and don'ts: [`design/icon/README.md`](../../design/icon/README.md).

### Assets

Masters live in `design/icon/`; every raster is generated from them.

| Master | Use | Generated outputs |
|---|---|---|
| `app.svg` | App icon, 48 px and up | `src-tauri/icons/source.png` → `icon.icns`, `128x128.png`, `128x128@2x.png`, `Square*Logo.png`, `android/`, `ios/`, ico 48/64/256 layers; with `--site`, the landing's `assets/icon-192.png`, `icon-512.png`, `apple-touch-icon.png` (cropped to the body, opaque) |
| `glyph-32.svg` | 32 px | `src-tauri/icons/32x32.png`, ico 32 layer, `public/favicon.png` (app `index.html`), the landing's `favicon.ico` 32 |
| `glyph-24.svg` | 24 px | ico 24 layer |
| `glyph-16.svg` | 16 px | ico 16 layer, the landing's `favicon.ico` 16 and `assets/favicon.svg` (with a `prefers-color-scheme: dark` palette) |
| `maskable.svg` | PWA / Android adaptive | the landing's `assets/icon-maskable-512.png` (glyph inside the 40% safe zone) |
| `app-mono.svg` | Tinted, unplated, single-ink print | used as is (recolour the alpha) |
| `mark.svg` / `mark-dark.svg` | Mark on light / dark backgrounds (lockups, docs, `Logo.tsx`) | used as is |

Tauri `bundle.icon` points at `icons/32x32.png`, `icons/128x128.png`, `icons/128x128@2x.png`, `icons/icon.icns`, `icons/icon.ico`.

### Generator

```sh
pnpm icons        # or: make icons, node scripts/icons/generate.mjs
```

`scripts/icons/generate.mjs` rasterises with `@resvg/resvg-js` (falls back to `rsvg-convert` or `inkscape`), runs `pnpm tauri icon` on `source.png`, then rewrites `icon.ico` (32, 16, 24, 48, 64, 256, with 16/24/32 from the hand-tuned glyphs) and `32x32.png`. Edit the SVGs, regenerate, commit the outputs; never edit a PNG/ICO/ICNS by hand.

### Original brief (kept for reference)

The brief below produced both exploration rounds. Where it differs from the construction above (the "Empty Mount" 2×2 grid, photo-corners), the construction wins.

- **Subject:** the Slot. The reference concept is **"The Empty Mount"**: a Buckram squircle holding a 2×2 grid of 3:4 portrait slots, three solid and one empty (bottom-right), the empty one held by four photo-corners. Candidates may explore other arrangements (single slot, row of slots, shelf, two-slot solid-next-to-hollow), but every candidate must show **solid next to missing**.
- **Colours:** tile `#0E5E6F` (Buckram). All solid slots in **one** paper colour `#FBFBF9` (do not mix Manila and paper cells: at small sizes that reads as coloured quadrants). Manila `#E6C171` is allowed only as a small accent (the "applied" tab or a fill sliver), dropped below 64px.
- **Dark variant:** keep a Buckram tile (lightness floor about L 0.44–0.50), never near-black, so it holds its edge on dark docks and taskbars. Cells `#ECEEEA`.
- **One-colour / tinted:** must read in a single colour (macOS 26 Tinted and Clear, Windows unplated). Solid cells knock out, the empty cell is read through its outline.
- **Canvas:** 1024px, body 824×824 with a 185.4px continuous-corner radius (100px gutter). Flat, opaque layers only: no baked blur, shadow, gloss or gradient (Icon Composer adds them).
- **Detail by size:**
  - 1024–128px: photo-corners, dashed inner outline, optional Manila accent.
  - 64–32px: corners collapse to a solid 1px hollow frame; accent dropped.
  - 24–16px: hand-tuned glyph, at most 2–4 shapes. Fallback if the 2×2 tests too close to the Windows logo or an "apps" grid: two shapes, a solid slot next to a hollow one.
- **Hard rules:** cells stay portrait 3:4, the hollow cell stays visibly empty, no red/green/blue/yellow quadrants, not fanned or stacked (SteamGridDB), not a cube (LaunchBox), no controller, D-pad, joystick, cartridge, Game Boy shape, SD-card notch, letterform, pixel art or scanlines.
- **Clearance before shipping:** reverse-image search plus a USPTO/EUIPO class 9 trademark search against Microsoft's four-square logo, Google's apps grid, SteamGridDB and LaunchBox.

### Deliverables from the generator

| Platform | Files |
|---|---|
| macOS | `icon.icns` (legacy) + Icon Composer `.icon` (max 4 groups: tile, solid slots, empty mount, accent) compiled with `actool` to `Assets.car`, plus `CFBundleIconName` in Info.plist. Without it macOS 26 puts the icon in a grey frame ("icon jail") |
| Windows | `icon.ico` with 16, 24, 32, 48, 64, 256 layers (32 first), 16/24/32 hand-tuned; MSIX `AppList.targetsize-*` with `_altform-unplated` and `_altform-lightunplated` if packaged |
| Linux | hicolor PNGs 32, 48, 64, 128, 256, 512 + scalable SVG, named with the app ID |
| Tauri | `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` (`bundle.icon`) |
| Web (landing + app `index.html`) | `favicon.ico` (16+32), `icon.svg` with a `prefers-color-scheme: dark` style block, `apple-touch-icon.png` 180px opaque (logo at 140px), `icon-192.png`, `icon-512.png`, `icon-mask.png` 512px maskable (content inside the 40% safe radius), `site.webmanifest` with `theme_color` `#F2F3EF` / dark `#121514` |
| Social | `og.png` / `og-en.png` 1200×630: the mount grid plus the wordmark on `--cr-bg` |
| Optional | Nightly/preview variant (Manila tile), Dock badge with the missing-art count |

---

## 10. Checklist for applying the brand

- [x] `scripts/brand/build_tokens.mjs` emits `src/styles/tokens.css` from `design/tokens.json` (the landing's `site/assets/tokens.css` now lives in MAECLY/coverly-roms-landing); `--check` wired into the local CI gates.
- [x] `tailwind.config.ts` maps shadcn names, radius, motion and fonts; `darkMode` uses the `data-theme` selector.
- [x] Fonts (Archivo variable, Atkinson Hyperlegible Next, Atkinson Hyperlegible Mono) self-hosted in both app and landing with their OFL files.
- [x] All hard-coded colours in `globals.css` and `site.css` replaced by tokens.
- [ ] Raw `<input>`, `<select>`, `<progress>`, `<textarea>` and modal replaced by branded components (shadcn primitives restyled with these tokens; checkbox and progress use the slot).
- [ ] Wordmark with the Manila `ROMs` tag in the app header and the landing nav.
- [x] Chosen icon (Lean-To) generated for every platform; `site/assets/favicon.svg` "temporary icon" comment removed; `TRADEMARKS.md` updated.
- [ ] Copy reviewed against §7 in both languages.
