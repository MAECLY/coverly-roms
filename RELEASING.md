# Releasing Coverly ROMs

> **TL;DR.** Push a tag `vX.Y.Z` and `.github/workflows/release.yml` builds every
> platform into a **draft** GitHub release, signed with the project's minisign
> key. A person checks the draft and clicks **Publish**; from that moment the
> app and `coverly update` offer the new version, and the landing page links
> it. Before the very first release the owner does three one-time things:
> create the signing key and its two secrets (§2), commit the public key to
> `crates/coverly-core/updater.pub` (§2), and make the repository public (§3).
> All three are done: the key is committed and the repository is public.

This runbook is for the maintainer. The design is in
[ADR-0008](docs/adr/0008-updater-public-releases.md) (what) and
[ADR-0011](docs/adr/0011-release-pipeline-and-update-key.md) (how); the moving
parts are described in [docs/04-architecture/updates.md](docs/04-architecture/updates.md)
and [docs/07-build-and-release/ci-cd.md](docs/07-build-and-release/ci-cd.md).

---

## 1. What a release is

```
git tag vX.Y.Z ──push──► release.yml
                          guard ─┬─► draft  (gh only, contents: write) ───────────────┐
                                 ├─► app    (4 builds, contents: read) ──artifacts──┤
                                 └─► cli    (5 builds, contents: read) ──artifacts──┴─► finalize
                                                                                         │
                       a draft release with every file, latest.json, SHA256SUMS ◄────────┘
person clicks Publish ──► release-channels.yml moves the beta channel (channel-beta)
```

`release.yml` runs on a tag `vX.Y.Z` (stable) or `vX.Y.Z-<pre>` (beta, for
example `v0.2.0-beta.1`). Its jobs:

| Job        | Runs on                                                | Does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | Holds                                             |
| ---------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| `guard`    | ubuntu                                                 | Refuses to start unless `TAURI_SIGNING_PRIVATE_KEY` is set (it reads only whether it is set, never its value), the repository is public, `crates/coverly-core/updater.pub` holds a real key (not `UNSET`), `scripts/bump-version.sh --check` passes, the tag equals the `Cargo.toml` version and `CHANGELOG.md` has a `## [X.Y.Z]` section.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | `contents: read`                                  |
| `draft`    | ubuntu                                                 | Runs only `gh`: creates the draft release (a prerelease for a `-pre` tag) with the bilingual notes from `.github/release-notes.md`, or reuses the draft of a re-run. Refuses a tag that is already published.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | `contents: write`                                 |
| `app`      | macOS (universal), Windows x64, Linux x64, Linux ARM64 | `pnpm tauri build` builds the bundles and their update artifacts (`bundle.createUpdaterArtifacts: true`), signed with a **throwaway key** generated in the job, then stages them under their release names and hands them to `finalize` as a workflow artifact (`app-<platform>`). On a prerelease tag, Windows builds the NSIS installer only (`--bundles nsis`): WiX refuses a non-numeric prerelease, so a beta has no `.msi`. It cannot write to the repository or the draft.                                                                                                                                                                                                                                                                                                                                          | `contents: read`, no signing secret               |
| `cli`      | the same 5 targets                                     | `cargo build --release -p coverly-cli`, staged as `coverly-<version>-<os>-<arch>[.exe]` and handed over the same way (`cli-<os>-<arch>`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | `contents: read`, no signing secret               |
| `finalize` | ubuntu                                                 | Compiles nothing. Downloads every `app-*` and `cli-*` artifact into its own folder (never merged) and checks with `release_tools.py stage-builds` that the run holds exactly those artifacts, each with only its own platform's files for this version (one build cannot add or replace another platform's file), then signs every update artifact and CLI binary with the **real key** bound to the version (`tauri signer sign --app-version`), writes `latest.json` itself (`release_tools.py updater-manifest` and `add-cli`), checks that all 10 keys are there, verifies every signature with `minisign` against the committed `updater.pub` (and that a tampered copy fails), writes `SHA256SUMS` over every file and uploads everything (bundles, `.sig`, CLI binaries, `latest.json`, `SHA256SUMS`) to the draft. | `contents: write`, the signing secrets (one step) |

