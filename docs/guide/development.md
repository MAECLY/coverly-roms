# Contributing and building from source

[Coverly ROMs](../../README.md) › Contributing and building from source · **English** · [Español](development.es.md)

## Help make it better

- **Try it on a real handheld** and [open an issue](https://github.com/MAECLY/coverly-roms/issues)
  saying which firmware, which layout was detected and how the fill went.
- **Add a layout profile or a folder alias** your firmware uses.
- **Improve a translation** in [`locales/en/ui.json`](../../locales/en/ui.json) or
  [`locales/es/ui.json`](../../locales/es/ui.json).

Read [CONTRIBUTING.md](../../CONTRIBUTING.md) first. In short: contributions are made
under the [Contributor License Agreement](../../CLA.md); `make check` runs everything
CI runs; commits follow [Conventional Commits](https://www.conventionalcommits.org/),
checked by commitlint; new behaviour in `coverly-core` comes with its test
first; and the repository never takes ROMs or game art. Security reports go
through [SECURITY.md](../../SECURITY.md) (a private GitHub advisory, or
`security@maecly.com`).

## For developers

### Stack, layout and building from source

**Stack.** Tauri 2 · Rust · React + TypeScript + Vite + Tailwind.

| Path                  | What it holds                                                      |
| --------------------- | ------------------------------------------------------------------ |
| `crates/coverly-core` | Scanning, identification, sources, fill, organise, undo (no Tauri) |
| `crates/coverly-cli`  | The `coverly` command-line tool                                    |
| `src-tauri`           | The desktop shell and its commands                                 |
| `src`                 | The React interface                                                |

**Prerequisites:** [Rust](https://rustup.rs/) 1.93 or newer,
[Node.js](https://nodejs.org/) 22.22.2 or newer with [pnpm](https://pnpm.io/), and
the [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/)
for your OS. `make doctor` shows which ones are installed.

```sh
make install                                   # install JS and Rust dependencies
make dev                                       # run the desktop app in development
make web                                       # only the UI, in a browser, with a mock backend
make example                                   # build a small example library and audit it
make example-fill                              # plan filling its covers (dry run)
make audit ROOT=<path>                         # audit a library from the terminal
make check                                     # lint, format check and all tests
```

Also: `make build` (an unsigned release bundle), `make cli ARGS="systems"`
(run the CLI), `make fill ROOT=<path>` (plan a fill, dry run).

`make help` lists every target. The example library lives in `target/example`
(`make example-apply`, `make example-undo`, `make example-clean`).

The landing page at [coverly-roms.maecly.com](https://coverly-roms.maecly.com)
lives in its own private repository, MAECLY/coverly-roms-landing,
which explains how to preview, check and publish it.

**Docs.** [docs/README.md](../README.md) is the index. Architecture in
[docs/04-architecture/](../04-architecture/system-architecture.md), decisions
in [docs/adr/](../adr/README.md), the release process in
[RELEASING.md](../../RELEASING.md) and the history in [CHANGELOG.md](../../CHANGELOG.md).
