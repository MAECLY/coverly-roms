# CI/CD

> **TL;DR.** `ci.yml` is the pull-request gate: frontend lint, typecheck, format, tests and a production build on ubuntu, and Rust `fmt`, `clippy` and the full workspace test suite on Linux, Windows and macOS. It builds no installer, reads no secret and publishes nothing. There is no release workflow yet (it arrives with the first packaged build).

## Workflows that exist

| Workflow | File | Trigger | Signs? | Publishes? |
|---|---|---|---|---|
| **CI** | `.github/workflows/ci.yml` | `pull_request`, `push` → `main`, `workflow_dispatch` | No | No |

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
| Installers / bundles (`pnpm tauri build`) | Not in CI; arrives with the release workflow |
| Dependency audits (`pnpm audit`, `cargo audit`) | Not in CI yet |
| Coverage (`pnpm test:coverage`) | Local only |