Only `draft` and `finalize` can write, and neither runs third-party build
code (`finalize` runs the Tauri CLI from the lockfile, installed without
install scripts, to sign). `finalize` signs what the read-only builds
produced: no build job can push, tag or replace a draft asset any more, so
the signed files are the ones the builds handed over. It does **not** prove a
build was clean: a dependency compromised at compile time ends up in the
bundles and gets signed like everything else
([ADR-0011](docs/adr/0011-release-pipeline-and-update-key.md)).

Nothing is published automatically: the updater offers a version the moment
it is public, so a person always looks at the draft first (§5).

The two manifests the app and the CLI read:

| Channel | Manifest                                                                            | Moved by                                                                         |
| ------- | ----------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| stable  | `https://github.com/MAECLY/coverly-roms/releases/latest/download/latest.json`       | GitHub itself: "latest" is the newest published release that is not a prerelease |
| beta    | `https://github.com/MAECLY/coverly-roms/releases/download/channel-beta/latest.json` | `release-channels.yml`, on every publish (§6)                                    |

## 2. One-time owner setup: the signing key

**Status: done.** The real public key is in `crates/coverly-core/updater.pub`
(commit `chore(release): add the updater public key`). Repeat this section only
to rotate the key (§9).

Only the owner does this, once, on a trusted machine. The key signs every
update forever; the public half is compiled into every build.

```bash
mkdir -p ~/.coverly-release && chmod 700 ~/.coverly-release
pnpm tauri signer generate -w ~/.coverly-release/updater.key   # choose a strong password
cat ~/.coverly-release/updater.key.pub                          # the public key (one base64 line)
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.coverly-release/updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD               # paste the password
```

Then put the public key in the one place the code reads it:

1. Open `crates/coverly-core/updater.pub` and replace the `UNSET` line with the
   single line printed by `cat ~/.coverly-release/updater.key.pub`. Keep the
   `#` comment lines above it.
2. Check it: `python3 -I scripts/release/release_tools.py pubkey --file crates/coverly-core/updater.pub`
   prints the key (it fails while the file still says `UNSET`), and
   `cargo test --workspace` stays green.
3. Commit it: `git commit -m "chore(release): add the updater public key" crates/coverly-core/updater.pub`.

Rules:

- **Back up** `updater.key` and its password offline (a password manager plus
  an encrypted copy elsewhere). Losing either strands every installed copy: it
  can never be updated again and every user has to reinstall by hand (§9).
- **Never commit the private key**, and never paste it anywhere but the
  `gh secret set` above.
- `src-tauri/tauri.conf.json` → `plugins.updater.pubkey` **stays the marker text**
  `set at runtime from crates/coverly-core/updater.pub`. The app passes the key
  from `updater.pub` to the updater at run time; release builds put a throwaway
  key there through a config overlay (§7). Do not paste the real key into
  `tauri.conf.json`.
- Until the real key is committed, every build says "this build cannot update
  itself" (`update-no-key`) and `coverly update install` exits 6. That is the
  intended state of a development checkout.

## 3. One-time: make the repository public

**Status: done.** `MAECLY/coverly-roms` is public, with Pages served from
`coverly-roms.maecly.com`.

Settings → General → Danger zone → **Change visibility** → Public.

It is required, not cosmetic:

- the updater downloads `latest.json` and the artifacts **anonymously**, which
  only works for a public repository's release assets;
- the Linux ARM64 jobs use the `ubuntu-22.04-arm` runner, free for public
  repositories;
- the `guard` job refuses to build from a private repository.

Do it after the key of §2 is committed and before the first tag.

## 4. Cutting a release

From an up-to-date `main` with a clean tree:

```bash
scripts/bump-version.sh 0.1.0            # or 0.2.0-beta.1 for the beta channel
```

It sets the version in `Cargo.toml` (the only source), `package.json` and
`Cargo.lock`, and prints the next commands. Then:

