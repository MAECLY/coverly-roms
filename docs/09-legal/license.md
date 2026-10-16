# License

> **TL;DR.** Coverly ROMs is licensed under the GNU Affero General Public License, version 3 only (`AGPL-3.0-only`). You can use it for anything (including commercial use), modify it, and redistribute it; if you distribute a modified version, or let people use a modified version over a network, you must offer them its source under the same licence. Contributions come in under the [Contributor License Agreement](../../CLA.md): contributors keep their copyright and give the owner a licence that lets him also offer Coverly ROMs on other terms. The name and the mark are not covered by the licence ([`TRADEMARKS.md`](../../TRADEMARKS.md)). The art Coverly ROMs downloads is not covered by it either (see [Art and metadata](#art-and-metadata-are-not-ours)).

This document is a plain-language explanation. **It is not legal advice.** The binding text is `LICENSE` at the repository root, the Free Software Foundation's AGPL-3.0 text, verbatim. A copy will ship inside every Coverly ROMs bundle, next to [`THIRD-PARTY-NOTICES.md`](../../THIRD-PARTY-NOTICES.md).

## What is the AGPL-3.0?

The **GNU Affero General Public License version 3** is the GNU GPL version 3 with one added section (section 13). Like the GPL it guarantees four freedoms: to run the software for any purpose, to study it, to redistribute it, and to modify it and distribute the modified versions.

The copyleft part means any modified version you distribute must also be released under the AGPL-3.0, with its source. Section 13 adds that if you **modify** the program and let people **interact with it remotely over a network**, you must offer them the source of your modified version too.

## The additional term about names and marks

`TRADEMARKS.md` states one additional term under section 7(e) of the AGPL: the licence grants no rights under trademark law to the Coverly ROMs name, wordmark and mark beyond reasonable and customary use in describing where the work came from, for example "a fork of Coverly ROMs". A fork is free to exist; it has to carry its own name. See [trademark.md](./trademark.md).

## What you (the user) can do

| Activity | Allowed? | Notes |
|---|---|---|
| Use Coverly ROMs at home or at work | Yes | No restrictions on use. |
| Install it on as many machines as you want | Yes | No licence keys, no fees. |
| Give copies to others | Yes | Provide the source or a written offer, and keep `LICENSE` and the notices with it. |
| Sell copies | Yes | You cannot stop recipients from redistributing. |
| Modify it for private use | Yes | No obligation while you neither distribute it nor offer it over a network. |
| Modify it and distribute the modification | Yes, under the AGPL-3.0 with source | Copyleft. |
| Embed `coverly-core` in a closed-source product | **No** | A combined work must be AGPL-3.0 as a whole. Running the `coverly` CLI as a separate process is generally "mere aggregation". Ask for a commercial licence otherwise. |
| Use the name and mark for your modified version | **No** | See [trademark.md](./trademark.md). |

## What you (the contributor) commit to

Contributions are made under the [Contributor License Agreement](../../CLA.md) (version 1.0, 2026-10-15):

1. **You keep the copyright in your contribution.** There is no assignment.
2. **You give the owner a broad licence to it**, including the right to publish it under other terms later (another open-source licence or a commercial one).
3. **You give a patent licence** for claims your contribution necessarily infringes.
4. **You promise the contribution is yours to give**, or say whose it is and under which licence.

You agree to it by opening a pull request. A CLA rather than a DCO sign-off keeps the owner able to offer Coverly ROMs on other terms later, as with his other apps Pluryprint and Versorium, without taking contributors' copyright away. Published versions stay available under the licence they were published with.

## Art and metadata are not ours

Coverly ROMs does not bundle or redistribute ROMs, BIOS files, game art or game databases. At the user's request it downloads art from third-party sources — libretro-thumbnails by default, and optionally ScreenScraper, SteamGridDB or TheGamesDB — and identification data from libretro-database, and writes them into the user's own library.

- The AGPL-3.0 on Coverly ROMs does **not** apply to that art or data, and the program's output (images placed in your library, `gamelist.xml` entries, audit reports) is not a derivative of the program.
- The art belongs to each game's developers and publishers. libretro-thumbnails states that its art "originates from the work of each respective game's developers and publishers" and publishes no licence for it. Each source's own terms apply to what you download from it; for example, ScreenScraper's API may only be used in fully free, distributed applications.
- Whether you may keep, share or publish the art you download is between you and its owners. Coverly ROMs only fetches it for your personal library.

## Source code availability

If you distribute Coverly ROMs or a modified version, you must make the corresponding source available (AGPL section 6): with the binary, through a written offer valid for at least three years, or from a network server at no charge, such as a public repository. For the binaries we publish on GitHub Releases, the source is in the same repository.

## Licence compatibility

The AGPL-3.0 can include code under AGPL-3.0, GPL-3.0, GPLv2-or-later, LGPL, MIT, ISC, Zlib, BSD-2/3-Clause, Unicode-3.0, Apache-2.0, and MPL-2.0 (unless marked "Incompatible With Secondary Licenses"). It cannot include GPLv2-only, CDDL, EPL, MPL 1.x or proprietary code. Every dependency is checked against this list before it is added, and its notice goes into [`THIRD-PARTY-NOTICES.md`](../../THIRD-PARTY-NOTICES.md).

## "Only", not "or later"

The SPDX id is `AGPL-3.0-only`, in every `Cargo.toml` and in `package.json`. A future AGPL version does not apply automatically; because contributions come in under the CLA, the owner can adopt one without asking every contributor again.

## How to comply if you redistribute

- [ ] Include `LICENSE`, `THIRD-PARTY-NOTICES.md` and the texts in `LICENSES/`.
- [ ] Keep the existing copyright and licence notices.
- [ ] Make the corresponding source available.
- [ ] If you modified it and offer it over a network, offer those users the source.
- [ ] Mark your modifications clearly.
- [ ] Change the name and the mark if you distribute a modified version.
- [ ] Add no restrictions beyond the AGPL-3.0.

## Where to read the actual licence

- The AGPL-3.0 text: <https://www.gnu.org/licenses/agpl-3.0.html>
- The FSF's FAQ: <https://www.gnu.org/licenses/gpl-faq.html>
- Our `LICENSE` file at the repository root (the operative copy).

## Disclaimer

This document is a summary written by the project's owner, not a lawyer. For specific legal questions, consult a qualified attorney.

## See also

- [`CLA.md`](../../CLA.md)
- [`TRADEMARKS.md`](../../TRADEMARKS.md)
- [trademark.md](./trademark.md)
- [`THIRD-PARTY-NOTICES.md`](../../THIRD-PARTY-NOTICES.md)
