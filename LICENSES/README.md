# Licence texts

This folder holds the full texts of the third-party licences that Coverly ROMs
must carry with its source and its binaries. `THIRD-PARTY-NOTICES.md` says
which component each one applies to. The whole folder is bundled into every
build as `LICENSES/`.

- `CC-BY-4.0.txt`: the system icons (RetroArch Assets).
- `ISC-lucide-react.txt`: the interface icons (lucide-react, with Feather's
  MIT notice).
- `Apache-2.0.txt`: pdf.js (`pdfjs-dist`), the PDF viewer.
- `BSD-3-Clause-Adobe-CMaps.txt`: the Adobe CMaps bundled with pdf.js.
- `BSD-3-Clause-PDFium-Foxit.txt`: the Foxit standard fonts bundled with
  pdf.js.
- `Liberation-fonts.txt`: the Liberation Sans fonts bundled with pdf.js
  (GPL v2 with the Liberation font exceptions).
- `generated/`: `rust.txt` and `js.txt`, one block per Rust crate and npm
  package in the build with its full licence text, generated from `Cargo.lock`
  and `pnpm-lock.yaml` by `pnpm notices` (checked by `pnpm notices:check`; do
  not edit by hand). The app shows them in Settings → Credits.
- `fonts/` (in the bundle only): the SIL Open Font License 1.1 of each
  typeface, copied at build time from `src/assets/fonts/OFL-*.txt`, where they
  sit next to the font files.

Coverly ROMs' own licence, the GNU Affero General Public License v3.0, is in
[`LICENSE`](../LICENSE) at the repository root.
