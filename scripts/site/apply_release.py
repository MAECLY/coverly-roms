#!/usr/bin/env python3
"""Fill the landing page in site/ with the published release, or with none.

Used by .github/workflows/pages.yml, so the landing follows every release on
its own: nothing in site/ names a version, a date, a download link or a file
size. Those places carry markers instead, and this script copies site/ to an
output folder with every marker replaced from the release's GitHub API record
(GET /repos/<repo>/releases/latest). The generated folder is what Pages
publishes; it is never committed.

Plain standard-library Python, so the runner needs nothing installed, and
src-tauri/tests/site_release.rs runs it against a fixture.

  apply_release.py --release-json F --repo OWNER/NAME --site DIR --out DIR
  apply_release.py --no-release     --repo OWNER/NAME --site DIR --out DIR

Blocks (in any text file, each comment on its own or inline):

  <!-- release:begin --> ... <!-- release:end -->
      kept, without the two comments, when there is a release; dropped
      with --no-release. Every {{release.*}} marker must sit in one.
  <!-- norelease:begin --> ... <!-- norelease:end -->
      the opposite: the "not published yet" state, kept only with
      --no-release.

Markers (in any .html, .txt, .webmanifest, .xml or .json file):

  {{release.version}}           0.1.0 (the tag without its "v")
  {{release.date}}              2026-11-02 (published_at, UTC)
  {{release.date_es}}           2 de noviembre de 2026
  {{release.date_en}}           2 November 2026
  {{release.notes_url}}         the release page
  {{release.sha256sums_url}}    the SHA256SUMS asset
  {{release.<file>.url}}        a download, <file> being one of DOWNLOADS
  {{release.<file>.name}}       its file name
  {{release.<file>.mib_en}}     its size in MiB, one decimal: 17.3
  {{release.<file>.mib_es}}     the same with a decimal comma: 17,3

It fails, and so the deploy fails, when the release is a draft or a
pre-release, its tag is not v<SemVer>, any download or SHA256SUMS is
missing, ambiguous, empty or not where the landing would link it, a marker is
unknown, left over or outside a release block, a block is unbalanced, a
download is linked nowhere, or a JSON file or JSON-LD block no longer parses.
There is no flag to skip any of that: the landing never links a file that is
not there. --no-release only exists for the time before the first release
(the workflow uses it only when the API answers 404 for the latest release).
"""

from __future__ import annotations

import argparse
import datetime
import json
import re
import shutil
import sys
from pathlib import Path

PRODUCT = "Coverly ROMs"

# Every download the landing offers: marker name -> pattern of the asset
# name, {v} being the version. The app bundles keep Tauri's names with the
# product name made safe for GitHub ("Coverly ROMs" -> "Coverly.ROMs"; see
# the staging step of release.yml), so they are matched like
# scripts/release/release_tools.py build_artifacts() does: any safe prefix,
# exactly one file per pattern.
APP = r"[A-Za-z0-9._-]+_{v}_"
DOWNLOADS = {
    "macos_universal": APP + r"universal\.dmg",
    "windows_x64": APP + r"x64-setup\.exe",
    "windows_x64_msi": APP + r"x64_[A-Za-z0-9-]+\.msi",
    "linux_x64_appimage": APP + r"amd64\.AppImage",
    "linux_x64_deb": APP + r"amd64\.deb",
    "linux_arm64_appimage": APP + r"aarch64\.AppImage",
    "linux_arm64_deb": APP + r"arm64\.deb",
    "cli_macos_arm64": r"coverly-{v}-darwin-aarch64",
    "cli_macos_x64": r"coverly-{v}-darwin-x86_64",
    "cli_windows_x64": r"coverly-{v}-windows-x86_64\.exe",
    "cli_linux_x64": r"coverly-{v}-linux-x86_64",
    "cli_linux_arm64": r"coverly-{v}-linux-aarch64",
}