1. In `CHANGELOG.md`, move the `## [Unreleased]` notes under a new heading
   `## [0.1.0] - YYYY-MM-DD` (today's date) and leave an empty `[Unreleased]`
   above it. The `guard` job refuses a tag without this heading.
2. `make check` — every gate, as CI runs them.
3. Commit, tag and push:

```bash
git commit -am "chore(release): 0.1.0"
git tag v0.1.0
git push origin main v0.1.0
```

The tag must equal the version exactly, with a leading `v`. A version with a
pre-release suffix (`-beta.1`) becomes a GitHub prerelease and only reaches the
beta channel; one without is stable.

## 5. Checking the draft, then publishing

When `release.yml` is green, open the draft on the Releases page and check:

- every platform file is there: `Coverly.ROMs_<version>_universal.dmg`, the
  macOS `.app.tar.gz`, the Windows `-setup.exe` and (stable only) `.msi`, the Linux
  `.AppImage` and `.deb` (x64 and ARM64), the five `coverly-<version>-<os>-<arch>`
  CLI binaries, and the `.sig` of every update artifact;
- `SHA256SUMS` is there and lists the `.dmg`;
- `latest.json` has the right `version` and **10 platform keys**: the five app
  keys `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64`, `linux-x86_64`,
  `linux-aarch64`, and the five CLI keys `cli-darwin-aarch64`,
  `cli-darwin-x86_64`, `cli-windows-x86_64`, `cli-linux-x86_64`,
  `cli-linux-aarch64`, plus bundle-specific keys (`darwin-aarch64-app`,
  `windows-x86_64-nsis`, `windows-x86_64-msi` (stable only), `linux-x86_64-appimage`,
  `linux-x86_64-deb`, …). `windows-x86_64` points at the NSIS `-setup.exe`,
  `darwin-aarch64` and `darwin-x86_64` at the universal `.app.tar.gz`, the
  Linux keys at the `.AppImage`. `finalize` already failed if one was
  missing;
- the notes read well in both languages.

Then click **Publish release**. Afterwards:

```bash
curl -sL https://github.com/MAECLY/coverly-roms/releases/latest/download/latest.json | jq .version
```

prints the new version for a stable release. Then try a real update:

- install the **previous** version of the app, open Settings → About →
  **Check for updates** → **Download and install** → **Restart now**, and see
  the new version in About;
- with the previous CLI: `coverly update check` says the new version is
  available, and `coverly update install --dry-run` downloads and verifies it
  without replacing the binary; then `coverly update install`.

The landing page follows on its own, two ways:

- **At once, in the browser.** `site/assets/release.js` asks
  `api.github.com/repos/MAECLY/coverly-roms/releases/latest` and, when every
  file the page links is in the newer release, swaps the links, sizes, version
  and date. Before the first deploy with a release, it swaps the "first
  release on its way" wording for the released one, shows the first-open
  steps and offers the installer for the visitor's system (the universal
  `.dmg`, the x64 `-setup.exe` or the x64/ARM64 AppImage) next to the release
  page. Open <https://coverly-roms.maecly.com/#descargar> in a private window:
  the version is the new one, and the download names it.
- **For good, a minute later.** Publishing fires `pages.yml`, whose `redeploy`
  job runs it again on `main`; `apply_release.py` bakes the release into the
  page (so it also works without JavaScript and for search engines). Check
  that the run is green and that `view-source:` shows the new version.

Last, try the first-open steps on a fresh download, as the landing, the README
and the notes describe them: on macOS, `xattr -rd com.apple.quarantine
"/Applications/Coverly ROMs.app"` lets the app open (and Open Anyway works
without it); on Windows, SmartScreen's **More info → Run anyway**; on Linux,
the AppImage after `chmod +x` and `sudo apt install ./<file>.deb`.

## 6. Beta channel

- A beta is a version with a pre-release suffix (`0.2.0-beta.1`), cut exactly
  as in §4. Its draft is a GitHub prerelease, so publishing it does not move
  the stable manifest.
- A beta has **no Windows `.msi`**: the WiX bundler accepts only a numeric
  prerelease (`1.0.0-1`), not `1.0.0-beta.1`, so on a prerelease tag the
  Windows `app` job builds the NSIS installer alone (`--bundles nsis`) and
  `finalize` does not ask for an `.msi` in `SHA256SUMS`. Windows beta users
  install and update through the NSIS `-setup.exe` (`windows-x86_64`).
- On every publish, stable or beta, `release-channels.yml` copies that
  release's `latest.json` to the fixed prerelease tagged **`channel-beta`**
  (creating it the first time). The beta channel reads
  `releases/download/channel-beta/latest.json`.
