# Contributing to Coverly ROMs

Contributions are welcome: bug reports, fixes, new layout profiles, system
aliases, translations, ideas.

## Before you open a pull request

- **Read and agree to the [Contributor License Agreement](CLA.md).** You keep
  the copyright in what you write; the agreement lets the project's owner keep
  publishing Coverly ROMs, change its licence or offer it commercially later.
  The pull request template asks you to confirm it.
- **Run the checks.** `make check` runs everything CI runs: frontend lint,
  formatting, type checks and unit tests, and on the Rust side
  `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace`.
- **Keep it small and say why.** One change per pull request, in
  [Conventional Commits](https://www.conventionalcommits.org/) form
  (`fix(scan): …`, `feat(profiles): …`, `docs: …`). Commit messages are
  checked by commitlint.
- **Write the test first.** New behaviour in `coverly-core` comes with a unit
  test or a fixture library tree that shows it.

## Ground rules for the code

- `crates/coverly-core` never depends on Tauri; the app and the CLI are thin
  layers over it.
- Never write to a user's library outside the operation the user asked for,
  and never outside the library root. The audit is read-only.
- Compare file extensions and folder names case-insensitively, and ignore
  macOS AppleDouble files (`._*`), `.DS_Store`, `Thumbs.db` and hidden entries.
- Interface text goes through i18n (`locales/en/ui.json`,
  `locales/es/ui.json`); never hard-code a user-visible string.
- Docs are written in English.

## Do not contribute ROMs or art

Coverly ROMs does not ship ROMs, BIOS files or game art, and the repository
must not contain them. Test fixtures are generated in tests from tiny fake
files.

## Licence and name

Coverly ROMs is published under the
[GNU Affero General Public License v3.0](LICENSE). The name, the wordmark and
the mark are not covered by that licence: a fork must use its own (see
[TRADEMARKS.md](TRADEMARKS.md)). For a commercial licence or any other
arrangement, write to hola@maecly.com.
