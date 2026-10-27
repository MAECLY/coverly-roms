#!/usr/bin/env python3
"""Helpers for .github/workflows/release.yml and release-channels.yml.

Plain standard-library Python (3.9 or newer), so the runners need nothing
installed, and every rule the workflows depend on is a function that
src-tauri/tests/release_pipeline.rs exercises on a developer machine.

Subcommands:

  semver-gt CANDIDATE CURRENT
      Exit 0 when CANDIDATE is a higher SemVer 2.0 precedence than CURRENT,
      1 otherwise (equal included). A leading "v" is ignored, and so is build
      metadata (SemVer section 10). Used to move the beta channel's manifest
      forward only.

  check-manifest --manifest F --version V [--require KEY ...]
                 [--signed-version V]
      Fail unless the manifest names version V and every required platform
      with a url and a signature. With --signed-version, every required
      platform's signature must also be bound to that version: its trusted
      comment carries the `version:V` field `tauri signer sign --app-version`
      writes, which the app's updater (`requireSignedVersion`) and
      `coverly update install` insist on.

  semver-check VERSION
      Exit 0 when VERSION is SemVer 2.0 (a leading "v" allowed), 2 otherwise.

  updater-artifacts --dir D
      Print the update artifacts among the bundles in D, one per line: the
      macOS .app.tar.gz, the Windows NSIS -setup.exe and .msi, the Linux
      .AppImage and .deb. These are the files the finalize job signs for
      latest.json; the .dmg is only downloaded by people.

  updater-manifest --dir D --version V --base-url URL --output F
                   [--changelog CHANGELOG.md] [--pub-date DATE]
      Write latest.json for the update artifacts in D, each with the
      signature in D/<file>.sig and the url <URL>/<file>. Platform keys:
      darwin-aarch64 and darwin-x86_64 -> the universal .app.tar.gz;
      windows-x86_64 -> the NSIS -setup.exe (the .msi only when there is no
      NSIS installer, as tauri-action's updaterJsonPreferNsis);
      linux-x86_64 / linux-aarch64 -> the .AppImage of that architecture;
      plus a bundle-specific key for each (darwin-aarch64-app,
      windows-x86_64-msi, windows-x86_64-nsis, linux-x86_64-appimage,
      linux-x86_64-deb, ...),
      which the updater prefers for the bundle a copy was installed from.
      The notes are CHANGELOG.md's `## [V]` section; pub_date is now (UTC)
      unless given. Fails (and writes nothing) on a missing .sig, two
      artifacts for one key or a name without an architecture.

  list-artifacts --manifest F
      Print the file name of every update artifact the manifest points at,
      one per line, each once (the last path segment of its url, decoded).
      Refuses a name that is a path. Kept for recovery: re-signing a
      manifest written elsewhere.

  resign-manifest --manifest F --sigdir D
      Replace every platform's signature with the contents of
      D/<artifact file name>.sig. Fails (and writes nothing) when a .sig file
      is missing. Kept for recovery, like list-artifacts.

  add-cli --manifest F --dir D --version V --base-url URL
      Add a `cli-<os>-<arch>` entry for every file
      `coverly-<V>-<os>-<arch>[.exe]` in D, with the signature in
      D/<file>.sig and the url <URL>/<file>. Creates the manifest when F does
      not exist. Fails on an unexpected file, a missing .sig or no binary.

  stage-builds --from DL --bundles B --cli C --version V
      Check the build artifacts finalize downloaded (DL/<artifact>/<file>,
      never merged) and copy their files into B (app bundles) and C (CLI
      binaries) under their release names: Coverly-ROMs_<V>_<platform><kind>
      and coverly_<V>_<platform>[.exe], platform being one of mac_arm,
      mac_intel, windows_x64, windows_arm, linux_x64, linux_arm. DL must
      hold exactly the artifacts the app and cli jobs upload, and each one
      exactly its own platform's files for version V (the Windows .msi only
      for a stable V: WiX refuses a non-numeric prerelease). Anything else
      (an extra artifact, another platform's file, a .sig, a symlink, a
      missing or second file of one kind, a release name GitHub would
      rewrite) fails, and nothing is copied: a build cannot slip a file
      into, or replace, another platform's bundles.

  pubkey --file crates/coverly-core/updater.pub
      Print the updater public key committed in the file (its first line that
      is neither blank nor a `#` comment), or fail while it is the
      placeholder UNSET. A UTF-8 BOM is ignored.

Every failure is one `::error::` line per problem on stderr and exit 1 (a
GitHub Actions annotation), never a traceback.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import datetime
import json
import os
import re
import shutil
import sys
import urllib.parse

SEMVER = re.compile(
    r"v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)

PUBKEY_PLACEHOLDER = "UNSET"


def semver_key(version: str):
    """A sort key with SemVer 2.0 precedence. Raises ValueError otherwise."""
    m = SEMVER.fullmatch(version.strip())
    if not m:
        raise ValueError(f"not a SemVer version: {version!r}")
    core = tuple(int(x) for x in m.group(1, 2, 3))
    pre = m.group(4)
    if pre is None:
        # A release ranks above every prerelease of the same core.
        return core, (1,)
    ids = []
    for part in pre.split("."):
        # Numeric identifiers rank below alphanumeric ones (section 11.4.3).
        ids.append((0, int(part), "") if part.isdigit() else (1, 0, part))
    return core, (0, tuple(ids))


def semver_gt(candidate: str, current: str) -> bool:
    return semver_key(candidate) > semver_key(current)


def signed_version(signature: str) -> str | None:
    """The version a manifest signature (base64 of the .sig file) is bound to.

    The Tauri CLI writes the trusted comment as tab-separated `key:value`
    fields, e.g. `timestamp:1700000000<TAB>file:x.exe<TAB>version:1.2.3`.
    This only reads it: minisign (the finalize job, the app and the CLI) is
    what proves the comment is covered by the signature.
    """
    try:
        text = base64.b64decode(signature, validate=True).decode("utf-8")
    except (binascii.Error, UnicodeDecodeError):
        return None
    prefix = "trusted comment: "
    for line in text.splitlines():
        if line.startswith(prefix):
            for field in line[len(prefix):].split("\t"):
                if field.startswith("version:"):
                    return field[len("version:"):]
    return None


def check_manifest(manifest: dict, version: str, required: list[str],
                   signed: str | None = None) -> list[str]:
    errors = []
    if str(manifest.get("version", "")).lstrip("v") != version.lstrip("v"):
        errors.append(
            f"latest.json is for {manifest.get('version')!r}, not {version!r}"
        )
    platforms = manifest.get("platforms", {})
    for key in required:
        entry = platforms.get(key)
        if not entry:
            errors.append(f"latest.json has no {key} entry")
        elif not entry.get("url") or not entry.get("signature"):
            errors.append(f"latest.json's {key} entry lacks a url or a signature")
        elif signed is not None:
            got = signed_version(entry["signature"])
            if got is None:
                errors.append(
                    f"latest.json's {key} signature is not bound to a version; "
                    "sign with `tauri signer sign --app-version`, or the app's "
                    "updater (requireSignedVersion) refuses it"
                )
            elif got.lstrip("v") != signed.lstrip("v"):
                errors.append(
                    f"latest.json's {key} signature is bound to {got!r}, not {signed!r}"
                )
    return errors


class ManifestError(ValueError):
    """A manifest, file name or url this tool refuses (reported as ::error::)."""


def check_file_name(name: str) -> str:
    """`name` when it is a plain file name: never empty, never a path."""
    if (not name or name in (".", "..") or "/" in name or "\\" in name
            or ".." in name or "\0" in name):
        raise ManifestError(f"{name!r} is not a plain file name")
    return name


def asset_name(url: str) -> str:
    """The file name a release asset url points at (last segment, decoded).

    Refuses anything that would leave the folder it is joined to: a `/`, a
    `\\` or `..` (also once decoded, e.g. `%2F` or `..%2F`) or an empty name.
    """
    if not isinstance(url, str):
        raise ManifestError(f"url {url!r} is not a string")
    return check_file_name(urllib.parse.unquote(url.rstrip("/").rsplit("/", 1)[-1]))


def list_artifacts(manifest: dict) -> list[str]:
    seen = []
    for entry in manifest.get("platforms", {}).values():
        name = asset_name(entry["url"])
        if name not in seen:
            seen.append(name)
    return seen


def resign_manifest(manifest: dict, sigdir: str) -> list[str]:
    errors = []
    for key, entry in manifest.get("platforms", {}).items():
        path = os.path.join(sigdir, asset_name(entry["url"]) + ".sig")
        if not os.path.isfile(path):
            errors.append(f"{key}: no {path}")
            continue
        with open(path, encoding="utf-8") as f:
            signature = f.read().strip()
        if not signature:
            errors.append(f"{key}: {path} is empty")
            continue
        entry["signature"] = signature
    return errors


CLI_FILE = re.compile(
    r"coverly-(?P<v>[^-]+(?:-[0-9A-Za-z.-]+)?)-(?P<os>darwin|windows|linux)"
    r"-(?P<arch>x86_64|aarch64)(?:\.exe)?"
)


def add_cli(manifest: dict, directory: str, version: str, base_url: str) -> list[str]:
    errors, added = [], 0
    platforms = manifest.setdefault("platforms", {})
    for name in sorted(os.listdir(directory)):
        if name.endswith(".sig"):
            continue
        m = CLI_FILE.fullmatch(name)
        if not m or m.group("v") != version:
            errors.append(f"unexpected file {name}")
            continue
        sig = os.path.join(directory, name + ".sig")
        if not os.path.isfile(sig):
            errors.append(f"{name} has no .sig")
            continue
        with open(sig, encoding="utf-8") as f:
            platforms[f"cli-{m.group('os')}-{m.group('arch')}"] = {
                "url": f"{base_url.rstrip('/')}/{urllib.parse.quote(name)}",
                "signature": f.read().strip(),
            }
        added += 1
    if added == 0:
        errors.append(f"no CLI binaries in {directory}")
    return errors


# Release name suffix -> (os, arch) as the manifest keys spell them. The only
# place a platform is listed; every other table derives from it (spec §4).
RELEASE_PLATFORMS = {
    "mac_arm": ("darwin", "aarch64"),
    "mac_intel": ("darwin", "x86_64"),
    "windows_x64": ("windows", "x86_64"),
    "windows_arm": ("windows", "aarch64"),
    "linux_x64": ("linux", "x86_64"),
    "linux_arm": ("linux", "aarch64"),
}

# The architecture token Tauri writes into each bundle kind, per platform.
# The macOS .app.tar.gz carries the token the app job's staging step appends
# (matrix tauri_arch), the same one Tauri gives the .dmg.
TAURI_TOKENS = {
    ("darwin", "aarch64"): {".app.tar.gz": "aarch64", ".dmg": "aarch64"},
    ("darwin", "x86_64"): {".app.tar.gz": "x64", ".dmg": "x64"},
    ("windows", "x86_64"): {"-setup.exe": "x64", ".msi": "x64"},
    ("windows", "aarch64"): {"-setup.exe": "arm64", ".msi": "arm64"},
    ("linux", "x86_64"): {".AppImage": "amd64", ".deb": "amd64"},
    ("linux", "aarch64"): {".AppImage": "aarch64", ".deb": "arm64"},
}

APP_PREFIX = "Coverly-ROMs"
CLI_PREFIX = "coverly"
CLI_KIND = "CLI binary"
RELEASE_NAME = re.compile(r"[A-Za-z0-9._-]+")


def release_name(suffix: str, kind: str, version: str) -> str:
    """The name a staged file is published under (spec §4)."""
    v = version.lstrip("v")
    if kind == CLI_KIND:
        ext = ".exe" if RELEASE_PLATFORMS[suffix][0] == "windows" else ""
        return f"{CLI_PREFIX}_{v}_{suffix}{ext}"
    return f"{APP_PREFIX}_{v}_{suffix}{kind}"


# Update artifacts the app's updater installs, by file name suffix:
# (suffix, os, bundle). The .deb only serves its own bundle key (a copy
# installed from it); the .dmg is only downloaded by people, so it is
# uploaded and listed in SHA256SUMS but not in latest.json.
UPDATER_KINDS = (
    (".app.tar.gz", "darwin", "app"),
    ("-setup.exe", "windows", "nsis"),
    (".msi", "windows", "msi"),
    (".AppImage", "linux", "appimage"),
    (".deb", "linux", "deb"),
)

# The architecture token Tauri puts in a bundle name, and the manifest
# architectures it serves. A universal macOS app serves both.
ARCH_TOKENS = {
    "universal": ("aarch64", "x86_64"),
    "aarch64": ("aarch64",),
    "arm64": ("aarch64",),
    "x86_64": ("x86_64",),
    "x64": ("x86_64",),
    "amd64": ("x86_64",),
}
ARCH_RE = re.compile(r"[_.-](universal|aarch64|arm64|x86_64|x64|amd64)(?=[_.-])")


def updater_kind(name: str):
    """(os, bundle) of an update artifact, or None for any other file."""
    for suffix, os_name, bundle in UPDATER_KINDS:
        if name.endswith(suffix):
            return os_name, bundle
    return None


def updater_artifacts(names: list[str]) -> list[str]:
    """The update artifacts among `names` (no .sig), sorted."""
    return sorted(n for n in names
                  if not n.endswith(".sig") and updater_kind(n) is not None)


def updater_platforms(names: list[str]) -> tuple[dict, list[str]]:
    """Map manifest platform keys to the update artifact each one installs.

    Every artifact gets its bundle-specific key (`windows-x86_64-msi`,
    `darwin-aarch64-app`, …), which the updater tries first for the bundle a
    copy was installed from. The generic `<os>-<arch>` key goes to the only
    artifact of that platform, except on Windows, where the NSIS installer
    wins over the MSI (tauri-action's `updaterJsonPreferNsis`). A universal
    macOS app serves `darwin-aarch64` and `darwin-x86_64`.
    """
    errors: list[str] = []
    specific: dict[str, str] = {}
    for name in updater_artifacts(names):
        try:
            check_file_name(name)
        except ManifestError as e:
            errors.append(str(e))
            continue
        os_name, bundle = updater_kind(name)
        tokens = ARCH_RE.findall(name)
        if not tokens:
            errors.append(f"{name}: no architecture in the file name")
            continue
        for arch in ARCH_TOKENS[tokens[-1]]:
            key = f"{os_name}-{arch}-{bundle}"
            if key in specific:
                errors.append(f"{key}: both {specific[key]} and {name}")
                continue
            specific[key] = name
    platforms: dict[str, str] = {}
    preference = {"darwin": ("app",), "linux": ("appimage",),
                  "windows": ("nsis", "msi")}
    for key in sorted(specific):
        os_name, arch, _bundle = key.split("-", 2)
        generic = f"{os_name}-{arch}"
        if generic in platforms:
            continue
        for bundle in preference[os_name]:
            name = specific.get(f"{generic}-{bundle}")
            if name:
                platforms[generic] = name
                break
    platforms.update(specific)
    return platforms, errors


def build_artifacts(version: str) -> dict[str, tuple[str, str, dict[str, str]]]:
    """artifact name -> (destination: "bundles" or "cli", release suffix,
    {kind: pattern} of the files it must hold under Tauri's or Cargo's name,
    each pattern matched by exactly one file)."""
    plain = version.lstrip("v")
    v = re.escape(plain)
    stable = "-" not in plain
    app = r"[A-Za-z0-9._-]+_" + v + "_"
    out: dict[str, tuple[str, str, dict[str, str]]] = {}
    for suffix, (os_name, arch) in RELEASE_PLATFORMS.items():
        patterns: dict[str, str] = {}
        for kind, token in TAURI_TOKENS[(os_name, arch)].items():
            if kind == ".msi":
                # WiX refuses a non-numeric prerelease: no .msi on a beta.
                if stable:
                    patterns[kind] = app + re.escape(token) + r"_[A-Za-z0-9-]+\.msi"
            else:
                patterns[kind] = app + re.escape(token) + re.escape(kind)
        out[f"app-{os_name}-{arch}"] = ("bundles", suffix, patterns)
        ext = ".exe" if os_name == "windows" else ""
        out[f"cli-{os_name}-{arch}"] = (
            "cli", suffix, {CLI_KIND: f"coverly-{v}-{os_name}-{arch}{re.escape(ext)}"})
    return out


def stage_builds(source: str, bundles: str, cli: str, version: str) -> list[str]:
    """Check every downloaded build artifact, then copy all their files (or
    none) into `bundles` and `cli`, each under its release name."""
    expected = build_artifacts(version)
    found = sorted(os.listdir(source))
    errors = [f"unexpected build artifact {name}" for name in found
              if name not in expected]
    errors += [f"build artifact {name} is missing" for name in expected
               if name not in found]
    copies: list[tuple[str, str]] = []
    targets: dict[str, str] = {}
    for artifact, (dest, suffix, patterns) in sorted(expected.items()):
        folder = os.path.join(source, artifact)
        if not os.path.isdir(folder):
            continue
        matched: dict[str, list[str]] = {kind: [] for kind in patterns}
        for name in sorted(os.listdir(folder)):
            path = os.path.join(folder, name)
            kind = next((k for k, p in patterns.items() if re.fullmatch(p, name)), None)
            if kind is None or os.path.islink(path) or not os.path.isfile(path):
                errors.append(f"{artifact}: unexpected file {name}")
                continue
            matched[kind].append(name)
            new = release_name(suffix, kind, version)
            try:
                check_file_name(new)
            except ManifestError as e:
                errors.append(f"{artifact}: {e}")
                continue
            if not RELEASE_NAME.fullmatch(new):
                errors.append(f"{artifact}: {new!r} has a character GitHub would rewrite")
                continue
            if new in targets:
                errors.append(f"{artifact}: {name} and {targets[new]} would both be {new}")
                continue
            targets[new] = name
            copies.append((path, os.path.join(bundles if dest == "bundles" else cli, new)))
        for kind, names in matched.items():
            if not names:
                errors.append(f"{artifact}: no {kind} file for {version.lstrip('v')}")
            elif len(names) > 1:
                errors.append(f"{artifact}: more than one {kind} file: {', '.join(names)}")
    if errors:
        return errors
    if any(os.path.exists(dst) for _src, dst in copies):
        return [f"{bundles} or {cli} is not empty"]
    os.makedirs(bundles, exist_ok=True)
    os.makedirs(cli, exist_ok=True)
    for src, dst in copies:
        shutil.copyfile(src, dst)
    return []


def changelog_notes(text: str, version: str) -> str | None:
    """The body of CHANGELOG.md's `## [version]` section, or None."""
    lines = text.splitlines()
    head = re.compile(r"## \[" + re.escape(version.lstrip("v")) + r"\](\s|$)")
    for i, line in enumerate(lines):
        if head.match(line):
            body = []
            for rest in lines[i + 1:]:
                if rest.startswith("## ["):
                    break
                body.append(rest)
            return "\n".join(body).strip() or None
    return None


def updater_manifest(directory: str, version: str, base_url: str,
                     notes: str | None, pub_date: str) -> tuple[dict, list[str]]:
    """latest.json for the update artifacts in `directory`, each with the
    signature in `<file>.sig` next to it (written by the finalize job with the
    real key)."""
    platforms, errors = updater_platforms(os.listdir(directory))
    manifest = {"version": version.lstrip("v"), "notes": notes,
                "pub_date": pub_date, "platforms": {}}
    if not platforms:
        errors.append(f"no update artifacts in {directory}")
    for key, name in sorted(platforms.items()):
        sig = os.path.join(directory, name + ".sig")
        if not os.path.isfile(sig):
            errors.append(f"{key}: {name} has no .sig")
            continue
        with open(sig, encoding="utf-8") as f:
            signature = f.read().strip()
        if not signature:
            errors.append(f"{key}: {name}.sig is empty")
            continue
        manifest["platforms"][key] = {
            "url": f"{base_url.rstrip('/')}/{urllib.parse.quote(name)}",
            "signature": signature,
        }
    return manifest, errors


def now_utc() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def read_pubkey(path: str) -> str | None:
    # utf-8-sig: a key file saved by an editor that adds a BOM still reads.
    with open(path, encoding="utf-8-sig") as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#"):
                return None if line == PUBKEY_PLACEHOLDER else line
    return None


def validate_manifest(manifest) -> dict:
    """The manifest when it has the shape this tool reads, else ManifestError."""
    if not isinstance(manifest, dict):
        raise ManifestError("the manifest is not a JSON object")
    platforms = manifest.get("platforms", {})
    if not isinstance(platforms, dict):
        raise ManifestError("the manifest's platforms is not an object")
    for key, entry in platforms.items():
        if not isinstance(entry, dict):
            raise ManifestError(f"platform {key} is not an object")
        if not isinstance(entry.get("url"), str):
            raise ManifestError(f"platform {key} has no url")
        if not isinstance(entry.get("signature", ""), str):
            raise ManifestError(f"platform {key}'s signature is not a string")
    return manifest


def load_manifest(path: str) -> dict:
    try:
        with open(path, encoding="utf-8-sig") as f:
            manifest = json.load(f)
    except json.JSONDecodeError as e:
        raise ManifestError(f"{path} is not valid JSON: {e}") from None
    return validate_manifest(manifest)


def write_manifest(path: str, manifest: dict) -> None:
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")


def report(errors: list[str]) -> int:
    for e in errors:
        print(f"::error::{e}", file=sys.stderr)
    return 1 if errors else 0


def main(argv: list[str]) -> int:
    # The lines this prints are read back by `while read` loops: end them
    # with LF on every platform, never the CRLF Python writes on Windows.
    sys.stdout.reconfigure(newline="\n")
    parser = argparse.ArgumentParser(prog="release_tools.py")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("semver-gt")
    p.add_argument("candidate")
    p.add_argument("current")

    p = sub.add_parser("check-manifest")
    p.add_argument("--manifest", required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--require", action="append", default=[])
    p.add_argument("--signed-version")

    p = sub.add_parser("list-artifacts")
    p.add_argument("--manifest", required=True)

    p = sub.add_parser("resign-manifest")
    p.add_argument("--manifest", required=True)
    p.add_argument("--sigdir", required=True)

    p = sub.add_parser("add-cli")
    p.add_argument("--manifest", required=True)
    p.add_argument("--dir", required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--base-url", required=True)

    p = sub.add_parser("stage-builds")
    p.add_argument("--from", dest="source", required=True)
    p.add_argument("--bundles", required=True)
    p.add_argument("--cli", required=True)
    p.add_argument("--version", required=True)

    p = sub.add_parser("pubkey")
    p.add_argument("--file", required=True)

    p = sub.add_parser("semver-check")
    p.add_argument("version")

    p = sub.add_parser("updater-artifacts")
    p.add_argument("--dir", required=True)

    p = sub.add_parser("updater-manifest")
    p.add_argument("--dir", required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--base-url", required=True)
    p.add_argument("--output", required=True)
    p.add_argument("--changelog")
    p.add_argument("--pub-date")

    args = parser.parse_args(argv)
    try:
        return run(args)
    except (ManifestError, OSError, UnicodeDecodeError) as e:
        print(f"::error::{e}", file=sys.stderr)
        return 1


def run(args) -> int:

    if args.cmd == "semver-check":
        try:
            semver_key(args.version)
        except ValueError as e:
            print(f"::error::{e}", file=sys.stderr)
            return 2
        return 0

    if args.cmd == "updater-artifacts":
        for name in updater_artifacts(os.listdir(args.dir)):
            print(check_file_name(name))
        return 0

    if args.cmd == "updater-manifest":
        notes = None
        if args.changelog:
            with open(args.changelog, encoding="utf-8-sig") as f:
                notes = changelog_notes(f.read(), args.version)
            if notes is None:
                return report([f"{args.changelog} has no ## [{args.version.lstrip('v')}] section"])
        manifest, errors = updater_manifest(
            args.dir, args.version, args.base_url, notes, args.pub_date or now_utc())
        if errors:
            return report(errors)
        write_manifest(args.output, manifest)
        return 0

    if args.cmd == "semver-gt":
        try:
            return 0 if semver_gt(args.candidate, args.current) else 1
        except ValueError as e:
            print(f"::error::{e}", file=sys.stderr)
            return 2

    if args.cmd == "check-manifest":
        manifest = load_manifest(args.manifest)
        return report(check_manifest(manifest, args.version, args.require,
                                     args.signed_version))

    if args.cmd == "list-artifacts":
        for name in list_artifacts(load_manifest(args.manifest)):
            print(name)
        return 0

    if args.cmd == "resign-manifest":
        manifest = load_manifest(args.manifest)
        errors = resign_manifest(manifest, args.sigdir)
        if errors:
            return report(errors)
        write_manifest(args.manifest, manifest)
        return 0

    if args.cmd == "add-cli":
        if os.path.exists(args.manifest):
            manifest = load_manifest(args.manifest)
        else:
            manifest = {
                "version": args.version.lstrip("v"),
                "notes": None,
                "pub_date": now_utc(),
                "platforms": {},
            }
        errors = add_cli(manifest, args.dir, args.version.lstrip("v"), args.base_url)
        if errors:
            return report(errors)
        write_manifest(args.manifest, manifest)
        return 0

    if args.cmd == "stage-builds":
        return report(stage_builds(args.source, args.bundles, args.cli, args.version))

    if args.cmd == "pubkey":
        try:
            key = read_pubkey(args.file)
        except OSError as e:
            print(f"::error::{e}", file=sys.stderr)
            return 1
        if key is None:
            print("::error::updater.pub is UNSET; commit the release public key "
                  "(RELEASING.md section 2)", file=sys.stderr)
            return 1
        print(key)
        return 0

    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