- The pointer only moves **forward** (SemVer precedence): `0.2.0` replaces
  `0.2.0-beta.3`, so beta users move on to the stable release; a stable hotfix
  `0.1.5` published while beta is on `0.2.0-beta.3` leaves beta where it is.
  If the current `channel-beta` manifest has no version or one that is not
  SemVer, it cannot be compared: the pointer moves, with a warning in the run.
- Never delete or edit `channel-beta` by hand, and never download Coverly ROMs
  from it: it only carries the manifest.

## 7. Local builds

`bundle.createUpdaterArtifacts` is `true` in `src-tauri/tauri.conf.json`, so a
plain `pnpm tauri build` needs a signing key, and `tauri build` also decodes
`plugins.updater.pubkey`, which is only a marker in the repository. Two ways:

- **Unsigned bundle, no update artifacts** (what `make build` runs):

  ```bash
  pnpm tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
  ```

- **With update artifacts, signed with a throwaway key** (what the release
  `app` job does before handing the files to `finalize`). The overlay is required, otherwise the build fails with
  `failed to decode pubkey`:

  ```bash
  TMP=$(mktemp -d)
  pnpm tauri signer generate --ci -p local -w $TMP/k
  printf '{"plugins":{"updater":{"pubkey":"%s"}}}\n' "$(tr -d '\r\n' < $TMP/k.pub)" > $TMP/overlay.json
  TAURI_SIGNING_PRIVATE_KEY="$(cat $TMP/k)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=local \
    pnpm tauri build --config $TMP/overlay.json
  ```

  The overlay only satisfies the build: the running app still trusts the key
  in `crates/coverly-core/updater.pub`. Delete `$TMP` afterwards; never commit
  a throwaway key.

## 8. Testing updates locally

No test or local check talks to GitHub. To exercise the real update path
against a manifest you serve yourself:

- `COVERLY_UPDATE_ENDPOINT=http://127.0.0.1:PORT/latest.json` replaces the
  channel's manifest URL for the app and the CLI. It must be `https://` with
  a host, or plain `http://` to a loopback host (`localhost`, `127.0.0.1`,
  `[::1]`); the app takes `http://` in debug builds only (a release build
  refuses a non-`https` endpoint). It changes where the
  manifest comes from, never which key is trusted.
- `COVERLY_UPDATE_PUBKEY=<base64 public key>` makes a **debug** CLI build trust
  that key instead of `updater.pub`. Release builds ignore it. The app has no
  such override: to try the app's install path, use a scratch checkout whose
  `updater.pub` holds a throwaway key, and never commit it.
- `scripts/release/release_tools.py add-cli --manifest latest.json --dir DIR --version V --base-url http://127.0.0.1:PORT`
  builds a manifest from `coverly-<V>-<os>-<arch>` files and their `.sig`
  files, exactly as `finalize` does.

The recipe (throwaway key, a fake `coverly 9.9.9`, `python3 -m http.server`,
`coverly update check`, a tampered artifact that must exit 6, then a real
`coverly update install`) is Task 12 Step 4 of
[the M7 plan](docs/superpowers/plans/2026-10-16-m7-release.md). The UI states
can be seen without any server in `pnpm dev` with
`?mockUpdate=available|up-to-date|offline|bad-signature|no-key`.

## 9. If something goes wrong

