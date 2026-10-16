# ADR-0008: Updater on public GitHub Releases, signed with minisign

## Status

Accepted (implementation: milestone M7)

## Date

2026-10-15

## Context

Users must get updates easily on three OSes, by publishing a release and nothing more. Pluryprint already does this with `tauri-plugin-updater`, a `latest.json` manifest on GitHub Releases and minisign signatures, without OS code-signing certificates (its ADR-0011). The research ([`desktop-stack-updater.md`](../02-research/desktop-stack-updater.md)) confirmed that the updater downloads release assets anonymously, which only works when the repository is **public**; the repo is private today.

## Decision

- Use **tauri-plugin-updater** (2.14.x, with Tauri 2.12.x and tauri-action v1 pinned), `createUpdaterArtifacts: true`, `updaterJsonPreferNsis: true`.
- Endpoint: `https://github.com/MAECLY/coverly-roms/releases/latest/download/latest.json`; a **stable** and a **beta** channel, as in Pluryprint (`release.yml` + `release-channels.yml`).
- Updates are signed with a **new minisign keypair**; the private key lives only in repository secrets, never in git.
- No OS certificates at first: macOS builds are unsigned and the README explains "Open Anyway".
- The repository becomes **public before the first release** (v0.1.0).
- The CLI gets `coverly update check|install` on the same manifest.

## Consequences

### Positive

- Shipping an update is just publishing a GitHub release; clients verify every update with minisign.
- Public repositories get free `ubuntu-22.04-arm` runners for Linux ARM builds.

### Negative

- Unsigned binaries trigger Gatekeeper and SmartScreen warnings.
- The code must be public before the first release, which fixes the timing of the switch.

### Risks / Open Questions

- Losing the minisign private key would strand installed clients; it needs an offline backup.
