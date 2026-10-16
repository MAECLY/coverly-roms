# Updates

> **TL;DR.** One `latest.json` per channel on GitHub Releases serves both the app (five `<os>-<arch>` keys) and the CLI (five `cli-<os>-<arch>` keys). Every artifact is signed with the project's minisign key, bound to its version. The only trusted public key lives in `crates/coverly-core/updater.pub` (`UNSET` until the owner commits it). The app drives `tauri-plugin-updater` from Rust; the webview only calls four commands. An update never interrupts a fill job, and with "check on start" off nothing is requested at startup. Decisions: [ADR-0008](../adr/0008-updater-public-releases.md) and [ADR-0011](../adr/0011-release-pipeline-and-update-key.md); release procedure: [RELEASING.md](../../RELEASING.md).

## Components

```
                       crates/coverly-core/updater.pub   (the ONE public key; UNSET placeholder)
                                       │ include_str!
                                       ▼
 coverly-core::update ── endpoints (stable / beta / COVERLY_UPDATE_ENDPOINT), release_pubkey(),
   (mod.rs, verify.rs)   Manifest + parse_manifest (≤ 1 MiB), SemVer is_newer, check(),
                         download_verified (≤ 256 MiB, minisign + signed version), cli_platform_key()
         │                                                │
         ▼                                                ▼
 coverly-cli update.rs                          src-tauri/src/update.rs
   `coverly update check|install`                 UpdateManager (policy: phases, startup gate, job gate)
   download_verified → replace_executable          └─ UpdateDriver trait
   (atomic; old binary kept on failure)               ├─ PluginDriver → tauri-plugin-updater (pubkey set at run time,
                                                      │                  30 s check / 20 min download bounds)
                                                      └─ fake driver (unit tests)
                                                  src-tauri/src/ipc.rs: check_update, download_update,
                                                    install_update, restart_app; event `update-progress`
                                                               │ IPC (no `updater:` permission in the webview)
                                                               ▼
                                                  src/stores/updateStore.ts (status machine, job wait)
                                                  src/components/update/UpdatePanel.tsx (Settings → About,
                                                    "Check now" in General), UpdateBanner.tsx (startup banner)
                                                  src/ipc/mock/mockUpdates.ts (browser scenarios)
```

| Piece | File | Responsibility |
|---|---|---|
| Key file | `crates/coverly-core/updater.pub` | `#` comments + one base64 key line, or `UNSET`. `release_pubkey()` returns `None` while it is `UNSET`. |
| Core | `crates/coverly-core/src/update/mod.rs` | `STABLE_ENDPOINT`, `BETA_ENDPOINT` (`BETA_TAG = channel-beta`), `resolve_endpoint` (override → channel), manifest parsing, SemVer comparison, `check`, `download_verified`, `cli_platform_key`. |
| Verification | `crates/coverly-core/src/update/verify.rs` | minisign verification in Tauri's scheme (base64 of the minisign text) and the `version:` field of the trusted comment, which must equal the announced version. |
| Test keys | `crates/coverly-core/src/update/testing.rs` | Throwaway keys and signing for tests (feature `test-util`); never a committed key. |
| CLI | `crates/coverly-cli/src/update.rs` | `coverly update check [--channel] [--json]`, `coverly update install [--channel] [--json] [--dry-run]`, `replace_executable`. |
| App | `src-tauri/src/update.rs` | `UpdateManager`, `UpdateDriver`, `PluginDriver`, `classify` (plugin errors → IPC codes), `job_blocks_update`, `offer_view` (notes cut at 64 KiB). |
| UI | `src/stores/updateStore.ts`, `src/components/update/*` | Status `idle → checking → up-to-date / available → downloading → waiting-for-job → installing → ready` (or `error`); startup banner; About panel. |
| Mock | `src/ipc/mock/mockUpdates.ts` | `?mockUpdate=available|up-to-date|offline|bad-signature|no-key` (or localStorage `coverly.mock.update`); default `up-to-date`. |

## Check → download → install → restart