| Problem                                                                                                         | What to do                                                                                                                                                                                                                                                                                                                                         |
| --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **The private key or its password is lost**                                                                     | Installed copies can no longer be updated. Generate a new key (§2), commit the new `updater.pub`, release a new version and announce, in the release notes and the README, that everyone must download it by hand once.                                                                                                                            |
| **The private key leaks**                                                                                       | Delete the two secrets at once (`gh secret delete TAURI_SIGNING_PRIVATE_KEY`, `gh secret delete TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). The leaked key can sign updates that old installs accept, and it cannot be revoked: rotate as for a lost key, publish the new version quickly and tell users to reinstall from the Releases page.            |
| **A published release is bad**                                                                                  | Never delete and re-publish the same tag: copies that already updated keep it, and the updater only moves to higher versions. Fix it and ship a higher version (`0.1.1`). You may mark the bad release as a prerelease or edit its notes to warn.                                                                                                  |
| `guard`: `TAURI_SIGNING_PRIVATE_KEY is not set`                                                                 | Do §2.                                                                                                                                                                                                                                                                                                                                             |
| `guard`: `the repository is private`                                                                            | Do §3.                                                                                                                                                                                                                                                                                                                                             |
| `guard`: `release_tools.py pubkey` fails                                                                        | `updater.pub` still says `UNSET`: §2 step 1.                                                                                                                                                                                                                                                                                                       |
| `guard`: `tag X does not match Cargo.toml Y` or `bump-version.sh --check` fails                                 | Delete the tag (`git tag -d vX && git push origin :refs/tags/vX`), run `scripts/bump-version.sh X`, commit, tag again.                                                                                                                                                                                                                             |
| `guard`: `CHANGELOG.md has no ## [X] section`                                                                   | Add the heading (§4 step 1), commit, move the tag.                                                                                                                                                                                                                                                                                                 |
| `draft`: `vX is already published`                                                                              | The tag was published before. Ship a higher version.                                                                                                                                                                                                                                                                                               |
| An `app` or `cli` job fails (runner, network)                                                                   | Re-run the failed jobs from the Actions page. They only build and hand over workflow artifacts, so a re-run touches nothing on the draft; `finalize` runs after all of them succeed.                                                                                                                                                               |
| The Windows `app` job fails: `pre-release identifier in app version must be numeric-only`                       | WiX (the `.msi`) was asked to build a prerelease. A prerelease tag must build with `--bundles nsis` (§6); check the `Build the bundles` step of `release.yml`.                                                                                                                                                                                     |
| `finalize`: `stage-builds` says `unexpected build artifact`, `unexpected file`, `is missing` or `more than one` | The run's workflow artifacts are not exactly the `app-<platform>` and `cli-<os>-<arch>` ones, each with its own platform's files for this version. A renamed bundle or a changed staging step: fix `release.yml` or `build_artifacts` in `release_tools.py`. An artifact nobody added on purpose: treat the run as compromised and do not publish. |
| `finalize`: `a build handed over signature files`                                                               | A build staged a `.sig` (the throwaway one) or a `cli-*` artifact carries one. Fix the staging step in `release.yml`; never upload throwaway signatures.                                                                                                                                                                                           |
| `finalize`: `updater-manifest` fails (`no .sig`, `both … and …`, `no architecture`)                             | The bundles of the `app` artifacts do not map to one file per platform key: a missing or renamed bundle, two files for one key, or a name without `x64`/`amd64`/`aarch64`/`universal`. Look at the `ls -l staged` output of the `app` jobs, fix `release.yml` or `release_tools.py`, and re-run the workflow.                                      |
| `finalize`: `check-manifest` says `latest.json has no <key> entry`                                              | An `app` job produced no update artifact for that platform (for example `createUpdaterArtifacts` turned off). Fix it and re-run the workflow; `finalize` uploads nothing until all its checks pass, so the draft is never half-signed.                                                                                                             |
| `finalize`: `SHA256SUMS has no … file`                                                                          | A bundle type is missing from the `app` artifacts (a `bundle.targets` change?). Restore it, or update the check in `release.yml` on purpose.                                                                                                                                                                                                       |
| `finalize`: the upload to the draft fails half-way                                                              | Re-run `finalize` only: it signs and uploads again with `--clobber`, replacing every asset of the same name. The workflow artifacts are kept 7 days.                                                                                                                                                                                               |
| `finalize`: `minisign -V` fails                                                                                 | The secret does not match the committed `updater.pub`. Check that both come from the same `generate` run; never "fix" it by committing another public key unless you are rotating on purpose.                                                                                                                                                      |
| `release-channels.yml` fails                                                                                    | Publishing already happened (stable users are served). Fix the cause and re-run the workflow; it is idempotent and only moves `channel-beta` forward.                                                                                                                                                                                              |
| `release-channels.yml` warns `the beta manifest's version … is missing or not SemVer`                           | The `channel-beta` manifest was broken (edited by hand?), so it could not be compared; the workflow replaced it with the release just published. Check `releases/download/channel-beta/latest.json` and never edit it by hand.                                                                                                                     |
