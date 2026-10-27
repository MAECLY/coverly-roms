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

Top-level `permissions: {}`; each job asks for what it needs. `concurrency: release-<ref>` without cancelling, so a tag's run is never cut in half. Every action is pinned to a full SHA. Only `draft` and `finalize` hold `contents: write`, and neither compiles third-party code; the build jobs are read-only and hand their files to `finalize` as workflow artifacts (ADR-0011).

| Job | Needs | Runs on | Permissions | Secrets | What it does |
|---|---|---|---|---|---|
| `guard` | — | ubuntu-latest | `contents: read` | reads whether `TAURI_SIGNING_PRIVATE_KEY` is set | Refuses a missing secret, a private repository, `updater.pub` still `UNSET`, versions that disagree (`scripts/bump-version.sh --check`), a tag ≠ `Cargo.toml`, or no `## [X.Y.Z]` in `CHANGELOG.md`. |
| `draft` | guard | ubuntu-latest | `contents: write` | `GITHUB_TOKEN` | Runs only `gh`. Creates the draft (prerelease for a `-pre` tag) with `.github/release-notes.md`, or reuses the draft on a re-run; refuses an already published tag. |
| `app` | guard | macos-latest twice (Apple silicon `--target aarch64-apple-darwin`; Intel cross-compiled with `--target x86_64-apple-darwin`), windows-latest (x64), windows-11-arm (ARM64, native), ubuntu-22.04, ubuntu-22.04-arm | `contents: read` | none | On `windows-11-arm`, first checks the toolchain (installs rustup after checking its SHA-256; fails with `::error::` if `clang` or, on a stable tag, .NET Framework 3.5 for WiX is missing). Generates a throwaway updater key and a `--config` overlay with its public half (`tauri build` decodes `plugins.updater.pubkey`, which is only a marker in the repo), runs `pnpm tauri build` (`--bundles nsis` on Windows for a prerelease tag), flattens the bundles under Tauri's own names (spaces replaced; the macOS `.app.tar.gz` gets the version and the matrix's `tauri_arch`, `aarch64` or `x64`; the throwaway `.sig` files stay behind) and uploads them as the workflow artifact `app-<os>-<arch>`. Release names are chosen in `finalize`, not here. |
| `cli` | guard | the same 6 targets (the Windows ARM64 one on windows-11-arm after the same rustup and `clang` check) | `contents: read` | none | `cargo build --release --locked -p coverly-cli`, staged as `coverly-<version>-<os>-<arch>[.exe]` workflow artifacts (`cli-<os>-<arch>`). |
| `finalize` | draft, app, cli | ubuntu-latest | `contents: write` | the two signing secrets, in one step | Compiles nothing. Downloads the `app-*` and `cli-*` workflow artifacts, each into its own folder; `release_tools.py stage-builds` checks that the run holds exactly the 12 expected artifacts, each with only its own platform's Tauri or Cargo names for this version, and copies every file under its release name (`Coverly-ROMs_<version>_mac_arm.dmg`, `coverly_<version>_windows_arm.exe`, …; spec `docs/superpowers/specs/2026-10-26-release-names.md` §4); signs every update artifact (`release_tools.py updater-artifacts`) and CLI binary with `tauri signer sign --app-version`; writes `latest.json` with `release_tools.py updater-manifest` and `add-cli`, then `check-manifest` (12 required keys: six `<os>-<arch>` and six `cli-<os>-<arch>`, signatures bound to the version; the keys are the ones installed copies already ask for, only the urls carry the release names); `minisign -V` of every file against the committed `updater.pub` and a tampered copy that must fail; `SHA256SUMS` over every file (the `.dmg`, `.msi` and `.deb` included); uploads the bundles, their `.sig` files, the CLI binaries, `latest.json` and `SHA256SUMS` to the draft. |

macOS is signed ad hoc (`signingIdentity: "-"`) and Windows installers are not Authenticode-signed (ADR-0008); the release notes explain "Open Anyway" and "Run anyway".

## `release-channels.yml`

Runs when a release is published (drafts and `channel-beta` itself are ignored). One job, `beta-pointer`, with `contents: write`: it downloads the published release's `latest.json`, checks it with `release_tools.py check-manifest` (the same 12 keys, signed version), creates the `channel-beta` prerelease the first time (`--latest=false`), and replaces its `latest.json` only when the new version is higher (`release_tools.py semver-gt`). Stable needs no workflow: `releases/latest` is GitHub's newest published non-prerelease.

## The release helper

`scripts/release/release_tools.py` (Python 3.9+ standard library, run with `python3 -I`) holds every rule the workflows depend on — `semver-gt`, `semver-check`, `stage-builds`, `updater-artifacts`, `updater-manifest`, `check-manifest`, `add-cli`, `pubkey`, and `list-artifacts`/`resign-manifest` for recovery — so `src-tauri/tests/release_pipeline.rs` tests them on a developer machine, together with workflow invariants (pins, permissions, which job sees the secrets) and the docs this page links.

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