```
UI (updateStore)              Rust (UpdateManager)                    Network
────────────────              ────────────────────                    ───────
startupCheck(enabled)
  enabled = false ──────────► (nothing is called, nothing requested)
  enabled = true
check('startup'|'manual') ──► check_update(trigger)
                                trigger=startup and setting off → skipped
                                no key (UNSET)      → update-no-key
                                endpoint = override ?? channel
                                driver.check(endpoint, pubkey) ─────► GET latest.json
                              ◄─ up-to-date | available{offer}
  startup error → idle (silent)
  manual error  → error + message
download() ─────────────────► download_update
                                phase must be Available ────────────► GET artifact (url of <os>-<arch>)
                                emits update-progress (~10/s)          verify minisign + signed version
                              ◄─ ok | update-signature | update-offline
install()
  job running or paused? ───► (the store waits: waiting-for-job, then retries)
                              install_update
                                phase must be Downloaded
                                job running or paused → job-running (the store waits)
                              ◄─ { restart_required }   (Windows: the installer ends the app)
restart() (button disabled
  while a job runs) ────────► restart_app
                                phase must be Installed; job → job-running
                                app.restart()
```

Rules the tests pin:

- **Startup.** The startup check runs at most once per page and only with Settings → General → "Check for updates when the app starts" on; off means no request at all (`skipped`). Its failures are silent (status back to `idle`, no banner); a manual check shows the error.
- **Fill jobs.** A job that is gathering, applying or **paused** (offline, rate limit, volume gone) holds the install and the restart (`job_blocks_update` in Rust, `blocksUpdate` in the store). The download may proceed; the install waits.
- **Phases.** `download` needs `Available`, `install` needs `Downloaded`, `restart_app` needs `Installed`; otherwise `update-not-ready`. A new check while an update is downloading or installed is refused, so it cannot drop it.
- **Replay.** A manifest that pairs a newer `version` with an older release's genuine artifact is refused: the signature's `version:` must equal the announced version (`requireSignedVersion: true` in the plugin; the same check in the core for the CLI).
- **Hostile manifests.** Non-JSON, over 1 MiB, a non-SemVer version or a missing platform key give "manifest not valid" / "no update for this platform", never a crash or a false "up to date".
- **CLI self-replace.** The real file behind any symlink is replaced (the link stays a link). The new binary is downloaded with a total timeout of at least 5 minutes (`artifact_net_config`), written next to the old one (`.coverly.new-<pid>`), fsynced, made executable and renamed over it (Windows: the running file is moved to `coverly.old.exe` first and moved back on failure; a later `coverly update` deletes the stale `coverly.old.exe`). If anything fails the old binary is intact and no temporary file is left.

## The manifest

`release.yml`'s `finalize` job writes it (the notes are the version's `CHANGELOG.md` section); `release-channels.yml` copies it to `channel-beta`. Example (signatures shortened, bundle-specific keys left out):

```json
{
  "version": "0.1.0",
  "notes": "### Added\n\n- …",
  "pub_date": "2026-11-02T10:00:00Z",
  "platforms": {
    "darwin-aarch64": { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/Coverly.ROMs_0.1.0_universal.app.tar.gz", "signature": "dW50cnVzdGVk…" },
    "darwin-x86_64":  { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/Coverly.ROMs_0.1.0_universal.app.tar.gz", "signature": "dW50cnVzdGVk…" },
    "windows-x86_64": { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/Coverly.ROMs_0.1.0_x64-setup.exe", "signature": "dW50cnVzdGVk…" },
    "linux-x86_64":   { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/Coverly.ROMs_0.1.0_amd64.AppImage", "signature": "dW50cnVzdGVk…" },
    "linux-aarch64":  { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/Coverly.ROMs_0.1.0_aarch64.AppImage", "signature": "dW50cnVzdGVk…" },
    "cli-darwin-aarch64":  { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/coverly-0.1.0-darwin-aarch64", "signature": "dW50cnVzdGVk…" },
    "cli-darwin-x86_64":   { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/coverly-0.1.0-darwin-x86_64", "signature": "dW50cnVzdGVk…" },
    "cli-windows-x86_64":  { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/coverly-0.1.0-windows-x86_64.exe", "signature": "dW50cnVzdGVk…" },
    "cli-linux-x86_64":    { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/coverly-0.1.0-linux-x86_64", "signature": "dW50cnVzdGVk…" },
    "cli-linux-aarch64":   { "url": "https://github.com/MAECLY/coverly-roms/releases/download/v0.1.0/coverly-0.1.0-linux-aarch64", "signature": "dW50cnVzdGVk…" }
  }
}
```

