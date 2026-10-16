# CI/CD

> **TL;DR.** `ci.yml` is the pull-request gate: frontend lint, typecheck, format, tests and a production build on ubuntu, and Rust `fmt`, `clippy` and the full workspace test suite on Linux, Windows and macOS. It builds no installer, reads no secret and publishes nothing. `release.yml` (on a version tag) builds every platform into a **draft** release signed with the project's minisign key, and `release-channels.yml` (on publish) moves the beta channel's manifest. The owner's procedure is [RELEASING.md](../../RELEASING.md); the design is [ADR-0011](../adr/0011-release-pipeline-and-update-key.md).

## Workflows that exist

| Workflow | File | Trigger | Signs? | Publishes? |
|---|---|---|---|---|
| **CI** | `.github/workflows/ci.yml` | `pull_request`, `push` → `main`, `workflow_dispatch` | No | No |
| **Release** | `.github/workflows/release.yml` | `push` of a tag `vX.Y.Z` or `vX.Y.Z-*` | Yes: a throwaway key in `app`, the real key only in `finalize` | A **draft** release (a person publishes it) |
| **Release channels** | `.github/workflows/release-channels.yml` | `release: published` | No | Replaces `latest.json` on the `channel-beta` prerelease (forward only) |

`ci.yml` cancels superseded runs:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

If a new commit lands on a PR, the in-flight run for the previous commit is cancelled. The workflow has `permissions: contents: read` and checks out with `persist-credentials: false`.

Every third-party action is pinned to a full commit SHA (with the tag as a comment), the same pins Pluryprint uses.

## `ci.yml` job by job

Two jobs, both blocking.

### `frontend` — ubuntu-latest

Node 24, pnpm from `package.json` → `packageManager`. `pnpm install --frozen-lockfile --ignore-scripts`, then:

1. `pnpm lint` (ESLint, including `eslint-plugin-jsx-a11y`)
2. `pnpm typecheck` (`tsc --noEmit`)
3. `pnpm format:check` (Prettier)
4. `pnpm test` (Vitest)
5. `pnpm build` (`tsc -b && vite build`)

### `backend` — matrix `ubuntu-latest` + `windows-latest` + `macos-latest`

`coverly-core` and `coverly-cli` deal with filesystem paths, case sensitivity and hidden files, and the app ships on all three desktop platforms, so the whole Cargo workspace is checked on each one. `fail-fast: false` keeps one platform's failure from hiding the others'.

1. Linux only: install the Tauri 2 system libraries (`libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf build-essential libssl-dev libxdo-dev`).
2. Install the stable Rust toolchain with `rustfmt` and `clippy` through `rustup` (preinstalled on hosted runners).
3. Restore the cargo registry and `target/` cache, keyed on `Cargo.lock`.
4. `mkdir -p dist`: `tauri::generate_context!` in `src-tauri` requires `frontendDist` (`../dist`) to exist at compile time. The Rust checks do not need the real bundle, so an empty directory is enough.
5. Linux only: `cargo fmt --all -- --check` (formatting does not depend on the platform).
6. `cargo clippy --workspace --all-targets -- -D warnings`
7. `cargo test --workspace` — the full suite, never filtered.

## `release.yml` job by job

Top-level `permissions: {}`; each job asks for what it needs. `concurrency: release-<ref>` without cancelling, so a tag's run is never cut in half. Every action is pinned to a full SHA; tauri-action is `tauri-apps/tauri-action@1deb371b0cd8bd54025b384f1cd735e725c4060f # action-v1.0.0`.

| Job | Needs | Runs on | Permissions | Secrets | What it does |
|---|---|---|---|---|---|
| `guard` | — | ubuntu-latest | `contents: read` | reads whether `TAURI_SIGNING_PRIVATE_KEY` is set | Refuses a missing secret, a private repository, `updater.pub` still `UNSET`, versions that disagree (`scripts/bump-version.sh --check`), a tag ≠ `Cargo.toml`, or no `## [X.Y.Z]` in `CHANGELOG.md`. |
| `draft` | guard | ubuntu-latest | `contents: write` | `GITHUB_TOKEN` | Creates the draft (prerelease for a `-pre` tag) with `.github/release-notes.md`, or reuses the draft on a re-run; refuses an already published tag. |
| `app` | draft | macos-latest (universal: `--target universal-apple-darwin`), windows-latest, ubuntu-22.04, ubuntu-22.04-arm | `contents: write` (tauri-action uploads to the draft) | `GITHUB_TOKEN` only | Generates a throwaway updater key and a `--config` overlay with its public half (`tauri build` decodes `plugins.updater.pubkey`, which is only a marker in the repo), then tauri-action v1 builds the bundles and update artifacts and merges its platform into the draft's `latest.json` (`uploadUpdaterJson: true`, `updaterJsonPreferNsis: true`, `uploadUpdaterSignatures: false`). |
| `cli` | guard | the same 5 targets | `contents: read` | none | `cargo build --release --locked -p coverly-cli`, staged as `coverly-<version>-<os>-<arch>[.exe]` workflow artifacts. |
| `finalize` | draft, app, cli | ubuntu-latest | `contents: write` | the two signing secrets, in one step | Downloads `latest.json` and every update artifact from the draft and the CLI artifacts; signs each with `tauri signer sign --app-version`; `release_tools.py resign-manifest`, `add-cli`, `check-manifest` (10 keys, signatures bound to the version); `minisign -V` of every file against the committed `updater.pub` and a tampered copy that must fail; `SHA256SUMS` over every file (the `.dmg` included); uploads `latest.json`, `SHA256SUMS`, the `.sig` files and the CLI binaries. |

macOS is signed ad hoc (`signingIdentity: "-"`) and Windows installers are not Authenticode-signed (ADR-0008); the release notes explain "Open Anyway" and "Run anyway".

## `release-channels.yml`

Runs when a release is published (drafts and `channel-beta` itself are ignored). One job, `beta-pointer`, with `contents: write`: it downloads the published release's `latest.json`, checks it with `release_tools.py check-manifest` (10 keys, signed version), creates the `channel-beta` prerelease the first time (`--latest=false`), and replaces its `latest.json` only when the new version is higher (`release_tools.py semver-gt`). Stable needs no workflow: `releases/latest` is GitHub's newest published non-prerelease.

## The release helper

`scripts/release/release_tools.py` (Python 3.9+ standard library, run with `python3 -I`) holds every rule the workflows depend on — `semver-gt`, `check-manifest`, `list-artifacts`, `resign-manifest`, `add-cli`, `pubkey` — so `src-tauri/tests/release_pipeline.rs` tests them on a developer machine, together with workflow invariants (pins, permissions, which job sees the secrets) and the docs this page links.

## Running the same checks locally

`make check` runs every gate CI runs, without writing:

```bash
pnpm lint && pnpm typecheck && pnpm format:check && pnpm test
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## What CI does not run (yet)

| Check | Status |
|---|---|
| Installers / bundles (`pnpm tauri build`) | Only in `release.yml`, on a tag; locally `make build` (no update artifacts) or RELEASING.md §7 |
| Dependency audits (`pnpm audit`, `cargo audit`) | Not in CI yet |
| Coverage (`pnpm test:coverage`) | Local only |
