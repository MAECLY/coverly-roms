#!/usr/bin/env bash
#
# Set or check the version of Coverly ROMs: scripts/bump-version.sh X.Y.Z[-pre.N]
#
# The root Cargo.toml [workspace.package] version is the only version the
# programs read: coverly-core, coverly-cli and the app inherit it, and
# tauri.conf.json has no `version`, so the bundles, the updater and
# CARGO_PKG_VERSION agree by construction. package.json is kept equal for the
# frontend and the release guard, and Cargo.lock's own entries are patched in
# the same commit so the next cargo command does not dirty the tree.
#
# A version with a pre-release suffix (0.2.0-beta.1) is published to the beta
# channel; one without is stable. .github/workflows/release.yml refuses a tag
# that differs from what this script wrote. See RELEASING.md.
#
# Options:
#   --root DIR   operate on another checkout (tests)
#   --check      print every source and exit 1 when they disagree
#
# It does not commit or tag; it prints the commands to do so.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
check=0
version=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --root) shift; root="$(cd "${1:?--root needs a directory}" && pwd)" ;;
    --check) check=1 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) version="$1" ;;
  esac
  shift
done

cargo_toml="$root/Cargo.toml"
cargo_lock="$root/Cargo.lock"
package_json="$root/package.json"
for f in "$cargo_toml" "$cargo_lock" "$package_json"; do
  [[ -f "$f" ]] || { echo "missing $f" >&2; exit 1; }
done

current_versions() {
  python3 -I - "$cargo_toml" "$package_json" "$cargo_lock" <<'PY'
import json, re, sys
cargo, pkg, lock = sys.argv[1:]
text = open(cargo, encoding="utf-8").read()
m = re.search(r'(?ms)^\[workspace\.package\].*?^version = "([^"]+)"', text)
print("Cargo.toml", m.group(1) if m else "<none>")
print("package.json", json.load(open(pkg, encoding="utf-8")).get("version", "<none>"))
lock_text = open(lock, encoding="utf-8").read()
for name in ("coverly-core", "coverly-cli", "coverly-roms"):
    m = re.search(r'name = "%s"\nversion = "([^"]+)"' % re.escape(name), lock_text)
    print("Cargo.lock:" + name, m.group(1) if m else "<none>")
PY
}

if [[ $check -eq 1 ]]; then
  out="$(current_versions)"
  echo "$out"
  if [[ "$(echo "$out" | awk '{print $2}' | sort -u | wc -l | tr -d ' ')" != "1" ]]; then
    echo "versions disagree; run scripts/bump-version.sh X.Y.Z" >&2
    exit 1
  fi
  exit 0
fi

# SemVer 2.0, no leading "v", no build metadata (a tag is v<version>).
semver='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$'
if [[ ! "$version" =~ $semver ]]; then
  echo "usage: $0 X.Y.Z[-pre.N]   ('${version}' is not a SemVer version without a leading v)" >&2
  exit 2
fi

python3 -I - "$version" "$cargo_toml" "$package_json" "$cargo_lock" <<'PY'
import json, re, sys
version, cargo, pkg, lock = sys.argv[1:]
text = open(cargo, encoding="utf-8").read()
new, n = re.subn(r'(?ms)(^\[workspace\.package\].*?^version = ")[^"]+(")', r'\g<1>' + version + r'\g<2>', text, count=1)
if n != 1:
    sys.exit("no [workspace.package] version in Cargo.toml")
open(cargo, "w", encoding="utf-8").write(new)
data = json.load(open(pkg, encoding="utf-8"))
data["version"] = version
with open(pkg, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2, ensure_ascii=False)
    f.write("\n")
lock_text = open(lock, encoding="utf-8").read()
for name in ("coverly-core", "coverly-cli", "coverly-roms"):
    lock_text, n = re.subn(r'(name = "%s"\nversion = ")[^"]+(")' % re.escape(name), r'\g<1>' + version + r'\g<2>', lock_text)
    if n != 1:
        sys.exit(f"no {name} entry in Cargo.lock")
open(lock, "w", encoding="utf-8").write(lock_text)
PY

echo "Version set to $version. Next:"
echo "  move CHANGELOG.md's [Unreleased] notes under ## [$version] - $(date +%Y-%m-%d)"
echo "  git commit -am \"chore(release): $version\" && git tag v$version"