SEMVER = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
REPO = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
MARKER = re.compile(r"\{\{\s*release\.([^}]*?)\s*\}\}")
LEFTOVER = re.compile(r"\{\{\s*release|release\.[a-z_.0-9]*\}\}")
BLOCK = re.compile(r"<!--\s*(no)?release:(begin|end)\s*-->")
TEXT_SUFFIXES = {".html", ".txt", ".webmanifest", ".xml", ".json"}
JSON_LD = re.compile(
    r'<script type="application/ld\+json">(.*?)</script>', re.DOTALL
)

MONTHS_ES = [
    "enero", "febrero", "marzo", "abril", "mayo", "junio", "julio",
    "agosto", "septiembre", "octubre", "noviembre", "diciembre",
]
MONTHS_EN = [
    "January", "February", "March", "April", "May", "June", "July",
    "August", "September", "October", "November", "December",
]


class ReleaseError(Exception):
    pass


def mib(size: int) -> str:
    return f"{size / (1024 * 1024):.1f}"


def values(release: dict, repo: str) -> dict[str, str]:
    """Every marker's value, after checking the release is one to link."""
    if release.get("draft") or release.get("prerelease"):
        raise ReleaseError("the release is a draft or a pre-release")
    tag = release.get("tag_name")
    if not isinstance(tag, str) or not tag.startswith("v"):
        raise ReleaseError(f"tag {tag!r} is not v<version>")
    version = tag[1:]
    if not SEMVER.match(version):
        raise ReleaseError(f"tag {tag!r} is not v<SemVer>")

    try:
        published = datetime.datetime.fromisoformat(
            str(release.get("published_at")).replace("Z", "+00:00")
        ).astimezone(datetime.timezone.utc)
    except ValueError as e:
        raise ReleaseError(f"published_at: {e}") from None

    base = f"https://github.com/{repo}/releases"
    notes = f"{base}/tag/{tag}"
    if release.get("html_url") != notes:
        raise ReleaseError(f"html_url {release.get('html_url')!r} is not {notes}")

    assets = [a for a in release.get("assets") or [] if isinstance(a, dict)]

    def asset(pattern: str, label: str) -> tuple[str, str, int]:
        rx = re.compile(pattern.replace("{v}", re.escape(version)))
        found = [a for a in assets if rx.fullmatch(str(a.get("name")))]
        if not found:
            raise ReleaseError(f"{tag} has no asset for {label} ({rx.pattern})")
        if len(found) > 1:
            names = ", ".join(sorted(str(a.get("name")) for a in found))
            raise ReleaseError(f"{tag} has several assets for {label}: {names}")
        a = found[0]
        name = str(a["name"])
        if a.get("state", "uploaded") != "uploaded":
            raise ReleaseError(f"{name} is {a.get('state')}, not uploaded")
        size = a.get("size")
        if not isinstance(size, int) or size <= 0:
            raise ReleaseError(f"{name} is empty")
        url = f"{base}/download/{tag}/{name}"
        if a.get("browser_download_url") != url:
            raise ReleaseError(
                f"{name} downloads from {a.get('browser_download_url')!r},"
                f" not {url}"
            )
        return name, url, size

    out = {
        "version": version,
        "date": published.strftime("%Y-%m-%d"),
        "date_es": (
            f"{published.day} de {MONTHS_ES[published.month - 1]}"
            f" de {published.year}"
        ),
        "date_en": (
            f"{published.day} {MONTHS_EN[published.month - 1]} {published.year}"
        ),
        "notes_url": notes,
        "sha256sums_url": asset(r"SHA256SUMS", "SHA256SUMS")[1],
    }
    for label, pattern in DOWNLOADS.items():
        name, url, size = asset(pattern, label)
        out[f"{label}.name"] = name
        out[f"{label}.url"] = url
        out[f"{label}.mib_en"] = mib(size)
        out[f"{label}.mib_es"] = mib(size).replace(".", ",")
    return out


