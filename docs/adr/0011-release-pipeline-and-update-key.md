# ADR-0011: Release pipeline and the update key

## Status

Accepted (implemented in M7)

## Date

2026-10-16

## Context

[ADR-0008](./0008-updater-public-releases.md) decided **what** ships: `tauri-plugin-updater` on public GitHub Releases, minisign signatures, a stable and a beta channel, and `coverly update check|install` on the same manifest. It left the **how** open, under constraints that only became concrete in M7:

- Three things belong to the owner alone and cannot exist while the pipeline is built: the real minisign keypair and its two secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), the switch to a public repository, and publishing v0.1.0. The code must build, and every test must pass, without the real key.
- `tauri build` with `bundle.createUpdaterArtifacts: true` needs a signing key **and** base64-decodes `plugins.updater.pubkey`, so the config cannot hold a placeholder that is not a key.
- The `app` jobs compile third-party code (npm and crates.io dependencies, build scripts). Anything they can read, a compromised dependency can read.
- The updater downloads a manifest that anyone who controls the network or a release asset could replay: a genuine old artifact relabelled as a newer version must be refused.
- The CLI has no Tauri runtime but must trust exactly the same key and manifest.
- An update must never interrupt a fill job writing to an SD card.

## Decision

1. **One place for the public key: `crates/coverly-core/updater.pub`.** It holds `#` comment lines and one key line, the placeholder `UNSET` until the owner commits the real key (RELEASING.md §2). `coverly-core` embeds it with `include_str!` and exposes `release_pubkey()` (`None` while `UNSET`). The app passes it to the plugin at run time (`updater_builder().pubkey(..)`), and `tauri.conf.json` → `plugins.updater.pubkey` only holds the marker `set at runtime from crates/coverly-core/updater.pub`. With `UNSET`, the app answers `update-no-key` and `coverly update install` exits 6. Debug CLI builds also accept `COVERLY_UPDATE_PUBKEY` for local tests; release builds ignore it.
2. **Builds never see the real key, and cannot write.** The jobs that compile third-party code (`app`, `cli`) hold `contents: read` only and never see the signing secret. Each `app` job runs `pnpm tauri build` with a throwaway key generated on the spot (its public half passed through a `--config` overlay so `tauri build` can decode it), stages the bundles under their release names and hands them over with `actions/upload-artifact`; it cannot push, tag or touch the draft. Only two jobs hold `contents: write`, and neither compiles anything: `draft` runs `gh` to create the draft, and `finalize` downloads the workflow artifacts, signs every update artifact and CLI binary with `tauri signer sign --app-version <version>` (the one step that sees the real key; the Tauri CLI comes from the lockfile, installed without install scripts), writes `latest.json` itself with `release_tools.py updater-manifest` (`darwin-aarch64`/`darwin-x86_64` → the universal `.app.tar.gz`, `windows-x86_64` → the NSIS installer as tauri-action's `updaterJsonPreferNsis` did, `linux-<arch>` → the AppImage, plus bundle-specific keys), verifies every signature with `minisign` against the committed `updater.pub`, including a negative check on a tampered copy, and uploads every file to the draft. The app enables `requireSignedVersion: true` and the CLI checks the same `version:` field, so a signature made for another version is refused (no replay, no downgrade by relabelling).
3. **CLI binaries live in the same `latest.json`.** `finalize` adds `cli-<os>-<arch>` entries (`cli-darwin-aarch64`, `cli-darwin-x86_64`, `cli-windows-x86_64`, `cli-linux-x86_64`, `cli-linux-aarch64`) next to the app's five keys, signed the same way. One manifest per channel serves both programs; `release_tools.py check-manifest` requires all ten keys and signatures bound to the version. `SHA256SUMS` covers every downloadable file.
4. **Updates are driven from Rust.** The webview has no `updater:` permission (capabilities stay `core:default` + `dialog:allow-open`); it calls `check_update`, `download_update`, `install_update` and `restart_app`, and the endpoint and key are always chosen in Rust. The policy lives in `UpdateManager` behind an `UpdateDriver` trait, unit-tested with a fake driver. The startup check runs only with "check on start" on (otherwise no request at all) and fails silently; a manual check shows its error. Installing is refused with `job-running` while a fill job is running **or paused**, and so is restarting; the UI waits for the job instead of interrupting it. Manifest checks are bounded at 30 s, downloads at 20 min, release notes at 64 KiB.
5. **One version source.** The workspace `Cargo.toml` version is the only version; `tauri.conf.json` has none, `package.json` and `Cargo.lock` are kept equal by `scripts/bump-version.sh`, and the `guard` job refuses a tag that differs from it or has no `CHANGELOG.md` section.

The beta channel is a fixed prerelease tagged `channel-beta` whose `latest.json` `release-channels.yml` moves forward only (SemVer precedence) on every publish; the stable channel is GitHub's `releases/latest`.

## Consequences

### Positive

- The repository builds, tests and ships development builds with no secret at all; the owner's steps are a short, checkable runbook ([RELEASING.md](../../RELEASING.md)).
- A compromised build dependency cannot steal the signing key (it never reaches a job that compiles third-party code), and it cannot push, tag or replace draft assets either: the build jobs are read-only and only hand files to `finalize`. What `finalize` signs is exactly what the read-only builds produced, and every signature is verified against the committed key before anything is uploaded.
- A replayed or relabelled manifest is refused by both the app and the CLI, because each signature is bound to its version.
- A person always publishes: the draft can be inspected, and nothing is offered to users before that.
- The update policy (startup gating, job gating) is covered by fast unit tests that need neither a network nor a Tauri runtime.

### Negative

- `finalize` cannot detect a build that was compromised while it compiled: a malicious dependency or build script that alters the binaries it is building ends up in the bundles, and `finalize` signs them like any others. Separating the jobs only stops a build from reaching the key, the repository and the draft. Mitigations are upstream of this pipeline: the lockfiles (`pnpm-lock.yaml`, installed with `--frozen-lockfile --ignore-scripts`; `Cargo.lock`, which the CLI builds with `--locked`), actions pinned to full SHAs, and a person checking the draft before publishing.
- `finalize` runs the Tauri CLI (an npm package from the lockfile) with the real key and `contents: write`; a compromised `@tauri-apps/cli` would see the key there.
- `pnpm tauri build` needs a key locally, because `createUpdaterArtifacts` is on: `make build` turns update artifacts off, or a throwaway key plus a pubkey overlay is used (RELEASING.md §7).
- `latest.json` is written in one place (`finalize`) instead of being merged into the draft by four parallel jobs, so the old race that could drop a platform is gone; the price is that the platform-key mapping tauri-action used to do now lives in `release_tools.py` and its tests.
- The key is compiled in: rotating it, or losing it, means every installed copy must be reinstalled by hand once.

### Risks / Open Questions

- Until the owner commits the real key and makes the repository public, nothing can be released; the `guard` job says which step is missing.
- Windows installers and macOS builds are still not signed with OS certificates (ADR-0008); the release notes and the README explain the warnings.
