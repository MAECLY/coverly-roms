# ADR-0004: Licence: AGPL-3.0-only with CLA and trademark policy

## Status

Accepted

## Date

2026-10-15

## Context

Coverly ROMs is for public use and should stay free software, under the same scheme as the author's other projects (Pluryprint, after its ADR-0010, and Versorium). Two more forces apply: the name and mark must stay protected even though the code is open, and the copyright holder wants to keep the option of other licensing, which needs contributors to grant those rights. ScreenScraper also only grants API access to **free** apps (spec §16.9).

## Decision

- The code is licensed **AGPL-3.0-only**, declared in every `Cargo.toml` and `package.json`, with the full text in `LICENSE` (see also `LICENSES/`).
- An additional term under AGPL §7(e) grants **no rights to the trademarks**; [`TRADEMARKS.md`](../../TRADEMARKS.md) sets the policy.
- Contributions are accepted under the [CLA](../../CLA.md); the pull request template asks contributors to confirm it.
- Copyright holder: Miguel Angel Esparza Calero (MAECLY).
- The app downloads art from third-party sources at the user's request; it bundles and redistributes no ROMs and no art.

Details: [`docs/09-legal/license.md`](../09-legal/license.md) and [`trademark.md`](../09-legal/trademark.md).

## Consequences

### Positive

- Same legal scheme as the sibling projects; files and policies are reused.
- Free-software status keeps Coverly eligible for the ScreenScraper API.

### Negative

- The CLA adds a step for first-time contributors.
- Copyleft over the network can deter some downstream reuse of the core.

### Risks / Open Questions

- The terms of libretro-database DATs (download at runtime vs redistribution) are still to be checked (spec §17, question 3).