def blocks(text: str, released: bool, where: str) -> str:
    """Keeps the blocks for this state (without their comments) and drops
    the others; a marker left outside a kept release block is an error."""
    out: list[str] = []
    pos = 0
    open_kind: str | None = None
    for m in BLOCK.finditer(text):
        kind = "norelease" if m.group(1) else "release"
        chunk = text[pos:m.start()]
        if m.group(2) == "begin":
            if open_kind is not None:
                raise ReleaseError(f"{where}: {kind}:begin inside {open_kind}")
            if MARKER.search(chunk):
                raise ReleaseError(f"{where}: a marker sits outside a release block")
            out.append(chunk)
            open_kind = kind
        else:
            if open_kind != kind:
                raise ReleaseError(f"{where}: {kind}:end without its begin")
            if kind == "norelease" and MARKER.search(chunk):
                raise ReleaseError(f"{where}: a marker sits in a norelease block")
            if (kind == "release") == released:
                out.append(chunk)
            open_kind = None
        pos = m.end()
    if open_kind is not None:
        raise ReleaseError(f"{where}: {open_kind}:begin is never closed")
    tail = text[pos:]
    if MARKER.search(tail):
        raise ReleaseError(f"{where}: a marker sits outside a release block")
    out.append(tail)
    return "".join(out)


def render(text: str, vals: dict[str, str], where: str, used: set[str]) -> str:
    def one(m: re.Match) -> str:
        key = m.group(1)
        if key not in vals:
            raise ReleaseError(f"{where}: unknown marker {m.group(0)}")
        used.add(key)
        return vals[key]

    out = MARKER.sub(one, text)
    if LEFTOVER.search(out):
        raise ReleaseError(f"{where}: a malformed marker is left over")
    return out


def check_json(path: Path, text: str) -> None:
    try:
        if path.suffix in (".json", ".webmanifest"):
            json.loads(text)
        elif path.suffix == ".html":
            for block in JSON_LD.findall(text):
                json.loads(block)
    except json.JSONDecodeError as e:
        raise ReleaseError(f"{path}: no longer valid JSON: {e}") from None


def apply(release: dict | None, repo: str, site: Path, out: Path) -> dict[str, str]:
    """Copies site/ to out/ filled for `release` (None: not published yet)."""
    if not REPO.match(repo):
        raise ReleaseError(f"not an OWNER/NAME repository: {repo!r}")
    vals = values(release, repo) if release is not None else {}
    if not site.is_dir():
        raise ReleaseError(f"{site} is not a folder")
    if out.exists() and any(out.iterdir()):
        raise ReleaseError(f"{out} is not empty")
    if out.resolve().is_relative_to(site.resolve()):
        raise ReleaseError(f"{out} is inside {site}")
    shutil.copytree(site, out, dirs_exist_ok=True)

    used: set[str] = set()
    for path in sorted(out.rglob("*")):
        if not path.is_file() or path.suffix not in TEXT_SUFFIXES:
            continue
        text = path.read_text(encoding="utf-8")
        rel = path.relative_to(out).as_posix()
        new = blocks(text, release is not None, rel)
        new = render(new, vals, rel, used)
        check_json(path.relative_to(out), new)
        if new != text:
            path.write_text(new, encoding="utf-8")

    if release is not None:
        unused = [f"{label}.url" for label in DOWNLOADS if f"{label}.url" not in used]
        if unused:
            raise ReleaseError(f"site/ never links {', '.join(unused)}")
    return vals


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    src = p.add_mutually_exclusive_group(required=True)
    src.add_argument("--release-json", type=Path)
    src.add_argument(
        "--no-release",
        action="store_true",
        help="no release is published yet: keep the norelease blocks",
    )
    p.add_argument("--repo", required=True)
    p.add_argument("--site", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    a = p.parse_args(argv)
    try:
        release = None
        if a.release_json is not None:
            release = json.loads(a.release_json.read_text(encoding="utf-8"))
            if not isinstance(release, dict):
                raise ReleaseError("the release JSON is not an object")
        vals = apply(release, a.repo, a.site, a.out)
    except (OSError, json.JSONDecodeError, ReleaseError) as e:
        print(f"apply_release: {e}", file=sys.stderr)
        return 1
    if release is None:
        print(f"site/ filled for {PRODUCT} with no release published yet")
        return 0
    print(f"site/ filled with {PRODUCT} {vals['version']} ({vals['date']}):")
    for label in DOWNLOADS:
        print(f"  {vals[label + '.url']} ({vals[label + '.mib_en']} MiB)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
