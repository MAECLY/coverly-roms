#!/usr/bin/env bash
#
# Clippy for Windows from a macOS/Linux machine: coverly-core on
# x86_64-pc-windows-msvc with -D warnings, the lints the Windows CI job
# applies (unused cfg(unix) helpers and imports, Windows-only code paths...).
#
# Needs the target: rustup target add x86_64-pc-windows-msvc
#
# Build scripts that compile C (bundled SQLite, ring) cannot cross-compile
# without an MSVC toolchain. Clippy never links, so when the plain run fails
# on a C toolchain error the script retries with a stub C compiler/archiver
# that only writes empty objects: every Rust item, cfg(windows) included, is
# still linted. If even that fails, it falls back to `cargo check` and says
# so. The last line printed is the result to paste into docs/TRACKER.md.
set -uo pipefail

cd "$(dirname "$0")/../.."

target=x86_64-pc-windows-msvc
env_target=${target//-/_}
if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
  echo "windows: target $target not installed (rustup target add $target)" >&2
  exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/clippy-windows.XXXXXX")
trap 'rm -rf "$work"' EXIT
log="$work/log"

clippy() {
  cargo clippy -p coverly-core --all-targets --all-features --target "$target" -- -D warnings
}

if clippy 2>&1 | tee "$log"; then
  echo "windows: clippy ok ($target, -D warnings)"
  exit 0
fi

if ! grep -Eq "cc-rs|failed to find tool|link\.exe|lib\.exe|cl\.exe|linker .* not found" "$log"; then
  echo "windows: clippy failed ($target, -D warnings)"
  exit 1
fi

# Stub C compiler and archiver: write the requested output file, empty.
stub="$work/stub-cc"
cat >"$stub" <<'STUB'
#!/bin/sh
for a in "$@"; do [ "$a" = "-E" ] && exit 0; done
case "${1:-}" in
  *[!crsqtuvD]* | "") ;;
  *) [ $# -ge 2 ] && : >"$2"; exit 0 ;; # ar-style: ar cq out.a objs...
esac
prev=""
for a in "$@"; do
  [ "$prev" = "-o" ] && : >"$a"
  case "$a" in
    -out:* | -OUT:* | /out:* | /OUT:*) : >"${a#*:}" ;;
    -Fo?* | /Fo?*) : >"${a#?Fo}" ;;
  esac
  prev="$a"
done
exit 0
STUB
chmod +x "$stub"
export "CC_${env_target}=$stub" "CXX_${env_target}=$stub" "AR_${env_target}=$stub"

echo "windows: no C cross toolchain; retrying with stubbed C objects" >&2
if clippy 2>&1 | tee "$log"; then
  echo "windows: clippy ok ($target, -D warnings; C objects stubbed, no MSVC toolchain)"
  exit 0
fi
if grep -Eq "^(error|warning)(\[[A-Za-z0-9_:]+\])?: " "$log" &&
  ! grep -Eq "cc-rs|failed to run custom build command" "$log"; then
  echo "windows: clippy failed ($target, -D warnings)"
  exit 1
fi

echo "windows: stubbed clippy could not build; falling back to cargo check" >&2
if cargo check -p coverly-core --all-targets --all-features --target "$target"; then
  echo "windows: check-only (no C cross toolchain)"
  exit 0
fi
echo "windows: check failed ($target)"
exit 1
