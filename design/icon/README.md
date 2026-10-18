# Coverly ROMs icon: Lean-To

The single source of the Coverly ROMs mark. Every icon the app ships
(`src-tauri/icons/`, `public/favicon.png`) is rasterised from the SVGs in this
folder by `pnpm icons` (`make icons`, `scripts/icons/generate.mjs`). The
landing's icons (`favicon.ico`, `assets/favicon.svg`,
`assets/apple-touch-icon.png`, `assets/icon-*.png`, `assets/mark*.svg` in the
`site/` folder of [MAECLY/coverly-roms-landing](https://github.com/MAECLY/coverly-roms-landing))
come from the same SVGs with `pnpm icons --site <landing checkout>/site`. Edit
the SVGs here, then regenerate; never edit the PNG/ICO/ICNS outputs by hand.

**Chosen 2026-10-17 from round 2, v05 "Lean-To".** A solid cover leans about
12° against an upright empty slot, and both rest on a shelf rule: the gap in the
shelf that Coverly fills. History: [round 1](../../docs/08-brand/icon-explorations/round-1/index.html)
(10 concepts, #10 Shelf Gap picked) and [round 2](../../docs/08-brand/icon-explorations/round-2/index.html)
(10 Shelf Gap variants, v05 Lean-To picked). See also
[`docs/08-brand/icon-explorations/README.md`](../../docs/08-brand/icon-explorations/README.md)
and the brand rules in [`docs/08-brand/brand.md`](../../docs/08-brand/brand.md#9-icon).

## Files

| File | What it is | Use it for |
|---|---|---|
| `app.svg` | 1024 canvas, 824 px Buckram squircle body (rx 185.4, 100 px gutter), paper cover and slot, Manila shelf | App icon at 48 px and up: Tauri source, macOS, Linux, `icon-192/512.png`. Cropped to its body (corners squared) for `apple-touch-icon.png` |
| `glyph-32.svg` | 32 px full-bleed tile, pixel-snapped, paper shelf | 32 px: `icon.ico`, `32x32.png`, `public/favicon.png`, the landing's `favicon.ico` |
| `glyph-24.svg` | 24 px full-bleed tile, pixel-snapped, paper shelf | 24 px layer of `icon.ico` |
| `glyph-16.svg` | 16 px full-bleed tile, the lean drawn as a one-pixel step with three mid-tone pixels | 16 px: `icon.ico`, the landing's `favicon.ico`, and the source of its `assets/favicon.svg` (which adds a dark-scheme palette) |
| `app-mono.svg` | Glyph only, one ink (`#000`), transparent canvas, same geometry as `app.svg` | macOS 26 Tinted/Clear template, Windows unplated tiles, single-ink print, embossing. Only the alpha matters: recolour it |
| `maskable.svg` | Full-bleed Buckram square, glyph at 88% so every pixel sits inside the 40% safe-zone circle (farthest point r ≈ 367 of 409.6) | `icon-maskable-512.png` (PWA `purpose: maskable`), Android adaptive icons |
| `mark.svg` | Glyph only in Buckram `#0E5E6F`, transparent | The mark on light backgrounds (lockups, docs, print) |
| `mark-dark.svg` | Glyph only in Buckram dark `#5DB6C4`, transparent | The mark on dark backgrounds |

Colours: tile `#0E5E6F` (Buckram), cover/slot `#FBFBF9` (paper), shelf `#E6C171`
(Manila). Favicon dark scheme: tile `#205F6A`, paper `#ECEEEA`.

## Which drawing at which size

| Size | Drawing | Why |
|---|---|---|
| 1024–64 | `app.svg` | Full detail. Manila shelf |
| 48 | `app.svg` cropped to its body (ico layer) | Still clean; shelf is 3 px |
| 32, 24, 16 | `glyph-32/24/16.svg` | Downscaling `app.svg` turns the 12° edge and the 1–2 px Manila shelf into mud. The glyphs snap the slot and shelf to whole pixels, draw the shelf in paper (Manila on Buckram is too weak this small), and fill the tile edge to edge |

## Geometry (1024 glyph space, before the 0.8 scale in `app.svg`)

- Cover 300 × 400, rx 48, rotated 12° about its bottom-right corner. Its lowest
  point sits on the same line as the slot's bottom edge (y 703) and its top-right
  corner touches the slot's outer edge (x 540): it leans, it does not float.
- Slot 302 × 404 outer (246 × 348 + 56 stroke), outer corner radius 50 to match
  the cover.
- Shelf 680 × 56, rx 28, 34 below the objects. Same weight as the slot stroke.
- Optical balance: the glyph's bounding box is centred horizontally (511.5) and
  sits 3.5 px above centre vertically (optical centre).

Final polish over the round-2 candidate: the cover was moved so it really
touches the slot (the candidate left an 8-unit hairline gap) and rests level
with it (the candidate's rounded corner floated 9 units higher), the slot moved
9 units left to recentre the pair, the shelf moved 16 units up so the objects
read as standing on it, and the group was re-centred (5 units higher). Hand-tuned 24 and 32 px glyphs were added and
the 16 px glyph got mid-tone pixels so its lean reads as a slant, not a notch.

## Clear space and minimum size

- Clear space: **25% of the icon width** on every side (256 px at 1024). For
  `mark.svg`, measure from the glyph's bounding box.
- Minimum: 16 px on screen (use `glyph-16.svg`, never a downscale of `app.svg`),
  5 mm in print. Below 128 px use the icon, not the mark with a wordmark lockup.

## Don'ts

- Do not redraw the glyph in another colour mix. One paper colour for cover and
  slot; Manila only on the shelf, and only at 48 px and up.
- Do not add shadows, gloss, gradients, blur or blend modes. Layers stay flat
  and opaque (Icon Composer and the OS add depth).
- Do not change the lean angle, flip it, or stand the cover upright.
- Do not fill the slot. The empty slot is the point.
- The app icon tile is always Buckram (`#0E5E6F`, or `#205F6A` for the dark
  favicon), never near-black or another hue.
- Do not stretch, rotate the whole mark, or crop into the shelf.
- Do not downscale `app.svg` below 48 px; use the hand-tuned glyphs.
- Do not combine it with controllers, cartridges, console logos or letterforms.