`finalize` writes it with `release_tools.py updater-manifest` from the bundles the read-only `app` jobs handed over: `darwin-aarch64` and `darwin-x86_64` → the universal `.app.tar.gz`, `windows-x86_64` → the NSIS `-setup.exe` (the `.msi` only if there were no NSIS installer, tauri-action's `updaterJsonPreferNsis`), `linux-<arch>` → the `.AppImage`; it also writes bundle-specific keys (`darwin-aarch64-app`, `windows-x86_64-nsis`, `windows-x86_64-msi`, `linux-x86_64-appimage`, `linux-x86_64-deb`, …), which the updater tries first for the bundle a copy was installed from. Each `signature` is the base64 of a minisign signature file whose trusted comment carries `version:0.1.0` (`tauri signer sign --app-version`). The app reads `<os>-<arch>`; the CLI reads `cli-<os>-<arch>` (`cli_platform_key()`: `macos` → `darwin`; arch `x86_64` or `aarch64`, anything else has no key).

| Channel | URL |
|---|---|
| stable | `https://github.com/MAECLY/coverly-roms/releases/latest/download/latest.json` |
| beta | `https://github.com/MAECLY/coverly-roms/releases/download/channel-beta/latest.json` |

## Errors

App (IPC codes, shown as `errors.<code>` in en and es):

| Code | When |
|---|---|
| `update-offline` | The manifest or artifact could not be fetched (network, timeout), or a 200 answer was not JSON. |
| `update-signature` | The artifact failed minisign, its signature is malformed, or it is bound to another version (or to none). Nothing is installed. |
| `update-unavailable` | No manifest for this channel (the plugin reports any non-2xx, 404 and 5xx alike, this way) or no entry for this system. |
| `update-no-key` | `updater.pub` is `UNSET`: this build cannot update itself; the UI links to the Releases page. |
| `update-failed` | Anything else (installer error, invalid endpoint, a check while an update is on its way); `detail` says what. |
| `update-not-ready` | A step out of order (download before check, install before download, restart before install). |
| `job-running` | Install or restart while a fill job is running or paused. |

CLI exit codes: `0` up to date / updated / dry run verified; `6` signature failure or no key (`UNSET`), and the binary is unchanged; `1` anything else (offline, manifest not valid, no update for this platform, the folder is not writable); `2` bad usage. `--json` on `check` prints `{ current, latest, available, notes }`.

## Environment variables

| Variable | Read by | Effect |
|---|---|---|
| `COVERLY_UPDATE_ENDPOINT` | app and CLI | Replaces the channel's manifest URL (local tests). It must be `https://` with a host, or `http://` to a loopback host (`localhost`, `127.0.0.0/8`, `[::1]`); the app also refuses `http://` in release builds (the plugin). The key is unchanged. |
| `COVERLY_UPDATE_PUBKEY` | CLI, **debug builds only** | Trust this base64 public key instead of `updater.pub` (tests and the local recipe in RELEASING.md §8). |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | `tauri build`, `tauri signer sign` | Signing. In CI: a throwaway key in `app`, the real secrets only in `finalize`. Never read by the app or the CLI. |

## Verified locally (M7, 2026-10-16)

Checked on macOS arm64 with Tauri CLI 2.12 and a throwaway key (evidence in `docs/TRACKER.md`, «Evidencia M7»):

- `tauri build` with `createUpdaterArtifacts: true` and the signing key set fails with `failed to decode pubkey` while `plugins.updater.pubkey` is the marker; with the throwaway `.pub` overlaid through `--config` it writes `Coverly ROMs.app.tar.gz` and its `.sig`. `make build`'s form (`createUpdaterArtifacts: false`) needs no key.
- The `.sig` that `tauri build` writes already carries `version:<app version>` in its trusted comment. The `finalize` job re-signs with `--app-version` anyway, so the real signature never depends on that.
- `coverly update check|install` against a local `latest.json` written by `release_tools.py add-cli`: a tampered artifact exits 6 and leaves the binary byte-for-byte unchanged; a good one replaces it.

## Connected drives on the Start screen (M3-06)

Not part of the updater, but shipped in M7: `coverly_core::volumes::mounted_volumes()` (IPC `mounted_volumes`, `Backend.mountedVolumes()`) lists the drives connected now so the Start screen can open one as a library. It only lists folders, never opens, writes or mounts anything: on macOS the children of `/Volumes`, skipping the boot disk (it resolves to `/`) and the system's own mounts (`is_system_mount`: hidden names, `com.apple.*` such as Time Machine's local snapshots, `Recovery`, `Preboot`, `VM`, `Update`, `xarts`, `iSCPreboot`, `Hardware`, and anything that resolves under `/System/Volumes`); on Linux `/media/$USER`, `/run/media/$USER` and `/media`; on Windows every existing `D:\`…`Z:\` except the system drive. Entries are de-duplicated by their real path and sorted by name, ignoring case. The browser mock lists one drive, `/mock/Small library`.
